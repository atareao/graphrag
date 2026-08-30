//! Generates a complete demo database with 20 notes, 24 entities,
//! and computed relationships using synthetic deterministic embeddings.
//!
//! This is the Rust equivalent of the original Python `seed_data.py`.
//! No Ollama or external embedding service is required — all vectors
//! are derived deterministically from the text content via
//! [`crate::vector::synthetic`].

use anyhow::{Context, Result};
use rusqlite::Connection;
use serde_json::json;
use log::debug;

use crate::db::schema;
use crate::vector::{self, synthetic};

/// Dimensionality of the synthetic embeddings.
const DIMS: usize = 1024;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// A note (document) that references one or more entities.
struct Nota {
    label: String,
    content: String,
    slug: String,
    entities: Vec<&'static str>,
}

/// An entity (concept, tool, language, …) in the knowledge graph.
struct Entidad {
    label: String,
    r#type: String,
    description: String,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Creates a demo SQLite database at `path` with 24 entities, 20 notes,
/// and ~80 edges (both `mentioned_in` and `co_occurs_with` relationships).
///
/// The database is freshly created — any existing file at `path` is
/// removed first.  The schema is initialised via [`schema::init_db`],
/// and then all nodes and edges are inserted inside a single transaction
/// for performance.
///
/// # Errors
///
/// Returns an error if any SQLite operation fails (permissions, disk
/// full, schema violation, …).
///
/// # Example
///
/// ```rust,ignore
/// use graphrag_seed::create_demo_db;
/// create_demo_db("/tmp/demo.graphrag.db")?;
/// ```
pub fn create_demo_db(path: &str) -> Result<()> {
    // Remove an existing database so we start clean.
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(format!("{path}-wal"));
    let _ = std::fs::remove_file(format!("{path}-shm"));

    let conn = Connection::open(path).context("failed to open database")?;
    schema::init_db(&conn).context("failed to initialise schema")?;

    // Wrap all inserts in a single transaction for performance.
    let tx = conn
        .unchecked_transaction()
        .context("failed to begin transaction")?;

    // =====================================================================
    // ENTITIES (24)
    // =====================================================================
    let entidades = vec![
        Entidad { label: "Docker".into(), r#type: "tool".into(), description: "Plataforma de contenedores".into() },
        Entidad { label: "Podman".into(), r#type: "tool".into(), description: "Alternativa daemonless a Docker".into() },
        Entidad { label: "Traefik".into(), r#type: "tool".into(), description: "Reverse proxy y load balancer".into() },
        Entidad { label: "Python".into(), r#type: "language".into(), description: "Lenguaje de programación".into() },
        Entidad { label: "Rust".into(), r#type: "language".into(), description: "Lenguaje de sistemas".into() },
        Entidad { label: "PostgreSQL".into(), r#type: "database".into(), description: "Base de datos relacional".into() },
        Entidad { label: "SQLite".into(), r#type: "database".into(), description: "Base de datos embebida".into() },
        Entidad { label: "SQLAlchemy".into(), r#type: "library".into(), description: "ORM para Python".into() },
        Entidad { label: "pandas".into(), r#type: "library".into(), description: "Análisis de datos en Python".into() },
        Entidad { label: "FastAPI".into(), r#type: "framework".into(), description: "Framework web para Python".into() },
        Entidad { label: "nginx".into(), r#type: "tool".into(), description: "Servidor web y proxy".into() },
        Entidad { label: "Redis".into(), r#type: "database".into(), description: "Base de datos en memoria".into() },
        Entidad { label: "Linux".into(), r#type: "os".into(), description: "Sistema operativo".into() },
        Entidad { label: "AppArmor".into(), r#type: "security".into(), description: "Módulo de seguridad Linux".into() },
        Entidad { label: "Seccomp".into(), r#type: "security".into(), description: "Filtro de syscalls".into() },
        Entidad { label: "Namespaces".into(), r#type: "concept".into(), description: "Aislamiento de procesos Linux".into() },
        Entidad { label: "Cgroups".into(), r#type: "concept".into(), description: "Control de recursos Linux".into() },
        Entidad { label: "Docker Compose".into(), r#type: "tool".into(), description: "Orquestación multi-contenedor".into() },
        Entidad { label: "Kubernetes".into(), r#type: "tool".into(), description: "Orquestador de contenedores".into() },
        Entidad { label: "Prometheus".into(), r#type: "tool".into(), description: "Sistema de monitorización".into() },
        Entidad { label: "Grafana".into(), r#type: "tool".into(), description: "Visualización de métricas".into() },
        Entidad { label: "LLM".into(), r#type: "concept".into(), description: "Modelos de lenguaje".into() },
        Entidad { label: "Embeddings".into(), r#type: "concept".into(), description: "Vectores semánticos".into() },
        Entidad { label: "GraphRAG".into(), r#type: "concept".into(), description: "Búsqueda híbrida con grafos".into() },
    ];

    for ent in &entidades {
        let emb = synthetic::synthetic_embedding(&ent.label, DIMS);
        let blob = vector::vector_to_blob(&emb);

        tx.execute(
            "INSERT INTO nodes (label, type, embedding, metadata) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                ent.label,
                ent.r#type,
                blob,
                json!({"description": ent.description}).to_string(),
            ],
        )
        .with_context(|| format!("failed to insert entity '{}'", ent.label))?;
    }
    debug!("Entidades insertadas: {}", entidades.len());

    // =====================================================================
    // NOTES (20)
    // =====================================================================
    let notas = vec![
        Nota {
            label: "Introducción a Docker".into(),
            slug: "Introducción a Docker".into(),
            content: "Docker es una plataforma de contenedores que permite empaquetar aplicaciones. Usa namespaces y cgroups de Linux para aislamiento.".into(),
            entities: vec!["Docker", "Linux", "Namespaces", "Cgroups"],
        },
        Nota {
            label: "Docker Compose: gestión multi-contenedor".into(),
            slug: "Docker Compose: gestión multi-contenedor".into(),
            content: "Docker Compose permite definir y ejecutar aplicaciones multi-contenedor. Se integra con Traefik como reverse proxy.".into(),
            entities: vec!["Docker", "Docker Compose", "Traefik"],
        },
        Nota {
            label: "Podman vs Docker".into(),
            slug: "Podman vs Docker".into(),
            content: "Podman es una alternativa a Docker que no requiere daemon. Es compatible con imágenes Docker y añade capacidades rootless.".into(),
            entities: vec!["Podman", "Docker", "Linux"],
        },
        Nota {
            label: "Traefik como reverse proxy".into(),
            slug: "Traefik como reverse proxy".into(),
            content: "Traefik es un reverse proxy moderno con auto-descubrimiento. Se integra con Docker, Kubernetes y Prometheus.".into(),
            entities: vec!["Traefik", "Docker", "Kubernetes", "Prometheus"],
        },
        Nota {
            label: "Seguridad en Docker".into(),
            slug: "Seguridad en Docker".into(),
            content: "Docker ofrece varias capas de seguridad: AppArmor, Seccomp, rootless mode y capabilities de Linux.".into(),
            entities: vec!["Docker", "AppArmor", "Seccomp", "Linux", "Namespaces"],
        },
        Nota {
            label: "Python para scripting".into(),
            slug: "Python para scripting".into(),
            content: "Python es ideal para automatización. Con SQLAlchemy y pandas puedes procesar datos y persistir en PostgreSQL o SQLite.".into(),
            entities: vec!["Python", "SQLAlchemy", "pandas", "PostgreSQL", "SQLite"],
        },
        Nota {
            label: "FastAPI: APIs modernas".into(),
            slug: "FastAPI: APIs modernas".into(),
            content: "FastAPI es un framework web moderno para Python. Se despliega típicamente con Docker y Traefik como proxy.".into(),
            entities: vec!["FastAPI", "Python", "Docker", "Traefik"],
        },
        Nota {
            label: "PostgreSQL avanzado".into(),
            slug: "PostgreSQL avanzado".into(),
            content: "PostgreSQL ofrece tipos avanzados, índices parciales y extensiones. Se puede migrar a SQLite para entornos más ligeros.".into(),
            entities: vec!["PostgreSQL", "SQLite"],
        },
        Nota {
            label: "SQLite con Python".into(),
            slug: "SQLite con Python".into(),
            content: "SQLite es la base de datos embebida perfecta para scripts Python. Se integra bien con pandas y SQLAlchemy.".into(),
            entities: vec!["SQLite", "Python", "pandas", "SQLAlchemy"],
        },
        Nota {
            label: "Kubernetes: orquestación".into(),
            slug: "Kubernetes: orquestación".into(),
            content: "Kubernetes orquesta contenedores a escala. Se integra con Prometheus para monitorización y Grafana para dashboards.".into(),
            entities: vec!["Kubernetes", "Docker", "Prometheus", "Grafana"],
        },
        Nota {
            label: "Monitorización con Prometheus".into(),
            slug: "Monitorización con Prometheus".into(),
            content: "Prometheus recopila métricas de servicios. Grafana visualiza esos datos en dashboards personalizados.".into(),
            entities: vec!["Prometheus", "Grafana"],
        },
        Nota {
            label: "Embeddings y RAG".into(),
            slug: "Embeddings y RAG".into(),
            content: "Los embeddings convierten texto en vectores. Combinados con grafos de conocimiento forman GraphRAG para búsqueda híbrida.".into(),
            entities: vec!["Embeddings", "LLM", "GraphRAG", "SQLite"],
        },
        Nota {
            label: "GraphRAG con SQLite".into(),
            slug: "GraphRAG con SQLite".into(),
            content: "GraphRAG almacena nodos, aristas y embeddings en SQLite con FTS5 para búsqueda híbrida local.".into(),
            entities: vec!["GraphRAG", "SQLite", "Embeddings", "LLM"],
        },
        Nota {
            label: "Rust vs Python".into(),
            slug: "Rust vs Python".into(),
            content: "Rust ofrece rendimiento nativo sin GC. Python prioriza velocidad de desarrollo. Ambos se complementan en el ecosistema moderno.".into(),
            entities: vec!["Rust", "Python"],
        },
        Nota {
            label: "AppArmor en Ubuntu".into(),
            slug: "AppArmor en Ubuntu".into(),
            content: "AppArmor restringe capacidades de programas mediante perfiles. Docker lo usa por defecto en Ubuntu.".into(),
            entities: vec!["AppArmor", "Docker", "Linux"],
        },
        Nota {
            label: "Hardening de imágenes Docker".into(),
            slug: "Hardening de imágenes Docker".into(),
            content: "Para hardening: usar imágenes minimalistas, evitar root, aplicar AppArmor y Seccomp, y escanear vulnerabilidades.".into(),
            entities: vec!["Docker", "AppArmor", "Seccomp", "Linux"],
        },
        Nota {
            label: "Redis como caché".into(),
            slug: "Redis como caché".into(),
            content: "Redis es una base de datos en memoria clave-valor. Se usa como caché con Python y FastAPI, a menudo en contenedores Docker.".into(),
            entities: vec!["Redis", "Python", "FastAPI", "Docker"],
        },
        Nota {
            label: "Nginx como proxy".into(),
            slug: "Nginx como proxy".into(),
            content: "Nginx sirve como proxy inverso y terminador SSL. Alternativa más ligera a Traefik para despliegues simples.".into(),
            entities: vec!["nginx", "Docker"],
        },
        Nota {
            label: "Capacidades de Linux".into(),
            slug: "Capacidades de Linux".into(),
            content: "Linux capabilities permiten granularidad de permisos sin root total. Docker las usa para contenedores más seguros.".into(),
            entities: vec!["Linux", "Docker", "Namespaces"],
        },
        Nota {
            label: "SQLAlchemy avanzado".into(),
            slug: "SQLAlchemy avanzado".into(),
            content: "SQLAlchemy ofrece ORM completo y Core SQL. Soporta PostgreSQL, SQLite y migraciones con Alembic.".into(),
            entities: vec!["SQLAlchemy", "Python", "PostgreSQL", "SQLite"],
        },
    ];

    for nota in &notas {
        // Embedding determinista con pequeña variación basada en el contenido
        let emb = synthetic::similar_embedding(&nota.content, 0.05, DIMS);
        let blob = vector::vector_to_blob(&emb);

        tx.execute(
            "INSERT INTO nodes (label, type, embedding, metadata) VALUES (?1, 'note', ?2, ?3)",
            rusqlite::params![
                nota.label,
                blob,
                json!({"path": nota.slug, "content": nota.content}).to_string(),
            ],
        )
        .with_context(|| format!("failed to insert note '{}'", nota.label))?;

        // Retrieve the auto-generated id for this note.
        let note_id: i64 = tx
            .query_row(
                "SELECT id FROM nodes WHERE label = ?1",
                rusqlite::params![nota.label],
                |row| row.get(0),
            )
            .with_context(|| format!("failed to find note id for '{}'", nota.label))?;

        // Edges: entity ──mentioned_in──> note
        for ent_label in &nota.entities {
            let ent_id: i64 = tx
                .query_row(
                    "SELECT id FROM nodes WHERE label = ?1",
                    rusqlite::params![ent_label],
                    |row| row.get(0),
                )
                .with_context(|| format!("failed to find entity id for '{ent_label}'"))?;

            tx.execute(
                "INSERT OR IGNORE INTO edges (source_id, target_id, type, weight)
                 VALUES (?1, ?2, 'mentioned_in', 1.0)",
                rusqlite::params![ent_id, note_id],
            )?;
        }
    }
    debug!("Notas insertadas: {}", notas.len());

    // =====================================================================
    // CO-OCCURRENCE EDGES between entities
    // =====================================================================
    let co_occurrences: Vec<(&str, &str, f64)> = vec![
        ("Docker", "Namespaces", 0.9), ("Docker", "Cgroups", 0.9),
        ("Docker", "AppArmor", 0.8), ("Docker", "Seccomp", 0.8),
        ("Docker", "Linux", 0.7), ("Docker", "Podman", 0.8),
        ("Docker", "Docker Compose", 0.9), ("Docker", "Kubernetes", 0.7),
        ("Docker", "Traefik", 0.7), ("Docker", "nginx", 0.5),
        ("Docker", "Redis", 0.5), ("Docker", "FastAPI", 0.6),
        ("Python", "FastAPI", 0.9), ("Python", "SQLAlchemy", 0.9),
        ("Python", "pandas", 0.9), ("Python", "SQLite", 0.7),
        ("Python", "PostgreSQL", 0.7), ("Python", "Redis", 0.6),
        ("Python", "Rust", 0.5),
        ("PostgreSQL", "SQLite", 0.6), ("PostgreSQL", "SQLAlchemy", 0.8),
        ("SQLite", "SQLAlchemy", 0.7), ("SQLite", "pandas", 0.5),
        ("FastAPI", "Traefik", 0.7), ("FastAPI", "Docker", 0.6),
        ("Traefik", "Kubernetes", 0.6), ("Traefik", "Prometheus", 0.5),
        ("Kubernetes", "Prometheus", 0.8), ("Kubernetes", "Grafana", 0.7),
        ("Prometheus", "Grafana", 0.9),
        ("AppArmor", "Seccomp", 0.8), ("AppArmor", "Linux", 0.7),
        ("Seccomp", "Linux", 0.7), ("Namespaces", "Cgroups", 0.8),
        ("Namespaces", "Linux", 0.7), ("Cgroups", "Linux", 0.7),
        ("LLM", "Embeddings", 0.9), ("LLM", "GraphRAG", 0.8),
        ("Embeddings", "GraphRAG", 0.9), ("GraphRAG", "SQLite", 0.7),
        ("Docker", "Embeddings", 0.3), ("Podman", "Namespaces", 0.7),
        ("nginx", "Traefik", 0.4),
    ];

    for (src, dst, weight) in &co_occurrences {
        let src_id: i64 = tx
            .query_row(
                "SELECT id FROM nodes WHERE label = ?1",
                rusqlite::params![src],
                |row| row.get(0),
            )
            .with_context(|| format!("failed to find src entity '{src}' for co-occurrence"))?;

        let dst_id: i64 = tx
            .query_row(
                "SELECT id FROM nodes WHERE label = ?1",
                rusqlite::params![dst],
                |row| row.get(0),
            )
            .with_context(|| format!("failed to find dst entity '{dst}' for co-occurrence"))?;

        tx.execute(
            "INSERT OR IGNORE INTO edges (source_id, target_id, type, weight)
             VALUES (?1, ?2, 'co_occurs_with', ?3)",
            rusqlite::params![src_id, dst_id, weight],
        )?;
    }
    debug!("Aristas co_occurs_with insertadas: {}", co_occurrences.len());

    // ── Repoblar FTS5 (sin triggers) ────────────────────────────────────
    // Insertar todos los nodos con contenido no vacío.
    conn.execute_batch(
        "DELETE FROM notes_fts;
         INSERT INTO notes_fts(rowid, title, content)
         SELECT id, label, COALESCE(json_extract(metadata, '$.content'), '')
         FROM nodes
         WHERE COALESCE(json_extract(metadata, '$.content'), '') != '';"
    )?;

    // Commit the transaction.
    tx.commit().context("failed to commit transaction")?;

    // Print summary.
    let total_nodes: i64 = conn
        .query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))
        .context("failed to count nodes")?;
    let total_edges: i64 = conn
        .query_row("SELECT COUNT(*) FROM edges", [], |r| r.get(0))
        .context("failed to count edges")?;

    println!("✅ Base de datos demo creada: {path}");
    println!("   Nodos: {total_nodes}, Aristas: {total_edges}");

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that `create_demo_db` produces the expected number of
    /// nodes and edges.
    #[test]
    fn demo_db_has_expected_counts() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();

        create_demo_db(path).unwrap();

        let conn = Connection::open(path).unwrap();

        let nodes: i64 = conn
            .query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))
            .unwrap();
        // 3 bootstrap nodes (_root, _graphrag_core, _unresolved)
        // + 24 entities + 20 notes = 47
        assert_eq!(nodes, 44, "expected 44 nodes (24 entities + 20 notes)");

        let edges: i64 = conn
            .query_row("SELECT COUNT(*) FROM edges", [], |r| r.get(0))
            .unwrap();
        // 1 bootstrap edge + ~20*avg_entities mentioned_in + 42 co_occurrences
        // Exact count depends on the data above, but assert a reasonable range.
        assert!(
            edges >= 80,
            "expected at least 80 edges, got {edges}"
        );
    }

    /// Every note should have a non-null embedding.
    #[test]
    fn all_notes_have_embeddings() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        create_demo_db(path).unwrap();

        let conn = Connection::open(path).unwrap();
        let mut stmt = conn
            .prepare("SELECT COUNT(*) FROM nodes WHERE type = 'note' AND embedding IS NULL")
            .unwrap();
        let null_embeddings: i64 = stmt.query_row([], |r| r.get(0)).unwrap();
        assert_eq!(null_embeddings, 0, "all notes must have embeddings");
    }

    /// Every entity should have a non-null embedding.
    #[test]
    fn all_entities_have_embeddings() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        create_demo_db(path).unwrap();

        let conn = Connection::open(path).unwrap();
        let mut stmt = conn
            .prepare("SELECT COUNT(*) FROM nodes WHERE type != 'note' AND type != 'root' AND type != 'system' AND embedding IS NULL")
            .unwrap();
        let null_embeddings: i64 = stmt.query_row([], |r| r.get(0)).unwrap();
        assert_eq!(null_embeddings, 0, "all entities must have embeddings");
    }

    /// Verify that the `mentioned_in` edges exist between entities and notes.
    #[test]
    fn mentioned_in_edges_exist() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        create_demo_db(path).unwrap();

        let conn = Connection::open(path).unwrap();
        let mut stmt = conn
            .prepare("SELECT COUNT(*) FROM edges WHERE type = 'mentioned_in'")
            .unwrap();
        let count: i64 = stmt.query_row([], |r| r.get(0)).unwrap();
        assert!(count > 0, "expected mentioned_in edges");
    }

    /// Verify that the `co_occurs_with` edges exist between entities.
    #[test]
    fn co_occurs_with_edges_exist() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        create_demo_db(path).unwrap();

        let conn = Connection::open(path).unwrap();
        let mut stmt = conn
            .prepare("SELECT COUNT(*) FROM edges WHERE type = 'co_occurs_with'")
            .unwrap();
        let count: i64 = stmt.query_row([], |r| r.get(0)).unwrap();
        assert_eq!(count, 43, "expected 43 co_occurs_with edges");
    }

    /// The FTS5 index should be populated for notes (repopulated after build).
    #[test]
    fn fts_index_contains_notes() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        create_demo_db(path).unwrap();

        let conn = Connection::open(path).unwrap();
        let mut stmt = conn
            .prepare("SELECT COUNT(*) FROM notes_fts")
            .unwrap();
        let count: i64 = stmt.query_row([], |r| r.get(0)).unwrap();
        // 20 notas con 'content' en metadata → 20 filas en FTS.
        assert_eq!(count, 20, "expected 20 rows in notes_fts (one per note), got {count}");
    }
}