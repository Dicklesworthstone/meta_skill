//! Structural validation rules for skills.
//!
//! These rules check the structural integrity of skill specifications,
//! ensuring required fields are present and section IDs are valid.

use std::collections::HashSet;

use crate::core::skill::SkillSpec;
use crate::error::Result;
use crate::lint::config::ValidationContext;
use crate::lint::diagnostic::{Diagnostic, RuleCategory, Severity};
use crate::lint::rule::ValidationRule;

/// Rule that checks for required metadata fields.
pub struct RequiredMetadataRule;

impl ValidationRule for RequiredMetadataRule {
    fn id(&self) -> &'static str {
        "required-metadata"
    }

    fn name(&self) -> &'static str {
        "Required Metadata"
    }

    fn description(&self) -> &'static str {
        "Skills must have id, name, and description fields"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Structure
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn validate(&self, ctx: &ValidationContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let skill = ctx.skill;

        // Check for empty ID
        if skill.metadata.id.is_empty() {
            diagnostics.push(
                Diagnostic::error(self.id(), "Skill must have an 'id' field")
                    .with_suggestion("Add 'id: your-skill-id' to the metadata")
                    .with_category(RuleCategory::Structure),
            );
        }

        // Check for empty name
        if skill.metadata.name.is_empty() {
            diagnostics.push(
                Diagnostic::error(self.id(), "Skill must have a 'name' field")
                    .with_suggestion("Add 'name: Your Skill Name' to the metadata")
                    .with_category(RuleCategory::Structure),
            );
        }

        // Check for empty description (warning, not error)
        if skill.metadata.description.is_empty() {
            diagnostics.push(
                Diagnostic::warning(self.id(), "Skill should have a 'description' field")
                    .with_suggestion("Add a brief description of what this skill covers")
                    .with_fix()
                    .with_category(RuleCategory::Structure),
            );
        }

        diagnostics
    }

    fn can_fix(&self) -> bool {
        true
    }

    fn fix(&self, skill: &mut SkillSpec, diagnostic: &Diagnostic) -> Result<()> {
        if diagnostic.message.contains("description") {
            skill.metadata.description =
                format!("TODO: Add description for {}", skill.metadata.id);
            Ok(())
        } else {
            Err(crate::error::MsError::NotImplemented(
                "Cannot auto-fix id or name - please provide manually".into(),
            ))
        }
    }
}

/// Rule that checks for valid version format.
pub struct ValidVersionRule;

impl ValidationRule for ValidVersionRule {
    fn id(&self) -> &'static str {
        "valid-version"
    }

    fn name(&self) -> &'static str {
        "Valid Version"
    }

    fn description(&self) -> &'static str {
        "Version must be a valid semver string"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Structure
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn validate(&self, ctx: &ValidationContext<'_>) -> Vec<Diagnostic> {
        let version = &ctx.skill.metadata.version;

        if version.is_empty() {
            return vec![Diagnostic::warning(self.id(), "Skill should have a version")
                .with_suggestion("Add 'version: 1.0.0' to the metadata")
                .with_category(RuleCategory::Structure)];
        }

        // Simple semver check: X.Y.Z pattern
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() != 3 || !parts.iter().all(|p| p.parse::<u32>().is_ok()) {
            return vec![Diagnostic::warning(
                self.id(),
                format!("Version '{version}' is not valid semver (expected X.Y.Z)"),
            )
            .with_suggestion("Use semantic versioning like '1.0.0' or '2.1.3'")
            .with_category(RuleCategory::Structure)];
        }

        vec![]
    }
}

/// Rule that checks for unique section IDs.
pub struct UniqueSectionIdsRule;

impl ValidationRule for UniqueSectionIdsRule {
    fn id(&self) -> &'static str {
        "unique-section-ids"
    }

    fn name(&self) -> &'static str {
        "Unique Section IDs"
    }

    fn description(&self) -> &'static str {
        "All section IDs must be unique within a skill"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Structure
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn validate(&self, ctx: &ValidationContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut seen_ids: HashSet<&str> = HashSet::new();

        for section in &ctx.skill.sections {
            if !seen_ids.insert(&section.id) {
                diagnostics.push(
                    Diagnostic::error(
                        self.id(),
                        format!("Duplicate section ID: '{}'", section.id),
                    )
                    .with_suggestion("Each section must have a unique ID")
                    .with_category(RuleCategory::Structure),
                );
            }
        }

        diagnostics
    }
}

/// Rule that checks for unique block IDs within sections.
pub struct UniqueBlockIdsRule;

impl ValidationRule for UniqueBlockIdsRule {
    fn id(&self) -> &'static str {
        "unique-block-ids"
    }

    fn name(&self) -> &'static str {
        "Unique Block IDs"
    }

    fn description(&self) -> &'static str {
        "All block IDs must be unique within a section"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Structure
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn validate(&self, ctx: &ValidationContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for section in &ctx.skill.sections {
            let mut seen_ids: HashSet<&str> = HashSet::new();

            for block in &section.blocks {
                if !seen_ids.insert(&block.id) {
                    diagnostics.push(
                        Diagnostic::error(
                            self.id(),
                            format!(
                                "Duplicate block ID '{}' in section '{}'",
                                block.id, section.id
                            ),
                        )
                        .with_suggestion("Each block must have a unique ID within its section")
                        .with_category(RuleCategory::Structure),
                    );
                }
            }
        }

        diagnostics
    }
}

/// Rule that checks for non-empty block content.
pub struct NonEmptyBlocksRule;

