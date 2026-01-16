//! Skill deduplication engine
//!
//! Detects near-duplicate skills using semantic and structural similarity.
//!
//! ## Strategy
//!
//! 1. **Semantic similarity**: Compare embeddings using cosine similarity
//! 2. **Structural similarity**: Compare triggers, tags, requirements
//! 3. **Hybrid scoring**: Weighted combination of semantic + structural
//!
//! ## Usage
//!
//! ```ignore
//! use meta_skill::dedup::{DeduplicationEngine, DedupConfig};
//!
//! let engine = DeduplicationEngine::new(config, embedder);
//! let duplicates = engine.find_duplicates(&skill_record)?;
//! ```

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::search::Embedder;
use crate::storage::sqlite::{Database, SkillRecord};

/// Configuration for deduplication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DedupConfig {
    /// Minimum similarity threshold for semantic match (0.0-1.0)
    /// Default: 0.85
    #[serde(default = "default_similarity_threshold")]
    pub similarity_threshold: f32,

    /// Weight for semantic (embedding) similarity (0.0-1.0)
    /// Default: 0.7
    #[serde(default = "default_semantic_weight")]
    pub semantic_weight: f32,

    /// Weight for structural similarity (0.0-1.0)
    /// Default: 0.3
    #[serde(default = "default_structural_weight")]
    pub structural_weight: f32,

    /// Maximum number of candidates to evaluate
    /// Default: 100
    #[serde(default = "default_max_candidates")]
    pub max_candidates: usize,

    /// Minimum tag overlap ratio to boost structural score
    /// Default: 0.5
    #[serde(default = "default_tag_overlap_threshold")]
    pub tag_overlap_threshold: f32,
}

fn default_similarity_threshold() -> f32 {
    0.85
}

fn default_semantic_weight() -> f32 {
    0.7
}

fn default_structural_weight() -> f32 {
    0.3
}

fn default_max_candidates() -> usize {
    100
}

fn default_tag_overlap_threshold() -> f32 {
    0.5
}

impl Default for DedupConfig {
    fn default() -> Self {
        Self {
            similarity_threshold: default_similarity_threshold(),
            semantic_weight: default_semantic_weight(),
            structural_weight: default_structural_weight(),
            max_candidates: default_max_candidates(),
            tag_overlap_threshold: default_tag_overlap_threshold(),
        }
    }
}

/// A match found by the deduplication engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateMatch {
    /// ID of the potentially duplicate skill
    pub skill_id: String,
    /// Name of the potentially duplicate skill
    pub skill_name: String,
    /// Overall similarity score (0.0-1.0)
    pub similarity: f32,
    /// Semantic (embedding) similarity score
    pub semantic_score: f32,
    /// Structural similarity score
    pub structural_score: f32,
    /// Details about what matched structurally
    pub structural_details: StructuralDetails,
    /// Recommended action
    pub recommendation: DeduplicationAction,
}

/// Details about structural similarity
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StructuralDetails {
    /// Number of overlapping tags
    pub tag_overlap: usize,
    /// Total tags in primary skill
    pub primary_tags: usize,
    /// Total tags in candidate skill
    pub candidate_tags: usize,
    /// Jaccard similarity of tags
    pub tag_jaccard: f32,
    /// Whether descriptions are similar
    pub similar_description: bool,
    /// Whether requirements overlap
    pub requirements_overlap: bool,
}

/// Recommended action for handling duplicates
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeduplicationAction {
    /// Keep both skills (likely false positive or legitimately different)
    KeepBoth,
    /// Review manually - similarity is borderline
    Review,
    /// Merge into primary skill (high confidence duplicate)
    Merge,
    /// Mark secondary as alias of primary
    Alias,
    /// Deprecate secondary skill
    Deprecate,
}

/// Deduplication engine for finding near-duplicate skills
pub struct DeduplicationEngine<'a> {
    config: DedupConfig,
    embedder: &'a dyn Embedder,
}

impl<'a> DeduplicationEngine<'a> {
    /// Create a new deduplication engine
    pub fn new(config: DedupConfig, embedder: &'a dyn Embedder) -> Self {
        Self { config, embedder }
    }

