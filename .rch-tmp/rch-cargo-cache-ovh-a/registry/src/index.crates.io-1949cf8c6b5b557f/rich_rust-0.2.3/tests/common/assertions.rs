//! Logged assertion helpers for rich_rust tests.
//!
//! These functions wrap standard assertions with tracing logs,
//! providing detailed context when assertions fail.
//!
//! Note: Not all helpers are currently used, but they're available for future tests.

#![allow(dead_code)]

use std::fmt::Debug;

/// Assert equality with detailed logging.
///
/// Logs both values at debug level before comparison,
/// making it easier to diagnose failures in CI logs.
///
/// # Example
///
/// ```rust,ignore
/// assert_eq_logged("color count", colors.len(), 16);
/// ```
#[track_caller]
pub fn assert_eq_logged<T: PartialEq + Debug>(context: &str, actual: T, expected: T) {
    tracing::debug!(
        context = context,
        expected = ?expected,
        actual = ?actual,
        "asserting equality"
    );

    if actual != expected {
        tracing::error!(
            context = context,
            expected = ?expected,
            actual = ?actual,
            "assertion failed: values not equal"
        );
    }

    assert_eq!(
        actual, expected,
        "{context}: expected {expected:?}, got {actual:?}"
    );

    tracing::trace!(context = context, "assertion passed");
}

/// Assert that a value is true with logging.
///
/// # Example
///
/// ```rust,ignore
/// assert_true_logged("color is valid", color.is_valid());
/// ```
#[track_caller]
pub fn assert_true_logged(context: &str, value: bool) {
    tracing::debug!(context = context, value = value, "asserting true");

    if !value {
        tracing::error!(
            context = context,
            value = value,
            "assertion failed: expected true"
        );
    }

    assert!(value, "{context}: expected true, got false");

    tracing::trace!(context = context, "assertion passed");
}

/// Assert that a value is false with logging.
///
/// # Example
///
/// ```rust,ignore
/// assert_false_logged("should not be terminal", is_terminal);
/// ```
#[track_caller]
pub fn assert_false_logged(context: &str, value: bool) {
    tracing::debug!(context = context, value = value, "asserting false");

    if value {
        tracing::error!(
            context = context,
            value = value,
            "assertion failed: expected false"
        );
    }

    assert!(!value, "{context}: expected false, got true");

    tracing::trace!(context = context, "assertion passed");
}

/// Assert that a Result is Ok with logging.
///
/// Returns the Ok value for further assertions.
///
/// # Example
///
/// ```rust,ignore
/// let color = assert_ok_logged("parse color", Color::parse("#ff0000"));
/// ```
#[track_caller]
pub fn assert_ok_logged<T: Debug, E: Debug>(context: &str, result: Result<T, E>) -> T {
    tracing::debug!(context = context, result = ?result, "asserting Ok");

    match result {
        Ok(value) => {
            tracing::trace!(context = context, value = ?value, "assertion passed: got Ok");
            value
        }
        Err(ref e) => {
            tracing::error!(context = context, error = ?e, "assertion failed: expected Ok, got Err");
            panic!("{context}: expected Ok, got Err({e:?})");
        }
    }
}

/// Assert that a Result is Err with logging.
///
/// Returns the Err value for further assertions.
///
/// # Example
///
/// ```rust,ignore
/// let error = assert_err_logged("invalid color", Color::parse("not-a-color"));
/// ```
#[track_caller]
pub fn assert_err_logged<T: Debug, E: Debug>(context: &str, result: Result<T, E>) -> E {
    tracing::debug!(context = context, result = ?result, "asserting Err");

    match result {
        Err(e) => {
            tracing::trace!(context = context, error = ?e, "assertion passed: got Err");
            e
        }
        Ok(ref value) => {
            tracing::error!(
                context = context,
                value = ?value,
                "assertion failed: expected Err, got Ok"
            );
            panic!("{context}: expected Err, got Ok({value:?})");
        }
    }
}

