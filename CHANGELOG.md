# Changelog
## [0.2.0] - 2026-09-25

### Features

- ✨ Community detection with Leiden CPM hierarchical clustering
- ✨ LLM-powered community summarization (JSON structured output)
- ✨ Answer mode: community summaries + chunk evidence as RAG context
- ✨ Chunk embeddings via Ollama for hybrid search
- ✨ Metadata filters (`--min-weight`, `--notes-only`)
- ✨ MCP tools: `community_detect`, `community_summarize`, `search_answer`
- ✨ Config: `summary_model`, default `embed_model` → `bge-m3:latest`
- ✨ OpenSpec specs for all modules

### Bug Fixes

- CI: Use `GITHUB_TOKEN` for checkout, `GH_PAT` for push/sync
- CI: Force update development branch after release