    /// Find duplicates for a given skill from the database
    pub fn find_duplicates(
        &self,
        db: &Database,
        skill: &SkillRecord,
    ) -> Result<Vec<DuplicateMatch>> {
        // Get all skills from DB
        let all_skills = db.list_skills(self.config.max_candidates * 2, 0)?;

        // Compute embedding for target skill
        let target_text = self.skill_to_text(skill);
        let target_embedding = self.embedder.embed(&target_text);

        let mut matches = Vec::new();

        for candidate in &all_skills {
            // Skip self
            if candidate.id == skill.id {
                continue;
            }

            // Compute semantic similarity
            let candidate_text = self.skill_to_text(candidate);
            let candidate_embedding = self.embedder.embed(&candidate_text);
            let semantic_score = cosine_similarity(&target_embedding, &candidate_embedding);

            // Compute structural similarity
            let (structural_score, structural_details) =
                self.compute_structural_similarity(skill, candidate);

            // Compute weighted overall score (clamped to valid range)
            let similarity = (self.config.semantic_weight * semantic_score
                + self.config.structural_weight * structural_score)
                .clamp(0.0, 1.0);

            // Only include if above threshold
            if similarity >= self.config.similarity_threshold {
                let recommendation = self.recommend_action(similarity, &structural_details);

                matches.push(DuplicateMatch {
                    skill_id: candidate.id.clone(),
                    skill_name: candidate.name.clone(),
                    similarity,
                    semantic_score,
                    structural_score,
                    structural_details,
                    recommendation,
                });
            }
        }

        // Sort by similarity descending
        matches.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(Ordering::Equal)
        });

        // Limit results
        matches.truncate(self.config.max_candidates);

        Ok(matches)
    }

    /// Scan all skills for duplicates
    pub fn scan_all(&self, db: &Database) -> Result<Vec<DuplicatePair>> {
        let all_skills = db.list_skills(10000, 0)?;
        let mut pairs: Vec<DuplicatePair> = Vec::new();
        let mut seen: HashSet<(String, String)> = HashSet::new();

        // Precompute embeddings for all skills
        let embeddings: Vec<(String, Vec<f32>)> = all_skills
            .iter()
            .map(|s| {
                let text = self.skill_to_text(s);
                (s.id.clone(), self.embedder.embed(&text))
            })
            .collect();

        for (i, skill_a) in all_skills.iter().enumerate() {
            for (j, skill_b) in all_skills.iter().enumerate() {
                if i >= j {
                    continue;
                }

                // Create ordered key to avoid duplicates
                let key = if skill_a.id < skill_b.id {
                    (skill_a.id.clone(), skill_b.id.clone())
                } else {
                    (skill_b.id.clone(), skill_a.id.clone())
                };

                if seen.contains(&key) {
                    continue;
                }

                // Compute semantic similarity
                let semantic_score =
                    cosine_similarity(&embeddings[i].1, &embeddings[j].1);

                // Quick filter - if semantic is too low, skip structural
                // Use max(0.0, ...) to handle edge case of very low thresholds
                let semantic_filter = (self.config.similarity_threshold - 0.2).max(0.0);
                if semantic_score < semantic_filter {
                    continue;
                }

                // Compute structural similarity
                let (structural_score, structural_details) =
                    self.compute_structural_similarity(skill_a, skill_b);

                // Compute weighted overall score (clamped to valid range)
                let similarity = (self.config.semantic_weight * semantic_score
                    + self.config.structural_weight * structural_score)
                    .clamp(0.0, 1.0);

                if similarity >= self.config.similarity_threshold {
                    seen.insert(key);

                    let recommendation = self.recommend_action(similarity, &structural_details);

                    pairs.push(DuplicatePair {
                        skill_a_id: skill_a.id.clone(),
                        skill_a_name: skill_a.name.clone(),
                        skill_b_id: skill_b.id.clone(),
                        skill_b_name: skill_b.name.clone(),
                        similarity,
                        semantic_score,
                        structural_score,
                        structural_details,
                        recommendation,
                    });
                }
            }
        }

        // Sort by similarity descending
        pairs.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(Ordering::Equal)
        });

        Ok(pairs)
    }

    /// Convert skill to text for embedding
    fn skill_to_text(&self, skill: &SkillRecord) -> String {
        let mut text = String::new();

        // Include name with higher weight (repeated)
        text.push_str(&skill.name);
        text.push(' ');
        text.push_str(&skill.name);
        text.push(' ');

        // Include description
        text.push_str(&skill.description);
        text.push(' ');

        // Parse metadata for tags
        if let Ok(metadata) = serde_json::from_str::<serde_json::Value>(&skill.metadata_json) {
            if let Some(tags) = metadata.get("tags").and_then(|t| t.as_array()) {
                for tag in tags {
                    if let Some(t) = tag.as_str() {
                        text.push_str(t);
                        text.push(' ');
                    }
                }
            }
        }

        // Include body (truncated to avoid overwhelming)
        let body_preview: String = skill.body.chars().take(500).collect();
        text.push_str(&body_preview);

        text
    }

    /// Compute structural similarity between two skills
    fn compute_structural_similarity(
        &self,
        skill_a: &SkillRecord,
        skill_b: &SkillRecord,
    ) -> (f32, StructuralDetails) {
        let mut details = StructuralDetails::default();

        // Extract tags from metadata
        let tags_a = extract_tags(&skill_a.metadata_json);
        let tags_b = extract_tags(&skill_b.metadata_json);

        details.primary_tags = tags_a.len();
        details.candidate_tags = tags_b.len();

        // Compute tag overlap
        let intersection: HashSet<_> = tags_a.intersection(&tags_b).collect();
        details.tag_overlap = intersection.len();

        // Jaccard similarity for tags
        let union_size = tags_a.len() + tags_b.len() - intersection.len();
        details.tag_jaccard = if union_size > 0 {
            intersection.len() as f32 / union_size as f32
        } else {
            0.0
        };

        // Check description similarity (simple word overlap)
        let desc_sim = word_overlap_similarity(&skill_a.description, &skill_b.description);
        details.similar_description = desc_sim > 0.5;

        // Check requirements overlap
        let reqs_a = extract_requires(&skill_a.metadata_json);
        let reqs_b = extract_requires(&skill_b.metadata_json);
        if !reqs_a.is_empty() && !reqs_b.is_empty() {
            let reqs_intersection: HashSet<_> = reqs_a.intersection(&reqs_b).collect();
            details.requirements_overlap = !reqs_intersection.is_empty();
        }

        // Compute weighted structural score
        let mut score = 0.0;

        // Tag similarity (40% weight)
        score += 0.4 * details.tag_jaccard;

        // Boost for strong tag overlap (configurable)
        if details.tag_jaccard >= self.config.tag_overlap_threshold {
            score += 0.1;
        }

        // Description similarity (30% weight)
        score += 0.3 * desc_sim;

        // Requirements overlap (30% weight)
        if details.requirements_overlap {
            score += 0.3;
        } else if reqs_a.is_empty() && reqs_b.is_empty() {
            // No requirements on either - neutral
            score += 0.15;
        }

        (score.clamp(0.0, 1.0), details)
    }

    /// Recommend an action based on similarity scores
    fn recommend_action(
        &self,
        similarity: f32,
        details: &StructuralDetails,
    ) -> DeduplicationAction {
        // Very high similarity with tag overlap -> likely duplicate
        if similarity >= 0.95 && details.tag_jaccard >= 0.5 {
            return DeduplicationAction::Merge;
        }

        // High similarity but low tag overlap -> might be alias
        if similarity >= 0.90 && details.tag_jaccard < 0.3 {
            return DeduplicationAction::Alias;
        }

        // High similarity -> needs review
        if similarity >= 0.90 {
            return DeduplicationAction::Merge;
        }

        // Medium similarity -> review
        if similarity >= 0.85 {
            return DeduplicationAction::Review;
        }

        // Below threshold but included -> keep both
        DeduplicationAction::KeepBoth
    }
}

