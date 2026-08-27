//! End-to-end tests for interactive prompts, select, confirm, pager, and status.
//!
//! Tests use simulated input via `BufRead` readers and non-terminal consoles
//! to verify prompt logic, validation, defaults, and error handling.

mod common;

use common::init_test_logging;
use rich_rust::interactive::{Confirm, Select};
use rich_rust::prelude::*;
use std::io::{self, Cursor};
use std::sync::Arc;

/// Helper: create a console configured as interactive (terminal).
fn interactive_console() -> Console {
    Console::builder()
        .width(80)
        .force_terminal(true)
        .markup(true)
        .build()
}

/// Helper: create a console configured as non-interactive (pipe/redirect).
fn non_interactive_console() -> Console {
    Console::builder()
        .width(80)
        .force_terminal(false)
        .markup(true)
        .build()
}

/// Helper: create a BufRead reader from a string.
fn input(s: &str) -> Cursor<Vec<u8>> {
    Cursor::new(s.as_bytes().to_vec())
}

// =============================================================================
// Text Prompt: Basic Input
// =============================================================================

/// Test: Prompt accepts simple text input.
#[test]
fn test_prompt_basic_text_input() {
    init_test_logging();

    let console = interactive_console();
    let prompt = Prompt::new("Name");
    let mut reader = input("Alice\n");

    let result = prompt.ask_from(&console, &mut reader).unwrap();
    assert_eq!(result, "Alice");
}

/// Test: Prompt with default returns default on empty input.
#[test]
fn test_prompt_default_on_empty_input() {
    init_test_logging();

    let console = interactive_console();
    let prompt = Prompt::new("Name").default("Bob");
    let mut reader = input("\n");

    let result = prompt.ask_from(&console, &mut reader).unwrap();
    assert_eq!(result, "Bob");
}

/// Test: Prompt returns user input even when default is set.
#[test]
fn test_prompt_user_input_overrides_default() {
    init_test_logging();

    let console = interactive_console();
    let prompt = Prompt::new("Name").default("Bob");
    let mut reader = input("Charlie\n");

    let result = prompt.ask_from(&console, &mut reader).unwrap();
    assert_eq!(result, "Charlie");
}

/// Test: Prompt with allow_empty accepts empty input.
#[test]
fn test_prompt_allow_empty() {
    init_test_logging();

    let console = interactive_console();
    let prompt = Prompt::new("Optional").allow_empty(true);
    let mut reader = input("\n");

    let result = prompt.ask_from(&console, &mut reader).unwrap();
    assert_eq!(result, "");
}

/// Test: Prompt strips trailing whitespace from input.
#[test]
fn test_prompt_trims_trailing_whitespace() {
    init_test_logging();

    let console = interactive_console();
    let prompt = Prompt::new("Input");
    let mut reader = input("hello   \n");

    let result = prompt.ask_from(&console, &mut reader).unwrap();
    assert_eq!(result, "hello");
}

// =============================================================================
// Text Prompt: Validation
// =============================================================================

/// Test: Prompt with validator retries on invalid input.
#[test]
fn test_prompt_validation_retry() {
    init_test_logging();

    let console = interactive_console();
    let prompt = Prompt::new("Age").validate(|s: &str| {
        s.parse::<u32>()
            .map(|_| ())
            .map_err(|_| "Enter a number".to_string())
    });

    // First input is invalid "abc", second is valid "25"
    let mut reader = input("abc\n25\n");

    let result = prompt.ask_from(&console, &mut reader).unwrap();
    assert_eq!(result, "25");
}

/// Test: Prompt validator accepts valid input on first try.
#[test]
fn test_prompt_validation_passes() {
    init_test_logging();

    let console = interactive_console();
    let prompt = Prompt::new("Email").validate(|s: &str| {
        if s.contains('@') {
            Ok(())
        } else {
            Err("Must contain @".to_string())
        }
    });
    let mut reader = input("user@example.com\n");

    let result = prompt.ask_from(&console, &mut reader).unwrap();
    assert_eq!(result, "user@example.com");
}

// =============================================================================
// Text Prompt: Non-Interactive Mode
// =============================================================================

/// Test: Non-interactive console returns default without reading input.
#[test]
fn test_prompt_non_interactive_returns_default() {
    init_test_logging();

    let console = non_interactive_console();
    let prompt = Prompt::new("Name").default("DefaultUser");

    let result = prompt.ask(&console).unwrap();
    assert_eq!(result, "DefaultUser");
}

