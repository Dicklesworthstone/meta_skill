//! Skill deduplication engine
//!
//! Detects near-duplicate skills using semantic similarity (embeddings) and
//! structural comparison (tags, triggers, requirements). Provides actions
//! for merging, aliasing, or deprecating duplicates.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::search::embeddings::{Embedder, HashEmbedder, VectorIndex};
use crate::storage::sqlite::SkillRecord;

/// Default similarity threshold for duplicate detection
const DEFAULT_SIMILARITY_THRESHOLD: f32 = 0.85;

/// A match indicating potential duplicate skills
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateMatch {
    /// ID of the first skill (usually the one being checked)
    pub skill_a: String,
    /// ID of the potentially duplicate skill
    pub skill_b: String,
    /// Semantic similarity score (0.0 - 1.0)
    pub similarity: f32,
    /// Structural similarity details
    pub structural: StructuralSimilarity,
    /// Recommended action
    pub recommendation: DeduplicationRecommendation,
}

/// Structural similarity between two skills
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StructuralSimilarity {
    /// Overlapping tags
    pub common_tags: Vec<String>,
    /// Tag overlap ratio (0.0 - 1.0)
    pub tag_overlap: f32,
    /// Whether skills have identical triggers
    pub same_triggers: bool,
    /// Whether skills have overlapping requirements
    pub overlapping_requirements: bool,
    /// Whether skills are in the same layer
    pub same_layer: bool,
}

/// Recommended action for a duplicate pair
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeduplicationRecommendation {
    /// Keep both skills (likely false positive)
    KeepBoth,
    /// Merge into primary skill
    Merge,
    /// Mark secondary as alias of primary
    Alias,
    /// Review manually (uncertain)
    Review,
}

/// Action to apply during deduplication
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum DeduplicationAction {
    /// Keep both skills (false positive)
    KeepBoth { skill_a: String, skill_b: String },
    /// Merge secondary into primary
    Merge { primary: String, secondary: String },
    /// Mark secondary as alias of primary
    Alias { primary: String, alias: String },
    /// Deprecate a skill
    Deprecate { skill_id: String, reason: String },
}

/// Engine for detecting and managing duplicate skills
pub struct DeduplicationEngine {
    embedder: Box<dyn Embedder>,
    index: VectorIndex,
    similarity_threshold: f32,
    skill_texts: HashMap<String, String>,
}

impl Default for DeduplicationEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl DeduplicationEngine {
    /// Create a new deduplication engine with default settings
    pub fn new() -> Self {
        let embedder = Box::new(HashEmbedder::new(384));
        let dims = embedder.dims();
        Self {
            embedder,
            index: VectorIndex::new(dims),
            similarity_threshold: DEFAULT_SIMILARITY_THRESHOLD,
            skill_texts: HashMap::new(),
        }
    }

    /// Create with a custom embedder
    pub fn with_embedder(embedder: Box<dyn Embedder>) -> Self {
        let dims = embedder.dims();
        Self {
            embedder,
            index: VectorIndex::new(dims),
            similarity_threshold: DEFAULT_SIMILARITY_THRESHOLD,
            skill_texts: HashMap::new(),
        }
    }

    /// Set the similarity threshold
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.similarity_threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Get current similarity threshold
    pub fn threshold(&self) -> f32 {
        self.similarity_threshold
    }

    /// Index a skill for duplicate detection
    pub fn index_skill(&mut self, skill: &SkillRecord) {
        let text = skill_to_text(skill);
        let embedding = self.embedder.embed(&text);
        self.index.insert(&skill.id, embedding);
        self.skill_texts.insert(skill.id.clone(), text);
    }

    /// Index multiple skills
    pub fn index_skills(&mut self, skills: &[SkillRecord]) {
        for skill in skills {
            self.index_skill(skill);
        }
    }

    /// Clear the index
    pub fn clear(&mut self) {
        let dims = self.embedder.dims();
        self.index = VectorIndex::new(dims);
        self.skill_texts.clear();
    }

    /// Number of indexed skills
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Whether the index is empty
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Find skills similar to the given skill
    pub fn find_similar(
        &self,
        skill: &SkillRecord,
        limit: usize,
    ) -> Vec<(String, f32)> {
        let text = skill_to_text(skill);
        let embedding = self.embedder.embed(&text);

        self.index
            .search(&embedding, limit + 1) // +1 to account for self-match
            .into_iter()
            .filter(|(id, score)| id != &skill.id && *score >= self.similarity_threshold)
            .take(limit)
            .collect()
    }

