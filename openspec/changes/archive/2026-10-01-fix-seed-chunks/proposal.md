# Fix: `seed` no inserta chunks (búsqueda vacía sobre la demo)

## Why

La búsqueda híbrida (`search`, `ask`, `similar`) se alimenta **exclusivamente de embeddings de filas de la tabla `chunks`**. Sin embargo `graphrag seed` inserta embeddings solo a nivel de **nodo** y **nunca inserta filas en `chunks`**. Resultado: sobre una BD recién sembrada, `search`/`ask`/`similar` devuelven **0 resultados** (verificado: `stats` muestra `Chunks: 0`).

Esto rompió el "quick start" de los READMEs y deja la demo inútil para búsqueda. Es una regresión introducida cuando `search` pasó a usar chunk-embeddings (change `chunk-embeddings-ollama`). `seed` ya requiere Ollama y genera embeddings vía `ollama.embed`, así que puede crear los chunks con embeddings en la misma pasada sin coste adicional relevante.

## What Changes

- `create_demo_db` (`src/seed/demo_data.rs`) **añade**: por cada nota demo inserta al menos una fila en `chunks` referenciando su `note_id`, con `header` (el título de la nota), `text` (el contenido de la nota), `slug` (slug del título) y `embedding` no nulo calculado con Ollama, usando `crate::db::chunks::insert_chunk(conn, note_id, header, text, slug, Some(&blob), None)`.
- Esa inserción ocurre **dentro de la misma transacción** que inserta nodos/aristas y **antes** del repoblado de FTS5.
- Se mantiene que `seed` **requiere Ollama**; sin Ollama el comando falla y no deja una BD a medias.
- Se actualizan tests y documentación (READMEs, guía de usuario y `AGENTS.md`).

## Capabilities

### New Capabilities

- `demo-seed`: `graphrag seed` genera una base de datos demo lista para búsqueda, con chunks embebidos por nota además de los nodos y aristas.

### Modified Capabilities

<!-- Ninguna: `build`, `search` y el esquema no cambian de comportamiento. -->

## Impact

| Área | Impacto |
|------|---------|
| `src/seed/demo_data.rs` | `create_demo_db` inserta un chunk con embedding por nota (helper puro nota→chunk + `insert_chunk`) dentro de la transacción existente |
| `src/db/chunks.rs` | Reutilizado tal cual (`insert_chunk`); sin cambios |
| `src/db/schema.rs` | Sin cambios (la tabla `chunks` ya existe) |
| Tests | Nuevo test `#[ignore = "needs Ollama"]` de conteo de chunks; test unitario no-ignorado del helper puro |
| Docs | `README.md`, `README.es.md`, `GUIA_USUARIO.md`, `AGENTS.md` (quitar la limitación "la demo no tiene chunks") |
| Compatibilidad | `seed` sigue requiriendo Ollama; `build`/`search`/`ask`/`similar` no cambian |

## Out of scope

- No se cambia `build` ni `search` (ni su pipeline de chunk-embeddings).
- No se añaden embeddings de nodo adicionales ni se modifican los ya existentes.
- No se toca el esquema SQLite (`chunks` ya existe desde `chunk-embeddings-ollama`).