/// Test: Non-interactive console without default returns NotInteractive error.
#[test]
fn test_prompt_non_interactive_no_default_errors() {
    init_test_logging();

    let console = non_interactive_console();
    let prompt = Prompt::new("Name");

    let result = prompt.ask(&console);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err().to_string(),
        "prompt requires an interactive console"
    );
}

// =============================================================================
// Text Prompt: EOF Handling
// =============================================================================

/// Test: Prompt returns Eof error when input stream is empty.
#[test]
fn test_prompt_eof_on_empty_stream() {
    init_test_logging();

    let console = interactive_console();
    let prompt = Prompt::new("Input");
    let mut reader = input(""); // empty = EOF

    let result = prompt.ask_from(&console, &mut reader);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().to_string(), "prompt input reached EOF");
}

// =============================================================================
// Text Prompt: Max Length
// =============================================================================

/// Test: Prompt with max_length rejects oversized input.
#[test]
fn test_prompt_max_length_exceeded() {
    init_test_logging();

    let console = interactive_console();
    let prompt = Prompt::new("Input").max_length(10);
    // Input exceeds 10 bytes
    let mut reader = input("This is way too long\n");

    let result = prompt.ask_from(&console, &mut reader);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_input_too_long());
    assert_eq!(err.input_limit(), Some(10));
}

/// Test: Prompt with max_length accepts input within limit.
#[test]
fn test_prompt_max_length_within_limit() {
    init_test_logging();

    let console = interactive_console();
    let prompt = Prompt::new("Input").max_length(100);
    let mut reader = input("Short\n");

    let result = prompt.ask_from(&console, &mut reader).unwrap();
    assert_eq!(result, "Short");
}

// =============================================================================
// Integer Prompt (via Validation)
// =============================================================================

/// Test: Integer validation accepts valid integer input.
#[test]
fn test_integer_prompt_valid() {
    init_test_logging();

    let console = interactive_console();
    let prompt = Prompt::new("Count").validate(|s: &str| {
        s.parse::<i64>()
            .map(|_| ())
            .map_err(|_| "Enter a valid integer".to_string())
    });
    let mut reader = input("42\n");

    let result = prompt.ask_from(&console, &mut reader).unwrap();
    assert_eq!(result, "42");
}

/// Test: Integer validation rejects non-numeric input, then accepts.
#[test]
fn test_integer_prompt_retry_then_valid() {
    init_test_logging();

    let console = interactive_console();
    let prompt = Prompt::new("Count").validate(|s: &str| {
        s.parse::<i64>()
            .map(|_| ())
            .map_err(|_| "Enter a valid integer".to_string())
    });
    let mut reader = input("abc\n3.14\n99\n");

    let result = prompt.ask_from(&console, &mut reader).unwrap();
    assert_eq!(result, "99");
}

/// Test: Negative integer accepted by integer validator.
#[test]
fn test_integer_prompt_negative() {
    init_test_logging();

    let console = interactive_console();
    let prompt = Prompt::new("Temperature").validate(|s: &str| {
        s.parse::<i64>()
            .map(|_| ())
            .map_err(|_| "Enter a valid integer".to_string())
    });
    let mut reader = input("-10\n");

    let result = prompt.ask_from(&console, &mut reader).unwrap();
    assert_eq!(result, "-10");
}

// =============================================================================
// Float Prompt (via Validation)
// =============================================================================

/// Test: Float validation accepts valid float input.
#[test]
fn test_float_prompt_valid() {
    init_test_logging();

    let console = interactive_console();
    let prompt = Prompt::new("Price").validate(|s: &str| {
        s.parse::<f64>()
            .map(|_| ())
            .map_err(|_| "Enter a valid number".to_string())
    });
    let mut reader = input("3.14\n");

    let result = prompt.ask_from(&console, &mut reader).unwrap();
    assert_eq!(result, "3.14");
}

/// Test: Float validation rejects text, then accepts.
#[test]
fn test_float_prompt_retry() {
    init_test_logging();

    let console = interactive_console();
    let prompt = Prompt::new("Weight").validate(|s: &str| {
        s.parse::<f64>()
            .map(|_| ())
            .map_err(|_| "Enter a valid number".to_string())
    });
    let mut reader = input("heavy\n72.5\n");

    let result = prompt.ask_from(&console, &mut reader).unwrap();
    assert_eq!(result, "72.5");
}

// =============================================================================
// Confirm Prompt
// =============================================================================

/// Test: Confirm accepts "y" as true.
#[test]
fn test_confirm_yes() {
    init_test_logging();

    let console = interactive_console();
    let confirm = Confirm::new("Proceed?");
    let mut reader = input("y\n");

    let result = confirm.ask_from(&console, &mut reader).unwrap();
    assert!(result);
}

