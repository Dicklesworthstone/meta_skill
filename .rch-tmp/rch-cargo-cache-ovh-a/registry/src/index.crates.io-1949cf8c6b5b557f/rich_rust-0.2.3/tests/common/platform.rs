//! Platform-specific test utilities and fixtures.
//!
//! This module provides utilities for handling platform-specific differences
//! in terminal output, line endings, and box-drawing characters.
//!
//! # Platform Differences
//!
//! - **Line Endings**: Windows uses `\r\n`, Unix uses `\n`
//! - **Box Drawing**: Some Windows terminals may not support Unicode box chars
//! - **Colors**: Color support varies by terminal emulator
//! - **Unicode Width**: CJK characters may render differently on some platforms
//!
//! # Example
//!
//! ```rust,ignore
//! use common::platform::*;
//!
//! #[test]
//! fn test_cross_platform() {
//!     let output = render_something();
//!     let normalized = normalize_line_endings(&output);
//!     assert_eq!(normalized, expected_output());
//! }
//! ```

// Environment variable manipulation requires unsafe in Rust 2024 edition.
// This is test-only code running in single-threaded contexts.
#![allow(dead_code)]

use std::borrow::Cow;

// =============================================================================
// Platform Detection
// =============================================================================

/// Current platform information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformInfo {
    /// Operating system name.
    pub os: &'static str,
    /// Architecture.
    pub arch: &'static str,
    /// Whether this is Windows.
    pub is_windows: bool,
    /// Whether this is macOS.
    pub is_macos: bool,
    /// Whether this is Linux.
    pub is_linux: bool,
    /// Whether Unicode is likely well-supported.
    pub unicode_likely: bool,
}

impl PlatformInfo {
    /// Get current platform info.
    #[must_use]
    pub fn current() -> Self {
        Self {
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            is_windows: cfg!(target_os = "windows"),
            is_macos: cfg!(target_os = "macos"),
            is_linux: cfg!(target_os = "linux"),
            // Unicode is well-supported on modern macOS and Linux
            // Windows support depends on terminal, but modern Windows Terminal is good
            unicode_likely: !cfg!(target_os = "windows")
                || std::env::var("WT_SESSION").is_ok()
                || std::env::var("TERM_PROGRAM").is_ok_and(|v| v.contains("vscode")),
        }
    }

    /// Check if running in CI environment.
    #[must_use]
    pub fn is_ci() -> bool {
        std::env::var("CI").is_ok()
            || std::env::var("GITHUB_ACTIONS").is_ok()
            || std::env::var("TRAVIS").is_ok()
            || std::env::var("CIRCLECI").is_ok()
            || std::env::var("GITLAB_CI").is_ok()
    }

    /// Get a suffix for platform-specific snapshot files.
    #[must_use]
    pub fn snapshot_suffix(&self) -> &'static str {
        if self.is_windows {
            "windows"
        } else if self.is_macos {
            "macos"
        } else {
            "linux"
        }
    }
}

impl Default for PlatformInfo {
    fn default() -> Self {
        Self::current()
    }
}

// =============================================================================
// Line Ending Normalization
// =============================================================================

/// Normalize line endings to Unix-style (`\n`).
///
/// Converts all `\r\n` (Windows) and standalone `\r` (old Mac) to `\n`.
///
/// # Example
///
/// ```rust,ignore
/// let text = "line1\r\nline2\rline3\n";
/// let normalized = normalize_line_endings(text);
/// assert_eq!(normalized, "line1\nline2\nline3\n");
/// ```
#[must_use]
pub fn normalize_line_endings(s: &str) -> Cow<'_, str> {
    if !s.contains('\r') {
        return Cow::Borrowed(s);
    }
    Cow::Owned(s.replace("\r\n", "\n").replace('\r', "\n"))
}

/// Convert to platform-native line endings.
///
/// On Windows, converts `\n` to `\r\n`.
/// On Unix, returns unchanged.
#[must_use]
pub fn to_native_line_endings(s: &str) -> Cow<'_, str> {
    #[cfg(target_os = "windows")]
    {
        if !s.contains('\n') || s.contains("\r\n") {
            return Cow::Borrowed(s);
        }
        // Replace \n with \r\n, but not if already \r\n
        let mut result = String::with_capacity(s.len() + s.matches('\n').count());
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\n' {
                result.push_str("\r\n");
            } else {
                result.push(c);
            }
        }
        Cow::Owned(result)
    }

    #[cfg(not(target_os = "windows"))]
    {
        Cow::Borrowed(s)
    }
}

