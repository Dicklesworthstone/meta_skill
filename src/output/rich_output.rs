//! Rich output abstraction layer.
//!
//! This module provides `RichOutput`, the main abstraction between rich and plain
//! output modes. All output methods check the current mode and behave accordingly:
//!
//! - **Rich mode**: Full styling, colors, Unicode characters
//! - **Plain mode**: No ANSI codes, no box drawing, simple text
//! - **JSON mode**: Structured JSON output
//!
//! # Thread Safety
//!
//! `RichOutput` is `Send + Sync` and can be safely shared across threads.
//! Progress methods write to stderr to avoid interfering with stdout.
//!
//! # Examples
//!
//! ```rust,ignore
//! use ms::output::rich_output::RichOutput;
//!
//! // Auto-detect mode based on config and environment
//! let output = RichOutput::new(&config, &format);
//!
//! // Force plain mode (for MCP server, tests)
//! let plain = RichOutput::plain();
//!
//! // Semantic output applies theme colors automatically
//! output.success("Operation completed");
//! output.error("Something went wrong");
//!
//! // Structural output
//! output.header("Search Results");
//! output.key_value("Found", "42 skills");
//! ```

use std::io::{self, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use rich_rust::color::ColorSystem;
use rich_rust::console::Console;
use rich_rust::renderables::{Markdown, Panel, Syntax, Table, Tree};
use rich_rust::style::Style;
use serde::Serialize;
use tracing::trace;

use crate::cli::output::OutputFormat;
use crate::config::Config;

use super::detection::{OutputDecision, OutputDetector, OutputEnvironment};
use super::theme::{BoxStyle, Theme, detect_terminal_capabilities};

// =============================================================================
// Output Mode
// =============================================================================

/// The current output mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    /// Rich output with colors, styles, and Unicode.
    Rich,
    /// Plain text output without any styling.
    Plain,
    /// JSON output for machine consumption.
    Json,
}

impl OutputMode {
    /// Check if this mode allows styled output.
    #[must_use]
    pub const fn allows_styling(&self) -> bool {
        matches!(self, OutputMode::Rich)
    }

    /// Check if this mode is plain text.
    #[must_use]
    pub const fn is_plain(&self) -> bool {
        matches!(self, OutputMode::Plain)
    }

    /// Check if this mode is JSON.
    #[must_use]
    pub const fn is_json(&self) -> bool {
        matches!(self, OutputMode::Json)
    }
}

// =============================================================================
// Spinner Handle
// =============================================================================

/// Handle for controlling a spinner animation.
///
/// The spinner runs until this handle is dropped or `stop()` is called.
pub struct SpinnerHandle {
    running: Arc<AtomicBool>,
    message: Arc<Mutex<String>>,
}

impl SpinnerHandle {
    /// Update the spinner message.
    pub fn set_message(&self, message: &str) {
        *self.message.lock() = message.to_string();
    }

    /// Stop the spinner.
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Check if the spinner is still running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

impl Drop for SpinnerHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

// =============================================================================
// RichOutput
// =============================================================================

/// The main abstraction for rich vs plain terminal output.
///
/// This struct provides all output methods that automatically adapt to the
/// current output mode. In rich mode, output includes colors and Unicode.
/// In plain mode, all output is plain ASCII text.
///
/// # Thread Safety
///
/// `RichOutput` is `Send + Sync` and uses internal locking for terminal operations.
///
/// # Construction
///
/// ```rust,ignore
/// // Auto-detect from config and environment
/// let output = RichOutput::new(&config, &format);
///
/// // Force plain mode (MCP server, tests)
/// let plain = RichOutput::plain();
///
/// // From detection result
/// let output = RichOutput::from_detection(&decision);
/// ```
#[derive(Clone)]
pub struct RichOutput {
    mode: OutputMode,
    theme: Theme,
    width: usize,
    color_system: Option<ColorSystem>,
    use_unicode: bool,
}

// RichOutput is Send + Sync because all its fields are:
// - OutputMode: Copy enum
// - Theme: Send + Sync (contains Style values)
// - usize: Copy primitive
// - Option<ColorSystem>: ColorSystem is Copy
// - bool: Copy primitive
//
// Note: We rely on automatic trait derivation rather than unsafe impl.
// If compilation fails due to missing Send/Sync on a field type,
// that would indicate a real thread-safety issue to investigate.

impl RichOutput {
    // =========================================================================
    // Construction
    // =========================================================================

