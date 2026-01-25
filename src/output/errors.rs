//! Error display system with rich formatting.
//!
//! This module provides beautiful error rendering for both human and machine consumers:
//!
//! - **Rich mode**: Red-bordered panels, code snippets, formatted suggestions
//! - **Plain mode**: Structured text with error codes and hints
//! - **JSON mode**: Full `StructuredError` serialization
//!
//! # Usage
//!
//! ```rust,ignore
//! use ms::error::{MsError, StructuredError};
//! use ms::output::{RichOutput, errors::ErrorRenderer};
//!
//! let output = RichOutput::new(&config, &format, robot_mode);
//! let renderer = ErrorRenderer::new(&output);
//!
//! // Render an MsError
//! let err = MsError::SkillNotFound("my-skill".to_string());
//! renderer.render(&err);
//!
//! // Render with additional context
//! renderer.render_with_context(&err, Some("src/skills/my-skill.md:42"));
//!
//! // Render warnings
//! let warning = WarningRenderer::new(&output);
//! warning.render("Index may be stale", "Run `ms index` to refresh");
//! ```
//!
//! # Error Chain Display
//!
//! For errors with causes, use `ErrorChainDisplay` to show the full chain:
//!
//! ```rust,ignore
//! let chain = ErrorChainDisplay::new(&err);
//! renderer.render_chain(&chain);
//! ```

use rich_rust::prelude::*;
use rich_rust::renderables::Panel;
use serde::Serialize;

use crate::error::{ErrorCode, MsError, StructuredError};

use super::plain_format::{JsonError, PlainError};
use super::rich_output::{OutputMode, RichOutput};

// =============================================================================
// Constants
// =============================================================================

/// Default panel width for error rendering.
const DEFAULT_WIDTH: usize = 80;

/// Maximum width for code snippets.
const MAX_SNIPPET_WIDTH: usize = 120;

// =============================================================================
// ErrorRenderer
// =============================================================================

/// Renders errors with rich formatting.
///
/// Adapts output based on the current `OutputMode`:
/// - Rich: Colorful bordered panels with icons
/// - Plain: Structured text with codes
/// - JSON: Machine-parseable structured output
pub struct ErrorRenderer<'a> {
    output: &'a RichOutput,
    width: usize,
}

impl<'a> ErrorRenderer<'a> {
    /// Create a new error renderer.
    #[must_use]
    pub fn new(output: &'a RichOutput) -> Self {
        Self {
            output,
            width: output.width().min(DEFAULT_WIDTH),
        }
    }

    /// Create an error renderer with custom width.
    #[must_use]
    pub fn with_width(output: &'a RichOutput, width: usize) -> Self {
        Self { output, width }
    }

    /// Render an `MsError`.
    pub fn render(&self, error: &MsError) {
        let structured = error.to_structured();
        self.render_structured(&structured);
    }

    /// Render an `MsError` with additional location context.
    pub fn render_with_context(&self, error: &MsError, location: Option<&str>) {
        let mut structured = error.to_structured();
        if let Some(loc) = location {
            if let Some(ctx) = structured.context.as_mut() {
                if let Some(obj) = ctx.as_object_mut() {
                    obj.insert("location".to_string(), serde_json::json!(loc));
                }
            } else {
                structured.context = Some(serde_json::json!({ "location": loc }));
            }
        }
        self.render_structured(&structured);
    }

    /// Render a `StructuredError`.
    pub fn render_structured(&self, error: &StructuredError) {
        match self.output.mode() {
            OutputMode::Rich => self.render_rich(error),
            OutputMode::Plain => self.render_plain(error),
            OutputMode::Json => self.render_json(error),
        }
    }

