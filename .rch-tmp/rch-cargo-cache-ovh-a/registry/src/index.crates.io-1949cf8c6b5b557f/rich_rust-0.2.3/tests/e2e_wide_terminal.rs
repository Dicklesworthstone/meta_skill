//! E2E tests for wide terminal handling (bd-2tvq).
//!
//! These tests verify that renderables handle very wide terminals (300+ columns)
//! without producing excessive whitespace or layout issues.
//!
//! ## Background
//!
//! When running on very wide terminals (e.g., 382 columns in WezTerm on Mac),
//! the Columns renderable was stretching content across the entire width,
//! creating massive gaps between column items.
//!
//! ## Fix
//!
//! Added `max_width` option to Columns to limit expansion on wide terminals
//! while still allowing some expansion for a polished look.

use rich_rust::console::Console;
use rich_rust::renderables::align::AlignMethod;
use rich_rust::renderables::columns::Columns;

/// Helper: count maximum consecutive spaces in a string.
fn max_consecutive_spaces(s: &str) -> usize {
    let mut max = 0;
    let mut current = 0;
    for ch in s.chars() {
        if ch == ' ' {
            current += 1;
            if current > max {
                max = current;
            }
        } else {
            current = 0;
        }
    }
    max
}

/// Test that Columns without max_width stretches excessively on wide terminals.
/// This documents the bug behavior before the fix.
#[test]
fn columns_without_max_width_stretches_on_wide_terminal() {
    println!("[TEST] Columns WITHOUT max_width on 400-column terminal");

    let cols = Columns::from_strings(&["A", "B", "C"])
        .column_count(3)
        .gutter(4)
        .expand(true); // No max_width

    let lines = cols.render(400);
    let text: String = lines[0].iter().map(|s| s.text.as_ref()).collect();

    println!("[TEST] Rendered line length: {}", text.len());
    println!(
        "[TEST] Max consecutive spaces: {}",
        max_consecutive_spaces(&text)
    );

    // Without max_width, the output fills 400 columns with massive gaps
    assert!(
        text.len() > 300,
        "Without max_width, columns should expand to fill wide terminal"
    );
    assert!(
        max_consecutive_spaces(&text) > 50,
        "Without max_width, there should be 50+ consecutive spaces"
    );
    println!("[TEST] PASS: Documented bug behavior (excessive stretching)");
}

/// Test that Columns WITH max_width limits expansion on wide terminals.
/// This verifies the fix works correctly.
#[test]
fn columns_with_max_width_limits_expansion() {
    println!("[TEST] Columns WITH max_width on 400-column terminal");

    let cols = Columns::from_strings(&["A", "B", "C"])
        .column_count(3)
        .gutter(4)
        .expand(true)
        .max_width(100);

    let lines = cols.render(400);
    let text: String = lines[0].iter().map(|s| s.text.as_ref()).collect();

    println!("[TEST] Rendered line length: {}", text.len());
    println!(
        "[TEST] Max consecutive spaces: {}",
        max_consecutive_spaces(&text)
    );

    // With max_width=100, output is capped (for ASCII content, byte length = cell width)
    assert!(
        text.len() <= 100,
        "With max_width=100, line should not exceed 100 columns, got {}",
        text.len()
    );
    assert!(
        max_consecutive_spaces(&text) < 50,
        "With max_width, should not have 50+ consecutive spaces"
    );
    println!("[TEST] PASS: max_width limits expansion");
}

/// Test that max_width doesn't affect narrow terminals.
#[test]
fn max_width_no_effect_on_narrow_terminal() {
    println!("[TEST] max_width on 80-column terminal (should have no effect)");

    let cols = Columns::from_strings(&["Hello", "World", "Test"])
        .column_count(3)
        .gutter(4)
        .expand(true)
        .max_width(200); // Higher than terminal width

    let lines = cols.render(80);
    let text: String = lines[0].iter().map(|s| s.text.as_ref()).collect();

    println!("[TEST] Rendered line length: {}", text.len());

    // Should use terminal width (80), not max_width (200)
    assert!(
        text.len() <= 80,
        "Should not exceed terminal width 80, got {}",
        text.len()
    );
    println!("[TEST] PASS: max_width doesn't affect narrow terminals");
}

