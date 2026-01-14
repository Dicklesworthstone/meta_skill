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

pub mod context;
pub mod embeddings;
pub mod filters;
pub mod hybrid;
pub mod tantivy;
pub mod tantivy_index;

// Re-export main types
pub use context::{FilterResult, SearchContext, SearchFilters, SearchLayer};
pub use embeddings::{build_embedder, Embedder, HashEmbedder, VectorIndex};
pub use filters::{filter_hybrid_results, filter_skill_ids, matches_skill_record};
pub use hybrid::{fuse_results, fuse_simple, fuse_with_limit, HybridResult, RrfConfig};
pub use tantivy::{Bm25Index, Bm25Result};
pub use tantivy_index::SearchIndex;
