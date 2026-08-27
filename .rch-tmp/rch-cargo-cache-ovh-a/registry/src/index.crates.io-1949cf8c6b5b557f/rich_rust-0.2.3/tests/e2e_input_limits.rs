//! End-to-end tests for input length limiting in interactive prompts.
//!
//! These tests verify that:
//! 1. Normal input within limits works
//! 2. Input at exact limit works
//! 3. Input exceeding limit fails with proper error
//! 4. UTF-8 boundary cases are handled correctly
//! 5. All prompt types (Prompt, Select, Confirm) respect limits
//! 6. Error messages are clear and informative
//!
//! Run with: cargo test --test e2e_input_limits -- --nocapture

mod common;

use common::init_test_logging;
use rich_rust::interactive::{Confirm, PromptError, Select};
use rich_rust::prelude::*;
use std::io::Cursor;
use tracing::info;

/// Helper: create a console configured as interactive (terminal).
fn interactive_console() -> Console {
    Console::builder()
        .width(80)
        .force_terminal(true)
        .markup(true)
        .build()
}

/// Helper: create a BufRead reader from a string.
fn input(s: &str) -> Cursor<Vec<u8>> {
    Cursor::new(s.as_bytes().to_vec())
}

/// Helper: create a BufRead reader from raw bytes.
fn input_bytes(bytes: Vec<u8>) -> Cursor<Vec<u8>> {
    Cursor::new(bytes)
}

/// Helper: create a separator line for logging.
fn separator() -> &'static str {
    "--------------------------------------------------"
}

// =============================================================================
// TEST SECTION 1: Basic Input Length Limits
// =============================================================================

/// Test: Input well under the limit is accepted.
#[test]
fn test_input_well_under_limit() {
    init_test_logging();
    info!("TEST: Input well under limit");
    info!("{}", separator());

    let console = interactive_console();
    let limit = 1024;
    let short_input = "hello\n"; // 6 bytes

    info!(
        input_len = short_input.len(),
        limit = limit,
        "Testing short input"
    );

    let prompt = Prompt::new("Name").max_length(limit);
    let mut reader = input(short_input);
    let result = prompt.ask_from(&console, &mut reader);

    assert!(result.is_ok(), "Short input should be accepted");
    assert_eq!(result.unwrap(), "hello");
    info!("Short input accepted correctly");
}

/// Test: Input exactly at the limit is accepted.
#[test]
fn test_input_exactly_at_limit() {
    init_test_logging();
    info!("TEST: Input exactly at limit");
    info!("{}", separator());

    let console = interactive_console();
    let limit = 6; // "ab\n" is 3 bytes, "hello\n" is 6 bytes
    let exact_input = "hello\n"; // exactly 6 bytes

    info!(
        input_len = exact_input.len(),
        limit = limit,
        "Testing exact-limit input"
    );

    let prompt = Prompt::new("Input").max_length(limit);
    let mut reader = input(exact_input);
    let result = prompt.ask_from(&console, &mut reader);

    assert!(result.is_ok(), "Exact-limit input should be accepted");
    assert_eq!(result.unwrap(), "hello");
    info!("[PASS] Exact-limit input accepted correctly");
}

/// Test: Input one byte over the limit is rejected.
#[test]
fn test_input_one_byte_over_limit() {
    init_test_logging();
    info!("TEST: Input one byte over limit");
    info!("{}", separator());

    let console = interactive_console();
    let limit = 5; // "hello\n" is 6 bytes, limit is 5
    let over_input = "hello\n";

    info!(
        input_len = over_input.len(),
        limit = limit,
        "Testing over-limit input"
    );

    let prompt = Prompt::new("Input").max_length(limit);
    let mut reader = input(over_input);
    let result = prompt.ask_from(&console, &mut reader);

    assert!(result.is_err(), "Over-limit input should be rejected");
    let err = result.unwrap_err();
    assert!(err.is_input_too_long());
    assert_eq!(err.input_limit(), Some(limit));
    info!("[PASS] Over-limit input rejected with InputTooLong error");
}

/// Test: Very long input is rejected without excessive memory allocation.
#[test]
fn test_very_long_input_rejected() {
    init_test_logging();
    info!("TEST: Very long input (simulate attack)");
    info!("{}", separator());

    let console = interactive_console();
    let limit = 1024;
    // Create 100KB of input (should fail fast, not allocate all of it)
    let long_input: String = "x".repeat(100_000) + "\n";

    info!(
        input_len = long_input.len(),
        limit = limit,
        "Testing very long input"
    );

    let prompt = Prompt::new("Input").max_length(limit);
    let mut reader = input(&long_input);
    let result = prompt.ask_from(&console, &mut reader);

    assert!(result.is_err(), "Very long input should be rejected");
    let err = result.unwrap_err();
    assert!(err.is_input_too_long());
    info!("[PASS] Very long input rejected (memory protected)");
}

