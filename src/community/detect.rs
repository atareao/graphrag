//! Community detection using the Leiden algorithm.
//!
//! Loads entity nodes and co_occurs_with edges from the SQLite database,
//! runs hierarchical Leiden community detection, and stores the resulting
//! communities back into the database.

use std::collections::HashMap;

use anyhow::{Context, Result};
use leiden_rs::graph::GraphDataBuilder;
use leiden_rs::hierarchy::HierarchicalOutput;
use leiden_rs::leiden::{Leiden, LeidenConfigBuilder, QualityType};
use rusqlite::Connection;

use crate::db::communities::{delete_all_communities, insert_community, Community};
use crate::db::schema;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Entity node loaded from the graph.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct EntityNode {
    pub id: i64,
    pub label: String,
}

/// Entity edge (co_occurs_with).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct EntityEdge {
    pub source_id: i64,
    pub target_id: i64,
    pub weight: f64,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Load entity nodes and `co_occurs_with` edges from the database.
///
/// Excludes nodes with type `'note'` or `'tag'`.
#[allow(dead_code)]
pub fn load_entity_graph(conn: &Connection) -> Result<(Vec<EntityNode>, Vec<EntityEdge>)> {
    // Load entity nodes (exclude notes and tags).
    let mut node_stmt = conn
        .prepare("SELECT id, label FROM nodes WHERE type NOT IN ('note', 'tag') ORDER BY id")
        .context("failed to prepare node query")?;

    let nodes: Vec<EntityNode> = node_stmt
        .query_map([], |row| {
            Ok(EntityNode {
                id: row.get(0)?,
                label: row.get(1)?,
            })
        })
        .context("failed to query entity nodes")?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("failed to collect entity nodes")?;

    // Load co_occurs_with edges.
    let mut edge_stmt = conn
        .prepare("SELECT source_id, target_id, weight FROM edges WHERE type = 'co_occurs_with'")
        .context("failed to prepare edge query")?;

    let edges: Vec<EntityEdge> = edge_stmt
        .query_map([], |row| {
            Ok(EntityEdge {
                source_id: row.get(0)?,
                target_id: row.get(1)?,
                weight: row.get(2)?,
            })
        })
        .context("failed to query entity edges")?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("failed to collect entity edges")?;

    Ok((nodes, edges))
}

/// Run Leiden community detection and return communities at all hierarchy levels.
///
/// Uses CPM (Constant Potts Model) quality function with the given resolution.
/// Returns `(level, member_entity_ids, label)` for each detected community
/// with 2 or more members.
///
/// The label is auto-generated as `\"community_{level}_{index}\"`.
#[allow(dead_code)]
pub fn detect_communities(
    conn: &Connection,
    resolution: f64,
) -> Result<Vec<(i32, Vec<i64>, String)>> {
    let (nodes, edges) = load_entity_graph(conn)?;

    if nodes.is_empty() {
        return Ok(Vec::new());
    }

    // Build a node-id → graph-index map for O(1) lookups.
    let node_index: HashMap<i64, usize> =
        nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();

    // Build the graph for leiden-rs.
    let mut builder = GraphDataBuilder::new(nodes.len());
    for edge in &edges {
        // Only include edges where both endpoints are entity nodes
        // (edges referencing notes/tags that were excluded are skipped).
        if let (Some(&src), Some(&dst)) = (
            node_index.get(&edge.source_id),
            node_index.get(&edge.target_id),
        ) {
            builder
                .add_edge(src, dst, edge.weight)
                .context("failed to add edge to graph builder")?;
        }
    }
    let graph = builder.build()?;

    // Configure and run Leiden.
    let config = LeidenConfigBuilder::default()
        .resolution(resolution)
        .quality(QualityType::CPM)
        .build();

    let leiden = Leiden::new(config);
    let output: HierarchicalOutput = leiden
        .run_hierarchical(&graph)
        .context("Leiden hierarchical run failed")?;

    // Process each hierarchy level.
    let mut results = Vec::new();
    for (level_idx, level_data) in output.levels.iter().enumerate() {
        // Skip the coarsest level (typically 1 community containing all nodes).
        if level_data.num_communities <= 1 {
            continue;
        }

        // Group nodes by community ID at this level.
        let mut communities: HashMap<usize, Vec<i64>> = HashMap::new();
        for (node_pos, &community_id) in level_data.membership.iter().enumerate() {
            let entity_id = nodes[node_pos].id;
            communities.entry(community_id).or_default().push(entity_id);
        }

        // Emit each community with 2+ members, sorted by member list.
        let mut community_entries: Vec<_> = communities.into_iter().collect();
        community_entries.sort_by(|a, b| {
            let a_min = a.1.iter().min();
            let b_min = b.1.iter().min();
            a_min.cmp(&b_min)
        });

        for (comm_idx, (_community_id, member_ids)) in community_entries.iter().enumerate() {
            if member_ids.len() >= 2 {
                let label = format!("community_{level_idx}_{comm_idx}");
                results.push((level_idx as i32, member_ids.clone(), label));
            }
        }
    }

    Ok(results)
}

