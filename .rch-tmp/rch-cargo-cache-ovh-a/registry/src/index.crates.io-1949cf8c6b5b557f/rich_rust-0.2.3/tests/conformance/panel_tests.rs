//! Panel markup parsing conformance tests.
//!
//! These tests verify Panel behavior with markup in titles and content.
//!
//! **Important Note on Panel Markup Behavior:**
//!
//! Panel does NOT automatically parse Rich markup syntax in most methods:
//! - `Panel::from_text("...")` - Does NOT parse markup, treats as plain text
//! - `Panel::title("...")` - Does NOT parse markup (uses Text::new internally)
//! - `Panel::subtitle("...")` - Does NOT parse markup
//!
//! To use styled content, you must:
//! 1. Create a `Text` object with explicit spans/styles
//! 2. Use `Console` to render markup to segments first
//! 3. Use `Panel::from_rich_text()` for pre-styled Text objects
//!
//! This is consistent with the Cell API behavior documented in bd-2llx.

use super::TestCase;
use rich_rust::prelude::*;
use rich_rust::renderables::panel::Panel;
use rich_rust::segment::Segment;
use rich_rust::text::Text;

/// Test case for Panel rendering.
#[derive(Debug)]
pub struct PanelTest {
    pub name: &'static str,
    pub content: &'static str,
    pub title: Option<&'static str>,
    pub subtitle: Option<&'static str>,
    pub width: usize,
}

impl TestCase for PanelTest {
    fn name(&self) -> &str {
        self.name
    }

    fn render(&self) -> Vec<Segment<'static>> {
        let mut panel = Panel::from_text(self.content).width(self.width);

        if let Some(title) = self.title {
            panel = panel.title(title);
        }

        if let Some(subtitle) = self.subtitle {
            panel = panel.subtitle(subtitle);
        }

        panel.render(self.width)
    }

    fn python_rich_code(&self) -> Option<String> {
        let mut code = format!(
            r#"from rich.console import Console
from rich.panel import Panel

console = Console(force_terminal=True, width={})
panel = Panel("{}"{}{})"#,
            self.width,
            self.content,
            self.title
                .map(|t| format!(", title=\"{}\"", t))
                .unwrap_or_default(),
            self.subtitle
                .map(|s| format!(", subtitle=\"{}\"", s))
                .unwrap_or_default()
        );
        code.push_str("\nconsole.print(panel, end=\"\")");
        Some(code)
    }
}

