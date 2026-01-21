//! Standardized error codes for machine-parseable output.
//!
//! Error codes follow a numeric taxonomy:
//! - 1xx: Skill errors
//! - 2xx: Index errors
//! - 3xx: Config errors
//! - 4xx: Search errors
//! - 5xx: Network errors
//! - 6xx: Storage errors
//! - 7xx: Git errors
//! - 8xx: Validation errors
//! - 9xx: Internal errors

use serde::{Deserialize, Serialize};

/// Standardized error codes for robot mode output.
///
/// Each variant maps to a numeric code (e.g., `SkillNotFound` -> E101).
/// Codes are grouped by category for easy identification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    // ========================================
    // Skill errors (1xx)
    // ========================================
    /// E101: Requested skill was not found in the index
    SkillNotFound,
    /// E102: Skill file exists but has invalid format
    SkillInvalid,
    /// E103: Failed to parse skill markdown/spec
    SkillParseError,
    /// E104: Skill depends on another skill that doesn't exist
    SkillDependencyMissing,
    /// E105: Circular dependency chain detected
    SkillCyclicDependency,
    /// E106: Parent skill not found during inheritance resolution
    SkillParentNotFound,

    // ========================================
    // Index errors (2xx)
    // ========================================
    /// E201: No skills have been indexed yet
    IndexEmpty,
    /// E202: Index files are corrupted or unreadable
    IndexCorrupted,
    /// E203: Another process is currently indexing
    IndexBusy,
    /// E204: Index version doesn't match current ms version
    IndexVersionMismatch,

    // ========================================
    // Config errors (3xx)
    // ========================================
    /// E301: Config file not found
    ConfigNotFound,
    /// E302: Config file has invalid syntax or values
    ConfigInvalid,
    /// E303: Cannot read/write config file due to permissions
    ConfigPermissionDenied,
    /// E304: Required config value is missing
    ConfigMissingRequired,

    // ========================================
    // Search errors (4xx)
    // ========================================
    /// E401: Search query has invalid syntax
    SearchQueryInvalid,
    /// E402: Search operation timed out
    SearchTimeout,
    /// E403: Search returned zero results
    SearchNoResults,

    // ========================================
    // Network errors (5xx)
    // ========================================
    /// E501: Cannot reach remote server
    NetworkUnreachable,
    /// E502: Network request timed out
    NetworkTimeout,
    /// E503: Authentication with remote failed
    NetworkAuthFailed,

    // ========================================
    // Storage errors (6xx)
    // ========================================
    /// E601: Failed to read from storage
    StorageReadError,
    /// E602: Failed to write to storage
    StorageWriteError,
    /// E603: Storage device is full
    StorageFull,
    /// E604: Database operation failed
    DatabaseError,
    /// E605: Serialization/deserialization failed
    SerializationError,

    // ========================================
    // Git errors (7xx)
    // ========================================
    /// E701: Directory is not a git repository
    GitNotRepository,
    /// E702: Git merge conflict detected
    GitConflict,
    /// E703: Git remote operation failed
    GitRemoteError,
    /// E704: General git error
    GitError,

    // ========================================
    // Validation errors (8xx)
    // ========================================
    /// E801: Validation rules failed
    ValidationFailed,
    /// E802: Operation requires explicit approval
    ApprovalRequired,
    /// E803: Security policy violation detected
    SecurityViolation,
    /// E804: ACIP (Agent Content Injection Prevention) blocked content
    AcipBlocked,
    /// E805: Destructive operation was blocked
    DestructiveBlocked,

    // ========================================
    // Lock/Transaction errors (85x)
    // ========================================
    /// E851: Failed to acquire lock within timeout
    LockTimeout,
    /// E852: Failed to acquire lock
    LockFailed,
    /// E853: Transaction operation failed
    TransactionFailed,
    /// E854: Two-phase commit failed
    TwoPhaseCommitFailed,

    // ========================================
    // Integration errors (88x)
    // ========================================
    /// E881: CASS (session search) is not available
    CassUnavailable,
    /// E882: CM (CASS Memory) is not available
    CmUnavailable,
    /// E883: Beads issue tracker is not available
    BeadsUnavailable,
    /// E884: Mining operation failed
    MiningFailed,
    /// E885: Import operation failed
    ImportFailed,

    // ========================================
    // Internal errors (9xx)
    // ========================================
    /// E901: Unexpected internal error
    InternalError,
    /// E902: Feature not yet implemented
    NotImplemented,
    /// E903: Operation timed out
    Timeout,
    /// E904: Internal assertion failed
    AssertionFailed,
    /// E905: Generic not found (catch-all)
    NotFound,
    /// E906: IO operation failed
    IoError,
}

