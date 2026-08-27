//! End-to-end tests for Table rendering.
//!
//! Tables are the most complex renderable with many interacting features:
//! column sizing, row rendering, borders, alignment, padding, and more.
//!
//! Run with: RUST_LOG=debug cargo test --test e2e_table -- --nocapture

mod common;

use common::init_test_logging;
use rich_rust::cells;
use rich_rust::prelude::*;

// =============================================================================
// Scenario 1: Simple Table
// =============================================================================

#[test]
fn e2e_table_simple_2x2() {
    init_test_logging();
    tracing::info!("Starting E2E simple 2x2 table test");

    let mut table = Table::new()
        .with_column(Column::new("Name"))
        .with_column(Column::new("Value"));

    table.add_row_cells(["Alice", "100"]);
    table.add_row_cells(["Bob", "200"]);

    tracing::debug!("Table has {} columns, {} rows", 2, 2);

    let output = table.render_plain(50);
    tracing::debug!(output = %output, "Rendered table");

    // Verify structure
    assert!(output.contains("Name"), "Missing header 'Name'");
    assert!(output.contains("Value"), "Missing header 'Value'");
    assert!(output.contains("Alice"), "Missing cell 'Alice'");
    assert!(output.contains("Bob"), "Missing cell 'Bob'");
    assert!(output.contains("100"), "Missing cell '100'");
    assert!(output.contains("200"), "Missing cell '200'");

    // Verify box characters (default is HEAVY_HEAD or SQUARE variants)
    // Check for both square (┌├) and heavy (┏┡) box corners
    assert!(
        output.contains('┌')
            || output.contains('├')
            || output.contains('┏')
            || output.contains('┡')
            || output.contains('└'),
        "Missing box corners, output: {output}"
    );
    assert!(output.contains("─"), "Missing horizontal box line");
    assert!(output.contains("│"), "Missing vertical box line");

    tracing::info!("E2E simple 2x2 table test PASSED");
}

#[test]
fn e2e_table_single_column() {
    init_test_logging();
    tracing::info!("Starting E2E single column table test");

    let mut table = Table::new().with_column(Column::new("Item"));

    table.add_row_cells(["First"]);
    table.add_row_cells(["Second"]);
    table.add_row_cells(["Third"]);

    let output = table.render_plain(30);
    tracing::debug!(output = %output, "Single column table");

    assert!(output.contains("Item"), "Missing header");
    assert!(output.contains("First"), "Missing row 1");
    assert!(output.contains("Second"), "Missing row 2");
    assert!(output.contains("Third"), "Missing row 3");

    tracing::info!("E2E single column table test PASSED");
}

// =============================================================================
// Scenario 2: Table with All Features
// =============================================================================

#[test]
fn e2e_table_with_title_and_caption() {
    init_test_logging();
    tracing::info!("Starting E2E table with title and caption test");

    let mut table = Table::new()
        .title("User Statistics")
        .caption("Q4 2024 Report")
        .with_column(Column::new("Metric"))
        .with_column(Column::new("Value"));

    table.add_row_cells(["Active Users", "1,234"]);
    table.add_row_cells(["Sessions", "5,678"]);

    let output = table.render_plain(50);
    tracing::debug!(output = %output, "Table with title/caption");

    assert!(output.contains("User Statistics"), "Missing title");
    assert!(output.contains("Q4 2024 Report"), "Missing caption");
    assert!(output.contains("Active Users"), "Missing data");

    tracing::info!("E2E table with title and caption test PASSED");
}

#[test]
fn e2e_table_title_alignment_width() {
    init_test_logging();
    tracing::info!("Starting E2E table title alignment width test");

    let mut base = Table::new().title("Title").with_column(Column::new("A"));
    base.add_row_cells(["1"]);

    let width = 40;
    let tables = [
        base.clone().title_justify(JustifyMethod::Left),
        base.clone().title_justify(JustifyMethod::Center),
        base.title_justify(JustifyMethod::Right),
    ];

    for table in tables {
        let output = table.render_plain(width);
        let lines: Vec<&str> = output.lines().filter(|line| !line.is_empty()).collect();
        let title_line = lines
            .iter()
            .copied()
            .find(|line| line.contains("Title"))
            .expect("missing title line");
        let table_width_line = lines
            .iter()
            .copied()
            .find(|line| !line.contains("Title"))
            .expect("missing table width line");
        assert_eq!(
            cells::cell_len(title_line),
            cells::cell_len(table_width_line),
            "title line should match table width"
        );
    }

    tracing::info!("E2E table title alignment width test PASSED");
}

