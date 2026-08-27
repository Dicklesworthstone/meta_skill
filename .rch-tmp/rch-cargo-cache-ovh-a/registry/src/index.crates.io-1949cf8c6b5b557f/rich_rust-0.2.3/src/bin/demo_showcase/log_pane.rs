//! Streaming log pane for the demo_showcase dashboard.
//!
//! Provides a bounded, styled log view that integrates with Live displays.
//! Renders log lines with:
//! - Level-based coloring (TRACE, DEBUG, INFO, WARN, ERROR)
//! - Timestamp prefixes
//! - HTTP method highlighting (GET, POST, PUT, DELETE, PATCH)
//! - Keyword emphasis for common patterns

// Module is prepared for scene implementations; used in tests
#![allow(dead_code)]

use std::time::Duration;

use rich_rust::console::{Console, ConsoleOptions};
use rich_rust::renderables::Renderable;
use rich_rust::segment::Segment;
use rich_rust::text::Text;

use super::state::{LogLevel, LogLine};

/// Default number of log lines to display in the pane.
pub const DEFAULT_LOG_LIMIT: usize = 12;

/// A streaming log pane that displays bounded log output.
///
/// The pane renders the most recent log lines with appropriate styling:
/// - Each log level has a distinct color
/// - HTTP methods (GET, POST, etc.) are highlighted
/// - Timestamps are displayed in a compact format
#[derive(Debug, Clone)]
pub struct LogPane {
    /// Log lines to display (already bounded by caller).
    lines: Vec<LogLine>,
    /// Maximum lines to display.
    limit: usize,
    /// Whether to show timestamps.
    show_timestamps: bool,
    /// Whether to show log levels.
    show_levels: bool,
    /// Title for the pane (optional).
    title: Option<String>,
}

impl Default for LogPane {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            limit: DEFAULT_LOG_LIMIT,
            show_timestamps: true,
            show_levels: true,
            title: None,
        }
    }
}

impl LogPane {
    /// Create a new log pane with the given lines.
    #[must_use]
    pub fn new(lines: Vec<LogLine>) -> Self {
        Self {
            lines,
            ..Self::default()
        }
    }

    /// Create from a snapshot, taking the last `limit` lines.
    #[must_use]
    pub fn from_snapshot(logs: &[LogLine], limit: usize) -> Self {
        let start = logs.len().saturating_sub(limit);
        Self {
            lines: logs[start..].to_vec(),
            limit,
            ..Self::default()
        }
    }

