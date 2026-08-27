//! Integration tests for the logging infrastructure.
//!
//! Run with: RUST_LOG=debug cargo test --test logging_test -- --nocapture

mod common;

use common::{
    assertions::{
        assert_contains_logged, assert_eq_logged, assert_len_logged, assert_none_logged,
        assert_ok_logged, assert_some_logged, assert_true_logged,
    },
    init_test_logging, log_test_context, test_phase,
};

#[test]
fn test_logging_infrastructure_works() {
    init_test_logging();
    log_test_context(
        "test_logging_infrastructure_works",
        "Verifies tracing setup",
    );

    {
        let _setup = test_phase("setup");
        tracing::debug!("Setting up test");
    }

    {
        let _execute = test_phase("execute");
        tracing::info!("Executing test logic");
    }

    {
        let _verify = test_phase("verify");
        tracing::debug!("Verification complete");
    }
}

#[test]
fn test_logged_assertions_basic() {
    init_test_logging();
    log_test_context(
        "test_logged_assertions_basic",
        "Tests basic assertion logging",
    );

    assert_eq_logged("integer equality", 42, 42);
    assert_eq_logged("string equality", "hello", "hello");
    assert_true_logged("simple boolean", true);
}

#[test]
fn test_logged_assertions_result() {
    init_test_logging();
    log_test_context(
        "test_logged_assertions_result",
        "Tests Result assertion logging",
    );

    let ok_result: Result<i32, &str> = Ok(100);
    let value = assert_ok_logged("successful result", ok_result);
    assert_eq_logged("ok value", value, 100);
}

#[test]
fn test_logged_assertions_option() {
    init_test_logging();
    log_test_context(
        "test_logged_assertions_option",
        "Tests Option assertion logging",
    );

    let some_value: Option<&str> = Some("present");
    let value = assert_some_logged("some option", some_value);
    assert_eq_logged("some value", value, "present");

    let none_value: Option<i32> = None;
    assert_none_logged("none option", none_value);
}

#[test]
fn test_logged_assertions_collection() {
    init_test_logging();
    log_test_context(
        "test_logged_assertions_collection",
        "Tests collection assertion logging",
    );

    let vec = vec![1, 2, 3, 4, 5];
    assert_len_logged("vector length", &vec, 5);

    let s = "hello world";
    assert_contains_logged("substring check", s, "world");
}

#[test]
fn test_with_rich_rust_types() {
    init_test_logging();
    log_test_context(
        "test_with_rich_rust_types",
        "Tests logging with rich_rust library types",
    );

    // Test with color parsing from rich_rust
    use rich_rust::Color;

    {
        let _parse = test_phase("color_parsing");

        // Parse a named color
        let red_result = Color::parse("red");
        let red = assert_ok_logged("parse red", red_result);
        tracing::debug!(color = ?red, "Parsed red color");

        // Parse a hex color
        let hex_result = Color::parse("#00ff00");
        let green = assert_ok_logged("parse hex green", hex_result);
        tracing::debug!(color = ?green, "Parsed hex green color");
    }

    {
        let _style = test_phase("style_testing");

        use rich_rust::{Attributes, Style};

        let style = Style::parse("bold red").unwrap();
        tracing::debug!(style = ?style, "Created bold red style");

        // Verify the style has the expected attributes
        assert_true_logged("style is bold", style.attributes.contains(Attributes::BOLD));
    }
}
