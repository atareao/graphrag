//! LLM-based community summarization.
//!
//! For each community detected by the Leiden algorithm, this module
//! builds a prompt listing the community's member entities and their
//! relationships, calls Ollama's `/api/generate` to produce a structured
//! JSON summary, embeds the summary text, and persists everything back
//! into the `communities` table.

use anyhow::{Context, Result};
use indicatif::ProgressBar;
use log::{info, warn};
use rusqlite::Connection;

use crate::db::communities;
use crate::embed::ollama::OllamaClient;
use crate::vector::vector_to_blob;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Summarization result for one community.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SummaryResult {
    pub community_id: i64,
    pub summary: String,
    pub keywords: Vec<String>,
    pub topics: Vec<String>,
    pub tokens: i64,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Build the LLM prompt for summarizing a community.
///
/// Takes the community label, member entity labels, and their relationships
/// (edges) with weights, and formats them into a prompt that asks the LLM
/// for a structured JSON response with `summary`, `keywords`, and `topics`.
#[allow(dead_code)]
pub fn build_community_prompt(
    community_label: &str,
    entity_labels: &[String],
    relationships: &[(String, String, f64)],
) -> String {
    let entities = entity_labels.join(", ");

    let mut rel_lines = String::new();
    for (i, (a, b, weight)) in relationships.iter().enumerate() {
        if i > 0 {
            rel_lines.push('\n');
        }
        rel_lines.push_str(&format!("- {} --[{:.2}]--> {}", a, weight, b));
    }

    format!(
        r#"You are analyzing a community of related entities from a knowledge graph.

Community: {community_label}

Member entities:
{entities}

Relationships between them:
{rel_lines}

Provide a concise summary describing what this community represents, what the entities have in common, and their overall purpose or domain. Also extract keywords and topics.

Respond in the following JSON format only:
{{
  "summary": "concise summary text",
  "keywords": ["keyword1", "keyword2"],
  "topics": ["topic1", "topic2"]
}}
"#
    )
}

/// Summarize a single community by calling Ollama.
///
/// Queries the database for community info and member relationships,
/// builds a prompt via [`build_community_prompt`], calls the LLM, and
/// parses the structured JSON response. Retries once if parsing fails.
#[allow(dead_code)]
pub fn summarize_community(
    client: &OllamaClient,
    conn: &Connection,
    community_id: i64,
    summary_model: &str,
) -> Result<SummaryResult> {
    // Get community info
    let community = communities::get_community_by_id(conn, community_id)?
        .with_context(|| format!("community {community_id} not found"))?;

    let label = &community.label;
    let member_ids = &community.member_ids;

    if member_ids.is_empty() {
        return Err(anyhow::anyhow!(
            "community {community_id} ('{label}') has no members"
        ));
    }

    // Build parameter placeholders for the IN clause
    let placeholders: Vec<String> = member_ids.iter().map(|_| "?".to_string()).collect();
    let placeholders_str = placeholders.join(",");

    // Query member entity labels
    let mut node_stmt = conn
        .prepare(&format!(
            "SELECT label FROM nodes WHERE id IN ({placeholders_str}) ORDER BY id"
        ))
        .context("failed to prepare node query for community members")?;

    let entity_labels: Vec<String> = {
        let params: Vec<&dyn rusqlite::types::ToSql> = member_ids
            .iter()
            .map(|id| id as &dyn rusqlite::types::ToSql)
            .collect();
        node_stmt
            .query_map(params.as_slice(), |row| row.get::<_, String>(0))
            .context("failed to query member entity labels")?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("failed to collect member entity labels")?
    };

    // Query relationships (co_occurs_with edges between members)
    let mut edge_stmt = conn
        .prepare(&format!(
            "SELECT n1.label, n2.label, e.weight \
             FROM edges e \
             JOIN nodes n1 ON e.source_id = n1.id \
             JOIN nodes n2 ON e.target_id = n2.id \
             WHERE e.type = 'co_occurs_with' \
             AND e.source_id IN ({placeholders_str}) \
             AND e.target_id IN ({placeholders_str})"
        ))
        .context("failed to prepare edge query for community relationships")?;

    let relationships: Vec<(String, String, f64)> = {
        // We need two copies of the member_ids for the two IN clauses
        let mut params: Vec<&dyn rusqlite::types::ToSql> = Vec::new();
        for id in member_ids {
            params.push(id as &dyn rusqlite::types::ToSql);
        }
        for id in member_ids {
            params.push(id as &dyn rusqlite::types::ToSql);
        }
        edge_stmt
            .query_map(params.as_slice(), |row| {
                let a: String = row.get(0)?;
                let b: String = row.get(1)?;
                let weight: f64 = row.get(2)?;
                Ok((a, b, weight))
            })
            .context("failed to query member relationships")?
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("failed to collect member relationships")?
    };

    // Build the prompt
    let prompt = build_community_prompt(label, &entity_labels, &relationships);

    // Approximate input token count (4 chars per token)
    let input_tokens = (prompt.len() / 4) as i64;

    // Call Ollama with JSON mode
    let response_text = client.generate(&prompt, summary_model, Some("json"))?;

    // Parse the JSON response — retry once on failure
    let parsed = parse_summary_json(&response_text);
    let (summary, keywords, topics) = match parsed {
        Ok(result) => result,
        Err(e) => {
            warn!(
                "community {community_id}: failed to parse LLM response (attempt 1): {e}. Retrying..."
            );
            // Retry with an error message asking for valid JSON
            let retry_prompt = format!(
                "{prompt}\n\nYour previous response was not valid JSON. \
                 Please respond with valid JSON only: {{ \"summary\": ..., \"keywords\": [...], \"topics\": [...] }}"
            );
            let response_text2 = client.generate(&retry_prompt, summary_model, Some("json"))?;
            parse_summary_json(&response_text2).with_context(|| {
                format!(
                    "community {community_id}: failed to parse LLM response after retry. \
                     Raw response: {response_text2}"
                )
            })?
        }
    };

    // Approximate output token count
    let output_tokens = (response_text.len() / 4) as i64;
    let total_tokens = input_tokens + output_tokens;

    Ok(SummaryResult {
        community_id,
        summary,
        keywords,
        topics,
        tokens: total_tokens,
    })
}

/// Summarize ALL communities in the database that don't have a summary yet.
///
/// Opens a connection to `db_path`, iterates over communities with
/// `summary IS NULL`, calls [`summarize_community`] for each, embeds
/// the summary, and persists to the database.
///
/// Returns `(total_summaries, total_tokens)`.
#[allow(dead_code)]
pub fn summarize_all_communities(
    db_path: &str,
    ollama_url: &str,
    summary_model: &str,
    embed_model: &str,
) -> Result<(usize, i64)> {
    let conn = Connection::open(db_path).context("failed to open database")?;

    // Get all communities that need summarization
    let all_communities =
        communities::get_all_communities(&conn).context("failed to get all communities")?;

    let to_summarize: Vec<_> = all_communities
        .iter()
        .filter(|c| c.summary.is_none())
        .collect();

    if to_summarize.is_empty() {
        info!("All communities already summarized");
        return Ok((0, 0));
    }

    let client = OllamaClient::new(ollama_url, embed_model);
    let total = to_summarize.len();
    let mut total_tokens: i64 = 0;

    let pb = ProgressBar::new(total as u64);
    pb.set_style(
        indicatif::ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} communities")
            .context("failed to create progress bar style")
            .unwrap_or_else(|_| indicatif::ProgressStyle::default_bar()),
    );

    for community in to_summarize {
        let community_id = community.id;
        let label = &community.label;

        match summarize_community(&client, &conn, community_id, summary_model) {
            Ok(result) => {
                // Embed the summary text
                let embedding = match client.embed(&result.summary) {
                    Ok(emb) => emb,
                    Err(e) => {
                        warn!(
                            "community {community_id} ('{label}'): failed to embed summary: {e}. Skipping."
                        );
                        pb.inc(1);
                        continue;
                    }
                };
                let embedding_blob = vector_to_blob(&embedding);

                // Persist to DB
                communities::update_community_summary(
                    &conn,
                    community_id,
                    &result.summary,
                    &embedding_blob,
                    summary_model,
                    embed_model,
                    result.tokens,
                )
                .context("failed to update community summary")?;

                total_tokens += result.tokens;
                info!(
                    "community {community_id} ('{label}'): summarized ({} tokens)",
                    result.tokens
                );
            }
            Err(e) => {
                warn!("community {community_id} ('{label}'): summarization failed: {e}. Skipping.");
            }
        }

        pb.inc(1);
    }

    pb.finish_with_message(format!(
        "{total}/{total} communities summarized ({total_tokens} tokens total)"
    ));

    Ok((total, total_tokens))
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Parse the LLM's JSON response into (summary, keywords, topics).
///
/// Attempts to parse the response as a JSON object with fields:
/// - `summary`: string
/// - `keywords`: array of strings
/// - `topics`: array of strings
#[allow(dead_code)]
fn parse_summary_json(response: &str) -> Result<(String, Vec<String>, Vec<String>)> {
    // Try to find a JSON object in the response (the model may add extra text)
    let json_str = extract_json_object(response)
        .ok_or_else(|| anyhow::anyhow!("no JSON object found in response"))?;

    let value: serde_json::Value =
        serde_json::from_str(json_str).context("failed to parse JSON from LLM response")?;

    let summary = value
        .get("summary")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("missing 'summary' field in LLM response"))?;

    let keywords = value
        .get("keywords")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let topics = value
        .get("topics")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    Ok((summary, keywords, topics))
}

