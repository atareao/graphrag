# community-summarization Specification

## Purpose
Generates structured LLM summaries for each detected community, storing them as JSON with summary text, keywords, and topics. Summary text is also embedded for semantic search.

## Requirements

### Requirement: LLM generates structured JSON summary per community

For each community in the `communities` table with `summary IS NULL`, `summarize` SHALL build a
prompt with the community's member entities and their inter-relationships, call Ollama generate, and
parse the result as structured JSON with fields: `summary` (string), `keywords` (array of strings),
`topics` (array of strings). The parsed `summary` text SHALL be stored in `communities.summary`, and
the parsed `keywords` and `topics` arrays SHALL be persisted in their own columns (see the
persistence requirement below) instead of being discarded.

#### Scenario: Community summary prompt

- **GIVEN** a community with entities ["Python", "FastAPI", "SQLite", "Pydantic"] and edges (Python→FastAPI: `uses`, FastAPI→SQLite: `connects`, Python→Pydantic: `uses`)
- **WHEN** `build_community_prompt()` is called
- **THEN** the prompt SHALL include the list of entities
- **AND** the edges/relationships between them
- **AND** request JSON output with fields `summary`, `keywords`, and `topics`

#### Scenario: Successful summarization

- **WHEN** the LLM returns valid JSON `{"summary": "Ecosistema Python para desarrollo web...", "keywords": ["python", "fastapi", "web"], "topics": ["web development", "api"]}`
- **THEN** `communities.summary` SHALL be set to the `summary` text
- **AND** `communities.keywords` SHALL be set to `["python", "fastapi", "web"]`
- **AND** `communities.topics` SHALL be set to `["web development", "api"]`
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

### Requirement: keywords and topics are persisted

`community summarize` SHALL persist the LLM-provided `keywords` and `topics` for each summarized
community. The `communities` table SHALL gain two nullable `TEXT` columns, `keywords` and `topics`,
holding JSON arrays. Communities that have not been summarized SHALL have both columns equal to
`NULL`. The recurrence of summarization SHALL not lose already-persisted keywords/topics for
communities that are not re-processed.

#### Scenario: Persisted keywords on fresh summary

- **GIVEN** a detected community with `summary IS NULL`
- **WHEN** `summarize` processes it and the LLM returns `keywords=["tar","compresión"]` and `topics=["linux","ficheros"]`
- **THEN** the community row SHALL have `keywords` equal to the JSON array `["tar","compresión"]`
- **AND** `topics` equal to the JSON array `["linux","ficheros"]`

#### Scenario: Empty arrays are preserved

- **GIVEN** the LLM returns `keywords=[]` and `topics=[]`
- **WHEN** the community is summarized
- **THEN** `keywords` and `topics` SHALL be stored as the JSON arrays `[]` (not `NULL`)

#### Scenario: Unsummarized community has NULL keywords

- **GIVEN** a community that has never been summarized
- **WHEN** its row is read
- **THEN** `keywords` and `topics` SHALL both be `NULL`

### Requirement: Schema migration to v3 adds keywords and topics columns

`init_db` SHALL migrate an existing `communities` table that lacks the `keywords`/`topics` columns
by adding them (`ALTER TABLE communities ADD COLUMN`), gated by `PRAGMA user_version < 3` and made
idempotent via a column-existence check. After migration, `PRAGMA user_version` SHALL be `3`. Fresh
databases (created by `init_db` from scratch) SHALL include both columns directly and SHALL also end
at `user_version = 3`.

#### Scenario: Legacy v2 database is migrated

- **GIVEN** a database with a v2 `communities` table (no `keywords`/`topics` columns) and `user_version = 2`
- **WHEN** `init_db` runs
- **THEN** both columns SHALL exist
- **AND** pre-existing community rows SHALL survive with `keywords IS NULL` and `topics IS NULL`
- **AND** `PRAGMA user_version` SHALL be `3`

#### Scenario: Migration is idempotent

- **GIVEN** a database already at `user_version = 3`
- **WHEN** `init_db` runs again
- **THEN** no error SHALL occur
- **AND** `PRAGMA user_version` SHALL remain `3`
- **AND** the columns SHALL NOT be duplicated

#### Scenario: Fresh database has columns and version 3

- **GIVEN** an empty database file
- **WHEN** `init_db` runs
- **THEN** `communities` SHALL have the `keywords` and `topics` columns
- **AND** `PRAGMA user_version` SHALL be `3`
