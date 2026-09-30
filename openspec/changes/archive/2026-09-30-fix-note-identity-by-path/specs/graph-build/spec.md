# graph-build Specification

## Purpose

The `build_graph` function scans a directory of Markdown files, extracts entities via Ollama NER, and builds a knowledge graph. This delta changes note identification from the title (`label`) to the file path (`key`) so that builds converge even when multiple files share the same title.

## ADDED Requirements

### Requirement: Build upserts notes by key, not by label

The writer SHALL identify note nodes by `key = "note:" + relative_path` and SHALL use `ON CONFLICT(key) DO UPDATE` for the note insert. All subsequent `SELECT id FROM nodes` resolutions for notes SHALL query by `key`.

#### Scenario: Note insert does not collide on duplicate titles

- **GIVEN** two files `a/dup.md` and `b/dup.md`, both with `title: Duplicado`
- **WHEN** the writer processes both
- **THEN** two distinct note nodes SHALL exist with keys `note:a/dup.md` and `note:b/dup.md`
- **AND** both SHALL have `label = 'Duplicado'`
- **AND** neither insert SHALL overwrite the other

### Requirement: Build converges for files that share a title

After a successful build, re-running the build SHALL mark files as processed based on their path and stored hash, so that files sharing a title do not remain perpetually pending.

#### Scenario: Duplicate-title repro converges

- **GIVEN** an empty database and two files `a/dup.md` and `b/dup.md` with the same `title`
- **WHEN** the build runs twice
- **THEN** the first build SHALL store 2 note nodes
- **AND** the second build SHALL report `0` files to process

#### Scenario: Incremental build still skips unchanged files

- **GIVEN** a database where a note `note:a/dup.md` exists with a hash matching the file on disk
- **WHEN** the pre-examine runs
- **THEN** `a/dup.md` SHALL NOT be added to `to_process`

### Requirement: Entity nodes retain their identity during build

The writer SHALL upsert entity, tag and directory nodes by `key = "node:" + label`, so processing a note whose title matches an entity does not convert or overwrite that entity.

#### Scenario: Processing a note titled like an entity

- **GIVEN** an entity node `label='funzzy'`, `type='tool'`, `key='node:funzzy'`
- **WHEN** the build processes `Notas/Comandos Linux/funzzy.md`
- **THEN** the note node SHALL be created with `key='note:Notas/Comandos Linux/funzzy.md'`
- **AND** the entity node SHALL remain with `type='tool'` and `key='node:funzzy'`
