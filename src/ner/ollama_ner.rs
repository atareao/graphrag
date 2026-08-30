use anyhow::{Context, Result};
use log::debug;
use serde::{Deserialize, Serialize};

use crate::chunking::slugify;

/// Una entidad extraída del texto
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: String,
    pub label: String,
    pub type_: String,
    pub score: f64,
}

/// Etiquetas por defecto para extracción de entidades
pub const DEFAULT_LABELS: &[&str] = &[
    "technology",
    "framework",
    "programming language",
    "database",
    "tool",
    "person",
    "organization",
    "concept",
    "protocol",
    "operating system",
    "library",
    "format",
];

/// Envía un prompt al modelo Ollama y devuelve la respuesta cruda (response o thinking).
fn call_ollama(ollama_url: &str, model: &str, prompt: &str) -> Result<String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .context("Failed to create HTTP client")?;

    let body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "stream": false,
        "options": {
            "temperature": 0.1,
            "num_predict": 4096
        }
    });

    let resp = client
        .post(format!("{}/api/generate", ollama_url.trim_end_matches('/')))
        .json(&body)
        .send()
        .context("Error connecting to Ollama for entity extraction")?
        .error_for_status()
        .context("Ollama returned error during entity extraction")?;

    let resp_text = resp.text().context("Failed to read response body")?;
    let preview: String = resp_text.chars().take(200).collect();
    debug!(
        "NER respuesta cruda ({} chars): {}",
        resp_text.len(),
        preview
    );

    let data: OllamaResponse =
        serde_json::from_str(&resp_text).context("Failed to parse Ollama response JSON")?;

    // Algunos modelos (como qwen3.5) ponen la respuesta en 'thinking' en lugar de 'response'
    let raw = if !data.response.trim().is_empty() {
        data.response
    } else if !data.thinking.trim().is_empty() {
        debug!(
            "NER: response vacío, usando campo 'thinking' ({} chars)",
            data.thinking.len()
        );
        data.thinking
    } else {
        debug!("NER respuesta vacía para modelo '{}'.", model);
        return Ok(String::new());
    };

    Ok(raw)
}

#[derive(Deserialize)]
struct OllamaResponse {
    response: String,
    #[serde(default)]
    thinking: String,
}

/// Parsea una respuesta JSON cruda de Ollama en un Vec de valores JSON.
/// Intenta 3 estrategias: array directo, objeto con campo "entities"/"results", o regex.
fn parse_entities_json(raw: &str) -> Vec<serde_json::Value> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Vec::new();
    }

    // Limpiar posibles wrappers: ```json ... ```, ``` ... ```, o texto alrededor
    let cleaned = raw
        .strip_prefix("```json")
        .or_else(|| raw.strip_prefix("```"))
        .map(|s| s.strip_suffix("```").unwrap_or(s))
        .unwrap_or(raw)
        .trim();

    // Intento 1: el LLM devolvió un array JSON directamente
    if let Ok(entities) = serde_json::from_str::<Vec<serde_json::Value>>(cleaned) {
        return entities;
    }

    // Intento 2: el LLM devolvió un objeto con campo "entities" o "results"
    if let Ok(wrapper) = serde_json::from_str::<serde_json::Value>(cleaned) {
        return wrapper
            .get("entities")
            .or_else(|| wrapper.get("results"))
            .and_then(|arr| serde_json::from_value::<Vec<serde_json::Value>>(arr.clone()).ok())
            .unwrap_or_default();
    }

    // Intento 3: buscar un array JSON dentro del texto (modelos thinking o con markdown)
    // Usamos (?s) para que . cruce saltos de línea
    let re = regex::Regex::new(r"(?s)\[.*?\]").ok();
    if let Some(re) = re {
        if let Some(mat) = re.find(cleaned) {
            let json_str = mat.as_str();
            if let Ok(entities) = serde_json::from_str::<Vec<serde_json::Value>>(json_str) {
                return entities;
            }
        }
    }

    Vec::new()
}

/// Convierte un Vec de valores JSON crudos en `Vec<Entity>`, deduplicando y
/// filtrando por score >= 0.5. Si `section` está presente en el objeto,
/// se ignora (el caller lo usa para agrupar).
fn raw_entities_to_entities(raw_entities: Vec<serde_json::Value>) -> Vec<Entity> {
    let mut seen = std::collections::HashSet::new();
    let mut entities = Vec::new();

    for raw in raw_entities {
        let label = raw
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();

        if label.is_empty() {
            continue;
        }

        let key = label.to_lowercase();
        if seen.contains(&key) {
            continue;
        }

        let type_ = raw
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("concept")
            .to_string();

        let score = raw.get("score").and_then(|v| v.as_f64()).unwrap_or(0.5);

        if score < 0.5 {
            continue;
        }

        seen.insert(key);
        entities.push(Entity {
            id: slugify(&label),
            label,
            type_,
            score,
        });
    }

    entities
}

