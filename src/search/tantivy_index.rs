//! Tantivy search index (re-exports from tantivy module)
//!
//! This module provides backward compatibility. Use `search::tantivy::Bm25Index` directly.

pub use super::tantivy::{Bm25Index, Bm25Result};

/// A single search result (alias for `Bm25Result`)
pub type SearchResult = Bm25Result;

/// `SearchIndex` is an alias for `Bm25Index`
pub type SearchIndex = Bm25Index;
