//! High-level message renderers for success, info, hints, and status tracking.
//!
//! This module provides structured renderers that produce consistent,
//! mode-aware output across all commands. Each renderer adapts to the
//! current output mode:
//!
//! - **Rich mode**: Styled panels, colored icons, Unicode decorations
//! - **Plain mode**: Clean text with ASCII indicators
//! - **Agent/JSON mode**: Structured JSON output, no styling or tips
//!
//! # Example
//!
//! ```rust,ignore
//! use ms::output::messages::{SuccessRenderer, InfoRenderer, HintDisplay, StatusTracker};
//! use ms::output::RichOutput;
//!
//! let output = RichOutput::plain();
//!
//! // Success with next steps
//! SuccessRenderer::new(&output, "Skill created")
//!     .next_step("Run `ms show my-skill` to view it")
//!     .next_step("Run `ms search` to find it")
//!     .render();
//!
//! // Info message
//! InfoRenderer::new(&output, "Indexing 42 skills").render();
//!
//! // Hint/tip
//! HintDisplay::new(&output, "Use --explain for score breakdowns").render();
//!
//! // Status tracking
//! let mut tracker = StatusTracker::new(&output, "Import workflow");
//! tracker.step("Parsing input");
//! tracker.step("Validating skills");
//! tracker.complete("Imported 5 skills");
//! ```

use serde::Serialize;
use tracing::trace;

use super::rich_output::{OutputMode, RichOutput};

// =============================================================================
// SuccessRenderer
// =============================================================================

/// Renders success messages with optional next steps.
///
/// In rich mode, displays a green-bordered panel with a checkmark icon.
/// In plain mode, outputs "OK: message" with optional numbered steps.
/// In JSON mode, outputs `{"status": "success", "message": "..."}`.
pub struct SuccessRenderer<'a> {
    output: &'a RichOutput,
    message: String,
    next_steps: Vec<String>,
    detail: Option<String>,
}

impl<'a> SuccessRenderer<'a> {
    /// Create a new success renderer.
    #[must_use]
    pub fn new(output: &'a RichOutput, message: &str) -> Self {
        Self {
            output,
            message: message.to_string(),
            next_steps: Vec::new(),
            detail: None,
        }
    }

    /// Add a next step hint.
    #[must_use]
    pub fn next_step(mut self, step: &str) -> Self {
        self.next_steps.push(step.to_string());
        self
    }

    /// Add optional detail text shown below the message.
    #[must_use]
    pub fn detail(mut self, detail: &str) -> Self {
        self.detail = Some(detail.to_string());
        self
    }

    /// Render the success message to stdout.
    pub fn render(&self) {
        trace!(mode = ?self.output.mode(), "SuccessRenderer::render");

        match self.output.mode() {
            OutputMode::Json => {
                self.render_json();
            }
            OutputMode::Rich => {
                self.render_rich();
            }
            OutputMode::Plain => {
                self.render_plain();
            }
        }
    }

    /// Render as structured JSON.
    fn render_json(&self) {
        #[derive(Serialize)]
        struct SuccessJson {
            status: &'static str,
            message: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            detail: Option<String>,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            next_steps: Vec<String>,
        }

        let json = SuccessJson {
            status: "success",
            message: self.message.clone(),
            detail: self.detail.clone(),
            next_steps: self.next_steps.clone(),
        };

        if let Ok(s) = serde_json::to_string(&json) {
            println!("{s}");
        }
    }

    /// Render with rich styling.
    fn render_rich(&self) {
        let icon = self.output.theme().icons.get("success", self.output.use_unicode());
        let styled_msg = self.output.format_styled(&self.message, "bold green");

        println!();
        println!("{icon} {styled_msg}");

        if let Some(ref detail) = self.detail {
            let styled = self.output.format_styled(detail, "dim");
            println!("  {styled}");
        }

        if !self.next_steps.is_empty() {
            println!();
            let label = self.output.format_styled("Next steps:", "bold");
            println!("  {label}");
            for (i, step) in self.next_steps.iter().enumerate() {
                let num = self.output.format_styled(&format!("{}.", i + 1), "bold cyan");
                println!("    {num} {step}");
            }
        }
        println!();
    }

