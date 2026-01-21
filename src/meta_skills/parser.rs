use std::path::Path;

use crate::error::{MsError, Result};

use super::types::{MetaSkill, MetaSkillDoc};

pub struct MetaSkillParser;

impl MetaSkillParser {
    pub fn parse_str(content: &str, source: &Path) -> Result<MetaSkill> {
        let doc: MetaSkillDoc = toml::from_str(content).map_err(|err| {
            MsError::InvalidSkill(format!(
                "meta-skill parse error ({}): {err}",
                source.display()
            ))
        })?;
        doc.into_meta_skill()
    }

    pub fn parse_path(path: &Path) -> Result<MetaSkill> {
        let content = std::fs::read_to_string(path).map_err(|err| {
            MsError::InvalidSkill(format!("read meta-skill {}: {err}", path.display()))
        })?;
        Self::parse_str(&content, path)
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::{MetaDisclosureLevel, PinStrategy, SliceCondition};
    use super::*;

    #[test]
    fn parse_meta_skill_minimal() {
        let toml = r#"
            [meta_skill]
            id = "test-meta"
            name = "Test Meta"
            description = "A test meta-skill"

            [[slices]]
            skill_id = "skill-1"
            slice_ids = ["slice-a", "slice-b"]
            priority = 10
            required = true
        "#;

        let parsed = MetaSkillParser::parse_str(toml, Path::new("test.toml")).unwrap();
        assert_eq!(parsed.id, "test-meta");
        assert_eq!(parsed.slices.len(), 1);
        assert!(parsed.slices[0].required);
    }

    #[test]
    fn parse_meta_skill_full() {
        let toml = r#"
            [meta_skill]
            id = "full-meta"
            name = "Full Meta Skill"
            description = "A complete meta-skill with all fields"
            min_context_tokens = 100
            recommended_context_tokens = 500

            [meta_skill.metadata]
            author = "alice"
            version = "1.2.3"
            tags = ["rust", "cli"]
            tech_stacks = ["rust", "linux"]

            [[slices]]
            skill_id = "skill-a"
            slice_ids = ["overview", "examples"]
            level = "extended"
            priority = 5
            required = true

            [[slices]]
            skill_id = "skill-b"
            priority = 3
            required = false
        "#;

        let parsed = MetaSkillParser::parse_str(toml, Path::new("full.toml")).unwrap();
        assert_eq!(parsed.id, "full-meta");
        assert_eq!(parsed.name, "Full Meta Skill");
        assert_eq!(parsed.min_context_tokens, 100);
        assert_eq!(parsed.recommended_context_tokens, 500);
        assert_eq!(parsed.metadata.author, Some("alice".to_string()));
        assert_eq!(parsed.metadata.version, "1.2.3");
        assert_eq!(parsed.metadata.tags.len(), 2);
        assert_eq!(parsed.slices.len(), 2);
        assert_eq!(parsed.slices[0].level, Some(MetaDisclosureLevel::Extended));
    }

    #[test]
    fn parse_meta_skill_with_pin_strategy() {
        let toml = r#"
            [meta_skill]
            id = "pinned"
            name = "Pinned"
            description = "Test pin strategy"

            [meta_skill.pin_strategy]
            exact_version = "2.0.0"

            [[slices]]
            skill_id = "skill-1"
        "#;

        let parsed = MetaSkillParser::parse_str(toml, Path::new("pinned.toml")).unwrap();
        assert_eq!(
            parsed.pin_strategy,
            PinStrategy::ExactVersion("2.0.0".to_string())
        );
    }

    #[test]
    fn parse_meta_skill_with_conditions() {
        let toml = r#"
            [meta_skill]
            id = "conditional"
            name = "Conditional"
            description = "Test conditions"

            [[slices]]
            skill_id = "skill-1"

            [[slices.conditions]]
            type = "tech_stack"
            value = "rust"

            [[slices.conditions]]
            type = "file_exists"
            value = "Cargo.toml"
        "#;

        let parsed = MetaSkillParser::parse_str(toml, Path::new("cond.toml")).unwrap();
        assert_eq!(parsed.slices[0].conditions.len(), 2);

        if let SliceCondition::TechStack { value } = &parsed.slices[0].conditions[0] {
            assert_eq!(value, "rust");
        } else {
            panic!("Expected TechStack condition");
        }
    }

    #[test]
    fn parse_meta_skill_with_depends_on_condition() {
        let toml = r#"
            [meta_skill]
            id = "deps"
            name = "Dependencies"
            description = "Test depends_on"

            [[slices]]
            skill_id = "skill-1"

            [[slices]]
            skill_id = "skill-2"

            [[slices.conditions]]
            type = "depends_on"
            skill_id = "skill-1"
            slice_id = "overview"
        "#;

        let parsed = MetaSkillParser::parse_str(toml, Path::new("deps.toml")).unwrap();
        assert_eq!(parsed.slices[1].conditions.len(), 1);

        if let SliceCondition::DependsOn { skill_id, slice_id } = &parsed.slices[1].conditions[0] {
            assert_eq!(skill_id, "skill-1");
            assert_eq!(slice_id, "overview");
        } else {
            panic!("Expected DependsOn condition");
        }
    }

    #[test]
    fn parse_meta_skill_invalid_toml() {
        let toml = "not valid toml {{{{";
        let result = MetaSkillParser::parse_str(toml, Path::new("bad.toml"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("parse error"));
    }

    #[test]
    fn parse_meta_skill_missing_required_field() {
        let toml = r#"
            [meta_skill]
            id = "missing"
            name = "Missing Desc"
            # description is missing

            [[slices]]
            skill_id = "skill-1"
        "#;

        let result = MetaSkillParser::parse_str(toml, Path::new("missing.toml"));
        assert!(result.is_err());
    }

    #[test]
    fn parse_meta_skill_empty_slices_array() {
        let toml = r#"
            [meta_skill]
            id = "empty-slices"
            name = "Empty"
            description = "No slices"
        "#;

        let result = MetaSkillParser::parse_str(toml, Path::new("empty.toml"));
        // Should fail validation (requires at least one slice)
        assert!(result.is_err());
    }

    #[test]
    fn parse_meta_skill_disclosure_levels() {
        let toml = r#"
            [meta_skill]
            id = "levels"
            name = "Levels"
            description = "Test disclosure levels"

            [[slices]]
            skill_id = "skill-1"
            level = "core"

            [[slices]]
            skill_id = "skill-2"
            level = "extended"

            [[slices]]
            skill_id = "skill-3"
            level = "deep"
        "#;

        let parsed = MetaSkillParser::parse_str(toml, Path::new("levels.toml")).unwrap();
        assert_eq!(parsed.slices[0].level, Some(MetaDisclosureLevel::Core));
        assert_eq!(parsed.slices[1].level, Some(MetaDisclosureLevel::Extended));
        assert_eq!(parsed.slices[2].level, Some(MetaDisclosureLevel::Deep));
    }

    #[test]
    fn parse_meta_skill_default_values() {
        let toml = r#"
            [meta_skill]
            id = "defaults"
            name = "Defaults"
            description = "Test defaults"

            [[slices]]
            skill_id = "skill-1"
        "#;

        let parsed = MetaSkillParser::parse_str(toml, Path::new("defaults.toml")).unwrap();
        assert_eq!(parsed.pin_strategy, PinStrategy::LatestCompatible);
        assert_eq!(parsed.min_context_tokens, 0);
        assert_eq!(parsed.recommended_context_tokens, 0);
        assert_eq!(parsed.slices[0].priority, 0);
        assert!(!parsed.slices[0].required);
        assert!(parsed.slices[0].level.is_none());
        assert!(parsed.slices[0].conditions.is_empty());
    }

    #[test]
    fn parse_meta_skill_env_var_condition() {
        let toml = r#"
            [meta_skill]
            id = "env"
            name = "Env"
            description = "Test env condition"

            [[slices]]
            skill_id = "skill-1"

            [[slices.conditions]]
            type = "env_var"
            value = "DEBUG"
        "#;

        let parsed = MetaSkillParser::parse_str(toml, Path::new("env.toml")).unwrap();
        if let SliceCondition::EnvVar { value } = &parsed.slices[0].conditions[0] {
            assert_eq!(value, "DEBUG");
        } else {
            panic!("Expected EnvVar condition");
        }
    }

    #[test]
    fn parse_path_nonexistent_file() {
        let result = MetaSkillParser::parse_path(Path::new("/nonexistent/path/meta.toml"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("read meta-skill"));
    }
}
