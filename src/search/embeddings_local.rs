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
use crate::search::embeddings::Embedder;

/// Local ONNX-based embedder (placeholder)
///
/// This struct exists as a placeholder for future ONNX integration.
/// When the `ort` crate is added, this will load and run local ML models.
pub struct LocalEmbedder {
    dims: usize,
}

impl LocalEmbedder {
    /// Create a new local embedder
    ///
    /// # Errors
    ///
    /// Currently always returns `NotImplemented` error.
    pub fn new(_model_path: &std::path::Path, _dims: usize) -> Result<Self> {
        Err(MsError::NotImplemented(
            "Local ONNX embeddings require the 'ort' crate. \
             Use embedding_backend='hash' or 'api' instead."
                .to_string(),
        ))
    }

    /// Check if local embeddings are available
    #[must_use] 
    pub const fn is_available() -> bool {
        false
    }
}

impl Embedder for LocalEmbedder {
    fn embed(&self, _text: &str) -> Vec<f32> {
        // This code is unreachable since new() always fails,
        // but we provide an implementation for trait completeness
        vec![0.0; self.dims]
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
}