/// Assert that an Option is Some with logging.
///
/// Returns the inner value for further assertions.
///
/// # Example
///
/// ```rust,ignore
/// let triplet = assert_some_logged("color triplet", color.triplet());
/// ```
#[track_caller]
pub fn assert_some_logged<T: Debug>(context: &str, option: Option<T>) -> T {
    tracing::debug!(context = context, option = ?option, "asserting Some");

    match option {
        Some(value) => {
            tracing::trace!(context = context, value = ?value, "assertion passed: got Some");
            value
        }
        None => {
            tracing::error!(
                context = context,
                "assertion failed: expected Some, got None"
            );
            panic!("{context}: expected Some, got None");
        }
    }
}

/// Assert that an Option is None with logging.
///
/// # Example
///
/// ```rust,ignore
/// assert_none_logged("default has no triplet", default_color.triplet());
/// ```
#[track_caller]
pub fn assert_none_logged<T: Debug>(context: &str, option: Option<T>) {
    tracing::debug!(context = context, option = ?option, "asserting None");

    if let Some(ref value) = option {
        tracing::error!(
            context = context,
            value = ?value,
            "assertion failed: expected None, got Some"
        );
        panic!("{context}: expected None, got Some({value:?})");
    }

    tracing::trace!(context = context, "assertion passed");
}

/// Assert that a string contains a substring with logging.
///
/// # Example
///
/// ```rust,ignore
/// assert_contains_logged("ansi output", &output, "\x1b[31m");
/// ```
#[track_caller]
pub fn assert_contains_logged(context: &str, haystack: &str, needle: &str) {
    tracing::debug!(
        context = context,
        haystack_len = haystack.len(),
        needle = needle,
        "asserting contains"
    );

    if !haystack.contains(needle) {
        tracing::error!(
            context = context,
            haystack = haystack,
            needle = needle,
            "assertion failed: string does not contain substring"
        );
        panic!(
            "{context}: expected string to contain {needle:?}, but it doesn't.\nString: {haystack:?}"
        );
    }

    tracing::trace!(context = context, "assertion passed");
}

/// Assert that a string does not contain a substring with logging.
///
/// # Example
///
/// ```rust,ignore
/// assert_not_contains_logged("plain output", &output, "\x1b[");
/// ```
#[track_caller]
pub fn assert_not_contains_logged(context: &str, haystack: &str, needle: &str) {
    tracing::debug!(
        context = context,
        haystack_len = haystack.len(),
        needle = needle,
        "asserting not contains"
    );

    if haystack.contains(needle) {
        tracing::error!(
            context = context,
            haystack = haystack,
            needle = needle,
            "assertion failed: string contains unwanted substring"
        );
        panic!(
            "{context}: expected string to not contain {needle:?}, but it does.\nString: {haystack:?}"
        );
    }

    tracing::trace!(context = context, "assertion passed");
}

/// Assert approximate equality for floating point values with logging.
///
/// Uses a relative epsilon for comparison.
///
/// # Example
///
/// ```rust,ignore
/// assert_approx_eq_logged("normalized red", normalized.0, 1.0, 0.001);
/// ```
#[track_caller]
pub fn assert_approx_eq_logged(context: &str, actual: f64, expected: f64, epsilon: f64) {
    tracing::debug!(
        context = context,
        expected = expected,
        actual = actual,
        epsilon = epsilon,
        "asserting approximate equality"
    );

    let diff = (actual - expected).abs();
    if diff > epsilon {
        tracing::error!(
            context = context,
            expected = expected,
            actual = actual,
            diff = diff,
            epsilon = epsilon,
            "assertion failed: values not approximately equal"
        );
        panic!("{context}: expected {expected} (within {epsilon}), got {actual} (diff: {diff})");
    }

    tracing::trace!(context = context, "assertion passed");
}