    /// Render an error with a code snippet for context.
    pub fn render_with_snippet(
        &self,
        error: &MsError,
        snippet: &str,
        language: &str,
        line_number: Option<usize>,
    ) {
        let structured = error.to_structured();

        match self.output.mode() {
            OutputMode::Rich => {
                self.render_rich(&structured);
                self.output.newline();
                self.render_code_snippet(snippet, language, line_number);
            }
            OutputMode::Plain => {
                self.render_plain(&structured);
                eprintln!();
                self.render_code_snippet_plain(snippet, line_number);
            }
            OutputMode::Json => {
                // Include snippet in JSON output
                let mut json_error = self.build_json_error(&structured);
                if let serde_json::Value::Object(obj) = &mut json_error {
                    if let Some(serde_json::Value::Object(err)) = obj.get_mut("error") {
                        err.insert("snippet".to_string(), serde_json::json!(snippet));
                        err.insert("snippet_language".to_string(), serde_json::json!(language));
                        if let Some(line) = line_number {
                            err.insert("snippet_line".to_string(), serde_json::json!(line));
                        }
                    }
                }
                eprintln!(
                    "{}",
                    serde_json::to_string_pretty(&json_error).unwrap_or_default()
                );
            }
        }
    }

    /// Render an error chain (error with causes).
    pub fn render_chain(&self, chain: &ErrorChainDisplay<'_>) {
        match self.output.mode() {
            OutputMode::Rich => self.render_chain_rich(chain),
            OutputMode::Plain => self.render_chain_plain(chain),
            OutputMode::Json => self.render_chain_json(chain),
        }
    }

    // =========================================================================
    // Rich Mode Rendering
    // =========================================================================

    fn render_rich(&self, error: &StructuredError) {
        let title = format!(
            "{} Error [{}]",
            self.output
                .theme()
                .icons
                .get("error", self.output.use_unicode()),
            error.code
        );

        // Build content with message, suggestion, and optional context
        let mut content = error.message.clone();

        // Add context if present
        if let Some(ref ctx) = error.context {
            if let Some(obj) = ctx.as_object() {
                if !obj.is_empty() {
                    content.push_str("\n\nContext:");
                    for (key, value) in obj {
                        let value_str = match value {
                            serde_json::Value::String(s) => s.clone(),
                            _ => value.to_string(),
                        };
                        content.push_str(&format!("\n  {}: {}", key, value_str));
                    }
                }
            }
        }

        // Add suggestion
        content.push_str(&format!(
            "\n\n{} {}",
            self.output
                .theme()
                .icons
                .get("hint", self.output.use_unicode()),
            error.suggestion
        ));

        // Add help URL if available
        if let Some(ref url) = error.help_url {
            content.push_str(&format!("\n\nMore info: {}", url));
        }

        // Create panel with red border
        let panel = Panel::from_text(&content)
            .title(title)
            .border_style(Style::new().color(Color::parse("red").unwrap_or(Color::default())));

        eprintln!("{}", panel.render_plain(self.width));
    }

    fn render_chain_rich(&self, chain: &ErrorChainDisplay<'_>) {
        // Render primary error
        self.render_rich(&chain.primary);

        // Render cause chain
        for (i, cause) in chain.causes.iter().enumerate() {
            eprintln!();
            let title = format!(
                "{} Caused by [{}]",
                if i == chain.causes.len() - 1 {
                    "└─"
                } else {
                    "├─"
                },
                cause.code
            );

            let content = format!("{}\n\nSuggestion: {}", cause.message, cause.suggestion);

            let panel = Panel::from_text(&content).title(title).border_style(
                Style::new().color(Color::parse("yellow").unwrap_or(Color::default())),
            );

            eprintln!("{}", panel.render_plain(self.width.saturating_sub(2)));
        }
    }

    // =========================================================================
    // Plain Mode Rendering
    // =========================================================================

    fn render_plain(&self, error: &StructuredError) {
        let plain_error = PlainError::new(error.code.to_string(), &error.message);

        // Add location from context if present
        let plain_error = if let Some(ref ctx) = error.context {
            if let Some(loc) = ctx.get("location").and_then(|v| v.as_str()) {
                plain_error.with_location(loc)
            } else {
                plain_error
            }
        } else {
            plain_error
        };

        // Add hint
        let plain_error = plain_error.with_hint(&error.suggestion);

        eprintln!("{}", plain_error.format_plain());
    }

