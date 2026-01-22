//! Fallback rendering implementations.
//!
//! This module provides simple, reliable fallback renderers for when rich
//! output fails. Each fallback level provides progressively simpler output.
//!
//! # Fallback Levels
//!
//! - **Full**: All features available (rich output works)
//! - **Reduced**: Basic styling only (complex features disabled)
//! - **ColorOnly**: Just ANSI colors, no box drawing
//! - **Plain**: No styling at all, pure text
//!
//! The fallback renderers are designed to be as simple and reliable as
//! possible - they use only basic `println!` and `eprintln!` calls with
//! no external dependencies that could fail.

use std::io::{self, Write};

// =============================================================================
// FallbackLevel
// =============================================================================

/// The level of fallback being used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FallbackLevel {
    /// Full rich output available.
    Full,
    /// Reduced features (basic styling only).
    Reduced,
    /// ANSI colors only, no complex rendering.
    ColorOnly,
    /// Plain text, no styling whatsoever.
    Plain,
}

impl FallbackLevel {
    /// Check if this level allows any styling.
    #[must_use]
    pub const fn allows_styling(&self) -> bool {
        matches!(self, FallbackLevel::Full | FallbackLevel::Reduced)
    }

    /// Check if this level allows colors.
    #[must_use]
    pub const fn allows_colors(&self) -> bool {
        !matches!(self, FallbackLevel::Plain)
    }

    /// Get a human-readable description.
    #[must_use]
    pub const fn description(&self) -> &'static str {
        match self {
            FallbackLevel::Full => "full rich output",
            FallbackLevel::Reduced => "reduced styling",
            FallbackLevel::ColorOnly => "colors only",
            FallbackLevel::Plain => "plain text",
        }
    }
}

impl Default for FallbackLevel {
    fn default() -> Self {
        FallbackLevel::Plain
    }
}

// =============================================================================
// FallbackRenderer
// =============================================================================

/// A minimal renderer that provides basic output without rich formatting.
///
/// This is used when the primary `RichOutput` fails or is unavailable.
/// All methods are designed to be as simple and reliable as possible.
#[derive(Debug, Clone)]
pub struct FallbackRenderer {
    level: FallbackLevel,
    width: usize,
}

impl FallbackRenderer {
    /// Create a new fallback renderer.
    #[must_use]
    pub fn new(level: FallbackLevel) -> Self {
        Self { level, width: 80 }
    }

    /// Create a fallback renderer with custom width.
    #[must_use]
    pub fn with_width(level: FallbackLevel, width: usize) -> Self {
        Self { level, width }
    }

    /// Get the current fallback level.
    #[must_use]
    pub const fn level(&self) -> FallbackLevel {
        self.level
    }

    // =========================================================================
    // Semantic Output
    // =========================================================================

    /// Print a success message.
    pub fn success(&self, message: &str) {
        if self.level.allows_colors() {
            println!("\x1b[32m✓\x1b[0m {message}");
        } else {
            println!("OK: {message}");
        }
    }

    /// Print an error message.
    pub fn error(&self, message: &str) {
        if self.level.allows_colors() {
            eprintln!("\x1b[31m✗\x1b[0m {message}");
        } else {
            eprintln!("ERROR: {message}");
        }
    }

    /// Print a warning message.
    pub fn warning(&self, message: &str) {
        if self.level.allows_colors() {
            eprintln!("\x1b[33m⚠\x1b[0m {message}");
        } else {
            eprintln!("WARN: {message}");
        }
    }

    /// Print an info message.
    pub fn info(&self, message: &str) {
        if self.level.allows_colors() {
            println!("\x1b[34mℹ\x1b[0m {message}");
        } else {
            println!("INFO: {message}");
        }
    }

    /// Print a hint message.
    pub fn hint(&self, message: &str) {
        if self.level.allows_colors() {
            println!("\x1b[36m💡\x1b[0m {message}");
        } else {
            println!("HINT: {message}");
        }
    }

    // =========================================================================
    // Structural Output
    // =========================================================================

    /// Print a header.
    pub fn header(&self, text: &str) {
        println!();
        if self.level.allows_colors() {
            println!("\x1b[1;37m{text}\x1b[0m");
        } else {
            println!("{text}");
        }
        println!("{}", "=".repeat(text.len().min(self.width)));
    }

    /// Print a subheader.
    pub fn subheader(&self, text: &str) {
        println!();
        if self.level.allows_colors() {
            println!("\x1b[1m{text}\x1b[0m");
        } else {
            println!("{text}");
        }
        println!("{}", "-".repeat(text.len().min(self.width)));
    }

