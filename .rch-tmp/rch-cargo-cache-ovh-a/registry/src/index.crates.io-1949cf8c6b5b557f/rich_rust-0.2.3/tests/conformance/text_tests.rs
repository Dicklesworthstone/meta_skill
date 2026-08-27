//! Text rendering conformance tests.

use super::{TestCase, strip_ansi};
use rich_rust::markup;
use rich_rust::segment::Segment;
use rich_rust::style::Style;
use rich_rust::text::Text;

/// Test case for basic text rendering with markup.
#[derive(Debug)]
pub struct MarkupTextTest {
    pub name: &'static str,
    pub markup: &'static str,
    pub width: usize,
}

impl TestCase for MarkupTextTest {
    fn name(&self) -> &str {
        self.name
    }

    fn render(&self) -> Vec<Segment<'static>> {
        let text = markup::render_or_plain(self.markup);
        text.render("")
            .into_iter()
            .map(Segment::into_owned)
            .collect()
    }

    fn python_rich_code(&self) -> Option<String> {
        Some(format!(
            r#"from rich.console import Console
from rich.text import Text

console = Console(force_terminal=True, width={})
text = Text.from_markup("{}")
console.print(text, end="")"#,
            self.width,
            self.markup.replace('"', r#"\""#)
        ))
    }
}

/// Test case for Text with explicit styling.
#[derive(Debug)]
pub struct StyledTextTest {
    pub name: &'static str,
    pub text: &'static str,
    pub style_str: &'static str,
    pub start: usize,
    pub end: usize,
}

impl TestCase for StyledTextTest {
    fn name(&self) -> &str {
        self.name
    }

    fn render(&self) -> Vec<Segment<'static>> {
        let mut text = Text::new(self.text);
        if let Ok(style) = Style::parse(self.style_str) {
            text.stylize(self.start, self.end, style);
        }
        text.render("")
            .into_iter()
            .map(Segment::into_owned)
            .collect()
    }

    fn python_rich_code(&self) -> Option<String> {
        Some(format!(
            r#"from rich.console import Console
from rich.text import Text
from rich.style import Style

console = Console(force_terminal=True, width=80)
text = Text("{}")
text.stylize({}, {}, "{}")
console.print(text, end="")"#,
            self.text, self.start, self.end, self.style_str
        ))
    }
}

/// Standard text test cases for conformance testing.
pub fn standard_text_tests() -> Vec<Box<dyn TestCase>> {
    vec![
        Box::new(MarkupTextTest {
            name: "plain_text",
            markup: "Hello, World!",
            width: 80,
        }),
        Box::new(MarkupTextTest {
            name: "bold_text",
            markup: "[bold]Bold text[/]",
            width: 80,
        }),
        Box::new(MarkupTextTest {
            name: "italic_text",
            markup: "[italic]Italic text[/]",
            width: 80,
        }),
        Box::new(MarkupTextTest {
            name: "bold_italic",
            markup: "[bold italic]Bold and italic[/]",
            width: 80,
        }),
        Box::new(MarkupTextTest {
            name: "colored_text",
            markup: "[red]Red[/] and [green]Green[/]",
            width: 80,
        }),
        Box::new(MarkupTextTest {
            name: "nested_styles",
            markup: "[bold]Bold [italic]and italic[/italic] text[/bold]",
            width: 80,
        }),
        Box::new(MarkupTextTest {
            name: "background_color",
            markup: "[white on red]White on red[/]",
            width: 80,
        }),
        Box::new(StyledTextTest {
            name: "styled_range",
            text: "Hello, World!",
            style_str: "bold underline",
            start: 0,
            end: 5,
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conformance::run_test;

    #[test]
    fn test_plain_text() {
        let test = MarkupTextTest {
            name: "plain_text",
            markup: "Hello, World!",
            width: 80,
        };
        let output = run_test(&test);
        assert_eq!(output, "Hello, World!");
    }

    #[test]
    fn test_bold_text() {
        let test = MarkupTextTest {
            name: "bold_text",
            markup: "[bold]Bold text[/]",
            width: 80,
        };
        let output = run_test(&test);
        assert_eq!(strip_ansi(&output), "Bold text");
    }

    #[test]
    fn test_all_standard_text_tests() {
        for test in standard_text_tests() {
            let output = run_test(test.as_ref());
            assert!(
                !output.is_empty(),
                "Test '{}' produced empty output",
                test.name()
            );
        }
    }

    // =========================================================================
    // Markup Parsing Tests - Text::new vs markup::render_or_plain
    // =========================================================================

    #[test]
    fn test_text_new_does_not_parse_markup() {
        let text = Text::new("[bold]Hello[/]");
        let output: String = text
            .render("")
            .into_iter()
            .map(|s| s.text.into_owned())
            .collect();

        assert!(
            output.contains("[bold]"),
            "Text::new should preserve literal markup"
        );
        assert!(
            output.contains("[/]"),
            "Text::new should preserve literal markup close tag"
        );
        assert!(
            text.spans().is_empty(),
            "Text::new should not create styled spans"
        );
    }

    #[test]
    fn test_markup_render_parses_markup() {
        let text = markup::render_or_plain("[bold]Hello[/]");
        assert_eq!(text.plain(), "Hello");
        assert!(!text.spans().is_empty(), "Markup should create spans");

        let output: String = text
            .render("")
            .into_iter()
            .map(|s| s.text.into_owned())
            .collect();
        assert!(
            !output.contains("[bold]"),
            "Parsed markup should not include raw tags"
        );
    }
}
