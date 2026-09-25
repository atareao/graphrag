# Design

## Context

Ver proposal.md para motivación. Este diseño cubre los aspectos técnicos del nuevo subcomando `graphrag similar` y la limpieza de referencias a synthetic.

Arquitectura actual relevante:
- `HybridSearch` carga todos los `chunks.embedding` en memoria (`load_all_chunks()`)
- La similitud coseno se calcula contra el embedding de la query en `hybrid_search()`
- Los embeddings se generan exclusivamente vía Ollama (no existe código synthetic funcional)
- `src/vector/synthetic.rs` es un cascarón con un comentario

## Goals / Non-Goals

**Goals:**
- Nuevo subcomando `graphrag similar` con dos modos: `--label` y `--file`
- Modo `--label`: reusa embeddings almacenados en la BD, sin llamar a Ollama
- Modo `--file`: chonquea archivo externo y genera embedding vía Ollama
- Ambos modos usan **match individual**: cada chunk origen se compara individualmente contra todos los chunks destino, y el score de cada nota es el **máximo** entre todas las comparaciones
- Compartir flags con `search`: `-k`, `-d`, `--notes-only`, `--min-weight`, `--filter`
- Eliminar `inject_embedding` y todas las referencias a "synthetic" en `hybrid.rs`
- Eliminar `src/vector/synthetic.rs`
- Actualizar `README.md` y `README.es.md` con documentación del nuevo subcomando
- Reutilizar al máximo el código existente (`load_all_chunks`, `cosine_similarity_raw`, `expand_neighbors`, `chunk_document`, `batch_embed`)

**Non-Goals:**
- No se modifica el comando `search` ni su comportamiento
- No se añaden nuevas dependencias
- No se modifica el esquema de BD
- No se implementa búsqueda por embedding de nodo individual (solo por chunks)

## Decisions

### Decisión 1: Algoritmo match individual para similar_by_label

- **Qué**: Nuevo método público `similar_by_label(&mut self, label: &str, k: usize, depth: i32, min_weight: Option<f64>, notes_only: bool, filters: &[Filter]) -> Result<Vec<SearchResult>>`
- **Cómo**: 
  1. Buscar `note_id` por label en `nodes`
  2. Cargar todos los chunks de esa nota desde `chunks` (por `note_id`)
  3. Para **cada** chunk origen, calcular similitud coseno contra **todos** los chunks almacenados
  4. Agrupar resultados por `note_id` destino: score = **max** de todas las comparaciones (origen→destino)
  5. Reranking + graph expansion (misma lógica que `hybrid_search()`)
- **Ventaja clave**: Sin llamada a Ollama. Un chunk sobre "Docker" en una nota que también habla de "Python" encontrará notas sobre Docker, no un término medio diluido.

### Decisión 2: Mismo algoritmo para similar_by_file

- **Qué**: Nuevo método público `similar_by_file(&mut self, path: &str, k: usize, depth: i32, ...)`
- **Cómo**:
  1. Leer archivo con `std::fs::read_to_string()`
  2. Chonquear con `chunking::markdown::chunk_document()`
  3. Embed via `self.ollama.batch_embed()` (cada chunk se embedé por separado)
  4. Misma lógica de match individual: cada chunk origen vs todos los chunks destino, score = max por nota
- **Optimización**: Los chunks del archivo externo no se insertan en la BD. Solo se usan para calcular similitudes.

### Decisión 3: Refactor del loop de coseno

- El cálculo de similitud coseno (FASE 1 actual en `hybrid_search()`) se extrae a un método privado:
  ```rust
  fn compute_similarities(&self, query_vec: &[f32]) -> Vec<(usize, f64)>
  ```
  Devuelve `(chunk_index, score)` para todos los chunks, ordenado por score descendente.
- `hybrid_search()` lo llama una vez y agrupa por nota (cogiendo el primer match por nota).
- `similar_by_label()` y `similar_by_file()` lo llaman N veces (una por chunk origen) y agrupan por nota con **max**.
- Esto evita duplicar el loop de cosine similarity manteniendo lógicas de agregación distintas.

### Decisión 4: Eliminación de `inject_embedding`

- El test `test_search_empty_chunks` es el único que usa `inject_embedding`
- Se marcará como `#[ignore = "needs Ollama"]` y se eliminará el método y su impl block `#[cfg(test)]`
- Alternativa considerada y descartada: mock de OllamaClient — demasiado complejo para un solo test

### Decisión 5: Estructura del CLI

- Nueva variante en `Commands`:
  ```
  Similar {
      label: Option<String>,
      file: Option<String>,
      db: String,
      k: usize,
      depth: i32,
      min_weight: Option<f64>,
      notes_only: bool,
      filter: Vec<String>,
  }
  ```
- Validación: exactamente uno de `--label` o `--file` debe estar presente
- Si no se especifica ninguno, error: "Debes especificar --label o --file"

## Risks / Trade-offs

- **[Rendimiento] `similar_by_label()` carga todos los embeddings en memoria.** Igual que `search`. Para miles de chunks (~100k) puede ser pesado, pero es el mismo bottleneck existente.
- **[Rendimiento] Match individual hace N pases de coseno (N = chunks origen).** Para una nota con 10 chunks, son 10 pases en lugar de 1. Sigue siendo microsegundos por pase (solo aritmética vectorial en memoria).
- **[Ollama dependency] `--file` requiere Ollama.** Es la misma dependencia que `search` y `build`. Si no hay Ollama, el usuario puede usar `--label` que no lo necesita.
- **[Archivos no Markdown] `--file` asume sintaxis Markdown.** Si el archivo no tiene frontmatter ni headers, `chunk_document()` devuelve un solo chunk con header vacío. Funciona, pero el chunking no aporta valor.