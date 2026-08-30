use std::collections::HashSet;
use anyhow::Result;
use rusqlite::Connection;
use serde::Serialize;
use log::debug;

use crate::embed::ollama::OllamaClient;
use crate::vector::{self, synthetic};
use crate::graph::expand::{self, Neighbor};

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
}

impl SearchResult {
    fn new(id: i64, label: String, type_: String, score: f64, content: String, file: String) -> Self {
        Self {
            id,
            label,
            r#type: type_,
            score,
            content,
            file,
            neighbors: Vec::new(),
        }
    }
}

/// Metadatos de un nodo cargado para búsqueda
#[derive(Debug, Clone)]
struct NodeMeta {
    id: i64,
    label: String,
    r#type: String,
}

/// Motor de búsqueda híbrida: vectores + grafos de conocimiento
pub struct HybridSearch {
    conn: Connection,
    ollama: Option<OllamaClient>,
    #[allow(dead_code)]
    use_synthetic: bool,
    dims: usize,
    // Cache de embeddings de nodos
    all_vecs: Vec<Vec<f32>>,
    all_meta: Vec<NodeMeta>,
    loaded: bool,
    // Cache de embeddings de consultas
    embed_cache: std::collections::HashMap<String, Vec<f32>>,
}

impl HybridSearch {
    /// Crea un nuevo motor HybridSearch
    /// - db_path: ruta a la base de datos SQLite
    /// - ollama: cliente Ollama opcional (si es None, usa embeddings sintéticos)
    pub fn new(db_path: &str, ollama: Option<OllamaClient>) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        let use_synthetic = ollama.is_none();
        let dims = ollama.as_ref().map_or(1024, |o| o.embedding_dimension());
        debug!("HybridSearch abierto: db={}, ollama={}, dims={}",
               db_path, if ollama.is_some() { "sí" } else { "no (sintético)" }, dims);

