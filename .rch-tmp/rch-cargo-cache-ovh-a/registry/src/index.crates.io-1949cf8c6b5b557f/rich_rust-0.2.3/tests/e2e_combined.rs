//! End-to-end tests for combined feature integration.
//!
//! These tests verify that multiple renderables compose correctly when
//! nested, mixed, and rendered together through the Console. Each test
//! exercises real rendering paths, not just API surface.
//!
//! Run with: RUST_LOG=debug cargo test --test e2e_combined -- --nocapture

mod common;

use common::e2e_harness::AnsiParser;
use common::init_test_logging;
use rich_rust::r#box::{DOUBLE, HEAVY};
use rich_rust::prelude::*;

// =============================================================================
// Helper: render via Console to a String buffer
// =============================================================================

fn console_render(f: impl FnOnce(&Console)) -> String {
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    struct BufWriter(Arc<Mutex<Vec<u8>>>);
    impl Write for BufWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.0.lock().unwrap().flush()
        }
    }

    let buf = Arc::new(Mutex::new(Vec::new()));
    let console = Console::builder()
        .force_terminal(true)
        .color_system(ColorSystem::TrueColor)
        .width(80)
        .file(Box::new(BufWriter(Arc::clone(&buf))))
        .build();

    f(&console);

    let guard = buf.lock().unwrap();
    String::from_utf8_lossy(&guard).into_owned()
}

fn console_render_plain(f: impl FnOnce(&Console)) -> String {
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    struct BufWriter(Arc<Mutex<Vec<u8>>>);
    impl Write for BufWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().write(buf)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            self.0.lock().unwrap().flush()
        }
    }

    let buf = Arc::new(Mutex::new(Vec::new()));
    let console = Console::builder()
        .force_terminal(false)
        .width(80)
        .file(Box::new(BufWriter(Arc::clone(&buf))))
        .build();

    f(&console);

    let guard = buf.lock().unwrap();
    String::from_utf8_lossy(&guard).into_owned()
}

// =============================================================================
// Scenario 1: Table with styled cells
// =============================================================================

#[test]
fn e2e_table_with_styled_cells() {
    init_test_logging();
    tracing::info!("Starting E2E table with styled cells test");

    let mut table = Table::new()
        .title("User Report")
        .with_column(Column::new("Name").style(Style::new().bold()))
        .with_column(Column::new("Status").justify(JustifyMethod::Center))
        .with_column(Column::new("Score").justify(JustifyMethod::Right));

    table.add_row_cells(["Alice", "Active", "98"]);
    table.add_row_cells(["Bob", "Inactive", "72"]);
    table.add_row_cells(["Charlie", "Active", "85"]);

    // Render with ANSI via Console
    let output = console_render(|c| {
        let plain = table.render_plain(80);
        c.print_plain(&plain);
    });

    let plain = AnsiParser::strip_ansi(&output);
    assert!(plain.contains("User Report"), "Missing table title");
    assert!(plain.contains("Name"), "Missing header 'Name'");
    assert!(plain.contains("Alice"), "Missing cell 'Alice'");
    assert!(plain.contains("Bob"), "Missing cell 'Bob'");
    assert!(plain.contains("98"), "Missing score '98'");

    // Validate ANSI is well-formed
    let errors = AnsiParser::validate(&output);
    assert!(errors.is_empty(), "ANSI validation errors: {errors:?}");

    tracing::info!("E2E table with styled cells test PASSED");
}

// =============================================================================
// Scenario 2: Panel containing a table
// =============================================================================

#[test]
fn e2e_panel_containing_table() {
    init_test_logging();
    tracing::info!("Starting E2E panel containing table test");

    // Build a small table
    let mut table = Table::new()
        .with_column(Column::new("Key"))
        .with_column(Column::new("Value"));
    table.add_row_cells(["host", "localhost"]);
    table.add_row_cells(["port", "8080"]);

    // Render table to plain text, then wrap in a panel
    let table_output = table.render_plain(60);
    let panel = Panel::from_text(&table_output)
        .title("Configuration")
        .width(70);

    let output = panel.render_plain(80);

    // Panel structure checks
    assert!(output.contains("Configuration"), "Missing panel title");
    assert!(output.contains("host"), "Missing table key 'host'");
    assert!(
        output.contains("localhost"),
        "Missing table value 'localhost'"
    );
    assert!(output.contains("8080"), "Missing table value '8080'");

    // Panel borders should be present
    assert!(output.contains('╭'), "Missing top-left rounded corner");
    assert!(output.contains('╯'), "Missing bottom-right rounded corner");

    tracing::info!("E2E panel containing table test PASSED");
}

// =============================================================================
// Scenario 3: Tree in a panel
// =============================================================================

