# community-listing Specification

## Purpose
Inspect the communities stored in the database from the CLI. Until now `community` only exposed
`detect` and `summarize`; listing required raw SQL. This capability adds `community list`, which
shows each community's identity (id, level, label), its keywords/topics (persisted by
`community-summarization`), and the entities that belong to it, with table and JSON output.

## Requirements

### Requirement: `community list` command lists stored communities

`graphrag community list [db]` SHALL read the `communities` table and print one entry per
community. It SHALL NOT require Ollama. It SHALL accept:

- an optional positional `db` path (default `graphrag.db`, with the same sentinel/config fallback as
  the other commands),
- `--level <N>` (integer): show only communities at that level,
- `--limit <N>` (integer, default `50`): show at most N entries,
- `--format table|json` (default `table`).

For every listed community, member entity labels SHALL be resolved from `member_ids` by querying the
`nodes` table. Communities SHALL be ordered by `level` ascending and then by `id` ascending, so
output is deterministic.

#### Scenario: List all communities

- **GIVEN** a database with 3 communities at levels 1 and 2
- **WHEN** the user runs `graphrag community list db`
- **THEN** the output SHALL contain 3 entries
- **AND** each entry SHALL include its `id`, `level` and `member_count`

#### Scenario: Filter by level

- **GIVEN** a database with 2 level-1 communities and 3 level-2 communities
- **WHEN** the user runs `graphrag community list db --level 2`
- **THEN** the output SHALL contain exactly the 3 level-2 communities

#### Scenario: Limit the number of entries

- **GIVEN** a database with 100 communities
- **WHEN** the user runs `graphrag community list db --limit 10`
- **THEN** the output SHALL contain at most 10 entries
- **AND** the entries SHALL be the first 10 in the deterministic `(level, id)` order

#### Scenario: Empty communities table

- **GIVEN** a database with no rows in `communities`
- **WHEN** the user runs `graphrag community list db`
- **THEN** the command SHALL print a message indicating there are no communities
- **AND** exit with status 0

#### Scenario: No Ollama required

- **GIVEN** Ollama is not running
- **WHEN** the user runs `graphrag community list db`
- **THEN** the command SHALL succeed and list the communities
- **AND** SHALL NOT attempt any network call

### Requirement: Table output shows keywords (with label fallback) and members

In `table` format (the default), each community SHALL be rendered as one row with columns:
`id`, `nivel` (level), `n` (member_count), `palabras clave`, and `miembros` (a preview of member
entity labels joined by `, `, truncated with `…` when long).

The `palabras clave` column SHALL show the community's persisted `keywords` joined by `, `. When a
community has no keywords (not summarized yet, or the LLM returned none), the column SHALL fall
back to the community's `label` so the row is never empty. When the community has a non-null
`summary`, the `miembros` cell SHALL render that summary text (truncated) instead of the member
preview; otherwise it SHALL render the member labels preview.

#### Scenario: Keywords shown when present

- **GIVEN** a summarized community with `keywords = ["tar", "compresión"]`
- **WHEN** it is rendered in `table` format
- **THEN** the `palabras clave` column SHALL contain `tar, compresión`

#### Scenario: Label fallback when no keywords

- **GIVEN** a community with no keywords and no summary, `label = "community_2_289"`
- **WHEN** it is rendered in `table` format
- **THEN** the `palabras clave` column SHALL contain `community_2_289`

#### Scenario: Members preview

- **GIVEN** a community whose `member_ids` resolve to `["bzip2","gunzip","tar -cvzf","tar -xvzf"]`
- **WHEN** it is rendered in `table` format
- **THEN** the `miembros` column SHALL contain the member labels joined by `, `

### Requirement: JSON output exposes full community fields

In `json` format, `community list` SHALL print a JSON array whose elements contain, for each
community: `id`, `label`, `level`, `parent_id`, `member_count`, `keywords` (array), `topics`
(array), `summary` (string or null), and `members` (array of member entity labels, untruncated).
The output SHALL be valid JSON even when there are no communities (an empty array `[]`).

#### Scenario: JSON is valid and complete

- **GIVEN** a database with 1 community having members `[1,2]` resolving to `["Python","FastAPI"]`
- **WHEN** the user runs `graphrag community list db --format json`
- **THEN** the output SHALL parse as a JSON array with 1 element
- **AND** the element SHALL contain a `members` array equal to `["Python","FastAPI"]`
- **AND** `keywords` and `topics` SHALL be arrays

#### Scenario: Empty JSON output

- **GIVEN** a database with no communities
- **WHEN** the user runs `graphrag community list db --format json`
- **THEN** the output SHALL be `[]`
