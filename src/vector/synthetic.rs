use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use log::debug;

use crate::vector;

/// Genera un embedding sintético determinista a partir de un texto.
///
/// Usa el hash del texto como seed para un generador congruencial lineal (LCG),
/// generando `dims` valores normalmente distribuidos (aproximación Box-Muller),
/// y los normaliza a longitud unitaria.
///
/// Esto permite demos sin Ollama: embeddings predecibles y reproductibles
/// donde textos similares producen vectores similares.
pub fn synthetic_embedding(text: &str, dims: usize) -> Vec<f32> {
    let seed = hash_text(text);
    let mut rng = SimpleRng::new(seed);

    // Generar números normales usando Box-Muller
    let mut vec = Vec::with_capacity(dims);
    let mut i = 0;
    while i < dims {
        let u1 = rng.next_f32();
        let u2 = rng.next_f32();
        let r = (-2.0 * u1.ln()).sqrt();
        let theta = 2.0 * std::f32::consts::PI * u2;
        if i < dims {
            vec.push(r * theta.cos());
            i += 1;
        }
        if i < dims {
            vec.push(r * theta.sin());
            i += 1;
        }
    }

    let preview: String = text.chars().take(40).collect();
    debug!("synthetic_embedding: text='{}'... → {}d, seed={}", preview, dims, seed);
    vector::normalize(&vec)
}

/// Deriva un embedding similar a otro, añadiendo ruido gaussiano
/// controlado por `variation` (0.0 = idéntico, 1.0 = muy diferente).
pub fn similar_embedding(base_text: &str, variation: f32, dims: usize) -> Vec<f32> {
    let base = synthetic_embedding(base_text, dims);
    if variation == 0.0 {
        return base;
    }

    let noise = synthetic_embedding(&format!("{}__noise__{}", base_text, variation), dims);
    let noise_scaled: Vec<f32> = noise.iter().map(|x| x * variation).collect();

    // Mezclar: base * (1-variation) + noise * variation
    let mixed: Vec<f32> = base
        .iter()
        .zip(noise_scaled.iter())
        .map(|(b, n)| b * (1.0 - variation) + n)
        .collect();

    vector::normalize(&mixed)
}

/// Calcula un hash u64 determinista a partir del texto.
fn hash_text(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Generador pseudoaleatorio LCG simple y determinista.
struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u32(&mut self) -> u32 {
        // LCG: x_{n+1} = (a * x_n + c) mod 2^64
        // Parámetros de Numerical Recipes
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.state >> 33) as u32
    }

    fn next_f32(&mut self) -> f32 {
        // Genera float en (0, 1)
        (self.next_u32() as f32 + 0.5) / 4_294_967_296.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_synthetic_is_deterministic() {
        let a = synthetic_embedding("Docker", 384);
        let b = synthetic_embedding("Docker", 384);
        assert_eq!(a, b);
    }

    #[test]
    fn test_synthetic_is_normalized() {
        let v = synthetic_embedding("seguridad en contenedores", 1024);
        let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((n - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_similar_embeddings_are_similar() {
        let a = synthetic_embedding("Docker", 384);
        let b = similar_embedding("Docker", 0.1, 384);
        let sim = vector::cosine_similarity(&a, &b);
        assert!(sim > 0.9, "similarity was {}", sim);
    }

    #[test]
    fn test_different_texts_different_vectors() {
        let a = synthetic_embedding("Python", 384);
        let b = synthetic_embedding("Rust", 384);
        let sim = vector::cosine_similarity(&a, &b);
        assert!(sim < 0.5, "similarity was {}", sim);
    }
}