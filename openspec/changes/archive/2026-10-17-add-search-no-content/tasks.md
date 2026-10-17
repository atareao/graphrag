# Tasks: add-search-no-content

## TDD Checklist

### RED — Escribir los tests que fallan

- [x] **RED-1** `src/main.rs` (`mod tests`): `render_results` con `OutputFormat::Table` y `no_content=true` sobre un resultado con `chunk_header` no vacío → el buffer contiene la línea `score  [type]  label  (file)  (vecinos: N)` y **no** contiene `└──`. (Hoy no compila: `render_results` no existe / no tiene el parámetro.) → `test_render_results_no_content_omits_preview`
- [x] **RED-2** `src/main.rs`: mismo resultado con `no_content=false` → el buffer **sí** contiene `└── {header}:`. Guarda de regresión del comportamiento actual. → `test_render_results_default_keeps_preview`
- [x] **RED-3** `src/main.rs`: con `no_content=true` el bloque `🔗 Vecinos de '<label>'` del primer resultado sigue imprimiéndose. → `test_render_results_no_content_keeps_neighbors`
- [x] **RED-4** `src/main.rs`: `no_content=true` es un no-op en `OutputFormat::List` y en `OutputFormat::Json` (misma salida que con `no_content=false`). → `test_render_results_no_content_is_noop_for_list` y `test_render_results_no_content_is_noop_for_json`
- [x] **RED-5** `src/main.rs`: parsing clap — `search`, `similar` y `fts` aceptan `--no-content` y lo exponen como `true`; sin el flag, `false`. → `test_search_args_parse_no_content`, `test_similar_args_parse_no_content`, `test_fts_args_parse_no_content`
- [x] Ejecutar `cargo test` → los tests nuevos fallan (o no compilan); los existentes siguen VERDE.

### GREEN — Implementación mínima

- [x] **GREEN-1** `src/main.rs`: extraer `fn render_results(results: &[SearchResult], format: &OutputFormat, no_content: bool, out: &mut dyn std::io::Write) -> std::io::Result<()>` con el cuerpo actual de `display_results` (sustituyendo `println!` por `writeln!(out, ...)`), y condicionar la línea `└──` a `if !no_content && !r.chunk_header.is_empty()`.
- [x] **GREEN-2** `src/main.rs`: `display_results(results, format, no_content)` queda como envoltorio que bloquea `stdout` y llama a `render_results`.
- [x] **GREEN-3** `src/main.rs`: añadir `no_content: bool` (`#[arg(long)]`) a `Commands::Search`, `Commands::Similar` y `Commands::Fts`, y propagarlo en el dispatch de `main()` hasta `cmd_search`, `cmd_similar` y `cmd_fts`. En `ask`, las llamadas internas a `display_results` pasan `false`.
- [x] Ejecutar `cargo test` → 100% verde; `cargo check` sin errores.

### REFACTOR — Limpieza

- [x] `cargo clippy --all-targets -- -D warnings` → cero warnings.
- [x] `cargo fmt --check` → formato correcto.
- [x] Revisar que `-h/--help` de los tres comandos describe el flag de forma consistente.

### Docs

- [x] `README.md`, `README.es.md` y `GUIA_USUARIO.md`: fila `--no-content` en las tablas de flags de `search`, `similar` y `fts`.
- [x] Corregido el texto de las specs (`no line starting with `└──`` → ya no depende de la sangría).

### Verify end-to-end

- [x] Compilar `cargo build --release` y reinstalar el binario.
- [x] Control sobre la BD real: `graphrag search "servidor web seguro" /data/graph.db | grep -c '└──'` > 0.
- [x] Con el flag: `graphrag search "servidor web seguro" /data/graph.db --no-content | grep -c '└──'` = 0, y desaparece el bloque `Salida real:` que provocaba el guion del podcast.
- [x] `graphrag similar --label '"Instalar Android Studio en Ubuntu desde repositorio"' /data/graph.db --no-content | grep -c '└──'` = 0.
- [x] `graphrag fts "Docker" /data/graph.db --no-content` es idéntico a `graphrag fts "Docker" /data/graph.db` (no-op esperado).
- [x] Sin el flag, la salida de `search`/`similar` sigue mostrando el preview (no hay regresión).

### Archive

- [x] Consolidar los deltas en `openspec/specs/` y archivar el change. (archivado 2026-10-17)
