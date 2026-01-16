//! RRF (Reciprocal Rank Fusion) for hybrid search
//!
//! Combines BM25 full-text search with semantic vector search using
//! Reciprocal Rank Fusion, a simple and effective method for merging
//! ranked lists from different retrieval systems.
//!
//! ## Algorithm
//!
//! RRF score for document d:
//! ```text
//! RRF(d) = Σ weight_i / (k + rank_i(d))
//! ```
//!
//! Where:
//! - k is a smoothing constant (typically 60)
//! - `rank_i(d)` is the 1-indexed position of d in list i
//! - `weight_i` is the importance weight for list i

use std::collections::HashMap;

/// Reciprocal Rank Fusion configuration
#[derive(Debug, Clone)]
pub struct RrfConfig {
    /// K parameter (smoothing constant, default: 60)
    /// Higher values reduce the impact of rank differences
    pub k: f32,
    /// Weight for BM25 (lexical) results
    pub bm25_weight: f32,
    /// Weight for semantic (vector) results
    pub semantic_weight: f32,
}

impl Default for RrfConfig {
    fn default() -> Self {
        Self {
            k: 60.0,
            bm25_weight: 1.0,
            semantic_weight: 1.0,
        }
    }
}

impl RrfConfig {
    /// Create config with custom k value
    #[must_use] 
    pub fn with_k(k: f32) -> Self {
        Self {
            k,
            ..Default::default()
        }
    }

    /// Create config with custom weights
    #[must_use] 
    pub fn with_weights(bm25_weight: f32, semantic_weight: f32) -> Self {
        Self {
            bm25_weight,
            semantic_weight,
            ..Default::default()
        }
    }
}

/// A single hybrid search result
#[derive(Debug, Clone)]
pub struct HybridResult {
    /// Skill ID
    pub skill_id: String,
    /// Combined RRF score
    pub score: f32,
    /// BM25 rank (1-indexed, None if not in BM25 results)
    pub bm25_rank: Option<usize>,
    /// Semantic rank (1-indexed, None if not in semantic results)
    pub semantic_rank: Option<usize>,
    /// Original BM25 score (if present)
    pub bm25_score: Option<f32>,
    /// Original semantic score (if present)
    pub semantic_score: Option<f32>,
}

/// Fuse BM25 and semantic results using Reciprocal Rank Fusion
///
/// Both input lists are expected to be sorted by score (descending).
/// The output is sorted by combined RRF score (descending).
///
/// # Arguments
///
/// * `bm25_results` - BM25 results as (`skill_id`, score) pairs
/// * `semantic_results` - Semantic results as (`skill_id`, score) pairs
/// * `config` - RRF configuration parameters
///
/// # Returns
///
/// Combined results sorted by RRF score
#[must_use] 
pub fn fuse_results(
    bm25_results: &[(String, f32)],
    semantic_results: &[(String, f32)],
    config: &RrfConfig,
) -> Vec<HybridResult> {
    let mut scores: HashMap<String, HybridResult> = HashMap::new();

    // Process BM25 results
    for (rank, (skill_id, score)) in bm25_results.iter().enumerate() {
        let rank_1_indexed = rank + 1;
        let rrf_contribution = config.bm25_weight / (config.k + rank_1_indexed as f32);

        scores
            .entry(skill_id.clone())
            .and_modify(|r| {
                r.score += rrf_contribution;
                r.bm25_rank = Some(rank_1_indexed);
                r.bm25_score = Some(*score);
            })
            .or_insert(HybridResult {
                skill_id: skill_id.clone(),
                score: rrf_contribution,
                bm25_rank: Some(rank_1_indexed),
                semantic_rank: None,
                bm25_score: Some(*score),
                semantic_score: None,
            });
    }

    // Process semantic results
    for (rank, (skill_id, score)) in semantic_results.iter().enumerate() {
        let rank_1_indexed = rank + 1;
        let rrf_contribution = config.semantic_weight / (config.k + rank_1_indexed as f32);

        scores
            .entry(skill_id.clone())
            .and_modify(|r| {
                r.score += rrf_contribution;
                r.semantic_rank = Some(rank_1_indexed);
                r.semantic_score = Some(*score);
            })
            .or_insert(HybridResult {
                skill_id: skill_id.clone(),
                score: rrf_contribution,
                bm25_rank: None,
                semantic_rank: Some(rank_1_indexed),
                bm25_score: None,
                semantic_score: Some(*score),
            });
    }

    // Sort by RRF score (descending)
    let mut results: Vec<HybridResult> = scores.into_values().collect();
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    results
}

/// Simple fusion returning only (`skill_id`, score) pairs
#[must_use] 
pub fn fuse_simple(
    bm25_results: &[(String, f32)],
    semantic_results: &[(String, f32)],
    config: &RrfConfig,
) -> Vec<(String, f32)> {
    fuse_results(bm25_results, semantic_results, config)
        .into_iter()
        .map(|r| (r.skill_id, r.score))
        .collect()
}

