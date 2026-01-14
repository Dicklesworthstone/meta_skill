//! Hash embeddings (xf-style)
//!
//! Implements FNV-1a based hash embeddings for semantic similarity.
//! No ML model dependencies - fully deterministic.

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::config::SearchConfig;
use crate::error::{MsError, Result};

/// Pluggable embedding backend interface
pub trait Embedder {
    fn embed(&self, text: &str) -> Vec<f32>;
    fn dims(&self) -> usize;
}

/// Build an embedder from search config.
pub fn build_embedder(config: &SearchConfig) -> Result<Box<dyn Embedder>> {
    let backend = config.embedding_backend.trim().to_lowercase();
    let dims = config.embedding_dims as usize;
    if dims == 0 {
        return Err(MsError::Config(
            "search.embedding_dims must be greater than 0".to_string(),
        ));
    }

    match backend.as_str() {
        "" | "hash" => Ok(Box::new(HashEmbedder::new(dims))),
        "local" => Err(MsError::Config(
            "search.embedding_backend=local is not implemented yet".to_string(),
        )),
        "api" => Err(MsError::Config(
            "search.embedding_backend=api is not implemented yet".to_string(),
        )),
        other => Err(MsError::Config(format!(
            "unknown embedding backend: {other}"
        ))),
    }
}

/// Hash embedder using FNV-1a
pub struct HashEmbedder {
    /// Embedding dimension (default: 384)
    dim: usize,
}

impl Default for HashEmbedder {
    fn default() -> Self {
        Self { dim: 384 }
    }
}

impl HashEmbedder {
    /// Create embedder with specified dimension
    pub fn new(dim: usize) -> Self {
        Self { dim }
    }

    /// Embedding dimension
    pub fn dims(&self) -> usize {
        self.dim
    }
    
    /// Embed text into vector
    pub fn embed(&self, text: &str) -> Vec<f32> {
        if self.dim == 0 {
            return Vec::new();
        }

        let tokens = tokenize(text);
        let mut embedding = vec![0.0; self.dim];

        if tokens.is_empty() {
            return embedding;
        }

        for token in &tokens {
            accumulate_embedding(&mut embedding, token, 1.0);
        }

        for window in tokens.windows(2) {
            let bigram = format!("{} {}", window[0], window[1]);
            accumulate_embedding(&mut embedding, &bigram, 0.5);
        }

        l2_normalize(&mut embedding);
        embedding
    }
    
    /// Compute cosine similarity between two embeddings
    pub fn similarity(&self, a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }
        
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        
        if norm_a == 0.0 || norm_b == 0.0 {
            0.0
        } else {
            dot / (norm_a * norm_b)
        }
    }
}

impl Embedder for HashEmbedder {
    fn embed(&self, text: &str) -> Vec<f32> {
        HashEmbedder::embed(self, text)
    }

    fn dims(&self) -> usize {
        self.dim
    }
}

/// In-memory vector index for semantic search
pub struct VectorIndex {
    embeddings: HashMap<String, Vec<f32>>,
    dims: usize,
}

impl VectorIndex {
    /// Create a new empty vector index
    pub fn new(dims: usize) -> Self {
        Self {
            embeddings: HashMap::new(),
            dims,
        }
    }

    /// Current embedding dimension
    pub fn dims(&self) -> usize {
        self.dims
    }

    /// Number of embeddings stored
    pub fn len(&self) -> usize {
        self.embeddings.len()
    }

    /// Whether the index is empty
    pub fn is_empty(&self) -> bool {
        self.embeddings.is_empty()
    }

    /// Insert or replace an embedding
    pub fn insert(&mut self, skill_id: impl Into<String>, embedding: Vec<f32>) -> bool {
        if embedding.len() != self.dims {
            return false;
        }
        self.embeddings.insert(skill_id.into(), embedding);
        true
    }

    /// Remove an embedding by skill id
    pub fn remove(&mut self, skill_id: &str) -> Option<Vec<f32>> {
        self.embeddings.remove(skill_id)
    }

