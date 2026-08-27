//! Test fixtures for rich_rust.
//!
//! This module provides pre-built, deterministic test fixtures for use in
//! unit and integration tests. All fixtures return consistent values
//! across test runs.
//!
//! # Example
//!
//! ```rust,ignore
//! use common::fixtures::*;
//!
//! #[test]
//! fn test_table_rendering() {
//!     let console = sample_console();
//!     let table = sample_table();
//!     let segments = table.render(console.width());
//!     assert!(!segments.is_empty());
//! }
//! ```

#![allow(dead_code)]

use rich_rust::prelude::*;
use rich_rust::renderables::{Cell, Column, Row, Table, Tree, TreeGuides, TreeNode};

// =============================================================================
// Console Fixtures
// =============================================================================

/// Create a Console with standard test dimensions (80x24).
///
/// The console is configured for deterministic output:
/// - Width: 80 columns
/// - Height: 24 rows (standard terminal)
/// - Force terminal mode enabled
/// - Color system: TrueColor
#[must_use]
pub fn sample_console() -> Console {
    Console::builder()
        .force_terminal(true)
        .width(80)
        .height(24)
        .color_system(ColorSystem::TrueColor)
        .build()
}

/// Create a Console with narrow width for wrap testing.
#[must_use]
pub fn narrow_console() -> Console {
    Console::builder()
        .force_terminal(true)
        .width(40)
        .height(24)
        .color_system(ColorSystem::TrueColor)
        .build()
}

/// Create a Console with wide width for layout testing.
#[must_use]
pub fn wide_console() -> Console {
    Console::builder()
        .force_terminal(true)
        .width(120)
        .height(50)
        .color_system(ColorSystem::TrueColor)
        .build()
}

/// Create a Console with standard (4-bit) color support.
#[must_use]
pub fn standard_color_console() -> Console {
    Console::builder()
        .force_terminal(true)
        .width(80)
        .height(24)
        .color_system(ColorSystem::Standard)
        .build()
}

/// Create a Console with 256 color support.
#[must_use]
pub fn color256_console() -> Console {
    Console::builder()
        .force_terminal(true)
        .width(80)
        .height(24)
        .color_system(ColorSystem::EightBit)
        .build()
}

// =============================================================================
// Style Fixtures
// =============================================================================

/// Create a vector of common style combinations for testing.
///
/// Returns styles covering:
/// - Basic attributes (bold, italic, underline)
/// - Standard colors (red, green, blue, etc.)
/// - Combined styles
/// - Background colors
#[must_use]
pub fn sample_styles() -> Vec<(&'static str, Style)> {
    vec![
        ("plain", Style::new()),
        ("bold", Style::new().bold()),
        ("italic", Style::new().italic()),
        ("underline", Style::new().underline()),
        ("dim", Style::new().dim()),
        ("bold_italic", Style::new().bold().italic()),
        ("red", Style::new().color(Color::parse("red").unwrap())),
        ("green", Style::new().color(Color::parse("green").unwrap())),
        ("blue", Style::new().color(Color::parse("blue").unwrap())),
        (
            "yellow",
            Style::new().color(Color::parse("yellow").unwrap()),
        ),
        (
            "magenta",
            Style::new().color(Color::parse("magenta").unwrap()),
        ),
        ("cyan", Style::new().color(Color::parse("cyan").unwrap())),
        (
            "bold_red",
            Style::new().bold().color(Color::parse("red").unwrap()),
        ),
        (
            "red_on_white",
            Style::new()
                .color(Color::parse("red").unwrap())
                .bgcolor(Color::parse("white").unwrap()),
        ),
        (
            "bright_green",
            Style::new().color(Color::parse("bright_green").unwrap()),
        ),
        (
            "rgb_color",
            Style::new().color(Color::from_rgb(100, 150, 200)),
        ),
        (
            "hex_color",
            Style::new().color(Color::parse("#ff8800").unwrap()),
        ),
    ]
}

/// Create a simple bold style.
#[must_use]
pub fn bold_style() -> Style {
    Style::new().bold()
}

/// Create a red color style.
#[must_use]
pub fn red_style() -> Style {
    Style::new().color(Color::parse("red").unwrap())
}

/// Create a green color style.
#[must_use]
pub fn green_style() -> Style {
    Style::new().color(Color::parse("green").unwrap())
}

/// Create a style with all common attributes.
#[must_use]
pub fn full_style() -> Style {
    Style::new()
        .bold()
        .italic()
        .underline()
        .color(Color::parse("cyan").unwrap())
        .bgcolor(Color::parse("black").unwrap())
}

// =============================================================================
// Text Fixtures
// =============================================================================

