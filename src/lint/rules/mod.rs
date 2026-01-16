//! Built-in validation rules for skill linting.
//!
//! This module contains all the built-in validation rules organized by category:
//!
//! - **Structural rules** (`structural`): Check skill structure integrity
//! - **Reference rules** (`reference`): Validate references and inheritance
//! - **Security rules** (`security`): Detect secrets, injection, and unsafe patterns
//! - **Quality rules** (`quality`): Check content quality (descriptions, rules, examples)
//! - **Performance rules** (`quality`): Token budget and embedding quality hints
//!
//! # Usage
//!
//! ```
//! use ms::lint::rules::all_rules;
//! use ms::lint::ValidationEngine;
//!
//! let mut engine = ValidationEngine::with_defaults();
//! for rule in all_rules() {
//!     engine.register(rule);
//! }
//! ```

pub mod quality;
pub mod reference;
pub mod security;
pub mod structural;

use crate::lint::rule::BoxedRule;

// Re-export individual rules for direct use
pub use quality::{
    ActionableRulesRule, BalancedContentRule, EmbeddingQualityRule, ExamplesHaveCodeRule,
    MeaningfulDescriptionRule, TokenBudgetRule,
};
pub use reference::{DeepInheritanceRule, FormatVersionRule, NoCycleRule, ValidExtendsRule};
pub use security::{InputSanitizationRule, NoPromptInjectionRule, NoSecretsRule, SafePathsRule};
pub use structural::{
    NonEmptyBlocksRule, RequiredMetadataRule, UniqueBlockIdsRule, UniqueSectionIdsRule,
    ValidVersionRule,
};

/// Returns all structural validation rules.
#[must_use] 
pub fn structural_rules() -> Vec<BoxedRule> {
    structural::structural_rules()
}

/// Returns all reference validation rules.
#[must_use] 
pub fn reference_rules() -> Vec<BoxedRule> {
    reference::reference_rules()
}

/// Returns all security validation rules.
#[must_use] 
pub fn security_rules() -> Vec<BoxedRule> {
    security::security_rules()
}

/// Returns all quality validation rules.
#[must_use] 
pub fn quality_rules() -> Vec<BoxedRule> {
    quality::quality_rules()
}

/// Returns all performance validation rules.
#[must_use] 
pub fn performance_rules() -> Vec<BoxedRule> {
    quality::performance_rules()
}

/// Returns all built-in validation rules.
///
/// This is a convenience function that combines all rule categories.
#[must_use] 
pub fn all_rules() -> Vec<BoxedRule> {
    let mut rules = structural_rules();
    rules.extend(reference_rules());
    rules.extend(security_rules());
    rules.extend(quality_rules());
    rules.extend(performance_rules());
    rules
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_rules_not_empty() {
        let rules = all_rules();
        assert!(!rules.is_empty());
        // Should have: 5 structural + 4 reference + 4 security + 4 quality + 2 performance = 19
        assert!(rules.len() >= 19);
    }

    #[test]
    fn test_structural_rules_count() {
        let rules = structural_rules();
        assert_eq!(rules.len(), 5);
    }

    #[test]
    fn test_reference_rules_count() {
        let rules = reference_rules();
        assert_eq!(rules.len(), 4);
    }

    #[test]
    fn test_security_rules_count() {
        let rules = security_rules();
        assert_eq!(rules.len(), 4);
    }

    #[test]
    fn test_quality_rules_count() {
        let rules = quality_rules();
        assert_eq!(rules.len(), 4);
    }

    #[test]
    fn test_performance_rules_count() {
        let rules = performance_rules();
        assert_eq!(rules.len(), 2);
    }

    #[test]
    fn test_rule_ids_unique() {
        let rules = all_rules();
        let mut ids: Vec<&str> = rules.iter().map(|r| r.id()).collect();
        let original_len = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), original_len, "All rule IDs must be unique");
    }
}