/// Test: Confirm accepts "n" as false.
#[test]
fn test_confirm_no() {
    init_test_logging();

    let console = interactive_console();
    let confirm = Confirm::new("Proceed?");
    let mut reader = input("n\n");

    let result = confirm.ask_from(&console, &mut reader).unwrap();
    assert!(!result);
}

/// Test: Confirm accepts "yes" (full word) as true.
#[test]
fn test_confirm_yes_full() {
    init_test_logging();

    let console = interactive_console();
    let confirm = Confirm::new("Continue?");
    let mut reader = input("yes\n");

    let result = confirm.ask_from(&console, &mut reader).unwrap();
    assert!(result);
}

/// Test: Confirm accepts "no" (full word) as false.
#[test]
fn test_confirm_no_full() {
    init_test_logging();

    let console = interactive_console();
    let confirm = Confirm::new("Continue?");
    let mut reader = input("no\n");

    let result = confirm.ask_from(&console, &mut reader).unwrap();
    assert!(!result);
}

/// Test: Confirm accepts "true" and "1" as true.
#[test]
fn test_confirm_true_and_one() {
    init_test_logging();

    let console = interactive_console();

    let confirm = Confirm::new("Q?");
    let mut reader = input("true\n");
    assert!(confirm.ask_from(&console, &mut reader).unwrap());

    let confirm = Confirm::new("Q?");
    let mut reader = input("1\n");
    assert!(confirm.ask_from(&console, &mut reader).unwrap());
}

/// Test: Confirm accepts "false" and "0" as false.
#[test]
fn test_confirm_false_and_zero() {
    init_test_logging();

    let console = interactive_console();

    let confirm = Confirm::new("Q?");
    let mut reader = input("false\n");
    assert!(!confirm.ask_from(&console, &mut reader).unwrap());

    let confirm = Confirm::new("Q?");
    let mut reader = input("0\n");
    assert!(!confirm.ask_from(&console, &mut reader).unwrap());
}

/// Test: Confirm with default returns default on empty input.
#[test]
fn test_confirm_default_on_empty() {
    init_test_logging();

    let console = interactive_console();
    let confirm = Confirm::new("Proceed?").default(true);
    let mut reader = input("\n");

    let result = confirm.ask_from(&console, &mut reader).unwrap();
    assert!(result);
}

/// Test: Confirm with default(false) returns false on empty input.
#[test]
fn test_confirm_default_false_on_empty() {
    init_test_logging();

    let console = interactive_console();
    let confirm = Confirm::new("Delete?").default(false);
    let mut reader = input("\n");

    let result = confirm.ask_from(&console, &mut reader).unwrap();
    assert!(!result);
}

/// Test: Confirm retries on invalid input, then accepts valid.
#[test]
fn test_confirm_invalid_then_valid() {
    init_test_logging();

    let console = interactive_console();
    let confirm = Confirm::new("Proceed?");
    let mut reader = input("maybe\ny\n");

    let result = confirm.ask_from(&console, &mut reader).unwrap();
    assert!(result);
}

/// Test: Confirm case-insensitive ("Y", "N").
#[test]
fn test_confirm_case_insensitive() {
    init_test_logging();

    let console = interactive_console();

    let confirm = Confirm::new("Q?");
    let mut reader = input("Y\n");
    assert!(confirm.ask_from(&console, &mut reader).unwrap());

    let confirm = Confirm::new("Q?");
    let mut reader = input("N\n");
    assert!(!confirm.ask_from(&console, &mut reader).unwrap());

    let confirm = Confirm::new("Q?");
    let mut reader = input("YES\n");
    assert!(confirm.ask_from(&console, &mut reader).unwrap());
}

/// Test: Confirm non-interactive returns default.
#[test]
fn test_confirm_non_interactive_returns_default() {
    init_test_logging();

    let console = non_interactive_console();
    let confirm = Confirm::new("Q?").default(true);

    let result = confirm.ask(&console).unwrap();
    assert!(result);
}

/// Test: Confirm non-interactive without default returns NotInteractive.
#[test]
fn test_confirm_non_interactive_no_default_errors() {
    init_test_logging();

    let console = non_interactive_console();
    let confirm = Confirm::new("Q?");

    let result = confirm.ask(&console);
    assert!(result.is_err());
}

// =============================================================================
// Select Prompt
// =============================================================================

/// Test: Select by exact text match.
#[test]
fn test_select_by_text() {
    init_test_logging();

    let console = interactive_console();
    let select = Select::new("Color").choices(["red", "green", "blue"]);
    let mut reader = input("green\n");

    let result = select.ask_from(&console, &mut reader).unwrap();
    assert_eq!(result, "green");
}