/// Create a vector of sample Text objects for testing.
///
/// Returns texts covering:
/// - Plain text
/// - Styled text
/// - Multi-line text
/// - Text with spans
#[must_use]
pub fn sample_texts() -> Vec<(&'static str, Text)> {
    vec![
        ("plain", Text::new("Hello, World!")),
        ("empty", Text::new("")),
        ("multiline", Text::new("Line 1\nLine 2\nLine 3")),
        ("with_tabs", Text::new("Col1\tCol2\tCol3")),
        (
            "long",
            Text::new(
                "This is a longer piece of text that might need to be wrapped when rendered in a narrow console.",
            ),
        ),
        ("styled", {
            let mut t = Text::new("Styled ");
            t.append_styled("text", Style::new().bold());
            t
        }),
        ("multi_styled", {
            let mut t = Text::new("");
            t.append_styled("Red", Style::new().color(Color::parse("red").unwrap()));
            t.append(" and ");
            t.append_styled("Blue", Style::new().color(Color::parse("blue").unwrap()));
            t
        }),
    ]
}

/// Create plain text.
#[must_use]
pub fn plain_text() -> Text {
    Text::new("Hello, World!")
}

/// Create styled text with bold.
#[must_use]
pub fn bold_text() -> Text {
    let mut text = Text::new("Bold ");
    text.append_styled("text", Style::new().bold());
    text
}

/// Create multi-line text.
#[must_use]
pub fn multiline_text() -> Text {
    Text::new("Line 1\nLine 2\nLine 3\nLine 4\nLine 5")
}

/// Create text that needs wrapping.
#[must_use]
pub fn long_text() -> Text {
    Text::new(
        "This is a longer piece of text that demonstrates word wrapping behavior. \
         It contains multiple sentences and should wrap nicely at word boundaries \
         when rendered in a console with limited width.",
    )
}

/// Create text with multiple styles.
#[must_use]
pub fn rainbow_text() -> Text {
    // Use valid color names
    let colors = ["red", "yellow", "green", "cyan", "blue", "magenta"];
    let mut text = Text::new("");
    for (i, color) in colors.iter().enumerate() {
        if i > 0 {
            text.append(" ");
        }
        let style = Style::new().color(Color::parse(color).unwrap());
        text.append_styled(&color.to_uppercase(), style);
    }
    text
}

// =============================================================================
// Table Fixtures
// =============================================================================

/// Create a simple table for testing.
///
/// Returns a 3-column table with sample data:
/// | Name  | Age | City      |
/// |-------|-----|-----------|
/// | Alice | 30  | New York  |
/// | Bob   | 25  | London    |
/// | Carol | 35  | Tokyo     |
#[must_use]
pub fn sample_table() -> Table {
    Table::new()
        .with_column(Column::new("Name").style(Style::new().bold()))
        .with_column(Column::new("Age").justify(JustifyMethod::Right))
        .with_column(Column::new("City"))
        .with_row_cells(["Alice", "30", "New York"])
        .with_row_cells(["Bob", "25", "London"])
        .with_row_cells(["Carol", "35", "Tokyo"])
}

/// Create an empty table for edge case testing.
#[must_use]
pub fn empty_table() -> Table {
    Table::new()
}

/// Create a table with headers only.
#[must_use]
pub fn headers_only_table() -> Table {
    Table::new()
        .with_column(Column::new("Name"))
        .with_column(Column::new("Value"))
}

/// Create a single-cell table.
#[must_use]
pub fn single_cell_table() -> Table {
    Table::new()
        .with_column(Column::new("Data"))
        .with_row_cells(["Single cell"])
}

/// Create a wide table for testing horizontal scrolling/truncation.
#[must_use]
pub fn wide_table() -> Table {
    Table::new()
        .with_column(Column::new("Column 1"))
        .with_column(Column::new("Column 2"))
        .with_column(Column::new("Column 3"))
        .with_column(Column::new("Column 4"))
        .with_column(Column::new("Column 5"))
        .with_column(Column::new("Column 6"))
        .with_row_cells(["Data 1", "Data 2", "Data 3", "Data 4", "Data 5", "Data 6"])
}

/// Create a table with styled cells.
#[must_use]
pub fn styled_table() -> Table {
    Table::new()
        .title("Styled Table")
        .with_column(
            Column::new("Status")
                .style(Style::new().bold())
                .justify(JustifyMethod::Center),
        )
        .with_column(Column::new("Message"))
        .with_row(Row::new(vec![
            Cell::new("OK").style(Style::new().color(Color::parse("green").unwrap())),
            Cell::new("All systems operational"),
        ]))
        .with_row(Row::new(vec![
            Cell::new("WARN").style(Style::new().color(Color::parse("yellow").unwrap())),
            Cell::new("High memory usage"),
        ]))
        .with_row(Row::new(vec![
            Cell::new("ERR").style(Style::new().color(Color::parse("red").unwrap())),
            Cell::new("Connection failed"),
        ]))
}

// =============================================================================
// Tree Fixtures
// =============================================================================

