# Spec Delta

## MODIFIED Requirements

### Requirement: All synthetic embedding code paths removed

All code paths that generate or use synthetic (hash-based) embeddings SHALL be removed. The `src/vector/synthetic.rs` file SHALL be deleted. The `inject_embedding` method and its `#[cfg(test)]` impl block in `src/search/hybrid.rs` SHALL be removed. All comments referencing "synthetic" in `src/search/hybrid.rs` SHALL be updated to remove the term. Tests that depend on `inject_embedding` SHALL be rewritten to use Ollama or skipped.

#### Scenario: No synthetic.rs file exists
- **WHEN** the project is checked after the change
- **THEN** `src/vector/synthetic.rs` SHALL NOT exist

#### Scenario: No synthetic references in hybrid.rs
- **WHEN** the project compiles after the change
- **THEN** `src/search/hybrid.rs` SHALL contain no occurrences of the word "synthetic"
- **AND** the `inject_embedding` method SHALL NOT exist

#### Scenario: Test with inject_embedding replaced
- **WHEN** `test_search_empty_chunks` is executed
- **THEN** it SHALL run without calling `inject_embedding`
- **AND** SHALL either use an Ollama mock or be marked `#[ignore = "needs Ollama"]`