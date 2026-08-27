//! Columns - Arrange items in multiple columns.
//!
//! This module provides a Columns renderable for arranging content
//! in a newspaper-style multi-column layout.
//!
//! # Example
//!
//! ```rust,ignore
//! use rich_rust::renderables::columns::Columns;
//! use rich_rust::segment::Segment;
//!
//! let items = vec![
//!     vec![Segment::new("Item 1", None)],
//!     vec![Segment::new("Item 2", None)],
//!     vec![Segment::new("Item 3", None)],
//! ];
//! let columns = Columns::new(items)
//!     .column_count(2)
//!     .gutter(2)
//!     .render(40);
//! ```

use crate::cells::cell_len;
use crate::console::{Console, ConsoleOptions};
use crate::renderables::Renderable;
use crate::segment::Segment;
use crate::style::Style;

use super::align::{Align, AlignMethod};

/// A renderable that arranges items in columns.
#[derive(Debug, Clone)]
pub struct Columns<'a> {
    /// Items to arrange (each item is a list of segments representing one line).
    items: Vec<Vec<Segment<'a>>>,
    /// Number of columns (None = auto-calculate based on content width).
    column_count: Option<usize>,
    /// Space between columns.
    gutter: usize,
    /// Whether to expand columns to fill available width.
    expand: bool,
    /// Whether columns should have equal width.
    equal_width: bool,
    /// Alignment within each column.
    align: AlignMethod,
    /// Padding around each item.
    padding: usize,
    /// Style for column separators (gutter).
    gutter_style: Style,
    /// Maximum total width for the columns layout.
    /// When set, prevents columns from spreading across very wide terminals.
    max_width: Option<usize>,
}

impl Default for Columns<'_> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            column_count: None,
            gutter: 2,
            expand: true,
            equal_width: false,
            align: AlignMethod::Left,
            padding: 0,
            gutter_style: Style::new(),
            max_width: None,
        }
    }
}

impl<'a> Columns<'a> {
    /// Create a new Columns layout with the given items.
    #[must_use]
    pub fn new(items: Vec<Vec<Segment<'a>>>) -> Self {
        Self {
            items,
            ..Default::default()
        }
    }

