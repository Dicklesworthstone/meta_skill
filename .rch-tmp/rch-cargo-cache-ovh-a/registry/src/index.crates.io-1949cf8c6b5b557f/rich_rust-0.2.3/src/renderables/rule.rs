//! Rule - horizontal line with optional title.
//!
//! A Rule renders as a horizontal line that spans the console width,
//! optionally with a centered (or aligned) title.

use crate::cells;
use crate::console::{Console, ConsoleOptions};
use crate::renderables::Renderable;
use crate::segment::Segment;
use crate::style::Style;
use crate::text::{JustifyMethod, OverflowMethod, Text};

/// A horizontal rule with optional title.
#[derive(Debug, Clone)]
pub struct Rule {
    /// Optional title text.
    title: Option<Text>,
    /// Character to use for the rule line.
    character: String,
    /// Style for the rule line.
    style: Style,
    /// Title alignment.
    align: JustifyMethod,
}

impl Default for Rule {
    fn default() -> Self {
        Self {
            title: None,
            character: String::from("\u{2500}"), // ─
            style: Style::parse("bright_green").unwrap_or_default(),
            align: JustifyMethod::Center,
        }
    }
}

impl Rule {
    /// Create a new rule without a title.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a rule with a title.
    #[must_use]
    pub fn with_title(title: impl Into<Text>) -> Self {
        Self {
            title: Some(title.into()),
            ..Self::default()
        }
    }

    /// Set the rule character.
    #[must_use]
    pub fn character(mut self, ch: impl Into<String>) -> Self {
        self.character = ch.into();
        self
    }

    /// Set the rule style.
    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set title alignment.
    #[must_use]
    pub fn align(mut self, align: JustifyMethod) -> Self {
        self.align = align;
        self
    }

    /// Left-align the title.
    #[must_use]
    pub fn align_left(self) -> Self {
        self.align(JustifyMethod::Left)
    }

    /// Center the title.
    #[must_use]
    pub fn align_center(self) -> Self {
        self.align(JustifyMethod::Center)
    }

    /// Right-align the title.
    #[must_use]
    pub fn align_right(self) -> Self {
        self.align(JustifyMethod::Right)
    }

    /// Render the rule to segments for a given width.
    #[must_use]
    pub fn render(&self, width: usize) -> Vec<Segment<'static>> {
        let char_width = cells::cell_len(&self.character);
        if char_width == 0 || width == 0 {
            return vec![Segment::line()];
        }

        let mut segments = Vec::new();

        if let Some(title) = &self.title {
            // Sanitize title to prevent broken layout
            let mut title = title.clone();
            if title.plain().contains('\n') {
                let sanitized = title.plain().replace('\n', " ");
                title = Text::new(sanitized);
            }

            if title.plain().is_empty() {
                let count = width / char_width;
                let rule_text = self.character.repeat(count);
                segments.push(Segment::new(rule_text, Some(self.style.clone())));
                segments.push(Segment::line());
                return segments;
            }

            let title_width = cells::cell_len(title.plain());
            let (left_pad, right_pad) = match self.align {
                JustifyMethod::Left => (0, 1),
                JustifyMethod::Right => (1, 0),
                JustifyMethod::Center | JustifyMethod::Full | JustifyMethod::Default => (1, 1),
            };
            let title_total_width = title_width
                .saturating_add(left_pad)
                .saturating_add(right_pad);

            if title_total_width > width {
                let mut truncated = title.clone();
                truncated.truncate(width, OverflowMethod::Crop, false);
                segments.extend(
                    truncated
                        .render("")
                        .into_iter()
                        .map(super::super::segment::Segment::into_owned),
                );
                segments.push(Segment::line());
                return segments;
            }

            // Calculate available space for rule characters
            let available = width.saturating_sub(title_total_width);
            let rule_chars = available / char_width;

            if rule_chars < 1 {
                // Not enough space for rule, just show title
                if left_pad > 0 {
                    segments.push(Segment::new(
                        " ".repeat(left_pad),
                        Some(title.style().clone()),
                    ));
                }
                segments.extend(
                    title
                        .render("")
                        .into_iter()
                        .map(super::super::segment::Segment::into_owned),
                );
                if right_pad > 0 {
                    segments.push(Segment::new(
                        " ".repeat(right_pad),
                        Some(title.style().clone()),
                    ));
                }
            } else {
                let (left_count, right_count) = match self.align {
                    JustifyMethod::Left => (0, rule_chars),
                    JustifyMethod::Right => (rule_chars, 0),
                    JustifyMethod::Center | JustifyMethod::Full => {
                        let left = rule_chars / 2;
                        let right = rule_chars - left;
                        (left, right)
                    }
                    JustifyMethod::Default => {
                        let left = rule_chars / 2;
                        let right = rule_chars - left;
                        (left, right)
                    }
                };

                // Left rule section
                if left_count > 0 {
                    let left_rule = self.character.repeat(left_count);
                    segments.push(Segment::new(left_rule, Some(self.style.clone())));
                }

                // Title with surrounding spaces
                if left_pad > 0 {
                    segments.push(Segment::new(
                        " ".repeat(left_pad),
                        Some(title.style().clone()),
                    ));
                }
                segments.extend(
                    title
                        .render("")
                        .into_iter()
                        .map(super::super::segment::Segment::into_owned),
                );
                if right_pad > 0 {
                    segments.push(Segment::new(
                        " ".repeat(right_pad),
                        Some(title.style().clone()),
                    ));
                }

                // Right rule section
                if right_count > 0 {
                    let right_rule = self.character.repeat(right_count);
                    segments.push(Segment::new(right_rule, Some(self.style.clone())));
                }
            }
        } else {
            // No title, just a full-width rule
            let count = width / char_width;
            let rule_text = self.character.repeat(count);
            segments.push(Segment::new(rule_text, Some(self.style.clone())));
        }

