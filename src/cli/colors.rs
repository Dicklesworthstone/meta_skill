//! Unified color module for ms CLI
//!
//! Provides semantic colors, pre-built styles, and color support detection
//! that respects terminal capabilities and user preferences.
//!
//! # Usage
//!
//! ```rust,ignore
//! use crate::cli::colors::{MsColors, MsStyles, ColorSupport, styled};
//!
//! let support = ColorSupport::detect();
//! let text = styled("success", MsStyles::success(), support);
//! ```

use colored::{ColoredString, Colorize};
use std::io::IsTerminal;

// ============================================================================
// Color Support Detection
// ============================================================================

/// Level of color support detected for the terminal
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSupport {
    /// No color support (NO_COLOR set, TERM=dumb, piped output)
    None,
    /// Basic 16-color support
    Basic,
    /// Extended 256-color support
    Extended,
    /// True color (24-bit) support
    TrueColor,
}

impl ColorSupport {
    /// Detect color support from environment and terminal capabilities
    #[must_use]
    pub fn detect() -> Self {
        // Check NO_COLOR first (highest priority per spec)
        // https://no-color.org/
        if std::env::var("NO_COLOR").is_ok() {
            return Self::None;
        }

        // Check FORCE_COLOR (override for piped output)
        if std::env::var("FORCE_COLOR").is_ok() {
            return Self::detect_level();
        }

        // Check if stdout is a TTY
        if !std::io::stdout().is_terminal() {
            return Self::None;
        }

        // Check TERM for dumb terminals
        if let Ok(term) = std::env::var("TERM") {
            if term == "dumb" {
                return Self::None;
            }
        }

        Self::detect_level()
    }

    /// Detect the level of color support (assuming colors are enabled)
    fn detect_level() -> Self {
        // Check COLORTERM for truecolor
        if let Ok(ct) = std::env::var("COLORTERM") {
            if ct == "truecolor" || ct == "24bit" {
                return Self::TrueColor;
            }
        }

        // Check terminal for 256 color support
        if let Ok(term) = std::env::var("TERM") {
            if term.contains("256color") || term.contains("kitty") || term.contains("alacritty") {
                return Self::Extended;
            }
        }

        Self::Basic
    }

    /// Check if 256 colors are supported
    #[must_use]
    pub const fn supports_256(&self) -> bool {
        matches!(self, Self::Extended | Self::TrueColor)
    }

    /// Check if truecolor is supported
    #[must_use]
    pub const fn supports_truecolor(&self) -> bool {
        matches!(self, Self::TrueColor)
    }

    /// Check if any color is supported
    #[must_use]
    pub const fn has_color(&self) -> bool {
        !matches!(self, Self::None)
    }
}

impl Default for ColorSupport {
    fn default() -> Self {
        Self::detect()
    }
}

// ============================================================================
// Semantic Colors
// ============================================================================

/// Semantic color palette for ms CLI
///
/// Colors are organized by their semantic meaning, not their visual appearance.
/// This allows consistent theming across the CLI.
pub struct MsColors;

impl MsColors {
    // === Layer Colors (skill sources) ===
    /// Base layer skills (system defaults)
    pub const LAYER_BASE: colored::Color = colored::Color::Blue;
    /// Organization layer skills
    pub const LAYER_ORG: colored::Color = colored::Color::Green;
    /// Project-specific skills
    pub const LAYER_PROJECT: colored::Color = colored::Color::Yellow;
    /// User-customized skills
    pub const LAYER_USER: colored::Color = colored::Color::Magenta;

    // === Status Colors ===
    /// Success status
    pub const SUCCESS: colored::Color = colored::Color::Green;
    /// Warning status
    pub const WARNING: colored::Color = colored::Color::Yellow;
    /// Error status
    pub const ERROR: colored::Color = colored::Color::Red;
    /// Informational status
    pub const INFO: colored::Color = colored::Color::Cyan;

    // === UI Element Colors ===
    /// Muted/secondary text
    pub const MUTED: colored::Color = colored::Color::BrightBlack;
    /// Highlighted/emphasized text
    pub const HIGHLIGHT: colored::Color = colored::Color::White;
    /// Accent color for special elements
    pub const ACCENT: colored::Color = colored::Color::BrightBlue;
    /// Links and URLs
    pub const LINK: colored::Color = colored::Color::BrightCyan;

