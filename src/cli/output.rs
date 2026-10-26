use std::sync::atomic::{AtomicU8, Ordering};

use chrono::{DateTime, Utc};
use clap::ValueEnum;
use console::style;
use serde::Serialize;

use crate::error::{ErrorCode, MsError, Result, StructuredError};

static ACTIVE_OUTPUT_FORMAT: AtomicU8 = AtomicU8::new(0);

/// Set the process-wide active output format so helpers like [`emit_json`]
/// automatically render `Toon`, `Jsonl`, `Tsv`, or `Plain` when requested.
pub fn set_active_output_format(format: OutputFormat) {
    ACTIVE_OUTPUT_FORMAT.store(format.to_u8(), Ordering::Relaxed);
}

/// Get the process-wide active output format.
#[must_use]
pub fn active_output_format() -> OutputFormat {
    OutputFormat::from_u8(ACTIVE_OUTPUT_FORMAT.load(Ordering::Relaxed))
}

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
    /// Token-optimized output notation (40-60% fewer tokens than JSON)
    Toon,
}

impl OutputFormat {
    const fn to_u8(self) -> u8 {
        match self {
            Self::Human => 0,
            Self::Json => 1,
            Self::Jsonl => 2,
            Self::Plain => 3,
            Self::Tsv => 4,
            Self::Toon => 5,
        }
    }

    const fn from_u8(val: u8) -> Self {
        match val {
            1 => Self::Json,
            2 => Self::Jsonl,
            3 => Self::Plain,
            4 => Self::Tsv,
            5 => Self::Toon,
            _ => Self::Human,
        }
    }

    /// Parse a format string case-insensitively (e.g. from config or env vars).
    #[must_use]
    pub fn from_str_loose(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "human" => Some(Self::Human),
            "json" => Some(Self::Json),
            "jsonl" | "ndjson" => Some(Self::Jsonl),
            "plain" | "text" => Some(Self::Plain),
            "tsv" => Some(Self::Tsv),
            "toon" => Some(Self::Toon),
            _ => None,
        }
    }

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
            OutputFormat::Json | OutputFormat::Jsonl | OutputFormat::Tsv | OutputFormat::Toon
        )
    }
}

/// Legacy robot response format.
///
/// # Deprecated
///
/// This type is deprecated in favor of [`crate::output::JsonResponse`] which provides
/// a cleaner, more consistent API for machine-readable output. New code should use:
///
/// ```rust,ignore
/// use ms::output::{JsonResponse, JsonEnvelope};
///
/// // For success:
/// let response = JsonResponse::success(data);
///
/// // For errors:
/// let response = JsonResponse::error("NOT_FOUND", "Skill not found");
/// ```
///
/// `RobotResponse` is maintained for backward compatibility with existing integrations.
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
    robot_error_from_structured(err.to_structured())
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

static ACTIVE_ROBOT_INCLUDE_METADATA: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(true);

/// Configure whether [`emit_robot`] includes top-level `timestamp` and `version` metadata.
pub fn set_robot_include_metadata(include: bool) {
    ACTIVE_ROBOT_INCLUDE_METADATA.store(include, Ordering::Relaxed);
}

/// Return whether [`emit_robot`] includes top-level `timestamp` and `version` metadata.
#[must_use]
pub fn robot_include_metadata() -> bool {
    ACTIVE_ROBOT_INCLUDE_METADATA.load(Ordering::Relaxed)
}

pub fn emit_robot<T: Serialize>(response: &RobotResponse<T>) -> Result<()> {
    if std::env::var_os("MS_JSON_ENVELOPE").is_some() {
        if matches!(response.status, RobotStatus::Ok) && !robot_include_metadata() {
            return emit_success(&response.data);
        }
        let converted = robot_to_json_response(response);
        return emit_json(&converted);
    }
    if robot_include_metadata() {
        emit_json(response)
    } else {
        let mut val = serde_json::to_value(response)
            .map_err(|err| MsError::Config(format!("serialize output: {err}")))?;
        if let serde_json::Value::Object(ref mut map) = val {
            map.remove("timestamp");
            map.remove("version");
        }
        emit_json(&val)
    }
}

pub fn emit_json<T: Serialize>(value: &T) -> Result<()> {
    emit_json_for_format(value, active_output_format())
}

