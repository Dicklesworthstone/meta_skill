//! Align - Horizontal alignment wrapper for renderables.
//!
//! This module provides alignment functionality to position content
//! within a given width (left, center, or right aligned).
//!
//! # Example
//!
//! ```rust,ignore
//! use rich_rust::renderables::align::{Align, AlignMethod};
//! use rich_rust::segment::Segment;
//!
//! let content = vec![Segment::new("Hello", None)];
//! let aligned = Align::new(content, 20)
//!     .method(AlignMethod::Center)
//!     .render();
//! ```

use crate::cells::cell_len;
use crate::console::{Console, ConsoleOptions};
use crate::renderables::Renderable;
use crate::segment::Segment;
use crate::style::Style;

/// Horizontal alignment method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlignMethod {
    /// Align content to the left (default).
    #[default]
    Left,
    /// Center content horizontally.
    Center,
    /// Align content to the right.
    Right,
}

/// Vertical alignment method for multi-line content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerticalAlignMethod {
    /// Align content to the top (default).
    #[default]
    Top,
    /// Center content vertically.
    Middle,
    /// Align content to the bottom.
    Bottom,
}

/// A wrapper that aligns content within a given width.
#[derive(Debug, Clone)]
pub struct Align<'a> {
    /// Content segments to align.
    content: Vec<Segment<'a>>,
    /// Target width.
    width: usize,
    /// Horizontal alignment method.
    method: AlignMethod,
    /// Style for padding spaces.
    pad_style: Style,
}

impl<'a> Align<'a> {
    /// Create a new Align wrapper for single-line content.
    #[must_use]
    pub fn new(content: impl IntoIterator<Item = Segment<'a>>, width: usize) -> Self {
        Self {
            content: content.into_iter().collect(),
            width,
            method: AlignMethod::Left,
            pad_style: Style::new(),
        }
    }

    /// Create an Align wrapper from a string.
    #[must_use]
    pub fn from_str(text: &'a str, width: usize) -> Self {
        Self::new(vec![Segment::new(text, None)], width)
    }

    /// Set the alignment method.
    #[must_use]
    pub fn method(mut self, method: AlignMethod) -> Self {
        self.method = method;
        self
    }

    /// Align content to the left.
    #[must_use]
    pub fn left(self) -> Self {
        self.method(AlignMethod::Left)
    }

    /// Center content horizontally.
    #[must_use]
    pub fn center(self) -> Self {
        self.method(AlignMethod::Center)
    }

    /// Align content to the right.
    #[must_use]
    pub fn right(self) -> Self {
        self.method(AlignMethod::Right)
    }

    /// Set the style for padding spaces.
    #[must_use]
    pub fn pad_style(mut self, style: Style) -> Self {
        self.pad_style = style;
        self
    }

    /// Get the content width in cells.
    #[must_use]
    pub fn content_width(&self) -> usize {
        self.content.iter().map(|s| cell_len(&s.text)).sum()
    }

    /// Render the aligned content.
    #[must_use]
    pub fn render(self) -> Vec<Segment<'a>> {
        let content_width = self.content_width();

        // If content is wider than or equal to target width, return as-is
        if content_width >= self.width {
            return self.content;
        }

        let padding_total = self.width - content_width;
        let mut result = Vec::with_capacity(self.content.len() + 2);

        match self.method {
            AlignMethod::Left => {
                // Content first, then right padding
                result.extend(self.content);
                if padding_total > 0 {
                    result.push(Segment::new(
                        " ".repeat(padding_total),
                        Some(self.pad_style),
                    ));
                }
            }
            AlignMethod::Center => {
                // Left padding, content, right padding
                let left_pad = padding_total / 2;
                let right_pad = padding_total - left_pad;

                if left_pad > 0 {
                    result.push(Segment::new(
                        " ".repeat(left_pad),
                        Some(self.pad_style.clone()),
                    ));
                }
                result.extend(self.content);
                if right_pad > 0 {
                    result.push(Segment::new(" ".repeat(right_pad), Some(self.pad_style)));
                }
            }
            AlignMethod::Right => {
                // Left padding first, then content
                if padding_total > 0 {
                    result.push(Segment::new(
                        " ".repeat(padding_total),
                        Some(self.pad_style),
                    ));
                }
                result.extend(self.content);
            }
        }

