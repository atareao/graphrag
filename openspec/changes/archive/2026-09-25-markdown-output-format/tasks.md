# Tasks

## 1. Add `OutputFormat` enum and `--format` flag to command structs

- [x] 1.1 Add `#[derive(ValueEnum, Clone)] enum OutputFormat { Table, List, Json }` with `Default` impl for `Table` in `src/main.rs` and verify `cargo check` passes
- [x] 1.2 Add `--format` arg (`#[arg(long, default_value = "table")]`) to `Similar` command struct and verify `cargo check` passes
- [x] 1.3 Add `--format` arg to `Search` command struct and verify `cargo check` passes
- [x] 1.4 Add `--format` arg to `Fts` command struct and verify `cargo check` passes

## 2. Implement shared `display_results()` in search/hybrid.rs

- [x] 2.1 Write a `display_results(results: &[SearchResult], format: OutputFormat)` function with three match arms (Table, List, Json) and verify it compiles
- [x] 2.2 Implement `table` mode: identical to current `display_similar_results()` logic — score aligned, type bracketed, label, file, neighbors count, chunk preview
- [x] 2.3 Implement `list` mode: `"- [{label}]({file})"` for notes, skip entities/tags, no scores or neighbors shown; verify via `cargo test` that existing display tests pass
- [x] 2.4 Implement `json` mode: `serde_json::to_string_pretty(&results)`; verify via `cargo test`

## 3. Wire `--format` through cmd_search, cmd_similar, cmd_fts

- [x] 3.1 Pass `format` from `Similar` command args to `cmd_similar()`, then to `display_results()` instead of the old `display_similar_results()`; verify `cargo test` passes
- [x] 3.2 Pass `format` from `Search` command args to `cmd_search()`, replace inline display with `display_results()`; verify `cargo test` passes
- [x] 3.3 Pass `format` from `Fts` command args to `cmd_fts()`, replace inline display with `display_results()`; verify `cargo test` passes
- [x] 3.4 Verify full pipeline: `cargo build --release` compiles cleanly with zero warnings

## 4. Manual smoke-test all three formats on real DB

- [x] 4.1 Run `graphrag search "markdown" -k 3 --format table` and verify output matches current table format
- [x] 4.2 Run `graphrag similar --label "Markdown" -k 5 --format list` and verify markdown bullet links output
- [x] 4.3 Run `graphrag fts "markdown" --limit 5 --format json` and verify valid JSON output
- [x] 4.4 Run `graphrag search "markdown" --format invalid` and verify clap error with valid values listed
- [x] 4.5 Run `cargo clippy -- -D warnings` to enforce zero warnings

## 5. Finalize

- [x] 5.1 Run `cargo test` and confirm all 30+ tests pass (green)
- [x] 5.2 Run `cargo fmt --check` to confirm formatting is clean