/// High-level orchestrator: loads graph, runs Leiden, stores in DB.
///
/// Returns `(num_level1, num_level2)` — the count of communities at
/// level 1 and level 2 respectively.
#[allow(dead_code)]
pub fn run_community_detection(db_path: &str, resolution: f64) -> Result<(usize, usize)> {
    let conn = Connection::open(db_path).context("failed to open database")?;

    // Apply the idempotent schema migrations before touching `communities`.
    // The `community` commands open the database directly with
    // `Connection::open` (not through `init_db`), so a legacy database
    // (`user_version=2`, without the v3 `keywords`/`topics` columns) would
    // otherwise fail with `no such column: keywords`.
    schema::init_db(&conn).context("failed to initialise/migrate the database schema")?;

    // Load, detect, and store.
    let communities = detect_communities(&conn, resolution)?;

    // Replace the previous communities atomically: a failure while inserting
    // the new set rolls back the deletion, so the table is never left empty.
    replace_communities(&conn, &communities, resolution)?;

    let num_level1 = communities.iter().filter(|(l, _, _)| *l == 1).count();
    let num_level2 = communities.iter().filter(|(l, _, _)| *l == 2).count();

    Ok((num_level1, num_level2))
}

/// Atomically replace every stored community with `communities`.
///
/// The previous rows are deleted and the new ones inserted inside a single
/// transaction: if any insertion fails, the whole operation is rolled back
/// and the existing communities are preserved (the table never ends up empty
/// because of a mid-operation failure).
fn replace_communities(
    conn: &Connection,
    communities: &[(i32, Vec<i64>, String)],
    resolution: f64,
) -> Result<()> {
    let tx = conn
        .unchecked_transaction()
        .context("failed to start communities transaction")?;

    delete_all_communities(&tx).context("failed to clear existing communities")?;

    for (level, member_ids, label) in communities {
        let community = Community {
            id: 0, // auto-assigned by SQLite
            label: label.clone(),
            level: *level,
            parent_id: None,
            summary: None,
            summary_embedding: None,
            member_ids: member_ids.clone(),
            member_count: member_ids.len() as i32,
            algorithm: "leiden".into(),
            quality_fn: "cpm".into(),
            resolution,
            summary_model: None,
            embed_model: None,
            summary_tokens: None,
            keywords: Vec::new(),
            topics: Vec::new(),
        };
        insert_community(&tx, &community).context("failed to insert community")?;
    }

    tx.commit()
        .context("failed to commit communities transaction")?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::init_db;

    /// Open an in-memory database and initialise the schema.
    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn
    }

    /// Insert an entity node.
    fn insert_node(conn: &Connection, id: i64, label: &str, type_: &str) {
        let key = if type_ == "note" {
            crate::db::keys::note_key(label)
        } else {
            crate::db::keys::node_key(label)
        };
        conn.execute(
            "INSERT INTO nodes (id, key, label, type, metadata) VALUES (?1, ?2, ?3, ?4, '{}')",
            rusqlite::params![id, key, label, type_],
        )
        .unwrap();
    }

    /// Insert a co_occurs_with edge.
    fn insert_edge(conn: &Connection, src: i64, dst: i64, weight: f64) {
        conn.execute(
            "INSERT INTO edges (source_id, target_id, type, weight, context) \
             VALUES (?1, ?2, 'co_occurs_with', ?3, 'test')",
            rusqlite::params![src, dst, weight],
        )
        .unwrap();
    }

    /// Downgrade an initialised database to `user_version=2` by stripping the
    /// v3 `keywords`/`topics` columns from `communities` (fixture for the
    /// regression: the `community` commands used to open the DB without
    /// running `init_db`, so a v2 database failed with
    /// `no such column: keywords`).
    fn downgrade_to_v2(conn: &Connection) {
        conn.execute_batch(
            "ALTER TABLE communities DROP COLUMN keywords;
             ALTER TABLE communities DROP COLUMN topics;
             PRAGMA user_version=2;",
        )
        .expect("v2 downgrade fixture should apply");
    }

    /// Read the current `PRAGMA user_version`.
    fn user_version(conn: &Connection) -> i64 {
        conn.query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap()
    }

    /// Populate a 6-entity, 2-cluster graph (deterministic clusters for
    /// Leiden) in the given connection.
    fn insert_two_clusters(conn: &Connection) {
        for i in 1..=6 {
            insert_node(conn, i, &format!("entity_{i}"), "language");
        }
        insert_edge(conn, 1, 2, 0.9);
        insert_edge(conn, 1, 3, 0.8);
        insert_edge(conn, 2, 3, 0.7);
        insert_edge(conn, 4, 5, 0.9);
        insert_edge(conn, 4, 6, 0.8);
        insert_edge(conn, 5, 6, 0.7);
        insert_edge(conn, 3, 4, 0.1);
        insert_edge(conn, 2, 5, 0.05);
    }

    // ------------------------------------------------------------------
    // test_load_entity_graph
    // ------------------------------------------------------------------

    #[test]
    fn test_load_entity_graph() {
        let conn = setup_db();

        // Insert entities (should be loaded).
        insert_node(&conn, 1, "Rust", "language");
        insert_node(&conn, 2, "Python", "language");
        insert_node(&conn, 3, "Cargo", "tool");

        // Insert a note and a tag (should be excluded).
        insert_node(&conn, 10, "Some Note", "note");
        insert_node(&conn, 11, "Some Tag", "tag");

        // Insert co_occurs_with edges.
        insert_edge(&conn, 1, 2, 0.8);
        insert_edge(&conn, 2, 3, 0.5);
        // Edge referencing a note — still loaded from edges table
        // but will be skipped during graph construction.
        insert_edge(&conn, 1, 10, 0.3);

        let (nodes, edges) = load_entity_graph(&conn).expect("load should succeed");

        // Only 3 entity nodes (notes/tags excluded).
        assert_eq!(nodes.len(), 3, "should load 3 entity nodes");
        assert_eq!(nodes[0].id, 1);
        assert_eq!(nodes[0].label, "Rust");
        assert_eq!(nodes[1].id, 2);
        assert_eq!(nodes[1].label, "Python");
        assert_eq!(nodes[2].id, 3);
        assert_eq!(nodes[2].label, "Cargo");

        // All 3 edges are returned (filtering happens in detect_communities).
        assert_eq!(edges.len(), 3, "should load 3 edges");
    }

    // ------------------------------------------------------------------
    // test_detect_communities_basic
    // ------------------------------------------------------------------

    #[test]
    fn test_detect_communities_basic() {
        let conn = setup_db();

        // Create 6 entities forming 2 clear clusters:
        //   Cluster A: {1, 2, 3} fully connected with high weights
        //   Cluster B: {4, 5, 6} fully connected with high weights
        //   Weak cross edges between clusters
        for i in 1..=6 {
            insert_node(&conn, i, &format!("entity_{i}"), "language");
        }

        // Cluster A (1, 2, 3).
        insert_edge(&conn, 1, 2, 0.9);
        insert_edge(&conn, 1, 3, 0.8);
        insert_edge(&conn, 2, 3, 0.7);

        // Cluster B (4, 5, 6).
        insert_edge(&conn, 4, 5, 0.9);
        insert_edge(&conn, 4, 6, 0.8);
        insert_edge(&conn, 5, 6, 0.7);

        // Weak cross edges.
        insert_edge(&conn, 3, 4, 0.1);
        insert_edge(&conn, 2, 5, 0.05);

        let results = detect_communities(&conn, 0.5).expect("detect should succeed");
        assert!(
            !results.is_empty(),
            "should detect at least 1 community at resolution 0.5"
        );
    }

    // ------------------------------------------------------------------
    // test_detect_empty_graph
    // ------------------------------------------------------------------

    #[test]
    fn test_detect_empty_graph() {
        let conn = setup_db();
        let results = detect_communities(&conn, 1.0).expect("detect should succeed");
        assert!(
            results.is_empty(),
            "empty graph should return no communities"
        );
    }

    // ------------------------------------------------------------------
    // test_run_community_detection_full
    // ------------------------------------------------------------------

    #[test]
    fn test_run_community_detection_full() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let db_path = tmp.path().to_str().unwrap().to_owned();

        // Insert data in a separate scope so the connection closes.
        {
            let conn = Connection::open(&db_path).unwrap();
            init_db(&conn).unwrap();

            // Same 6 entities as above.
            for i in 1..=6 {
                insert_node(&conn, i, &format!("entity_{i}"), "language");
            }

            insert_edge(&conn, 1, 2, 0.9);
            insert_edge(&conn, 1, 3, 0.8);
            insert_edge(&conn, 2, 3, 0.7);
            insert_edge(&conn, 4, 5, 0.9);
            insert_edge(&conn, 4, 6, 0.8);
            insert_edge(&conn, 5, 6, 0.7);
            insert_edge(&conn, 3, 4, 0.1);
            insert_edge(&conn, 2, 5, 0.05);
        }

        let (num_level1, num_level2) =
            run_community_detection(&db_path, 0.5).expect("full pipeline should succeed");

        // Verify the data was persisted. Small graphs may produce communities
        // only at level 0 (the first aggregation level), so we check the DB
        // directly rather than relying solely on level-1 / level-2 counts.
        let conn = Connection::open(&db_path).unwrap();
        let stored = crate::db::communities::get_all_communities(&conn).unwrap();
        assert!(
            !stored.is_empty(),
            "should have stored communities (level1={num_level1}, level2={num_level2})"
        );

        // Each community should have 2+ members and a valid label.
        for community in &stored {
            assert!(
                community.member_count >= 2,
                "community should have 2+ members"
            );
            assert!(
                community.label.starts_with("community_"),
                "label should start with 'community_', got '{}'",
                community.label
            );
        }
    }

    // ==================================================================
    // RED — fix-community-schema-migration (community-detection delta)
    //
    // `run_community_detection` MUST migrate the schema (v2 → v3) before
    // touching `communities`, and MUST replace communities atomically.
    // ==================================================================

    /// RED-1 — `run_community_detection` over a v2 database migrates the
    /// schema to v3 and inserts the detected communities without the
    /// `no such column: keywords` error.
    #[test]
    fn test_run_community_detection_migrates_v2_db() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let db_path = tmp.path().to_str().unwrap().to_owned();

        {
            let conn = Connection::open(&db_path).unwrap();
            init_db(&conn).unwrap();
            insert_two_clusters(&conn);
            downgrade_to_v2(&conn);
            assert_eq!(user_version(&conn), 2, "fixture must start at v2");
        }

        let result = run_community_detection(&db_path, 0.5);
        assert!(
            result.is_ok(),
            "detect on a v2 db must migrate and succeed, got: {result:?}"
        );

        let conn = Connection::open(&db_path).unwrap();
        assert_eq!(
            user_version(&conn),
            3,
            "detect must leave the database at user_version 3"
        );

        let stored = crate::db::communities::get_all_communities(&conn).unwrap();
        assert!(
            !stored.is_empty(),
            "communities must be inserted after the migration"
        );
    }

    /// RED-2 — the replacement of communities is atomic: if an insertion
    /// fails, the previously stored communities survive (the table is never
    /// left empty by a mid-operation failure).
    #[test]
    fn test_run_community_detection_atomic_on_insert_failure() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let db_path = tmp.path().to_str().unwrap().to_owned();

        {
            let conn = Connection::open(&db_path).unwrap();
            init_db(&conn).unwrap();
            insert_two_clusters(&conn);

            // A pre-existing community that must never be lost.
            let pre_existing = Community {
                id: 0,
                label: "pre-existing".into(),
                level: 1,
                parent_id: None,
                summary: None,
                summary_embedding: None,
                member_ids: vec![1, 2],
                member_count: 2,
                algorithm: "leiden".into(),
                quality_fn: "cpm".into(),
                resolution: 1.0,
                summary_model: None,
                embed_model: None,
                summary_tokens: None,
                keywords: vec![],
                topics: vec![],
            };
            insert_community(&conn, &pre_existing).unwrap();

            // Force every INSERT into `communities` to abort.
            conn.execute_batch(
                "CREATE TRIGGER boom BEFORE INSERT ON communities \
                 BEGIN SELECT RAISE(ABORT, 'boom'); END;",
            )
            .unwrap();
        }

        let result = run_community_detection(&db_path, 0.5);
        assert!(
            result.is_err(),
            "a forced insert failure must surface as an error"
        );

        let conn = Connection::open(&db_path).unwrap();
        let stored = crate::db::communities::get_all_communities(&conn).unwrap();
        assert!(
            !stored.is_empty(),
            "the pre-existing community must survive a failed replacement"
        );
        assert_eq!(stored.len(), 1, "only the pre-existing community remains");
        assert_eq!(stored[0].label, "pre-existing");
    }
}