impl ValidationRule for NonEmptyBlocksRule {
    fn id(&self) -> &'static str {
        "non-empty-blocks"
    }

    fn name(&self) -> &'static str {
        "Non-Empty Blocks"
    }

    fn description(&self) -> &'static str {
        "Blocks should have meaningful content"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Structure
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn validate(&self, ctx: &ValidationContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for section in &ctx.skill.sections {
            for block in &section.blocks {
                if block.content.trim().is_empty() {
                    diagnostics.push(
                        Diagnostic::warning(
                            self.id(),
                            format!(
                                "Block '{}' in section '{}' has no content",
                                block.id, section.id
                            ),
                        )
                        .with_suggestion("Add meaningful content or remove the empty block")
                        .with_category(RuleCategory::Structure),
                    );
                }
            }
        }

        diagnostics
    }
}

/// Returns all structural validation rules.
#[must_use] 
pub fn structural_rules() -> Vec<Box<dyn ValidationRule>> {
    vec![
        Box::new(RequiredMetadataRule),
        Box::new(ValidVersionRule),
        Box::new(UniqueSectionIdsRule),
        Box::new(UniqueBlockIdsRule),
        Box::new(NonEmptyBlocksRule),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::skill::{SkillBlock, SkillSection};
    use crate::lint::config::ValidationConfig;

    fn make_context<'a>(skill: &'a SkillSpec, config: &'a ValidationConfig) -> ValidationContext<'a> {
        ValidationContext::new(skill, config)
    }

    #[test]
    fn test_required_metadata_missing_id() {
        let rule = RequiredMetadataRule;
        let config = ValidationConfig::new();
        let skill = SkillSpec::default();
        let ctx = make_context(&skill, &config);

        let diagnostics = rule.validate(&ctx);
        assert!(diagnostics.iter().any(|d| d.message.contains("id")));
    }

    #[test]
    fn test_required_metadata_valid() {
        let rule = RequiredMetadataRule;
        let config = ValidationConfig::new();
        let mut skill = SkillSpec::new("test-skill", "Test Skill");
        skill.metadata.description = "A test skill".to_string();
        let ctx = make_context(&skill, &config);

        let diagnostics = rule.validate(&ctx);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_valid_version_semver() {
        let rule = ValidVersionRule;
        let config = ValidationConfig::new();
        let mut skill = SkillSpec::new("test", "Test");
        skill.metadata.version = "1.2.3".to_string();
        let ctx = make_context(&skill, &config);

        let diagnostics = rule.validate(&ctx);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_valid_version_invalid() {
        let rule = ValidVersionRule;
        let config = ValidationConfig::new();
        let mut skill = SkillSpec::new("test", "Test");
        skill.metadata.version = "v1.0".to_string();
        let ctx = make_context(&skill, &config);

        let diagnostics = rule.validate(&ctx);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("not valid semver"));
    }

    #[test]
    fn test_unique_section_ids_valid() {
        let rule = UniqueSectionIdsRule;
        let config = ValidationConfig::new();
        let mut skill = SkillSpec::new("test", "Test");
        skill.sections = vec![
            SkillSection {
                id: "section-1".to_string(),
                title: "Section 1".to_string(),
                blocks: vec![],
            },
            SkillSection {
                id: "section-2".to_string(),
                title: "Section 2".to_string(),
                blocks: vec![],
            },
        ];
        let ctx = make_context(&skill, &config);

        let diagnostics = rule.validate(&ctx);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_unique_section_ids_duplicate() {
        let rule = UniqueSectionIdsRule;
        let config = ValidationConfig::new();
        let mut skill = SkillSpec::new("test", "Test");
        skill.sections = vec![
            SkillSection {
                id: "section-1".to_string(),
                title: "Section 1".to_string(),
                blocks: vec![],
            },
            SkillSection {
                id: "section-1".to_string(), // Duplicate!
                title: "Section 1 Again".to_string(),
                blocks: vec![],
            },
        ];
        let ctx = make_context(&skill, &config);

        let diagnostics = rule.validate(&ctx);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("Duplicate section ID"));
    }

    #[test]
    fn test_unique_block_ids() {
        let rule = UniqueBlockIdsRule;
        let config = ValidationConfig::new();
        let mut skill = SkillSpec::new("test", "Test");
        skill.sections = vec![SkillSection {
            id: "section-1".to_string(),
            title: "Section 1".to_string(),
            blocks: vec![
                SkillBlock {
                    id: "block-1".to_string(),
                    block_type: Default::default(),
                    content: "Content".to_string(),
                },
                SkillBlock {
                    id: "block-1".to_string(), // Duplicate!
                    block_type: Default::default(),
                    content: "More content".to_string(),
                },
            ],
        }];
        let ctx = make_context(&skill, &config);

        let diagnostics = rule.validate(&ctx);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("Duplicate block ID"));
    }

    #[test]
    fn test_non_empty_blocks() {
        let rule = NonEmptyBlocksRule;
        let config = ValidationConfig::new();
        let mut skill = SkillSpec::new("test", "Test");
        skill.sections = vec![SkillSection {
            id: "section-1".to_string(),
            title: "Section 1".to_string(),
            blocks: vec![SkillBlock {
                id: "block-1".to_string(),
                block_type: Default::default(),
                content: "   ".to_string(), // Empty!
            }],
        }];
        let ctx = make_context(&skill, &config);

        let diagnostics = rule.validate(&ctx);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("no content"));
    }
}
