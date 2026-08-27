//! Box drawing characters for tables and panels.
//!
//! This module provides box drawing character sets for creating
//! bordered tables and panels in the terminal.

use std::fmt;

/// Row level for box drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowLevel {
    /// Top of the box.
    Top,
    /// Header row separator.
    HeadRow,
    /// Middle row separator.
    Mid,
    /// Regular row separator.
    Row,
    /// Footer row separator.
    FootRow,
    /// Bottom of the box.
    Bottom,
}

/// Box drawing character set.
///
/// Each row is 4 characters: [left, middle, cross/divider, right]
/// - left: leftmost edge character
/// - middle: horizontal line character
/// - cross: intersection or divider character
/// - right: rightmost edge character
#[derive(Debug, Clone)]
pub struct BoxChars {
    /// Top row: ┌─┬┐
    pub top: [char; 4],
    /// Head row (cell content): │ ││
    pub head: [char; 4],
    /// Head separator: ├─┼┤
    pub head_row: [char; 4],
    /// Mid separator: ├─┼┤
    pub mid: [char; 4],
    /// Row separator: ├─┼┤
    pub row: [char; 4],
    /// Foot separator: ├─┼┤
    pub foot_row: [char; 4],
    /// Foot row (cell content): │ ││
    pub foot: [char; 4],
    /// Bottom row: └─┴┘
    pub bottom: [char; 4],
    /// Whether this box uses ASCII-only characters.
    pub ascii: bool,
}

impl BoxChars {
    /// Create a new box from character arrays.
    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "struct constructor needs all fields"
    )]
    pub const fn new(
        top: [char; 4],
        head: [char; 4],
        head_row: [char; 4],
        mid: [char; 4],
        row: [char; 4],
        foot_row: [char; 4],
        foot: [char; 4],
        bottom: [char; 4],
        ascii: bool,
    ) -> Self {
        Self {
            top,
            head,
            head_row,
            mid,
            row,
            foot_row,
            foot,
            bottom,
            ascii,
        }
    }

    /// Get the row characters for a specific level.
    #[must_use]
    pub fn get_row_chars(&self, level: RowLevel) -> &[char; 4] {
        match level {
            RowLevel::Top => &self.top,
            RowLevel::HeadRow => &self.head_row,
            RowLevel::Mid => &self.mid,
            RowLevel::Row => &self.row,
            RowLevel::FootRow => &self.foot_row,
            RowLevel::Bottom => &self.bottom,
        }
    }

    /// Build a row string for the given column widths.
    #[must_use]
    pub fn build_row(&self, widths: &[usize], level: RowLevel, edge: bool) -> String {
        let chars = self.get_row_chars(level);
        let left = chars[0];
        let middle = chars[1];
        let cross = chars[2];
        let right = chars[3];

        let mut result = String::new();

        if edge && left != ' ' {
            result.push(left);
        }

        for (i, &width) in widths.iter().enumerate() {
            // Add horizontal line for this column
            for _ in 0..width {
                result.push(middle);
            }

            // Add cross or right edge
            if i < widths.len() - 1 {
                result.push(cross);
            } else if edge && right != ' ' {
                result.push(right);
            }
        }

        result
    }

    /// Build the top border.
    #[must_use]
    pub fn get_top(&self, widths: &[usize]) -> String {
        self.build_row(widths, RowLevel::Top, true)
    }

    /// Build the bottom border.
    #[must_use]
    pub fn get_bottom(&self, widths: &[usize]) -> String {
        self.build_row(widths, RowLevel::Bottom, true)
    }

    /// Build the header separator.
    #[must_use]
    pub fn get_head_row(&self, widths: &[usize]) -> String {
        self.build_row(widths, RowLevel::HeadRow, true)
    }

    /// Build a mid-table separator.
    #[must_use]
    pub fn get_mid(&self, widths: &[usize]) -> String {
        self.build_row(widths, RowLevel::Mid, true)
    }

    /// Build a regular row separator.
    #[must_use]
    pub fn get_row(&self, widths: &[usize]) -> String {
        self.build_row(widths, RowLevel::Row, true)
    }

    /// Get the cell left edge character.
    #[must_use]
    pub fn cell_left(&self) -> char {
        self.head[0]
    }

    /// Get the cell divider character.
    #[must_use]
    pub fn cell_divider(&self) -> char {
        self.head[2]
    }

    /// Get the cell right edge character.
    #[must_use]
    pub fn cell_right(&self) -> char {
        self.head[3]
    }

    /// Substitute characters for ASCII-safe rendering.
    #[must_use]
    pub fn substitute(&self, safe: bool) -> &Self {
        if safe && !self.ascii { &ASCII } else { self }
    }
}

