//! Table rendering conformance tests.

use super::TestCase;
use rich_rust::renderables::table::{Column, Table};
use rich_rust::segment::Segment;

/// Test case for basic table rendering.
#[derive(Debug)]
pub struct TableTest {
    pub name: &'static str,
    pub columns: Vec<&'static str>,
    pub rows: Vec<Vec<&'static str>>,
    pub width: usize,
    pub show_header: bool,
    pub show_lines: bool,
}

impl TestCase for TableTest {
    fn name(&self) -> &str {
        self.name
    }

    fn render(&self) -> Vec<Segment<'static>> {
        let mut table = Table::new()
            .show_header(self.show_header)
            .show_lines(self.show_lines);

        for col_name in &self.columns {
            table = table.with_column(Column::new(*col_name));
        }

        for row in &self.rows {
            let cells: Vec<&str> = row.to_vec();
            table.add_row_cells(cells);
        }

        table.render(self.width)
    }

    fn python_rich_code(&self) -> Option<String> {
        let cols: Vec<String> = self
            .columns
            .iter()
            .map(|c| format!("table.add_column(\"{}\")", c))
            .collect();
        let rows: Vec<String> = self
            .rows
            .iter()
            .map(|r| {
                let cells: Vec<String> = r.iter().map(|c| format!("\"{}\"", c)).collect();
                format!("table.add_row({})", cells.join(", "))
            })
            .collect();

        Some(format!(
            r#"from rich.console import Console
from rich.table import Table

console = Console(force_terminal=True, width={})
table = Table(show_header={}, show_lines={})
{}
{}
console.print(table, end="")"#,
            self.width,
            if self.show_header { "True" } else { "False" },
            if self.show_lines { "True" } else { "False" },
            cols.join("\n"),
            rows.join("\n")
        ))
    }
}

/// Standard table test cases for conformance testing.
pub fn standard_table_tests() -> Vec<Box<dyn TestCase>> {
    vec![
        Box::new(TableTest {
            name: "table_simple",
            columns: vec!["Name", "Age"],
            rows: vec![vec!["Alice", "30"], vec!["Bob", "25"]],
            width: 40,
            show_header: true,
            show_lines: false,
        }),
        Box::new(TableTest {
            name: "table_no_header",
            columns: vec!["Col1", "Col2"],
            rows: vec![vec!["A", "B"], vec!["C", "D"]],
            width: 40,
            show_header: false,
            show_lines: false,
        }),
        Box::new(TableTest {
            name: "table_with_lines",
            columns: vec!["X", "Y", "Z"],
            rows: vec![vec!["1", "2", "3"], vec!["4", "5", "6"]],
            width: 40,
            show_header: true,
            show_lines: true,
        }),
        Box::new(TableTest {
            name: "table_single_column",
            columns: vec!["Items"],
            rows: vec![vec!["One"], vec!["Two"], vec!["Three"]],
            width: 30,
            show_header: true,
            show_lines: false,
        }),
        Box::new(TableTest {
            name: "table_narrow",
            columns: vec!["A", "B"],
            rows: vec![vec!["Data", "Info"]],
            width: 20,
            show_header: true,
            show_lines: false,
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conformance::run_test;

    #[test]
    fn test_table_simple() {
        let test = TableTest {
            name: "table_simple",
            columns: vec!["Name", "Age"],
            rows: vec![vec!["Alice", "30"]],
            width: 40,
            show_header: true,
            show_lines: false,
        };
        let output = run_test(&test);
        assert!(output.contains("Alice"));
        assert!(output.contains("30"));
    }

    #[test]
    fn test_table_headers() {
        let test = TableTest {
            name: "table_with_headers",
            columns: vec!["Col1", "Col2"],
            rows: vec![vec!["A", "B"]],
            width: 40,
            show_header: true,
            show_lines: false,
        };
        let output = run_test(&test);
        assert!(output.contains("Col1"));
        assert!(output.contains("Col2"));
    }

    #[test]
    fn test_all_standard_table_tests() {
        for test in standard_table_tests() {
            let output = run_test(test.as_ref());
            assert!(
                !output.is_empty(),
                "Test '{}' produced empty output",
                test.name()
            );
        }
    }
}
