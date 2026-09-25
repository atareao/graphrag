# graphrag

**Hybrid search engine: vector embeddings + knowledge graph over Markdown notes.**

GraphRAG scans a repository of Markdown notes, extracts entities via local AI (Ollama), builds a knowledge graph, and enables hybrid search (semantic vectors + graph expansion) to find related articles.

All in **a single Rust binary** — no Python, no npm, no servers. 100% local and private.

[![Crates.io](https://img.shields.io/crates/v/graphrag.svg)](https://crates.io/crates/graphrag)
[![MIT License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Quick start

```bash
# 1. Install
cargo install graphrag

# 2. Populate demo data and test
graphrag seed graph.db
graphrag search "Python" graph.db -k 5
graphrag path "Docker" "SQLite" graph.db
graphrag stats graph.db

# 3. Build graph from your notes (uses ~/.config/graphrag/config.toml)
graphrag build

# 4. Reset database (rm + init)
graphrag reset

# 5. Generate shell completions
graphrag completions bash > ~/.local/share/bash-completion/completions/graphrag

# 6. Generate Neovim plugin config
graphrag init neovim --output ~/.config/nvim/lua/plugins/graphrag.lua

# 7. Start MCP server for Claude Desktop
graphrag mcp --db graph.db
```

---

## Table of Contents

- [Installation](#installation)
- [Configuration](#configuration)
- [CLI commands](#cli-commands)
- [Neovim plugin](#neovim-plugin)
- [MCP server](#mcp-server)
- [Architecture](#architecture)
- [Performance](#performance)
- [Tests](#tests)
- [Debugging](#debugging)

---

## Installation

### From crates.io

```bash
cargo install graphrag
```

### From source

```bash
git clone https://github.com/atareao/graphrag
cd graphrag
cargo build --release

# Binary at:
#   target/release/graphrag (~6.5 MB)
```

**Dependencies:** Rust ≥ 1.75 only. SQLite is statically compiled (bundled).

**Optional:** [Ollama](https://ollama.ai/) with an embedding model (e.g. `bge-m3`) and an LLM for entity extraction (e.g. `gemma4:e2b`).

---

## Configuration

GraphRAG auto-creates a config file at `~/.config/graphrag/config.toml` on first run:

```toml
# Default database path
db = "graph.db"

# Ollama server URL
ollama_url = "http://localhost:11434"

# Embedding model (used when --ollama is passed)
embed_model = "bge-m3:latest"

# Model for entity extraction during build
ner_model = "gemma4:e2b"

# Default number of search results
k = 5

# Default graph expansion depth
depth = 2

# Vector weight (0.0 = pure graph, 1.0 = pure vector)
alpha = 0.7

# Notes directory (used by build and Neovim plugin)
notes_dir = ""

# Number of parallel threads for build (NER workers)
num_threads = 4
```

All fields are optional. CLI flags override config values.

---

## CLI commands

```bash
graphrag <COMMAND> [ARGS]
```

### `graphrag init db [DB]`

Create an empty SQLite database with the GraphRAG schema.

```bash
graphrag init db graph.db
```

### `graphrag init neovim [--output <path>]`

Generate Neovim plugin Lua config ready for lazy.nvim.

```bash
# Print to stdout
graphrag init neovim

# Save to file
graphrag init neovim --output ~/.config/nvim/lua/plugins/graphrag.lua
```

### `graphrag completions <bash|zsh|fish>`

Generate shell completion scripts.

```bash
graphrag completions bash > completions.bash
graphrag completions zsh > _graphrag
graphrag completions fish > graphrag.fish
```

### `graphrag seed [DB]`

Populate the database with demo data (44 nodes, ~113 edges).

```bash
graphrag seed graph.db
graphrag search "Docker security" graph.db -k 5
```

### `graphrag build [REPO] [DB]`

Scan a directory of `.md` files, extract entities via Ollama NER, and build the knowledge graph.

**Optimized processing:**
- **NER batch:** all chunks of a file are sent in a single Ollama call
- **N worker threads** for parallel NER + **1 writer thread** (serialized SQLite, no lock contention)
- **Retry with backoff:** 3 attempts (2s, 4s, 8s) on connection errors
- **Incremental:** only processes new/modified files (SHA256 hash)
- **Auto-cleanup:** deleted notes are pruned from the graph

```bash
# Build from configured notes directory
graphrag build

# Build from a specific directory
graphrag build ~/notes graph.db

# Build with specific model
graphrag build ~/notes graph.db --ner-model gemma4:e2b
```

### `graphrag reset [DB]`

Delete the database and recreate it empty (rm + init).

```bash
graphrag reset
graphrag reset /data/notes/graph.db
```

### `graphrag search <QUERY> [DB]`

Hybrid search: semantic vectors + graph expansion.

```bash
graphrag search "Python and databases" graph.db -k 10 -d 2
```

| Flag | Default | Description |
|------|---------|-------------|
| `-k` | 5 | Number of results |
| `-d` | 2 | Graph expansion depth |
| `-a` | 0.7 | Vector weight (0.0 = pure graph, 1.0 = pure vectors) |
| `--vector-only` | — | Vector-only search, skip graph |
| `--notes-only` | — | Show only notes (no entities/tags) |
| `--min-weight` | — | Minimum edge weight for expansion |
| `--ollama` | — | Use Ollama for embeddings (default: synthetic) |

### `graphrag similar --label <LABEL> --file <PATH> [DB]`

Find semantically similar notes. Two modes:
- `--label`: find notes similar to an **existing** indexed note (no Ollama needed — uses stored embeddings)
- `--file`: find notes similar to an **external** `.md` file (chunks and embeds via Ollama)

Both modes use **match individual**: each chunk of the source is compared independently, and each target note is scored by its best-matching chunk pair.

```bash
# Find notes similar to an existing note
$ graphrag similar --label "Python frameworks" graph.db -k 5

# Find notes similar to an external file
$ graphrag similar --file ~/drafts/new-idea.md graph.db -k 10 -d 2
```

| Flag | Default | Description |
|------|---------|-------------|
| `--label` | — | Label of an existing note in the database |
| `--file` | — | Path to an external `.md` file |
| `-k` | 5 | Number of results |
| `-d` | 2 | Graph expansion depth |
| `--notes-only` | — | Show only notes (no entities/tags) |
| `--min-weight` | — | Minimum edge weight for expansion |
| `--filter` | — | Filter by metadata field (`--filter "category = tutorial"`) |

Note: `--label` and `--file` are mutually exclusive. Use exactly one.

### `graphrag fts <QUERY> [DB]`

Exact text search with FTS5.

```bash
graphrag fts "Python" graph.db -l 20
```

### `graphrag graph <LABEL> [DB]`

Show neighbors of a node in the graph.

```bash
graphrag graph "Docker" graph.db -d 2
```

### `graphrag path <FROM> <TO> [DB]`

Shortest path between two nodes.

```bash
graphrag path "Python" "Docker" graph.db
```

### `graphrag stats [DB]`

Graph statistics.

```bash
graphrag stats graph.db
```

### `graphrag mcp`

Start an MCP server over stdio (see MCP section).

---

## Neovim plugin

The `graphrag.nvim` plugin lets you search related articles from Neovim and open results or insert them as links at the end of the article.

### Installation

```bash
graphrag init neovim --output ~/.config/nvim/lua/plugins/graphrag.lua
```

lazy.nvim loads it automatically from `lua/plugins/`.

### Commands

| Command | Description |
|---------|-------------|
| `:GraphRAG related` | Find notes related to current buffer. Floating window. |
| `:GraphRAG insert` | Find related and insert `## Related` links at end. |
| `:GraphRAG search <q>` | Hybrid search |
| `:GraphRAG fts <q>` | Text search |
| `:GraphRAG path <A> <B>` | Shortest path |
| `:GraphRAG stats` | Statistics |

### Keymaps

| Key | Command |
|-----|---------|
| `,gr` | `:GraphRAG related` |
| `,gi` | `:GraphRAG insert` |

---

## MCP server

### Claude Desktop configuration

```json
{
  "mcpServers": {
    "graphrag": {
      "command": "/full/path/graphrag",
      "args": ["mcp", "--db", "/full/path/graph.db"]
    }
  }
}
```

### MCP tools

| Tool | Description |
|------|-------------|
| `build` | Build/update the graph |
| `search` | Hybrid search |
| `fts` | Text search |
| `graph` | Node neighbors |
| `path` | Shortest path |
| `stats` | Statistics |
| `seed` | Demo data |

### MCP resources

| URI | Content |
|-----|---------|
| `graphrag://stats` | Statistics |
| `graphrag://nodes` | All nodes |
| `graphrag://notes` | Only notes |
| `graphrag://entities` | Only entities |
| `graphrag://edges` | Edges |

---

## Architecture

```
📦 graphrag
├── src/
│   ├── main.rs           ← CLI (clap, 11 subcommands)
│   ├── config.rs         ← TOML config (auto-created)
│   ├── mcp/mod.rs        ← MCP server (JSON-RPC stdio)
│   ├── db/schema.rs      ← SQLite schema + FTS5
│   ├── graph/
│   │   ├── build.rs      ← Incremental + parallel build
│   │   └── expand.rs     ← Recursive CTE, shortest path
│   ├── search/hybrid.rs  ← HybridSearch 3 phases
│   ├── embed/ollama.rs   ← Ollama HTTP client
│   ├── vector/
│   │   ├── mod.rs        ← Blob <-> Vec<f32>
│   │   └── synthetic.rs  ← Deterministic hash-based embeddings
│   ├── chunking/markdown.rs ← Frontmatter parsing + section chunking
│   ├── ner/ollama_ner.rs ← Entity extraction (batch mode)
│   └── seed/demo_data.rs ← Demo data generator
└── target/release/graphrag  ← Single binary (~6.5 MB)
```

### Build pipeline

```
.md files → Pre-scan (SHA256 hash) → Filter unchanged
          → N worker threads: parse frontmatter → chunk → NER batch (Ollama)
          → 1 writer thread: insert nodes/edges in SQLite (serialized)
          → Prune deleted notes → Repopulate FTS5
```

### Search pipeline

```
Query → PHASE 1: Cosine similarity over embeddings
      → PHASE 2: Rerank by title/type
      → PHASE 3: Recursive CTE graph expansion
      → Results with score, content, path, and neighbors
```

---

## Performance

### Recommended models

| Purpose | Model | Params | VRAM | Quality |
|---------|-------|--------|------|---------|
| NER | `gemma4:e2b` | 5.1B | ~5GB | Very good |
| NER | `gemma4:12b` | 11.9B | ~7GB | Excellent |
| NER | `llama3.2:3b` | 3.2B | ~2GB | Good (fast) |
| Embeddings | `bge-m3` | 566M | ~1GB | Excellent |
| Embeddings | `nomic-embed-text` | 137M | ~0.5GB | Good |

### Throughput

With `gemma4:e2b` and 4 threads: ~0.08 files/s → ~22h for 6254 files (first build only, subsequent builds are incremental).

### Configuration

```toml
# ~/.config/graphrag/config.toml
ner_model = "gemma4:e2b"
num_threads = 4
```

---

## Tests

```bash
cargo test                    # 32 tests, 2 ignored (need Ollama)
cargo test -- --ignored       # Ollama-dependent tests
```

Tests use `tempfile` for temporary DBs. No external services needed for the 30 non-ignored tests.

---

## Debugging

```bash
# Detailed logs
RUST_LOG=debug graphrag build

# Filter by module
RUST_LOG=graphrag::graph::build=debug graphrag build
RUST_LOG=graphrag::ner=debug graphrag build

# Info only (default)
graphrag build
```

---

## License

MIT