impl ErrorCode {
    /// Get the numeric error code (e.g., `SkillNotFound` -> 101).
    #[must_use]
    pub const fn numeric(&self) -> u16 {
        match self {
            // Skill errors (1xx)
            Self::SkillNotFound => 101,
            Self::SkillInvalid => 102,
            Self::SkillParseError => 103,
            Self::SkillDependencyMissing => 104,
            Self::SkillCyclicDependency => 105,
            Self::SkillParentNotFound => 106,

            // Index errors (2xx)
            Self::IndexEmpty => 201,
            Self::IndexCorrupted => 202,
            Self::IndexBusy => 203,
            Self::IndexVersionMismatch => 204,

            // Config errors (3xx)
            Self::ConfigNotFound => 301,
            Self::ConfigInvalid => 302,
            Self::ConfigPermissionDenied => 303,
            Self::ConfigMissingRequired => 304,

            // Search errors (4xx)
            Self::SearchQueryInvalid => 401,
            Self::SearchTimeout => 402,
            Self::SearchNoResults => 403,

            // Network errors (5xx)
            Self::NetworkUnreachable => 501,
            Self::NetworkTimeout => 502,
            Self::NetworkAuthFailed => 503,

            // Storage errors (6xx)
            Self::StorageReadError => 601,
            Self::StorageWriteError => 602,
            Self::StorageFull => 603,
            Self::DatabaseError => 604,
            Self::SerializationError => 605,

            // Git errors (7xx)
            Self::GitNotRepository => 701,
            Self::GitConflict => 702,
            Self::GitRemoteError => 703,
            Self::GitError => 704,

            // Validation errors (8xx)
            Self::ValidationFailed => 801,
            Self::ApprovalRequired => 802,
            Self::SecurityViolation => 803,
            Self::AcipBlocked => 804,
            Self::DestructiveBlocked => 805,

            // Lock/Transaction errors (85x)
            Self::LockTimeout => 851,
            Self::LockFailed => 852,
            Self::TransactionFailed => 853,
            Self::TwoPhaseCommitFailed => 854,

            // Integration errors (88x)
            Self::CassUnavailable => 881,
            Self::CmUnavailable => 882,
            Self::BeadsUnavailable => 883,
            Self::MiningFailed => 884,
            Self::ImportFailed => 885,

            // Internal errors (9xx)
            Self::InternalError => 901,
            Self::NotImplemented => 902,
            Self::Timeout => 903,
            Self::AssertionFailed => 904,
            Self::NotFound => 905,
            Self::IoError => 906,
        }
    }

    /// Get the error code as a formatted string (e.g., "E101").
    #[must_use]
    pub fn code_string(&self) -> String {
        format!("E{}", self.numeric())
    }