#[test]
fn e2e_table_with_footer() {
    init_test_logging();
    tracing::info!("Starting E2E table with footer test");

    let mut table = Table::new()
        .show_footer(true)
        .with_column(Column::new("Product").footer("Total"))
        .with_column(Column::new("Sales").footer("$1,500"));

    table.add_row_cells(["Widget A", "$500"]);
    table.add_row_cells(["Widget B", "$1,000"]);

    let output = table.render_plain(40);
    tracing::debug!(output = %output, "Table with footer");

    assert!(output.contains("Product"), "Missing header");
    assert!(output.contains("Total"), "Missing footer left");
    assert!(output.contains("$1,500"), "Missing footer right");

    tracing::info!("E2E table with footer test PASSED");
}

#[test]
fn e2e_table_ascii_box() {
    init_test_logging();
    tracing::info!("Starting E2E ASCII box table test");

    let mut table = Table::new()
        .ascii()
        .with_column(Column::new("A"))
        .with_column(Column::new("B"));

    table.add_row_cells(["1", "2"]);

    let output = table.render_plain(30);
    tracing::debug!(output = %output, "ASCII box table");

    // ASCII uses +, -, |
    assert!(output.contains("+"), "Missing ASCII corner '+'");
    assert!(output.contains("-"), "Missing ASCII horizontal '-'");
    assert!(output.contains("|"), "Missing ASCII vertical '|'");

    // Should NOT contain Unicode box chars
    assert!(!output.contains("┌"), "Should not have Unicode box chars");
    assert!(!output.contains("─"), "Should not have Unicode horizontal");

    tracing::info!("E2E ASCII box table test PASSED");
}

#[test]
fn e2e_table_no_header() {
    init_test_logging();
    tracing::info!("Starting E2E table without header test");

    let mut table = Table::new()
        .show_header(false)
        .with_column(Column::new("Hidden"))
        .with_column(Column::new("Header"));

    table.add_row_cells(["Data", "Only"]);

    let output = table.render_plain(30);
    tracing::debug!(output = %output, "Table without header");

    assert!(!output.contains("Hidden"), "Header should be hidden");
    assert!(output.contains("Data"), "Data should be visible");

    tracing::info!("E2E table without header test PASSED");
}

#[test]
fn e2e_table_with_row_lines() {
    init_test_logging();
    tracing::info!("Starting E2E table with row separators test");

    let mut table = Table::new()
        .show_lines(true)
        .with_column(Column::new("Item"));

    table.add_row_cells(["One"]);
    table.add_row_cells(["Two"]);
    table.add_row_cells(["Three"]);

    let output = table.render_plain(30);
    tracing::debug!(output = %output, "Table with row lines");

    // Count horizontal separators - should have multiple
    let line_count = output.matches("├").count() + output.matches("─").count();
    tracing::debug!(line_count = line_count, "Separator count");

    assert!(line_count > 3, "Should have multiple row separators");

    tracing::info!("E2E table with row separators test PASSED");
}

// =============================================================================
// Scenario 3: Width Constraints
// =============================================================================

#[test]
fn e2e_table_fixed_column_width() {
    init_test_logging();
    tracing::info!("Starting E2E fixed column width test");

    let mut table = Table::new()
        .with_column(Column::new("Name").width(10))
        .with_column(Column::new("Description").width(20));

    table.add_row_cells(["A", "Short"]);
    table.add_row_cells(["B", "A longer description here"]);

    let output = table.render_plain(50);
    tracing::debug!(output = %output, "Fixed width table");

    // Content should be present (possibly truncated)
    assert!(output.contains("Name"), "Missing header");
    assert!(output.contains("Description"), "Missing header");

    tracing::info!("E2E fixed column width test PASSED");
}

#[test]
fn e2e_table_min_max_width() {
    init_test_logging();
    tracing::info!("Starting E2E min/max column width test");

    let mut table = Table::new()
        .with_column(Column::new("ID").min_width(5).max_width(10))
        .with_column(Column::new("Data"));

    table.add_row_cells(["1", "Some data"]);

    let output = table.render_plain(60);
    tracing::debug!(output = %output, "Min/max width table");

    assert!(output.contains("ID"), "Missing ID column");
    assert!(output.contains("Data"), "Missing Data column");

    tracing::info!("E2E min/max column width test PASSED");
}

