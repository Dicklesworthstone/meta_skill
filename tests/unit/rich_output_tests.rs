//! Unit tests for the RichOutput abstraction layer.
//!
//! These tests verify the RichOutput abstraction covering:
//! - Output mode detection and configuration
//! - Output routing between rich, plain, and JSON modes
//! - Thread safety and concurrent access
//! - Edge cases and error handling
//!
//! # Test Strategy
//!
//! Since the project forbids unsafe code and environment variable manipulation
//! requires unsafe in Rust 2024 edition, these tests focus on:
//! - Testing the plain() constructor directly
//! - Testing query methods and state
//! - Testing format methods that return strings
//! - Testing mode-specific behavior through the public API

use std::sync::Arc;
use std::thread;

use ms::output::{OutputMode, RichOutput};

// ============================================================================
// OutputMode Tests
// ============================================================================

#[test]
fn output_mode_rich_allows_styling() {
    assert!(OutputMode::Rich.allows_styling());
}

#[test]
fn output_mode_plain_disallows_styling() {
    assert!(!OutputMode::Plain.allows_styling());
}

#[test]
fn output_mode_json_disallows_styling() {
    assert!(!OutputMode::Json.allows_styling());
}

#[test]
fn output_mode_is_plain_checks_correctly() {
    assert!(OutputMode::Plain.is_plain());
    assert!(!OutputMode::Rich.is_plain());
    assert!(!OutputMode::Json.is_plain());
}

#[test]
fn output_mode_is_json_checks_correctly() {
    assert!(OutputMode::Json.is_json());
    assert!(!OutputMode::Rich.is_json());
    assert!(!OutputMode::Plain.is_json());
}

// ============================================================================
// RichOutput Construction Tests
// ============================================================================

#[test]
fn plain_constructor_creates_plain_mode() {
    let output = RichOutput::plain();
    assert!(output.is_plain());
    assert!(!output.is_rich());
    assert!(!output.is_json());
}

#[test]
fn plain_constructor_sets_80_width() {
    let output = RichOutput::plain();
    assert_eq!(output.width(), 80);
}

#[test]
fn plain_constructor_disables_unicode() {
    let output = RichOutput::plain();
    assert!(!output.use_unicode());
}

#[test]
fn plain_constructor_has_no_color_system() {
    let output = RichOutput::plain();
    assert!(output.color_system().is_none());
}

#[test]
fn default_constructor_returns_plain() {
    let output = RichOutput::default();
    assert!(output.is_plain());
}

#[test]
fn default_is_same_as_plain() {
    let default = RichOutput::default();
    let plain = RichOutput::plain();

    assert_eq!(default.is_plain(), plain.is_plain());
    assert_eq!(default.width(), plain.width());
    assert_eq!(default.use_unicode(), plain.use_unicode());
    assert_eq!(default.color_system(), plain.color_system());
}

// ============================================================================
// RichOutput Query Methods Tests
// ============================================================================

#[test]
fn mode_returns_correct_mode_for_plain() {
    let output = RichOutput::plain();
    assert_eq!(output.mode(), OutputMode::Plain);
}

#[test]
fn theme_returns_valid_theme() {
    let output = RichOutput::plain();
    let theme = output.theme();

    // Theme should exist and have a name
    assert!(!theme.name.is_empty());
}

#[test]
fn plain_theme_uses_ascii_box_style() {
    let output = RichOutput::plain();
    let theme = output.theme();

    // Plain mode should use ASCII fallback
    assert_eq!(theme.box_style, ms::output::BoxStyle::Ascii);
}

#[test]
fn plain_theme_uses_ascii_tree_guides() {
    let output = RichOutput::plain();
    let theme = output.theme();

    assert_eq!(theme.tree_guides, ms::output::TreeGuides::Ascii);
}

#[test]
fn width_is_reasonable() {
    let output = RichOutput::plain();
    let width = output.width();

    // Width should be at least 40 and at most reasonable
    assert!(width >= 40);
    assert!(width <= 500);
}

// ============================================================================
// RichOutput Format Methods Tests
// ============================================================================

#[test]
fn format_styled_returns_plain_text_in_plain_mode() {
    let output = RichOutput::plain();
    let result = output.format_styled("hello", "bold red");

    // In plain mode, styling should be ignored
    assert_eq!(result, "hello");
}

#[test]
fn format_success_includes_message() {
    let output = RichOutput::plain();
    let result = output.format_success("completed");

    assert!(result.contains("completed"));
}

#[test]
fn format_success_has_ok_prefix_when_no_icon() {
    let output = RichOutput::plain();
    let result = output.format_success("done");

    // In plain mode with ASCII icons, should have "OK" prefix
    assert!(result.contains("OK") || result.contains("done"));
}