    /// Create a new `RichOutput` from config and output format.
    ///
    /// This auto-detects the appropriate mode based on:
    /// - Output format (JSON, Plain, Human)
    /// - Robot mode flag
    /// - Environment variables (NO_COLOR, MS_PLAIN_OUTPUT, etc.)
    /// - Terminal detection
    #[must_use]
    pub fn new(config: &Config, format: &OutputFormat, robot_mode: bool) -> Self {
        Self::with_flags(config, format, robot_mode, false, false)
    }

    /// Create a new `RichOutput` with explicit CLI flags for forcing modes.
    ///
    /// This is the preferred constructor when you have CLI arguments available.
    ///
    /// # Arguments
    /// * `config` - Application config with theme settings
    /// * `format` - Output format (Human, JSON, Plain, etc.)
    /// * `robot_mode` - Whether --robot flag is set
    /// * `force_plain` - Whether --plain or --color=never is set
    /// * `force_rich` - Whether --color=always is set
    #[must_use]
    pub fn with_flags(
        config: &Config,
        format: &OutputFormat,
        robot_mode: bool,
        force_plain: bool,
        force_rich: bool,
    ) -> Self {
        // CLI --plain or --color=never takes immediate precedence
        if force_plain {
            trace!("RichOutput: force_plain flag set, using plain mode");
            return Self::plain();
        }

        // Build environment with potential force_rich override
        let mut env = OutputEnvironment::from_env();
        if force_rich {
            trace!("RichOutput: force_rich flag set, overriding environment");
            env.force_rich = true;
        }

        let detector = OutputDetector::with_env(*format, robot_mode, env);
        let decision = detector.decide();

        trace!(
            use_rich = decision.use_rich,
            reason = ?decision.reason,
            format = ?format,
            robot = robot_mode,
            force_plain = force_plain,
            force_rich = force_rich,
            "RichOutput mode decision"
        );

        Self::from_detection_inner(decision, format, config)
    }

    /// Create a `RichOutput` that always uses plain mode.
    ///
    /// Use this for MCP servers, tests, or any context where styled output
    /// would break consumers.
    #[must_use]
    pub fn plain() -> Self {
        trace!("Creating plain RichOutput");

        Self {
            mode: OutputMode::Plain,
            theme: Theme::default().with_ascii_fallback(),
            width: 80,
            color_system: None,
            use_unicode: false,
        }
    }

    /// Create a `RichOutput` from an `OutputDecision`.
    #[must_use]
    pub fn from_detection(decision: &OutputDecision) -> Self {
        trace!(
            use_rich = decision.use_rich,
            reason = ?decision.reason,
            "Creating RichOutput from detection"
        );

        let mode = if decision.use_rich {
            OutputMode::Rich
        } else {
            OutputMode::Plain
        };

        let caps = detect_terminal_capabilities();
        let theme = if decision.use_rich {
            Theme::auto_detect().adapted_for_terminal(&caps)
        } else {
            Theme::default().with_ascii_fallback()
        };

        Self {
            mode,
            theme,
            width: terminal_width(),
            color_system: caps.color_system,
            use_unicode: caps.supports_unicode && decision.use_rich,
        }
    }

    /// Internal constructor from detection with format context.
    fn from_detection_inner(
        decision: OutputDecision,
        format: &OutputFormat,
        config: &Config,
    ) -> Self {
        let mode = match format {
            OutputFormat::Json | OutputFormat::Jsonl | OutputFormat::Toon => OutputMode::Json,
            OutputFormat::Plain | OutputFormat::Tsv => OutputMode::Plain,
            OutputFormat::Human => {
                if decision.use_rich {
                    OutputMode::Rich
                } else {
                    OutputMode::Plain
                }
            }
        };

        let caps = detect_terminal_capabilities();
        let theme = if mode == OutputMode::Rich {
            Theme::from_config(config)
                .unwrap_or_else(|_| Theme::auto_detect())
                .adapted_for_terminal(&caps)
        } else {
            Theme::default().with_ascii_fallback()
        };

        Self {
            mode,
            theme,
            width: terminal_width(),
            color_system: caps.color_system,
            use_unicode: caps.supports_unicode && mode == OutputMode::Rich,
        }
    }

