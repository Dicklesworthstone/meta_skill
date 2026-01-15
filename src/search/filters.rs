//! Search filter utilities for post-fusion result filtering
//!
//! Provides utility functions for filtering search results using SearchFilters
//! from the context module. These functions operate on SkillRecord lookups
//! and HybridResult vectors.

use crate::storage::sqlite::SkillRecord;

use super::context::SearchFilters;
use super::hybrid::HybridResult;

/// Check if a skill record matches the given filters
pub fn matches_skill_record(filters: &SearchFilters, skill: &SkillRecord) -> bool {
    let skill_tags = parse_tags_from_metadata(&skill.metadata_json);
    filters.matches(
        &skill_tags,
        &skill.source_layer,
        skill.quality_score as f32,
        skill.is_deprecated,
    )
}

/// Filter a list of skill IDs based on a lookup function
pub fn filter_skill_ids<F>(skill_ids: &[String], filters: &SearchFilters, lookup: F) -> Vec<String>
where
    F: Fn(&str) -> Option<SkillRecord>,
{
    skill_ids
        .iter()
        .filter(|id| {
            if let Some(skill) = lookup(id) {
                matches_skill_record(filters, &skill)
            } else {
                false
            }
        })
        .cloned()
        .collect()
}

/// Filter hybrid results maintaining order and scores
pub fn filter_hybrid_results(
    results: Vec<HybridResult>,
    filters: &SearchFilters,
    lookup: impl Fn(&str) -> Option<SkillRecord>,
) -> Vec<HybridResult> {
    results
        .into_iter()
        .filter(|r| {
            if let Some(skill) = lookup(&r.skill_id) {
                matches_skill_record(filters, &skill)
            } else {
                false
            }
        })
        .collect()
}

/// Parse tags from metadata JSON
fn parse_tags_from_metadata(metadata_json: &str) -> Vec<String> {
    if let Ok(meta) = serde_json::from_str::<serde_json::Value>(metadata_json) {
        if let Some(tags) = meta.get("tags").and_then(|t| t.as_array()) {
            return tags
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
        }
    }
    vec![]
}

#[cfg(test)]
mod tests {
    use super::super::context::SearchLayer;
    use super::*;

    fn make_skill(
        id: &str,
        layer: &str,
        quality: f64,
        deprecated: bool,
        tags: &[&str],
    ) -> SkillRecord {
        let tags_json = serde_json::json!({ "tags": tags });
        SkillRecord {
            id: id.to_string(),
            name: id.to_string(),
            description: "Test skill".to_string(),
            version: Some("1.0.0".to_string()),
            author: None,
            source_path: "/test".to_string(),
            source_layer: layer.to_string(),
            git_remote: None,
            git_commit: None,
            content_hash: "hash".to_string(),
            body: "content".to_string(),
            metadata_json: tags_json.to_string(),
            assets_json: "{}".to_string(),
            token_count: 100,
            quality_score: quality,
            indexed_at: "2025-01-01T00:00:00Z".to_string(),
            modified_at: "2025-01-01T00:00:00Z".to_string(),
            is_deprecated: deprecated,
            deprecation_reason: None,
        }
    }

    #[test]
    fn test_empty_filters_match_non_deprecated() {
        // Default filters exclude deprecated, so only non-deprecated match
        let filters = SearchFilters::new();
        let skill = make_skill("test", "project", 0.8, false, &["rust"]);
        assert!(matches_skill_record(&filters, &skill));
    }

    #[test]
    fn test_layer_filter() {
        let filters = SearchFilters::new().layer(SearchLayer::Project);

        let project_skill = make_skill("s1", "project", 0.8, false, &[]);
        let org_skill = make_skill("s2", "org", 0.8, false, &[]);

        assert!(matches_skill_record(&filters, &project_skill));
        assert!(!matches_skill_record(&filters, &org_skill));
    }

