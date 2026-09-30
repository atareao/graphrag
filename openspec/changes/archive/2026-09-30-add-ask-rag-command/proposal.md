# Add: comando `ask` (RAG unificado)

## Why

Hoy la capacidad de "responder" existe, pero está **escondida y dispersa**:

- Vive detrás del flag `search --answer`, que mezcla el retrieval de inspección con la generación.
- Está **hardcodeada**: `community::search::answer_query()` fija top-3 comunidades y top-5 chunks, e ignora `-k`/`-a`/`--filter`.
- No devuelve **fuentes estructuradas**: la respuesta no expone de qué notas sale la evidencia.
- Si Ollama no está disponible, el flag falla; no hay degradación elegante a retrieval puro.

No hay, por tanto, un **único punto de entrada RAG** claro ni defendible como contrato.

## What Changes

1. **Nuevo subcomando `graphrag ask <QUERY> [DB]`** — pipeline RAG completo:
   `retrieval híbrido → contexto (comunidades + evidencia de chunks) → generación con Ollama → respuesta con fuentes`.
2. **Flags**: `-k`, `-d`, `-a`, `--min-weight`, `--notes-only`, `--filter`, `--communities <N>`, `--model <MODEL>`, `--format`.
3. **Fuentes / citas**: se listan siempre las notas usadas como evidencia; con `--format json` la salida es `{ "answer": ..., "sources": [...], "results": [...] }`.
4. **Degradación elegante**: sin Ollama → muestra los resultados híbridos y un aviso (exit 0); sin comunidades → responde solo con evidencia de chunks; nunca falla.
5. **Refactor de `community::search::answer_query`**: pasa a recibir opciones (nº de comunidades, nº de evidencias, límites de truncado) y a devolver `answer + sources`. Lo reutilizan `ask`, `search --answer` y la herramienta MCP `search_answer`.

## Capabilities

### New Capabilities
- `rag-ask`: comando RAG de alto nivel con generación y citas de fuentes.

### Modified Capabilities
- `community-search`: `answer_query` pasa a ser configurable y devuelve fuentes estructuradas.

## Impact

| Área | Impacto |
|------|---------|
| `src/main.rs` | Nuevo subcomando `Ask` + `cmd_ask` |
| `src/community/search.rs` | Nueva firma `answer_query(ollama, query, results, communities, opts) -> AnswerResult { answer, sources }` |
| `src/search/hybrid.rs` | Reutilizado tal cual (sin cambios funcionales) |
| `src/mcp/mod.rs` | `search_answer` se adapta a la nueva firma; opcionalmente se expone `ask` |
| `src/config.rs` | `--model` cae a `summary_model` de la config si se omite |
| Compatibilidad | `search --answer` sigue existiendo; `search`/`fts`/`graph`/`path`/`similar` quedan como retrieval de inspección sin LLM |

## Out of scope

- No se eliminan ni renombran los comandos actuales (`search`, `fts`, `graph`, `path`, `similar`).
- No hay streaming de tokens ni modo chat multi-turno.
- No se cambia el modelo de embeddings ni la detección/sumarización de comunidades.
- No se añade un índice de vectores nuevo (se sigue con similitud coseno en memoria).