#[test]
fn e2e_table_expand() {
    init_test_logging();
    tracing::info!("Starting E2E table expand test");

    let mut table = Table::new()
        .expand(true)
        .with_column(Column::new("A"))
        .with_column(Column::new("B"));

    table.add_row_cells(["X", "Y"]);

    let narrow = table.render_plain(30);
    let wide = table.render_plain(60);

    tracing::debug!(
        narrow_len = narrow.lines().next().map(|l| l.len()),
        "Narrow table"
    );
    tracing::debug!(
        wide_len = wide.lines().next().map(|l| l.len()),
        "Wide table"
    );

    // Wide table should be wider than narrow (expanded to fill)
    let narrow_first_line = narrow.lines().next().unwrap_or("").len();
    let wide_first_line = wide.lines().next().unwrap_or("").len();

    assert!(
        wide_first_line >= narrow_first_line,
        "Expanded table should be at least as wide"
    );

    tracing::info!("E2E table expand test PASSED");
}

#[test]
fn e2e_table_collapse_narrow() {
    init_test_logging();
    tracing::info!("Starting E2E table collapse (narrow width) test");

    let mut table = Table::new()
        .with_column(Column::new("Very Long Header"))
        .with_column(Column::new("Another Long Header"));

    table.add_row_cells(["Short", "Values"]);

    // Render at a very narrow width - should collapse/truncate
    let output = table.render_plain(25);
    tracing::debug!(output = %output, "Narrow table");

    // Should still render the row values even when collapsed
    assert!(output.contains("Short"), "Missing row value 'Short'");
    assert!(output.contains("Values"), "Missing row value 'Values'");

    tracing::info!("E2E table collapse (narrow width) test PASSED");
}

// =============================================================================
// Scenario 4: Wide Characters in Cells
// =============================================================================

#[test]
fn e2e_table_cjk_content() {
    init_test_logging();
    tracing::info!("Starting E2E table with CJK content test");

    let mut table = Table::new()
        .with_column(Column::new("日本語"))
        .with_column(Column::new("English"));

    table.add_row_cells(["東京", "Tokyo"]);
    table.add_row_cells(["大阪", "Osaka"]);
    table.add_row_cells(["京都", "Kyoto"]);

    let output = table.render_plain(40);
    tracing::debug!(output = %output, "CJK table");

    // Verify content preserved
    assert!(output.contains("日本語"), "Missing Japanese header");
    assert!(output.contains("東京"), "Missing Tokyo in Japanese");
    assert!(output.contains("Tokyo"), "Missing Tokyo in English");

    tracing::info!("E2E table with CJK content test PASSED");
}

#[test]
fn e2e_table_emoji_content() {
    init_test_logging();
    tracing::info!("Starting E2E table with emoji content test");

    let mut table = Table::new()
        .with_column(Column::new("Status"))
        .with_column(Column::new("Task"));

    table.add_row_cells(["✓", "Complete"]);
    table.add_row_cells(["⏳", "In Progress"]);
    table.add_row_cells(["✗", "Failed"]);

    let output = table.render_plain(30);
    tracing::debug!(output = %output, "Emoji table");

    assert!(output.contains("✓"), "Missing checkmark");
    assert!(output.contains("⏳"), "Missing hourglass");
    assert!(output.contains("✗"), "Missing X");

    tracing::info!("E2E table with emoji content test PASSED");
}

#[test]
fn e2e_table_mixed_width_chars() {
    init_test_logging();
    tracing::info!("Starting E2E table with mixed-width characters test");

    let mut table = Table::new()
        .with_column(Column::new("Mixed"))
        .with_column(Column::new("Content"));

    table.add_row_cells(["Hello世界!", "αβγδ"]);
    table.add_row_cells(["你好World", "Café"]);

    let output = table.render_plain(40);
    tracing::debug!(output = %output, "Mixed-width table");

    assert!(output.contains("Hello世界!"), "Missing mixed content");
    assert!(output.contains("你好World"), "Missing mixed content");

    tracing::info!("E2E table with mixed-width characters test PASSED");
}

// =============================================================================
// Scenario 5: Multi-line Cells
// =============================================================================

