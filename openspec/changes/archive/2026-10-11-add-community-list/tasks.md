# Tasks: add-community-list

## TDD Checklist

### RED — Write failing tests

- [x] **RED-1** `src/db/schema.rs`: test de migración v2→v3 — una BD con `communities` sin `keywords`/`topics` y `user_version=2` pasa a tener ambas columnas y `user_version=3`, conservando filas.
- [x] **RED-2** `src/db/schema.rs`: test de idempotencia — re-ejecutar `init_db` sobre v3 no falla ni duplica columnas; BD nueva nace en v3 con las columnas.
- [x] **RED-3** `src/db/communities.rs`: test de `update_community_summary` persistiendo `keywords`/`topics` y de lectura vía `get_community_by_id` (arrays JSON y caso vacío `[]` vs `NULL`).
- [x] **RED-4** `src/db/communities.rs`: test del nuevo helper de resolución de `label`s de miembros a partir de `member_ids` (orden e ids inexistentes).
- [x] **RED-5** `src/community/summarize.rs`: test de que el resultado de `summarize` incluye `keywords`/`topics` y que se pasan a la persistencia (el persist hoy los ignora).
- [x] **RED-6** `src/main.rs`: tests de `cmd_community_list` — filtrado por `--level`, aplicación de `--limit` y salida JSON válida (array con campos `keywords`/`topics`/`members`), usando una BD temporal.
- [x] Ejecutar `cargo test` → los tests nuevos fallan; los existentes siguen VERDE.

### GREEN — Minimal implementation

- [x] **GREEN-1** `src/db/schema.rs`: añadir `keywords TEXT` y `topics TEXT` a `SCHEMA_MAIN`/`SCHEMA_SEED`; migración v3 con `column_exists` y `PRAGMA user_version=3`.
- [x] **GREEN-2** `src/db/communities.rs`: ampliar `Community` con `keywords`/`topics` (`Vec<String>`); actualizar `parse_community_row`, `insert_community`, `update_community_summary`; añadir helper de resolución de labels de miembros.
- [x] **GREEN-3** `src/community/detect.rs`: inicializar `keywords`/`topics` vacíos al insertar comunidades.
- [x] **GREEN-4** `src/community/summarize.rs`: pasar `result.keywords`/`result.topics` a `update_community_summary`.
- [x] **GREEN-5** `src/main.rs`: subcomando `CommunityCommands::List { db, level, limit, format }` + `cmd_community_list` (table/json, fallback a `label`, preview de miembros).
- [x] **GREEN-6** `src/mcp/mod.rs`: incluir `keywords`/`topics` en `handle_resource_communities` y `handle_resource_community_by_id`.
- [x] Ejecutar `cargo test` → 100% verde.
- [x] Ejecutar `cargo check` → sin errores.

### REFACTOR — Clean & consolidate

- [x] `cargo clippy --all-targets -- -D warnings` → cero warnings (eliminar `#[allow(dead_code)]` que dejen de aplicar).
- [x] `cargo fmt --check` → formato correcto.

### Verify end-to-end

- [x] `graphrag community list <db>` sobre una BD con comunidades → tabla con `id`, `nivel`, `n`, palabras clave (o label) y miembros.
- [x] `graphrag community list <db> --level 2 --limit 5` → filtra y limita.
- [x] `graphrag community list <db> --format json | jq .` → JSON válido con `members`/`keywords`/`topics`.
- [x] Re-ejecutar `init_db` sobre `/data/notas/graph.db` (copia) → migra a v3 sin perder las 3124 comunidades.
- [x] `graphrag community summarize` sobre una BD de prueba → `keywords`/`topics` quedan persistidos y visibles en `list`. (verificado e2e con Ollama)
- [x] Sin Ollama → `community list` sigue funcionando (no hace red).

### Review follow-ups (cierre de hallazgos)

- [x] Error claro + exit ≠ 0 si la BD no tiene tabla `communities` (`cmd_community_list`); el caso "tabla vacía" sigue siendo mensaje + exit 0.
- [x] Orden determinista en SQL: `get_all_communities`/`get_communities_by_level` usan `ORDER BY level, id`.
- [x] Documentado `--limit 0` = sin límite en el atributo clap de `CommunityCommands::List`.
- [x] **T-a** `cmd_community_list` con `communities` vacía → `Ok(())` (table y json) — scenario "Empty communities table".
- [x] **T-b** `cmd_community_list` sin tabla `communities` → `Err` con mensaje claro.
- [x] **T-c** `init_db` sobre BD v1 migra en una pasada a `user_version=3` y `communities` acaba con `keywords`/`topics` (cubierto extendiendo `test_migrate_v1_to_v2`).
- [x] **T-d** `--limit 0` = sin límite (no trunca), a nivel de `select_communities`.
- [x] `get_member_labels` trocea la cláusula `IN` en lotes de 500 (sin cambiar la firma).

### Archive

- [x] Consolidar `community-listing` (nueva) y los deltas de `community-summarization` en `openspec/specs/` y archivar el change. (archivado 2026-10-11)
