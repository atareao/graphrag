# Spec Delta

## Purpose

Controls output rendering for search commands (`search`, `similar`, `fts`) — users choose between a human-readable table, a copy-pasteable markdown list, or machine-readable JSON output without changing how the search works.

## ADDED Requirements

### Requirement: `--format` flag on search, similar, and fts

The `search`, `similar`, and `fts` commands SHALL accept a `--format` flag with values `table`, `list`, or `json`. The default value SHALL be `table`, preserving current display behavior.

#### Scenario: Default format is table
- **WHEN** user runs `graphrag search "query" graph.db` without `--format`
- **THEN** the output SHALL be identical to the current tabular display (score, type, label, file, neighbors)

#### Scenario: Explicit table format
- **WHEN** user runs `graphrag search "query" graph.db --format table`
- **THEN** the output SHALL match the default table format

#### Scenario: Invalid format value
- **WHEN** user runs `graphrag search "query" graph.db --format csv`
- **THEN** the command SHALL exit with a clap validation error listing valid values: `table`, `list`, `json`

### Requirement: `--format list` outputs markdown bullet links

When `--format list` is used, each result note SHALL be rendered as a single markdown bullet line: `- [label](/path/file.md)`. Non-note results (entities, tags) SHALL be omitted since they lack file paths. No scores, types, or neighbor information SHALL be shown.

#### Scenario: List format for search
- **WHEN** user runs `graphrag search "markdown" graph.db -k 3 --format list`
- **THEN** the output SHALL contain only lines matching `- [Note Label](/path/to/note.md)`
- **AND** SHALL contain at most 3 lines (one per result)
- **AND** SHALL omit entities and tags from output

#### Scenario: List format for similar --label
- **WHEN** user runs `graphrag similar --label "Python" graph.db -k 5 --format list`
- **THEN** the output SHALL contain markdown bullet links similar to search's list format

#### Scenario: List format for fts
- **WHEN** user runs `graphrag fts "python" graph.db -l 10 --format list`
- **THEN** the output SHALL contain markdown bullet links for each FTS result

#### Scenario: List format with notes_only already active
- **WHEN** user runs `graphrag search "query" graph.db --format list --notes-only`
- **THEN** the output SHALL be the same as `--format list` alone (list mode already filters non-notes)

### Requirement: `--format json` outputs SearchResult array as JSON

When `--format json` is used, the full results array SHALL be serialized as a JSON array using `serde_json`. Each element SHALL contain all `SearchResult` fields: `id`, `label`, `type`, `score`, `content`, `file`, `neighbors`, `chunk_header`, `chunk_text`.

#### Scenario: JSON format for search
- **WHEN** user runs `graphrag search "markdown" graph.db -k 3 --format json`
- **THEN** the output SHALL be valid JSON
- **AND** SHALL be a JSON array with at most 3 elements
- **AND** each element SHALL contain the fields: `id`, `label`, `type`, `score`, `content`, `file`, `neighbors`, `chunk_header`, `chunk_text`

#### Scenario: Empty results in JSON
- **WHEN** a search returns no results with `--format json`
- **THEN** the output SHALL be `[]` (an empty JSON array)

#### Scenario: JSON format for similar --label
- **WHEN** user runs `graphrag similar --label "Python" graph.db -k 5 --format json`
- **THEN** the output SHALL be valid JSON identical in structure to search's JSON format