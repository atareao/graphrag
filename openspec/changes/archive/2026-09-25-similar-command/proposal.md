# Proposal

## Why

Actualmente `graphrag search` requiere una frase textual como query. No hay forma de decir "encuentra notas similares a esta nota que ya está indexada" sin teclear su título, ni de pasar un archivo externo para encontrar notas relacionadas en la BD. Ambas operaciones son naturales en un motor de búsqueda híbrida y cubren casos de uso reales: exploración de notas relacionadas y recomendación de contenido similar a un borrador.

## What Changes

- Nuevo subcomando `graphrag similar` con dos modos de entrada:
  - `--label <label>`: busca documentos similares a una nota **ya indexada** en la BD, usando su embedding almacenado (sin llamar a Ollama).
  - `--file <path>`: busca documentos similares a un archivo `.md` **externo**, chonkeándolo y generando su embedding vía Ollama.
- Flags compartidas con `search`: `-k`, `-d`, `--notes-only`, `--min-weight`, `--filter`.
- Eliminación completa de referencias a "synthetic": borrar `src/vector/synthetic.rs`, renombrar/quitar el helper `inject_embedding` en `hybrid.rs`, limpiar comentarios.

## Capabilities

### New Capabilities
- `similar-command`: Nuevo subcomando `graphrag similar` que encuentra notas similares por embedding, aceptando `--label` (ya en BD, sin Ollama) o `--file` (externo, con Ollama).

### Modified Capabilities
- `chunk-embedding`: Actualizar el requirement 6 (eliminación de synthetic) para reflejar la limpieza completa, y añadir requirement para el modo `--label` que reusa embeddings almacenados sin Ollama.

## Impact

- `src/main.rs`: nuevo subcomando `Similar` + función `cmd_similar`.
- `src/search/hybrid.rs`: nuevo método `similar_by_label(label, k, depth, ...)` y `similar_by_file(file_path, k, depth, ...)`. Eliminar `inject_embedding` y comentarios synthetic.
- `src/vector/synthetic.rs`: eliminar archivo (ya es solo un comentario).
- Sin nuevas dependencias externas.