    /// Render as plain text.
    fn render_plain(&self) {
        println!("OK: {}", self.message);

        if let Some(ref detail) = self.detail {
            println!("  {detail}");
        }

        if !self.next_steps.is_empty() {
            println!("Next steps:");
            for (i, step) in self.next_steps.iter().enumerate() {
                println!("  {}. {}", i + 1, step);
            }
        }
    }
}

// =============================================================================
// InfoRenderer
// =============================================================================

/// Renders informational messages.
///
/// In rich mode, displays with a blue info icon and optional context.
/// In plain mode, outputs "INFO: message".
/// In JSON mode, outputs `{"level": "info", "message": "..."}`.
pub struct InfoRenderer<'a> {
    output: &'a RichOutput,
    message: String,
    context: Vec<(String, String)>,
}

impl<'a> InfoRenderer<'a> {
    /// Create a new info renderer.
    #[must_use]
    pub fn new(output: &'a RichOutput, message: &str) -> Self {
        Self {
            output,
            message: message.to_string(),
            context: Vec::new(),
        }
    }

    /// Add a key-value context pair displayed alongside the message.
    #[must_use]
    pub fn context(mut self, key: &str, value: &str) -> Self {
        self.context.push((key.to_string(), value.to_string()));
        self
    }

    /// Render the info message to stdout.
    pub fn render(&self) {
        trace!(mode = ?self.output.mode(), "InfoRenderer::render");

        match self.output.mode() {
            OutputMode::Json => {
                self.render_json();
            }
            OutputMode::Rich => {
                self.render_rich();
            }
            OutputMode::Plain => {
                self.render_plain();
            }
        }
    }

    /// Render as structured JSON.
    fn render_json(&self) {
        #[derive(Serialize)]
        struct InfoJson {
            level: &'static str,
            message: String,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            context: Vec<(String, String)>,
        }

        let json = InfoJson {
            level: "info",
            message: self.message.clone(),
            context: self.context.clone(),
        };

        if let Ok(s) = serde_json::to_string(&json) {
            println!("{s}");
        }
    }

    /// Render with rich styling.
    fn render_rich(&self) {
        let icon = self.output.theme().icons.get("info", self.output.use_unicode());
        let styled_msg = self.output.format_styled(&self.message, "cyan");
        println!("{icon} {styled_msg}");

        for (key, value) in &self.context {
            self.output.key_value(key, value);
        }
    }

    /// Render as plain text.
    fn render_plain(&self) {
        println!("INFO: {}", self.message);

        for (key, value) in &self.context {
            println!("  {key}: {value}");
        }
    }
}

// =============================================================================
// HintDisplay
// =============================================================================

/// Renders hints, tips, and feature discovery messages.
///
/// In rich mode, displays with a dim/light style and hint icon.
/// In plain mode, outputs "HINT: message".
/// In JSON/agent mode, hints are omitted entirely to avoid noise.
pub struct HintDisplay<'a> {
    output: &'a RichOutput,
    message: String,
    label: Option<String>,
}

impl<'a> HintDisplay<'a> {
    /// Create a new hint display.
    #[must_use]
    pub fn new(output: &'a RichOutput, message: &str) -> Self {
        Self {
            output,
            message: message.to_string(),
            label: None,
        }
    }

    /// Set a custom label (default is "Tip" in rich mode, "HINT" in plain).
    #[must_use]
    pub fn label(mut self, label: &str) -> Self {
        self.label = Some(label.to_string());
        self
    }

    /// Render the hint. Does nothing in JSON/agent mode.
    pub fn render(&self) {
        trace!(mode = ?self.output.mode(), "HintDisplay::render");

        match self.output.mode() {
            OutputMode::Json => {
                // Hints are intentionally omitted in agent mode
            }
            OutputMode::Rich => {
                self.render_rich();
            }
            OutputMode::Plain => {
                self.render_plain();
            }
        }
    }

    /// Render with rich styling.
    fn render_rich(&self) {
        let icon = self.output.theme().icons.get("hint", self.output.use_unicode());
        let label = self.label.as_deref().unwrap_or("Tip");
        let styled_label = self.output.format_styled(label, "dim bold");
        let styled_msg = self.output.format_styled(&self.message, "dim");
        println!("{icon} {styled_label}: {styled_msg}");
    }