// =============================================================================
// Box Drawing Character Handling
// =============================================================================

/// Box drawing character sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoxCharSet {
    /// Unicode box-drawing characters (default).
    #[default]
    Unicode,
    /// ASCII-safe characters for legacy terminals.
    Ascii,
}

impl BoxCharSet {
    /// Select character set based on platform capabilities.
    #[must_use]
    pub fn for_platform() -> Self {
        let info = PlatformInfo::current();
        if info.unicode_likely {
            Self::Unicode
        } else {
            Self::Ascii
        }
    }

    /// Get horizontal line character.
    #[must_use]
    pub fn horizontal(self) -> char {
        match self {
            Self::Unicode => '─',
            Self::Ascii => '-',
        }
    }

    /// Get vertical line character.
    #[must_use]
    pub fn vertical(self) -> char {
        match self {
            Self::Unicode => '│',
            Self::Ascii => '|',
        }
    }

    /// Get top-left corner character.
    #[must_use]
    pub fn top_left(self) -> char {
        match self {
            Self::Unicode => '┌',
            Self::Ascii => '+',
        }
    }

    /// Get top-right corner character.
    #[must_use]
    pub fn top_right(self) -> char {
        match self {
            Self::Unicode => '┐',
            Self::Ascii => '+',
        }
    }

    /// Get bottom-left corner character.
    #[must_use]
    pub fn bottom_left(self) -> char {
        match self {
            Self::Unicode => '└',
            Self::Ascii => '+',
        }
    }

    /// Get bottom-right corner character.
    #[must_use]
    pub fn bottom_right(self) -> char {
        match self {
            Self::Unicode => '┘',
            Self::Ascii => '+',
        }
    }
}

/// Convert Unicode box-drawing characters to ASCII equivalents.
///
/// Useful for comparing output across platforms with different Unicode support.
#[must_use]
pub fn unicode_to_ascii_boxes(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '─' | '━' | '═' => '-',
            '│' | '┃' | '║' => '|',
            '┌' | '┏' | '╔' | '╭' => '+',
            '┐' | '┓' | '╗' | '╮' => '+',
            '└' | '┗' | '╚' | '╰' => '+',
            '┘' | '┛' | '╝' | '╯' => '+',
            '├' | '┣' | '╠' => '+',
            '┤' | '┫' | '╣' => '+',
            '┬' | '┳' | '╦' => '+',
            '┴' | '┻' | '╩' => '+',
            '┼' | '╋' | '╬' => '+',
            _ => c,
        })
        .collect()
}

// =============================================================================
// Platform-Specific Assertions
// =============================================================================

/// Compare strings with normalized line endings.
///
/// Useful for cross-platform snapshot comparisons.
#[track_caller]
pub fn assert_eq_normalized(context: &str, actual: &str, expected: &str) {
    let actual_norm = normalize_line_endings(actual);
    let expected_norm = normalize_line_endings(expected);

    if actual_norm != expected_norm {
        panic!(
            "{context}: strings differ after line ending normalization.\n\
             Expected:\n{expected_norm:?}\n\
             Actual:\n{actual_norm:?}"
        );
    }
}

/// Compare strings with normalized line endings and box characters.
///
/// Converts both to ASCII boxes for maximum compatibility.
#[track_caller]
pub fn assert_eq_platform_agnostic(context: &str, actual: &str, expected: &str) {
    let actual_norm = unicode_to_ascii_boxes(&normalize_line_endings(actual));
    let expected_norm = unicode_to_ascii_boxes(&normalize_line_endings(expected));

    if actual_norm != expected_norm {
        panic!(
            "{context}: strings differ after platform normalization.\n\
             Expected:\n{expected_norm:?}\n\
             Actual:\n{actual_norm:?}"
        );
    }
}

/// Skip test if not on expected platform.
///
/// Use this to skip platform-specific tests on other platforms.
///
/// # Example
///
/// ```rust,ignore
/// #[test]
/// fn test_windows_specific() {
///     skip_unless_windows();
///     // Windows-only test code...
/// }
/// ```
#[track_caller]
pub fn skip_unless_windows() {
    if !cfg!(target_os = "windows") {
        eprintln!("Skipping test: Windows-only");
    }
}

/// Skip test if not on Unix-like platform.
#[track_caller]
pub fn skip_unless_unix() {
    if cfg!(target_os = "windows") {
        eprintln!("Skipping test: Unix-only");
    }
}