/// A pair of potentially duplicate skills
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicatePair {
    pub skill_a_id: String,
    pub skill_a_name: String,
    pub skill_b_id: String,
    pub skill_b_name: String,
    pub similarity: f32,
    pub semantic_score: f32,
    pub structural_score: f32,
    pub structural_details: StructuralDetails,
    pub recommendation: DeduplicationAction,
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

/// Extract tags from metadata JSON
fn extract_tags(metadata_json: &str) -> HashSet<String> {
    let mut tags = HashSet::new();
    if let Ok(metadata) = serde_json::from_str::<serde_json::Value>(metadata_json) {
        if let Some(arr) = metadata.get("tags").and_then(|t| t.as_array()) {
            for tag in arr {
                if let Some(t) = tag.as_str() {
                    tags.insert(t.to_lowercase());
                }
            }
        }
    }
    tags
}

/// Extract requires from metadata JSON
fn extract_requires(metadata_json: &str) -> HashSet<String> {
    let mut reqs = HashSet::new();
    if let Ok(metadata) = serde_json::from_str::<serde_json::Value>(metadata_json) {
        if let Some(arr) = metadata.get("requires").and_then(|t| t.as_array()) {
            for req in arr {
                if let Some(r) = req.as_str() {
                    reqs.insert(r.to_lowercase());
                }
            }
        }
    }
    reqs
}

/// Compute word overlap similarity between two strings
fn word_overlap_similarity(a: &str, b: &str) -> f32 {
    let words_a: HashSet<String> = a
        .to_lowercase()
        .split_whitespace()
        .filter(|w| w.len() >= 3)
        .map(|s| s.to_string())
        .collect();
    let words_b: HashSet<String> = b
        .to_lowercase()
        .split_whitespace()
        .filter(|w| w.len() >= 3)
        .map(|s| s.to_string())
        .collect();

    if words_a.is_empty() || words_b.is_empty() {
        return 0.0;
    }

    let intersection: HashSet<_> = words_a.intersection(&words_b).collect();
    let union_size = words_a.len() + words_b.len() - intersection.len();

    if union_size > 0 {
        intersection.len() as f32 / union_size as f32
    } else {
        0.0
    }
}

/// Summary of a deduplication scan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeduplicationSummary {
    /// Total skills scanned
    pub total_skills: usize,
    /// Number of duplicate pairs found
    pub duplicate_pairs: usize,
    /// Breakdown by recommendation type
    pub by_recommendation: HashMap<String, usize>,
    /// Top duplicate pairs by similarity (limited)
    pub top_duplicates: Vec<DuplicatePair>,
}