/// Fuse results with limit
#[must_use] 
pub fn fuse_with_limit(
    bm25_results: &[(String, f32)],
    semantic_results: &[(String, f32)],
    config: &RrfConfig,
    limit: usize,
) -> Vec<HybridResult> {
    let mut results = fuse_results(bm25_results, semantic_results, config);
    results.truncate(limit);
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rrf_config_default() {
        let config = RrfConfig::default();
        assert_eq!(config.k, 60.0);
        assert_eq!(config.bm25_weight, 1.0);
        assert_eq!(config.semantic_weight, 1.0);
    }

    #[test]
    fn test_empty_inputs() {
        let config = RrfConfig::default();
        let results = fuse_results(&[], &[], &config);
        assert!(results.is_empty());
    }

    #[test]
    fn test_bm25_only() {
        let config = RrfConfig::default();
        let bm25 = vec![("skill-1".to_string(), 5.0), ("skill-2".to_string(), 3.0)];

        let results = fuse_results(&bm25, &[], &config);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].skill_id, "skill-1");
        assert_eq!(results[1].skill_id, "skill-2");
        assert!(results[0].bm25_rank.is_some());
        assert!(results[0].semantic_rank.is_none());
    }

    #[test]
    fn test_semantic_only() {
        let config = RrfConfig::default();
        let semantic = vec![("skill-a".to_string(), 0.9), ("skill-b".to_string(), 0.7)];

        let results = fuse_results(&[], &semantic, &config);

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].skill_id, "skill-a");
        assert!(results[0].bm25_rank.is_none());
        assert!(results[0].semantic_rank.is_some());
    }

    #[test]
    fn test_fusion_overlapping() {
        let config = RrfConfig::default();

        // Both systems rank the same skills
        let bm25 = vec![("skill-1".to_string(), 5.0), ("skill-2".to_string(), 3.0)];
        let semantic = vec![("skill-2".to_string(), 0.9), ("skill-1".to_string(), 0.8)];

        let results = fuse_results(&bm25, &semantic, &config);

        assert_eq!(results.len(), 2);

        // Both skills should have both ranks
        for result in &results {
            assert!(result.bm25_rank.is_some());
            assert!(result.semantic_rank.is_some());
        }
    }

    #[test]
    fn test_fusion_boosts_agreement() {
        let config = RrfConfig::default();

        // skill-1 is #1 in both lists, should be ranked highest
        let bm25 = vec![
            ("skill-1".to_string(), 5.0),
            ("skill-2".to_string(), 3.0),
            ("skill-3".to_string(), 2.0),
        ];
        let semantic = vec![
            ("skill-1".to_string(), 0.95),
            ("skill-3".to_string(), 0.8),
            ("skill-4".to_string(), 0.7),
        ];

        let results = fuse_results(&bm25, &semantic, &config);

        // skill-1 should be first (ranked #1 in both)
        assert_eq!(results[0].skill_id, "skill-1");

        // skill-1 should have both ranks = 1
        assert_eq!(results[0].bm25_rank, Some(1));
        assert_eq!(results[0].semantic_rank, Some(1));
    }

    #[test]
    fn test_rrf_score_calculation() {
        let config = RrfConfig::with_k(60.0);

        let bm25 = vec![("skill-1".to_string(), 5.0)];
        let semantic = vec![("skill-1".to_string(), 0.9)];

        let results = fuse_results(&bm25, &semantic, &config);

        // RRF score = 1/(60+1) + 1/(60+1) = 2/61 ≈ 0.0328
        let expected_score = 2.0 / 61.0;
        assert!((results[0].score - expected_score).abs() < 0.001);
    }

    #[test]
    fn test_weighted_fusion() {
        // Give more weight to BM25
        let config = RrfConfig::with_weights(2.0, 1.0);

        let bm25 = vec![("bm25-winner".to_string(), 5.0)];
        let semantic = vec![("semantic-winner".to_string(), 0.9)];

        let results = fuse_results(&bm25, &semantic, &config);

        // BM25 result should win because of higher weight
        assert_eq!(results[0].skill_id, "bm25-winner");
    }

    #[test]
    fn test_fuse_simple() {
        let config = RrfConfig::default();
        let bm25 = vec![("skill-1".to_string(), 5.0)];
        let semantic = vec![("skill-2".to_string(), 0.9)];

        let results = fuse_simple(&bm25, &semantic, &config);

        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|(id, _)| id == "skill-1"));
        assert!(results.iter().any(|(id, _)| id == "skill-2"));
    }

    #[test]
    fn test_fuse_with_limit() {
        let config = RrfConfig::default();
        let bm25 = vec![
            ("s1".to_string(), 5.0),
            ("s2".to_string(), 4.0),
            ("s3".to_string(), 3.0),
        ];
        let semantic = vec![
            ("s4".to_string(), 0.9),
            ("s5".to_string(), 0.8),
            ("s6".to_string(), 0.7),
        ];

        let results = fuse_with_limit(&bm25, &semantic, &config, 3);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_preserves_original_scores() {
        let config = RrfConfig::default();
        let bm25 = vec![("skill-1".to_string(), 5.5)];
        let semantic = vec![("skill-1".to_string(), 0.88)];

        let results = fuse_results(&bm25, &semantic, &config);

        assert_eq!(results[0].bm25_score, Some(5.5));
        assert_eq!(results[0].semantic_score, Some(0.88));
    }
}
