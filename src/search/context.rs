//! Context-aware search ranking and filtering
//!
//! Provides `SearchContext` for personalized ranking and `SearchFilters`
//! for structured result filtering.

use serde::{Deserialize, Serialize};

/// Context for personalized search ranking
#[derive(Debug, Clone, Default)]
pub struct SearchContext {
    /// Current working directory
    pub cwd: Option<String>,
    /// Recent skills accessed
    pub recent_skills: Vec<String>,
    /// Project tech stack
    pub tech_stack: Vec<String>,
}

/// Search layer (source of skills)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchLayer {
    /// Base/system skills (built-in)
    Base,
    /// Organization/global skills
    Org,
    /// Project-specific skills
    Project,
    /// User/local skills
    User,
}

impl SearchLayer {
    /// Parse from string
    #[must_use]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "base" | "system" => Some(Self::Base),
            "org" | "global" => Some(Self::Org),
            "project" => Some(Self::Project),
            "user" | "local" => Some(Self::User),
            _ => None,
        }
    }

    /// Convert to string
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::Org => "org",
            Self::Project => "project",
            Self::User => "user",
        }
    }
}

impl std::fmt::Display for SearchLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Filters for narrowing search results
///
/// Filters are applied post-fusion (after BM25 + vector RRF) to the
/// merged result set. They can be combined for fine-grained control.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchFilters {
    /// Filter by tags (any-match: result passes if it has ANY of these tags)
    #[serde(default)]
    pub tags: Vec<String>,

    /// Filter by source layer
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer: Option<SearchLayer>,

    /// Minimum quality score (0.0 - 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_quality: Option<f32>,

    /// Include deprecated skills (default: false)
    #[serde(default)]
    pub include_deprecated: bool,
}

impl SearchFilters {
    /// Create new empty filters
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create filters with tags
    #[must_use]
    pub fn with_tags(tags: Vec<String>) -> Self {
        Self {
            tags,
            ..Default::default()
        }
    }

    /// Create filters with layer
    #[must_use]
    pub fn with_layer(layer: SearchLayer) -> Self {
        Self {
            layer: Some(layer),
            ..Default::default()
        }
    }

    /// Create filters with minimum quality
    #[must_use]
    pub fn with_min_quality(min_quality: f32) -> Self {
        Self {
            min_quality: Some(min_quality.clamp(0.0, 1.0)),
            ..Default::default()
        }
    }

    /// Builder: add tags filter
    #[must_use]
    pub fn tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Builder: add layer filter
    #[must_use]
    pub const fn layer(mut self, layer: SearchLayer) -> Self {
        self.layer = Some(layer);
        self
    }

    /// Builder: set minimum quality
    #[must_use]
    pub const fn min_quality(mut self, min_quality: f32) -> Self {
        self.min_quality = Some(min_quality.clamp(0.0, 1.0));
        self
    }

    /// Builder: include deprecated skills
    #[must_use]
    pub const fn include_deprecated(mut self, include: bool) -> Self {
        self.include_deprecated = include;
        self
    }

    /// Check if filters are empty (no filtering will occur)
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty()
            && self.layer.is_none()
            && self.min_quality.is_none()
            && self.include_deprecated // if true, no deprecation filtering; if false, filtering occurs
    }

    /// Check if a skill passes all filters
    ///
    /// # Arguments
    /// * `skill_tags` - Tags from the skill
    /// * `skill_layer` - Layer the skill is from
    /// * `skill_quality` - Quality score (0.0 - 1.0)
    /// * `is_deprecated` - Whether the skill is deprecated
    #[must_use]
    pub fn matches(
        &self,
        skill_tags: &[String],
        skill_layer: &str,
        skill_quality: f32,
        is_deprecated: bool,
    ) -> bool {
        // Check deprecated filter (exclude deprecated by default)
        if is_deprecated && !self.include_deprecated {
            return false;
        }

        // Check layer filter
        if let Some(ref layer) = self.layer {
            let layer_lc = skill_layer.to_lowercase();
            let normalized_layer = match layer_lc.as_str() {
                "base" | "system" => "base",
                "org" | "global" => "org",
                "project" => "project",
                "user" | "local" => "user",
                _ => layer_lc.as_str(),
            };
            if normalized_layer != layer.as_str() {
                return false;
            }
        }

        // Check quality filter
        if let Some(min_q) = self.min_quality {
            if skill_quality < min_q {
                return false;
            }
        }

        // Check tags filter (any-match)
        if !self.tags.is_empty() {
            let has_matching_tag = self.tags.iter().any(|t| skill_tags.contains(t));
            if !has_matching_tag {
                return false;
            }
        }

        true
    }

    /// Parse tags from comma-separated string
    #[must_use]
    pub fn parse_tags(tags_str: &str) -> Vec<String> {
        tags_str
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect()
    }
}

