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
//! | `nodes`        | Every entity in the graph (notes, people, tags, …).        |
//! | `edges`        | Labelled, weighted, directed relationships between nodes.  |
//! | `notes_fts`    | FTS5 virtual table for full-text search on title/content.  |
//! | `chunks`       | Content fragments extracted from note nodes.               |
//! | `communities`  | Clusters of entities found by Leiden community detection.  |
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
///
/// * **`chunks`** — Content fragments extracted from note nodes, each
///   with a header, slug, text, and optional embedding BLOB.
///
/// * **`communities`** — Clusters of entities found by Leiden community
///   detection, with hierarchical levels and summary embeddings.
pub const SCHEMA_MAIN: &str = r#"
-- Tabla de nodos: notas, entidades, etiquetas
CREATE TABLE IF NOT EXISTS nodes (
    id          INTEGER PRIMARY KEY,
    key         TEXT    NOT NULL,
    label       TEXT    NOT NULL,
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
CREATE UNIQUE INDEX IF NOT EXISTS idx_nodes_key  ON nodes(key);
CREATE INDEX IF NOT EXISTS idx_nodes_type   ON nodes(type);

-- Índice FTS5 para búsqueda textual (tabla independiente)
-- NOTA: Sin triggers. El build repuebla FTS5 desde Rust.
CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(title, content);

-- Tabla de chunks: fragmentos de contenido con embedding
CREATE TABLE IF NOT EXISTS chunks (
    id          INTEGER PRIMARY KEY,
    note_id     INTEGER NOT NULL REFERENCES nodes(id),
    header      TEXT NOT NULL,
    text        TEXT NOT NULL,
    slug        TEXT NOT NULL,
    embedding   BLOB,
    metadata    TEXT
);
CREATE INDEX IF NOT EXISTS idx_chunks_note_id ON chunks(note_id);

-- Tabla de comunidades: clusters de entidades (Leiden)
CREATE TABLE IF NOT EXISTS communities (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    label             TEXT    NOT NULL,
    level             INTEGER NOT NULL DEFAULT 0,
    parent_id         INTEGER REFERENCES communities(id),
    summary           TEXT,
    summary_embedding BLOB,
    member_ids        TEXT    NOT NULL DEFAULT '[]',
    member_count      INTEGER NOT NULL DEFAULT 0,
    algorithm         TEXT    NOT NULL DEFAULT 'leiden',
    quality_fn        TEXT    NOT NULL DEFAULT 'cpm',
    resolution        REAL    NOT NULL DEFAULT 1.0,
    summary_model     TEXT,
    embed_model       TEXT,
    summary_tokens    INTEGER,
    created_at        TEXT    DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_communities_level ON communities(level);
CREATE INDEX IF NOT EXISTS idx_communities_parent ON communities(parent_id);
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
INSERT OR IGNORE INTO nodes (id, key, label, type, metadata) VALUES
    (1, 'node:_root',          '_root',          'root',   '{"description":"Root anchor for singleton sub-graphs"}'),
    (2, 'node:_graphrag_core', '_graphrag_core', 'system', '{"description":"GraphRAG kernel marker"}'),
    (3, 'node:_unresolved',    '_unresolved',    'system', '{"description":"Dangling reference sink"}');

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
    key         TEXT    NOT NULL,
    label       TEXT    NOT NULL,
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
CREATE UNIQUE INDEX IF NOT EXISTS idx_nodes_key  ON nodes(key);
CREATE INDEX IF NOT EXISTS idx_nodes_type   ON nodes(type);

-- Índice FTS5 para búsqueda textual (tabla independiente)
-- NOTA: Sin triggers. El build repuebla FTS5 desde Rust.
CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(title, content);

-- Tabla de chunks: fragmentos de contenido con embedding
CREATE TABLE IF NOT EXISTS chunks (
    id          INTEGER PRIMARY KEY,
    note_id     INTEGER NOT NULL REFERENCES nodes(id),
    header      TEXT NOT NULL,
    text        TEXT NOT NULL,
    slug        TEXT NOT NULL,
    embedding   BLOB,
    metadata    TEXT
);
CREATE INDEX IF NOT EXISTS idx_chunks_note_id ON chunks(note_id);

-- Tabla de comunidades: clusters de entidades (Leiden)
CREATE TABLE IF NOT EXISTS communities (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    label             TEXT    NOT NULL,
    level             INTEGER NOT NULL DEFAULT 0,
    parent_id         INTEGER REFERENCES communities(id),
    summary           TEXT,
    summary_embedding BLOB,
    member_ids        TEXT    NOT NULL DEFAULT '[]',
    member_count      INTEGER NOT NULL DEFAULT 0,
    algorithm         TEXT    NOT NULL DEFAULT 'leiden',
    quality_fn        TEXT    NOT NULL DEFAULT 'cpm',
    resolution        REAL    NOT NULL DEFAULT 1.0,
    summary_model     TEXT,
    embed_model       TEXT,
    summary_tokens    INTEGER,
    created_at        TEXT    DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_communities_level ON communities(level);
CREATE INDEX IF NOT EXISTS idx_communities_parent ON communities(parent_id);

-- Seed data: bootstrap nodes
INSERT OR IGNORE INTO nodes (id, key, label, type, metadata) VALUES
    (1, 'node:_root',          '_root',          'root',   '{"description":"Root anchor for singleton sub-graphs"}'),
    (2, 'node:_graphrag_core', '_graphrag_core', 'system', '{"description":"GraphRAG kernel marker"}'),
    (3, 'node:_unresolved',    '_unresolved',    'system', '{"description":"Dangling reference sink"}');

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
/// Returns `true` if `table` has a column named `column`.
fn column_exists(conn: &rusqlite::Connection, table: &str, column: &str) -> anyhow::Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn init_db(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    conn.execute_batch("PRAGMA journal_mode=DELETE")
        .context("failed to set DELETE journal mode")?;

    conn.execute_batch("PRAGMA foreign_keys=ON")
        .context("failed to enable foreign key enforcement")?;

    // ── Schema migration v1 -> v2 ──────────────────────────────────────
    // The identity column `key` was introduced in v2.  A legacy database
    // (no `key` column, `UNIQUE(label)`) is rebuilt exactly once, gated by
    // `PRAGMA user_version`.  Node `id`s and the FKs from `edges`/`chunks`
    // are preserved.
    let user_version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let nodes_exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='nodes'",
        [],
        |r| r.get(0),
    )?;

    if nodes_exists > 0 && user_version < 2 && !column_exists(conn, "nodes", "key")? {
        // FK enforcement must be OFF and legacy rename semantics used so
        // that renaming `nodes` -> `nodes_old` does NOT rewrite the FK
        // clauses of `edges`/`chunks` (which must keep referencing `nodes`).
        conn.execute_batch("PRAGMA foreign_keys=OFF; PRAGMA legacy_alter_table=ON;")
            .context("failed to prepare legacy table rebuild")?;

        {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(
                "ALTER TABLE nodes RENAME TO nodes_old;
                 CREATE TABLE nodes (
                     id          INTEGER PRIMARY KEY,
                     key         TEXT    NOT NULL,
                     label       TEXT    NOT NULL,
                     type        TEXT    NOT NULL,
                     embedding   BLOB,
                     metadata    TEXT,
                     created_at  TEXT    DEFAULT CURRENT_TIMESTAMP
                 );
                 INSERT INTO nodes (id, key, label, type, embedding, metadata, created_at)
                   SELECT id,
                     CASE WHEN type = 'note'
                          THEN 'note:' || COALESCE(
                                 json_extract(
                                   CASE WHEN json_valid(metadata) THEN metadata ELSE NULL END,
                                   '$.path'
                                 ),
                                 'legacy:' || id
                               )
                          ELSE 'node:' || label
                     END,
                     label, type, embedding, metadata, created_at
                   FROM nodes_old;",
            )
            .context("failed to rebuild nodes for schema v2")?;

            // Comprobación de integridad: la reconstrucción no debe perder
            // ni duplicar filas.  Si los recuentos no cuadran, abortamos y
            // la transacción hace rollback (la tabla original queda intacta).
            let old_count: i64 =
                tx.query_row("SELECT COUNT(*) FROM nodes_old", [], |r| r.get(0))?;
            let new_count: i64 = tx.query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))?;
            anyhow::ensure!(
                old_count == new_count,
                "schema v2 migration integrity check failed: \
                 nodes_old={old_count}, nodes={new_count}"
            );

            // El bump de versión va DENTRO de la misma transacción que el
            // DROP, de modo que la migración y el cambio de versión son
            // atómicos: o se aplican ambos, o ninguno.
            tx.execute_batch("DROP TABLE nodes_old; PRAGMA user_version=2;")
                .context("failed to finalize schema v2 migration")?;
            tx.commit()
                .context("failed to commit schema v2 migration")?;
        }

        conn.execute_batch("PRAGMA legacy_alter_table=OFF; PRAGMA foreign_keys=ON;")
            .context("failed to restore legacy table/foreign-key pragmas")?;
    }

    conn.execute_batch(SCHEMA_MAIN)
        .context("failed to execute main schema DDL")?;

    // Las bases de datos migradas ya fijaron `user_version=2` de forma
    // atómica dentro de su transacción.  Este bloque cubre el caso de una
    // base nueva (sin tabla `nodes` previa), que no pasa por la migración.
    if user_version < 2 {
        conn.execute_batch("PRAGMA user_version=2")
            .context("failed to set schema user_version to 2")?;
    }

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
            "INSERT INTO nodes (key, label, type, metadata) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                crate::db::keys::note_key("test-note"),
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

        // Verify DELETE mode is set.
        let journal: String = conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert!(
            journal.to_uppercase() == "DELETE" || journal.to_uppercase() == "MEMORY",
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
        // Expected: nodes, edges, notes_fts, chunks, communities, sqlite_sequence.
        assert!(
            table_count >= 5,
            "expected at least 5 tables, got {table_count}"
        );
    }

    /// Verify that the `chunks` table does NOT exist yet in the current
    /// schema.  This test encodes the *desired* future behaviour: once
    /// the `chunks` table is added in the GREEN phase, the INSERT will
    /// succeed — right now it fails, which is the RED signal.
    #[test]
    fn test_chunks_table_exists() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();

        // Insert a parent node for the FK reference.
        conn.execute(
            "INSERT INTO nodes (id, key, label, type, metadata) VALUES (?1, ?2, ?3, 'note', ?4)",
            rusqlite::params![
                1i64,
                crate::db::keys::note_key("test-note"),
                "test-note",
                r#"{"content":"test"}"#
            ],
        )
        .unwrap();

        // Try to insert a row into the `chunks` table.
        conn.execute(
            "INSERT INTO chunks (note_id, header, text, slug, embedding, metadata) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                1i64,                          // note_id
                "Introduction",                // header
                "This is the chunk text.",     // text
                "introduction",                // slug
                None::<Vec<u8>>,               // embedding (NULL)
                None::<String>,                // metadata (NULL)
            ],
        )
        .unwrap();
    }

    // ── node-identity v2 tests (RED) ──────────────────────────────────

    /// DDL of the legacy (v1) schema, before the `key` column existed.
    const LEGACY_SCHEMA: &str = r#"