impl fmt::Display for BoxChars {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Display a sample 3x3 box
        let widths = [3, 3, 3];
        writeln!(f, "{}", self.get_top(&widths))?;
        writeln!(
            f,
            "{}   {}   {}   {}",
            self.head[0], self.head[2], self.head[2], self.head[3]
        )?;
        writeln!(f, "{}", self.get_head_row(&widths))?;
        writeln!(
            f,
            "{}   {}   {}   {}",
            self.head[0], self.head[2], self.head[2], self.head[3]
        )?;
        write!(f, "{}", self.get_bottom(&widths))
    }
}

// ============================================================================
// Built-in Box Styles
// ============================================================================

/// ASCII box (safe for all terminals).
pub const ASCII: BoxChars = BoxChars::new(
    ['+', '-', '+', '+'],
    ['|', ' ', '|', '|'],
    ['|', '-', '+', '|'],
    ['|', '-', '+', '|'],
    ['|', '-', '+', '|'],
    ['|', '-', '+', '|'],
    ['|', ' ', '|', '|'],
    ['+', '-', '+', '+'],
    true,
);

/// ASCII2 box with double lines at intersections.
pub const ASCII2: BoxChars = BoxChars::new(
    ['+', '-', '+', '+'],
    ['|', ' ', '|', '|'],
    ['+', '-', '+', '+'],
    ['+', '-', '+', '+'],
    ['+', '-', '+', '+'],
    ['+', '-', '+', '+'],
    ['|', ' ', '|', '|'],
    ['+', '-', '+', '+'],
    true,
);

/// ASCII with double header.
pub const ASCII_DOUBLE_HEAD: BoxChars = BoxChars::new(
    ['+', '-', '+', '+'],
    ['|', ' ', '|', '|'],
    ['+', '=', '+', '+'],
    ['|', '-', '+', '|'],
    ['|', '-', '+', '|'],
    ['|', '-', '+', '|'],
    ['|', ' ', '|', '|'],
    ['+', '-', '+', '+'],
    true,
);

/// Unicode rounded box.
pub const ROUNDED: BoxChars = BoxChars::new(
    ['\u{256D}', '\u{2500}', '\u{252C}', '\u{256E}'], // ╭─┬╮
    ['\u{2502}', ' ', '\u{2502}', '\u{2502}'],        // │ ││
    ['\u{251C}', '\u{2500}', '\u{253C}', '\u{2524}'], // ├─┼┤
    ['\u{251C}', '\u{2500}', '\u{253C}', '\u{2524}'], // ├─┼┤
    ['\u{251C}', '\u{2500}', '\u{253C}', '\u{2524}'], // ├─┼┤
    ['\u{251C}', '\u{2500}', '\u{253C}', '\u{2524}'], // ├─┼┤
    ['\u{2502}', ' ', '\u{2502}', '\u{2502}'],        // │ ││
    ['\u{2570}', '\u{2500}', '\u{2534}', '\u{256F}'], // ╰─┴╯
    false,
);