    /// Create columns from strings.
    #[must_use]
    pub fn from_strings(items: &[&'a str]) -> Self {
        let segments: Vec<Vec<Segment<'a>>> =
            items.iter().map(|s| vec![Segment::new(*s, None)]).collect();
        Self::new(segments)
    }

    /// Set the number of columns.
    #[must_use]
    pub fn column_count(mut self, count: usize) -> Self {
        self.column_count = Some(count.max(1));
        self
    }

    /// Set the gutter (space between columns).
    #[must_use]
    pub fn gutter(mut self, gutter: usize) -> Self {
        self.gutter = gutter;
        self
    }

    /// Set whether to expand columns to fill width.
    #[must_use]
    pub fn expand(mut self, expand: bool) -> Self {
        self.expand = expand;
        self
    }

    /// Set whether columns should have equal width.
    #[must_use]
    pub fn equal_width(mut self, equal: bool) -> Self {
        self.equal_width = equal;
        self
    }

    /// Set alignment within columns.
    #[must_use]
    pub fn align(mut self, align: AlignMethod) -> Self {
        self.align = align;
        self
    }

    /// Set padding around each item.
    #[must_use]
    pub fn padding(mut self, padding: usize) -> Self {
        self.padding = padding;
        self
    }

    /// Set the gutter style.
    #[must_use]
    pub fn gutter_style(mut self, style: Style) -> Self {
        self.gutter_style = style;
        self
    }

    /// Set a maximum width for the columns layout.
    ///
    /// When set, the columns will not expand beyond this width even if
    /// more terminal space is available. This prevents excessive whitespace
    /// on very wide terminals (e.g., 300+ columns).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let cols = Columns::from_strings(&["A", "B", "C"])
    ///     .max_width(120)  // Never wider than 120 columns
    ///     .expand(true);   // But expand up to that limit
    /// ```
    #[must_use]
    pub fn max_width(mut self, width: usize) -> Self {
        self.max_width = Some(width);
        self
    }

    /// Get the width of an item in cells.
    fn item_width(item: &[Segment<'_>]) -> usize {
        item.iter().map(|s| cell_len(&s.text)).sum()
    }

    /// Calculate column widths.
    fn calculate_column_widths(&self, total_width: usize, num_columns: usize) -> Vec<usize> {
        if num_columns == 0 || self.items.is_empty() {
            return vec![];
        }

        // Calculate gutter space needed
        let total_gutter = self.gutter * (num_columns - 1);
        let available_width = total_width.saturating_sub(total_gutter);

        if self.equal_width {
            // Equal width columns
            let column_width = available_width / num_columns;
            vec![column_width; num_columns]
        } else {
            // Calculate max width for each column based on content
            let mut max_widths = vec![0usize; num_columns];

            for (idx, item) in self.items.iter().enumerate() {
                let col = idx % num_columns;
                let item_w = Self::item_width(item) + self.padding * 2;
                max_widths[col] = max_widths[col].max(item_w);
            }

            if self.expand {
                // Distribute remaining space proportionally
                let content_total: usize = max_widths.iter().sum();
                if content_total < available_width {
                    let extra = available_width - content_total;
                    let per_column = extra / num_columns;
                    let remainder = extra % num_columns;

                    for (i, width) in max_widths.iter_mut().enumerate() {
                        *width += per_column;
                        if i < remainder {
                            *width += 1;
                        }
                    }
                }
            }

            // Ensure widths fit within the available space.
            let total: usize = max_widths.iter().sum();
            if total > available_width {
                max_widths = self.collapse_widths(&max_widths, available_width);
            }

            max_widths
        }
    }

    /// Collapse column widths to fit the available width.
    fn collapse_widths(&self, widths: &[usize], available_width: usize) -> Vec<usize> {
        let total: usize = widths.iter().sum();
        if total <= available_width {
            return widths.to_vec();
        }

        let mut result = widths.to_vec();
        let excess = total - available_width;
        let minimums = vec![0usize; widths.len()];
        let shrinkable: Vec<usize> = result
            .iter()
            .zip(minimums.iter())
            .map(|(w, m)| w.saturating_sub(*m))
            .collect();
        let total_shrinkable: usize = shrinkable.iter().sum();
        if total_shrinkable == 0 {
            return result;
        }

        for (i, shrink) in shrinkable.iter().enumerate() {
            if *shrink > 0 {
                let reduction = *shrink * excess / total_shrinkable;
                result[i] = result[i].saturating_sub(reduction);
            }
        }

        let new_total: usize = result.iter().sum();
        if new_total > available_width {
            let mut diff = new_total - available_width;
            for i in (0..result.len()).rev() {
                if diff == 0 {
                    break;
                }
                if result[i] > minimums[i] {
                    let can_remove = (result[i] - minimums[i]).min(diff);
                    result[i] -= can_remove;
                    diff -= can_remove;
                }
            }
        }

        result
    }

    /// Auto-calculate number of columns based on content and width.
    fn auto_column_count(&self, total_width: usize) -> usize {
        if self.items.is_empty() {
            return 1;
        }

        // Find the widest item
        let max_item_width = self
            .items
            .iter()
            .map(|item| Self::item_width(item) + self.padding * 2)
            .max()
            .unwrap_or(1);

        // Calculate how many columns can fit
        let min_column_width = max_item_width.max(1);
        let mut columns = 1;

        while columns < self.items.len() {
            let next = columns + 1;
            let needed_width = next * min_column_width + (next - 1) * self.gutter;
            if needed_width > total_width {
                break;
            }
            columns = next;
        }

        columns
    }

    /// Render the columns to lines of segments.
    #[must_use]
    pub fn render(&self, total_width: usize) -> Vec<Vec<Segment<'a>>> {
        if self.items.is_empty() {
            return vec![];
        }

        // Apply max_width constraint to prevent excessive spreading on wide terminals
        let effective_width = match self.max_width {
            Some(max) => total_width.min(max),
            None => total_width,
        };

        let num_columns = self
            .column_count
            .unwrap_or_else(|| self.auto_column_count(effective_width));
        let column_widths = self.calculate_column_widths(effective_width, num_columns);

        if column_widths.is_empty() {
            return vec![];
        }

        // Calculate number of rows needed
        let num_rows = self.items.len().div_ceil(num_columns);

        let mut result = Vec::with_capacity(num_rows);

        for row_idx in 0..num_rows {
            let mut row_segments = Vec::new();

            #[expect(
                clippy::needless_range_loop,
                reason = "col_idx used for multiple purposes"
            )]
            for col_idx in 0..num_columns {
                let item_idx = row_idx * num_columns + col_idx;
                let column_width = column_widths[col_idx];

                // Add gutter before columns (except first)
                if col_idx > 0 && self.gutter > 0 {
                    row_segments.push(Segment::new(
                        " ".repeat(self.gutter),
                        Some(self.gutter_style.clone()),
                    ));
                }

                if item_idx < self.items.len() {
                    // Add padding, content, padding
                    let effective_padding = self.padding.min(column_width / 2);
                    if effective_padding > 0 {
                        row_segments.push(Segment::new(" ".repeat(effective_padding), None));
                    }

                    let content_width = column_width.saturating_sub(effective_padding * 2);
                    let mut content = self.items[item_idx].clone();

                    // Sanitize content to prevent layout breakage
                    for seg in &mut content {
                        if seg.text.contains('\n') {
                            seg.text = std::borrow::Cow::Owned(seg.text.replace('\n', " "));
                        }
                    }

                    content =
                        crate::segment::adjust_line_length(content, content_width, None, false);
                    let aligned = Align::new(content, content_width)
                        .method(self.align)
                        .render();
                    row_segments.extend(aligned);

                    if effective_padding > 0 {
                        row_segments.push(Segment::new(" ".repeat(effective_padding), None));
                    }
                } else {
                    // Empty cell - fill with spaces
                    row_segments.push(Segment::new(" ".repeat(column_width), None));
                }
            }

            // Keep each row within the requested width, even when explicit
            // gutters are wider than the available width budget.
            result.push(crate::segment::adjust_line_length(
                row_segments,
                effective_width,
                None,
                false,
            ));
        }

