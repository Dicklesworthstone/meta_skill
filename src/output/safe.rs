//! Safe wrapper for `RichOutput` with graceful degradation.
//!
//! This module provides `SafeRichOutput`, a wrapper around `RichOutput` that
//! ensures the application never crashes due to rich output failures. Rich
//! output is treated as a "nice-to-have" feature - if it fails, we degrade
//! gracefully to simpler output modes.
//!
//! # Fallback Chain
//!
//! When rendering fails, the system automatically falls back through:
//!
//! ```text
//! Rich Output (full styling)
//!     |
//!     v [on error]
//! Reduced Rich Output (basic styling, no complex features)
//!     |
//!     v [on error]
//! Plain Colored Output (ANSI colors only)
//!     |
//!     v [on error]
//! Plain Text Output (no styling at all)
//!     |
//!     v [should never fail]
//! Raw println!() / eprintln!()
//! ```
//!
//! # Panic Prevention
//!
//! All rendering operations are wrapped in panic handlers. If rich output
//! panics for any reason, we catch it, log the issue, and fall back to
//! plain output. The application continues normally.
//!
//! # Example
//!
//! ```rust,ignore
//! use ms::output::{SafeRichOutput, RichOutput};
//!
//! // Create with automatic fallback detection
//! let output = SafeRichOutput::new(&config, &format);
//!
//! // All methods are safe - they never panic
//! output.success("Operation completed");
//! output.print_table_safe(&table);
//! output.print_markdown_safe(markdown_content);
//! ```

use std::fmt;
use std::io::{self, Write};
use std::panic::{self, AssertUnwindSafe};

use rich_rust::renderables::{Table, Tree};
use tracing::{debug, error, trace, warn};

use crate::cli::output::OutputFormat;
use crate::config::Config;

use super::fallback::{FallbackLevel, FallbackRenderer};
use super::rich_output::{OutputMode, RichOutput};
use super::theme::Theme;

// =============================================================================
// RichOutputError
// =============================================================================

/// Error types for rich output operations.
#[derive(Debug)]
pub struct RichOutputError {
    /// The kind of error that occurred.
    pub kind: RichOutputErrorKind,
    /// Human-readable error message.
    pub message: String,
    /// The underlying error, if any.
    pub source: Option<Box<dyn std::error::Error + Send + Sync>>,
    /// Whether a fallback was used to recover.
    pub fallback_used: bool,
}

impl RichOutputError {
    /// Create a new rich output error.
    #[must_use]
    pub fn new(kind: RichOutputErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            source: None,
            fallback_used: false,
        }
    }

    /// Add a source error.
    #[must_use]
    pub fn with_source(mut self, source: impl std::error::Error + Send + Sync + 'static) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    /// Mark that a fallback was used.
    #[must_use]
    pub fn with_fallback(mut self) -> Self {
        self.fallback_used = true;
        self
    }
}

impl fmt::Display for RichOutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)?;
        if self.fallback_used {
            write!(f, " (fallback used)")?;
        }
        Ok(())
    }
}

impl std::error::Error for RichOutputError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|e| e.as_ref() as _)
    }
}

/// Categories of rich output errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RichOutputErrorKind {
    /// Failed to initialize rich output subsystem.
    Initialization,
    /// Error during content rendering.
    Rendering,
    /// Terminal-related error (size, capabilities, etc.).
    Terminal,
    /// Text encoding issue.
    Encoding,
    /// Caught panic from underlying library.
    Panic,
}

// =============================================================================
// SafeRichOutput
// =============================================================================

/// A safe wrapper around `RichOutput` with graceful degradation.
///
/// This wrapper ensures that rich output failures never crash the application.
/// All rendering methods have `_safe` variants that catch errors and fall back
/// to simpler output modes.
///
/// # Thread Safety
///
/// `SafeRichOutput` is `Send + Sync` and can be safely shared across threads.
///
/// # Construction
///
/// ```rust,ignore
/// // Auto-detect with fallback
/// let output = SafeRichOutput::new(&config, &format);
///
/// // Force plain mode (safest)
/// let plain = SafeRichOutput::plain();
///
/// // Wrap existing RichOutput
/// let output = SafeRichOutput::wrap(rich_output);
/// ```
#[derive(Clone)]
pub struct SafeRichOutput {
    inner: RichOutput,
    fallback_level: FallbackLevel,
    fallback: FallbackRenderer,
}

