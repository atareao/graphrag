use regex::Regex;
use std::collections::HashMap;
use unicode_normalization::UnicodeNormalization;

/// Un chunk de texto de un documento Markdown
#[derive(Debug, Clone)]
pub struct Chunk {
    pub text: String,
    pub header: String,
    #[allow(dead_code)]
    pub metadata: HashMap<String, String>,
}

/// Normaliza un texto a slug ASCII (para usar como ID de nodo)
/// "FastAPI" → "fastapi", "PostgreSQL" → "postgresql"
pub fn slugify(text: &str) -> String {
    let normalized: String = text.nfkd().collect();
    let ascii: String = normalized
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || c.is_ascii_whitespace() || *c == '-')
        .collect();
    let lower = ascii.to_lowercase();
    let re = Regex::new(r"[^\w\s-]").unwrap();
    let cleaned = re.replace_all(&lower, "");
    let re2 = Regex::new(r"[-\s]+").unwrap();
    let slugged = re2.replace_all(&cleaned, "-");
    slugged.trim_matches('-').to_string()
}

/// Extrae el frontmatter YAML (entre ---) de un texto Markdown
/// Devuelve (metadata, cuerpo_sin_frontmatter)
pub fn parse_frontmatter(text: &str) -> (HashMap<String, String>, &str) {
    let mut metadata = HashMap::new();

    if !text.starts_with("---") {
        return (metadata, text);
    }

    // Buscar el cierre del frontmatter
    let rest = &text[3..];
    if let Some(end) = rest.find("\n---") {
        let fm_str = &rest[..end];
        // El cuerpo empieza después del cierre
        let body = &rest[end + 4..];
        // Si el cuerpo empieza con \n, saltarlo
        let body = body.strip_prefix('\n').unwrap_or(body);

        for line in fm_str.lines() {
            if let Some(col_idx) = line.find(':') {
                let key = line[..col_idx].trim().to_string();
                let val = line[col_idx + 1..].trim().to_string();
                metadata.insert(key, val);
            }
        }

        (metadata, body)
    } else {
        (metadata, text)
    }
}

/// Divide un documento Markdown en chunks por secciones (headers).
///
/// Reglas:
/// 1. Extrae y preserva el frontmatter como metadatos
/// 2. Cada header (# / ## / ###) inicia un nuevo chunk
/// 3. No rompe bloques de código (``` fences)
/// 4. Descarta fragmentos < 50 caracteres
pub fn chunk_document(text: &str) -> Vec<Chunk> {
    let (metadata, body) = parse_frontmatter(text);

    // Regex para headers markdown: #, ## o ### al inicio de línea
    let header_re = Regex::new(r"(?m)^(#{1,3})\s+(.+?)$").unwrap();

    let mut chunks: Vec<Chunk> = Vec::new();
    let _current_text = String::new();
    let mut current_header = String::new();
    let _last_end = 0;

    // Primera pasada: detectar bloques de código para no romperlos
    let code_fence_re = Regex::new(r"(?ms)^```").unwrap();
    let code_ranges: Vec<(usize, usize)> = {
        let mut ranges = Vec::new();
        let mut in_code = false;
        let mut code_start = 0;
        for mat in code_fence_re.find_iter(body) {
            if !in_code {
                code_start = mat.start();
                in_code = true;
            } else {
                ranges.push((code_start, mat.end()));
                in_code = false;
            }
        }
        ranges
    };

    fn is_in_code(pos: usize, code_ranges: &[(usize, usize)]) -> bool {
        code_ranges.iter().any(|&(s, e)| pos >= s && pos < e)
    }

    // Iterar por headers, saltando los que están dentro de bloques de código
    let matches: Vec<_> = header_re.find_iter(body).collect();
    let mut prev_end = 0;

    for mat in &matches {
        let header_start = mat.start();
        let header_text = mat.as_str().to_string();

        if is_in_code(header_start, &code_ranges) {
            continue;
        }

        // Guardar el texto antes de este header (el chunk anterior)
        let chunk_text = &body[prev_end..header_start];
        let trimmed = chunk_text.trim();
        if !trimmed.is_empty() && trimmed.len() >= 50 {
            chunks.push(Chunk {
                text: trimmed.to_string(),
                header: current_header.clone(),
                metadata: metadata.clone(),
            });
        }

        current_header = header_text;
        prev_end = mat.end();
    }

    // Último chunk: todo después del último header
    let remaining = &body[prev_end..];
    let trimmed = remaining.trim();
    if !trimmed.is_empty() && trimmed.len() >= 50 {
        chunks.push(Chunk {
            text: trimmed.to_string(),
            header: current_header.clone(),
            metadata: metadata.clone(),
        });
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("FastAPI"), "fastapi");
        assert_eq!(slugify("PostgreSQL"), "postgresql");
        assert_eq!(slugify("Sebastián Ramírez"), "sebastian-ramirez");
        assert_eq!(slugify("Linux capabilities"), "linux-capabilities");
    }

    #[test]
    fn test_parse_frontmatter() {
        let text = "---\ntitle: Mi Nota\ntags: docker, rust\n---\n\n# Contenido";
        let (meta, body) = parse_frontmatter(text);
        assert_eq!(meta.get("title").unwrap(), "Mi Nota");
        assert_eq!(meta.get("tags").unwrap(), "docker, rust");
        assert!(body.contains("# Contenido"));
    }

    #[test]
    fn test_chunk_by_headers() {
        let text = "# Intro\n\nPrimer párrafo con suficiente texto para superar el mínimo de 50 caracteres.\n\n## Sección 1\n\nMás contenido aquí que también es bastante largo para que no se descarte.\n\n### Subsección\n\nY aquí otro bloque con suficiente texto para que no se descarte por corto.\n";
        let chunks = chunk_document(text);
        assert!(
            chunks.len() >= 2,
            "Expected at least 2 chunks, got {}",
            chunks.len()
        );
    }

    #[test]
    fn test_does_not_split_code_blocks() {
        let text = "# Title\n\nSome text.\n\n```\n# This is not a real header\nfn main() {}\n```\n\n## Real section\n\nMore text.\n";
        let chunks = chunk_document(text);
        // The # inside ``` should not be treated as a header
        assert_eq!(
            chunks.len(),
            1,
            "Expected 1 section header, got {} chunks",
            chunks.len()
        );
    }
}