    fn render_chain_plain(&self, chain: &ErrorChainDisplay<'_>) {
        // Render primary error
        self.render_plain(&chain.primary);

        // Render causes
        for cause in &chain.causes {
            eprintln!("  caused by: [{}] {}", cause.code, cause.message);
        }
    }

    // =========================================================================
    // JSON Mode Rendering
    // =========================================================================

    fn render_json(&self, error: &StructuredError) {
        let json_error = self.build_json_error(error);
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&json_error).unwrap_or_default()
        );
    }

    fn build_json_error(&self, error: &StructuredError) -> serde_json::Value {
        let json_error =
            JsonError::new(error.code.to_string(), &error.message).with_hint(&error.suggestion);

        let mut value = serde_json::to_value(&json_error).unwrap_or(serde_json::json!({}));

        // Add extra fields from StructuredError
        if let serde_json::Value::Object(obj) = &mut value {
            if let Some(serde_json::Value::Object(err)) = obj.get_mut("error") {
                err.insert(
                    "numeric_code".to_string(),
                    serde_json::json!(error.numeric_code),
                );
                err.insert("category".to_string(), serde_json::json!(error.category));
                err.insert(
                    "recoverable".to_string(),
                    serde_json::json!(error.recoverable),
                );

                if let Some(ref ctx) = error.context {
                    err.insert("context".to_string(), ctx.clone());
                }

                if let Some(ref url) = error.help_url {
                    err.insert("help_url".to_string(), serde_json::json!(url));
                }
            }
        }

        value
    }

    fn render_chain_json(&self, chain: &ErrorChainDisplay<'_>) {
        let causes: Vec<serde_json::Value> = chain
            .causes
            .iter()
            .map(|c| {
                serde_json::json!({
                    "code": c.code.to_string(),
                    "numeric_code": c.numeric_code,
                    "message": c.message,
                    "suggestion": c.suggestion,
                })
            })
            .collect();

        let mut primary = self.build_json_error(&chain.primary);
        if let serde_json::Value::Object(ref mut obj) = primary {
            obj.insert("causes".to_string(), serde_json::json!(causes));
        }

        eprintln!(
            "{}",
            serde_json::to_string_pretty(&primary).unwrap_or_default()
        );
    }

    // =========================================================================
    // Code Snippet Rendering
    // =========================================================================

    fn render_code_snippet(&self, snippet: &str, language: &str, line_number: Option<usize>) {
        let width = self.width.min(MAX_SNIPPET_WIDTH);

        // Build snippet with line numbers
        let lines: Vec<&str> = snippet.lines().collect();
        let start_line = line_number.unwrap_or(1);
        let line_width = (start_line + lines.len()).to_string().len();

        let numbered_lines: Vec<String> = lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let ln = start_line + i;
                format!("{:>width$} │ {}", ln, line, width = line_width)
            })
            .collect();

        let content = numbered_lines.join("\n");

        let panel = Panel::from_text(&content)
            .title(format!("Code ({})", language))
            .border_style(Style::new().color(Color::parse("dim").unwrap_or(Color::default())));

        eprintln!("{}", panel.render_plain(width));
    }

    fn render_code_snippet_plain(&self, snippet: &str, line_number: Option<usize>) {
        eprintln!("--- Code ---");
        let start_line = line_number.unwrap_or(1);
        for (i, line) in snippet.lines().enumerate() {
            eprintln!("{:>4} | {}", start_line + i, line);
        }
        eprintln!("---");
    }
}

// =============================================================================
// WarningRenderer
// =============================================================================

/// Renders warnings with rich formatting.
///
/// Warnings are less severe than errors and use yellow styling.
pub struct WarningRenderer<'a> {
    output: &'a RichOutput,
    width: usize,
}

impl<'a> WarningRenderer<'a> {
    /// Create a new warning renderer.
    #[must_use]
    pub fn new(output: &'a RichOutput) -> Self {
        Self {
            output,
            width: output.width().min(DEFAULT_WIDTH),
        }
    }

