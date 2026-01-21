use std::collections::HashMap;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::error::{MsError, Result};

use super::parser::MetaSkillParser;
use super::types::MetaSkill;

#[derive(Debug, Default)]
pub struct MetaSkillRegistry {
    meta_skills: HashMap<String, MetaSkill>,
    tag_index: HashMap<String, Vec<String>>,
    tech_stack_index: HashMap<String, Vec<String>>,
}

impl MetaSkillRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, meta_skill: MetaSkill) -> Result<()> {
        meta_skill.validate()?;
        let id = meta_skill.id.clone();

        // Remove old index entries if updating
        if let Some(old) = self.meta_skills.insert(id, meta_skill.clone()) {
            self.remove_from_indexes(&old);
        }

        // Add new index entries
        self.add_to_indexes(&meta_skill);
        Ok(())
    }

    fn add_to_indexes(&mut self, meta_skill: &MetaSkill) {
        for tag in &meta_skill.metadata.tags {
            self.tag_index
                .entry(tag.clone())
                .or_default()
                .push(meta_skill.id.clone());
        }

        for stack in &meta_skill.metadata.tech_stacks {
            self.tech_stack_index
                .entry(stack.clone())
                .or_default()
                .push(meta_skill.id.clone());
        }
    }

    fn remove_from_indexes(&mut self, meta_skill: &MetaSkill) {
        for tag in &meta_skill.metadata.tags {
            if let Some(list) = self.tag_index.get_mut(tag) {
                if let Some(pos) = list.iter().position(|id| id == &meta_skill.id) {
                    list.swap_remove(pos);
                }
            }
        }

        for stack in &meta_skill.metadata.tech_stacks {
            if let Some(list) = self.tech_stack_index.get_mut(stack) {
                if let Some(pos) = list.iter().position(|id| id == &meta_skill.id) {
                    list.swap_remove(pos);
                }
            }
        }
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&MetaSkill> {
        self.meta_skills.get(id)
    }

    #[must_use]
    pub fn all(&self) -> Vec<&MetaSkill> {
        self.meta_skills.values().collect()
    }

    #[must_use]
    pub fn search(&self, query: &MetaSkillQuery) -> Vec<&MetaSkill> {
        let mut results: Vec<&MetaSkill> = self.meta_skills.values().collect();

        if let Some(text) = &query.text {
            let needle = text.to_lowercase();
            results.retain(|ms| {
                ms.id.to_lowercase().contains(&needle)
                    || ms.name.to_lowercase().contains(&needle)
                    || ms.description.to_lowercase().contains(&needle)
            });
        }

        if !query.tags.is_empty() {
            results.retain(|ms| {
                query
                    .tags
                    .iter()
                    .any(|tag| ms.metadata.tags.iter().any(|t| t == tag))
            });
        }

        if let Some(stack) = &query.tech_stack {
            results.retain(|ms| ms.metadata.tech_stacks.iter().any(|s| s == stack));
        }

        results
    }

    pub fn load_from_paths(&mut self, paths: &[PathBuf]) -> Result<usize> {
        let mut count = 0usize;
        for path in paths {
            if path.is_file() {
                if let Some(meta) = parse_if_meta_skill(path)? {
                    self.insert(meta)?;
                    count += 1;
                }
                continue;
            }

            if path.is_dir() {
                for entry in WalkDir::new(path)
                    .follow_links(true)
                    .into_iter()
                    .filter_map(std::result::Result::ok)
                {
                    let entry_path = entry.path();
                    if !entry_path.is_file() {
                        continue;
                    }
                    if let Some(meta) = parse_if_meta_skill(entry_path)? {
                        self.insert(meta)?;
                        count += 1;
                    }
                }
            }
        }
        Ok(count)
    }

    #[must_use]
    pub fn stats(&self) -> MetaSkillRegistryStats {
        MetaSkillRegistryStats {
            total: self.meta_skills.len(),
            tags_indexed: self.tag_index.len(),
            tech_stacks_indexed: self.tech_stack_index.len(),
        }
    }
}

#[derive(Debug, Default)]
pub struct MetaSkillQuery {
    pub text: Option<String>,
    pub tags: Vec<String>,
    pub tech_stack: Option<String>,
}

#[derive(Debug)]
pub struct MetaSkillRegistryStats {
    pub total: usize,
    pub tags_indexed: usize,
    pub tech_stacks_indexed: usize,
}

fn is_meta_skill_file(path: &Path) -> bool {
    matches!(path.extension().and_then(|ext| ext.to_str()), Some("toml"))
}

