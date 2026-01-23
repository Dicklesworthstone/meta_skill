//! Plain output format specification and traits.
//!
//! This module defines the exact output formats for plain/agent mode to ensure
//! machine parseability. All plain output formats are designed to be:
//! - Tab-separated (TSV) for structured data
//! - Line-based for easy parsing
//! - Consistent across all commands
//!
//! # Output Formats
//!
//! ## Search Results
//!
//! Tab-separated, one result per line.
//! Format: `SCORE<TAB>NAME<TAB>TYPE<TAB>DESCRIPTION`
//!
//! Example:
//! - `0.95    my-skill    tool    A helpful tool for...`
//! - `0.87    another-skill    template    Template for...`
//!
//! ## List Results
//!
//! Tab-separated, one skill per line.
//! Format: `NAME<TAB>LAYER<TAB>TAGS<TAB>UPDATED`
//!
//! Example:
//! - `my-skill    user    cli,rust    2024-01-15`
//! - `another-skill    project    python,web    2024-01-10`
//!
//! ## Show Output
//!
//! YAML-like key-value format:
//! ```text
//! name: my-skill
//! type: tool
//! version: 1.0.0
//! description: A helpful tool...
//! tags: cli, rust
//! created: 2024-01-01
//! updated: 2024-01-15
//! ---
//! [content blocks follow]
//! ```
//!
//! ## Suggestion Output
//!
//! One suggestion per line with score (tab-separated):
//! - `0.92    skill-name-1`
//! - `0.85    skill-name-2`
//!
//! ## Doctor Output
//!
//! Status line per check (tab-separated: STATUS, NAME, MESSAGE):
//! - `OK    database    Database connection healthy`
//! - `WARN    index    Index may be stale (3 days old)`
//! - `FAIL    config    Missing required field: api_key`
//!
//! Followed by summary:
//! ```text
//! ---
//! SUMMARY: 1 OK, 1 WARN, 1 FAIL
//! ```
//!
//! ## Error Format (all commands)
//!
//! ```text
//! ERROR: [error_code] error message
//!   at: file:line (if applicable)
//!   hint: suggestion for fixing
//! ```
//!
//! ## Progress Format (all commands)
//!
//! ```text
//! PROGRESS: 50/100 Processing file.md
//! DONE: Processed 100 files in 2.3s
//! ```

use serde::{Deserialize, Serialize};

use crate::error::{ErrorCode, StructuredError};

/// Trait for types that can format themselves for plain text output.
///
/// This trait provides consistent plain-text formatting for all command outputs.
/// Implementations should follow these conventions:
/// - Use tabs as field separators
/// - Use newlines as record separators
/// - Avoid ANSI codes or Unicode box-drawing
/// - Include machine-parseable status prefixes
pub trait PlainFormatter {
    /// Format for plain text output (tab-separated, line-based).
    ///
    /// The output should be suitable for parsing with `cut`, `awk`, or similar tools.
    fn format_plain(&self) -> String;

    /// Format for JSON output with standard envelope.
    ///
    /// Returns a JSON value that will be wrapped in the standard response envelope.
    fn format_json(&self) -> serde_json::Value;
}

/// Standard JSON response envelope for all command outputs.
///
/// This provides a consistent structure for machine consumers:
/// ```json
/// {
///   "success": true,
///   "data": { ... command-specific data ... },
///   "meta": {
///     "duration_ms": 42,
///     "timestamp": "2024-01-15T10:30:00Z"
///   }
/// }
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct JsonEnvelope<T: Serialize> {
    /// Whether the operation succeeded
    pub success: bool,
    /// Command-specific data
    pub data: T,
    /// Metadata about the operation
    pub meta: JsonMeta,
}

/// Metadata for JSON responses.
#[derive(Debug, Clone, Default, Serialize)]
pub struct JsonMeta {
    /// Duration in milliseconds (if timed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// ISO 8601 timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// Number of results (for list operations)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
    /// Additional context
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub extra: Option<serde_json::Value>,
}