    /// Set the maximum number of lines to display.
    #[must_use]
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = limit.max(1);
        self
    }

    /// Set whether to show timestamps.
    #[must_use]
    pub fn show_timestamps(mut self, show: bool) -> Self {
        self.show_timestamps = show;
        self
    }

    /// Set whether to show log levels.
    #[must_use]
    pub fn show_levels(mut self, show: bool) -> Self {
        self.show_levels = show;
        self
    }

    /// Set an optional title for the pane.
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Get the style name for a log level.
    #[must_use]
    fn level_style(level: LogLevel) -> &'static str {
        match level {
            LogLevel::Trace => "log.trace",
            LogLevel::Debug => "log.debug",
            LogLevel::Info => "log.info",
            LogLevel::Warn => "log.warn",
            LogLevel::Error => "log.error",
        }
    }

    /// Format a duration as a compact timestamp.
    #[must_use]
    fn format_timestamp(duration: Duration) -> String {
        let total_ms = duration.as_millis();
        if total_ms < 1000 {
            format!("{total_ms:>4}ms")
        } else if total_ms < 60_000 {
            let secs = total_ms / 1000;
            let ms = total_ms % 1000;
            format!("{secs:>2}.{ms:03}s")
        } else {
            let mins = total_ms / 60_000;
            let secs = (total_ms % 60_000) / 1000;
            format!("{mins:>2}m{secs:02}s")
        }
    }

    /// Highlight HTTP methods and common keywords in a message.
    ///
    /// Returns markup-enhanced text.
    #[must_use]
    fn highlight_message(message: &str, base_style: &str) -> String {
        let mut result = message.to_string();

        // HTTP methods - make them stand out
        let http_methods = [
            ("GET", "[bold cyan]GET[/]"),
            ("POST", "[bold green]POST[/]"),
            ("PUT", "[bold yellow]PUT[/]"),
            ("DELETE", "[bold red]DELETE[/]"),
            ("PATCH", "[bold magenta]PATCH[/]"),
            ("HEAD", "[bold blue]HEAD[/]"),
            ("OPTIONS", "[dim cyan]OPTIONS[/]"),
        ];

        for (method, replacement) in http_methods {
            // Only replace whole words (avoid matching inside other words)
            result = replace_whole_word(&result, method, replacement);
        }

        // Status codes - highlight common ones
        let status_patterns = [
            ("200", "[green]200[/]"),
            ("201", "[green]201[/]"),
            ("204", "[green]204[/]"),
            ("400", "[yellow]400[/]"),
            ("401", "[yellow]401[/]"),
            ("403", "[yellow]403[/]"),
            ("404", "[yellow]404[/]"),
            ("500", "[bold red]500[/]"),
            ("502", "[bold red]502[/]"),
            ("503", "[bold red]503[/]"),
        ];

        for (code, replacement) in status_patterns {
            result = replace_whole_word(&result, code, replacement);
        }

        // Wrap entire message in base style
        format!("[{base_style}]{result}[/]")
    }

    /// Render a single log line to markup.
    #[must_use]
    fn render_line(&self, line: &LogLine) -> String {
        let mut parts = Vec::new();

        // Timestamp
        if self.show_timestamps {
            let ts = Self::format_timestamp(line.t);
            parts.push(format!("[dim]{ts}[/]"));
        }

        // Level badge
        if self.show_levels {
            let level_style = Self::level_style(line.level);
            let level_text = line.level.as_str();
            parts.push(format!("[{level_style}]{level_text:>5}[/]"));
        }

        // Message with highlights
        let msg_style = Self::level_style(line.level);
        let highlighted = Self::highlight_message(&line.message, msg_style);
        parts.push(highlighted);

        parts.join(" ")
    }

    /// Render the log pane to a vector of markup strings (one per line).
    #[must_use]
    pub fn render_lines(&self) -> Vec<String> {
        let start = self.lines.len().saturating_sub(self.limit);
        self.lines[start..]
            .iter()
            .map(|line| self.render_line(line))
            .collect()
    }

    /// Render to a single multi-line string with markup.
    #[must_use]
    pub fn render_markup(&self) -> String {
        self.render_lines().join("\n")
    }

    /// Create a Text object from the log lines for embedding in other renderables.
    #[must_use]
    pub fn as_text(&self) -> Text {
        let markup = self.render_markup();
        rich_rust::markup::render_or_plain(&markup)
    }
}

impl Renderable for LogPane {
    fn render<'a>(&'a self, _console: &Console, _options: &ConsoleOptions) -> Vec<Segment<'a>> {
        let text = self.as_text();
        text.render("")
            .into_iter()
            .map(Segment::into_owned)
            .collect()
    }
}