        result
    }

    /// Render to a single flat list of segments with newlines.
    #[must_use]
    pub fn render_flat(&self, total_width: usize) -> Vec<Segment<'a>> {
        let lines = self.render(total_width);
        let mut result = Vec::new();

        for (i, line) in lines.into_iter().enumerate() {
            if i > 0 {
                result.push(Segment::new("\n", None));
            }
            result.extend(line);
        }

        result
    }
}

impl Renderable for Columns<'_> {
    fn render<'b>(&'b self, _console: &Console, options: &ConsoleOptions) -> Vec<Segment<'b>> {
        self.render_flat(options.max_width).into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_columns_new() {
        let items = vec![vec![Segment::new("A", None)], vec![Segment::new("B", None)]];
        let cols = Columns::new(items);
        assert_eq!(cols.items.len(), 2);
    }

    #[test]
    fn test_columns_from_strings() {
        let cols = Columns::from_strings(&["A", "B", "C"]);
        assert_eq!(cols.items.len(), 3);
    }

    #[test]
    fn test_columns_builder() {
        let cols = Columns::from_strings(&["A", "B"])
            .column_count(2)
            .gutter(4)
            .expand(false)
            .equal_width(true)
            .align(AlignMethod::Center)
            .padding(1);

        assert_eq!(cols.column_count, Some(2));
        assert_eq!(cols.gutter, 4);
        assert!(!cols.expand);
        assert!(cols.equal_width);
        assert_eq!(cols.align, AlignMethod::Center);
        assert_eq!(cols.padding, 1);
    }

    #[test]
    fn test_columns_render_two_columns() {
        let cols = Columns::from_strings(&["A", "B", "C", "D"])
            .column_count(2)
            .gutter(2)
            .expand(false);

        let lines = cols.render(20);

        // Should have 2 rows (4 items / 2 columns)
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn test_columns_render_three_columns() {
        let cols = Columns::from_strings(&["A", "B", "C"])
            .column_count(3)
            .gutter(1);

        let lines = cols.render(30);

        // Should have 1 row (3 items / 3 columns)
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_columns_render_empty() {
        let cols = Columns::new(vec![]);
        let lines = cols.render(40);
        assert_eq!(lines.len(), 0);
    }

    #[test]
    fn test_columns_auto_count() {
        // With narrow width, should fit fewer columns
        let cols = Columns::from_strings(&["Hello", "World", "Test", "Here"]);

        // Auto-calc should determine column count based on content width
        let auto_count = cols.auto_column_count(50);
        assert!(auto_count >= 1);
    }

    #[test]
    fn test_columns_equal_width() {
        let cols = Columns::from_strings(&["Short", "Much Longer Item"])
            .column_count(2)
            .equal_width(true);

        let widths = cols.calculate_column_widths(40, 2);

        // Both columns should be same width
        assert_eq!(widths[0], widths[1]);
    }

    #[test]
    fn test_columns_with_gutter() {
        let cols = Columns::from_strings(&["A", "B"]).column_count(2).gutter(4);

        let lines = cols.render(20);
        let line = &lines[0];

        // Check that gutter is present (spaces between columns)
        let text: String = line.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("    ")); // 4 spaces for gutter
    }

    #[test]
    fn test_columns_alignment() {
        let cols = Columns::from_strings(&["Hi"])
            .column_count(1)
            .expand(true)
            .equal_width(true)
            .align(AlignMethod::Center);

        let lines = cols.render(20);
        let text: String = lines[0].iter().map(|s| s.text.as_ref()).collect();

        // Content should be centered
        assert!(text.starts_with(' ')); // Has leading spaces
        assert!(text.ends_with(' ')); // Has trailing spaces
    }

    #[test]
    fn test_columns_render_flat() {
        let cols = Columns::from_strings(&["A", "B", "C", "D"]).column_count(2);

        let segments = cols.render_flat(20);

        // Should contain a newline between rows
        let has_newline = segments.iter().any(|s| s.text.contains('\n'));
        assert!(has_newline);
    }

    #[test]
    fn test_columns_padding_does_not_overflow_width() {
        let cols = Columns::from_strings(&["A", "B"])
            .column_count(2)
            .gutter(1)
            .padding(2);

        let total_width = 4;
        let lines = cols.render(total_width);

        for line in lines {
            let width: usize = line.iter().map(Segment::cell_length).sum();
            assert!(
                width <= total_width,
                "line width {width} exceeds total_width {total_width}"
            );
        }
    }

    #[test]
    fn test_columns_uneven_items() {
        // 5 items in 2 columns = 3 rows (last row has 1 item + 1 empty)
        let cols = Columns::from_strings(&["1", "2", "3", "4", "5"]).column_count(2);

        let lines = cols.render(20);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_item_width() {
        let item = vec![
            Segment::new("Hello", None),
            Segment::new(" ", None),
            Segment::new("World", None),
        ];
        assert_eq!(Columns::item_width(&item), 11);
    }

    #[test]
    fn test_columns_single_column() {
        let cols = Columns::from_strings(&["A", "B", "C"]).column_count(1);

        let lines = cols.render(20);

        // Should have 3 rows (1 item per row)
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_columns_narrow_width() {
        // Width too small for content
        let cols = Columns::from_strings(&["Hello", "World"])
            .column_count(2)
            .gutter(2);

        // Even with narrow width, should not panic
        let total_width = 5;
        let lines = cols.render(total_width);
        assert_ne!(lines.len(), 0);
        for line in lines {
            let width: usize = line.iter().map(Segment::cell_length).sum();
            assert!(width <= total_width);
        }
    }

    #[test]
    fn test_columns_tiny_width_with_gutter_does_not_overflow() {
        let cols = Columns::from_strings(&["A", "B"])
            .column_count(2)
            .gutter(3)
            .equal_width(true)
            .expand(false);

        let total_width = 1;
        let lines = cols.render(total_width);

        assert_eq!(lines.len(), 1);
        for line in lines {
            let width: usize = line.iter().map(Segment::cell_length).sum();
            assert!(
                width <= total_width,
                "line width {width} exceeds total_width {total_width}"
            );
        }
    }

    #[test]
    fn test_columns_tiny_width_with_many_columns_does_not_overflow() {
        let cols = Columns::from_strings(&["A", "B", "C"])
            .column_count(3)
            .gutter(2)
            .equal_width(true);

        let total_width = 2;
        let lines = cols.render(total_width);

        assert_eq!(lines.len(), 1);
        for line in lines {
            let width: usize = line.iter().map(Segment::cell_length).sum();
            assert!(
                width <= total_width,
                "line width {width} exceeds total_width {total_width}"
            );
        }
    }

    #[test]
    fn test_columns_zero_width() {
        let cols = Columns::from_strings(&["A", "B"]);
        let lines = cols.render(0);
        // Zero width should still produce the correct number of rows,
        // but each line must be zero-width to avoid overflow.
        assert_eq!(lines.len(), 2);
        for line in &lines {
            let width: usize = line.iter().map(Segment::cell_length).sum();
            assert_eq!(width, 0);
        }
    }

    #[test]
    fn test_columns_many_items() {
        // Test with many items to verify row calculation
        let items: Vec<&str> = (0..20).map(|_| "X").collect();
        let cols = Columns::from_strings(&items).column_count(4);

        let lines = cols.render(40);

        // 20 items / 4 columns = 5 rows
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn test_columns_wide_unicode() {
        // Test with CJK characters (2 cells wide each)
        let items = vec![
            vec![Segment::new("你好", None)], // 4 cells
            vec![Segment::new("世界", None)], // 4 cells
        ];
        let cols = Columns::new(items).column_count(2).gutter(2);

        let lines = cols.render(20);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_columns_content_width_calculation() {
        // Verify content-based width calculation
        let cols = Columns::from_strings(&["Short", "Much Longer Item"])
            .column_count(2)
            .equal_width(false)
            .expand(false);

        let widths = cols.calculate_column_widths(40, 2);

        // Without equal_width, columns can have different widths based on content
        // Both should be within bounds
        assert_eq!(widths.len(), 2);
    }

    #[test]
    fn test_columns_expand_distribution() {
        let cols = Columns::from_strings(&["A", "B"])
            .column_count(2)
            .gutter(2)
            .expand(true);

        let widths = cols.calculate_column_widths(20, 2);

        // With gutter=2, available = 18, so columns should fill that
        let total: usize = widths.iter().sum();
        assert!(total > 0);
    }

    #[test]
    fn test_columns_right_align() {
        let cols = Columns::from_strings(&["X"])
            .column_count(1)
            .expand(true)
            .equal_width(true)
            .align(AlignMethod::Right);

        let lines = cols.render(20);
        let text: String = lines[0].iter().map(|s| s.text.as_ref()).collect();

        // Content should be right-aligned (leading spaces)
        assert!(text.starts_with(' '));
    }

    #[test]
    fn test_columns_padding_applied() {
        let cols = Columns::from_strings(&["X"]).column_count(1).padding(2);

        let lines = cols.render(20);
        let text: String = lines[0].iter().map(|s| s.text.as_ref()).collect();

        // Should have padding around content
        assert!(text.starts_with("  ")); // 2 spaces padding
    }

    #[test]
    fn test_columns_max_width_limits_expansion() {
        // Without max_width, columns expand to fill 400 columns
        let cols = Columns::from_strings(&["A", "B", "C"])
            .column_count(3)
            .gutter(2)
            .expand(true)
            .max_width(60);

        let lines = cols.render(400); // Simulating very wide terminal
        let text: String = lines[0].iter().map(|s| s.text.as_ref()).collect();

        // Width should be capped at 60, not 400
        assert!(
            text.len() <= 60,
            "Output width {} exceeds max_width 60",
            text.len()
        );
    }

    #[test]
    fn test_columns_max_width_no_effect_on_narrow_terminal() {
        // max_width should not affect narrow terminals
        let cols = Columns::from_strings(&["A", "B"])
            .column_count(2)
            .gutter(2)
            .expand(true)
            .max_width(100);

        let lines = cols.render(40); // Narrow terminal
        let text: String = lines[0].iter().map(|s| s.text.as_ref()).collect();

        // Should use actual terminal width (40), not max_width (100)
        assert!(
            text.len() <= 40,
            "Output width {} exceeds terminal width 40",
            text.len()
        );
    }

    #[test]
    fn test_columns_at_400_width_without_max_causes_spread() {
        // This test demonstrates the bug: without max_width, columns spread too wide
        let cols = Columns::from_strings(&["A", "B", "C"])
            .column_count(3)
            .gutter(4)
            .expand(true);

        let lines = cols.render(400);
        let text: String = lines[0].iter().map(|s| s.text.as_ref()).collect();

        // Without max_width, output fills the 400 column width
        // This contains excessive whitespace (30+ consecutive spaces)
        let has_excessive_whitespace = text.contains(&" ".repeat(30));
        assert!(
            has_excessive_whitespace,
            "Expected excessive whitespace without max_width constraint"
        );
    }

    #[test]
    fn test_columns_at_400_width_with_max_prevents_spread() {
        // With max_width, excessive spreading is prevented
        let cols = Columns::from_strings(&["A", "B", "C"])
            .column_count(3)
            .gutter(4)
            .expand(true)
            .max_width(60);

        let lines = cols.render(400);
        let text: String = lines[0].iter().map(|s| s.text.as_ref()).collect();

        // With max_width=60, no runs of 30+ consecutive spaces
        let has_excessive_whitespace = text.contains(&" ".repeat(30));
        assert!(
            !has_excessive_whitespace,
            "max_width should prevent excessive whitespace runs"
        );
    }
}
