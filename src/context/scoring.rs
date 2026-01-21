//! Relevance scoring for context-aware skill loading.
//!
//! Implements multi-factor scoring to rank skills based on how well they
//! match the current working context.

use std::collections::HashSet;

use crate::core::skill::{ContextTags, SkillMetadata};

use super::detector::{DetectedProject, ProjectType};

/// Working context captured from the current environment.
///
/// Represents the user's current working state: what project they're in,
/// what files they've been working with, and what tools are available.
#[derive(Debug, Clone, Default)]
pub struct WorkingContext {
    /// Detected project types with confidence scores.
    pub detected_projects: Vec<DetectedProject>,
    /// Recently accessed/modified files.
    pub recent_files: Vec<String>,
    /// Tools detected in PATH or environment.
    pub detected_tools: HashSet<String>,
    /// Content snippets for signal matching (file content, command history, etc.).
    pub content_snippets: Vec<String>,
}

impl WorkingContext {
    /// Create an empty working context.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add detected projects from the project detector.
    #[must_use]
    pub fn with_projects(mut self, projects: Vec<DetectedProject>) -> Self {
        self.detected_projects = projects;
        self
    }

    /// Add recent files.
    #[must_use]
    pub fn with_recent_files(mut self, files: Vec<String>) -> Self {
        self.recent_files = files;
        self
    }

    /// Add detected tools.
    pub fn with_tools(mut self, tools: impl IntoIterator<Item = String>) -> Self {
        self.detected_tools = tools.into_iter().collect();
        self
    }

    /// Add content snippets for signal matching.
    #[must_use]
    pub fn with_content(mut self, content: Vec<String>) -> Self {
        self.content_snippets = content;
        self
    }

    /// Check if any content matches a signal pattern.
    #[must_use]
    pub fn matches_signal(&self, pattern: &str) -> bool {
        let regex = match regex::Regex::new(pattern) {
            Ok(r) => r,
            Err(_) => return false,
        };
        self.content_snippets.iter().any(|s| regex.is_match(s))
    }

    /// Get the primary (highest confidence) project type.
    #[must_use]
    pub fn primary_project_type(&self) -> Option<ProjectType> {
        self.detected_projects
            .iter()
            .max_by(|a, b| {
                a.confidence
                    .partial_cmp(&b.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|d| d.project_type)
    }
}

/// Weights for the different scoring factors.
#[derive(Debug, Clone)]
pub struct ScoringWeights {
    /// Weight for project type matching (default: 0.40).
    pub project_type: f32,
    /// Weight for file pattern matching (default: 0.25).
    pub file_patterns: f32,
    /// Weight for tool matching (default: 0.20).
    pub tools: f32,
    /// Weight for signal matching (default: 0.10).
    pub signals: f32,
    /// Weight for historical affinity (default: 0.05).
    pub historical: f32,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            project_type: 0.40,
            file_patterns: 0.25,
            tools: 0.20,
            signals: 0.10,
            historical: 0.05,
        }
    }
}

impl ScoringWeights {
    /// Create weights with custom values.
    #[must_use]
    pub const fn new(
        project_type: f32,
        file_patterns: f32,
        tools: f32,
        signals: f32,
        historical: f32,
    ) -> Self {
        Self {
            project_type,
            file_patterns,
            tools,
            signals,
            historical,
        }
    }

    /// Normalize weights to sum to 1.0.
    #[must_use]
    pub fn normalized(&self) -> Self {
        let sum =
            self.project_type + self.file_patterns + self.tools + self.signals + self.historical;
        if sum == 0.0 {
            return self.clone();
        }
        Self {
            project_type: self.project_type / sum,
            file_patterns: self.file_patterns / sum,
            tools: self.tools / sum,
            signals: self.signals / sum,
            historical: self.historical / sum,
        }
    }
}

/// Breakdown of individual score components.
#[derive(Debug, Clone, Default)]
pub struct ScoreBreakdown {
    /// Project type match score (0.0-1.0).
    pub project_type: f32,
    /// File pattern match score (0.0-1.0).
    pub file_patterns: f32,
    /// Tool match score (0.0-1.0).
    pub tools: f32,
    /// Signal match score (0.0-1.0).
    pub signals: f32,
    /// Historical affinity score (0.0-1.0).
    pub historical: f32,
}

/// A skill with its relevance score and breakdown.
#[derive(Debug, Clone)]
pub struct RankedSkill {
    /// Skill ID.
    pub skill_id: String,
    /// Skill name.
    pub skill_name: String,
    /// Overall relevance score (0.0-1.0).
    pub score: f32,
    /// Breakdown of individual components.
    pub breakdown: ScoreBreakdown,
}