/// Replace whole words only (not substrings).
fn replace_whole_word(text: &str, word: &str, replacement: &str) -> String {
    let mut result = String::with_capacity(text.len() + replacement.len());
    let mut chars = text.char_indices().peekable();
    let word_chars: Vec<char> = word.chars().collect();

    while let Some((i, c)) = chars.next() {
        // Check if this could be the start of our word
        if c == word_chars[0] {
            // Check if previous char was a word boundary (or start of string)
            let at_word_start = i == 0 || {
                let prev = text[..i].chars().last().unwrap();
                !prev.is_alphanumeric() && prev != '_'
            };

            if at_word_start {
                // Try to match the whole word
                let remaining = &text[i..];
                if remaining.starts_with(word) {
                    // Check if next char is a word boundary (or end of string)
                    let end_idx = i + word.len();
                    let at_word_end = end_idx >= text.len() || {
                        let next = text[end_idx..].chars().next().unwrap();
                        !next.is_alphanumeric() && next != '_'
                    };

                    if at_word_end {
                        result.push_str(replacement);
                        // Skip the matched word
                        for _ in 1..word.len() {
                            chars.next();
                        }
                        continue;
                    }
                }
            }
        }
        result.push(c);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_log(level: LogLevel, ms: u64, msg: &str) -> LogLine {
        LogLine {
            t: Duration::from_millis(ms),
            level,
            message: msg.to_string(),
        }
    }

    #[test]
    fn test_format_timestamp_millis() {
        assert_eq!(
            LogPane::format_timestamp(Duration::from_millis(42)),
            "  42ms"
        );
        assert_eq!(
            LogPane::format_timestamp(Duration::from_millis(999)),
            " 999ms"
        );
    }

    #[test]
    fn test_format_timestamp_seconds() {
        assert_eq!(
            LogPane::format_timestamp(Duration::from_millis(1234)),
            " 1.234s"
        );
        assert_eq!(
            LogPane::format_timestamp(Duration::from_millis(59999)),
            "59.999s"
        );
    }

    #[test]
    fn test_format_timestamp_minutes() {
        assert_eq!(
            LogPane::format_timestamp(Duration::from_millis(60_000)),
            " 1m00s"
        );
        assert_eq!(
            LogPane::format_timestamp(Duration::from_millis(125_000)),
            " 2m05s"
        );
    }

    #[test]
    fn test_level_style_mapping() {
        assert_eq!(LogPane::level_style(LogLevel::Trace), "log.trace");
        assert_eq!(LogPane::level_style(LogLevel::Debug), "log.debug");
        assert_eq!(LogPane::level_style(LogLevel::Info), "log.info");
        assert_eq!(LogPane::level_style(LogLevel::Warn), "log.warn");
        assert_eq!(LogPane::level_style(LogLevel::Error), "log.error");
    }

    #[test]
    fn test_replace_whole_word() {
        // Should replace whole word
        assert_eq!(
            replace_whole_word("GET /api", "GET", "[bold]GET[/]"),
            "[bold]GET[/] /api"
        );

        // Should not replace partial match
        assert_eq!(
            replace_whole_word("GETTING started", "GET", "[bold]GET[/]"),
            "GETTING started"
        );

        // Multiple occurrences
        assert_eq!(replace_whole_word("GET and GET", "GET", "X"), "X and X");
    }

    #[test]
    fn test_highlight_http_methods() {
        let result = LogPane::highlight_message("GET /api/users", "log.info");
        assert!(result.contains("[bold cyan]GET[/]"));
        assert!(result.contains("/api/users"));

        let result = LogPane::highlight_message("POST /api/create", "log.info");
        assert!(result.contains("[bold green]POST[/]"));
    }

    #[test]
    fn test_highlight_status_codes() {
        let result = LogPane::highlight_message("Response: 200 OK", "log.info");
        assert!(result.contains("[green]200[/]"));

        let result = LogPane::highlight_message("Error: 500", "log.error");
        assert!(result.contains("[bold red]500[/]"));
    }

    #[test]
    fn test_log_pane_from_snapshot() {
        let logs = vec![
            make_log(LogLevel::Info, 100, "one"),
            make_log(LogLevel::Info, 200, "two"),
            make_log(LogLevel::Info, 300, "three"),
            make_log(LogLevel::Info, 400, "four"),
        ];

        let pane = LogPane::from_snapshot(&logs, 2);
        let lines = pane.render_lines();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("three"));
        assert!(lines[1].contains("four"));
    }

    #[test]
    fn test_log_pane_render_line() {
        let pane = LogPane::new(vec![make_log(LogLevel::Info, 42, "test message")]);
        let lines = pane.render_lines();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("42ms"));
        assert!(lines[0].contains("INFO"));
        assert!(lines[0].contains("test message"));
    }

    #[test]
    fn test_log_pane_no_timestamps() {
        let pane = LogPane::new(vec![make_log(LogLevel::Info, 42, "test")]).show_timestamps(false);
        let lines = pane.render_lines();
        assert!(!lines[0].contains("42ms"));
        assert!(lines[0].contains("INFO"));
    }

    #[test]
    fn test_log_pane_no_levels() {
        let pane = LogPane::new(vec![make_log(LogLevel::Info, 42, "test")]).show_levels(false);
        let lines = pane.render_lines();
        assert!(!lines[0].contains("INFO"));
        assert!(lines[0].contains("test"));
    }
}