/// Unicode square/single line box.
pub const SQUARE: BoxChars = BoxChars::new(
    ['\u{250C}', '\u{2500}', '\u{252C}', '\u{2510}'], // ┌─┬┐
    ['\u{2502}', ' ', '\u{2502}', '\u{2502}'],        // │ ││
    ['\u{251C}', '\u{2500}', '\u{253C}', '\u{2524}'], // ├─┼┤
    ['\u{251C}', '\u{2500}', '\u{253C}', '\u{2524}'], // ├─┼┤
    ['\u{251C}', '\u{2500}', '\u{253C}', '\u{2524}'], // ├─┼┤
    ['\u{251C}', '\u{2500}', '\u{253C}', '\u{2524}'], // ├─┼┤
    ['\u{2502}', ' ', '\u{2502}', '\u{2502}'],        // │ ││
    ['\u{2514}', '\u{2500}', '\u{2534}', '\u{2518}'], // └─┴┘
    false,
);

/// Unicode double line box.
pub const DOUBLE: BoxChars = BoxChars::new(
    ['\u{2554}', '\u{2550}', '\u{2566}', '\u{2557}'], // ╔═╦╗
    ['\u{2551}', ' ', '\u{2551}', '\u{2551}'],        // ║ ║║
    ['\u{2560}', '\u{2550}', '\u{256C}', '\u{2563}'], // ╠═╬╣
    ['\u{2560}', '\u{2550}', '\u{256C}', '\u{2563}'], // ╠═╬╣
    ['\u{2560}', '\u{2550}', '\u{256C}', '\u{2563}'], // ╠═╬╣
    ['\u{2560}', '\u{2550}', '\u{256C}', '\u{2563}'], // ╠═╬╣
    ['\u{2551}', ' ', '\u{2551}', '\u{2551}'],        // ║ ║║
    ['\u{255A}', '\u{2550}', '\u{2569}', '\u{255D}'], // ╚═╩╝
    false,
);

/// Heavy (thick) line box.
pub const HEAVY: BoxChars = BoxChars::new(
    ['\u{250F}', '\u{2501}', '\u{2533}', '\u{2513}'], // ┏━┳┓
    ['\u{2503}', ' ', '\u{2503}', '\u{2503}'],        // ┃ ┃┃
    ['\u{2523}', '\u{2501}', '\u{254B}', '\u{252B}'], // ┣━╋┫
    ['\u{2523}', '\u{2501}', '\u{254B}', '\u{252B}'], // ┣━╋┫
    ['\u{2523}', '\u{2501}', '\u{254B}', '\u{252B}'], // ┣━╋┫
    ['\u{2523}', '\u{2501}', '\u{254B}', '\u{252B}'], // ┣━╋┫
    ['\u{2503}', ' ', '\u{2503}', '\u{2503}'],        // ┃ ┃┃
    ['\u{2517}', '\u{2501}', '\u{253B}', '\u{251B}'], // ┗━┻┛
    false,
);

/// Heavy head with single body.
pub const HEAVY_HEAD: BoxChars = BoxChars::new(
    ['\u{250F}', '\u{2501}', '\u{2533}', '\u{2513}'], // ┏━┳┓
    ['\u{2503}', ' ', '\u{2503}', '\u{2503}'],        // ┃ ┃┃
    ['\u{2521}', '\u{2501}', '\u{2547}', '\u{2529}'], // ┡━╇┩
    ['\u{251C}', '\u{2500}', '\u{253C}', '\u{2524}'], // ├─┼┤
    ['\u{251C}', '\u{2500}', '\u{253C}', '\u{2524}'], // ├─┼┤
    ['\u{251C}', '\u{2500}', '\u{253C}', '\u{2524}'], // ├─┼┤
    ['\u{2502}', ' ', '\u{2502}', '\u{2502}'],        // │ ││
    ['\u{2514}', '\u{2500}', '\u{2534}', '\u{2518}'], // └─┴┘
    false,
);

