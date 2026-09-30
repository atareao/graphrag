# Fix: Identidad de notas por ruta (colisión de `label` por título)

## Why

`nodes.label` tiene `UNIQUE` y las notas se insertan con `INSERT ... ON CONFLICT(label) DO UPDATE`, usando el **título** de la nota como identidad. Dos ficheros `.md` distintos con el mismo `title` (p. ej. el mismo artículo publicado en `web/articulos/` y en `Notas/`) comparten un único nodo y **se pisan el `metadata.path` mutuamente**.

Resultado: el build "procesa" ficheros correctamente (log `✓ Guardado`) pero el conjunto de pendientes **nunca converge**:

- Fichero A tiene `path=P_A` y Fichero B `path=P_B`, mismo título.
- El build almacena `path=P_B`. En el siguiente build, A vuelve a estar pendiente y B no.
- A se procesa y deja `path=P_A`; entonces B vuelve a estar pendiente. Ping-pong infinito.

### Evidencia

Repro mínimo con dos ficheros `a/dup.md` y `b/dup.md` con `title: Duplicado`:

| Build | Pendientes | Notas en BD | `path` almacenado |
|-------|-----------|-------------|-------------------|
| #1 | 2 | 1 | `b/dup.md` |
| #2 | 1 | 1 | `a/dup.md` |
| #3 | 1 | 1 | `b/dup.md` |

En el repo real `/data/notas`:
- 6.285 ficheros `.md`, 5.064 notas en BD → 1.221 pendientes (coincide con los `1223` observados).
- **1.210 ficheros** colisionan en `label` con una nota existente en **otra ruta**.
- **1.060 títulos duplicados** entre ficheros.

Además hay 5 ficheros cuyo título coincide con una **entidad** existente (p. ej. `funzzy` → `tool`), que el `ON CONFLICT(label)` actual **convierte de entidad a nota**, destruyendo el nodo de entidad.

### Causa

La identidad de una nota no puede ser el título, porque el título no es único por fichero. Debe ser la **ruta relativa**.

## What Changes

1. **Nueva identidad `nodes.key` (UNIQUE)**:
   - Notas: `key = "note:" + <ruta relativa>`.
   - Resto de nodos (entidades, tags, directorios): `key = "node:" + <label>`.
   - Se elimina la restricción `UNIQUE(label)`; `label` pasa a ser solo texto visible (el título de la nota). Se permiten títulos duplicados.

2. **Migración de esquema v1 → v2** (`PRAGMA user_version`):
   - Añade la columna `key`, crea el índice único `idx_nodes_key` y reconstruye `nodes` para eliminar `UNIQUE(label)`, **preservando `id`** y las FKs de `edges`/`chunks`.
   - Idempotente: solo se ejecuta si `user_version < 2`. Al terminar fija `user_version = 2`.
   - Backfill: notas → `'note:' || path` (o `'note:legacy:' || id` si no hay path); no-notas → `'node:' || label`.

3. **`build_graph` usa `key` para upsert y resolución de ids**:
   - Nota: `ON CONFLICT(key) DO UPDATE`.
   - Entidades/tags/directorios: `ON CONFLICT(key) DO UPDATE` con `key = "node:" + label`.
   - Todas las resoluciones `SELECT id FROM nodes WHERE label = ?` pasan a `WHERE key = ?`.

4. **Convergencia**: tras el fix, el repro mínimo pasa a Build #1 → 2 notas (2 keys distintas) y Build #2 → 0 pendientes.

## Capabilities

### New Capabilities
- `node-identity`: semántica de identidad única por `key`, distinguiendo notas (por ruta) del resto (por label), y su migración.

### Modified Capabilities
- `graph-build`: el build identifica las notas por ruta y converge aunque haya títulos duplicados.

## Impact

| Área | Impacto |
|------|---------|
| `src/db/schema.rs` | Esquema v2 (`key` UNIQUE, sin `UNIQUE(label)`), índice `idx_nodes_key`, migración idempotente con `user_version` |
| `src/db/keys.rs` (nuevo) | Helpers `note_key(path)` y `node_key(label)` |
| `src/graph/build.rs` | Upsert por `key`; resolución de ids por `key`; `collect_existing_hashes` sin cambios (ya usa `path`) |
| `src/seed/demo_data.rs` | Inserciones con `key` |
| `src/main.rs` / `src/mcp/mod.rs` / `src/search/hybrid.rs` | `graph`/`similar`/MCP resuelven por `key`; display sigue usando `label` |
| Base de datos | Migración única: reconstrucción de `nodes` (~19k filas) preservando `id` |
| Dependencias | Sin nuevas dependencias |
| Compatibilidad | Los nodos existentes conservan `label` (título); solo se añade `key` y se permite duplicar `label` |

## Out of scope

- No se fusionan ficheros duplicados: cada fichero es una nota independiente.
- No se cambia el formato de salida ni el texto visible (`label` = título).