impl<T: Serialize> JsonEnvelope<T> {
    /// Create a success envelope with data.
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data,
            meta: JsonMeta::default(),
        }
    }

    /// Create a success envelope with data and metadata.
    pub fn success_with_meta(data: T, meta: JsonMeta) -> Self {
        Self {
            success: true,
            data,
            meta,
        }
    }

    /// Set the duration in milliseconds.
    #[must_use]
    pub fn with_duration(mut self, duration_ms: u64) -> Self {
        self.meta.duration_ms = Some(duration_ms);
        self
    }

    /// Set the timestamp to current time.
    #[must_use]
    pub fn with_timestamp(mut self) -> Self {
        self.meta.timestamp = Some(chrono::Utc::now().to_rfc3339());
        self
    }

    /// Set the count for list operations.
    #[must_use]
    pub fn with_count(mut self, count: usize) -> Self {
        self.meta.count = Some(count);
        self
    }
}

/// Standard JSON error response.
///
/// ```json
/// {
///   "success": false,
///   "error": {
///     "code": "NOT_FOUND",
///     "message": "Skill not found: xyz",
///     "hint": "Did you mean: abc?"
///   }
/// }
/// ```
#[derive(Debug, Clone, Serialize)]
pub struct JsonError {
    /// Always false for errors
    pub success: bool,
    /// Error details
    pub error: JsonErrorDetail,
}

/// Error detail structure.
#[derive(Debug, Clone, Serialize)]
pub struct JsonErrorDetail {
    /// Error code (e.g., "NOT_FOUND", "VALIDATION_ERROR")
    pub code: String,
    /// Human-readable error message
    pub message: String,
    /// Optional hint for fixing the error
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Optional file location
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

impl JsonError {
    /// Create a new error response.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            success: false,
            error: JsonErrorDetail {
                code: code.into(),
                message: message.into(),
                hint: None,
                location: None,
            },
        }
    }

    /// Add a hint to the error.
    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.error.hint = Some(hint.into());
        self
    }

    /// Add a location to the error.
    #[must_use]
    pub fn with_location(mut self, location: impl Into<String>) -> Self {
        self.error.location = Some(location.into());
        self
    }
}

// =============================================================================
// Unified JSON Response (Success or Error)
// =============================================================================

/// Unified JSON response that can represent either success or error.
///
/// This is the recommended response format for all new API endpoints.
/// It provides a consistent structure for agents to parse:
///
/// Success:
/// ```json
/// {
///   "success": true,
///   "data": { ... },
///   "meta": { "duration_ms": 42 }
/// }
/// ```
///
/// Error:
/// ```json
/// {
///   "success": false,
///   "error": {
///     "code": "SKILL_NOT_FOUND",
///     "message": "Skill not found: xyz",
///     "hint": "Did you mean: abc?"
///   }
/// }
/// ```
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum JsonResponse<T: Serialize> {
    /// Successful response with data
    Success(JsonEnvelope<T>),
    /// Error response
    Error(JsonError),
}

impl<T: Serialize> JsonResponse<T> {
    /// Create a success response.
    pub fn success(data: T) -> Self {
        Self::Success(JsonEnvelope::success(data))
    }

    /// Create a success response with metadata.
    pub fn success_with_meta(data: T, meta: JsonMeta) -> Self {
        Self::Success(JsonEnvelope::success_with_meta(data, meta))
    }

    /// Create an error response.
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Error(JsonError::new(code, message))
    }

    /// Create an error response with hint.
    pub fn error_with_hint(
        code: impl Into<String>,
        message: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        Self::Error(JsonError::new(code, message).with_hint(hint))
    }

    /// Check if this is a success response.
    #[must_use]
    pub const fn is_success(&self) -> bool {
        matches!(self, Self::Success(_))
    }

    /// Check if this is an error response.
    #[must_use]
    pub const fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }
}