impl SafeRichOutput {
    // =========================================================================
    // Construction
    // =========================================================================

    /// Create a new `SafeRichOutput` with automatic fallback detection.
    ///
    /// Attempts to create a full-featured `RichOutput`, falling back to
    /// progressively simpler modes if initialization fails.
    #[must_use]
    pub fn new(config: &Config, format: &OutputFormat, robot_mode: bool) -> Self {
        Self::with_flags(config, format, robot_mode, false, false)
    }

    /// Create a new `SafeRichOutput` with explicit CLI flags.
    #[must_use]
    pub fn with_flags(
        config: &Config,
        format: &OutputFormat,
        robot_mode: bool,
        force_plain: bool,
        force_rich: bool,
    ) -> Self {
        let (inner, level) =
            Self::try_create_with_fallback(config, format, robot_mode, force_plain, force_rich);

        trace!(
            fallback_level = ?level,
            mode = ?inner.mode(),
            "SafeRichOutput created"
        );

        let fallback = FallbackRenderer::new(level);

        Self {
            inner,
            fallback_level: level,
            fallback,
        }
    }

    /// Create a `SafeRichOutput` that always uses plain mode.
    ///
    /// This is the safest option and should be used for MCP servers,
    /// automated tests, or any context where rich output could cause issues.
    #[must_use]
    pub fn plain() -> Self {
        trace!("Creating plain SafeRichOutput");
        let inner = RichOutput::plain();
        let fallback = FallbackRenderer::new(FallbackLevel::Plain);

        Self {
            inner,
            fallback_level: FallbackLevel::Plain,
            fallback,
        }
    }

    /// Wrap an existing `RichOutput` with safe degradation.
    #[must_use]
    pub fn wrap(inner: RichOutput) -> Self {
        let level = match inner.mode() {
            OutputMode::Rich => FallbackLevel::Full,
            OutputMode::Plain | OutputMode::Json => FallbackLevel::Plain,
        };
        let fallback = FallbackRenderer::new(level);

        Self {
            inner,
            fallback_level: level,
            fallback,
        }
    }

    /// Attempt to create `RichOutput` with progressive fallback.
    fn try_create_with_fallback(
        config: &Config,
        format: &OutputFormat,
        robot_mode: bool,
        force_plain: bool,
        force_rich: bool,
    ) -> (RichOutput, FallbackLevel) {
        // If forced plain, don't even try rich
        if force_plain || robot_mode || matches!(format, OutputFormat::Json | OutputFormat::Jsonl) {
            debug!("Using plain mode due to format/flags");
            return (RichOutput::plain(), FallbackLevel::Plain);
        }

        // Try full rich output
        let rich_result = panic::catch_unwind(AssertUnwindSafe(|| {
            RichOutput::with_flags(config, format, robot_mode, force_plain, force_rich)
        }));

        match rich_result {
            Ok(rich) if rich.is_rich() => {
                trace!("Full rich output available");
                (rich, FallbackLevel::Full)
            }
            Ok(rich) => {
                // RichOutput decided to use plain mode
                debug!("RichOutput chose plain mode");
                (rich, FallbackLevel::Plain)
            }
            Err(panic) => {
                // Initialization panicked - fall back
                let panic_msg = panic_to_string(&panic);
                warn!(
                    error = %panic_msg,
                    "Rich output initialization panicked, using plain fallback"
                );
                (RichOutput::plain(), FallbackLevel::Plain)
            }
        }
    }

    // =========================================================================
    // Query Methods
    // =========================================================================

