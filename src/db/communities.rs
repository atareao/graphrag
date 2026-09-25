//! Helper functions for interacting with the `communities` table.
//!
//! The `communities` table stores clusters of entities discovered by
//! Leiden community detection.  Each community has a hierarchical
//! level (0 = root, 1 = coarse, 2 = fine, etc.), an optional parent,
//! a list of member entity IDs, and an optional summary with its
//! embedding.

use anyhow::{Context, Result};
use serde_json;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single community row from the `communities` table.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Community {
    pub id: i64,
    pub label: String,
    pub level: i32,
    pub parent_id: Option<i64>,
    pub summary: Option<String>,
    pub summary_embedding: Option<Vec<u8>>,
    pub member_ids: Vec<i64>,
    pub member_count: i32,
    pub algorithm: String,
    pub quality_fn: String,
    pub resolution: f64,
    pub summary_model: Option<String>,
    pub embed_model: Option<String>,
    pub summary_tokens: Option<i64>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Insert a new community.  Returns the new row ID.
#[allow(dead_code)]
pub fn insert_community(conn: &rusqlite::Connection, community: &Community) -> Result<i64> {
    let member_ids_json = serde_json::to_string(&community.member_ids)
        .context("failed to serialise member_ids as JSON")?;

    conn.execute(
        "INSERT INTO communities \
         (label, level, parent_id, summary, summary_embedding, \
          member_ids, member_count, algorithm, quality_fn, resolution, \
          summary_model, embed_model, summary_tokens) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        rusqlite::params![
            community.label,
            community.level,
            community.parent_id,
            community.summary,
            community.summary_embedding,
            member_ids_json,
            community.member_count,
            community.algorithm,
            community.quality_fn,
            community.resolution,
            community.summary_model,
            community.embed_model,
            community.summary_tokens,
        ],
    )
    .context("failed to insert community")?;

    Ok(conn.last_insert_rowid())
}

/// Get a single community by ID.
#[allow(dead_code)]
pub fn get_community_by_id(conn: &rusqlite::Connection, id: i64) -> Result<Option<Community>> {
    let mut stmt = conn.prepare(
        "SELECT id, label, level, parent_id, summary, summary_embedding, \
                member_ids, member_count, algorithm, quality_fn, resolution, \
                summary_model, embed_model, summary_tokens \
         FROM communities WHERE id = ?1",
    )?;

    let mut rows = stmt.query_map(rusqlite::params![id], parse_community_row)?;

    match rows.next() {
        Some(Ok(community)) => Ok(Some(community)),
        Some(Err(e)) => Err(e.into()),
        None => Ok(None),
    }
}

/// Get all communities at a given level (e.g. level=1 for coarse, level=2 for fine).
#[allow(dead_code)]
pub fn get_communities_by_level(conn: &rusqlite::Connection, level: i32) -> Result<Vec<Community>> {
    let mut stmt = conn.prepare(
        "SELECT id, label, level, parent_id, summary, summary_embedding, \
                member_ids, member_count, algorithm, quality_fn, resolution, \
                summary_model, embed_model, summary_tokens \
         FROM communities WHERE level = ?1 ORDER BY id",
    )?;

    let rows = stmt.query_map(rusqlite::params![level], parse_community_row)?;
    collect_results(rows)
}

/// Get all communities (all levels).
#[allow(dead_code)]
pub fn get_all_communities(conn: &rusqlite::Connection) -> Result<Vec<Community>> {
    let mut stmt = conn.prepare(
        "SELECT id, label, level, parent_id, summary, summary_embedding, \
                member_ids, member_count, algorithm, quality_fn, resolution, \
                summary_model, embed_model, summary_tokens \
         FROM communities ORDER BY id",
    )?;

    let rows = stmt.query_map([], parse_community_row)?;
    collect_results(rows)
}

/// Update summary and related fields for an existing community.
#[allow(dead_code)]
pub fn update_community_summary(
    conn: &rusqlite::Connection,
    id: i64,
    summary: &str,
    summary_embedding: &[u8],
    summary_model: &str,
    embed_model: &str,
    summary_tokens: i64,
) -> Result<()> {
    conn.execute(
        "UPDATE communities \
         SET summary = ?1, summary_embedding = ?2, \
             summary_model = ?3, embed_model = ?4, summary_tokens = ?5 \
         WHERE id = ?6",
        rusqlite::params![
            summary,
            summary_embedding,
            summary_model,
            embed_model,
            summary_tokens,
            id
        ],
    )
    .context("failed to update community summary")?;

    Ok(())
}