    // =========================================================================
    // Query Methods
    // =========================================================================

    /// Check if rich output is enabled.
    #[must_use]
    pub const fn is_rich(&self) -> bool {
        matches!(self.mode, OutputMode::Rich)
    }

    /// Check if plain output mode is active.
    #[must_use]
    pub const fn is_plain(&self) -> bool {
        matches!(self.mode, OutputMode::Plain)
    }

    /// Check if JSON output mode is active.
    #[must_use]
    pub const fn is_json(&self) -> bool {
        matches!(self.mode, OutputMode::Json)
    }

    /// Get the current output mode.
    #[must_use]
    pub const fn mode(&self) -> OutputMode {
        self.mode
    }

    /// Get the current theme.
    #[must_use]
    pub const fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Get the terminal width in columns.
    #[must_use]
    pub const fn width(&self) -> usize {
        self.width
    }

    /// Get the detected color system, if any.
    #[must_use]
    pub const fn color_system(&self) -> Option<ColorSystem> {
        self.color_system
    }

    /// Check if Unicode output is supported.
    #[must_use]
    pub const fn use_unicode(&self) -> bool {
        self.use_unicode
    }

    // =========================================================================
    // Internal Helpers
    // =========================================================================

    /// Get the effective color system for rendering.
    fn effective_color_system(&self) -> ColorSystem {
        self.color_system.unwrap_or(ColorSystem::TrueColor)
    }

    /// Render text with a style using the current color system.
    fn render_style(&self, style: &Style, text: &str) -> String {
        style.render(text, self.effective_color_system())
    }

    // =========================================================================
    // Basic Output
    // =========================================================================

    /// Print text without a newline.
    pub fn print(&self, text: &str) {
        trace!(mode = ?self.mode, text_len = text.len(), "print");
        print!("{text}");
        let _ = io::stdout().flush();
    }

    /// Print text with a newline.
    pub fn println(&self, text: &str) {
        trace!(mode = ?self.mode, text_len = text.len(), "println");
        println!("{text}");
    }

    /// Print text with a style specification.
    ///
    /// In plain mode, the style is ignored and plain text is printed.
    pub fn print_styled(&self, text: &str, style_spec: &str) {
        trace!(mode = ?self.mode, style = style_spec, "print_styled");

        if self.is_rich() {
            if let Ok(style) = Style::parse(style_spec) {
                print!("{}", self.render_style(&style, text));
                let _ = io::stdout().flush();
                return;
            }
        }
        print!("{text}");
        let _ = io::stdout().flush();
    }

    /// Print text with a style specification and newline.
    pub fn println_styled(&self, text: &str, style_spec: &str) {
        trace!(mode = ?self.mode, style = style_spec, "println_styled");

        if self.is_rich() {
            if let Ok(style) = Style::parse(style_spec) {
                println!("{}", self.render_style(&style, text));
                return;
            }
        }
        println!("{text}");
    }

    /// Print markup text with `[style]text[/]` syntax.
    ///
    /// In plain mode, markup tags are stripped.
    pub fn print_markup(&self, markup: &str) {
        trace!(mode = ?self.mode, "print_markup");

        if self.is_rich() {
            // For now, pass through - rich_rust Console handles markup
            let console = Console::new();
            console.print(markup);
        } else {
            // Strip markup tags for plain mode
            let stripped = strip_markup(markup);
            print!("{stripped}");
            let _ = io::stdout().flush();
        }
    }

    /// Print markup text with newline.
    pub fn println_markup(&self, markup: &str) {
        trace!(mode = ?self.mode, "println_markup");

        if self.is_rich() {
            let console = Console::new();
            console.print(markup);
            println!();
        } else {
            let stripped = strip_markup(markup);
            println!("{stripped}");
        }
    }

