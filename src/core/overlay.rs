//! Skill Overlays
//!
//! Overlays allow dynamic modification of skills based on runtime context.
//! This enables features like environment-specific adjustments, A/B testing,
//! and conditional skill variations.

use serde::{Deserialize, Serialize};

use super::skill::SkillSpec;

/// Result of applying an overlay to a skill
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OverlayApplicationResult {
    /// ID of the overlay that was applied
    pub overlay_id: String,
    /// Whether the overlay was successfully applied
    pub applied: bool,
    /// Description of what changed (if anything)
    pub changes: Vec<String>,
}

/// Context for overlay application (e.g., environment, user preferences)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OverlayContext {
    /// Environment name (e.g., "development", "production")
    pub environment: Option<String>,
    /// User-specific settings
    pub user_settings: std::collections::HashMap<String, String>,
}

impl OverlayContext {
    /// Create context from environment variables
    #[must_use]
    pub fn from_env() -> Self {
        let environment = std::env::var("MS_ENVIRONMENT").ok();
        Self {
            environment,
            user_settings: std::collections::HashMap::new(),
        }
    }
}

/// An overlay that can modify a skill based on context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillOverlay {
    /// Unique ID for this overlay
    pub id: String,
    /// ID of the skill this overlay applies to
    pub skill_id: String,
    /// Priority for overlay application (higher = applied later)
    pub priority: i32,
    /// Conditions under which this overlay applies
    pub conditions: Vec<OverlayCondition>,
    /// Modifications to apply
    pub modifications: Vec<OverlayModification>,
}

impl SkillOverlay {
    /// Apply this overlay to a skill spec
    pub fn apply_to(
        &self,
        spec: &mut SkillSpec,
        context: &OverlayContext,
    ) -> OverlayApplicationResult {
        // Check if conditions are met
        if !self.conditions_met(context) {
            return OverlayApplicationResult {
                overlay_id: self.id.clone(),
                applied: false,
                changes: vec![],
            };
        }

        let mut changes = Vec::new();

        for modification in &self.modifications {
            match modification {
                OverlayModification::AppendDescription(text) => {
                    spec.metadata.description.push_str(text);
                    changes.push(format!("Appended to description: {text}"));
                }
                OverlayModification::AddTag(tag) => {
                    spec.metadata.tags.push(tag.clone());
                    changes.push(format!("Added tag: {tag}"));
                }
                OverlayModification::SetMetadata { key, value } => {
                    let key_str = key.as_str();
                    match key_str {
                        "author" => spec.metadata.author = Some(value.clone()),
                        "license" => spec.metadata.license = Some(value.clone()),
                        "version" => spec.metadata.version = value.clone(),
                        "name" => spec.metadata.name = value.clone(),
                        "description" => spec.metadata.description = value.clone(),
                        _ => {} // Unknown key
                    }
                    changes.push(format!("Set metadata {key}: {value}"));
                }
                OverlayModification::ReplaceBlock { block_id, content } => {
                    let mut replaced = false;
                    for section in &mut spec.sections {
                        for block in &mut section.blocks {
                            if block.id == *block_id {
                                block.content.clone_from(content);
                                replaced = true;
                            }
                        }
                    }
                    if replaced {
                        changes.push(format!("Replaced block {block_id}"));
                    }
                }
                OverlayModification::RemoveBlock { block_id } => {
                    let mut removed = false;
                    for section in &mut spec.sections {
                        let before = section.blocks.len();
                        section.blocks.retain(|b| b.id != *block_id);
                        if section.blocks.len() < before {
                            removed = true;
                        }
                    }
                    if removed {
                        changes.push(format!("Removed block {block_id}"));
                    }
                }
                OverlayModification::AppendBlock { section_id, block } => {
                    if let Some(section) = spec
                        .sections
                        .iter_mut()
                        .find(|s| s.id == *section_id || s.title.eq_ignore_ascii_case(section_id))
                    {
                        section.blocks.push(block.clone());
                    } else {
                        spec.sections.push(super::skill::SkillSection {
                            id: section_id.clone(),
                            title: section_id.clone(),
                            blocks: vec![block.clone()],
                        });
                    }
                    changes.push(format!(
                        "Appended block {} to section {section_id}",
                        block.id
                    ));
                }
            }
        }

        OverlayApplicationResult {
            overlay_id: self.id.clone(),
            applied: true,
            changes,
        }
    }

