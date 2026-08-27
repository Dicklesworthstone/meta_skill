//! Padding - CSS-style padding for renderables.
//!
//! This module provides padding dimensions that follow CSS conventions:
//! - 1 value: all sides equal
//! - 2 values: (vertical, horizontal) -> top/bottom, left/right
//! - 4 values: (top, right, bottom, left) -> individual sides

use crate::console::{Console, ConsoleOptions};
use crate::renderables::Renderable;
use crate::segment::Segment;
use crate::style::Style;

/// CSS-style padding dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PaddingDimensions {
    /// Top padding in cells.
    pub top: usize,
    /// Right padding in cells.
    pub right: usize,
    /// Bottom padding in cells.
    pub bottom: usize,
    /// Left padding in cells.
    pub left: usize,
}

impl PaddingDimensions {
    /// Create padding with all sides equal.
    #[must_use]
    pub const fn all(n: usize) -> Self {
        Self {
            top: n,
            right: n,
            bottom: n,
            left: n,
        }
    }

    /// Create padding with separate vertical and horizontal values.
    #[must_use]
    pub const fn symmetric(vertical: usize, horizontal: usize) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }

    /// Create padding with individual values for each side.
    #[must_use]
    pub const fn new(top: usize, right: usize, bottom: usize, left: usize) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    /// Create zero padding.
    #[must_use]
    pub const fn zero() -> Self {
        Self::all(0)
    }

    /// Total horizontal padding (left + right).
    #[must_use]
    pub const fn horizontal(&self) -> usize {
        self.left + self.right
    }

    /// Total vertical padding (top + bottom).
    #[must_use]
    pub const fn vertical(&self) -> usize {
        self.top + self.bottom
    }
}

impl From<usize> for PaddingDimensions {
    fn from(n: usize) -> Self {
        Self::all(n)
    }
}

impl From<(usize, usize)> for PaddingDimensions {
    fn from((vertical, horizontal): (usize, usize)) -> Self {
        Self::symmetric(vertical, horizontal)
    }
}

impl From<[usize; 2]> for PaddingDimensions {
    fn from([vertical, horizontal]: [usize; 2]) -> Self {
        Self::symmetric(vertical, horizontal)
    }
}

impl From<(usize, usize, usize, usize)> for PaddingDimensions {
    fn from((top, right, bottom, left): (usize, usize, usize, usize)) -> Self {
        Self::new(top, right, bottom, left)
    }
}

impl From<[usize; 4]> for PaddingDimensions {
    fn from([top, right, bottom, left]: [usize; 4]) -> Self {
        Self::new(top, right, bottom, left)
    }
}

/// A wrapper that adds padding around content.
#[derive(Debug, Clone)]
pub struct Padding<'a> {
    /// Lines of content (each line is a Vec of Segments).
    content_lines: Vec<Vec<Segment<'a>>>,
    /// Padding dimensions.
    pad: PaddingDimensions,
    /// Style for the padding (background fill).
    style: Style,
    /// Width to expand content to.
    width: usize,
    /// Expand lines to the full inner width.
    expand: bool,
}

impl<'a> Padding<'a> {
    /// Create a new Padding wrapper.
    #[must_use]
    pub fn new(
        content_lines: Vec<Vec<Segment<'a>>>,
        pad: impl Into<PaddingDimensions>,
        width: usize,
    ) -> Self {
        Self {
            content_lines,
            pad: pad.into(),
            style: Style::new(),
            width,
            expand: true,
        }
    }

    /// Set the padding style.
    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set whether to expand lines to the full inner width.
    #[must_use]
    pub const fn expand(mut self, expand: bool) -> Self {
        self.expand = expand;
        self
    }

    /// Get the width of a line in cells.
    fn line_width(line: &[Segment<'_>]) -> usize {
        line.iter().map(Segment::cell_length).sum()
    }

    /// Render with padding applied.
    #[must_use]
    pub fn render(self) -> Vec<Vec<Segment<'a>>> {
        let mut result = Vec::new();

        let inner_width = self.width.saturating_sub(self.pad.horizontal());
        let left_pad = " ".repeat(self.pad.left);
        let right_pad = " ".repeat(self.pad.right);
        let blank_line_inner = " ".repeat(inner_width);

        // Top padding
        for _ in 0..self.pad.top {
            let mut line = Vec::new();
            if self.pad.left > 0 {
                line.push(Segment::new(left_pad.clone(), Some(self.style.clone())));
            }
            line.push(Segment::new(
                blank_line_inner.clone(),
                Some(self.style.clone()),
            ));
            if self.pad.right > 0 {
                line.push(Segment::new(right_pad.clone(), Some(self.style.clone())));
            }
            result.push(line);
        }

        // Content lines with left/right padding
        for content_line in self.content_lines {
            let mut line = Vec::new();

            if self.pad.left > 0 {
                line.push(Segment::new(left_pad.clone(), Some(self.style.clone())));
            }

            let content_width = Self::line_width(&content_line);
            line.extend(content_line);

            if self.expand && content_width < inner_width {
                let fill = inner_width.saturating_sub(content_width);
                if fill > 0 {
                    line.push(Segment::new(" ".repeat(fill), Some(self.style.clone())));
                }
            }

            if self.pad.right > 0 {
                line.push(Segment::new(right_pad.clone(), Some(self.style.clone())));
            }

            result.push(line);
        }

        // Bottom padding
        for _ in 0..self.pad.bottom {
            let mut line = Vec::new();
            if self.pad.left > 0 {
                line.push(Segment::new(left_pad.clone(), Some(self.style.clone())));
            }
            line.push(Segment::new(
                blank_line_inner.clone(),
                Some(self.style.clone()),
            ));
            if self.pad.right > 0 {
                line.push(Segment::new(right_pad.clone(), Some(self.style.clone())));
            }
            result.push(line);
        }

        result
    }
}