    /// Create a warning renderer with custom width.
    #[must_use]
    pub fn with_width(output: &'a RichOutput, width: usize) -> Self {
        Self { output, width }
    }

    /// Render a simple warning message.
    pub fn render(&self, message: &str, suggestion: Option<&str>) {
        match self.output.mode() {
            OutputMode::Rich => self.render_rich(message, suggestion),
            OutputMode::Plain => self.render_plain(message, suggestion),
            OutputMode::Json => self.render_json(message, suggestion),
        }
    }

    /// Render a warning with a title.
    pub fn render_titled(&self, title: &str, message: &str, suggestion: Option<&str>) {
        match self.output.mode() {
            OutputMode::Rich => self.render_rich_titled(title, message, suggestion),
            OutputMode::Plain => self.render_plain_titled(title, message, suggestion),
            OutputMode::Json => self.render_json_titled(title, message, suggestion),
        }
    }

    /// Render multiple warnings as a list.
    pub fn render_list(&self, warnings: &[WarningItem]) {
        match self.output.mode() {
            OutputMode::Rich => self.render_list_rich(warnings),
            OutputMode::Plain => self.render_list_plain(warnings),
            OutputMode::Json => self.render_list_json(warnings),
        }
    }

    // =========================================================================
    // Rich Mode
    // =========================================================================

    fn render_rich(&self, message: &str, suggestion: Option<&str>) {
        let icon = self
            .output
            .theme()
            .icons
            .get("warning", self.output.use_unicode());
        let title = format!("{} Warning", icon);

        let content = match suggestion {
            Some(s) => format!(
                "{}\n\n{} {}",
                message,
                self.output
                    .theme()
                    .icons
                    .get("hint", self.output.use_unicode()),
                s
            ),
            None => message.to_string(),
        };

        let panel = Panel::from_text(&content)
            .title(title)
            .border_style(Style::new().color(Color::parse("yellow").unwrap_or(Color::default())));

        eprintln!("{}", panel.render_plain(self.width));
    }

    fn render_rich_titled(&self, title: &str, message: &str, suggestion: Option<&str>) {
        let icon = self
            .output
            .theme()
            .icons
            .get("warning", self.output.use_unicode());
        let full_title = format!("{} {}", icon, title);

        let content = match suggestion {
            Some(s) => format!(
                "{}\n\n{} {}",
                message,
                self.output
                    .theme()
                    .icons
                    .get("hint", self.output.use_unicode()),
                s
            ),
            None => message.to_string(),
        };

        let panel = Panel::from_text(&content)
            .title(full_title)
            .border_style(Style::new().color(Color::parse("yellow").unwrap_or(Color::default())));

        eprintln!("{}", panel.render_plain(self.width));
    }

    fn render_list_rich(&self, warnings: &[WarningItem]) {
        let icon = self
            .output
            .theme()
            .icons
            .get("warning", self.output.use_unicode());

        let items: Vec<String> = warnings
            .iter()
            .map(|w| match &w.suggestion {
                Some(s) => format!("• {} (hint: {})", w.message, s),
                None => format!("• {}", w.message),
            })
            .collect();

        let content = items.join("\n");

        let panel = Panel::from_text(&content)
            .title(format!("{} {} Warning(s)", icon, warnings.len()))
            .border_style(Style::new().color(Color::parse("yellow").unwrap_or(Color::default())));

        eprintln!("{}", panel.render_plain(self.width));
    }

    // =========================================================================
    // Plain Mode
    // =========================================================================

    fn render_plain(&self, message: &str, suggestion: Option<&str>) {
        eprintln!("WARN: {}", message);
        if let Some(s) = suggestion {
            eprintln!("  hint: {}", s);
        }
    }

    fn render_plain_titled(&self, title: &str, message: &str, suggestion: Option<&str>) {
        eprintln!("WARN [{}]: {}", title, message);
        if let Some(s) = suggestion {
            eprintln!("  hint: {}", s);
        }
    }

    fn render_list_plain(&self, warnings: &[WarningItem]) {
        for w in warnings {
            self.render_plain(&w.message, w.suggestion.as_deref());
        }
    }

