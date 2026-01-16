//! Reference validation rules for skills.
//!
//! These rules check reference integrity, including inheritance chains,
//! cycle detection, and reference validity.

use crate::core::resolution::{detect_inheritance_cycle, CycleDetectionResult, MAX_INHERITANCE_DEPTH};
use crate::lint::config::ValidationContext;
use crate::lint::diagnostic::{Diagnostic, RuleCategory, Severity};
use crate::lint::rule::ValidationRule;

/// Rule that validates extends references exist.
pub struct ValidExtendsRule;

impl ValidationRule for ValidExtendsRule {
    fn id(&self) -> &'static str {
        "valid-extends"
    }

    fn name(&self) -> &'static str {
        "Valid Extends Reference"
    }

    fn description(&self) -> &'static str {
        "The extends field must reference an existing skill"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Reference
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn validate(&self, ctx: &ValidationContext<'_>) -> Vec<Diagnostic> {
        let Some(parent_id) = &ctx.skill.extends else {
            return vec![];
        };

        let Some(repository) = ctx.repository else {
            // Can't validate without repository access
            return vec![Diagnostic::info(
                self.id(),
                "Cannot validate extends reference without repository access",
            )
            .with_category(RuleCategory::Reference)];
        };

        match repository.get(parent_id) {
            Ok(Some(_)) => vec![], // Parent exists
            Ok(None) => {
                vec![
                    Diagnostic::error(
                        self.id(),
                        format!("Parent skill '{parent_id}' not found"),
                    )
                    .with_suggestion("Check that the parent skill ID is correct and indexed")
                    .with_category(RuleCategory::Reference),
                ]
            }
            Err(e) => {
                vec![Diagnostic::warning(
                    self.id(),
                    format!("Could not validate parent skill '{parent_id}': {e}"),
                )
                .with_category(RuleCategory::Reference)]
            }
        }
    }
}

/// Rule that detects circular inheritance.
pub struct NoCycleRule;

impl ValidationRule for NoCycleRule {
    fn id(&self) -> &'static str {
        "no-cycle"
    }

    fn name(&self) -> &'static str {
        "No Circular Dependencies"
    }

    fn description(&self) -> &'static str {
        "Skills must not form circular inheritance chains"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Reference
    }

    fn default_severity(&self) -> Severity {
        Severity::Error
    }

    fn validate(&self, ctx: &ValidationContext<'_>) -> Vec<Diagnostic> {
        // Only check if the skill has inheritance
        if ctx.skill.extends.is_none() {
            return vec![];
        }

        let Some(repository) = ctx.repository else {
            return vec![Diagnostic::info(
                self.id(),
                "Cannot check for cycles without repository access",
            )
            .with_category(RuleCategory::Reference)];
        };

        match detect_inheritance_cycle(&ctx.skill.metadata.id, repository) {
            Ok(CycleDetectionResult::NoCycle) => vec![],
            Ok(CycleDetectionResult::CycleFound(cycle)) => {
                vec![
                    Diagnostic::error(
                        self.id(),
                        format!("Circular dependency detected: {}", cycle.join(" → ")),
                    )
                    .with_suggestion("Remove one of the extends relationships to break the cycle")
                    .with_category(RuleCategory::Reference),
                ]
            }
            Err(e) => {
                vec![Diagnostic::warning(
                    self.id(),
                    format!("Could not check for cycles: {e}"),
                )
                .with_category(RuleCategory::Reference)]
            }
        }
    }
}

/// Rule that warns about deep inheritance chains.
pub struct DeepInheritanceRule {
    max_depth: usize,
}

impl Default for DeepInheritanceRule {
    fn default() -> Self {
        Self {
            max_depth: MAX_INHERITANCE_DEPTH,
        }
    }
}

impl DeepInheritanceRule {
    /// Create a new rule with custom max depth.
    #[must_use] 
    pub const fn with_max_depth(max_depth: usize) -> Self {
        Self { max_depth }
    }