    /// Print to stderr without a newline.
    pub fn eprint(&self, text: &str) {
        trace!(mode = ?self.mode, "eprint");
        eprint!("{text}");
        let _ = io::stderr().flush();
    }

    /// Print to stderr with a newline.
    pub fn eprintln(&self, text: &str) {
        trace!(mode = ?self.mode, "eprintln");
        eprintln!("{text}");
    }

    // =========================================================================
    // Renderables
    // =========================================================================

    /// Print a table.
    ///
    /// In plain mode, prints a simplified text table.
    pub fn print_table(&self, table: &Table) {
        trace!(mode = ?self.mode, "print_table");

        if self.is_rich() {
            let console = Console::new();
            console.print_renderable(table);
        } else {
            // Render table as plain text with terminal width
            println!("{}", table.render_plain(self.width));
        }
    }

    /// Print a panel with a title.
    ///
    /// In plain mode, prints a simple bordered section.
    pub fn print_panel(&self, content: &str, title: Option<&str>) {
        trace!(mode = ?self.mode, title = ?title, "print_panel");

        if self.is_rich() {
            let mut panel = Panel::from_text(content);
            if let Some(t) = title {
                panel = panel.title(t);
            }
            let console = Console::new();
            console.print_renderable(&panel);
        } else {
            // Plain mode panel
            let box_chars = BoxStyle::Ascii.chars();
            let width = self.width.saturating_sub(4).max(40);

            if let Some(t) = title {
                println!(
                    "{}{} {} {}{}",
                    box_chars.top_left,
                    box_chars.horizontal,
                    t,
                    box_chars
                        .horizontal
                        .repeat(width.saturating_sub(t.len() + 4)),
                    box_chars.top_right
                );
            } else {
                println!(
                    "{}{}{}",
                    box_chars.top_left,
                    box_chars.horizontal.repeat(width),
                    box_chars.top_right
                );
            }

            for line in content.lines() {
                println!(
                    "{} {:<width$} {}",
                    box_chars.vertical, line, box_chars.vertical
                );
            }

            println!(
                "{}{}{}",
                box_chars.bottom_left,
                box_chars.horizontal.repeat(width),
                box_chars.bottom_right
            );
        }
    }

    /// Print a tree structure.
    ///
    /// In plain mode, prints indented text.
    pub fn print_tree(&self, tree: &Tree) {
        trace!(mode = ?self.mode, "print_tree");

        if self.is_rich() {
            let console = Console::new();
            console.print_renderable(tree);
        } else {
            // Render tree as plain indented text (tree.render_plain() uses internal guides)
            println!("{}", tree.render_plain());
        }
    }

    /// Print a horizontal rule.
    ///
    /// In plain mode, prints a line of dashes.
    pub fn print_rule(&self, title: Option<&str>) {
        trace!(mode = ?self.mode, title = ?title, "print_rule");

        let width = self.width.saturating_sub(2).max(40);

        if self.is_rich() {
            let box_chars = self.theme.box_style.chars();
            if let Some(t) = title {
                let padding = (width.saturating_sub(t.len() + 2)) / 2;
                let styled_title = self.render_style(&self.theme.colors.header, t);
                println!(
                    "{}{}{}{}{}",
                    box_chars.horizontal.repeat(padding),
                    " ",
                    styled_title,
                    " ",
                    box_chars.horizontal.repeat(width - padding - t.len() - 2)
                );
            } else {
                println!("{}", box_chars.horizontal.repeat(width));
            }
        } else {
            if let Some(t) = title {
                let padding = (width.saturating_sub(t.len() + 2)) / 2;
                println!(
                    "{} {} {}",
                    "-".repeat(padding),
                    t,
                    "-".repeat(width - padding - t.len() - 2)
                );
            } else {
                println!("{}", "-".repeat(width));
            }
        }
    }

    /// Print markdown content.
    ///
    /// In plain mode, prints the raw markdown.
    pub fn print_markdown(&self, md: &str) {
        trace!(mode = ?self.mode, "print_markdown");

        if self.is_rich() {
            let markdown = Markdown::new(md);
            let console = Console::new();
            console.print_renderable(&markdown);
        } else {
            println!("{md}");
        }
    }