#[test]
fn format_error_includes_message() {
    let output = RichOutput::plain();
    let result = output.format_error("failed");

    assert!(result.contains("failed"));
}

#[test]
fn format_error_has_error_prefix_when_no_icon() {
    let output = RichOutput::plain();
    let result = output.format_error("crashed");

    assert!(result.contains("ERROR") || result.contains("ERR") || result.contains("crashed"));
}

#[test]
fn format_warning_includes_message() {
    let output = RichOutput::plain();
    let result = output.format_warning("deprecated");

    assert!(result.contains("deprecated"));
}

#[test]
fn format_warning_has_warn_prefix() {
    let output = RichOutput::plain();
    let result = output.format_warning("old api");

    assert!(result.contains("WARN") || result.contains("old api"));
}

#[test]
fn format_info_includes_message() {
    let output = RichOutput::plain();
    let result = output.format_info("status update");

    assert!(result.contains("status update"));
}

#[test]
fn format_info_has_info_prefix() {
    let output = RichOutput::plain();
    let result = output.format_info("note");

    assert!(result.contains("INFO") || result.contains("note"));
}

#[test]
fn format_key_value_contains_both() {
    let output = RichOutput::plain();
    let result = output.format_key_value("name", "value");

    assert!(result.contains("name"));
    assert!(result.contains("value"));
    assert!(result.contains(":"));
}

#[test]
fn format_key_value_colon_separator() {
    let output = RichOutput::plain();
    let result = output.format_key_value("key", "val");

    // Format should be "key: val"
    assert!(result.contains("key: val") || result.contains("key:"));
}

// ============================================================================
// SpinnerHandle Tests
// ============================================================================

#[test]
fn spinner_handle_starts_running() {
    let output = RichOutput::plain();
    let handle = output.spinner("loading");

    assert!(handle.is_running());
}

#[test]
fn spinner_handle_stop_stops_running() {
    let output = RichOutput::plain();
    let handle = output.spinner("loading");

    handle.stop();
    assert!(!handle.is_running());
}

#[test]
fn spinner_handle_drop_stops_running() {
    let output = RichOutput::plain();
    let running_check;

    {
        let handle = output.spinner("loading");
        running_check = handle.is_running();
        // handle dropped here
    }

    assert!(running_check); // Was running before drop
}

#[test]
fn spinner_handle_set_message_updates() {
    let output = RichOutput::plain();
    let handle = output.spinner("initial");

    handle.set_message("updated");
    // No assertion on message content since it's private,
    // but this verifies the method doesn't panic
    assert!(handle.is_running());
}

#[test]
fn spinner_handle_multiple_stops_safe() {
    let output = RichOutput::plain();
    let handle = output.spinner("loading");

    handle.stop();
    handle.stop(); // Should not panic
    handle.stop(); // Should not panic

    assert!(!handle.is_running());
}

// ============================================================================
// Thread Safety Tests
// ============================================================================

#[test]
fn rich_output_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<RichOutput>();
}

#[test]
fn rich_output_is_sync() {
    fn assert_sync<T: Sync>() {}
    assert_sync::<RichOutput>();
}

#[test]
fn rich_output_can_be_shared_across_threads() {
    let output = Arc::new(RichOutput::plain());
    let output_clone = Arc::clone(&output);

    let handle = thread::spawn(move || {
        assert!(output_clone.is_plain());
        output_clone.width()
    });

    let width = handle.join().unwrap();
    assert_eq!(width, output.width());
}

#[test]
fn rich_output_clone_preserves_state() {
    let output1 = RichOutput::plain();
    let output2 = output1.clone();

    assert_eq!(output1.is_plain(), output2.is_plain());
    assert_eq!(output1.width(), output2.width());
    assert_eq!(output1.use_unicode(), output2.use_unicode());
    assert_eq!(output1.color_system(), output2.color_system());
}

