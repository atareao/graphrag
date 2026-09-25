//! Helper functions for interacting with the `chunks` table.
//!
//! The `chunks` table stores content fragments extracted from note
//! nodes during the build process.  Each chunk has a header (the
//! Markdown heading under which it appeared), the raw text, a URL-safe
//! slug, and an optional embedding BLOB and metadata JSON.

use anyhow::Result as AnyhowResult;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A single row from the `chunks` table.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub struct ChunkRow {
    pub id: i64,
    pub note_id: i64,
    pub header: String,
    pub text: String,
    pub slug: String,
    pub embedding: Option<Vec<u8>>,
    pub metadata: Option<String>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Insert a new chunk row and return the auto-generated `id`.
///
/// `embedding` should be a flat f32 BLOB (e.g. from
/// [`vector_to_blob`]) or `None` to store NULL.  `metadata` should be a
/// JSON string or `None`.
pub fn insert_chunk(
    conn: &rusqlite::Connection,
    note_id: i64,
    header: &str,
    text: &str,
    slug: &str,
    embedding: Option<&[u8]>,
    metadata: Option<&str>,
) -> AnyhowResult<i64> {
    conn.execute(
        "INSERT INTO chunks (note_id, header, text, slug, embedding, metadata) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![note_id, header, text, slug, embedding, metadata],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Retrieve every chunk attached to a given note, ordered by `id`.
#[allow(dead_code)]
pub fn get_chunks_by_note(
    conn: &rusqlite::Connection,
    note_id: i64,
) -> AnyhowResult<Vec<ChunkRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, note_id, header, text, slug, embedding, metadata FROM chunks WHERE note_id = ?1 ORDER BY id",
    )?;
    let rows = stmt.query_map(rusqlite::params![note_id], |row| {
        Ok(ChunkRow {
            id: row.get(0)?,
            note_id: row.get(1)?,
            header: row.get(2)?,
            text: row.get(3)?,
            slug: row.get(4)?,
            embedding: row.get(5)?,
            metadata: row.get(6)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// Load the `(id, note_id, embedding_vector, header, text)` tuple for
/// every chunk that has a non-NULL embedding.
///
/// The returned `Vec<f32>` is the deserialised BLOB (the caller should
/// use [`blob_to_vector`] on the raw bytes internally).
#[allow(clippy::type_complexity)]
pub fn load_all_chunk_embeddings(
    conn: &rusqlite::Connection,
) -> AnyhowResult<Vec<(i64, i64, Vec<f32>, String, String)>> {
    let mut stmt =
        conn.prepare("SELECT id, note_id, embedding, header, text FROM chunks ORDER BY id")?;
    let rows = stmt.query_map([], |row| {
        let id: i64 = row.get(0)?;
        let note_id: i64 = row.get(1)?;
        let embedding: Option<Vec<u8>> = row.get(2)?;
        let header: String = row.get(3)?;
        let text: String = row.get(4)?;
        Ok((id, note_id, embedding, header, text))
    })?;
    let mut result = Vec::new();
    for row in rows {
        let (id, note_id, embedding, header, text) = row?;
        let vec = match embedding {
            Some(ref blob) => crate::vector::blob_to_vector(blob)?,
            None => Vec::new(),
        };
        result.push((id, note_id, vec, header, text));
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

    /// Open an in-memory database, initialise the schema, and insert a
    /// parent note node that the chunk FK can reference.
    fn setup_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO nodes (id, label, type, metadata) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![1i64, "test-note", "note", r#"{"content":"test"}"#],
        )
        .unwrap();
        conn
    }

    // ------------------------------------------------------------------
    // test_insert_and_get_chunks
    // ------------------------------------------------------------------

    #[test]
    fn test_insert_and_get_chunks() {
        let conn = setup_db();

        // Insert a chunk without embedding.
        let chunk_id = insert_chunk(
            &conn,
            1,
            "Intro",
            "Hello world this is a test chunk with enough text",
            "intro",
            None,
            None,
        )
        .expect("insert_chunk should succeed");

        assert!(chunk_id > 0, "chunk id should be positive");

        // Retrieve chunks by note.
        let chunks = get_chunks_by_note(&conn, 1).expect("get_chunks_by_note should succeed");
        assert_eq!(chunks.len(), 1, "should have exactly one chunk");

        assert_eq!(chunks[0].header, "Intro");
        assert_eq!(
            chunks[0].text,
            "Hello world this is a test chunk with enough text"
        );
        assert_eq!(chunks[0].slug, "intro");
        assert_eq!(chunks[0].note_id, 1);
        assert!(chunks[0].embedding.is_none());
    }

    // ------------------------------------------------------------------
    // test_load_all_chunk_embeddings
    // ------------------------------------------------------------------

    #[test]
    fn test_load_all_chunk_embeddings() {
        let conn = setup_db();

        // Chunk 1: with a non-NULL embedding (a single f32 value).
        let emb_bytes = vector_to_blob(&[0.5_f32, 0.25_f32, 0.125_f32]);
        insert_chunk(
            &conn,
            1,
            "First",
            "Chunk with embedding",
            "first",
            Some(&emb_bytes),
            None,
        )
        .expect("insert first chunk should succeed");

        // Chunk 2: with NULL embedding.
        insert_chunk(
            &conn,
            1,
            "Second",
            "Chunk without embedding",
            "second",
            None,
            None,
        )
        .expect("insert second chunk should succeed");

        let all =
            load_all_chunk_embeddings(&conn).expect("load_all_chunk_embeddings should succeed");

        // Both chunks should be returned: the first with an embedding
        // vec, the second with an empty vec (since its BLOB is NULL).
        assert_eq!(all.len(), 2, "should return 2 entries");

        // First entry: non-empty embedding.
        let (id1, note_id1, emb1, header1, text1) = &all[0];
        assert!(*id1 > 0);
        assert_eq!(*note_id1, 1);
        assert!(
            !emb1.is_empty(),
            "first chunk should have a non-empty embedding"
        );
        assert_eq!(header1, "First");
        assert_eq!(text1, "Chunk with embedding");

        // Second entry: empty embedding (NULL BLOB → empty vec).
        let (id2, note_id2, emb2, header2, text2) = &all[1];
        assert!(*id2 > 0);
        assert_eq!(*note_id2, 1);
        assert!(
            emb2.is_empty(),
            "second chunk should have an empty embedding"
        );
        assert_eq!(header2, "Second");
        assert_eq!(text2, "Chunk without embedding");
    }

    // ------------------------------------------------------------------
    // test_insert_chunk_no_embedding
    // ------------------------------------------------------------------

    #[test]
    fn test_insert_chunk_no_embedding() {
        let conn = setup_db();

        let chunk_id = insert_chunk(
            &conn,
            1,
            "Null Embed",
            "This chunk has no embedding blob",
            "null-embed",
            None,
            None,
        )
        .expect("insert_chunk should succeed");

        let chunks = get_chunks_by_note(&conn, 1).expect("get_chunks_by_note should succeed");

        // There should be exactly one chunk and its embedding is None.
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].id, chunk_id);
        assert_eq!(chunks[0].header, "Null Embed");
        assert_eq!(chunks[0].text, "This chunk has no embedding blob");
        assert!(chunks[0].embedding.is_none(), "embedding should be None");
    }
}
