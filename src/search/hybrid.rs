use anyhow::Result;
use log::debug;
use rusqlite::Connection;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

use crate::embed::ollama::OllamaClient;
use crate::graph::expand::{self, Neighbor};
use crate::search::filter::Filter;
use crate::vector;

/// Clips a text string to at most `max_len` characters.
fn clip_text(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let end = s
            .char_indices()
            .nth(max_len)
            .map(|(i, _)| i)
            .unwrap_or(s.len());
        format!("{}...", &s[..end])
    }
}

/// Un resultado individual de la búsqueda híbrida
#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub id: i64,
    pub label: String,
    pub r#type: String,
    pub score: f64,
    pub content: String,
    /// Ruta del archivo (si es una nota)
    pub file: String,
    pub neighbors: Vec<Neighbor>,
    /// Cabecera del chunk que matched (si aplica)
    pub chunk_header: String,
    /// Texto completo del chunk que matched (si aplica)
    pub chunk_text: String,
}

impl SearchResult {
    #[allow(dead_code)]
    fn new(
        id: i64,
        label: String,
        type_: String,
        score: f64,
        content: String,
        file: String,
    ) -> Self {
        Self {
            id,
            label,
            r#type: type_,
            score,
            content,
            file,
            neighbors: Vec::new(),
            chunk_header: String::new(),
            chunk_text: String::new(),
        }
    }
}

/// Metadatos de un chunk cargado para búsqueda
#[derive(Debug, Clone)]
struct ChunkMeta {
    #[allow(dead_code)]
    chunk_id: i64,
    note_id: i64,
    header: String,
    text: String,
}

/// Motor de búsqueda híbrida: vectores (por chunks) + grafos de conocimiento
pub struct HybridSearch {
    conn: Connection,
    ollama: OllamaClient,
    // Vectores de chunks
    chunk_vecs: Vec<Vec<f32>>,
    chunk_meta: Vec<ChunkMeta>,
    loaded: bool,
    // Cache de embeddings de consultas
    embed_cache: std::collections::HashMap<String, Vec<f32>>,
}

impl HybridSearch {
    /// Crea un nuevo motor HybridSearch
    pub fn new(db_path: &str, ollama: OllamaClient) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        debug!("HybridSearch abierto: db={}", db_path);

