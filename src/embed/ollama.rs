use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use log::debug;

/// Cliente HTTP para el API de embeddings de Ollama
pub struct OllamaClient {
    pub url: String,
    pub model: String,
    client: reqwest::blocking::Client,
}

// Ollama API response structure
#[derive(Deserialize)]
struct EmbeddingResponse {
    embedding: Vec<f32>,
}

impl OllamaClient {
    /// Crea un nuevo cliente Ollama
    /// - url: "http://localhost:11434"
    /// - model: "nomic-embed-text" o "bge-m3"
    pub fn new(url: &str, model: &str) -> Self {
        Self {
            url: url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            client: reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    /// Genera un embedding para el texto dado
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let body = serde_json::json!({
            "model": self.model,
            "prompt": text
        });

        let resp = self
            .client
            .post(format!("{}/api/embeddings", self.url))
            .json(&body)
            .send()
            .context("Error connecting to Ollama. Is it running?")?
            .error_for_status()
            .context(format!("Ollama API error for model '{}'", self.model))?;

        let data: EmbeddingResponse =
            resp.json().context("Failed to parse embedding response")?;
        let preview: String = text.chars().take(40).collect();
        debug!("Ollama embed: model={}, text='{}'... → {} dims",
               self.model, preview, data.embedding.len());

        if data.embedding.is_empty() {
            return Err(anyhow!("Ollama returned empty embedding for: {}", text));
        }

        Ok(data.embedding)
    }

    /// Verifica que Ollama esté accesible y el modelo exista
    pub fn health_check(&self) -> Result<()> {
        let resp = self
            .client
            .get(format!("{}/api/tags", self.url))
            .send()
            .context("Cannot reach Ollama. Start it with: ollama serve")?;
        debug!("Ollama health check: {} → {}", self.url, resp.status());

        if !resp.status().is_success() {
            return Err(anyhow!("Ollama returned status {}", resp.status()));
        }

        // Check that the model exists in the list
        #[derive(Deserialize)]
        struct ModelList {
            models: Vec<ModelInfo>,
        }
        #[derive(Deserialize)]
        struct ModelInfo {
            name: String,
        }

        let models: ModelList = resp.json().context("Failed to parse model list")?;
        debug!("Ollama modelos disponibles: {} encontrados. Buscando '{}'...",
               models.models.len(), self.model);
        let model_found = models
            .models
            .iter()
            .any(|m| m.name.starts_with(&self.model));

        if !model_found {
            return Err(anyhow!(
                "Model '{}' not found. Pull it with: ollama pull {}",
                self.model,
                self.model
            ));
        }

        Ok(())
    }

    /// Intenta obtener la dimensión del modelo de embeddings
    /// nomic-embed-text = 384, bge-m3 = 1024
    pub fn embedding_dimension(&self) -> usize {
        if self.model.contains("bge-m3") || self.model.contains("bge-large") {
            1024
        } else if self.model.contains("nomic-embed") {
            384
        } else if self.model.contains("mxbai") {
            1024
        } else {
            768 // default fallback
        }
    }
}