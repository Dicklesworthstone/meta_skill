//! Validation configuration.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::diagnostic::Severity;
use crate::core::resolution::SkillRepository;
use crate::core::skill::SkillSpec;

/// Configuration for validation
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ValidationConfig {
    /// Rules to disable by ID
    #[serde(default)]
    pub disabled_rules: HashSet<String>,

    /// Severity overrides by rule ID
    #[serde(default)]
    pub severity_overrides: HashMap<String, Severity>,

    /// Treat warnings as errors
    #[serde(default)]
    pub strict: bool,

    /// Maximum errors before stopping validation
    #[serde(default)]
    pub max_errors: Option<usize>,

    /// Enable auto-fix when available
    #[serde(default)]
    pub auto_fix: bool,
}

impl ValidationConfig {
    /// Create a new default config
    #[must_use] 
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable strict mode
    #[must_use] 
    pub const fn strict(mut self) -> Self {
        self.strict = true;
        self
    }

    /// Set max errors
    #[must_use] 
    pub const fn with_max_errors(mut self, max: usize) -> Self {
        self.max_errors = Some(max);
        self
    }

    /// Disable a rule
    pub fn disable_rule(mut self, rule_id: impl Into<String>) -> Self {
        self.disabled_rules.insert(rule_id.into());
        self
    }

    /// Override severity for a rule
    pub fn override_severity(mut self, rule_id: impl Into<String>, severity: Severity) -> Self {
        self.severity_overrides.insert(rule_id.into(), severity);
        self
    }

    /// Check if a rule is disabled
    #[must_use] 
    pub fn is_rule_disabled(&self, rule_id: &str) -> bool {
        self.disabled_rules.contains(rule_id)
    }

    /// Get effective severity for a rule
    #[must_use] 
    pub fn effective_severity(&self, rule_id: &str, default: Severity) -> Severity {
        let severity = self
            .severity_overrides
            .get(rule_id)
            .copied()
            .unwrap_or(default);

        if self.strict && severity == Severity::Warning {
            Severity::Error
        } else {
            severity
        }
    }
}

/// Context provided to validation rules during validation
pub struct ValidationContext<'a> {
    /// The skill being validated
    pub skill: &'a SkillSpec,

    /// Access to skill repository for reference checking
    pub repository: Option<&'a dyn SkillRepository>,

    /// Configuration for validation
    pub config: &'a ValidationConfig,

    /// Original source text for span calculation
    pub source: Option<&'a str>,

    /// Path to skill file
    pub file_path: Option<&'a Path>,
}

impl<'a> ValidationContext<'a> {
    /// Create a minimal context for validation
    #[must_use] 
    pub fn new(skill: &'a SkillSpec, config: &'a ValidationConfig) -> Self {
        Self {
            skill,
            repository: None,
            config,
            source: None,
            file_path: None,
        }
    }

    /// Set the repository
    pub fn with_repository(mut self, repository: &'a dyn SkillRepository) -> Self {
        self.repository = Some(repository);
        self
    }

    /// Set the source text
    #[must_use] 
    pub const fn with_source(mut self, source: &'a str) -> Self {
        self.source = Some(source);
        self
    }

    /// Set the file path
    #[must_use] 
    pub const fn with_file_path(mut self, path: &'a Path) -> Self {
        self.file_path = Some(path);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = ValidationConfig::new();
        assert!(!config.strict);
        assert!(config.disabled_rules.is_empty());
        assert!(config.severity_overrides.is_empty());
        assert!(config.max_errors.is_none());
    }

    #[test]
    fn test_config_builder() {
        let config = ValidationConfig::new()
            .strict()
            .with_max_errors(10)
            .disable_rule("no-empty-description")
            .override_severity("missing-id", Severity::Warning);

        assert!(config.strict);
        assert_eq!(config.max_errors, Some(10));
        assert!(config.is_rule_disabled("no-empty-description"));
        assert!(!config.is_rule_disabled("other-rule"));
    }

    #[test]
    fn test_effective_severity() {
        let config = ValidationConfig::new()
            .override_severity("custom-rule", Severity::Info);

        assert_eq!(
            config.effective_severity("custom-rule", Severity::Error),
            Severity::Info
        );
        assert_eq!(
            config.effective_severity("other-rule", Severity::Warning),
            Severity::Warning
        );
    }

    #[test]
    fn test_strict_mode_elevates_warnings() {
        let config = ValidationConfig::new().strict();

        assert_eq!(
            config.effective_severity("any-rule", Severity::Warning),
            Severity::Error
        );
        assert_eq!(
            config.effective_severity("any-rule", Severity::Info),
            Severity::Info
        );
    }
}
