//! Convert skills into beads issues for bv analysis.

use std::collections::{HashMap, HashSet};

use serde_json::Value as JsonValue;

use crate::beads::{Dependency, Issue, IssueStatus, IssueType, Priority};
use crate::error::Result;
use crate::storage::sqlite::SkillRecord;

#[derive(Debug, Default, Clone)]
struct SkillMeta {
    tags: Vec<String>,
    requires: Vec<String>,
    provides: Vec<String>,
}

pub fn skills_to_issues(skills: &[SkillRecord]) -> Result<Vec<Issue>> {
    let mut meta_by_id = HashMap::new();
    let mut name_by_id = HashMap::new();
    let mut status_by_id = HashMap::new();
    let mut providers: HashMap<String, Vec<String>> = HashMap::new();

    for skill in skills {
        let meta = parse_meta(&skill.metadata_json);
        name_by_id.insert(skill.id.clone(), skill.name.clone());
        status_by_id.insert(skill.id.clone(), skill_status(skill));

        providers
            .entry(skill.id.clone())
            .or_default()
            .push(skill.id.clone());
        for cap in &meta.provides {
            providers
                .entry(cap.to_lowercase())
                .or_default()
                .push(skill.id.clone());
        }
        meta_by_id.insert(skill.id.clone(), meta);
    }

    let mut issues = Vec::with_capacity(skills.len());
    for skill in skills {
        let meta = meta_by_id.get(&skill.id).cloned().unwrap_or_default();
        let mut dep_ids = HashSet::new();
        for req in &meta.requires {
            // Check direct ID match first, then capability match (case-insensitive)
            if let Some(ids) = providers.get(req) {
                for id in ids {
                    if id != &skill.id {
                        dep_ids.insert(id.clone());
                    }
                }
            } else if let Some(ids) = providers.get(&req.to_lowercase()) {
                for id in ids {
                    if id != &skill.id {
                        dep_ids.insert(id.clone());
                    }
                }
            }
        }
        let mut dep_ids: Vec<String> = dep_ids.into_iter().collect();
        dep_ids.sort();

        let dependencies = dep_ids
            .iter()
            .map(|id| Dependency {
                id: id.clone(),
                title: name_by_id.get(id).cloned().unwrap_or_default(),
                status: status_by_id.get(id).copied(),
                dependency_type: None,
            })
            .collect();

        let mut labels: Vec<String> = meta.tags.iter().map(|t| t.to_lowercase()).collect();
        labels.push(format!("layer:{}", skill.source_layer));
        labels.sort();
        labels.dedup();

        let mut extra = HashMap::new();
        extra.insert(
            "skill_version".to_string(),
            JsonValue::String(skill.version.clone().unwrap_or_else(|| "0.1.0".to_string())),
        );
        extra.insert(
            "quality_score".to_string(),
            JsonValue::from(skill.quality_score),
        );

        let issue = Issue {
            id: skill.id.clone(),
            title: skill.name.clone(),
            description: skill.description.clone(),
            status: skill_status(skill),
            priority: quality_to_priority(skill.quality_score),
            issue_type: IssueType::Task,
            owner: skill.author.clone(),
            assignee: None,
            labels,
            notes: None,
            created_at: None,
            created_by: None,
            updated_at: None,
            closed_at: None,
            dependencies,
            dependents: Vec::new(),
            extra,
        };

        issues.push(issue);
    }

    Ok(issues)
}

fn parse_meta(metadata_json: &str) -> SkillMeta {
    let parsed: serde_json::Value = serde_json::from_str(metadata_json).unwrap_or_default();
    SkillMeta {
        tags: parse_list(&parsed, "tags"),
        requires: parse_list(&parsed, "requires"),
        provides: parse_list(&parsed, "provides"),
    }
}

fn parse_list(value: &serde_json::Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.as_str().map(String::from))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn quality_to_priority(score: f64) -> Priority {
    if score >= 0.9 {
        0
    } else if score >= 0.7 {
        1
    } else if score >= 0.5 {
        2
    } else if score >= 0.3 {
        3
    } else {
        4
    }
}