impl DeduplicationSummary {
    /// Create summary from scan results
    pub fn from_pairs(total_skills: usize, pairs: Vec<DuplicatePair>, top_limit: usize) -> Self {
        let duplicate_pairs = pairs.len();
        let mut by_recommendation: HashMap<String, usize> = HashMap::new();

        for pair in &pairs {
            let key = match pair.recommendation {
                DeduplicationAction::KeepBoth => "keep_both",
                DeduplicationAction::Review => "review",
                DeduplicationAction::Merge => "merge",
                DeduplicationAction::Alias => "alias",
                DeduplicationAction::Deprecate => "deprecate",
            };
            *by_recommendation.entry(key.to_string()).or_insert(0) += 1;
        }

        let top_duplicates: Vec<DuplicatePair> = pairs.into_iter().take(top_limit).collect();

        Self {
            total_skills,
            duplicate_pairs,
            by_recommendation,
            top_duplicates,
        }
    }
}

// ============================================================================
// Personalization Engine
// ============================================================================

/// User coding style profile extracted from CASS sessions
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StyleProfile {
    /// Preferred code patterns (e.g., early returns, guard clauses)
    pub patterns: Vec<CodePattern>,
    /// Variable naming conventions
    pub naming: NamingConvention,
    /// Preferred libraries and frameworks
    pub tech_preferences: Vec<String>,
    /// Comment style preferences
    pub comment_style: CommentStyle,
    /// Language-specific preferences
    pub language_prefs: HashMap<String, LanguagePrefs>,
}

