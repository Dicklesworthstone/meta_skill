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

    // =========================================
    // Test Helpers
    // =========================================

    fn valid_slice_ref() -> MetaSkillSliceRef {
        MetaSkillSliceRef {
            skill_id: "skill-1".to_string(),
            slice_ids: vec!["slice-a".to_string()],
            level: None,
            priority: 0,
            required: false,
            conditions: vec![],
        }
    }

    fn valid_meta_skill() -> MetaSkill {
        MetaSkill {
            id: "test-meta".to_string(),
            name: "Test Meta".to_string(),
            description: "A test meta-skill".to_string(),
            slices: vec![valid_slice_ref()],
            pin_strategy: PinStrategy::LatestCompatible,
            metadata: MetaSkillMetadata::default(),
            min_context_tokens: 0,
            recommended_context_tokens: 0,
        }
    }

    // =========================================
    // MetaSkill Validation Tests
    // =========================================

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
    fn meta_skill_validation_rejects_empty_id() {
        let mut meta = valid_meta_skill();
        meta.id = "".to_string();
        let err = meta.validate().unwrap_err();
        assert!(err.to_string().contains("id"));
    }

    #[test]
    fn meta_skill_validation_rejects_whitespace_id() {
        let mut meta = valid_meta_skill();
        meta.id = "   ".to_string();
        assert!(meta.validate().is_err());
    }

    #[test]
    fn meta_skill_validation_rejects_empty_name() {
        let mut meta = valid_meta_skill();
        meta.name = "".to_string();
        let err = meta.validate().unwrap_err();
        assert!(err.to_string().contains("name"));
    }

    #[test]
    fn meta_skill_validation_rejects_empty_description() {
        let mut meta = valid_meta_skill();
        meta.description = "".to_string();
        let err = meta.validate().unwrap_err();
        assert!(err.to_string().contains("description"));
    }

    #[test]
    fn meta_skill_validation_rejects_empty_slices() {
        let mut meta = valid_meta_skill();
        meta.slices = vec![];
        let err = meta.validate().unwrap_err();
        assert!(err.to_string().contains("slice"));
    }

    #[test]
    fn meta_skill_validation_rejects_invalid_token_bounds() {
        let mut meta = valid_meta_skill();
        meta.min_context_tokens = 1000;
        meta.recommended_context_tokens = 500;
        let err = meta.validate().unwrap_err();
        assert!(err.to_string().contains("token"));
    }

    #[test]
    fn meta_skill_validation_allows_zero_token_bounds() {
        let mut meta = valid_meta_skill();
        meta.min_context_tokens = 0;
        meta.recommended_context_tokens = 0;
        assert!(meta.validate().is_ok());
    }

    #[test]
    fn meta_skill_validation_allows_equal_token_bounds() {
        let mut meta = valid_meta_skill();
        meta.min_context_tokens = 100;
        meta.recommended_context_tokens = 100;
        assert!(meta.validate().is_ok());
    }

    #[test]
    fn meta_skill_validation_passes_for_valid() {
        let meta = valid_meta_skill();
        assert!(meta.validate().is_ok());
    }

    #[test]
    fn meta_skill_validation_validates_slices() {
        let mut meta = valid_meta_skill();
        meta.slices = vec![MetaSkillSliceRef {
            skill_id: "".to_string(), // Invalid
            slice_ids: vec![],
            level: None,
            priority: 0,
            required: false,
            conditions: vec![],
        }];
        assert!(meta.validate().is_err());
    }

    // =========================================
    // MetaSkillSliceRef Tests
    // =========================================

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

    #[test]
    fn slice_ref_rejects_whitespace_skill_id() {
        let slice = MetaSkillSliceRef {
            skill_id: "   ".to_string(),
            slice_ids: vec![],
            level: None,
            priority: 0,
            required: false,
            conditions: vec![],
        };
        assert!(slice.validate().is_err());
    }

    #[test]
    fn slice_ref_allows_empty_slice_ids() {
        let slice = MetaSkillSliceRef {
            skill_id: "skill-1".to_string(),
            slice_ids: vec![], // Empty is fine - means select by level
            level: None,
            priority: 0,
            required: false,
            conditions: vec![],
        };
        assert!(slice.validate().is_ok());
    }

    #[test]
    fn slice_ref_clone() {
        let slice = valid_slice_ref();
        let cloned = slice.clone();
        assert_eq!(cloned.skill_id, slice.skill_id);
        assert_eq!(cloned.priority, slice.priority);
    }

    #[test]
    fn slice_ref_debug() {
        let slice = valid_slice_ref();
        let debug = format!("{:?}", slice);
        assert!(debug.contains("MetaSkillSliceRef"));
        assert!(debug.contains("skill-1"));
    }

    // =========================================
    // MetaSkillMetadata Tests
    // =========================================

    #[test]
    fn metadata_default_values() {
        let meta = MetaSkillMetadata::default();
        assert!(meta.author.is_none());
        assert_eq!(meta.version, "0.1.0");
        assert!(meta.tags.is_empty());
        assert!(meta.tech_stacks.is_empty());
        assert!(meta.updated_at.is_none());
    }

    #[test]
    fn metadata_clone() {
        let meta = MetaSkillMetadata {
            author: Some("alice".to_string()),
            version: "1.0.0".to_string(),
            tags: vec!["tag1".to_string()],
            tech_stacks: vec!["rust".to_string()],
            updated_at: None,
        };
        let cloned = meta.clone();
        assert_eq!(cloned.author, meta.author);
        assert_eq!(cloned.version, meta.version);
    }

    #[test]
    fn metadata_debug() {
        let meta = MetaSkillMetadata::default();
        let debug = format!("{:?}", meta);
        assert!(debug.contains("MetaSkillMetadata"));
    }

    // =========================================
    // MetaDisclosureLevel Tests
    // =========================================

    #[test]
    fn disclosure_level_values() {
        assert_ne!(MetaDisclosureLevel::Core, MetaDisclosureLevel::Extended);
        assert_ne!(MetaDisclosureLevel::Extended, MetaDisclosureLevel::Deep);
        assert_ne!(MetaDisclosureLevel::Core, MetaDisclosureLevel::Deep);
    }

    #[test]
    fn disclosure_level_copy() {
        let level = MetaDisclosureLevel::Core;
        let copied = level;
        assert_eq!(copied, MetaDisclosureLevel::Core);
    }

    #[test]
    fn disclosure_level_clone() {
        let level = MetaDisclosureLevel::Extended;
        let cloned = level.clone();
        assert_eq!(cloned, MetaDisclosureLevel::Extended);
    }

    #[test]
    fn disclosure_level_debug() {
        let debug = format!("{:?}", MetaDisclosureLevel::Deep);
        assert!(debug.contains("Deep"));
    }

    // =========================================
    // PinStrategy Tests
    // =========================================

    #[test]
    fn pin_strategy_default() {
        let strategy = PinStrategy::default();
        assert_eq!(strategy, PinStrategy::LatestCompatible);
    }

    #[test]
    fn pin_strategy_exact_version() {
        let strategy = PinStrategy::ExactVersion("1.2.3".to_string());
        if let PinStrategy::ExactVersion(v) = strategy {
            assert_eq!(v, "1.2.3");
        } else {
            panic!("Expected ExactVersion");
        }
    }

    #[test]
    fn pin_strategy_per_skill() {
        let mut map = HashMap::new();
        map.insert("skill-a".to_string(), "1.0.0".to_string());
        let strategy = PinStrategy::PerSkill(map);
        if let PinStrategy::PerSkill(m) = strategy {
            assert_eq!(m.get("skill-a"), Some(&"1.0.0".to_string()));
        } else {
            panic!("Expected PerSkill");
        }
    }

    #[test]
    fn pin_strategy_equality() {
        assert_eq!(PinStrategy::LatestCompatible, PinStrategy::LatestCompatible);
        assert_eq!(PinStrategy::FloatingMajor, PinStrategy::FloatingMajor);
        assert_eq!(PinStrategy::LocalInstalled, PinStrategy::LocalInstalled);
        assert_ne!(PinStrategy::LatestCompatible, PinStrategy::FloatingMajor);
    }

    #[test]
    fn pin_strategy_clone() {
        let strategy = PinStrategy::ExactVersion("2.0.0".to_string());
        let cloned = strategy.clone();
        assert_eq!(cloned, PinStrategy::ExactVersion("2.0.0".to_string()));
    }

    // =========================================
    // SliceCondition Tests
    // =========================================

    #[test]
    fn slice_condition_tech_stack() {
        let cond = SliceCondition::TechStack {
            value: "rust".to_string(),
        };
        if let SliceCondition::TechStack { value } = cond {
            assert_eq!(value, "rust");
        }
    }

    #[test]
    fn slice_condition_file_exists() {
        let cond = SliceCondition::FileExists {
            value: "Cargo.toml".to_string(),
        };
        if let SliceCondition::FileExists { value } = cond {
            assert_eq!(value, "Cargo.toml");
        }
    }

    #[test]
    fn slice_condition_env_var() {
        let cond = SliceCondition::EnvVar {
            value: "HOME".to_string(),
        };
        if let SliceCondition::EnvVar { value } = cond {
            assert_eq!(value, "HOME");
        }
    }

    #[test]
    fn slice_condition_depends_on() {
        let cond = SliceCondition::DependsOn {
            skill_id: "skill-a".to_string(),
            slice_id: "slice-1".to_string(),
        };
        if let SliceCondition::DependsOn { skill_id, slice_id } = cond {
            assert_eq!(skill_id, "skill-a");
            assert_eq!(slice_id, "slice-1");
        }
    }

    #[test]
    fn slice_condition_clone() {
        let cond = SliceCondition::TechStack {
            value: "go".to_string(),
        };
        let cloned = cond.clone();
        if let SliceCondition::TechStack { value } = cloned {
            assert_eq!(value, "go");
        }
    }

    #[test]
    fn slice_condition_debug() {
        let cond = SliceCondition::EnvVar {
            value: "PATH".to_string(),
        };
        let debug = format!("{:?}", cond);
        assert!(debug.contains("EnvVar"));
        assert!(debug.contains("PATH"));
    }

    // =========================================
    // MetaSkillDoc Tests
    // =========================================

    #[test]
    fn meta_skill_doc_into_meta_skill() {
        let doc = MetaSkillDoc {
            meta_skill: MetaSkillHeader {
                id: "doc-meta".to_string(),
                name: "Doc Meta".to_string(),
                description: "From doc".to_string(),
                pin_strategy: PinStrategy::FloatingMajor,
                metadata: MetaSkillMetadata::default(),
                min_context_tokens: 100,
                recommended_context_tokens: 500,
            },
            slices: vec![valid_slice_ref()],
        };

        let meta = doc.into_meta_skill().unwrap();
        assert_eq!(meta.id, "doc-meta");
        assert_eq!(meta.pin_strategy, PinStrategy::FloatingMajor);
        assert_eq!(meta.min_context_tokens, 100);
    }

    #[test]
    fn meta_skill_doc_validates_on_conversion() {
        let doc = MetaSkillDoc {
            meta_skill: MetaSkillHeader {
                id: "".to_string(), // Invalid
                name: "Name".to_string(),
                description: "Desc".to_string(),
                pin_strategy: PinStrategy::default(),
                metadata: MetaSkillMetadata::default(),
                min_context_tokens: 0,
                recommended_context_tokens: 0,
            },
            slices: vec![valid_slice_ref()],
        };

        assert!(doc.into_meta_skill().is_err());
    }

    #[test]
    fn meta_skill_doc_clone() {
        let doc = MetaSkillDoc {
            meta_skill: MetaSkillHeader {
                id: "clone-test".to_string(),
                name: "Clone".to_string(),
                description: "Test".to_string(),
                pin_strategy: PinStrategy::default(),
                metadata: MetaSkillMetadata::default(),
                min_context_tokens: 0,
                recommended_context_tokens: 0,
            },
            slices: vec![],
        };
        let cloned = doc.clone();
        assert_eq!(cloned.meta_skill.id, "clone-test");
    }

    // =========================================
    // MetaSkillHeader Tests
    // =========================================

    #[test]
    fn meta_skill_header_debug() {
        let header = MetaSkillHeader {
            id: "hdr".to_string(),
            name: "Header".to_string(),
            description: "A header".to_string(),
            pin_strategy: PinStrategy::default(),
            metadata: MetaSkillMetadata::default(),
            min_context_tokens: 0,
            recommended_context_tokens: 0,
        };
        let debug = format!("{:?}", header);
        assert!(debug.contains("MetaSkillHeader"));
        assert!(debug.contains("hdr"));
    }

    // =========================================
    // Serde Serialization Tests
    // =========================================

    #[test]
    fn meta_skill_roundtrip_json() {
        let meta = valid_meta_skill();
        let json = serde_json::to_string(&meta).unwrap();
        let restored: MetaSkill = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.id, meta.id);
        assert_eq!(restored.name, meta.name);
    }

    #[test]
    fn pin_strategy_serializes_correctly() {
        let strategy = PinStrategy::ExactVersion("3.0.0".to_string());
        let json = serde_json::to_string(&strategy).unwrap();
        assert!(json.contains("exact_version"));
        assert!(json.contains("3.0.0"));
    }

    #[test]
    fn disclosure_level_serializes_correctly() {
        let level = MetaDisclosureLevel::Extended;
        let json = serde_json::to_string(&level).unwrap();
        assert!(json.contains("extended"));
    }

    #[test]
    fn slice_condition_serializes_correctly() {
        let cond = SliceCondition::TechStack {
            value: "python".to_string(),
        };
        let json = serde_json::to_string(&cond).unwrap();
        assert!(json.contains("tech_stack"));
        assert!(json.contains("python"));
    }

    #[test]
    fn metadata_with_updated_at() {
        let meta = MetaSkillMetadata {
            author: Some("test".to_string()),
            version: "1.0.0".to_string(),
            tags: vec![],
            tech_stacks: vec![],
            updated_at: Some(chrono::Utc::now()),
        };
        let json = serde_json::to_string(&meta).unwrap();
        assert!(json.contains("updated_at"));
    }
}