    // =========================================================================
    // JSON Mode
    // =========================================================================

    fn render_json(&self, message: &str, suggestion: Option<&str>) {
        let warning = WarningJson {
            level: "warning",
            message: message.to_string(),
            suggestion: suggestion.map(String::from),
            title: None,
        };
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&warning).unwrap_or_default()
        );
    }

    fn render_json_titled(&self, title: &str, message: &str, suggestion: Option<&str>) {
        let warning = WarningJson {
            level: "warning",
            message: message.to_string(),
            suggestion: suggestion.map(String::from),
            title: Some(title.to_string()),
        };
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&warning).unwrap_or_default()
        );
    }

    fn render_list_json(&self, warnings: &[WarningItem]) {
        let json_warnings: Vec<WarningJson> = warnings
            .iter()
            .map(|w| WarningJson {
                level: "warning",
                message: w.message.clone(),
                suggestion: w.suggestion.clone(),
                title: w.title.clone(),
            })
            .collect();

        let output = serde_json::json!({
            "warnings": json_warnings,
            "count": warnings.len(),
        });

        eprintln!(
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_default()
        );
    }
}

// =============================================================================
// Supporting Types
// =============================================================================

/// A warning item for batch rendering.
#[derive(Debug, Clone)]
pub struct WarningItem {
    /// Warning message.
    pub message: String,
    /// Optional suggestion for resolution.
    pub suggestion: Option<String>,
    /// Optional title/category.
    pub title: Option<String>,
}

impl WarningItem {
    /// Create a new warning item.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            suggestion: None,
            title: None,
        }
    }

    /// Add a suggestion.
    #[must_use]
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    /// Add a title.
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

/// JSON representation of a warning.
#[derive(Debug, Clone, Serialize)]
struct WarningJson {
    level: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggestion: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
}

/// Displays an error with its chain of causes.
#[derive(Debug)]
pub struct ErrorChainDisplay<'a> {
    /// The primary error.
    pub primary: StructuredError,
    /// Chain of cause errors (from immediate to root).
    pub causes: Vec<StructuredError>,
    /// Original error reference for additional context.
    _original: Option<&'a MsError>,
}

impl<'a> ErrorChainDisplay<'a> {
    /// Create an error chain display from an `MsError`.
    #[must_use]
    pub fn new(error: &'a MsError) -> Self {
        Self {
            primary: error.to_structured(),
            causes: Vec::new(),
            _original: Some(error),
        }
    }

    /// Create an error chain display from a `StructuredError`.
    #[must_use]
    pub fn from_structured(error: StructuredError) -> Self {
        Self {
            primary: error,
            causes: Vec::new(),
            _original: None,
        }
    }

    /// Add a cause to the chain.
    #[must_use]
    pub fn with_cause(mut self, cause: StructuredError) -> Self {
        self.causes.push(cause);
        self
    }

    /// Add a cause from an `MsError`.
    #[must_use]
    pub fn with_cause_error(mut self, cause: &MsError) -> Self {
        self.causes.push(cause.to_structured());
        self
    }
}

// =============================================================================
// Convenience Functions
// =============================================================================

/// Render an error using default settings.
///
/// This is a convenience function for simple error display.
pub fn render_error(output: &RichOutput, error: &MsError) {
    ErrorRenderer::new(output).render(error);
}

/// Render a structured error using default settings.
pub fn render_structured_error(output: &RichOutput, error: &StructuredError) {
    ErrorRenderer::new(output).render_structured(error);
}

/// Render a warning using default settings.
pub fn render_warning(output: &RichOutput, message: &str, suggestion: Option<&str>) {
    WarningRenderer::new(output).render(message, suggestion);
}

/// Format an error as a JSON string.
///
/// Returns `None` if serialization fails.
#[must_use]
pub fn error_to_json(error: &StructuredError) -> Option<String> {
    let json_error =
        JsonError::new(error.code.to_string(), &error.message).with_hint(&error.suggestion);

    serde_json::to_string_pretty(&json_error).ok()
}