#[test]
fn e2e_table_cell_wrapping() {
    init_test_logging();
    tracing::info!("Starting E2E table cell wrapping test");

    // Note: The current implementation may not support multi-line cells fully,
    // but let's test that long content at least renders without panic
    let mut table = Table::new().with_column(Column::new("Description").width(15));

    table.add_row_cells(["This is a very long piece of text that needs wrapping"]);

    let output = table.render_plain(30);
    tracing::debug!(output = %output, "Wrapped cell table");

    // Should include header and some of the long content
    assert!(output.contains("Description"), "Should have header");
    assert!(output.contains("This is"), "Should include row content");

    tracing::info!("E2E table cell wrapping test PASSED");
}

// =============================================================================
// Scenario 6: Column Alignment
// =============================================================================

#[test]
fn e2e_table_right_align() {
    init_test_logging();
    tracing::info!("Starting E2E table right alignment test");

    let mut table = Table::new()
        .with_column(Column::new("Item"))
        .with_column(Column::new("Price").justify(JustifyMethod::Right));

    table.add_row_cells(["Widget", "$10"]);
    table.add_row_cells(["Gadget", "$100"]);
    table.add_row_cells(["Gizmo", "$1,000"]);

    let output = table.render_plain(30);
    tracing::debug!(output = %output, "Right-aligned table");

    assert!(output.contains("Item"), "Missing header");
    assert!(output.contains("Price"), "Missing header");
    assert!(output.contains("$10"), "Missing price");

    tracing::info!("E2E table right alignment test PASSED");
}

#[test]
fn e2e_table_center_align() {
    init_test_logging();
    tracing::info!("Starting E2E table center alignment test");

    let mut table = Table::new().with_column(
        Column::new("Centered")
            .justify(JustifyMethod::Center)
            .width(20),
    );

    table.add_row_cells(["X"]);
    table.add_row_cells(["Short"]);
    table.add_row_cells(["Medium Text"]);

    let output = table.render_plain(30);
    tracing::debug!(output = %output, "Center-aligned table");

    assert!(output.contains("Centered"), "Missing header");
    assert!(output.contains("X"), "Missing short content");

    tracing::info!("E2E table center alignment test PASSED");
}

// =============================================================================
// Scenario 7: Edge Cases
// =============================================================================

#[test]
fn e2e_table_empty() {
    init_test_logging();
    tracing::info!("Starting E2E empty table test");

    let table = Table::new();
    let output = table.render_plain(40);
    tracing::debug!(output = %output, "Empty table");

    // Empty table should render to empty string or minimal content
    assert!(
        output.is_empty() || output.trim().is_empty(),
        "Empty table should be empty"
    );

    tracing::info!("E2E empty table test PASSED");
}

#[test]
fn e2e_table_columns_no_rows() {
    init_test_logging();
    tracing::info!("Starting E2E table with columns but no rows test");

    let table = Table::new()
        .with_column(Column::new("Header1"))
        .with_column(Column::new("Header2"));

    let output = table.render_plain(40);
    tracing::debug!(output = %output, "Headers-only table");

    // Should show headers
    assert!(output.contains("Header1"), "Missing header");
    assert!(output.contains("Header2"), "Missing header");

    tracing::info!("E2E table with columns but no rows test PASSED");
}

#[test]
fn e2e_table_single_cell() {
    init_test_logging();
    tracing::info!("Starting E2E single cell table test");

    let mut table = Table::new().with_column(Column::new("Solo"));

    table.add_row_cells(["One"]);

    let output = table.render_plain(20);
    tracing::debug!(output = %output, "Single cell table");

    assert!(output.contains("Solo"), "Missing header");
    assert!(output.contains("One"), "Missing cell");

    tracing::info!("E2E single cell table test PASSED");
}

#[test]
fn e2e_table_empty_cells() {
    init_test_logging();
    tracing::info!("Starting E2E table with empty cells test");

    let mut table = Table::new()
        .with_column(Column::new("A"))
        .with_column(Column::new("B"))
        .with_column(Column::new("C"));

    table.add_row_cells(["1", "", "3"]);
    table.add_row_cells(["", "2", ""]);

    let output = table.render_plain(30);
    tracing::debug!(output = %output, "Table with empty cells");

    assert!(output.contains("A"), "Missing header");
    assert!(output.contains("1"), "Missing cell content");
    assert!(output.contains("3"), "Missing cell content");

    tracing::info!("E2E table with empty cells test PASSED");
}

