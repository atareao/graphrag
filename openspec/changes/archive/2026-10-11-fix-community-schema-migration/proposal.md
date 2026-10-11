# Fix: los comandos `community` no migran el esquema (regresión de add-community-list)

## Why

`add-community-list` añadió las columnas `keywords`/`topics` a la tabla `communities` (migración
**v3 en `init_db`**). Pero los comandos del grupo `community` abren la base de datos con
`Connection::open` **sin ejecutar `init_db`**, así que una base en **v2 nunca se migra**. Resultado
verificado en `/data/notas/graph.db` (`user_version=2`, 3124 comunidades):

- `graphrag community detect` → ejecuta `delete_all_communities()` y luego falla al insertar
  (`no such column: keywords`, enmascarado por `.context("failed to insert community")`). **Las
  comunidades se borran y la tabla queda a 0** (pérdida de datos).
- `graphrag community list` → `Error: no such column: keywords`.
- `community summarize` y los recursos MCP de comunidades fallarían igual.

Es una **regresión** introducida al añadir columnas nuevas sin garantizar que las rutas que las usan
apliquen la migración.

## What Changes

- Los puntos de entrada del grupo `community` —`run_community_detection`, `summarize_all_communities`,
  `cmd_community_list`— y los recursos MCP de comunidades SHALL ejecutar `schema::init_db` tras abrir
  la base, para aplicar las migraciones idempotentes (v2 → v3).
- `run_community_detection` SHALL realizar el borrado de comunidades previas y la inserción de las
  nuevas de forma **atómica** (una transacción), de modo que un error **nunca** deje la tabla vacía:
  o se reemplazan todas, o se conservan las existentes.
- `cmd_community_detect` y `cmd_community_summarize` SHALL imprimir la **cadena de error completa**
  (`{e:#}`), para que la causa SQLite real no quede enmascarada por el contexto.
- Tests que reproduzcan el fallo sobre una BD v2 (migración + listado + detect) y que verifiquen la
  atomicidad ante un fallo.

## Capabilities

### Modified Capabilities

- `community-detection`: `detect` migra el esquema antes de escribir y reemplaza comunidades de forma atómica.
- `community-summarization`: `summarize` migra el esquema antes de escribir.
- `community-listing`: `list` migra el esquema antes de leer.

## Impact

| Área | Impacto |
|------|---------|
| `src/community/detect.rs` | `init_db` al abrir + `delete+insert` en transacción |
| `src/community/summarize.rs` | `init_db` al abrir |
| `src/main.rs` | `cmd_community_list` llama `init_db`; mensajes de error con cadena completa en detect/summarize |
| `src/mcp/mod.rs` | recursos `graphrag://communities[/{id}]` migran antes de leer |
| Compatibilidad | idempotente; las BD v2 se migran a v3 en la primera operación de comunidades |

## Out of scope

- No se cambia el algoritmo Leiden ni el esquema de columnas.
- No se re-migran otras tablas ni se altera `init_db`.
- No se recuperan automáticamente las comunidades perdidas (basta re-ejecutar `detect`).
