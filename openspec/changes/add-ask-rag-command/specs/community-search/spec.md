# community-search Specification

## Purpose

Delta: make the RAG generator (`answer_query`) configurable and return structured sources, so it can back the new `graphrag ask` command as well as `search --answer`.

## MODIFIED Requirements

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
