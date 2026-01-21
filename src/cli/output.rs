use chrono::{DateTime, Utc};
use clap::ValueEnum;
use console::style;
use serde::Serialize;

use crate::error::{ErrorCode, MsError, Result, StructuredError};

/// Legacy output mode (Human/Robot)
#[derive(Debug, Clone, Copy)]
pub enum OutputMode {
    Human,
    Robot,
}

/// Output format for CLI commands
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum OutputFormat {
    /// Human-readable formatted output with colors (default)
    #[default]
    Human,
    /// Pretty-printed JSON
    Json,
    /// Newline-delimited JSON (one object per line)
    Jsonl,
    /// Plain text without colors or formatting
    Plain,
    /// Tab-separated values (for shell scripting)
    Tsv,
}

impl OutputFormat {
    /// Determine format from CLI args (robot flag overrides explicit format for backward compat)
    #[must_use]
    pub fn from_args(robot: bool, format: Option<OutputFormat>) -> Self {
        if robot {
            OutputFormat::Json
        } else {
            format.unwrap_or_default()
        }
    }

    /// Check if this format should use colors
    #[must_use]
    pub const fn use_colors(&self) -> bool {
        matches!(self, OutputFormat::Human)
    }

    /// Check if this format is machine-readable
    #[must_use]
    pub const fn is_machine_readable(&self) -> bool {
        matches!(
            self,
            OutputFormat::Json | OutputFormat::Jsonl | OutputFormat::Tsv
        )
    }
}

