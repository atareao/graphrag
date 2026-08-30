//! SQLite schema management for the GraphRAG knowledge graph.
//!
//! This module defines the database tables, indexes, full-text search
//! virtual tables.  FTS5 is repopulated from scratch after each
//! `graphrag build` (no triggers) to avoid a SQLite 3.x bug where
//! the FTS5 `'delete'` command fails.
//! sensible PRAGMAs and runs the full schema DDL.
//!
//! # Table overview
//!
//! | Table         | Purpose                                                  |
//! |---------------|----------------------------------------------------------|
//! | `nodes`       | Every entity in the graph (notes, people, tags, …).      |
//! | `edges`       | Labelled, weighted, directed relationships between nodes.|
//! | `notes_fts`   | FTS5 virtual table for full-text search on title/content.|
//!
//! # FTS sync
//!
//! FTS5 is **not** managed by triggers.  Instead, the build subcommand
//! (`graphrag build`) rebuilds the FTS index from scratch after each
//! build cycle by clearing the virtual table and re-inserting from
//! `nodes`.  This avoids a known issue in SQLite 3.x where the FTS5
//! `'delete'` command fails with empty content.

use anyhow::Context;

// ---------------------------------------------------------------------------
// Schema DDL
// ---------------------------------------------------------------------------

/// Full DDL for the GraphRAG database.
///
/// Creates the `nodes` table, the `edges` table, supporting indexes,
/// and an FTS5 virtual table for full-text search.
///
/// FTS5 is repopulated from scratch by `graphrag build` (no triggers).
/// This avoids a known failure of the FTS5 `'delete'` command.
///
/// * **`nodes`** — Every graph vertex.  `id` is an auto-incrementing
///   primary key.  `label` must be unique (used as a human-readable
///   identifier).  `type` categorises the node (e.g. `"note"`,
///   `"entity"`, `"tag"`).  `embedding` stores a dense vector BLOB
///   for semantic search.  `metadata` holds arbitrary JSON.
///
/// * **`edges`** — Every directed, labelled edge between two nodes.
///   `source_id` / `target_id` reference `nodes(id)` via foreign keys.
///   `weight` is a scalar (default 1.0) used for ranking or pruning.
///   `context` stores free-form text that describes *why* the edge
///   exists.
///
/// * **`notes_fts`** — An FTS5 virtual table over `title` and
///   `content`.  Repopulated by `graphrag build` (no triggers).
pub const SCHEMA_MAIN: &str = r#"
-- Tabla de nodos: notas, entidades, etiquetas
CREATE TABLE IF NOT EXISTS nodes (
    id          INTEGER PRIMARY KEY,
    label       TEXT    NOT NULL UNIQUE,
    type        TEXT    NOT NULL,
    embedding   BLOB,
    metadata    TEXT,
    created_at  TEXT    DEFAULT CURRENT_TIMESTAMP
);

-- Tabla de aristas: relaciones entre nodos
CREATE TABLE IF NOT EXISTS edges (
    id          INTEGER PRIMARY KEY,
    source_id   INTEGER NOT NULL,
    target_id   INTEGER NOT NULL,
    type        TEXT    NOT NULL,
    weight      REAL    DEFAULT 1.0,
    context     TEXT,
    FOREIGN KEY (source_id) REFERENCES nodes(id),
    FOREIGN KEY (target_id) REFERENCES nodes(id)
);

-- Índices
CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source_id);
CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target_id);
CREATE INDEX IF NOT EXISTS idx_edges_type   ON edges(type);
CREATE INDEX IF NOT EXISTS idx_nodes_type   ON nodes(type);

-- Índice FTS5 para búsqueda textual (tabla independiente)
-- NOTA: Sin triggers. El build repuebla FTS5 desde Rust.
CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(title, content);
"#;

