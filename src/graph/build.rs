use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use log::{debug, info};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::Duration;
use walkdir::WalkDir;

use crate::chunking::{chunk_document, parse_frontmatter, slugify};
use crate::db::schema;
use crate::ner::{extract_entities_batch, Entity, DEFAULT_LABELS};
use crate::vector;

/// Dimensión de los embeddings sintéticos
const EMBED_DIMS: usize = 1024;

/// Estadísticas de la construcción del grafo
#[derive(Debug, Default, Clone)]
pub struct BuildStats {
    pub files: usize,
    pub entities: usize,
    pub total_nodes: i64,
    pub total_edges: i64,
}

impl std::fmt::Display for BuildStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "📊 Resumen del grafo:\n\
             ├── Archivos procesados: {}\n\
             ├── Entidades extraídas: {}\n\
             ├── Total nodos:        {}\n\
             └── Total aristas:      {}",
            self.files, self.entities, self.total_nodes, self.total_edges
        )
    }
}

/// Resultado del procesamiento de un archivo, enviado del worker al writer
struct FileResult {
    relative_str: String,
    note_title: String,
    note_metadata: String,
    batch_entities: Vec<Vec<Entity>>,
    chunks: Vec<String>,
    parent_dir: String,
    tags: Vec<String>,
}