#[derive(Serialize)]
pub struct RobotResponse<T> {
    pub status: RobotStatus,
    pub timestamp: DateTime<Utc>,
    pub version: String,
    pub data: T,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RobotStatus {
    Ok,
    /// Simple error with just code and message (legacy)
    Error {
        code: String,
        message: String,
    },
    /// Rich error with structured information
    #[serde(rename = "error")]
    StructuredError {
        /// Error code enum value (e.g., "SKILL_NOT_FOUND")
        code: ErrorCode,
        /// Numeric error code (e.g., 101)
        numeric_code: u16,
        /// Human-readable error message
        message: String,
        /// Actionable suggestion for recovery
        suggestion: String,
        /// Additional context for debugging
        #[serde(skip_serializing_if = "Option::is_none")]
        context: Option<serde_json::Value>,
        /// Whether this error is recoverable by the user
        recoverable: bool,
        /// Error category (e.g., "skill", "config")
        category: String,
        /// URL to documentation about this error
        #[serde(skip_serializing_if = "Option::is_none")]
        help_url: Option<String>,
    },
    Partial {
        completed: usize,
        failed: usize,
    },
}

pub fn robot_ok<T: Serialize>(data: T) -> RobotResponse<T> {
    RobotResponse {
        status: RobotStatus::Ok,
        timestamp: Utc::now(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        data,
        warnings: Vec::new(),
    }
}

/// Create a robot error response (legacy format with string code/message).
pub fn robot_error(
    code: impl Into<String>,
    message: impl Into<String>,
) -> RobotResponse<serde_json::Value> {
    RobotResponse {
        status: RobotStatus::Error {
            code: code.into(),
            message: message.into(),
        },
        timestamp: Utc::now(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        data: serde_json::Value::Null,
        warnings: Vec::new(),
    }
}

/// Create a robot error response from an MsError with structured information.
///
/// This includes error codes, suggestions, context, and recovery hints.
pub fn robot_error_structured(err: &MsError) -> RobotResponse<serde_json::Value> {
    let structured = err.to_structured();
    RobotResponse {
        status: RobotStatus::StructuredError {
            code: structured.code,
            numeric_code: structured.numeric_code,
            message: structured.message,
            suggestion: structured.suggestion,
            context: structured.context,
            recoverable: structured.recoverable,
            category: structured.category,
            help_url: structured.help_url,
        },
        timestamp: Utc::now(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        data: serde_json::Value::Null,
        warnings: Vec::new(),
    }
}

/// Create a robot error response from a StructuredError.
pub fn robot_error_from_structured(err: StructuredError) -> RobotResponse<serde_json::Value> {
    RobotResponse {
        status: RobotStatus::StructuredError {
            code: err.code,
            numeric_code: err.numeric_code,
            message: err.message,
            suggestion: err.suggestion,
            context: err.context,
            recoverable: err.recoverable,
            category: err.category,
            help_url: err.help_url,
        },
        timestamp: Utc::now(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        data: serde_json::Value::Null,
        warnings: Vec::new(),
    }
}

impl From<&MsError> for RobotStatus {
    fn from(err: &MsError) -> Self {
        let structured = err.to_structured();
        RobotStatus::StructuredError {
            code: structured.code,
            numeric_code: structured.numeric_code,
            message: structured.message,
            suggestion: structured.suggestion,
            context: structured.context,
            recoverable: structured.recoverable,
            category: structured.category,
            help_url: structured.help_url,
        }
    }
}

impl From<StructuredError> for RobotStatus {
    fn from(err: StructuredError) -> Self {
        RobotStatus::StructuredError {
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

pub fn emit_robot<T: Serialize>(response: &RobotResponse<T>) -> Result<()> {
    emit_json(response)
}

pub fn emit_json<T: Serialize>(value: &T) -> Result<()> {
    let payload = serde_json::to_string_pretty(value)
        .map_err(|err| MsError::Config(format!("serialize output: {err}")))?;
    println!("{payload}");
    Ok(())
}

pub struct HumanLayout {
    lines: Vec<String>,
    key_width: usize,
}

impl Default for HumanLayout {
    fn default() -> Self {
        Self::new()
    }
}

impl HumanLayout {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            lines: Vec::new(),
            key_width: 18,
        }
    }

    pub fn title(&mut self, text: &str) -> &mut Self {
        self.lines.push(style(text).bold().to_string());
        self.lines.push(String::new());
        self
    }

    pub fn section(&mut self, text: &str) -> &mut Self {
        self.lines.push(style(text).bold().to_string());
        self.lines.push("-".repeat(text.len().max(3)));
        self
    }

    pub fn kv(&mut self, key: &str, value: &str) -> &mut Self {
        let key_style = style(key).dim().to_string();
        self.lines.push(format!(
            "{key_style:width$} {value}",
            width = self.key_width
        ));
        self
    }

    pub fn bullet(&mut self, text: &str) -> &mut Self {
        self.lines.push(format!("- {text}"));
        self
    }

    pub fn blank(&mut self) -> &mut Self {
        self.lines.push(String::new());
        self
    }

    pub fn push_line(&mut self, line: impl Into<String>) -> &mut Self {
        self.lines.push(line.into());
        self
    }

    #[must_use]
    pub fn build(self) -> String {
        self.lines.join("\n")
    }
}

pub fn emit_human(layout: HumanLayout) {
    println!("{}", layout.build());
}

/// Trait for types that can format themselves for different output modes
pub trait Formattable {
    /// Format this value for the given output format
    fn format(&self, fmt: OutputFormat) -> String;
}

/// Emit a formattable value to stdout
pub fn emit<T: Formattable>(value: &T, format: OutputFormat) {
    println!("{}", value.format(format));
}

/// Emit a JSON-serializable value with format-aware output
pub fn emit_formatted<T: Serialize>(
    value: &T,
    format: OutputFormat,
    human_fn: impl FnOnce(&T) -> String,
    plain_fn: impl FnOnce(&T) -> String,
    tsv_fn: impl FnOnce(&T) -> String,
) -> Result<()> {
    match format {
        OutputFormat::Human => println!("{}", human_fn(value)),
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(value)
                .map_err(|e| MsError::Config(format!("serialize output: {e}")))?;
            println!("{json}");
        }
        OutputFormat::Jsonl => {
            let json = serde_json::to_string(value)
                .map_err(|e| MsError::Config(format!("serialize output: {e}")))?;
            println!("{json}");
        }
        OutputFormat::Plain => println!("{}", plain_fn(value)),
        OutputFormat::Tsv => println!("{}", tsv_fn(value)),
    }
    Ok(())
}

/// Emit a slice of items in JSONL format (one JSON object per line)
pub fn emit_jsonl<T: Serialize>(items: &[T]) -> Result<()> {
    for item in items {
        let json = serde_json::to_string(item)
            .map_err(|e| MsError::Config(format!("serialize output: {e}")))?;
        println!("{json}");
    }
    Ok(())
}

/// Emit TSV output with headers
pub fn emit_tsv<T, F>(headers: &[&str], items: &[T], row_fn: F)
where
    F: Fn(&T) -> Vec<String>,
{
    println!("{}", headers.join("\t"));
    for item in items {
        println!("{}", row_fn(item).join("\t"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_format_from_args_robot_overrides() {
        // Robot flag overrides explicit format
        assert_eq!(
            OutputFormat::from_args(true, Some(OutputFormat::Plain)),
            OutputFormat::Json
        );
    }

    #[test]
    fn output_format_from_args_uses_explicit() {
        // Explicit format when no robot flag
        assert_eq!(
            OutputFormat::from_args(false, Some(OutputFormat::Jsonl)),
            OutputFormat::Jsonl
        );
    }

    #[test]
    fn output_format_from_args_defaults_to_human() {
        // Default when neither specified
        assert_eq!(OutputFormat::from_args(false, None), OutputFormat::Human);
    }

    #[test]
    fn output_format_use_colors() {
        assert!(OutputFormat::Human.use_colors());
        assert!(!OutputFormat::Json.use_colors());
        assert!(!OutputFormat::Jsonl.use_colors());
        assert!(!OutputFormat::Plain.use_colors());
        assert!(!OutputFormat::Tsv.use_colors());
    }

    #[test]
    fn output_format_is_machine_readable() {
        assert!(!OutputFormat::Human.is_machine_readable());
        assert!(OutputFormat::Json.is_machine_readable());
        assert!(OutputFormat::Jsonl.is_machine_readable());
        assert!(!OutputFormat::Plain.is_machine_readable());
        assert!(OutputFormat::Tsv.is_machine_readable());
    }

    #[test]
    fn output_format_default_is_human() {
        assert_eq!(OutputFormat::default(), OutputFormat::Human);
    }

    #[test]
    fn robot_error_structured_includes_all_fields() {
        let err = MsError::SkillNotFound("test-skill".into());
        let response = robot_error_structured(&err);

        match response.status {
            RobotStatus::StructuredError {
                code,
                numeric_code,
                message,
                suggestion,
                recoverable,
                category,
                ..
            } => {
                assert_eq!(code, ErrorCode::SkillNotFound);
                assert_eq!(numeric_code, 101);
                assert!(message.contains("test-skill"));
                assert!(!suggestion.is_empty());
                assert!(recoverable);
                assert_eq!(category, "skill");
            }
            _ => panic!("Expected StructuredError status"),
        }
    }

    #[test]
    fn robot_status_from_ms_error() {
        let err = MsError::Config("bad config".into());
        let status: RobotStatus = (&err).into();

        match status {
            RobotStatus::StructuredError {
                code,
                numeric_code,
                category,
                ..
            } => {
                assert_eq!(code, ErrorCode::ConfigInvalid);
                assert_eq!(numeric_code, 302);
                assert_eq!(category, "config");
            }
            _ => panic!("Expected StructuredError status"),
        }
    }

    #[test]
    fn robot_error_structured_serialization() {
        let err = MsError::SkillNotFound("my-skill".into());
        let response = robot_error_structured(&err);
        let json = serde_json::to_string(&response).unwrap();

        // Check key fields are present in serialized output
        assert!(json.contains("SKILL_NOT_FOUND"));
        assert!(json.contains("\"numeric_code\":101"));
        assert!(json.contains("\"recoverable\":true"));
        assert!(json.contains("\"category\":\"skill\""));
        assert!(json.contains("\"suggestion\":"));
    }

    #[test]
    fn robot_error_from_structured_error() {
        let structured = StructuredError::new(ErrorCode::NetworkTimeout, "Connection timed out");
        let response = robot_error_from_structured(structured);

        match response.status {
            RobotStatus::StructuredError {
                code,
                numeric_code,
                recoverable,
                ..
            } => {
                assert_eq!(code, ErrorCode::NetworkTimeout);
                assert_eq!(numeric_code, 502);
                assert!(recoverable);
            }
            _ => panic!("Expected StructuredError status"),
        }
    }
}