/// Format an error as a plain text string.
#[must_use]
pub fn error_to_plain(error: &StructuredError) -> String {
    let plain_error =
        PlainError::new(error.code.to_string(), &error.message).with_hint(&error.suggestion);
    plain_error.format_plain()
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn plain_output() -> RichOutput {
        RichOutput::plain()
    }

    #[test]
    fn test_error_renderer_creation() {
        let output = plain_output();
        let renderer = ErrorRenderer::new(&output);
        assert!(renderer.width <= DEFAULT_WIDTH);
    }

    #[test]
    fn test_error_renderer_with_width() {
        let output = plain_output();
        let renderer = ErrorRenderer::with_width(&output, 60);
        assert_eq!(renderer.width, 60);
    }

    #[test]
    fn test_warning_renderer_creation() {
        let output = plain_output();
        let renderer = WarningRenderer::new(&output);
        assert!(renderer.width <= DEFAULT_WIDTH);
    }

    #[test]
    fn test_warning_item_builder() {
        let item = WarningItem::new("Test warning")
            .with_suggestion("Fix it")
            .with_title("Category");

        assert_eq!(item.message, "Test warning");
        assert_eq!(item.suggestion, Some("Fix it".to_string()));
        assert_eq!(item.title, Some("Category".to_string()));
    }

    #[test]
    fn test_error_chain_display_creation() {
        let error = MsError::SkillNotFound("test".to_string());
        let chain = ErrorChainDisplay::new(&error);

        assert_eq!(chain.primary.code, ErrorCode::SkillNotFound);
        assert!(chain.causes.is_empty());
    }

    #[test]
    fn test_error_chain_display_with_causes() {
        let error = MsError::SkillNotFound("test".to_string());
        let cause = MsError::Config("bad config".to_string());

        let chain = ErrorChainDisplay::new(&error).with_cause_error(&cause);

        assert_eq!(chain.primary.code, ErrorCode::SkillNotFound);
        assert_eq!(chain.causes.len(), 1);
        assert_eq!(chain.causes[0].code, ErrorCode::ConfigInvalid);
    }

    #[test]
    fn test_error_to_plain() {
        let error = StructuredError::new(ErrorCode::SkillNotFound, "Skill 'test' not found");
        let plain = error_to_plain(&error);

        assert!(plain.contains("ERROR:"));
        assert!(plain.contains("E101"));
        assert!(plain.contains("test"));
    }

    #[test]
    fn test_error_to_json() {
        let error = StructuredError::new(ErrorCode::SkillNotFound, "Skill 'test' not found");
        let json = error_to_json(&error);

        assert!(json.is_some());
        let json_str = json.unwrap();
        assert!(json_str.contains("\"success\": false"));
        assert!(json_str.contains("SKILL_NOT_FOUND") || json_str.contains("E101"));
    }

    #[test]
    fn test_render_error_convenience() {
        let output = plain_output();
        let error = MsError::SkillNotFound("my-skill".to_string());

        // Should not panic
        render_error(&output, &error);
    }

    #[test]
    fn test_render_warning_convenience() {
        let output = plain_output();

        // Should not panic
        render_warning(&output, "Test warning", Some("Fix it"));
        render_warning(&output, "Another warning", None);
    }

    #[test]
    fn test_json_error_structure() {
        let error = StructuredError::new(ErrorCode::ConfigInvalid, "Bad config")
            .with_suggestion("Check the file");

        let json = error_to_json(&error).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["success"], false);
        assert!(parsed["error"]["code"].as_str().is_some());
        assert!(parsed["error"]["message"].as_str().is_some());
    }

    #[test]
    fn test_warning_json_serialization() {
        let warning = WarningJson {
            level: "warning",
            message: "Test".to_string(),
            suggestion: Some("Fix".to_string()),
            title: None,
        };

        let json = serde_json::to_string(&warning).unwrap();
        assert!(json.contains("warning"));
        assert!(json.contains("Test"));
        assert!(json.contains("Fix"));
    }
}
