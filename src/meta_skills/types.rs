use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::{MsError, Result};

/// A meta-skill is a curated bundle of slices from one or more skills.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub slices: Vec<MetaSkillSliceRef>,
    #[serde(default)]
    pub pin_strategy: PinStrategy,
    #[serde(default)]
    pub metadata: MetaSkillMetadata,
    #[serde(default)]
    pub min_context_tokens: usize,
    #[serde(default)]
    pub recommended_context_tokens: usize,
}

impl MetaSkill {
    pub fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() {
            return Err(MsError::ValidationFailed(
                "meta-skill id must be non-empty".to_string(),
            ));
        }
        if self.name.trim().is_empty() {
            return Err(MsError::ValidationFailed(
                "meta-skill name must be non-empty".to_string(),
            ));
        }
        if self.description.trim().is_empty() {
            return Err(MsError::ValidationFailed(
                "meta-skill description must be non-empty".to_string(),
            ));
        }
        if self.slices.is_empty() {
            return Err(MsError::ValidationFailed(
                "meta-skill must include at least one slice".to_string(),
            ));
        }
        if self.recommended_context_tokens > 0
            && self.min_context_tokens > 0
            && self.recommended_context_tokens < self.min_context_tokens
        {
            return Err(MsError::ValidationFailed(
                "recommended_context_tokens must be >= min_context_tokens".to_string(),
            ));
        }
        for slice in &self.slices {
            slice.validate()?;
        }
        Ok(())
    }
}

/// Metadata for meta-skill discovery and categorization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaSkillMetadata {
    pub author: Option<String>,
    pub version: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub tech_stacks: Vec<String>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl Default for MetaSkillMetadata {
    fn default() -> Self {
        Self {
            author: None,
            version: "0.1.0".to_string(),
            tags: Vec::new(),
            tech_stacks: Vec::new(),
            updated_at: None,
        }
    }
}

/// A reference to slices within a skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaSkillSliceRef {
    pub skill_id: String,
    #[serde(default)]
    pub slice_ids: Vec<String>,
    pub level: Option<MetaDisclosureLevel>,
    #[serde(default)]
    pub priority: u8,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub conditions: Vec<SliceCondition>,
}

impl MetaSkillSliceRef {
    pub fn validate(&self) -> Result<()> {
        if self.skill_id.trim().is_empty() {
            return Err(MsError::ValidationFailed(
                "slice ref skill_id must be non-empty".to_string(),
            ));
        }
        Ok(())
    }
}

/// Disclosure level for meta-skill slices.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MetaDisclosureLevel {
    Core,
    Extended,
    Deep,
}

/// Conditions for conditional slice inclusion.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SliceCondition {
    TechStack { value: String },
    FileExists { value: String },
    EnvVar { value: String },
    DependsOn { skill_id: String, slice_id: String },
}

/// Strategy for resolving skill versions when loading meta-skills.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum PinStrategy {
    #[default]
    LatestCompatible,
    ExactVersion(String),
    FloatingMajor,
    LocalInstalled,
    PerSkill(HashMap<String, String>),
}


/// TOML document for meta-skill definitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaSkillDoc {
    pub meta_skill: MetaSkillHeader,
    #[serde(default)]
    pub slices: Vec<MetaSkillSliceRef>,
}

impl MetaSkillDoc {
    pub fn into_meta_skill(self) -> Result<MetaSkill> {
        let meta_skill = MetaSkill {
            id: self.meta_skill.id,
            name: self.meta_skill.name,
            description: self.meta_skill.description,
            slices: self.slices,
            pin_strategy: self.meta_skill.pin_strategy,
            metadata: self.meta_skill.metadata,
            min_context_tokens: self.meta_skill.min_context_tokens,
            recommended_context_tokens: self.meta_skill.recommended_context_tokens,
        };
        meta_skill.validate()?;
        Ok(meta_skill)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaSkillHeader {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub pin_strategy: PinStrategy,
    #[serde(default)]
    pub metadata: MetaSkillMetadata,
    #[serde(default)]
    pub min_context_tokens: usize,
    #[serde(default)]
    pub recommended_context_tokens: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_skill_validation_rejects_missing_fields() {
        let meta = MetaSkill {
            id: "".to_string(),
            name: "".to_string(),
            description: "".to_string(),
            slices: vec![],
            pin_strategy: PinStrategy::LatestCompatible,
            metadata: MetaSkillMetadata::default(),
            min_context_tokens: 0,
            recommended_context_tokens: 0,
        };
        assert!(meta.validate().is_err());
    }

    #[test]
    fn slice_ref_requires_skill_id() {
        let slice = MetaSkillSliceRef {
            skill_id: "".to_string(),
            slice_ids: vec![],
            level: None,
            priority: 0,
            required: false,
            conditions: vec![],
        };
        assert!(slice.validate().is_err());
    }
}
