# Tasks

## 1. Dependencies and module scaffolding

- [x] 1.1 Add `ratatui` and `crossterm` dependencies to `Cargo.toml` and verify `cargo check` passes
- [x] 1.2 Create `src/map/mod.rs` with module declaration and verify `cargo check` passes
- [x] 1.3 Create `src/map/layout.rs` with module declaration and verify `cargo check` passes
- [x] 1.4 Create `src/map/tui.rs` with module declaration and verify `cargo check` passes
- [x] 1.5 Register `mod map;` in `src/main.rs` and verify `cargo check` passes

## 2. Data loading (`src/map/mod.rs`)

- [x] 2.1 Define `MapNode` struct with id, label, type_ fields and verify compilation
- [x] 2.2 Define `MapEdge` struct with source_id, target_id, type_, weight, context fields and verify compilation
- [x] 2.3 Define `GraphData` struct with nodes and edges vectors and verify compilation
- [x] 2.4 Implement `load_graph(db, from, depth)` that loads all nodes (excluding root/system) and verify with a test on seed data
- [x] 2.5 Implement `load_graph` with `--from` support: CTE recursive expansion from starting node and verify with a test
- [x] 2.6 Implement `load_graph` edge loading: load edges whose both endpoints are in the loaded nodes and verify with a test
- [x] 2.7 Implement `cmd_map(db, from, depth)` that loads data and launches TUI, verify `cargo check` passes

## 3. Force-directed layout (`src/map/layout.rs`)

- [x] 3.1 Define `Position` struct with x, y fields and verify compilation
- [x] 3.2 Define `Layout` struct with positions, width, height fields and verify compilation
- [x] 3.3 Implement `force_directed_layout()` with random initialization and verify it returns correct number of positions
- [x] 3.4 Implement repulsion force between all node pairs and verify positions change after iteration
- [x] 3.5 Implement attraction force along edges and verify connected nodes move closer
- [x] 3.6 Implement cooling factor and convergence and verify layout stabilizes after enough iterations
- [x] 3.7 Implement normalization to canvas boundaries and verify all positions are within bounds
- [x] 3.8 Implement collision detection/avoidance and verify no two nodes overlap

## 4. TUI core (`src/map/tui.rs`)

- [x] 4.1 Define `AppState` struct with all state fields and verify compilation
- [x] 4.2 Implement `run_tui()` with terminal setup (enter alternate screen, raw mode) and verify it opens and closes cleanly
- [x] 4.3 Implement main event loop with `q`/Esc to quit and verify exit works
- [x] 4.4 Implement Canvas rendering of nodes as labeled boxes and verify nodes are visible
- [x] 4.5 Implement Canvas rendering of edges as directed lines with arrowheads and verify edges are visible
- [x] 4.6 Implement edge label rendering when space permits and verify labels appear
- [x] 4.7 Implement node coloring by type (note=blue, entity=green, tag=yellow) and verify colors are correct
- [x] 4.8 Implement empty graph message and verify it displays when no nodes

## 5. Node selection and interaction

- [x] 5.1 Implement arrow key navigation (nearest node in direction) and verify selection moves correctly
- [x] 5.2 Implement vim key navigation (h/j/k/l) and verify it matches arrow keys
- [x] 5.3 Implement Enter to select node and verify details panel updates
- [x] 5.4 Implement mouse click support for node selection and verify clicking a node selects it
- [x] 5.5 Implement selection highlight (different color/border on selected node) and verify it's visible

## 6. Details panel

- [x] 6.1 Implement details panel layout (right side or bottom) and verify it renders
- [x] 6.2 Show node label, type, and neighbor count in details panel and verify correctness
- [x] 6.3 Show neighbor list with edge type and weight in details panel and verify correctness
- [x] 6.4 Show metadata content (truncated) in details panel and verify it displays

## 7. Filter by type

- [x] 7.1 Implement `t` key to cycle filter: all → notes → entities → tags → all and verify cycling works
- [x] 7.2 Implement filter hiding of non-matching nodes from canvas and verify nodes disappear
- [x] 7.3 Implement filter hiding of edges with filtered endpoints and verify edges disappear
- [x] 7.4 Show active filter in status bar and verify it updates on each cycle

## 8. Search

- [x] 8.1 Implement `/` key to open search input and verify input field appears
- [x] 8.2 Implement search query input with character typing and verify text entry works
- [x] 8.3 Implement Enter to execute search and highlight matching nodes and verify highlights appear
- [x] 8.4 Implement Escape to cancel search and verify input clears and mode exits
- [x] 8.5 Implement case-insensitive search matching and verify "python" matches "Python"

## 9. Status bar

- [x] 9.1 Implement status bar showing total visible node count and verify count is correct
- [x] 9.2 Implement status bar showing keybinding hints and verify hints are readable
- [x] 9.3 Implement status bar showing selected node info and verify it updates on selection change

## 10. CLI integration (`src/main.rs`)

- [x] 10.1 Add `Map` variant to `Commands` enum with `--from`, `--depth`, `--db` flags and verify `graphrag map --help` works
- [x] 10.2 Implement `cmd_map` dispatch in main match block with sentinel DB logic and verify `cargo check` passes
- [x] 10.3 Wire `cmd_map` to `map::cmd_map()` and verify end-to-end: `graphrag map seed.db` opens TUI

## 11. Testing

- [x] 11.1 Write unit tests for `load_graph` with seed data and verify `cargo test` passes
- [x] 11.2 Write unit tests for `force_directed_layout` and verify `cargo test` passes
- [x] 11.3 Write unit tests for `AppState` filter/search logic and verify `cargo test` passes
- [x] 11.4 Run `cargo clippy -- -D warnings` and verify zero warnings
- [x] 11.5 Run `cargo test` and verify all tests pass (including existing)