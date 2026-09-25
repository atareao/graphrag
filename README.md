# graphrag

<div align="center">

**Hybrid search engine: vector embeddings + knowledge graph over Markdown notes.**

[![Crates.io](https://img.shields.io/crates/v/graphrag.svg)](https://crates.io/crates/graphrag)
[![Crates.io Downloads](https://img.shields.io/crates/d/graphrag)](https://crates.io/crates/graphrag)
[![MIT License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange)](https://www.rust-lang.org)
[![Built with](https://img.shields.io/badge/built%20with-Rust-1f425f.svg)](https://www.rust-lang.org)

**A single Rust binary — no Python, no npm, no servers. 100% local and private.**

</div>

---

## 🧭 GraphRAG for absolute beginners

*(If you already know what RAG, vectors, and knowledge graphs are, skip ahead to [What is Graph RAG?](#-what-is-graph-rag))*

### The problem: hundreds of notes, can't find anything

Imagine you've been taking notes about Linux for years. You have a `~/linux-notes/`
folder with **500 Markdown files** on topics like:

```
linux-notes/
├── apt-comandos-basicos.md
├── systemd-crear-servicio.md
├── docker-compose-nginx.md
├── particionar-disco-gpt.md
├── script-bash-backup.md
├── firewall-ufw-reglas.md
├── kernel-compilar-modulo.md
├── ssh-tunel-inverso.md
├── lvm-redimensionar.md
├── nginx-proxy-inverso.md
├── rsync-incremental-backup.md
├── ... (500 files)
```

When you need to find something, you do the usual:

```bash
grep -ri "partitions" ~/linux-notes/
```

And it works... until it doesn't. Because:

- You search **"partitions"** but the note is titled **"resizing LVM"** and never uses that word
- You search **"network monitoring"** but your note talks about **"ntopng"** and **"iftop"** without saying "monitoring"
- You search **"high availability"** and get one note, but you miss the **"keepalived"** and **"nginx load balancing"** notes that are closely related

**Your knowledge is there. The problem is you don't know it's there.**

---

### The solution: a search engine that understands *relationships*, not just words

GraphRAG scans your 500 Linux notes, automatically extracts key concepts
(apt, systemd, LVM, Nginx, Docker, firewalld...) and builds a **knowledge map**
that knows how they relate to each other.

Now, when you search:

```bash
graphrag search "hard disk partitions" linux.db
```

GraphRAG doesn't just find the note mentioning "partitions". It also shows you:
- The note about **LVM** (because LVM and partitions are neighboring concepts in the graph)
- The note about **GPT vs MBR** (connected through `partitioning`)
- The note about **mounting filesystems** (connected by `disk` → `storage`)

**This is Graph RAG**: a search engine that uncovers connections you didn't even know existed.

---

### Real-world example: how it works with your Linux notes

#### 1. Build the knowledge map

```bash
# One command scans all your notes and builds the graph
graphrag build ~/linux-notes linux.db
```

Under the hood, GraphRAG:
1. Reads each note and splits it into sections
2. Sends sections to a local AI (Ollama) that extracts entities like `systemd`,
   `docker`, `nginx`, `partitioning`, `firewall`
3. Detects which entities appear together in the same notes (co-occurrence)
4. Builds a graph where each entity is a node and each co-occurrence is a connection
5. Computes embeddings (vectors) for each note and entity

The result is a SQLite database that knows:
```
[Docker] ──co-occurs── [Nginx] ──co-occurs── [reverse-proxy]
                                              ──co-occurs── [certbot-ssl]
                                              ──co-occurs── [load-balancing]
```

#### 2. Search for something specific

```bash
graphrag search "secure web server" linux.db -k 5
```

Results:
```
 1. Nginx as reverse proxy                  Score: 0.92
    Connections: nginx, reverse-proxy, http

 2. Certbot: SSL certificates with Let's Encrypt  Score: 0.67
    Connections: ssl, certbot, nginx
    ↑ Appears because it shares "nginx" with the previous note

 3. Fail2ban: protecting SSH and services    Score: 0.45
    Connections: fail2ban, firewall, security
    ↑ Appears because "security" is connected to "nginx" in the graph
```

Note 3 never mentions "secure web server" — but GraphRAG knows that
fail2ban is used to protect web servers, because both notes share
entities in the graph. **grep would never have shown you that note.**

#### 3. Explore connections

```bash
# What does Docker have to do with systemd?
graphrag path "Docker" "systemd" linux.db
# → Docker → containers → systemd → services

# What entities surround "firewall"?
graphrag graph "firewall" linux.db -d 1
# Neighbors: ufw, iptables, nftables, fail2ban, security, ports

# General statistics
graphrag stats linux.db
# → 500 notes, 1200 entities, 3400 connections
```

---

### And all this without internet?

Yes. GraphRAG is a **single Rust binary** that:
- Doesn't need Python, Node.js, or Docker
- Runs entirely on your machine, 100% local
- Never sends your data to any server
- If you don't have Ollama, uses synthetic embeddings (no external AI needed)
- The SQLite database is smaller than an MP3 and fully portable

**In summary:** If you have hundreds of Markdown notes and you're tired of searching
with grep or missing notes you know exist but can't find,
GraphRAG is the search engine that turns your file chaos into an
explorable knowledge map.

---

## 📖 What is Graph RAG?

**Graph RAG** (Retrieval-Augmented Generation with Graphs) is a retrieval technique that improves on plain vector search by incorporating **graph structure** into relevance ranking.

Instead of just finding notes that *mention* your query, it discovers notes connected through **shared concepts and entities** — even if those notes never use the same words.

### Why it matters

| Search method | What it finds | What it misses |
|:---|---|:---|
| **FTS** (full-text) | Exact keyword matches | Synonyms, typos, related concepts |
| **Vector search** | Semantically similar content | Indirect relationships |
| **Graph RAG** | Semantic matches **+** graph-connected content | Nothing — it's strictly better |

**Concrete example:** You have notes about "Docker Compose", "PostgreSQL", and "Patroni HA clusters". Vector search for "PostgreSQL replication" finds the PostgreSQL note. Graph RAG also surfaces the Patroni note — not because it mentions PostgreSQL, but because both connect through the entity `high-availability` in the knowledge graph.

### The three-phase pipeline

```
Query "Python databases"
    │
    ▼
┌──────────────────────────────────────────────────────────┐
│ PHASE 1 — Vector Search                                  │
│ Cosine similarity: query vs every note/entity embedding  │
│ → Top-K candidates ranked by semantic proximity          │
├──────────────────────────────────────────────────────────┤
│ PHASE 2 — Reranking                                      │
│ Boost: title match (+0.1) • note type (+0.2)             │
│ Combine vector score + graph score via α (default 0.7)   │
├──────────────────────────────────────────────────────────┤
│ PHASE 3 — Graph Expansion                                │
│ From top candidates, traverse the knowledge graph         │
│ via recursive CTE (SQLite). Neighbors get decaying score  │
│ → Discovers notes connected through shared entities      │
├──────────────────────────────────────────────────────────┤
│ Results: hybrid score • note content • neighbors         │
└──────────────────────────────────────────────────────────┘
```

> **TL;DR:** Vector search tells you what's similar. Graph expansion tells you what's *connected*. Together they find what you didn't know to look for.

### When to use it

- 🧠 **Personal knowledge bases** — connect ideas across hundreds of notes
- 📘 **Technical documentation** — find related tools, libraries, and concepts
- 🔬 **Research workflows** — discover papers and notes linked by shared entities
- 🌱 **Zettelkasten / digital gardens** — surface implicit links between atomic notes
- 👥 **Team wikis** — find documents connected through shared terminology

---

## ✨ Features

| | Feature | Why it matters |
|---|---|---|
| 🔍 | **Hybrid search** (vectors + graph) | Finds relevant notes *and* connected content you didn't explicitly search for |
| 🧩 | **Entity extraction** via Ollama LLM | Automatically builds the knowledge graph from your notes — no manual tagging |
| 🏘️ | **Community detection** (Leiden algorithm) | Groups related entities into clusters for higher-level understanding |
| 📝 | **Narrative answers** (`--answer`) | Get LLM-generated summaries grounded in community context, not just result lists |
| 📄 | **FTS5 full-text search** | Classic keyword search when you know exactly what you're looking for |
| 🔗 | **Path finding** | Shortest path between any two nodes — discover surprising connections |
| 📊 | **Graph statistics** | Understand your knowledge base structure at a glance |
| ⚡ | **Incremental builds** | Only processes new/modified files (SHA256). Second build is instant |
| 🔄 | **Parallel NER** | N worker threads for entity extraction, 1 serial writer — no SQLite lock contention |
| 🤖 | **MCP server** | Claude Desktop, Cline, and other AI assistants can query your graph in real-time |
| 📝 | **Neovim plugin** | Search and insert related notes without leaving your editor |
| 🚫 | **Zero external deps** | SQLite compiled statically. No Python, Node, Docker, or servers needed |
| 🔒 | **100% local & private** | Everything runs on your machine. No data ever leaves your computer |

---

## 🚀 Quick start (under 2 minutes)

```bash
# 1. Install
cargo install graphrag

# 2. Populate demo data (44 nodes, ~113 edges)
graphrag seed demo.db

# 3. Search — finds notes semantically related to "Python"
graphrag search "Python" demo.db -k 5

# 4. Path — discovers how two technologies connect
graphrag path "Docker" "SQLite" demo.db
# → Docker → virtualization → embedded-database → SQLite

# 5. Graph — see what's connected to a concept
graphrag graph "machine-learning" demo.db -d 2

# 6. Stats — understand your graph
graphrag stats demo.db
```

### Sample output

```
$ graphrag search "Python" demo.db -k 3

Results for "Python" (α=0.70, depth=2):
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 1. Python: A versatile language
    Score: 0.982 | Type: note
    Python is widely used in data science, web development, and automation.
    Neighbors: pandas, flask, machine-learning, data-science

 2. pandas: Data analysis library
    Score: 0.623 | Type: note
    pandas provides high-performance, easy-to-use data structures.
    Neighbors: Python, numpy, data-science, jupyter

 3. Data Science overview
    Score: 0.412 | Type: note
    An introduction to data science concepts and tooling.
    Neighbors: Python, R, machine-learning, statistics
```

### Full demo walkthrough

```bash
# Create a database with demo data
graphrag seed graph.db

# Hybrid search with graph expansion (depth 2)
graphrag search "Docker security" graph.db -k 5

# Search with narrative answer (requires community detect + summarize)
graphrag community detect graph.db
graphrag community summarize graph.db
graphrag search "Docker security" graph.db --answer

# Exact text search
graphrag fts "Python" graph.db -l 10

# Explore the graph
graphrag graph "Docker" graph.db -d 1
graphrag path "Python" "Docker" graph.db
graphrag stats graph.db

# Clean up when done
graphrag reset graph.db
```

---

## 📦 Installation

### From crates.io (recommended)

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

**Requirements:** Rust ≥ 1.75 only. SQLite is compiled statically via `rusqlite` bundled feature — no system libraries needed.

### Optional: Ollama setup

GraphRAG works **without Ollama** using synthetic (deterministic hash-based) embeddings.
For better results, install [Ollama](https://ollama.ai/) and pull the models you need:

```bash
# Minimal setup (fast, works on any hardware)
ollama pull llama3.2:3b    # NER — 3.2B params, ~2GB VRAM
ollama pull nomic-embed-text  # Embeddings — 137M params, ~0.5GB VRAM

# Recommended setup (best quality)
ollama pull gemma4:e2b      # NER — 5.1B params, ~5GB VRAM
ollama pull bge-m3          # Embeddings — 566M params, ~1GB VRAM
```

---

## ⚙️ Configuration

GraphRAG auto-creates a config file at `~/.config/graphrag/config.toml` on first run.
**All fields are optional** — CLI flags always override config values.

```toml
# =============================================
# GraphRAG configuration
# =============================================

# --- Database ---
db = "graph.db"                    # Default DB path

# --- Ollama connection ---
ollama_url = "http://localhost:11434"
embed_model = "bge-m3:latest"      # Embedding model
ner_model = "gemma4:e2b"           # Entity extraction model

# --- Search defaults ---
k = 5                              # Results count
depth = 2                          # Graph expansion hops
alpha = 0.7                        # Vector weight (0=graph only, 1=vector only)

# --- Notes directory ---
notes_dir = "~/notes"              # Used by `graphrag build` when no dir is given

# --- Build ---
num_threads = 4                    # Parallel NER workers
```

### Practical config examples

<details>
<summary><b>📋 Minimal config (no Ollama, synthetic embeddings only)</b></summary>

```toml
db = "search.db"
notes_dir = "~/notes"
k = 10
depth = 2
# No Ollama fields — uses deterministic synthetic embeddings automatically
```

</details>

<details>
<summary><b>📋 Knowledge base with 1000+ notes</b></summary>

```toml
db = "~/kb/search.db"
notes_dir = "~/kb/notes"
ner_model = "gemma4:e2b"
embed_model = "bge-m3:latest"
k = 10
depth = 2
alpha = 0.7
num_threads = 6
```

</details>

---

## 🛠️ CLI commands

```
graphrag <COMMAND> [ARGS] [OPTIONS]
```

### Core commands

| Command | Description |
|---------|-------------|
| [`build`](#graphrag-build-repo-db) | 📥 Scan `.md` files → extract entities → build knowledge graph |
| [`search`](#graphrag-search-query-db) | 🔍 Hybrid vector + graph search |
| [`fts`](#graphrag-fts-query-db) | 📄 Exact full-text search (FTS5) |
| [`graph`](#graphrag-graph-label-db) | 🕸️ Show neighbors of a node |
| [`path`](#graphrag-path-from-to-db) | 🔗 Shortest path between two nodes |
| [`seed`](#graphrag-seed-db) | 🌱 Populate demo data |
| [`stats`](#graphrag-stats-db) | 📊 Graph statistics |
| [`reset`](#graphrag-reset-db) | 🗑️ Delete DB and recreate empty |

### Advanced commands

| Command | Description |
|---------|-------------|
| [`community detect`](#graphrag-community-detect-db) | 🏘️ Leiden community detection |
| [`community summarize`](#graphrag-community-summarize-db) | 📝 LLM summaries per community |
| [`mcp`](#graphrag-mcp) | 🤖 MCP server for AI assistants |
| [`init db`](#graphrag-init-db-db) | 🆕 Create empty database |
| [`init neovim`](#graphrag-init-neovim---output-path) | 📝 Generate Neovim plugin config |
| [`completions`](#graphrag-completions-bashzshfish) | ⌨️ Shell completions (bash/zsh/fish) |

---

### `graphrag init db [DB]`

Create an empty SQLite database with the GraphRAG schema.

```bash
graphrag init db graph.db
```

### `graphrag init neovim [--output <path>]`

Generate Neovim plugin Lua config ready for [lazy.nvim](https://github.com/folke/lazy.nvim).

```bash
# Print to stdout
graphrag init neovim

# Save directly
graphrag init neovim --output ~/.config/nvim/lua/plugins/graphrag.lua
```

---

### `graphrag community detect [DB]`

Run **Leiden community detection** on the entity graph — groups related entities into clusters.

```bash
graphrag community detect graph.db

# Coarser communities (lower resolution = fewer, larger groups)
graphrag community detect graph.db --resolution 0.8

# Finer communities (higher resolution = more, smaller groups)
graphrag community detect graph.db --resolution 1.5
```

| Flag | Default | Description |
|------|---------|-------------|
| `--resolution` | 1.0 | CPM quality function resolution parameter |

### `graphrag community summarize [DB]`

Generate LLM summaries for all detected communities. Requires `community detect` first.

```bash
graphrag community summarize graph.db --summary-model gemma4:e2b
```

| Flag | Default | Description |
|------|---------|-------------|
| `--ollama-url` | `http://localhost:11434` | Ollama server URL |
| `--summary-model` | `llama3.2:3b` | LLM for generating summaries |
| `--embed-model` | `nomic-embed-text` | Embedding model |

After summarizing, use `graphrag search --answer` to get **narrative answers** grounded in community context.

---

### `graphrag completions <bash|zsh|fish>`

Generate shell completion scripts.

```bash
graphrag completions bash > ~/.local/share/bash-completion/completions/graphrag
graphrag completions zsh > _graphrag
graphrag completions fish > graphrag.fish
```

---

### `graphrag seed [DB]`

Populate a database with demo data (44 nodes: notes, entities, tags; ~113 edges).

```bash
graphrag seed graph.db
graphrag search "Docker security" graph.db -k 5
```

| Flag | Default | Description |
|------|---------|-------------|
| `--ollama-url` | `http://localhost:11434` | Ollama server URL |
| `--embed-model` | `nomic-embed-text` | Embedding model |

---

### `graphrag build [REPO] [DB]`

The heart of GraphRAG — scan `.md` files, extract entities via Ollama, and build the graph.

**Optimized for large repos:**
| Optimization | How it works |
|:---|---|
| ⚡ **Incremental** | Skips unchanged files via SHA256 hash. Second builds are near-instant |
| 🧵 **Parallel NER** | N worker threads extract entities in parallel |
| ✍️ **Serial writer** | Single thread writes to SQLite — no lock contention |
| 📦 **Batch NER** | All chunks of a file sent in one Ollama call |
| 🔁 **Retry with backoff** | 3 attempts (2s, 4s, 8s) on connection errors |
| 🧹 **Auto-cleanup** | Deleted notes and orphaned entities pruned automatically |

```bash
# Build from configured notes directory
graphrag build

# Build from a specific directory
graphrag build ~/notes graph.db

# Build with specific model
graphrag build ~/notes graph.db --ner-model gemma4:e2b
```

| Flag | Default | Description |
|------|---------|-------------|
| `--embed-model` | `nomic-embed-text` | Embedding model |
| `--ner-model` | `llama3.2:3b` | Entity extraction model |
| `--ollama-url` | `http://localhost:11434` | Ollama server URL |

---

### `graphrag reset [DB]`

Delete the database and recreate it empty (equivalent to `rm + init`).

```bash
graphrag reset
graphrag reset /data/notes/graph.db
```

---

### `graphrag search <QUERY> [DB]`

Hybrid search — semantic vectors + graph expansion. This is what makes GraphRAG special.

```bash
graphrag search "Python and databases" graph.db -k 10 -d 2
```

| Flag | Default | Description |
|------|---------|-------------|
| `-k` | 5 | Number of results |
| `-d` | 2 | Graph expansion depth (hops from initial results) |
| `-a` | 0.7 | Vector weight (0.0 = pure graph, 1.0 = pure vector) |
| `--ollama-url` | `http://localhost:11434` | Ollama server URL |
| `--embed-model` | `nomic-embed-text` | Embedding model |
| `--notes-only` | — | Show only notes (hide entities and tags) |
| `--min-weight` | — | Minimum edge weight for graph expansion |
| `--filter` | — | Filter by metadata (repeatable). Format: `'field op value'` e.g. `--filter 'date >= 2023'` |
| `--answer` | — | Narrative answer using community context (requires `community detect + summarize`) |
| `--format` | `table` | Output format: `table`, `list`, or `json` |

---

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
| `--format` | `table` | Output format: `table`, `list`, or `json` |

Note: `--label` and `--file` are mutually exclusive. Use exactly one.

### `graphrag fts <QUERY> [DB]`

Exact full-text search via SQLite FTS5 — classic keyword search.

```bash
graphrag fts "Python" graph.db -l 20
```

| Flag | Default | Description |
|------|---------|-------------|
| `-l` | 10 | Number of results |
| `--notes-only` | — | Show only notes (no entities/tags) |
| `--format` | `table` | Output format: `table`, `list`, or `json` |

---

### `graphrag graph <LABEL> [DB]`

Show neighbors of a node in the knowledge graph. Great for exploring.

```bash
graphrag graph "Docker" graph.db -d 2
```

---

### `graphrag path <FROM> <TO> [DB]`

Shortest path between two nodes — discovers how concepts connect.

```bash
graphrag path "Python" "Docker" graph.db
# → Python → web-development → containerization → Docker
```

| Flag | Default | Description |
|------|---------|-------------|
| `-d` | 10 | Maximum traversal depth |

---

### `graphrag stats [DB]`

Graph statistics. Omits the DB path to use the configured default.

```bash
graphrag stats graph.db
graphrag stats            # uses config default
```

Output example:
```
Graph Statistics
═══════════════════════════════════════
 Nodes:    156
  • notes:      89
  • entities:   52
  • tags:       15
 Edges:    342
 Density:  0.028
 Communities: 12
```

---

### `graphrag mcp`

Start an MCP server over stdio for AI assistant integration (see [MCP section](#-mcp-server)).

```bash
graphrag mcp --db graph.db
graphrag mcp --db graph.db --ollama-url http://localhost:11434
```

| Flag | Default | Description |
|------|---------|-------------|
| `--db` | `graphrag.db` | Path to SQLite database |
| `--ollama-url` | `http://localhost:11434` | Ollama server URL |
| `--embed-model` | `nomic-embed-text` | Embedding model |

---

## 🎯 Use cases & examples

### 🔍 Personal knowledge base

You have 300 notes about programming, DevOps, and system design. You search for
"async Rust" and get back notes about tokio and async/await — *plus* a note
about the "actor model" that never mentions those words.

```bash
# Build the graph
graphrag build ~/zettelkasten kb.db

# Hybrid search — finds connected content you didn't explicitly search for
graphrag search "async Rust" kb.db -k 10 -d 2

# Example result you wouldn't get with vector-only search:
# "Actor Model in Distributed Systems" — Score: 0.34 (from graph expansion)
# Connected via: concurrency (shared entity with async Rust notes)
```

### 📚 Technical documentation

Search across multiple project docs and discover cross-project connections.

```bash
graphrag build ~/docs --ner-model gemma4:12b
graphrag search "PostgreSQL high availability" -k 10 -d 3
graphrag path "Kubernetes" "SQLite"
# → Kubernetes → container-orchestration → Docker → embedded-database → SQLite
```

### 🧪 Research paper collection

200 ML paper summaries. Find cross-disciplinary connections.

```bash
graphrag build ~/papers research.db

# Searching for "transformer attention" also surfaces a paper about
# "sparse mixture of experts" — both connect through "self-attention"
graphrag search "transformer attention mechanisms" research.db -k 15 -d 2

# Explore concepts
graphrag graph "self-attention" research.db -d 1
```

### 🔗 Hidden connections

Discover how seemingly unrelated topics connect through your graph.

```bash
# CSS to PostgreSQL? There's a path through web development
graphrag path "CSS" "PostgreSQL"
# → CSS → frontend → web-development → backend → PostgreSQL

# Explore an entity's neighborhood
graphrag graph "machine-learning" -d 2

# Get a narrative answer about a topic
graphrag community detect research.db
graphrag community summarize research.db --summary-model gemma4:e2b
graphrag search "attention mechanisms" research.db --answer
```

### 🤖 AI assistant integration (MCP)

Let Claude, Cline, or any MCP-compatible AI assistant query your knowledge base
in real-time during conversations.

```json
{
  "mcpServers": {
    "graphrag": {
      "command": "/usr/local/bin/graphrag",
      "args": ["mcp", "--db", "/home/user/kb.db"]
    }
  }
}
```

Claude can now use tools to explore your knowledge graph:
- `search` — hybrid semantic + graph search 🔍
- `graph` — explore neighbors of a node 🕸️
- `path` — find connections between concepts 🔗
- `stats` — understand graph structure 📊

---

## 📝 Neovim plugin

Search and insert related notes without leaving your editor.

### Installation

```bash
graphrag init neovim --output ~/.config/nvim/lua/plugins/graphrag.lua
```

[lazy.nvim](https://github.com/folke/lazy.nvim) loads it automatically from `lua/plugins/`.

### Commands & keymaps

| Command | Keymap | Description |
|---------|--------|-------------|
| `:GraphRAG related` | `,gr` | Find notes related to current buffer (floating window) |
| `:GraphRAG insert` | `,gi` | Find related and insert `## Related` links at end of file |
| `:GraphRAG search <q>` | — | Hybrid search from command line |
| `:GraphRAG fts <q>` | — | Full-text search |
| `:GraphRAG path <A> <B>` | — | Shortest path |
| `:GraphRAG stats` | — | Graph statistics |

---

## 🤖 MCP server

Expose your knowledge graph to any MCP-compatible AI assistant (Claude Desktop, Cline, etc.).

### Claude Desktop configuration

```json
{
  "mcpServers": {
    "graphrag": {
      "command": "/usr/local/bin/graphrag",
      "args": ["mcp", "--db", "/Users/me/knowledge/kb.db"]
    }
  }
}
```

### Tools exposed

| Tool | Description |
|------|-------------|
| `build` | Build or update the graph from your notes directory |
| `search` | Hybrid search (vectors + graph) |
| `fts` | Full-text search |
| `graph` | Show neighbors of a node |
| `path` | Shortest path between two nodes |
| `stats` | Graph statistics |
| `seed` | Populate demo data |

### Resources exposed

| URI | Content |
|-----|---------|
| `graphrag://stats` | Graph statistics |
| `graphrag://nodes` | All nodes |
| `graphrag://notes` | Notes only |
| `graphrag://entities` | Entities only |
| `graphrag://edges` | All edges |
| `graphrag://nodes/<label>` | Single node details |

---

## 🏗️ Architecture

### Module map

```
📦 graphrag  (single ~6.5 MB binary)
├── src/
│   ├── main.rs              ← CLI entrypoint (clap, 12 subcommands)
│   ├── config.rs            ← TOML config (auto-created, all fields optional)
│   ├── mcp/mod.rs           ← MCP server (stdio, JSON-RPC 2.0)
│   ├── db/
│   │   └── schema.rs        ← SQLite schema + FTS5 virtual table
│   ├── graph/
│   │   ├── build.rs         ← Incremental build (parallel NER, serial writer)
│   │   └── expand.rs        ← Recursive CTE expansion + shortest path
│   ├── search/
│   │   └── hybrid.rs        ← 3-phase hybrid search engine
│   ├── embed/
│   │   └── ollama.rs        ← Ollama HTTP client (blocking reqwest)
│   ├── vector/
│   │   ├── mod.rs           ← f32 ↔ BLOB, cosine similarity
│   │   └── synthetic.rs     ← Deterministic hash-based embeddings
│   ├── chunking/
│   │   └── markdown.rs      ← Frontmatter parsing + section chunking
│   ├── ner/
│   │   └── ollama_ner.rs    ← LLM entity extraction (batch mode)
│   └── seed/
│       └── demo_data.rs     ← Demo data generator (44 nodes, 113 edges)
```

### How data flows

```
BUILD PIPELINE:
┌──────────┐   ┌──────────┐   ┌──────────────────────┐   ┌──────────┐
│ .md files│ → │ SHA256   │ → │ N worker threads:     │ → │ 1 writer │
│          │   │ pre-scan │   │ frontmatter → chunk   │   │ thread   │
│          │   │          │   │ → NER batch (Ollama)  │   │ → SQLite │
└──────────┘   └──────────┘   └──────────────────────┘   └──────────┘
                                    ↓                         ↓
                              Entities + co-occurrence    Nodes + Edges
                                                                 ↓
                              ┌─────────────────────────────────┘
                              ↓
                         ┌──────────┐
                         │ Prune    │ → Repopulate FTS5
                         │ deleted  │
                         └──────────┘

SEARCH PIPELINE:
Query → Embed → Cosine similarity → Rerank → Graph expansion → Results
  │        │           │               │            │              │
  │    Ollama or   Top-K scores    +title      Recursive     Hybrid score
  │    synthetic   by semantic     +type       CTE from      + content
  │                 similarity     +alpha      candidates    + neighbors
```

### Database schema

```
┌──────────────────┐       ┌───────────────────────┐
│      nodes       │       │        edges          │
├──────────────────┤       ├───────────────────────┤
│ id        INT PK │──┐    │ source_id INT FK      │
│ label     TEXT   │  └────┤ target_id INT FK      │
│ type      TEXT   │  ┌────┤ type       TEXT        │
│ embedding BLOB  │  │    │ weight     REAL        │
│ metadata  JSON  │  │    │ context    TEXT        │
│ created_at INT  │  │    └───────────────────────┘
└──────────────────┘  │
                      │    ┌───────────────────────┐
      notes_fts       │    │      node_types       │
   (FTS5 virtual)     │    ├───────────────────────┤
                      │    │ note                  │
                      │    │ entity                │
                      │    │ tag                   │
                      └────┤                       │
                           └───────────────────────┘
```

---

## ⚡ Performance

### Recommended Ollama models

| Purpose | Model | Params | VRAM | Quality |
|:---|---:|:---:|:---:|:---:|
| NER | `gemma4:e2b` | 5.1B | ~5GB | ⭐⭐⭐ Very good |
| NER | `gemma4:12b` | 11.9B | ~7GB | ⭐⭐⭐ Excellent |
| NER | `llama3.2:3b` | 3.2B | ~2GB | ⭐⭐ Good (fast) |
| Embeddings | `bge-m3` | 566M | ~1GB | ⭐⭐⭐ Excellent |
| Embeddings | `nomic-embed-text` | 137M | ~0.5GB | ⭐⭐ Good |

### Build throughput

| Scenario | Files | Time | Notes |
|:---|---:|---:|:---|
| First build | 100 | ~20 min | With `gemma4:e2b`, 4 threads |
| First build | 6254 | ~22 h | Large docs repo |
| Incremental | 1 changed | ~2 s | Only re-processes modified files |
| No Ollama | any | ~5 s | Synthetic embeddings, no NER |

> **Tip:** The first build is the slowest (NER is LLM-bound). Subsequent builds are
> near-instant because only new/modified files are processed.

### Memory

| Setup | RAM | Notes |
|:---|---:|:---|
| No Ollama (synthetic) | ~15 MB | Minimal |
| With `nomic-embed-text` | ~500 MB | + Ollama process |
| With `bge-m3` | ~1.5 GB | + Ollama process |
| With `gemma4:e2b` (NER) | ~5 GB | + Ollama process |

---

## 🧪 Tests

```bash
# All tests (30 tests, no external services needed)
cargo test

# Ollama-dependent tests (2 tests, requires Ollama running)
cargo test -- --ignored

# Lint
cargo clippy -- -D warnings

# Format check
cargo fmt --check
```

Tests use `tempfile` for temporary databases — no cleanup needed.

---

## 🔧 Troubleshooting

### "Ollama is not running"

```
Error: connection refused
```

GraphRAG works without Ollama using synthetic embeddings. If you want Ollama
for better results:

```bash
# Start Ollama
ollama serve

# Verify it's running
curl http://localhost:11434/api/tags
```

### "Model not found"

```
WARN: model "gemma4:e2b" not found, skipping...
```

Pull the model:

```bash
ollama pull gemma4:e2b
```

### Build is slow

The first build runs NER (entity extraction) via LLM on every file. This is
expected — LLMs are slow. After the first build, only changed files are processed.

Tips:
- Increase `num_threads` in config (watch your VRAM)
- Use a smaller NER model: `llama3.2:3b` is 2× faster than `gemma4:e2b`
- Use `--ollama-url` to point to a remote Ollama server on a GPU machine

### "No entities found for any chunks"

Some models don't support Ollama's `format: json` mode. Try:

```bash
# Switch to a model that handles JSON output well
graphrag build --ner-model llama3.2:3b

# Or use gemma4 which has excellent JSON support
graphrag build --ner-model gemma4:e2b
```

### "graphrag.db" uses config but I wanted a literal file

If you pass `graphrag.db` (the default), GraphRAG substitutes the path from
your config file. To force a literal `graphrag.db` file, use the config path:

```toml
# In config.toml
db = "graphrag.db"
```

---

## ❓ FAQ

<details>
<summary><b>Does GraphRAG need Ollama?</b></summary>
No. Without Ollama, GraphRAG uses deterministic synthetic embeddings (hash-based).
Search still works — just without the semantic quality that LLM embeddings provide.
</details>

<details>
<summary><b>Can I use it with non-Markdown files?</b></summary>
Currently only `.md` files are supported. The build pipeline parses Markdown
frontmatter and section headers for chunking.
</details>

<details>
<summary><b>How is this different from Microsoft's GraphRAG?</b></summary>
Microsoft GraphRAG is a Python project that uses global community summarization
for question-answering over large datasets. This project is a lightweight Rust
binary focused on personal knowledge bases — instant search, no server, single
binary, works entirely offline.
</details>

<details>
<summary><b>Can I share the database between machines?</b></summary>
Yes — the SQLite database is portable. Copy it between machines and it works.
Synthetic embeddings are deterministic (same text = same vector), so builds
are reproducible across machines.
</details>

<details>
<summary><b>How do I update the graph when I add new notes?</b></summary>
Just run `graphrag build` again. It only processes new and modified files
(checked via SHA256 hash). Deleted files are auto-pruned.
</details>

<details>
<summary><b>Does it work with multiple users?</b></summary>
The SQLite database supports concurrent readers. For write access (build),
only one process at a time.
</details>

---

## 🤝 Contributing

Contributions are welcome! Here's how to get started:

```bash
# Clone and build
git clone https://github.com/atareao/graphrag
cd graphrag
cargo build

# Run tests
cargo test

# Check lint
cargo clippy -- -D warnings

# Format
cargo fmt --check
```

### Development tips

- Use `RUST_LOG=debug graphrag build` to see detailed build logs
- The demo data seed is at `src/seed/demo_data.rs` — great starting point
- All CLI commands are in `src/main.rs` — easy to add new ones
- SQLite schema is in `src/db/schema.rs`

---

## 📚 Further reading

- [Microsoft GraphRAG paper](https://arxiv.org/abs/2404.16130) — the original research
- [SQLite FTS5 documentation](https://www.sqlite.org/fts5.html) — full-text search engine
- [Leiden algorithm](https://arxiv.org/abs/1810.08473) — community detection used by GraphRAG
- [Ollama](https://ollama.ai/) — local LLM server

---

## 📄 License

MIT — see [LICENSE](LICENSE).

---

<div align="center">
  <sub>Built with 🦀 Rust • SQLite • Ollama</sub>
</div>