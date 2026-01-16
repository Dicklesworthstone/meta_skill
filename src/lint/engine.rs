//! Validation engine for running rules against skills.

use crate::core::skill::SkillSpec;
use crate::error::{MsError, Result};

use super::config::{ValidationConfig, ValidationContext};
use super::diagnostic::{Diagnostic, RuleCategory, Severity};
use super::rule::BoxedRule;

/// Result of validation
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// All diagnostics collected
    pub diagnostics: Vec<Diagnostic>,
    /// Whether validation was truncated due to `max_errors`
    pub truncated: bool,
    /// Whether validation passed (no errors)
    pub passed: bool,
}

impl ValidationResult {
    /// Create a new empty result
    #[must_use] 
    pub const fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
            truncated: false,
            passed: true,
        }
    }

    /// Get error diagnostics
    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
    }

    /// Get warning diagnostics
    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Warning)
    }

    /// Get info diagnostics
    pub fn infos(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Info)
    }

    /// Filter diagnostics by category
    pub fn by_category(&self, category: RuleCategory) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics
            .iter()
            .filter(move |d| d.category == category)
    }

    /// Get count of errors
    #[must_use] 
    pub fn error_count(&self) -> usize {
        self.errors().count()
    }

    /// Get count of warnings
    #[must_use] 
    pub fn warning_count(&self) -> usize {
        self.warnings().count()
    }

    /// Get total count of diagnostics
    #[must_use] 
    pub fn total_count(&self) -> usize {
        self.diagnostics.len()
    }
}

impl Default for ValidationResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of auto-fix operation
#[derive(Debug, Clone)]
pub struct FixResult {
    /// Rule IDs that were successfully fixed
    pub fixed: Vec<String>,
    /// Rule IDs that failed to fix with error message
    pub failed: Vec<(String, String)>,
}

impl FixResult {
    /// Create a new empty fix result
    #[must_use] 
    pub const fn new() -> Self {
        Self {
            fixed: Vec::new(),
            failed: Vec::new(),
        }
    }

    /// Check if all fixes succeeded
    #[must_use] 
    pub fn all_succeeded(&self) -> bool {
        self.failed.is_empty()
    }

    /// Get number of successful fixes
    #[must_use] 
    pub fn fixed_count(&self) -> usize {
        self.fixed.len()
    }
}

impl Default for FixResult {
    fn default() -> Self {
        Self::new()
    }
}

/// The validation engine that manages and runs rules
pub struct ValidationEngine {
    rules: Vec<BoxedRule>,
    config: ValidationConfig,
}

impl ValidationEngine {
    /// Create a new validation engine with the given config
    #[must_use] 
    pub fn new(config: ValidationConfig) -> Self {
        Self {
            rules: Vec::new(),
            config,
        }
    }

    /// Create a new engine with default config
    #[must_use] 
    pub fn with_defaults() -> Self {
        Self::new(ValidationConfig::default())
    }

    /// Register a validation rule
    pub fn register(&mut self, rule: BoxedRule) {
        self.rules.push(rule);
    }

    /// Register a validation rule (builder pattern)
    #[must_use] 
    pub fn with_rule(mut self, rule: BoxedRule) -> Self {
        self.register(rule);
        self
    }

    /// Get registered rules
    #[must_use] 
    pub fn rules(&self) -> &[BoxedRule] {
        &self.rules
    }

    /// Get the config
    #[must_use] 
    pub const fn config(&self) -> &ValidationConfig {
        &self.config
    }

    /// Set the config
    pub fn set_config(&mut self, config: ValidationConfig) {
        self.config = config;
    }

    /// Validate a skill spec
    #[must_use] 
    pub fn validate(&self, skill: &SkillSpec) -> ValidationResult {
        let ctx = ValidationContext::new(skill, &self.config);
        self.validate_with_context(&ctx)
    }