    /// Apply this overlay (alias for `apply_to`).
    pub fn apply(
        &self,
        spec: &mut SkillSpec,
        context: &OverlayContext,
    ) -> OverlayApplicationResult {
        self.apply_to(spec, context)
    }

    fn conditions_met(&self, context: &OverlayContext) -> bool {
        self.conditions.iter().all(|c| c.is_met(context))
    }
}

/// Load overlays for a given skill from `<ms_root>/overlays/<skill_id>/` and `<ms_root>/overlays/`,
/// sorted by ascending `priority`.
#[must_use]
pub fn load_overlays_for_skill(ms_root: &std::path::Path, skill_id: &str) -> Vec<SkillOverlay> {
    let mut overlays = Vec::new();
    let overlays_root = ms_root.join("overlays");
    let candidate_dirs = [overlays_root.join(skill_id), overlays_root];

    for dir in candidate_dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut paths: Vec<std::path::PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        paths.sort();

        for path in paths {
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let parsed: Option<SkillOverlay> = match ext.as_str() {
                "json" => serde_json::from_str(&content).ok(),
                "toml" => toml::from_str(&content).ok(),
                "yaml" | "yml" => serde_yaml::from_str(&content).ok(),
                _ => None,
            };
            if let Some(overlay) = parsed {
                if overlay.skill_id == skill_id || overlay.skill_id == "*" {
                    if !overlays.contains(&overlay) {
                        overlays.push(overlay);
                    }
                }
            }
        }
    }

    overlays.sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| a.id.cmp(&b.id)));
    overlays
}

/// Load and apply all matching overlays from `<ms_root>/overlays` to `spec`.
pub fn apply_overlays_from_dir(
    spec: &mut SkillSpec,
    ms_root: &std::path::Path,
    context: &OverlayContext,
) -> Vec<OverlayApplicationResult> {
    let overlays = load_overlays_for_skill(ms_root, &spec.metadata.id);
    overlays
        .into_iter()
        .map(|overlay| overlay.apply_to(spec, context))
        .collect()
}

