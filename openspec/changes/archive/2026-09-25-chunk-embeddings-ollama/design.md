# Design

## Context

Current `build` (src/graph/build.rs) uses synthetic embeddings (hash + Box-Muller) for all nodes — note titles, entities, tags. The `embed_model` parameter is unused (`_embed_model`). During search with `--ollama`, the query is embedded via Ollama but compared against synthetic vectors, producing near-random results (cosine similarity ~0.09). The actual chunk content is never embedded — only the note title and entity labels get vectors.

The project has ~19855 nodes in the user's database from ~5000 `.md` files. Chunking already exists (for NER entity extraction) but chunks are discarded after entity+co-occurrence processing.

## Goals / Non-Goals

**Goals:**
- Replace all synthetic embeddings with real Ollama embeddings during build
- Embed each chunk's actual text content (title + header + body) via Ollama batch API
- Store chunk embeddings in a new `chunks` table for vector search
- Search against chunk embeddings (not just note titles)
- Add structured filtering via `--filter` on frontmatter metadata fields
- Remove `--ollama` flag (Ollama becomes required)
- Remove all synthetic embedding code

**Non-Goals:**
- Changing the NER pipeline (entity extraction stays as-is)
- Changing FTS5 (still regenerated after build, same as today)
- Changing graph expansion logic (CTE recursion stays identical)
- Real-time incremental updates to chunk embeddings (full rebuild required)
- Support for multiple embedding models simultaneously

## Decisions

### D1: Embed chunk text as `header + "\n" + body` rather than including the note title
- **Rationale**: The chunk body contains the actual semantic content. Prepending the section header gives structural context (e.g., "## Instalación" tells the model this is about installation steps). The note title is excluded to avoid repeating the same title across all chunks of a document, which would skew similarity towards generic title matches. Title matching is handled in the reranking phase.
- **Alternative considered**: Embedding `title + header + body`. Rejected because the title repeats across all chunks of a note and dilutes the per-chunk semantic signal. Embedding only the body (without header). Rejected because the header provides useful section-level context that the embedding model can leverage.

### D2: Store chunks in a separate `chunks` table rather than adding rows to `nodes`
- **Rationale**: A note can have many chunks. The `nodes` table uses `ON CONFLICT(label)` for dedup. Chunks don't have unique labels across notes. A separate table with FK to `nodes.id` is cleaner and allows efficient vector search filtered by `note_id`.
- **Alternative considered**: Adding chunk rows as `type = 'chunk'` in `nodes`. Rejected because it conflates graph nodes with content fragments and complicates graph expansion (we don't want chunks as graph nodes).

### D3: Use Ollama `/api/embed` in batch mode (multiple texts per request)
- **Rationale**: Ollama supports sending an array of strings to `/api/embed`. A single file with 10 chunks becomes 1 HTTP call instead of 10. This reduces build time significantly.
- **Implementation**: Collect all chunk texts from a file, send as `{"model": "bge-m3", "input": [chunk1, chunk2, ...]}`. Parse the returned `embeddings` array.
- **Alternative considered**: One call per chunk. Rejected: ~5x more HTTP round-trips.

### D4: Search returns chunks, grouped by parent note
- **Rationale**: Users care about which notes are relevant, but showing the matched chunk text provides context. Grouping by `note_id` avoids displaying 3 chunks from the same note as separate results.
- **Detail**: Top-k is computed over chunks. Then results are grouped by `note_id`. Each group shows the note title, matched chunk text, and max score. Graph expansion runs on the parent note nodes.

### D5: `--filter` uses SQLite `json_extract()` at query time
- **Rationale**: No schema changes needed beyond what exists. The `nodes.metadata` column already stores frontmatter as JSON. `json_extract()` is efficient with an index on metadata fields.
- **Alternative considered**: Promoting common frontmatter fields to columns during build. Rejected: more schema churn, less flexible.
- **Performance**: For 5000 notes, `json_extract` filtering is fast enough. If needed later, an expression index can be added.

### D6: Remove `--vector-only` flag
- **Rationale**: With chunk embeddings, `vector-only` search is equivalent to `search` with `--depth 0`. The flag adds surface area without value.
- **Migration**: Users who want vector-only search can use `--depth 0`.

## Risks / Trade-offs

| Risk | Mitigation |
|------|------------|
| Build time increases significantly (~1 Ollama call per file chunk) | Use batch `/api/embed` to minimize HTTP calls; ~1 call per file regardless of chunk count |
| Ollama becomes a hard dependency; offline mode lost | Acceptable trade-off for real semantic search. Users without Ollama cannot run build/search |
| Large DB migration: existing synthetic embeddings become stale | User must run full `build` to regenerate with Ollama. Old DB is fully compatible (schema adds table, doesn't remove old columns) |
| `json_extract` on metadata may be slow with many concurrent filters | For the expected <10K notes, it's fast. Expression indexes can be added later if needed |
| Chunk table grows large (~15000 rows for 5000 files at 3 chunks/file) | Acceptable: each row is ~1KB text + 4KB embedding = ~75MB total. Negligible for SQLite |

## Migration Plan

1. Schema change: add `chunks` table (backward-compatible, existing DBs work with old code)
2. Update `build_graph()` to embed chunks via Ollama batch API
3. Update `search` to query against `chunks.embedding`
4. Remove `--ollama` flag and synthetic code
5. User runs `graphrag build` to regenerate the DB with real embeddings
6. Old DBs still function for FTS5 and graph operations, but vector search requires rebuild

## Open Questions

(none — all decisions are resolved in this design)