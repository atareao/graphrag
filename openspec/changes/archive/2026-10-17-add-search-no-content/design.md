# Design: `--no-content`

## Decisión 1 — el flag vive en el renderizador, no en el pipeline

`display_results` —hoy extraído a `render_results`— es el único punto que imprime el preview. El flag se propaga
como parámetro booleano hasta ahí; no toca `HybridSearch`, ni `SearchResult`, ni las fases de búsqueda.

## Decisión 2 — extraer un `render_results` escribible para poder testearlo

Hoy `display_results` escribe con `println!`, de modo que su salida no es verificable en tests (los
existentes solo comprueban que no entra en pánico). Para poder afirmar «no contiene `└──`» en un test,
se extrae una función que escribe en un `Write` inyectado:

```rust
fn render_results(
    results: &[SearchResult],
    format: &OutputFormat,
    no_content: bool,
    out: &mut dyn std::io::Write,
) -> std::io::Result<()>
```

`display_results(results, format, no_content)` queda como envoltorio que bloquea `stdout`:

```rust
let stdout = std::io::stdout();
let mut lock = stdout.lock();
let _ = render_results(results, format, no_content, &mut lock);
```

Los tests renderizan a un `Vec<u8>` y comparan el texto resultante. Se conserva el nombre
`display_results` para no reescribir los 6 puntos de llamada más de lo necesario.

## Decisión 3 — `no_content` solo afecta a `--format table`

`list` y `json` no imprimen preview, así que el flag se acepta y se ignora (sin error). Precedente ya
especificado: `--format list --notes-only` es un no-op en la spec de `output-format`.

## Decisión 4 — `--no-content` en `fts`

Los resultados de `fts` llegan con `chunk_header` vacío (verificado sobre `/data/graph.db`), así que
hoy la tabla de `fts` **ya** sale sin preview y el flag es un no-op. Se añade igualmente por coherencia
de superficie de CLI entre los tres comandos que comparten `display_results`.

## Alternativas descartadas

- **Valor de `--format`** (p. ej. `--format table-brief`): rompe la semántica de `--format` y obliga a
  combinaciones raras con `--notes-only`.
- **Suprimir también `🔗 Vecinos de ...`** con el mismo flag: se pidió explícitamente solo el bloque de
  contenido (opción A).
- **Filtrar resultados con score 0**: cambia el *conjunto* de resultados, no su renderizado, y es otro
  problema.
- **`-q/--quiet`**: descartado por el usuario en favor de `--no-content`.