#[test]
fn e2e_table_sparse_rows() {
    init_test_logging();
    tracing::info!("Starting E2E table with sparse rows test");

    // Rows with fewer cells than columns
    let mut table = Table::new()
        .with_column(Column::new("A"))
        .with_column(Column::new("B"))
        .with_column(Column::new("C"));

    table.add_row_cells(["Only A"]); // Missing B and C
    table.add_row_cells(["X", "Y"]); // Missing C

    let output = table.render_plain(40);
    tracing::debug!(output = %output, "Sparse rows table");

    assert!(output.contains("Only A"), "Missing sparse row content");
    assert!(output.contains("X"), "Missing row content");
    assert!(output.contains("Y"), "Missing row content");

    tracing::info!("E2E table with sparse rows test PASSED");
}

#[test]
fn e2e_table_no_edges() {
    init_test_logging();
    tracing::info!("Starting E2E table without edges test");

    let mut table = Table::new()
        .show_edge(false)
        .with_column(Column::new("Col1"))
        .with_column(Column::new("Col2"));

    table.add_row_cells(["A", "B"]);

    let output = table.render_plain(30);
    tracing::debug!(output = %output, "Table without edges");

    // Should have content
    assert!(output.contains("Col1"), "Missing header");
    assert!(output.contains("A"), "Missing cell");

    tracing::info!("E2E table without edges test PASSED");
}

// =============================================================================
// Scenario 8: Styled Tables
// =============================================================================

#[test]
fn e2e_table_with_styles() {
    init_test_logging();
    tracing::info!("Starting E2E styled table test");

    let bold = Style::new().bold();
    let red = Style::new().color(Color::parse("red").unwrap());

    let mut table = Table::new()
        .header_style(bold)
        .border_style(red)
        .with_column(Column::new("Styled"))
        .with_column(Column::new("Table"));

    table.add_row_cells(["Data", "Here"]);

    let segments = table.render(40);
    let output: String = segments.iter().map(|s| s.text.as_ref()).collect();
    tracing::debug!(output = %output, "Styled table output");

    // Check that styles are present (segments have style)
    let has_styled_segments = segments.iter().any(|s| s.style.is_some());
    assert!(has_styled_segments, "Should have styled segments");

    tracing::info!("E2E styled table test PASSED");
}

#[test]
fn e2e_table_alternating_rows() {
    init_test_logging();
    tracing::info!("Starting E2E table with alternating row styles test");

    let style1 = Style::new();
    let style2 = Style::new().dim();

    let mut table = Table::new()
        .row_styles(vec![style1, style2])
        .with_column(Column::new("Row"));

    table.add_row_cells(["One"]);
    table.add_row_cells(["Two"]);
    table.add_row_cells(["Three"]);
    table.add_row_cells(["Four"]);

    let output = table.render_plain(20);
    tracing::debug!(output = %output, "Alternating row styles table");

    assert!(output.contains("One"), "Missing row 1");
    assert!(output.contains("Four"), "Missing row 4");

    tracing::info!("E2E table with alternating row styles test PASSED");
}

// =============================================================================
// Snapshot Tests for Visual Regression
// =============================================================================

#[test]
fn e2e_snapshot_simple_table() {
    init_test_logging();

    let mut table = Table::new()
        .with_column(Column::new("Name"))
        .with_column(Column::new("Age"))
        .with_column(Column::new("City"));

    table.add_row_cells(["Alice", "30", "NYC"]);
    table.add_row_cells(["Bob", "25", "LA"]);
    table.add_row_cells(["Carol", "35", "Chicago"]);

    let output = table.render_plain(50);
    insta::assert_snapshot!("e2e_simple_table", output);
}

#[test]
fn e2e_snapshot_ascii_table() {
    init_test_logging();

    let mut table = Table::new()
        .ascii()
        .with_column(Column::new("ID"))
        .with_column(Column::new("Status"));

    table.add_row_cells(["1", "OK"]);
    table.add_row_cells(["2", "FAIL"]);

    let output = table.render_plain(30);
    insta::assert_snapshot!("e2e_ascii_table", output);
}

#[test]
fn e2e_snapshot_table_with_title() {
    init_test_logging();

    let mut table = Table::new()
        .title("Monthly Report")
        .caption("Generated: 2024-01-15")
        .with_column(Column::new("Month"))
        .with_column(Column::new("Revenue").justify(JustifyMethod::Right));

    table.add_row_cells(["January", "$10,000"]);
    table.add_row_cells(["February", "$12,500"]);
    table.add_row_cells(["March", "$15,000"]);

    let output = table.render_plain(40);
    insta::assert_snapshot!("e2e_table_with_title", output);
}