/// Assert that a slice has a specific length with logging.
///
/// # Example
///
/// ```rust,ignore
/// assert_len_logged("segments", &segments, 5);
/// ```
#[track_caller]
pub fn assert_len_logged<T>(context: &str, slice: &[T], expected_len: usize) {
    let actual_len = slice.len();
    tracing::debug!(
        context = context,
        expected_len = expected_len,
        actual_len = actual_len,
        "asserting length"
    );

    if actual_len != expected_len {
        tracing::error!(
            context = context,
            expected_len = expected_len,
            actual_len = actual_len,
            "assertion failed: unexpected length"
        );
        panic!("{context}: expected length {expected_len}, got {actual_len}");
    }

    tracing::trace!(context = context, "assertion passed");
}

// =============================================================================
// Style Verification Helpers (bd-2sa2)
// These helpers verify that ANSI styles are correctly applied in output.
// =============================================================================

/// Common ANSI SGR (Select Graphic Rendition) codes.
pub mod ansi {
    /// Bold (SGR 1)
    pub const BOLD: &str = "\x1b[1m";
    /// Dim (SGR 2)
    pub const DIM: &str = "\x1b[2m";
    /// Italic (SGR 3)
    pub const ITALIC: &str = "\x1b[3m";
    /// Underline (SGR 4)
    pub const UNDERLINE: &str = "\x1b[4m";
    /// Blink (SGR 5)
    pub const BLINK: &str = "\x1b[5m";
    /// Reverse (SGR 7)
    pub const REVERSE: &str = "\x1b[7m";
    /// Strikethrough (SGR 9)
    pub const STRIKETHROUGH: &str = "\x1b[9m";
    /// Reset all (SGR 0)
    pub const RESET: &str = "\x1b[0m";

    // Standard foreground colors (SGR 30-37)
    /// Black foreground (SGR 30)
    pub const FG_BLACK: &str = "\x1b[30m";
    /// Red foreground (SGR 31)
    pub const FG_RED: &str = "\x1b[31m";
    /// Green foreground (SGR 32)
    pub const FG_GREEN: &str = "\x1b[32m";
    /// Yellow foreground (SGR 33)
    pub const FG_YELLOW: &str = "\x1b[33m";
    /// Blue foreground (SGR 34)
    pub const FG_BLUE: &str = "\x1b[34m";
    /// Magenta foreground (SGR 35)
    pub const FG_MAGENTA: &str = "\x1b[35m";
    /// Cyan foreground (SGR 36)
    pub const FG_CYAN: &str = "\x1b[36m";
    /// White foreground (SGR 37)
    pub const FG_WHITE: &str = "\x1b[37m";

    // Standard background colors (SGR 40-47)
    /// Black background (SGR 40)
    pub const BG_BLACK: &str = "\x1b[40m";
    /// Red background (SGR 41)
    pub const BG_RED: &str = "\x1b[41m";
    /// Green background (SGR 42)
    pub const BG_GREEN: &str = "\x1b[42m";
    /// Yellow background (SGR 43)
    pub const BG_YELLOW: &str = "\x1b[43m";
    /// Blue background (SGR 44)
    pub const BG_BLUE: &str = "\x1b[44m";
    /// Magenta background (SGR 45)
    pub const BG_MAGENTA: &str = "\x1b[45m";
    /// Cyan background (SGR 46)
    pub const BG_CYAN: &str = "\x1b[46m";
    /// White background (SGR 47)
    pub const BG_WHITE: &str = "\x1b[47m";
}

/// Check if output contains any ANSI escape sequences.
///
/// Returns true if the string contains `\x1b[` (CSI sequence intro).
///
/// # Example
///
/// ```rust,ignore
/// let output = render_markup("[bold]text[/]");
/// assert!(has_ansi_codes(&output), "Expected ANSI codes in styled output");
/// ```
#[must_use]
pub fn has_ansi_codes(output: &str) -> bool {
    output.contains("\x1b[")
}