    /// Validate with a custom context
    #[must_use] 
    pub fn validate_with_context(&self, ctx: &ValidationContext<'_>) -> ValidationResult {
        let mut result = ValidationResult::new();
        let mut error_count = 0;

        for rule in &self.rules {
            // Skip disabled rules
            if self.config.is_rule_disabled(rule.id()) {
                continue;
            }

            let rule_diagnostics = rule.validate(ctx);

            for mut diag in rule_diagnostics {
                // Apply severity override and strict mode
                diag.severity = self
                    .config
                    .effective_severity(&diag.rule_id, diag.severity);

                if diag.severity == Severity::Error {
                    error_count += 1;
                }

                result.diagnostics.push(diag);

                // Check max errors
                if let Some(max) = self.config.max_errors {
                    if error_count >= max {
                        result.truncated = true;
                        result.passed = false;
                        return result;
                    }
                }
            }
        }

        result.passed = error_count == 0;
        result
    }

    /// Apply auto-fixes to a skill
    pub fn auto_fix(&self, skill: &mut SkillSpec) -> Result<FixResult> {
        let mut result = FixResult::new();

        // First, collect all diagnostics that can be fixed
        let ctx = ValidationContext::new(skill, &self.config);
        let diagnostics: Vec<(String, Diagnostic)> = self
            .rules
            .iter()
            .filter(|r| r.can_fix() && !self.config.is_rule_disabled(r.id()))
            .flat_map(|r| {
                r.validate(&ctx)
                    .into_iter()
                    .filter(|d| d.fix_available)
                    .map(|d| (r.id().to_string(), d))
            })
            .collect();

        // Apply fixes
        for (rule_id, diagnostic) in diagnostics {
            let rule = self
                .rules
                .iter()
                .find(|r| r.id() == rule_id)
                .ok_or_else(|| MsError::NotFound(format!("Rule '{rule_id}' not found")))?;

            match rule.fix(skill, &diagnostic) {
                Ok(()) => result.fixed.push(rule_id),
                Err(e) => result.failed.push((rule_id, e.to_string())),
            }
        }

        Ok(result)
    }

    /// List all registered rules
    #[must_use] 
    pub fn list_rules(&self) -> Vec<RuleInfo> {
        self.rules
            .iter()
            .map(|r| RuleInfo {
                id: r.id().to_string(),
                name: r.name().to_string(),
                description: r.description().to_string(),
                category: r.category(),
                default_severity: r.default_severity(),
                can_fix: r.can_fix(),
                disabled: self.config.is_rule_disabled(r.id()),
            })
            .collect()
    }
}

/// Information about a registered rule
#[derive(Debug, Clone)]
pub struct RuleInfo {
    /// Rule ID
    pub id: String,
    /// Rule name
    pub name: String,
    /// Rule description
    pub description: String,
    /// Rule category
    pub category: RuleCategory,
    /// Default severity
    pub default_severity: Severity,
    /// Whether the rule supports auto-fix
    pub can_fix: bool,
    /// Whether the rule is disabled
    pub disabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::config::ValidationConfig;
    use crate::lint::rule::ValidationRule;

    // Test rules
    struct EmptyIdRule;

    impl ValidationRule for EmptyIdRule {
        fn id(&self) -> &str {
            "no-empty-id"
        }
        fn name(&self) -> &str {
            "No Empty ID"
        }
        fn description(&self) -> &str {
            "Skill ID must not be empty"
        }
        fn category(&self) -> RuleCategory {
            RuleCategory::Structure
        }
        fn default_severity(&self) -> Severity {
            Severity::Error
        }
        fn validate(&self, ctx: &ValidationContext<'_>) -> Vec<Diagnostic> {
            if ctx.skill.metadata.id.is_empty() {
                vec![Diagnostic::error(self.id(), "Skill ID is empty")
                    .with_category(RuleCategory::Structure)]
            } else {
                vec![]
            }
        }
    }

    struct EmptyDescriptionRule;

    impl ValidationRule for EmptyDescriptionRule {
        fn id(&self) -> &str {
            "no-empty-description"
        }
        fn name(&self) -> &str {
            "No Empty Description"
        }
        fn description(&self) -> &str {
            "Skill description should not be empty"
        }
        fn category(&self) -> RuleCategory {
            RuleCategory::Quality
        }
        fn default_severity(&self) -> Severity {
            Severity::Warning
        }
        fn validate(&self, ctx: &ValidationContext<'_>) -> Vec<Diagnostic> {
            if ctx.skill.metadata.description.is_empty() {
                vec![Diagnostic::warning(self.id(), "Skill description is empty")
                    .with_category(RuleCategory::Quality)]
            } else {
                vec![]
            }
        }
    }

