# Design

## Context

The CLI has three search commands (`search`, `similar`, `fts`) that each render results inline with slightly different display logic. `cmd_search` has inline display, `cmd_fts` has inline display, and `cmd_similar` delegates to a shared `display_similar_results()` helper. All three use `SearchResult` as their output struct (already derives `Serialize`). See proposal.md for motivation.

## Goals / Non-Goals

**Goals:**
- Add `--format <table|list|json>` to `search`, `similar`, and `fts`
- `table` mode: identical to current display (backward compatible)
- `list` mode: `- [label](/path/file.md)` per result (notes only have paths)
- `json` mode: serialized `SearchResult[]` as JSON array
- Extract shared display logic to avoid duplication

**Non-Goals:**
- Changing search, scoring, or expansion logic
- Adding new output formats beyond these three
- Modifying the `graph` or `path` commands (they have different output needs)

## Decisions

### Decision: Single `--format` enum (not separate `--markdown` / `--json` flags)

Chosen over separate boolean flags because:
- Mutually exclusive modes are naturally modelled as an enum
- Adding a 4th format later only touches the enum, not a new flag per command
- `--format json` is more self-documenting than `--json` when mixed with other flags

Alternatives considered: `--markdown` / `--json` as separate boolean flags (rejected — they'd need mutual exclusion logic).

### Decision: `list` mode only emits notes (not entities/tags)

Entities and tags rarely have meaningful file paths in metadata. Showing them as `- [EntityName]` without a link would be confusing. The `list` mode skips non-note results. Users wanting entities/tags can use the default `table` mode or pipe `json` to `jq`.

### Decision: Shared `display_results()` function

A single `fn display_results(results: &[SearchResult], format: &str)` function handles all three modes. This keeps display logic in one place and makes it trivial to add the flag to new commands later.

### Decision: JSON output uses existing `SearchResult` Serialize derive

`SearchResult` already derives `serde::Serialize`. No new struct needed. The JSON output is a plain `serde_json::to_string_pretty()` of the slice.

### Decision: CLI flag is a `clap::ValueEnum`

Defined as:

```rust
#[derive(ValueEnum, Clone)]
enum OutputFormat {
    Table,
    List,
    Json,
}
```

This gives `--format table|list|json` for free with autocomplete and error messages.

## Risks / Trade-offs

- [Performance] `json` mode serializes the full result including `neighbors`, `chunk_text`. Large result sets could produce verbose JSON. Mitigation: `k` limits already cap results at `k*2` max.
- [Compatibility] Users scripting the current table output (e.g., `awk` parsing) will break if output changes. Mitigation: `table` mode is unchanged; `json` mode is explicitly opt-in.
- [Entities in list mode] Entities/tags without file paths are silently dropped. If users need them, they fall back to `table` or `json`.