    /// Find duplicate candidates for a skill
    pub fn find_duplicates(
        &self,
        skill: &SkillRecord,
        all_skills: &HashMap<String, SkillRecord>,
    ) -> Vec<DuplicateMatch> {
        let similar = self.find_similar(skill, 10);

        similar
            .into_iter()
            .filter_map(|(other_id, similarity)| {
                let other = all_skills.get(&other_id)?;
                let structural = compute_structural_similarity(skill, other);
                let recommendation = recommend_action(similarity, &structural);

                Some(DuplicateMatch {
                    skill_a: skill.id.clone(),
                    skill_b: other_id,
                    similarity,
                    structural,
                    recommendation,
                })
            })
            .collect()
    }

    /// Scan all indexed skills for duplicates
    pub fn scan_all(
        &self,
        skills: &HashMap<String, SkillRecord>,
    ) -> Vec<DuplicateMatch> {
        let mut duplicates = Vec::new();
        let mut seen_pairs: HashSet<(String, String)> = HashSet::new();

        for skill in skills.values() {
            for dup in self.find_duplicates(skill, skills) {
                // Normalize pair order to avoid duplicates
                let pair = if dup.skill_a < dup.skill_b {
                    (dup.skill_a.clone(), dup.skill_b.clone())
                } else {
                    (dup.skill_b.clone(), dup.skill_a.clone())
                };

                if seen_pairs.insert(pair) {
                    duplicates.push(dup);
                }
            }
        }

        // Sort by similarity descending
        duplicates.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        duplicates
    }

    /// Compute similarity between two skills directly
    pub fn compute_similarity(&self, skill_a: &SkillRecord, skill_b: &SkillRecord) -> f32 {
        let text_a = skill_to_text(skill_a);
        let text_b = skill_to_text(skill_b);

        let emb_a = self.embedder.embed(&text_a);
        let emb_b = self.embedder.embed(&text_b);

        cosine_similarity(&emb_a, &emb_b)
    }
}

/// Convert a skill record to searchable text
fn skill_to_text(skill: &SkillRecord) -> String {
    let mut parts = vec![
        skill.name.clone(),
        skill.description.clone(),
    ];

    // Include body content (the actual skill markdown)
    if !skill.body.is_empty() {
        parts.push(skill.body.clone());
    }

    // Parse metadata for tags
    if let Ok(metadata) = serde_json::from_str::<serde_json::Value>(&skill.metadata_json) {
        if let Some(tags) = metadata.get("tags").and_then(|t| t.as_array()) {
            for tag in tags {
                if let Some(s) = tag.as_str() {
                    parts.push(s.to_string());
                }
            }
        }
    }

    parts.join(" ")
}

/// Compute structural similarity between two skills
fn compute_structural_similarity(a: &SkillRecord, b: &SkillRecord) -> StructuralSimilarity {
    let tags_a = extract_tags(&a.metadata_json);
    let tags_b = extract_tags(&b.metadata_json);

    let common_tags: Vec<String> = tags_a
        .intersection(&tags_b)
        .cloned()
        .collect();

    let tag_overlap = if tags_a.is_empty() && tags_b.is_empty() {
        0.0
    } else {
        let union_size = tags_a.union(&tags_b).count();
        if union_size == 0 {
            0.0
        } else {
            common_tags.len() as f32 / union_size as f32
        }
    };

    let triggers_a = extract_triggers(&a.metadata_json);
    let triggers_b = extract_triggers(&b.metadata_json);
    let same_triggers = !triggers_a.is_empty() && triggers_a == triggers_b;

    let requires_a = extract_requires(&a.metadata_json);
    let requires_b = extract_requires(&b.metadata_json);
    let overlapping_requirements = !requires_a.is_disjoint(&requires_b);

    let same_layer = a.source_layer == b.source_layer;

    StructuralSimilarity {
        common_tags,
        tag_overlap,
        same_triggers,
        overlapping_requirements,
        same_layer,
    }
}

/// Extract tags from metadata JSON
fn extract_tags(metadata_json: &str) -> HashSet<String> {
    serde_json::from_str::<serde_json::Value>(metadata_json)
        .ok()
        .and_then(|v| v.get("tags")?.as_array().cloned())
        .map(|arr| {
            arr.into_iter()
                .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
                .collect()
        })
        .unwrap_or_default()
}