    /// Print a horizontal rule.
    pub fn rule(&self, title: Option<&str>) {
        let line_char = '-';
        let width = self.width.saturating_sub(2).max(40);

        match title {
            Some(t) => {
                let padding = (width.saturating_sub(t.len() + 2)) / 2;
                println!(
                    "{} {} {}",
                    line_char.to_string().repeat(padding),
                    t,
                    line_char
                        .to_string()
                        .repeat(width - padding - t.len() - 2)
                );
            }
            None => {
                println!("{}", line_char.to_string().repeat(width));
            }
        }
    }

    // =========================================================================
    // Data Display
    // =========================================================================

    /// Print a key-value pair.
    pub fn key_value(&self, key: &str, value: &str) {
        if self.level.allows_colors() {
            println!("\x1b[1m{key}\x1b[0m: {value}");
        } else {
            println!("{key}: {value}");
        }
    }

    /// Print a list of key-value pairs.
    pub fn key_value_list(&self, pairs: &[(&str, &str)]) {
        let max_key_len = pairs.iter().map(|(k, _)| k.len()).max().unwrap_or(0);

        for (key, value) in pairs {
            if self.level.allows_colors() {
                println!("\x1b[1m{key:>max_key_len$}\x1b[0m: {value}");
            } else {
                println!("{key:>max_key_len$}: {value}");
            }
        }
    }

    /// Print a bulleted list.
    pub fn list(&self, items: &[&str]) {
        for item in items {
            println!("  - {item}");
        }
    }

    /// Print a numbered list.
    pub fn numbered_list(&self, items: &[&str]) {
        let width = items.len().to_string().len();
        for (i, item) in items.iter().enumerate() {
            println!("  {:>width$}. {item}", i + 1);
        }
    }

    // =========================================================================
    // Complex Renderables (Simplified)
    // =========================================================================

    /// Print a simple panel (box with optional title).
    pub fn panel(&self, content: &str, title: Option<&str>) {
        let width = self.width.saturating_sub(4).max(40);

        // Top border
        if let Some(t) = title {
            println!("+-- {} {}", t, "-".repeat(width.saturating_sub(t.len() + 5)));
        } else {
            println!("+{}", "-".repeat(width));
        }

        // Content
        for line in content.lines() {
            println!("| {line}");
        }

        // Bottom border
        println!("+{}", "-".repeat(width));
    }

    /// Print code with simple formatting.
    pub fn code(&self, code: &str, language: &str) {
        println!("```{language}");
        println!("{code}");
        println!("```");
    }

    // =========================================================================
    // Progress
    // =========================================================================

    /// Print a progress indicator.
    pub fn progress(&self, current: u64, total: u64, message: &str) {
        let pct = if total > 0 {
            (current * 100) / total
        } else {
            0
        };

        let bar_width = 20;
        let filled = (pct as usize * bar_width) / 100;
        let empty = bar_width - filled;

        let bar = format!("{}{}", "#".repeat(filled), ".".repeat(empty));

        if self.level.allows_colors() {
            eprint!("\r\x1b[32m[{bar}]\x1b[0m {pct:>3}% {message}");
        } else {
            eprint!("\r[{bar}] {pct:>3}% {message}");
        }
        let _ = io::stderr().flush();
    }

    /// Print a status line.
    pub fn status_line(&self, status: &str, message: &str) {
        if self.level.allows_colors() {
            eprint!("\r\x1b[34m{status}\x1b[0m: {message}");
        } else {
            eprint!("\r{status}: {message}");
        }
        let _ = io::stderr().flush();
    }

    // =========================================================================
    // Table (Simplified TSV)
    // =========================================================================

    /// Print tabular data as simple tab-separated values.
    pub fn table_tsv(&self, headers: &[&str], rows: &[Vec<String>]) {
        // Print headers
        println!("{}", headers.join("\t"));

        // Print separator
        println!(
            "{}",
            headers
                .iter()
                .map(|h| "-".repeat(h.len()))
                .collect::<Vec<_>>()
                .join("\t")
        );

        // Print rows
        for row in rows {
            println!("{}", row.join("\t"));
        }
    }
}

