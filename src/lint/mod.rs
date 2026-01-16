//! Skill linting and validation framework.
//!
//! This module provides a flexible validation framework for skills, including:
//!
//! - `ValidationRule` trait for implementing custom validation rules
//! - `ValidationEngine` for running rules against skills
//! - `Diagnostic` types for reporting issues with spans and suggestions
//! - `ValidationConfig` for configuring rule behavior
//!
//! # Example
//!
//! ```
//! use ms::lint::{ValidationEngine, ValidationConfig, ValidationRule, Diagnostic, RuleCategory, Severity};
//! use ms::lint::config::ValidationContext;
//! use ms::core::skill::SkillSpec;
//!
//! // Define a custom rule
//! struct MyRule;
//!
//! impl ValidationRule for MyRule {
//!     fn id(&self) -> &str { "my-rule" }
//!     fn name(&self) -> &str { "My Rule" }
//!     fn description(&self) -> &str { "Checks something" }
//!     fn category(&self) -> RuleCategory { RuleCategory::Structure }
//!     fn default_severity(&self) -> Severity { Severity::Warning }
//!     fn validate(&self, ctx: &ValidationContext<'_>) -> Vec<Diagnostic> {
//!         vec![]
//!     }
//! }
//!
//! // Create an engine and register rules
//! let mut engine = ValidationEngine::with_defaults();
//! engine.register(Box::new(MyRule));
//!
//! // Validate a skill
//! let skill = SkillSpec::new("test", "Test Skill");
//! let result = engine.validate(&skill);
//!
//! if !result.passed {
//!     for error in result.errors() {
//!         eprintln!("{}", error);
//!     }
//! }
//! ```

pub mod config;
pub mod diagnostic;
pub mod engine;
pub mod rule;
pub mod rules;

// Re-export main types for convenience
pub use config::{ValidationConfig, ValidationContext};
pub use diagnostic::{Diagnostic, RuleCategory, Severity, SourceSpan};
pub use engine::{FixResult, RuleInfo, ValidationEngine, ValidationResult};
pub use rule::{BoxedRule, ValidationRule};

// Re-export rule collection functions
pub use rules::{all_rules, reference_rules, structural_rules};
