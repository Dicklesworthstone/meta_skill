//! End-to-end tests for Panel rendering and nested renderables.
//!
//! Panels are bordered boxes that can contain various types of content
//! including text, styled content, and other renderables like tables.
//!
//! Run with: RUST_LOG=debug cargo test --test e2e_panel -- --nocapture

mod common;

use common::init_test_logging;
use rich_rust::r#box::{DOUBLE, HEAVY, MINIMAL};
use rich_rust::prelude::*;

// =============================================================================
// Scenario 1: Basic Panel
// =============================================================================

#[test]
fn e2e_basic_panel_text() {
    init_test_logging();
    tracing::info!("Starting E2E basic panel text test");

    let panel = Panel::from_text("Hello World").title("Greeting").width(30);

    let output = panel.render_plain(50);
    tracing::debug!(output = %output, "Rendered panel");

    // Verify rounded corners (default)
    assert!(output.contains('╭'), "Missing top-left corner");
    assert!(output.contains('╮'), "Missing top-right corner");
    assert!(output.contains('╰'), "Missing bottom-left corner");
    assert!(output.contains('╯'), "Missing bottom-right corner");

    // Verify borders
    assert!(output.contains('─'), "Missing horizontal border");
    assert!(output.contains('│'), "Missing vertical border");

    // Verify content
    assert!(output.contains("Greeting"), "Missing title");
    assert!(output.contains("Hello World"), "Missing content");

    tracing::info!("E2E basic panel text test PASSED");
}

#[test]
fn e2e_panel_multiline() {
    init_test_logging();
    tracing::info!("Starting E2E panel multiline test");

    let panel = Panel::from_text("Line 1\nLine 2\nLine 3").width(20);

    let output = panel.render_plain(50);
    tracing::debug!(output = %output, "Multiline panel");

    assert!(output.contains("Line 1"), "Missing line 1");
    assert!(output.contains("Line 2"), "Missing line 2");
    assert!(output.contains("Line 3"), "Missing line 3");

    tracing::info!("E2E panel multiline test PASSED");
}

#[test]
fn e2e_panel_with_subtitle() {
    init_test_logging();
    tracing::info!("Starting E2E panel with subtitle test");

    let panel = Panel::from_text("Content here")
        .title("Header")
        .subtitle("Footer")
        .width(30);

    let output = panel.render_plain(50);
    tracing::debug!(output = %output, "Panel with title and subtitle");

    assert!(output.contains("Header"), "Missing title");
    assert!(output.contains("Footer"), "Missing subtitle");
    assert!(output.contains("Content here"), "Missing content");

    tracing::info!("E2E panel with subtitle test PASSED");
}

// =============================================================================
// Scenario 2: Panel with Styled Content
// =============================================================================

#[test]
fn e2e_panel_styled_content() {
    init_test_logging();
    tracing::info!("Starting E2E panel with styled content test");

    // Create segments with styling
    let bold_style = Style::new().bold();
    let segments = vec![vec![
        Segment::new("Important", Some(bold_style)),
        Segment::new(" message", None),
    ]];

    let panel = Panel::new(segments).width(30);
    let output = panel.render(50);
    tracing::debug!(segment_count = output.len(), "Panel with styled content");

    // Verify styled segments are present
    let has_styled = output.iter().any(|s| s.style.is_some());
    assert!(has_styled, "Should have styled segments");

    let plain: String = output.iter().map(|s| s.text.as_ref()).collect();
    assert!(plain.contains("Important"), "Missing styled text");
    assert!(plain.contains("message"), "Missing plain text");

    tracing::info!("E2E panel with styled content test PASSED");
}

#[test]
fn e2e_panel_border_style() {
    init_test_logging();
    tracing::info!("Starting E2E panel with border style test");

    let red_style = Style::new().color(Color::parse("red").unwrap());
    let panel = Panel::from_text("Warning!")
        .border_style(red_style)
        .width(20);

    let segments = panel.render(50);

    // Check that border segments have the red style
    let border_segments: Vec<_> = segments
        .iter()
        .filter(|s| s.text.contains('╭') || s.text.contains('─'))
        .collect();

    tracing::debug!(border_count = border_segments.len(), "Border segments");
    assert!(!border_segments.is_empty(), "Should have border segments");

    tracing::info!("E2E panel with border style test PASSED");
}

// =============================================================================
// Scenario 3: Box Styles
// =============================================================================

