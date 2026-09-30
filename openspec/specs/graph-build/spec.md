# graph-build Specification

## Purpose
The `build_graph` function scans a directory of Markdown files, extracts entities via Ollama NER, builds a knowledge graph (nodes + edges), computes vector embeddings, and populates FTS5. This spec covers the prune phase, which removes nodes for notes deleted from disk.

## Requirements

### Requirement: Prune deletes chunks before nodes to avoid FK constraint failure

When pruning notes whose source files no longer exist on disk, the build SHALL delete associated rows from the `chunks` table before deleting the note's node, to avoid violating the `FOREIGN KEY (note_id) REFERENCES nodes(id)` constraint.

#### Scenario: Prune de nota con chunks no falla por FK

- **GIVEN** a database with a note node having `type = 'note'` and metadata containing a `path` that does not exist on disk
- **AND** the `chunks` table contains rows referencing that note's `id` via `note_id`
- **AND** the note has edges in the `edges` table
- **WHEN** `prune_stale_notes` executes the deletion loop
- **THEN** the chunks SHALL be deleted first (`DELETE FROM chunks WHERE note_id = ?`)
- **AND** the edges SHALL be deleted next
- **AND** the note node SHALL be deleted successfully
- **AND** no `FOREIGN KEY constraint failed` error SHALL occur

#### Scenario: Prune de nota sin chunks es segura

- **GIVEN** a database with a note node having `type = 'note'` with a non-existent file path
- **AND** the `chunks` table has NO rows for that note
- **WHEN** prune executes `DELETE FROM chunks WHERE note_id = ?`
- **THEN** the DELETE SHALL succeed (no-op, 0 rows affected)
- **AND** the subsequent node deletion SHALL succeed

#### Scenario: Prune de nota conserva otras notas y chunks

- **GIVEN** a database with two notes: Note A (file deleted) and Note B (file exists)
- **AND** both notes have chunks referencing them
- **WHEN** prune executes
- **THEN** only Note A SHALL be deleted
- **AND** Note B and its chunks SHALL remain intact

### Requirement: Notes without hash/path in metadata are migrated during build

When the `build_graph` pre-examine encounters a note node whose JSON metadata lacks `hash` or `path` fields (from a previous graphrag version), the build SHALL:
1. Treat that note as "needs reprocessing" (include it in `to_process`)
2. After reprocessing, store the new metadata WITH `hash` and `path` fields
3. The pre-examine SHALL NOT skip these notes — they must be reprocessed at least once to populate the hash/path

This ensures that after one full build, every note in the database has `hash` and `path` metadata, enabling true incremental builds on subsequent runs.

#### Scenario: Legacy note without hash/path is reprocessed

- **GIVEN** a database with a note node having `type = 'note'`
- **AND** its `metadata` column contains `{"slug": "foo", "content": "..."}` (no `hash` or `path` keys)
- **AND** the corresponding `.md` file exists on disk
- **WHEN** `build_graph` executes the pre-examine phase
- **THEN** the note SHALL NOT be added to the `existing` HashMap
- **AND** its file SHALL be added to `to_process`
- **WHEN** the writer processes this file
- **AND** commits the transaction
- **THEN** the note's `metadata` SHALL contain `path`, `hash`, and `slug` keys

### Requirement: Pre-examine uses a separate progress indicator

The pre-examine phase (hash comparison loop) SHALL NOT reuse the main processing progress bar. Instead:

- The pre-examine SHALL display a **spinner** with a message like `"🔍 Comparando hashes: {skipped}/{total}"`
- The main processing SHALL use a **progress bar** initialized with `total_to_process` after the pre-examine completes
- This prevents users from seeing `1/1224` (skipped count) and mistaking it for a fresh build

#### Scenario: Pre-examine spinner shown during hash comparison

- **GIVEN** a database with 7 notes that have stored hashes
- **AND** a directory of 1224 `.md` files (7 unchanged, 1217 new/modified)
- **WHEN** `build_graph` executes
- **THEN** a spinner SHALL appear showing `"🔍 Comparando hashes: 7/1224"`
- **AND** after the pre-examine, the main progress bar SHALL show `0/1217`
- **AND** after processing the first file, the bar SHALL show `1/1217`

### Requirement: SIGINT handler checkpoints WAL before exit

When the process receives SIGINT (Ctrl+C), the build SHALL:

1. Interrupt the current processing
2. Execute `PRAGMA wal_checkpoint(TRUNCATE)` on the main connection to flush committed transactions from the WAL to the main DB file
3. Print a message: `"⚠️  Build interrumpido. Checkpoint WAL completado. {n} archivos procesados."`
4. Exit with code 130

This ensures that any committed file transactions survive the process termination and are visible on the next build.

#### Scenario: SIGINT triggers WAL checkpoint

- **GIVEN** a running `build_graph` that has committed 7 file transactions
- **WHEN** the process receives SIGINT
- **THEN** `PRAGMA wal_checkpoint(TRUNCATE)` SHALL be executed
- **AND** the program SHALL print the interrupted message
- **AND** exit with code 130
- **AND** on the next `Connection::open`, the 7 committed notes SHALL be visible in the database

#### Scenario: SIGINT during pre-examine is safe

- **GIVEN** a running `build_graph` still in the pre-examine phase (no writer transactions started)
- **WHEN** the process receives SIGINT
- **THEN** the program SHALL exit cleanly with code 130
- **AND** no WAL checkpoint is needed (no writes occurred)

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
