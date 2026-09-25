# chunk-embedding Specification

## Purpose
Computes and stores real semantic embeddings from Ollama for each document chunk during build, and enables vector search against chunk content rather than only note titles.

## Requirements

### Requirement: Build embeds each chunk via Ollama batch API
During `build`, every chunk (text section between headers) SHALL be embedded using Ollama's `/api/embed` endpoint. The embedding SHALL be computed from the concatenation of `chunk_header + "\n" + chunk_text` (the section header followed by the section body). Chunks shorter than 50 characters SHALL be discarded before embedding.

#### Scenario: Chunk embedding during build
- **WHEN** `build` processes a `.md` file with frontmatter `title: Mi Nota` and a chunk with header `## Instalación` and text `Para instalar VSCode en Ubuntu...`
- **THEN** the chunk SHALL be embedded as text `"## Instalación\nPara instalar VSCode en Ubuntu..."` via Ollama `/api/embed`
- **AND** the resulting embedding vector SHALL be stored in the `chunks` table

#### Scenario: Short chunk discarded
- **WHEN** a chunk has fewer than 50 characters after trimming
- **THEN** the chunk SHALL be discarded and SHALL NOT be embedded or stored

#### Scenario: Ollama unavailable during build
- **WHEN** Ollama is unreachable during `build`
- **THEN** `build` SHALL fail with a clear error message indicating Ollama connection failure
- **AND** no partial data SHALL be persisted for the current file

### Requirement: Chunks table stores embedding and context
The SQLite schema SHALL include a `chunks` table with columns: `id` (INTEGER PRIMARY KEY), `note_id` (INTEGER FK → nodes.id), `header` (TEXT, the section header), `text` (TEXT, the chunk body content), `slug` (TEXT, URL-safe identifier), `embedding` (BLOB, f32 vector), `metadata` (TEXT, JSON). The table SHALL have an index on `note_id`.

#### Scenario: Chunks table created on init
- **WHEN** `init db` runs
- **THEN** the `chunks` table SHALL exist with all required columns and the index on `note_id`

#### Scenario: Chunk insertion and retrieval
- **WHEN** a chunk is embedded and stored during build
- **THEN** the `chunks` row SHALL contain the correct `note_id` referencing the parent note node
- **AND** the `embedding` blob SHALL be a valid f32 vector (1024 dimensions)
- **AND** the `text` SHALL match the original chunk text

### Requirement: Search queries against chunk embeddings
`search` SHALL compute the query embedding via Ollama, then find the top-k most similar chunks by cosine similarity against the `chunks.embedding` column. Results SHALL be grouped by parent `note_id`, and the note title and matched chunk text SHALL be displayed. Graph expansion (neighbors, depth) SHALL operate on the parent note node.

#### Scenario: Search returns chunk-matched results
- **WHEN** user runs `graphrag search "editores de código" graph.db`
- **THEN** the query SHALL be embedded via Ollama
- **AND** results SHALL be determined by cosine similarity against `chunks.embedding`
- **AND** each result SHALL display the note title, the matched chunk text preview, and the cosine similarity score

#### Scenario: Empty database search
- **WHEN** the `chunks` table is empty (no build has been run)
- **THEN** `search` SHALL return an empty result set with a message indicating no data

### Requirement: Graph expansion from chunk-matched notes
After finding top-k chunks, `search` SHALL resolve the parent `note_id` for each chunk and perform graph expansion (CTE recursive) from those note nodes. The `--depth` flag SHALL control expansion depth.

#### Scenario: Graph expansion shows neighbors of matched note
- **WHEN** search finds a chunk from note "VSCode" with depth=1
- **THEN** the neighbors of the "VSCode" note node SHALL be included in the result set with reduced scores

### Requirement: Ollama is a hard dependency
Both `build` and `search` SHALL require a running Ollama instance. The `--ollama` flag SHALL be removed from CLI. If Ollama is unreachable at startup, the command SHALL exit with a non-zero status and a clear error message.

#### Scenario: Search without Ollama
- **WHEN** `search` runs and Ollama is not reachable
- **THEN** the command SHALL exit with an error: "Ollama is required but not reachable at http://localhost:11434"

#### Scenario: Build without Ollama
- **WHEN** `build` runs and Ollama is not reachable
- **THEN** the command SHALL exit with an error before processing any files

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
