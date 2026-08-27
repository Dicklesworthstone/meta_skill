//! Conformance testing framework for rich_rust.
//!
//! This module provides infrastructure for triple-duty testing:
//! 1. **Integration tests** - Verify correct behavior
//! 2. **Conformance tests** - Compare output against Python Rich
//! 3. **Benchmark harness** - Measure performance
//!
//! # Architecture
//!
//! Test cases are defined as structs implementing `TestCase`. Each test case
//! specifies:
//! - How to render content in rich_rust
//! - Expected behavior (via snapshot or explicit assertion)
//! - Optional Python Rich equivalent for conformance checking
//!
//! # Usage
//!
//! ```ignore
//! use conformance::{TestCase, run_test};
//!
//! let test = TextRenderTest {
//!     input: "Hello, [bold]World[/]!",
//!     width: 40,
//! };
//! run_test(&test);
//! ```

use rich_rust::segment::Segment;
use std::fmt::Debug;

pub mod layout_tests;
pub mod live_tests;
pub mod logging_tests;
pub mod panel_tests;
pub mod progress_tests;
pub mod rule_tests;
pub mod table_tests;
pub mod text_tests;
pub mod tree_tests;

/// A test case that can be used for integration tests, conformance, and benchmarks.
pub trait TestCase: Debug {
    /// Name of the test case (used for reporting).
    fn name(&self) -> &str;

    /// Render the test case and return segments.
    fn render(&self) -> Vec<Segment<'static>>;

    /// Render the test case and return plain text output.
    fn render_plain(&self) -> String {
        self.render()
            .into_iter()
            .map(|s| s.text.into_owned())
            .collect()
    }

    /// Optional: Return the equivalent Python Rich code for conformance testing.
    /// Returns None if no Python equivalent exists.
    fn python_rich_code(&self) -> Option<String> {
        None
    }
}

/// Strip ANSI escape codes from a string.
pub fn strip_ansi(s: &str) -> String {
    let ansi_regex = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    ansi_regex.replace_all(s, "").to_string()
}

/// Normalize output for comparison (strip newlines, trim).
#[allow(dead_code)]
pub fn normalize_output(s: &str) -> String {
    s.lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Check if two outputs match after normalization.
#[allow(dead_code)]
pub fn outputs_match(actual: &str, expected: &str) -> bool {
    normalize_output(actual) == normalize_output(expected)
}

/// Run a test case and verify it produces valid output.
pub fn run_test<T: TestCase + ?Sized>(test: &T) -> String {
    let output = test.render_plain();
    let plain = strip_ansi(&output);

    // Basic sanity check - output should not be empty for most cases
    // (unless explicitly expected)
    assert!(
        !plain.is_empty() || test.name().contains("empty"),
        "Test '{}' produced empty output",
        test.name()
    );

    plain
}

/// Macro to define a test case struct with common fields.
#[macro_export]
macro_rules! define_test_case {
    ($name:ident { $($field:ident : $type:ty),* $(,)? }) => {
        #[derive(Debug)]
        pub struct $name {
            pub name: &'static str,
            $(pub $field: $type,)*
        }
    };
}
