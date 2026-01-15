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
                .entry(cap.clone())
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
            if let Some(ids) = providers.get(req) {
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

        let mut labels = meta.tags.clone();
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

fn skill_status(skill: &SkillRecord) -> IssueStatus {
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
}