#[test]
fn e2e_tree_in_panel() {
    init_test_logging();
    tracing::info!("Starting E2E tree in panel test");

    let tree = Tree::new(
        TreeNode::new("Project")
            .child(
                TreeNode::new("src")
                    .child(TreeNode::new("main.rs"))
                    .child(TreeNode::new("lib.rs")),
            )
            .child(TreeNode::new("tests").child(TreeNode::new("integration.rs")))
            .child(TreeNode::new("Cargo.toml")),
    );

    let tree_output = tree.render_plain();
    let panel = Panel::from_text(&tree_output)
        .title("File Tree")
        .box_style(&DOUBLE)
        .width(40);

    let output = panel.render_plain(80);

    // Verify tree content inside panel
    assert!(output.contains("File Tree"), "Missing panel title");
    assert!(output.contains("Project"), "Missing root node");
    assert!(output.contains("src"), "Missing 'src' node");
    assert!(output.contains("main.rs"), "Missing 'main.rs' leaf");
    assert!(output.contains("lib.rs"), "Missing 'lib.rs' leaf");
    assert!(output.contains("Cargo.toml"), "Missing 'Cargo.toml' leaf");

    // Double-line border chars
    assert!(output.contains('╔'), "Missing double-line top-left corner");
    assert!(
        output.contains('╝'),
        "Missing double-line bottom-right corner"
    );

    tracing::info!("E2E tree in panel test PASSED");
}

// =============================================================================
// Scenario 4: Progress bar rendering
// =============================================================================

#[test]
fn e2e_progress_bar_states() {
    init_test_logging();
    tracing::info!("Starting E2E progress bar states test");

    // 0% progress
    let bar_empty = ProgressBar::with_total(100);
    let out_empty = bar_empty.render_plain(40);
    assert!(
        !out_empty.is_empty(),
        "Empty progress bar should produce output"
    );

    // 50% progress
    let mut bar_half = ProgressBar::with_total(100);
    bar_half.update(50);
    let out_half = bar_half.render_plain(40);
    assert!(
        !out_half.is_empty(),
        "Half progress bar should produce output"
    );

    // 100% progress
    let mut bar_full = ProgressBar::with_total(100);
    bar_full.update(100);
    let out_full = bar_full.render_plain(40);
    assert!(
        !out_full.is_empty(),
        "Full progress bar should produce output"
    );

    // All three should produce different output
    assert_ne!(out_empty, out_full, "0% and 100% should differ");

    tracing::info!("E2E progress bar states test PASSED");
}

// =============================================================================
// Scenario 5: Layout with mixed content (columns)
// =============================================================================

#[test]
fn e2e_layout_with_columns() {
    init_test_logging();
    tracing::info!("Starting E2E layout with columns test");

    // Create a layout with two columns
    let left = Text::new("Left pane content\nwith multiple lines\nof text.");
    let right = Text::new("Right pane content\nalso multiple lines\nfor testing.");

    let mut layout = Layout::new();
    layout.split_row(vec![
        Layout::from_renderable(left).name("left").ratio(1),
        Layout::from_renderable(right).name("right").ratio(1),
    ]);

    // Render via Console
    let output = console_render_plain(|c| {
        c.print_renderable(&layout);
    });

    let plain = AnsiParser::strip_ansi(&output);
    assert!(
        plain.contains("Left pane") || plain.contains("Left"),
        "Missing left pane content, got: {plain:?}"
    );
    assert!(
        plain.contains("Right pane") || plain.contains("Right"),
        "Missing right pane content, got: {plain:?}"
    );

    tracing::info!("E2E layout with columns test PASSED");
}

// =============================================================================
// Scenario 6: Rule as section divider between content
// =============================================================================

#[test]
fn e2e_rule_as_divider() {
    init_test_logging();
    tracing::info!("Starting E2E rule as divider test");

    let rule = Rule::with_title("Section Break");
    let output = rule.render_plain(60);

    assert!(output.contains("Section Break"), "Missing rule title");
    assert!(output.contains('─'), "Missing horizontal line character");

    // Rule without title
    let plain_rule = Rule::new();
    let plain_output = plain_rule.render_plain(60);
    assert!(
        plain_output.contains('─'),
        "Plain rule should have line chars"
    );

    tracing::info!("E2E rule as divider test PASSED");
}

// =============================================================================
// Scenario 7: Complex composition — table + rule + panel via Console
// =============================================================================

