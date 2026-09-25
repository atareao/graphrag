//! Community-aware answer mode for GraphRAG.
//!
//! Given a user query and hybrid search results, this module retrieves
//! relevant community summaries and uses them as additional RAG context
//! to produce a more comprehensive answer via Ollama.
//!
//! # Pipeline
//!
//! 1. Compute a query embedding via `OllamaClient::embed()`
//! 2. Find top-3 communities by cosine similarity against community embeddings
//! 3. Format community summaries as context
//! 4. Take top-5 hybrid search results as chunk evidence
//! 5. Build a prompt combining community context + chunk evidence + query
//! 6. Call `OllamaClient::generate()` with no format restriction
//! 7. Return the generated answer text

use anyhow::Result;

use crate::embed::ollama::OllamaClient;
use crate::search::hybrid::SearchResult;
use crate::vector;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

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
///
/// # Returns
///
/// The generated answer text, or `"No relevant information found."` if there
/// are no communities or no results to use as context.
pub fn answer_query(
    ollama: &OllamaClient,
    query: &str,
    results: &[SearchResult],
    community_embeddings: &[(i64, Vec<f32>, String, String)],
    summary_model: &str,
) -> Result<String> {
    // Bail early if there is nothing to work with.
    if results.is_empty() {
        return Ok("No relevant information found.".into());
    }

    // ---- Step 1: Compute query embedding ----
    let query_emb = ollama.embed(query)?;

    // ---- Step 2: Find top-3 communities by cosine similarity ----
    let top_k = 3;
    let mut scored: Vec<(f32, &str, &str)> = community_embeddings
        .iter()
        .map(|(_id, emb, label, summary)| {
            let sim = vector::cosine_similarity_raw(&query_emb, emb);
            (sim, label.as_str(), summary.as_str())
        })
        .collect();

    // Sort descending by similarity and take top-k.
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let top_communities: Vec<(&str, &str)> = scored
        .into_iter()
        .take(top_k)
        .filter(|(sim, _, _)| *sim > 0.0)
        .map(|(_, label, summary)| (label, summary))
        .collect();

    // ---- Step 3: Format community context ----
    let community_context = format_community_context(&top_communities);

    // ---- Step 4: Find top-5 results by score ----
    let mut sorted_results: Vec<&SearchResult> = results.iter().collect();
    sorted_results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let top_results: Vec<&SearchResult> = sorted_results.into_iter().take(5).collect();

    // ---- Step 5: Format chunk evidence ----
    let chunk_evidence = format_chunk_evidence(&top_results);

    // ---- Step 6: Build prompt ----
    let prompt = build_answer_prompt(&community_context, &chunk_evidence, query);

    // ---- Step 7: Call Ollama generate (no JSON mode) ----
    let answer = ollama.generate(&prompt, summary_model, None)?;

    Ok(answer)
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
fn format_chunk_evidence(results: &[&SearchResult]) -> String {
    let max_chars: usize = 300;

    let entries: Vec<String> = results
        .iter()
        .filter(|r| !r.chunk_text.is_empty())
        .map(|r| {
            let preview = if r.chunk_text.len() <= max_chars {
                r.chunk_text.clone()
            } else {
                format!("{}...", &r.chunk_text[..max_chars])
            };
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
        let result = answer_query(&ollama, "test", &[], &[], "llama3.2:3b");
        assert_eq!(
            result.unwrap(),
            "No relevant information found.",
            "empty results should produce the 'no info' message"
        );
    }
}
