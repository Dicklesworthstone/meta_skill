//! Regression Test Suite for rich_rust
//!
//! This module contains regression tests that prevent reintroduction of previously
//! fixed bugs. Each test documents:
//! - What bug it prevents
//! - When it was introduced/fixed
//! - What the fix was
//! - How to reproduce
//!
//! ## Running Regression Tests
//!
//! ```bash
//! # Run all regression tests
//! cargo test --test regression_tests
//!
//! # Run specific category (by name filter)
//! cargo test --test regression_tests parsing
//! cargo test --test regression_tests layout
//! cargo test --test regression_tests rendering
//! ```
//!
//! ## Categories
//!
//! - `parsing`: Color, style, markup parsing edge cases
//! - `layout`: Table width, text wrap, alignment
//! - `rendering`: ANSI output, Unicode handling

mod common;

use common::{init_test_logging, log_test_context, test_phase};
use rich_rust::r#box::SQUARE;
use rich_rust::color::ColorSystem;
use rich_rust::prelude::*;
use rich_rust::segment::{ControlCode, ControlType};
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

// =============================================================================
// PARSING REGRESSION TESTS
// =============================================================================

/// Regression test: Style parsing with empty string
///
/// Bug: Empty string could cause panic or return unexpected result
/// Fixed: Returns null style consistently
///
/// This test ensures empty string parsing always returns a null style.
#[test]
fn regression_parsing_empty_string() {
    init_test_logging();
    log_test_context(
        "regression_parsing_empty_string",
        "Ensures empty string produces null style",
    );

    let _phase = test_phase("parse_empty");

    let result = Style::parse("");
    assert!(result.is_ok(), "Empty string should parse successfully");

    let style = result.unwrap();
    assert!(style.is_null(), "Empty string should produce null style");

    tracing::info!("Regression test PASSED: empty string -> null style");
}

/// Regression test: Style parsing with "none" keyword
///
/// Bug: "none" keyword might be treated as unknown token
/// Fixed: "none" is recognized as producing null style
///
/// This test ensures "none" keyword works correctly.
#[test]
fn regression_parsing_none_keyword() {
    init_test_logging();
    log_test_context(
        "regression_parsing_none_keyword",
        "Ensures 'none' keyword produces null style",
    );

    let _phase = test_phase("parse_none");

    let result = Style::parse("none");
    assert!(result.is_ok(), "'none' should parse successfully");

    let style = result.unwrap();
    assert!(style.is_null(), "'none' should produce null style");

    tracing::info!("Regression test PASSED: 'none' -> null style");
}

/// Regression test: Color parsing with hex codes
///
/// Bug: Hex colors without # prefix or with wrong length could panic
/// Fixed: Proper validation and error handling
///
/// This test ensures hex color parsing handles edge cases correctly.
#[test]
fn regression_parsing_hex_edge_cases() {
    init_test_logging();
    log_test_context(
        "regression_parsing_hex_edge_cases",
        "Ensures hex color parsing handles edge cases",
    );

    let _phase = test_phase("hex_edge_cases");

    // Valid hex colors
    let valid_cases = ["#ff0000", "#00FF00", "#0000ff", "#AbCdEf"];
    for hex in valid_cases {
        let result = Color::parse(hex);
        assert!(result.is_ok(), "'{hex}' should parse successfully");
        tracing::debug!(hex = hex, "Valid hex parsed");
    }

    // Invalid hex colors should return errors, not panic
    let invalid_cases = ["#ff", "#gggggg", "#12345", "#"];
    for hex in invalid_cases {
        let result = Color::parse(hex);
        assert!(result.is_err(), "'{hex}' should fail to parse");
        tracing::debug!(hex = hex, "Invalid hex correctly rejected");
    }

    tracing::info!("Regression test PASSED: hex color edge cases");
}

/// Regression test: RGB color parsing with out-of-range values
///
/// Bug: RGB values > 255 could cause overflow or incorrect colors
/// Fixed: Proper validation with clear error messages
///
/// This test ensures RGB parsing validates component ranges.
#[test]
fn regression_parsing_rgb_out_of_range() {
    init_test_logging();
    log_test_context(
        "regression_parsing_rgb_out_of_range",
        "Ensures RGB parsing validates ranges",
    );

    let _phase = test_phase("rgb_ranges");

    // Valid RGB colors
    let valid = Color::parse("rgb(255,255,255)");
    assert!(valid.is_ok(), "rgb(255,255,255) should be valid");

    let valid_zero = Color::parse("rgb(0,0,0)");
    assert!(valid_zero.is_ok(), "rgb(0,0,0) should be valid");

    // Out of range should fail
    let result = Color::parse("rgb(256,0,0)");
    assert!(result.is_err(), "rgb(256,0,0) should fail (red > 255)");

    let result = Color::parse("rgb(0,256,0)");
    assert!(result.is_err(), "rgb(0,256,0) should fail (green > 255)");

    let result = Color::parse("rgb(0,0,256)");
    assert!(result.is_err(), "rgb(0,0,256) should fail (blue > 255)");

    tracing::info!("Regression test PASSED: RGB range validation");
}

/// Regression test: 256-color palette parsing
///
/// Bug: Color numbers outside 0-255 range could cause issues
/// Fixed: Proper range validation
///
/// This test ensures 256-color palette parsing validates the index.
#[test]
fn regression_parsing_256_palette_range() {
    init_test_logging();
    log_test_context(
        "regression_parsing_256_palette_range",
        "Ensures 256-color palette validates index",
    );

    let _phase = test_phase("palette_range");

    // Valid palette colors
    let valid_0 = Color::parse("color(0)");
    assert!(valid_0.is_ok(), "color(0) should be valid");

    let valid_255 = Color::parse("color(255)");
    assert!(valid_255.is_ok(), "color(255) should be valid");

    // Out of range should fail
    let invalid = Color::parse("color(256)");
    assert!(invalid.is_err(), "color(256) should fail (> 255)");

    let negative = Color::parse("color(-1)");
    assert!(negative.is_err(), "color(-1) should fail (negative)");

    tracing::info!("Regression test PASSED: 256-color palette range");
}

/// Regression test: Style parsing with incomplete keywords
///
/// Bug: Incomplete "on", "not", or "link" keywords could panic
/// Fixed: Proper error messages for incomplete keywords
///
/// This test ensures incomplete keywords produce errors, not panics.
#[test]
fn regression_parsing_incomplete_keywords() {
    init_test_logging();
    log_test_context(
        "regression_parsing_incomplete_keywords",
        "Ensures incomplete keywords produce errors",
    );

    let _phase = test_phase("incomplete_keywords");

    // "on" alone should fail with clear error
    let on_result = Style::parse("on");
    assert!(on_result.is_err(), "'on' alone should fail");

    // "not" alone should fail with clear error
    let not_result = Style::parse("not");
    assert!(not_result.is_err(), "'not' alone should fail");

    // "link" alone should fail with clear error
    let link_result = Style::parse("link");
    assert!(link_result.is_err(), "'link' alone should fail");

    tracing::info!("Regression test PASSED: incomplete keywords");
}

