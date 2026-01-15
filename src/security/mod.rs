//! Security features for ms (prompt injection, command safety, audits).

pub mod acip;
pub mod command_safety;
pub mod path_policy;
pub mod secret_scanner;

pub use acip::{
    AcipAnalysis, AcipClassification, AcipConfig, AcipEngine, ContentSource, QuarantineRecord,
    TrustBoundaryConfig, TrustLevel, contains_injection_patterns, contains_sensitive_data,
};
pub use command_safety::{CommandSafetyEvent, SafetyGate, SafetyStatus};
pub use path_policy::{
    PathPolicyViolation, canonicalize_with_root, deny_symlink_escape, is_under_root,
    normalize_path, safe_join, validate_path_component,
};
pub use secret_scanner::{
    SecretMatch, SecretScanSummary, SecretType, contains_secrets, redact_secrets,
    redact_secrets_typed, scan_secrets, scan_secrets_summary,
};
