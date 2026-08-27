use rich_rust::r#box::BoxChars;
use rich_rust::renderables::table::{Column, Table};

#[test]
fn test_table_uses_foot_row_separator() {
    // Custom box style with distinct FootRow
    static DISTINCT_BOX: BoxChars = BoxChars::new(
        ['+', '-', '+', '+'], // top
        ['|', ' ', '|', '|'], // head
        ['+', '=', '+', '+'], // head_row
        ['|', '-', '+', '|'], // mid
        ['|', '-', '+', '|'], // row (uses -)
        ['+', '*', '+', '+'], // foot_row (uses *)
        ['|', ' ', '|', '|'], // foot
        ['+', '-', '+', '+'], // bottom
        true,
    );

    let mut table = Table::new()
        .box_style(&DISTINCT_BOX)
        .with_column(Column::new("Head").footer("Foot"))
        .show_footer(true)
        .show_lines(true); // Enable lines to trigger the bug

    table.add_row_cells(["Body"]);

    let output = table.render_plain(20);
    let lines: Vec<&str> = output.lines().collect();

    // Expected structure:
    // 1. Top
    // 2. Header
    // 3. HeadRow (=)
    // 4. Body
    // 5. FootRow (*) <--- This is what we want to verify. Buggy version gives Row (-)
    // 6. Footer
    // 7. Bottom

    println!("Output:\n{}", output);

    assert_eq!(lines.len(), 7);

    let foot_sep = lines[4];
    assert!(
        foot_sep.contains('*'),
        "Expected FootRow separator to contain '*', got '{}'",
        foot_sep
    );
    assert!(
        !foot_sep.contains('-'),
        "FootRow separator should not contain '-' (Row style)"
    );
}
