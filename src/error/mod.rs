//! Error handling for meta_skill.
//!
//! This module provides:
//! - [`MsError`]: The main error enum for all ms operations
//! - [`ErrorCode`]: Standardized error codes for machine parsing
//! - [`StructuredError`]: Rich error type with suggestions and context
//! - Suggestion helpers for context-aware error recovery hints

mod codes;
mod suggestions;

use std::io;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

pub use codes::ErrorCode;
pub use suggestions::{suggest_for_error, suggest_similar_skills};

/// Main error type for meta_skill operations.
#[derive(Error, Debug)]
pub enum MsError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Git error: {0}")]
    Git(#[from] git2::Error),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Skill not found: {0}")]
    SkillNotFound(String),

    #[error("Invalid skill format: {0}")]
    InvalidSkill(String),

    #[error("Skill validation failed: {0}")]
    ValidationFailed(String),

    #[error("Search index error: {0}")]
    SearchIndex(#[from] tantivy::TantivyError),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("YAML serialization error: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("Query parse error: {0}")]
    QueryParse(String),

    #[error("CASS not available: {0}")]
    CassUnavailable(String),

    #[error("CM not available: {0}")]
    CmUnavailable(String),

    #[error("Beads not available: {0}")]
    BeadsUnavailable(String),

    #[error("Mining failed: {0}")]
    MiningFailed(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Missing required config: {0}")]
    MissingConfig(String),

    #[error("Transaction failed: {0}")]
    TransactionFailed(String),

    #[error("Two-phase commit failed at {phase}: {reason}")]
    TwoPhaseCommitFailed { phase: String, reason: String },

    #[error("Operation requires approval: {0}")]
    ApprovalRequired(String),

    #[error("Destructive operation blocked: {0}")]
    DestructiveBlocked(String),

    #[error("ACIP error: {0}")]
    AcipError(String),

    #[error("Lock timeout: {0}")]
    LockTimeout(String),

    #[error("Lock failed: {0}")]
    LockFailed(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Not implemented: {0}")]
    NotImplemented(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Assertion failed: {0}")]
    AssertionFailed(String),

    #[error("Cyclic inheritance detected for skill '{skill_id}': {}", .cycle.join(" -> "))]
    CyclicInheritance {
        skill_id: String,
        cycle: Vec<String>,
    },

    #[error("Parent skill not found: {parent_id} (required by {child_id})")]
    ParentSkillNotFound { parent_id: String, child_id: String },

    #[error("Import error: {0}")]
    Import(String),
}

impl MsError {
    /// Get the error code for this error.
    #[must_use]
    pub fn code(&self) -> ErrorCode {
        match self {
            Self::Database(_) => ErrorCode::DatabaseError,
            Self::Git(_) => ErrorCode::GitError,
            Self::Io(_) => ErrorCode::IoError,
            Self::SkillNotFound(_) => ErrorCode::SkillNotFound,
            Self::InvalidSkill(_) => ErrorCode::SkillInvalid,
            Self::ValidationFailed(_) => ErrorCode::ValidationFailed,
            Self::SearchIndex(_) => ErrorCode::IndexCorrupted,
            Self::Json(_) | Self::Yaml(_) | Self::Serialization(_) => ErrorCode::SerializationError,
            Self::QueryParse(_) => ErrorCode::SearchQueryInvalid,
            Self::CassUnavailable(_) => ErrorCode::CassUnavailable,
            Self::CmUnavailable(_) => ErrorCode::CmUnavailable,
            Self::BeadsUnavailable(_) => ErrorCode::BeadsUnavailable,
            Self::MiningFailed(_) => ErrorCode::MiningFailed,
            Self::Config(_) => ErrorCode::ConfigInvalid,
            Self::MissingConfig(_) => ErrorCode::ConfigMissingRequired,
            Self::TransactionFailed(_) => ErrorCode::TransactionFailed,
            Self::TwoPhaseCommitFailed { .. } => ErrorCode::TwoPhaseCommitFailed,
            Self::ApprovalRequired(_) => ErrorCode::ApprovalRequired,
            Self::DestructiveBlocked(_) => ErrorCode::DestructiveBlocked,
            Self::AcipError(_) => ErrorCode::AcipBlocked,
            Self::LockTimeout(_) => ErrorCode::LockTimeout,
            Self::LockFailed(_) => ErrorCode::LockFailed,
            Self::NotImplemented(_) => ErrorCode::NotImplemented,
            Self::NotFound(_) => ErrorCode::NotFound,
            Self::Timeout(_) => ErrorCode::Timeout,
            Self::AssertionFailed(_) => ErrorCode::AssertionFailed,
            Self::CyclicInheritance { .. } => ErrorCode::SkillCyclicDependency,
            Self::ParentSkillNotFound { .. } => ErrorCode::SkillParentNotFound,
            Self::Import(_) => ErrorCode::ImportFailed,
        }
    }

    /// Get context information for this error as JSON.
    #[must_use]
    pub fn context(&self) -> Option<Value> {
        match self {
            Self::SkillNotFound(id) => Some(serde_json::json!({ "skill_id": id })),
            Self::InvalidSkill(reason) => Some(serde_json::json!({ "reason": reason })),
            Self::CyclicInheritance { skill_id, cycle } => {
                Some(serde_json::json!({ "skill_id": skill_id, "cycle": cycle }))
            }
            Self::ParentSkillNotFound {
                parent_id,
                child_id,
            } => Some(serde_json::json!({ "parent_id": parent_id, "child_id": child_id })),
            Self::MissingConfig(key) => Some(serde_json::json!({ "config_key": key })),
            Self::TwoPhaseCommitFailed { phase, reason } => {
                Some(serde_json::json!({ "phase": phase, "reason": reason }))
            }
            _ => None,
        }
    }

    /// Convert this error to a structured error.
    #[must_use]
    pub fn to_structured(&self) -> StructuredError {
        StructuredError::from_ms_error(self)
    }
}

/// A structured error with machine-readable code, suggestion, and context.
///
/// This type is designed for robot mode output where AI agents need
/// to parse errors and take appropriate action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredError {
    /// The error code (e.g., "SKILL_NOT_FOUND")
    pub code: ErrorCode,

    /// The numeric error code (e.g., 101)
    pub numeric_code: u16,

    /// Human-readable error message
    pub message: String,

    /// Actionable suggestion for recovery
    pub suggestion: String,

    /// Additional context for debugging
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<Value>,

    /// Whether this error is potentially recoverable by the user
    pub recoverable: bool,

    /// URL to documentation about this error
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help_url: Option<String>,

    /// Error category (e.g., "skill", "config", "network")
    pub category: String,
}

impl StructuredError {
    /// Create a new structured error.
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            numeric_code: code.numeric(),
            suggestion: code.suggestion().to_string(),
            context: None,
            recoverable: code.is_recoverable(),
            help_url: code.help_url(),
            category: code.category().to_string(),
            code,
            message,
        }
    }

    /// Create a structured error from an MsError.
    #[must_use]
    pub fn from_ms_error(err: &MsError) -> Self {
        let code = err.code();
        let context = err.context();
        let message = err.to_string();

        // Get context-aware suggestion
        let suggestion = suggest_for_error(code, context.as_ref());

        Self {
            code,
            numeric_code: code.numeric(),
            message,
            suggestion,
            context,
            recoverable: code.is_recoverable(),
            help_url: code.help_url(),
            category: code.category().to_string(),
        }
    }

    /// Add context to this error.
    #[must_use]
    pub fn with_context(mut self, context: Value) -> Self {
        self.context = Some(context);
        // Regenerate suggestion with new context
        self.suggestion = suggest_for_error(self.code, self.context.as_ref());
        self
    }

    /// Set a custom suggestion.
    #[must_use]
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = suggestion.into();
        self
    }

    /// Set a custom help URL.
    #[must_use]
    pub fn with_help_url(mut self, url: impl Into<String>) -> Self {
        self.help_url = Some(url.into());
        self
    }
}