/// Rich error detail with full structured error information.
///
/// This mirrors the StructuredError from the error module but is
/// designed for JSON serialization in API responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonStructuredError {
    /// Error code enum value (e.g., "SKILL_NOT_FOUND")
    pub code: ErrorCode,
    /// Numeric error code (e.g., 101)
    pub numeric_code: u16,
    /// Human-readable error message
    pub message: String,
    /// Actionable suggestion for recovery
    pub suggestion: String,
    /// Additional context for debugging
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,
    /// Whether this error is recoverable by the user
    pub recoverable: bool,
    /// Error category (e.g., "skill", "config")
    pub category: String,
    /// URL to documentation about this error
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help_url: Option<String>,
}

impl From<StructuredError> for JsonStructuredError {
    fn from(err: StructuredError) -> Self {
        Self {
            code: err.code,
            numeric_code: err.numeric_code,
            message: err.message,
            suggestion: err.suggestion,
            context: err.context,
            recoverable: err.recoverable,
            category: err.category,
            help_url: err.help_url,
        }
    }
}

/// Full JSON error response with structured error details.
///
/// This is the richest error format, providing all available information
/// for AI agents to understand and potentially recover from errors.
#[derive(Debug, Clone, Serialize)]
pub struct JsonStructuredErrorResponse {
    /// Always false for errors
    pub success: bool,
    /// Structured error details
    pub error: JsonStructuredError,
    /// Metadata about the operation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<JsonMeta>,
}

impl JsonStructuredErrorResponse {
    /// Create from a StructuredError.
    pub fn from_structured(err: StructuredError) -> Self {
        Self {
            success: false,
            error: err.into(),
            meta: None,
        }
    }

    /// Add metadata to the error response.
    #[must_use]
    pub fn with_meta(mut self, meta: JsonMeta) -> Self {
        self.meta = Some(meta);
        self
    }
}

/// Check status for doctor-style output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlainStatus {
    /// Check passed
    Ok,
    /// Warning - non-critical issue
    Warn,
    /// Failure - critical issue
    Fail,
    /// Informational
    Info,
}

impl PlainStatus {
    /// Get the plain-text status string.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Warn => "WARN",
            Self::Fail => "FAIL",
            Self::Info => "INFO",
        }
    }
}

impl std::fmt::Display for PlainStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A single check result for plain output.
#[derive(Debug, Clone)]
pub struct PlainCheckResult {
    /// Status of the check
    pub status: PlainStatus,
    /// Name/identifier of the check
    pub name: String,
    /// Description/message
    pub message: String,
}

impl PlainCheckResult {
    /// Create a new check result.
    pub fn new(status: PlainStatus, name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status,
            name: name.into(),
            message: message.into(),
        }
    }

    /// Create an OK result.
    pub fn ok(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(PlainStatus::Ok, name, message)
    }

    /// Create a warning result.
    pub fn warn(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(PlainStatus::Warn, name, message)
    }

    /// Create a failure result.
    pub fn fail(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(PlainStatus::Fail, name, message)
    }

    /// Format as plain tab-separated line.
    #[must_use]
    pub fn format_plain(&self) -> String {
        format!("{}\t{}\t{}", self.status, self.name, self.message)
    }
}

/// Collection of check results with summary.
#[derive(Debug, Clone, Default)]
pub struct PlainCheckResults {
    /// Individual check results
    pub results: Vec<PlainCheckResult>,
}

impl PlainCheckResults {
    /// Create a new empty results collection.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a check result.
    pub fn add(&mut self, result: PlainCheckResult) {
        self.results.push(result);
    }

    /// Add an OK result.
    pub fn ok(&mut self, name: impl Into<String>, message: impl Into<String>) {
        self.add(PlainCheckResult::ok(name, message));
    }

    /// Add a warning result.
    pub fn warn(&mut self, name: impl Into<String>, message: impl Into<String>) {
        self.add(PlainCheckResult::warn(name, message));
    }

    /// Add a failure result.
    pub fn fail(&mut self, name: impl Into<String>, message: impl Into<String>) {
        self.add(PlainCheckResult::fail(name, message));
    }

    /// Count results by status.
    fn count_by_status(&self, status: PlainStatus) -> usize {
        self.results.iter().filter(|r| r.status == status).count()
    }