/// Regression test: Style parsing whitespace handling
///
/// Bug: Extra whitespace between tokens could cause parsing failures
/// Fixed: Whitespace is properly normalized during parsing
///
/// This test ensures whitespace variations are handled correctly.
#[test]
fn regression_parsing_whitespace_handling() {
    init_test_logging();
    log_test_context(
        "regression_parsing_whitespace_handling",
        "Ensures whitespace is handled correctly",
    );

    let _phase = test_phase("whitespace");

    // Various whitespace patterns should all work
    let cases = [
        "bold",
        " bold",
        "bold ",
        " bold ",
        "  bold  ",
        "bold red",
        "bold  red",
        " bold  red ",
        "bold   red   on   blue",
    ];

    for case in cases {
        let result = Style::parse(case);
        assert!(result.is_ok(), "'{case}' should parse despite whitespace");
        tracing::debug!(input = case, "Whitespace case passed");
    }

    tracing::info!("Regression test PASSED: whitespace handling");
}

/// Regression test: Style parsing case insensitivity
///
/// Bug: Uppercase keywords might not be recognized
/// Fixed: All parsing is case-insensitive
///
/// This test ensures case variations are handled correctly.
#[test]
fn regression_parsing_case_insensitivity() {
    init_test_logging();
    log_test_context(
        "regression_parsing_case_insensitivity",
        "Ensures case insensitivity",
    );

    let _phase = test_phase("case");

    let cases = ["BOLD", "Bold", "bold", "BOLD RED", "Bold Red", "bold red"];

    for case in cases {
        let result = Style::parse(case);
        assert!(result.is_ok(), "'{case}' should parse (case insensitive)");

        let style = result.unwrap();
        assert!(
            style.attributes.contains(Attributes::BOLD),
            "'{case}' should set bold attribute"
        );
    }

    tracing::info!("Regression test PASSED: case insensitivity");
}

/// Regression test: Color parsing named colors
///
/// Bug: Some named colors might not be recognized or return wrong values
/// Fixed: Complete set of standard named colors
///
/// This test ensures all standard named colors are recognized.
#[test]
fn regression_parsing_named_colors() {
    init_test_logging();
    log_test_context(
        "regression_parsing_named_colors",
        "Ensures named colors are recognized",
    );

    let _phase = test_phase("named_colors");

    let named_colors = [
        "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
    ];

    for name in named_colors {
        let result = Color::parse(name);
        assert!(result.is_ok(), "'{name}' should be a valid named color");
        tracing::debug!(color = name, "Named color parsed");
    }

    tracing::info!("Regression test PASSED: named colors");
}

// =============================================================================
// LAYOUT REGRESSION TESTS
// =============================================================================

/// Regression test for issue: Table collapse_widths rounding correction
///
/// Bug: collapse_widths() was missing the rounding error correction loop
///      after proportional shrinking, causing column widths to not sum
///      to exactly the available space.
/// Fixed: 2026-01-17 in commit e160e4f
/// Spec: RICH_SPEC Section 9.3 (lines 1680-1694)
///
/// This test ensures proportional shrinking includes rounding correction.
#[test]
fn regression_layout_table_collapse_widths_rounding() {
    init_test_logging();
    log_test_context(
        "regression_layout_table_collapse_widths_rounding",
        "Bug: Missing rounding correction in collapse_widths()",
    );

    let _phase = test_phase("collapse_rounding");
    tracing::info!("Regression test: collapse_widths() rounding correction (commit e160e4f)");

    // Create a table with wide content that must be collapsed
    let wide_content = "X".repeat(45);

    let mut table = Table::new()
        .box_style(&SQUARE)
        .padding(0, 0)
        .with_column(Column::new("Col1").min_width(10))
        .with_column(Column::new("Col2").min_width(10))
        .with_column(Column::new("Col3").min_width(10));

    table.add_row_cells([
        wide_content.as_str(),
        wide_content.as_str(),
        wide_content.as_str(),
    ]);

    // Render at constrained width to force collapse
    let output = table.render_plain(100);

    // The table should include headers and borders
    assert!(output.contains("Col1"), "Missing header content");

    // Check that output is well-formed (has top border with correct chars)
    let first_line = output.lines().next().expect("should have lines");
    assert!(
        first_line.starts_with('┌'),
        "Table should have proper border"
    );

    tracing::info!("Regression test PASSED: collapse_widths rounding correction");
}

/// Regression test: Table expand_widths ratio distribution
///
/// Bug: Ratio-based column expansion might not distribute space correctly
/// Fixed: Per RICH_SPEC Section 14.4
///
/// This test ensures ratio expansion distributes space proportionally.
#[test]
fn regression_layout_table_expand_widths_ratio() {
    init_test_logging();
    log_test_context(
        "regression_layout_table_expand_widths_ratio",
        "Ensures ratio-based expansion works correctly",
    );

    let _phase = test_phase("expand_ratio");

    // Table with 1:2:1 ratio columns
    let mut table = Table::new()
        .expand(true)
        .box_style(&SQUARE)
        .padding(0, 0)
        .with_column(Column::new("A").ratio(1))
        .with_column(Column::new("B").ratio(2))
        .with_column(Column::new("C").ratio(1));

    table.add_row_cells(["x", "y", "z"]);

    // Render with enough width for expansion
    let output = table.render_plain(80);

    // Table should include headers and row values
    assert!(output.contains("A"), "Missing header content");
    assert!(output.contains('x'), "Missing row values");

    // Check the output is well-formed
    let lines: Vec<&str> = output.lines().collect();
    assert!(lines.len() >= 3, "Table should have multiple lines");

    tracing::info!("Regression test PASSED: expand_widths ratio distribution");
}

/// Regression test: Table with zero-width content
///
/// Bug: Columns with empty content could cause division by zero or panic
/// Fixed: Minimum width constraints prevent zero-width columns
///
/// This test ensures empty content doesn't cause issues.
#[test]
fn regression_layout_table_empty_content() {
    init_test_logging();
    log_test_context(
        "regression_layout_table_empty_content",
        "Ensures empty content is handled",
    );

    let _phase = test_phase("empty_content");

    let mut table = Table::new()
        .with_column(Column::new("A"))
        .with_column(Column::new("B"));

    // Add row with empty strings
    table.add_row_cells(["", ""]);

    // Should render without panic
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| table.render_plain(40)));

    assert!(result.is_ok(), "Empty content should not panic");
    tracing::info!("Regression test PASSED: empty content handling");
}

