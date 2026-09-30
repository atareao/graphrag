//! Community-aware answer mode for GraphRAG.
//!
//! Given a user query and hybrid search results, this module retrieves
//! relevant community summaries and uses them as additional RAG context
//! to produce a more comprehensive answer via Ollama.
//!
//! # Pipeline
//!
//! 1. Compute a query embedding via `OllamaClient::embed()`
//! 2. Find the top-N communities by cosine similarity against community
//!    embeddings (N configurable via [`AnswerOptions::communities`], default 3)
//! 3. Format community summaries as context
//! 4. Take the top-M hybrid search results as chunk evidence
//!    (M configurable via [`AnswerOptions::evidence`], default 5)
//! 5. Build a prompt combining community context + chunk evidence + query
//! 6. Call `OllamaClient::generate()` with no format restriction
//! 7. Return an [`AnswerResult`] with the answer text and its deduplicated
//!    [`Source`] list

use anyhow::Result;
use serde::Serialize;

use crate::embed::ollama::OllamaClient;
use crate::search::hybrid::SearchResult;
use crate::vector;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Controls how much context is packed into the RAG prompt.
///
/// All counts are upper bounds; the generator never emits more than this.
#[derive(Debug, Clone)]
pub struct AnswerOptions {
    /// Maximum number of community summaries to include.
    pub communities: usize,
    /// Maximum number of chunk evidences to include.
    pub evidence: usize,
    /// Maximum characters per community summary.
    pub summary_chars: usize,
    /// Maximum characters per chunk evidence.
    pub chunk_chars: usize,
}

impl Default for AnswerOptions {
    fn default() -> Self {
        Self {
            communities: 3,
            evidence: 5,
            summary_chars: 500,
            chunk_chars: 300,
        }
    }
}

/// A single source note backing a generated answer.
#[derive(Debug, Clone, Serialize)]
pub struct Source {
    /// Display label of the source note.
    pub label: String,
    /// Filesystem path of the source note, when known.
    pub path: Option<String>,
}

/// Structured result of an answer generation.
#[derive(Debug, Clone)]
pub struct AnswerResult {
    /// The generated narrative answer.
    pub answer: String,
    /// Deduplicated list of source notes, in score order.
    pub sources: Vec<Source>,
}