/// Assert that output contains ANSI escape sequences.
///
/// Use this to verify that styling was applied to the output.
///
/// # Example
///
/// ```rust,ignore
/// let output = render_markup("[bold]text[/]");
/// assert_has_ansi_codes("styled output", &output);
/// ```
#[track_caller]
pub fn assert_has_ansi_codes(context: &str, output: &str) {
    tracing::debug!(
        context = context,
        output_len = output.len(),
        "asserting has ANSI codes"
    );

    if !has_ansi_codes(output) {
        tracing::error!(
            context = context,
            output = output,
            "assertion failed: no ANSI codes found"
        );
        panic!("{context}: expected ANSI escape codes but found none.\nOutput: {output:?}");
    }

    tracing::trace!(context = context, "assertion passed: has ANSI codes");
}

/// Assert that output does NOT contain ANSI escape sequences.
///
/// Use this to verify that plain text rendering works correctly.
///
/// # Example
///
/// ```rust,ignore
/// let output = console.export_text("plain text");
/// assert_no_ansi_codes("plain export", &output);
/// ```
#[track_caller]
pub fn assert_no_ansi_codes(context: &str, output: &str) {
    tracing::debug!(
        context = context,
        output_len = output.len(),
        "asserting no ANSI codes"
    );

    if has_ansi_codes(output) {
        tracing::error!(
            context = context,
            output = output,
            "assertion failed: unexpected ANSI codes found"
        );
        panic!("{context}: expected no ANSI codes but found some.\nOutput: {output:?}");
    }

    tracing::trace!(context = context, "assertion passed: no ANSI codes");
}

/// Check if output contains a specific ANSI code.
///
/// # Example
///
/// ```rust,ignore
/// assert!(has_style(&output, ansi::BOLD), "Expected bold");
/// assert!(has_style(&output, ansi::FG_RED), "Expected red");
/// ```
#[must_use]
pub fn has_style(output: &str, ansi_code: &str) -> bool {
    output.contains(ansi_code)
}

/// Assert that output contains bold styling.
///
/// # Example
///
/// ```rust,ignore
/// let output = render_markup("[bold]text[/]");
/// assert_has_bold("bold output", &output);
/// ```
#[track_caller]
pub fn assert_has_bold(context: &str, output: &str) {
    tracing::debug!(context = context, "asserting has bold style");

    // Bold can be \x1b[1m or combined like \x1b[1;31m
    let has_bold = output.contains("\x1b[1m") || output.contains("\x1b[1;");

    if !has_bold {
        tracing::error!(
            context = context,
            output = output,
            "assertion failed: no bold ANSI code found"
        );
        panic!("{context}: expected bold style (SGR 1) but not found.\nOutput: {output:?}");
    }

    tracing::trace!(context = context, "assertion passed: has bold");
}

/// Assert that output contains italic styling.
///
/// # Example
///
/// ```rust,ignore
/// let output = render_markup("[italic]text[/]");
/// assert_has_italic("italic output", &output);
/// ```
#[track_caller]
pub fn assert_has_italic(context: &str, output: &str) {
    tracing::debug!(context = context, "asserting has italic style");

    // Italic is SGR 3
    let has_italic = output.contains("\x1b[3m") || output.contains(";3m") || output.contains(";3;");

    if !has_italic {
        tracing::error!(
            context = context,
            output = output,
            "assertion failed: no italic ANSI code found"
        );
        panic!("{context}: expected italic style (SGR 3) but not found.\nOutput: {output:?}");
    }

    tracing::trace!(context = context, "assertion passed: has italic");
}

