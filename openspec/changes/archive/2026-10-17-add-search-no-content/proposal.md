# Propuesta: `--no-content` para `search`, `similar` y `fts`

## Why

En `--format table` (el defecto) cada resultado imprime, además de su línea de score, un **bloque de
contenido**: `└── {chunk_header}: {primeros 200 caracteres del chunk}`. Cuando la nota recuperada
*cita* salida de terminal o pega bloques de código, ese preview se confunde con la salida real del
propio comando. Caso verificado sobre `/data/graph.db`:

```
graphrag search "servidor web seguro"
...
        0.917  [note    ] search-hybrida (Podcasts/guiones/0841/ejemplos-0841/search-hybrida.md)  (vecinos: 20)
   └── ## La consulta base: ```bash
graphrag search "servidor web seguro" grafo.db -k 5
```

Salida real:

```
🔍 Consulta: 'servidor web seguro'
```

El tercer resultado es un guion del podcast que **contiene el comando y su salida pegados**; su chunk
coincide palabra por palabra con la consulta (de ahí el 0.917). El efecto es que parece que la
consulta se ha ejecutado dos veces.

Hoy no hay manera de ocultar ese bloque conservando el listado: `--notes-only` **no** lo suprime
(verificado) y `--format list` elimina también scores, tipos y vecinos.

## What Changes

- `search`, `similar` y `fts` SHALL aceptar un flag booleano `--no-content`.
- Con `--no-content` y `--format table`, SHALL omitirse la línea de preview (`└── ...`) de cada
  resultado. El resto de la salida SHALL permanecer **idéntica**: la línea
  `score  [type]  label  (file)  (vecinos: N)` y el bloque `🔗 Vecinos de ...` del primer resultado.
- Con `--format list` o `--format json`, `--no-content` SHALL aceptarse y ser un **no-op** (esos
  formatos no renderizan el preview). Mismo precedente que `--format list --notes-only`.
- Sin el flag, la salida SHALL ser **idéntica a la actual** (sin cambios de comportamiento).

## Capabilities

### New Capabilities

Ninguna.

### Modified Capabilities

- `output-format`: nuevo requisito que define el flag `--no-content` y su efecto sobre el renderizado
  de `search`/`similar`/`fts`.
- `similar-command`: se matiza el requisito «Similar command uses same output format as search», que
  hoy describe el preview de chunk como incondicional.

## Impact

| Área | Impacto |
|------|---------|
| `src/main.rs` — `display_results` | recibe `no_content: bool`; se extrae un `render_results` escribible para poder testearlo |
| `src/main.rs` — `Cli` / `Commands` | flag `--no-content` en `Search`, `Similar`, `Fts` |
| `src/main.rs` — `cmd_search`, `cmd_similar`, `cmd_fts` | propagan el flag al renderizador |
| Salida por defecto | sin cambios |
| Pipeline de búsqueda | sin cambios (fases vectorial / reranking / expansión) |
| Esquema de BD | sin cambios |

## Out of scope

- `ask` no recibe el flag: su salida principal es la respuesta narrativa, no la tabla de resultados.
- No se suprime el bloque `🔗 Vecinos de ...` con este flag (se pidió solo el bloque de contenido).
- No se toca la sección `🏘️ Comunidades relacionadas` que imprime `search` tras los resultados.
- No se filtran resultados con score ~0 ni se cambia el recorte `truncate(k * 2)`.
- Las herramientas y recursos MCP no cambian (devuelven JSON).