impl PartialEq for SkillOverlay {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for SkillOverlay {}

// NOTE: Intentionally NOT implementing Ord/PartialOrd.
// Equality is based on ID, but sorting should be by priority.
// These semantics are incompatible with Rust's Ord contract.
// Use explicit sort_by(|a, b| a.priority.cmp(&b.priority)) when sorting.

/// Conditions for overlay application
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OverlayCondition {
    /// Apply only in specific environment
    Environment(String),
    /// Apply based on user setting
    UserSetting { key: String, value: String },
    /// Always apply
    Always,
}

impl OverlayCondition {
    fn is_met(&self, context: &OverlayContext) -> bool {
        match self {
            Self::Environment(env) => context.environment.as_ref() == Some(env),
            Self::UserSetting { key, value } => context.user_settings.get(key) == Some(value),
            Self::Always => true,
        }
    }
}

/// Modifications an overlay can make
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OverlayModification {
    /// Append text to description
    AppendDescription(String),
    /// Add a tag
    AddTag(String),
    /// Set arbitrary metadata
    SetMetadata { key: String, value: String },
    /// Replace a block's content by block ID
    ReplaceBlock { block_id: String, content: String },
    /// Remove a block by block ID
    RemoveBlock { block_id: String },
    /// Append a block to a section by section ID or title
    AppendBlock {
        section_id: String,
        block: super::skill::SkillBlock,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_overlay_context_from_env() {
        let ctx = OverlayContext::from_env();
        // Just verify it doesn't panic
        assert!(ctx.user_settings.is_empty());
    }

    #[test]
    fn test_condition_always() {
        let cond = OverlayCondition::Always;
        let ctx = OverlayContext::default();
        assert!(cond.is_met(&ctx));
    }

    #[test]
    fn test_overlay_priority_sorting() {
        let low = SkillOverlay {
            id: "low".into(),
            skill_id: "test".into(),
            priority: 1,
            conditions: vec![],
            modifications: vec![],
        };
        let high = SkillOverlay {
            id: "high".into(),
            skill_id: "test".into(),
            priority: 10,
            conditions: vec![],
            modifications: vec![],
        };
        // Use explicit priority comparison (not Ord trait)
        assert!(low.priority < high.priority);

        // Verify sorting works correctly
        let mut overlays = [high.clone(), low.clone()];
        overlays.sort_by(|a, b| a.priority.cmp(&b.priority));
        assert_eq!(overlays[0].id, "low");
        assert_eq!(overlays[1].id, "high");
    }

    #[test]
    fn test_overlay_equality_by_id() {
        // Equality is based on ID only, not on priority or other fields
        let overlay1 = SkillOverlay {
            id: "same-id".into(),
            skill_id: "test".into(),
            priority: 1,
            conditions: vec![],
            modifications: vec![],
        };
        let overlay2 = SkillOverlay {
            id: "same-id".into(),
            skill_id: "different".into(), // Different skill_id
            priority: 100,                // Different priority
            conditions: vec![OverlayCondition::Always],
            modifications: vec![],
        };
        // Same ID means equal, regardless of other fields
        assert_eq!(overlay1, overlay2);

        let overlay3 = SkillOverlay {
            id: "different-id".into(),
            skill_id: "test".into(),
            priority: 1,
            conditions: vec![],
            modifications: vec![],
        };
        // Different ID means not equal, even with same other fields
        assert_ne!(overlay1, overlay3);
    }

    #[test]
    fn test_overlay_set_metadata() {
        let mut spec = SkillSpec::new("test", "Original Name");
        spec.metadata.author = Some("Original Author".into());

        let overlay = SkillOverlay {
            id: "meta-update".into(),
            skill_id: "test".into(),
            priority: 1,
            conditions: vec![OverlayCondition::Always],
            modifications: vec![
                OverlayModification::SetMetadata {
                    key: "name".into(),
                    value: "New Name".into(),
                },
                OverlayModification::SetMetadata {
                    key: "author".into(),
                    value: "New Author".into(),
                },
                OverlayModification::SetMetadata {
                    key: "unknown".into(),
                    value: "ignored".into(),
                },
            ],
        };

        let context = OverlayContext::default();
        let result = overlay.apply_to(&mut spec, &context);

        assert!(result.applied);
        assert_eq!(spec.metadata.name, "New Name");
        assert_eq!(spec.metadata.author.as_deref(), Some("New Author"));
        // Check that unknown key was logged but didn't crash
        assert!(
            result
                .changes
                .iter()
                .any(|c| c.contains("Set metadata unknown"))
        );
    }

    #[test]
    fn test_overlay_block_modifications_and_load_from_dir() {
        use crate::core::skill::{BlockType, SkillBlock, SkillSection};
        let mut spec = SkillSpec::new("test-skill", "Test Skill");
        spec.sections = vec![SkillSection {
            id: "rules".into(),
            title: "Rules".into(),
            blocks: vec![
                SkillBlock {
                    id: "rules-1".into(),
                    block_type: BlockType::Rule,
                    content: "Original rule 1".into(),
                },
                SkillBlock {
                    id: "rules-2".into(),
                    block_type: BlockType::Rule,
                    content: "Obsolete rule 2".into(),
                },
            ],
        }];

        let temp = tempfile::tempdir().unwrap();
        let overlay_dir = temp.path().join("overlays").join("test-skill");
        std::fs::create_dir_all(&overlay_dir).unwrap();

        let overlay = SkillOverlay {
            id: "block-mod".into(),
            skill_id: "test-skill".into(),
            priority: 10,
            conditions: vec![OverlayCondition::Always],
            modifications: vec![
                OverlayModification::ReplaceBlock {
                    block_id: "rules-1".into(),
                    content: "Updated rule 1".into(),
                },
                OverlayModification::RemoveBlock {
                    block_id: "rules-2".into(),
                },
                OverlayModification::AppendBlock {
                    section_id: "rules".into(),
                    block: SkillBlock {
                        id: "rules-3".into(),
                        block_type: BlockType::Rule,
                        content: "Appended rule 3".into(),
                    },
                },
            ],
        };
        let yaml = serde_yaml::to_string(&overlay).unwrap();
        std::fs::write(overlay_dir.join("01-overlay.yaml"), yaml).unwrap();

        let ctx = OverlayContext::default();
        let results = apply_overlays_from_dir(&mut spec, temp.path(), &ctx);
        assert_eq!(results.len(), 1);
        assert!(results[0].applied);
        assert_eq!(spec.sections[0].blocks.len(), 2);
        assert_eq!(spec.sections[0].blocks[0].content, "Updated rule 1");
        assert_eq!(spec.sections[0].blocks[1].id, "rules-3");
        assert_eq!(spec.sections[0].blocks[1].content, "Appended rule 3");
    }
}
