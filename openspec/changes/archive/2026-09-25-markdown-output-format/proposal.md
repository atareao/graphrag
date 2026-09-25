# Proposal: `--format` flag for search commands (table | list | json)

## Why

Currently `graphrag search`, `graphrag similar`, and `graphrag fts` only display results in a single fixed tabular format. Users want to:

1. Get output as Markdown links to copy-paste into their notes (`--format list`)
2. Get machine-readable output for scripting and tool integration (`--format json`)
3. Keep the current human-readable table as default (`--format table`)

This is a pure display-layer change: no search logic, scoring, or query behavior is affected.

## What Changes

- Add `--format <table|list|json>` flag to three commands: `search`, `similar`, `fts`
- Default value: `table` (preserves current behavior)
- `list` mode outputs markdown bullet links: `- [label](/path/file.md)`
- `json` mode serializes results as JSON array of `SearchResult` objects
- Extract shared display logic into a `display_results()` helper to avoid duplication across the three commands

## Capabilities

### New Capabilities

- `output-format`: Defines the shared `--format` flag (table | list | json) accepted by `search`, `similar`, and `fts`. Controls output rendering without affecting search logic.

### Modified Capabilities

- `similar-command`: Requirement "Similar command uses same output format as search" SHALL be updated to reflect that format is now controlled by `--format`, and that `similar` accepts this flag directly.

## Impact

- **Files**: `src/main.rs` — add `--format` arg to three command structs, update `cmd_search`, `cmd_similar`, `cmd_fts`, and `display_similar_results`
- **No changes** to search logic, graph expansion, chunking, embeddings, DB schema, or dependencies
- All existing tests SHALL pass without modification