/// Test real-world scenario: demo_showcase layout scene features.
#[test]
fn layout_scene_features_at_382_columns() {
    println!("[TEST] Layout scene features at 382 columns (user-reported width)");

    let features = [
        "Tables", "Panels", "Trees", "Progress", "Syntax", "Markdown",
    ];

    // Configuration from layout_scene.rs
    let cols = Columns::from_strings(&features)
        .column_count(3)
        .gutter(4)
        .equal_width(true)
        .align(AlignMethod::Center)
        .max_width(100);

    let lines = cols.render(382);

    for (i, line) in lines.iter().enumerate() {
        let text: String = line.iter().map(|s| s.text.as_ref()).collect();
        println!("[TEST] Line {}: '{}' (len={})", i, text.trim(), text.len());

        // Verify no excessive whitespace
        let spaces = max_consecutive_spaces(&text);
        assert!(
            spaces < 30,
            "Line {} has {} consecutive spaces, expected < 30",
            i,
            spaces
        );

        // Verify line width is reasonable
        assert!(
            text.len() <= 100,
            "Line {} width {} exceeds max_width 100",
            i,
            text.len()
        );
    }

    println!("[TEST] PASS: Layout scene features work at 382 columns");
}

/// Test real-world scenario: demo_showcase layout scene cards.
#[test]
fn layout_scene_cards_at_382_columns() {
    println!("[TEST] Layout scene cards at 382 columns");

    let cards = [
        "Tables: Structured data",
        "Panels: Bordered content",
        "Trees: Hierarchical views",
        "Progress: Live updates",
    ];

    // Configuration from layout_scene.rs
    let cols = Columns::from_strings(&cards)
        .column_count(2)
        .gutter(4)
        .equal_width(true)
        .max_width(100);

    let lines = cols.render(382);

    for (i, line) in lines.iter().enumerate() {
        let text: String = line.iter().map(|s| s.text.as_ref()).collect();
        println!("[TEST] Line {}: '{}' (len={})", i, text.trim(), text.len());

        let spaces = max_consecutive_spaces(&text);
        assert!(
            spaces < 30,
            "Line {} has {} consecutive spaces, expected < 30",
            i,
            spaces
        );

        assert!(
            text.len() <= 100,
            "Line {} width {} exceeds max_width 100",
            i,
            text.len()
        );
    }

    println!("[TEST] PASS: Layout scene cards work at 382 columns");
}

/// Test various wide terminal widths.
#[test]
fn columns_at_various_wide_widths() {
    println!("[TEST] Columns at various wide terminal widths");

    let widths = [200, 300, 400, 500];

    for width in widths {
        let cols = Columns::from_strings(&["Item1", "Item2", "Item3"])
            .column_count(3)
            .gutter(4)
            .expand(true)
            .max_width(120);

        let lines = cols.render(width);
        let text: String = lines[0].iter().map(|s| s.text.as_ref()).collect();

        println!("[TEST] Width {}: output len={}", width, text.len());

        assert!(
            text.len() <= 120,
            "At width {}, output {} exceeds max_width 120",
            width,
            text.len()
        );
    }

    println!("[TEST] PASS: Columns behave consistently at all wide widths");
}

/// Test that Columns via Renderable trait respects max_width.
#[test]
fn columns_renderable_trait_respects_max_width() {
    println!("[TEST] Columns via Renderable trait on wide console");

    use rich_rust::console::ConsoleOptions;
    use rich_rust::renderables::Renderable;

    let console = Console::builder().width(400).force_terminal(false).build();

    let cols = Columns::from_strings(&["A", "B", "C"])
        .column_count(3)
        .gutter(4)
        .expand(true)
        .max_width(100);

    let options = ConsoleOptions {
        max_width: 400,
        ..Default::default()
    };

    let segments = Renderable::render(&cols, &console, &options);
    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();

    // Remove newlines and check the first line
    let first_line = text.lines().next().unwrap_or("");

    println!(
        "[TEST] First line: '{}' (len={})",
        first_line.trim(),
        first_line.len()
    );

    assert!(
        first_line.len() <= 100,
        "Renderable output should respect max_width, got {}",
        first_line.len()
    );

    println!("[TEST] PASS: Renderable trait respects max_width");
}
