# rag-ask Specification

## Purpose

Point of entry that answers a question in natural language: retrieves relevant knowledge with hybrid search and generates a grounded answer with Ollama, citing the source notes. This is the "one command that is RAG" — retrieval + generation in a single step — while `search`/`fts`/`graph`/`path` stay as pure-inspection tools.

## ADDED Requirements

### Requirement: `ask` command returns a grounded answer

`graphrag ask <QUERY> [DB]` SHALL retrieve with hybrid search and generate a narrative answer grounded in the retrieved note content.

#### Scenario: Answer grounded in notes

- **GIVEN** a database with notes about `nginx` and Ollama available
- **WHEN** the user runs `graphrag ask "cómo configuro nginx" db`
- **THEN** the output SHALL contain a narrative answer (not only a result list)
- **AND** a "Sources" section listing the note labels/paths used as evidence

#### Scenario: No relevant matches

- **GIVEN** a database with no relevant notes
- **WHEN** the user runs `graphrag ask "asdjkhqwe" db`
- **THEN** the output SHALL state that no relevant information was found
- **AND** the exit code SHALL be `0`

### Requirement: Retrieval honors the same flags as `search`

`ask` SHALL accept `-k`, `-d`, `-a`, `--min-weight`, `--notes-only` and `--filter`, and SHALL apply them to retrieval.

#### Scenario: Filter narrows the evidence

- **GIVEN** a database with dated notes
- **WHEN** the user runs `graphrag ask "backups" db --filter 'date >= 2024'`
- **THEN** only notes matching the filter SHALL be used as evidence
- **AND** the number of retrieved results SHALL follow `-k`

#### Scenario: Graph expansion depth is configurable

- **WHEN** the user runs `graphrag ask "nginx" db -d 0`
- **THEN** retrieval SHALL be vector-only (no graph expansion)

### Requirement: Generation context combines communities and chunk evidence

`ask` SHALL build the prompt from the top-N most similar community summaries (default 3, configurable with `--communities`) plus the top-k retrieved chunks (default `k`), truncating summaries and chunks to the configured limits.

#### Scenario: Communities available

- **GIVEN** a database with community summaries
- **WHEN** the user runs `graphrag ask "nginx" db`
- **THEN** the prompt SHALL include up to 3 community summaries
- **AND** up to `k` chunk evidences

#### Scenario: No communities

- **GIVEN** a database where `community detect` has not been run
- **WHEN** the user runs `graphrag ask "nginx" db`
- **THEN** the answer SHALL be generated using only chunk evidence
- **AND** a notice SHALL suggest running `graphrag community detect`

### Requirement: Graceful degradation without Ollama

If Ollama is unreachable, `ask` SHALL NOT fail: it SHALL print the hybrid search results and a warning, and SHALL exit with code `0`.

#### Scenario: Ollama is down

- **GIVEN** Ollama is not running
- **WHEN** the user runs `graphrag ask "nginx" db`
- **THEN** the command SHALL print the retrieval results
- **AND** a warning that answer generation was skipped
- **AND** the exit code SHALL be `0`

### Requirement: Sources and JSON output

`ask` SHALL list the source notes used as evidence. With `--format json`, the output SHALL be a JSON object with `answer`, `sources` and `results`.

#### Scenario: JSON output

- **WHEN** the user runs `graphrag ask "nginx" db --format json`
- **THEN** stdout SHALL be valid JSON
- **AND** it SHALL contain `answer` (string) and `sources` (array of objects with `label` and `path`)

#### Scenario: Sources are deduplicated

- **GIVEN** two retrieved chunks belonging to the same note
- **WHEN** the user runs `graphrag ask "nginx" db`
- **THEN** that note SHALL appear only once in the "Sources" section

### Requirement: Model selection

`ask` SHALL use `--model` when provided, otherwise the configured `summary_model`.

#### Scenario: Model override

- **WHEN** the user runs `graphrag ask "nginx" db --model gpt-oss:latest`
- **THEN** answer generation SHALL use `gpt-oss:latest`