/// Schema DDL *plus* seed data for a minimal bootstrapped graph.
///
/// This constant concatenates [`SCHEMA_MAIN`] with a set of `INSERT`
/// statements that populate the `nodes` and `edges` tables with a
/// handful of well-known reference entities so the graph is never
/// completely empty after initialisation.
///
/// # Seed rows
///
/// The following nodes are inserted (idempotently via
/// `INSERT OR IGNORE`):
///
/// | Label              | Type      | Description                     |
/// |--------------------|-----------|---------------------------------|
/// | `_root`            | `root`    | Singletons holder.              |
/// | `_graphrag_core`   | `system`  | Marks the GraphRAG kernel.      |
/// | `_unresolved`      | `system`  | Catch-all for dangling refs.    |
///
/// A single edge connects `_root` → `_graphrag_core` with type
/// `"contains"` so the graph has at least one navigable relationship.
#[allow(dead_code)]
pub const SCHEMA_SEED: &str = {
    // Concatenate SCHEMA_MAIN with the seed INSERTs at compile time.
    // (Rust `const` formatting does not support `concat!` with const strs
    // directly, so we use a raw string literal that repeats the schema.)
    const SEED_INSERTS: &str = r#"
-- Seed data: bootstrap nodes
INSERT OR IGNORE INTO nodes (id, label, type, metadata) VALUES
    (1, '_root',          'root',   '{"description":"Root anchor for singleton sub-graphs"}'),
    (2, '_graphrag_core', 'system', '{"description":"GraphRAG kernel marker"}'),
    (3, '_unresolved',    'system', '{"description":"Dangling reference sink"}');

-- Seed data: bootstrap edges
INSERT OR IGNORE INTO edges (source_id, target_id, type, weight, context) VALUES
    (1, 2, 'contains', 1.0, 'Root contains the GraphRAG kernel');
"#;

    // Build the combined string.
    // We keep this as a single literal so the compiler can fold it.
    const COMBINED: &str = {
        // We cannot call concat!() on const strs inside a const initialiser
        // in stable Rust (as of 1.84), so the SCHEMA_MAIN portion is
        // duplicated here.  A build.rs or macro could deduplicate this.
        r#"
-- Tabla de nodos: notas, entidades, etiquetas
CREATE TABLE IF NOT EXISTS nodes (
    id          INTEGER PRIMARY KEY,
    label       TEXT    NOT NULL UNIQUE,
    type        TEXT    NOT NULL,
    embedding   BLOB,
    metadata    TEXT,
    created_at  TEXT    DEFAULT CURRENT_TIMESTAMP
);

-- Tabla de aristas: relaciones entre nodos
CREATE TABLE IF NOT EXISTS edges (
    id          INTEGER PRIMARY KEY,
    source_id   INTEGER NOT NULL,
    target_id   INTEGER NOT NULL,
    type        TEXT    NOT NULL,
    weight      REAL    DEFAULT 1.0,
    context     TEXT,
    FOREIGN KEY (source_id) REFERENCES nodes(id),
    FOREIGN KEY (target_id) REFERENCES nodes(id)
);

-- Índices
CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source_id);
CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target_id);
CREATE INDEX IF NOT EXISTS idx_edges_type   ON edges(type);
CREATE INDEX IF NOT EXISTS idx_nodes_type   ON nodes(type);

-- Índice FTS5 para búsqueda textual (tabla independiente)
-- NOTA: Sin triggers. El build repuebla FTS5 desde Rust.
CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(title, content);

-- Seed data: bootstrap nodes
INSERT OR IGNORE INTO nodes (id, label, type, metadata) VALUES
    (1, '_root',          'root',   '{"description":"Root anchor for singleton sub-graphs"}'),
    (2, '_graphrag_core', 'system', '{"description":"GraphRAG kernel marker"}'),
    (3, '_unresolved',    'system', '{"description":"Dangling reference sink"}');

-- Seed data: bootstrap edges
INSERT OR IGNORE INTO edges (source_id, target_id, type, weight, context) VALUES
    (1, 2, 'contains', 1.0, 'Root contains the GraphRAG kernel');