/// Relevance scorer for ranking skills by context match.
#[derive(Debug, Clone)]
pub struct RelevanceScorer {
    weights: ScoringWeights,
}

impl Default for RelevanceScorer {
    fn default() -> Self {
        Self::new(ScoringWeights::default())
    }
}

impl RelevanceScorer {
    /// Create a new scorer with the given weights.
    #[must_use]
    pub fn new(weights: ScoringWeights) -> Self {
        Self {
            weights: weights.normalized(),
        }
    }

    /// Get the scoring weights.
    #[must_use]
    pub const fn weights(&self) -> &ScoringWeights {
        &self.weights
    }

    /// Score a single skill against the given context.
    #[must_use]
    pub fn score(&self, metadata: &SkillMetadata, context: &WorkingContext) -> f32 {
        let breakdown = self.breakdown(metadata, context);
        self.weighted_score(&breakdown)
    }

    /// Get the score breakdown for a skill.
    #[must_use]
    pub fn breakdown(&self, metadata: &SkillMetadata, context: &WorkingContext) -> ScoreBreakdown {
        ScoreBreakdown {
            project_type: self.project_type_match(&metadata.context, context),
            file_patterns: self.file_pattern_match(&metadata.context, context),
            tools: self.tool_match(&metadata.context, context),
            signals: self.signal_match(&metadata.context, context),
            historical: 0.0, // TODO: Connect to recommendation engine
        }
    }

    /// Compute weighted score from breakdown.
    fn weighted_score(&self, breakdown: &ScoreBreakdown) -> f32 {
        self.weights.historical.mul_add(
            breakdown.historical,
            self.weights.signals.mul_add(
                breakdown.signals,
                self.weights.tools.mul_add(
                    breakdown.tools,
                    self.weights.project_type.mul_add(
                        breakdown.project_type,
                        self.weights.file_patterns * breakdown.file_patterns,
                    ),
                ),
            ),
        )
    }

    /// Score and rank multiple skills.
    #[must_use]
    pub fn rank(&self, skills: &[SkillMetadata], context: &WorkingContext) -> Vec<RankedSkill> {
        let mut ranked: Vec<RankedSkill> = skills
            .iter()
            .map(|s| {
                let breakdown = self.breakdown(s, context);
                let score = self.weighted_score(&breakdown);
                RankedSkill {
                    skill_id: s.id.clone(),
                    skill_name: s.name.clone(),
                    score,
                    breakdown,
                }
            })
            .collect();

        // Sort by score descending
        ranked.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        ranked
    }

    /// Get the top N relevant skills.
    #[must_use]
    pub fn top_n(
        &self,
        skills: &[SkillMetadata],
        context: &WorkingContext,
        n: usize,
    ) -> Vec<RankedSkill> {
        let ranked = self.rank(skills, context);
        ranked.into_iter().take(n).collect()
    }

    /// Get skills with relevance score above threshold.
    #[must_use]
    pub fn above_threshold(
        &self,
        skills: &[SkillMetadata],
        context: &WorkingContext,
        threshold: f32,
    ) -> Vec<RankedSkill> {
        self.rank(skills, context)
            .into_iter()
            .filter(|r| r.score >= threshold)
            .collect()
    }

    // =========================================================================
    // Individual matching functions
    // =========================================================================

    /// Match skill's project types against detected projects.
    fn project_type_match(&self, skill_context: &ContextTags, context: &WorkingContext) -> f32 {
        if skill_context.project_types.is_empty() {
            return 0.0;
        }

        // Find the best matching detected project
        context
            .detected_projects
            .iter()
            .filter(|d| {
                skill_context
                    .project_types
                    .iter()
                    .any(|pt| pt.eq_ignore_ascii_case(d.project_type.id()))
            })
            .map(|d| d.confidence)
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or(0.0)
    }

    /// Match skill's file patterns against recent files.
    fn file_pattern_match(&self, skill_context: &ContextTags, context: &WorkingContext) -> f32 {
        if skill_context.file_patterns.is_empty() || context.recent_files.is_empty() {
            return 0.0;
        }

        let matches = context
            .recent_files
            .iter()
            .filter(|f| skill_context.matches_file(f))
            .count();

        // Normalize: more matches = higher score, capped at 1.0
        (matches as f32 / context.recent_files.len() as f32).min(1.0)
    }

