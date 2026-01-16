//! Validation rule trait and base implementations.

use crate::core::skill::SkillSpec;
use crate::error::Result;

use super::config::ValidationContext;
use super::diagnostic::{Diagnostic, RuleCategory, Severity};

/// A validation rule that checks skills for issues.
///
/// Rules should be stateless and reusable. All state needed for validation
/// should be passed through the `ValidationContext`.
pub trait ValidationRule: Send + Sync {
    /// Unique identifier for this rule (e.g., "no-empty-id")
    fn id(&self) -> &str;

    /// Human-readable name
    fn name(&self) -> &str;

    /// Detailed description of what this rule checks
    fn description(&self) -> &str;

    /// Category this rule belongs to
    fn category(&self) -> RuleCategory;

    /// Default severity level
    fn default_severity(&self) -> Severity;

    /// Run the validation check
    fn validate(&self, ctx: &ValidationContext<'_>) -> Vec<Diagnostic>;

    /// Can this rule auto-fix issues?
    fn can_fix(&self) -> bool {
        false
    }

    /// Apply auto-fix for issues (if can_fix() is true).
    /// Returns Ok(()) if fix was applied successfully.
    fn fix(&self, _skill: &mut SkillSpec, _diagnostic: &Diagnostic) -> Result<()> {
        Err(crate::error::MsError::NotImplemented(format!(
            "auto-fix not implemented for rule '{}'",
            self.id()
        )))
    }
}

/// A boxed validation rule for dynamic dispatch
pub type BoxedRule = Box<dyn ValidationRule>;

/// Helper macro to simplify rule implementation
#[macro_export]
macro_rules! impl_rule {
    (
        $struct_name:ident,
        id: $id:expr,
        name: $name:expr,
        description: $desc:expr,
        category: $cat:expr,
        severity: $sev:expr,
        validate: |$ctx:ident| $validate_body:expr
    ) => {
        pub struct $struct_name;

        impl $crate::lint::rule::ValidationRule for $struct_name {
            fn id(&self) -> &str {
                $id
            }

            fn name(&self) -> &str {
                $name
            }

            fn description(&self) -> &str {
                $desc
            }

            fn category(&self) -> $crate::lint::diagnostic::RuleCategory {
                $cat
            }

            fn default_severity(&self) -> $crate::lint::diagnostic::Severity {
                $sev
            }

            fn validate(&self, $ctx: &$crate::lint::config::ValidationContext<'_>) -> Vec<$crate::lint::diagnostic::Diagnostic> {
                $validate_body
            }
        }
    };
}

// Re-export the macro
pub use impl_rule;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::config::ValidationConfig;

    // Simple test rule for testing
    struct TestRule;

    impl ValidationRule for TestRule {
        fn id(&self) -> &str {
            "test-rule"
        }

        fn name(&self) -> &str {
            "Test Rule"
        }

        fn description(&self) -> &str {
            "A test rule for unit testing"
        }

        fn category(&self) -> RuleCategory {
            RuleCategory::Structure
        }

        fn default_severity(&self) -> Severity {
            Severity::Warning
        }

        fn validate(&self, ctx: &ValidationContext<'_>) -> Vec<Diagnostic> {
            if ctx.skill.metadata.id.is_empty() {
                vec![Diagnostic::warning(self.id(), "Skill ID is empty")]
            } else {
                vec![]
            }
        }
    }

    #[test]
    fn test_rule_implementation() {
        let rule = TestRule;
        assert_eq!(rule.id(), "test-rule");
        assert_eq!(rule.name(), "Test Rule");
        assert_eq!(rule.category(), RuleCategory::Structure);
        assert_eq!(rule.default_severity(), Severity::Warning);
        assert!(!rule.can_fix());
    }

    #[test]
    fn test_rule_validate() {
        let rule = TestRule;
        let config = ValidationConfig::new();

        // Skill with empty ID
        let skill = SkillSpec::default();
        let ctx = ValidationContext::new(&skill, &config);
        let diagnostics = rule.validate(&ctx);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].rule_id, "test-rule");

        // Skill with valid ID
        let skill = SkillSpec::new("valid-id", "Valid Skill");
        let ctx = ValidationContext::new(&skill, &config);
        let diagnostics = rule.validate(&ctx);
        assert!(diagnostics.is_empty());
    }
}