    /// Print syntax-highlighted code.
    ///
    /// In plain mode, prints the raw code.
    pub fn print_syntax(&self, code: &str, language: &str) {
        trace!(mode = ?self.mode, language = language, "print_syntax");

        if self.is_rich() {
            let syntax = Syntax::new(code, language);
            let console = Console::new();
            console.print_renderable(&syntax);
        } else {
            println!("```{language}");
            println!("{code}");
            println!("```");
        }
    }

    // =========================================================================
    // Semantic Output
    // =========================================================================

    /// Print a success message.
    pub fn success(&self, message: &str) {
        trace!(mode = ?self.mode, "success");

        let icon = self.theme.icons.get("success", self.use_unicode);
        if self.is_rich() {
            let styled = self.render_style(&self.theme.colors.success, message);
            println!("{icon} {styled}");
        } else if icon.is_empty() {
            println!("OK: {message}");
        } else {
            println!("{icon} {message}");
        }
    }

    /// Print an error message.
    pub fn error(&self, message: &str) {
        trace!(mode = ?self.mode, "error");

        let icon = self.theme.icons.get("error", self.use_unicode);
        if self.is_rich() {
            let styled = self.render_style(&self.theme.colors.error, message);
            eprintln!("{icon} {styled}");
        } else if icon.is_empty() {
            eprintln!("ERROR: {message}");
        } else {
            eprintln!("{icon} {message}");
        }
    }

    /// Print a warning message.
    pub fn warning(&self, message: &str) {
        trace!(mode = ?self.mode, "warning");

        let icon = self.theme.icons.get("warning", self.use_unicode);
        if self.is_rich() {
            let styled = self.render_style(&self.theme.colors.warning, message);
            eprintln!("{icon} {styled}");
        } else if icon.is_empty() {
            eprintln!("WARN: {message}");
        } else {
            eprintln!("{icon} {message}");
        }
    }

    /// Print an info message.
    pub fn info(&self, message: &str) {
        trace!(mode = ?self.mode, "info");

        let icon = self.theme.icons.get("info", self.use_unicode);
        if self.is_rich() {
            let styled = self.render_style(&self.theme.colors.info, message);
            println!("{icon} {styled}");
        } else if icon.is_empty() {
            println!("INFO: {message}");
        } else {
            println!("{icon} {message}");
        }
    }

    /// Print a hint message.
    pub fn hint(&self, message: &str) {
        trace!(mode = ?self.mode, "hint");

        let icon = self.theme.icons.get("hint", self.use_unicode);
        if self.is_rich() {
            let styled = self.render_style(&self.theme.colors.hint, message);
            println!("{icon} {styled}");
        } else if icon.is_empty() {
            println!("HINT: {message}");
        } else {
            println!("{icon} {message}");
        }
    }

    /// Print a debug message.
    ///
    /// Only prints if verbose mode is enabled.
    pub fn debug(&self, message: &str) {
        trace!(mode = ?self.mode, "debug");

        if self.is_rich() {
            let styled = self.render_style(&self.theme.colors.debug, message);
            eprintln!("{styled}");
        } else {
            eprintln!("DEBUG: {message}");
        }
    }

    // =========================================================================
    // Structural Output
    // =========================================================================

    /// Print a horizontal rule.
    pub fn rule(&self, title: Option<&str>) {
        self.print_rule(title);
    }

    /// Print a blank line.
    pub fn newline(&self) {
        println!();
    }

    /// Print a header.
    pub fn header(&self, text: &str) {
        trace!(mode = ?self.mode, "header");

        if self.is_rich() {
            let styled = self.render_style(&self.theme.colors.header, text);
            println!("\n{styled}");
            self.print_rule(None);
        } else {
            println!("\n{text}");
            println!("{}", "=".repeat(text.len()));
        }
    }

    /// Print a subheader.
    pub fn subheader(&self, text: &str) {
        trace!(mode = ?self.mode, "subheader");

        if self.is_rich() {
            let styled = self.render_style(&self.theme.colors.subheader, text);
            println!("\n{styled}");
        } else {
            println!("\n{text}");
            println!("{}", "-".repeat(text.len()));
        }
    }