/// Emit a serializable value according to the specified [`OutputFormat`].
pub fn emit_json_for_format<T: Serialize>(value: &T, format: OutputFormat) -> Result<()> {
    let _uses_colors = format.use_colors();
    match format {
        OutputFormat::Human | OutputFormat::Json => {
            let payload = serde_json::to_string_pretty(value)
                .map_err(|err| MsError::Config(format!("serialize output: {err}")))?;
            println!("{payload}");
            Ok(())
        }
        OutputFormat::Jsonl => {
            let val = serde_json::to_value(value)
                .map_err(|err| MsError::Config(format!("serialize output: {err}")))?;
            if let serde_json::Value::Array(items) = val {
                emit_jsonl(&items)?;
            } else {
                let line = serde_json::to_string(&val)
                    .map_err(|err| MsError::Config(format!("serialize output: {err}")))?;
                println!("{line}");
            }
            Ok(())
        }
        OutputFormat::Toon => emit_toon(value),
        OutputFormat::Plain => {
            let val = serde_json::to_value(value)
                .map_err(|err| MsError::Config(format!("serialize output: {err}")))?;
            println!("{}", format_value_plain(&val));
            Ok(())
        }
        OutputFormat::Tsv => {
            let val = serde_json::to_value(value)
                .map_err(|err| MsError::Config(format!("serialize output: {err}")))?;
            println!("{}", format_value_tsv(&val));
            Ok(())
        }
    }
}

fn scalar_to_plain(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

fn format_value_plain(val: &serde_json::Value) -> String {
    let mut lines = Vec::new();
    match val {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                match v {
                    serde_json::Value::Object(sub) => {
                        for (sk, sv) in sub {
                            lines.push(format!("{k}.{sk}: {}", scalar_to_plain(sv)));
                        }
                    }
                    serde_json::Value::Array(arr) => {
                        lines.push(format!("{k}_count: {}", arr.len()));
                        for (idx, item) in arr.iter().enumerate() {
                            if let serde_json::Value::Object(obj) = item {
                                let summary: Vec<String> = obj
                                    .iter()
                                    .map(|(ik, iv)| format!("{ik}={}", scalar_to_plain(iv)))
                                    .collect();
                                lines.push(format!("{k}[{idx}]: {}", summary.join(" ")));
                            } else {
                                lines.push(format!("{k}[{idx}]: {}", scalar_to_plain(item)));
                            }
                        }
                    }
                    _ => lines.push(format!("{k}: {}", scalar_to_plain(v))),
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                if let serde_json::Value::Object(obj) = item {
                    let summary: Vec<String> = obj
                        .iter()
                        .map(|(ik, iv)| format!("{ik}={}", scalar_to_plain(iv)))
                        .collect();
                    lines.push(summary.join(" "));
                } else {
                    lines.push(scalar_to_plain(item));
                }
            }
        }
        _ => lines.push(scalar_to_plain(val)),
    }
    lines.join("\n")
}

fn format_value_tsv(val: &serde_json::Value) -> String {
    // If top-level is an array of objects, or an object containing an array of objects, render table TSV
    let array_opt = match val {
        serde_json::Value::Array(arr) => Some(arr),
        serde_json::Value::Object(map) => map.values().find_map(|v| {
            if let serde_json::Value::Array(arr) = v {
                if arr.first().is_some_and(serde_json::Value::is_object) {
                    return Some(arr);
                }
            }
            None
        }),
        _ => None,
    };

    if let Some(arr) = array_opt {
        if let Some(serde_json::Value::Object(first)) = arr.first() {
            let headers: Vec<&str> = first.keys().map(String::as_str).collect();
            let mut rows = Vec::with_capacity(arr.len() + 1);
            rows.push(headers.join("\t"));
            for item in arr {
                if let serde_json::Value::Object(obj) = item {
                    let cols: Vec<String> = headers
                        .iter()
                        .map(|h| {
                            obj.get(*h)
                                .map(scalar_to_plain)
                                .unwrap_or_default()
                                .replace(['\t', '\n', '\r'], " ")
                        })
                        .collect();
                    rows.push(cols.join("\t"));
                }
            }
            return rows.join("\n");
        }
    }

    if let serde_json::Value::Object(map) = val {
        let mut rows = Vec::with_capacity(map.len() + 1);
        rows.push("key\tvalue".to_string());
        for (k, v) in map {
            let clean = scalar_to_plain(v).replace(['\t', '\n', '\r'], " ");
            rows.push(format!("{k}\t{clean}"));
        }
        return rows.join("\n");
    }

    format!(
        "value\t{}",
        scalar_to_plain(val).replace(['\t', '\n', '\r'], " ")
    )
}

