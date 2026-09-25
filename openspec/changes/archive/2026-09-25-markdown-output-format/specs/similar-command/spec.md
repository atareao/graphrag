# Spec Delta

## MODIFIED Requirements

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