    /// Calculate inheritance depth by walking the chain.
    fn calculate_depth(&self, ctx: &ValidationContext<'_>) -> Option<usize> {
        let repository = ctx.repository?;
        let mut depth = 0;
        let mut current_id = ctx.skill.metadata.id.clone();

        loop {
            let skill = repository.get(&current_id).ok()??;
            match &skill.extends {
                Some(parent_id) => {
                    depth += 1;
                    current_id = parent_id.clone();
                    // Safety limit to prevent infinite loops
                    if depth > 100 {
                        return Some(depth);
                    }
                }
                None => return Some(depth),
            }
        }
    }
}

impl ValidationRule for DeepInheritanceRule {
    fn id(&self) -> &'static str {
        "deep-inheritance"
    }

    fn name(&self) -> &'static str {
        "Deep Inheritance Warning"
    }

    fn description(&self) -> &'static str {
        "Warns about deeply nested inheritance chains"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Reference
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn validate(&self, ctx: &ValidationContext<'_>) -> Vec<Diagnostic> {
        // Only check if the skill has inheritance
        if ctx.skill.extends.is_none() {
            return vec![];
        }

        let Some(depth) = self.calculate_depth(ctx) else {
            return vec![Diagnostic::info(
                self.id(),
                "Cannot calculate inheritance depth without repository access",
            )
            .with_category(RuleCategory::Reference)];
        };

        if depth > self.max_depth {
            vec![Diagnostic::warning(
                self.id(),
                format!(
                    "Inheritance depth {} exceeds recommended maximum {}",
                    depth, self.max_depth
                ),
            )
            .with_suggestion("Consider flattening the inheritance chain or using composition")
            .with_category(RuleCategory::Reference)]
        } else {
            vec![]
        }
    }
}

/// Rule that validates format version compatibility.
pub struct FormatVersionRule;

impl ValidationRule for FormatVersionRule {
    fn id(&self) -> &'static str {
        "format-version"
    }

    fn name(&self) -> &'static str {
        "Format Version Check"
    }

    fn description(&self) -> &'static str {
        "Warns if format version is unknown or outdated"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Structure
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn validate(&self, ctx: &ValidationContext<'_>) -> Vec<Diagnostic> {
        let version = &ctx.skill.format_version;
        let current = crate::core::skill::SkillSpec::FORMAT_VERSION;

        if version.is_empty() {
            return vec![Diagnostic::warning(
                self.id(),
                "Skill has no format_version specified",
            )
            .with_suggestion(format!("Add 'format_version: {current}' to the metadata"))
            .with_category(RuleCategory::Structure)];
        }

        // Parse versions for comparison
        let skill_version: Vec<u32> = version
            .split('.')
            .filter_map(|s| s.parse().ok())
            .collect();
        let current_version: Vec<u32> = current
            .split('.')
            .filter_map(|s| s.parse().ok())
            .collect();

        if skill_version.is_empty() {
            return vec![Diagnostic::warning(
                self.id(),
                format!("Invalid format_version '{version}' (expected X.Y)"),
            )
            .with_category(RuleCategory::Structure)];
        }

        // Check if skill version is newer than current
        if skill_version > current_version {
            return vec![Diagnostic::warning(
                self.id(),
                format!(
                    "Skill format_version '{version}' is newer than current '{current}'"
                ),
            )
            .with_suggestion("This skill may use features not supported by this version")
            .with_category(RuleCategory::Structure)];
        }

        vec![]
    }
}