/// Result of applying filters (for audit/debug)
#[derive(Debug, Clone, Serialize)]
pub struct FilterResult {
    /// Total results before filtering
    pub total_before: usize,
    /// Total results after filtering
    pub total_after: usize,
    /// Number filtered out by tags
    pub filtered_by_tags: usize,
    /// Number filtered out by layer
    pub filtered_by_layer: usize,
    /// Number filtered out by quality
    pub filtered_by_quality: usize,
    /// Number filtered out by deprecation
    pub filtered_by_deprecated: usize,
    /// Applied filters (for audit)
    pub applied_filters: SearchFilters,
}

impl FilterResult {
    /// Create new filter result tracking
    #[must_use]
    pub fn new(total_before: usize, filters: &SearchFilters) -> Self {
        Self {
            total_before,
            total_after: 0,
            filtered_by_tags: 0,
            filtered_by_layer: 0,
            filtered_by_quality: 0,
            filtered_by_deprecated: 0,
            applied_filters: filters.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_filters_default() {
        let filters = SearchFilters::default();
        assert!(filters.tags.is_empty());
        assert!(filters.layer.is_none());
        assert!(filters.min_quality.is_none());
        assert!(!filters.include_deprecated);
    }

    #[test]
    fn test_search_filters_builder() {
        let filters = SearchFilters::new()
            .tags(vec!["git".to_string(), "workflow".to_string()])
            .layer(SearchLayer::Project)
            .min_quality(0.7)
            .include_deprecated(true);

        assert_eq!(filters.tags, vec!["git", "workflow"]);
        assert_eq!(filters.layer, Some(SearchLayer::Project));
        assert_eq!(filters.min_quality, Some(0.7));
        assert!(filters.include_deprecated);
    }

    #[test]
    fn test_matches_no_filters() {
        let filters = SearchFilters::new().include_deprecated(true); // Override default exclusion
        assert!(filters.matches(&[], "project", 0.5, false));
        assert!(filters.matches(&["git".to_string()], "org", 0.9, true));
    }

    #[test]
    fn test_matches_tags_any_match() {
        let filters = SearchFilters::with_tags(vec!["git".to_string(), "rust".to_string()]);

        // Has "git" - matches
        assert!(filters.matches(&["git".to_string()], "project", 0.5, false));

        // Has "rust" - matches
        assert!(filters.matches(&["rust".to_string()], "project", 0.5, false));

        // Has both - matches
        assert!(filters.matches(
            &["git".to_string(), "rust".to_string()],
            "project",
            0.5,
            false
        ));

        // Has neither - doesn't match
        assert!(!filters.matches(&["python".to_string()], "project", 0.5, false));

        // Empty tags - doesn't match
        assert!(!filters.matches(&[], "project", 0.5, false));
    }

    #[test]
    fn test_matches_layer_filter() {
        let filters = SearchFilters::with_layer(SearchLayer::Project);

        assert!(filters.matches(&[], "project", 0.5, false));
        assert!(!filters.matches(&[], "org", 0.5, false));
        assert!(!filters.matches(&[], "base", 0.5, false));
    }

    #[test]
    fn test_matches_quality_filter() {
        let filters = SearchFilters::with_min_quality(0.7);

        assert!(filters.matches(&[], "project", 0.8, false));
        assert!(filters.matches(&[], "project", 0.7, false));
        assert!(!filters.matches(&[], "project", 0.69, false));
        assert!(!filters.matches(&[], "project", 0.5, false));
    }

    #[test]
    fn test_matches_deprecated_filter() {
        // Default: exclude deprecated
        let filters = SearchFilters::default();
        assert!(filters.matches(&[], "project", 0.5, false));
        assert!(!filters.matches(&[], "project", 0.5, true));

        // Include deprecated
        let filters_with_deprecated = SearchFilters::new().include_deprecated(true);
        assert!(filters_with_deprecated.matches(&[], "project", 0.5, false));
        assert!(filters_with_deprecated.matches(&[], "project", 0.5, true));
    }

    #[test]
    fn test_matches_combined_filters() {
        let filters = SearchFilters::new()
            .tags(vec!["git".to_string()])
            .layer(SearchLayer::Project)
            .min_quality(0.7)
            .include_deprecated(false);

        // Passes all filters
        assert!(filters.matches(&["git".to_string()], "project", 0.8, false));

        // Fails tag filter
        assert!(!filters.matches(&["rust".to_string()], "project", 0.8, false));

        // Fails layer filter
        assert!(!filters.matches(&["git".to_string()], "org", 0.8, false));

        // Fails quality filter
        assert!(!filters.matches(&["git".to_string()], "project", 0.5, false));

        // Fails deprecated filter
        assert!(!filters.matches(&["git".to_string()], "project", 0.8, true));
    }

    #[test]
    fn test_parse_tags() {
        let tags = SearchFilters::parse_tags("git, workflow, rust");
        assert_eq!(tags, vec!["git", "workflow", "rust"]);

        let tags = SearchFilters::parse_tags("single");
        assert_eq!(tags, vec!["single"]);

        let tags = SearchFilters::parse_tags("");
        assert!(tags.is_empty());

        let tags = SearchFilters::parse_tags("  spaced  ,  tags  ");
        assert_eq!(tags, vec!["spaced", "tags"]);
    }

    #[test]
    fn test_min_quality_clamped() {
        let filters = SearchFilters::with_min_quality(1.5);
        assert_eq!(filters.min_quality, Some(1.0));

        let filters = SearchFilters::with_min_quality(-0.5);
        assert_eq!(filters.min_quality, Some(0.0));
    }

    #[test]
    fn test_search_layer_from_str() {
        assert_eq!(SearchLayer::from_str("system"), Some(SearchLayer::Base));
        assert_eq!(SearchLayer::from_str("base"), Some(SearchLayer::Base));
        assert_eq!(SearchLayer::from_str("GLOBAL"), Some(SearchLayer::Org));
        assert_eq!(SearchLayer::from_str("org"), Some(SearchLayer::Org));
        assert_eq!(SearchLayer::from_str("Project"), Some(SearchLayer::Project));
        assert_eq!(SearchLayer::from_str("local"), Some(SearchLayer::User));
        assert_eq!(SearchLayer::from_str("user"), Some(SearchLayer::User));
        assert_eq!(SearchLayer::from_str("invalid"), None);
    }

    #[test]
    fn test_search_layer_as_str() {
        assert_eq!(SearchLayer::Base.as_str(), "base");
        assert_eq!(SearchLayer::Org.as_str(), "org");
        assert_eq!(SearchLayer::Project.as_str(), "project");
        assert_eq!(SearchLayer::User.as_str(), "user");
    }

    #[test]
    fn test_filters_is_empty() {
        // Default filters are NOT empty because they exclude deprecated
        let filters = SearchFilters::default();
        assert!(!filters.is_empty());

        // With include_deprecated=true, filters become empty
        let filters = SearchFilters::new().include_deprecated(true);
        // Still not empty because include_deprecated=true means we're including, not filtering
        // Actually is_empty() should return true here since no filtering occurs
        assert!(filters.is_empty());

        // With actual filters
        let filters = SearchFilters::with_tags(vec!["git".to_string()]);
        assert!(!filters.is_empty());
    }

    #[test]
    fn test_filter_result() {
        let filters = SearchFilters::with_tags(vec!["git".to_string()]);
        let result = FilterResult::new(100, &filters);

        assert_eq!(result.total_before, 100);
        assert_eq!(result.total_after, 0);
        assert_eq!(result.filtered_by_tags, 0);
    }
}