    // === Score Colors ===
    /// High score (>= 0.8)
    pub const SCORE_HIGH: colored::Color = colored::Color::Green;
    /// Medium score (>= 0.5)
    pub const SCORE_MED: colored::Color = colored::Color::Yellow;
    /// Low score (< 0.5)
    pub const SCORE_LOW: colored::Color = colored::Color::Red;

    // === Diff Colors ===
    /// Added content
    pub const DIFF_ADD: colored::Color = colored::Color::Green;
    /// Removed content
    pub const DIFF_REMOVE: colored::Color = colored::Color::Red;
    /// Changed content
    pub const DIFF_CHANGE: colored::Color = colored::Color::Yellow;

    // === Priority Colors ===
    /// P0/Critical priority
    pub const PRIORITY_P0: colored::Color = colored::Color::Red;
    /// P1/High priority
    pub const PRIORITY_P1: colored::Color = colored::Color::Yellow;
    /// P2/Medium priority
    pub const PRIORITY_P2: colored::Color = colored::Color::Cyan;
    /// P3/Low priority
    pub const PRIORITY_P3: colored::Color = colored::Color::Blue;
    /// P4/Backlog priority
    pub const PRIORITY_P4: colored::Color = colored::Color::BrightBlack;
}

// ============================================================================
// Pre-built Styles
// ============================================================================

/// Pre-built styles for common use cases
pub struct MsStyles;

impl MsStyles {
    // === Status Styles ===

    /// Style for success messages (green, bold)
    pub fn success<S: AsRef<str>>(text: S) -> ColoredString {
        text.as_ref().green().bold()
    }

    /// Style for error messages (red, bold)
    pub fn error<S: AsRef<str>>(text: S) -> ColoredString {
        text.as_ref().red().bold()
    }

    /// Style for warning messages (yellow)
    pub fn warning<S: AsRef<str>>(text: S) -> ColoredString {
        text.as_ref().yellow()
    }

    /// Style for info messages (cyan)
    pub fn info<S: AsRef<str>>(text: S) -> ColoredString {
        text.as_ref().cyan()
    }

    /// Style for muted/secondary text (dim)
    pub fn muted<S: AsRef<str>>(text: S) -> ColoredString {
        text.as_ref().dimmed()
    }

    /// Style for highlighted text (white, bold)
    pub fn highlight<S: AsRef<str>>(text: S) -> ColoredString {
        text.as_ref().white().bold()
    }

    /// Style for emphasized text (bold only)
    pub fn bold<S: AsRef<str>>(text: S) -> ColoredString {
        text.as_ref().bold()
    }

    // === Layer Styles ===

    /// Style for layer name based on layer type
    pub fn layer<S: AsRef<str>>(text: S, layer: &str) -> ColoredString {
        match layer.to_lowercase().as_str() {
            "base" | "system" => text.as_ref().blue(),
            "org" | "organization" | "global" => text.as_ref().green(),
            "project" => text.as_ref().yellow(),
            "user" | "local" | "session" => text.as_ref().magenta(),
            _ => text.as_ref().dimmed(),
        }
    }

    // === Score Styles ===

    /// Style for score value based on score level
    pub fn score<S: AsRef<str>>(text: S, value: f64) -> ColoredString {
        if value >= 0.8 {
            text.as_ref().green()
        } else if value >= 0.5 {
            text.as_ref().yellow()
        } else {
            text.as_ref().red()
        }
    }

    /// Style for score with bold if high
    pub fn score_bold<S: AsRef<str>>(text: S, value: f64) -> ColoredString {
        if value >= 0.8 {
            text.as_ref().green().bold()
        } else if value >= 0.5 {
            text.as_ref().yellow()
        } else {
            text.as_ref().red()
        }
    }

    // === Priority Styles ===

    /// Style for priority label
    pub fn priority<S: AsRef<str>>(text: S, priority: u8) -> ColoredString {
        match priority {
            0 => text.as_ref().red().bold(),
            1 => text.as_ref().yellow().bold(),
            2 => text.as_ref().cyan(),
            3 => text.as_ref().blue(),
            _ => text.as_ref().dimmed(),
        }
    }

    // === Diff Styles ===

    /// Style for added content
    pub fn diff_add<S: AsRef<str>>(text: S) -> ColoredString {
        text.as_ref().green()
    }