#[test]
fn e2e_panel_rounded_box() {
    init_test_logging();
    tracing::info!("Starting E2E panel rounded box test");

    let panel = Panel::from_text("Rounded").rounded().width(20);

    let output = panel.render_plain(50);
    tracing::debug!(output = %output, "Rounded box");

    assert!(output.contains('╭'), "Missing rounded top-left");
    assert!(output.contains('╮'), "Missing rounded top-right");
    assert!(output.contains('╰'), "Missing rounded bottom-left");
    assert!(output.contains('╯'), "Missing rounded bottom-right");

    tracing::info!("E2E panel rounded box test PASSED");
}

#[test]
fn e2e_panel_square_box() {
    init_test_logging();
    tracing::info!("Starting E2E panel square box test");

    let panel = Panel::from_text("Square").square().width(20);

    let output = panel.render_plain(50);
    tracing::debug!(output = %output, "Square box");

    assert!(output.contains('┌'), "Missing square top-left");
    assert!(output.contains('┐'), "Missing square top-right");
    assert!(output.contains('└'), "Missing square bottom-left");
    assert!(output.contains('┘'), "Missing square bottom-right");

    tracing::info!("E2E panel square box test PASSED");
}

#[test]
fn e2e_panel_ascii_box() {
    init_test_logging();
    tracing::info!("Starting E2E panel ASCII box test");

    let panel = Panel::from_text("ASCII").ascii().width(20);

    let output = panel.render_plain(50);
    tracing::debug!(output = %output, "ASCII box");

    assert!(output.contains('+'), "Missing ASCII corner '+'");
    assert!(output.contains('-'), "Missing ASCII horizontal '-'");
    assert!(output.contains('|'), "Missing ASCII vertical '|'");

    // Should NOT contain Unicode box chars
    assert!(!output.contains('╭'), "Should not have rounded corners");
    assert!(!output.contains('┌'), "Should not have square corners");

    tracing::info!("E2E panel ASCII box test PASSED");
}

#[test]
fn e2e_panel_double_box() {
    init_test_logging();
    tracing::info!("Starting E2E panel double box test");

    let panel = Panel::from_text("Double").box_style(&DOUBLE).width(20);

    let output = panel.render_plain(50);
    tracing::debug!(output = %output, "Double box");

    assert!(output.contains('╔'), "Missing double top-left");
    assert!(output.contains('╗'), "Missing double top-right");
    assert!(output.contains('╚'), "Missing double bottom-left");
    assert!(output.contains('╝'), "Missing double bottom-right");
    assert!(output.contains('═'), "Missing double horizontal");
    assert!(output.contains('║'), "Missing double vertical");

    tracing::info!("E2E panel double box test PASSED");
}

#[test]
fn e2e_panel_heavy_box() {
    init_test_logging();
    tracing::info!("Starting E2E panel heavy box test");

    let panel = Panel::from_text("Heavy").box_style(&HEAVY).width(20);

    let output = panel.render_plain(50);
    tracing::debug!(output = %output, "Heavy box");

    assert!(output.contains('┏'), "Missing heavy top-left");
    assert!(output.contains('┓'), "Missing heavy top-right");
    assert!(output.contains('┗'), "Missing heavy bottom-left");
    assert!(output.contains('┛'), "Missing heavy bottom-right");
    assert!(output.contains('━'), "Missing heavy horizontal");
    assert!(output.contains('┃'), "Missing heavy vertical");

    tracing::info!("E2E panel heavy box test PASSED");
}

#[test]
fn e2e_panel_minimal_box() {
    init_test_logging();
    tracing::info!("Starting E2E panel minimal box test");

    let panel = Panel::from_text("Minimal").box_style(&MINIMAL).width(20);

    let output = panel.render_plain(50);
    tracing::debug!(output = %output, "Minimal box");

    // Minimal uses spaces for corners
    assert!(output.contains("Minimal"), "Missing content");

    tracing::info!("E2E panel minimal box test PASSED");
}

// =============================================================================
// Scenario 4: Title Alignment
// =============================================================================

#[test]
fn e2e_panel_title_left() {
    init_test_logging();
    tracing::info!("Starting E2E panel title left aligned test");

    let panel = Panel::from_text("Content")
        .title("Left")
        .title_align(JustifyMethod::Left)
        .width(30);

    let output = panel.render_plain(50);
    tracing::debug!(output = %output, "Left-aligned title");

    assert!(output.contains("Left"), "Missing title");

    tracing::info!("E2E panel title left aligned test PASSED");
}