/// Assert that output contains underline styling.
///
/// # Example
///
/// ```rust,ignore
/// let output = render_markup("[underline]text[/]");
/// assert_has_underline("underline output", &output);
/// ```
#[track_caller]
pub fn assert_has_underline(context: &str, output: &str) {
    tracing::debug!(context = context, "asserting has underline style");

    // Underline is SGR 4
    let has_underline =
        output.contains("\x1b[4m") || output.contains(";4m") || output.contains(";4;");

    if !has_underline {
        tracing::error!(
            context = context,
            output = output,
            "assertion failed: no underline ANSI code found"
        );
        panic!("{context}: expected underline style (SGR 4) but not found.\nOutput: {output:?}");
    }

    tracing::trace!(context = context, "assertion passed: has underline");
}

/// Assert that output contains a specific foreground color.
///
/// Checks for standard 16-color, 256-color, and true color sequences.
///
/// # Example
///
/// ```rust,ignore
/// assert_has_color("red text", &output, "red");
/// assert_has_color("green text", &output, "green");
/// ```
#[track_caller]
pub fn assert_has_color(context: &str, output: &str, color_name: &str) {
    tracing::debug!(context = context, color = color_name, "asserting has color");

    let has_color = match color_name.to_lowercase().as_str() {
        "black" => {
            output.contains("\x1b[30m")
                || output.contains("\x1b[38;5;0m")
                || output.contains("\x1b[38;2;0;0;0m")
        }
        "red" => {
            output.contains("\x1b[31m")
                || output.contains("\x1b[38;5;")
                || output.contains("\x1b[38;2;")
        }
        "green" => {
            output.contains("\x1b[32m")
                || output.contains("\x1b[38;5;")
                || output.contains("\x1b[38;2;")
        }
        "yellow" => {
            output.contains("\x1b[33m")
                || output.contains("\x1b[38;5;")
                || output.contains("\x1b[38;2;")
        }
        "blue" => {
            output.contains("\x1b[34m")
                || output.contains("\x1b[38;5;")
                || output.contains("\x1b[38;2;")
        }
        "magenta" => {
            output.contains("\x1b[35m")
                || output.contains("\x1b[38;5;")
                || output.contains("\x1b[38;2;")
        }
        "cyan" => {
            output.contains("\x1b[36m")
                || output.contains("\x1b[38;5;")
                || output.contains("\x1b[38;2;")
        }
        "white" => {
            output.contains("\x1b[37m")
                || output.contains("\x1b[38;5;15m")
                || output.contains("\x1b[38;2;")
        }
        _ => {
            // For unknown colors, just check if ANY color code is present
            output.contains("\x1b[3") || output.contains("\x1b[38;")
        }
    };

    if !has_color {
        tracing::error!(
            context = context,
            color = color_name,
            output = output,
            "assertion failed: color not found"
        );
        panic!("{context}: expected {color_name} color but not found.\nOutput: {output:?}");
    }

    tracing::trace!(
        context = context,
        color = color_name,
        "assertion passed: has color"
    );
}

/// Assert that output contains the reset sequence.
///
/// Properly styled output should reset styles after styled regions.
///
/// # Example
///
/// ```rust,ignore
/// assert_has_reset("styled output", &output);
/// ```
#[track_caller]
pub fn assert_has_reset(context: &str, output: &str) {
    tracing::debug!(context = context, "asserting has reset");

    if !output.contains(ansi::RESET) {
        tracing::error!(
            context = context,
            output = output,
            "assertion failed: no reset code found"
        );
        panic!("{context}: expected reset code (SGR 0) but not found.\nOutput: {output:?}");
    }

    tracing::trace!(context = context, "assertion passed: has reset");
}

/// Strip ANSI escape sequences from a string.
///
/// Returns plain text with all ANSI codes removed.
///
/// # Example
///
/// ```rust,ignore
/// let plain = strip_ansi("\x1b[1mBold\x1b[0m");
/// assert_eq!(plain, "Bold");
/// ```
#[must_use]
pub fn strip_ansi(s: &str) -> String {
    let re = regex::Regex::new(r"\x1b\[[0-9;]*m|\x1b\]8;;[^\x1b]*\x1b\\").unwrap();
    re.replace_all(s, "").to_string()
}