fn parse_if_meta_skill(path: &Path) -> Result<Option<MetaSkill>> {
    if !is_meta_skill_file(path) {
        return Ok(None);
    }

    let content = std::fs::read_to_string(path).map_err(|err| {
        MsError::InvalidSkill(format!("read meta-skill {}: {err}", path.display()))
    })?;
    if !content.contains("[meta_skill]") {
        return Ok(None);
    }

    let meta = MetaSkillParser::parse_str(&content, path)?;
    Ok(Some(meta))
}

#[cfg(test)]
mod tests {
    use super::super::types::{MetaSkillMetadata, MetaSkillSliceRef, PinStrategy};
    use super::*;

    // =========================================
    // Test Helpers
    // =========================================

    fn valid_slice_ref() -> MetaSkillSliceRef {
        MetaSkillSliceRef {
            skill_id: "skill-1".to_string(),
            slice_ids: vec![],
            level: None,
            priority: 0,
            required: false,
            conditions: vec![],
        }
    }

    fn meta_skill_with_id(id: &str) -> MetaSkill {
        MetaSkill {
            id: id.to_string(),
            name: format!("Meta {id}"),
            description: format!("Description for {id}"),
            slices: vec![valid_slice_ref()],
            pin_strategy: PinStrategy::LatestCompatible,
            metadata: MetaSkillMetadata::default(),
            min_context_tokens: 0,
            recommended_context_tokens: 0,
        }
    }

    fn meta_skill_with_tags(id: &str, tags: Vec<&str>) -> MetaSkill {
        let mut meta = meta_skill_with_id(id);
        meta.metadata.tags = tags.into_iter().map(String::from).collect();
        meta
    }

    fn meta_skill_with_stacks(id: &str, stacks: Vec<&str>) -> MetaSkill {
        let mut meta = meta_skill_with_id(id);
        meta.metadata.tech_stacks = stacks.into_iter().map(String::from).collect();
        meta
    }

    // =========================================
    // Registry Construction Tests
    // =========================================

    #[test]
    fn registry_new_is_empty() {
        let registry = MetaSkillRegistry::new();
        assert!(registry.all().is_empty());
        let stats = registry.stats();
        assert_eq!(stats.total, 0);
    }

    #[test]
    fn registry_default_is_empty() {
        let registry = MetaSkillRegistry::default();
        assert!(registry.all().is_empty());
    }

    #[test]
    fn registry_debug() {
        let registry = MetaSkillRegistry::new();
        let debug = format!("{:?}", registry);
        assert!(debug.contains("MetaSkillRegistry"));
    }

    // =========================================
    // Registry Insert Tests
    // =========================================

    #[test]
    fn registry_insert_single() {
        let mut registry = MetaSkillRegistry::new();
        let meta = meta_skill_with_id("meta-1");
        registry.insert(meta).unwrap();

        assert_eq!(registry.all().len(), 1);
        assert!(registry.get("meta-1").is_some());
    }

    #[test]
    fn registry_insert_multiple() {
        let mut registry = MetaSkillRegistry::new();
        registry.insert(meta_skill_with_id("meta-1")).unwrap();
        registry.insert(meta_skill_with_id("meta-2")).unwrap();
        registry.insert(meta_skill_with_id("meta-3")).unwrap();

        assert_eq!(registry.all().len(), 3);
    }

    #[test]
    fn registry_insert_updates_existing() {
        let mut registry = MetaSkillRegistry::new();
        let mut meta = meta_skill_with_id("meta-1");
        meta.description = "Original".to_string();
        registry.insert(meta).unwrap();

        let mut updated = meta_skill_with_id("meta-1");
        updated.description = "Updated".to_string();
        registry.insert(updated).unwrap();

        assert_eq!(registry.all().len(), 1);
        let found = registry.get("meta-1").unwrap();
        assert_eq!(found.description, "Updated");
    }

    #[test]
    fn registry_insert_validates() {
        let mut registry = MetaSkillRegistry::new();
        let invalid = MetaSkill {
            id: "".to_string(), // Invalid
            name: "Name".to_string(),
            description: "Desc".to_string(),
            slices: vec![valid_slice_ref()],
            pin_strategy: PinStrategy::LatestCompatible,
            metadata: MetaSkillMetadata::default(),
            min_context_tokens: 0,
            recommended_context_tokens: 0,
        };

        assert!(registry.insert(invalid).is_err());
        assert_eq!(registry.all().len(), 0);
    }

    // =========================================
    // Registry Indexing Tests
    // =========================================