        result
    }
}

impl Renderable for Align<'_> {
    fn render<'b>(&'b self, _console: &Console, _options: &ConsoleOptions) -> Vec<Segment<'b>> {
        // Since Align consumes self in render(), we need to clone it if we want to implement Renderable for &Align
        // But Renderable takes &self.
        // Align::render(self).
        // Align is cheap to clone (Vec<Segment> might be expensive if deep).
        // Ideally Align::render should take &self.
        self.clone().render().into_iter().collect()
    }
}

/// A wrapper that aligns multiple lines of content.
#[derive(Debug, Clone)]
pub struct AlignLines<'a> {
    /// Lines of content (each line is a Vec of Segments).
    lines: Vec<Vec<Segment<'a>>>,
    /// Target width.
    width: usize,
    /// Horizontal alignment method.
    method: AlignMethod,
    /// Style for padding spaces.
    pad_style: Style,
}

impl<'a> AlignLines<'a> {
    /// Create a new `AlignLines` wrapper.
    #[must_use]
    pub fn new(lines: Vec<Vec<Segment<'a>>>, width: usize) -> Self {
        Self {
            lines,
            width,
            method: AlignMethod::Left,
            pad_style: Style::new(),
        }
    }

    /// Set the alignment method.
    #[must_use]
    pub fn method(mut self, method: AlignMethod) -> Self {
        self.method = method;
        self
    }

    /// Align content to the left.
    #[must_use]
    pub fn left(self) -> Self {
        self.method(AlignMethod::Left)
    }

    /// Center content horizontally.
    #[must_use]
    pub fn center(self) -> Self {
        self.method(AlignMethod::Center)
    }

    /// Align content to the right.
    #[must_use]
    pub fn right(self) -> Self {
        self.method(AlignMethod::Right)
    }

    /// Set the style for padding spaces.
    #[must_use]
    pub fn pad_style(mut self, style: Style) -> Self {
        self.pad_style = style;
        self
    }

    /// Get the width of a line in cells.
    #[must_use]
    pub fn line_width(line: &[Segment]) -> usize {
        line.iter().map(|s| cell_len(&s.text)).sum()
    }

    /// Render all lines with alignment applied.
    #[must_use]
    pub fn render(self) -> Vec<Vec<Segment<'a>>> {
        self.lines
            .into_iter()
            .map(|line| {
                Align::new(line, self.width)
                    .method(self.method)
                    .pad_style(self.pad_style.clone())
                    .render()
            })
            .collect()
    }
}

impl Renderable for AlignLines<'_> {
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