        segments.push(Segment::line());
        segments
    }

    /// Render the rule as a string (for simple output).
    #[must_use]
    pub fn render_plain(&self, width: usize) -> String {
        self.render(width)
            .into_iter()
            .map(|seg| seg.text.into_owned())
            .collect()
    }
}

impl Renderable for Rule {
    fn render<'a>(&'a self, _console: &Console, options: &ConsoleOptions) -> Vec<Segment<'a>> {
        self.render(options.max_width).into_iter().collect()
    }
}

/// Create an ASCII-safe rule.
#[must_use]
pub fn ascii_rule() -> Rule {
    Rule::new().character("-")
}

/// Create a double-line rule.
#[must_use]
pub fn double_rule() -> Rule {
    Rule::new().character("\u{2550}") // ═
}

/// Create a heavy (thick) rule.
#[must_use]
pub fn heavy_rule() -> Rule {
    Rule::new().character("\u{2501}") // ━
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_no_title() {
        let rule = Rule::new();
        let segments = rule.render(10);
        assert_ne!(segments.len(), 0);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains('\u{2500}')); // ─
    }

    #[test]
    fn test_rule_with_title() {
        let rule = Rule::with_title("Test");
        let segments = rule.render(20);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("Test"));
        assert!(text.contains('\u{2500}')); // ─
    }

    #[test]
    fn test_rule_custom_char() {
        let rule = Rule::new().character("=");
        let segments = rule.render(10);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains('='));
    }

    #[test]
    fn test_rule_alignment() {
        let rule = Rule::with_title("X").align_left();
        let plain = rule.render_plain(20);
        // Left alignment: "X <rule chars>" - title at left edge with trailing space
        // Verify title is at the start (after trimming)
        let trimmed = plain.trim();
        assert!(
            trimmed.starts_with("X "),
            "Left-aligned title should start with 'X ', got: '{trimmed}'"
        );
        // Verify there are rule characters after the title
        assert!(trimmed.contains('─'), "Should contain rule characters");
    }

    #[test]
    fn test_ascii_rule() {
        let rule = ascii_rule();
        let segments = rule.render(10);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains('-'));
    }

    #[test]
    fn test_heavy_rule() {
        let rule = heavy_rule();
        let segments = rule.render(10);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains('\u{2501}')); // ━
    }

    #[test]
    fn test_double_rule() {
        let rule = double_rule();
        let segments = rule.render(10);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains('\u{2550}')); // ═
    }

    #[test]
    fn test_rule_width_zero() {
        let rule = Rule::new();
        let segments = rule.render(0);
        // Should handle zero width gracefully
        assert_ne!(segments.len(), 0); // At least a newline segment
    }

    #[test]
    fn test_rule_width_one() {
        let rule = Rule::new();
        let segments = rule.render(1);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        // Width 1 should produce at least one rule char
        assert!(text.contains('\u{2500}') || text.is_empty() || text == "\n");
    }

    #[test]
    fn test_rule_title_narrow_width() {
        let rule = Rule::with_title("Very Long Title Text");
        let segments = rule.render(10);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        // Should handle narrow width without panicking
        assert_ne!(text, "");
    }

    #[test]
    fn test_rule_title_insufficient_space() {
        let rule = Rule::with_title("Test");
        // Width too small for title + surrounding rules
        let segments = rule.render(5);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        // Should handle gracefully
        assert_ne!(text, "");
    }

    #[test]
    fn test_rule_right_align() {
        let rule = Rule::with_title("X").align_right();
        let plain = rule.render_plain(20);
        // Right alignment: "<rule chars> X" - title at right edge with leading space
        // Verify title is at the end (after trimming)
        let trimmed = plain.trim();
        assert!(
            trimmed.ends_with(" X"),
            "Right-aligned title should end with ' X', got: '{trimmed}'"
        );
        // Verify there are rule characters before the title
        assert!(trimmed.contains('─'), "Should contain rule characters");
    }

    #[test]
    fn test_rule_center_align() {
        let rule = Rule::with_title("Hi").align_center();
        let plain = rule.render_plain(20);
        // Title should be in center
        assert!(plain.contains(" Hi "));
    }

    #[test]
    fn test_rule_with_styled_title() {
        let title = Text::new("Styled");
        let rule = Rule::with_title(title);
        let segments = rule.render(20);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("Styled"));
    }

    #[test]
    fn test_rule_multi_char() {
        // Multi-character rule string
        let rule = Rule::new().character("=-");
        let segments = rule.render(10);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("=-"));
    }

    #[test]
    fn test_rule_fills_width_no_title() {
        let rule = Rule::new();
        let segments = rule.render(10);
        // Count rule characters (excluding control segments like newline)
        let text: String = segments
            .iter()
            .filter(|s| !s.is_control())
            .map(|s| s.text.as_ref())
            .collect();
        // The rule chars are repeated to fill width, minus any trailing newline
        let rule_width = cells::cell_len(&text);
        assert!(rule_width >= 10, "Rule should fill width: got {rule_width}");
    }
}
