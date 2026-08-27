//! Phase 2 demo: Rule, Panel, and Table components.

use rich_rust::prelude::*;
// Box styles available: ROUNDED, DOUBLE, HEAVY, ASCII (used via Panel methods)

fn main() {
    let console = Console::new();
    let width = console.width().min(80);

    // ========================================================================
    // Rule Demo
    // ========================================================================
    println!("\n=== Rule Demo ===\n");

    // Simple rule
    let rule = Rule::new();
    for seg in rule.render(width) {
        print!("{}", seg.text);
    }

    // Rule with title
    let rule_titled =
        Rule::with_title("Section Title").style(Style::parse("cyan").unwrap_or_default());
    for seg in rule_titled.render(width) {
        print!("{}", seg.text);
    }

    // Rule with left-aligned title
    let rule_left = Rule::with_title("Left Aligned").align_left();
    for seg in rule_left.render(width) {
        print!("{}", seg.text);
    }

    // Heavy rule
    let heavy = rich_rust::renderables::rule::heavy_rule();
    for seg in heavy.render(width) {
        print!("{}", seg.text);
    }

    // ========================================================================
    // Panel Demo
    // ========================================================================
    println!("\n=== Panel Demo ===\n");

    // Simple panel
    let panel = Panel::from_text("Hello, Panel!")
        .title("Greeting")
        .width(40);

    for seg in panel.render(width) {
        print!("{}", seg.text);
    }

    println!();

    // Panel with subtitle
    let panel2 = Panel::from_text("This panel has both\na title and subtitle.")
        .title("Info")
        .subtitle("v1.0")
        .width(40);

    for seg in panel2.render(width) {
        print!("{}", seg.text);
    }

    println!();

    // Square panel
    let panel3 = Panel::from_text("Square corners")
        .square()
        .title("Square")
        .width(30);

    for seg in panel3.render(width) {
        print!("{}", seg.text);
    }

    println!();

    // ASCII panel for legacy terminals
    let panel4 = Panel::from_text("ASCII safe!")
        .ascii()
        .title("Legacy")
        .width(25);

    for seg in panel4.render(width) {
        print!("{}", seg.text);
    }

    // ========================================================================
    // Table Demo
    // ========================================================================
    println!("\n=== Table Demo ===\n");

    // Simple table
    let mut table = Table::new()
        .with_column(Column::new("Name"))
        .with_column(Column::new("Age").justify(JustifyMethod::Right))
        .with_column(Column::new("City"));

    table.add_row_cells(["Alice", "30", "New York"]);
    table.add_row_cells(["Bob", "25", "San Francisco"]);
    table.add_row_cells(["Charlie", "35", "Chicago"]);

    for seg in table.render(width) {
        print!("{}", seg.text);
    }

    println!();

    // Table with title and styled header
    let mut table2 = Table::new()
        .title("Employee Directory")
        .with_column(Column::new("ID").width(5))
        .with_column(Column::new("Employee Name").min_width(15))
        .with_column(Column::new("Department"))
        .with_column(Column::new("Salary").justify(JustifyMethod::Right));

    table2.add_row_cells(["001", "John Smith", "Engineering", "$85,000"]);
    table2.add_row_cells(["002", "Jane Doe", "Marketing", "$75,000"]);
    table2.add_row_cells(["003", "Bob Wilson", "Sales", "$70,000"]);

    for seg in table2.render(width) {
        print!("{}", seg.text);
    }

    println!();

    // ASCII table
    let mut table3 = Table::new()
        .ascii()
        .with_column(Column::new("Key"))
        .with_column(Column::new("Value"));

    table3.add_row_cells(["version", "1.0.0"]);
    table3.add_row_cells(["author", "Rich Rust"]);

    for seg in table3.render(width) {
        print!("{}", seg.text);
    }

    println!();

    // Minimal table (no header)
    let mut table4 = Table::new()
        .show_header(false)
        .with_column(Column::new("A"))
        .with_column(Column::new("B"));

    table4.add_row_cells(["Data 1", "Data 2"]);
    table4.add_row_cells(["Data 3", "Data 4"]);

    for seg in table4.render(width) {
        print!("{}", seg.text);
    }

    // ========================================================================
    // Combined Demo
    // ========================================================================
    println!("\n=== Combined Demo ===\n");

    // Use rule as section dividers
    let rule = Rule::with_title("Final Summary").style(Style::new().bold());
    for seg in rule.render(width) {
        print!("{}", seg.text);
    }

    console.print("[green]Phase 2 components demonstrated successfully![/]");
    println!();
}