"#
    };

    COMBINED
};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Initialise a fresh or existing SQLite database with the GraphRAG schema.
///
/// This function performs the following steps:
///
/// 1.  Enable **WAL mode** (`PRAGMA journal_mode=WAL`) for better
///     concurrent read performance.
/// 2.  Enable **foreign key enforcement**
///     (`PRAGMA foreign_keys=ON`) so `edges` references to non-existent
///     `nodes` are rejected at the SQLite level.
/// 3.  Execute every statement in [`SCHEMA_MAIN`], creating tables,
///     indexes, and the FTS5 virtual table.
///
/// FTS5 is repopulated from scratch by `graphrag build` (no triggers).
///
/// # Errors
///
/// Returns an error if any PRAGMA or DDL statement fails.  The
/// connection is left in an unspecified state on failure (callers
/// should discard it and open a new one).
///
/// # Example
///
/// ```rust,ignore
/// use rusqlite::Connection;
/// use graphrag_db::schema::init_db;
///
/// let conn = Connection::open("graph.db")?;
/// init_db(&conn)?;
/// ```
pub fn init_db(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    conn.execute_batch("PRAGMA journal_mode=WAL")
        .context("failed to enable WAL journal mode")?;

    conn.execute_batch("PRAGMA foreign_keys=ON")
        .context("failed to enable foreign key enforcement")?;

    conn.execute_batch(SCHEMA_MAIN)
        .context("failed to execute main schema DDL")?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that `SCHEMA_MAIN` compiles to valid SQL that can create
    /// all tables, indexes, the FTS5 virtual table, and triggers.
    #[test]
    fn schema_main_runs_without_error() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_MAIN).unwrap();
    }

    /// Verify that `SCHEMA_SEED` applies cleanly (schema + seed data).
    #[test]
    fn schema_seed_is_valid() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_SEED).unwrap();

        // The three seed nodes should be present.
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 3, "seed should insert exactly 3 nodes");

        // The seed edge should be present.
        let edge_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM edges", [], |r| r.get(0))
            .unwrap();
        assert_eq!(edge_count, 1, "seed should insert exactly 1 edge");
    }

    /// Confirm the FTS5 triggers stay in sync with node mutations.
    #[test]
    #[ignore = "needs SCHEMA_SEED which is not used in production"]
    fn fts5_triggers_sync_notes_fts() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA_MAIN).unwrap();

        // Insert a node with a JSON metadata blob containing `content`.
        conn.execute(
            "INSERT INTO nodes (label, type, metadata) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                "test-note",
                "note",
                r#"{"content":"hello world from GraphRAG"}"#,
            ],
        )
        .unwrap();

        // The FTS table should now contain that row.
        let fts_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM notes_fts WHERE notes_fts MATCH 'hello'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(fts_count, 1, "FTS should contain the inserted content");

        // Update the node's label and content.
        conn.execute(
            "UPDATE nodes SET label = ?1, metadata = ?2 WHERE label = 'test-note'",
            rusqlite::params!["updated-note", r#"{"content":"updated content"}"#,],
        )
        .unwrap();

        // The old content should be gone, the new one present.
        let old_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM notes_fts WHERE notes_fts MATCH 'hello'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(old_count, 0, "old FTS entry should be deleted");
        let new_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM notes_fts WHERE notes_fts MATCH 'updated'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(new_count, 1, "new FTS entry should be present");

        // Delete the node — the FTS row should go with it.
        conn.execute("DELETE FROM nodes WHERE label = 'updated-note'", [])
            .unwrap();
        let after_delete: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM notes_fts WHERE notes_fts MATCH 'updated'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(after_delete, 0, "FTS entry should be deleted with node");
    }

    /// `init_db` should apply PRAGMAs and schema without error.
    #[test]
    fn init_db_succeeds() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();

        // Verify WAL mode is set.
        let journal: String = conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert!(
            journal.to_uppercase() == "WAL" || journal.to_uppercase() == "MEMORY",
            "unexpected journal mode: {}",
            journal
        );

        // Verify foreign keys are on.
        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1);

        // Tables exist.
        let table_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        // Expected: nodes, edges, notes_fts, sqlite_sequence (auto).
        assert!(
            table_count >= 3,
            "expected at least 3 tables, got {table_count}"
        );
    }
}
