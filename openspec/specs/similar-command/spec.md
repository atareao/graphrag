# similar-command Specification

## Purpose

Given a label of an indexed note or an external file, finds semantically similar notes in the graph by comparing their chunk embeddings.

## Requirements

### Requirement: similar --label reuses stored embeddings without Ollama

`graphrag similar --label <label>` SHALL find the note by its label in the `nodes` table, load all chunk embeddings belonging to that note, then for each source chunk compute cosine similarity against all other chunk embeddings in the database. Each target note SHALL be scored by the maximum similarity across all source-chunk-to-target-chunk comparisons. It SHALL NOT call Ollama for this path. It SHALL accept `-k` (number of results), `-d` (graph expansion depth), `--notes-only`, `--min-weight`, and `--filter` flags with the same semantics as `search`.

#### Scenario: Similar by label returns semantically close notes
- **WHEN** user runs `graphrag similar --label "Python frameworks" graph.db -k 3`
- **THEN** the command SHALL load the chunk embeddings of "Python frameworks" from the `chunks` table
- **AND** SHALL compare each of its chunks individually against all stored chunks
- **AND** SHALL score each target note by the maximum cosine similarity across all chunk pairs
- **AND** SHALL return the top-3 notes by that score without calling Ollama

#### Scenario: Similar by label with graph expansion
- **WHEN** user runs `graphrag similar --label "Python frameworks" graph.db -k 5 -d 2`
- **THEN** after finding top-5 similar notes by embedding, graph expansion SHALL be performed at depth 2 from each result

#### Scenario: Label not found
- **WHEN** user runs `graphrag similar --label "NonExistent" graph.db`
- **THEN** the command SHALL exit with an error message: "Nodo 'NonExistent' no encontrado"

### Requirement: similar --file chunks and embeds external document

`graphrag similar --file <path>` SHALL read the file, parse frontmatter and headers using the same `chunk_document()` used by `build`, concatenate each chunk as `"header\ntext"`, and embed via Ollama's `batch_embed()`. Each resulting chunk embedding SHALL be compared individually against all stored chunk embeddings, and each target note SHALL be scored by the maximum similarity across all comparisons. The same flags (`-k`, `-d`, `--notes-only`, `--min-weight`, `--filter`) SHALL be supported.

#### Scenario: Similar by file returns matches
- **WHEN** user runs `graphrag similar --file /tmp/new-note.md graph.db -k 5`
- **THEN** the file SHALL be chunked using `chunk_document()`
- **AND** each chunk SHALL be embedded via Ollama `batch_embed()`
- **AND** each chunk embedding SHALL be compared individually against stored chunk embeddings
- **AND** top-5 results by best-matching chunk pairs SHALL be displayed

#### Scenario: File not found
- **WHEN** user runs `graphrag similar --file /nonexistent/path.md graph.db`
- **THEN** the command SHALL exit with an error: "Archivo no encontrado: /nonexistent/path.md"

#### Scenario: Ollama unreachable for --file
- **WHEN** `graphrag similar --file note.md graph.db` runs and Ollama is not reachable
- **THEN** the command SHALL exit with an error indicating Ollama connection failure

### Requirement: Similar command uses same output format as search

Results from `graphrag similar` SHALL use the same `SearchResult` structure as `graphrag search`. The output format SHALL be controlled by the `--format` flag, which accepts `table`, `list`, or `json` (default: `table`). `table` mode displays score, type, label, file path, chunk header, chunk text preview, and neighbor list. `list` mode renders markdown bullet links for notes. `json` mode serializes results as JSON.

#### Scenario: Output format matches search
- **WHEN** `graphrag similar --label "Python" graph.db` returns results
- **THEN** each result SHALL include: score, label, type, file, chunk_header, chunk_text, and neighbors
- **AND** the display format SHALL match that of `graphrag search`

#### Scenario: List format produces markdown links
- **WHEN** user runs `graphrag similar --label "Python" graph.db --format list`
- **THEN** the output SHALL be markdown bullet links matching the `output-format` spec

#### Scenario: JSON format produces serialized results
- **WHEN** user runs `graphrag similar --label "Python" graph.db --format json`
- **THEN** the output SHALL be valid JSON matching the `output-format` spec