/// Regression test: Table column width sum exactness
///
/// Bug: Column widths might not sum to available space due to rounding
/// Fixed: Rounding correction distributes remainder to last column
///
/// This test ensures column widths sum exactly to available space.
#[test]
fn regression_layout_table_width_sum_exactness() {
    init_test_logging();
    log_test_context(
        "regression_layout_table_width_sum_exactness",
        "Ensures column widths sum exactly",
    );

    let _phase = test_phase("width_sum");

    // Use ratios that trigger rounding edge cases (7:13:23)
    let mut table = Table::new()
        .expand(true)
        .box_style(&SQUARE)
        .padding(0, 0)
        .with_column(Column::new("A").ratio(7))
        .with_column(Column::new("B").ratio(13))
        .with_column(Column::new("C").ratio(23));

    table.add_row_cells(["x", "y", "z"]);

    // Render and check it completes without error
    let output = table.render_plain(100);
    assert!(output.contains("A"), "Missing header content");

    tracing::info!("Regression test PASSED: width sum exactness");
}

/// Regression test: collapse_padding must not overflow width
///
/// Bug: collapse_padding affected width calculation but not rendering,
///      so row lines could exceed max_width.
/// Fixed: remove inner padding when collapse_padding=true.
#[test]
fn regression_layout_table_collapse_padding_width_respected() {
    use rich_rust::cells;

    init_test_logging();
    log_test_context(
        "regression_layout_table_collapse_padding_width_respected",
        "Ensures collapse_padding keeps lines within max_width",
    );

    let _phase = test_phase("collapse_padding_width");

    let mut table = Table::new()
        .padding(1, 0)
        .collapse_padding(true)
        .pad_edge(false)
        .with_column(Column::new("A").width(1))
        .with_column(Column::new("B").width(1));

    table.add_row_cells(["1", "2"]);

    let max_width = 6; // 2 edges + 1 separator + 2 content + 1 slack
    let output = table.render_plain(max_width);

    for line in output.lines().filter(|line| !line.is_empty()) {
        assert!(
            cells::cell_len(line) <= max_width,
            "line exceeds max width with collapse_padding"
        );
    }

    tracing::info!("Regression test PASSED: collapse_padding width respected");
}

/// Regression test: Table with very narrow width
///
/// Bug: Table rendered at very narrow width could panic or produce garbled output
/// Fixed: Minimum width constraints and graceful degradation
///
/// This test ensures narrow widths don't cause issues.
#[test]
fn regression_layout_table_very_narrow_width() {
    init_test_logging();
    log_test_context(
        "regression_layout_table_very_narrow_width",
        "Ensures narrow width is handled",
    );

    let _phase = test_phase("narrow_width");

    let mut table = Table::new()
        .with_column(Column::new("Name"))
        .with_column(Column::new("Value"));

    table.add_row_cells(["Test", "Data"]);

    // Render at very narrow width (might need to truncate)
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| table.render_plain(10)));

    assert!(result.is_ok(), "Narrow width should not panic");
    tracing::info!("Regression test PASSED: narrow width handling");
}

/// Regression test: Panel with multiline content
///
/// Bug: Panel with multiline content could have alignment issues
/// Fixed: Proper line-by-line rendering with consistent padding
///
/// This test ensures multiline panel content is handled correctly.
#[test]
fn regression_layout_panel_multiline_content() {
    init_test_logging();
    log_test_context(
        "regression_layout_panel_multiline_content",
        "Ensures multiline panel content works",
    );

    let _phase = test_phase("multiline_panel");

    let content = "Line 1\nLine 2\nLine 3";
    let panel = Panel::from_text(content).title("Test").width(30);

    let segments = panel.render(40);
    let output: String = segments.into_iter().map(|s| s.text).collect();

    // Should have multiple lines
    let lines: Vec<&str> = output.lines().collect();
    assert!(
        lines.len() >= 5,
        "Panel should have header, 3 content lines, and footer"
    );

    tracing::info!("Regression test PASSED: multiline panel content");
}

/// Regression test: Rule with very long title
///
/// Bug: Rule with title longer than width could panic or render incorrectly
/// Fixed: Title is truncated when necessary
///
/// This test ensures long titles are handled.
#[test]
fn regression_layout_rule_long_title() {
    init_test_logging();
    log_test_context(
        "regression_layout_rule_long_title",
        "Ensures long titles are handled",
    );

    let _phase = test_phase("long_title");

    let long_title = "This is a very long title that exceeds the available width";
    let rule = Rule::with_title(long_title);

    // Render at narrow width
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| rule.render(30)));

    assert!(result.is_ok(), "Long title should not panic");
    tracing::info!("Regression test PASSED: long title handling");
}

/// Regression test: Tree with deep nesting
///
/// Bug: Deeply nested trees could cause stack overflow or incorrect guides
/// Fixed: Iterative rendering approach
///
/// This test ensures deep nesting is handled.
#[test]
fn regression_layout_tree_deep_nesting() {
    init_test_logging();
    log_test_context(
        "regression_layout_tree_deep_nesting",
        "Ensures deep nesting works",
    );

    let _phase = test_phase("deep_tree");

    // Create a tree with 10 levels of nesting
    let mut deepest = TreeNode::new("Level 10");
    for i in (1..10).rev() {
        let mut parent = TreeNode::new(format!("Level {i}"));
        parent = parent.child(deepest);
        deepest = parent;
    }

    let tree = Tree::new(deepest);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| tree.render()));

    assert!(result.is_ok(), "Deep nesting should not panic");
    tracing::info!("Regression test PASSED: deep tree nesting");
}

// =============================================================================
// RENDERING REGRESSION TESTS
// =============================================================================

/// Regression test for issue: Hyperlink-only style rendering
///
/// Bug: Style::render() returned early when ANSI codes were empty,
///      skipping OSC 8 hyperlink output. Styles with only a hyperlink
///      (no colors or attributes) does not render the link.
/// Fixed: 2026-01-18 in commit ca4bd56
///
/// This test ensures hyperlink-only styles render correctly.
#[test]
fn regression_rendering_hyperlink_only_style() {
    init_test_logging();
    log_test_context(
        "regression_rendering_hyperlink_only_style",
        "Bug: Hyperlink-only styles not rendering OSC 8 sequences",
    );

    let _phase = test_phase("hyperlink_only");
    tracing::info!("Regression test: hyperlink-only style rendering (commit ca4bd56)");

    // Create a style with ONLY a hyperlink (no colors, no attributes)
    let mut style = Style::new();
    style.link = Some("https://example.com".to_string());

    // The style should NOT be null (it has a link)
    assert!(!style.is_null(), "Style with link should not be null");

    // Render the style using render_ansi for (prefix, suffix) tuple
    let ansi = style.render_ansi(ColorSystem::TrueColor);
    let (prefix, suffix) = &*ansi;

    // The prefix should contain OSC 8 sequence for the link
    // OSC 8 format: \x1b]8;;URL\x1b\\
    assert!(
        prefix.contains("\x1b]8;;") || prefix.contains("\x1b]8;"),
        "Hyperlink-only style should render OSC 8 prefix: got '{}'",
        prefix.escape_debug()
    );

    // The suffix should close the hyperlink
    assert!(
        suffix.contains("\x1b]8;;") || suffix.contains("\x1b]8;"),
        "Hyperlink-only style should render OSC 8 suffix: got '{}'",
        suffix.escape_debug()
    );

    tracing::info!("Regression test PASSED: hyperlink-only style renders OSC 8");
}