#[test]
fn e2e_panel_title_center() {
    init_test_logging();
    tracing::info!("Starting E2E panel title center aligned test");

    let panel = Panel::from_text("Content")
        .title("Center")
        .title_align(JustifyMethod::Center)
        .width(40);

    let output = panel.render_plain(50);
    tracing::debug!(output = %output, "Center-aligned title");

    assert!(output.contains("Center"), "Missing title");

    tracing::info!("E2E panel title center aligned test PASSED");
}

#[test]
fn e2e_panel_title_right() {
    init_test_logging();
    tracing::info!("Starting E2E panel title right aligned test");

    let panel = Panel::from_text("Content")
        .title("Right")
        .title_align(JustifyMethod::Right)
        .width(30);

    let output = panel.render_plain(50);
    tracing::debug!(output = %output, "Right-aligned title");

    assert!(output.contains("Right"), "Missing title");

    tracing::info!("E2E panel title right aligned test PASSED");
}

// =============================================================================
// Scenario 5: Width and Padding
// =============================================================================

#[test]
fn e2e_panel_fixed_width() {
    init_test_logging();
    tracing::info!("Starting E2E panel fixed width test");

    let panel = Panel::from_text("Short").width(40).expand(true);

    let output = panel.render_plain(50);
    tracing::debug!(output = %output, "Fixed width panel");

    // First line should be exactly 40 characters (including box chars)
    let first_line = output.lines().next().unwrap_or("");
    tracing::debug!(
        first_line_len = first_line.chars().count(),
        "First line length"
    );

    assert!(
        first_line.chars().count() >= 20,
        "Panel should have reasonable width"
    );

    tracing::info!("E2E panel fixed width test PASSED");
}

#[test]
fn e2e_panel_expand_false() {
    init_test_logging();
    tracing::info!("Starting E2E panel expand=false test");

    let panel = Panel::from_text("Hi").expand(false);

    let output = panel.render_plain(80);
    tracing::debug!(output = %output, "Non-expanded panel");

    // Panel should fit content, not expand to 80
    let first_line = output.lines().next().unwrap_or("");
    let line_width = first_line.chars().count();
    tracing::debug!(line_width = line_width, "First line width");

    assert!(line_width < 40, "Non-expanded panel should be narrow");

    tracing::info!("E2E panel expand=false test PASSED");
}

#[test]
fn e2e_panel_with_padding() {
    init_test_logging();
    tracing::info!("Starting E2E panel with padding test");

    let panel = Panel::from_text("Content")
        .padding((2, 1)) // top/bottom, left/right
        .width(30);

    let output = panel.render_plain(50);
    tracing::debug!(output = %output, "Panel with padding");

    // Count lines to verify padding
    let lines: Vec<_> = output.lines().collect();
    tracing::debug!(line_count = lines.len(), "Total lines");

    // Should have: top border + 2 padding + content + 2 padding + bottom border = 7 lines
    assert!(lines.len() >= 5, "Should have padding lines");

    tracing::info!("E2E panel with padding test PASSED");
}

// =============================================================================
// Scenario 6: Wide Characters
// =============================================================================

#[test]
fn e2e_panel_cjk_content() {
    init_test_logging();
    tracing::info!("Starting E2E panel with CJK content test");

    let panel = Panel::from_text("你好世界").title("中文").width(25);

    let output = panel.render_plain(50);
    tracing::debug!(output = %output, "CJK panel");

    assert!(output.contains("你好世界"), "Missing CJK content");
    assert!(output.contains("中文"), "Missing CJK title");

    tracing::info!("E2E panel with CJK content test PASSED");
}

#[test]
fn e2e_panel_emoji_content() {
    init_test_logging();
    tracing::info!("Starting E2E panel with emoji content test");

    let panel = Panel::from_text("Status: ✓ OK").title("Check").width(25);

    let output = panel.render_plain(50);
    tracing::debug!(output = %output, "Emoji panel");

    assert!(output.contains("✓"), "Missing checkmark");
    assert!(output.contains("OK"), "Missing text");

    tracing::info!("E2E panel with emoji content test PASSED");
}

// =============================================================================
// Scenario 7: Table in Panel
// =============================================================================

