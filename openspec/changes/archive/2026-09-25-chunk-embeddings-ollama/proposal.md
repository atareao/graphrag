# Proposal

## Why

GraphRAG builds a knowledge graph from Markdown notes and performs hybrid search (vectors + graph). Currently, vector embeddings are computed using a synthetic hash-based method (Box-Muller) only on note titles, while the `--ollama` flag in search generates real semantic embeddings for the query. This creates a **vector space mismatch** that makes `search --ollama` return near-random results (cosine similarity ~0.09). Additionally, the actual content of note chunks is never embedded — only the title — so even without `--ollama`, semantic search is limited to title-level matching.

This change eliminates synthetic embeddings entirely, replaces them with real Ollama embeddings of chunk **content**, and adds structured filtering by frontmatter metadata.

## What Changes

- **BREAKING**: Remove `--ollama` flag from `search` and `build` — Ollama becomes a hard dependency
- **BREAKING**: Remove all synthetic embedding code (`src/vector/synthetic.rs` and references)
- **NEW**: New `chunks` table in SQLite schema to store chunk text, header, slug, and Ollama embedding per chunk
- **NEW**: During `build`, each chunk's text content (with title prepended) is embedded via Ollama `/api/embed` batch API and stored in `chunks`
- **NEW**: `search` queries against chunk embeddings (not note title embeddings) for real semantic retrieval
- **NEW**: Structured filters by frontmatter metadata: `--filter 'date >= 2023'`, `--filter 'category = tutorial'`
- **MODIFIED**: The `note` node embedding in the nodes table is now also generated via Ollama (for graph-level vector operations)
- **REMOVED**: `src/vector/synthetic.rs` and `src/vector/mod.rs` synthetic paths

## Capabilities

### New Capabilities
- `chunk-embedding`: Compute and store Ollama embeddings for each document chunk during build; search against chunk content vectors
- `metadata-filter`: Structured query filters on frontmatter fields (`date`, `category`, etc.) via SQLite JSON extraction

### Modified Capabilities
- (none — no existing specs in project)

## Impact

| Area | Impact |
|------|--------|
| `src/graph/build.rs` | Add chunk embedding step (Ollama batch API); store in `chunks` table; remove synthetic calls |
| `src/search/hybrid.rs` | Search against `chunks` table instead of note title embeddings; add metadata filter support |
| `src/db/schema.rs` | New `chunks` table: `(id, note_id, header, text, slug, embedding BLOB, metadata JSON)` |
| `src/vector/` | Remove `synthetic.rs` module entirely |
| `src/main.rs` | Remove `--ollama`, `--vector-only` flags; add `--filter` flag; plumbing for new build/search signatures |
| `src/embed/ollama.rs` | May need batch embedding support |
| Dependencies | Ollama now required (no fallback); no new crate dependencies |
| Performance | Build slower (~1 extra Ollama call per chunk, batchable); search same speed |