/// Convenience function to align a single line of text.
#[must_use]
pub fn align_text(text: &str, width: usize, method: AlignMethod) -> String {
    let content_width = cell_len(text);

    if content_width >= width {
        return text.to_string();
    }

    let padding_total = width - content_width;

    match method {
        AlignMethod::Left => {
            format!("{text}{}", " ".repeat(padding_total))
        }
        AlignMethod::Center => {
            let left_pad = padding_total / 2;
            let right_pad = padding_total - left_pad;
            format!("{}{text}{}", " ".repeat(left_pad), " ".repeat(right_pad))
        }
        AlignMethod::Right => {
            format!("{}{text}", " ".repeat(padding_total))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_align_method_default() {
        assert_eq!(AlignMethod::default(), AlignMethod::Left);
    }

    #[test]
    fn test_align_left() {
        let content = vec![Segment::new("Hi", None)];
        let aligned = Align::new(content, 10).left().render();

        let text: String = aligned.iter().map(|s| s.text.as_ref()).collect();
        assert_eq!(text, "Hi        ");
        assert_eq!(cell_len(&text), 10);
    }

    #[test]
    fn test_align_center() {
        let content = vec![Segment::new("Hi", None)];
        let aligned = Align::new(content, 10).center().render();

        let text: String = aligned.iter().map(|s| s.text.as_ref()).collect();
        assert_eq!(text, "    Hi    ");
        assert_eq!(cell_len(&text), 10);
    }

    #[test]
    fn test_align_right() {
        let content = vec![Segment::new("Hi", None)];
        let aligned = Align::new(content, 10).right().render();

        let text: String = aligned.iter().map(|s| s.text.as_ref()).collect();
        assert_eq!(text, "        Hi");
        assert_eq!(cell_len(&text), 10);
    }

    #[test]
    fn test_align_content_too_wide() {
        let content = vec![Segment::new("Hello World", None)];
        let aligned = Align::new(content, 5).center().render();

        // Should return content as-is when wider than target
        let text: String = aligned.iter().map(|s| s.text.as_ref()).collect();
        assert_eq!(text, "Hello World");
    }

    #[test]
    fn test_align_exact_width() {
        let content = vec![Segment::new("Hello", None)];
        let aligned = Align::new(content, 5).center().render();

        let text: String = aligned.iter().map(|s| s.text.as_ref()).collect();
        assert_eq!(text, "Hello");
    }

    #[test]
    fn test_align_multiple_segments() {
        let content = vec![
            Segment::new("Hello", None),
            Segment::new(" ", None),
            Segment::new("World", None),
        ];
        let aligned = Align::new(content, 20).center().render();

        let text: String = aligned.iter().map(|s| s.text.as_ref()).collect();
        assert_eq!(cell_len(&text), 20);
        assert!(text.contains("Hello World"));
    }

    #[test]
    fn test_align_center_odd_padding() {
        // 3 char content in 10 width = 7 padding
        // Left: 3, Right: 4
        let content = vec![Segment::new("abc", None)];
        let aligned = Align::new(content, 10).center().render();

        let text: String = aligned.iter().map(|s| s.text.as_ref()).collect();
        assert_eq!(text, "   abc    ");
        assert_eq!(cell_len(&text), 10);
    }

    #[test]
    fn test_align_from_str() {
        let aligned = Align::from_str("Test", 10).right().render();

        let text: String = aligned.iter().map(|s| s.text.as_ref()).collect();
        assert_eq!(text, "      Test");
    }

    #[test]
    fn test_align_lines() {
        let lines = vec![
            vec![Segment::new("Short", None)],
            vec![Segment::new("Longer line", None)],
            vec![Segment::new("Hi", None)],
        ];
        let aligned = AlignLines::new(lines, 15).center().render();

        assert_eq!(aligned.len(), 3);

        // Each line should be 15 chars wide
        for line in &aligned {
            let text: String = line.iter().map(|s| s.text.as_ref()).collect();
            assert_eq!(cell_len(&text), 15);
        }
    }

    #[test]
    fn test_align_text_function() {
        assert_eq!(align_text("Hi", 10, AlignMethod::Left), "Hi        ");
        assert_eq!(align_text("Hi", 10, AlignMethod::Center), "    Hi    ");
        assert_eq!(align_text("Hi", 10, AlignMethod::Right), "        Hi");
    }

    #[test]
    fn test_align_with_cjk() {
        // CJK characters are 2 cells wide
        let content = vec![Segment::new("日本", None)]; // 4 cells wide
        let aligned = Align::new(content, 10).center().render();

        let text: String = aligned.iter().map(|s| s.text.as_ref()).collect();
        assert_eq!(cell_len(&text), 10);
        // 10 - 4 = 6 padding, left: 3, right: 3
        assert!(text.starts_with("   "));
        assert!(text.ends_with("   "));
    }

    #[test]
    fn test_content_width() {
        let content = vec![
            Segment::new("Hello", None),
            Segment::new(" ", None),
            Segment::new("World", None),
        ];
        let align = Align::new(content, 20);
        assert_eq!(align.content_width(), 11);
    }

    #[test]
    fn test_vertical_align_default() {
        assert_eq!(VerticalAlignMethod::default(), VerticalAlignMethod::Top);
    }
}
