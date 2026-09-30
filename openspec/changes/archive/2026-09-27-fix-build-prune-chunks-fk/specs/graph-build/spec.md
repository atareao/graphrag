# graph-build Specification

## Purpose

The `build_graph` function scans a directory of Markdown files, extracts entities via Ollama NER, builds a knowledge graph (nodes + edges), computes vector embeddings, and populates FTS5. This spec covers the prune phase, which removes nodes for notes deleted from disk.

## ADDED Requirements

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