// =============================================================================
// TEST SECTION 2: UTF-8 Boundary Cases
// =============================================================================

/// Test: Multi-byte UTF-8 characters within limit.
#[test]
fn test_utf8_multibyte_within_limit() {
    init_test_logging();
    info!("TEST: Multi-byte UTF-8 within limit");
    info!("{}", separator());

    let console = interactive_console();
    // "世界\n" = 3 + 3 + 1 = 7 bytes
    let utf8_input = "世界\n";
    let limit = 10;

    info!(
        input = utf8_input.trim(),
        byte_len = utf8_input.len(),
        char_count = utf8_input.chars().count(),
        limit = limit,
        "Testing UTF-8 input"
    );

    let prompt = Prompt::new("City").max_length(limit);
    let mut reader = input(utf8_input);
    let result = prompt.ask_from(&console, &mut reader);

    assert!(
        result.is_ok(),
        "UTF-8 input within limit should be accepted"
    );
    assert_eq!(result.unwrap(), "世界");
    info!("[PASS] Multi-byte UTF-8 accepted correctly");
}

/// Test: Multi-byte UTF-8 crossing limit fails cleanly.
#[test]
fn test_utf8_crossing_limit() {
    init_test_logging();
    info!("TEST: UTF-8 crossing byte limit");
    info!("{}", separator());

    let console = interactive_console();
    // "世界\n" = 7 bytes, limit at 6 cuts into a character
    let utf8_input = "世界\n";
    let limit = 6;

    info!(
        input = utf8_input.trim(),
        byte_len = utf8_input.len(),
        limit = limit,
        "Testing UTF-8 at byte boundary"
    );

    let prompt = Prompt::new("City").max_length(limit);
    let mut reader = input(utf8_input);
    let result = prompt.ask_from(&console, &mut reader);

    assert!(result.is_err(), "UTF-8 crossing limit should be rejected");
    let err = result.unwrap_err();
    assert!(err.is_input_too_long());
    info!("[PASS] UTF-8 boundary handled correctly - rejected before invalid split");
}

/// Test: Emoji input near limit.
#[test]
fn test_emoji_input_within_limit() {
    init_test_logging();
    info!("TEST: Emoji input near limit");
    info!("{}", separator());

    let console = interactive_console();
    // "👋🌍\n" = 4 + 4 + 1 = 9 bytes
    let emoji_input = "👋🌍\n";
    let limit = 10;

    info!(
        input = emoji_input.trim(),
        byte_len = emoji_input.len(),
        limit = limit,
        "Testing emoji input"
    );

    let prompt = Prompt::new("Greeting").max_length(limit);
    let mut reader = input(emoji_input);
    let result = prompt.ask_from(&console, &mut reader);

    assert!(result.is_ok(), "Emoji within limit should be accepted");
    assert_eq!(result.unwrap(), "👋🌍");
    info!("[PASS] Emoji input handled correctly");
}

/// Test: Mixed ASCII and UTF-8.
#[test]
fn test_mixed_ascii_utf8() {
    init_test_logging();
    info!("TEST: Mixed ASCII and UTF-8");
    info!("{}", separator());

    let console = interactive_console();
    // "Hi 世界!\n" = 2 + 1 + 6 + 1 + 1 = 11 bytes
    let mixed_input = "Hi 世界!\n";
    let limit = 15;

    info!(
        input = mixed_input.trim(),
        byte_len = mixed_input.len(),
        limit = limit,
        "Testing mixed encoding"
    );

    let prompt = Prompt::new("Message").max_length(limit);
    let mut reader = input(mixed_input);
    let result = prompt.ask_from(&console, &mut reader);

    assert!(
        result.is_ok(),
        "Mixed input within limit should be accepted"
    );
    assert_eq!(result.unwrap(), "Hi 世界!");
    info!("[PASS] Mixed ASCII/UTF-8 handled correctly");
}

