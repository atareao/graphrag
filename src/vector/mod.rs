pub mod synthetic;

use anyhow::Result;

/// Convierte un vector de floats a BLOB binario (float32 LE)
pub fn vector_to_blob(v: &[f32]) -> Vec<u8> {
    bytemuck::cast_slice(v).to_vec()
}

/// Convierte un BLOB binario a vector de floats
pub fn blob_to_vector(blob: &[u8]) -> Result<Vec<f32>> {
    let v = bytemuck::cast_slice::<u8, f32>(blob).to_vec();
    Ok(v)
}

/// Producto escalar entre dos vectores
pub fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// Norma L2 de un vector
pub fn norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

/// Normaliza un vector a longitud unitaria
pub fn normalize(v: &[f32]) -> Vec<f32> {
    let n = norm(v);
    if n == 0.0 {
        v.to_vec()
    } else {
        v.iter().map(|x| x / n).collect()
    }
}

/// Similitud coseno entre dos vectores normalizados
/// Si no están normalizados, usa cosine_similarity_raw
#[allow(dead_code)]
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    dot(a, b)
}

/// Similitud coseno sin asumir normalización
#[allow(dead_code)]
pub fn cosine_similarity_raw(a: &[f32], b: &[f32]) -> f32 {
    let na = norm(a);
    let nb = norm(b);
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot(a, b) / (na * nb)
}