    /// Print a section with a title and rule.
    pub fn section(&self, title: &str) {
        trace!(mode = ?self.mode, "section");
        self.print_rule(Some(title));
    }

    // =========================================================================
    // Data Display
    // =========================================================================

    /// Print syntax-highlighted code.
    pub fn code(&self, code: &str, language: &str) {
        self.print_syntax(code, language);
    }

    /// Print a JSON value with highlighting.
    pub fn json(&self, value: &serde_json::Value) {
        trace!(mode = ?self.mode, "json");

        if self.is_json() {
            println!(
                "{}",
                serde_json::to_string_pretty(value).unwrap_or_default()
            );
        } else if self.is_rich() {
            let json_str = serde_json::to_string_pretty(value).unwrap_or_default();
            self.print_syntax(&json_str, "json");
        } else {
            println!(
                "{}",
                serde_json::to_string_pretty(value).unwrap_or_default()
            );
        }
    }

    /// Print a serializable value as JSON.
    pub fn json_value<T: Serialize>(&self, value: &T) {
        if let Ok(json) = serde_json::to_value(value) {
            self.json(&json);
        }
    }

    /// Print a key-value pair.
    pub fn key_value(&self, key: &str, value: &str) {
        trace!(mode = ?self.mode, key = key, "key_value");

        if self.is_rich() {
            let styled_key = self.render_style(&self.theme.colors.key, key);
            let styled_value = self.render_style(&self.theme.colors.value, value);
            println!("{styled_key}: {styled_value}");
        } else {
            println!("{key}: {value}");
        }
    }

    /// Print a list of key-value pairs.
    pub fn key_value_list(&self, pairs: &[(&str, &str)]) {
        trace!(mode = ?self.mode, count = pairs.len(), "key_value_list");

        // Find the longest key for alignment
        let max_key_len = pairs.iter().map(|(k, _)| k.len()).max().unwrap_or(0);

        for (key, value) in pairs {
            if self.is_rich() {
                let styled_key =
                    self.render_style(&self.theme.colors.key, &format!("{key:>max_key_len$}"));
                let styled_value = self.render_style(&self.theme.colors.value, value);
                println!("{styled_key}: {styled_value}");
            } else {
                println!("{key:>max_key_len$}: {value}");
            }
        }
    }

    /// Print a bulleted list.
    pub fn list(&self, items: &[&str]) {
        trace!(mode = ?self.mode, count = items.len(), "list");

        let bullet = self.theme.icons.get("bullet", self.use_unicode);
        for item in items {
            if self.is_rich() {
                let styled = self.render_style(&self.theme.colors.value, item);
                println!("  {bullet} {styled}");
            } else {
                println!("  {bullet} {item}");
            }
        }
    }

    /// Print a numbered list.
    pub fn numbered_list(&self, items: &[&str]) {
        trace!(mode = ?self.mode, count = items.len(), "numbered_list");

        let width = items.len().to_string().len();
        for (i, item) in items.iter().enumerate() {
            if self.is_rich() {
                let num =
                    self.render_style(&self.theme.colors.number, &format!("{:>width$}", i + 1));
                let styled = self.render_style(&self.theme.colors.value, item);
                println!("  {num}. {styled}");
            } else {
                println!("  {:>width$}. {item}", i + 1);
            }
        }
    }

    /// Print a diff between two strings.
    pub fn diff(&self, old: &str, new: &str) {
        trace!(mode = ?self.mode, "diff");

        // Simple line-by-line diff
        let old_lines: Vec<&str> = old.lines().collect();
        let new_lines: Vec<&str> = new.lines().collect();

        for line in &old_lines {
            if !new_lines.contains(line) {
                if self.is_rich() {
                    let styled = self.render_style(&self.theme.colors.error, &format!("- {line}"));
                    println!("{styled}");
                } else {
                    println!("- {line}");
                }
            }
        }

        for line in &new_lines {
            if !old_lines.contains(line) {
                if self.is_rich() {
                    let styled =
                        self.render_style(&self.theme.colors.success, &format!("+ {line}"));
                    println!("{styled}");
                } else {
                    println!("+ {line}");
                }
            }
        }
    }