/// Test: Select by number.
#[test]
fn test_select_by_number() {
    init_test_logging();

    let console = interactive_console();
    let select = Select::new("Fruit").choices(["apple", "banana", "cherry"]);
    let mut reader = input("2\n");

    let result = select.ask_from(&console, &mut reader).unwrap();
    assert_eq!(result, "banana");
}

/// Test: Select uses default on empty input.
#[test]
fn test_select_default_on_empty() {
    init_test_logging();

    let console = interactive_console();
    let select = Select::new("Size")
        .choices(["small", "medium", "large"])
        .default("medium");
    let mut reader = input("\n");

    let result = select.ask_from(&console, &mut reader).unwrap();
    assert_eq!(result, "medium");
}

/// Test: Select retries on invalid input.
#[test]
fn test_select_invalid_then_valid() {
    init_test_logging();

    let console = interactive_console();
    let select = Select::new("Color").choices(["red", "green", "blue"]);
    // "purple" is not a valid choice, "1" selects "red"
    let mut reader = input("purple\n1\n");

    let result = select.ask_from(&console, &mut reader).unwrap();
    assert_eq!(result, "red");
}

/// Test: Select case-insensitive match.
#[test]
fn test_select_case_insensitive() {
    init_test_logging();

    let console = interactive_console();
    let select = Select::new("Pick").choices(["Red", "Green", "Blue"]);
    let mut reader = input("red\n");

    let result = select.ask_from(&console, &mut reader).unwrap();
    assert_eq!(result, "Red");
}

/// Test: Select with no choices returns validation error.
#[test]
fn test_select_no_choices_error() {
    init_test_logging();

    let console = interactive_console();
    let select = Select::new("Pick");
    let mut reader = input("anything\n");

    let result = select.ask_from(&console, &mut reader);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("No choices"));
}

/// Test: Select non-interactive returns default.
#[test]
fn test_select_non_interactive_returns_default() {
    init_test_logging();

    let console = non_interactive_console();
    let select = Select::new("Pick").choices(["a", "b", "c"]).default("b");

    let result = select.ask(&console).unwrap();
    assert_eq!(result, "b");
}

/// Test: Select non-interactive without default returns NotInteractive.
#[test]
fn test_select_non_interactive_no_default_errors() {
    init_test_logging();

    let console = non_interactive_console();
    let select = Select::new("Pick").choices(["a", "b", "c"]);

    let result = select.ask(&console);
    assert!(result.is_err());
}

/// Test: Select by boundary numbers (first and last).
#[test]
fn test_select_boundary_numbers() {
    init_test_logging();

    let console = interactive_console();
    let select = Select::new("Pick").choices(["first", "middle", "last"]);

    let mut reader = input("1\n");
    let result = select.ask_from(&console, &mut reader).unwrap();
    assert_eq!(result, "first");

    let select = Select::new("Pick").choices(["first", "middle", "last"]);
    let mut reader = input("3\n");
    let result = select.ask_from(&console, &mut reader).unwrap();
    assert_eq!(result, "last");
}

/// Test: Select with out-of-range number retries.
#[test]
fn test_select_out_of_range_number() {
    init_test_logging();

    let console = interactive_console();
    let select = Select::new("Pick").choices(["a", "b"]);
    // 5 is out of range, then "a" is valid
    let mut reader = input("5\na\n");

    let result = select.ask_from(&console, &mut reader).unwrap();
    assert_eq!(result, "a");
}

// =============================================================================
// Pager
// =============================================================================

/// Test: Pager can be constructed with defaults.
#[test]
fn test_pager_default_construction() {
    init_test_logging();

    let pager = Pager::default();
    // Pager should exist; further behavior depends on terminal
    let _ = pager;
}

/// Test: Pager with custom command.
#[test]
fn test_pager_custom_command() {
    init_test_logging();

    let pager = Pager::default().command("less");
    let _ = pager;
}

/// Test: Pager with color toggle.
#[test]
fn test_pager_allow_color() {
    init_test_logging();

    let pager = Pager::default().allow_color(true);
    let _ = pager;

    let pager2 = Pager::default().allow_color(false);
    let _ = pager2;
}

// =============================================================================
// Status Display
// =============================================================================

/// Test: Status can be created in non-interactive mode.
#[test]
fn test_status_non_interactive() {
    init_test_logging();

    let console = Arc::new(non_interactive_console());
    let status = Status::new(&console, "Loading...").unwrap();

    // Status should be created successfully
    status.update("Still loading...");
    drop(status);
}

