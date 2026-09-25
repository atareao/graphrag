//! Data loading module for the concept-map TUI.
//!
//! This module provides the bridge between the SQLite graph database and
//! the interactive terminal UI (TUI).  It exposes a [`GraphData`] struct
//! that contains everything the TUI needs to render: nodes and edges.
//!
//! # Loading strategies
//!
//! * **All nodes** (`from = None`): loads every node whose type is neither
//!   `"root"` nor `"system"`, plus every edge whose endpoints are both in
//!   that set.
//!
//! * **Subgraph from label** (`from = Some(label)`): uses a bidirectional
//!   recursive CTE (the same pattern as [`crate::graph::expand`]) to find
//!   all nodes reachable within `depth` hops, then loads only the edges
//!   that connect nodes inside that set.

pub mod layout;
mod tui;

use anyhow::{Context, Result};
use rusqlite::Connection;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// A single node in the concept-map graph.
#[derive(Debug, Clone)]
pub struct MapNode {
    pub id: i64,
    pub label: String,
    pub type_: String,
}

/// A directed, labelled edge between two nodes.
#[derive(Debug, Clone)]
pub struct MapEdge {
    pub source_id: i64,
    pub target_id: i64,
    pub type_: String,
    pub weight: f64,
    #[allow(dead_code)]
    pub context: Option<String>,
}

/// The full data set needed to render the concept map.
#[derive(Debug, Clone)]
pub struct GraphData {
    pub nodes: Vec<MapNode>,
    pub edges: Vec<MapEdge>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Load graph data from a SQLite database.
///
/// # Arguments
///
/// * `db`   – Path to the SQLite database file.
/// * `from` – Optional starting node label.  `None` loads the entire
///   graph (minus `root`/`system` sentinel nodes); `Some(label)`
///   loads the subgraph reachable within `depth` hops.
/// * `depth` – Maximum number of edge hops (only used when `from` is set).
///
/// # Errors
///
/// Returns an error if the database cannot be opened, the schema is
/// missing, or (when `from` is `Some`) the label does not exist.
pub fn load_graph(db: &str, from: Option<&str>, depth: i32) -> Result<GraphData> {
    let conn = Connection::open(db).with_context(|| format!("failed to open database: {db}"))?;

    let node_ids = resolve_node_ids(&conn, from, depth)?;

    let nodes = load_nodes(&conn, &node_ids)?;
    let edges = load_edges(&conn, &node_ids)?;

    Ok(GraphData { nodes, edges })
}

/// CLI entry point for the `graphrag map` subcommand.
///
/// Loads graph data from the database, computes a force-directed layout,
/// and launches the interactive TUI.
pub fn cmd_map(db: &str, from: Option<String>, depth: i32) -> Result<()> {
    let data = load_graph(db, from.as_deref(), depth)?;

    if data.nodes.is_empty() {
        println!("No nodes to display.");
        return Ok(());
    }

    // Compute force-directed layout
    let width = 200.0;
    let height = 150.0;
    let iterations = 100;

    // Build edge index pairs for the layout algorithm
    let node_id_to_idx: std::collections::HashMap<i64, usize> = data
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id, i))
        .collect();

    let edge_pairs: Vec<(usize, usize)> = data
        .edges
        .iter()
        .filter_map(|e| {
            let si = node_id_to_idx.get(&e.source_id)?;
            let ti = node_id_to_idx.get(&e.target_id)?;
            Some((*si, *ti))
        })
        .collect();

    let layout = crate::map::layout::force_directed_layout(
        data.nodes.len(),
        &edge_pairs,
        width,
        height,
        iterations,
    );

    // Build AppState and launch TUI
    let state = crate::map::tui::AppState {
        nodes: data.nodes,
        edges: data.edges,
        positions: layout.positions,
        selected_idx: None,
        filter_type: None,
        search_query: String::new(),
        search_active: false,
        offset_x: 0.0,
        offset_y: 0.0,
        depth,
    };

    crate::map::tui::run_tui(state)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Resolve the set of node IDs to include based on the `from` / `depth`
