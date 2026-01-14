//! Security features for ms (prompt injection, command safety, audits).

pub mod acip;
pub mod command_safety;
pub mod path_policy;
pub mod secret_scanner;

pub use acip::{
    contains_injection_patterns, contains_sensitive_data, AcipAnalysis, AcipClassification,
    AcipConfig, AcipEngine, ContentSource, QuarantineRecord, TrustBoundaryConfig, TrustLevel,
};
pub use command_safety::{CommandSafetyEvent, SafetyGate, SafetyStatus};
pub use path_policy::{
    canonicalize_with_root, deny_symlink_escape, is_under_root, normalize_path, safe_join,
    validate_path_component, PathPolicyViolation,
};
pub use secret_scanner::{
    contains_secrets, redact_secrets, redact_secrets_typed, scan_secrets, scan_secrets_summary,
    SecretMatch, SecretScanSummary, SecretType,
};
