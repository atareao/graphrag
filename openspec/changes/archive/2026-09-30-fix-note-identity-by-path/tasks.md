# Tasks: fix-note-identity-by-path

## TDD Checklist

### RED — Write failing tests

- [x] **RED-1** `src/db/keys.rs` (nuevo): tests de `note_key("a/foo.md") == "note:a/foo.md"` y `node_key("Python") == "node:Python"`.
- [x] **RED-2** `src/db/schema.rs`: test `test_duplicate_labels_allowed` — con esquema v2, insertar dos nodos con mismo `label` y distinto `key` debe funcionar.
- [x] **RED-3** `src/db/schema.rs`: test `test_key_unique_enforced` — insertar dos nodos con el mismo `key` debe fallar.
- [x] **RED-4** `src/db/schema.rs`: test `test_migrate_v1_to_v2` — crear una BD con el DDL legacy + filas (nota con `path`, entidad), ejecutar `init_db`, y verificar: columna `key`, backfill correcto, `id` preservado, `idx_nodes_key`, `user_version = 2`.
- [x] **RED-5** `src/db/schema.rs`: test `test_migrate_idempotent` — segunda ejecución de `init_db` no re-clavea filas ni falla.
- [x] **RED-6** `src/graph/build.rs`: test a nivel SQL `test_upsert_note_by_key_distinct_titles` — simular dos upserts de nota con mismo `label` y distinto `key` y comprobar 2 nodos (sin Ollama).
- [x] **RED-7** `src/graph/build.rs`: test `#[ignore]` (necesita Ollama) `test_build_duplicate_titles_converges` — repo con `a/dup.md` y `b/dup.md` mismo título; primer build guarda 2 notas; segundo build reporta 0 pendientes.
- [x] Ejecutar `cargo test` → los nuevos fallan (RED) y los existentes siguen VERDE.

### GREEN — Minimal implementation

- [x] **GREEN-1** `src/db/keys.rs`: helpers `note_key`, `node_key` (+ módulo en `src/db/mod.rs`).
- [x] **GREEN-2** `src/db/schema.rs`: esquema v2 en `SCHEMA_MAIN` **y** en el DDL duplicado de `SCHEMA_SEED` (columna `key`, sin `UNIQUE(label)`, índice único `idx_nodes_key`).
- [x] **GREEN-3** `src/db/schema.rs`: `init_db` ejecuta migración idempotente `v1 -> v2` (rename `nodes`→`nodes_old` con `legacy_alter_table=ON` y `foreign_keys=OFF`, crear `nodes` v2, `INSERT ... SELECT` con claves calculadas preservando `id`, `DROP nodes_old`, crear índices, `user_version=2`).
- [x] **GREEN-4** `src/graph/build.rs`: insertar nota con `key` + `ON CONFLICT(key) DO UPDATE`; resolver `note_node_id` por `key`.
- [x] **GREEN-5** `src/graph/build.rs`: upsert de entidades/tags/directorios con `ON CONFLICT(key)` y `key = node_key(label)`; resolver sus ids por `key`.
- [x] **GREEN-6** `src/seed/demo_data.rs`: incluir `key` en todas las inserciones de `nodes`.
- [x] **GREEN-7** `src/main.rs` (`graph`, `similar`) y `src/mcp/mod.rs`: resolver nodos por `key`; mantener `label` como texto visible.
- [x] Ejecutar `cargo test` → 100% verde (incluidos los tests de migración sin Ollama).
- [x] Ejecutar `cargo check` → sin errores.

### REFACTOR — Clean & consolidate

- [x] `cargo clippy -- -D warnings` → cero warnings.
- [x] `cargo fmt --check` → formato correcto.
- [x] Revisar que no queden `ON CONFLICT(label)` ni `WHERE label = ?` para nodos identificables.

### Verify end-to-end

- [x] Copia de la BD real a `/tmp`: ejecutar `init`/migración y comprobar `user_version=2`, `idx_nodes_key` y que los `id`/aristas se preservan.
- [x] Repro manual con `a/dup.md` + `b/dup.md`: Build #1 → 2 notas; Build #2 → 0 pendientes.
- [x] `rust-reviewer` audita la migración (FKs, `id` preservados, idempotencia) y el nuevo flujo de upsert.

### Archive

- [x] Consolidar deltas en `openspec/specs/node-identity/spec.md` y `openspec/specs/graph-build/spec.md`, y archivar el change.
