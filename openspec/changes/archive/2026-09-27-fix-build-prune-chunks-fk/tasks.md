# Tasks: fix-build-prune-chunks-fk

## TDD Checklist

### RED — Write failing tests

- [x] **RED-1**: Añadir test `test_prune_stale_notes_fk_failure` en `src/graph/build.rs` que:
  - Crea nota en DB con chunks y file path inexistente
  - Llama a `prune_stale_notes` con el bug (sin DELETE FROM chunks)
  - Verifica que falla con FK constraint error

### GREEN — Minimal implementation

- [x] **GREEN-1**: Añadir `DELETE FROM chunks WHERE note_id = ?1` entre la eliminación de edges y la eliminación del nodo en `prune_stale_notes`
- [x] **GREEN-2**: Reemplazar código inline de prune en `build_graph` por llamada a `prune_stale_notes`
- [x] Verificar que `cargo test` pasa (tests nuevos + existentes): ✅ 124 passed, 0 failed
- [x] Verificar que `cargo check` compila sin errores

### REFACTOR — Clean & consolidate

- [x] Ejecutar `cargo clippy -- -D warnings` — ✅ zero warnings
- [x] Ejecutar `cargo fmt --check` — ✅ formatting correcto

### Archive

- [ ] ~~Ejecutar `openspec archive fix-build-prune-chunks-fk`~~ → Los headers del delta no matchean specs existentes. Se archiva manualmente.