#[test]
fn e2e_snapshot_table_all_features() {
    init_test_logging();

    let mut table = Table::new()
        .title("Complete Table")
        .caption("All features enabled")
        .show_footer(true)
        .show_lines(true)
        .with_column(Column::new("Product").footer("Total"))
        .with_column(
            Column::new("Qty")
                .justify(JustifyMethod::Center)
                .footer("10"),
        )
        .with_column(
            Column::new("Price")
                .justify(JustifyMethod::Right)
                .footer("$250"),
        );

    table.add_row_cells(["Widget", "3", "$75"]);
    table.add_row_cells(["Gadget", "5", "$125"]);
    table.add_row_cells(["Gizmo", "2", "$50"]);

    let output = table.render_plain(45);
    insta::assert_snapshot!("e2e_table_all_features", output);
}

// =============================================================================
// Scenario 9: Table Leading (Extra Blank Lines Between Rows)
// =============================================================================

#[test]
fn e2e_table_leading_basic() {
    init_test_logging();
    tracing::info!("Starting E2E table leading basic test");

    let mut table = Table::new()
        .leading(1)
        .with_column(Column::new("Name"))
        .with_column(Column::new("Value"));

    table.add_row_cells(["Row1", "A"]);
    table.add_row_cells(["Row2", "B"]);
    table.add_row_cells(["Row3", "C"]);

    let output = table.render_plain(30);
    tracing::debug!(output = %output, "Table with leading=1");

    // Verify content
    assert!(output.contains("Row1"), "Missing Row1");
    assert!(output.contains("Row2"), "Missing Row2");
    assert!(output.contains("Row3"), "Missing Row3");

    // Count non-empty lines (should have more due to leading)
    let lines: Vec<&str> = output.lines().collect();
    tracing::debug!(line_count = lines.len(), "Line count");

    // With leading=1, there should be extra blank lines between rows
    // Check for blank lines within the table (lines with only borders and spaces)
    let has_blank_rows = lines.iter().any(|line| {
        let trimmed = line.replace("│", "").replace("|", "");
        trimmed.trim().is_empty()
    });
    assert!(has_blank_rows, "Should have blank rows for leading");

    tracing::info!("E2E table leading basic test PASSED");
}

#[test]
fn e2e_table_leading_with_show_lines() {
    init_test_logging();
    tracing::info!("Starting E2E table leading with show_lines test");

    let mut table = Table::new()
        .leading(2)
        .show_lines(true)
        .with_column(Column::new("Item"));

    table.add_row_cells(["First"]);
    table.add_row_cells(["Second"]);
    table.add_row_cells(["Third"]);

    let output = table.render_plain(20);
    tracing::debug!(output = %output, "Table with leading=2 and show_lines=true");

    // Content should be present
    assert!(output.contains("First"), "Missing First");
    assert!(output.contains("Second"), "Missing Second");
    assert!(output.contains("Third"), "Missing Third");

    // Should have both row separators (from show_lines) and blank rows (from leading)
    let separator_count = output.matches("├").count() + output.matches("+").count();
    tracing::debug!(separator_count = separator_count, "Row separator count");

    assert!(separator_count >= 2, "Should have row separators");

    tracing::info!("E2E table leading with show_lines test PASSED");
}

#[test]
fn e2e_table_leading_zero() {
    init_test_logging();
    tracing::info!("Starting E2E table leading=0 test");

    let mut table = Table::new().leading(0).with_column(Column::new("Col"));

    table.add_row_cells(["A"]);
    table.add_row_cells(["B"]);

    let output = table.render_plain(20);
    tracing::debug!(output = %output, "Table with leading=0");

    // With leading=0, no extra blank rows should be added
    let lines: Vec<&str> = output.lines().collect();

    // Count lines that are purely blank (only borders and spaces, no content)
    let blank_row_count = lines
        .iter()
        .filter(|line| {
            let trimmed = line.replace("│", "").replace("|", "");
            trimmed.trim().is_empty() && !line.contains("─") && !line.contains("-")
        })
        .count();

    assert_eq!(blank_row_count, 0, "leading=0 should not add blank rows");

    tracing::info!("E2E table leading=0 test PASSED");
}