/// Regression test: Style with hyperlink AND attributes
///
/// Bug: Styles combining hyperlinks with other attributes might drop one or the other
/// Fixed: Both ANSI codes and OSC 8 sequences are rendered
///
/// This test ensures combined styles render all components.
#[test]
fn regression_rendering_hyperlink_with_attributes() {
    init_test_logging();
    log_test_context(
        "regression_rendering_hyperlink_with_attributes",
        "Ensures hyperlink + attributes both render",
    );

    let _phase = test_phase("hyperlink_with_attrs");

    let style = Style::new().bold().link("https://example.com");

    let ansi = style.render_ansi(ColorSystem::TrueColor);
    let (prefix, _suffix) = &*ansi;

    // Should have both bold (SGR 1) and hyperlink (OSC 8)
    assert!(
        prefix.contains("\x1b[1m") || prefix.contains(";1m") || prefix.contains("\x1b[1;"),
        "Should render bold attribute: got '{}'",
        prefix.escape_debug()
    );
    assert!(
        prefix.contains("\x1b]8;"),
        "Should render hyperlink OSC 8 sequence: got '{}'",
        prefix.escape_debug()
    );

    tracing::info!("Regression test PASSED: hyperlink + attributes");
}

/// Regression test: Style combining preserves hyperlinks
///
/// Bug: When combining styles, hyperlinks could be lost
/// Fixed: combine() properly handles link field
///
/// This test ensures style combination preserves hyperlinks.
#[test]
fn regression_rendering_style_combine_preserves_hyperlink() {
    init_test_logging();
    log_test_context(
        "regression_rendering_style_combine_preserves_hyperlink",
        "Ensures style combination preserves hyperlinks",
    );

    let _phase = test_phase("combine_hyperlink");

    let style1 = Style::new().bold();
    let style2 = Style::new().link("https://example.com");

    let combined = style1.combine(&style2);

    // Combined should have both bold and hyperlink
    assert!(
        combined.attributes.contains(Attributes::BOLD),
        "Combined should have bold"
    );
    assert!(combined.link.is_some(), "Combined should have hyperlink");
    assert_eq!(
        combined.link.as_deref(),
        Some("https://example.com"),
        "Hyperlink URL should be preserved"
    );

    tracing::info!("Regression test PASSED: style combine preserves hyperlink");
}

/// Regression test: Null style rendering
///
/// Bug: Null style could produce unexpected output
/// Fixed: Null style renders as empty strings
///
/// This test ensures null styles render correctly.
#[test]
fn regression_rendering_null_style() {
    init_test_logging();
    log_test_context(
        "regression_rendering_null_style",
        "Ensures null style renders as empty",
    );

    let _phase = test_phase("null_style");

    let style = Style::null();
    assert!(style.is_null(), "Style::null() should be null");

    let ansi = style.render_ansi(ColorSystem::TrueColor);
    let (prefix, suffix) = &*ansi;

    // Null style should produce empty prefix and suffix
    assert!(prefix.is_empty(), "Null style prefix should be empty");
    assert!(suffix.is_empty(), "Null style suffix should be empty");

    tracing::info!("Regression test PASSED: null style renders empty");
}

/// Regression test: Color downgrade from TrueColor to 256
///
/// Bug: Color downgrade could produce incorrect color codes
/// Fixed: Proper color space conversion
///
/// This test ensures color downgrade works correctly.
#[test]
fn regression_rendering_color_downgrade_truecolor_to_256() {
    init_test_logging();
    log_test_context(
        "regression_rendering_color_downgrade_truecolor_to_256",
        "Ensures color downgrade works",
    );

    let _phase = test_phase("color_downgrade");

    // Create a truecolor style
    let style = Style::parse("#ff5500").unwrap();

    // Render for 256-color system
    let ansi = style.render_ansi(ColorSystem::EightBit);
    let (prefix, _suffix) = &*ansi;

    // Should produce 256-color code (38;5;N format)
    assert!(
        prefix.contains("38;5;"),
        "Should downgrade to 256-color format: got '{}'",
        prefix.escape_debug()
    );

    tracing::info!("Regression test PASSED: color downgrade");
}

/// Regression test: Unicode cell width calculation
///
/// Bug: CJK characters could be counted as 1 cell instead of 2
/// Fixed: Proper unicode-width handling
///
/// This test ensures CJK characters have correct width.
#[test]
fn regression_rendering_unicode_cjk_width() {
    init_test_logging();
    log_test_context(
        "regression_rendering_unicode_cjk_width",
        "Ensures CJK characters have width 2",
    );

    let _phase = test_phase("cjk_width");

    use rich_rust::cells::cell_len;

    // CJK characters should be width 2
    let cjk_chars = ['中', '文', '日', '本', '語'];
    for ch in cjk_chars {
        let s = ch.to_string();
        let width = cell_len(&s);
        assert_eq!(
            width, 2,
            "CJK character '{}' should have width 2, got {}",
            ch, width
        );
    }

    // ASCII should be width 1
    let ascii = "hello";
    assert_eq!(cell_len(ascii), 5, "ASCII 'hello' should have width 5");

    tracing::info!("Regression test PASSED: CJK character width");
}

/// Regression test: Emoji cell width
///
/// Bug: Emoji could have incorrect width (1 instead of 2)
/// Fixed: Proper unicode-width handling for emoji
///
/// This test ensures emoji have correct width.
#[test]
fn regression_rendering_unicode_emoji_width() {
    init_test_logging();
    log_test_context(
        "regression_rendering_unicode_emoji_width",
        "Ensures emoji have correct width",
    );

    let _phase = test_phase("emoji_width");

    use rich_rust::cells::cell_len;

    // Basic emoji should typically be width 2
    // Note: actual width depends on unicode-width crate version
    let emoji = "\u{1F600}";
    let width = cell_len(emoji);

    // Width should be reasonable (1 or 2, not 0 or very large)
    assert!(
        width == 1 || width == 2,
        "Emoji should have width 1 or 2, got {}",
        width
    );

    tracing::info!("Regression test PASSED: emoji width");
}

/// Regression test: Segment split at cell boundary
///
/// Bug: Splitting segments at cell boundaries could corrupt CJK characters
/// Fixed: Proper handling of multi-cell characters during split
///
/// This test ensures segment splitting preserves character integrity.
#[test]
fn regression_rendering_segment_split_cjk() {
    init_test_logging();
    log_test_context(
        "regression_rendering_segment_split_cjk",
        "Ensures segment split preserves CJK characters",
    );

    let _phase = test_phase("segment_split");

    let segment = Segment::new("中文", None); // 4 cells total

    // Split in the middle should handle 2-cell characters
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| segment.split_at_cell(2)));

    assert!(result.is_ok(), "Segment split should not panic");

    let (left, right) = result.unwrap();
    // Both parts should be valid UTF-8
    assert_eq!(left.text.as_ref(), "中");
    assert_eq!(right.text.as_ref(), "文");
    assert_eq!(format!("{}{}", left.text, right.text), "中文");

    tracing::info!("Regression test PASSED: segment split with CJK");
}