/// parameters.
fn resolve_node_ids(conn: &Connection, from: Option<&str>, depth: i32) -> Result<Vec<i64>> {
    match from {
        None => {
            let mut stmt =
                conn.prepare_cached("SELECT id FROM nodes WHERE type NOT IN ('root', 'system')")?;
            let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
            let mut ids = Vec::new();
            for row in rows {
                ids.push(row?);
            }
            Ok(ids)
        }
        Some(label) => {
            // Look up the starting node.
            let node_id: i64 = conn
                .query_row(
                    "SELECT id FROM nodes WHERE label = ?1",
                    rusqlite::params![label],
                    |row| row.get(0),
                )
                .with_context(|| format!("node with label '{label}' not found"))?;

            // When depth <= 0 the subgraph contains only the start node
            // itself (no edges).
            if depth <= 0 {
                return Ok(vec![node_id]);
            }

            // Bidirectional recursive CTE — same pattern as
            // `graph::expand::expand_neighbors` but without a LIMIT so
            // the entire reachable subgraph is returned.
            // We exclude root/system sentinel nodes so the TUI never
            // shows internal plumbing.
            let mut stmt = conn.prepare_cached(
                r#"
                WITH RECURSIVE reachable AS (
                    SELECT id, 0 AS level FROM nodes WHERE id = ?1
                    UNION ALL
                    SELECT e.target_id, r.level + 1
                    FROM reachable r
                    JOIN edges e ON e.source_id = r.id
                    WHERE r.level < ?2
                    UNION ALL
                    SELECT e.source_id, r.level + 1
                    FROM reachable r
                    JOIN edges e ON e.target_id = r.id
                    WHERE r.level < ?2
                )
                SELECT DISTINCT r.id
                FROM reachable r
                JOIN nodes n ON n.id = r.id
                WHERE n.type NOT IN ('root', 'system')
                "#,
            )?;

            let rows = stmt.query_map(rusqlite::params![node_id, depth], |row| {
                row.get::<_, i64>(0)
            })?;

            let mut ids = Vec::new();
            for row in rows {
                ids.push(row?);
            }
            Ok(ids)
        }
    }
}

/// Load [`MapNode`] records for every ID in `ids`.
///
/// Returns an empty `Vec` when `ids` is empty.
fn load_nodes(conn: &Connection, ids: &[i64]) -> Result<Vec<MapNode>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders: Vec<String> = (0..ids.len()).map(|i| format!("?{}", i + 1)).collect();
    let sql = format!(
        "SELECT id, label, type FROM nodes WHERE id IN ({})",
        placeholders.join(", ")
    );

    let mut stmt = conn.prepare_cached(&sql)?;

    // Bind all IDs as a slice of trait-object references.
    let params: Vec<&dyn rusqlite::types::ToSql> = ids
        .iter()
        .map(|id| id as &dyn rusqlite::types::ToSql)
        .collect();

    let rows = stmt.query_map(params.as_slice(), |row| {
        Ok(MapNode {
            id: row.get(0)?,
            label: row.get(1)?,
            type_: row.get(2)?,
        })
    })?;

    let mut nodes = Vec::new();
    for row in rows {
        nodes.push(row?);
    }
    Ok(nodes)
}

