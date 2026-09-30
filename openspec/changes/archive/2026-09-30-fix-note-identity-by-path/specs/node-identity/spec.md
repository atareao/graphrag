# node-identity Specification

## Purpose

Every node in the GraphRAG knowledge graph must have a stable, unique identity. Notes are identified by their file path; all other nodes (entities, tags, directories) are identified by their label. This spec defines the `key` column, its uniqueness, and the migration from the legacy schema where `nodes.label` was the sole unique identity.

## ADDED Requirements

### Requirement: Every node has a unique `key`

The `nodes` table SHALL have a `key TEXT NOT NULL` column with a unique index `idx_nodes_key`. The `key` is the node identity used for upserts and lookups.

- `key` SHALL be non-null and unique across all nodes.
- `label` SHALL remain a display field and SHALL NOT be unique.

#### Scenario: Two nodes may share a label with different keys

- **GIVEN** a database with the v2 schema
- **WHEN** two nodes are inserted with `label = 'Duplicado'` but keys `note:a/dup.md` and `note:b/dup.md`
- **THEN** both inserts SHALL succeed
- **AND** the table SHALL contain exactly 2 nodes

#### Scenario: Duplicate key is rejected

- **GIVEN** a database with the v2 schema
- **AND** a node with `key = 'note:a/dup.md'`
- **WHEN** another node with `key = 'note:a/dup.md'` is inserted
- **THEN** the insert SHALL fail with a uniqueness constraint error

### Requirement: Note keys are derived from the relative path

A note node's `key` SHALL be `"note:" + <ruta relativa del fichero>`, using the same path string stored in `metadata.path`.

#### Scenario: Note key matches its stored path

- **GIVEN** a file `Notas/Ubuntu/foo.md` processed by the build
- **WHEN** the note node is stored
- **THEN** `nodes.key` SHALL be `note:Notas/Ubuntu/foo.md`
- **AND** `json_extract(metadata,'$.path')` SHALL be `Notas/Ubuntu/foo.md`

### Requirement: Non-note keys are derived from the label

Entity, tag and directory nodes SHALL have `key = "node:" + <label>`.

#### Scenario: Entity key is prefixed

- **GIVEN** the build extracts an entity with `label = "Python"`
- **WHEN** the entity node is stored
- **THEN** `nodes.key` SHALL be `node:Python`

### Requirement: A note and a non-note may share a label

Because keys use different prefixes (`note:` vs `node:`), a note titled `hyperfine` and the `tool` entity `hyperfine` SHALL be able to coexist as two distinct nodes, without converting the entity into a note.

#### Scenario: Note does not overwrite a same-named entity

- **GIVEN** an entity node with `label = 'funzzy'`, `type = 'tool'`, `key = 'node:funzzy'`
- **WHEN** the build processes the file `Notas/Comandos Linux/funzzy.md` (title `funzzy`)
- **THEN** a new note node SHALL be created with `key = 'note:Notas/Comandos Linux/funzzy.md'`
- **AND** the entity node SHALL keep `type = 'tool'`

### Requirement: Schema migration v1 -> v2 is idempotent and preserves identity data

`init_db` SHALL migrate a legacy database (without `key`) to the v2 schema exactly once, gated by `PRAGMA user_version < 2`.

- The migration SHALL preserve every node `id` and the FKs from `edges`/`chunks`.
- Backfill: notes -> `'note:' || COALESCE(json_extract(metadata,'$.path'), 'legacy:' || id)`; non-notes -> `'node:' || label`.
- After migration, `user_version` SHALL be `2`.
- Running `init_db` again on a v2 database SHALL be a no-op.

#### Scenario: Legacy database is migrated

- **GIVEN** a database with the legacy schema (`UNIQUE(label)`, no `key` column, `user_version = 0`)
- **AND** a note node with `label='Foo'` and `metadata.path='a/foo.md'`, and an entity node `label='Python'`
- **WHEN** `init_db` runs
- **THEN** the note's `key` SHALL be `note:a/foo.md`
- **AND** the entity's `key` SHALL be `node:Python`
- **AND** both node `ids` SHALL be unchanged
- **AND** `user_version` SHALL be `2`
- **AND** the unique index `idx_nodes_key` SHALL exist

#### Scenario: Migration is idempotent

- **GIVEN** a database already at `user_version = 2`
- **WHEN** `init_db` runs again
- **THEN** no rows SHALL be modified or re-keyed
- **AND** `user_version` SHALL remain `2`