    #[test]
    fn test_engine_register_rules() {
        let mut engine = ValidationEngine::with_defaults();
        engine.register(Box::new(EmptyIdRule));
        engine.register(Box::new(EmptyDescriptionRule));

        assert_eq!(engine.rules().len(), 2);
    }

    #[test]
    fn test_engine_validate_empty_skill() {
        let mut engine = ValidationEngine::with_defaults();
        engine.register(Box::new(EmptyIdRule));
        engine.register(Box::new(EmptyDescriptionRule));

        let skill = SkillSpec::default();
        let result = engine.validate(&skill);

        assert!(!result.passed);
        assert_eq!(result.error_count(), 1); // Empty ID is error
        assert_eq!(result.warning_count(), 1); // Empty description is warning
    }

    #[test]
    fn test_engine_validate_valid_skill() {
        let mut engine = ValidationEngine::with_defaults();
        engine.register(Box::new(EmptyIdRule));
        engine.register(Box::new(EmptyDescriptionRule));

        let mut skill = SkillSpec::new("valid-id", "Valid Skill");
        skill.metadata.description = "A valid description".to_string();
        let result = engine.validate(&skill);

        assert!(result.passed);
        assert_eq!(result.total_count(), 0);
    }

    #[test]
    fn test_engine_disabled_rules() {
        let config = ValidationConfig::new().disable_rule("no-empty-id");
        let mut engine = ValidationEngine::new(config);
        engine.register(Box::new(EmptyIdRule));

        let skill = SkillSpec::default();
        let result = engine.validate(&skill);

        // ID rule is disabled, so no errors
        assert!(result.passed);
    }

    #[test]
    fn test_engine_strict_mode() {
        let config = ValidationConfig::new().strict();
        let mut engine = ValidationEngine::new(config);
        engine.register(Box::new(EmptyDescriptionRule));

        let skill = SkillSpec::new("test", "Test");
        let result = engine.validate(&skill);

        // Warning should be elevated to error in strict mode
        assert!(!result.passed);
        assert_eq!(result.error_count(), 1);
    }

    #[test]
    fn test_engine_max_errors() {
        let config = ValidationConfig::new().with_max_errors(1);
        let mut engine = ValidationEngine::new(config);
        engine.register(Box::new(EmptyIdRule));
        engine.register(Box::new(EmptyDescriptionRule));

        // Both rules would trigger, but max_errors=1 should stop after first error
        let config_strict = ValidationConfig::new().strict().with_max_errors(1);
        let mut engine_strict = ValidationEngine::new(config_strict);
        engine_strict.register(Box::new(EmptyIdRule));
        engine_strict.register(Box::new(EmptyDescriptionRule));

        let skill = SkillSpec::default();
        let result = engine_strict.validate(&skill);

        assert!(result.truncated);
        assert!(!result.passed);
    }

    #[test]
    fn test_engine_list_rules() {
        let config = ValidationConfig::new().disable_rule("no-empty-id");
        let mut engine = ValidationEngine::new(config);
        engine.register(Box::new(EmptyIdRule));
        engine.register(Box::new(EmptyDescriptionRule));

        let rules = engine.list_rules();
        assert_eq!(rules.len(), 2);

        let id_rule = rules.iter().find(|r| r.id == "no-empty-id").unwrap();
        assert!(id_rule.disabled);

        let desc_rule = rules
            .iter()
            .find(|r| r.id == "no-empty-description")
            .unwrap();
        assert!(!desc_rule.disabled);
    }

    #[test]
    fn test_validation_result_filters() {
        let mut result = ValidationResult::new();
        result.diagnostics.push(
            Diagnostic::error("rule1", "Error 1").with_category(RuleCategory::Structure),
        );
        result.diagnostics.push(
            Diagnostic::warning("rule2", "Warning 1").with_category(RuleCategory::Quality),
        );
        result.diagnostics.push(
            Diagnostic::info("rule3", "Info 1").with_category(RuleCategory::Structure),
        );

        assert_eq!(result.errors().count(), 1);
        assert_eq!(result.warnings().count(), 1);
        assert_eq!(result.infos().count(), 1);
        assert_eq!(result.by_category(RuleCategory::Structure).count(), 2);
        assert_eq!(result.by_category(RuleCategory::Quality).count(), 1);
    }
}