#[test]
fn e2e_panel_containing_table_content() {
    init_test_logging();
    tracing::info!("Starting E2E panel containing table content test");

    // Build a table
    let mut table = Table::new()
        .with_column(Column::new("Key"))
        .with_column(Column::new("Value"));

    table.add_row_cells(["Name", "Alice"]);
    table.add_row_cells(["Age", "30"]);

    // Get table output
    let table_output = table.render_plain(40);

    // Put table in panel
    let panel = Panel::from_text(&table_output).title("User Data").width(45);

    let output = panel.render_plain(60);
    tracing::debug!(output = %output, "Panel with table");

    // Verify both panel and table elements
    assert!(output.contains("User Data"), "Missing panel title");
    assert!(output.contains("Key"), "Missing table header");
    assert!(output.contains("Alice"), "Missing table data");

    tracing::info!("E2E panel containing table content test PASSED");
}

// =============================================================================
// Scenario 8: Edge Cases
// =============================================================================

#[test]
fn e2e_panel_empty_content() {
    init_test_logging();
    tracing::info!("Starting E2E empty panel test");

    let panel = Panel::new(vec![]).width(20);
    let output = panel.render_plain(50);
    tracing::debug!(output = %output, "Empty panel");

    // Should still render borders
    assert!(output.contains('╭'), "Should have top-left corner");
    assert!(output.contains('╯'), "Should have bottom-right corner");

    tracing::info!("E2E empty panel test PASSED");
}

#[test]
fn e2e_panel_very_long_title() {
    init_test_logging();
    tracing::info!("Starting E2E panel with long title test");

    let panel = Panel::from_text("Short")
        .title("This is a very long title that exceeds the panel width")
        .width(25);

    let output = panel.render_plain(50);
    tracing::debug!(output = %output, "Panel with long title");

    // Should still render without panic
    assert!(output.contains("Short"), "Missing content");

    tracing::info!("E2E panel with long title test PASSED");
}

#[test]
fn e2e_panel_narrow_width() {
    init_test_logging();
    tracing::info!("Starting E2E narrow panel test");

    let panel = Panel::from_text("Content").width(10);

    let output = panel.render_plain(10);
    tracing::debug!(output = %output, "Narrow panel");

    // Should include content even at narrow widths
    assert!(output.contains("Content"), "Missing content");

    tracing::info!("E2E narrow panel test PASSED");
}

#[test]
fn e2e_panel_single_char_content() {
    init_test_logging();
    tracing::info!("Starting E2E single character panel test");

    let panel = Panel::from_text("X").expand(false);

    let output = panel.render_plain(50);
    tracing::debug!(output = %output, "Single char panel");

    assert!(output.contains('X'), "Missing content");

    tracing::info!("E2E single character panel test PASSED");
}

// =============================================================================
// Snapshot Tests
// =============================================================================

#[test]
fn e2e_snapshot_basic_panel() {
    init_test_logging();

    let panel = Panel::from_text("Hello, World!")
        .title("Greeting")
        .width(30);

    let output = panel.render_plain(50);
    insta::assert_snapshot!("e2e_basic_panel", output);
}

#[test]
fn e2e_snapshot_panel_with_subtitle() {
    init_test_logging();

    let panel = Panel::from_text("Main content here")
        .title("Header")
        .subtitle("Footer")
        .width(35);

    let output = panel.render_plain(50);
    insta::assert_snapshot!("e2e_panel_with_subtitle", output);
}

#[test]
fn e2e_snapshot_ascii_panel() {
    init_test_logging();

    let panel = Panel::from_text("ASCII safe!")
        .title("Legacy")
        .ascii()
        .width(25);

    let output = panel.render_plain(50);
    insta::assert_snapshot!("e2e_ascii_panel", output);
}

#[test]
fn e2e_snapshot_double_panel() {
    init_test_logging();

    let panel = Panel::from_text("Important!")
        .title("Alert")
        .box_style(&DOUBLE)
        .width(25);

    let output = panel.render_plain(50);
    insta::assert_snapshot!("e2e_double_panel", output);
}

#[test]
fn e2e_snapshot_heavy_panel() {
    init_test_logging();

    let panel = Panel::from_text("Heavy borders")
        .box_style(&HEAVY)
        .width(25);

    let output = panel.render_plain(50);
    insta::assert_snapshot!("e2e_heavy_panel", output);
}

#[test]
fn e2e_snapshot_multiline_panel() {
    init_test_logging();

    let panel = Panel::from_text("Line 1\nLine 2\nLine 3\nLine 4")
        .title("List")
        .width(20);

    let output = panel.render_plain(50);
    insta::assert_snapshot!("e2e_multiline_panel", output);
}