    /// Get the current fallback level.
    #[must_use]
    pub const fn fallback_level(&self) -> FallbackLevel {
        self.fallback_level
    }

    /// Check if full rich output is available.
    #[must_use]
    pub const fn is_full_rich(&self) -> bool {
        matches!(self.fallback_level, FallbackLevel::Full)
    }

    /// Check if we're in reduced mode.
    #[must_use]
    pub const fn is_reduced(&self) -> bool {
        matches!(self.fallback_level, FallbackLevel::Reduced)
    }

    /// Check if we're in plain mode.
    #[must_use]
    pub const fn is_plain(&self) -> bool {
        matches!(self.fallback_level, FallbackLevel::Plain)
    }

    /// Get the current output mode.
    #[must_use]
    pub const fn mode(&self) -> OutputMode {
        self.inner.mode()
    }

    /// Get the terminal width.
    #[must_use]
    pub const fn width(&self) -> usize {
        self.inner.width()
    }

    /// Get the theme.
    #[must_use]
    pub const fn theme(&self) -> &Theme {
        self.inner.theme()
    }

    /// Get the inner `RichOutput` for direct access.
    ///
    /// Use with caution - direct calls bypass safety wrappers.
    #[must_use]
    pub const fn inner(&self) -> &RichOutput {
        &self.inner
    }

    // =========================================================================
    // Safe Rendering Helpers
    // =========================================================================

    /// Execute a rendering closure with panic protection.
    ///
    /// If the closure panics, logs the error and returns `None`.
    fn render_safe<F, T>(&self, component: &str, f: F) -> Option<T>
    where
        F: FnOnce() -> T + panic::UnwindSafe,
    {
        match panic::catch_unwind(f) {
            Ok(result) => Some(result),
            Err(panic) => {
                let panic_msg = panic_to_string(&panic);
                error!(
                    component = %component,
                    panic = %panic_msg,
                    "Rich output panicked, using fallback"
                );
                None
            }
        }
    }

    /// Execute a rendering closure with panic protection, calling fallback on failure.
    fn render_or_fallback<F, G>(&self, component: &str, f: F, fallback: G)
    where
        F: FnOnce() + panic::UnwindSafe,
        G: FnOnce(),
    {
        if self.render_safe(component, f).is_none() {
            debug!(
                component = %component,
                fallback_level = ?self.fallback_level,
                "Using fallback renderer"
            );
            fallback();
        }
    }

    // =========================================================================
    // Safe Basic Output
    // =========================================================================

    /// Print text with a newline (always safe).
    pub fn println(&self, text: &str) {
        println!("{text}");
    }

    /// Print text without a newline (always safe).
    pub fn print(&self, text: &str) {
        print!("{text}");
        let _ = io::stdout().flush();
    }

    /// Print to stderr (always safe).
    pub fn eprintln(&self, text: &str) {
        eprintln!("{text}");
    }

    // =========================================================================
    // Safe Semantic Output
    // =========================================================================

    /// Print a success message with fallback.
    pub fn success(&self, message: &str) {
        let inner = &self.inner;
        let fallback = &self.fallback;
        let msg = message.to_string();

        self.render_or_fallback(
            "success",
            move || inner.success(&msg),
            || fallback.success(message),
        );
    }

    /// Print an error message with fallback.
    pub fn error(&self, message: &str) {
        let inner = &self.inner;
        let fallback = &self.fallback;
        let msg = message.to_string();

        self.render_or_fallback(
            "error",
            move || inner.error(&msg),
            || fallback.error(message),
        );
    }

    /// Print a warning message with fallback.
    pub fn warning(&self, message: &str) {
        let inner = &self.inner;
        let fallback = &self.fallback;
        let msg = message.to_string();

        self.render_or_fallback(
            "warning",
            move || inner.warning(&msg),
            || fallback.warning(message),
        );
    }

    /// Print an info message with fallback.
    pub fn info(&self, message: &str) {
        let inner = &self.inner;
        let fallback = &self.fallback;
        let msg = message.to_string();

        self.render_or_fallback(
            "info",
            move || inner.info(&msg),
            || fallback.info(message),
        );
    }

