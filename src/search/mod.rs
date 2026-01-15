//! Search engine for skills
//!
//! Implements hybrid search: BM25 full-text + hash embeddings + RRF fusion.
//!
//! ## Architecture
//!
//! ```text
//! ┌────────────────────────────────────────────────────────────────┐
//! │                        Search Query                            │
//! └────────────────────────────────────────────────────────────────┘
//!                     │                          │
//!                     ▼                          ▼
//! ┌──────────────────────────────┐  ┌──────────────────────────────┐
//! │       Bm25Index              │  │       VectorIndex            │
//! │   (Tantivy BM25 search)      │  │   (Hash embeddings)          │
//! └──────────────────────────────┘  └──────────────────────────────┘
//!                     │                          │
//!                     └──────────┬───────────────┘
//!                                ▼
//!                ┌───────────────────────────────┐
//!                │   RRF Fusion (hybrid.rs)      │
//!                └───────────────────────────────┘
//!                                │
//!                                ▼
//!                     Combined ranked results
//! ```
//!
//! ## Caching
//!
//! The `cache` module provides LRU caching for query results and embeddings
//! to reduce latency for repeated operations. See `CacheLayer` for details.

pub mod cache;
pub mod context;
pub mod embeddings;
pub mod embeddings_local;
pub mod filters;
pub mod hybrid;
pub mod tantivy;
pub mod tantivy_index;

// Re-export main types
pub use cache::{CacheLayer, CacheStats, CachedQueryResult, SessionFingerprint};
pub use context::{FilterResult, SearchContext, SearchFilters, SearchLayer};
pub use embeddings::{ApiEmbedder, Embedder, HashEmbedder, VectorIndex, build_embedder};
pub use embeddings_local::LocalEmbedder;
pub use filters::{filter_hybrid_results, filter_skill_ids, matches_skill_record};
pub use hybrid::{HybridResult, RrfConfig, fuse_results, fuse_simple, fuse_with_limit};
pub use tantivy::{Bm25Index, Bm25Result};
pub use tantivy_index::SearchIndex;