const fn skill_status(skill: &SkillRecord) -> IssueStatus {
    if skill.is_deprecated {
        IssueStatus::Closed
    } else {
        IssueStatus::Open
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record_with_meta(id: &str, meta: &serde_json::Value) -> SkillRecord {
        SkillRecord {
            id: id.to_string(),
            name: format!("Skill {id}"),
            description: String::new(),
            version: Some("0.1.0".to_string()),
            author: None,
            source_path: String::new(),
            source_layer: "project".to_string(),
            git_remote: None,
            git_commit: None,
            content_hash: "hash".to_string(),
            body: String::new(),
            metadata_json: meta.to_string(),
            assets_json: "{}".to_string(),
            token_count: 0,
            quality_score: 0.5,
            indexed_at: String::new(),
            modified_at: String::new(),
            is_deprecated: false,
            deprecation_reason: None,
        }
    }

    #[test]
    fn test_skills_to_issues_dependencies() {
        let skill_a = record_with_meta(
            "skill-a",
            &serde_json::json!({
                "requires": ["cap-b"],
                "provides": ["cap-a"],
            }),
        );
        let skill_b = record_with_meta(
            "skill-b",
            &serde_json::json!({
                "provides": ["cap-b"],
            }),
        );

        let issues = skills_to_issues(&[skill_a, skill_b]).unwrap();
        let issue_a = issues.iter().find(|i| i.id == "skill-a").unwrap();
        assert_eq!(issue_a.dependencies.len(), 1);
        assert_eq!(issue_a.dependencies[0].id, "skill-b");
    }

    #[test]
    fn test_skills_to_issues_labels_and_status() {
        let skill = SkillRecord {
            id: "skill-x".to_string(),
            name: "Skill X".to_string(),
            description: String::new(),
            version: Some("0.2.0".to_string()),
            author: Some("alice".to_string()),
            source_path: String::new(),
            source_layer: "project".to_string(),
            git_remote: None,
            git_commit: None,
            content_hash: "hash".to_string(),
            body: String::new(),
            metadata_json: serde_json::json!({
                "tags": ["alpha", "beta"],
                "requires": [],
                "provides": [],
            })
            .to_string(),
            assets_json: "{}".to_string(),
            token_count: 0,
            quality_score: 0.95,
            indexed_at: String::new(),
            modified_at: String::new(),
            is_deprecated: true,
            deprecation_reason: None,
        };

        let issues = skills_to_issues(&[skill]).unwrap();
        let issue = &issues[0];

        assert_eq!(issue.status, IssueStatus::Closed);
        assert_eq!(issue.priority, 0);
        assert!(issue.labels.contains(&"alpha".to_string()));
        assert!(issue.labels.contains(&"beta".to_string()));
        assert!(issue.labels.contains(&"layer:project".to_string()));
    }

    #[test]
    fn test_skills_to_issues_dependencies_case_insensitive() {
        // Skill A provides "Database" (mixed case)
        let skill_a = record_with_meta(
            "skill-a",
            &serde_json::json!({
                "provides": ["Database"],
            }),
        );
        // Skill B requires "database" (lowercase)
        let skill_b = record_with_meta(
            "skill-b",
            &serde_json::json!({
                "requires": ["database"],
            }),
        );

        let issues = skills_to_issues(&[skill_a, skill_b]).unwrap();
        let issue_b = issues.iter().find(|i| i.id == "skill-b").unwrap();

        // Should find skill-a as a dependency despite case mismatch
        assert_eq!(
            issue_b.dependencies.len(),
            1,
            "Should resolve dependency case-insensitively"
        );
        assert_eq!(issue_b.dependencies[0].id, "skill-a");
    }

    #[test]
    fn test_skills_to_issues_tags_normalization() {
        let skill = record_with_meta(
            "skill-tags",
            &serde_json::json!({
                "tags": ["Rust", "RUST", "rust"],
            }),
        );

        let issues = skills_to_issues(&[skill]).unwrap();
        let issue = &issues[0];

        // Should be deduped to just "rust" (and "layer:project")
        // "layer:project" is added automatically
        let tags: Vec<_> = issue
            .labels
            .iter()
            .filter(|l| !l.starts_with("layer:"))
            .collect();
        assert_eq!(tags.len(), 1, "Tags should be normalized and deduped");
        assert_eq!(tags[0], "rust");
    }

    // =========================================
    // parse_meta Tests
    // =========================================

    #[test]
    fn test_parse_meta_empty_string() {
        let meta = parse_meta("");
        assert!(meta.tags.is_empty());
        assert!(meta.requires.is_empty());
        assert!(meta.provides.is_empty());
    }

    #[test]
    fn test_parse_meta_empty_object() {
        let meta = parse_meta("{}");
        assert!(meta.tags.is_empty());
        assert!(meta.requires.is_empty());
        assert!(meta.provides.is_empty());
    }

    #[test]
    fn test_parse_meta_invalid_json() {
        let meta = parse_meta("not valid json");
        assert!(meta.tags.is_empty());
        assert!(meta.requires.is_empty());
        assert!(meta.provides.is_empty());
    }

    #[test]
    fn test_parse_meta_partial_fields() {
        let meta = parse_meta(r#"{"tags": ["foo"], "provides": ["bar"]}"#);
        assert_eq!(meta.tags, vec!["foo"]);
        assert!(meta.requires.is_empty());
        assert_eq!(meta.provides, vec!["bar"]);
    }

    #[test]
    fn test_parse_meta_all_fields() {
        let meta = parse_meta(r#"{"tags": ["a", "b"], "requires": ["c"], "provides": ["d", "e"]}"#);
        assert_eq!(meta.tags, vec!["a", "b"]);
        assert_eq!(meta.requires, vec!["c"]);
        assert_eq!(meta.provides, vec!["d", "e"]);
    }

    #[test]
    fn test_parse_meta_non_array_values() {
        // If tags/requires/provides aren't arrays, should return empty
        let meta = parse_meta(r#"{"tags": "not-an-array", "requires": 123}"#);
        assert!(meta.tags.is_empty());
        assert!(meta.requires.is_empty());
    }

    // =========================================
    // parse_list Tests
    // =========================================

    #[test]
    fn test_parse_list_empty_array() {
        let value: serde_json::Value = serde_json::json!({"items": []});
        let list = parse_list(&value, "items");
        assert!(list.is_empty());
    }

    #[test]
    fn test_parse_list_missing_key() {
        let value: serde_json::Value = serde_json::json!({"other": ["a"]});
        let list = parse_list(&value, "items");
        assert!(list.is_empty());
    }

    #[test]
    fn test_parse_list_non_array_value() {
        let value: serde_json::Value = serde_json::json!({"items": "string"});
        let list = parse_list(&value, "items");
        assert!(list.is_empty());
    }

    #[test]
    fn test_parse_list_mixed_types() {
        // Only string items should be kept
        let value: serde_json::Value = serde_json::json!({"items": ["a", 123, "b", null, "c"]});
        let list = parse_list(&value, "items");
        assert_eq!(list, vec!["a", "b", "c"]);
    }

    // =========================================
    // quality_to_priority Tests
    // =========================================

    #[test]
    fn test_quality_to_priority_p0() {
        assert_eq!(quality_to_priority(0.95), 0);
        assert_eq!(quality_to_priority(0.90), 0);
        assert_eq!(quality_to_priority(1.0), 0);
    }

    #[test]
    fn test_quality_to_priority_p1() {
        assert_eq!(quality_to_priority(0.89), 1);
        assert_eq!(quality_to_priority(0.70), 1);
        assert_eq!(quality_to_priority(0.75), 1);
    }

    #[test]
    fn test_quality_to_priority_p2() {
        assert_eq!(quality_to_priority(0.69), 2);
        assert_eq!(quality_to_priority(0.50), 2);
        assert_eq!(quality_to_priority(0.55), 2);
    }

    #[test]
    fn test_quality_to_priority_p3() {
        assert_eq!(quality_to_priority(0.49), 3);
        assert_eq!(quality_to_priority(0.30), 3);
        assert_eq!(quality_to_priority(0.35), 3);
    }

    #[test]
    fn test_quality_to_priority_p4() {
        assert_eq!(quality_to_priority(0.29), 4);
        assert_eq!(quality_to_priority(0.10), 4);
        assert_eq!(quality_to_priority(0.0), 4);
    }

    // =========================================
    // skill_status Tests
    // =========================================

    #[test]
    fn test_skill_status_active() {
        let skill = record_with_meta("test", &serde_json::json!({}));
        assert_eq!(skill_status(&skill), IssueStatus::Open);
    }

    #[test]
    fn test_skill_status_deprecated() {
        let mut skill = record_with_meta("test", &serde_json::json!({}));
        skill.is_deprecated = true;
        assert_eq!(skill_status(&skill), IssueStatus::Closed);
    }

    // =========================================
    // skills_to_issues Edge Cases
    // =========================================

    #[test]
    fn test_skills_to_issues_empty_input() {
        let issues = skills_to_issues(&[]).unwrap();
        assert!(issues.is_empty());
    }

    #[test]
    fn test_skills_to_issues_no_dependencies() {
        let skill = record_with_meta("standalone", &serde_json::json!({}));
        let issues = skills_to_issues(&[skill]).unwrap();
        assert_eq!(issues.len(), 1);
        assert!(issues[0].dependencies.is_empty());
    }

    #[test]
    fn test_skills_to_issues_self_reference_filtered() {
        // A skill requiring a capability it provides itself shouldn't depend on itself
        let skill = record_with_meta(
            "self-sufficient",
            &serde_json::json!({
                "requires": ["cap-a"],
                "provides": ["cap-a"],
            }),
        );
        let issues = skills_to_issues(&[skill]).unwrap();
        assert!(
            issues[0].dependencies.is_empty(),
            "Self-dependencies should be filtered"
        );
    }

    #[test]
    fn test_skills_to_issues_unresolved_dependency() {
        // Requiring something no one provides
        let skill = record_with_meta(
            "needs-unknown",
            &serde_json::json!({
                "requires": ["unknown-capability"],
            }),
        );
        let issues = skills_to_issues(&[skill]).unwrap();
        // Should have no dependencies since nothing provides unknown-capability
        assert!(issues[0].dependencies.is_empty());
    }

    #[test]
    fn test_skills_to_issues_multiple_providers() {
        // Multiple skills provide the same capability
        let skill_a = record_with_meta("skill-a", &serde_json::json!({ "provides": ["logging"] }));
        let skill_b = record_with_meta("skill-b", &serde_json::json!({ "provides": ["logging"] }));
        let skill_c = record_with_meta("skill-c", &serde_json::json!({ "requires": ["logging"] }));

        let issues = skills_to_issues(&[skill_a, skill_b, skill_c]).unwrap();
        let issue_c = issues.iter().find(|i| i.id == "skill-c").unwrap();

        // skill-c should depend on both skill-a and skill-b
        assert_eq!(issue_c.dependencies.len(), 2);
        let dep_ids: Vec<_> = issue_c.dependencies.iter().map(|d| d.id.as_str()).collect();
        assert!(dep_ids.contains(&"skill-a"));
        assert!(dep_ids.contains(&"skill-b"));
    }

    #[test]
    fn test_skills_to_issues_direct_id_match() {
        // Can depend on skill directly by ID, not just capability
        let skill_a = record_with_meta("skill-a", &serde_json::json!({}));
        let skill_b = record_with_meta("skill-b", &serde_json::json!({ "requires": ["skill-a"] }));

        let issues = skills_to_issues(&[skill_a, skill_b]).unwrap();
        let issue_b = issues.iter().find(|i| i.id == "skill-b").unwrap();
        assert_eq!(issue_b.dependencies.len(), 1);
        assert_eq!(issue_b.dependencies[0].id, "skill-a");
    }

    #[test]
    fn test_skills_to_issues_extra_fields() {
        let mut skill = record_with_meta("test", &serde_json::json!({}));
        skill.version = Some("1.2.3".to_string());
        skill.quality_score = 0.85;

        let issues = skills_to_issues(&[skill]).unwrap();
        let extra = &issues[0].extra;

        assert_eq!(extra.get("skill_version").unwrap(), "1.2.3");
        assert_eq!(extra.get("quality_score").unwrap(), 0.85);
    }

    #[test]
    fn test_skills_to_issues_default_version() {
        let mut skill = record_with_meta("test", &serde_json::json!({}));
        skill.version = None;

        let issues = skills_to_issues(&[skill]).unwrap();
        let extra = &issues[0].extra;

        assert_eq!(extra.get("skill_version").unwrap(), "0.1.0");
    }

    #[test]
    fn test_skills_to_issues_owner_preserved() {
        let mut skill = record_with_meta("test", &serde_json::json!({}));
        skill.author = Some("alice".to_string());

        let issues = skills_to_issues(&[skill]).unwrap();
        assert_eq!(issues[0].owner, Some("alice".to_string()));
    }

    #[test]
    fn test_skills_to_issues_layer_label() {
        let mut skill = record_with_meta("test", &serde_json::json!({}));
        skill.source_layer = "global".to_string();

        let issues = skills_to_issues(&[skill]).unwrap();
        assert!(issues[0].labels.contains(&"layer:global".to_string()));
    }

    #[test]
    fn test_skills_to_issues_issue_type() {
        let skill = record_with_meta("test", &serde_json::json!({}));
        let issues = skills_to_issues(&[skill]).unwrap();
        assert_eq!(issues[0].issue_type, IssueType::Task);
    }

    #[test]
    fn test_skills_to_issues_dependency_titles() {
        let skill_a = record_with_meta("skill-a", &serde_json::json!({ "provides": ["cap"] }));
        let skill_b = record_with_meta("skill-b", &serde_json::json!({ "requires": ["cap"] }));

        let issues = skills_to_issues(&[skill_a, skill_b]).unwrap();
        let issue_b = issues.iter().find(|i| i.id == "skill-b").unwrap();

        assert_eq!(issue_b.dependencies[0].title, "Skill skill-a");
    }

    #[test]
    fn test_skills_to_issues_dependency_status() {
        let mut skill_a = record_with_meta("skill-a", &serde_json::json!({ "provides": ["cap"] }));
        skill_a.is_deprecated = true;
        let skill_b = record_with_meta("skill-b", &serde_json::json!({ "requires": ["cap"] }));

        let issues = skills_to_issues(&[skill_a, skill_b]).unwrap();
        let issue_b = issues.iter().find(|i| i.id == "skill-b").unwrap();

        assert_eq!(issue_b.dependencies[0].status, Some(IssueStatus::Closed));
    }

    // =========================================
    // SkillMeta Tests
    // =========================================

    #[test]
    fn test_skill_meta_default() {
        let meta = SkillMeta::default();
        assert!(meta.tags.is_empty());
        assert!(meta.requires.is_empty());
        assert!(meta.provides.is_empty());
    }

    #[test]
    fn test_skill_meta_clone() {
        let meta = SkillMeta {
            tags: vec!["a".to_string()],
            requires: vec!["b".to_string()],
            provides: vec!["c".to_string()],
        };
        let cloned = meta.clone();
        assert_eq!(cloned.tags, meta.tags);
        assert_eq!(cloned.requires, meta.requires);
        assert_eq!(cloned.provides, meta.provides);
    }

    #[test]
    fn test_skill_meta_debug() {
        let meta = SkillMeta::default();
        let debug_str = format!("{:?}", meta);
        assert!(debug_str.contains("SkillMeta"));
    }
}
