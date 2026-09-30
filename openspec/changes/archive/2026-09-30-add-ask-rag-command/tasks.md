# Tasks: add-ask-rag-command

## TDD Checklist

### RED — Write failing tests

- [x] **RED-1** `src/community/search.rs`: test de `AnswerResult` — con resultados que abarcan 3 notas, `sources` contiene 3 entradas únicas (label+path), sin duplicados.
- [x] **RED-2** `src/community/search.rs`: test de opciones — `answer_query` con `communities: 1, evidence: 3` construye un prompt que contiene 1 resumen de comunidad y 3 evidencias (testear el builder de prompt con un helper puro, sin Ollama).
- [x] **RED-3** `src/community/search.rs`: test de degradación sin comunidades — sin comunidades, el prompt no incluye sección de comunidades y la respuesta se genera solo con evidencia.
- [x] **RED-4** `src/main.rs`: test de parseo de args de `ask` — flags `-k`, `-d`, `-a`, `--filter`, `--communities`, `--model`, `--format` aceptados; `ask` sin query es error de clap.
- [x] **RED-5** `src/main.rs`: test de salida JSON (función de ensamblado pura) — `{answer, sources, results}` con `sources` deduplicadas.
- [x] **RED-6** Integración `#[ignore]` (necesita Ollama) `test_ask_end_to_end` — `ask` sobre BD de demo devuelve respuesta no vacía y >=1 fuente.
- [x] Ejecutar `cargo test` → los nuevos fallan (RED) y los existentes siguen VERDE.

### GREEN — Minimal implementation

- [x] **GREEN-1** `src/community/search.rs`: `AnswerOptions { communities: usize, evidence: usize, summary_chars: usize, chunk_chars: usize }` con `Default` (3/5/500/300) y `AnswerResult { answer: String, sources: Vec<Source> }`; `Source { label: String, path: Option<String> }`.
- [x] **GREEN-2** `src/community/search.rs`: refactor de `answer_query` a la nueva firma; `search --answer` adapta la llamada con `AnswerOptions::default()` y muestra las fuentes.
- [x] **GREEN-3** `src/main.rs`: subcomando `Ask` con sus flags + `cmd_ask`; reutiliza `hybrid_search` con los mismos flags que `search`.
- [x] **GREEN-4** `src/main.rs`: ensamblado de salida (`table`/`list`/`json`) con respuesta + sección "Sources" deduplicada; `--format json` → `{answer, sources, results}`.
- [x] **GREEN-5** `src/main.rs`: degradación — si `ollama.health_check()` o `embed()` fallan, imprimir resultados híbridos + aviso, exit 0.
- [x] **GREEN-6** `src/main.rs`: aviso "run graphrag community detect" cuando no hay comunidades.
- [x] **GREEN-7** `src/main.rs`/`src/config.rs`: `--model` con fallback a `summary_model`.
- [x] **GREEN-8** `src/mcp/mod.rs`: adaptar `search_answer` a la nueva firma (y, opcionalmente, exponer tool `ask`).
- [x] Ejecutar `cargo test` → 100% verde.
- [x] Ejecutar `cargo check` → sin errores.

### REFACTOR — Clean & consolidate

- [x] `cargo clippy --all-targets -- -D warnings` → cero warnings.
- [x] `cargo fmt --check` → formato correcto.
- [x] Revisar que `search --answer` mantiene su comportamiento por defecto (sin regresión).

### Verify end-to-end

- [x] `graphrag ask "nginx" /data/notas/graph.db` → respuesta + fuentes (con aviso de comunidades, ya que hay 0).
- [x] `graphrag ask "nginx" /data/notas/graph.db --format json` → JSON válido con `answer` + `sources`.
- [x] Ollama parado → `ask` muestra retrieval + aviso, exit 0.
- [x] `graphrag search --answer "..." /data/notas/graph.db` sigue funcionando.
- [x] `rust-reviewer` audita el refactor de `answer_query` y la degradación.

### Docs

- [x] Documentar `ask` en `README.md`/`README.es.md` (tabla de comandos + ejemplo) y en `AGENTS.md`.
- [x] (Aparte) actualizar en `AGENTS.md` la tabla de flags obsoletos (`--vector-only`, `--ollama`) — ver hallazgo de documentación.

### Archive

- [x] Consolidar `rag-ask` en `openspec/specs/rag-ask/spec.md` y archivar el change.