/// Returns all reference validation rules.
#[must_use] 
pub fn reference_rules() -> Vec<Box<dyn ValidationRule>> {
    vec![
        Box::new(ValidExtendsRule),
        Box::new(NoCycleRule),
        Box::new(DeepInheritanceRule::default()),
        Box::new(FormatVersionRule),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::resolution::SkillRepository;
    use crate::core::skill::SkillSpec;
    use crate::error::Result;
    use crate::lint::config::ValidationConfig;
    use std::collections::HashMap;

    /// Simple in-memory repository for testing
    struct TestRepository {
        skills: HashMap<String, SkillSpec>,
    }

    impl TestRepository {
        fn new() -> Self {
            Self {
                skills: HashMap::new(),
            }
        }

        fn add(&mut self, skill: SkillSpec) {
            self.skills.insert(skill.metadata.id.clone(), skill);
        }
    }

    impl SkillRepository for TestRepository {
        fn get(&self, skill_id: &str) -> Result<Option<SkillSpec>> {
            Ok(self.skills.get(skill_id).cloned())
        }
    }

    fn make_context<'a>(
        skill: &'a SkillSpec,
        config: &'a ValidationConfig,
        repo: Option<&'a dyn SkillRepository>,
    ) -> ValidationContext<'a> {
        let mut ctx = ValidationContext::new(skill, config);
        if let Some(r) = repo {
            ctx = ctx.with_repository(r);
        }
        ctx
    }

    #[test]
    fn test_valid_extends_no_parent() {
        let rule = ValidExtendsRule;
        let config = ValidationConfig::new();
        let skill = SkillSpec::new("test", "Test");
        let ctx = make_context(&skill, &config, None);

        let diagnostics = rule.validate(&ctx);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_valid_extends_parent_exists() {
        let rule = ValidExtendsRule;
        let config = ValidationConfig::new();

        let mut repo = TestRepository::new();
        let parent = SkillSpec::new("parent", "Parent Skill");
        repo.add(parent);

        let mut skill = SkillSpec::new("child", "Child Skill");
        skill.extends = Some("parent".to_string());

        let ctx = make_context(&skill, &config, Some(&repo));
        let diagnostics = rule.validate(&ctx);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_valid_extends_parent_missing() {
        let rule = ValidExtendsRule;
        let config = ValidationConfig::new();

        let repo = TestRepository::new();

        let mut skill = SkillSpec::new("child", "Child Skill");
        skill.extends = Some("nonexistent".to_string());

        let ctx = make_context(&skill, &config, Some(&repo));
        let diagnostics = rule.validate(&ctx);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("not found"));
    }

    #[test]
    fn test_no_cycle_rule_no_inheritance() {
        let rule = NoCycleRule;
        let config = ValidationConfig::new();
        let skill = SkillSpec::new("test", "Test");
        let ctx = make_context(&skill, &config, None);

        let diagnostics = rule.validate(&ctx);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_no_cycle_rule_detects_cycle() {
        let rule = NoCycleRule;
        let config = ValidationConfig::new();

        let mut repo = TestRepository::new();

        // Create a cycle: A -> B -> A
        let mut skill_a = SkillSpec::new("skill-a", "Skill A");
        skill_a.extends = Some("skill-b".to_string());

        let mut skill_b = SkillSpec::new("skill-b", "Skill B");
        skill_b.extends = Some("skill-a".to_string());

        repo.add(skill_a.clone());
        repo.add(skill_b);

        let ctx = make_context(&skill_a, &config, Some(&repo));
        let diagnostics = rule.validate(&ctx);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("Circular dependency"));
    }

    #[test]
    fn test_deep_inheritance_within_limit() {
        let rule = DeepInheritanceRule::with_max_depth(3);
        let config = ValidationConfig::new();

        let mut repo = TestRepository::new();

        // Create chain: A -> B -> C (depth 2)
        let skill_c = SkillSpec::new("skill-c", "Skill C");
        let mut skill_b = SkillSpec::new("skill-b", "Skill B");
        skill_b.extends = Some("skill-c".to_string());
        let mut skill_a = SkillSpec::new("skill-a", "Skill A");
        skill_a.extends = Some("skill-b".to_string());

        repo.add(skill_c);
        repo.add(skill_b);
        repo.add(skill_a.clone());

        let ctx = make_context(&skill_a, &config, Some(&repo));
        let diagnostics = rule.validate(&ctx);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_format_version_valid() {
        let rule = FormatVersionRule;
        let config = ValidationConfig::new();
        let skill = SkillSpec::new("test", "Test"); // Default format_version is "1.0"
        let ctx = make_context(&skill, &config, None);

        let diagnostics = rule.validate(&ctx);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_format_version_newer() {
        let rule = FormatVersionRule;
        let config = ValidationConfig::new();
        let mut skill = SkillSpec::new("test", "Test");
        skill.format_version = "99.0".to_string();
        let ctx = make_context(&skill, &config, None);

        let diagnostics = rule.validate(&ctx);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("newer"));
    }
}