/// Emit a value in TOON format (Token-Optimized Object Notation).
///
/// TOON provides 40-60% token savings compared to JSON, making it ideal
/// for AI agent workflows where token efficiency matters.
pub fn emit_toon<T: Serialize>(value: &T) -> Result<()> {
    let json = serde_json::to_value(value)
        .map_err(|e| MsError::Config(format!("serialize for TOON: {e}")))?;
    let toon = toon_rust::encode(json, None);
    println!("{toon}");
    Ok(())
}

// =============================================================================
// Migration Helpers: RobotResponse -> JsonResponse
// =============================================================================

use crate::output::{JsonEnvelope, JsonMeta, JsonResponse, JsonStructuredErrorResponse};

/// Emit a success response using the new JsonResponse format.
///
/// This is the recommended way to emit JSON responses for new code.
/// It produces output compatible with the unified response schema.
pub fn emit_success<T: Serialize>(data: T) -> Result<()> {
    let meta = JsonEnvelope::success(()).with_timestamp().meta;
    emit_success_with_meta(data, meta)
}

/// Emit a success response with metadata.
pub fn emit_success_with_meta<T: Serialize>(data: T, meta: JsonMeta) -> Result<()> {
    let envelope = JsonEnvelope::success_with_meta(data, meta);
    emit_json(&envelope)
}

/// Emit an error response using the new JsonResponse format.
pub fn emit_error_response(code: impl Into<String>, message: impl Into<String>) -> Result<()> {
    let response: JsonResponse<()> = JsonResponse::error(code, message);
    emit_json(&response)
}

/// Emit a structured error response with full error details.
pub fn emit_structured_error(err: &MsError) -> Result<()> {
    let structured = err.to_structured();
    let response = JsonStructuredErrorResponse::from_structured(structured);
    emit_json(&response)
}

/// Convert a RobotResponse to the new JsonResponse format.
///
/// This helper allows gradual migration from the legacy format.
/// Returns a serde_json::Value that matches the new schema.
pub fn robot_to_json_response<T: Serialize>(robot: &RobotResponse<T>) -> serde_json::Value {
    match &robot.status {
        RobotStatus::Ok => {
            let meta = JsonMeta {
                timestamp: Some(robot.timestamp.to_rfc3339()),
                ..Default::default()
            };
            serde_json::json!({
                "success": true,
                "data": robot.data,
                "meta": meta,
            })
        }
        RobotStatus::Error { code, message } => {
            serde_json::json!({
                "success": false,
                "error": {
                    "code": code,
                    "message": message,
                }
            })
        }
        RobotStatus::StructuredError {
            code,
            numeric_code,
            message,
            suggestion,
            context,
            recoverable,
            category,
            help_url,
        } => {
            serde_json::json!({
                "success": false,
                "error": {
                    "code": code,
                    "numeric_code": numeric_code,
                    "message": message,
                    "suggestion": suggestion,
                    "context": context,
                    "recoverable": recoverable,
                    "category": category,
                    "help_url": help_url,
                }
            })
        }
        RobotStatus::Partial { completed, failed } => {
            serde_json::json!({
                "success": false,
                "error": {
                    "code": "PARTIAL_FAILURE",
                    "message": format!("Partial completion: {} succeeded, {} failed", completed, failed),
                },
                "meta": {
                    "completed": completed,
                    "failed": failed,
                }
            })
        }
    }
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
        OutputFormat::Toon => emit_toon(value)?,
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
        assert!(!OutputFormat::Toon.use_colors());
    }

    #[test]
    fn output_format_is_machine_readable() {
        assert!(!OutputFormat::Human.is_machine_readable());
        assert!(OutputFormat::Json.is_machine_readable());
        assert!(OutputFormat::Jsonl.is_machine_readable());
        assert!(!OutputFormat::Plain.is_machine_readable());
        assert!(OutputFormat::Tsv.is_machine_readable());
        assert!(OutputFormat::Toon.is_machine_readable());
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

    #[test]
    fn test_robot_include_metadata_toggle() {
        set_robot_include_metadata(false);
        assert!(!robot_include_metadata());
        set_robot_include_metadata(true);
        assert!(robot_include_metadata());
    }
}