/// Test: Invalid UTF-8 returns validation error.
#[test]
fn test_invalid_utf8_rejected() {
    init_test_logging();
    info!("TEST: Invalid UTF-8 handling");
    info!("{}", separator());

    let console = interactive_console();
    // Invalid UTF-8 sequence followed by newline
    let invalid_bytes = vec![0xff, 0xfe, b'\n'];
    let limit = 100;

    info!(
        byte_len = invalid_bytes.len(),
        limit = limit,
        "Testing invalid UTF-8 bytes"
    );

    let prompt = Prompt::new("Data").max_length(limit);
    let mut reader = input_bytes(invalid_bytes);
    let result = prompt.ask_from(&console, &mut reader);

    assert!(result.is_err(), "Invalid UTF-8 should be rejected");
    // Should be a Validation error, not InputTooLong
    let err = result.unwrap_err();
    assert!(
        !err.is_input_too_long(),
        "Invalid UTF-8 should not be InputTooLong error"
    );
    info!("[PASS] Invalid UTF-8 rejected with validation error");
}

// =============================================================================
// TEST SECTION 3: Select and Confirm Prompts
// =============================================================================

/// Test: Select prompt respects max_length.
#[test]
fn test_select_max_length_exceeded() {
    init_test_logging();
    info!("TEST: Select prompt max_length exceeded");
    info!("{}", separator());

    let console = interactive_console();
    let limit = 5;
    // Very long input for a selection
    let long_input = "this is way too long for a selection\n";

    info!(
        input_len = long_input.len(),
        limit = limit,
        "Testing Select with long input"
    );

    let select = Select::new("Choose")
        .choices(["Option A", "Option B", "Option C"])
        .max_length(limit);
    let mut reader = input(long_input);
    let result = select.ask_from(&console, &mut reader);

    assert!(result.is_err(), "Long Select input should be rejected");
    let err = result.unwrap_err();
    assert!(err.is_input_too_long());
    info!("[PASS] Select rejects oversized input");
}

/// Test: Select prompt accepts valid short input.
#[test]
fn test_select_max_length_valid() {
    init_test_logging();
    info!("TEST: Select prompt valid input");
    info!("{}", separator());

    let console = interactive_console();
    let limit = 100;
    let short_input = "1\n"; // Select option 1

    info!(
        input_len = short_input.len(),
        limit = limit,
        "Testing Select with short input"
    );

    let select = Select::new("Choose")
        .choices(["Option A", "Option B", "Option C"])
        .max_length(limit);
    let mut reader = input(short_input);
    let result = select.ask_from(&console, &mut reader);

    assert!(result.is_ok(), "Short Select input should be accepted");
    assert_eq!(result.unwrap(), "Option A"); // "1" selects first option
    info!("[PASS] Select accepts valid input");
}

/// Test: Confirm prompt respects max_length with absurd input.
#[test]
fn test_confirm_max_length_exceeded() {
    init_test_logging();
    info!("TEST: Confirm prompt max_length exceeded");
    info!("{}", separator());

    let console = interactive_console();
    let limit = 10;
    // Absurd input for a yes/no confirmation
    let absurd_input = "yesyesyesyesyesyes\n";

    info!(
        input_len = absurd_input.len(),
        limit = limit,
        "Testing Confirm with absurd input"
    );

    let confirm = Confirm::new("Continue?").max_length(limit);
    let mut reader = input(absurd_input);
    let result = confirm.ask_from(&console, &mut reader);

    assert!(result.is_err(), "Absurd Confirm input should be rejected");
    let err = result.unwrap_err();
    assert!(err.is_input_too_long());
    info!("[PASS] Confirm rejects absurd input");
}

/// Test: Confirm prompt accepts valid yes/no.
#[test]
fn test_confirm_max_length_valid() {
    init_test_logging();
    info!("TEST: Confirm prompt valid input");
    info!("{}", separator());

    let console = interactive_console();
    let limit = 100;

    for (inp, expected) in [
        ("y\n", true),
        ("n\n", false),
        ("yes\n", true),
        ("no\n", false),
    ] {
        info!(input = inp.trim(), expected = expected, "Testing Confirm");

        let confirm = Confirm::new("Continue?").max_length(limit);
        let mut reader = input(inp);
        let result = confirm.ask_from(&console, &mut reader);

        assert!(result.is_ok(), "Valid Confirm input should be accepted");
        assert_eq!(result.unwrap(), expected);
    }
    info!("[PASS] Confirm accepts valid yes/no inputs");
}

// =============================================================================
// TEST SECTION 4: Error Message Clarity
// =============================================================================