/// Construye un grafo de conocimiento desde un directorio de notas Markdown.
///
/// # Flujo
///
/// 1. Escanea recursivamente el directorio en busca de archivos `*.md`.
/// 2. Pre-examina cada archivo: calcula hash SHA256 y salta los no modificados.
/// 3. Procesa los archivos nuevos/modificados en PARALELO usando `std::thread::scope`.
///    Cada hilo abre su propia conexión SQLite.
/// 4. Para cada archivo: parsea frontmatter, trocea por cabeceras, extrae
///    entidades vía `extract_entities_batch` (una sola llamada a Ollama por archivo),
///    calcula relaciones de co-ocurrencia, e inserta todo.
/// 5. También extrae relaciones estructurales (directorio contenedor, tags del frontmatter).
/// 6. Tras todos los hilos, el hilo principal limpia notas eliminadas, entidades
///    huérfanas y repuebla el índice FTS5.
///
/// # Argumentos
///
/// * `repo_path` — Ruta al directorio raíz con las notas `.md`.
/// * `db_path` — Ruta al archivo SQLite de salida.
/// * `ollama_url` — URL del servidor Ollama (ej. `http://localhost:11434`).
/// * `ner_model` — Nombre del modelo Ollama para extracción de entidades.
/// * `embed_model` — Nombre del modelo Ollama para embeddings (reservado).
///
/// # Errores
///
/// Devuelve `anyhow::Error` si el directorio no existe, no contiene archivos
/// `.md`, o falla alguna operación de E/S o de base de datos.
pub fn build_graph(
    repo_path: &str,
    db_path: &str,
    ollama_url: &str,
    ner_model: &str,
    _embed_model: &str,
    num_threads: usize,
) -> Result<BuildStats> {
    let repo = Path::new(repo_path);
    if !repo.is_dir() {
        anyhow::bail!("'{}' no es un directorio válido", repo_path);
    }

    // ── Inicializar base de datos ──────────────────────────────────────────
    let conn = Connection::open(db_path)
        .with_context(|| format!("No se pudo abrir base de datos: {}", db_path))?;
    schema::init_db(&conn)?;

    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA cache_size=-64000;",
    )?;

    // ── Cargar hashes existentes de la BD ────────────────────────────────
    // Mapa: ruta_relativa → (hash, id_del_nodo)
    let mut existing: HashMap<String, (String, i64)> = HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT id, label, metadata FROM nodes WHERE type = 'note'")?;
        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let label: String = row.get(1)?;
            let meta_str: String = row.get::<_, Option<String>>(2)?.unwrap_or_default();
            Ok((id, label, meta_str))
        })?;

        for row in rows {
            let (id, _label, meta_str) = row?;
            if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&meta_str) {
                if let Some(hash) = meta.get("hash").and_then(|h| h.as_str()) {
                    if let Some(path) = meta.get("path").and_then(|p| p.as_str()) {
                        existing.insert(path.to_string(), (hash.to_string(), id));
                    }
                }
            }
        }
    }

    let total_files = existing.len();
    info!("Notas existentes en BD: {}", total_files);

    // ── Escanear archivos .md en disco ────────────────────────────────────
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );
    spinner.set_message("🔍 Escaneando archivos .md...");

    let md_files: Vec<_> = WalkDir::new(repo)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
        .collect();

    spinner.finish_with_message(format!("✅ Encontrados {} archivos .md", md_files.len()));

    let total = md_files.len();
    info!("Encontrados {} archivos .md en {}", total, repo_path);
    debug!("Walkdir completado. Archivos .md encontrados: {}", total);

    if total == 0 {
        anyhow::bail!("No se encontraron archivos .md en {}", repo_path);
    }

    // ── Barra de progreso ──────────────────────────────────────────────────
    let pb = Mutex::new(ProgressBar::new(total as u64));
    pb.lock().unwrap().set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg} ({per_sec:.yellow}, ETA: {eta})")
            .unwrap()
            .progress_chars("━╸━"),
    );

    // ── Pre-examen: identificar archivos nuevos o modificados ──────────────
    let mut to_process: Vec<&Path> = Vec::new();
    for entry in &md_files {
        let md_path = entry.path();
        let relative = md_path.strip_prefix(repo).unwrap_or(md_path);
        let relative_str = relative.display().to_string();

        let text = match std::fs::read_to_string(md_path) {
            Ok(t) => t,
            Err(_) => {
                pb.lock().unwrap().inc(1);
                continue;
            }
        };

        let mut hasher = Sha256::new();
        hasher.update(text.as_bytes());
        let result = hasher.finalize();
        let file_hash = result
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();

        if let Some((stored_hash, _)) = existing.get(&relative_str) {
            if *stored_hash == file_hash {
                debug!("Archivo sin cambios, saltando: {}", relative_str);
                pb.lock().unwrap().inc(1);
                continue;
            }
        }

        to_process.push(md_path);
    }

    let total_to_process = to_process.len();
    info!(
        "Archivos a procesar (nuevos/modificados): {}",
        total_to_process
    );

    // Ajustar la barra de progreso al número real de archivos a procesar
    pb.lock().unwrap().set_length(total_to_process as u64);
    pb.lock().unwrap().reset();

    // ── Procesamiento paralelo ────────────────────────────────────────────
    // Workers: leen archivos, parsean frontmatter, chunk, llaman a Ollama.
    // Writer: una sola conexión SQLite, escritura serializada vía canal.

    // Determinar número de hilos (de la configuración, con límite sensato)
    let num_threads = num_threads.max(1).min(16);

    info!(
        "Procesando con {} hilos en paralelo (NER) + 1 escritor",
        num_threads
    );

    let chunk_size = (total_to_process + num_threads - 1) / num_threads;
    let chunks: Vec<&[&Path]> = to_process.chunks(chunk_size.max(1)).collect();

    // Canal para enviar datos de hilos → writer
    let (tx, rx) = mpsc::channel::<FileResult>();

    // Rebinding como referencias: move capturará copias (las referencias son Copy)
    let stats_lock = Mutex::new(BuildStats::default());
    let pb = &pb;
    let stats_lock = &stats_lock;
    let existing = &existing;
    let repo = repo;
    let db_path = &db_path;
    let ollama_url = &ollama_url;
    let ner_model = &ner_model;

    // Directorios a saltar para relaciones estructurales
    let skip_dirs = ["notas", "muestra"];

    std::thread::scope(|s| {
        // ── Writer thread ──────────────────────────────────────────────
        // Una sola conexión, escritura serializada
        let _writer_handle = s.spawn(|| {
            let conn = match Connection::open(db_path) {
                Ok(c) => c,
                Err(e) => {
                    log::error!("Error abriendo conexión writer: {}", e);
                    return;
                }
            };
            if let Err(e) = schema::init_db(&conn) {
                log::error!("Error inicializando schema writer: {}", e);
                return;
            }
            conn.execute_batch(
                "PRAGMA journal_mode=WAL;
                 PRAGMA synchronous=NORMAL;
                 PRAGMA cache_size=-64000;",
            ).ok();

            for result in rx {
                let FileResult {
                    relative_str,
                    note_title,
                    note_metadata,
                    batch_entities,
                    ref chunks,
                    parent_dir,
                    tags,
                } = result;

                // Transacción por archivo
                let tx = match conn.unchecked_transaction() {
                    Ok(t) => t,
                    Err(e) => {
                        log::error!("Error iniciando transacción writer para {}: {}", relative_str, e);
                        pb.lock().unwrap().inc(1);
                        continue;
                    }
                };

                // Si el archivo YA existía, limpiar sus aristas viejas
                // (lo hacemos por path en el metadata del nodo tipo 'note')
                // Buscamos el nodo anterior por path para borrar sus edges
                if let Some((_, old_id)) = existing.iter().find_map(|(p, v)| {
                    if *p == relative_str { Some(v) } else { None }
                }) {
                    let _ = tx.execute(
                        "DELETE FROM edges WHERE (target_id = ?1 AND type = 'mentioned_in') OR (source_id = ?1 AND type IN ('tagged_with', 'belongs_to', 'co_occurs_with', 'mentioned_in'))",
                        rusqlite::params![old_id],
                    );
                    let _ = tx.execute(
                        "DELETE FROM edges WHERE target_id = ?1 OR source_id = ?1",
                        rusqlite::params![old_id],
                    );
                }

                // Insertar/actualizar nodo nota con embedding sintético
                let note_emb = vector::vector_to_blob(&vector::synthetic::synthetic_embedding(&note_title, EMBED_DIMS));
                if let Err(e) = tx.execute(
                    "INSERT INTO nodes (label, type, metadata, embedding)
                     VALUES (?1, 'note', ?2, ?3)
                     ON CONFLICT(label) DO UPDATE SET
                       type = excluded.type,
                       metadata = excluded.metadata,
                       embedding = excluded.embedding",
                    rusqlite::params![note_title, note_metadata, note_emb],
                ) {
                    log::warn!("⚠️  Error insertando nodo nota '{}': {}", note_title, e);
                    pb.lock().unwrap().inc(1);
                    continue;
                }

                // Obtener ID de la nota
                let note_node_id: i64 = match tx.query_row(
                    "SELECT id FROM nodes WHERE label = ?1",
                    rusqlite::params![note_title],
                    |row| row.get(0),
                ) {
                    Ok(id) => id,
                    Err(e) => {
                        log::warn!("⚠️  Error obteniendo node_id para '{}': {}", note_title, e);
                        pb.lock().unwrap().inc(1);
                        continue;
                    }
                };

                // Insertar entidades y aristas por cada chunk
                let mut all_entity_count: usize = 0;

                for (chunk_idx, entities) in batch_entities.iter().enumerate() {
                    let chunk_ref = if chunk_idx < chunks.len() && !chunks[chunk_idx].is_empty() {
                        format!("{}#{}", relative_str, chunks[chunk_idx])
                    } else {
                        relative_str.clone()
                    };

                    if entities.is_empty() {
                        continue;
                    }

                    // Contar entidades para stats
                    all_entity_count += entities.len();

                    // Insertar nodos de entidades y aristas "mentioned_in"
                    for ent in entities {
                        let ent_metadata = serde_json::json!({
                            "score": ent.score,
                            "source": ner_model,
                        });

                        let ent_emb = vector::vector_to_blob(&vector::synthetic::synthetic_embedding(&ent.label, EMBED_DIMS));
                        let _ = tx.execute(
                            "INSERT INTO nodes (label, type, metadata, embedding)
                             VALUES (?1, ?2, ?3, ?4)
                             ON CONFLICT(label) DO UPDATE SET
                               metadata = excluded.metadata,
                               embedding = excluded.embedding",
                            rusqlite::params![ent.label, ent.type_, ent_metadata.to_string(), ent_emb],
                        );

                        if let Ok(ent_node_id) = tx.query_row::<i64, _, _>(
                            "SELECT id FROM nodes WHERE label = ?1",
                            rusqlite::params![ent.label],
                            |row| row.get(0),
                        ) {
                            let _ = tx.execute(
                                "INSERT OR IGNORE INTO edges (source_id, target_id, type, weight, context)
                                 VALUES (?1, ?2, 'mentioned_in', ?3, ?4)",
                                rusqlite::params![ent_node_id, note_node_id, ent.score, chunk_ref],
                            );
                        }
                    }

                    // Co-ocurrencias dentro del mismo chunk
                    for i in 0..entities.len() {
                        for j in (i + 1)..entities.len() {
                            if entities[i].id == entities[j].id { continue; }
                            let weight = entities[i].score.min(entities[j].score);

                            if let (Ok(src_id), Ok(dst_id)) = (
                                tx.query_row::<i64, _, _>("SELECT id FROM nodes WHERE label = ?1",
                                    rusqlite::params![entities[i].label], |row| row.get(0)),
                                tx.query_row::<i64, _, _>("SELECT id FROM nodes WHERE label = ?1",
                                    rusqlite::params![entities[j].label], |row| row.get(0)),
                            ) {
                                let _ = tx.execute(
                                    "INSERT OR IGNORE INTO edges (source_id, target_id, type, weight, context)
                                     VALUES (?1, ?2, 'co_occurs_with', ?3, ?4)",
                                    rusqlite::params![src_id, dst_id, weight, chunk_ref],
                                );
                            }
                        }
                    }
                }

                // Relaciones estructurales: directorio
                if !parent_dir.is_empty() && !skip_dirs.contains(&parent_dir.as_str()) {
                    let dir_emb = vector::vector_to_blob(&vector::synthetic::synthetic_embedding(&parent_dir, EMBED_DIMS));
                    let _ = tx.execute(
                        "INSERT INTO nodes (label, type, embedding) VALUES (?1, 'tag', ?2) ON CONFLICT(label) DO UPDATE SET embedding = excluded.embedding",
                        rusqlite::params![parent_dir, dir_emb],
                    );
                    if let Ok(dir_id) = tx.query_row::<i64, _, _>(
                        "SELECT id FROM nodes WHERE label = ?1",
                        rusqlite::params![parent_dir],
                        |row| row.get(0),
                    ) {
                        let _ = tx.execute(
                            "INSERT OR IGNORE INTO edges (source_id, target_id, type, weight, context)
                             VALUES (?1, ?2, 'belongs_to', 1.0, ?3)",
                            rusqlite::params![note_node_id, dir_id, relative_str],
                        );
                    }
                }

                // Relaciones estructurales: tags
                for tag in &tags {
                    let tag_emb = vector::vector_to_blob(&vector::synthetic::synthetic_embedding(tag, EMBED_DIMS));
                    let _ = tx.execute(
                        "INSERT INTO nodes (label, type, embedding) VALUES (?1, 'tag', ?2) ON CONFLICT(label) DO UPDATE SET embedding = excluded.embedding",
                        rusqlite::params![tag, tag_emb],
                    );
                    if let Ok(tag_id) = tx.query_row::<i64, _, _>(
                        "SELECT id FROM nodes WHERE label = ?1",
                        rusqlite::params![tag],
                        |row| row.get(0),
                    ) {
                        let _ = tx.execute(
                            "INSERT OR IGNORE INTO edges (source_id, target_id, type, weight, context)
                             VALUES (?1, ?2, 'tagged_with', 1.0, ?3)",
                            rusqlite::params![note_node_id, tag_id, relative_str],
                        );
                    }
                }

                // Commit de la transacción del archivo
                if let Err(e) = tx.commit() {
                    log::error!("Error haciendo commit writer para {}: {}", relative_str, e);
                    pb.lock().unwrap().inc(1);
                    continue;
                }

                // Actualizar estadísticas compartidas
                {
                    let mut stats = stats_lock.lock().unwrap();
                    stats.files += 1;
                    stats.entities += all_entity_count;
                }

                pb.lock().unwrap().inc(1);
                let msg = format!("{} ({} chunks, {} entidades)", relative_str, chunks.len(), all_entity_count);
                pb.lock().unwrap().set_message(msg);
                debug!("  Commit OK. Nota '{}' procesada.", note_title);
            }
        });

        // ── Worker threads ──────────────────────────────────────────────
        // Solo leen archivos y llaman a Ollama. NO abren conexión SQLite.
        for chunk in chunks {
            let tx = tx.clone();
            s.spawn(move || {
                for md_path in chunk {
                    let relative = md_path.strip_prefix(repo).unwrap_or(md_path);
                    let relative_str = relative.display().to_string();

                    // Leer archivo y calcular hash
                    let text = match std::fs::read_to_string(md_path) {
                        Ok(t) => t,
                        Err(e) => {
                            log::warn!("⚠️  Error leyendo {}: {}", md_path.display(), e);
                            pb.lock().unwrap().inc(1);
                            continue;
                        }
                    };

                    let mut hasher = Sha256::new();
                    hasher.update(text.as_bytes());
                    let result = hasher.finalize();
                    let file_hash = result
                        .iter()
                        .map(|b| format!("{:02x}", b))
                        .collect::<String>();

                    let (metadata, body) = parse_frontmatter(&text);
                    debug!(
                        "Procesando archivo: {} (hash={})",
                        relative_str,
                        &file_hash[..8]
                    );

                    let note_stem = md_path.file_stem().unwrap().to_string_lossy();
                    if note_stem.is_empty() {
                        log::warn!("⚠️  Saltando archivo sin nombre: {}", md_path.display());
                        pb.lock().unwrap().inc(1);
                        continue;
                    }

                    let note_id_slug = slugify(&note_stem);
                    let note_title = metadata
                        .get("title")
                        .cloned()
                        .unwrap_or_else(|| note_stem.to_string());

                    let note_metadata = serde_json::json!({
                        "path": relative_str,
                        "slug": note_id_slug,
                        "hash": file_hash,
                        "content": body.trim(),
                    });

                    // Chunking + NER batch con reintentos
                    let chunks = chunk_document(&text);
                    debug!("  Chunks generados: {}", chunks.len());

                    let chunk_texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();
                    let labels = DEFAULT_LABELS;

                    // Reintentar NER hasta 3 veces con backoff
                    let batch_entities = {
                        let mut result = None;
                        for attempt in 1..=3 {
                            match extract_entities_batch(
                                ollama_url,
                                ner_model,
                                &chunk_texts,
                                labels,
                            ) {
                                Ok(ents) => {
                                    result = Some(ents);
                                    break;
                                }
                                Err(e) => {
                                    log::warn!(
                                        "⚠️  Error en NER batch para {} (intento {}/3): {}",
                                        relative_str,
                                        attempt,
                                        e
                                    );
                                    if attempt < 3 {
                                        let delay = Duration::from_secs(2u64.pow(attempt));
                                        std::thread::sleep(delay);
                                    }
                                }
                            }
                        }
                        match result {
                            Some(ents) => ents,
                            None => {
                                log::error!(
                                    "❌ NER falló tras 3 intentos para {}. Saltando archivo.",
                                    relative_str
                                );
                                pb.lock().unwrap().inc(1);
                                continue;
                            }
                        }
                    };

                    // Tags del frontmatter
                    let tags: Vec<String> = metadata
                        .get("tags")
                        .map(|t| {
                            t.split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect()
                        })
                        .unwrap_or_default();

                    // Directorio padre
                    let parent_dir = md_path
                        .parent()
                        .and_then(|p| p.file_name())
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();

                    // Enviar al writer
                    let result = FileResult {
                        relative_str: relative_str.to_string(),
                        note_title,
                        note_metadata: note_metadata.to_string(),
                        batch_entities,
                        chunks: chunks.iter().map(|c| c.header.clone()).collect(),
                        parent_dir,
                        tags,
                    };

                    if let Err(e) = tx.send(result) {
                        log::error!("Error enviando resultado al writer: {}", e);
                    }
                }
            });
        }

        // Drop del tx original para que el writer termine cuando todos los workers acaben
        drop(tx);
    });

    // ── Recolectar stats de los hilos ─────────────────────────────────────
    let thread_stats = stats_lock.lock().unwrap().clone();

    // ── Limpiar notas eliminadas del disco ────────────────────────────────
    let prune_spinner = ProgressBar::new_spinner();
    prune_spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.red} {msg}")
            .unwrap(),
    );
    prune_spinner.set_message("🧹 Limpiando notas eliminadas...");

    let ids_to_prune: Vec<(i64, String)> = {
        let mut stmt = conn.prepare("SELECT id, metadata FROM nodes WHERE type = 'note'")?;
        let rows = stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let meta_str: String = row.get::<_, Option<String>>(1)?.unwrap_or_default();
            Ok((id, meta_str))
        })?;

        let mut to_remove = Vec::new();
        for row in rows {
            let (id, meta_str) = row?;
            if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&meta_str) {
                let should_keep = meta
                    .get("path")
                    .and_then(|p| p.as_str())
                    .map(|p| {
                        let full_path = repo.join(p);
                        full_path.exists()
                    })
                    .unwrap_or(false);

                if !should_keep {
                    let note_label = meta
                        .get("slug")
                        .and_then(|s| s.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    to_remove.push((id, note_label));
                }
            }
        }
        to_remove
    };

    let pruned = ids_to_prune.len();
    if pruned > 0 {
        for (id, label) in &ids_to_prune {
            conn.execute(
                "DELETE FROM edges WHERE source_id = ?1 OR target_id = ?1",
                rusqlite::params![id],
            )
            .with_context(|| format!("Error deleting edges for pruned note id={}", id))?;
            conn.execute("DELETE FROM nodes WHERE id = ?1", rusqlite::params![id])
                .with_context(|| format!("Error deleting node id={} label={}", id, label))?;
            info!("Nota eliminada (archivo no encontrado): {}", label);
        }
        info!("Notas eliminadas del grafo: {}", pruned);
        debug!(
            "Limpieza completada: {} notas eliminadas del disco.",
            pruned
        );
    }
    prune_spinner.finish_with_message(format!("✅ {} notas eliminadas del disco", pruned));

    // ── Limpiar entidades huérfanas ──────────────────────────────────────
    // Nodos de tipo 'entity' o 'tag' sin ninguna arista → se eliminan
    conn.execute_batch(
        "DELETE FROM nodes WHERE type IN ('entity', 'tag', 'language', 'tool', 'database', 'library', 'framework', 'concept', 'security', 'os')
         AND id NOT IN (SELECT source_id FROM edges UNION SELECT target_id FROM edges);"
    ).ok();

    pb.lock().unwrap().finish_with_message("✅ ¡Completado!");

    // ── Repoblar índice FTS5 ──────────────────────────────────────────────
    let fts_spinner = ProgressBar::new_spinner();
    fts_spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.blue} {msg}")
            .unwrap(),
    );
    fts_spinner.set_message("📑 Repoblando índice FTS5...");

    conn.execute_batch(
        "DELETE FROM notes_fts;
         INSERT INTO notes_fts(rowid, title, content)
         SELECT id, label, COALESCE(json_extract(metadata, '$.content'), '')
         FROM nodes
         WHERE COALESCE(json_extract(metadata, '$.content'), '') != '';",
    )?;

    fts_spinner.finish_with_message("✅ Índice FTS5 repoblado");

    // ── Estadísticas finales ──────────────────────────────────────────────
    let mut stats = thread_stats;
    stats.total_nodes = conn.query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))?;
    stats.total_edges = conn.query_row("SELECT COUNT(*) FROM edges", [], |r| r.get(0))?;

    debug!("Índice FTS5 repoblado desde {} nodos.", stats.total_nodes);
    info!("Grafo construido: {}", stats);

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::NamedTempFile;

    /// Crea un directorio temporal con algunos archivos .md para pruebas
    /// de integración.
    fn setup_test_repo() -> (tempfile::TempDir, NamedTempFile) {
        let dir = tempfile::tempdir().expect("crear tempdir");
        let db = NamedTempFile::new().expect("crear db temporal");

        // Escribir un par de notas de prueba
        fs::write(
            dir.path().join("hola.md"),
            "---\ntitle: Hola Mundo\ntags: prueba, rust\n---\n\n# Introducción\n\nEsto es una nota de prueba.\n\n# Contenido\n\nPython es un lenguaje. Rust es otro lenguaje.\n",
        )
        .expect("escribir hola.md");

        fs::write(
            dir.path().join("subdir").join("otra.md"),
            "---\ntitle: Otra Nota\ntags: ejemplo\n---\n\n# Sección\n\nPython y Rust son lenguajes de programación.\n",
        )
        .expect("escribir otra.md");

        // Crear subdirectorio
        fs::create_dir_all(dir.path().join("subdir")).expect("crear subdir");

        (dir, db)
    }

    #[test]
    fn test_build_graph_empty_dir() {
        let dir = tempfile::tempdir().expect("crear tempdir");
        let db = NamedTempFile::new().expect("crear db temporal");

        // Directorio sin .md debe fallar
        let result = build_graph(
            dir.path().to_str().unwrap(),
            db.path().to_str().unwrap(),
            "http://localhost:11434",
            "test-model",
            "test-embed",
            4,
        );
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("No se encontraron archivos .md"),
            "error: {}",
            err
        );
    }

    #[test]
    fn test_build_graph_invalid_repo() {
        let db = NamedTempFile::new().expect("crear db temporal");
        let result = build_graph(
            "/ruta/que/no/existe",
            db.path().to_str().unwrap(),
            "http://localhost:11434",
            "test-model",
            "test-embed",
            4,
        );
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("no es un directorio válido"), "error: {}", err);
    }

    // Prueba de integración que verifica que el pipeline no crashea
    // (aunque fallará en extract_entities porque no hay Ollama).
    // Util para depurar el flujo hasta NER.
    #[test]
    #[ignore = "needs Ollama running"]
    fn test_build_graph_pipeline_structure() {
        let (_dir, _db) = setup_test_repo();
        // Esta prueba fallará por no tener Ollama, pero verifica que
        // el resto del pipeline (lectura, chunking, schema SQL) funciona.
        let result = build_graph(
            _dir.path().to_str().unwrap(),
            _db.path().to_str().unwrap(),
            "http://localhost:11434",
            "test-model",
            "test-embed",
            4,
        );
        // Esperamos un error de conexión a Ollama, NO un error de
        // schema, directorio, o SQL.
        if let Err(e) = &result {
            let msg = e.to_string();
            // No debe ser "no es un directorio" ni "no se encontraron"
            assert!(
                !msg.contains("no es un directorio"),
                "fallo temprano inesperado: {}",
                msg
            );
            assert!(
                !msg.contains("No se encontraron"),
                "fallo temprano inesperado: {}",
                msg
            );
        }
    }

    #[test]
    fn test_build_stats_display() {
        let stats = BuildStats {
            files: 10,
            entities: 42,
            total_nodes: 100,
            total_edges: 250,
        };
        let output = stats.to_string();
        assert!(output.contains("10"));
        assert!(output.contains("42"));
        assert!(output.contains("100"));
        assert!(output.contains("250"));
        assert!(output.contains("Archivos procesados"));
    }
}