    #[test]
    fn test_tags_filter_any_match() {
        let filters = SearchFilters::new().tags(vec!["rust".to_string(), "cli".to_string()]);

        let rust_skill = make_skill("s1", "project", 0.8, false, &["rust", "web"]);
        let cli_skill = make_skill("s2", "project", 0.8, false, &["cli"]);
        let python_skill = make_skill("s3", "project", 0.8, false, &["python"]);

        assert!(matches_skill_record(&filters, &rust_skill)); // has "rust"
        assert!(matches_skill_record(&filters, &cli_skill)); // has "cli"
        assert!(!matches_skill_record(&filters, &python_skill)); // no match
    }

    #[test]
    fn test_quality_filter() {
        let filters = SearchFilters::new().min_quality(0.7);

        let high_quality = make_skill("s1", "project", 0.9, false, &[]);
        let low_quality = make_skill("s2", "project", 0.5, false, &[]);
        let edge_quality = make_skill("s3", "project", 0.7, false, &[]);

        assert!(matches_skill_record(&filters, &high_quality));
        assert!(!matches_skill_record(&filters, &low_quality));
        assert!(matches_skill_record(&filters, &edge_quality)); // exactly at threshold
    }

    #[test]
    fn test_deprecated_filter_default_excludes() {
        let filters = SearchFilters::new();

        let active_skill = make_skill("s1", "project", 0.8, false, &[]);
        let deprecated_skill = make_skill("s2", "project", 0.8, true, &[]);

        assert!(matches_skill_record(&filters, &active_skill));
        assert!(!matches_skill_record(&filters, &deprecated_skill));
    }

    #[test]
    fn test_deprecated_filter_include() {
        let filters = SearchFilters::new().include_deprecated(true);

        let deprecated_skill = make_skill("s1", "project", 0.8, true, &[]);
        assert!(matches_skill_record(&filters, &deprecated_skill));
    }

    #[test]
    fn test_combined_filters() {
        let filters = SearchFilters::new()
            .layer(SearchLayer::Project)
            .tags(vec!["rust".to_string()])
            .min_quality(0.7);

        // Matches all filters
        let good_skill = make_skill("s1", "project", 0.8, false, &["rust"]);
        assert!(matches_skill_record(&filters, &good_skill));

        // Wrong layer
        let wrong_layer = make_skill("s2", "org", 0.8, false, &["rust"]);
        assert!(!matches_skill_record(&filters, &wrong_layer));

        // Wrong tags
        let wrong_tags = make_skill("s3", "project", 0.8, false, &["python"]);
        assert!(!matches_skill_record(&filters, &wrong_tags));

        // Low quality
        let low_quality = make_skill("s4", "project", 0.5, false, &["rust"]);
        assert!(!matches_skill_record(&filters, &low_quality));
    }

    #[test]
    fn test_parse_tags_from_metadata() {
        let json = r#"{"tags": ["rust", "cli", "search"]}"#;
        let tags = parse_tags_from_metadata(json);
        assert_eq!(tags, vec!["rust", "cli", "search"]);
    }

    #[test]
    fn test_parse_tags_empty_metadata() {
        let tags = parse_tags_from_metadata("{}");
        assert!(tags.is_empty());
    }

    #[test]
    fn test_parse_tags_invalid_json() {
        let tags = parse_tags_from_metadata("not json");
        assert!(tags.is_empty());
    }

    #[test]
    fn test_filter_skill_ids() {
        let skills = vec![
            make_skill("rust-cli", "project", 0.8, false, &["rust"]),
            make_skill("python-web", "org", 0.9, false, &["python"]),
            make_skill("deprecated", "project", 0.7, true, &["rust"]),
        ];

        let lookup = |id: &str| skills.iter().find(|s| s.id == id).cloned();

        let filters = SearchFilters::new().layer(SearchLayer::Project);
        let ids = vec![
            "rust-cli".to_string(),
            "python-web".to_string(),
            "deprecated".to_string(),
        ];

        // Should filter to project layer, excluding deprecated
        let filtered = filter_skill_ids(&ids, &filters, lookup);
        assert_eq!(filtered, vec!["rust-cli"]);
    }
}
