use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use log::{debug, info};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use walkdir::WalkDir;

use crate::chunking::{chunk_document, parse_frontmatter, slugify};
use crate::db::keys::{node_key, note_key};
use crate::db::schema;
use crate::embed::ollama::OllamaClient;
use crate::ner::{extract_entities_batch, Entity, DEFAULT_LABELS};
use crate::vector;

/// Datos de un chunk ya embedido listo para almacenar
struct ChunkData {
    header: String,
    text: String,
    slug: String,
    embedding_blob: Vec<u8>,
}

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
    chunk_data: Vec<ChunkData>,
    parent_dir: String,
    tags: Vec<String>,
}

/// Load existing note hashes from the database.
///
/// Returns a map of relative path → (sha256 hash, node_id) for all existing
/// note nodes that have both `hash` and `path` fields in their JSON metadata.
/// Notes without these fields (e.g., from an older graphrag version) are
/// silently excluded — they will be reprocessed and updated with the fields
/// on the next build.
pub fn collect_existing_hashes(conn: &Connection) -> Result<HashMap<String, (String, i64)>> {
    // Mapa: ruta_relativa → (hash, id_del_nodo)
    let mut existing: HashMap<String, (String, i64)> = HashMap::new();
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

    Ok(existing)
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
    embed_model: &str,
    num_threads: usize,
) -> Result<BuildStats> {
    let repo = Path::new(repo_path);
    if !repo.is_dir() {
        anyhow::bail!("'{}' no es un directorio válido", repo_path);
    }

    // ── Inicializar base de datos ──────────────────────────────────────────
    // Si la BD venía de WAL mode (versiones anteriores), la transición a
    // DELETE la gestiona SQLite de forma segura (checkpoint automático al
    // cambiar `PRAGMA journal_mode`). NO borrar los ficheros `-wal`/`-shm`
    // a mano: hacerlo puede perder datos ya confirmados.
    let conn = Connection::open(db_path)
        .with_context(|| format!("No se pudo abrir base de datos: {}", db_path))?;
    schema::init_db(&conn)?;

    conn.execute_batch(
        "PRAGMA synchronous=NORMAL;
         PRAGMA cache_size=-64000;",
    )?;

    // Verificar que el journal mode es DELETE
    if let Ok(mode) = conn.query_row::<String, _, _>("PRAGMA journal_mode", [], |r| r.get(0)) {
        log::debug!("Main connection journal_mode: {}", mode);
        if mode.to_lowercase() != "delete" {
            log::warn!("Journal mode es '{}', no 'delete'. Forzando...", mode);
            let _ = conn.execute_batch("PRAGMA journal_mode=DELETE;");
        }
    }

    // ── Instalar manejador SIGINT (Ctrl+C) ─────────────────────────────
    // Dos etapas:
    //   1ª Ctrl+C: flag de interrupción → writer checkea flag, corta
    //   2ª Ctrl+C: exit inmediato (workers en llamadas Ollama lentas)
    let interrupted = Arc::new(AtomicBool::new(false));
    let sigint_flag = interrupted.clone();
    let second_signal = Arc::new(AtomicBool::new(false));
    let second_flag = second_signal.clone();
    if let Err(e) = ctrlc::set_handler(move || {
        if second_flag.load(Ordering::SeqCst) {
            // Segundo Ctrl+C: avisar que ya estamos parando
            eprintln!("⚠️  Ya estamos terminando. Espera a que los workers acaben su NER actual.");
            return;
        }
        // Primer Ctrl+C: flag para shutdown graceful
        sigint_flag.store(true, Ordering::SeqCst);
        eprintln!("\n⚠️  Recibida señal de interrupción. Terminando archivo actual...");
        eprintln!("⚠️  Presiona Ctrl+C de nuevo para ver este mensaje.");
        second_flag.store(true, Ordering::SeqCst);
    }) {
        log::warn!("No se pudo instalar manejador SIGINT: {}", e);
    }

    // ── Cargar hashes existentes de la BD ────────────────────────────────
    let existing = collect_existing_hashes(&conn)?;

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
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        .collect();

    spinner.finish_with_message(format!("✅ Encontrados {} archivos .md", md_files.len()));

    let total = md_files.len();
    info!("Encontrados {} archivos .md en {}", total, repo_path);
    debug!("Walkdir completado. Archivos .md encontrados: {}", total);

    if total == 0 {
        anyhow::bail!("No se encontraron archivos .md en {}", repo_path);
    }

    // ── Pre-examen: identificar archivos nuevos o modificados ──────────────
    let pre_spinner = ProgressBar::new_spinner();
    pre_spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.yellow} {msg}")
            .unwrap(),
    );
    pre_spinner.set_message(format!("🔍 Comparando hashes: 0/{}", total));
    let mut examined: usize = 0;
    let mut to_process: Vec<&Path> = Vec::new();
    for entry in &md_files {
        let md_path = entry.path();
        let relative = md_path.strip_prefix(repo).unwrap_or(md_path);
        let relative_str = relative.display().to_string();

        let text = match std::fs::read_to_string(md_path) {
            Ok(t) => t,
            Err(_) => {
                examined += 1;
                pre_spinner.set_message(format!("🔍 Comparando hashes: {}/{}", examined, total));
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
                examined += 1;
                pre_spinner.set_message(format!("🔍 Comparando hashes: {}/{}", examined, total));
                continue;
            }
        }

        examined += 1;
        pre_spinner.set_message(format!("🔍 Comparando hashes: {}/{}", examined, total));
        to_process.push(md_path);
    }

    let total_to_process = to_process.len();
    info!(
        "Archivos a procesar (nuevos/modificados): {}",
        total_to_process
    );

    pre_spinner.finish_with_message(format!(
        "✅ Pre-examen completado: {} archivos a procesar",
        total_to_process
    ));

    // ── Barra de progreso principal ──────────────────────────────────────
    let pb = Mutex::new(ProgressBar::new(total_to_process as u64));
    pb.lock().unwrap().set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg} ({per_sec:.yellow}, ETA: {eta})")
            .unwrap()
            .progress_chars("━╸━"),
    );

    // ── Procesamiento paralelo ────────────────────────────────────────────
    // Workers: leen archivos, parsean frontmatter, chunk, llaman a Ollama.
    // Writer: una sola conexión SQLite, escritura serializada vía canal.

    // Determinar número de hilos (de la configuración, con límite sensato)
    let num_threads = num_threads.clamp(1, 16);

    info!(
        "Procesando con {} hilos en paralelo (NER) + 1 escritor",
        num_threads
    );

    let chunk_size = total_to_process.div_ceil(num_threads);
    let chunks: Vec<&[&Path]> = to_process.chunks(chunk_size.max(1)).collect();

    // Canal para enviar datos de hilos → writer
    let (tx, rx) = mpsc::channel::<FileResult>();

    // Rebinding como referencias: move capturará copias (las referencias son Copy)
    let stats_lock = Mutex::new(BuildStats::default());
    let pb = &pb;
    let stats_lock = &stats_lock;
    let existing = &existing;
    let db_path = &db_path;
    let ollama_url = &ollama_url;
    let ner_model = &ner_model;
    let embed_model = &embed_model;

    // Directorios a saltar para relaciones estructurales
    let skip_dirs = ["notas", "muestra"];
    let interrupted = &interrupted;

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
            // NO llamamos a schema::init_db — la conexión principal ya creó
            // las tablas. Solo configuramos PRAGMAs.
            conn.execute_batch(
                "PRAGMA journal_mode=DELETE;
                 PRAGMA synchronous=NORMAL;
                 PRAGMA cache_size=-64000;",
            ).ok();
            // Verificar que el journal mode es DELETE
            if let Ok(mode) = conn.query_row::<String, _, _>(
                "PRAGMA journal_mode", [], |r| r.get(0),
            ) {
                log::debug!("Writer journal_mode: {}", mode);
            }

            for result in rx {
                let FileResult {
                    relative_str,
                    note_title,
                    note_metadata,
                    batch_entities,
                    ref chunk_data,
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
                    if let Err(e) = tx.execute(
                        "DELETE FROM edges WHERE (target_id = ?1 AND type = 'mentioned_in') OR (source_id = ?1 AND type IN ('tagged_with', 'belongs_to', 'co_occurs_with', 'mentioned_in'))",
                        rusqlite::params![old_id],
                    ) {
                        log::warn!("⚠️  Error limpiando edges viejos para {}: {}", relative_str, e);
                    }
                    if let Err(e) = tx.execute(
                        "DELETE FROM edges WHERE target_id = ?1 OR source_id = ?1",
                        rusqlite::params![old_id],
                    ) {
                        log::warn!("⚠️  Error limpiando edges viejos (2) para {}: {}", relative_str, e);
                    }
                }

                // Insertar/actualizar nodo nota por `key` (identidad = ruta)
                if let Err(e) = tx.execute(
                    "INSERT INTO nodes (key, label, type, metadata, embedding)
                     VALUES (?1, ?2, 'note', ?3, ?4)
                     ON CONFLICT(key) DO UPDATE SET
                       label = excluded.label,
                       metadata = excluded.metadata,
                       embedding = excluded.embedding",
                    rusqlite::params![
                        note_key(&relative_str),
                        note_title,
                        note_metadata,
                        None::<&[u8]>
                    ],
                ) {
                    log::warn!("⚠️  Error insertando nodo nota '{}': {}", note_title, e);
                    pb.lock().unwrap().inc(1);
                    continue;
                }

                // Obtener ID de la nota
                let note_node_id: i64 = match tx.query_row(
                    "SELECT id FROM nodes WHERE key = ?1",
                    rusqlite::params![note_key(&relative_str)],
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
                    let chunk_ref = if chunk_idx < chunk_data.len() && !chunk_data[chunk_idx].header.is_empty() {
                        format!("{}#{}", relative_str, chunk_data[chunk_idx].header)
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

                        if let Err(e) = tx.execute(
                            "INSERT INTO nodes (key, label, type, metadata, embedding)
                             VALUES (?1, ?2, ?3, ?4, ?5)
                             ON CONFLICT(key) DO UPDATE SET
                               metadata = excluded.metadata,
                               embedding = excluded.embedding",
                            rusqlite::params![node_key(&ent.label), ent.label, ent.type_, ent_metadata.to_string(), None::<&[u8]>],
                        ) {
                            log::warn!("⚠️  Error insertando entidad '{}': {}", ent.label, e);
                        }

                        if let Ok(ent_node_id) = tx.query_row::<i64, _, _>(
                            "SELECT id FROM nodes WHERE key = ?1",
                            rusqlite::params![node_key(&ent.label)],
                            |row| row.get(0),
                        ) {
                            if let Err(e) = tx.execute(
                                "INSERT OR IGNORE INTO edges (source_id, target_id, type, weight, context)
                                 VALUES (?1, ?2, 'mentioned_in', ?3, ?4)",
                                rusqlite::params![ent_node_id, note_node_id, ent.score, chunk_ref],
                            ) {
                                log::warn!("⚠️  Error insertando edge mentioned_in: {}", e);
                            }
                        }
                    }

                    // Co-ocurrencias dentro del mismo chunk
                    for i in 0..entities.len() {
                        for j in (i + 1)..entities.len() {
                            if entities[i].id == entities[j].id { continue; }
                            let weight = entities[i].score.min(entities[j].score);

                            if let (Ok(src_id), Ok(dst_id)) = (
                                tx.query_row::<i64, _, _>("SELECT id FROM nodes WHERE key = ?1",
                                    rusqlite::params![node_key(&entities[i].label)], |row| row.get(0)),
                                tx.query_row::<i64, _, _>("SELECT id FROM nodes WHERE key = ?1",
                                    rusqlite::params![node_key(&entities[j].label)], |row| row.get(0)),
                            ) {
                                if let Err(e) = tx.execute(
                                    "INSERT OR IGNORE INTO edges (source_id, target_id, type, weight, context)
                                     VALUES (?1, ?2, 'co_occurs_with', ?3, ?4)",
                                    rusqlite::params![src_id, dst_id, weight, chunk_ref],
                                ) {
                                    log::warn!("⚠️  Error insertando edge co_occurs_with: {}", e);
                                }
                            }
                        }
                    }
                }

                // Relaciones estructurales: directorio
                if !parent_dir.is_empty() && !skip_dirs.contains(&parent_dir.as_str()) {
                    if let Err(e) = tx.execute(
                        "INSERT INTO nodes (key, label, type, embedding) VALUES (?1, ?2, 'tag', ?3) ON CONFLICT(key) DO UPDATE SET embedding = excluded.embedding",
                        rusqlite::params![node_key(&parent_dir), parent_dir, None::<&[u8]>],
                    ) {
                        log::warn!("⚠️  Error insertando tag directorio '{}': {}", parent_dir, e);
                    }
                    if let Ok(dir_id) = tx.query_row::<i64, _, _>(
                        "SELECT id FROM nodes WHERE key = ?1",
                        rusqlite::params![node_key(&parent_dir)],
                        |row| row.get(0),
                    ) {
                        if let Err(e) = tx.execute(
                            "INSERT OR IGNORE INTO edges (source_id, target_id, type, weight, context)
                             VALUES (?1, ?2, 'belongs_to', 1.0, ?3)",
                            rusqlite::params![note_node_id, dir_id, relative_str],
                        ) {
                            log::warn!("⚠️  Error insertando edge belongs_to: {}", e);
                        }
                    }
                }

                // Relaciones estructurales: tags
                for tag in &tags {
                    if let Err(e) = tx.execute(
                        "INSERT INTO nodes (key, label, type, embedding) VALUES (?1, ?2, 'tag', ?3) ON CONFLICT(key) DO UPDATE SET embedding = excluded.embedding",
                        rusqlite::params![node_key(tag), tag, None::<&[u8]>],
                    ) {
                        log::warn!("⚠️  Error insertando tag '{}': {}", tag, e);
                    }
                    if let Ok(tag_id) = tx.query_row::<i64, _, _>(
                        "SELECT id FROM nodes WHERE key = ?1",
                        rusqlite::params![node_key(tag)],
                        |row| row.get(0),
                    ) {
                        if let Err(e) = tx.execute(
                            "INSERT OR IGNORE INTO edges (source_id, target_id, type, weight, context)
                             VALUES (?1, ?2, 'tagged_with', 1.0, ?3)",
                            rusqlite::params![note_node_id, tag_id, relative_str],
                        ) {
                            log::warn!("⚠️  Error insertando edge tagged_with: {}", e);
                        }
                    }
                }

                // Insertar chunks embedidos en la tabla `chunks`
                for cd in chunk_data {
                    if let Err(e) = crate::db::chunks::insert_chunk(
                        &tx, note_node_id, &cd.header, &cd.text, &cd.slug,
                        Some(&cd.embedding_blob), None,
                    ) {
                        log::warn!("⚠️  Error insertando chunk '{}': {}", cd.header, e);
                    }
                }

                // Commit de la transacción del archivo
                log::info!("  ▶ Guardando '{}' en BD...", relative_str);
                if let Err(e) = tx.commit() {
                    log::error!("Error haciendo commit writer para {}: {}", relative_str, e);
                    pb.lock().unwrap().inc(1);
                    continue;
                }
                log::info!("  ✓ Guardado '{}' en BD", relative_str);
                // Checkpoint TRUNCATE en la MISMA conexión del writer:
                // garantiza que los datos se escriban al DB principal
                // antes de que el writer entregue el control.
                let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");

                // Actualizar estadísticas compartidas
                {
                    let mut stats = stats_lock.lock().unwrap();
                    stats.files += 1;
                    stats.entities += all_entity_count;
                }

                pb.lock().unwrap().inc(1);

                // Verificar SIGINT después de commit
                if interrupted.load(Ordering::SeqCst) {
                    info!("Build interrumpido. Cortando...");
                    break;
                }

                let msg = format!("{} ({} chunks, {} entidades)", relative_str, chunk_data.len(), all_entity_count);
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
                    // Verificar interrupción antes de procesar cada archivo
                    if interrupted.load(Ordering::SeqCst) {
                        break;
                    }
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
                                        "⚠️  Error en NER batch [model={}] para {} (intento {}/3): {}",
                                        ner_model,
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

                    // Verificar interrupción después de NER
                    if interrupted.load(Ordering::SeqCst) {
                        break;
                    }

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

                    // Batch embed chunks via Ollama
                    let chunk_data: Vec<ChunkData> = {
                        if chunks.is_empty() {
                            Vec::new()
                        } else {
                            let embed_texts: Vec<String> = chunks
                                .iter()
                                .map(|c| format!("{}\n{}", c.header, c.text))
                                .collect();
                            let embed_refs: Vec<&str> =
                                embed_texts.iter().map(|s| s.as_str()).collect();

                            let ollama = OllamaClient::new(ollama_url, embed_model);
                            match ollama.batch_embed(&embed_refs) {
                                Ok(embs) => chunks
                                    .iter()
                                    .zip(embs.iter())
                                    .map(|(chunk, emb)| {
                                        let slug =
                                            slugify(&chunk.text.chars().take(80).collect::<String>());
                                        ChunkData {
                                            header: chunk.header.clone(),
                                            text: chunk.text.clone(),
                                            slug,
                                            embedding_blob: vector::vector_to_blob(emb),
                                        }
                                    })
                                    .collect(),
                                Err(e) => {
                                    log::warn!(
                                        "⚠️  Ollama embedding failed for {} (model '{}' at {}): {}. \
                                         File processed without chunk embeddings — vector search \
                                         for this file's content will not work until embeddings \
                                         are available.",
                                        relative_str,
                                        embed_model,
                                        ollama_url,
                                        e
                                    );
                                    Vec::new()
                                }
                            }
                        }
                    };

                    // Verificar interrupción después de embeddings
                    if interrupted.load(Ordering::SeqCst) {
                        break;
                    }

                    // Enviar al writer
                    let result = FileResult {
                        relative_str: relative_str.to_string(),
                        note_title,
                        note_metadata: note_metadata.to_string(),
                        batch_entities,
                        chunk_data,
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

    // ── Verificar interrupción por SIGINT ─────────────────────────────────
    if interrupted.load(Ordering::SeqCst) {
        pb.lock()
            .unwrap()
            .finish_with_message("⚠️ Build interrumpido por el usuario");
        eprintln!(
            "⚠️  Build interrumpido. {} archivos procesados. Continuando cierre limpio...",
            thread_stats.files
        );
        // NO llamamos a process::exit(130) aquí, dejamos que el conexión
        // se cierre limpiamente.
        // El writer ya hizo TRUNCATE checkpoint antes de romper el loop,
        // los datos están en el DB principal.
        let mut stats = thread_stats;
        // Consultar estadísticas reales de la BD antes de salir
        stats.total_nodes = conn
            .query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))
            .unwrap_or(0);
        stats.total_edges = conn
            .query_row("SELECT COUNT(*) FROM edges", [], |r| r.get(0))
            .unwrap_or(0);
        return Ok(stats);
    }

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
        prune_stale_notes(&conn, &ids_to_prune)?;
    }
    prune_spinner.finish_with_message(format!("✅ {} notas eliminadas del disco", pruned));

    // ── Limpiar entidades huérfanas ──────────────────────────────────────
    // Nodos que no son notas y no tienen ninguna arista → se eliminan.
    delete_orphan_entities(&conn)?;

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