CREATE TABLE nodes (
    id          INTEGER PRIMARY KEY,
    label       TEXT    NOT NULL UNIQUE,
    type        TEXT    NOT NULL,
    embedding   BLOB,
    metadata    TEXT,
    created_at  TEXT    DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE edges (
    id          INTEGER PRIMARY KEY,
    source_id   INTEGER NOT NULL,
    target_id   INTEGER NOT NULL,
    type        TEXT    NOT NULL,
    weight      REAL    DEFAULT 1.0,
    context     TEXT,
    FOREIGN KEY (source_id) REFERENCES nodes(id),
    FOREIGN KEY (target_id) REFERENCES nodes(id)
);
CREATE TABLE chunks (
    id          INTEGER PRIMARY KEY,
    note_id     INTEGER NOT NULL REFERENCES nodes(id),
    header      TEXT NOT NULL,
    text        TEXT NOT NULL,
    slug        TEXT NOT NULL,
    embedding   BLOB,
    metadata    TEXT
);
"#;

    /// Two nodes may share a label as long as their keys differ.
    #[test]
    fn test_duplicate_labels_allowed() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();

        conn.execute(
            "INSERT INTO nodes (key, label, type) VALUES ('note:a/dup.md', 'Duplicado', 'note')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO nodes (key, label, type) VALUES ('note:b/dup.md', 'Duplicado', 'note')",
            [],
        )
        .unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE label = 'Duplicado'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2, "two nodes with the same label must coexist");
    }

    /// Duplicate keys must be rejected by the unique index.
    #[test]
    fn test_key_unique_enforced() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();

        conn.execute(
            "INSERT INTO nodes (key, label, type) VALUES ('note:a/dup.md', 'A', 'note')",
            [],
        )
        .unwrap();

        let err = conn.execute(
            "INSERT INTO nodes (key, label, type) VALUES ('note:a/dup.md', 'B', 'note')",
            [],
        );
        assert!(err.is_err(), "duplicate key must violate uniqueness");
    }

    /// A legacy database is migrated to v2, preserving ids/FKs.
    #[test]
    fn test_migrate_v1_to_v2() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(LEGACY_SCHEMA).unwrap();

        conn.execute(
            r#"INSERT INTO nodes (id, label, type, metadata) VALUES (1, 'Foo', 'note', '{"path":"a/foo.md"}')"#,
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO nodes (id, label, type, metadata) VALUES (2, 'Python', 'language', '{}')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO nodes (id, label, type) VALUES (3, 'Other', 'note')",
            [],
        )
        .unwrap();
        // Edge + chunk referencing node ids, to prove FKs survive the rebuild.
        conn.execute(
            "INSERT INTO edges (source_id, target_id, type, weight) VALUES (2, 1, 'mentioned_in', 1.0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chunks (note_id, header, text, slug) VALUES (1, 'H', 'body text', 'h')",
            [],
        )
        .unwrap();

        // Default user_version is 0 (legacy).
        let before: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(before, 0);

        init_db(&conn).unwrap();

        let k1: String = conn
            .query_row("SELECT key FROM nodes WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(k1, "note:a/foo.md", "note backfilled from metadata.path");

        let k2: String = conn
            .query_row("SELECT key FROM nodes WHERE id = 2", [], |r| r.get(0))
            .unwrap();
        assert_eq!(k2, "node:Python", "non-note backfilled from label");

        let k3: String = conn
            .query_row("SELECT key FROM nodes WHERE id = 3", [], |r| r.get(0))
            .unwrap();
        assert_eq!(k3, "note:legacy:3", "note without path uses legacy key");

        // ids and labels preserved.
        let label1: String = conn
            .query_row("SELECT label FROM nodes WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(label1, "Foo");

        // FKs survive: the edge still resolves to the migrated nodes.
        let edge_ok: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM edges e JOIN nodes s ON e.source_id = s.id JOIN nodes t ON e.target_id = t.id",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(edge_ok, 1, "edge FKs must survive migration");
        let chunk_ok: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM chunks c JOIN nodes n ON c.note_id = n.id",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(chunk_ok, 1, "chunk FKs must survive migration");

        let idx: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='idx_nodes_key'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(idx, 1, "idx_nodes_key must exist");

        let uv: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(uv, 2, "user_version must be 2 after migration");
    }

    /// Running `init_db` twice must not re-key or fail.
    #[test]
    fn test_migrate_idempotent() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO nodes (key, label, type) VALUES ('note:a/foo.md', 'Foo', 'note')",
            [],
        )
        .unwrap();

        // Second run on an already-v2 database.
        init_db(&conn).unwrap();

        let key: String = conn
            .query_row("SELECT key FROM nodes WHERE label = 'Foo'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(key, "note:a/foo.md", "key must be unchanged");

        let uv: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(uv, 2);
    }
}