    /// Render as plain text.
    fn render_plain(&self) {
        let label = self.label.as_deref().unwrap_or("HINT");
        println!("{label}: {}", self.message);
    }
}

// =============================================================================
// StatusTracker
// =============================================================================

/// Tracks multi-step operation progress with labeled phases.
///
/// In rich mode, displays step-by-step progress with checkmarks and styling.
/// In plain mode, prints numbered steps.
/// In JSON mode, each step emits a structured status line and the final
/// `complete()` emits a summary object.
///
/// # Example
///
/// ```rust,ignore
/// let mut tracker = StatusTracker::new(&output, "Import workflow");
/// tracker.step("Parsing YAML files");
/// // ... do work ...
/// tracker.step("Validating schemas");
/// // ... do work ...
/// tracker.complete("Imported 12 skills successfully");
/// ```
pub struct StatusTracker<'a> {
    output: &'a RichOutput,
    steps: Vec<String>,
    current_step: usize,
}

impl<'a> StatusTracker<'a> {
    /// Create a new status tracker.
    #[must_use]
    pub fn new(output: &'a RichOutput, title: &str) -> Self {
        trace!(mode = ?output.mode(), title = title, "StatusTracker::new");

        let tracker = Self {
            output,
            steps: Vec::new(),
            current_step: 0,
        };

        // Print header on creation
        match output.mode() {
            OutputMode::Rich => {
                let styled = output.format_styled(title, "bold");
                println!("{styled}");
            }
            OutputMode::Plain => {
                println!("{title}");
            }
            OutputMode::Json => {
                // No header in JSON mode
            }
        }

        tracker
    }

    /// Record a completed step.
    pub fn step(&mut self, description: &str) {
        self.current_step += 1;
        self.steps.push(description.to_string());

        trace!(
            step = self.current_step,
            description = description,
            "StatusTracker::step"
        );

        match self.output.mode() {
            OutputMode::Json => {
                #[derive(Serialize)]
                struct StepJson {
                    step: usize,
                    description: String,
                    status: &'static str,
                }
                let json = StepJson {
                    step: self.current_step,
                    description: description.to_string(),
                    status: "done",
                };
                if let Ok(s) = serde_json::to_string(&json) {
                    eprintln!("{s}");
                }
            }
            OutputMode::Rich => {
                let icon = self.output.theme().icons.get("success", self.output.use_unicode());
                let styled_desc = self.output.format_styled(description, "green");
                let num = self.output.format_styled(
                    &format!("[{}]", self.current_step),
                    "dim",
                );
                println!("  {icon} {num} {styled_desc}");
            }
            OutputMode::Plain => {
                println!("  [{}] {}", self.current_step, description);
            }
        }
    }

    /// Mark the operation as complete with a summary message.
    pub fn complete(&self, summary: &str) {
        trace!(
            total_steps = self.current_step,
            summary = summary,
            "StatusTracker::complete"
        );

        match self.output.mode() {
            OutputMode::Json => {
                #[derive(Serialize)]
                struct CompleteJson {
                    status: &'static str,
                    summary: String,
                    total_steps: usize,
                }
                let json = CompleteJson {
                    status: "complete",
                    summary: summary.to_string(),
                    total_steps: self.current_step,
                };
                if let Ok(s) = serde_json::to_string(&json) {
                    println!("{s}");
                }
            }
            OutputMode::Rich => {
                let box_chars = self.output.theme().box_style.chars();
                let width = self.output.width().saturating_sub(4).min(30);
                println!("  {}", box_chars.horizontal.repeat(width));
                let icon = self.output.theme().icons.get("success", self.output.use_unicode());
                let styled = self.output.format_styled(summary, "bold green");
                println!("  {icon} {styled}");
                println!();
            }
            OutputMode::Plain => {
                println!("  ---");
                println!("  Done: {summary}");
            }
        }
    }

    /// Mark the operation as failed with an error message.
    pub fn fail(&self, error: &str) {
        trace!(
            total_steps = self.current_step,
            error = error,
            "StatusTracker::fail"
        );

        match self.output.mode() {
            OutputMode::Json => {
                #[derive(Serialize)]
                struct FailJson {
                    status: &'static str,
                    error: String,
                    completed_steps: usize,
                }
                let json = FailJson {
                    status: "failed",
                    error: error.to_string(),
                    completed_steps: self.current_step,
                };
                if let Ok(s) = serde_json::to_string(&json) {
                    println!("{s}");
                }
            }
            OutputMode::Rich => {
                let icon = self.output.theme().icons.get("error", self.output.use_unicode());
                let styled = self.output.format_styled(error, "bold red");
                eprintln!("  {icon} {styled}");
            }
            OutputMode::Plain => {
                eprintln!("  ERROR: {error}");
            }
        }
    }

