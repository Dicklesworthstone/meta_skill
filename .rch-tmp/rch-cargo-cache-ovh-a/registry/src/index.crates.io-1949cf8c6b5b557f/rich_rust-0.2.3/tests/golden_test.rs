//! Golden file (snapshot) tests for visual regression detection.
//!
//! These tests capture the expected rendering output of various components
//! and detect visual regressions by comparing actual output against stored expectations.
//!
//! ## Running Tests
//!
//! ```bash
//! # Run all golden tests
//! cargo test --test golden_test
//!
//! # Update snapshots when intentional changes are made
//! cargo insta test --accept
//!
//! # Review pending snapshots interactively
//! cargo insta review
//! ```
//!
//! ## Environment Variables
//!
//! - `INSTA_UPDATE=always` - Auto-accept new snapshots
//! - `INSTA_UPDATE=unseen` - Only update new snapshots, fail on changed
//! - `INSTA_UPDATE=no` - Never update, fail on any difference

mod common;

use common::init_test_logging;
use rich_rust::prelude::*;

/// Strip ANSI escape codes for text-only comparison.
fn strip_ansi(s: &str) -> String {
    let ansi_regex = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    ansi_regex.replace_all(s, "").to_string()
}

/// Collect segments into a single string.
fn segments_to_string(segments: Vec<Segment>) -> String {
    segments.into_iter().map(|s| s.text).collect()
}

// =============================================================================
// Rule Tests
// =============================================================================

#[test]
fn golden_rule_simple() {
    init_test_logging();
    let rule = Rule::new();
    let output = segments_to_string(rule.render(40));
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("rule_simple", plain);
}

#[test]
fn golden_rule_with_title() {
    init_test_logging();
    let rule = Rule::with_title("Section Title");
    let output = segments_to_string(rule.render(40));
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("rule_with_title", plain);
}

#[test]
fn golden_rule_left_aligned() {
    init_test_logging();
    let rule = Rule::with_title("Left").align_left();
    let output = segments_to_string(rule.render(40));
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("rule_left_aligned", plain);
}

#[test]
fn golden_rule_right_aligned() {
    init_test_logging();
    let rule = Rule::with_title("Right").align_right();
    let output = segments_to_string(rule.render(40));
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("rule_right_aligned", plain);
}

// =============================================================================
// Panel Tests
// =============================================================================

#[test]
fn golden_panel_simple() {
    init_test_logging();
    let panel = Panel::from_text("Hello, Panel!").width(30);
    let output = segments_to_string(panel.render(40));
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("panel_simple", plain);
}

#[test]
fn golden_panel_with_title() {
    init_test_logging();
    let panel = Panel::from_text("Content here").title("My Panel").width(30);
    let output = segments_to_string(panel.render(40));
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("panel_with_title", plain);
}

#[test]
fn golden_panel_with_title_and_subtitle() {
    init_test_logging();
    let panel = Panel::from_text("Multi-line\nContent\nHere")
        .title("Info Panel")
        .subtitle("v1.0")
        .width(30);
    let output = segments_to_string(panel.render(40));
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("panel_with_title_subtitle", plain);
}

#[test]
fn golden_panel_square() {
    init_test_logging();
    let panel = Panel::from_text("Square corners")
        .square()
        .title("Square")
        .width(30);
    let output = segments_to_string(panel.render(40));
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("panel_square", plain);
}

#[test]
fn golden_panel_ascii() {
    init_test_logging();
    let panel = Panel::from_text("ASCII safe!")
        .ascii()
        .title("Legacy")
        .width(25);
    let output = segments_to_string(panel.render(40));
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("panel_ascii", plain);
}

// =============================================================================
// Table Tests
// =============================================================================

#[test]
fn golden_table_basic() {
    init_test_logging();
    let mut table = Table::new()
        .with_column(Column::new("Name"))
        .with_column(Column::new("Value"));

    table.add_row_cells(["Key1", "Value1"]);
    table.add_row_cells(["Key2", "Value2"]);
    table.add_row_cells(["Key3", "Value3"]);

    let output = segments_to_string(table.render(40));
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("table_basic", plain);
}

#[test]
fn golden_table_with_title() {
    init_test_logging();
    let mut table = Table::new()
        .title("Data Table")
        .with_column(Column::new("ID").width(5))
        .with_column(Column::new("Name").min_width(10))
        .with_column(Column::new("Score").justify(JustifyMethod::Right));

    table.add_row_cells(["1", "Alice", "100"]);
    table.add_row_cells(["2", "Bob", "95"]);
    table.add_row_cells(["3", "Charlie", "87"]);

    let output = segments_to_string(table.render(50));
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("table_with_title", plain);
}

#[test]
fn golden_table_ascii() {
    init_test_logging();
    let mut table = Table::new()
        .ascii()
        .with_column(Column::new("Key"))
        .with_column(Column::new("Value"));

    table.add_row_cells(["version", "1.0.0"]);
    table.add_row_cells(["author", "Test"]);

    let output = segments_to_string(table.render(40));
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("table_ascii", plain);
}

#[test]
fn golden_table_no_header() {
    init_test_logging();
    let mut table = Table::new()
        .show_header(false)
        .with_column(Column::new("A"))
        .with_column(Column::new("B"));

    table.add_row_cells(["Data 1", "Data 2"]);
    table.add_row_cells(["Data 3", "Data 4"]);

    let output = segments_to_string(table.render(40));
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("table_no_header", plain);
}