#[test]
fn e2e_table_leading_ascii() {
    init_test_logging();
    tracing::info!("Starting E2E table leading with ASCII box test");

    let mut table = Table::new()
        .leading(1)
        .ascii()
        .with_column(Column::new("X"))
        .with_column(Column::new("Y"));

    table.add_row_cells(["1", "2"]);
    table.add_row_cells(["3", "4"]);

    let output = table.render_plain(20);
    tracing::debug!(output = %output, "ASCII table with leading=1");

    // Verify ASCII box characters
    assert!(output.contains("|"), "Should have ASCII vertical bars");
    assert!(output.contains("-"), "Should have ASCII horizontal lines");

    // Content present
    assert!(output.contains("1"), "Missing cell 1");
    assert!(output.contains("4"), "Missing cell 4");

    tracing::info!("E2E table leading with ASCII box test PASSED");
}

#[test]
fn e2e_snapshot_table_with_leading() {
    init_test_logging();

    let mut table = Table::new()
        .title("Spaced Table")
        .leading(1)
        .with_column(Column::new("Name"))
        .with_column(Column::new("Score").justify(JustifyMethod::Right));

    table.add_row_cells(["Alice", "95"]);
    table.add_row_cells(["Bob", "87"]);
    table.add_row_cells(["Carol", "92"]);

    let output = table.render_plain(35);
    insta::assert_snapshot!("e2e_table_with_leading", output);
}

#[test]
fn e2e_snapshot_table_leading_with_separators() {
    init_test_logging();

    let mut table = Table::new()
        .leading(1)
        .show_lines(true)
        .with_column(Column::new("Task"))
        .with_column(Column::new("Status"));

    table.add_row_cells(["Build", "Done"]);
    table.add_row_cells(["Test", "Running"]);
    table.add_row_cells(["Deploy", "Pending"]);

    let output = table.render_plain(30);
    insta::assert_snapshot!("e2e_table_leading_with_separators", output);
}

// =============================================================================
// Scenario 25: Column Width Edge Cases (bd-2ec5)
// =============================================================================

#[test]
fn e2e_table_extremely_narrow_width() {
    // Edge case: table width less than minimum overhead
    init_test_logging();
    tracing::info!("Testing table with extremely narrow width");

    let mut table = Table::new()
        .with_column(Column::new("Name"))
        .with_column(Column::new("Value"));

    table.add_row_cells(["Alice", "100"]);

    // Render at width 5 - barely enough for borders
    let output = table.render_plain(5);

    // Should still produce some output without panicking
    assert!(
        !output.is_empty(),
        "Table should produce output even at narrow width"
    );
    tracing::info!("Extremely narrow width test PASSED");
}

#[test]
fn e2e_table_ratio_column_sizing() {
    // Test ratio-based column sizing
    init_test_logging();
    tracing::info!("Testing ratio-based column sizing");

    let mut table = Table::new()
        .expand(true)
        .with_column(Column::new("Small").ratio(1))
        .with_column(Column::new("Large").ratio(3));

    table.add_row_cells(["A", "B"]);

    let output = table.render_plain(60);

    // Both columns should be present
    assert!(output.contains("Small"), "Missing 'Small' header");
    assert!(output.contains("Large"), "Missing 'Large' header");

    // Verify the output lines are consistent width (expanded table)
    let lines: Vec<&str> = output.lines().filter(|l| !l.is_empty()).collect();
    if lines.len() > 1 {
        let first_len = cells::cell_len(lines[0]);
        for line in &lines[1..] {
            let len = cells::cell_len(line);
            assert!(
                len == first_len || len == first_len - 1 || len == first_len + 1,
                "Lines should have consistent width, got {} vs {}",
                len,
                first_len
            );
        }
    }
    tracing::info!("Ratio column sizing test PASSED");
}

#[test]
fn e2e_table_mixed_ratio_and_fixed() {
    // Test mixed ratio and fixed-width columns
    init_test_logging();
    tracing::info!("Testing mixed ratio and fixed-width columns");

    let mut table = Table::new()
        .expand(true)
        .with_column(Column::new("Fixed").width(10))
        .with_column(Column::new("Flex1").ratio(1))
        .with_column(Column::new("Flex2").ratio(2));

    table.add_row_cells(["X", "Y", "Z"]);

    let output = table.render_plain(60);

    assert!(output.contains("Fixed"), "Missing 'Fixed' header");
    assert!(output.contains("Flex1"), "Missing 'Flex1' header");
    assert!(output.contains("Flex2"), "Missing 'Flex2' header");
    tracing::info!("Mixed ratio and fixed columns test PASSED");
}