/// Standard panel test cases for conformance testing.
pub fn standard_panel_tests() -> Vec<Box<dyn TestCase>> {
    vec![
        Box::new(PanelTest {
            name: "panel_simple",
            content: "Hello World",
            title: None,
            subtitle: None,
            width: 30,
        }),
        Box::new(PanelTest {
            name: "panel_with_title",
            content: "Content here",
            title: Some("My Title"),
            subtitle: None,
            width: 30,
        }),
        Box::new(PanelTest {
            name: "panel_with_subtitle",
            content: "Body text",
            title: None,
            subtitle: Some("Footer"),
            width: 30,
        }),
        Box::new(PanelTest {
            name: "panel_full",
            content: "Main content",
            title: Some("Header"),
            subtitle: Some("Footer"),
            width: 40,
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conformance::run_test;

    // =========================================================================
    // Basic Panel Tests
    // =========================================================================

    #[test]
    fn test_panel_simple() {
        let test = PanelTest {
            name: "panel_simple",
            content: "Hello World",
            title: None,
            subtitle: None,
            width: 30,
        };
        let output = run_test(&test);
        assert!(output.contains("Hello World"), "Content should be present");
    }

    #[test]
    fn test_panel_with_title() {
        let test = PanelTest {
            name: "panel_with_title",
            content: "Content",
            title: Some("Title"),
            subtitle: None,
            width: 30,
        };
        let output = run_test(&test);
        assert!(output.contains("Title"), "Title should be present");
        assert!(output.contains("Content"), "Content should be present");
    }

    #[test]
    fn test_panel_with_subtitle() {
        let test = PanelTest {
            name: "panel_with_subtitle",
            content: "Content",
            title: None,
            subtitle: Some("Footer"),
            width: 30,
        };
        let output = run_test(&test);
        assert!(output.contains("Footer"), "Subtitle should be present");
        assert!(output.contains("Content"), "Content should be present");
    }

    // =========================================================================
    // Markup Parsing Tests - Title
    // =========================================================================

    #[test]
    fn test_panel_title_does_not_parse_markup() {
        // Panel::title() takes impl Into<Text>, which converts strings via Text::new().
        // Text::new() does NOT parse markup - this is by design (consistent with Cell::new()).
        // Therefore, markup tags appear literally in the output.
        let panel = Panel::from_text("Content")
            .title("[bold]Styled Title[/]")
            .width(50);

        let output: String = panel
            .render(50)
            .into_iter()
            .map(|s| s.text.into_owned())
            .collect();

        // The raw markup tags should be present (NOT parsed)
        assert!(
            output.contains("[bold]"),
            "Raw [bold] tag should appear literally (not parsed)"
        );
        assert!(
            output.contains("[/]"),
            "Raw [/] tag should appear literally (not parsed)"
        );
        assert!(
            output.contains("Styled Title"),
            "Title text should be present"
        );
    }

    #[test]
    fn test_panel_title_with_prestyled_text_preserves_styles() {
        // To get styled titles, create a Text object with explicit styles.
        // This is the intended pattern for styled Panel titles.
        let mut styled_title = Text::new("Styled Title");
        styled_title.stylize(0, 6, Style::new().bold()); // "Styled" in bold

        let panel = Panel::from_text("Content").title(styled_title).width(50);

        let segments: Vec<Segment<'static>> = panel.render(50);

        // Find the title segment
        let title_segment = segments
            .iter()
            .find(|seg| seg.text.contains("Styled"))
            .expect("Title segment should exist");

        // The style should be preserved
        let style = title_segment.style.as_ref().expect("Should have style");
        assert!(
            style.attributes.contains(Attributes::BOLD),
            "Title should have bold attribute from explicit styling"
        );

        // Should NOT contain raw markup tags
        let output: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(
            !output.contains("[bold]"),
            "Should not contain raw [bold] tag"
        );
    }

    // =========================================================================
    // Markup Parsing Tests - Subtitle
    // =========================================================================

    #[test]
    fn test_panel_subtitle_does_not_parse_markup() {
        let panel = Panel::from_text("Content")
            .subtitle("[italic]Footer[/]")
            .width(50);

        let output: String = panel
            .render(50)
            .into_iter()
            .map(|s| s.text.into_owned())
            .collect();

        // Raw markup should appear literally
        assert!(
            output.contains("[italic]"),
            "Raw [italic] tag should appear literally"
        );
        assert!(
            output.contains("[/]"),
            "Raw [/] tag should appear literally"
        );
    }

    #[test]
    fn test_panel_subtitle_with_prestyled_text() {
        let mut styled = Text::new("Footer");
        styled.stylize(0, 6, Style::new().italic());

        let panel = Panel::from_text("Content").subtitle(styled).width(50);
        let segments: Vec<Segment<'static>> = panel.render(50);

        // Find footer segment
        let footer_segment = segments
            .iter()
            .find(|seg| seg.text.contains("Footer"))
            .expect("Footer segment should exist");

        let style = footer_segment.style.as_ref().expect("Should have style");
        assert!(
            style.attributes.contains(Attributes::ITALIC),
            "Footer should have italic attribute"
        );
    }

    // =========================================================================
    // Markup Parsing Tests - Content
    // =========================================================================

    #[test]
    fn test_panel_from_text_does_not_parse_markup() {
        // Panel::from_text() uses Segment::new() for each line, which doesn't parse markup.
        let panel = Panel::from_text("[bold]Content[/]").width(50);

        let output: String = panel
            .render(50)
            .into_iter()
            .map(|s| s.text.into_owned())
            .collect();

        // Raw markup should appear
        assert!(
            output.contains("[bold]"),
            "Raw [bold] tag should appear in content"
        );
        assert!(
            output.contains("[/]"),
            "Raw [/] tag should appear in content"
        );
    }

    #[test]
    fn test_panel_from_rich_text_preserves_styles() {
        // Use Panel::from_rich_text() with a pre-styled Text object
        let mut text = Text::new("Bold Content");
        text.stylize(0, 4, Style::new().bold()); // "Bold" in bold

        // Render directly without collecting to Vec<Segment<'static>>
        let panel = Panel::from_rich_text(&text, 50);
        let segments = panel.render(50);

        // Find content segment
        let content_segment = segments
            .iter()
            .find(|seg| seg.text.contains("Bold"))
            .expect("Content segment should exist");

        let style = content_segment.style.as_ref().expect("Should have style");
        assert!(
            style.attributes.contains(Attributes::BOLD),
            "Content should have bold attribute from explicit styling"
        );
    }

    // =========================================================================
    // ANSI Code Verification Tests
    // =========================================================================

    #[test]
    fn test_panel_with_border_style_has_style_in_segments() {
        let panel = Panel::from_text("Content")
            .border_style(Style::new().color(Color::parse("red").unwrap()))
            .width(30);

        let segments = panel.render(30);

        // Check that border segments have red color style
        let has_colored_border = segments.iter().any(|seg| {
            if let Some(ref style) = seg.style {
                style.color.is_some()
            } else {
                false
            }
        });

        assert!(
            has_colored_border,
            "Border segments should have color style applied"
        );
    }

    #[test]
    fn test_panel_with_styled_title_has_ansi_codes() {
        let mut styled_title = Text::new("Title");
        styled_title.stylize(0, 5, Style::new().bold());

        let panel = Panel::from_text("Content").title(styled_title).width(30);
        let segments: Vec<Segment<'static>> = panel.render(30);

        // Check that at least one segment has bold attribute
        let has_bold = segments.iter().any(|seg| {
            seg.style
                .as_ref()
                .map(|s| s.attributes.contains(Attributes::BOLD))
                .unwrap_or(false)
        });

        assert!(has_bold, "Should have bold style in segments");
    }

    // =========================================================================
    // Raw Markup Absence Tests
    // =========================================================================

    #[test]
    fn test_panel_parsed_content_has_no_raw_markup() {
        // When using pre-styled Text, no raw markup should appear
        let mut text = Text::new("Styled Content");
        text.stylize(0, 6, Style::new().bold());

        let panel = Panel::from_rich_text(&text, 50);
        let output: String = panel
            .render(50)
            .into_iter()
            .map(|s| s.text.into_owned())
            .collect();

        // Verify common markup patterns are absent
        let markup_patterns = ["[bold]", "[/bold]", "[italic]", "[/italic]", "[red]", "[/]"];

        for pattern in markup_patterns {
            assert!(
                !output.contains(pattern),
                "Pre-styled content should not contain raw markup: {}",
                pattern
            );
        }
    }

    // =========================================================================
    // Nested Markup Styles Test
    // =========================================================================

    #[test]
    fn test_panel_nested_styles() {
        // Create text with overlapping styles
        let mut text = Text::new("Hello World");
        text.stylize(0, 5, Style::new().bold()); // "Hello" bold
        text.stylize(6, 11, Style::new().italic()); // "World" italic

        let panel = Panel::from_rich_text(&text, 50);
        let segments = panel.render(50);

        // Check for both styles
        let has_bold = segments.iter().any(|seg| {
            seg.text.contains("Hello")
                && seg
                    .style
                    .as_ref()
                    .map(|s| s.attributes.contains(Attributes::BOLD))
                    .unwrap_or(false)
        });

        let has_italic = segments.iter().any(|seg| {
            seg.text.contains("World")
                && seg
                    .style
                    .as_ref()
                    .map(|s| s.attributes.contains(Attributes::ITALIC))
                    .unwrap_or(false)
        });

        assert!(has_bold, "Should have bold style on 'Hello'");
        assert!(has_italic, "Should have italic style on 'World'");
    }

    // =========================================================================
    // Conformance Test Runner
    // =========================================================================

    #[test]
    fn test_all_standard_panel_tests() {
        for test in standard_panel_tests() {
            let output = run_test(test.as_ref());
            assert!(
                !output.is_empty(),
                "Test '{}' produced empty output",
                test.name()
            );
        }
    }
}
