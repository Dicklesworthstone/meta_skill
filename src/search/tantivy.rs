//! Tantivy BM25 full-text search

use crate::error::Result;

/// BM25 search index using Tantivy
pub struct Bm25Index {
    // TODO: Add tantivy index and schema
}

impl Bm25Index {
    /// Search skills by query
    pub fn search(&self, _query: &str, _limit: usize) -> Result<Vec<String>> {
        // TODO: Implement BM25 search
        Ok(vec![])
    }
}