    /// Style for removed content
    pub fn diff_remove<S: AsRef<str>>(text: S) -> ColoredString {
        text.as_ref().red()
    }

    /// Style for changed content
    pub fn diff_change<S: AsRef<str>>(text: S) -> ColoredString {
        text.as_ref().yellow()
    }

    // === Symbol Styles ===

    /// Success checkmark (✓)
    pub fn check() -> ColoredString {
        "✓".green().bold()
    }

    /// Error X (✗)
    pub fn cross() -> ColoredString {
        "✗".red().bold()
    }

    /// Warning indicator (!)
    pub fn exclaim() -> ColoredString {
        "!".yellow()
    }

    /// Info indicator (ℹ)
    pub fn info_symbol() -> ColoredString {
        "ℹ".cyan()
    }

    /// Bullet point (•)
    pub fn bullet() -> ColoredString {
        "•".dimmed()
    }

    /// Arrow (→)
    pub fn arrow() -> ColoredString {
        "→".dimmed()
    }

    // === Skill ID and Name Styles ===

    /// Style for skill IDs
    pub fn skill_id<S: AsRef<str>>(text: S) -> ColoredString {
        text.as_ref().cyan()
    }

    /// Style for skill names
    pub fn skill_name<S: AsRef<str>>(text: S) -> ColoredString {
        text.as_ref().white().bold()
    }

    /// Style for skill descriptions
    pub fn skill_desc<S: AsRef<str>>(text: S) -> ColoredString {
        text.as_ref().normal()
    }

    // === Command and Code Styles ===

    /// Style for command names
    pub fn command<S: AsRef<str>>(text: S) -> ColoredString {
        text.as_ref().cyan().bold()
    }

    /// Style for code snippets
    pub fn code<S: AsRef<str>>(text: S) -> ColoredString {
        text.as_ref().bright_white()
    }

    /// Style for file paths
    pub fn path<S: AsRef<str>>(text: S) -> ColoredString {
        text.as_ref().blue().underline()
    }

    /// Style for URLs
    pub fn url<S: AsRef<str>>(text: S) -> ColoredString {
        text.as_ref().bright_cyan().underline()
    }
}

// ============================================================================
// Conditional Styling
// ============================================================================

/// Apply a style conditionally based on color support
///
/// If colors are not supported, returns the plain text.
/// Otherwise, applies the style function.
///
/// # Examples
///
/// ```rust,ignore
/// use crate::cli::colors::{styled, MsStyles, ColorSupport};
///
/// let support = ColorSupport::detect();
/// let text = styled("success", MsStyles::success, support);
/// ```
pub fn styled<S, F>(text: S, style_fn: F, support: ColorSupport) -> String
where
    S: AsRef<str>,
    F: FnOnce(&str) -> ColoredString,
{
    if support.has_color() {
        style_fn(text.as_ref()).to_string()
    } else {
        text.as_ref().to_string()
    }
}

/// Apply a colored string conditionally based on color support
///
/// If colors are not supported, returns the plain text.
/// Otherwise, returns the colored string as-is.
pub fn with_color<S: AsRef<str>>(colored: ColoredString, plain: S, support: ColorSupport) -> String {
    if support.has_color() {
        colored.to_string()
    } else {
        plain.as_ref().to_string()
    }
}

// ============================================================================
// Formatting Helpers
// ============================================================================

/// Format a score with appropriate color and precision
pub fn format_score(value: f64, support: ColorSupport) -> String {
    let text = format!("{:.2}", value);
    if support.has_color() {
        MsStyles::score(&text, value).to_string()
    } else {
        text
    }
}

/// Format a layer name with appropriate color
pub fn format_layer(layer: &str, support: ColorSupport) -> String {
    if support.has_color() {
        MsStyles::layer(layer, layer).to_string()
    } else {
        layer.to_string()
    }
}

/// Format a status indicator (check/cross/exclaim)
pub fn format_status(success: Option<bool>, support: ColorSupport) -> String {
    match success {
        Some(true) => with_color(MsStyles::check(), "✓", support),
        Some(false) => with_color(MsStyles::cross(), "✗", support),
        None => with_color(MsStyles::exclaim(), "!", support),
    }
}