/// Test: Status update changes message.
#[test]
fn test_status_update_message() {
    init_test_logging();

    let console = Arc::new(non_interactive_console());
    let status = Status::new(&console, "First message").unwrap();

    status.update("Second message");
    status.update("Third message");
    // No crash; update succeeds silently in non-interactive mode
    drop(status);
}

/// Test: Status can be dropped safely.
#[test]
fn test_status_drop_safety() {
    init_test_logging();

    let console = Arc::new(non_interactive_console());

    // Create and immediately drop
    {
        let status = Status::new(&console, "Temporary").unwrap();
        let _ = status;
    }
    // No panic or resource leak
}

// =============================================================================
// PromptError
// =============================================================================

/// Test: PromptError display messages.
#[test]
fn test_prompt_error_display() {
    init_test_logging();

    assert_eq!(
        PromptError::NotInteractive.to_string(),
        "prompt requires an interactive console"
    );
    assert_eq!(PromptError::Eof.to_string(), "prompt input reached EOF");
    assert_eq!(
        PromptError::Validation("bad input".into()).to_string(),
        "bad input"
    );

    let io_err = PromptError::Io(io::Error::new(io::ErrorKind::BrokenPipe, "pipe broke"));
    assert!(io_err.to_string().contains("pipe broke"));
}

/// Test: PromptError::InputTooLong display and helpers.
#[test]
fn test_prompt_error_input_too_long() {
    init_test_logging();

    let err = PromptError::InputTooLong {
        limit: 100,
        received: 200,
    };
    assert!(err.is_input_too_long());
    assert_eq!(err.input_limit(), Some(100));
    assert!(err.to_string().contains("200"));
    assert!(err.to_string().contains("100"));
}

/// Test: PromptError implements std::error::Error.
#[test]
fn test_prompt_error_std_error_trait() {
    init_test_logging();

    use std::error::Error;

    let err = PromptError::NotInteractive;
    assert!(err.source().is_none());

    let io_err = PromptError::Io(io::Error::other("test"));
    assert!(io_err.source().is_some());
}

/// Test: PromptError From<io::Error>.
#[test]
fn test_prompt_error_from_io() {
    init_test_logging();

    let io_err = io::Error::new(io::ErrorKind::UnexpectedEof, "eof");
    let prompt_err: PromptError = io_err.into();
    assert!(prompt_err.to_string().contains("eof"));
}

// =============================================================================
// Prompt with Markup
// =============================================================================

/// Test: Prompt with markup disabled.
#[test]
fn test_prompt_no_markup() {
    init_test_logging();

    let console = interactive_console();
    let prompt = Prompt::new("Input").markup(false);
    let mut reader = input("test\n");

    let result = prompt.ask_from(&console, &mut reader).unwrap();
    assert_eq!(result, "test");
}

/// Test: Prompt with show_default disabled.
#[test]
fn test_prompt_hide_default() {
    init_test_logging();

    let console = interactive_console();
    let prompt = Prompt::new("Name").default("Bob").show_default(false);
    let mut reader = input("\n");

    // Still uses default, just doesn't show it
    let result = prompt.ask_from(&console, &mut reader).unwrap();
    assert_eq!(result, "Bob");
}

// =============================================================================
// Combined Workflows
// =============================================================================

/// Test: Full interactive workflow: prompt → validate → select → confirm.
#[test]
fn test_full_interactive_workflow() {
    init_test_logging();

    let console = interactive_console();

    // Step 1: Get a name
    let prompt = Prompt::new("Name");
    let mut reader = input("Alice\n");
    let name = prompt.ask_from(&console, &mut reader).unwrap();
    assert_eq!(name, "Alice");

    // Step 2: Select a role
    let select = Select::new("Role").choices(["admin", "user", "guest"]);
    let mut reader = input("2\n");
    let role = select.ask_from(&console, &mut reader).unwrap();
    assert_eq!(role, "user");

    // Step 3: Confirm
    let confirm = Confirm::new("Create account?").default(true);
    let mut reader = input("y\n");
    let confirmed = confirm.ask_from(&console, &mut reader).unwrap();
    assert!(confirmed);
}

/// Test: Non-interactive workflow uses all defaults.
#[test]
fn test_non_interactive_all_defaults() {
    init_test_logging();

    let console = non_interactive_console();

    let name = Prompt::new("Name").default("System").ask(&console).unwrap();
    assert_eq!(name, "System");

    let role = Select::new("Role")
        .choices(["admin", "user"])
        .default("user")
        .ask(&console)
        .unwrap();
    assert_eq!(role, "user");

    let confirmed = Confirm::new("Proceed?")
        .default(true)
        .ask(&console)
        .unwrap();
    assert!(confirmed);
}