    /// Match skill's required tools against detected tools.
    fn tool_match(&self, skill_context: &ContextTags, context: &WorkingContext) -> f32 {
        if skill_context.tools.is_empty() {
            return 0.0;
        }

        let matches = skill_context
            .tools
            .iter()
            .filter(|t| {
                context
                    .detected_tools
                    .iter()
                    .any(|dt| dt.eq_ignore_ascii_case(t))
            })
            .count();

        matches as f32 / skill_context.tools.len() as f32
    }

    /// Match skill's signals against context content.
    fn signal_match(&self, skill_context: &ContextTags, context: &WorkingContext) -> f32 {
        if skill_context.signals.is_empty() {
            return 0.0;
        }

        let weighted_sum: f32 = skill_context
            .signals
            .iter()
            .filter_map(|s| {
                if context.matches_signal(&s.pattern) {
                    Some(s.weight)
                } else {
                    None
                }
            })
            .sum();

        let max_possible: f32 = skill_context.signals.iter().map(|s| s.weight).sum();

        if max_possible == 0.0 {
            0.0
        } else {
            (weighted_sum / max_possible).min(1.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::skill::ContextSignal;
    use std::path::PathBuf;

    fn sample_rust_skill() -> SkillMetadata {
        SkillMetadata {
            id: "rust-errors".to_string(),
            name: "Rust Error Handling".to_string(),
            context: ContextTags {
                project_types: vec!["rust".to_string()],
                file_patterns: vec!["*.rs".to_string(), "Cargo.toml".to_string()],
                tools: vec!["cargo".to_string(), "rustc".to_string()],
                signals: vec![ContextSignal::new("thiserror", "use.*thiserror", 0.8)],
            },
            ..Default::default()
        }
    }

    fn sample_node_skill() -> SkillMetadata {
        SkillMetadata {
            id: "node-testing".to_string(),
            name: "Node.js Testing".to_string(),
            context: ContextTags {
                project_types: vec!["node".to_string()],
                file_patterns: vec!["*.ts".to_string(), "*.js".to_string()],
                tools: vec!["npm".to_string(), "node".to_string()],
                signals: vec![],
            },
            ..Default::default()
        }
    }

    fn sample_generic_skill() -> SkillMetadata {
        SkillMetadata {
            id: "git-workflow".to_string(),
            name: "Git Workflow".to_string(),
            context: ContextTags::default(), // No context = generic skill
            ..Default::default()
        }
    }

    fn rust_context() -> WorkingContext {
        WorkingContext::new()
            .with_projects(vec![DetectedProject {
                project_type: ProjectType::Rust,
                confidence: 1.0,
                marker_path: PathBuf::from("Cargo.toml"),
                marker_pattern: "Cargo.toml".to_string(),
            }])
            .with_recent_files(vec![
                "src/main.rs".to_string(),
                "src/lib.rs".to_string(),
                "Cargo.toml".to_string(),
            ])
            .with_tools(["cargo", "rustc", "git"].map(String::from))
            .with_content(vec!["use thiserror::Error;".to_string()])
    }

    fn node_context() -> WorkingContext {
        WorkingContext::new()
            .with_projects(vec![DetectedProject {
                project_type: ProjectType::Node,
                confidence: 0.9,
                marker_path: PathBuf::from("package.json"),
                marker_pattern: "package.json".to_string(),
            }])
            .with_recent_files(vec!["src/index.ts".to_string(), "package.json".to_string()])
            .with_tools(["npm", "node", "git"].map(String::from))
    }

    #[test]
    fn test_scorer_default_weights() {
        let scorer = RelevanceScorer::default();
        let w = scorer.weights();
        // Weights should sum to approximately 1.0 after normalization
        let sum = w.project_type + w.file_patterns + w.tools + w.signals + w.historical;
        assert!((sum - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_project_type_match_exact() {
        let scorer = RelevanceScorer::default();
        let skill = sample_rust_skill();
        let context = rust_context();

        let breakdown = scorer.breakdown(&skill, &context);
        assert!((breakdown.project_type - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_project_type_match_none() {
        let scorer = RelevanceScorer::default();
        let skill = sample_rust_skill();
        let context = node_context();

        let breakdown = scorer.breakdown(&skill, &context);
        assert!(breakdown.project_type < 0.001);
    }

    #[test]
    fn test_file_pattern_match() {
        let scorer = RelevanceScorer::default();
        let skill = sample_rust_skill();
        let context = rust_context();

        let breakdown = scorer.breakdown(&skill, &context);
        // All 3 files match (*.rs for 2, Cargo.toml for 1)
        assert!(breakdown.file_patterns > 0.9);
    }

    #[test]
    fn test_file_pattern_no_match() {
        let scorer = RelevanceScorer::default();
        let skill = sample_rust_skill();
        let context = node_context();

        let breakdown = scorer.breakdown(&skill, &context);
        assert!(breakdown.file_patterns < 0.001);
    }

    #[test]
    fn test_tool_match() {
        let scorer = RelevanceScorer::default();
        let skill = sample_rust_skill();
        let context = rust_context();

        let breakdown = scorer.breakdown(&skill, &context);
        // Both cargo and rustc are present
        assert!((breakdown.tools - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_tool_partial_match() {
        let scorer = RelevanceScorer::default();
        let skill = sample_rust_skill();
        // Context with only cargo, no rustc
        let context = WorkingContext::new().with_tools(["cargo", "git"].map(String::from));

        let breakdown = scorer.breakdown(&skill, &context);
        // Only 1 of 2 tools match
        assert!((breakdown.tools - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_signal_match() {
        let scorer = RelevanceScorer::default();
        let skill = sample_rust_skill();
        let context = rust_context();

        let breakdown = scorer.breakdown(&skill, &context);
        // Signal pattern matches "use thiserror"
        assert!(breakdown.signals > 0.7);
    }

    #[test]
    fn test_signal_no_match() {
        let scorer = RelevanceScorer::default();
        let skill = sample_rust_skill();
        let context = WorkingContext::new().with_content(vec!["fn main() {}".to_string()]);

        let breakdown = scorer.breakdown(&skill, &context);
        assert!(breakdown.signals < 0.001);
    }

    #[test]
    fn test_generic_skill_no_boost() {
        let scorer = RelevanceScorer::default();
        let skill = sample_generic_skill();
        let context = rust_context();

        let score = scorer.score(&skill, &context);
        // Generic skill (no context) should have zero score
        assert!(score < 0.001);
    }

    #[test]
    fn test_ranking() {
        let scorer = RelevanceScorer::default();
        let skills = vec![
            sample_rust_skill(),
            sample_node_skill(),
            sample_generic_skill(),
        ];
        let context = rust_context();

        let ranked = scorer.rank(&skills, &context);

        assert_eq!(ranked.len(), 3);
        // Rust skill should be first in rust context
        assert_eq!(ranked[0].skill_id, "rust-errors");
        // Generic skill should be last
        assert_eq!(ranked[2].skill_id, "git-workflow");
    }

    #[test]
    fn test_top_n() {
        let scorer = RelevanceScorer::default();
        let skills = vec![
            sample_rust_skill(),
            sample_node_skill(),
            sample_generic_skill(),
        ];
        let context = rust_context();

        let top = scorer.top_n(&skills, &context, 1);

        assert_eq!(top.len(), 1);
        assert_eq!(top[0].skill_id, "rust-errors");
    }

    #[test]
    fn test_above_threshold() {
        let scorer = RelevanceScorer::default();
        let skills = vec![
            sample_rust_skill(),
            sample_node_skill(),
            sample_generic_skill(),
        ];
        let context = rust_context();

        let relevant = scorer.above_threshold(&skills, &context, 0.3);

        // Only rust skill should be above 0.3 in rust context
        assert_eq!(relevant.len(), 1);
        assert_eq!(relevant[0].skill_id, "rust-errors");
    }

    #[test]
    fn test_custom_weights() {
        let weights = ScoringWeights::new(1.0, 0.0, 0.0, 0.0, 0.0);
        let scorer = RelevanceScorer::new(weights);
        let skill = sample_rust_skill();
        let context = rust_context();

        let score = scorer.score(&skill, &context);
        // With only project_type weight, score should equal project_type match
        assert!((score - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_working_context_primary_type() {
        let context = rust_context();
        assert_eq!(context.primary_project_type(), Some(ProjectType::Rust));
    }

    #[test]
    fn test_working_context_matches_signal() {
        let context = rust_context();
        assert!(context.matches_signal("use.*thiserror"));
        assert!(!context.matches_signal("import.*react"));
    }

    #[test]
    fn test_score_breakdown_explainability() {
        let scorer = RelevanceScorer::default();
        let skill = sample_rust_skill();
        let context = rust_context();

        let breakdown = scorer.breakdown(&skill, &context);

        // All components should be visible
        assert!(breakdown.project_type > 0.0);
        assert!(breakdown.file_patterns > 0.0);
        assert!(breakdown.tools > 0.0);
        assert!(breakdown.signals > 0.0);
    }
}