    /// Print a hint message with fallback.
    pub fn hint(&self, message: &str) {
        let inner = &self.inner;
        let fallback = &self.fallback;
        let msg = message.to_string();

        self.render_or_fallback(
            "hint",
            move || inner.hint(&msg),
            || fallback.hint(message),
        );
    }

    // =========================================================================
    // Safe Structural Output
    // =========================================================================

    /// Print a header with fallback.
    pub fn header(&self, text: &str) {
        let inner = &self.inner;
        let fallback = &self.fallback;
        let txt = text.to_string();

        self.render_or_fallback(
            "header",
            move || inner.header(&txt),
            || fallback.header(text),
        );
    }

    /// Print a subheader with fallback.
    pub fn subheader(&self, text: &str) {
        let inner = &self.inner;
        let fallback = &self.fallback;
        let txt = text.to_string();

        self.render_or_fallback(
            "subheader",
            move || inner.subheader(&txt),
            || fallback.subheader(text),
        );
    }

    /// Print a horizontal rule with fallback.
    pub fn rule(&self, title: Option<&str>) {
        let inner = &self.inner;
        let fallback = &self.fallback;
        let t = title.map(String::from);

        self.render_or_fallback(
            "rule",
            move || inner.rule(t.as_deref()),
            || fallback.rule(title),
        );
    }

    /// Print a blank line.
    pub fn newline(&self) {
        println!();
    }

    // =========================================================================
    // Safe Data Display
    // =========================================================================

    /// Print a key-value pair with fallback.
    pub fn key_value(&self, key: &str, value: &str) {
        let inner = &self.inner;
        let fallback = &self.fallback;
        let k = key.to_string();
        let v = value.to_string();

        self.render_or_fallback(
            "key_value",
            move || inner.key_value(&k, &v),
            || fallback.key_value(key, value),
        );
    }

    /// Print a list of key-value pairs with fallback.
    pub fn key_value_list(&self, pairs: &[(&str, &str)]) {
        // Clone pairs for the closure
        let pairs_owned: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        let inner = &self.inner;
        let fallback = &self.fallback;
        let pairs_ref: Vec<(&str, &str)> = pairs_owned
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();

        self.render_or_fallback(
            "key_value_list",
            move || inner.key_value_list(&pairs_ref),
            || fallback.key_value_list(pairs),
        );
    }

    /// Print a bulleted list with fallback.
    pub fn list(&self, items: &[&str]) {
        let items_owned: Vec<String> = items.iter().map(|s| s.to_string()).collect();

        let inner = &self.inner;
        let fallback = &self.fallback;
        let items_ref: Vec<&str> = items_owned.iter().map(String::as_str).collect();

        self.render_or_fallback(
            "list",
            move || inner.list(&items_ref),
            || fallback.list(items),
        );
    }

    /// Print a numbered list with fallback.
    pub fn numbered_list(&self, items: &[&str]) {
        let items_owned: Vec<String> = items.iter().map(|s| s.to_string()).collect();

        let inner = &self.inner;
        let fallback = &self.fallback;
        let items_ref: Vec<&str> = items_owned.iter().map(String::as_str).collect();

        self.render_or_fallback(
            "numbered_list",
            move || inner.numbered_list(&items_ref),
            || fallback.numbered_list(items),
        );
    }

    // =========================================================================
    // Safe Renderable Output
    // =========================================================================

    /// Print a table with fallback to plain text.
    pub fn print_table_safe(&self, table: &Table) {
        let width = self.inner.width();

        if self.fallback_level == FallbackLevel::Full {
            // Try rich rendering
            let inner = &self.inner;
            if self
                .render_safe("table", || {
                    inner.print_table(table);
                })
                .is_some()
            {
                return;
            }
        }

        // Fallback: render as plain text
        debug!("Using plain table fallback");
        println!("{}", table.render_plain(width));
    }