impl Renderable for Padding<'_> {
    fn render<'b>(&'b self, _console: &Console, _options: &ConsoleOptions) -> Vec<Segment<'b>> {
        let lines = self.clone().render();
        let mut result = Vec::new();
        for (i, line) in lines.into_iter().enumerate() {
            if i > 0 {
                result.push(Segment::line());
            }
            result.extend(line);
        }
        result.into_iter().collect()
    }
}

/// Create indentation padding (left-side only).
#[must_use]
pub fn indent(level: usize) -> PaddingDimensions {
    PaddingDimensions::new(0, 0, 0, level)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cells::cell_len;

    #[test]
    fn test_padding_all() {
        let pad = PaddingDimensions::all(2);
        assert_eq!(pad.top, 2);
        assert_eq!(pad.right, 2);
        assert_eq!(pad.bottom, 2);
        assert_eq!(pad.left, 2);
    }

    #[test]
    fn test_padding_symmetric() {
        let pad = PaddingDimensions::symmetric(1, 3);
        assert_eq!(pad.top, 1);
        assert_eq!(pad.right, 3);
        assert_eq!(pad.bottom, 1);
        assert_eq!(pad.left, 3);
    }

    #[test]
    fn test_padding_individual() {
        let pad = PaddingDimensions::new(1, 2, 3, 4);
        assert_eq!(pad.top, 1);
        assert_eq!(pad.right, 2);
        assert_eq!(pad.bottom, 3);
        assert_eq!(pad.left, 4);
    }

    #[test]
    fn test_padding_from_usize() {
        let pad: PaddingDimensions = 5.into();
        assert_eq!(pad, PaddingDimensions::all(5));
    }

    #[test]
    fn test_padding_from_tuple2() {
        let pad: PaddingDimensions = (1, 2).into();
        assert_eq!(pad, PaddingDimensions::symmetric(1, 2));
    }

    #[test]
    fn test_padding_from_tuple4() {
        let pad: PaddingDimensions = (1, 2, 3, 4).into();
        assert_eq!(pad, PaddingDimensions::new(1, 2, 3, 4));
    }

    #[test]
    fn test_horizontal_vertical() {
        let pad = PaddingDimensions::new(1, 2, 3, 4);
        assert_eq!(pad.horizontal(), 6); // 2 + 4
        assert_eq!(pad.vertical(), 4); // 1 + 3
    }

    #[test]
    fn test_indent() {
        let pad = indent(4);
        assert_eq!(pad.left, 4);
        assert_eq!(pad.right, 0);
        assert_eq!(pad.top, 0);
        assert_eq!(pad.bottom, 0);
    }

    #[test]
    fn test_padding_render() {
        let content = vec![vec![Segment::new("Hello", None)]];
        let padded = Padding::new(content, 1, 10);
        let lines = padded.render();

        // Should have 1 top + 1 content + 1 bottom = 3 lines
        assert_eq!(lines.len(), 3);
    }

    fn line_text(line: &[Segment<'_>]) -> String {
        line.iter().map(|seg| seg.text.as_ref()).collect::<String>()
    }

    fn line_width(line: &[Segment<'_>]) -> usize {
        line.iter().map(|seg| cell_len(&seg.text)).sum()
    }

    #[test]
    fn test_padding_left_right_expand() {
        let content = vec![vec![Segment::new("Hi", None)]];
        let padded = Padding::new(content, (0, 2, 0, 1), 6);
        let lines = padded.render();

        assert_eq!(lines.len(), 1);
        assert_eq!(line_width(&lines[0]), 6);
        assert_eq!(line_text(&lines[0]), " Hi   ");
    }

    #[test]
    fn test_padding_no_expand() {
        let content = vec![vec![Segment::new("Hi", None)]];
        let padded = Padding::new(content, (0, 2, 0, 1), 6).expand(false);
        let lines = padded.render();

        assert_eq!(lines.len(), 1);
        assert_eq!(line_width(&lines[0]), 5);
        assert_eq!(line_text(&lines[0]), " Hi  ");
    }

    #[test]
    fn test_padding_zero_noop() {
        let content = vec![vec![Segment::new("Hi", None)]];
        let padded = Padding::new(content.clone(), 0, 2).expand(false);
        let lines = padded.render();

        assert_eq!(lines.len(), 1);
        assert_eq!(line_text(&lines[0]), "Hi");
    }

    #[test]
    fn test_padding_nested_accumulates() {
        // Nested padding should add top/bottom lines additively
        // Inner: 1 content line + 1 top + 1 bottom = 3 lines
        // Outer: 3 content lines + 1 top + 1 bottom = 5 lines
        let content = vec![vec![Segment::new("Hi", None)]];
        let inner = Padding::new(content, 1, 4).render();
        let outer = Padding::new(inner, 1, 6).render();

        assert_eq!(outer.len(), 5);
        // Line 0: outer top padding
        // Line 1: inner top padding (wrapped with outer horizontal padding)
        // Line 2: content "Hi" (wrapped with both paddings)
        // Line 3: inner bottom padding (wrapped with outer horizontal padding)
        // Line 4: outer bottom padding
        assert_eq!(line_width(&outer[2]), 6);
        assert_eq!(line_text(&outer[2]), "  Hi  ");
    }
}
