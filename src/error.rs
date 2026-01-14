use std::io;

use thiserror::Error;

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

    #[error("Query parse error: {0}")]
    QueryParse(String),

    #[error("CASS not available: {0}")]
    CassUnavailable(String),

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
}

pub type Result<T> = std::result::Result<T, MsError>;