    /// Get the number of completed steps.
    #[must_use]
    pub fn step_count(&self) -> usize {
        self.current_step
    }
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

    fn json_output() -> RichOutput {
        RichOutput::json_mode()
    }

    // =========================================================================
    // SuccessRenderer Tests
    // =========================================================================

    #[test]
    fn test_success_renderer_plain() {
        let output = plain_output();
        let renderer = SuccessRenderer::new(&output, "Skill created");
        // Should not panic
        renderer.render();
    }

    #[test]
    fn test_success_renderer_with_steps() {
        let output = plain_output();
        let renderer = SuccessRenderer::new(&output, "Import complete")
            .next_step("Run `ms list` to see imported skills")
            .next_step("Run `ms search` to find them");
        renderer.render();
    }

    #[test]
    fn test_success_renderer_with_detail() {
        let output = plain_output();
        let renderer = SuccessRenderer::new(&output, "Validated")
            .detail("All 5 checks passed");
        renderer.render();
    }

    #[test]
    fn test_success_renderer_json() {
        let output = json_output();
        let renderer = SuccessRenderer::new(&output, "Created skill")
            .next_step("View with ms show")
            .detail("Saved to project layer");
        renderer.render();
    }

    // =========================================================================
    // InfoRenderer Tests
    // =========================================================================

    #[test]
    fn test_info_renderer_plain() {
        let output = plain_output();
        let renderer = InfoRenderer::new(&output, "Indexing 42 skills");
        renderer.render();
    }

    #[test]
    fn test_info_renderer_with_context() {
        let output = plain_output();
        let renderer = InfoRenderer::new(&output, "Search complete")
            .context("Results", "12")
            .context("Time", "45ms");
        renderer.render();
    }

    #[test]
    fn test_info_renderer_json() {
        let output = json_output();
        let renderer = InfoRenderer::new(&output, "Processing")
            .context("items", "10");
        renderer.render();
    }

    // =========================================================================
    // HintDisplay Tests
    // =========================================================================

    #[test]
    fn test_hint_display_plain() {
        let output = plain_output();
        let hint = HintDisplay::new(&output, "Use --explain for score breakdowns");
        hint.render();
    }

    #[test]
    fn test_hint_display_custom_label() {
        let output = plain_output();
        let hint = HintDisplay::new(&output, "Ctrl+C to cancel")
            .label("Shortcut");
        hint.render();
    }

    #[test]
    fn test_hint_display_omitted_in_json() {
        let output = json_output();
        // Should not produce any output in JSON mode
        let hint = HintDisplay::new(&output, "This should not appear");
        hint.render();
    }

    // =========================================================================
    // StatusTracker Tests
    // =========================================================================

    #[test]
    fn test_status_tracker_plain() {
        let output = plain_output();
        let mut tracker = StatusTracker::new(&output, "Import");
        tracker.step("Parsing files");
        tracker.step("Validating schemas");
        tracker.step("Writing to database");
        tracker.complete("Imported 3 skills");
        assert_eq!(tracker.step_count(), 3);
    }

    #[test]
    fn test_status_tracker_fail() {
        let output = plain_output();
        let mut tracker = StatusTracker::new(&output, "Sync");
        tracker.step("Fetching remote");
        tracker.fail("Connection refused");
        assert_eq!(tracker.step_count(), 1);
    }

    #[test]
    fn test_status_tracker_json() {
        let output = json_output();
        let mut tracker = StatusTracker::new(&output, "Build");
        tracker.step("Compiling");
        tracker.step("Linking");
        tracker.complete("Build successful");
        assert_eq!(tracker.step_count(), 2);
    }

    #[test]
    fn test_status_tracker_empty_complete() {
        let output = plain_output();
        let tracker = StatusTracker::new(&output, "Quick op");
        tracker.complete("Done instantly");
        assert_eq!(tracker.step_count(), 0);
    }
}
