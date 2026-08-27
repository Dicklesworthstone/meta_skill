use rich_rust::renderables::table::{Column, Table};

#[test]
fn test_wide_char_in_narrow_column() {
    // Use ASCII borders to make width assertions straightforward.
    let mut table = Table::new().with_column(Column::new("A").width(1)).ascii();

    // CJK character '日' is 2 cells wide
    table.add_row_cells(["日"]);

    // Render with enough table width so the fixed column width constraint is the limiter.
    let segments = table.render(20);
    let output: String = segments.iter().map(|s| s.text.as_ref()).collect();

    let lines: Vec<&str> = output.lines().collect();

    use rich_rust::cells::cell_len;

    let header_cell_len = cell_len(lines[1]);
    let row_cell_len = cell_len(lines[3]);

    assert_eq!(
        header_cell_len, row_cell_len,
        "Table border misalignment detected! Header: {}, Row: {}",
        header_cell_len, row_cell_len
    );
}