    #[test]
    fn registry_indexes_tags_and_stacks() {
        let meta = MetaSkill {
            id: "test".to_string(),
            name: "Test".to_string(),
            description: "Desc".to_string(),
            slices: vec![valid_slice_ref()],
            pin_strategy: PinStrategy::LatestCompatible,
            metadata: MetaSkillMetadata {
                author: None,
                version: "0.1.0".to_string(),
                tags: vec!["tag1".to_string()],
                tech_stacks: vec!["rust".to_string()],
                updated_at: None,
            },
            min_context_tokens: 0,
            recommended_context_tokens: 0,
        };

        let mut registry = MetaSkillRegistry::new();
        registry.insert(meta).unwrap();
        let stats = registry.stats();
        assert_eq!(stats.total, 1);
        assert_eq!(stats.tags_indexed, 1);
        assert_eq!(stats.tech_stacks_indexed, 1);
    }

    #[test]
    fn registry_indexes_multiple_tags() {
        let mut registry = MetaSkillRegistry::new();
        registry
            .insert(meta_skill_with_tags("m1", vec!["cli", "rust"]))
            .unwrap();

        let stats = registry.stats();
        assert_eq!(stats.tags_indexed, 2);
    }

    #[test]
    fn registry_indexes_shared_tags() {
        let mut registry = MetaSkillRegistry::new();
        registry
            .insert(meta_skill_with_tags("m1", vec!["rust"]))
            .unwrap();
        registry
            .insert(meta_skill_with_tags("m2", vec!["rust"]))
            .unwrap();

        // Both share "rust" tag
        let stats = registry.stats();
        assert_eq!(stats.tags_indexed, 1);
        assert_eq!(stats.total, 2);
    }

    #[test]
    fn registry_update_reindexes_tags() {
        let mut registry = MetaSkillRegistry::new();
        registry
            .insert(meta_skill_with_tags("m1", vec!["old-tag"]))
            .unwrap();

        // Update with new tags
        registry
            .insert(meta_skill_with_tags("m1", vec!["new-tag"]))
            .unwrap();

        let query = MetaSkillQuery {
            text: None,
            tags: vec!["old-tag".to_string()],
            tech_stack: None,
        };
        assert!(registry.search(&query).is_empty());

        let query = MetaSkillQuery {
            text: None,
            tags: vec!["new-tag".to_string()],
            tech_stack: None,
        };
        assert_eq!(registry.search(&query).len(), 1);
    }

    // =========================================
    // Registry Get Tests
    // =========================================