    /// Print a panel with fallback to simple bordered text.
    pub fn print_panel_safe(&self, content: &str, title: Option<&str>) {
        if self.fallback_level == FallbackLevel::Full {
            let inner = &self.inner;
            let c = content.to_string();
            let t = title.map(String::from);

            if self
                .render_safe("panel", move || {
                    inner.print_panel(&c, t.as_deref());
                })
                .is_some()
            {
                return;
            }
        }

        // Fallback: simple text panel
        self.fallback.panel(content, title);
    }

    /// Print a tree with fallback to indented text.
    pub fn print_tree_safe(&self, tree: &Tree) {
        if self.fallback_level == FallbackLevel::Full {
            let inner = &self.inner;
            if self
                .render_safe("tree", || {
                    inner.print_tree(tree);
                })
                .is_some()
            {
                return;
            }
        }

        // Fallback: render as plain indented text
        debug!("Using plain tree fallback");
        println!("{}", tree.render_plain());
    }

    /// Print markdown with fallback to raw text.
    pub fn print_markdown_safe(&self, md: &str) {
        if self.fallback_level == FallbackLevel::Full {
            let inner = &self.inner;
            let content = md.to_string();

            if self
                .render_safe("markdown", move || {
                    inner.print_markdown(&content);
                })
                .is_some()
            {
                return;
            }
        }

        // Fallback: print raw markdown
        warn!("Markdown rendering failed, printing raw");
        println!("{md}");
    }

    /// Print syntax-highlighted code with fallback to plain code.
    pub fn print_syntax_safe(&self, code: &str, language: &str) {
        if self.fallback_level == FallbackLevel::Full {
            let inner = &self.inner;
            let c = code.to_string();
            let l = language.to_string();

            if self
                .render_safe("syntax", move || {
                    inner.print_syntax(&c, &l);
                })
                .is_some()
            {
                return;
            }
        }

        // Fallback: print as code block
        self.fallback.code(code, language);
    }

    // =========================================================================
    // Progress (always safe, writes to stderr)
    // =========================================================================

    /// Print a progress indicator.
    pub fn progress(&self, current: u64, total: u64, message: &str) {
        let inner = &self.inner;
        let fallback = &self.fallback;
        let msg = message.to_string();

        self.render_or_fallback(
            "progress",
            move || inner.progress(current, total, &msg),
            || fallback.progress(current, total, message),
        );
    }

    /// Print a status line.
    pub fn status_line(&self, status: &str, message: &str) {
        let inner = &self.inner;
        let fallback = &self.fallback;
        let s = status.to_string();
        let m = message.to_string();

        self.render_or_fallback(
            "status_line",
            move || inner.status_line(&s, &m),
            || fallback.status_line(status, message),
        );
    }

    /// Clear the current status line.
    pub fn clear_status(&self) {
        let inner = &self.inner;
        self.render_or_fallback("clear_status", || inner.clear_status(), || {
            eprint!("\r{:80}\r", "");
            let _ = io::stderr().flush();
        });
    }
}

impl Default for SafeRichOutput {
    fn default() -> Self {
        Self::plain()
    }
}

impl fmt::Debug for SafeRichOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SafeRichOutput")
            .field("fallback_level", &self.fallback_level)
            .field("mode", &self.inner.mode())
            .field("width", &self.inner.width())
            .finish()
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Convert a panic payload to a string for logging.
fn panic_to_string(panic: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = panic.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = panic.downcast_ref::<String>() {
        s.clone()
    } else {
        "Unknown panic".to_string()
    }
}