    /// Get the default suggestion for this error code.
    #[must_use]
    pub const fn suggestion(&self) -> &'static str {
        match self {
            // Skill errors
            Self::SkillNotFound => {
                "Run `ms search <query>` to find similar skills, or `ms list` to see all available skills"
            }
            Self::SkillInvalid => {
                "Check the skill file for syntax errors. Run `ms validate <skill>` for detailed diagnostics"
            }
            Self::SkillParseError => {
                "Ensure the skill file follows SKILL.md format. Run `ms template show` for examples"
            }
            Self::SkillDependencyMissing => {
                "Install the missing dependency with `ms bundle install` or create it manually"
            }
            Self::SkillCyclicDependency => {
                "Review the dependency chain and break the cycle by removing one of the `extends` or `includes` references"
            }
            Self::SkillParentNotFound => {
                "Ensure the parent skill exists before defining the child. Check `extends` field for typos"
            }

            // Index errors
            Self::IndexEmpty => "Run `ms index <path>` to index skills from a directory",
            Self::IndexCorrupted => "Run `ms doctor --fix` to rebuild the index from source files",
            Self::IndexBusy => {
                "Wait for the current indexing operation to complete, or check for stale lock files"
            }
            Self::IndexVersionMismatch => {
                "Run `ms migrate` to update the index to the current version"
            }

            // Config errors
            Self::ConfigNotFound => {
                "Run `ms init` to create a new configuration, or specify --config <path>"
            }
            Self::ConfigInvalid => {
                "Run `ms config` to see current values. Check TOML syntax in config file"
            }
            Self::ConfigPermissionDenied => {
                "Check file permissions on the config file. You may need to run with different permissions"
            }
            Self::ConfigMissingRequired => {
                "Set the required config value with `ms config <key> <value>`"
            }

            // Search errors
            Self::SearchQueryInvalid => {
                "Check query syntax. Use quotes for phrases, AND/OR for boolean, * for wildcards"
            }
            Self::SearchTimeout => "Try a simpler query or increase timeout with --timeout flag",
            Self::SearchNoResults => {
                "Try broader search terms, or run `ms list` to see all available skills"
            }

            // Network errors
            Self::NetworkUnreachable => {
                "Check your network connection and ensure the remote server is accessible"
            }
            Self::NetworkTimeout => {
                "Check network connectivity. The remote server may be slow or unreachable"
            }
            Self::NetworkAuthFailed => {
                "Verify your credentials. Check SSH keys or tokens in config"
            }

            // Storage errors
            Self::StorageReadError => {
                "Check file permissions and ensure the storage path is accessible"
            }
            Self::StorageWriteError => {
                "Check disk space and write permissions on the storage directory"
            }
            Self::StorageFull => "Free up disk space or configure a different storage location",
            Self::DatabaseError => {
                "Run `ms doctor` to check database health. May need to rebuild with `ms doctor --fix`"
            }
            Self::SerializationError => {
                "The data format may be corrupted. Check input data for validity"
            }

            // Git errors
            Self::GitNotRepository => {
                "Run `git init` to initialize a repository, or change to a directory with a git repo"
            }
            Self::GitConflict => "Resolve merge conflicts manually, then run `ms sync` again",
            Self::GitRemoteError => {
                "Check remote URL and credentials. Run `git remote -v` to verify"
            }
            Self::GitError => {
                "Check git status with `git status`. There may be uncommitted changes or other issues"
            }

            // Validation errors
            Self::ValidationFailed => {
                "Review the validation errors and fix each issue. Run `ms validate <skill>` for details"
            }
            Self::ApprovalRequired => {
                "This operation requires explicit approval. Set MS_APPROVE_COMMAND environment variable"
            }
            Self::SecurityViolation => {
                "Review security policy. This content may contain disallowed patterns"
            }
            Self::AcipBlocked => {
                "Content was blocked by ACIP. Run `ms security quarantine list` to review"
            }
            Self::DestructiveBlocked => {
                "Destructive operations are blocked by DCG. Use --approve flag if intended"
            }

            // Lock/Transaction errors
            Self::LockTimeout => {
                "Another process may be holding the lock. Wait and retry, or check for stale locks"
            }
            Self::LockFailed => {
                "Failed to acquire lock. Check for other ms processes or stale lock files"
            }
            Self::TransactionFailed => {
                "The operation was rolled back. Check error details and retry"
            }
            Self::TwoPhaseCommitFailed => {
                "Two-phase commit failed. Data may be partially committed. Run `ms doctor`"
            }

            // Integration errors
            Self::CassUnavailable => {
                "CASS is not installed or not running. Install with `cargo install cass`"
            }
            Self::CmUnavailable => {
                "CM (CASS Memory) is not available. Check installation and configuration"
            }
            Self::BeadsUnavailable => {
                "Beads is not available. Install with `cargo install beads_viewer`"
            }
            Self::MiningFailed => {
                "Session mining failed. Check CASS connection and query parameters"
            }
            Self::ImportFailed => {
                "Import operation failed. Check the source format and permissions"
            }

            // Internal errors
            Self::InternalError => {
                "An unexpected error occurred. Please report this issue with full error output"
            }
            Self::NotImplemented => {
                "This feature is not yet implemented. Check documentation for alternatives"
            }
            Self::Timeout => "Operation timed out. Try again or increase timeout settings",
            Self::AssertionFailed => {
                "Internal assertion failed. This is a bug. Please report with full context"
            }
            Self::NotFound => "The requested resource was not found. Check the path or identifier",
            Self::IoError => "File operation failed. Check path exists and permissions are correct",
        }
    }

    /// Check if this error is potentially recoverable by the user.
    #[must_use]
    pub const fn is_recoverable(&self) -> bool {
        match self {
            // User can take action to fix these
            Self::SkillNotFound
            | Self::SkillInvalid
            | Self::SkillParseError
            | Self::SkillDependencyMissing
            | Self::SkillCyclicDependency
            | Self::SkillParentNotFound
            | Self::IndexEmpty
            | Self::IndexBusy
            | Self::IndexVersionMismatch
            | Self::ConfigNotFound
            | Self::ConfigInvalid
            | Self::ConfigPermissionDenied
            | Self::ConfigMissingRequired
            | Self::SearchQueryInvalid
            | Self::SearchTimeout
            | Self::SearchNoResults
            | Self::NetworkUnreachable
            | Self::NetworkTimeout
            | Self::NetworkAuthFailed
            | Self::StorageReadError
            | Self::StorageWriteError
            | Self::StorageFull
            | Self::GitNotRepository
            | Self::GitConflict
            | Self::GitRemoteError
            | Self::GitError
            | Self::ValidationFailed
            | Self::ApprovalRequired
            | Self::SecurityViolation
            | Self::AcipBlocked
            | Self::DestructiveBlocked
            | Self::LockTimeout
            | Self::LockFailed
            | Self::TransactionFailed
            | Self::CassUnavailable
            | Self::CmUnavailable
            | Self::BeadsUnavailable
            | Self::MiningFailed
            | Self::ImportFailed
            | Self::Timeout
            | Self::NotFound
            | Self::IoError => true,

            // These typically require code changes or bug fixes
            Self::IndexCorrupted
            | Self::DatabaseError
            | Self::SerializationError
            | Self::TwoPhaseCommitFailed
            | Self::InternalError
            | Self::NotImplemented
            | Self::AssertionFailed => false,
        }
    }

    /// Get the error category name.
    #[must_use]
    pub const fn category(&self) -> &'static str {
        match self.numeric() / 100 {
            1 => "skill",
            2 => "index",
            3 => "config",
            4 => "search",
            5 => "network",
            6 => "storage",
            7 => "git",
            8 => "validation",
            9 => "internal",
            _ => "unknown",
        }
    }

    /// Get a URL to documentation for this error (if available).
    #[must_use]
    pub fn help_url(&self) -> Option<String> {
        Some(format!(
            "https://docs.ms-skill.dev/errors/{}",
            self.code_string()
        ))
    }

    /// Iterate over all error codes.
    pub fn all() -> impl Iterator<Item = Self> {
        [
            Self::SkillNotFound,
            Self::SkillInvalid,
            Self::SkillParseError,
            Self::SkillDependencyMissing,
            Self::SkillCyclicDependency,
            Self::SkillParentNotFound,
            Self::IndexEmpty,
            Self::IndexCorrupted,
            Self::IndexBusy,
            Self::IndexVersionMismatch,
            Self::ConfigNotFound,
            Self::ConfigInvalid,
            Self::ConfigPermissionDenied,
            Self::ConfigMissingRequired,
            Self::SearchQueryInvalid,
            Self::SearchTimeout,
            Self::SearchNoResults,
            Self::NetworkUnreachable,
            Self::NetworkTimeout,
            Self::NetworkAuthFailed,
            Self::StorageReadError,
            Self::StorageWriteError,
            Self::StorageFull,
            Self::DatabaseError,
            Self::SerializationError,
            Self::GitNotRepository,
            Self::GitConflict,
            Self::GitRemoteError,
            Self::GitError,
            Self::ValidationFailed,
            Self::ApprovalRequired,
            Self::SecurityViolation,
            Self::AcipBlocked,
            Self::DestructiveBlocked,
            Self::LockTimeout,
            Self::LockFailed,
            Self::TransactionFailed,
            Self::TwoPhaseCommitFailed,
            Self::CassUnavailable,
            Self::CmUnavailable,
            Self::BeadsUnavailable,
            Self::MiningFailed,
            Self::ImportFailed,
            Self::InternalError,
            Self::NotImplemented,
            Self::Timeout,
            Self::AssertionFailed,
            Self::NotFound,
            Self::IoError,
        ]
        .into_iter()
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_numeric() {
        assert_eq!(ErrorCode::SkillNotFound.numeric(), 101);
        assert_eq!(ErrorCode::IndexEmpty.numeric(), 201);
        assert_eq!(ErrorCode::ConfigNotFound.numeric(), 301);
        assert_eq!(ErrorCode::SearchQueryInvalid.numeric(), 401);
        assert_eq!(ErrorCode::NetworkUnreachable.numeric(), 501);
        assert_eq!(ErrorCode::StorageReadError.numeric(), 601);
        assert_eq!(ErrorCode::GitNotRepository.numeric(), 701);
        assert_eq!(ErrorCode::ValidationFailed.numeric(), 801);
        assert_eq!(ErrorCode::InternalError.numeric(), 901);
    }

    #[test]
    fn test_error_code_string() {
        assert_eq!(ErrorCode::SkillNotFound.code_string(), "E101");
        assert_eq!(ErrorCode::IndexEmpty.code_string(), "E201");
        assert_eq!(ErrorCode::InternalError.code_string(), "E901");
    }

    #[test]
    fn test_all_codes_have_suggestions() {
        for code in ErrorCode::all() {
            let suggestion = code.suggestion();
            assert!(
                !suggestion.is_empty(),
                "ErrorCode::{:?} has empty suggestion",
                code
            );
        }
    }

    #[test]
    fn test_all_codes_have_categories() {
        for code in ErrorCode::all() {
            let category = code.category();
            assert!(
                !category.is_empty() && category != "unknown",
                "ErrorCode::{:?} has invalid category",
                code
            );
        }
    }

    #[test]
    fn test_error_code_serialization() {
        let code = ErrorCode::SkillNotFound;
        let json = serde_json::to_string(&code).unwrap();
        assert_eq!(json, "\"SKILL_NOT_FOUND\"");

        let deserialized: ErrorCode = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, code);
    }

    #[test]
    fn test_error_code_display() {
        assert_eq!(format!("{}", ErrorCode::SkillNotFound), "E101");
        assert_eq!(format!("{}", ErrorCode::InternalError), "E901");
    }

    #[test]
    fn test_recoverable_categorization() {
        // User-fixable errors should be recoverable
        assert!(ErrorCode::SkillNotFound.is_recoverable());
        assert!(ErrorCode::ConfigNotFound.is_recoverable());
        assert!(ErrorCode::SearchQueryInvalid.is_recoverable());

        // Internal errors should not be recoverable
        assert!(!ErrorCode::InternalError.is_recoverable());
        assert!(!ErrorCode::AssertionFailed.is_recoverable());
        assert!(!ErrorCode::NotImplemented.is_recoverable());
    }

    #[test]
    fn test_category_assignment() {
        assert_eq!(ErrorCode::SkillNotFound.category(), "skill");
        assert_eq!(ErrorCode::IndexEmpty.category(), "index");
        assert_eq!(ErrorCode::ConfigNotFound.category(), "config");
        assert_eq!(ErrorCode::SearchQueryInvalid.category(), "search");
        assert_eq!(ErrorCode::NetworkUnreachable.category(), "network");
        assert_eq!(ErrorCode::StorageReadError.category(), "storage");
        assert_eq!(ErrorCode::GitNotRepository.category(), "git");
        assert_eq!(ErrorCode::ValidationFailed.category(), "validation");
        assert_eq!(ErrorCode::InternalError.category(), "internal");
    }

    #[test]
    fn test_help_url_format() {
        let url = ErrorCode::SkillNotFound.help_url();
        assert!(url.is_some());
        assert!(url.unwrap().contains("E101"));
    }

    #[test]
    fn test_all_iterator_coverage() {
        let all_codes: Vec<_> = ErrorCode::all().collect();
        // Ensure we have all expected codes
        assert!(all_codes.len() >= 40, "Expected at least 40 error codes");

        // Check no duplicates
        let mut seen = std::collections::HashSet::new();
        for code in &all_codes {
            assert!(
                seen.insert(code.numeric()),
                "Duplicate numeric code: {}",
                code.numeric()
            );
        }
    }
}