/// Extract a JSON object `{...}` from a string, handling potential
/// surrounding text (markdown fences, explanatory text, etc.).
#[allow(dead_code)]
fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end <= start {
        return None;
    }
    Some(&text[start..=end])
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::communities::Community;
    use crate::db::schema::init_db;

    /// Open an in-memory database and initialise the schema.
    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn
    }

    /// Insert an entity node.
    fn insert_node(conn: &Connection, id: i64, label: &str, type_: &str) {
        conn.execute(
            "INSERT INTO nodes (id, label, type, metadata) VALUES (?1, ?2, ?3, '{}')",
            rusqlite::params![id, label, type_],
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

    /// Insert a community row.
    fn insert_community(conn: &Connection, label: &str, level: i32, member_ids: Vec<i64>) -> i64 {
        let member_ids_json = serde_json::to_string(&member_ids).unwrap();
        conn.execute(
            "INSERT INTO communities \
             (label, level, parent_id, summary, summary_embedding, \
              member_ids, member_count, algorithm, quality_fn, resolution, \
              summary_model, embed_model, summary_tokens) \
             VALUES (?1, ?2, NULL, NULL, NULL, ?3, ?4, 'test', 'test', 1.0, NULL, NULL, NULL)",
            rusqlite::params![label, level, member_ids_json, member_ids.len() as i32],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    // ------------------------------------------------------------------
    // test_build_community_prompt_format
    // ------------------------------------------------------------------

    #[test]
    fn test_build_community_prompt_format() {
        let entities = vec![
            "Python".to_string(),
            "FastAPI".to_string(),
            "SQLite".to_string(),
        ];
        let relationships = vec![
            ("Python".to_string(), "FastAPI".to_string(), 0.9),
            ("FastAPI".to_string(), "SQLite".to_string(), 0.7),
        ];

        let prompt = build_community_prompt("web-dev", &entities, &relationships);

        // Verify the prompt contains entity names
        assert!(prompt.contains("Python"), "prompt should contain Python");
        assert!(prompt.contains("FastAPI"), "prompt should contain FastAPI");
        assert!(prompt.contains("SQLite"), "prompt should contain SQLite");

        // Verify relationships are included with weights
        assert!(prompt.contains("0.90"), "prompt should contain weight 0.90");
        assert!(prompt.contains("0.70"), "prompt should contain weight 0.70");

        // Verify JSON instructions are present
        assert!(
            prompt.contains("summary"),
            "prompt should contain summary field"
        );
        assert!(
            prompt.contains("keywords"),
            "prompt should contain keywords field"
        );
        assert!(
            prompt.contains("topics"),
            "prompt should contain topics field"
        );

        // Verify community label is included
        assert!(
            prompt.contains("web-dev"),
            "prompt should contain community label"
        );
    }

    // ------------------------------------------------------------------
    // test_parse_summary_json
    // ------------------------------------------------------------------

    #[test]
    fn test_parse_summary_json_valid() {
        let response = r#"{
            "summary": "A test community",
            "keywords": ["test", "rust"],
            "topics": ["programming", "testing"]
        }"#;

        let (summary, keywords, topics) =
            parse_summary_json(response).expect("should parse valid JSON");

        assert_eq!(summary, "A test community");
        assert_eq!(keywords, vec!["test", "rust"]);
        assert_eq!(topics, vec!["programming", "testing"]);
    }

    // ------------------------------------------------------------------
    // test_parse_summary_json_with_extra_text
    // ------------------------------------------------------------------

    #[test]
    fn test_parse_summary_json_with_extra_text() {
        let response = "Here is the summary:\n```json\n{\"summary\": \"Python ecosystem\", \"keywords\": [\"python\"], \"topics\": [\"web\"]}\n```";

        let (summary, keywords, topics) =
            parse_summary_json(response).expect("should parse JSON even with extra text");

        assert_eq!(summary, "Python ecosystem");
        assert_eq!(keywords, vec!["python"]);
        assert_eq!(topics, vec!["web"]);
    }

    // ------------------------------------------------------------------
    // test_parse_summary_json_missing_fields
    // ------------------------------------------------------------------

    #[test]
    fn test_parse_summary_json_missing_fields() {
        let response = r#"{"summary": "Only summary"}"#;

        let (summary, keywords, topics) =
            parse_summary_json(response).expect("should parse with missing fields");
        assert_eq!(summary, "Only summary");
        // Missing fields should default to empty
        assert!(keywords.is_empty(), "keywords should default to empty");
        assert!(topics.is_empty(), "topics should default to empty");
    }

    // ------------------------------------------------------------------
    // test_extract_json_object
    // ------------------------------------------------------------------

    #[test]
    fn test_extract_json_object_plain() {
        let text = r#"{"summary": "test"}"#;
        let extracted = extract_json_object(text).expect("should extract JSON");
        assert_eq!(extracted, r#"{"summary": "test"}"#);
    }

    #[test]
    fn test_extract_json_object_with_surrounding_text() {
        let text = "Some text before\n{\"key\": \"value\"}\nSome text after";
        let extracted = extract_json_object(text).expect("should extract JSON");
        assert_eq!(extracted, r#"{"key": "value"}"#);
    }

    #[test]
    fn test_extract_json_object_with_markdown_fence() {
        let text = "```json\n{\"a\": 1}\n```";
        let extracted = extract_json_object(text).expect("should extract JSON from fence");
        assert_eq!(extracted, r#"{"a": 1}"#);
    }

    #[test]
    fn test_extract_json_object_none() {
        assert!(extract_json_object("no JSON here").is_none());
        assert!(extract_json_object("").is_none());
    }

    // ------------------------------------------------------------------
    // test_summarize_community
    // ------------------------------------------------------------------

    #[test]
    #[ignore = "needs Ollama"]
    fn test_summarize_community() {
        let conn = setup_db();

        // Insert entity nodes
        insert_node(&conn, 1, "Python", "language");
        insert_node(&conn, 2, "FastAPI", "tool");
        insert_node(&conn, 3, "SQLite", "database");

        // Insert edges
        insert_edge(&conn, 1, 2, 0.9);
        insert_edge(&conn, 2, 3, 0.7);

        // Insert a community with these members
        let community_id = insert_community(&conn, "test-community", 1, vec![1, 2, 3]);

        let client = OllamaClient::new("http://localhost:11434", "nomic-embed-text");
        let result = summarize_community(&client, &conn, community_id, "llama3.2:3b")
            .expect("summarize should succeed with Ollama");

        assert!(!result.summary.is_empty(), "summary should not be empty");
        assert!(!result.keywords.is_empty(), "keywords should not be empty");
        assert!(!result.topics.is_empty(), "topics should not be empty");
        assert!(result.tokens > 0, "tokens should be positive");
        assert_eq!(
            result.community_id, community_id,
            "community ID should match"
        );
    }

    // ------------------------------------------------------------------
    // test_summarize_all_communities_empty
    // ------------------------------------------------------------------

    #[test]
    fn test_summarize_all_communities_empty() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let db_path = tmp.path().to_str().unwrap().to_owned();

        // Initialize an empty DB (no communities)
        {
            let conn = Connection::open(&db_path).unwrap();
            init_db(&conn).unwrap();
            // No communities inserted — DB has the communities table but no rows
        }

        let (count, tokens) = summarize_all_communities(
            &db_path,
            "http://localhost:11434",
            "llama3.2:3b",
            "nomic-embed-text",
        )
        .expect("summarize with no communities should succeed");
        assert_eq!(count, 0, "should return 0 summaries");
        assert_eq!(tokens, 0, "should return 0 tokens");
    }

    // ------------------------------------------------------------------
    // test_summarize_all_communities_already_done
    // ------------------------------------------------------------------

    #[test]
    fn test_summarize_all_communities_already_done() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let db_path = tmp.path().to_str().unwrap().to_owned();

        {
            let conn = Connection::open(&db_path).unwrap();
            init_db(&conn).unwrap();

            // Insert a community that already has a summary
            let emb = vector_to_blob(&[0.1_f32, 0.2_f32, 0.3_f32]);
            let community = Community {
                id: 0,
                label: "already-done".into(),
                level: 1,
                parent_id: None,
                summary: Some("Already summarized".into()),
                summary_embedding: Some(emb),
                member_ids: vec![1, 2],
                member_count: 2,
                algorithm: "leiden".into(),
                quality_fn: "cpm".into(),
                resolution: 1.0,
                summary_model: Some("model-x".into()),
                embed_model: Some("embed-y".into()),
                summary_tokens: Some(42),
            };
            communities::insert_community(&conn, &community).unwrap();
        }

        let (count, tokens) = summarize_all_communities(
            &db_path,
            "http://localhost:11434",
            "llama3.2:3b",
            "nomic-embed-text",
        )
        .expect("summarize with all already done should succeed");
        assert_eq!(count, 0, "should return 0 summaries");
        assert_eq!(tokens, 0, "should return 0 tokens");
    }
}