    /// Get the summary line.
    #[must_use]
    pub fn summary(&self) -> String {
        let ok = self.count_by_status(PlainStatus::Ok);
        let warn = self.count_by_status(PlainStatus::Warn);
        let fail = self.count_by_status(PlainStatus::Fail);
        format!("SUMMARY: {} OK, {} WARN, {} FAIL", ok, warn, fail)
    }
}

impl PlainFormatter for PlainCheckResults {
    fn format_plain(&self) -> String {
        let mut lines: Vec<String> = self.results.iter().map(|r| r.format_plain()).collect();
        lines.push("---".to_string());
        lines.push(self.summary());
        lines.join("\n")
    }

    fn format_json(&self) -> serde_json::Value {
        let results: Vec<serde_json::Value> = self
            .results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "status": r.status.as_str().to_lowercase(),
                    "name": r.name,
                    "message": r.message,
                })
            })
            .collect();

        serde_json::json!({
            "results": results,
            "summary": {
                "ok": self.count_by_status(PlainStatus::Ok),
                "warn": self.count_by_status(PlainStatus::Warn),
                "fail": self.count_by_status(PlainStatus::Fail),
            }
        })
    }
}

/// Progress indicator for plain output.
#[derive(Debug, Clone)]
pub struct PlainProgress {
    /// Current progress count
    pub current: u64,
    /// Total count
    pub total: u64,
    /// Optional message
    pub message: Option<String>,
}

impl PlainProgress {
    /// Create a new progress indicator.
    pub fn new(current: u64, total: u64) -> Self {
        Self {
            current,
            total,
            message: None,
        }
    }

    /// Create a progress indicator with a message.
    pub fn with_message(current: u64, total: u64, message: impl Into<String>) -> Self {
        Self {
            current,
            total,
            message: Some(message.into()),
        }
    }

    /// Format as plain progress line.
    #[must_use]
    pub fn format_plain(&self) -> String {
        match &self.message {
            Some(msg) => format!("PROGRESS: {}/{} {}", self.current, self.total, msg),
            None => format!("PROGRESS: {}/{}", self.current, self.total),
        }
    }
}

/// Completion message for plain output.
#[derive(Debug, Clone)]
pub struct PlainDone {
    /// Completion message
    pub message: String,
    /// Optional duration
    pub duration: Option<std::time::Duration>,
}

impl PlainDone {
    /// Create a simple completion message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            duration: None,
        }
    }

    /// Create a completion message with duration.
    pub fn with_duration(message: impl Into<String>, duration: std::time::Duration) -> Self {
        Self {
            message: message.into(),
            duration: Some(duration),
        }
    }

    /// Format as plain done line.
    #[must_use]
    pub fn format_plain(&self) -> String {
        match self.duration {
            Some(d) => format!("DONE: {} in {:.1}s", self.message, d.as_secs_f64()),
            None => format!("DONE: {}", self.message),
        }
    }
}

/// Plain error output formatter.
#[derive(Debug, Clone)]
pub struct PlainError {
    /// Error code
    pub code: String,
    /// Error message
    pub message: String,
    /// Optional location (file:line)
    pub location: Option<String>,
    /// Optional hint
    pub hint: Option<String>,
}

impl PlainError {
    /// Create a new plain error.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            location: None,
            hint: None,
        }
    }

    /// Add a location.
    #[must_use]
    pub fn with_location(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }

    /// Add a hint.
    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// Format as plain error output.
    #[must_use]
    pub fn format_plain(&self) -> String {
        let mut lines = vec![format!("ERROR: [{}] {}", self.code, self.message)];

        if let Some(ref loc) = self.location {
            lines.push(format!("  at: {}", loc));
        }

        if let Some(ref hint) = self.hint {
            lines.push(format!("  hint: {}", hint));
        }

        lines.join("\n")
    }
}

/// Key-value pair formatter for plain output.
#[derive(Debug, Clone)]
pub struct PlainKeyValue {
    pairs: Vec<(String, String)>,
}

impl PlainKeyValue {
    /// Create a new key-value formatter.
    #[must_use]
    pub fn new() -> Self {
        Self { pairs: Vec::new() }
    }

