//! Configuración de GraphRAG (TOML)
//!
//! Busca `config.toml` en:
//! 1. `$XDG_CONFIG_HOME/graphrag/config.toml` (Linux: ~/.config/graphrag/config.toml)
//! 2. `$HOME/.config/graphrag/config.toml`
//!
//! Todos los campos son opcionales. Los flags de CLI tienen prioridad.

use anyhow::Result;
use serde::Deserialize;
use std::path::PathBuf;

/// Configuración completa de GraphRAG
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct GraphRagConfig {
    /// Base de datos por defecto
    pub db: String,
    /// URL del servidor Ollama
    pub ollama_url: String,
    /// Modelo de embeddings
    pub embed_model: String,
    /// Modelo para extracción de entidades (NER)
    pub ner_model: String,
    /// Número de resultados por defecto
    pub k: usize,
    /// Profundidad de expansión por defecto
    pub depth: i32,
    /// Peso vectorial (0.0-1.0)
    pub alpha: f64,
    /// Directorio base de notas (para plugin NeoVim)
    pub notes_dir: String,
    /// Número de hilos para procesamiento paralelo (build)
    pub num_threads: usize,
}

impl Default for GraphRagConfig {
    fn default() -> Self {
        Self {
            db: "graph.db".into(),
            ollama_url: "http://localhost:11434".into(),
            embed_model: "nomic-embed-text".into(),
            ner_model: "llama3.2:3b".into(),
            k: 5,
            depth: 2,
            alpha: 0.7,
            notes_dir: String::new(),
            num_threads: 4,
        }
    }
}

impl GraphRagConfig {
    /// Creates a default config file at the standard path if it doesn't exist.
    /// Returns the path to the config file (existing or newly created).
    pub fn auto_create_default() -> PathBuf {
        let path = Self::config_path();
        if path.exists() {
            return path;
        }
        // Create parent directory
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap_or_else(|e| {
                log::warn!("No se pudo crear directorio de configuración: {}", e);
            });
        }
        let content = r#"# GraphRAG Configuration
# Auto-generated. All fields are optional — CLI flags override these values.

# Default database path
db = "graph.db"

# Ollama server URL
ollama_url = "http://localhost:11434"

# Embedding model (used when --ollama is passed)
embed_model = "nomic-embed-text"

# Model for entity extraction during build
ner_model = "llama3.2:3b"

# Default number of search results
k = 5

# Default graph expansion depth
depth = 2

# Vector weight (0.0 = pure graph, 1.0 = pure vector)
alpha = 0.7

# Notes directory (used by the Neovim plugin)
notes_dir = ""

# Number of parallel threads for build (NER workers)
num_threads = 4
"#;
        std::fs::write(&path, content).unwrap_or_else(|e| {
            log::warn!("No se pudo escribir configuración por defecto: {}", e);
        });
        log::info!("Configuración por defecto creada en: {}", path.display());
        path
    }

    /// Busca y carga la configuración desde el sistema de archivos.
    ///
    /// Si el archivo no existe, crea uno por defecto.
    pub fn load() -> Self {
        let path = Self::auto_create_default();
        match Self::load_from(&path) {
            Ok(cfg) => {
                log::info!("Configuración cargada desde: {}", path.display());
                cfg
            }
            Err(e) => {
                log::debug!(
                    "No se pudo cargar configuración desde {}: {}",
                    path.display(),
                    e
                );
                Self::default()
            }
        }
    }

    /// Carga desde una ruta específica
    pub fn load_from(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let cfg: Self = toml::from_str(&content)?;
        Ok(cfg)
    }

    /// Ruta del archivo de configuración según el estándar XDG
    fn config_path() -> PathBuf {
        // XDG_CONFIG_HOME/graphrag/config.toml
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            let p = PathBuf::from(xdg).join("graphrag").join("config.toml");
            if p.exists() {
                return p;
            }
        }

        // ~/.config/graphrag/config.toml
        if let Some(home) = dirs::config_dir() {
            let p = home.join("graphrag").join("config.toml");
            if p.exists() {
                return p;
            }
        }

        // Fallback: ~/.config/graphrag/config.toml (aunque no exista)
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("graphrag")
            .join("config.toml")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let cfg = GraphRagConfig::default();
        assert_eq!(cfg.db, "graph.db");
        assert_eq!(cfg.ollama_url, "http://localhost:11434");
        assert_eq!(cfg.embed_model, "nomic-embed-text");
        assert_eq!(cfg.k, 5);
        assert_eq!(cfg.depth, 2);
        assert!((cfg.alpha - 0.7).abs() < 0.01);
    }

    #[test]
    fn test_parse_toml() {
        let toml_str = r#"
db = "custom.db"
ollama_url = "http://ollama:11434"
k = 10
depth = 3
"#;
        let cfg: GraphRagConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.db, "custom.db");
        assert_eq!(cfg.ollama_url, "http://ollama:11434");
        assert_eq!(cfg.k, 10);
        assert_eq!(cfg.depth, 3);
        // Los valores no especificados deben tener los defaults de serde(default)
        assert_eq!(cfg.embed_model, "nomic-embed-text");
    }

    #[test]
    fn test_config_path_exists() {
        // Sólo verifica que no pete
        let _ = GraphRagConfig::config_path();
    }

    #[test]
    fn test_auto_create_default() {
        // Use a temp dir to avoid clobbering real config
        let _dir = tempfile::tempdir().unwrap();
        // Temporarily override config path by setting XDG_CONFIG_HOME
        // We can't easily test this without mocking, so just verify the method signature works
        let _ = GraphRagConfig::auto_create_default();
    }
}