    #[test]
    fn registry_get_existing() {
        let mut registry = MetaSkillRegistry::new();
        registry.insert(meta_skill_with_id("meta-1")).unwrap();

        let found = registry.get("meta-1");
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, "meta-1");
    }

    #[test]
    fn registry_get_nonexistent() {
        let registry = MetaSkillRegistry::new();
        assert!(registry.get("nonexistent").is_none());
    }

    // =========================================
    // Registry Search Tests
    // =========================================

    #[test]
    fn registry_search_by_text_in_id() {
        let mut registry = MetaSkillRegistry::new();
        registry.insert(meta_skill_with_id("rust-cli")).unwrap();
        registry.insert(meta_skill_with_id("python-web")).unwrap();

        let query = MetaSkillQuery {
            text: Some("rust".to_string()),
            tags: vec![],
            tech_stack: None,
        };

        let results = registry.search(&query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "rust-cli");
    }

    #[test]
    fn registry_search_by_text_in_name() {
        let mut registry = MetaSkillRegistry::new();
        let mut meta = meta_skill_with_id("m1");
        meta.name = "Rust CLI Tools".to_string();
        registry.insert(meta).unwrap();

        let query = MetaSkillQuery {
            text: Some("tools".to_string()),
            tags: vec![],
            tech_stack: None,
        };

        let results = registry.search(&query);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn registry_search_by_text_in_description() {
        let mut registry = MetaSkillRegistry::new();
        let mut meta = meta_skill_with_id("m1");
        meta.description = "Learn advanced Kubernetes techniques".to_string();
        registry.insert(meta).unwrap();

        let query = MetaSkillQuery {
            text: Some("kubernetes".to_string()),
            tags: vec![],
            tech_stack: None,
        };

        let results = registry.search(&query);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn registry_search_by_text_case_insensitive() {
        let mut registry = MetaSkillRegistry::new();
        registry.insert(meta_skill_with_id("RUST-CLI")).unwrap();

        let query = MetaSkillQuery {
            text: Some("rust".to_string()),
            tags: vec![],
            tech_stack: None,
        };

        assert_eq!(registry.search(&query).len(), 1);
    }

    #[test]
    fn registry_search_by_tags() {
        let mut registry = MetaSkillRegistry::new();
        registry
            .insert(meta_skill_with_tags("m1", vec!["rust", "cli"]))
            .unwrap();
        registry
            .insert(meta_skill_with_tags("m2", vec!["python"]))
            .unwrap();

        let query = MetaSkillQuery {
            text: None,
            tags: vec!["rust".to_string()],
            tech_stack: None,
        };

        let results = registry.search(&query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "m1");
    }

    #[test]
    fn registry_search_by_multiple_tags_any_match() {
        let mut registry = MetaSkillRegistry::new();
        registry
            .insert(meta_skill_with_tags("m1", vec!["rust"]))
            .unwrap();
        registry
            .insert(meta_skill_with_tags("m2", vec!["python"]))
            .unwrap();

        // Should find m1 because it has "rust"
        let query = MetaSkillQuery {
            text: None,
            tags: vec!["rust".to_string(), "go".to_string()],
            tech_stack: None,
        };

        let results = registry.search(&query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "m1");
    }

    #[test]
    fn registry_search_by_tech_stack() {
        let mut registry = MetaSkillRegistry::new();
        registry
            .insert(meta_skill_with_stacks("m1", vec!["rust"]))
            .unwrap();
        registry
            .insert(meta_skill_with_stacks("m2", vec!["python"]))
            .unwrap();

        let query = MetaSkillQuery {
            text: None,
            tags: vec![],
            tech_stack: Some("rust".to_string()),
        };

        let results = registry.search(&query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "m1");
    }

    #[test]
    fn registry_search_combined_filters() {
        let mut registry = MetaSkillRegistry::new();
        let mut m1 = meta_skill_with_tags("rust-web", vec!["web"]);
        m1.metadata.tech_stacks = vec!["rust".to_string()];
        registry.insert(m1).unwrap();

        let mut m2 = meta_skill_with_tags("python-web", vec!["web"]);
        m2.metadata.tech_stacks = vec!["python".to_string()];
        registry.insert(m2).unwrap();

        // Search for "web" tag with "rust" tech stack
        let query = MetaSkillQuery {
            text: None,
            tags: vec!["web".to_string()],
            tech_stack: Some("rust".to_string()),
        };

        let results = registry.search(&query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "rust-web");
    }

    #[test]
    fn registry_search_empty_query_returns_all() {
        let mut registry = MetaSkillRegistry::new();
        registry.insert(meta_skill_with_id("m1")).unwrap();
        registry.insert(meta_skill_with_id("m2")).unwrap();

        let query = MetaSkillQuery::default();
        let results = registry.search(&query);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn registry_search_no_matches() {
        let mut registry = MetaSkillRegistry::new();
        registry.insert(meta_skill_with_id("rust-cli")).unwrap();

        let query = MetaSkillQuery {
            text: Some("nonexistent".to_string()),
            tags: vec![],
            tech_stack: None,
        };

        assert!(registry.search(&query).is_empty());
    }

    // =========================================
    // Registry Stats Tests
    // =========================================

    #[test]
    fn registry_stats_empty() {
        let registry = MetaSkillRegistry::new();
        let stats = registry.stats();
        assert_eq!(stats.total, 0);
        assert_eq!(stats.tags_indexed, 0);
        assert_eq!(stats.tech_stacks_indexed, 0);
    }

    #[test]
    fn registry_stats_debug() {
        let registry = MetaSkillRegistry::new();
        let stats = registry.stats();
        let debug = format!("{:?}", stats);
        assert!(debug.contains("MetaSkillRegistryStats"));
    }

    // =========================================
    // MetaSkillQuery Tests
    // =========================================

    #[test]
    fn query_default_values() {
        let query = MetaSkillQuery::default();
        assert!(query.text.is_none());
        assert!(query.tags.is_empty());
        assert!(query.tech_stack.is_none());
    }

    #[test]
    fn query_debug() {
        let query = MetaSkillQuery {
            text: Some("test".to_string()),
            tags: vec!["tag1".to_string()],
            tech_stack: Some("rust".to_string()),
        };
        let debug = format!("{:?}", query);
        assert!(debug.contains("MetaSkillQuery"));
        assert!(debug.contains("test"));
    }

    // =========================================
    // is_meta_skill_file Tests
    // =========================================

    #[test]
    fn is_meta_skill_file_toml() {
        assert!(is_meta_skill_file(Path::new("skill.toml")));
        assert!(is_meta_skill_file(Path::new("/path/to/meta.toml")));
    }

    #[test]
    fn is_meta_skill_file_non_toml() {
        assert!(!is_meta_skill_file(Path::new("skill.md")));
        assert!(!is_meta_skill_file(Path::new("skill.json")));
        assert!(!is_meta_skill_file(Path::new("skill")));
    }
}