/// Get terminal width with safe default.
#[must_use]
pub fn get_width_safe() -> usize {
    crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(80)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_output_plain_creation() {
        let output = SafeRichOutput::plain();
        assert!(output.is_plain());
        assert_eq!(output.fallback_level(), FallbackLevel::Plain);
    }

    #[test]
    fn test_safe_output_default_is_plain() {
        let output = SafeRichOutput::default();
        assert!(output.is_plain());
    }

    #[test]
    fn test_safe_output_wrap() {
        let inner = RichOutput::plain();
        let output = SafeRichOutput::wrap(inner);
        assert!(output.is_plain());
    }

    #[test]
    fn test_error_types() {
        let err = RichOutputError::new(RichOutputErrorKind::Rendering, "Test error");
        assert_eq!(err.kind, RichOutputErrorKind::Rendering);
        assert!(!err.fallback_used);

        let err = err.with_fallback();
        assert!(err.fallback_used);
    }

    #[test]
    fn test_error_display() {
        let err = RichOutputError::new(RichOutputErrorKind::Panic, "Caught panic");
        let display = format!("{err}");
        assert!(display.contains("Panic"));
        assert!(display.contains("Caught panic"));
    }

    #[test]
    fn test_error_with_fallback_display() {
        let err = RichOutputError::new(RichOutputErrorKind::Terminal, "Size error").with_fallback();
        let display = format!("{err}");
        assert!(display.contains("fallback used"));
    }

    #[test]
    fn test_get_width_safe_returns_reasonable_default() {
        let width = get_width_safe();
        assert!(width >= 40); // Should be at least 40
        assert!(width <= 500); // Should be reasonable
    }

    #[test]
    fn test_panic_to_string_with_str() {
        let payload: Box<dyn std::any::Any + Send> = Box::new("test panic");
        let msg = panic_to_string(&payload);
        assert_eq!(msg, "test panic");
    }

    #[test]
    fn test_panic_to_string_with_string() {
        let payload: Box<dyn std::any::Any + Send> = Box::new(String::from("test panic string"));
        let msg = panic_to_string(&payload);
        assert_eq!(msg, "test panic string");
    }

    #[test]
    fn test_panic_to_string_with_other() {
        let payload: Box<dyn std::any::Any + Send> = Box::new(42i32);
        let msg = panic_to_string(&payload);
        assert_eq!(msg, "Unknown panic");
    }

    #[test]
    fn test_debug_impl() {
        let output = SafeRichOutput::plain();
        let debug = format!("{output:?}");
        assert!(debug.contains("SafeRichOutput"));
        assert!(debug.contains("Plain"));
    }

    #[test]
    fn test_basic_output_methods_dont_panic() {
        let output = SafeRichOutput::plain();

        // These should never panic
        output.println("test");
        output.print("test");
        output.eprintln("test");
        output.newline();
    }

    #[test]
    fn test_semantic_output_methods_dont_panic() {
        let output = SafeRichOutput::plain();

        // These should never panic even with weird input
        output.success("");
        output.error("");
        output.warning("");
        output.info("");
        output.hint("");
    }

    #[test]
    fn test_structural_output_methods_dont_panic() {
        let output = SafeRichOutput::plain();

        output.header("Test Header");
        output.subheader("Test Subheader");
        output.rule(Some("Title"));
        output.rule(None);
    }

    #[test]
    fn test_data_display_methods_dont_panic() {
        let output = SafeRichOutput::plain();

        output.key_value("key", "value");
        output.key_value_list(&[("k1", "v1"), ("k2", "v2")]);
        output.list(&["item1", "item2"]);
        output.numbered_list(&["first", "second"]);
    }

    #[test]
    fn test_markdown_safe_with_malformed_input() {
        let output = SafeRichOutput::plain();

        // Malformed markdown should still work
        output.print_markdown_safe("# Heading\n\n```broken");
        output.print_markdown_safe("");
        output.print_markdown_safe("[[[[[");
    }

    #[test]
    fn test_syntax_safe_with_unknown_language() {
        let output = SafeRichOutput::plain();

        // Unknown language should fall back gracefully
        output.print_syntax_safe("fn main() {}", "unknown_lang_xyz");
        output.print_syntax_safe("", "");
    }

    #[test]
    fn test_progress_safe() {
        let output = SafeRichOutput::plain();

        output.progress(50, 100, "Processing...");
        output.progress(0, 0, "Edge case");
        output.progress(100, 50, "Over 100%");
    }
}