/// Format a priority label (P0-P4)
pub fn format_priority(priority: u8, support: ColorSupport) -> String {
    let label = format!("P{}", priority);
    if support.has_color() {
        MsStyles::priority(&label, priority).to_string()
    } else {
        label
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_support_none_behavior() {
        // Test ColorSupport::None behavior directly
        let support = ColorSupport::None;
        assert_eq!(support, ColorSupport::None);
        assert!(!support.has_color());
        assert!(!support.supports_256());
        assert!(!support.supports_truecolor());
    }

    #[test]
    fn test_color_support_has_color() {
        assert!(ColorSupport::Basic.has_color());
        assert!(ColorSupport::Extended.has_color());
        assert!(ColorSupport::TrueColor.has_color());
        assert!(!ColorSupport::None.has_color());
    }

    #[test]
    fn test_color_support_256() {
        assert!(!ColorSupport::None.supports_256());
        assert!(!ColorSupport::Basic.supports_256());
        assert!(ColorSupport::Extended.supports_256());
        assert!(ColorSupport::TrueColor.supports_256());
    }

    #[test]
    fn test_color_support_truecolor() {
        assert!(!ColorSupport::None.supports_truecolor());
        assert!(!ColorSupport::Basic.supports_truecolor());
        assert!(!ColorSupport::Extended.supports_truecolor());
        assert!(ColorSupport::TrueColor.supports_truecolor());
    }

    #[test]
    fn test_styled_with_no_color() {
        let text = styled("hello", |s| s.green(), ColorSupport::None);
        assert_eq!(text, "hello");
    }

    #[test]
    fn test_styled_with_color() {
        let text = styled("hello", |s| s.green(), ColorSupport::Basic);
        // The text content should be preserved
        // Note: colored crate may not emit ANSI in non-TTY environments
        assert!(text.contains("hello"));
    }

    #[test]
    fn test_score_style_gradient() {
        // Verify the score function doesn't panic and preserves content
        let high = MsStyles::score("0.95", 0.95);
        assert!(high.to_string().contains("0.95"));

        let med = MsStyles::score("0.60", 0.60);
        assert!(med.to_string().contains("0.60"));

        let low = MsStyles::score("0.30", 0.30);
        assert!(low.to_string().contains("0.30"));
    }

    #[test]
    fn test_layer_styles_contain_text() {
        // Verify layer function preserves text content
        let base = MsStyles::layer("base", "base").to_string();
        let org = MsStyles::layer("org", "org").to_string();
        let project = MsStyles::layer("project", "project").to_string();
        let user = MsStyles::layer("user", "user").to_string();

        // Each should contain its text
        assert!(base.contains("base"));
        assert!(org.contains("org"));
        assert!(project.contains("project"));
        assert!(user.contains("user"));
    }

    #[test]
    fn test_format_score() {
        let score = format_score(0.85, ColorSupport::None);
        assert_eq!(score, "0.85");

        let score_color = format_score(0.85, ColorSupport::Basic);
        // Content should be preserved
        assert!(score_color.contains("0.85"));
    }

    #[test]
    fn test_format_status() {
        assert!(format_status(Some(true), ColorSupport::None).contains("✓"));
        assert!(format_status(Some(false), ColorSupport::None).contains("✗"));
        assert!(format_status(None, ColorSupport::None).contains("!"));
    }

    #[test]
    fn test_format_priority() {
        assert!(format_priority(0, ColorSupport::None).contains("P0"));
        assert!(format_priority(1, ColorSupport::None).contains("P1"));
        assert!(format_priority(2, ColorSupport::None).contains("P2"));
    }

    #[test]
    fn test_symbol_styles() {
        // Just verify they don't panic and produce expected output
        assert!(MsStyles::check().to_string().contains("✓"));
        assert!(MsStyles::cross().to_string().contains("✗"));
        assert!(MsStyles::exclaim().to_string().contains("!"));
        assert!(MsStyles::bullet().to_string().contains("•"));
        assert!(MsStyles::arrow().to_string().contains("→"));
    }

    #[test]
    fn test_with_color() {
        let colored = "test".green();

        // With color support - content should be preserved
        let result = with_color(colored.clone(), "test", ColorSupport::Basic);
        assert!(result.contains("test"));

        // Without color support - should return plain text
        let result = with_color(colored, "test", ColorSupport::None);
        assert_eq!(result, "test");
    }
}
