//! Search engine for skills
//!
//! Implements hybrid search: BM25 full-text + hash embeddings + RRF fusion.

use std::path::Path;

use crate::error::Result;

pub mod tantivy;
pub mod embeddings;
pub mod hybrid;
pub mod context;

/// Search index wrapper
pub struct SearchIndex {
    // TODO: Add tantivy index
    // TODO: Add embeddings store
    _path: std::path::PathBuf,
}

impl SearchIndex {
    /// Open or create search index at the given path
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        
        // Ensure directory exists
        std::fs::create_dir_all(path)?;
        
        Ok(Self {
            _path: path.to_path_buf(),
        })
    }
}
