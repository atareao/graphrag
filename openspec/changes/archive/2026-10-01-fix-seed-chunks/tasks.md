# Tasks: fix-seed-chunks

## TDD Checklist

### RED — Write failing tests

- [x] **RED-1** `src/seed/demo_data.rs`: test unitario **no-ignorado** de un helper puro que, dado un embedding precalculado, asocia una nota a su `ChunkData` — verifica `header == label`, `text == content`, `slug == slug` y blob no vacío (sin Ollama).
- [x] **RED-2** `src/seed/demo_data.rs`: test `test_seed_creates_chunk_embeddings` marcado `#[ignore = "needs Ollama"]` que abre la BD demo y afirma `COUNT(*) FROM chunks == 20` y que ningún `embedding` es NULL.
- [x] Ejecutar `cargo test` → RED-1 falla (el helper aún no existe) y los tests existentes siguen VERDE.

### GREEN — Minimal implementation

- [x] **GREEN-1** `src/seed/demo_data.rs`: helper puro que construye los datos del chunk (header/text/slug) desde una `Nota` y un embedding.
- [x] **GREEN-2** `src/seed/demo_data.rs`: dentro de la transacción de `create_demo_db`, por cada nota calcular el embedding con `ollama.embed` y llamar `crate::db::chunks::insert_chunk(tx, note_id, header, text, slug, Some(&blob), None)` antes del repoblado de FTS5.
- [x] Ejecutar `cargo test` → 100% verde.
- [x] Ejecutar `cargo check` → sin errores.

### REFACTOR — Clean & consolidate

- [x] `cargo clippy --all-targets -- -D warnings` → cero warnings.
- [x] `cargo fmt --check` → formato correcto.

### Verify end-to-end

- [x] `graphrag seed` sobre una BD nueva → crea la demo con Ollama y reporta nodos/aristas.
- [x] `graphrag stats` → `Chunks` > 0 y `Con embeddings` > 0.
- [x] `graphrag search "<tema>" db -k 5` → ≥1 resultado con nota y score.
- [x] `graphrag similar --label "<nota>" db` → ≥1 nota similar.
- [x] Ollama parado → `graphrag seed` falla con "Error connecting to Ollama" y no deja BD parcial (verificado: no queda fichero .db).
- [x] `graphrag fts "<palabra>" db` → devuelve notas (FTS intacto).

### Docs

- [x] Actualizar `GUIA_USUARIO.md` (quick start y limitaciones de `seed`).
- [x] Actualizar `README.md` y `README.es.md` (quitar la limitación "la demo no tiene chunks").
- [x] Actualizar `AGENTS.md` (estado del quick start y comandos) para reflejar que la demo ya es buscable.

### Archive

- [ ] Consolidar `demo-seed` en `openspec/specs/demo-seed/spec.md` y archivar el change.
