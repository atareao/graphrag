# Tasks

## 1. Cleanup: Eliminar referencias a synthetic

- [x] 1.1 Eliminar `src/vector/synthetic.rs` y verificar que el proyecto compila (`cargo check`)
- [x] 1.2 Eliminar el método `inject_embedding` y su impl block `#[cfg(test)]` en `src/search/hybrid.rs`. Verificar que `cargo check` no falla (el método puede quedar como dead code warning)
- [x] 1.3 Actualizar comentarios en `src/search/hybrid.rs` que mencionan "synthetic" (líneas 652, 1014). Verificar con `grep -rn synthetic src/` que no quedan ocurrencias
- [x] 1.4 Marcar `test_search_empty_chunks` como `#[ignore = "needs Ollama"]` ya que sin `inject_embedding` requiere Ollama real. Verificar que `cargo test` no ejecuta ese test ignorado

## 2. Core: Refactor loop de similitud coseno

- [x] 2.1 Extraer método privado `compute_similarities(&self, query_vec: &[f32]) -> Vec<(usize, f64)>` en `HybridSearch` que devuelva `(chunk_index, score)` para todos los chunks ordenado por score descendente. Verificar que `cargo test` pasa
- [x] 2.2 Refactorizar `hybrid_search()` para que genere el `query_vec` via `self.embed()`, llame a `compute_similarities()`, agrupe por nota (cogiendo el primer match por nota), y continúe con reranking + graph expansion. Verificar que `cargo test` sigue pasando

## 3. Core: Implementar similar_by_label (match individual)

- [x] 3.1 Implementar `HybridSearch::similar_by_label(&mut self, label: &str, k, depth, ...) -> Result<Vec<SearchResult>>`: buscar note_id, cargar chunks de esa nota, para cada chunk origen llamar a `compute_similarities()`, agregar scores por nota destino usando **max** entre todas las comparaciones. Verificar que `cargo test` pasa
- [x] 3.2 Añadir tests unitarios para `similar_by_label` en `hybrid.rs`: caso label existente retorna resultados, label no encontrado lanza error. Verificar que `cargo test` pasa

## 4. Core: Implementar similar_by_file (match individual)

- [x] 4.1 Implementar `HybridSearch::similar_by_file(&mut self, path: &str, k, depth, ...) -> Result<Vec<SearchResult>>`: leer archivo, chonquear con `chunk_document()`, batch embed via Ollama, para cada chunk origen llamar a `compute_similarities()`, agregar con **max** igual que `similar_by_label`. Verificar que `cargo check` pasa
- [x] 4.2 Añadir test unitario para `similar_by_file`: caso archivo no encontrado lanza error. Verificar que `cargo test` pasa

## 5. CLI: Nuevo subcomando Similar

- [x] 5.1 Añadir variante `Similar` al enum `Commands` en `main.rs` con flags: `--label`, `--file`, `-k`, `-d`, `--notes-only`, `--min-weight`, `--filter`, `db`. Verificar que `cargo check` pasa
- [x] 5.2 Implementar `cmd_similar()` en `main.rs` que valide que exactamente uno de `--label`/`--file` esté presente, construya el `HybridSearch`, y llame al método correspondiente. Verificar que `cargo build` compila

## 6. Documentación

- [x] 6.1 Actualizar `README.md` en inglés con el nuevo subcomando `graphrag similar`, flags, ejemplos de `--label` y `--file`. Verificar que el contenido se ha añadido (`grep 'similar' README.md`)
- [x] 6.2 Actualizar `README.es.md` con la misma documentación en español. Verificar consistencia con README.md (no existe el archivo)

## 7. Verificación final

- [x] 7.1 Ejecutar `cargo test` completo y confirmar que todos los tests pasan (los ignorados por Ollama no se ejecutan)
- [x] 7.2 Ejecutar `cargo clippy -- -D warnings` y confirmar cero warnings
- [x] 7.3 Ejecutar `cargo fmt --check` y confirmar formato correcto
- [x] 7.4 Verificar que `grep -rn synthetic` no encuentra resultados en `src/`
- [x] 7.5 Verificar que `src/vector/synthetic.rs` no existe