    /// Cosine similarity search (expects embeddings to be L2 normalized)
    pub fn search(&self, query_embedding: &[f32], limit: usize) -> Vec<(String, f32)> {
        if query_embedding.len() != self.dims {
            return Vec::new();
        }

        let mut scores: Vec<(String, f32)> = self
            .embeddings
            .iter()
            .map(|(id, emb)| (id.clone(), dot_product(query_embedding, emb)))
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
        scores.truncate(limit);
        scores
    }
}

fn tokenize(text: &str) -> Vec<String> {
    let lowered = text.to_lowercase();
    lowered
        .split(|c: char| !c.is_alphanumeric())
        .filter(|token| token.len() > 2)
        .map(|token| token.to_string())
        .collect()
}

fn accumulate_embedding(embedding: &mut [f32], token: &str, weight: f32) {
    let token_hash = fnv1a_hash(token.as_bytes());

    for i in 0..embedding.len() {
        let dim_hash = fnv1a_hash_with_salt(token_hash, i as u64);
        let sign = if dim_hash & 1 == 0 { weight } else { -weight };
        let dim = ((dim_hash >> 1) as usize) % embedding.len();
        embedding[dim] += sign;
    }
}

fn fnv1a_hash_with_salt(seed: u64, salt: u64) -> u64 {
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&seed.to_le_bytes());
    bytes[8..].copy_from_slice(&salt.to_le_bytes());
    fnv1a_hash(&bytes)
}

fn fnv1a_hash(data: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET;
    for byte in data {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn l2_normalize(vec: &mut [f32]) {
    let norm = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in vec.iter_mut() {
            *value /= norm;
        }
    }
}

fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fnv1a_hash_known_value() {
        assert_eq!(fnv1a_hash(b"hello"), 0xa430d84680aabd0b);
    }

    #[test]
    fn test_embedding_dimensions() {
        let embedder = HashEmbedder::new(64);
        let embedding = embedder.embed("git commit workflow");
        assert_eq!(embedding.len(), 64);
    }

    #[test]
    fn test_embedding_normalized() {
        let embedder = HashEmbedder::new(128);
        let embedding = embedder.embed("semantic search for skills");
        let norm = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-3);
    }

    #[test]
    fn test_embedding_empty_input() {
        let embedder = HashEmbedder::new(32);
        let embedding = embedder.embed("a an of in");
        let norm = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert_eq!(norm, 0.0);
    }

    #[test]
    fn test_similarity_prefers_related_text() {
        let embedder = HashEmbedder::new(64);
        let a = embedder.embed("git commit workflow");
        let b = embedder.embed("git commit messages");
        let c = embedder.embed("quantum entanglement photons");

        let sim_ab = embedder.similarity(&a, &b);
        let sim_ac = embedder.similarity(&a, &c);

        assert!(sim_ab > sim_ac);
    }

    #[test]
    fn test_vector_index_search() {
        // Use higher dimensions for more reliable similarity
        let embedder = HashEmbedder::new(128);
        let mut index = VectorIndex::new(128);
        index.insert("git".to_string(), embedder.embed("git commit workflow repository version"));
        index.insert("rust".to_string(), embedder.embed("rust error handling compile memory safety"));

        let query = embedder.embed("git commit workflow");
        let results = index.search(&query, 1);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "git");
    }

    #[test]
    fn test_embedding_normalized_random_inputs() {
        let embedder = HashEmbedder::new(64);
        let mut seed = 0x1234_5678_9abc_def0u64;

        for _ in 0..20 {
            let text = random_text(&mut seed, 6);
            let embedding = embedder.embed(&text);
            let norm = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
            assert!((norm - 1.0).abs() < 1e-3);
        }
    }

    fn random_text(seed: &mut u64, words: usize) -> String {
        let mut out = String::new();
        for i in 0..words {
            if i > 0 {
                out.push(' ');
            }
            let len = 3 + (next_u32(seed) % 6) as usize;
            out.push_str(&random_word(seed, len));
        }
        out
    }

    fn random_word(seed: &mut u64, len: usize) -> String {
        let mut out = String::with_capacity(len);
        for _ in 0..len {
            let ch = (b'a' + (next_u32(seed) % 26) as u8) as char;
            out.push(ch);
        }
        out
    }

    fn next_u32(seed: &mut u64) -> u32 {
        *seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1);
        (*seed >> 32) as u32
    }
}
