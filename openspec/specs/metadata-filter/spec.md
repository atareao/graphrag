# metadata-filter Specification

## Purpose
Enables structured filtering of search results by frontmatter metadata fields (date, category, etc.), allowing users to narrow semantic searches with precise data-level constraints.

## Requirements

### Requirement: Metadata filters narrow search results
`search` SHALL accept one or more `--filter` flags with expressions in the form `field operator value`, where `field` is a frontmatter key stored in the `nodes.metadata` JSON, `operator` is one of `=`, `!=`, `>=`, `<=`, `>`, `<`, and `value` is a string or number. Filters SHALL be applied via SQLite `json_extract()` against the parent note's metadata. Only notes matching ALL filters SHALL be included as candidates for vector search.

#### Scenario: Single filter by date
- **WHEN** user runs `graphrag search "tutorial" graph.db --filter 'date >= 2023'`
- **THEN** only chunks whose parent note has `metadata->>'$.date' >= 2023` SHALL be considered for vector search

#### Scenario: Multiple filters combined with AND
- **WHEN** user runs `graphrag search "docker" graph.db --filter 'category = tutorial' --filter 'date >= 2022'`
- **THEN** only notes matching BOTH `category = tutorial` AND `date >= 2022` SHALL be candidates

#### Scenario: Filter with non-existent field
- **WHEN** user runs `graphrag search "rust" graph.db --filter 'nonexistent = value'`
- **THEN** notes without that field in metadata SHALL NOT match the filter (null comparison)

#### Scenario: Invalid filter syntax
- **WHEN** user runs `graphrag search "test" graph.db --filter 'badformat'`
- **THEN** the command SHALL exit with an error indicating the correct `field operator value` syntax

#### Scenario: Multiple --filter flags
- **WHEN** user passes multiple `--filter` flags
- **THEN** they SHALL be combined as AND conditions
- **AND** the order of filters SHALL NOT affect the result set