/// A code pattern preference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodePattern {
    /// Pattern name (e.g., "early_return", "guard_clause")
    pub name: String,
    /// Description of the pattern
    pub description: String,
    /// Example code demonstrating the pattern
    pub example: Option<String>,
    /// How strongly this pattern is preferred (0.0-1.0)
    pub preference_strength: f32,
}

/// Variable naming convention
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NamingConvention {
    /// Variable case style (snake_case, camelCase, PascalCase)
    pub variable_case: CaseStyle,
    /// Function case style
    pub function_case: CaseStyle,
    /// Whether to use abbreviated names
    pub use_abbreviations: bool,
    /// Common abbreviations used (e.g., "msg" for "message")
    pub abbreviations: Vec<(String, String)>,
}

/// Case style for identifiers
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CaseStyle {
    #[default]
    SnakeCase,
    CamelCase,
    PascalCase,
    KebabCase,
}

/// Comment style preferences
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommentStyle {
    /// Whether to use doc comments for public items
    pub use_doc_comments: bool,
    /// Preferred comment marker (// vs /* */)
    pub inline_style: InlineCommentStyle,
    /// Whether to include TODO/FIXME markers
    pub use_todo_markers: bool,
}

/// Inline comment style
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InlineCommentStyle {
    #[default]
    DoubleSlash,
    BlockComment,
}

/// Language-specific preferences
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LanguagePrefs {
    /// Preferred error handling style
    pub error_handling: Option<String>,
    /// Preferred async/await patterns
    pub async_patterns: Option<String>,
    /// Preferred testing framework
    pub test_framework: Option<String>,
}

/// Personalizer adapts generic skills to user style
pub struct Personalizer {
    style: StyleProfile,
}

impl Personalizer {
    /// Create a new personalizer with the given style profile
    pub fn new(style: StyleProfile) -> Self {
        Self { style }
    }

    /// Get the style profile
    pub fn style(&self) -> &StyleProfile {
        &self.style
    }

    /// Personalize a skill record by adapting its content to user style
    ///
    /// Applies the following adaptations:
    /// - Convert variable/function naming to preferred case style in code blocks
    /// - Adjust terminology based on tech preferences
    /// - Apply pattern preferences where applicable
    pub fn personalize(&self, skill: &SkillRecord) -> PersonalizedSkill {
        let mut adaptations = Vec::new();
        let mut content = skill.body.clone();

        // Adapt code examples in code blocks
        let (adapted_code, code_adaptations) = self.adapt_code_examples(&content);
        if !code_adaptations.is_empty() {
            content = adapted_code;
            adaptations.extend(code_adaptations);
        }

        // Apply terminology adjustments
        let (adapted_terms, term_adaptations) = self.adapt_terminology(&content);
        if !term_adaptations.is_empty() {
            content = adapted_terms;
            adaptations.extend(term_adaptations);
        }

        PersonalizedSkill {
            original_id: skill.id.clone(),
            original_name: skill.name.clone(),
            adapted_content: content,
            adaptations_applied: adaptations,
        }
    }

    /// Check if personalization is available based on the current style profile.
    ///
    /// Returns true if the style profile has patterns or tech preferences that
    /// could be applied, or if naming conventions differ from defaults.
    pub fn should_personalize(&self, _skill: &SkillRecord) -> bool {
        // Check if we have any non-default style preferences
        let has_naming_prefs = self.style.naming.variable_case != CaseStyle::SnakeCase
            || self.style.naming.use_abbreviations
            || !self.style.naming.abbreviations.is_empty();
        let has_patterns = !self.style.patterns.is_empty();
        let has_tech_prefs = !self.style.tech_preferences.is_empty();

        has_naming_prefs || has_patterns || has_tech_prefs
    }

