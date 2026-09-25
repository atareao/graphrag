# Tasks

## 1. Schema — New `chunks` table

- [ ] 1.1 Add `chunks` table to `src/db/schema.rs`: `(id INTEGER PK, note_id INTEGER FK→nodes.id, header TEXT, text TEXT, slug TEXT, embedding BLOB, metadata TEXT)` with index on `note_id` and verify `cargo test` passes
- [ ] 1.2 Add helper functions: `insert_chunk()`, `get_chunks_by_note()`, `load_all_chunk_embeddings()` in `src/db/` and verify with unit tests using temp DB

## 2. Build — Chunk embedding via Ollama

- [x] 2.1 Modify `build_graph()` to accept `OllamaClient` (remove `_embed_model` parameter, pass client directly) and verify `cargo check` passes
- [x] 2.2 In the writer thread, after inserting note node and entities, embed each chunk's text (title + header + body) via Ollama batch `/api/embed` and store in `chunks` table; verify with integration test using mock Ollama
- [x] 2.3 Remove all `synthetic::synthetic_embedding()` calls from `build.rs` (lines 302, 355, 402, 422) and verify `cargo test` passes with no synthetic references

## 3. Search — Query against chunk vectors

- [ ] 3.1 Add `load_all_chunk_embeddings()` method to `HybridSearch` that loads `(id, note_id, embedding, text, header)` from `chunks` table and verify via unit test
- [ ] 3.2 Modify `hybrid_search()` to compute query embedding via Ollama and find top-k chunks by cosine similarity against `chunks.embedding`, then group by `note_id` and verify with integration test
- [ ] 3.3 Display matched chunk `header` and `text` preview in search output alongside note title and verify via CLI smoke test
- [ ] 3.4 Ensure graph expansion (depth parameter) operates on parent `note_id` of matched chunks and verify with integration test

## 4. Metadata filters — `--filter` flag

- [x] 4.1 Add `--filter` CLI argument (repeatable) to `search` subcommand in `src/main.rs` and verify `graphrag search --help` shows it
- [x] 4.2 Implement filter parsing: split `field operator value`, validate operator ∈ `{=, !=, >=, <=, >, <}` and verify with unit tests
- [x] 4.3 Apply filters via SQL `WHERE json_extract(metadata, '$.{field}') {op} {value}` against parent note's metadata and verify with integration test using temp DB with known frontmatter
- [x] 4.4 Combine multiple `--filter` flags as AND conditions and verify with test that requires all conditions

## 5. Remove `--ollama` flag and synthetic code

- [x] 5.1 Remove `--ollama` flag from `search` subcommand in `src/main.rs`; always create `OllamaClient` and verify `cargo check` passes
- [x] 5.2 Remove `--ollama` flag from `build` subcommand in `src/main.rs`; always verify Ollama health check and verify `cargo check` passes
- [x] 5.3 Delete `src/vector/synthetic.rs` and remove `pub mod synthetic;` from `src/vector/mod.rs` and verify `cargo test` passes
- [x] 5.4 Remove `use_synthetic` field from `HybridSearch` struct and all related code paths; verify `cargo clippy -- -D warnings` passes
- [x] 5.5 Remove `--vector-only` flag from `search` (equivalent to `--depth 0`) and refactor `vector_only()` into `hybrid_search()` with depth=0; verify `cargo test` passes

## 6. Error handling & edge cases

- [ ] 6.1 Handle Ollama unavailable during `build`: fail with clear error before processing files and verify with test that mocks connection failure
- [ ] 6.2 Handle Ollama unavailable during `search`: exit with non-zero status and error message and verify with test
- [ ] 6.3 Handle empty `chunks` table in `search`: return empty results with message and verify with test
- [ ] 6.4 Handle chunks <50 chars: discard before embedding and verify with test using tiny markdown sections
- [ ] 6.5 Handle invalid `--filter` syntax: exit with usage error and verify with test

## 7. Cleanup & verification

- [ ] 7.1 Run `cargo test` full suite (30+ tests) and confirm all pass
- [ ] 7.2 Run `cargo clippy -- -D warnings` and confirm zero warnings
- [ ] 7.3 Run `cargo fmt --check` and confirm formatting is correct
- [ ] 7.4 Run full end-to-end smoke test: `init db` → `build` with real notes → `search` with query → verify results include chunk content, not just titles