/// Minimal (no outer border).
pub const MINIMAL: BoxChars = BoxChars::new(
    [' ', ' ', ' ', ' '],
    [' ', ' ', '\u{2502}', ' '],        //   │
    [' ', '\u{2500}', '\u{253C}', ' '], //  ─┼
    [' ', ' ', ' ', ' '],
    [' ', ' ', ' ', ' '],
    [' ', ' ', ' ', ' '],
    [' ', ' ', '\u{2502}', ' '], //   │
    [' ', ' ', ' ', ' '],
    false,
);

/// Simple (just horizontal lines).
pub const SIMPLE: BoxChars = BoxChars::new(
    [' ', ' ', ' ', ' '],
    [' ', ' ', ' ', ' '],
    [' ', '\u{2500}', '\u{2500}', ' '], //  ──
    [' ', ' ', ' ', ' '],
    [' ', ' ', ' ', ' '],
    [' ', '\u{2500}', '\u{2500}', ' '], //  ──
    [' ', ' ', ' ', ' '],
    [' ', ' ', ' ', ' '],
    false,
);

/// Simple heavy (just thick horizontal lines).
pub const SIMPLE_HEAVY: BoxChars = BoxChars::new(
    [' ', ' ', ' ', ' '],
    [' ', ' ', ' ', ' '],
    [' ', '\u{2501}', '\u{2501}', ' '], //  ━━
    [' ', ' ', ' ', ' '],
    [' ', ' ', ' ', ' '],
    [' ', '\u{2501}', '\u{2501}', ' '], //  ━━
    [' ', ' ', ' ', ' '],
    [' ', ' ', ' ', ' '],
    false,
);

/// Horizontals only - no vertical dividers between columns.
///
/// Creates a clean look with just horizontal lines at header/footer.
pub const HORIZONTALS: BoxChars = BoxChars::new(
    [' ', '\u{2500}', '\u{2500}', ' '], //  ──
    [' ', ' ', ' ', ' '],               //    (no dividers)
    [' ', '\u{2500}', '\u{2500}', ' '], //  ──
    [' ', ' ', ' ', ' '],
    [' ', ' ', ' ', ' '],
    [' ', '\u{2500}', '\u{2500}', ' '], //  ──
    [' ', ' ', ' ', ' '],
    [' ', '\u{2500}', '\u{2500}', ' '], //  ──
    false,
);

/// Markdown-compatible table format.
///
/// Uses pipes and dashes that render correctly in Markdown viewers.
pub const MARKDOWN: BoxChars = BoxChars::new(
    [' ', ' ', ' ', ' '], // No top border
    ['|', ' ', '|', '|'], // | | |
    ['|', '-', '|', '|'], // |-|-|
    ['|', '-', '|', '|'], // |-|-|
    ['|', ' ', '|', '|'], // | | | (no separator for body rows)
    ['|', '-', '|', '|'], // |-|-|
    ['|', ' ', '|', '|'], // | | |
    [' ', ' ', ' ', ' '], // No bottom border
    true,                 // ASCII-safe
);

/// Minimal with heavy (thick) header separator.
///
/// Like MINIMAL but with a heavy line under the header.
pub const MINIMAL_HEAVY_HEAD: BoxChars = BoxChars::new(
    [' ', ' ', ' ', ' '],
    [' ', ' ', '\u{2502}', ' '],        //   │
    [' ', '\u{2501}', '\u{2547}', ' '], //  ━╇ (heavy line with mixed cross)
    [' ', ' ', ' ', ' '],
    [' ', ' ', ' ', ' '],
    [' ', ' ', ' ', ' '],
    [' ', ' ', '\u{2502}', ' '], //   │
    [' ', ' ', ' ', ' '],
    false,
);

