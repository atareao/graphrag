# GraphRAG — AGENTS.md

Single Rust binary (`graphrag`). Hybrid search engine: vector embeddings + knowledge graph over Markdown notes. No Python, no npm, no servers.

## Quick start

```bash
cargo build --release
./target/release/graphrag seed graph.db
./target/release/graphrag search "Python" graph.db -k 5
```

## Commands

| Command | Purpose |
|---------|---------|
| `init db <db>` | Create empty DB with schema |
| `init neovim` | Generate Neovim plugin Lua config |
| `seed <db>` | Populate demo data (44 nodes, ~113 edges) |
| `build <repo> <db>` | Scan `.md` dir, extract entities via Ollama NER, build graph. Default repo: `.` |
| `search <query> <db>` | Hybrid search (vectors + graph expansion) |
| `fts <query> <db>` | FTS5 exact-text search |
| `graph <label> <db>` | Show neighbors of a node |
| `path <from> <to> <db>` | Shortest path between two nodes |
| `stats <db>` | Graph statistics |
| `reset <db>` | Delete DB and recreate empty (rm + init) |
| `mcp --db <db>` | MCP server over stdio (JSON-RPC 2.0) |
| `completions <shell>` | Generate shell completions (bash/zsh/fish) |

## Key flags

| Flag | Applies to | Default | Notes |
|------|-----------|---------|-------|
| `-k` | search | 5 | Number of results |
| `-d` | search, graph | 2 | Graph expansion depth |
| `-a` | search | 0.7 | Vector weight (0.0=pure graph, 1.0=pure vector) |
| `--vector-only` | search | false | Skip graph expansion entirely |
| `--min-weight` | search | none | Filter edges by minimum weight |
| `--ollama` | search | false | Use Ollama for embeddings (default: synthetic) |
| `--ollama-url` | build, search, mcp | `http://localhost:11434` | |
| `--ner-model` | build | `llama3.2:3b` | Model for entity extraction |
| `--embed-model` | build, search, mcp | `nomic-embed-text` | Model for embeddings |
| `-C` | global | none | Override config file path |
| `num_threads` | config | 4 | Parallel NER workers for build |

## Architecture

```
src/
├── main.rs              ← CLI entrypoint (clap, 11 subcommands)
├── config.rs            ← TOML config (XDG: ~/.config/graphrag/config.toml, auto-created)
├── db/schema.rs         ← SQLite schema + FTS5 (no triggers)
├── graph/
│   ├── build.rs         ← Incremental build (SHA256 hash-based, parallel NER + serial writer)
│   └── expand.rs        ← CTE recursive expansion + shortest path
├── search/hybrid.rs     ← HybridSearch: 3-phase (vector → rerank → graph)
├── embed/ollama.rs      ← Ollama HTTP client (blocking reqwest)
├── vector/
│   ├── mod.rs           ← f32 <-> BLOB, cosine similarity
│   └── synthetic.rs     ← Deterministic hash-based embeddings (no Ollama needed)
├── chunking/markdown.rs ← Frontmatter parsing, section chunking
├── ner/ollama_ner.rs    ← LLM-based entity extraction (batch mode, JSON format, temp=0.1)
├── mcp/mod.rs           ← MCP server (stdio, JSON-RPC 2.0)
└── seed/demo_data.rs    ← Demo data generator
```

## Quirks & gotchas

- **Config file is auto-created** on first run at `$XDG_CONFIG_HOME/graphrag/config.toml` with documented defaults. All fields optional. CLI flags override config.
- **`graphrag.db` default is a sentinel.** If the CLI default `graphrag.db` is used, it falls back to the config file value. This means `graphrag search "foo"` uses the config DB, not a literal `graphrag.db` file.
- **FTS5 is repopulated from scratch** after every `build` (no triggers). This avoids a SQLite 3.x bug where FTS5 `'delete'` fails with empty content.
- **Build is incremental.** Each note stores a SHA256 hash in metadata. Only new/modified files are reprocessed. Deleted files are pruned automatically.
- **Build uses `unchecked_transaction`** per file (not nested). Each file gets its own transaction.
- **Build is parallel.** N worker threads do NER in parallel, sending results through a channel to a single writer thread (avoids SQLite lock contention). Number of threads configurable via `num_threads` in config.
- **NER batch mode.** All chunks of a file are sent in a single Ollama call with `---SECTION N---` markers. The model returns entities with a `section` field.
- **NER retry with backoff.** On connection error, retries 3 times (2s, 4s, 8s). If all fail, the file is skipped entirely (not inserted without entities).
- **NER requires Ollama.** If Ollama is unreachable or the model is not found, `build` warns per-chunk and continues with empty entities. The `search` command defaults to synthetic embeddings unless `--ollama` is passed.
- **NER model must support plain-text JSON output.** Some models (e.g. some Qwen variants) don't support Ollama's `format: json` mode and return empty responses. The prompt already asks for JSON explicitly — if a model returns empty, try a different model like `llama3.2:3b`.
- **"Thinking" models** (qwen3.5, deepseek-r1) put their response in the `thinking` field instead of `response`. The code falls back to `thinking` and uses regex to extract JSON arrays.
- **Synthetic embeddings** are deterministic (hash-based LCG + Box-Muller). Same text → same vector. No Ollama needed for basic search.
- **MCP server** runs over stdio (JSON-RPC 2.0). Protocol version `2024-11-05`. Exposes 7 tools + 5 resources.

## Testing

```bash
cargo test                    # 32 tests, 2 ignored (need Ollama)
cargo test -- --ignored       # Run Ollama-dependent tests
```

Tests use `tempfile` for temporary DBs. No external services needed for the 30 non-ignored tests.

## Build profile

Release profile in `Cargo.toml`: `lto=true`, `codegen-units=1`, `strip=true`, `opt-level=3`. Binary ~6.5 MB.

## Dependencies

- `rusqlite` with `bundled` feature (SQLite compiled statically, no system dep)
- `reqwest` with `blocking` client (no async runtime)
- `clap` derive for CLI
- `clap_complete` for shell completion generation
- `serde`/`serde_json` for serialization
- `bytemuck` for f32 <-> byte casting
- `indicatif` for progress bars (build command)
- `sha2` for incremental build hashing
- `walkdir` for directory traversal
- `toml` + `dirs` for config loading

## Git Flow

This project follows strict gitflow. See [GIT_FLOW.md](./GIT_FLOW.md) for:
- Branch structure (main, development, feature/*, hotfix/*)
- Conventional commits with gitmoji
- How to create features, hotfixes, and releases
- CI/CD workflows for automated versioning and publishing