/// Extract triggers from metadata JSON
fn extract_triggers(metadata_json: &str) -> HashSet<String> {
    serde_json::from_str::<serde_json::Value>(metadata_json)
        .ok()
        .and_then(|v| v.get("triggers")?.as_array().cloned())
        .map(|arr| {
            arr.into_iter()
                .filter_map(|v| {
                    v.get("pattern")
                        .and_then(|p| p.as_str())
                        .map(|s| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Extract requires from metadata JSON
fn extract_requires(metadata_json: &str) -> HashSet<String> {
    serde_json::from_str::<serde_json::Value>(metadata_json)
        .ok()
        .and_then(|v| v.get("requires")?.as_array().cloned())
        .map(|arr| {
            arr.into_iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Recommend an action based on similarity and structural analysis
fn recommend_action(similarity: f32, structural: &StructuralSimilarity) -> DeduplicationRecommendation {
    // Very high similarity with structural overlap -> merge
    if similarity >= 0.95 && (structural.same_triggers || structural.tag_overlap >= 0.5) {
        return DeduplicationRecommendation::Merge;
    }

    // High similarity with same triggers -> alias
    if similarity >= 0.90 && structural.same_triggers {
        return DeduplicationRecommendation::Alias;
    }

    // High similarity but no structural overlap -> review
    if similarity >= 0.90 {
        return DeduplicationRecommendation::Review;
    }

    // Moderate similarity with significant structural overlap -> alias
    if similarity >= 0.85 && structural.tag_overlap >= 0.6 {
        return DeduplicationRecommendation::Alias;
    }

    // Different layers often serve different purposes
    if !structural.same_layer {
        return DeduplicationRecommendation::KeepBoth;
    }

    // Default: needs review
    DeduplicationRecommendation::Review
}

/// Compute cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
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

/// Summary of a deduplication scan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeduplicationSummary {
    /// Total skills scanned
    pub total_skills: usize,
    /// Number of duplicate pairs found
    pub duplicate_pairs: usize,
    /// Breakdown by recommendation
    pub by_recommendation: HashMap<String, usize>,
    /// Top duplicate pairs (by similarity)
    pub top_duplicates: Vec<DuplicateMatch>,
}

impl DeduplicationSummary {
    /// Create summary from scan results
    pub fn from_matches(total_skills: usize, matches: Vec<DuplicateMatch>) -> Self {
        let total_matches = matches.len();
        let mut by_recommendation: HashMap<String, usize> = HashMap::new();

        for m in &matches {
            let key = format!("{:?}", m.recommendation).to_lowercase();
            *by_recommendation.entry(key).or_insert(0) += 1;
        }

        let top_duplicates: Vec<DuplicateMatch> = matches
            .into_iter()
            .take(10)
            .collect();

        Self {
            total_skills,
            duplicate_pairs: total_matches,
            by_recommendation,
            top_duplicates,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_skill(id: &str, name: &str, desc: &str, tags: &[&str]) -> SkillRecord {
        let tags_json: Vec<String> = tags.iter().map(|s| format!("\"{}\"", s)).collect();
        let metadata_json = format!(r#"{{"tags":[{}]}}"#, tags_json.join(","));

        SkillRecord {
            id: id.to_string(),
            name: name.to_string(),
            description: desc.to_string(),
            version: Some("0.1.0".to_string()),
            author: None,
            source_path: format!("/skills/{}", id),
            source_layer: "project".to_string(),
            git_remote: None,
            git_commit: None,
            content_hash: format!("hash-{}", id),
            body: format!("# {}\n\n{}", name, desc),
            metadata_json,
            assets_json: "{}".to_string(),
            token_count: 100,
            quality_score: 0.8,
            indexed_at: "2024-01-01T00:00:00Z".to_string(),
            modified_at: "2024-01-01T00:00:00Z".to_string(),
            is_deprecated: false,
            deprecation_reason: None,
        }
    }

    #[test]
    fn test_engine_creation() {
        let engine = DeduplicationEngine::new();
        assert_eq!(engine.threshold(), DEFAULT_SIMILARITY_THRESHOLD);
        assert!(engine.is_empty());
    }

    #[test]
    fn test_custom_threshold() {
        let engine = DeduplicationEngine::new().with_threshold(0.90);
        assert_eq!(engine.threshold(), 0.90);
    }

    #[test]
    fn test_threshold_clamping() {
        let engine = DeduplicationEngine::new().with_threshold(1.5);
        assert_eq!(engine.threshold(), 1.0);

        let engine = DeduplicationEngine::new().with_threshold(-0.5);
        assert_eq!(engine.threshold(), 0.0);
    }

    #[test]
    fn test_index_skill() {
        let mut engine = DeduplicationEngine::new();
        let skill = make_skill("test-1", "Test Skill", "A test skill", &["rust", "testing"]);

        engine.index_skill(&skill);
        assert_eq!(engine.len(), 1);
    }

    #[test]
    fn test_similar_skills_detected() {
        let mut engine = DeduplicationEngine::new().with_threshold(0.5);

        let skill_a = make_skill(
            "rust-error-handling",
            "Rust Error Handling",
            "Best practices for error handling in Rust using Result and Option types",
            &["rust", "errors", "best-practices"],
        );
        let skill_b = make_skill(
            "error-handling-patterns",
            "Error Handling Patterns",
            "Patterns for handling errors in Rust applications with Result types",
            &["rust", "errors", "patterns"],
        );
        let skill_c = make_skill(
            "git-workflow",
            "Git Workflow",
            "Standard git workflow for feature branches and pull requests",
            &["git", "workflow", "version-control"],
        );

        engine.index_skill(&skill_a);
        engine.index_skill(&skill_b);
        engine.index_skill(&skill_c);

        let similar = engine.find_similar(&skill_a, 5);

        // skill_b should be more similar to skill_a than skill_c
        assert!(!similar.is_empty());
        if similar.len() >= 2 {
            let b_score = similar.iter().find(|(id, _)| id == "error-handling-patterns").map(|(_, s)| *s);
            let c_score = similar.iter().find(|(id, _)| id == "git-workflow").map(|(_, s)| *s);

            if let (Some(b), Some(c)) = (b_score, c_score) {
                assert!(b > c, "Expected skill_b ({}) to be more similar than skill_c ({})", b, c);
            }
        }
    }

    #[test]
    fn test_structural_similarity_tags() {
        let skill_a = make_skill("a", "Skill A", "Description", &["rust", "async", "tokio"]);
        let skill_b = make_skill("b", "Skill B", "Description", &["rust", "async", "futures"]);

        let structural = compute_structural_similarity(&skill_a, &skill_b);

        assert!(structural.common_tags.contains(&"rust".to_string()));
        assert!(structural.common_tags.contains(&"async".to_string()));
        assert!(structural.tag_overlap > 0.0);
    }

    #[test]
    fn test_structural_similarity_same_layer() {
        let skill_a = make_skill("a", "Skill A", "Desc", &[]);
        let mut skill_b = make_skill("b", "Skill B", "Desc", &[]);
        skill_b.source_layer = "user".to_string();

        let structural = compute_structural_similarity(&skill_a, &skill_b);
        assert!(!structural.same_layer);
    }

    #[test]
    fn test_recommendation_merge() {
        let structural = StructuralSimilarity {
            common_tags: vec!["rust".to_string()],
            tag_overlap: 0.8,
            same_triggers: true,
            overlapping_requirements: false,
            same_layer: true,
        };

        let rec = recommend_action(0.96, &structural);
        assert_eq!(rec, DeduplicationRecommendation::Merge);
    }

    #[test]
    fn test_recommendation_keep_different_layers() {
        let structural = StructuralSimilarity {
            common_tags: vec![],
            tag_overlap: 0.0,
            same_triggers: false,
            overlapping_requirements: false,
            same_layer: false,
        };

        let rec = recommend_action(0.87, &structural);
        assert_eq!(rec, DeduplicationRecommendation::KeepBoth);
    }

    #[test]
    fn test_scan_all_no_duplicates() {
        let mut engine = DeduplicationEngine::new();

        let skills: HashMap<String, SkillRecord> = [
            make_skill("git", "Git Workflow", "Version control", &["git"]),
            make_skill("docker", "Docker Basics", "Containerization", &["docker"]),
            make_skill("rust", "Rust Fundamentals", "Systems programming", &["rust"]),
        ]
        .into_iter()
        .map(|s| (s.id.clone(), s))
        .collect();

        for skill in skills.values() {
            engine.index_skill(skill);
        }

        let duplicates = engine.scan_all(&skills);
        // With default threshold of 0.85, these very different skills shouldn't match
        assert!(duplicates.is_empty() || duplicates.iter().all(|d| d.similarity < 0.85));
    }

    #[test]
    fn test_compute_similarity_direct() {
        let engine = DeduplicationEngine::new();

        let skill_a = make_skill("a", "Rust Error Handling", "Error handling in Rust", &["rust"]);
        let skill_b = make_skill("b", "Rust Error Handling", "Error handling in Rust", &["rust"]);

        let similarity = engine.compute_similarity(&skill_a, &skill_b);
        assert!(similarity > 0.99, "Identical skills should have similarity near 1.0");
    }

    #[test]
    fn test_extract_tags() {
        let json = r#"{"tags":["Rust","async","TOKIO"]}"#;
        let tags = extract_tags(json);

        assert!(tags.contains("rust"));
        assert!(tags.contains("async"));
        assert!(tags.contains("tokio"));
    }

    #[test]
    fn test_extract_tags_empty() {
        let tags = extract_tags("{}");
        assert!(tags.is_empty());

        let tags = extract_tags("invalid json");
        assert!(tags.is_empty());
    }
}