/// Extrae entidades de un texto usando Ollama LLM.
///
/// Envía un prompt estructurado al modelo solicitando extracción de entidades
/// en formato JSON. Esto reemplaza a GLiNER del original Python.
///
/// El prompt pide explícitamente que devuelva solo JSON válido para poder
/// parsear la respuesta de forma determinista.
#[allow(dead_code)]
pub fn extract_entities(
    ollama_url: &str,
    model: &str,
    text: &str,
    labels: &[&str],
) -> Result<Vec<Entity>> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }

    let labels_str = labels.join(", ");

    let prompt = format!(
        r#"Extract named entities from the following text. Only extract entities that match these categories: {labels_str}.

Return a JSON array of objects with fields:
- "label": the entity text exactly as written
- "type": one of the categories listed above
- "score": confidence score between 0.0 and 1.0 (be honest, use 0.5 if unsure)

Rules:
- Deduplicate: if the same entity appears multiple times, list it once with the highest score
- Return ONLY the raw JSON array. NO markdown, NO code fences, NO ```json, NO explanations.
- Start with [ and end with ]. Nothing else.
- If no entities found, return an empty array []

Text:
---
{text}
---"#,
        labels_str = labels_str,
        text = text,
    );

    let raw = call_ollama(ollama_url, model, &prompt)?;

    if raw.is_empty() {
        return Ok(Vec::new());
    }

    let raw_entities = parse_entities_json(&raw);

    let entities = raw_entities_to_entities(raw_entities);

    let preview: String = text.chars().take(40).collect();
    debug!(
        "NER extraídas: {} entidades de '{}'...",
        entities.len(),
        preview
    );
    Ok(entities)
}

/// Extrae entidades de MÚLTIPLES textos (chunks) en una SOLA llamada a Ollama.
///
/// Envía todos los textos separados por marcadores `---SECTION N---` y pide
/// al modelo que incluya un campo `"section"` en cada entidad. Luego agrupa
/// las entidades por sección en un `Vec<Vec<Entity>>`.
///
/// # Argumentos
///
/// * `ollama_url` — URL del servidor Ollama.
/// * `model` — Nombre del modelo Ollama.
/// * `texts` — Slice de textos (chunks) a procesar.
/// * `labels` — Categorías de entidades a extraer.
///
/// # Devuelve
///
/// Un `Vec<Vec<Entity>>` donde el índice 0 corresponde al primer texto,
/// el índice 1 al segundo, etc. Si un texto no tiene entidades, su vector
/// estará vacío.
pub fn extract_entities_batch(
    ollama_url: &str,
    model: &str,
    texts: &[&str],
    labels: &[&str],
) -> Result<Vec<Vec<Entity>>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }

    let labels_str = labels.join(", ");

    // Construir el prompt con todas las secciones
    let mut prompt_parts = Vec::new();
    prompt_parts.push(
        r#"Extract named entities from each section of the following text. Sections are separated by "---SECTION N---" markers.

Only extract entities that match these categories: {labels_str}

Return a JSON array of objects with fields:
- "label": the entity text exactly as written
- "type": one of the categories listed above
- "score": confidence score between 0.0 and 1.0 (be honest, use 0.5 if unsure)
- "section": the section number (1-based integer) this entity belongs to

Rules:
- Deduplicate within each section: if the same entity appears multiple times in a section, list it once with the highest score
- Return ONLY the raw JSON array. NO markdown, NO code fences, NO ```json, NO explanations, NO extra text.
- Start with [ and end with ]. Nothing else.
- If no entities found in any section, return an empty array []

Text:
---"#.replace("{labels_str}", &labels_str),
    );

    for (i, text) in texts.iter().enumerate() {
        prompt_parts.push(format!("---SECTION {}---\n{}", i + 1, text));
    }

    prompt_parts.push("---".to_string());

    let prompt = prompt_parts.join("\n");

    let raw = call_ollama(ollama_url, model, &prompt)?;

    if raw.is_empty() {
        return Ok(texts.iter().map(|_| Vec::new()).collect());
    }

    let raw_entities = parse_entities_json(&raw);

    // Agrupar entidades por sección
    let mut result: Vec<Vec<Entity>> = texts.iter().map(|_| Vec::new()).collect();

    // Primero, agrupar los raw JSON values por section para deduplicar
    // dentro de cada sección
    struct RawEntity {
        value: serde_json::Value,
        section: usize,
    }

    let raw_with_sections: Vec<RawEntity> = raw_entities
        .into_iter()
        .map(|v| {
            let section = v
                .get("section")
                .and_then(|s| s.as_i64())
                .map(|s| (s as usize).saturating_sub(1)) // 1-based → 0-based
                .unwrap_or(0);
            RawEntity { value: v, section }
        })
        .collect();

    // Agrupar por sección
    use std::collections::HashMap;
    let mut by_section: HashMap<usize, Vec<serde_json::Value>> = HashMap::new();
    for re in raw_with_sections {
        by_section.entry(re.section).or_default().push(re.value);
    }

    let num_sections = by_section.len();

    // Convertir cada grupo a Entities y colocarlo en el índice correspondiente
    for (section_idx, raw_group) in by_section {
        if section_idx < result.len() {
            let entities = raw_entities_to_entities(raw_group);
            result[section_idx] = entities;
        }
    }

    debug!(
        "NER batch: {} textos, {} secciones con entidades",
        texts.len(),
        num_sections,
    );

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_text_returns_empty() {
        let result = extract_entities("http://localhost:11434", "llama3", "", DEFAULT_LABELS);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_whitespace_text_returns_empty() {
        let result = extract_entities(
            "http://localhost:11434",
            "llama3",
            "   \n  ",
            DEFAULT_LABELS,
        );
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_slugify_is_accessible() {
        let id = slugify("FastAPI Framework");
        assert_eq!(id, "fastapi-framework");
    }

    #[test]
    fn test_entity_serde_roundtrip() {
        let entity = Entity {
            id: "python".to_string(),
            label: "Python".to_string(),
            type_: "programming language".to_string(),
            score: 0.95,
        };
        let json = serde_json::to_string(&entity).unwrap();
        let deserialized: Entity = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "python");
        assert_eq!(deserialized.label, "Python");
        assert_eq!(deserialized.type_, "programming language");
        assert!((deserialized.score - 0.95).abs() < f64::EPSILON);
    }

    #[test]
    fn test_default_labels_are_not_empty() {
        assert!(!DEFAULT_LABELS.is_empty());
        assert!(DEFAULT_LABELS.contains(&"technology"));
        assert!(DEFAULT_LABELS.contains(&"person"));
    }
}