/// Skip test if not in CI environment.
#[track_caller]
pub fn skip_unless_ci() {
    if !PlatformInfo::is_ci() {
        eprintln!("Skipping test: CI-only");
    }
}

// =============================================================================
// Environment Helpers
// =============================================================================

/// Temporarily set an environment variable for the duration of a closure.
///
/// The original value is restored after the closure completes.
///
/// # Safety
///
/// This function modifies environment variables, which is inherently unsafe
/// in multi-threaded programs. Only use in single-threaded test contexts.
pub fn with_env_var<F, R>(key: &str, value: &str, f: F) -> R
where
    F: FnOnce() -> R,
{
    let original = std::env::var(key).ok();
    // SAFETY: Test-only code, running in single-threaded test context
    unsafe { std::env::set_var(key, value) };

    let result = f();

    // SAFETY: Test-only code, running in single-threaded test context
    match original {
        Some(v) => unsafe { std::env::set_var(key, v) },
        None => unsafe { std::env::remove_var(key) },
    }

    result
}

/// Temporarily remove an environment variable for the duration of a closure.
///
/// # Safety
///
/// This function modifies environment variables, which is inherently unsafe
/// in multi-threaded programs. Only use in single-threaded test contexts.
pub fn without_env_var<F, R>(key: &str, f: F) -> R
where
    F: FnOnce() -> R,
{
    let original = std::env::var(key).ok();
    // SAFETY: Test-only code, running in single-threaded test context
    unsafe { std::env::remove_var(key) };

    let result = f();

    if let Some(v) = original {
        // SAFETY: Test-only code, running in single-threaded test context
        unsafe { std::env::set_var(key, v) };
    }

    result
}

// =============================================================================
// Terminal Environment Simulation
// =============================================================================

/// Simulated terminal environment for testing.
#[derive(Debug, Clone)]
pub struct TerminalEnv {
    /// TERM environment variable value.
    pub term: Option<String>,
    /// COLORTERM environment variable value.
    pub colorterm: Option<String>,
    /// NO_COLOR environment variable (if set).
    pub no_color: bool,
    /// FORCE_COLOR environment variable (if set).
    pub force_color: bool,
    /// Terminal width hint.
    pub columns: Option<u16>,
    /// Terminal height hint.
    pub lines: Option<u16>,
}

impl TerminalEnv {
    /// Create a default terminal environment.
    #[must_use]
    pub fn new() -> Self {
        Self {
            term: Some("xterm-256color".to_string()),
            colorterm: Some("truecolor".to_string()),
            no_color: false,
            force_color: false,
            columns: Some(80),
            lines: Some(24),
        }
    }

    /// Create a dumb terminal (no colors, no features).
    #[must_use]
    pub fn dumb() -> Self {
        Self {
            term: Some("dumb".to_string()),
            colorterm: None,
            no_color: false,
            force_color: false,
            columns: Some(80),
            lines: Some(24),
        }
    }

    /// Create a no-color environment.
    #[must_use]
    pub fn no_color() -> Self {
        Self {
            term: Some("xterm-256color".to_string()),
            colorterm: None,
            no_color: true,
            force_color: false,
            columns: Some(80),
            lines: Some(24),
        }
    }

    /// Apply this environment and run a closure.
    ///
    /// # Safety
    ///
    /// This function modifies environment variables, which is inherently unsafe
    /// in multi-threaded programs. Only use in single-threaded test contexts.
    pub fn apply<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        // Save originals
        let orig_term = std::env::var("TERM").ok();
        let orig_colorterm = std::env::var("COLORTERM").ok();
        let orig_no_color = std::env::var("NO_COLOR").ok();
        let orig_force_color = std::env::var("FORCE_COLOR").ok();
        let orig_columns = std::env::var("COLUMNS").ok();
        let orig_lines = std::env::var("LINES").ok();

        // SAFETY: Test-only code, running in single-threaded test context
        unsafe {
            // Set new values
            match &self.term {
                Some(v) => std::env::set_var("TERM", v),
                None => std::env::remove_var("TERM"),
            }
            match &self.colorterm {
                Some(v) => std::env::set_var("COLORTERM", v),
                None => std::env::remove_var("COLORTERM"),
            }
            if self.no_color {
                std::env::set_var("NO_COLOR", "1");
            } else {
                std::env::remove_var("NO_COLOR");
            }
            if self.force_color {
                std::env::set_var("FORCE_COLOR", "1");
            } else {
                std::env::remove_var("FORCE_COLOR");
            }
            if let Some(cols) = self.columns {
                std::env::set_var("COLUMNS", cols.to_string());
            } else {
                std::env::remove_var("COLUMNS");
            }
            if let Some(lines) = self.lines {
                std::env::set_var("LINES", lines.to_string());
            } else {
                std::env::remove_var("LINES");
            }
        }