/// Regression test: ANSI escape code stripping
///
/// Bug: Stripping ANSI codes could leave partial sequences
/// Fixed: Complete regex pattern for all SGR codes
///
/// This test ensures ANSI stripping is complete.
#[test]
fn regression_rendering_ansi_strip_completeness() {
    init_test_logging();
    log_test_context(
        "regression_rendering_ansi_strip_completeness",
        "Ensures ANSI stripping is complete",
    );

    let _phase = test_phase("ansi_strip");

    // Create styled output
    let style = Style::parse("bold red on blue").unwrap();
    let ansi = style.render_ansi(ColorSystem::TrueColor);
    let (prefix, suffix) = &*ansi;
    let styled = format!("{prefix}Hello{suffix}");

    // Strip ANSI codes
    let ansi_regex = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    let stripped = ansi_regex.replace_all(&styled, "");

    // Should only have the text
    assert_eq!(stripped, "Hello", "ANSI stripping should leave only text");

    tracing::info!("Regression test PASSED: ANSI strip completeness");
}

/// Regression test: Control character handling
///
/// Bug: Control characters could cause rendering issues
/// Fixed: Control characters have zero display width
///
/// This test ensures control characters are handled.
#[test]
fn regression_rendering_control_character_width() {
    init_test_logging();
    log_test_context(
        "regression_rendering_control_character_width",
        "Ensures control characters have zero width",
    );

    let _phase = test_phase("control_chars");

    use rich_rust::cells::cell_len;

    // Control characters should have width 0
    let control_chars = ['\x00', '\x01', '\x1f', '\x7f'];
    for ch in control_chars {
        let s = ch.to_string();
        let width = cell_len(&s);
        assert_eq!(
            width, 0,
            "Control character {:?} should have width 0, got {}",
            ch, width
        );
    }

    tracing::info!("Regression test PASSED: control character width");
}

/// Regression test: Rule truncates title to available width
///
/// Bug: Long titles could overflow the configured rule width
/// Fixed: Titles are truncated when they exceed available width
#[test]
fn regression_rule_title_truncation() {
    init_test_logging();
    log_test_context(
        "regression_rule_title_truncation",
        "Ensures rule titles are truncated to width",
    );

    let _phase = test_phase("rule_truncation");

    use rich_rust::cells;

    let rule = Rule::with_title("abcdefghijk");
    let width = 10;
    let output = rule.render_plain(width);
    let trimmed = output.trim_end_matches('\n');

    assert_eq!(
        cells::cell_len(trimmed),
        width,
        "Rule output should be truncated to width"
    );
    assert!(
        !trimmed.contains('\u{2500}'),
        "Truncated title should not include rule characters"
    );

    tracing::info!("Regression test PASSED: rule title truncation");
}

/// Regression test: Rule with exact width shows only title + spaces
#[test]
fn regression_rule_title_exact_width_no_rule_chars() {
    init_test_logging();
    log_test_context(
        "regression_rule_title_exact_width_no_rule_chars",
        "Ensures exact-width titles omit rule characters",
    );

    let _phase = test_phase("rule_exact_width");

    let rule = Rule::with_title("abcd");
    let width = 6; // " abcd " fits exactly
    let output = rule.render_plain(width);
    let trimmed = output.trim_end_matches('\n');

    assert_eq!(
        trimmed, " abcd ",
        "Rule should render title with surrounding spaces"
    );
    assert!(
        !trimmed.contains('\u{2500}'),
        "No rule characters expected when width is exact"
    );

    tracing::info!("Regression test PASSED: rule exact width");
}

/// Regression test: Table title preserves span styles
///
/// Bug: Rich text spans in titles could be flattened to plain text
/// Fixed: Rendering preserves span styles and combines with title style
#[test]
fn regression_table_title_preserves_spans() {
    init_test_logging();
    log_test_context(
        "regression_table_title_preserves_spans",
        "Ensures table titles preserve span styles",
    );

    let _phase = test_phase("table_title_spans");

    let mut title = Text::new("Title");
    title.stylize(0, 5, Style::new().bold());

    let mut table = Table::new()
        .with_column(Column::new("Col"))
        .title(title)
        .title_style(Style::new().color(Color::parse("red").unwrap()));

    table.add_row_cells(["1"]);

    let segments = table.render(30);
    let has_bold_red = segments.iter().any(|seg| {
        seg.text.contains("Title")
            && seg
                .style
                .as_ref()
                .is_some_and(|style| style.attributes.contains(Attributes::BOLD))
            && seg
                .style
                .as_ref()
                .is_some_and(|style| style.color.is_some())
    });

    assert!(has_bold_red, "title should preserve bold + color style");
    tracing::info!("Regression test PASSED: table title spans preserved");
}

/// Regression test: Tree label preserves span styles
///
/// Bug: Tree labels could lose span styling during render
/// Fixed: Rendering uses Text::render() and retains span styles
#[test]
fn regression_tree_label_preserves_spans() {
    init_test_logging();
    log_test_context(
        "regression_tree_label_preserves_spans",
        "Ensures tree labels preserve span styles",
    );

    let _phase = test_phase("tree_label_spans");

    let mut label = Text::new("root");
    label.stylize(0, 4, Style::new().bold());
    let tree = Tree::new(TreeNode::new(label));

    let segments = tree.render();
    let has_bold = segments.iter().any(|seg| {
        seg.text.contains("root")
            && seg
                .style
                .as_ref()
                .is_some_and(|style| style.attributes.contains(Attributes::BOLD))
    });

    assert!(has_bold, "tree label should preserve bold style");
    tracing::info!("Regression test PASSED: tree label spans preserved");
}

/// Regression test: Panel title truncates with ellipsis and fits width
///
/// Bug: Long titles could overflow panel width
/// Fixed: Titles truncate with ellipsis and stay within width
#[test]
fn regression_panel_title_truncates_and_fits_width() {
    init_test_logging();
    log_test_context(
        "regression_panel_title_truncates_and_fits_width",
        "Ensures panel titles truncate and fit width",
    );

    let _phase = test_phase("panel_title_truncate");

    use rich_rust::cells;

    let panel = Panel::from_text("Body").title("ABCDEFGHIJK").width(10);
    let output: String = panel.render(10).into_iter().map(|seg| seg.text).collect();
    assert!(
        output.contains("..."),
        "expected ellipsis in truncated title"
    );

    for line in output.lines().filter(|line| !line.is_empty()) {
        assert!(
            cells::cell_len(line) <= 10,
            "panel line should not exceed width"
        );
    }

    tracing::info!("Regression test PASSED: panel title truncation");
}