/// Test: InputTooLong error contains limit and received values.
#[test]
fn test_error_contains_both_values() {
    init_test_logging();
    info!("TEST: Error message contains limit and received");
    info!("{}", separator());

    let err = PromptError::InputTooLong {
        limit: 100,
        received: 250,
    };

    let msg = err.to_string();
    info!(error_message = %msg, "Checking error message content");

    assert!(msg.contains("100"), "Error should mention limit");
    assert!(msg.contains("250"), "Error should mention received bytes");
    assert!(
        msg.contains("limit") || msg.contains("bytes"),
        "Error should be descriptive"
    );
    info!("[PASS] Error message is clear and informative");
}

/// Test: Error helper methods work correctly.
#[test]
fn test_error_helper_methods() {
    init_test_logging();
    info!("TEST: Error helper methods");
    info!("{}", separator());

    // InputTooLong case
    let too_long = PromptError::InputTooLong {
        limit: 64,
        received: 128,
    };
    assert!(too_long.is_input_too_long());
    assert_eq!(too_long.input_limit(), Some(64));
    info!("[PASS] InputTooLong helpers work");

    // Other error types should return false/None
    let eof = PromptError::Eof;
    assert!(!eof.is_input_too_long());
    assert_eq!(eof.input_limit(), None);

    let not_interactive = PromptError::NotInteractive;
    assert!(!not_interactive.is_input_too_long());
    assert_eq!(not_interactive.input_limit(), None);

    info!("[PASS] Non-InputTooLong errors return correct values");
}

/// Test: Error is Debug-printable for logging.
#[test]
fn test_error_debug_format() {
    init_test_logging();
    info!("TEST: Error Debug format");
    info!("{}", separator());

    let err = PromptError::InputTooLong {
        limit: 512,
        received: 1024,
    };

    let debug_str = format!("{:?}", err);
    info!(debug_output = %debug_str, "Debug format output");

    assert!(
        debug_str.contains("InputTooLong"),
        "Debug should show variant name"
    );
    assert!(debug_str.contains("512"), "Debug should show limit");
    assert!(debug_str.contains("1024"), "Debug should show received");
    info!("[PASS] Error Debug format is useful for logging");
}

// =============================================================================
// TEST SECTION 5: Default Limit Behavior
// =============================================================================

/// Test: Prompt without explicit max_length uses default (64 KiB).
#[test]
fn test_default_limit_used() {
    init_test_logging();
    info!("TEST: Default limit behavior");
    info!("{}", separator());

    let console = interactive_console();
    // Input under 64 KiB should work with default limit
    let normal_input = "This is a normal sized input\n";

    info!(
        input_len = normal_input.len(),
        "Testing with default limit (64 KiB)"
    );

    let prompt = Prompt::new("Message"); // No explicit max_length
    let mut reader = input(normal_input);
    let result = prompt.ask_from(&console, &mut reader);

    assert!(
        result.is_ok(),
        "Normal input should work with default 64 KiB limit"
    );
    info!("[PASS] Default limit allows normal input");
}

// =============================================================================
// TEST SECTION 6: Edge Cases
// =============================================================================

/// Test: Empty input with allow_empty under limit.
#[test]
fn test_empty_input_with_limit() {
    init_test_logging();
    info!("TEST: Empty input with limit");
    info!("{}", separator());

    let console = interactive_console();
    let limit = 100;

    let prompt = Prompt::new("Optional").allow_empty(true).max_length(limit);
    let mut reader = input("\n");
    let result = prompt.ask_from(&console, &mut reader);

    assert!(result.is_ok(), "Empty input should be allowed");
    assert_eq!(result.unwrap(), "");
    info!("[PASS] Empty input works with limit");
}

/// Test: Input with only whitespace under limit.
#[test]
fn test_whitespace_input_with_limit() {
    init_test_logging();
    info!("TEST: Whitespace input with limit");
    info!("{}", separator());

    let console = interactive_console();
    let limit = 100;

    let prompt = Prompt::new("Input").allow_empty(true).max_length(limit);
    let mut reader = input("   \n");
    let result = prompt.ask_from(&console, &mut reader);

    assert!(result.is_ok(), "Whitespace input should be accepted");
    // Trailing whitespace is trimmed
    assert_eq!(result.unwrap(), "");
    info!("[PASS] Whitespace input handled correctly");
}

/// Test: Multiple lines (only first line read).
#[test]
fn test_multiline_input_first_line_only() {
    init_test_logging();
    info!("TEST: Multiline input reads first line only");
    info!("{}", separator());

    let console = interactive_console();
    let limit = 100;
    let multiline = "first\nsecond\nthird\n";

    let prompt = Prompt::new("Line").max_length(limit);
    let mut reader = input(multiline);
    let result = prompt.ask_from(&console, &mut reader);

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "first");
    info!("[PASS] Only first line is read");
}