impl std::fmt::Display for StructuredError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl From<MsError> for StructuredError {
    fn from(err: MsError) -> Self {
        Self::from_ms_error(&err)
    }
}

impl From<&MsError> for StructuredError {
    fn from(err: &MsError) -> Self {
        Self::from_ms_error(err)
    }
}

/// Result type alias using MsError.
pub type Result<T> = std::result::Result<T, MsError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ms_error_code_mapping() {
        assert_eq!(
            MsError::SkillNotFound("test".into()).code(),
            ErrorCode::SkillNotFound
        );
        assert_eq!(
            MsError::Config("bad".into()).code(),
            ErrorCode::ConfigInvalid
        );
        assert_eq!(
            MsError::CyclicInheritance {
                skill_id: "a".into(),
                cycle: vec!["a".into(), "b".into(), "a".into()]
            }
            .code(),
            ErrorCode::SkillCyclicDependency
        );
    }

    #[test]
    fn test_ms_error_context() {
        let err = MsError::SkillNotFound("my-skill".into());
        let ctx = err.context().unwrap();
        assert_eq!(ctx.get("skill_id").unwrap(), "my-skill");
    }

    #[test]
    fn test_structured_error_from_ms_error() {
        let err = MsError::SkillNotFound("test-skill".into());
        let structured = StructuredError::from_ms_error(&err);

        assert_eq!(structured.code, ErrorCode::SkillNotFound);
        assert_eq!(structured.numeric_code, 101);
        assert!(structured.message.contains("test-skill"));
        assert!(!structured.suggestion.is_empty());
        assert!(structured.recoverable);
        assert_eq!(structured.category, "skill");
    }

    #[test]
    fn test_structured_error_serialization() {
        let err = StructuredError::new(ErrorCode::SkillNotFound, "Skill 'test' not found");
        let json = serde_json::to_string(&err).unwrap();

        assert!(json.contains("SKILL_NOT_FOUND"));
        assert!(json.contains("\"numeric_code\":101"));
        assert!(json.contains("\"recoverable\":true"));
        assert!(json.contains("\"category\":\"skill\""));
    }

    #[test]
    fn test_structured_error_with_context() {
        let err = StructuredError::new(ErrorCode::SkillNotFound, "Not found")
            .with_context(serde_json::json!({ "skill_id": "my-skill" }));

        assert!(err.context.is_some());
        // Suggestion should be regenerated with context
        assert!(err.suggestion.contains("my-skill"));
    }

    #[test]
    fn test_structured_error_display() {
        let err = StructuredError::new(ErrorCode::SkillNotFound, "Skill 'test' not found");
        let display = format!("{}", err);
        assert!(display.contains("E101"));
        assert!(display.contains("test"));
    }

    #[test]
    fn test_ms_error_to_structured() {
        let err = MsError::ParentSkillNotFound {
            parent_id: "parent".into(),
            child_id: "child".into(),
        };
        let structured = err.to_structured();

        assert_eq!(structured.code, ErrorCode::SkillParentNotFound);
        let ctx = structured.context.unwrap();
        assert_eq!(ctx.get("parent_id").unwrap(), "parent");
        assert_eq!(ctx.get("child_id").unwrap(), "child");
    }

    #[test]
    fn test_from_trait_implementations() {
        let err = MsError::SkillNotFound("test".into());

        let structured1: StructuredError = err.into();
        assert_eq!(structured1.code, ErrorCode::SkillNotFound);

        let err2 = MsError::Config("bad config".into());
        let structured2: StructuredError = (&err2).into();
        assert_eq!(structured2.code, ErrorCode::ConfigInvalid);
    }
}