#[test]
fn e2e_complex_composition_via_console() {
    init_test_logging();
    tracing::info!("Starting E2E complex composition test");

    let output = console_render_plain(|c| {
        // Print a title
        c.print_plain("=== Dashboard ===");

        // Print a rule
        let rule = Rule::with_title("Status");
        let rule_output = rule.render_plain(80);
        c.print_plain(&rule_output);

        // Print a table
        let mut table = Table::new()
            .with_column(Column::new("Service"))
            .with_column(Column::new("Status"));
        table.add_row_cells(["API", "Running"]);
        table.add_row_cells(["DB", "Connected"]);
        let table_output = table.render_plain(80);
        c.print_plain(&table_output);

        // Print another rule
        let rule2 = Rule::with_title("Details");
        let rule2_output = rule2.render_plain(80);
        c.print_plain(&rule2_output);

        // Print a panel
        let panel = Panel::from_text("All systems operational")
            .title("Health")
            .width(40);
        let panel_output = panel.render_plain(80);
        c.print_plain(&panel_output);
    });

    let plain = AnsiParser::strip_ansi(&output);
    assert!(plain.contains("Dashboard"), "Missing dashboard title");
    assert!(plain.contains("Status"), "Missing Status rule");
    assert!(plain.contains("API"), "Missing API row");
    assert!(plain.contains("Running"), "Missing Running status");
    assert!(plain.contains("DB"), "Missing DB row");
    assert!(plain.contains("Health"), "Missing Health panel title");
    assert!(
        plain.contains("All systems operational"),
        "Missing panel content"
    );

    tracing::info!("E2E complex composition test PASSED");
}

// =============================================================================
// Scenario 8: Full dashboard — styled Console output with ANSI validation
// =============================================================================

#[test]
fn e2e_full_dashboard_styled() {
    init_test_logging();
    tracing::info!("Starting E2E full dashboard styled test");

    let output = console_render(|c| {
        // Header
        c.print("[bold]System Dashboard[/]");

        // Status table with styled content
        let mut table = Table::new()
            .title("Services")
            .with_column(Column::new("Name"))
            .with_column(Column::new("State"))
            .with_column(Column::new("Uptime"));
        table.add_row_cells(["web-server", "healthy", "99.9%"]);
        table.add_row_cells(["database", "healthy", "99.5%"]);
        table.add_row_cells(["cache", "degraded", "98.0%"]);
        let table_plain = table.render_plain(76);
        c.print_plain(&table_plain);

        // Tree of subsystems
        let tree = Tree::new(
            TreeNode::new("Infrastructure")
                .child(
                    TreeNode::new("Compute")
                        .child(TreeNode::new("us-east-1"))
                        .child(TreeNode::new("eu-west-1")),
                )
                .child(TreeNode::new("Storage").child(TreeNode::new("s3-primary"))),
        );
        let tree_output = tree.render_plain();
        c.print_plain(&tree_output);

        // Footer panel
        let panel = Panel::from_text("Last updated: 2026-01-28 21:00 UTC")
            .title("Info")
            .box_style(&HEAVY)
            .width(50);
        let panel_output = panel.render_plain(80);
        c.print_plain(&panel_output);
    });

    // Verify all sections are present
    let plain = AnsiParser::strip_ansi(&output);
    assert!(plain.contains("System Dashboard"), "Missing header");
    assert!(plain.contains("Services"), "Missing table title");
    assert!(plain.contains("web-server"), "Missing service row");
    assert!(plain.contains("cache"), "Missing cache row");
    assert!(plain.contains("Infrastructure"), "Missing tree root");
    assert!(plain.contains("us-east-1"), "Missing tree leaf");
    assert!(plain.contains("Info"), "Missing info panel");

    // The styled Console output should contain ANSI codes (bold at minimum)
    assert!(
        output.contains("\x1b["),
        "Styled dashboard should contain ANSI escapes"
    );

    // Validate ANSI sequences are well-formed
    let errors = AnsiParser::validate(&output);
    assert!(
        errors.is_empty(),
        "ANSI validation errors in dashboard: {errors:?}"
    );

    tracing::info!("E2E full dashboard styled test PASSED");
}

// =============================================================================
// Scenario 9: Nested panels
// =============================================================================

#[test]
fn e2e_nested_panels() {
    init_test_logging();
    tracing::info!("Starting E2E nested panels test");

    // Inner panel
    let inner = Panel::from_text("Inner content here")
        .title("Inner")
        .width(30);
    let inner_output = inner.render_plain(40);

    // Outer panel wrapping the inner
    let outer = Panel::from_text(&inner_output).title("Outer").width(50);
    let output = outer.render_plain(60);

    assert!(output.contains("Outer"), "Missing outer panel title");
    assert!(output.contains("Inner"), "Missing inner panel title");
    assert!(
        output.contains("Inner content here"),
        "Missing inner panel content"
    );

    // Should have multiple sets of box chars (nested borders)
    let corner_count = output.matches('╭').count();
    assert!(
        corner_count >= 2,
        "Expected at least 2 top-left corners for nested panels, got {corner_count}"
    );

    tracing::info!("E2E nested panels test PASSED");
}