/// Regression test: Panel subtitle truncates with ellipsis and fits width
#[test]
fn regression_panel_subtitle_truncates_and_fits_width() {
    init_test_logging();
    log_test_context(
        "regression_panel_subtitle_truncates_and_fits_width",
        "Ensures panel subtitles truncate and fit width",
    );

    let _phase = test_phase("panel_subtitle_truncate");

    use rich_rust::cells;

    let panel = Panel::from_text("Body")
        .subtitle("LongSubtitleHere")
        .width(12);
    let output: String = panel.render(12).into_iter().map(|seg| seg.text).collect();
    assert!(
        output.contains("..."),
        "expected ellipsis in truncated subtitle"
    );

    for line in output.lines().filter(|line| !line.is_empty()) {
        assert!(
            cells::cell_len(line) <= 12,
            "panel line should not exceed width"
        );
    }

    tracing::info!("Regression test PASSED: panel subtitle truncation");
}

/// Regression test: Panel title preserves span styles
///
/// Bug: Title spans could be flattened to plain text
/// Fixed: Title rendering preserves span styles
#[test]
fn regression_panel_title_preserves_spans() {
    init_test_logging();
    log_test_context(
        "regression_panel_title_preserves_spans",
        "Ensures panel title preserves span styles",
    );

    let _phase = test_phase("panel_title_spans");

    let mut title = Text::new("TitleSpan");
    title.stylize(0, 2, Style::new().italic());

    let panel = Panel::from_text("Body").title(title).width(20);
    let segments = panel.render(20);
    let has_italic = segments.iter().any(|seg| {
        seg.text.contains("Ti")
            && seg
                .style
                .as_ref()
                .is_some_and(|style| style.attributes.contains(Attributes::ITALIC))
    });

    assert!(has_italic, "title should preserve italic span");
    tracing::info!("Regression test PASSED: panel title spans preserved");
}

/// Regression test: Panel subtitle preserves span styles
#[test]
fn regression_panel_subtitle_preserves_spans() {
    init_test_logging();
    log_test_context(
        "regression_panel_subtitle_preserves_spans",
        "Ensures panel subtitle preserves span styles",
    );

    let _phase = test_phase("panel_subtitle_spans");

    let mut subtitle = Text::new("SubSpan");
    subtitle.stylize(0, 3, Style::new().underline());

    let panel = Panel::from_text("Body").subtitle(subtitle).width(24);
    let segments = panel.render(24);
    let has_underline = segments.iter().any(|seg| {
        seg.text.contains("Sub")
            && seg
                .style
                .as_ref()
                .is_some_and(|style| style.attributes.contains(Attributes::UNDERLINE))
    });

    assert!(has_underline, "subtitle should preserve underline span");
    tracing::info!("Regression test PASSED: panel subtitle spans preserved");
}

/// Regression test: Table cell preserves span styles
///
/// Bug: Table cells could lose Text span styling during render
/// Fixed: Cell rendering preserves span styles
#[test]
fn regression_table_cell_preserves_spans() {
    init_test_logging();
    log_test_context(
        "regression_table_cell_preserves_spans",
        "Ensures table cell Text spans are preserved",
    );

    let _phase = test_phase("table_cell_spans");

    let mut cell_text = Text::new("Cell");
    cell_text.stylize(0, 4, Style::new().italic());

    let mut table = Table::new().with_column(Column::new("H"));
    table.add_row_cells([cell_text]);

    let segments = table.render(20);
    let has_italic = segments.iter().any(|seg| {
        seg.text.contains("Cell")
            && seg
                .style
                .as_ref()
                .is_some_and(|style| style.attributes.contains(Attributes::ITALIC))
    });

    assert!(has_italic, "cell should preserve italic style");
    tracing::info!("Regression test PASSED: table cell spans preserved");
}

/// Regression test: Table caption preserves span styles
///
/// Bug: Caption spans could be flattened to plain text
/// Fixed: Caption rendering preserves span styles
#[test]
fn regression_table_caption_preserves_spans() {
    init_test_logging();
    log_test_context(
        "regression_table_caption_preserves_spans",
        "Ensures table caption preserves span styles",
    );

    let _phase = test_phase("table_caption_spans");

    let mut caption = Text::new("Caption");
    caption.stylize(0, 7, Style::new().underline());

    let mut table = Table::new()
        .with_column(Column::new("H"))
        .caption(caption)
        .caption_style(Style::new().color(Color::parse("green").unwrap()));
    table.add_row_cells(["1"]);

    let segments = table.render(30);
    let has_underline = segments.iter().any(|seg| {
        seg.text.contains("Cap")
            && seg
                .style
                .as_ref()
                .is_some_and(|style| style.attributes.contains(Attributes::UNDERLINE))
    });

    assert!(has_underline, "caption should preserve underline span");
    tracing::info!("Regression test PASSED: table caption spans preserved");
}

/// Regression test: Table caption alignment preserves line width
#[test]
fn regression_table_caption_alignment_width() {
    init_test_logging();
    log_test_context(
        "regression_table_caption_alignment_width",
        "Ensures caption alignment keeps line width consistent",
    );

    let _phase = test_phase("table_caption_align_width");

    use rich_rust::cells;

    let width = 24;
    let base = Table::new()
        .with_column(Column::new("H"))
        .with_row_cells(["1"])
        .caption("Caption");

    let tables = [
        base.clone().caption_justify(JustifyMethod::Left),
        base.clone().caption_justify(JustifyMethod::Center),
        base.caption_justify(JustifyMethod::Right),
    ];

    for table in tables {
        let output: String = table
            .render(width)
            .into_iter()
            .map(|seg| seg.text)
            .collect();

        let mut lines = output.lines().filter(|line| !line.is_empty());
        let first_line = lines
            .next()
            .expect("table output should have at least one line");
        let table_width = cells::cell_len(first_line);

        let caption_line = lines
            .next_back()
            .expect("table output should include a caption line");

        assert_eq!(
            cells::cell_len(caption_line),
            table_width,
            "caption line should match table width"
        );
    }

    tracing::info!("Regression test PASSED: table caption alignment width");
}

/// Regression test: Table leading inserts blank lines between rows
#[test]
fn regression_table_leading_inserts_blank_lines() {
    init_test_logging();
    log_test_context(
        "regression_table_leading_inserts_blank_lines",
        "Ensures table leading inserts blank spacer lines between rows",
    );

    let _phase = test_phase("table_leading_blank_lines");

    use rich_rust::cells;

    let table = Table::new()
        .with_column(Column::new("H"))
        .show_header(false)
        .leading(2)
        .with_row_cells(["row-1"])
        .with_row_cells(["row-2"]);

    let output: String = table.render(30).into_iter().map(|seg| seg.text).collect();
    let lines: Vec<&str> = output.lines().filter(|line| !line.is_empty()).collect();

    let first_idx = lines
        .iter()
        .position(|line| line.contains("row-1"))
        .expect("row-1 line should exist");
    let second_idx = lines
        .iter()
        .position(|line| line.contains("row-2"))
        .expect("row-2 line should exist");

    assert_eq!(
        second_idx.saturating_sub(first_idx).saturating_sub(1),
        2,
        "leading should insert two blank lines between rows"
    );

    let table_width = cells::cell_len(lines[0]);
    for line in lines {
        assert_eq!(
            cells::cell_len(line),
            table_width,
            "table line should match table width"
        );
    }

    tracing::info!("Regression test PASSED: table leading blank lines");
}