/// Assert that output contains NO raw markup tags.
///
/// Markup like `[bold]` should be parsed, not appear literally.
///
/// # Example
///
/// ```rust,ignore
/// let output = render_markup("[bold]text[/]");
/// assert_no_raw_markup("parsed output", &output);
/// ```
#[track_caller]
pub fn assert_no_raw_markup(context: &str, output: &str) {
    tracing::debug!(context = context, "asserting no raw markup tags");

    let markup_patterns = [
        "[bold]",
        "[/bold]",
        "[italic]",
        "[/italic]",
        "[underline]",
        "[/underline]",
        "[red]",
        "[/red]",
        "[green]",
        "[/green]",
        "[blue]",
        "[/blue]",
        "[yellow]",
        "[/yellow]",
        "[/]",
    ];

    for pattern in markup_patterns {
        if output.contains(pattern) {
            tracing::error!(
                context = context,
                pattern = pattern,
                output = output,
                "assertion failed: raw markup tag found"
            );
            panic!("{context}: found raw markup tag '{pattern}' in output.\nOutput: {output:?}");
        }
    }

    tracing::trace!(context = context, "assertion passed: no raw markup");
}

/// Count occurrences of a specific ANSI code in output.
///
/// Useful for verifying the correct number of style transitions.
///
/// # Example
///
/// ```rust,ignore
/// let bold_count = count_ansi_code(&output, ansi::BOLD);
/// assert_eq!(bold_count, 2, "Expected 2 bold regions");
/// ```
#[must_use]
pub fn count_ansi_code(output: &str, ansi_code: &str) -> usize {
    output.matches(ansi_code).count()
}

/// Extract styled regions from output.
///
/// Returns a vector of (text, has_style) tuples showing styled vs unstyled regions.
/// Useful for debugging and detailed style verification.
///
/// # Example
///
/// ```rust,ignore
/// let regions = extract_styled_regions(&output);
/// for (text, styled) in regions {
///     println!("{}: styled={}", text, styled);
/// }
/// ```
#[must_use]
pub fn extract_styled_regions(output: &str) -> Vec<(String, bool)> {
    let re = regex::Regex::new(r"(\x1b\[[0-9;]*m)").unwrap();
    let mut regions = Vec::new();
    let mut last_end = 0;
    let mut in_styled_region = false;

    for cap in re.captures_iter(output) {
        let mat = cap.get(0).unwrap();

        // Text before this escape sequence
        if mat.start() > last_end {
            let text = &output[last_end..mat.start()];
            if !text.is_empty() {
                regions.push((text.to_string(), in_styled_region));
            }
        }

        // Check if this is a reset or a style
        let code = cap.get(0).map_or("", |m| m.as_str());
        in_styled_region = code != ansi::RESET;

        last_end = mat.end();
    }

    // Remaining text after last escape
    if last_end < output.len() {
        let text = &output[last_end..];
        if !text.is_empty() {
            regions.push((text.to_string(), in_styled_region));
        }
    }

    regions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::init_test_logging;

    #[test]
    fn test_assert_eq_logged_pass() {
        init_test_logging();
        assert_eq_logged("simple equality", 42, 42);
    }

    #[test]
    #[should_panic(expected = "expected 42")]
    fn test_assert_eq_logged_fail() {
        init_test_logging();
        assert_eq_logged("will fail", 0, 42);
    }

    #[test]
    fn test_assert_ok_logged_pass() {
        init_test_logging();
        let result: Result<i32, &str> = Ok(42);
        let value = assert_ok_logged("ok result", result);
        assert_eq!(value, 42);
    }

    #[test]
    fn test_assert_contains_logged_pass() {
        init_test_logging();
        assert_contains_logged("substring", "hello world", "world");
    }

    #[test]
    fn test_assert_len_logged_pass() {
        init_test_logging();
        let vec = vec![1, 2, 3, 4, 5];
        assert_len_logged("vec length", &vec, 5);
    }
}