/// Load [`MapEdge`] records whose *source* AND *target* are both in `ids`.
///
/// Returns an empty `Vec` when `ids` is empty.
fn load_edges(conn: &Connection, ids: &[i64]) -> Result<Vec<MapEdge>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    // We need two copies of the same IDs for the two IN clauses.
    let placeholders: Vec<String> = (0..ids.len()).map(|i| format!("?{}", i + 1)).collect();
    let offset_placeholders: Vec<String> = (0..ids.len())
        .map(|i| format!("?{}", i + 1 + ids.len()))
        .collect();
    let sql = format!(
        r#"SELECT source_id, target_id, type, weight, context
           FROM edges
           WHERE source_id IN ({src})
             AND target_id IN ({tgt})"#,
        src = placeholders.join(", "),
        tgt = offset_placeholders.join(", "),
    );

    let mut stmt = conn.prepare_cached(&sql)?;

    // Build a single flat slice referencing each ID twice.
    let mut all_bindings: Vec<&dyn rusqlite::types::ToSql> = Vec::with_capacity(ids.len() * 2);
    for id in ids {
        all_bindings.push(id);
    }
    for id in ids {
        all_bindings.push(id);
    }

    let rows = stmt.query_map(all_bindings.as_slice(), |row| {
        Ok(MapEdge {
            source_id: row.get(0)?,
            target_id: row.get(1)?,
            type_: row.get(2)?,
            weight: row.get(3)?,
            context: row.get(4)?,
        })
    })?;

    let mut edges = Vec::new();
    for row in rows {
        edges.push(row?);
    }
    Ok(edges)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;

    /// Create a temporary database file with the schema and test data,
    /// returning the path.
    fn create_test_db() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db_path_str = db_path.to_str().unwrap().to_string();

        let conn = Connection::open(&db_path).unwrap();
        schema::init_db(&conn).unwrap();

        // Insert test nodes (mix of normal, root, and system).
        conn.execute_batch(
            "INSERT INTO nodes (id, label, type) VALUES
                 (10, 'Python',    'language'),
                 (11, 'Rust',      'language'),
                 (12, 'PostgreSQL','database'),
                 (13, 'Docker',    'tool'),
                 (14, 'SQLAlchemy','library'),
                 (15, 'Actix',     'library'),
                 (16, 'Tokio',     'library'),
                 (20, '_root',     'root'),
                 (21, '_system',   'system');
             INSERT INTO edges (source_id, target_id, type, weight, context) VALUES
                 (10, 14, 'uses',     0.9, 'Python projects use SQLAlchemy for ORM'),
                 (14, 12, 'connects', 0.8, 'SQLAlchemy connects to PostgreSQL'),
                 (10, 13, 'runs_in',  0.7, 'Python runs inside Docker containers'),
                 (11, 15, 'uses',     0.6, 'Rust web apps use Actix'),
                 (11, 16, 'depends',  0.9, 'Actix depends on Tokio runtime'),
                 (15, 16, 'depends',  0.8, 'Actix uses Tokio internally'),
                 (10, 11, 'similar',  0.4, 'Both are programming languages'),
                 (20, 10, 'contains', 1.0, 'Root contains Python'),
                 (20, 11, 'contains', 1.0, 'Root contains Rust'),
                 (21, 20, 'links',    1.0, 'System links to root');
            ",
        )
        .unwrap();

        conn.close().unwrap();
        (dir, db_path_str)
    }

    // -----------------------------------------------------------------------
    // test_load_graph_all_nodes
    // -----------------------------------------------------------------------

    #[test]
    fn test_load_graph_all_nodes() {
        let (_dir, db_path) = create_test_db();

        let data = load_graph(&db_path, None, 0).unwrap();

        // Should include all non-root, non-system nodes: 7 normal nodes.
        assert_eq!(data.nodes.len(), 7);
        // Should include edges that connect only those nodes: 7 edges
        // (Python→SQLAlchemy, SQLAlchemy→PostgreSQL, Python→Docker,
        //  Rust→Actix, Actix→Tokio, Rust→Tokio, Python→Rust).
        // The edges involving root/system are excluded.
        assert_eq!(data.edges.len(), 7);

        // Verify specific nodes are present.
        let labels: Vec<&str> = data.nodes.iter().map(|n| n.label.as_str()).collect();
        assert!(labels.contains(&"Python"));
        assert!(labels.contains(&"Rust"));
        assert!(labels.contains(&"PostgreSQL"));

        // Verify root/system are absent.
        assert!(!labels.contains(&"_root"));
        assert!(!labels.contains(&"_system"));
    }

    // -----------------------------------------------------------------------
    // test_load_graph_from_node_depth_2
    // -----------------------------------------------------------------------

    #[test]
    fn test_load_graph_from_node_depth_2() {
        let (_dir, db_path) = create_test_db();

        // From "Python" with depth 2.
        let data = load_graph(&db_path, Some("Python"), 2).unwrap();

        // Reachable from Python at depth 2:
        //   Python (10, level 0)
        //   → SQLAlchemy (14, level 1), Docker (13, level 1), Rust (11, level 1)
        //   → PostgreSQL (12, level 2 via 14), Tokio (16, level 2 via 11), Actix (15, level 2 via 11)
        // Total: 7 nodes.
        let labels: Vec<&str> = data.nodes.iter().map(|n| n.label.as_str()).collect();
        assert_eq!(
            data.nodes.len(),
            7,
            "expected 7 reachable nodes from Python at depth 2"
        );
        assert!(labels.contains(&"Python"));
        assert!(labels.contains(&"SQLAlchemy"));
        assert!(labels.contains(&"PostgreSQL"));
        assert!(labels.contains(&"Docker"));
        assert!(labels.contains(&"Rust"));
        assert!(labels.contains(&"Actix"));
        assert!(labels.contains(&"Tokio"));

        // Root and system should not be included.
        assert!(!labels.contains(&"_root"));
        assert!(!labels.contains(&"_system"));

        // Edges should only connect reachable nodes.
        for edge in &data.edges {
            let src_label = data
                .nodes
                .iter()
                .find(|n| n.id == edge.source_id)
                .map(|n| n.label.as_str())
                .unwrap_or("?");
            let tgt_label = data
                .nodes
                .iter()
                .find(|n| n.id == edge.target_id)
                .map(|n| n.label.as_str())
                .unwrap_or("?");
            assert!(
                labels.contains(&src_label),
                "edge source '{src_label}' is not in reachable set"
            );
            assert!(
                labels.contains(&tgt_label),
                "edge target '{tgt_label}' is not in reachable set"
            );
        }

        // Verify edges include the cross-language edge Python→Rust but NOT
        // the root→Python or root→Rust edges.
        let edge_types: Vec<&str> = data.edges.iter().map(|e| e.type_.as_str()).collect();
        assert!(edge_types.contains(&"similar"));
        assert!(!edge_types.contains(&"contains"));
        assert!(!edge_types.contains(&"links"));
    }

    // -----------------------------------------------------------------------
    // test_load_graph_from_node_depth_1
    // -----------------------------------------------------------------------

    #[test]
    fn test_load_graph_from_node_depth_1() {
        let (_dir, db_path) = create_test_db();

        let data = load_graph(&db_path, Some("Python"), 1).unwrap();

        // Depth 1 from Python: Python, SQLAlchemy, Docker, Rust
        let labels: Vec<&str> = data.nodes.iter().map(|n| n.label.as_str()).collect();
        assert_eq!(data.nodes.len(), 4);
        assert!(labels.contains(&"Python"));
        assert!(labels.contains(&"SQLAlchemy"));
        assert!(labels.contains(&"Docker"));
        assert!(labels.contains(&"Rust"));

        // PostgreSQL is depth 2 away (Python→SQLAlchemy→PostgreSQL).
        assert!(!labels.contains(&"PostgreSQL"));
    }

    // -----------------------------------------------------------------------
    // test_load_graph_from_nonexistent
    // -----------------------------------------------------------------------

    #[test]
    fn test_load_graph_from_nonexistent() {
        let (_dir, db_path) = create_test_db();

        let result = load_graph(&db_path, Some("NonExistentLabel"), 2);
        assert!(result.is_err(), "expected error for non-existent label");
        let err = result.unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("NonExistentLabel"),
            "error message should mention the missing label, got: {msg}"
        );
    }

    // -----------------------------------------------------------------------
    // test_load_graph_empty_db
    // -----------------------------------------------------------------------

    #[test]
    fn test_load_graph_empty_db() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("empty.db");
        let db_path_str = db_path.to_str().unwrap().to_string();

        // Create a DB with only the schema — no data.
        {
            let conn = Connection::open(&db_path).unwrap();
            schema::init_db(&conn).unwrap();
        }

        // Load all nodes — should be empty.
        let data = load_graph(&db_path_str, None, 0).unwrap();
        assert_eq!(data.nodes.len(), 0, "empty DB should have no nodes");
        assert_eq!(data.edges.len(), 0, "empty DB should have no edges");

        // Load from a label — should fail.
        let result = load_graph(&db_path_str, Some("anything"), 2);
        assert!(result.is_err(), "expected error on empty DB with from=Some");
    }

    // -----------------------------------------------------------------------
    // test_load_graph_depth_zero
    // -----------------------------------------------------------------------

    #[test]
    fn test_load_graph_depth_zero() {
        let (_dir, db_path) = create_test_db();

        // Depth 0 — only the starting node, no edges.
        let data = load_graph(&db_path, Some("Python"), 0).unwrap();
        assert_eq!(data.nodes.len(), 1);
        assert_eq!(data.nodes[0].label, "Python");
        assert_eq!(data.edges.len(), 0);
    }
}