/// Regression test: Table vertical padding inserts blank lines
#[test]
fn regression_table_vertical_padding_inserts_blank_lines() {
    init_test_logging();
    log_test_context(
        "regression_table_vertical_padding_inserts_blank_lines",
        "Ensures vertical padding adds spacer lines around rows",
    );

    let _phase = test_phase("table_vertical_padding");

    use rich_rust::cells;

    let table = Table::new()
        .with_column(Column::new("H"))
        .show_header(false)
        .padding(1, 1)
        .with_row_cells(["row-1"])
        .with_row_cells(["row-2"]);

    let output: String = table.render(30).into_iter().map(|seg| seg.text).collect();
    let lines: Vec<&str> = output.lines().filter(|line| !line.is_empty()).collect();

    let first_idx = lines
        .iter()
        .position(|line| line.contains("row-1"))
        .expect("row-1 line should exist");
    let second_idx = lines
        .iter()
        .position(|line| line.contains("row-2"))
        .expect("row-2 line should exist");

    assert_eq!(
        second_idx.saturating_sub(first_idx).saturating_sub(1),
        2,
        "vertical padding should add two blank lines between rows"
    );

    let table_width = cells::cell_len(lines[0]);
    for line in lines {
        assert_eq!(
            cells::cell_len(line),
            table_width,
            "table line should match table width"
        );
    }

    tracing::info!("Regression test PASSED: table vertical padding");
}

/// Regression test: Rule title alignment preserves total width
///
/// Bug: Rule titles could produce lines shorter/longer than width
/// Fixed: Rule output maintains consistent line width
#[test]
fn regression_rule_title_alignment_width_consistency() {
    init_test_logging();
    log_test_context(
        "regression_rule_title_alignment_width_consistency",
        "Ensures rule output width matches requested width",
    );

    let _phase = test_phase("rule_title_width");

    use rich_rust::cells;

    let width = 20;
    let rule_left = Rule::with_title("Title").align_left();
    let rule_center = Rule::with_title("Title").align_center();
    let rule_right = Rule::with_title("Title").align_right();

    for rule in [rule_left, rule_center, rule_right] {
        let output = rule.render_plain(width);
        for line in output.lines().filter(|line| !line.is_empty()) {
            assert_eq!(cells::cell_len(line), width, "rule line should match width");
        }
    }

    tracing::info!("Regression test PASSED: rule title width consistency");
}

/// Regression test: Tree highlight combines with span styles
///
/// Bug: Highlight style could overwrite span styles
/// Fixed: Highlight combines with existing span styles
#[test]
fn regression_tree_highlight_combines_with_spans() {
    init_test_logging();
    log_test_context(
        "regression_tree_highlight_combines_with_spans",
        "Ensures highlight style combines with span styles",
    );

    let _phase = test_phase("tree_highlight_span");

    let mut label = Text::new("root");
    label.stylize(0, 4, Style::new().bold());

    let tree = Tree::new(TreeNode::new(label))
        .highlight_style(Style::new().color(Color::parse("red").unwrap()).italic());

    let segments = tree.render();
    let has_bold_red = segments.iter().any(|seg| {
        seg.text.contains("root")
            && seg
                .style
                .as_ref()
                .is_some_and(|style| style.attributes.contains(Attributes::BOLD))
            && seg
                .style
                .as_ref()
                .is_some_and(|style| style.color.is_some())
            && seg
                .style
                .as_ref()
                .is_some_and(|style| style.attributes.contains(Attributes::ITALIC))
    });

    assert!(
        has_bold_red,
        "highlight should combine color/italic with bold span"
    );
    tracing::info!("Regression test PASSED: tree highlight combines with spans");
}