    /// Add a key-value pair.
    pub fn add(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.pairs.push((key.into(), value.into()));
    }

    /// Add a key-value pair (builder pattern).
    #[must_use]
    pub fn with(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.add(key, value);
        self
    }

    /// Format as YAML-like key-value output.
    #[must_use]
    pub fn format_plain(&self) -> String {
        self.pairs
            .iter()
            .map(|(k, v)| format!("{}: {}", k, v))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Format as TSV (tab-separated key-value).
    #[must_use]
    pub fn format_tsv(&self) -> String {
        self.pairs
            .iter()
            .map(|(k, v)| format!("{}\t{}", k, v))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl Default for PlainKeyValue {
    fn default() -> Self {
        Self::new()
    }
}

/// Utility functions for plain formatting.
pub mod utils {
    /// Escape a string for TSV output (replace tabs and newlines).
    #[must_use]
    pub fn escape_tsv(s: &str) -> String {
        s.replace('\t', " ").replace('\n', " ").replace('\r', "")
    }

    /// Format a score as a decimal with consistent precision.
    #[must_use]
    pub fn format_score(score: f32) -> String {
        format!("{:.2}", score)
    }

    /// Format a quality score as a percentage.
    #[must_use]
    pub fn format_quality(quality: f64) -> String {
        format!("{:.0}%", quality * 100.0)
    }

    /// Format a list of tags as comma-separated.
    #[must_use]
    pub fn format_tags(tags: &[String]) -> String {
        tags.join(",")
    }

    /// Format an optional value, using "-" for None.
    #[must_use]
    pub fn format_optional(opt: Option<&str>) -> &str {
        opt.unwrap_or("-")
    }

    /// Build a TSV line from fields.
    #[must_use]
    pub fn tsv_line(fields: &[&str]) -> String {
        fields
            .iter()
            .map(|f| escape_tsv(f))
            .collect::<Vec<_>>()
            .join("\t")
    }

    /// Build a TSV header line from field names.
    #[must_use]
    pub fn tsv_header(fields: &[&str]) -> String {
        // Headers don't need escaping, but lowercase them for consistency
        fields
            .iter()
            .map(|f| f.to_lowercase())
            .collect::<Vec<_>>()
            .join("\t")
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_status_display() {
        assert_eq!(PlainStatus::Ok.to_string(), "OK");
        assert_eq!(PlainStatus::Warn.to_string(), "WARN");
        assert_eq!(PlainStatus::Fail.to_string(), "FAIL");
        assert_eq!(PlainStatus::Info.to_string(), "INFO");
    }

    #[test]
    fn test_plain_check_result_format() {
        let result = PlainCheckResult::ok("database", "Connection healthy");
        assert_eq!(result.format_plain(), "OK\tdatabase\tConnection healthy");

        let result = PlainCheckResult::warn("index", "May be stale");
        assert_eq!(result.format_plain(), "WARN\tindex\tMay be stale");

        let result = PlainCheckResult::fail("config", "Missing required field");
        assert_eq!(
            result.format_plain(),
            "FAIL\tconfig\tMissing required field"
        );
    }

    #[test]
    fn test_plain_check_results_summary() {
        let mut results = PlainCheckResults::new();
        results.ok("a", "good");
        results.ok("b", "also good");
        results.warn("c", "warning");
        results.fail("d", "bad");

        assert_eq!(results.summary(), "SUMMARY: 2 OK, 1 WARN, 1 FAIL");
    }

    #[test]
    fn test_plain_check_results_format() {
        let mut results = PlainCheckResults::new();
        results.ok("database", "Healthy");
        results.warn("index", "Stale");

        let output = results.format_plain();
        assert!(output.contains("OK\tdatabase\tHealthy"));
        assert!(output.contains("WARN\tindex\tStale"));
        assert!(output.contains("---"));
        assert!(output.contains("SUMMARY: 1 OK, 1 WARN, 0 FAIL"));
    }

    #[test]
    fn test_plain_progress_format() {
        let progress = PlainProgress::new(50, 100);
        assert_eq!(progress.format_plain(), "PROGRESS: 50/100");

        let progress = PlainProgress::with_message(50, 100, "Processing file.md");
        assert_eq!(
            progress.format_plain(),
            "PROGRESS: 50/100 Processing file.md"
        );
    }

    #[test]
    fn test_plain_done_format() {
        let done = PlainDone::new("Processed 100 files");
        assert_eq!(done.format_plain(), "DONE: Processed 100 files");

        let done = PlainDone::with_duration(
            "Processed 100 files",
            std::time::Duration::from_secs_f64(2.3),
        );
        assert!(
            done.format_plain()
                .starts_with("DONE: Processed 100 files in ")
        );
    }

    #[test]
    fn test_plain_error_format() {
        let error = PlainError::new("NOT_FOUND", "Skill not found: xyz");
        assert_eq!(
            error.format_plain(),
            "ERROR: [NOT_FOUND] Skill not found: xyz"
        );

        let error = PlainError::new("VALIDATION", "Invalid field")
            .with_location("skill.md:42")
            .with_hint("Check the format");

        let output = error.format_plain();
        assert!(output.contains("ERROR: [VALIDATION] Invalid field"));
        assert!(output.contains("at: skill.md:42"));
        assert!(output.contains("hint: Check the format"));
    }

    #[test]
    fn test_plain_key_value_format() {
        let kv = PlainKeyValue::new()
            .with("name", "my-skill")
            .with("version", "1.0.0")
            .with("tags", "cli, rust");

        let output = kv.format_plain();
        assert!(output.contains("name: my-skill"));
        assert!(output.contains("version: 1.0.0"));
        assert!(output.contains("tags: cli, rust"));
    }

    #[test]
    fn test_utils_escape_tsv() {
        assert_eq!(utils::escape_tsv("hello\tworld"), "hello world");
        assert_eq!(utils::escape_tsv("line1\nline2"), "line1 line2");
        assert_eq!(utils::escape_tsv("normal"), "normal");
    }

    #[test]
    fn test_utils_format_score() {
        assert_eq!(utils::format_score(0.95), "0.95");
        assert_eq!(utils::format_score(0.1), "0.10");
        assert_eq!(utils::format_score(1.0), "1.00");
    }

    #[test]
    fn test_utils_format_quality() {
        assert_eq!(utils::format_quality(0.85), "85%");
        assert_eq!(utils::format_quality(1.0), "100%");
        assert_eq!(utils::format_quality(0.0), "0%");
    }

    #[test]
    fn test_utils_tsv_line() {
        let line = utils::tsv_line(&["a", "b", "c"]);
        assert_eq!(line, "a\tb\tc");

        // With escaping
        let line = utils::tsv_line(&["hello\tworld", "foo\nbar"]);
        assert_eq!(line, "hello world\tfoo bar");
    }

    #[test]
    fn test_json_envelope_success() {
        let envelope = JsonEnvelope::success("data")
            .with_duration(42)
            .with_timestamp()
            .with_count(10);

        assert!(envelope.success);
        assert_eq!(envelope.data, "data");
        assert_eq!(envelope.meta.duration_ms, Some(42));
        assert_eq!(envelope.meta.count, Some(10));
        assert!(envelope.meta.timestamp.is_some());
    }

    #[test]
    fn test_json_error() {
        let error = JsonError::new("NOT_FOUND", "Skill not found")
            .with_hint("Try searching")
            .with_location("main.rs:42");

        assert!(!error.success);
        assert_eq!(error.error.code, "NOT_FOUND");
        assert_eq!(error.error.hint, Some("Try searching".to_string()));
        assert_eq!(error.error.location, Some("main.rs:42".to_string()));
    }

    #[test]
    fn test_plain_check_results_json() {
        let mut results = PlainCheckResults::new();
        results.ok("database", "Healthy");
        results.fail("index", "Corrupted");

        let json = results.format_json();
        assert!(json["results"].is_array());
        assert_eq!(json["summary"]["ok"], 1);
        assert_eq!(json["summary"]["fail"], 1);
    }
}