/// Get a box style by name.
#[must_use]
pub fn get_box(name: &str) -> Option<&'static BoxChars> {
    match name.to_lowercase().as_str() {
        "ascii" => Some(&ASCII),
        "ascii2" => Some(&ASCII2),
        "ascii_double_head" => Some(&ASCII_DOUBLE_HEAD),
        "rounded" => Some(&ROUNDED),
        "square" => Some(&SQUARE),
        "double" => Some(&DOUBLE),
        "heavy" => Some(&HEAVY),
        "heavy_head" => Some(&HEAVY_HEAD),
        "minimal" => Some(&MINIMAL),
        "minimal_heavy_head" => Some(&MINIMAL_HEAVY_HEAD),
        "simple" => Some(&SIMPLE),
        "simple_heavy" => Some(&SIMPLE_HEAVY),
        "horizontals" => Some(&HORIZONTALS),
        "markdown" => Some(&MARKDOWN),
        _ => None,
    }
}

/// Get an ASCII-safe version of a box style.
#[must_use]
pub fn get_safe_box(name: &str) -> &'static BoxChars {
    let box_style = get_box(name).unwrap_or(&SQUARE);
    if box_style.ascii { box_style } else { &ASCII }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ascii_box() {
        const { assert!(ASCII.ascii) };
        assert_eq!(ASCII.top[0], '+');
    }

    #[test]
    fn test_get_top() {
        let widths = [5, 3, 7];
        let top = ASCII.get_top(&widths);
        assert_eq!(top, "+-----+---+-------+");
    }

    #[test]
    fn test_get_bottom() {
        let widths = [5, 3];
        let bottom = ASCII.get_bottom(&widths);
        assert_eq!(bottom, "+-----+---+");
    }

    #[test]
    fn test_unicode_square() {
        const { assert!(!SQUARE.ascii) };
        assert_eq!(SQUARE.top[0], '\u{250C}'); // ┌
    }

    #[test]
    fn test_get_box() {
        assert!(get_box("ascii").is_some());
        assert!(get_box("SQUARE").is_some()); // Case insensitive
        assert!(get_box("nonexistent").is_none());
    }

    #[test]
    fn test_get_safe_box() {
        let safe = get_safe_box("double");
        assert!(safe.ascii); // Should return ASCII for non-ASCII box
    }

    #[test]
    fn test_build_row_widths() {
        let widths = [4, 4];
        let row = SQUARE.build_row(&widths, RowLevel::HeadRow, true);
        assert_ne!(row, "");
        assert!(row.contains('\u{253C}')); // ┼
    }

    #[test]
    fn test_cell_characters() {
        assert_eq!(ASCII.cell_left(), '|');
        assert_eq!(ASCII.cell_divider(), '|');
        assert_eq!(ASCII.cell_right(), '|');
    }

    #[test]
    fn test_rounded_box() {
        const { assert!(!ROUNDED.ascii) };
        assert_eq!(ROUNDED.top[0], '\u{256D}'); // ╭
        assert_eq!(ROUNDED.bottom[0], '\u{2570}'); // ╰
    }

    #[test]
    fn test_double_box() {
        const { assert!(!DOUBLE.ascii) };
        assert_eq!(DOUBLE.top[0], '\u{2554}'); // ╔
        assert_eq!(DOUBLE.top[1], '\u{2550}'); // ═
        assert_eq!(DOUBLE.bottom[3], '\u{255D}'); // ╝
    }

    #[test]
    fn test_heavy_box() {
        const { assert!(!HEAVY.ascii) };
        assert_eq!(HEAVY.top[0], '\u{250F}'); // ┏
        assert_eq!(HEAVY.top[1], '\u{2501}'); // ━
        assert_eq!(HEAVY.bottom[0], '\u{2517}'); // ┗
    }

    #[test]
    fn test_heavy_head_box() {
        const { assert!(!HEAVY_HEAD.ascii) };
        // Heavy top
        assert_eq!(HEAVY_HEAD.top[0], '\u{250F}'); // ┏
        // Light bottom
        assert_eq!(HEAVY_HEAD.bottom[0], '\u{2514}'); // └
    }

    #[test]
    fn test_minimal_box() {
        const { assert!(!MINIMAL.ascii) };
        // No outer border
        assert_eq!(MINIMAL.top[0], ' ');
        assert_eq!(MINIMAL.bottom[0], ' ');
        // Has internal divider
        assert_eq!(MINIMAL.head[2], '\u{2502}'); // │
    }

    #[test]
    fn test_simple_box() {
        const { assert!(!SIMPLE.ascii) };
        // No outer border
        assert_eq!(SIMPLE.top[0], ' ');
        // Head row has horizontal line
        assert_eq!(SIMPLE.head_row[1], '\u{2500}'); // ─
    }

    #[test]
    fn test_simple_heavy_box() {
        const { assert!(!SIMPLE_HEAVY.ascii) };
        // Heavy horizontal line
        assert_eq!(SIMPLE_HEAVY.head_row[1], '\u{2501}'); // ━
    }

    #[test]
    fn test_ascii2_box() {
        const { assert!(ASCII2.ascii) };
        // Uses + at intersections
        assert_eq!(ASCII2.head_row[0], '+');
        assert_eq!(ASCII2.head_row[2], '+');
    }

    #[test]
    fn test_ascii_double_head_box() {
        const { assert!(ASCII_DOUBLE_HEAD.ascii) };
        // Head row uses = for double line
        assert_eq!(ASCII_DOUBLE_HEAD.head_row[1], '=');
        // Other rows use -
        assert_eq!(ASCII_DOUBLE_HEAD.row[1], '-');
    }

    #[test]
    fn test_get_row_chars_all_levels() {
        // Test that get_row_chars returns correct arrays for each level
        assert_eq!(SQUARE.get_row_chars(RowLevel::Top), &SQUARE.top);
        assert_eq!(SQUARE.get_row_chars(RowLevel::HeadRow), &SQUARE.head_row);
        assert_eq!(SQUARE.get_row_chars(RowLevel::Mid), &SQUARE.mid);
        assert_eq!(SQUARE.get_row_chars(RowLevel::Row), &SQUARE.row);
        assert_eq!(SQUARE.get_row_chars(RowLevel::FootRow), &SQUARE.foot_row);
        assert_eq!(SQUARE.get_row_chars(RowLevel::Bottom), &SQUARE.bottom);
    }

    #[test]
    fn test_get_mid() {
        let widths = [3, 3];
        let mid = SQUARE.get_mid(&widths);
        assert!(mid.starts_with('\u{251C}')); // ├
        assert!(mid.ends_with('\u{2524}')); // ┤
    }

    #[test]
    fn test_get_row() {
        let widths = [3, 3];
        let row = SQUARE.get_row(&widths);
        // For SQUARE, row == mid
        assert_eq!(row, SQUARE.get_mid(&widths));
    }

    #[test]
    fn test_get_head_row() {
        let widths = [3, 3];
        let head_row = SQUARE.get_head_row(&widths);
        assert!(head_row.contains('\u{253C}')); // ┼
    }

    #[test]
    fn test_build_row_no_edge() {
        let widths = [3, 3];
        let row = MINIMAL.build_row(&widths, RowLevel::Top, false);
        // MINIMAL has spaces for top, should produce no edge characters
        assert!(!row.contains('\u{250C}')); // No corner
    }

    #[test]
    fn test_build_row_single_column() {
        let widths = [5];
        let top = ASCII.get_top(&widths);
        assert_eq!(top, "+-----+");
    }

    #[test]
    fn test_display_trait() {
        let display = format!("{ASCII}");
        assert!(display.contains('+'));
        assert!(display.contains('-'));
        assert!(display.contains('|'));
    }

    #[test]
    fn test_substitute_ascii() {
        // ASCII box should return self
        let subst = ASCII.substitute(true);
        assert!(subst.ascii);
        assert_eq!(subst.top, ASCII.top);
        assert_eq!(subst.head, ASCII.head);
        assert_eq!(subst.bottom, ASCII.bottom);
    }

    #[test]
    fn test_substitute_unicode() {
        // Unicode boxes substitute to ASCII in safe mode.
        let subst = SQUARE.substitute(true);
        assert!(subst.ascii);
        assert_eq!(subst.top, ASCII.top);
        assert_eq!(subst.head, ASCII.head);
        assert_eq!(subst.bottom, ASCII.bottom);
    }

    #[test]
    fn test_get_box_all_styles() {
        // Verify all named styles are accessible
        assert!(get_box("ascii").is_some());
        assert!(get_box("ascii2").is_some());
        assert!(get_box("ascii_double_head").is_some());
        assert!(get_box("rounded").is_some());
        assert!(get_box("square").is_some());
        assert!(get_box("double").is_some());
        assert!(get_box("heavy").is_some());
        assert!(get_box("heavy_head").is_some());
        assert!(get_box("minimal").is_some());
        assert!(get_box("minimal_heavy_head").is_some());
        assert!(get_box("simple").is_some());
        assert!(get_box("simple_heavy").is_some());
        assert!(get_box("horizontals").is_some());
        assert!(get_box("markdown").is_some());
    }

    #[test]
    fn test_horizontals_box() {
        const { assert!(!HORIZONTALS.ascii) };
        // No vertical dividers in cells
        assert_eq!(HORIZONTALS.head[2], ' ');
        // Has horizontal lines at top
        assert_eq!(HORIZONTALS.top[1], '\u{2500}'); // ─
        // Has horizontal lines at header row
        assert_eq!(HORIZONTALS.head_row[1], '\u{2500}'); // ─
    }

    #[test]
    fn test_markdown_box() {
        const { assert!(MARKDOWN.ascii) };
        // Uses pipes for cell dividers
        assert_eq!(MARKDOWN.head[0], '|');
        assert_eq!(MARKDOWN.head[2], '|');
        // Uses dashes for separators
        assert_eq!(MARKDOWN.head_row[1], '-');
        // No top or bottom border
        assert_eq!(MARKDOWN.top[0], ' ');
        assert_eq!(MARKDOWN.bottom[0], ' ');
    }

    #[test]
    fn test_minimal_heavy_head_box() {
        const { assert!(!MINIMAL_HEAVY_HEAD.ascii) };
        // No outer border
        assert_eq!(MINIMAL_HEAVY_HEAD.top[0], ' ');
        assert_eq!(MINIMAL_HEAVY_HEAD.bottom[0], ' ');
        // Light vertical divider in cells
        assert_eq!(MINIMAL_HEAVY_HEAD.head[2], '\u{2502}'); // │
        // Heavy horizontal line in head_row
        assert_eq!(MINIMAL_HEAVY_HEAD.head_row[1], '\u{2501}'); // ━
    }

    #[test]
    fn test_get_box_case_insensitive() {
        assert!(get_box("ASCII").is_some());
        assert!(get_box("Ascii").is_some());
        assert!(get_box("ROUNDED").is_some());
        assert!(get_box("Rounded").is_some());
    }

    #[test]
    fn test_get_safe_box_returns_ascii_for_ascii_input() {
        let safe = get_safe_box("ascii");
        assert!(safe.ascii);
    }

    #[test]
    fn test_get_safe_box_unknown_returns_ascii() {
        let safe = get_safe_box("nonexistent");
        assert!(safe.ascii);
    }

    #[test]
    fn test_cell_characters_unicode() {
        assert_eq!(SQUARE.cell_left(), '\u{2502}'); // │
        assert_eq!(SQUARE.cell_divider(), '\u{2502}'); // │
        assert_eq!(SQUARE.cell_right(), '\u{2502}'); // │
    }

    #[test]
    fn test_empty_widths() {
        let widths: [usize; 0] = [];
        let top = ASCII.get_top(&widths);
        // With no columns, only left edge is emitted (no content, no right edge)
        assert_eq!(top, "+");
    }
}
