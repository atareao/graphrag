# Add: listado de comunidades y persistencia de keywords/topics

## Why

`graphrag community` solo ofrece `detect` y `summarize`: **no hay forma de listar las comunidades
por CLI** (detect únicamente imprime el recuento por nivel). Y la identidad de una comunidad es
opaca: `label` es un identificador autogenerado (`community_{level}_{index}`) sin valor descriptivo,
y no existe campo de nombre. Lo más cercano a un nombre es `summary`, pero solo se rellena tras
resumir. Peor: `summarize` pide al LLM `keywords` y `topics`, los parsea en un `SummaryResult`, y
`update_community_summary` **los descarta** — solo persiste `summary`. Resultado real en
`/data/notas/graph.db`: 3124 comunidades, 0 resumidas, y para inspeccionarlas hay que escribir SQL
crudo con `json_each`.

## What Changes

- Nuevo subcomando **`graphrag community list [db]`** con flags `--level <N>` (filtra por nivel),
  `--limit <N>` (default 50) y `--format table|json`. Muestra `id`, `nivel`, `miembros` (nº),
  palabras clave (o `label` como fallback) y una vista previa de las entidades miembro.
- **Persistencia de `keywords` y `topics`**: dos columnas nuevas en `communities`
  (`keywords TEXT`, `topics TEXT`, arrays JSON) con migración de esquema **v2 → v3** idempotente.
- `community summarize` guarda `keywords`/`topics` además de `summary`.
- El recurso MCP `graphrag://communities[/{id}]` también los expone.
- Nuevo helper para resolver los `label` de las entidades miembro a partir de `member_ids`.

## Capabilities

### New Capabilities

- `community-listing`: comando `community list` — selección (todas o por nivel), límite, formatos
  table/json, resolución de miembros y de palabras clave con fallback.

### Modified Capabilities

- `community-summarization`: los `keywords` y `topics` que ya genera el LLM dejan de descartarse y
  SHALL persistirse en la tabla `communities` (incluye la migración de esquema que añade las columnas).

## Impact

| Área | Impacto |
|------|---------|
| `src/db/schema.rs` | v3: columnas `keywords`/`topics` en `communities` + migración idempotente; actualizar `SCHEMA_MAIN`/`SCHEMA_SEED` |
| `src/db/communities.rs` | `Community` struct + `parse_community_row` + `insert_community` + `update_community_summary`; nuevo helper de resolución de `label`s de miembros |
| `src/community/summarize.rs` | persistir `keywords`/`topics` |
| `src/community/detect.rs` | rellenar los campos nuevos como vacíos al insertar |
| `src/main.rs` | subcomando `List` en `CommunityCommands` + `cmd_community_list` |
| `src/mcp/mod.rs` | exponer `keywords`/`topics` en los recursos de comunidades |
| Compatibilidad | migración transparente: las BD existentes se actualizan en `init_db`; sin cambios de CLI previos |

## Out of scope

- No se cambia el algoritmo de detección (Leiden) ni el flag `--resolution`.
- No se cambia el prompt de resumen ni el modelo por defecto.
- No se añade navegación jerárquica padre/hijo al listado (el `parent_id` solo se expone en JSON).
- No se modifica `ask`/RAG ni el uso de `summary_embedding`.