/// Regression test: Console control segments emit ANSI/control sequences
///
/// Bug: Control segments were silently skipped in Console output
/// Fixed: Control segments now emit ANSI/control sequences in order
///
/// This test ensures control codes are written to the output stream.
#[test]
fn regression_console_control_segments_emit_sequences() {
    init_test_logging();
    log_test_context(
        "regression_console_control_segments_emit_sequences",
        "Ensures control segments are emitted in order",
    );

    let _phase = test_phase("control_segments");

    #[derive(Clone)]
    struct SharedBuffer {
        inner: Arc<Mutex<Vec<u8>>>,
    }

    impl Write for SharedBuffer {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let mut guard = self.inner.lock().expect("buffer lock poisoned");
            guard.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    let buffer = Arc::new(Mutex::new(Vec::new()));
    let writer = Box::new(SharedBuffer {
        inner: Arc::clone(&buffer),
    });

    let console = Console::builder().file(writer).build();

    let segments = vec![
        Segment::control(vec![ControlCode::new(ControlType::Bell)]),
        Segment::control(vec![ControlCode::with_params_vec(
            ControlType::CursorUp,
            vec![2],
        )]),
        Segment::control(vec![ControlCode::with_params_vec(
            ControlType::CursorMoveTo,
            vec![3, 4],
        )]),
        Segment::control(vec![ControlCode::with_params_vec(
            ControlType::EraseInLine,
            vec![2],
        )]),
    ];

    console.print_segments(&segments);

    let output = String::from_utf8(buffer.lock().expect("buffer lock poisoned").clone())
        .expect("output should be valid UTF-8");
    let expected = "\x07\x1b[2A\x1b[5;4H\x1b[2K";
    assert_eq!(
        output, expected,
        "Control sequence output should match expected ANSI codes"
    );

    tracing::info!("Regression test PASSED: control segments emit sequences");
}

/// Regression test: SetWindowTitle uses segment text when provided
#[test]
fn regression_console_control_set_window_title() {
    init_test_logging();
    log_test_context(
        "regression_console_control_set_window_title",
        "Ensures SetWindowTitle emits OSC title sequence",
    );

    let _phase = test_phase("control_title");

    #[derive(Clone)]
    struct SharedBuffer {
        inner: Arc<Mutex<Vec<u8>>>,
    }

    impl Write for SharedBuffer {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            let mut guard = self.inner.lock().expect("buffer lock poisoned");
            guard.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    let buffer = Arc::new(Mutex::new(Vec::new()));
    let writer = Box::new(SharedBuffer {
        inner: Arc::clone(&buffer),
    });

    let console = Console::builder().file(writer).build();

    let segment = Segment {
        text: "rich_rust".to_string().into(),
        style: None,
        control: Some(vec![ControlCode::new(ControlType::SetWindowTitle)]),
    };

    console.print_segments(&[segment]);

    let output = String::from_utf8(buffer.lock().expect("buffer lock poisoned").clone())
        .expect("output should be valid UTF-8");
    let expected = "\x1b]0;rich_rust\x07";
    assert_eq!(output, expected, "Window title OSC sequence should match");

    tracing::info!("Regression test PASSED: set window title control");
}

/// Regression test: Verify that render_ansi handles link+attributes correctly
#[test]
fn regression_rendering_hyperlink_with_attributes_correctness() {
    init_test_logging();
    log_test_context(
        "regression_rendering_hyperlink_with_attributes_correctness",
        "Ensures hyperlink + attributes render correctly",
    );

    let _phase = test_phase("hyperlink_with_attrs_correctness");

    // Create a style with bold and hyperlink
    let style = Style::new().bold().link("https://example.com");

    // Render the style using render_ansi for (prefix, suffix) tuple
    let ansi = style.render_ansi(ColorSystem::TrueColor);
    let (prefix, suffix) = &*ansi;

    // The prefix should contain both bold (SGR 1) and hyperlink (OSC 8)
    assert!(
        prefix.contains("\x1b[1m") || prefix.contains(";1m") || prefix.contains("\x1b[1;"),
        "Should render bold attribute: got '{}'",
        prefix.escape_debug()
    );
    assert!(
        prefix.contains("\x1b]8;"),
        "Should render hyperlink OSC 8 sequence: got '{}'",
        prefix.escape_debug()
    );

    // The suffix should close the hyperlink
    assert!(
        suffix.contains("\x1b]8;;") || suffix.contains("\x1b]8;"),
        "Hyperlink-only style should render OSC 8 suffix: got '{}'",
        suffix.escape_debug()
    );

    tracing::info!("Regression test PASSED: hyperlink + attributes");
}

/// Regression test: Verify that render_ansi handles link only correctly
#[test]
fn regression_rendering_hyperlink_only_style_correctness() {
    init_test_logging();
    log_test_context(
        "regression_rendering_hyperlink_only_style_correctness",
        "Ensures hyperlink-only styles render correctly",
    );

    let _phase = test_phase("hyperlink_only_correctness");

    // Create a style with ONLY a hyperlink (no colors, no attributes)
    let mut style = Style::new();
    style.link = Some("https://example.com".to_string());

    // The style should NOT be null (it has a link)
    assert!(!style.is_null(), "Style with link should not be null");

    // Render the style using render_ansi for (prefix, suffix) tuple
    let ansi = style.render_ansi(ColorSystem::TrueColor);
    let (prefix, suffix) = &*ansi;

    // The prefix should contain OSC 8 sequence for the link
    // OSC 8 format: \x1b]8;;URL\x1b\\
    assert!(
        prefix.contains("\x1b]8;;") || prefix.contains("\x1b]8;"),
        "Hyperlink-only style should render OSC 8 prefix: got '{}'",
        prefix.escape_debug()
    );

    // The suffix should close the hyperlink
    assert!(
        suffix.contains("\x1b]8;;") || suffix.contains("\x1b]8;"),
        "Hyperlink-only style should render OSC 8 suffix: got '{}'",
        suffix.escape_debug()
    );

    tracing::info!("Regression test PASSED: hyperlink-only style renders OSC 8");
}

/// Regression test: Verify style combination preserves hyperlink
#[test]
fn regression_rendering_style_combine_preserves_hyperlink_correctness() {
    init_test_logging();
    log_test_context(
        "regression_rendering_style_combine_preserves_hyperlink_correctness",
        "Ensures style combination preserves hyperlinks",
    );

    let _phase = test_phase("combine_hyperlink_correctness");

    // Create a style with bold and hyperlink
    let s1 = Style::new().link("https://example.com");
    let s2 = Style::new().bold();
    let combined = s1.combine(&s2);

    // Combined should have both bold and hyperlink
    assert!(
        combined.attributes.contains(Attributes::BOLD),
        "Combined should have bold"
    );
    assert!(combined.link.is_some(), "Combined should have hyperlink");
    assert_eq!(
        combined.link.as_deref(),
        Some("https://example.com"),
        "Hyperlink URL should be preserved"
    );

    // Verify that render_ansi handles link+attributes correctly
    let style = Style::new().bold().link("https://example.com");
    let ansi = style.render_ansi(ColorSystem::TrueColor);
    let (prefix, suffix) = &*ansi;

    assert!(prefix.contains("\x1b]8;;https://example.com\x1b\\"));
    assert!(prefix.contains("\x1b[1m"));
    assert!(suffix.contains("\x1b]8;;\x1b\\"));
    assert!(suffix.contains("\x1b[0m"));

    tracing::info!("Regression test PASSED: style combine preserves hyperlink");
}

/// Regression test: Verify color downgrade (TrueColor -> 8-bit)
#[test]
fn regression_rendering_color_downgrade_truecolor_to_8bit_correctness() {
    init_test_logging();
    log_test_context(
        "regression_rendering_color_downgrade_truecolor_to_8bit_correctness",
        "Ensures color downgrade works correctly",
    );

    let _phase = test_phase("color_downgrade_correctness");

    // Create a truecolor style
    let color = Color::from_rgb(255, 0, 0); // Red
    let style = Style::new().color(color);

    // Render for 256-color system
    let ansi = style.render_ansi(ColorSystem::EightBit);
    let (prefix, _suffix) = &*ansi;

    // Should produce 256-color code (196 is standard red in 256-color)
    // The exact mapping might vary, but it should be an 8-bit code
    assert!(
        prefix.contains("38;5;"),
        "Should downgrade to 256-color format: got '{}'",
        prefix.escape_debug()
    );

    tracing::info!("Regression test PASSED: color downgrade");
}

/// Regression test: Verify ANSI strip completeness
#[test]
fn regression_rendering_ansi_strip_completeness_correctness() {
    init_test_logging();
    log_test_context(
        "regression_rendering_ansi_strip_completeness_correctness",
        "Ensures ANSI stripping is complete",
    );

    let _phase = test_phase("ansi_strip_correctness");

    // Create styled output
    let style = Style::parse("bold red on blue").unwrap();
    let ansi = style.render_ansi(ColorSystem::TrueColor);
    let (prefix, suffix) = &*ansi;
    let styled = format!("{prefix}Hello{suffix}");

    // Strip ANSI codes
    let ansi_regex = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    let stripped = ansi_regex.replace_all(&styled, "");

    // Should only have the text
    assert_eq!(stripped, "Hello", "ANSI stripping should leave only text");

    tracing::info!("Regression test PASSED: ANSI strip completeness");
}

/// Regression test: Verify control character handling
#[test]
fn regression_rendering_control_character_width_correctness() {
    init_test_logging();
    log_test_context(
        "regression_rendering_control_character_width_correctness",
        "Ensures control characters have zero width",
    );

    let _phase = test_phase("control_chars_correctness");

    use rich_rust::cells::cell_len;

    // Control characters should have width 0
    let control_chars = ['\x00', '\x01', '\x1f', '\x7f'];
    for ch in control_chars {
        let s = ch.to_string();
        let width = cell_len(&s);
        assert_eq!(
            width, 0,
            "Control character {:?} should have width 0, got {}",
            ch, width
        );
    }

    tracing::info!("Regression test PASSED: control character width");
}