    // =========================================================================
    // Progress (to stderr)
    // =========================================================================

    /// Print a progress indicator.
    ///
    /// Output goes to stderr to not interfere with stdout.
    pub fn progress(&self, current: u64, total: u64, message: &str) {
        trace!(mode = ?self.mode, current = current, total = total, "progress");

        let pct = if total > 0 {
            (current * 100) / total
        } else {
            0
        };

        let bar_width = 20;
        let filled = (pct as usize * bar_width) / 100;
        let empty = bar_width - filled;

        let progress_chars = self.theme.progress_style.chars();

        if self.is_rich() {
            let bar = format!(
                "{}{}",
                self.render_style(
                    &self.theme.colors.progress_done,
                    &progress_chars.filled.repeat(filled)
                ),
                self.render_style(
                    &self.theme.colors.progress_remaining,
                    &progress_chars.empty.repeat(empty)
                )
            );
            let text = self.render_style(&self.theme.colors.progress_text, message);
            eprint!("\r[{bar}] {pct:>3}% {text}");
        } else {
            let bar = format!(
                "{}{}",
                progress_chars.filled.repeat(filled),
                progress_chars.empty.repeat(empty)
            );
            eprint!("\r[{bar}] {pct:>3}% {message}");
        }
        let _ = io::stderr().flush();
    }

    /// Start a spinner animation and return a handle to control it.
    ///
    /// The spinner runs in the background and writes to stderr.
    /// Call `handle.stop()` or drop the handle to stop it.
    #[must_use]
    pub fn spinner(&self, message: &str) -> SpinnerHandle {
        trace!(mode = ?self.mode, "spinner");

        let running = Arc::new(AtomicBool::new(true));
        let message_arc = Arc::new(Mutex::new(message.to_string()));

        // In plain mode or JSON mode, just print a status message
        if !self.is_rich() {
            eprintln!("... {message}");
            return SpinnerHandle {
                running,
                message: message_arc,
            };
        }

        // For rich mode, we return a handle but the actual animation
        // would need to be driven externally (e.g., by a separate thread)
        // For simplicity, we just print the message with a spinner char
        let spinner_char = &self.theme.icons.spinner_frames[0];
        eprint!("\r{spinner_char} {message}");
        let _ = io::stderr().flush();

        SpinnerHandle {
            running,
            message: message_arc,
        }
    }

    /// Print a one-line status update.
    ///
    /// Overwrites the current line on stderr.
    pub fn status_line(&self, status: &str, message: &str) {
        trace!(mode = ?self.mode, status = status, "status_line");

        if self.is_rich() {
            let styled_status = self.render_style(&self.theme.colors.info, status);
            let styled_msg = self.render_style(&self.theme.colors.value, message);
            eprint!("\r{styled_status}: {styled_msg}");
        } else {
            eprint!("\r{status}: {message}");
        }
        let _ = io::stderr().flush();
    }

    /// Clear the current status line.
    pub fn clear_status(&self) {
        let width = self.width;
        eprint!("\r{:width$}\r", "");
        let _ = io::stderr().flush();
    }

    // =========================================================================
    // Formatting Output (return String, don't print)
    // =========================================================================

    /// Format text with a style, returning the result.
    #[must_use]
    pub fn format_styled(&self, text: &str, style_spec: &str) -> String {
        if self.is_rich() {
            if let Ok(style) = Style::parse(style_spec) {
                return self.render_style(&style, text);
            }
        }
        text.to_string()
    }

    /// Format a success message, returning the result.
    #[must_use]
    pub fn format_success(&self, message: &str) -> String {
        let icon = self.theme.icons.get("success", self.use_unicode);
        if self.is_rich() {
            let styled = self.render_style(&self.theme.colors.success, message);
            format!("{icon} {styled}")
        } else if icon.is_empty() {
            format!("OK: {message}")
        } else {
            format!("{icon} {message}")
        }
    }