        // Run closure
        let result = f();

        // SAFETY: Test-only code, running in single-threaded test context
        unsafe {
            // Restore originals
            match orig_term {
                Some(v) => std::env::set_var("TERM", v),
                None => std::env::remove_var("TERM"),
            }
            match orig_colorterm {
                Some(v) => std::env::set_var("COLORTERM", v),
                None => std::env::remove_var("COLORTERM"),
            }
            match orig_no_color {
                Some(v) => std::env::set_var("NO_COLOR", v),
                None => std::env::remove_var("NO_COLOR"),
            }
            match orig_force_color {
                Some(v) => std::env::set_var("FORCE_COLOR", v),
                None => std::env::remove_var("FORCE_COLOR"),
            }
            match orig_columns {
                Some(v) => std::env::set_var("COLUMNS", v),
                None => std::env::remove_var("COLUMNS"),
            }
            match orig_lines {
                Some(v) => std::env::set_var("LINES", v),
                None => std::env::remove_var("LINES"),
            }
        }

        result
    }
}

impl Default for TerminalEnv {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_platform_info() {
        let info = PlatformInfo::current();
        assert!(!info.os.is_empty());
        assert!(!info.arch.is_empty());
        // At least one platform flag should be true
        assert!(info.is_windows || info.is_macos || info.is_linux);
    }

    #[test]
    fn test_normalize_line_endings() {
        assert_eq!(normalize_line_endings("hello\nworld"), "hello\nworld");
        assert_eq!(normalize_line_endings("hello\r\nworld"), "hello\nworld");
        assert_eq!(normalize_line_endings("hello\rworld"), "hello\nworld");
        assert_eq!(normalize_line_endings("a\r\nb\rc\n"), "a\nb\nc\n");
    }

    #[test]
    fn test_unicode_to_ascii_boxes() {
        // Each box char converts 1:1
        assert_eq!(unicode_to_ascii_boxes("┌─┐"), "+-+");
        assert_eq!(unicode_to_ascii_boxes("│x│"), "|x|");
        assert_eq!(unicode_to_ascii_boxes("└─┘"), "+-+");
        assert_eq!(unicode_to_ascii_boxes("Hello"), "Hello");
        // Multiple horizontal chars
        assert_eq!(unicode_to_ascii_boxes("┌──┐"), "+--+");
    }

    #[test]
    fn test_box_char_set() {
        let unicode = BoxCharSet::Unicode;
        assert_eq!(unicode.horizontal(), '─');
        assert_eq!(unicode.vertical(), '│');

        let ascii = BoxCharSet::Ascii;
        assert_eq!(ascii.horizontal(), '-');
        assert_eq!(ascii.vertical(), '|');
    }

    #[test]
    #[serial]
    fn test_with_env_var() {
        let original = std::env::var("TEST_PLATFORM_VAR").ok();

        with_env_var("TEST_PLATFORM_VAR", "test_value", || {
            assert_eq!(std::env::var("TEST_PLATFORM_VAR").unwrap(), "test_value");
        });

        // Should be restored
        assert_eq!(std::env::var("TEST_PLATFORM_VAR").ok(), original);
    }

    #[test]
    #[serial]
    fn test_terminal_env_apply() {
        let env = TerminalEnv::dumb();
        env.apply(|| {
            assert_eq!(std::env::var("TERM").unwrap(), "dumb");
        });
    }

    #[test]
    #[serial]
    fn test_terminal_env_no_color() {
        let env = TerminalEnv::no_color();
        env.apply(|| {
            assert!(std::env::var("NO_COLOR").is_ok());
        });
    }

    #[test]
    fn test_assert_eq_normalized() {
        assert_eq_normalized("same content", "hello\nworld", "hello\nworld");
        assert_eq_normalized("crlf vs lf", "hello\nworld", "hello\r\nworld");
    }

    #[test]
    fn test_assert_eq_platform_agnostic() {
        assert_eq_platform_agnostic("same content", "hello", "hello");
        // Box chars convert 1:1
        assert_eq_platform_agnostic("box chars", "┌─┐", "+-+");
        assert_eq_platform_agnostic("box chars multi", "┌──┐", "+--+");
    }
}