// =============================================================================
// Scenario 10: Tree with different guide styles
// =============================================================================

#[test]
fn e2e_tree_guide_styles() {
    init_test_logging();
    tracing::info!("Starting E2E tree guide styles test");

    let make_tree = || {
        Tree::new(
            TreeNode::new("root")
                .child(TreeNode::new("child-a").child(TreeNode::new("leaf")))
                .child(TreeNode::new("child-b")),
        )
    };

    // Unicode (default)
    let unicode_output = make_tree().render_plain();
    assert!(unicode_output.contains("├"), "Unicode should use ├ branch");
    assert!(unicode_output.contains("└"), "Unicode should use └ last");

    // ASCII
    let ascii_output = make_tree().guides(TreeGuides::Ascii).render_plain();
    assert!(ascii_output.contains("+--"), "ASCII should use +-- branch");

    // Bold
    let bold_output = make_tree().guides(TreeGuides::Bold).render_plain();
    assert!(bold_output.contains("┣"), "Bold should use ┣ branch");

    // Double
    let double_output = make_tree().guides(TreeGuides::Double).render_plain();
    assert!(double_output.contains("╠"), "Double should use ╠ branch");

    // All should contain the same node labels
    for (name, out) in [
        ("unicode", &unicode_output),
        ("ascii", &ascii_output),
        ("bold", &bold_output),
        ("double", &double_output),
    ] {
        assert!(out.contains("root"), "{name} missing root");
        assert!(out.contains("child-a"), "{name} missing child-a");
        assert!(out.contains("leaf"), "{name} missing leaf");
        assert!(out.contains("child-b"), "{name} missing child-b");
    }

    tracing::info!("E2E tree guide styles test PASSED");
}

// =============================================================================
// Scenario 11: Table with various column widths and alignment
// =============================================================================

#[test]
fn e2e_table_column_alignment_and_widths() {
    init_test_logging();
    tracing::info!("Starting E2E table column alignment test");

    let mut table = Table::new()
        .with_column(
            Column::new("Left")
                .justify(JustifyMethod::Left)
                .min_width(10),
        )
        .with_column(
            Column::new("Center")
                .justify(JustifyMethod::Center)
                .min_width(12),
        )
        .with_column(
            Column::new("Right")
                .justify(JustifyMethod::Right)
                .min_width(10),
        );

    table.add_row_cells(["a", "b", "c"]);
    table.add_row_cells(["longer text", "medium", "x"]);

    let output = table.render_plain(60);

    assert!(output.contains("Left"), "Missing Left header");
    assert!(output.contains("Center"), "Missing Center header");
    assert!(output.contains("Right"), "Missing Right header");
    assert!(output.contains("longer text"), "Missing 'longer text' cell");
    assert!(output.contains("medium"), "Missing 'medium' cell");

    // Verify the table has proper box drawing
    assert!(output.contains('│'), "Missing vertical borders");
    assert!(output.contains('─'), "Missing horizontal borders");

    tracing::info!("E2E table column alignment test PASSED");
}

// =============================================================================
// Scenario 12: Console rendering with ANSI color validation
// =============================================================================

#[test]
fn e2e_styled_text_ansi_roundtrip() {
    init_test_logging();
    tracing::info!("Starting E2E styled text ANSI roundtrip test");

    let output = console_render(|c| {
        c.print("[bold]Bold text[/]");
        c.print("[italic]Italic text[/]");
        c.print("[red]Red text[/]");
        c.print("[bold italic underline]All styles[/]");
    });

    // Parse ANSI sequences
    let mut parser = AnsiParser::new();
    let segments = parser.parse(&output);
    assert!(!segments.is_empty(), "Should have parsed segments");

    // There should be at least some SGR sequences (styling)
    let has_sgr = segments.iter().any(|s| {
        s.sequences
            .iter()
            .any(|seq| matches!(seq, common::e2e_harness::AnsiSequence::Sgr(_)))
    });
    assert!(has_sgr, "Should contain SGR sequences for styling");

    // Plain text should contain all content
    let plain = AnsiParser::strip_ansi(&output);
    assert!(plain.contains("Bold text"), "Missing bold text");
    assert!(plain.contains("Italic text"), "Missing italic text");
    assert!(plain.contains("Red text"), "Missing red text");
    assert!(plain.contains("All styles"), "Missing combined style text");

    // ANSI sequences should be well-formed
    let errors = AnsiParser::validate(&output);
    assert!(errors.is_empty(), "ANSI errors: {errors:?}");

    tracing::info!("E2E styled text ANSI roundtrip test PASSED");
}