        Ok(Self {
            conn,
            ollama,
            chunk_vecs: Vec::new(),
            chunk_meta: Vec::new(),
            loaded: false,
            embed_cache: std::collections::HashMap::new(),
        })
    }

    /// Constructor internal para tests (usa conexión existente)
    #[cfg(test)]
    fn new_internal(conn: Connection, ollama: OllamaClient) -> Result<Self> {
        Ok(Self {
            conn,
            ollama,
            chunk_vecs: Vec::new(),
            chunk_meta: Vec::new(),
            loaded: false,
            embed_cache: std::collections::HashMap::new(),
        })
    }

    /// Genera embedding para un texto usando Ollama
    fn embed(&mut self, text: &str) -> Result<Vec<f32>> {
        if let Some(cached) = self.embed_cache.get(text) {
            let preview: String = text.chars().take(40).collect();
            debug!("Embedding cache hit: '{}'...", preview);
            return Ok(cached.clone());
        }

        let vec = self.ollama.embed(text)?;
        let preview: String = text.chars().take(40).collect();
        debug!(
            "Embedding generado: '{}'... ({}d, Ollama)",
            preview,
            vec.len()
        );

        self.embed_cache.insert(text.to_string(), vec.clone());
        Ok(vec)
    }

    /// Carga todos los embeddings de chunks desde SQLite
    fn load_all_chunks(&mut self) -> Result<()> {
        if self.loaded {
            return Ok(());
        }

        let data = crate::db::chunks::load_all_chunk_embeddings(&self.conn)?;
        for (chunk_id, note_id, vec, header, text) in data {
            if vec.is_empty() {
                continue; // saltar chunks sin embedding
            }
            self.chunk_vecs.push(vec);
            self.chunk_meta.push(ChunkMeta {
                chunk_id,
                note_id,
                header,
                text,
            });
        }

        self.loaded = true;
        debug!("Chunks cargados: {} con embedding.", self.chunk_vecs.len());
        Ok(())
    }

    /// Computa similitud coseno de `query_vec` contra todos los chunks cargados.
    /// Devuelve Vec<(chunk_index, score)> para TODOS los chunks, ordenado por score descendente.
    fn compute_similarities(&self, query_vec: &[f32]) -> Vec<(usize, f64)> {
        let mut sims: Vec<(usize, f64)> = self
            .chunk_vecs
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let sim = vector::cosine_similarity_raw(v, query_vec) as f64;
                (i, sim)
            })
            .collect();
        sims.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        sims
    }

    /// Obtiene la ruta del archivo de un nodo desde metadata JSON
    fn get_file(&self, node_id: i64) -> Result<String> {
        let result: std::result::Result<String, _> = self.conn.query_row(
            "SELECT COALESCE(metadata, '{}') FROM nodes WHERE id = ?1",
            rusqlite::params![node_id],
            |row| row.get(0),
        );

        match result {
            Ok(meta_str) => {
                if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&meta_str) {
                    if let Some(path) = meta.get("path").and_then(|f| f.as_str()) {
                        return Ok(path.to_string());
                    }
                }
                Ok(String::new())
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(String::new()),
            Err(e) => Err(e.into()),
        }
    }

    /// Obtiene el contenido textual de un nodo desde metadata JSON
    fn get_content(&self, node_id: i64) -> Result<String> {
        let result: std::result::Result<String, _> = self.conn.query_row(
            "SELECT COALESCE(metadata, '{}') FROM nodes WHERE id = ?1",
            rusqlite::params![node_id],
            |row| row.get(0),
        );

        match result {
            Ok(meta_str) => {
                if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&meta_str) {
                    if let Some(content) = meta.get("content").and_then(|c| c.as_str()) {
                        return Ok(content.to_string());
                    }
                    if let Some(desc) = meta.get("description").and_then(|d| d.as_str()) {
                        return Ok(desc.to_string());
                    }
                }
                Ok(String::new())
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(String::new()),
            Err(e) => Err(e.into()),
        }
    }

    /// Obtiene el label de un nodo desde la tabla `nodes`
    fn get_note_label(&self, note_id: i64) -> Result<String> {
        let result: std::result::Result<String, _> = self.conn.query_row(
            "SELECT label FROM nodes WHERE id = ?1",
            rusqlite::params![note_id],
            |row| row.get(0),
        );
        match result {
            Ok(label) => Ok(label),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(format!("note#{}", note_id)),
            Err(e) => Err(e.into()),
        }
    }

    /// Obtiene el label de un nodo, aplicando filtros de metadatos.
    ///
    /// Returns `None` si el nodo existe pero no cumple los filtros,
    /// o si el nodo no existe.
    fn get_note_label_with_filters(
        &self,
        note_id: i64,
        filters: &[Filter],
    ) -> Result<Option<String>> {
        if filters.is_empty() {
            // Sin filtros, comportamiento normal
            return Ok(Some(self.get_note_label(note_id)?));
        }

        let mut sql = "SELECT label FROM nodes WHERE id = ?1".to_string();
        let (filter_sql, filter_vals) = crate::search::filter::build_filter_sql(filters);
        sql.push_str(&filter_sql);

        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(note_id)];
        for v in &filter_vals {
            params.push(Box::new(v.clone()));
        }

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();

        let result: std::result::Result<String, _> =
            self.conn
                .query_row(&sql, param_refs.as_slice(), |row| row.get(0));

        match result {
            Ok(label) => Ok(Some(label)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Búsqueda híbrida completa: 3 fases
    ///
    /// FASE 1 — Búsqueda vectorial: similitud coseno sobre chunks
    /// FASE 2 — Reranking: bonificaciones por título y agrupación por nota
    ///   Si se proporcionan `filters`, se aplican contra el metadata JSON
    ///   de la nota padre. Los chunks cuya nota no cumpla los filtros se omiten.
    /// FASE 3 — Expansión por grafo: CTE recursiva desde la nota padre
    #[allow(clippy::too_many_arguments)]
    pub fn hybrid_search(
        &mut self,
        query: &str,
        k: usize,
        depth: i32,
        alpha: f64,
        min_weight: Option<f64>,
        notes_only: bool,
        filters: &[Filter],
    ) -> Result<Vec<SearchResult>> {
        // === FASE 1: Búsqueda vectorial sobre chunks ===
        let query_vec = self.embed(query)?;
        self.load_all_chunks()?;

        if self.chunk_vecs.is_empty() {
            debug!("No chunks with embeddings found in database.");
            return Ok(Vec::new());
        }

        let query_norm = vector::norm(&query_vec);
        if query_norm == 0.0 {
            return Ok(Vec::new());
        }

        // Calcular similitud coseno con todos los vectores de chunks
        let sims = self.compute_similarities(&query_vec);
        let top_k: Vec<_> = sims.into_iter().take(k).collect();

        if top_k.is_empty() {
            return Ok(Vec::new());
        }

        debug!(
            "FASE 1 — Vector search: top {} de {} chunks. Mejor score: {:.4}",
            top_k.len(),
            self.chunk_vecs.len(),
            top_k.first().map(|(_, s)| s).unwrap_or(&0.0)
        );

        // === FASE 2: Reranking con agrupación por nota ===
        let query_lower = query.to_lowercase();
        let query_words: HashSet<&str> = query_lower.split_whitespace().collect();

        // Cache de labels de notas para evitar queries repetidas
        let mut label_cache: HashMap<i64, String> = HashMap::new();

        /// Candidato interno tras reranking
        struct ChunkCandidate {
            note_id: i64,
            score: f64,
            header: String,
            text: String,
        }

        let mut seen_notes: HashSet<i64> = HashSet::new();
        let mut candidates: Vec<ChunkCandidate> = Vec::new();

        for (idx, vec_score) in &top_k {
            let meta = &self.chunk_meta[*idx];

            // Saltar chunks repetidos de la misma nota (quedamos con el mejor)
            if seen_notes.contains(&meta.note_id) {
                continue;
            }
            seen_notes.insert(meta.note_id);

            // Obtener label de la nota padre, con filtros si existen
            let note_label = match label_cache.get(&meta.note_id) {
                Some(l) => l.clone(),
                None => {
                    match self.get_note_label_with_filters(meta.note_id, filters)? {
                        Some(l) => {
                            label_cache.insert(meta.note_id, l.clone());
                            l
                        }
                        None => {
                            // Nota no cumple filtros → omitir este chunk
                            continue;
                        }
                    }
                }
            };

            // Bonus si el título de la nota contiene palabras de la query
            let label_lower = note_label.to_lowercase();
            let label_words: HashSet<&str> = label_lower.split_whitespace().collect();
            let title_bonus = if query_words.intersection(&label_words).next().is_some() {
                0.1
            } else {
                0.0
            };

            // Bonus por ser nota (los chunks siempre son de notas)
            let type_bonus = 0.2;

            let score = alpha * (*vec_score + title_bonus + type_bonus);
            candidates.push(ChunkCandidate {
                note_id: meta.note_id,
                score,
                header: meta.header.clone(),
                text: meta.text.clone(),
            });
        }

        candidates.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        debug!(
            "FASE 2 — Reranking: {} candidatos (agrupados por nota). Mejor: '{}' ({:.4})",
            candidates.len(),
            candidates
                .first()
                .map(|c| { label_cache.get(&c.note_id).cloned().unwrap_or_default() })
                .unwrap_or_default(),
            candidates.first().map(|c| c.score).unwrap_or(0.0)
        );

        // === FASE 3: Expansión por grafo ===
        let mut seen_ids: HashSet<i64> = HashSet::new();
        let mut results: Vec<SearchResult> = Vec::new();

        for candidate in &candidates {
            if seen_ids.contains(&candidate.note_id) {
                continue;
            }
            seen_ids.insert(candidate.note_id);

            let neighbors = if depth > 0 {
                if let Some(min_w) = min_weight {
                    expand::expand_neighbors_weighted(&self.conn, candidate.note_id, depth, min_w)?
                } else {
                    expand::expand_neighbors(&self.conn, candidate.note_id, depth)?
                }
            } else {
                Vec::new()
            };

            let note_label = label_cache
                .get(&candidate.note_id)
                .cloned()
                .unwrap_or_else(|| self.get_note_label(candidate.note_id).unwrap_or_default());

            let result = SearchResult {
                id: candidate.note_id,
                label: note_label.clone(),
                r#type: "note".to_string(),
                score: candidate.score,
                content: clip_text(&candidate.text, 200),
                file: self.get_file(candidate.note_id)?,
                neighbors: neighbors.clone(),
                chunk_header: candidate.header.clone(),
                chunk_text: candidate.text.clone(),
            };
            results.push(result);

            // Añadir vecinos con score reducido
            for n in neighbors.iter().take(3) {
                if seen_ids.contains(&n.id) {
                    continue;
                }
                seen_ids.insert(n.id);

                let neighbor_score = (1.0 - alpha) * candidate.score;
                results.push(SearchResult {
                    id: n.id,
                    label: n.label.clone(),
                    r#type: n.r#type.clone(),
                    score: neighbor_score,
                    content: self.get_content(n.id)?,
                    file: self.get_file(n.id)?,
                    neighbors: Vec::new(),
                    chunk_header: String::new(),
                    chunk_text: String::new(),
                });
            }
        }

        debug!(
            "FASE 3 — Graph expansion: {} resultados finales (depth={})",
            results.len(),
            depth
        );

        // Orden final y límite
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(k * 2);

        // Normalizar scores (min-max)
        let (min_score, max_score) = if !results.is_empty() {
            let max_s = results
                .iter()
                .map(|r| r.score)
                .fold(f64::NEG_INFINITY, f64::max);
            let min_s = results
                .iter()
                .map(|r| r.score)
                .fold(f64::INFINITY, f64::min);
            if max_s > min_s {
                for r in &mut results {
                    r.score = (r.score - min_s) / (max_s - min_s);
                }
            }
            (min_s, max_s)
        } else {
            (0.0, 0.0)
        };
        debug!(
            "Normalización min-max: score range [{:.4}, {:.4}]",
            min_score, max_score
        );

        // Filtrar solo notas si se solicitó
        let results = if notes_only {
            results.into_iter().filter(|r| r.r#type == "note").collect()
        } else {
            results
        };

        Ok(results)
    }

    /// Búsqueda solo por grafo desde un nodo
    pub fn graph_only(&self, node_label: &str, depth: i32) -> Result<Vec<Neighbor>> {
        let id: i64 = self
            .conn
            .query_row(
                "SELECT id FROM nodes WHERE label = ?1",
                rusqlite::params![node_label],
                |row| row.get(0),
            )
            .map_err(|_| anyhow::anyhow!("Nodo '{}' no encontrado", node_label))?;

        expand::expand_neighbors(&self.conn, id, depth)
    }

    /// Busca notas similares a una nota ya indexada, comparando cada chunk
    /// individualmente (match individual) y agregando por nota destino con score = max.
    /// Sin llamar a Ollama — reusa embeddings almacenados en la BD.
    pub fn similar_by_label(
        &mut self,
        label: &str,
        k: usize,
        depth: i32,
        min_weight: Option<f64>,
        notes_only: bool,
        _filters: &[Filter],
    ) -> Result<Vec<SearchResult>> {
        // 1. Resolver label → note_id
        let note_id: i64 = self
            .conn
            .query_row(
                "SELECT id FROM nodes WHERE label = ?1",
                rusqlite::params![label],
                |row| row.get(0),
            )
            .map_err(|_| anyhow::anyhow!("Nodo '{}' no encontrado", label))?;

        // 2. Cargar chunks de la nota origen
        let source_chunks = crate::db::chunks::get_chunks_by_note(&self.conn, note_id)?;

        if source_chunks.is_empty() {
            return Ok(Vec::new());
        }

        // 3. Asegurar que los embeddings de chunks están cargados
        self.load_all_chunks()?;

        if self.chunk_vecs.is_empty() {
            return Ok(Vec::new());
        }

        // 4. Match individual: para cada chunk origen, compute_similarities,
        //    agregar por nota destino con max.
        //    Excluir los chunks de la propia nota origen.
        use std::collections::HashMap;

        let mut note_scores: HashMap<i64, f64> = HashMap::new();
        let mut label_cache: HashMap<i64, String> = HashMap::new();

        for chunk in &source_chunks {
            let emb = crate::vector::blob_to_vector(chunk.embedding.as_deref().unwrap_or(&[]))?;
            if emb.is_empty() {
                continue;
            }

            let sims = self.compute_similarities(&emb);

            for (idx, score) in &sims {
                let target_note_id = self.chunk_meta[*idx].note_id;
                // Excluir la propia nota origen
                if target_note_id == note_id {
                    continue;
                }
                // Actualizar score con el máximo encontrado
                let entry = note_scores.entry(target_note_id).or_insert(0.0);
                if *score > *entry {
                    *entry = *score;
                }
            }
        }

        // 5. Tomar top-K notas por score
        let mut sorted: Vec<(i64, f64)> = note_scores.into_iter().collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        sorted.truncate(k);

        // 6. Reranking + graph expansion (similar a hybrid_search FASE 3)
        let mut seen_ids: std::collections::HashSet<i64> = std::collections::HashSet::new();
        let mut results: Vec<SearchResult> = Vec::new();

        for (target_note_id, score) in &sorted {
            if seen_ids.contains(target_note_id) {
                continue;
            }
            seen_ids.insert(*target_note_id);

            let neighbors = if depth > 0 {
                if let Some(min_w) = min_weight {
                    crate::graph::expand::expand_neighbors_weighted(
                        &self.conn,
                        *target_note_id,
                        depth,
                        min_w,
                    )?
                } else {
                    crate::graph::expand::expand_neighbors(&self.conn, *target_note_id, depth)?
                }
            } else {
                Vec::new()
            };

            let note_label = match label_cache.get(target_note_id) {
                Some(l) => l.clone(),
                None => {
                    let l = self.get_note_label(*target_note_id)?;
                    label_cache.insert(*target_note_id, l.clone());
                    l
                }
            };

            // Obtener el chunk header/text del mejor match para display
            let chunk_info = {
                let mut best = (String::new(), String::new(), 0.0_f64);
                for chunk in &source_chunks {
                    let emb =
                        crate::vector::blob_to_vector(chunk.embedding.as_deref().unwrap_or(&[]))
                            .ok();
                    if let Some(ref e) = emb {
                        let sims = self.compute_similarities(e);
                        for (idx, s) in &sims {
                            if self.chunk_meta[*idx].note_id == *target_note_id && *s > best.2 {
                                best = (
                                    self.chunk_meta[*idx].header.clone(),
                                    self.chunk_meta[*idx].text.clone(),
                                    *s,
                                );
                            }
                        }
                    }
                }
                best
            };

            results.push(SearchResult {
                id: *target_note_id,
                label: note_label,
                r#type: "note".to_string(),
                score: *score,
                content: clip_text(&chunk_info.1, 200),
                file: self.get_file(*target_note_id)?,
                neighbors,
                chunk_header: chunk_info.0,
                chunk_text: chunk_info.1,
            });
        }

        // Orden final y límite
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(k * 2);

        // Normalizar scores (min-max)
        if !results.is_empty() {
            let max_s = results
                .iter()
                .map(|r| r.score)
                .fold(f64::NEG_INFINITY, f64::max);
            let min_s = results
                .iter()
                .map(|r| r.score)
                .fold(f64::INFINITY, f64::min);
            if max_s > min_s {
                for r in &mut results {
                    r.score = (r.score - min_s) / (max_s - min_s);
                }
            }
        }

        if notes_only {
            results.retain(|r| r.r#type == "note");
        }

        Ok(results)
    }

    /// Busca notas similares a un archivo externo (no indexado en la BD),
    /// chonkeándolo, generando embeddings vía Ollama, y comparando
    /// cada chunk individualmente (match individual).
    pub fn similar_by_file(
        &mut self,
        path: &str,
        k: usize,
        depth: i32,
        min_weight: Option<f64>,
        notes_only: bool,
        _filters: &[Filter],
    ) -> Result<Vec<SearchResult>> {
        let content = std::fs::read_to_string(path)
            .map_err(|_| anyhow::anyhow!("Archivo no encontrado: {}", path))?;

        let chunks = crate::chunking::markdown::chunk_document(&content);
        if chunks.is_empty() {
            return Ok(Vec::new());
        }

        // Concatenar header + "\n" + text para cada chunk
        let embed_texts: Vec<String> = chunks
            .iter()
            .map(|c| format!("{}\n{}", c.header, c.text))
            .collect();
        let embed_refs: Vec<&str> = embed_texts.iter().map(|s| s.as_str()).collect();

        // Batch embed via Ollama
        let embeddings = self.ollama.batch_embed(&embed_refs)?;

        // Cargar chunks de la BD
        self.load_all_chunks()?;

        if self.chunk_vecs.is_empty() {
            return Ok(Vec::new());
        }

        // Match individual: para cada chunk origen, compute_similarities,
        // agregar por nota destino con max
        use std::collections::HashMap;
        let mut note_scores: HashMap<i64, f64> = HashMap::new();
        let mut label_cache: HashMap<i64, String> = HashMap::new();

        for emb in &embeddings {
            let sims = self.compute_similarities(emb);

            for (idx, score) in &sims {
                let target_note_id = self.chunk_meta[*idx].note_id;
                let entry = note_scores.entry(target_note_id).or_insert(0.0);
                if *score > *entry {
                    *entry = *score;
                }
            }
        }

        // Tomar top-K
        let mut sorted: Vec<(i64, f64)> = note_scores.into_iter().collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        sorted.truncate(k);

        // Graph expansion + output assembly (idéntico a similar_by_label)
        let mut seen_ids: std::collections::HashSet<i64> = std::collections::HashSet::new();
        let mut results: Vec<SearchResult> = Vec::new();

        for (target_note_id, score) in &sorted {
            if seen_ids.contains(target_note_id) {
                continue;
            }
            seen_ids.insert(*target_note_id);

            let neighbors = if depth > 0 {
                if let Some(min_w) = min_weight {
                    crate::graph::expand::expand_neighbors_weighted(
                        &self.conn,
                        *target_note_id,
                        depth,
                        min_w,
                    )?
                } else {
                    crate::graph::expand::expand_neighbors(&self.conn, *target_note_id, depth)?
                }
            } else {
                Vec::new()
            };

            let note_label = match label_cache.get(target_note_id) {
                Some(l) => l.clone(),
                None => {
                    let l = self.get_note_label(*target_note_id)?;
                    label_cache.insert(*target_note_id, l.clone());
                    l
                }
            };

            // Mejor chunk de display
            let mut best = (String::new(), String::new(), 0.0_f64);
            for emb in &embeddings {
                let sims = self.compute_similarities(emb);
                for (idx, s) in &sims {
                    if self.chunk_meta[*idx].note_id == *target_note_id && *s > best.2 {
                        best = (
                            self.chunk_meta[*idx].header.clone(),
                            self.chunk_meta[*idx].text.clone(),
                            *s,
                        );
                    }
                }
            }

            results.push(SearchResult {
                id: *target_note_id,
                label: note_label,
                r#type: "note".to_string(),
                score: *score,
                content: clip_text(&best.1, 200),
                file: self.get_file(*target_note_id)?,
                neighbors,
                chunk_header: best.0,
                chunk_text: best.1,
            });
        }

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(k * 2);

        if !results.is_empty() {
            let max_s = results
                .iter()
                .map(|r| r.score)
                .fold(f64::NEG_INFINITY, f64::max);
            let min_s = results
                .iter()
                .map(|r| r.score)
                .fold(f64::INFINITY, f64::min);
            if max_s > min_s {
                for r in &mut results {
                    r.score = (r.score - min_s) / (max_s - min_s);
                }
            }
        }

        if notes_only {
            results.retain(|r| r.r#type == "note");
        }

        Ok(results)
    }

    /// Búsqueda FTS5 (textual exacta)
    pub fn fts_search(
        &self,
        query: &str,
        limit: usize,
        notes_only: bool,
    ) -> Result<Vec<SearchResult>> {
        let mut stmt = self.conn.prepare(
            "SELECT n.id, n.label, n.type, n.metadata, rank AS score
             FROM notes_fts
             JOIN nodes n ON notes_fts.rowid = n.id
             WHERE notes_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;

        let rows = stmt.query_map(rusqlite::params![query, limit as i64], |row| {
            let id: i64 = row.get(0)?;
            let label: String = row.get(1)?;
            let type_: String = row.get(2)?;
            let meta_str: String = row.get::<_, Option<String>>(3)?.unwrap_or_default();
            let score: f64 = row.get(4)?;

            let content = serde_json::from_str::<serde_json::Value>(&meta_str)
                .ok()
                .and_then(|v| {
                    v.get("content")
                        .and_then(|c| c.as_str().map(|s| s.to_owned()))
                })
                .unwrap_or_default();

            Ok((id, label, type_, score, content))
        })?;

        let mut results = Vec::new();
        for row in rows {
            let (id, label, type_, score, content) = row?;
            let file = self.get_file(id)?;
            results.push(SearchResult::new(id, label, type_, score, content, file));
        }
        debug!("FTS search: '{}' → {} resultados", query, results.len());

        if notes_only {
            results.retain(|r| r.r#type == "note");
        }

        Ok(results)
    }

    /// Estadísticas de la base de datos
    pub fn stats(&self) -> Result<DbStats> {
        let nodes: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))?;
        let notes: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM nodes WHERE type='note'", [], |r| {
                    r.get(0)
                })?;
        let entities: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE type != 'note' AND type != 'tag'",
            [],
            |r| r.get(0),
        )?;
        let tags: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM nodes WHERE type='tag'", [], |r| {
                    r.get(0)
                })?;
        let edges: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM edges", [], |r| r.get(0))?;
        let with_emb: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM chunks WHERE embedding IS NOT NULL",
            [],
            |r| r.get(0),
        )?;
        let chunks: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))?;

        Ok(DbStats {
            nodes,
            notes,
            entities,
            tags,
            edges,
            with_embeddings: with_emb,
            chunks,
        })
    }

    #[allow(dead_code)]
    pub fn close(self) {
        drop(self.conn);
    }
}

