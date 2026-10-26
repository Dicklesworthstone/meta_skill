//! Local ML embedder (ONNX runtime)
//!
//! This module provides local ML-based embeddings using ONNX runtime.
//! Currently a placeholder - requires `ort` crate to be implemented.
//!
//! ## Future Implementation
//!
//! When implemented, this will:
//! - Load ONNX models (e.g., all-MiniLM-L6-v2)
//! - Tokenize input using `HuggingFace` tokenizers
//! - Run inference locally without network access
//! - Provide higher quality embeddings than hash-based
//!
//! ## Configuration
//!
//! ```toml
//! [search]
//! embedding_backend = "local"
//! model_path = "~/.cache/ms/models/all-MiniLM-L6-v2.onnx"
//! ```
//!
//! ## Dependencies Required
//!
//! To implement this backend, add to Cargo.toml:
//! ```toml
//! ort = { version = "2.0", features = ["download-binaries"] }
//! tokenizers = "0.15"
//! ```

use crate::error::{MsError, Result};
use crate::search::embeddings::{Embedder, accumulate_embedding, l2_normalize, tokenize};

/// Local subword and ONNX-ready embedder.
///
/// Provides morphology-aware local embeddings combining character 3-grams,
/// character 4-grams, word unigrams, and word bigrams via FNV-1a feature hashing,
/// while reporting `NotImplemented` when explicitly asked to load an `.onnx` model
/// without the `ort` runtime crate.
pub struct LocalEmbedder {
    dims: usize,
}

impl LocalEmbedder {
    /// Create a new local embedder from a model path or subword descriptor.
    ///
    /// # Errors
    ///
    /// Returns `NotImplemented` when given an `.onnx` model path without the `ort` feature.
    pub fn new(model_path: &std::path::Path, dims: usize) -> Result<Self> {
        let is_onnx = model_path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("onnx"));
        if is_onnx || model_path.to_string_lossy().contains(".onnx") {
            return Err(MsError::NotImplemented(
                "Local ONNX embeddings require the 'ort' crate. \
                 Use embedding_backend='hash' or 'api' instead."
                    .to_string(),
            ));
        }
        Ok(Self::with_subword(dims))
    }

    /// Create a local subword (char 3/4-gram + word 1/2-gram) embedder.
    #[must_use]
    pub const fn with_subword(dims: usize) -> Self {
        Self { dims }
    }

    /// Check if local ONNX runtime embeddings are available.
    #[must_use]
    pub const fn is_available() -> bool {
        false
    }
}

impl Embedder for LocalEmbedder {
    fn embed(&self, text: &str) -> Vec<f32> {
        if self.dims == 0 {
            return Vec::new();
        }

        let tokens = tokenize(text);
        let mut embedding = vec![0.0; self.dims];
        if tokens.is_empty() {
            return embedding;
        }

        for token in &tokens {
            accumulate_embedding(&mut embedding, token, 1.0);
            let bounded = format!("<{token}>");
            let chars: Vec<char> = bounded.chars().collect();
            for n in [3usize, 4usize] {
                if chars.len() >= n {
                    for window in chars.windows(n) {
                        let ngram: String = window.iter().collect();
                        accumulate_embedding(&mut embedding, &format!("cg:{ngram}"), 0.35);
                    }
                }
            }
        }

        for window in tokens.windows(2) {
            let bigram = format!("{} {}", window[0], window[1]);
            accumulate_embedding(&mut embedding, &bigram, 0.5);
        }

        l2_normalize(&mut embedding);
        embedding
    }

    fn dims(&self) -> usize {
        self.dims
    }

    fn name(&self) -> &'static str {
        "local"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn local_embedder_returns_not_implemented() {
        let result = LocalEmbedder::new(Path::new("/fake/model.onnx"), 384);
        assert!(result.is_err());
        if let Err(MsError::NotImplemented(msg)) = result {
            assert!(msg.contains("ort"));
        } else {
            panic!("Expected NotImplemented error");
        }
    }

    #[test]
    fn local_embedder_not_available() {
        assert!(!LocalEmbedder::is_available());
    }

    #[test]
    fn local_subword_embedder_produces_normalized_morphological_vectors() {
        let embedder = LocalEmbedder::with_subword(128);
        let v1 = embedder.embed("refactoring rust modules");
        let v2 = embedder.embed("refactor rust module");
        let v3 = embedder.embed("kubernetes ingress controller");

        let norm1: f32 = v1.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm1 - 1.0).abs() < 1e-3);

        let sim_12: f32 = v1.iter().zip(&v2).map(|(a, b)| a * b).sum();
        let sim_13: f32 = v1.iter().zip(&v3).map(|(a, b)| a * b).sum();
        assert!(
            sim_12 > sim_13,
            "morphologically related phrases should have higher cosine similarity"
        );
    }
}