    /// Adapt code examples in the content to match user's naming conventions
    fn adapt_code_examples(&self, content: &str) -> (String, Vec<String>) {
        let mut adaptations = Vec::new();
        let mut result = String::new();
        let mut in_code_block = false;
        let mut code_block_lang = String::new();

        for line in content.lines() {
            if line.starts_with("```") {
                if in_code_block {
                    // End of code block
                    result.push_str(line);
                    result.push('\n');
                    in_code_block = false;
                    code_block_lang.clear();
                } else {
                    // Start of code block
                    code_block_lang = line.trim_start_matches("```").trim().to_string();
                    result.push_str(line);
                    result.push('\n');
                    in_code_block = true;
                }
            } else if in_code_block {
                // Apply naming convention transformations within code blocks
                let adapted_line = self.adapt_identifiers(line, &code_block_lang);
                if adapted_line != line && adaptations.is_empty() {
                    adaptations.push(format!(
                        "converted identifiers to {:?}",
                        self.style.naming.variable_case
                    ));
                }
                result.push_str(&adapted_line);
                result.push('\n');
            } else {
                result.push_str(line);
                result.push('\n');
            }
        }

        // Remove trailing newline if original didn't have one
        if !content.ends_with('\n') && result.ends_with('\n') {
            result.pop();
        }

        (result, adaptations)
    }

    /// Adapt identifiers in a line of code to match user's naming conventions
    fn adapt_identifiers(&self, line: &str, lang: &str) -> String {
        // Only convert if user prefers camelCase (most code defaults to snake_case)
        if self.style.naming.variable_case != CaseStyle::CamelCase {
            return line.to_string();
        }

        // Skip comment lines
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with("--") {
            return line.to_string();
        }

        // Skip if language typically uses snake_case (Rust, Python, etc.)
        let snake_case_langs = ["rust", "python", "ruby", "go"];
        if snake_case_langs.iter().any(|l| lang.eq_ignore_ascii_case(l)) {
            return line.to_string();
        }

        // Convert snake_case identifiers to camelCase in JS/TS code
        self.snake_to_camel(line)
    }

    /// Convert snake_case identifiers to camelCase
    fn snake_to_camel(&self, text: &str) -> String {
        let mut result = String::new();
        let mut chars = text.chars().peekable();
        let mut in_string = false;
        let mut string_char = ' ';
        let mut prev_was_escape = false;

        while let Some(c) = chars.next() {
            // Handle escape sequences in strings
            if in_string && prev_was_escape {
                prev_was_escape = false;
                result.push(c);
                continue;
            }

            if c == '\\' && in_string {
                prev_was_escape = true;
                result.push(c);
                continue;
            }

            // Track string boundaries
            if (c == '"' || c == '\'') && !in_string {
                in_string = true;
                string_char = c;
                result.push(c);
                continue;
            } else if c == string_char && in_string {
                in_string = false;
                result.push(c);
                continue;
            }

            // Don't convert inside strings
            if in_string {
                result.push(c);
                continue;
            }

            // Convert underscore followed by letter to uppercase letter
            if c == '_' {
                if let Some(&next) = chars.peek() {
                    if next.is_ascii_lowercase() {
                        chars.next();
                        result.push(next.to_ascii_uppercase());
                        continue;
                    }
                }
            }

            result.push(c);
        }

        result
    }

    /// Apply terminology adjustments based on tech preferences
    fn adapt_terminology(&self, content: &str) -> (String, Vec<String>) {
        let mut adaptations = Vec::new();
        let mut result = content.to_string();

        // Apply abbreviation substitutions
        for (abbrev, full) in &self.style.naming.abbreviations {
            if self.style.naming.use_abbreviations {
                // Convert full form to abbreviation
                if result.contains(full) {
                    result = result.replace(full, abbrev);
                    if adaptations.is_empty() {
                        adaptations.push("applied preferred abbreviations".to_string());
                    }
                }
            }
        }

        (result, adaptations)
    }
}