        Ok(Self {
            conn,
            ollama,
            use_synthetic,
            dims,
            all_vecs: Vec::new(),
            all_meta: Vec::new(),
            loaded: false,
            embed_cache: std::collections::HashMap::new(),
        })
    }

    /// Genera embedding para un texto (Ollama o sintético)
    fn embed(&mut self, text: &str) -> Result<Vec<f32>> {
        if let Some(cached) = self.embed_cache.get(text) {
            let preview: String = text.chars().take(40).collect();
            debug!("Embedding cache hit: '{}'...", preview);
            return Ok(cached.clone());
        }

        let vec = if let Some(ref client) = self.ollama {
            client.embed(text)?
        } else {
            synthetic::synthetic_embedding(text, self.dims)
        };
        let preview: String = text.chars().take(40).collect();
        debug!("Embedding generado: '{}'... ({}d, {})",
               preview, vec.len(),
               if self.ollama.is_some() { "Ollama" } else { "sintético" });

        self.embed_cache.insert(text.to_string(), vec.clone());
        Ok(vec)
    }

    /// Carga todos los embeddings de nodos desde SQLite
    fn load_all_embeddings(&mut self) -> Result<()> {
        if self.loaded {
            return Ok(());
        }

        let mut stmt = self.conn.prepare(
            "SELECT id, label, type, embedding FROM nodes WHERE embedding IS NOT NULL"
        )?;

        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let label: String = row.get(1)?;
            let type_: String = row.get(2)?;
            let blob: Vec<u8> = row.get(3)?;
            Ok((id, label, type_, blob))
        })?;

        for row in rows {
            let (id, label, type_, blob) = row?;
            let vec = vector::blob_to_vector(&blob)?;
            self.all_vecs.push(vec);
            self.all_meta.push(NodeMeta { id, label, r#type: type_ });
        }

        self.loaded = true;
        debug!("Embeddings cargados: {} nodos con vector.", self.all_vecs.len());
        Ok(())
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

    /// Búsqueda híbrida completa: 3 fases
    ///
    /// FASE 1 — Búsqueda vectorial: similitud coseno sobre todos los nodos
    /// FASE 2 — Reranking: bonificaciones por título y tipo
    /// FASE 3 — Expansión por grafo: CTE recursiva desde los candidatos
    pub fn hybrid_search(
        &mut self,
        query: &str,
        k: usize,
        depth: i32,
        alpha: f64,
        min_weight: Option<f64>,
    ) -> Result<Vec<SearchResult>> {
        // === FASE 1: Búsqueda vectorial ===
        let query_vec = self.embed(query)?;
        self.load_all_embeddings()?;

        if self.all_vecs.is_empty() {
            return Ok(Vec::new());
        }

        let query_norm = vector::norm(&query_vec);
        if query_norm == 0.0 {
            return Ok(Vec::new());
        }

        // Calcular similitud coseno con todos los vectores
        let mut sims: Vec<(usize, f64)> = self.all_vecs
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let sim = vector::cosine_similarity_raw(v, &query_vec) as f64;
                (i, sim)
            })
            .collect();

        // Ordenar por similitud descendente y tomar top-k
        sims.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let top_k: Vec<_> = sims.into_iter().take(k).collect();

        if top_k.is_empty() {
            return Ok(Vec::new());
        }

        debug!("FASE 1 — Vector search: top {} de {} nodos. Mejor score: {:.4}",
               top_k.len(), self.all_vecs.len(),
               top_k.first().map(|(_, s)| s).unwrap_or(&0.0));

        // === FASE 2: Reranking ===
        let query_lower = query.to_lowercase();
        let query_words: HashSet<&str> = query_lower.split_whitespace().collect();
        let mut candidates: Vec<(f64, &NodeMeta)> = Vec::new();

        for (idx, vec_score) in &top_k {
            let meta = &self.all_meta[*idx];

            // Bonus si el título contiene palabras de la query
            let label_lower = meta.label.to_lowercase();
            let label_words: HashSet<&str> = label_lower.split_whitespace().collect();
            let title_bonus = if query_words.intersection(&label_words).next().is_some() {
                0.1
            } else {
                0.0
            };

            // Bonus si es nota (pesa más que entidades sueltas)
            let type_bonus = if meta.r#type == "note" { 0.2 } else { 0.0 };

            let score = alpha * (*vec_score + title_bonus + type_bonus);
            candidates.push((score, meta));
        }

        candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        debug!("FASE 2 — Reranking: {} candidatos. Mejor: '{}' ({:.4})",
               candidates.len(),
               candidates.first().map(|(_, m)| &m.label).unwrap_or(&String::new()),
               candidates.first().map(|(s, _)| s).unwrap_or(&0.0));

        // === FASE 3: Expansión por grafo ===
        let mut seen_ids: HashSet<i64> = HashSet::new();
        let mut results: Vec<SearchResult> = Vec::new();

        for (score, meta) in &candidates {
            if seen_ids.contains(&meta.id) {
                continue;
            }
            seen_ids.insert(meta.id);

            let neighbors = if depth > 0 {
                if let Some(min_w) = min_weight {
                    expand::expand_neighbors_weighted(&self.conn, meta.id, depth, min_w)?
                } else {
                    expand::expand_neighbors(&self.conn, meta.id, depth)?
                }
            } else {
                Vec::new()
            };

            let mut result = SearchResult::new(
                meta.id,
                meta.label.clone(),
                meta.r#type.clone(),
                *score,
                self.get_content(meta.id)?,
                self.get_file(meta.id)?,
            );
            result.neighbors = neighbors.clone();
            results.push(result);

            // Añadir vecinos con score reducido
            for n in neighbors.iter().take(3) {
                if seen_ids.contains(&n.id) {
                    continue;
                }
                seen_ids.insert(n.id);

                let neighbor_score = (1.0 - alpha) * score;
                results.push(SearchResult::new(
                    n.id,
                    n.label.clone(),
                    n.r#type.clone(),
                    neighbor_score,
                    self.get_content(n.id)?,
                    self.get_file(n.id)?,
                ));
            }
        }

        debug!("FASE 3 — Graph expansion: {} resultados finales (depth={})", results.len(), depth);

        // Orden final y límite
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(k * 2);

        // Normalizar scores (min-max)
        let (min_score, max_score) = if !results.is_empty() {
            let max_s = results.iter().map(|r| r.score).fold(f64::NEG_INFINITY, f64::max);
            let min_s = results.iter().map(|r| r.score).fold(f64::INFINITY, f64::min);
            if max_s > min_s {
                for r in &mut results {
                    r.score = (r.score - min_s) / (max_s - min_s);
                }
            }
            (min_s, max_s)
        } else {
            (0.0, 0.0)
        };
        debug!("Normalización min-max: score range [{:.4}, {:.4}]", min_score, max_score);

        Ok(results)
    }

    /// Búsqueda solo vectorial (sin expansión por grafo)
    pub fn vector_only(&mut self, query: &str, k: usize) -> Result<Vec<SearchResult>> {
        self.hybrid_search(query, k, 0, 0.7, None)
    }

    /// Búsqueda solo por grafo desde un nodo
    pub fn graph_only(&self, node_label: &str, depth: i32) -> Result<Vec<Neighbor>> {
        let id: i64 = self.conn.query_row(
            "SELECT id FROM nodes WHERE label = ?1",
            rusqlite::params![node_label],
            |row| row.get(0),
        ).map_err(|_| anyhow::anyhow!("Nodo '{}' no encontrado", node_label))?;

        expand::expand_neighbors(&self.conn, id, depth)
    }

    /// Búsqueda FTS5 (textual exacta)
    pub fn fts_search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>> {
        let mut stmt = self.conn.prepare(
            "SELECT n.id, n.label, n.type, n.metadata, rank AS score
             FROM notes_fts
             JOIN nodes n ON notes_fts.rowid = n.id
             WHERE notes_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2"
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

        Ok(results)
    }

    /// Estadísticas de la base de datos
    pub fn stats(&self) -> Result<DbStats> {
        let nodes: i64 = self.conn.query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))?;
        let notes: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE type='note'", [], |r| r.get(0)
        )?;
        let entities: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE type != 'note' AND type != 'tag'", [], |r| r.get(0)
        )?;
        let tags: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE type='tag'", [], |r| r.get(0)
        )?;
        let edges: i64 = self.conn.query_row("SELECT COUNT(*) FROM edges", [], |r| r.get(0))?;
        let with_emb: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM nodes WHERE embedding IS NOT NULL", [], |r| r.get(0)
        )?;

        Ok(DbStats { nodes, notes, entities, tags, edges, with_embeddings: with_emb })
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
             └── Con embeddings:    {}",
            self.nodes, self.notes, self.entities, self.tags, self.edges, self.with_embeddings
        )
    }
}