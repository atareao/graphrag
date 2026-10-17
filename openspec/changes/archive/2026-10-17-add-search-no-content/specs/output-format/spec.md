# output-format Specification

## ADDED Requirements

### Requirement: `--no-content` flag suppresses the chunk preview on search, similar, and fts

`search`, `similar` and `fts` SHALL accept a boolean `--no-content` flag. When `--no-content` is
present and the output format is `table`, the per-result content preview line
(`└── {chunk_header}: {content}`) SHALL be omitted. Every other part of the table output SHALL remain
unchanged: the `score  [type]  label  (file)  (vecinos: N)` line and the `🔗 Vecinos de ...` block of
the first result. When `--no-content` is absent, the output SHALL be identical to the behaviour before
this change. With `--format list` or `--format json`, `--no-content` SHALL be accepted and SHALL have
no effect, since those formats do not render the preview.

#### Scenario: search table omits the content preview
- **GIVEN** a database whose search returns at least one note with a non-empty `chunk_header`
- **WHEN** user runs `graphrag search "query" graph.db --no-content`
- **THEN** each result SHALL still print its score, type, label, file and `(vecinos: N)` line
- **AND** the output SHALL NOT contain `└──`
- **AND** the `🔗 Vecinos de ...` block of the first result SHALL still be printed

#### Scenario: search table without the flag is unchanged
- **WHEN** user runs `graphrag search "query" graph.db` without `--no-content`
- **THEN** the output SHALL be identical to the output before this change, including the `└──` preview line

#### Scenario: similar table omits the content preview
- **GIVEN** a database containing a note whose chunks have non-empty headers
- **WHEN** user runs `graphrag similar --label "some note" graph.db --no-content`
- **THEN** the results SHALL be listed without `└──` preview lines
- **AND** the `score  [type]  label  (file)  (vecinos: N)` lines SHALL be unchanged

#### Scenario: fts accepts the flag
- **WHEN** user runs `graphrag fts "term" graph.db --no-content`
- **THEN** the command SHALL accept the flag and exit successfully
- **AND** the output SHALL be identical to `graphrag fts "term" graph.db`, because FTS results carry no chunk preview

#### Scenario: no effect with list or json format
- **WHEN** user runs `graphrag search "query" graph.db --format list --no-content`
- **THEN** the output SHALL be identical to `graphrag search "query" graph.db --format list`
- **AND** the same SHALL hold for `--format json --no-content` against `--format json`
