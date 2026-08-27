//! Conformance test suite for rich_rust.
//!
//! This test file provides triple-duty testing infrastructure:
//! 1. Integration tests - verify correct behavior
//! 2. Conformance tests - compare against Python Rich expectations
//! 3. Performance baseline - reusable in benchmarks
//!
//! # Running Tests
//!
//! ```bash
//! # Run all conformance tests
//! cargo test --test conformance_test
//!
//! # Run with output
//! cargo test --test conformance_test -- --nocapture
//! ```

mod conformance;

use conformance::rule_tests;
use conformance::table_tests;
use conformance::text_tests;
use conformance::{TestCase, run_test};

// =============================================================================
// Text Conformance Tests
// =============================================================================

#[test]
fn conformance_text_plain() {
    let test = text_tests::MarkupTextTest {
        name: "plain_text",
        markup: "Hello, World!",
        width: 80,
    };
    let output = run_test(&test);
    assert_eq!(output, "Hello, World!");
}

#[test]
fn conformance_text_bold() {
    let test = text_tests::MarkupTextTest {
        name: "bold_text",
        markup: "[bold]Bold text[/]",
        width: 80,
    };
    let output = run_test(&test);
    assert_eq!(conformance::strip_ansi(&output), "Bold text");
}

#[test]
fn conformance_text_colors() {
    let test = text_tests::MarkupTextTest {
        name: "colored_text",
        markup: "[red]Red[/] and [green]Green[/]",
        width: 80,
    };
    let output = run_test(&test);
    assert_eq!(conformance::strip_ansi(&output), "Red and Green");
}

#[test]
fn conformance_text_nested_styles() {
    let test = text_tests::MarkupTextTest {
        name: "nested_styles",
        markup: "[bold]Bold [italic]and italic[/italic] text[/bold]",
        width: 80,
    };
    let output = run_test(&test);
    assert_eq!(conformance::strip_ansi(&output), "Bold and italic text");
}

#[test]
fn conformance_all_text_tests() {
    for test in text_tests::standard_text_tests() {
        let test_ref: &dyn TestCase = test.as_ref();
        let output = run_test(test_ref);
        println!("Test '{}': {} chars", test_ref.name(), output.len());
        assert!(
            !output.is_empty(),
            "Test '{}' produced empty output",
            test_ref.name()
        );
    }
}

// =============================================================================
// Rule Conformance Tests
// =============================================================================

#[test]
fn conformance_rule_no_title() {
    let test = rule_tests::RuleTest {
        name: "rule_no_title",
        title: None,
        character: None,
        align: None,
        width: 40,
    };
    let output = run_test(&test);
    assert!(
        output.contains('─') || output.contains('-'),
        "Rule without title should render a horizontal line"
    );
}

#[test]
fn conformance_rule_with_title() {
    let test = rule_tests::RuleTest {
        name: "rule_with_title",
        title: Some("Section"),
        character: None,
        align: None,
        width: 40,
    };
    let output = run_test(&test);
    assert!(output.contains("Section"));
}

#[test]
fn conformance_all_rule_tests() {
    for test in rule_tests::standard_rule_tests() {
        let test_ref: &dyn TestCase = test.as_ref();
        let output = run_test(test_ref);
        println!("Test '{}': {} chars", test_ref.name(), output.len());
        assert!(
            !output.is_empty(),
            "Test '{}' produced empty output",
            test_ref.name()
        );
    }
}

// =============================================================================
// Table Conformance Tests
// =============================================================================

#[test]
fn conformance_table_simple() {
    let test = table_tests::TableTest {
        name: "table_simple",
        columns: vec!["Name", "Age"],
        rows: vec![vec!["Alice", "30"]],
        width: 40,
        show_header: true,
        show_lines: false,
    };
    let output = run_test(&test);
    assert!(output.contains("Alice"));
    assert!(output.contains("30"));
}

#[test]
fn conformance_table_with_lines() {
    let test = table_tests::TableTest {
        name: "table_with_lines",
        columns: vec!["A", "B"],
        rows: vec![vec!["1", "2"], vec!["3", "4"]],
        width: 30,
        show_header: true,
        show_lines: true,
    };
    let output = run_test(&test);
    assert!(output.contains("1"));
    assert!(output.contains("4"));
}

#[test]
fn conformance_all_table_tests() {
    for test in table_tests::standard_table_tests() {
        let test_ref: &dyn TestCase = test.as_ref();
        let output = run_test(test_ref);
        println!("Test '{}': {} chars", test_ref.name(), output.len());
        assert!(
            !output.is_empty(),
            "Test '{}' produced empty output",
            test_ref.name()
        );
    }
}

// =============================================================================
// Python Rich Comparison (Manual Verification)
// =============================================================================

/// Print Python Rich equivalent code for manual verification.
/// Run with: cargo test --test conformance_test print_python_equivalents -- --nocapture --ignored
#[test]
#[ignore]
fn print_python_equivalents() {
    println!("\n=== Python Rich Equivalent Code ===\n");

    for test in text_tests::standard_text_tests() {
        let test_ref: &dyn TestCase = test.as_ref();
        println!("--- {} ---", test_ref.name());
        if let Some(code) = test_ref.python_rich_code() {
            println!("{}\n", code);
        } else {
            println!("(No Python equivalent)\n");
        }
    }

    for test in rule_tests::standard_rule_tests() {
        let test_ref: &dyn TestCase = test.as_ref();
        println!("--- {} ---", test_ref.name());
        if let Some(code) = test_ref.python_rich_code() {
            println!("{}\n", code);
        } else {
            println!("(No Python equivalent)\n");
        }
    }

    for test in table_tests::standard_table_tests() {
        let test_ref: &dyn TestCase = test.as_ref();
        println!("--- {} ---", test_ref.name());
        if let Some(code) = test_ref.python_rich_code() {
            println!("{}\n", code);
        } else {
            println!("(No Python equivalent)\n");
        }
    }
}
