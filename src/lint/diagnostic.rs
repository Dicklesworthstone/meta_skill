//! Diagnostic types for skill validation.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Category of validation rule
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleCategory {
    /// Structural validity (YAML, required fields)
    Structure,
    /// Reference integrity (links, extends, includes)
    Reference,
    /// Content quality (meaningful descriptions, etc.)
    Quality,
    /// Security concerns (secrets, injection)
    Security,
    /// Performance hints (token budget, etc.)
    Performance,
}

impl fmt::Display for RuleCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuleCategory::Structure => write!(f, "structure"),
            RuleCategory::Reference => write!(f, "reference"),
            RuleCategory::Quality => write!(f, "quality"),
            RuleCategory::Security => write!(f, "security"),
            RuleCategory::Performance => write!(f, "performance"),
        }
    }
}

/// Severity level for diagnostics
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informational suggestion
    Info,
    /// Should fix, but not blocking
    Warning,
    /// Must fix, blocks indexing
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Info => write!(f, "info"),
            Severity::Warning => write!(f, "warning"),
            Severity::Error => write!(f, "error"),
        }
    }
}

/// A location span in source text
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceSpan {
    /// Starting line (1-indexed)
    pub start_line: usize,
    /// Starting column (1-indexed)
    pub start_col: usize,
    /// Ending line (1-indexed)
    pub end_line: usize,
    /// Ending column (1-indexed)
    pub end_col: usize,
}

impl SourceSpan {
    /// Create a new source span
    pub fn new(start_line: usize, start_col: usize, end_line: usize, end_col: usize) -> Self {
        Self {
            start_line,
            start_col,
            end_line,
            end_col,
        }
    }

    /// Create a span for a single line
    pub fn line(line: usize) -> Self {
        Self {
            start_line: line,
            start_col: 1,
            end_line: line,
            end_col: 1,
        }
    }
}

impl fmt::Display for SourceSpan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.start_line == self.end_line {
            write!(f, "{}:{}-{}", self.start_line, self.start_col, self.end_col)
        } else {
            write!(
                f,
                "{}:{}-{}:{}",
                self.start_line, self.start_col, self.end_line, self.end_col
            )
        }
    }
}

/// A diagnostic message from validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    /// The rule ID that generated this diagnostic
    pub rule_id: String,
    /// Severity level
    pub severity: Severity,
    /// The diagnostic message
    pub message: String,
    /// Optional location in source
    pub span: Option<SourceSpan>,
    /// Optional suggestion for fixing
    pub suggestion: Option<String>,
    /// Whether an auto-fix is available
    pub fix_available: bool,
    /// Category of the rule
    pub category: RuleCategory,
}

impl Diagnostic {
    /// Create a new diagnostic
    pub fn new(rule_id: impl Into<String>, severity: Severity, message: impl Into<String>) -> Self {
        Self {
            rule_id: rule_id.into(),
            severity,
            message: message.into(),
            span: None,
            suggestion: None,
            fix_available: false,
            category: RuleCategory::Structure,
        }
    }

    /// Create an error diagnostic
    pub fn error(rule_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(rule_id, Severity::Error, message)
    }

    /// Create a warning diagnostic
    pub fn warning(rule_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(rule_id, Severity::Warning, message)
    }

    /// Create an info diagnostic
    pub fn info(rule_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(rule_id, Severity::Info, message)
    }

    /// Set the span
    pub fn with_span(mut self, span: SourceSpan) -> Self {
        self.span = Some(span);
        self
    }

    /// Set a suggestion
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    /// Mark that a fix is available
    pub fn with_fix(mut self) -> Self {
        self.fix_available = true;
        self
    }

    /// Set the category
    pub fn with_category(mut self, category: RuleCategory) -> Self {
        self.category = category;
        self
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}: {}", self.severity, self.rule_id, self.message)?;
        if let Some(span) = &self.span {
            write!(f, " at {}", span)?;
        }
        if let Some(suggestion) = &self.suggestion {
            write!(f, " (hint: {})", suggestion)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Info < Severity::Warning);
        assert!(Severity::Warning < Severity::Error);
    }

    #[test]
    fn test_diagnostic_builder() {
        let diag = Diagnostic::error("test-rule", "Something is wrong")
            .with_span(SourceSpan::new(1, 5, 1, 10))
            .with_suggestion("Fix it like this")
            .with_fix()
            .with_category(RuleCategory::Security);

        assert_eq!(diag.rule_id, "test-rule");
        assert_eq!(diag.severity, Severity::Error);
        assert!(diag.span.is_some());
        assert!(diag.suggestion.is_some());
        assert!(diag.fix_available);
        assert_eq!(diag.category, RuleCategory::Security);
    }

    #[test]
    fn test_source_span_display() {
        let single_line = SourceSpan::new(5, 1, 5, 20);
        assert_eq!(format!("{}", single_line), "5:1-20");

        let multi_line = SourceSpan::new(5, 1, 10, 15);
        assert_eq!(format!("{}", multi_line), "5:1-10:15");
    }

    #[test]
    fn test_category_display() {
        assert_eq!(format!("{}", RuleCategory::Structure), "structure");
        assert_eq!(format!("{}", RuleCategory::Security), "security");
    }
}
