# Tasks: fix-community-schema-migration

## TDD Checklist

### RED — Write failing tests

- [x] **RED-1** `src/community/detect.rs`: test que abre una BD **v2** (tabla `communities` sin `keywords`/`topics`, `user_version=2`) con entidades y aristas, ejecuta `run_community_detection` y afirma: no falla, `user_version` pasa a 3 y hay comunidades insertadas. (Hoy falla: `no such column: keywords`.) → `test_run_community_detection_migrates_v2_db`
- [x] **RED-2** `src/community/detect.rs`: test de **atomicidad** — con comunidades preexistentes y una inserción forzada a fallar, `run_community_detection` devuelve error y las comunidades previas siguen intactas (tabla no vacía). (Hoy falla: el borrado no es transaccional.) → `test_run_community_detection_atomic_on_insert_failure` (trigger `RAISE(ABORT)`).
- [x] **RED-3** `src/main.rs`: test de `cmd_community_list` sobre una BD **v2** con comunidades → `Ok` y lista sin error de columna; `user_version` queda en 3. (Hoy falla: `no such column: keywords`.) → `test_cmd_community_list_migrates_v2_db`
- [x] **RED-4** `src/community/summarize.rs`: test (no requiere red) de que el camino de summarize migra el esquema — p. ej. `init_db` deja la BD v2 en v3 antes de escribir; o test con BD v2 que verifica que `update_community_summary` no falla por esquema. → `test_summarize_all_communities_migrates_v2_db` (comunidad ya resumida → `(0,0)` sin red, `user_version=3`).
- [x] Ejecutar `cargo test` → los tests nuevos fallan; los existentes siguen VERDE. → 164 passed; 4 failed (los nuevos).

### GREEN — Minimal implementation

- [x] **GREEN-1** `src/community/detect.rs`: llamar `schema::init_db(&conn)` tras `Connection::open` en `run_community_detection`; envolver `delete_all_communities` + inserciones en una **transacción** (commit al final; rollback si algo falla). → nuevo helper `replace_communities(conn, communities, resolution)` con `unchecked_transaction()`.
- [x] **GREEN-2** `src/community/summarize.rs`: llamar `schema::init_db(&conn)` tras `Connection::open` en `summarize_all_communities`.
- [x] **GREEN-3** `src/main.rs`: `cmd_community_list` llama `init_db` tras abrir; `cmd_community_detect`/`cmd_community_summarize` imprimen la cadena de error completa (`{e:#}` o similar) en vez de solo el contexto.
- [x] **GREEN-4** `src/mcp/mod.rs`: los recursos `graphrag://communities[/{id}]` migran el esquema antes de leer (llamar `init_db` tras abrir).
- [x] Ejecutar `cargo test` → 100% verde; `cargo check` sin errores. → 168 passed; 0 failed; 16 ignored. `cargo check` OK.

### REFACTOR — Clean & consolidate

- [x] `cargo clippy --all-targets -- -D warnings` → cero warnings.
- [x] `cargo fmt --check` → formato correcto.

### Verify end-to-end

- [x] Reproducir el escenario real: copia de una BD v2 → `graphrag community list` y `graphrag community detect` funcionan y dejan `user_version=3`. → copia byte-idéntica de `/data/notas/graph.db` (v2, 3124→0 comunidades por el bug); `list` migra a v3 y exit 0; `detect` regenera 3229 comunidades.
- [x] `graphrag community list` sobre `/data/notas/graph.db` (v2) → migra a v3 y lista sin error. → verificado sobre una **copia byte-idéntica** (no se mutó el fichero real del usuario); mismo código/camino.
- [x] Re-ejecutar `graphrag community detect` sobre `/data/notas/graph.db` → regenera las comunidades (las entidades/aristas están intactas). → verificado sobre la copia: nivel1=1072, nivel2=1075, 3229 comunidades persistidas.

### Archive

- [x] Consolidar los deltas en `openspec/specs/` y archivar el change. (archivado 2026-10-11)