/// Load all communities that have a non-NULL `summary_embedding`.
///
/// Returns `(id, embedding_vec, label, summary_text)` for each community.
#[allow(dead_code)]
#[allow(clippy::type_complexity)]
pub fn load_all_community_embeddings(
    conn: &rusqlite::Connection,
) -> Result<Vec<(i64, Vec<f32>, String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT id, summary_embedding, label, summary \
         FROM communities \
         WHERE summary_embedding IS NOT NULL \
         ORDER BY id",
    )?;

    let rows = stmt.query_map([], |row| {
        let id: i64 = row.get(0)?;
        let blob: Vec<u8> = row.get(1)?;
        let label: String = row.get(2)?;
        let summary: String = row.get(3)?;
        Ok((id, blob, label, summary))
    })?;

    let mut result = Vec::new();
    for row in rows {
        let (id, blob, label, summary) = row?;
        // Interpret the BLOB as a slice of f32 values.
        let embedding: Vec<f32> = bytemuck::cast_slice(&blob).to_vec();
        result.push((id, embedding, label, summary));
    }
    Ok(result)
}

/// Delete all communities (for regeneration).
#[allow(dead_code)]
pub fn delete_all_communities(conn: &rusqlite::Connection) -> Result<()> {
    conn.execute("DELETE FROM communities", [])
        .context("failed to delete all communities")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Parse a single row from the `communities` table into a `Community`.
#[allow(dead_code)]
fn parse_community_row(row: &rusqlite::Row) -> rusqlite::Result<Community> {
    let member_ids_json: String = row.get(6)?;
    let member_ids: Vec<i64> = serde_json::from_str(&member_ids_json).unwrap_or_default();

    Ok(Community {
        id: row.get(0)?,
        label: row.get(1)?,
        level: row.get(2)?,
        parent_id: row.get(3)?,
        summary: row.get(4)?,
        summary_embedding: row.get(5)?,
        member_ids,
        member_count: row.get(7)?,
        algorithm: row.get(8)?,
        quality_fn: row.get(9)?,
        resolution: row.get(10)?,
        summary_model: row.get(11)?,
        embed_model: row.get(12)?,
        summary_tokens: row.get(13)?,
    })
}

/// Collect the results of a `query_map` iterator into a `Vec`, wrapping
/// the first error (if any) as an `anyhow::Error`.
fn collect_results(
    rows: impl Iterator<Item = rusqlite::Result<Community>>,
) -> Result<Vec<Community>> {
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::init_db;
    use crate::vector::vector_to_blob;

    /// Open an in-memory database and initialise the schema.
    fn setup_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn
    }

    // ------------------------------------------------------------------
    // test_insert_and_get_community
    // ------------------------------------------------------------------

    #[test]
    fn test_insert_and_get_community() {
        let conn = setup_db();

        let community = Community {
            id: 0, // will be assigned by SQLite
            label: "test-community".into(),
            level: 1,
            parent_id: None,
            summary: Some("A test community".into()),
            summary_embedding: None,
            member_ids: vec![1, 2, 3],
            member_count: 3,
            algorithm: "leiden".into(),
            quality_fn: "cpm".into(),
            resolution: 1.0,
            summary_model: None,
            embed_model: None,
            summary_tokens: None,
        };

        let id = insert_community(&conn, &community).expect("insert should succeed");
        assert!(id > 0, "inserted community id should be positive");

        let fetched = get_community_by_id(&conn, id)
            .expect("get should succeed")
            .expect("community should exist");
        assert_eq!(fetched.label, "test-community");
        assert_eq!(fetched.level, 1);
        assert_eq!(fetched.member_ids, vec![1, 2, 3]);
        assert_eq!(fetched.member_count, 3);
        assert_eq!(fetched.algorithm, "leiden");
        assert_eq!(fetched.quality_fn, "cpm");
        assert_eq!(fetched.resolution, 1.0);
    }

    // ------------------------------------------------------------------
    // test_get_community_nonexistent
    // ------------------------------------------------------------------

    #[test]
    fn test_get_community_nonexistent() {
        let conn = setup_db();
        let result = get_community_by_id(&conn, 999).expect("get should succeed for missing id");
        assert!(
            result.is_none(),
            "non-existent community should return None"
        );
    }

    // ------------------------------------------------------------------
    // test_get_communities_by_level
    // ------------------------------------------------------------------

    #[test]
    fn test_get_communities_by_level() {
        let conn = setup_db();

        // Insert two communities at level 0 and one at level 1.
        for (label, level) in &[("l0-a", 0), ("l0-b", 0), ("l1-a", 1)] {
            insert_community(
                &conn,
                &Community {
                    id: 0,
                    label: label.to_string(),
                    level: *level,
                    parent_id: None,
                    summary: None,
                    summary_embedding: None,
                    member_ids: vec![],
                    member_count: 0,
                    algorithm: "leiden".into(),
                    quality_fn: "cpm".into(),
                    resolution: 1.0,
                    summary_model: None,
                    embed_model: None,
                    summary_tokens: None,
                },
            )
            .expect("insert should succeed");
        }

        let level0 = get_communities_by_level(&conn, 0).expect("get by level should succeed");
        assert_eq!(level0.len(), 2, "should have 2 communities at level 0");

        let level1 = get_communities_by_level(&conn, 1).expect("get by level should succeed");
        assert_eq!(level1.len(), 1, "should have 1 community at level 1");
        assert_eq!(level1[0].label, "l1-a");
    }

    // ------------------------------------------------------------------
    // test_get_all_communities
    // ------------------------------------------------------------------

    #[test]
    fn test_get_all_communities() {
        let conn = setup_db();

        for i in 0..3 {
            insert_community(
                &conn,
                &Community {
                    id: 0,
                    label: format!("c-{i}"),
                    level: i,
                    parent_id: None,
                    summary: None,
                    summary_embedding: None,
                    member_ids: vec![],
                    member_count: 0,
                    algorithm: "leiden".into(),
                    quality_fn: "cpm".into(),
                    resolution: 1.0,
                    summary_model: None,
                    embed_model: None,
                    summary_tokens: None,
                },
            )
            .expect("insert should succeed");
        }

        let all = get_all_communities(&conn).expect("get all should succeed");
        assert_eq!(all.len(), 3, "should return all 3 communities");
    }

    // ------------------------------------------------------------------
    // test_update_community_summary
    // ------------------------------------------------------------------

    #[test]
    fn test_update_community_summary() {
        let conn = setup_db();

        let id = insert_community(
            &conn,
            &Community {
                id: 0,
                label: "updatable".into(),
                level: 0,
                parent_id: None,
                summary: None,
                summary_embedding: None,
                member_ids: vec![],
                member_count: 0,
                algorithm: "leiden".into(),
                quality_fn: "cpm".into(),
                resolution: 1.0,
                summary_model: None,
                embed_model: None,
                summary_tokens: None,
            },
        )
        .expect("insert should succeed");

        let emb = vector_to_blob(&[0.1_f32, 0.2_f32, 0.3_f32]);
        update_community_summary(&conn, id, "Updated summary", &emb, "model-x", "embed-y", 42)
            .expect("update should succeed");

        let updated = get_community_by_id(&conn, id)
            .expect("get should succeed")
            .expect("community should exist");
        assert_eq!(updated.summary, Some("Updated summary".into()));
        assert_eq!(updated.summary_model, Some("model-x".into()));
        assert_eq!(updated.embed_model, Some("embed-y".into()));
        assert_eq!(updated.summary_tokens, Some(42));
        assert!(
            updated.summary_embedding.is_some(),
            "summary_embedding should be set"
        );
    }

    // ------------------------------------------------------------------
    // test_load_all_community_embeddings
    // ------------------------------------------------------------------

    #[test]
    fn test_load_all_community_embeddings() {
        let conn = setup_db();

        // Community with embedding.
        let emb = vector_to_blob(&[0.5_f32, 0.25_f32]);
        let id1 = insert_community(
            &conn,
            &Community {
                id: 0,
                label: "emb-c".into(),
                level: 0,
                parent_id: None,
                summary: Some("Has embedding".into()),
                summary_embedding: Some(emb.clone()),
                member_ids: vec![],
                member_count: 0,
                algorithm: "leiden".into(),
                quality_fn: "cpm".into(),
                resolution: 1.0,
                summary_model: Some("m".into()),
                embed_model: Some("e".into()),
                summary_tokens: Some(10),
            },
        )
        .expect("insert should succeed");

        // Community without embedding.
        insert_community(
            &conn,
            &Community {
                id: 0,
                label: "no-emb".into(),
                level: 0,
                parent_id: None,
                summary: Some("No embedding".into()),
                summary_embedding: None,
                member_ids: vec![],
                member_count: 0,
                algorithm: "leiden".into(),
                quality_fn: "cpm".into(),
                resolution: 1.0,
                summary_model: None,
                embed_model: None,
                summary_tokens: None,
            },
        )
        .expect("insert should succeed");

        let embeddings =
            load_all_community_embeddings(&conn).expect("load embeddings should succeed");

        assert_eq!(embeddings.len(), 1, "only one community has an embedding");
        assert_eq!(embeddings[0].0, id1, "id should match");
        assert_eq!(
            embeddings[0].1,
            vec![0.5_f32, 0.25_f32],
            "embedding should match"
        );
        assert_eq!(embeddings[0].2, "emb-c", "label should match");
        assert_eq!(embeddings[0].3, "Has embedding", "summary should match");
    }

    // ------------------------------------------------------------------
    // test_delete_all_communities
    // ------------------------------------------------------------------

    #[test]
    fn test_delete_all_communities() {
        let conn = setup_db();

        for i in 0..2 {
            insert_community(
                &conn,
                &Community {
                    id: 0,
                    label: format!("del-{i}"),
                    level: 0,
                    parent_id: None,
                    summary: None,
                    summary_embedding: None,
                    member_ids: vec![],
                    member_count: 0,
                    algorithm: "leiden".into(),
                    quality_fn: "cpm".into(),
                    resolution: 1.0,
                    summary_model: None,
                    embed_model: None,
                    summary_tokens: None,
                },
            )
            .expect("insert should succeed");
        }

        assert_eq!(
            get_all_communities(&conn).unwrap().len(),
            2,
            "should have 2 communities before delete"
        );

        delete_all_communities(&conn).expect("delete should succeed");

        assert_eq!(
            get_all_communities(&conn).unwrap().len(),
            0,
            "should have 0 communities after delete"
        );
    }
}
