# Fix: Build prune omite chunks al eliminar nodos de notas

## Why

La función `build_graph()` en `src/graph/build.rs` tiene una sección de limpieza (prune) que elimina nodos de notas cuyos archivos `.md` ya no existen en disco. El código original borraba las aristas (`edges`) y luego el nodo (`nodes`), pero **nunca borraba los chunks** asociados. Como la tabla `chunks` tiene `note_id INTEGER NOT NULL REFERENCES nodes(id)` y `PRAGMA foreign_keys=ON` está activo, el `DELETE FROM nodes` falla con `FOREIGN KEY constraint failed`.

Esto causa que el build falle completamente si hay notas eliminadas del disco que tengan chunks en la base de datos — un escenario común tras reorganizar o eliminar archivos Markdown.

## What Changes

1. **Extracción de `prune_stale_notes()`**: El bucle de prune se extrajo a una función pública `prune_stale_notes()` en `src/graph/build.rs`, fuera de `build_graph()`.

2. **Fix**: Se añadió `DELETE FROM chunks WHERE note_id = ?1` entre el borrado de edges y el borrado del nodo, respetando el orden de las FOREIGN KEY constraints:
   ```
   1. DELETE FROM edges WHERE source_id = ?1 OR target_id = ?1
   2. DELETE FROM chunks WHERE note_id = ?1        ← NUEVO
   3. DELETE FROM nodes WHERE id = ?1
   ```

3. **Refactor**: `build_graph()` ahora llama a `prune_stale_notes()` en lugar de tener el bucle inline, eliminando la duplicación de código.

4. **Test**: Nuevo test `test_prune_stale_notes_fk_failure` que verifica:
   - Creación de nota + chunks + edges en DB temporal
   - Llamada a `prune_stale_notes` con path inexistente
   - Verificación de que nota, chunks y edges se eliminan correctamente

## Scope

- **Archivo afectado:** `src/graph/build.rs`
- **Sin impacto en:** schema, API, CLI, MCP, u otros módulos
- **Test:** 1 nuevo test unitario, sin dependencia externa (no requiere Ollama)

## Root cause

La tabla `chunks` (definida en `schema.rs` línea 95) tiene:
```sql
note_id INTEGER NOT NULL REFERENCES nodes(id)
```

Con `PRAGMA foreign_keys=ON` (schema.rs línea 289), SQLite impide eliminar un nodo si hay chunks que lo referencian. El código de prune nunca los eliminaba.