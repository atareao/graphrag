use anyhow::{anyhow, Context, Result};
use log::debug;
use serde::Deserialize;

/// Cliente HTTP para el API de embeddings de Ollama
#[derive(Clone)]
pub struct OllamaClient {
    pub url: String,
    pub model: String,
    client: reqwest::blocking::Client,
    #[allow(dead_code)]
    generate_client: reqwest::blocking::Client,
}

// Ollama API response structures
#[derive(Deserialize)]
struct EmbeddingResponse {
    embedding: Vec<f32>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct GenerateResponse {
    response: String,
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
            generate_client: reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .expect("Failed to create HTTP generate client"),
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

        let data: EmbeddingResponse = resp.json().context("Failed to parse embedding response")?;
        let preview: String = text.chars().take(40).collect();
        debug!(
            "Ollama embed: model={}, text='{}'... → {} dims",
            self.model,
            preview,
            data.embedding.len()
        );

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
        debug!(
            "Ollama modelos disponibles: {} encontrados. Buscando '{}'...",
            models.models.len(),
            self.model
        );
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

    /// Genera embeddings para múltiples textos en lote.
    ///
    /// Llama a `/api/embeddings` para cada texto individualmente,
    /// compatible con todas las versiones de Ollama.
    pub fn batch_embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        texts.iter().map(|t| self.embed(t)).collect()
    }

    /// Intenta obtener la dimensión del modelo de embeddings
    /// nomic-embed-text = 384, bge-m3 = 1024
    #[allow(dead_code)]
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

    /// Generate text via Ollama's /api/generate endpoint.
    ///
    /// Sends a prompt to the specified model and returns the generated response text.
    /// Use `format = Some("json")` to enable Ollama's JSON mode for structured output.
    ///
    /// # Arguments
    /// * `prompt` - The full prompt text to send to the model
    /// * `model` - Which model to use (e.g., "llama3.2:3b")
    /// * `format` - Optional — pass `"json"` as Some("json") to enable JSON mode
    #[allow(dead_code)]
    pub fn generate(&self, prompt: &str, model: &str, format: Option<&str>) -> Result<String> {
        let format_value = match format {
            Some("json") => serde_json::json!("json"),
            _ => serde_json::Value::Null,
        };

        let body = serde_json::json!({
            "model": model,
            "prompt": prompt,
            "stream": false,
            "format": format_value,
        });

        let resp = self
            .generate_client
            .post(format!("{}/api/generate", self.url))
            .json(&body)
            .send()
            .context("Error connecting to Ollama for generation. Is it running?")?
            .error_for_status()
            .context(format!("Ollama generate API error for model '{}'", model))?;

        let data: GenerateResponse = resp.json().context("Failed to parse generate response")?;

        let preview: String = prompt.chars().take(60).collect();
        debug!(
            "Ollama generate: model={}, prompt='{}...' → {} chars response",
            model,
            preview,
            data.response.len()
        );

        Ok(data.response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "needs Ollama"]
    fn test_batch_embed_returns_correct_count() {
        let client = OllamaClient::new("http://localhost:11434", "nomic-embed-text");
        let texts = &["Hello world", "Rust programming language"];
        let result = client
            .batch_embed(texts)
            .expect("batch_embed should succeed");
        assert_eq!(result.len(), 2, "should return one embedding per text");
        for emb in &result {
            assert!(!emb.is_empty(), "each embedding should be non-empty");
        }
    }
}