/// A skill that has been personalized to user style
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalizedSkill {
    /// Original skill ID
    pub original_id: String,
    /// Original skill name
    pub original_name: String,
    /// Content adapted to user style
    pub adapted_content: String,
    /// List of adaptations that were applied
    pub adaptations_applied: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_extract_tags() {
        let json = r#"{"tags": ["rust", "error", "handling"]}"#;
        let tags = extract_tags(json);
        assert_eq!(tags.len(), 3);
        assert!(tags.contains("rust"));
        assert!(tags.contains("error"));
        assert!(tags.contains("handling"));
    }

    #[test]
    fn test_word_overlap_similarity() {
        let a = "rust error handling patterns";
        let b = "error handling in rust";
        let sim = word_overlap_similarity(a, b);
        assert!(sim > 0.5); // Should have good overlap
    }

    #[test]
    fn test_dedup_config_defaults() {
        let config = DedupConfig::default();
        assert!((config.similarity_threshold - 0.85).abs() < 1e-6);
        assert!((config.semantic_weight - 0.7).abs() < 1e-6);
        assert!((config.structural_weight - 0.3).abs() < 1e-6);
    }

    #[test]
    fn test_dedup_action_serialization() {
        let action = DeduplicationAction::Merge;
        let json = serde_json::to_string(&action).unwrap();
        assert_eq!(json, "\"merge\"");

        let parsed: DeduplicationAction = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, DeduplicationAction::Merge);
    }

    #[test]
    fn test_deduplication_summary() {
        let pairs = vec![
            DuplicatePair {
                skill_a_id: "a".to_string(),
                skill_a_name: "Skill A".to_string(),
                skill_b_id: "b".to_string(),
                skill_b_name: "Skill B".to_string(),
                similarity: 0.95,
                semantic_score: 0.9,
                structural_score: 0.8,
                structural_details: StructuralDetails::default(),
                recommendation: DeduplicationAction::Merge,
            },
            DuplicatePair {
                skill_a_id: "c".to_string(),
                skill_a_name: "Skill C".to_string(),
                skill_b_id: "d".to_string(),
                skill_b_name: "Skill D".to_string(),
                similarity: 0.87,
                semantic_score: 0.85,
                structural_score: 0.7,
                structural_details: StructuralDetails::default(),
                recommendation: DeduplicationAction::Review,
            },
        ];

        let summary = DeduplicationSummary::from_pairs(100, pairs, 10);

        assert_eq!(summary.total_skills, 100);
        assert_eq!(summary.duplicate_pairs, 2);
        assert_eq!(summary.by_recommendation.get("merge"), Some(&1));
        assert_eq!(summary.by_recommendation.get("review"), Some(&1));
        assert_eq!(summary.top_duplicates.len(), 2);
    }

    // Personalization tests

    #[test]
    fn test_style_profile_default() {
        let profile = StyleProfile::default();
        assert!(profile.patterns.is_empty());
        assert!(profile.tech_preferences.is_empty());
        assert_eq!(profile.naming.variable_case, CaseStyle::SnakeCase);
    }

    #[test]
    fn test_style_profile_serialization() {
        let profile = StyleProfile {
            patterns: vec![CodePattern {
                name: "early_return".to_string(),
                description: "Return early from functions".to_string(),
                example: Some("if !valid { return Err(...); }".to_string()),
                preference_strength: 0.9,
            }],
            naming: NamingConvention {
                variable_case: CaseStyle::SnakeCase,
                function_case: CaseStyle::SnakeCase,
                use_abbreviations: false,
                abbreviations: vec![],
            },
            tech_preferences: vec!["tokio".to_string(), "serde".to_string()],
            comment_style: CommentStyle::default(),
            language_prefs: HashMap::new(),
        };

        let json = serde_json::to_string(&profile).unwrap();
        let parsed: StyleProfile = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.patterns.len(), 1);
        assert_eq!(parsed.patterns[0].name, "early_return");
        assert_eq!(parsed.tech_preferences.len(), 2);
    }

    #[test]
    fn test_case_style_serialization() {
        let cases = vec![
            (CaseStyle::SnakeCase, "\"snake_case\""),
            (CaseStyle::CamelCase, "\"camel_case\""),
            (CaseStyle::PascalCase, "\"pascal_case\""),
            (CaseStyle::KebabCase, "\"kebab_case\""),
        ];

        for (style, expected) in cases {
            let json = serde_json::to_string(&style).unwrap();
            assert_eq!(json, expected);
        }
    }

    #[test]
    fn test_personalizer_creation() {
        let profile = StyleProfile::default();
        let personalizer = Personalizer::new(profile);
        assert!(personalizer.style().patterns.is_empty());
    }

    #[test]
    fn test_personalizer_snake_to_camel() {
        let profile = StyleProfile {
            naming: NamingConvention {
                variable_case: CaseStyle::CamelCase,
                ..Default::default()
            },
            ..Default::default()
        };
        let personalizer = Personalizer::new(profile);

        // Test basic conversion
        let result = personalizer.snake_to_camel("let user_name = get_user_id();");
        assert_eq!(result, "let userName = getUserId();");

        // Test preserving strings
        let result = personalizer.snake_to_camel("let s = \"hello_world\";");
        assert_eq!(result, "let s = \"hello_world\";");
    }

    #[test]
    fn test_personalizer_adapt_code_examples() {
        let profile = StyleProfile {
            naming: NamingConvention {
                variable_case: CaseStyle::CamelCase,
                ..Default::default()
            },
            ..Default::default()
        };
        let personalizer = Personalizer::new(profile);

        let content = r#"Here is an example:

```javascript
let user_name = "test";
function get_user_id() {
    return user_name;
}
```

This shows the pattern."#;

        let (adapted, adaptations) = personalizer.adapt_code_examples(content);
        assert!(adapted.contains("userName"));
        assert!(adapted.contains("getUserId"));
        assert!(!adaptations.is_empty());
    }

    #[test]
    fn test_personalizer_no_adapt_for_snake_case_langs() {
        let profile = StyleProfile {
            naming: NamingConvention {
                variable_case: CaseStyle::CamelCase,
                ..Default::default()
            },
            ..Default::default()
        };
        let personalizer = Personalizer::new(profile);

        // Rust code should NOT be converted (snake_case is idiomatic)
        let content = r#"```rust
let user_name = "test";
```"#;

        let (adapted, _) = personalizer.adapt_code_examples(content);
        assert!(adapted.contains("user_name")); // Should remain unchanged
    }

    #[test]
    fn test_personalizer_should_personalize() {
        // Default profile should not trigger personalization
        let default_profile = StyleProfile::default();
        let personalizer = Personalizer::new(default_profile);
        let skill = SkillRecord {
            id: "test".to_string(),
            name: "Test Skill".to_string(),
            description: "A test".to_string(),
            version: None,
            author: None,
            source_path: "/test".to_string(),
            source_layer: "local".to_string(),
            git_remote: None,
            git_commit: None,
            content_hash: "abc123".to_string(),
            body: "content".to_string(),
            metadata_json: "{}".to_string(),
            assets_json: "[]".to_string(),
            token_count: 100,
            quality_score: 0.8,
            indexed_at: "2025-01-01T00:00:00Z".to_string(),
            modified_at: "2025-01-01T00:00:00Z".to_string(),
            is_deprecated: false,
            deprecation_reason: None,
        };
        assert!(!personalizer.should_personalize(&skill));

        // Profile with camelCase preference should trigger
        let camel_profile = StyleProfile {
            naming: NamingConvention {
                variable_case: CaseStyle::CamelCase,
                ..Default::default()
            },
            ..Default::default()
        };
        let personalizer = Personalizer::new(camel_profile);
        assert!(personalizer.should_personalize(&skill));
    }

    #[test]
    fn test_personalizer_abbreviations() {
        let profile = StyleProfile {
            naming: NamingConvention {
                use_abbreviations: true,
                abbreviations: vec![
                    ("msg".to_string(), "message".to_string()),
                    ("cfg".to_string(), "configuration".to_string()),
                ],
                ..Default::default()
            },
            ..Default::default()
        };
        let personalizer = Personalizer::new(profile);

        let content = "The message contains the configuration settings.";
        let (adapted, adaptations) = personalizer.adapt_terminology(content);
        assert!(adapted.contains("msg"));
        assert!(adapted.contains("cfg"));
        assert!(!adaptations.is_empty());
    }
}
