# community-summarization Specification

## Purpose
Generates structured LLM summaries for each detected community, storing them as JSON with summary text, keywords, and topics. Summary text is also embedded for semantic search.

## Requirements

### Requirement: LLM generates structured JSON summary per community
For each community in the `communities` table with `summary IS NULL`, `summarize` SHALL build a prompt with the community's member entities and their inter-relationships, call Ollama generate, and parse the result as structured JSON with fields: `summary` (string), `keywords` (array of strings), `topics` (array of strings).

#### Scenario: Community summary prompt
- **GIVEN** a community with entities ["Python", "FastAPI", "SQLite", "Pydantic"] and edges (Python→FastAPI: `uses`, FastAPI→SQLite: `connects`, Python→Pydantic: `uses`)
- **WHEN** `build_community_prompt()` is called
- **THEN** the prompt SHALL include the list of entities
- **AND** the edges/relationships between them
- **AND** request JSON output with fields `summary`, `keywords`, and `topics`

#### Scenario: Successful summarization
- **WHEN** the LLM returns valid JSON `{"summary": "Ecosistema Python para desarrollo web...", "keywords": ["python", "fastapi", "web"], "topics": ["web development", "api"]}`
- **THEN** the `communities` row SHALL be updated with `summary` set to the JSON string
- **AND** `summary_tokens` SHALL record the total tokens used
- **AND** `summary_model` SHALL record the model name used

#### Scenario: Invalid JSON from LLM
- **WHEN** the LLM returns malformed JSON
- **THEN** `summarize` SHALL retry once with an error message in the prompt asking for valid JSON
- **AND** if the retry also fails, the community SHALL be skipped
- **AND** a warning SHALL be logged

### Requirement: Summary embedding for search
After summarization, the `summary` text field SHALL be embedded via Ollama `/api/embed` and stored in `summary_embedding` as a f32 BLOB.

#### Scenario: Summary embedding stored
- **WHEN** a community is successfully summarized
- **THEN** `summary_embedding` SHALL contain a valid f32 embedding vector of the summary text
- **AND** the embedding dimension SHALL match the configured embed model

### Requirement: Progress display
During summarization, a progress bar SHALL display: community index / total communities, current community label, and total tokens consumed so far.

#### Scenario: Progress bar visible
- **WHEN** `summarize` runs with 10 communities
- **THEN** a progress bar SHALL update after each community is processed
- **AND** the final display SHALL show "10/10 communities summarized (X tokens total)"

### Requirement: CLI integration
`graphrag community summarize <db>` SHALL be a valid command. It SHALL accept `--summary-model` (default from config) and `--embed-model` (default from config) flags.

#### Scenario: Summarize from CLI
- **GIVEN** a database with detected communities
- **WHEN** `graphrag community summarize test.db` runs
- **THEN** it SHALL summarize all communities with NULL summary
- **AND** display progress and final stats

#### Scenario: Already summarized
- **GIVEN** a database where all communities already have summaries
- **WHEN** `graphrag community summarize test.db` runs
- **THEN** it SHALL display "All communities already summarized"
- **AND** exit with status 0