#[test]
fn e2e_table_all_columns_at_minimum() {
    // Edge case: all columns already at minimum width, can't shrink
    init_test_logging();
    tracing::info!("Testing columns at minimum width");

    let mut table = Table::new()
        .with_column(Column::new("A").min_width(5))
        .with_column(Column::new("B").min_width(5))
        .with_column(Column::new("C").min_width(5));

    table.add_row_cells(["1", "2", "3"]);

    // Render at width smaller than sum of minimums + overhead
    let output = table.render_plain(10);

    // Should still produce output without panicking
    assert!(!output.is_empty(), "Should handle columns at minimum");
    tracing::info!("Columns at minimum width test PASSED");
}

#[test]
fn e2e_table_conflicting_min_max() {
    // Edge case: min_width greater than max_width should use min
    init_test_logging();
    tracing::info!("Testing conflicting min/max width constraints");

    let mut table = Table::new().with_column(Column::new("Header").min_width(20).max_width(10));

    table.add_row_cells(["Content"]);

    let output = table.render_plain(50);

    // Should still render without panicking
    assert!(output.contains("Header"), "Should contain header");
    assert!(output.contains("Content"), "Should contain content");
    tracing::info!("Conflicting min/max test PASSED");
}

#[test]
fn e2e_table_zero_width_column() {
    // Edge case: column with width(0)
    init_test_logging();
    tracing::info!("Testing zero-width column");

    let mut table = Table::new()
        .with_column(Column::new("Normal"))
        .with_column(Column::new("Zero").width(0));

    table.add_row_cells(["A", "B"]);

    let output = table.render_plain(40);

    // Should still produce valid output
    assert!(output.contains("Normal"), "Should contain Normal header");
    tracing::info!("Zero width column test PASSED");
}

#[test]
fn e2e_table_very_long_word_no_wrap() {
    // Edge case: long word that can't wrap in narrow column
    init_test_logging();
    tracing::info!("Testing very long word in narrow column");

    let mut table = Table::new()
        .with_column(Column::new("Short").max_width(5))
        .with_column(Column::new("Normal"));

    table.add_row_cells(["Supercalifragilisticexpialidocious", "OK"]);

    let output = table.render_plain(40);

    // Should render without panicking (word may be truncated/wrapped)
    assert!(output.contains("Short"), "Should contain Short header");
    assert!(output.contains("OK"), "Should contain OK");
    tracing::info!("Long word no wrap test PASSED");
}

#[test]
fn e2e_table_all_ratios_zero() {
    // Edge case: expand with all ratios being 0
    init_test_logging();
    tracing::info!("Testing expand with zero ratios");

    let mut table = Table::new()
        .expand(true)
        .with_column(Column::new("A").ratio(0))
        .with_column(Column::new("B").ratio(0));

    table.add_row_cells(["X", "Y"]);

    let output = table.render_plain(50);

    // Should fall back to weight-based expansion
    assert!(output.contains("A"), "Should contain A header");
    assert!(output.contains("B"), "Should contain B header");
    tracing::info!("Zero ratios test PASSED");
}

#[test]
fn e2e_table_single_huge_cell() {
    // Edge case: single cell much larger than table width
    init_test_logging();
    tracing::info!("Testing single huge cell");

    let huge_content = "A".repeat(200);
    let mut table = Table::new().with_column(Column::new("Data"));

    table.add_row_cells([huge_content.as_str()]);

    let output = table.render_plain(30);

    // Should wrap/truncate without panicking
    assert!(!output.is_empty(), "Should produce output");
    assert!(output.contains("Data"), "Should contain header");
    tracing::info!("Single huge cell test PASSED");
}

#[test]
fn e2e_table_width_equals_overhead() {
    // Edge case: table width exactly equals border overhead
    init_test_logging();
    tracing::info!("Testing width equals overhead");

    let mut table = Table::new()
        .with_column(Column::new("X"))
        .with_column(Column::new("Y"));

    table.add_row_cells(["1", "2"]);

    // With show_edge=true, 2 columns, padding 1:
    // overhead = 2 (border) + 3 (separator) + 2 (edge padding) = 7
    let output = table.render_plain(7);

    // Should handle edge case gracefully
    assert!(!output.is_empty(), "Should produce some output");
    tracing::info!("Width equals overhead test PASSED");
}