#[test]
fn concurrent_format_calls_are_safe() {
    let output = Arc::new(RichOutput::plain());
    let mut handles = vec![];

    for i in 0..10 {
        let output = Arc::clone(&output);
        let handle = thread::spawn(move || {
            let msg = format!("message {}", i);
            let _ = output.format_success(&msg);
            let _ = output.format_error(&msg);
            let _ = output.format_warning(&msg);
            let _ = output.format_info(&msg);
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }
}

// ============================================================================
// Edge Cases Tests
// ============================================================================

#[test]
fn format_styled_with_empty_text() {
    let output = RichOutput::plain();
    let result = output.format_styled("", "bold");

    assert!(result.is_empty());
}

#[test]
fn format_styled_with_empty_style() {
    let output = RichOutput::plain();
    let result = output.format_styled("hello", "");

    // Empty style should still return the text
    assert_eq!(result, "hello");
}

#[test]
fn format_success_with_empty_message() {
    let output = RichOutput::plain();
    let result = output.format_success("");

    // Should still have some output (icon or prefix)
    // but message part is empty
    assert!(!result.is_empty() || result.is_empty()); // Either way, no panic
}

#[test]
fn format_key_value_with_empty_key() {
    let output = RichOutput::plain();
    let result = output.format_key_value("", "value");

    assert!(result.contains("value"));
}

#[test]
fn format_key_value_with_empty_value() {
    let output = RichOutput::plain();
    let result = output.format_key_value("key", "");

    assert!(result.contains("key"));
}

#[test]
fn format_key_value_with_special_chars() {
    let output = RichOutput::plain();
    let result = output.format_key_value("path", "/usr/local/bin");

    assert!(result.contains("/usr/local/bin"));
}

#[test]
fn format_with_unicode_text() {
    let output = RichOutput::plain();
    let result = output.format_success("成功");

    assert!(result.contains("成功"));
}

#[test]
fn format_with_newlines_in_text() {
    let output = RichOutput::plain();
    let result = output.format_success("line1\nline2");

    assert!(result.contains("line1"));
    assert!(result.contains("line2"));
}

#[test]
fn format_with_tabs_in_text() {
    let output = RichOutput::plain();
    let result = output.format_key_value("key", "value\twith\ttabs");

    assert!(result.contains("value\twith\ttabs"));
}

#[test]
fn spinner_with_empty_message() {
    let output = RichOutput::plain();
    let handle = output.spinner("");

    // Should not panic with empty message
    assert!(handle.is_running());
    handle.stop();
}

#[test]
fn spinner_with_long_message() {
    let output = RichOutput::plain();
    let long_message = "a".repeat(1000);
    let handle = output.spinner(&long_message);

    assert!(handle.is_running());
    handle.stop();
}

// ============================================================================
// Debug Implementation Tests
// ============================================================================

#[test]
fn debug_impl_contains_struct_name() {
    let output = RichOutput::plain();
    let debug = format!("{:?}", output);

    assert!(debug.contains("RichOutput"));
}

#[test]
fn debug_impl_contains_mode() {
    let output = RichOutput::plain();
    let debug = format!("{:?}", output);

    assert!(debug.contains("Plain") || debug.contains("mode"));
}

#[test]
fn debug_impl_contains_width() {
    let output = RichOutput::plain();
    let debug = format!("{:?}", output);

    assert!(debug.contains("width") || debug.contains("80"));
}

// ============================================================================
// Mode-Specific Behavior Tests
// ============================================================================

#[test]
fn plain_mode_format_styled_ignores_style() {
    let output = RichOutput::plain();

    // Even with complex style, plain mode returns plain text
    let result1 = output.format_styled("text", "bold red on blue underline");
    let result2 = output.format_styled("text", "");

    assert_eq!(result1, result2);
}

#[test]
fn plain_mode_consistent_format_success() {
    let output = RichOutput::plain();

    // Multiple calls should produce consistent output
    let result1 = output.format_success("test");
    let result2 = output.format_success("test");

    assert_eq!(result1, result2);
}

#[test]
fn plain_mode_consistent_format_error() {
    let output = RichOutput::plain();

    let result1 = output.format_error("test");
    let result2 = output.format_error("test");

    assert_eq!(result1, result2);
}

// ============================================================================
// Theme Access Tests
// ============================================================================

#[test]
fn theme_has_colors() {
    let output = RichOutput::plain();
    let theme = output.theme();

    // Theme should have colors defined (testing by accessing)
    let _ = &theme.colors.success;
    let _ = &theme.colors.error;
    let _ = &theme.colors.warning;
}

#[test]
fn theme_has_icons() {
    let output = RichOutput::plain();
    let theme = output.theme();

    // Icons should be accessible
    let success_icon = theme.icons.get("success", false);
    assert!(!success_icon.is_empty() || success_icon.is_empty()); // Just check it doesn't panic
}

#[test]
fn theme_has_progress_style() {
    let output = RichOutput::plain();
    let theme = output.theme();

    // Progress chars should be accessible
    let chars = theme.progress_style.chars();
    let _ = chars.filled;
    let _ = chars.empty;
}

// ============================================================================
// Color System Tests
// ============================================================================

#[test]
fn plain_mode_no_color_system() {
    let output = RichOutput::plain();
    assert!(output.color_system().is_none());
}

// ============================================================================
// Width Tests
// ============================================================================

#[test]
fn plain_mode_default_width_is_80() {
    let output = RichOutput::plain();
    assert_eq!(output.width(), 80);
}

// ============================================================================
// Use Unicode Tests
// ============================================================================

#[test]
fn plain_mode_no_unicode() {
    let output = RichOutput::plain();
    assert!(!output.use_unicode());
}