impl Default for FallbackRenderer {
    fn default() -> Self {
        Self::new(FallbackLevel::Plain)
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fallback_level_ordering() {
        assert!(FallbackLevel::Full < FallbackLevel::Reduced);
        assert!(FallbackLevel::Reduced < FallbackLevel::ColorOnly);
        assert!(FallbackLevel::ColorOnly < FallbackLevel::Plain);
    }

    #[test]
    fn test_fallback_level_allows_styling() {
        assert!(FallbackLevel::Full.allows_styling());
        assert!(FallbackLevel::Reduced.allows_styling());
        assert!(!FallbackLevel::ColorOnly.allows_styling());
        assert!(!FallbackLevel::Plain.allows_styling());
    }

    #[test]
    fn test_fallback_level_allows_colors() {
        assert!(FallbackLevel::Full.allows_colors());
        assert!(FallbackLevel::Reduced.allows_colors());
        assert!(FallbackLevel::ColorOnly.allows_colors());
        assert!(!FallbackLevel::Plain.allows_colors());
    }

    #[test]
    fn test_fallback_level_description() {
        assert_eq!(FallbackLevel::Full.description(), "full rich output");
        assert_eq!(FallbackLevel::Plain.description(), "plain text");
    }

    #[test]
    fn test_fallback_level_default() {
        assert_eq!(FallbackLevel::default(), FallbackLevel::Plain);
    }

    #[test]
    fn test_fallback_renderer_creation() {
        let renderer = FallbackRenderer::new(FallbackLevel::Plain);
        assert_eq!(renderer.level(), FallbackLevel::Plain);
        assert_eq!(renderer.width, 80);
    }

    #[test]
    fn test_fallback_renderer_with_width() {
        let renderer = FallbackRenderer::with_width(FallbackLevel::ColorOnly, 120);
        assert_eq!(renderer.level(), FallbackLevel::ColorOnly);
        assert_eq!(renderer.width, 120);
    }

    #[test]
    fn test_fallback_renderer_default() {
        let renderer = FallbackRenderer::default();
        assert_eq!(renderer.level(), FallbackLevel::Plain);
    }

    #[test]
    fn test_semantic_output_plain() {
        let renderer = FallbackRenderer::new(FallbackLevel::Plain);

        // These should not panic
        renderer.success("test");
        renderer.error("test");
        renderer.warning("test");
        renderer.info("test");
        renderer.hint("test");
    }

    #[test]
    fn test_semantic_output_colored() {
        let renderer = FallbackRenderer::new(FallbackLevel::ColorOnly);

        // These should not panic
        renderer.success("test");
        renderer.error("test");
        renderer.warning("test");
        renderer.info("test");
        renderer.hint("test");
    }

    #[test]
    fn test_structural_output() {
        let renderer = FallbackRenderer::new(FallbackLevel::Plain);

        renderer.header("Test Header");
        renderer.subheader("Test Subheader");
        renderer.rule(Some("Title"));
        renderer.rule(None);
    }

    #[test]
    fn test_data_display() {
        let renderer = FallbackRenderer::new(FallbackLevel::Plain);

        renderer.key_value("key", "value");
        renderer.key_value_list(&[("k1", "v1"), ("k2", "v2")]);
        renderer.list(&["item1", "item2"]);
        renderer.numbered_list(&["first", "second"]);
    }

    #[test]
    fn test_panel_output() {
        let renderer = FallbackRenderer::new(FallbackLevel::Plain);

        renderer.panel("Content here", Some("Title"));
        renderer.panel("No title", None);
        renderer.panel("Multi\nline\ncontent", Some("Multiline"));
    }

    #[test]
    fn test_code_output() {
        let renderer = FallbackRenderer::new(FallbackLevel::Plain);

        renderer.code("fn main() {}", "rust");
        renderer.code("", "");
    }

    #[test]
    fn test_table_tsv() {
        let renderer = FallbackRenderer::new(FallbackLevel::Plain);

        renderer.table_tsv(
            &["Name", "Value"],
            &[
                vec!["key1".to_string(), "val1".to_string()],
                vec!["key2".to_string(), "val2".to_string()],
            ],
        );
    }

    #[test]
    fn test_progress_output() {
        let renderer = FallbackRenderer::new(FallbackLevel::Plain);

        renderer.progress(50, 100, "Processing...");
        renderer.progress(0, 0, "Edge case");
    }

    #[test]
    fn test_status_line() {
        let renderer = FallbackRenderer::new(FallbackLevel::Plain);

        renderer.status_line("Status", "message");
    }

    #[test]
    fn test_empty_inputs() {
        let renderer = FallbackRenderer::new(FallbackLevel::Plain);

        renderer.success("");
        renderer.header("");
        renderer.key_value("", "");
        renderer.list(&[]);
        renderer.numbered_list(&[]);
        renderer.panel("", None);
    }
}