/// Delete every entity-like node that is not referenced by any edge.
///
/// The `type NOT IN ('note', 'root', 'system')` predicate covers the remaining
/// entity-like node types (entity, tag, language, tool, …) without a hardcoded,
/// easily-diverging allow-list, while preserving the bootstrap nodes
/// (`_root`, `_graphrag_core`, `_unresolved`) that carry the `root`/`system`
/// types and may legitimately have no edges. Errors are propagated to the caller.
pub fn delete_orphan_entities(conn: &rusqlite::Connection) -> anyhow::Result<()> {
    conn.execute(
        "DELETE FROM nodes WHERE type NOT IN ('note', 'root', 'system')
         AND id NOT IN (SELECT source_id FROM edges UNION SELECT target_id FROM edges);",
        [],
    )
    .context("Error deleting orphan entity nodes")?;
    Ok(())
}

/// Delete stale notes (whose files no longer exist on disk) and their associated data.
/// Returns the number of pruned notes.
pub fn prune_stale_notes(
    conn: &rusqlite::Connection,
    ids_to_prune: &[(i64, String)],
) -> anyhow::Result<usize> {
    let pruned = ids_to_prune.len();
    if pruned > 0 {
        for (id, label) in ids_to_prune {
            conn.execute(
                "DELETE FROM edges WHERE source_id = ?1 OR target_id = ?1",
                rusqlite::params![id],
            )
            .with_context(|| format!("Error deleting edges for pruned note id={}", id))?;

            // Delete chunks referencing this note before deleting the node
            // to avoid FOREIGN KEY constraint failure.
            conn.execute(
                "DELETE FROM chunks WHERE note_id = ?1",
                rusqlite::params![id],
            )
            .with_context(|| format!("Error deleting chunks for pruned note id={}", id))?;

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

    Ok(pruned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;
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
    fn test_prune_stale_notes_fk_failure() {
        // Set up in-memory DB with schema (includes PRAGMA foreign_keys=ON)
        let conn = Connection::open_in_memory().unwrap();
        schema::init_db(&conn).unwrap();

        // Insert a note node
        conn.execute(
            "INSERT INTO nodes (id, key, label, type, metadata) VALUES (?1, ?2, ?3, 'note', ?4)",
            rusqlite::params![
                100,
                crate::db::keys::note_key("test-note"),
                "test-note",
                r#"{"path":"not/exists.md","slug":"test-note"}"#
            ],
        )
        .unwrap();

        // Insert 2 chunk rows referencing note_id=100
        conn.execute(
            "INSERT INTO chunks (note_id, header, text, slug) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![100, "Intro", "Test content one", "intro"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chunks (note_id, header, text, slug) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![100, "Details", "Test content two", "details"],
        )
        .unwrap();

        // Insert an edge referencing node_id=100
        conn.execute(
            "INSERT INTO edges (source_id, target_id, type, weight) VALUES (?1, ?2, 'test', 1.0)",
            rusqlite::params![100, 100],
        )
        .unwrap();

        // Call prune_stale_notes — should SUCCEED now because the fix deletes chunks first
        let result = prune_stale_notes(&conn, &[(100, "test-note".into())]);
        assert!(
            result.is_ok(),
            "GREEN: Expected success after fix — with DELETE FROM chunks before node deletion, \
             the FK constraint should not be violated. Got error: {:?}",
            result
        );

        // Verify the note node, its chunks, and its edges are all gone
        let note_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM nodes WHERE id = 100", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(note_count, 0, "note node should be deleted");

        let chunk_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM chunks WHERE note_id = 100", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(chunk_count, 0, "chunks should be deleted");

        let edge_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM edges WHERE source_id = 100 OR target_id = 100",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(edge_count, 0, "edges should be deleted");
    }

    #[test]
    fn test_delete_orphan_entities_removes_orphan_keeps_connected() {
        let conn = Connection::open_in_memory().unwrap();
        schema::init_db(&conn).unwrap();

        // A note acting as the anchor of the connected entity.
        conn.execute(
            "INSERT INTO nodes (id, key, label, type, metadata) VALUES (?1, ?2, ?3, 'note', '{}')",
            rusqlite::params![200, crate::db::keys::note_key("anchor"), "anchor"],
        )
        .unwrap();

        // Entity with NO edges → must be pruned.
        conn.execute(
            "INSERT INTO nodes (id, key, label, type, metadata) VALUES (?1, ?2, ?3, 'language', '{}')",
            rusqlite::params![201, "ent-orphan", "Orphan"],
        )
        .unwrap();

        // Bootstrap `system` node with NO edges → must be preserved.
        conn.execute(
            "INSERT INTO nodes (id, key, label, type, metadata) VALUES (?1, ?2, ?3, 'system', '{}')",
            rusqlite::params![203, "_graphrag_core", "_graphrag_core"],
        )
        .unwrap();

        // Bootstrap `root` node with NO edges → must be preserved.
        conn.execute(
            "INSERT INTO nodes (id, key, label, type, metadata) VALUES (?1, ?2, ?3, 'root', '{}')",
            rusqlite::params![204, "_root", "_root"],
        )
        .unwrap();

        // Entity WITH an edge → must survive.
        conn.execute(
            "INSERT INTO nodes (id, key, label, type, metadata) VALUES (?1, ?2, ?3, 'tool', '{}')",
            rusqlite::params![202, "ent-kept", "Kept"],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO edges (source_id, target_id, type, weight) VALUES (?1, ?2, 'mentioned_in', 1.0)",
            rusqlite::params![202, 200],
        )
        .unwrap();

        delete_orphan_entities(&conn).expect("orphan cleanup should succeed");

        let orphan: i64 = conn
            .query_row("SELECT COUNT(*) FROM nodes WHERE id = 201", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(orphan, 0, "orphan entity (no edges) should be deleted");

        let kept: i64 = conn
            .query_row("SELECT COUNT(*) FROM nodes WHERE id = 202", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(kept, 1, "entity with edges should remain");

        let note: i64 = conn
            .query_row("SELECT COUNT(*) FROM nodes WHERE id = 200", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(note, 1, "note nodes must never be pruned here");

        let system: i64 = conn
            .query_row("SELECT COUNT(*) FROM nodes WHERE id = 203", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(
            system, 1,
            "orphan `system` bootstrap node must be preserved"
        );

        let root: i64 = conn
            .query_row("SELECT COUNT(*) FROM nodes WHERE id = 204", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(root, 1, "orphan `root` bootstrap node must be preserved");
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

    // ── Tests for collect_existing_hashes ──────────────────────────────

    #[test]
    fn test_collect_existing_hashes_includes_matching() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::schema::init_db(&conn).unwrap();

        // Insert a note node with hash + path in metadata
        conn.execute(
            "INSERT INTO nodes (key, label, type, metadata) VALUES (?1, ?2, 'note', ?3)",
            rusqlite::params![
                crate::db::keys::note_key("test-note"),
                "test-note",
                r#"{"path": "docs/guide.md", "hash": "abc123def456", "content": "hello"}"#
            ],
        )
        .unwrap();

        let result = collect_existing_hashes(&conn).unwrap();

        assert_eq!(result.len(), 1, "should contain exactly one entry");
        let (hash, _id) = result
            .get("docs/guide.md")
            .expect("should contain key 'docs/guide.md'");
        assert_eq!(hash, "abc123def456", "hash should match");
    }

    #[test]
    fn test_collect_existing_hashes_excludes_legacy() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::schema::init_db(&conn).unwrap();

        // Insert a note node with metadata lacking hash and path (legacy format)
        conn.execute(
            "INSERT INTO nodes (key, label, type, metadata) VALUES (?1, ?2, 'note', ?3)",
            rusqlite::params![
                crate::db::keys::note_key("legacy-note"),
                "legacy-note",
                r#"{"slug": "foo", "content": "bar"}"#
            ],
        )
        .unwrap();

        let result = collect_existing_hashes(&conn).unwrap();

        assert!(
            result.is_empty(),
            "legacy note without hash/path should be excluded"
        );
    }

    #[test]
    fn test_wal_checkpoint_preserves_committed_data() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db_path = dir.path().join("test.db");
        let db_str = db_path.to_str().unwrap();

        // Connection 1: writer with WAL mode
        let conn1 = Connection::open(db_str)?;
        conn1.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        crate::db::schema::init_db(&conn1)?;

        // Insert a note in a transaction and commit
        conn1.execute(
            "INSERT INTO nodes (key, label, type, metadata) VALUES ('node:test-note', 'test-note', 'note', '{\"hash\":\"abc\",\"path\":\"test.md\"}')",
            [],
        )?;

        // Simulate WAL checkpoint (as SIGINT handler would do)
        conn1.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        drop(conn1);

        // Reopen and verify the data survives
        let conn2 = Connection::open(db_str)?;
        let count: i64 = conn2.query_row(
            "SELECT COUNT(*) FROM nodes WHERE label = 'test-note'",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(count, 1, "Data should survive WAL checkpoint + reopen");
        Ok(())
    }

    #[test]
    fn test_incremental_build_roundtrip() -> anyhow::Result<()> {
        // Create a temp directory with 3 .md files
        let dir = tempfile::tempdir()?;
        let db_path = dir.path().join("test.db");
        let db_str = db_path.to_str().unwrap().to_string();
        let repo_path = dir.path().join("notes");
        std::fs::create_dir_all(&repo_path)?;

        // Write 3 .md files
        let note1_path = repo_path.join("alpha.md");
        std::fs::write(
            &note1_path,
            "---\ntitle: Alpha\n---\n# Alpha\n\nContent about Python.\n",
        )?;

        let note2_path = repo_path.join("beta.md");
        std::fs::write(
            &note2_path,
            "---\ntitle: Beta\n---\n# Beta\n\nContent about Rust.\n",
        )?;

        let note3_path = repo_path.join("gamma.md");
        std::fs::write(
            &note3_path,
            "---\ntitle: Gamma\n---\n# Gamma\n\nContent about Go.\n",
        )?;

        // Compute hashes for all 3 files
        let compute_hash = |path: &std::path::Path| -> String {
            let text = std::fs::read_to_string(path).unwrap();
            let mut hasher = Sha256::new();
            hasher.update(text.as_bytes());
            let result = hasher.finalize();
            result
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>()
        };

        let hash1 = compute_hash(&note1_path);
        let hash2 = compute_hash(&note2_path);
        let hash3 = compute_hash(&note3_path);

        // Step 1: Insert notes into DB as if they were already processed
        // (simulating the state after a first successful build)
        let conn = Connection::open(&db_str)?;
        schema::init_db(&conn)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;

        let rel1 = "alpha.md";
        let rel2 = "beta.md";
        let rel3 = "gamma.md";

        conn.execute(
            "INSERT INTO nodes (key, label, type, metadata) VALUES ('node:Alpha', 'Alpha', 'note', ?1)",
            rusqlite::params![serde_json::json!({
                "path": rel1,
                "hash": hash1,
                "slug": "alpha",
                "content": "# Alpha\n\nContent about Python."
            })
            .to_string()],
        )?;
        conn.execute(
            "INSERT INTO nodes (key, label, type, metadata) VALUES ('node:Beta', 'Beta', 'note', ?1)",
            rusqlite::params![serde_json::json!({
                "path": rel2,
                "hash": hash2,
                "slug": "beta",
                "content": "# Beta\n\nContent about Rust."
            })
            .to_string()],
        )?;
        conn.execute(
            "INSERT INTO nodes (key, label, type, metadata) VALUES ('node:Gamma', 'Gamma', 'note', ?1)",
            rusqlite::params![serde_json::json!({
                "path": rel3,
                "hash": hash3,
                "slug": "gamma",
                "content": "# Gamma\n\nContent about Go."
            })
            .to_string()],
        )?;

        drop(conn);

        // Step 2: Verify collect_existing_hashes finds all 3 notes
        {
            let conn = Connection::open(&db_str)?;
            let existing = collect_existing_hashes(&conn)?;
            assert_eq!(existing.len(), 3, "Should find all 3 notes with hash/path");
            assert!(existing.contains_key("alpha.md"));
            assert!(existing.contains_key("beta.md"));
            assert!(existing.contains_key("gamma.md"));
        }

        // Step 3: Verify hashes match (files unchanged)
        {
            let conn = Connection::open(&db_str)?;
            let existing = collect_existing_hashes(&conn)?;
            assert_eq!(existing.len(), 3, "Should still find 3 notes");

            assert_eq!(
                existing.get("alpha.md").unwrap().0,
                compute_hash(&note1_path),
                "Hash for alpha should match"
            );
            assert_eq!(
                existing.get("beta.md").unwrap().0,
                compute_hash(&note2_path),
                "Hash for beta should match"
            );
            assert_eq!(
                existing.get("gamma.md").unwrap().0,
                compute_hash(&note3_path),
                "Hash for gamma should match"
            );
        }

        // Step 4: Modify one file
        std::fs::write(
            &note1_path,
            "---\ntitle: Alpha\n---\n# Alpha\n\nModified content about Python and Rust.\n",
        )?;
        let new_hash1 = compute_hash(&note1_path);
        assert_ne!(
            new_hash1, hash1,
            "Hash should be different after modification"
        );

        // Step 5: After modification, stored hash should differ from new file hash
        {
            let conn = Connection::open(&db_str)?;
            let existing = collect_existing_hashes(&conn)?;
            assert_eq!(existing.len(), 3, "Should still find 3 notes in DB");

            let (stored_hash, _) = existing.get("alpha.md").unwrap();
            assert_ne!(
                *stored_hash, new_hash1,
                "Stored hash should differ from new file hash"
            );
            // Stored hash should still equal the ORIGINAL hash
            assert_eq!(
                *stored_hash, hash1,
                "Stored hash should equal original hash"
            );
        }

        Ok(())
    }

    // ── node-identity by key (RED) ─────────────────────────────────────

    /// Simulate two note upserts sharing a label but with distinct keys.
    /// They must produce two nodes (no collision on title).
    #[test]
    fn test_upsert_note_by_key_distinct_titles() {
        let conn = Connection::open_in_memory().unwrap();
        schema::init_db(&conn).unwrap();

        let upsert = |key: &str, label: &str| {
            conn.execute(
                "INSERT INTO nodes (key, label, type, metadata, embedding)
                 VALUES (?1, ?2, 'note', '{}', NULL)
                 ON CONFLICT(key) DO UPDATE SET
                   label = excluded.label,
                   metadata = excluded.metadata,
                   embedding = excluded.embedding",
                rusqlite::params![key, label],
            )
            .unwrap();
        };

        upsert("note:a/dup.md", "Duplicado");
        upsert("note:b/dup.md", "Duplicado");

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM nodes WHERE label = 'Duplicado' AND type = 'note'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2, "distinct keys must yield two note nodes");
    }

    /// End-to-end convergence: two files with the same title.
    /// Requires a running Ollama server.
    #[test]
    #[ignore = "needs Ollama"]
    fn test_build_duplicate_titles_converges() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let repo = dir.path().join("repo");
        fs::create_dir_all(repo.join("a"))?;
        fs::create_dir_all(repo.join("b"))?;
        fs::write(
            repo.join("a/dup.md"),
            "---\ntitle: Duplicado\n---\n# A\n\nContenido uno sobre Python y Docker.\n",
        )?;
        fs::write(
            repo.join("b/dup.md"),
            "---\ntitle: Duplicado\n---\n# B\n\nContenido dos sobre Rust y SQLite.\n",
        )?;

        let db_path = dir.path().join("test.db");
        let db_str = db_path.to_str().unwrap();
        let repo_str = repo.to_str().unwrap();

        let stats1 = build_graph(
            repo_str,
            db_str,
            "http://localhost:11434",
            "llama3.2:3b",
            "nomic-embed-text",
            2,
        )?;
        assert_eq!(stats1.files, 2, "first build must process 2 files");

        {
            let conn = Connection::open(db_str)?;
            let notes: i64 =
                conn.query_row("SELECT COUNT(*) FROM nodes WHERE type = 'note'", [], |r| {
                    r.get(0)
                })?;
            assert_eq!(notes, 2, "two distinct note nodes must exist");
        }

        let stats2 = build_graph(
            repo_str,
            db_str,
            "http://localhost:11434",
            "llama3.2:3b",
            "nomic-embed-text",
            2,
        )?;
        assert_eq!(
            stats2.files, 0,
            "second build must report 0 files to process"
        );

        Ok(())
    }
}