#[test]
fn golden_table_wide() {
    init_test_logging();
    let mut table = Table::new()
        .with_column(Column::new("Name").min_width(15))
        .with_column(Column::new("Department").min_width(12))
        .with_column(Column::new("Email").min_width(20));

    table.add_row_cells(["John Smith", "Engineering", "john@example.com"]);
    table.add_row_cells(["Jane Doe", "Marketing", "jane@example.com"]);

    let output = segments_to_string(table.render(60));
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("table_wide", plain);
}

// =============================================================================
// Tree Tests
// =============================================================================

#[test]
fn golden_tree_simple() {
    init_test_logging();
    let tree = Tree::new(TreeNode::new("Root"))
        .child(TreeNode::new("Child 1"))
        .child(TreeNode::new("Child 2"))
        .child(TreeNode::new("Child 3"));

    let output = segments_to_string(tree.render());
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("tree_simple", plain);
}

#[test]
fn golden_tree_nested() {
    init_test_logging();
    let tree = Tree::new(TreeNode::new("Project"))
        .child(
            TreeNode::new("src")
                .child(TreeNode::new("main.rs"))
                .child(TreeNode::new("lib.rs")),
        )
        .child(
            TreeNode::new("tests")
                .child(TreeNode::new("unit.rs"))
                .child(TreeNode::new("integration.rs")),
        )
        .child(TreeNode::new("Cargo.toml"));

    let output = segments_to_string(tree.render());
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("tree_nested", plain);
}

#[test]
fn golden_tree_ascii() {
    init_test_logging();
    let tree = Tree::new(TreeNode::new("Root"))
        .guides(TreeGuides::Ascii)
        .child(TreeNode::new("Branch A").child(TreeNode::new("Leaf 1")))
        .child(TreeNode::new("Branch B").child(TreeNode::new("Leaf 2")));

    let output = segments_to_string(tree.render());
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("tree_ascii", plain);
}

// =============================================================================
// Columns Tests
// =============================================================================

#[test]
fn golden_columns_basic() {
    init_test_logging();
    let columns = Columns::from_strings(&["Column 1", "Column 2", "Column 3"]).equal_width(true);

    let output = segments_to_string(columns.render_flat(60));
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("columns_basic", plain);
}

#[test]
fn golden_columns_multiline() {
    init_test_logging();
    let columns = Columns::from_strings(&[
        "First column\nwith multiple\nlines",
        "Second column\nalso multiline",
        "Third",
    ])
    .equal_width(true);

    let output = segments_to_string(columns.render_flat(60));
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("columns_multiline", plain);
}

// =============================================================================
// Progress Bar Tests
// =============================================================================

#[test]
fn golden_progress_bar_empty() {
    init_test_logging();
    let mut bar = ProgressBar::new().width(30);
    bar.set_progress(0.0);
    let output = segments_to_string(bar.render(40));
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("progress_bar_empty", plain);
}

#[test]
fn golden_progress_bar_half() {
    init_test_logging();
    let mut bar = ProgressBar::new().width(30);
    bar.set_progress(0.5);
    let output = segments_to_string(bar.render(40));
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("progress_bar_half", plain);
}

#[test]
fn golden_progress_bar_full() {
    init_test_logging();
    let mut bar = ProgressBar::new().width(30);
    bar.set_progress(1.0);
    let output = segments_to_string(bar.render(40));
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("progress_bar_full", plain);
}

#[test]
fn golden_progress_bar_ascii() {
    init_test_logging();
    let mut bar = ProgressBar::new().width(30).bar_style(BarStyle::Ascii);
    bar.set_progress(0.75);
    let output = segments_to_string(bar.render(40));
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("progress_bar_ascii", plain);
}

// =============================================================================
// Text Wrapping Tests
// =============================================================================

#[test]
fn golden_text_wrapped() {
    init_test_logging();
    // Use Panel to demonstrate truncation to width (Panel does not wrap content).
    let long_text = "This is a very long piece of text that should be wrapped \
        across multiple lines when rendered at a narrow width.";

    let panel = Panel::from_text(long_text).width(35);
    let output = segments_to_string(panel.render(40));
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("text_wrapped", plain);
}

// =============================================================================
// Combined/Complex Tests
// =============================================================================

#[test]
fn golden_combined_rules_and_panels() {
    init_test_logging();
    let mut output = String::new();

    // Add a rule
    let rule = Rule::with_title("Section 1");
    output += &segments_to_string(rule.render(40));

    // Add a panel
    let panel = Panel::from_text("Content in section 1").width(38);
    output += &segments_to_string(panel.render(40));

    // Add another rule
    let rule2 = Rule::with_title("Section 2");
    output += &segments_to_string(rule2.render(40));

    let plain = strip_ansi(&output);
    insta::assert_snapshot!("combined_rules_panels", plain);
}

// =============================================================================
// Edge Case Tests
// =============================================================================

#[test]
fn golden_empty_table() {
    init_test_logging();
    let table = Table::new()
        .with_column(Column::new("A"))
        .with_column(Column::new("B"));

    let output = segments_to_string(table.render(40));
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("table_empty", plain);
}

#[test]
fn golden_single_cell_table() {
    init_test_logging();
    let mut table = Table::new().with_column(Column::new("Only"));
    table.add_row_cells(["Value"]);

    let output = segments_to_string(table.render(40));
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("table_single_cell", plain);
}

#[test]
fn golden_panel_empty() {
    init_test_logging();
    let panel = Panel::from_text("").width(20);
    let output = segments_to_string(panel.render(30));
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("panel_empty", plain);
}

#[test]
fn golden_rule_very_short() {
    init_test_logging();
    let rule = Rule::with_title("X");
    let output = segments_to_string(rule.render(10));
    let plain = strip_ansi(&output);
    insta::assert_snapshot!("rule_very_short", plain);
}