/// Create a sample tree for testing.
///
/// Returns a tree with the following structure:
/// ```text
/// Project
/// ├── src
/// │   ├── main.rs
/// │   └── lib.rs
/// ├── tests
/// │   └── test.rs
/// └── Cargo.toml
/// ```
#[must_use]
pub fn sample_tree() -> Tree {
    Tree::new(
        TreeNode::new("Project")
            .child(
                TreeNode::new("src")
                    .child(TreeNode::new("main.rs"))
                    .child(TreeNode::new("lib.rs")),
            )
            .child(TreeNode::new("tests").child(TreeNode::new("test.rs")))
            .child(TreeNode::new("Cargo.toml")),
    )
}

/// Create an empty tree (root only).
#[must_use]
pub fn empty_tree() -> Tree {
    Tree::new(TreeNode::new("Root"))
}

/// Create a deep tree for testing nested rendering.
#[must_use]
pub fn deep_tree() -> Tree {
    Tree::new(
        TreeNode::new("Level 0").child(
            TreeNode::new("Level 1").child(
                TreeNode::new("Level 2").child(
                    TreeNode::new("Level 3")
                        .child(TreeNode::new("Level 4").child(TreeNode::new("Level 5"))),
                ),
            ),
        ),
    )
}

/// Create a wide tree for testing many siblings.
#[must_use]
pub fn wide_tree() -> Tree {
    Tree::new(
        TreeNode::new("Parent")
            .child(TreeNode::new("Child 1"))
            .child(TreeNode::new("Child 2"))
            .child(TreeNode::new("Child 3"))
            .child(TreeNode::new("Child 4"))
            .child(TreeNode::new("Child 5"))
            .child(TreeNode::new("Child 6"))
            .child(TreeNode::new("Child 7"))
            .child(TreeNode::new("Child 8")),
    )
}

/// Create a tree with styled nodes.
#[must_use]
pub fn styled_tree() -> Tree {
    // Create styled Text labels for the tree nodes
    let mut root_label = Text::new("Root");
    root_label.stylize_all(Style::new().bold());

    let mut important_label = Text::new("Important");
    important_label.stylize_all(Style::new().color(Color::parse("red").unwrap()));

    let mut success_label = Text::new("Success");
    success_label.stylize_all(Style::new().color(Color::parse("green").unwrap()));

    Tree::new(
        TreeNode::new(root_label)
            .child(TreeNode::new(important_label))
            .child(TreeNode::new(success_label))
            .child(TreeNode::new("Normal")),
    )
    .guides(TreeGuides::Unicode)
}

// =============================================================================
// Panel Fixtures
// =============================================================================

/// Create a simple panel for testing.
#[must_use]
pub fn sample_panel() -> Panel<'static> {
    Panel::from_text("This is panel content.")
        .title("Sample Panel")
        .border_style(Style::new().color(Color::parse("blue").unwrap()))
}

/// Create a panel without title.
#[must_use]
pub fn untitled_panel() -> Panel<'static> {
    Panel::from_text("Content without a title.")
}

/// Create a panel with styled content.
#[must_use]
pub fn styled_panel() -> Panel<'static> {
    // Panel::from_text takes &str, so we'll use a static string
    Panel::from_text("Important: This is styled content")
        .title("Styled Panel")
        .title_align(JustifyMethod::Center)
}

// =============================================================================
// Rule Fixtures
// =============================================================================

/// Create a simple rule.
#[must_use]
pub fn sample_rule() -> Rule {
    Rule::new()
}

/// Create a rule with title.
#[must_use]
pub fn titled_rule() -> Rule {
    Rule::with_title("Section Title")
}

/// Create a styled rule.
#[must_use]
pub fn styled_rule() -> Rule {
    Rule::with_title("Styled Rule").style(Style::new().color(Color::parse("cyan").unwrap()))
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sample_console() {
        let console = sample_console();
        assert_eq!(console.width(), 80);
    }

    #[test]
    fn test_sample_styles() {
        let styles = sample_styles();
        assert!(!styles.is_empty());
        assert!(styles.iter().any(|(name, _)| *name == "bold"));
        assert!(styles.iter().any(|(name, _)| *name == "red"));
    }

    #[test]
    fn test_sample_texts() {
        let texts = sample_texts();
        assert!(!texts.is_empty());
        assert!(texts.iter().any(|(name, _)| *name == "plain"));
        assert!(texts.iter().any(|(name, _)| *name == "multiline"));
    }

    #[test]
    fn test_sample_table() {
        let table = sample_table();
        // Table should have 3 rows
        let segments = table.render(80);
        assert!(!segments.is_empty());
    }

    #[test]
    fn test_sample_tree() {
        let tree = sample_tree();
        let segments = tree.render();
        assert!(!segments.is_empty());
    }

    #[test]
    fn test_sample_panel() {
        let panel = sample_panel();
        let console = sample_console();
        let segments = panel.render(console.width());
        assert!(!segments.is_empty());
    }

    #[test]
    fn test_sample_rule() {
        let rule = sample_rule();
        let segments = rule.render(80);
        assert!(!segments.is_empty());
    }

    #[test]
    fn test_rainbow_text() {
        let text = rainbow_text();
        let plain = text.plain();
        assert!(plain.contains("RED"));
        assert!(plain.contains("BLUE"));
    }
}