impl AnswerResult {
    /// Derive the deduplicated source list from hybrid search results.
    ///
    /// Deduplication key is the note `path` when present, otherwise the
    /// `label`. Input order (score order) is preserved.
    pub fn sources_from_results(results: &[SearchResult]) -> Vec<Source> {
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut sources: Vec<Source> = Vec::new();

        for r in results {
            let key = if r.file.is_empty() {
                r.label.clone()
            } else {
                r.file.clone()
            };
            if seen.insert(key) {
                sources.push(Source {
                    label: r.label.clone(),
                    path: if r.file.is_empty() {
                        None
                    } else {
                        Some(r.file.clone())
                    },
                });
            }
        }

        sources
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Build the full RAG prompt for a query from community summaries and chunk
/// evidence, honouring the provided [`AnswerOptions`].
///
/// This is a pure function: it performs no I/O and can be tested without
/// Ollama. When `communities` is empty the community section is omitted.
pub fn build_rag_prompt(
    query: &str,
    communities: &[(String, String)],
    results: &[SearchResult],
    opts: &AnswerOptions,
) -> String {
    // ---- Communities: top-N, summaries truncated to `summary_chars` ----
    let truncated: Vec<(String, String)> = communities
        .iter()
        .take(opts.communities)
        .map(|(label, summary)| (label.clone(), truncate_chars(summary, opts.summary_chars)))
        .collect();
    let community_refs: Vec<(&str, &str)> = truncated
        .iter()
        .map(|(label, summary)| (label.as_str(), summary.as_str()))
        .collect();
    let community_context = format_community_context(&community_refs);

    // ---- Chunk evidence: top-M by score, truncated to `chunk_chars` ----
    let mut sorted_results: Vec<&SearchResult> = results.iter().collect();
    sorted_results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let top_results: Vec<&SearchResult> = sorted_results.into_iter().take(opts.evidence).collect();
    let chunk_evidence = format_chunk_evidence_limited(&top_results, opts.chunk_chars);

    build_answer_prompt(&community_context, &chunk_evidence, query)
}

/// Generate an answer using community summaries + chunk evidence as RAG context.
///
/// # Arguments
///
/// * `ollama`         — Initialised Ollama client (used for both embedding and generation).
/// * `query`          — The user's original search query.
/// * `results`        — Top hybrid-search results (already scored and ranked).
/// * `community_embeddings` — `(id, embedding, label, summary)` for every community
///   that has a non-`NULL` `summary_embedding` (see
///   [`crate::db::communities::load_all_community_embeddings`]).
/// * `summary_model`  — The Ollama model to use for answer generation
///   (e.g. `"llama3.2:3b"`).
/// * `opts`           — Generation options (counts and truncation limits).
///
/// # Returns
///
/// An [`AnswerResult`] with the generated answer and the deduplicated source
/// list. With empty `results` the answer is `"No relevant information found."`.
pub fn answer_query(
    ollama: &OllamaClient,
    query: &str,
    results: &[SearchResult],
    community_embeddings: &[(i64, Vec<f32>, String, String)],
    summary_model: &str,
    opts: &AnswerOptions,
) -> Result<AnswerResult> {
    // Bail early if there is nothing to work with.
    if results.is_empty() {
        return Ok(AnswerResult {
            answer: "No relevant information found.".into(),
            sources: Vec::new(),
        });
    }

    let sources = AnswerResult::sources_from_results(results);

    // ---- Step 1: Compute query embedding ----
    let query_emb = ollama.embed(query)?;

    // ---- Step 2: Find top-N communities by cosine similarity ----
    let mut scored: Vec<(f32, &str, &str)> = community_embeddings
        .iter()
        .map(|(_id, emb, label, summary)| {
            let sim = vector::cosine_similarity_raw(&query_emb, emb);
            (sim, label.as_str(), summary.as_str())
        })
        .collect();

    // Sort descending by similarity and take top-N.
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let top_communities: Vec<(String, String)> = scored
        .into_iter()
        .take(opts.communities)
        .filter(|(sim, _, _)| *sim > 0.0)
        .map(|(_, label, summary)| (label.to_string(), summary.to_string()))
        .collect();

    // ---- Step 3: Build the prompt (communities + evidence + query) ----
    let prompt = build_rag_prompt(query, &top_communities, results, opts);

    // ---- Step 4: Call Ollama generate (no JSON mode) ----
    let answer = ollama.generate(&prompt, summary_model, None)?;

    Ok(AnswerResult { answer, sources })
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Format community summaries as a Markdown section for the prompt.
///
/// ```markdown
/// ## Related Knowledge Communities
///
/// Community: {label}
/// {summary}
///
/// Community: {label}
/// {summary}
/// ```
fn format_community_context(communities: &[(&str, &str)]) -> String {
    if communities.is_empty() {
        return String::new();
    }

    let mut out = String::from("## Related Knowledge Communities\n");
    for (label, summary) in communities {
        out.push_str("\n\nCommunity: ");
        out.push_str(label);
        out.push('\n');
        out.push_str(summary);
    }
    out
}

/// Format chunk evidence as a Markdown section for the prompt.
///
/// Each chunk text is truncated to 300 characters maximum.  Only results
/// that have a non-empty `chunk_text` are included.
///
/// ```markdown
/// ## Relevant Notes
///
/// Note: {label} > {chunk_header}
/// {chunk_text_preview}
///
/// Note: {label} > {chunk_header}
/// {chunk_text_preview}
/// ```
#[allow(dead_code)] // exercised by unit tests; the generator uses the configurable variant
fn format_chunk_evidence(results: &[&SearchResult]) -> String {
    format_chunk_evidence_limited(results, 300)
}

/// Like [`format_chunk_evidence`] but with a configurable per-chunk character
/// limit. Shared by the default path and [`build_rag_prompt`].
fn format_chunk_evidence_limited(results: &[&SearchResult], max_chars: usize) -> String {
    let entries: Vec<String> = results
        .iter()
        .filter(|r| !r.chunk_text.is_empty())
        .map(|r| {
            let preview = truncate_chars(&r.chunk_text, max_chars);
            format!("Note: {} > {}\n{}", r.label, r.chunk_header, preview)
        })
        .collect();

    if entries.is_empty() {
        return String::new();
    }

    let mut out = String::from("## Relevant Notes\n\n");
    out.push_str(&entries.join("\n\n"));
    out
}

/// Truncate a string to at most `max_chars` Unicode scalar values, appending
/// `"..."` when truncation occurred. Char-safe (never splits a UTF-8 boundary).
fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{}...", truncated)
    }
}

/// Build the full answer prompt by combining community context, chunk
/// evidence, and the user's question.
fn build_answer_prompt(community_context: &str, chunk_evidence: &str, query: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    parts.push(
        "You are a knowledgeable assistant that answers questions based on provided context.",
    );

    if !community_context.is_empty() {
        parts.push(community_context);
    }
    if !chunk_evidence.is_empty() {
        parts.push(chunk_evidence);
    }

    // Question section
    let question = format!("\nQuestion: {query}");
    parts.push(&question);

    // Final instruction
    parts.push(
        "\nAnswer the question based on the context above. If the context doesn't contain enough \
         information, say so. Be concise but thorough.",
    );

    parts.join("\n\n")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::expand::Neighbor;

    /// Build a minimal SearchResult for testing.
    fn make_result(label: &str, score: f64, chunk_header: &str, chunk_text: &str) -> SearchResult {
        SearchResult {
            id: 0,
            label: label.to_string(),
            r#type: "note".into(),
            score,
            content: String::new(),
            file: String::new(),
            neighbors: Vec::<Neighbor>::new(),
            chunk_header: chunk_header.to_string(),
            chunk_text: chunk_text.to_string(),
        }
    }

    // ------------------------------------------------------------------
    // test_format_community_context_empty
    // ------------------------------------------------------------------

    #[test]
    fn test_format_community_context_empty() {
        let result = format_community_context(&[]);
        assert_eq!(result, "", "empty slice should produce empty string");
    }

    // ------------------------------------------------------------------
    // test_format_community_context_single
    // ------------------------------------------------------------------

    #[test]
    fn test_format_community_context_single() {
        let communities = &[("web-dev", "A community about web development tools.")];
        let result = format_community_context(communities);

        assert!(result.contains("## Related Knowledge Communities"));
        assert!(result.contains("Community: web-dev"));
        assert!(result.contains("A community about web development tools."));
    }

    // ------------------------------------------------------------------
    // test_format_community_context_multiple
    // ------------------------------------------------------------------

    #[test]
    fn test_format_community_context_multiple() {
        let communities = &[
            ("ai-ml", "Artificial intelligence and machine learning."),
            ("web-dev", "Web development frameworks."),
            ("databases", "Database technologies."),
        ];
        let result = format_community_context(communities);

        assert!(result.contains("Community: ai-ml"));
        assert!(result.contains("Community: web-dev"));
        assert!(result.contains("Community: databases"));
        assert!(result.contains("Artificial intelligence and machine learning."));
        assert!(result.contains("Web development frameworks."));
        assert!(result.contains("Database technologies."));
    }

    // ------------------------------------------------------------------
    // test_format_chunk_evidence_empty
    // ------------------------------------------------------------------

    #[test]
    fn test_format_chunk_evidence_empty() {
        let result = format_chunk_evidence(&[]);
        assert_eq!(result, "", "empty results should produce empty string");
    }

    // ------------------------------------------------------------------
    // test_format_chunk_evidence_skips_empty_chunk_text
    // ------------------------------------------------------------------

    #[test]
    fn test_format_chunk_evidence_skips_empty_chunk_text() {
        let r = make_result("Python", 0.9, "Introduction", "");
        let results = &[&r]; // empty chunk_text → should be skipped
        let result = format_chunk_evidence(results);
        assert_eq!(
            result, "",
            "results with empty chunk_text should be skipped"
        );
    }

    // ------------------------------------------------------------------
    // test_format_chunk_evidence_single
    // ------------------------------------------------------------------

    #[test]
    fn test_format_chunk_evidence_single() {
        let r = make_result(
            "Python",
            0.9,
            "Introduction",
            "Python is a high-level programming language.",
        );
        let results = &[&r];
        let result = format_chunk_evidence(results);

        assert!(result.contains("## Relevant Notes"));
        assert!(result.contains("Note: Python > Introduction"));
        assert!(result.contains("Python is a high-level programming language."));
    }

    // ------------------------------------------------------------------
    // test_format_chunk_evidence_truncation
    // ------------------------------------------------------------------

    #[test]
    fn test_format_chunk_evidence_truncation() {
        let long_text = "A".repeat(500);
        let r = make_result("Test", 1.0, "Header", &long_text);
        let results = &[&r];
        let result = format_chunk_evidence(results);

        // Should be truncated to 300 chars + "..."
        assert!(result.contains("..."));
        // The preview part should be at most 303 characters (300 + "...")
        let preview_start = result.find("Note: Test > Header").unwrap();
        let after_preview = &result[preview_start..];
        // Find the text after the header line
        let text_start = after_preview.find('\n').unwrap() + 1;
        let preview_text = &after_preview[text_start..];
        assert!(
            preview_text.len() <= 303,
            "preview should be <= 303 chars, got {}",
            preview_text.len()
        );
    }

    // ------------------------------------------------------------------
    // test_format_chunk_evidence_multiple
    // ------------------------------------------------------------------

    #[test]
    fn test_format_chunk_evidence_multiple() {
        let r1 = make_result("Rust", 0.95, "Ownership", "Rust's ownership system.");
        let r2 = make_result("Python", 0.90, "Intro", "Python is dynamic.");
        let results = &[&r1, &r2];
        let result = format_chunk_evidence(results);

        assert!(result.contains("Note: Rust > Ownership"));
        assert!(result.contains("Note: Python > Intro"));
        assert!(result.contains("Rust's ownership system."));
        assert!(result.contains("Python is dynamic."));
    }

    // ------------------------------------------------------------------
    // test_build_answer_prompt_structure
    // ------------------------------------------------------------------

    #[test]
    fn test_build_answer_prompt_structure() {
        let community_context = "## Related Knowledge Communities\n\nCommunity: test\nA test.";
        let chunk_evidence = "## Relevant Notes\n\nNote: Foo > Bar\nContent.";
        let query = "What is this about?";

        let prompt = build_answer_prompt(community_context, chunk_evidence, query);

        // Should contain the system instruction
        assert!(
            prompt.contains("You are a knowledgeable assistant"),
            "should contain system instruction"
        );

        // Should contain both context sections
        assert!(
            prompt.contains("Related Knowledge Communities"),
            "should contain community context"
        );
        assert!(
            prompt.contains("Relevant Notes"),
            "should contain chunk evidence"
        );

        // Should contain the question
        assert!(prompt.contains("Question: What is this about?"));

        // Should contain the final instruction
        assert!(
            prompt.contains("If the context doesn't contain enough information"),
            "should contain fallback instruction"
        );
    }

    // ------------------------------------------------------------------
    // test_build_answer_prompt_no_context
    // ------------------------------------------------------------------

    #[test]
    fn test_build_answer_prompt_no_context() {
        let prompt = build_answer_prompt("", "", "Hello");

        assert!(prompt.contains("You are a knowledgeable assistant"));
        assert!(prompt.contains("Question: Hello"));
        // No context sections
        assert!(!prompt.contains("Related Knowledge Communities"));
        assert!(!prompt.contains("Relevant Notes"));
    }

    // ------------------------------------------------------------------
    // test_answer_query_empty_results
    // ------------------------------------------------------------------

    #[test]
    fn test_answer_query_empty_results() {
        // When results is empty, answer_query should return "No relevant information found."
        let ollama = OllamaClient::new("http://localhost:11434", "nomic-embed-text");
        let result = answer_query(
            &ollama,
            "test",
            &[],
            &[],
            "llama3.2:3b",
            &AnswerOptions::default(),
        );
        assert_eq!(
            result.unwrap().answer,
            "No relevant information found.",
            "empty results should produce the 'no info' message"
        );
    }

    // ==================================================================
    // RED — add-ask-rag-command (community-search delta)
    //
    // These tests target the NEW public API of this module and therefore
    // do not compile until the GREEN phase lands:
    //   * `AnswerOptions`    — generation options (counts + truncation)
    //   * `AnswerResult`     — { answer, sources }
    //   * `Source`           — { label, path }
    //   * `AnswerResult::sources_from_results(&[SearchResult]) -> Vec<Source>`
    //   * `build_rag_prompt(query, communities, results, opts) -> String`
    // ==================================================================

    /// Build a note `SearchResult` that carries a file path (unlike
    /// [`make_result`], which leaves `file` empty).
    fn make_note_result(
        label: &str,
        path: &str,
        chunk_header: &str,
        chunk_text: &str,
    ) -> SearchResult {
        let mut r = make_result(label, 0.8, chunk_header, chunk_text);
        r.file = path.to_string();
        r
    }

    // ------------------------------------------------------------------
    // RED-1 — AnswerResult.sources is deduplicated per note
    // ------------------------------------------------------------------

    #[test]
    fn test_answer_result_sources_deduplicated() {
        // 3 distinct notes; the first note contributes 2 chunks that must
        // collapse into a single source entry.
        let results = vec![
            make_note_result("nginx", "/notes/nginx.md", "Setup", "server { listen 80; }"),
            make_note_result(
                "nginx",
                "/notes/nginx.md",
                "TLS",
                "ssl_certificate /etc/ssl/cert.pem;",
            ),
            make_note_result(
                "docker",
                "/notes/docker.md",
                "Intro",
                "Docker runs containers.",
            ),
            make_note_result("rust", "/notes/rust.md", "Ownership", "Rust owns memory."),
        ];

        let sources = AnswerResult::sources_from_results(&results);

        assert_eq!(
            sources.len(),
            3,
            "one source per distinct note, no duplicates even with 2 chunks of 'nginx'"
        );

        let unique: std::collections::HashSet<&str> =
            sources.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(unique.len(), 3, "all source labels must be unique");

        // label + path must both be populated for the duplicate note.
        let nginx = sources
            .iter()
            .find(|s| s.label == "nginx")
            .expect("nginx must appear once");
        assert_eq!(
            nginx.path.as_deref(),
            Some("/notes/nginx.md"),
            "source path must point at the underlying note"
        );
    }

    // ------------------------------------------------------------------
    // RED-2 — AnswerOptions are configurable (pure prompt builder)
    // ------------------------------------------------------------------

    #[test]
    fn test_rag_prompt_respects_options() {
        let opts = AnswerOptions {
            communities: 1,
            evidence: 3,
            ..AnswerOptions::default()
        };

        let communities = vec![
            (
                "ai-ml".to_string(),
                "Artificial intelligence and machine learning.".to_string(),
            ),
            (
                "web-dev".to_string(),
                "Web development frameworks.".to_string(),
            ),
            (
                "databases".to_string(),
                "Database technologies.".to_string(),
            ),
        ];

        let results = vec![
            make_note_result("nginx", "/notes/nginx.md", "Setup", "server { listen 80; }"),
            make_note_result(
                "docker",
                "/notes/docker.md",
                "Intro",
                "Docker runs containers.",
            ),
            make_note_result("rust", "/notes/rust.md", "Ownership", "Rust owns memory."),
            make_note_result("sqlite", "/notes/sqlite.md", "FTS", "SQLite FTS5 search."),
            make_note_result("linux", "/notes/linux.md", "Systemd", "systemd unit files."),
        ];

        let prompt = build_rag_prompt("nginx", &communities, &results, &opts);

        assert_eq!(
            prompt.matches("Community: ").count(),
            1,
            "opts.communities = 1 must include exactly one community summary"
        );
        assert_eq!(
            prompt.matches("Note: ").count(),
            3,
            "opts.evidence = 3 must include exactly three chunk evidences"
        );
    }

    // ------------------------------------------------------------------
    // RED-3 — graceful degradation when there are no communities
    // ------------------------------------------------------------------

    #[test]
    fn test_rag_prompt_without_communities() {
        let results = vec![
            make_note_result("nginx", "/notes/nginx.md", "Setup", "server { listen 80; }"),
            make_note_result(
                "docker",
                "/notes/docker.md",
                "Intro",
                "Docker runs containers.",
            ),
        ];

        let prompt = build_rag_prompt("nginx", &[], &results, &AnswerOptions::default());

        assert!(
            !prompt.contains("Related Knowledge Communities"),
            "prompt must NOT contain a community section when there are no communities"
        );
        assert!(
            prompt.contains("Relevant Notes"),
            "prompt must still build chunk evidence"
        );
        assert_eq!(prompt.matches("Note: ").count(), 2);
        assert!(prompt.contains("Question: nginx"));
    }

    // ------------------------------------------------------------------
    // RED-6 — end-to-end (requires Ollama; run with `cargo test -- --ignored`)
    // ------------------------------------------------------------------

    #[test]
    #[ignore = "requires Ollama with nomic-embed-text and llama3.2:3b"]
    #[allow(clippy::len_zero)]
    fn test_ask_end_to_end() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("demo.db");
        let db = db_path.to_str().expect("utf-8 path");

        crate::seed::demo_data::create_demo_db(db, "http://localhost:11434", "nomic-embed-text")
            .expect("seed demo db");

        let ollama = OllamaClient::new("http://localhost:11434", "nomic-embed-text");
        let mut hs = crate::search::HybridSearch::new(db, ollama.clone()).expect("hybrid search");
        let results = hs
            .hybrid_search("nginx", 5, 2, 0.7, None, false, &[])
            .expect("hybrid_search");
        assert!(
            !results.is_empty(),
            "demo db should yield results for 'nginx'"
        );

        let conn = rusqlite::Connection::open(db).expect("open db");
        let communities =
            crate::db::communities::load_all_community_embeddings(&conn).unwrap_or_default();

        let answer = answer_query(
            &ollama,
            "nginx",
            &results,
            &communities,
            "llama3.2:3b",
            &AnswerOptions::default(),
        )
        .expect("answer generation");

        assert!(!answer.answer.trim().is_empty(), "answer must be non-empty");
        assert!(
            answer.sources.len() >= 1,
            "answer must report at least one source"
        );
    }
}