/// Estadísticas de la base de datos
#[derive(Debug, Serialize)]
pub struct DbStats {
    pub nodes: i64,
    pub notes: i64,
    pub entities: i64,
    pub tags: i64,
    pub edges: i64,
    pub with_embeddings: i64,
    pub chunks: i64,
}

impl std::fmt::Display for DbStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "📊 Estadísticas del grafo:\n\
             ├── Nodos totales:     {}\n\
             ├── Notas:             {}\n\
             ├── Entidades:         {}\n\
             ├── Tags:              {}\n\
             ├── Aristas:           {}\n\
             ├── Chunks:            {}\n\
             └── Con embeddings:    {}",
            self.nodes,
            self.notes,
            self.entities,
            self.tags,
            self.edges,
            self.chunks,
            self.with_embeddings
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::init_db;
    use crate::search::filter::Filter;
    use crate::vector::vector_to_blob;

    /// Creates an in-memory database with a note and some chunks with embeddings.
    fn setup_db_with_chunks() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();

        // Insert a note node with metadata
        conn.execute(
            "INSERT INTO nodes (id, label, type, metadata) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![1i64, "test-note", "note", r#"{"path":"/tmp/test.md"}"#],
        )
        .unwrap();

        // Insert two chunks with embeddings
        let emb1 = vector_to_blob(&[0.1_f32, 0.2_f32, 0.3_f32]);
        let emb2 = vector_to_blob(&[0.4_f32, 0.5_f32, 0.6_f32]);
        crate::db::chunks::insert_chunk(
            &conn,
            1,
            "Header 1",
            "This is the first chunk text content",
            "header-1",
            Some(&emb1),
            None,
        )
        .unwrap();
        crate::db::chunks::insert_chunk(
            &conn,
            1,
            "Header 2",
            "This is the second chunk with more text content",
            "header-2",
            Some(&emb2),
            None,
        )
        .unwrap();

        conn
    }

    #[test]
    fn test_load_all_chunks_populates_vectors() {
        let conn = setup_db_with_chunks();

        // Create HybridSearch with a mock-like approach:
        // We can't easily construct HybridSearch without an Ollama client,
        // so we test load_all_chunks indirectly via the public API.
        // Instead, test the data insertion directly.
        let data = crate::db::chunks::load_all_chunk_embeddings(&conn).unwrap();
        assert_eq!(data.len(), 2, "should load 2 chunks");

        let (id1, note_id1, emb1, header1, text1) = &data[0];
        assert_eq!(*note_id1, 1);
        assert_eq!(emb1.len(), 3);
        assert_eq!(header1, "Header 1");
        assert_eq!(text1, "This is the first chunk text content");
        assert!(*id1 > 0);

        let (id2, note_id2, emb2, header2, text2) = &data[1];
        assert_eq!(*note_id2, 1);
        assert_eq!(emb2.len(), 3);
        assert_eq!(header2, "Header 2");
        assert_eq!(text2, "This is the second chunk with more text content");
        assert!(*id2 > *id1, "second chunk should have a larger id");
    }

    #[test]
    fn test_load_all_chunks_empty_db() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();

        let data = crate::db::chunks::load_all_chunk_embeddings(&conn).unwrap();
        assert!(data.is_empty(), "empty db should return no chunks");
    }

    #[test]
    fn test_load_all_chunks_skips_null_embeddings() {
        let conn = setup_db_with_chunks();

        // Add a chunk without embedding
        crate::db::chunks::insert_chunk(
            &conn,
            1,
            "No Embed",
            "This chunk has no embedding",
            "no-embed",
            None,
            None,
        )
        .unwrap();

        let data = crate::db::chunks::load_all_chunk_embeddings(&conn).unwrap();
        // All three chunks are returned, but the third has empty vec
        assert_eq!(data.len(), 3);
        assert!(data[2].2.is_empty(), "third chunk should have empty vec");
    }

    #[test]
    fn test_get_note_label() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();

        conn.execute(
            "INSERT INTO nodes (id, label, type) VALUES (?1, ?2, ?3)",
            rusqlite::params![42i64, "My Note", "note"],
        )
        .unwrap();

        // We can test via a direct query (HybridSearch::get_note_label is private)
        let label: String = conn
            .query_row(
                "SELECT label FROM nodes WHERE id = ?1",
                rusqlite::params![42i64],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(label, "My Note");
    }

    #[test]
    fn test_clip_text_short() {
        assert_eq!(clip_text("hello", 10), "hello");
    }

    #[test]
    fn test_clip_text_long() {
        let long = "a".repeat(300);
        let clipped = clip_text(&long, 200);
        assert_eq!(clipped.len(), 203); // 200 + "..."
        assert!(clipped.ends_with("..."));
    }

    #[test]
    fn test_clip_text_unicode() {
        let long = "í".repeat(300); // 'í' is 2 bytes in UTF-8
        let clipped = clip_text(&long, 200);
        assert_eq!(clipped.chars().count(), 203); // 200 chars + "..."
        assert!(clipped.ends_with("..."));
        // Verify it doesn't panic at byte boundaries
        let _ = clip_text(&"aéíóuñ".repeat(50), 200);
    }

    #[test]
    fn test_search_result_chunk_fields() {
        let r = SearchResult {
            id: 1,
            label: "test".into(),
            r#type: "note".into(),
            score: 0.9,
            content: "preview text".into(),
            file: "/path/file.md".into(),
            neighbors: vec![],
            chunk_header: "Introduction".into(),
            chunk_text: "Full chunk text here".into(),
        };
        assert_eq!(r.chunk_header, "Introduction");
        assert_eq!(r.chunk_text, "Full chunk text here");
        assert_eq!(r.content, "preview text");
    }

    #[test]
    fn test_get_note_label_with_filters_matching() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO nodes (id, label, type, metadata) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                42i64,
                "My Note",
                "note",
                r#"{"date": 2023, "category": "tutorial"}"#
            ],
        )
        .unwrap();

        let ollama = crate::embed::ollama::OllamaClient::new("http://localhost:11434", "test");
        let hs = HybridSearch::new_internal(conn, ollama).unwrap();

        let filters = vec![Filter {
            field: "category".into(),
            operator: "=".into(),
            value: "tutorial".into(),
        }];
        let result = hs.get_note_label_with_filters(42, &filters).unwrap();
        assert_eq!(result, Some("My Note".to_string()));
    }

    #[test]
    fn test_get_note_label_with_filters_non_matching() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO nodes (id, label, type, metadata) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                42i64,
                "My Note",
                "note",
                r#"{"date": 2021, "category": "guide"}"#
            ],
        )
        .unwrap();

        let ollama = crate::embed::ollama::OllamaClient::new("http://localhost:11434", "test");
        let hs = HybridSearch::new_internal(conn, ollama).unwrap();

        let filters = vec![Filter {
            field: "date".into(),
            operator: ">=".into(),
            value: "2023".into(),
        }];
        let result = hs.get_note_label_with_filters(42, &filters).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_get_note_label_with_filters_no_filters() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO nodes (id, label, type, metadata) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![42i64, "My Note", "note", r#"{"date": 2023}"#],
        )
        .unwrap();

        let ollama = crate::embed::ollama::OllamaClient::new("http://localhost:11434", "test");
        let hs = HybridSearch::new_internal(conn, ollama).unwrap();

        // No filters → should return the label
        let result = hs.get_note_label_with_filters(42, &[]).unwrap();
        assert_eq!(result, Some("My Note".to_string()));
    }

    #[test]
    fn test_get_note_label_with_filters_non_existent_field() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO nodes (id, label, type, metadata) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![42i64, "My Note", "note", r#"{"date": 2023}"#],
        )
        .unwrap();

        let ollama = crate::embed::ollama::OllamaClient::new("http://localhost:11434", "test");
        let hs = HybridSearch::new_internal(conn, ollama).unwrap();

        // Non-existent field → should not match (null comparison)
        let filters = vec![Filter {
            field: "nonexistent".into(),
            operator: "=".into(),
            value: "value".into(),
        }];
        let result = hs.get_note_label_with_filters(42, &filters).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_get_note_label_with_filters_multiple_and() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO nodes (id, label, type, metadata) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                42i64,
                "Tutorial Note",
                "note",
                r#"{"date": 2023, "category": "tutorial", "status": "published"}"#
            ],
        )
        .unwrap();

        let ollama = crate::embed::ollama::OllamaClient::new("http://localhost:11434", "test");
        let hs = HybridSearch::new_internal(conn, ollama).unwrap();

        // Both filters must match
        let filters = vec![
            Filter {
                field: "category".into(),
                operator: "=".into(),
                value: "tutorial".into(),
            },
            Filter {
                field: "date".into(),
                operator: ">=".into(),
                value: "2023".into(),
            },
        ];
        let result = hs.get_note_label_with_filters(42, &filters).unwrap();
        assert_eq!(result, Some("Tutorial Note".to_string()));
    }

    #[test]
    fn test_get_note_label_with_filters_multiple_and_one_fails() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO nodes (id, label, type, metadata) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                42i64,
                "Old Guide",
                "note",
                r#"{"date": 2021, "category": "guide", "status": "archived"}"#
            ],
        )
        .unwrap();

        let ollama = crate::embed::ollama::OllamaClient::new("http://localhost:11434", "test");
        let hs = HybridSearch::new_internal(conn, ollama).unwrap();

        // First filter matches, second fails → should return None
        let filters = vec![
            Filter {
                field: "category".into(),
                operator: "=".into(),
                value: "guide".into(),
            },
            Filter {
                field: "date".into(),
                operator: ">=".into(),
                value: "2023".into(),
            },
        ];
        let result = hs.get_note_label_with_filters(42, &filters).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_get_note_label_with_filters_note_not_found() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        // No nodes inserted

        let ollama = crate::embed::ollama::OllamaClient::new("http://localhost:11434", "test");
        let hs = HybridSearch::new_internal(conn, ollama).unwrap();

        let filters = vec![Filter {
            field: "date".into(),
            operator: ">=".into(),
            value: "2023".into(),
        }];
        let result = hs.get_note_label_with_filters(999, &filters).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    #[ignore = "needs Ollama"]
    fn test_search_empty_chunks() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();

        // Insert a note node (so FTS expansion doesn't crash) but NO chunks.
        conn.execute(
            "INSERT INTO nodes (id, label, type, metadata) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![1i64, "test-note", "note", r#"{"path":"/tmp/test.md"}"#],
        )
        .unwrap();

        let ollama = crate::embed::ollama::OllamaClient::new("http://localhost:11434", "test");
        let mut hs = HybridSearch::new_internal(conn, ollama).unwrap();

        let results = hs
            .hybrid_search("test query", 5, 2, 0.7, None, false, &[])
            .unwrap();
        assert!(
            results.is_empty(),
            "expected no results when chunks table is empty, got {}",
            results.len()
        );
    }

    #[test]
    #[ignore = "needs Ollama"]
    fn test_search_ollama_unreachable() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();

        // Point at a port where nothing is listening → connection refused.
        let ollama = crate::embed::ollama::OllamaClient::new("http://127.0.0.1:1", "test-model");
        let mut hs = HybridSearch::new_internal(conn, ollama).unwrap();

        let result = hs.hybrid_search("query", 5, 2, 0.7, None, false, &[]);
        assert!(result.is_err(), "expected error when Ollama is unreachable");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Error connecting to Ollama")
                || err.contains("Connection refused")
                || err.contains("error trying to connect"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_similar_by_label_returns_results() {
        let conn = setup_db_with_chunks();

        // Añadir una segunda nota con chunks para que haya algo con qué comparar
        conn.execute(
            "INSERT INTO nodes (id, label, type, metadata) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![2i64, "second-note", "note", r#"{"path":"/tmp/second.md"}"#],
        )
        .unwrap();

        // Chunk con embedding similar al chunk 1 de test-note (más cercano a [0.1,0.2,0.3])
        let emb_similar = crate::vector::vector_to_blob(&[0.11_f32, 0.21_f32, 0.31_f32]);
        crate::db::chunks::insert_chunk(
            &conn,
            2,
            "Similar Header",
            "similar text content",
            "similar",
            Some(&emb_similar),
            None,
        )
        .unwrap();

        let ollama = crate::embed::ollama::OllamaClient::new("http://localhost:11434", "test");
        let mut hs = HybridSearch::new_internal(conn, ollama).unwrap();

        let results = hs
            .similar_by_label("test-note", 5, 0, None, false, &[])
            .unwrap();
        assert!(!results.is_empty(), "should return at least one result");
        assert!(
            results.iter().any(|r| r.label == "second-note"),
            "second-note should be among results"
        );
    }

    #[test]
    fn test_similar_by_label_not_found() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::schema::init_db(&conn).unwrap();

        let ollama = crate::embed::ollama::OllamaClient::new("http://localhost:11434", "test");
        let mut hs = HybridSearch::new_internal(conn, ollama).unwrap();

        let result = hs.similar_by_label("NonExistent", 5, 0, None, false, &[]);
        assert!(result.is_err(), "should error for non-existent label");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("no encontrado") || err.contains("NonExistent"),
            "error should mention the label: {}",
            err
        );
    }

    #[test]
    fn test_similar_by_file_not_found() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::schema::init_db(&conn).unwrap();

        let ollama = crate::embed::ollama::OllamaClient::new("http://localhost:11434", "test");
        let mut hs = HybridSearch::new_internal(conn, ollama).unwrap();

        let result = hs.similar_by_file("/tmp/nonexistent_file_xyz.md", 5, 0, None, false, &[]);
        assert!(result.is_err(), "should error for non-existent file");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("no encontrado") || err.contains("NotFound"),
            "error should mention file not found: {}",
            err
        );
    }
}