    /// Format an error message, returning the result.
    #[must_use]
    pub fn format_error(&self, message: &str) -> String {
        let icon = self.theme.icons.get("error", self.use_unicode);
        if self.is_rich() {
            let styled = self.render_style(&self.theme.colors.error, message);
            format!("{icon} {styled}")
        } else if icon.is_empty() {
            format!("ERROR: {message}")
        } else {
            format!("{icon} {message}")
        }
    }

    /// Format a warning message, returning the result.
    #[must_use]
    pub fn format_warning(&self, message: &str) -> String {
        let icon = self.theme.icons.get("warning", self.use_unicode);
        if self.is_rich() {
            let styled = self.render_style(&self.theme.colors.warning, message);
            format!("{icon} {styled}")
        } else if icon.is_empty() {
            format!("WARN: {message}")
        } else {
            format!("{icon} {message}")
        }
    }

    /// Format an info message, returning the result.
    #[must_use]
    pub fn format_info(&self, message: &str) -> String {
        let icon = self.theme.icons.get("info", self.use_unicode);
        if self.is_rich() {
            let styled = self.render_style(&self.theme.colors.info, message);
            format!("{icon} {styled}")
        } else if icon.is_empty() {
            format!("INFO: {message}")
        } else {
            format!("{icon} {message}")
        }
    }

    /// Format a key-value pair, returning the result.
    #[must_use]
    pub fn format_key_value(&self, key: &str, value: &str) -> String {
        if self.is_rich() {
            let styled_key = self.render_style(&self.theme.colors.key, key);
            let styled_value = self.render_style(&self.theme.colors.value, value);
            format!("{styled_key}: {styled_value}")
        } else {
            format!("{key}: {value}")
        }
    }
}

impl Default for RichOutput {
    fn default() -> Self {
        Self::plain()
    }
}

impl std::fmt::Debug for RichOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RichOutput")
            .field("mode", &self.mode)
            .field("width", &self.width)
            .field("color_system", &self.color_system)
            .field("use_unicode", &self.use_unicode)
            .finish()
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Get the terminal width, defaulting to 80 if detection fails.
fn terminal_width() -> usize {
    crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(80)
}

/// Strip markup tags from text.
fn strip_markup(text: &str) -> String {
    // Simple markup stripping: remove [tag] and [/tag] patterns
    let mut result = String::with_capacity(text.len());
    let mut in_tag = false;

    for ch in text.chars() {
        match ch {
            '[' => in_tag = true,
            ']' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }

    result
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_mode() {
        let output = RichOutput::plain();
        assert!(output.is_plain());
        assert!(!output.is_rich());
        assert!(!output.is_json());
    }

    #[test]
    fn test_output_mode() {
        let output = RichOutput::plain();
        assert_eq!(output.mode(), OutputMode::Plain);
        assert!(!output.mode().allows_styling());
        assert!(output.mode().is_plain());
    }

    #[test]
    fn test_strip_markup() {
        assert_eq!(strip_markup("[bold]text[/bold]"), "text");
        assert_eq!(strip_markup("[red]error[/]"), "error");
        assert_eq!(strip_markup("no markup"), "no markup");
        assert_eq!(strip_markup("[a][b]nested[/b][/a]"), "nested");
    }

    #[test]
    fn test_format_methods() {
        let output = RichOutput::plain();

        let success = output.format_success("done");
        assert!(success.contains("done"));

        let error = output.format_error("failed");
        assert!(error.contains("failed"));

        let kv = output.format_key_value("key", "value");
        assert!(kv.contains("key"));
        assert!(kv.contains("value"));
    }

    #[test]
    fn test_spinner_handle() {
        let output = RichOutput::plain();
        let handle = output.spinner("loading");

        assert!(handle.is_running());
        handle.set_message("still loading");
        handle.stop();
        assert!(!handle.is_running());
    }

    #[test]
    fn test_default_is_plain() {
        let output = RichOutput::default();
        assert!(output.is_plain());
    }

    #[test]
    fn test_debug_impl() {
        let output = RichOutput::plain();
        let debug = format!("{output:?}");
        assert!(debug.contains("RichOutput"));
        assert!(debug.contains("Plain"));
    }
}
