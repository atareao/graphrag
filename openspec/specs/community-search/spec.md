# community-search Specification

## Purpose
Integrates community summaries into search: normal search displays relevant communities alongside chunk results; `--answer` mode uses community summaries as high-level RAG context plus chunk evidence for LLM-generated narrative answers.

## Requirements

### Requirement: Normal search shows relevant communities
After computing chunk-level results and graph expansion, `search` SHALL compute the top-2 most similar communities by cosine similarity against `communities.summary_embedding`. These SHALL be displayed below the main results with community label and summary preview.

#### Scenario: Search displays community context
- **GIVEN** a database with populated communities and their summaries
- **WHEN** user runs `graphrag search "Python web frameworks" test.db`
- **THEN** the output SHALL include the main results (notes + chunks)
- **AND** a "Related communities" section showing top-2 matching communities
- **AND** each community SHALL show its label and first 120 characters of the summary

#### Scenario: No communities exist
- **GIVEN** a database where `community detect` has not been run
- **WHEN** user runs `graphrag search "query" test.db`
- **THEN** the output SHALL NOT include a "Related communities" section
- **AND** no error SHALL be shown

### Requirement: Answer mode (`--answer`) uses community summaries as RAG context

When `--answer` is provided, `search` SHALL:
1. Compute the query embedding and find the top-N relevant communities (configurable, default 3)
2. Find the top-M relevant chunks (configurable, default 5)
3. Build an LLM prompt with community summaries as high-level context and chunk snippets as evidence
4. Call Ollama generate and return a narrative answer with source citations

The generator SHALL be exposed as a reusable function that accepts generation options (community count, evidence count, truncation limits) and SHALL return a structured result containing the answer text and the deduplicated list of source notes.

#### Scenario: Answer mode returns narrative response

- **GIVEN** a database with community summaries and chunk embeddings
- **WHEN** the user runs `graphrag search --answer "¿qué ecosistema es mejor para prototipado?" test.db`
- **THEN** the response SHALL be a narrative paragraph (not a list of results)
- **AND** it SHALL cite sources (note titles or chunk headers)

#### Scenario: Generator returns sources

- **GIVEN** a set of hybrid search results spanning several notes
- **WHEN** `answer_query` is called with those results
- **THEN** it SHALL return an `AnswerResult` whose `sources` list each underlying note exactly once

#### Scenario: Answer with no communities

- **GIVEN** a database where `community detect` has not been run
- **WHEN** the user runs `graphrag search --answer "query" test.db`
- **THEN** the command SHALL display a notice suggesting `graphrag community detect`
- **AND** SHALL fall back to answering using only chunk evidence

#### Scenario: Answer with no matches at all

- **GIVEN** a database with no relevant chunks or communities
- **WHEN** the user runs `graphrag search --answer "asdasdasdasd" test.db`
- **THEN** the command SHALL display: "No relevant information found for your query."

#### Scenario: Answer mode uses the configured generation model

- **GIVEN** a config with `summary_model = "llama3.2:3b"` and `embed_model = "bge-m3"`
- **WHEN** the user runs `graphrag search --answer "query" test.db`
- **THEN** answer generation SHALL use `summary_model`, not the embedding model
- **AND** the command SHALL produce a non-empty narrative answer

### Requirement: Answer mode respects context window limits

To avoid exceeding the LLM context window, the generator SHALL default to:
- Top-3 most relevant communities
- Top-5 most relevant chunks
- Community summaries truncated to 500 characters max each
- Chunk text truncated to 300 characters max each

These counts and limits SHALL be configurable through the generator options; the defaults SHALL remain unchanged for `search --answer`.

#### Scenario: Large result set truncated

- **GIVEN** a database with 20 communities and 50 relevant chunks
- **WHEN** the user runs `graphrag search --answer "query" test.db`
- **THEN** at most 3 communities and 5 chunks SHALL be included in the prompt

#### Scenario: Custom limits

- **GIVEN** a caller passing `communities = 1` and `evidence = 3`
- **WHEN** the generator builds the prompt
- **THEN** at most 1 community and 3 chunks SHALL be included

### Requirement: CLI integration
`graphrag search --answer <query> <db>` SHALL be a valid command. The `--answer` flag SHALL be a boolean flag (no value). Existing flags (`-k`, `-d`, `-a`, `--filter`, `--notes-only`, `--min-weight`) SHALL still work when combined with `--answer`.

#### Scenario: Answer mode with filters
- **WHEN** user runs `graphrag search --answer "web frameworks" test.db --filter 'date >= 2023'`
- **THEN** only chunks from notes matching the filter SHALL be used as evidence
- **AND** only communities whose entities appear in those notes SHALL be considered
