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
    use super::super::types::MetaSkillMetadata;
    use super::*;

    #[test]
    fn registry_indexes_tags_and_stacks() {
        let meta = MetaSkill {
            id: "test".to_string(),
            name: "Test".to_string(),
            description: "Desc".to_string(),
            slices: vec![super::super::types::MetaSkillSliceRef {
                skill_id: "skill".to_string(),
                slice_ids: vec![],
                level: None,
                priority: 0,
                required: false,
                conditions: vec![],
            }],
            pin_strategy: super::super::types::PinStrategy::LatestCompatible,
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
}
