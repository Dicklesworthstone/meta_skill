//! Progress bar markup parsing conformance tests.
//!
//! These tests verify ProgressBar behavior with markup in descriptions.
//!
//! **Important Note on ProgressBar Markup Behavior:**
//!
//! ProgressBar does NOT automatically parse Rich markup syntax in descriptions:
//! - `ProgressBar::description("...")` - Does NOT parse markup, treats as plain text
//! - `ProgressBar::finished_message("...")` - Takes String, no markup support
//!
//! To use styled descriptions, you must:
//! 1. Create a `Text` object with explicit spans/styles
//! 2. Use `ProgressBar::description(styled_text)` with pre-styled Text
//!
//! This is consistent with the Cell, Panel, and Tree API behavior.

use super::TestCase;
use rich_rust::prelude::*;
use rich_rust::renderables::progress::{BarStyle, ProgressBar, Spinner};
use rich_rust::segment::Segment;
use rich_rust::text::Text;

/// Test case for ProgressBar rendering.
#[derive(Debug)]
pub struct ProgressTest {
    pub name: &'static str,
    pub description: &'static str,
    pub progress: f64,
    pub width: usize,
}

impl TestCase for ProgressTest {
    fn name(&self) -> &str {
        self.name
    }

    fn render(&self) -> Vec<Segment<'static>> {
        let mut bar = ProgressBar::new()
            .description(self.description)
            .width(20)
            .show_brackets(true);
        bar.set_progress(self.progress);
        bar.render(self.width)
    }

    fn python_rich_code(&self) -> Option<String> {
        // Use Python Rich's ProgressBar renderable (not the Progress context manager) so we can
        // compare a single static render.
        let completed = (self.progress * 100.0).round() as i64;
        Some(format!(
            r#"
from rich.console import Console
from rich.progress_bar import ProgressBar

console = Console(width={width}, force_terminal=True, color_system="truecolor")
bar = ProgressBar(total=100, completed={completed}, width=20)
console.print("{desc}", bar)
"#,
            width = self.width,
            completed = completed,
            desc = self.description.replace('"', "\\\""),
        ))
    }
}

/// Standard progress test cases for conformance testing.
pub fn standard_progress_tests() -> Vec<Box<dyn TestCase>> {
    vec![
        Box::new(ProgressTest {
            name: "progress_simple",
            description: "Downloading",
            progress: 0.5,
            width: 80,
        }),
        Box::new(ProgressTest {
            name: "progress_complete",
            description: "Done",
            progress: 1.0,
            width: 80,
        }),
        Box::new(ProgressTest {
            name: "progress_start",
            description: "Starting",
            progress: 0.0,
            width: 80,
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conformance::run_test;

    // =========================================================================
    // Basic Progress Tests
    // =========================================================================

    #[test]
    fn test_progress_simple() {
        let test = ProgressTest {
            name: "progress_simple",
            description: "Downloading",
            progress: 0.5,
            width: 80,
        };
        let output = run_test(&test);
        assert!(
            output.contains("Downloading"),
            "Description should be present"
        );
        assert!(output.contains('%'), "Percentage should be present");
    }

    #[test]
    fn test_progress_with_various_levels() {
        for progress in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let mut bar = ProgressBar::new().width(20);
            bar.set_progress(progress);
            let output: String = bar
                .render(80)
                .into_iter()
                .map(|s| s.text.into_owned())
                .collect();
            assert!(
                !output.is_empty(),
                "Progress at {:.0}% should render",
                progress * 100.0
            );
        }
    }

    // =========================================================================
    // Markup Parsing Tests - Description
    // =========================================================================

    #[test]
    fn test_progress_description_does_not_parse_markup() {
        // ProgressBar::description() takes impl Into<Text>, which converts strings via Text::new().
        // Text::new() does NOT parse markup - this is by design.
        // Therefore, markup tags appear literally in the output.
        let bar = ProgressBar::new()
            .description("[bold]Downloading[/]")
            .width(20);

        let output: String = bar
            .render(80)
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
            output.contains("Downloading"),
            "Description text should be present"
        );
    }

    #[test]
    fn test_progress_description_with_color_markup_not_parsed() {
        let bar = ProgressBar::new().description("[red]Error[/red]").width(20);

        let output: String = bar
            .render(80)
            .into_iter()
            .map(|s| s.text.into_owned())
            .collect();

        assert!(
            output.contains("[red]"),
            "Raw [red] tag should appear literally"
        );
        assert!(
            output.contains("[/red]"),
            "Raw [/red] tag should appear literally"
        );
    }

    #[test]
    fn test_progress_description_with_prestyled_text_preserves_styles() {
        // To get styled descriptions, create a Text object with explicit styles.
        // This is the intended pattern for styled ProgressBar descriptions.
        let mut styled_desc = Text::new("Downloading");
        styled_desc.stylize(0, 11, Style::new().bold()); // "Downloading" in bold

        let bar = ProgressBar::new().description(styled_desc).width(20);
        let segments: Vec<Segment<'_>> = bar.render(80);

        // Find the description segment
        let desc_segment = segments
            .iter()
            .find(|seg| seg.text.contains("Downloading"))
            .expect("Description segment should exist");

        // The style should be preserved
        let style = desc_segment.style.as_ref().expect("Should have style");
        assert!(
            style.attributes.contains(Attributes::BOLD),
            "Description should have bold attribute from explicit styling"
        );

        // Should NOT contain raw markup tags
        let output: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(
            !output.contains("[bold]"),
            "Should not contain raw [bold] tag"
        );
    }

    #[test]
    fn test_progress_description_with_colored_prestyled_text() {
        let mut styled_desc = Text::new("Warning");
        styled_desc.stylize(0, 7, Style::new().color(Color::parse("yellow").unwrap()));

        let bar = ProgressBar::new().description(styled_desc).width(20);
        let segments = bar.render(80);

        // Find the description segment
        let desc_segment = segments
            .iter()
            .find(|seg| seg.text.contains("Warning"))
            .expect("Description segment should exist");

        let style = desc_segment.style.as_ref().expect("Should have style");
        assert!(style.color.is_some(), "Description should have color");
    }

    // =========================================================================
    // Finished Message Tests
    // =========================================================================

    #[test]
    fn test_progress_finished_message_is_plain_string() {
        // finished_message takes impl Into<String>, not impl Into<Text>
        // So it cannot support styled text at all.
        let mut bar = ProgressBar::new()
            .finished_message("[bold]Complete[/bold]")
            .width(20);
        bar.finish();

        let output: String = bar
            .render(80)
            .into_iter()
            .map(|s| s.text.into_owned())
            .collect();

        // The markup should appear literally since it's just a plain string
        assert!(
            output.contains("[bold]"),
            "Finished message is plain string, markup appears literally"
        );
        assert!(
            output.contains("Complete"),
            "Message text should be present"
        );
    }

    #[test]
    fn test_progress_finished_shows_checkmark() {
        let mut bar = ProgressBar::new().finished_message("Done").width(20);
        bar.finish();

        let output: String = bar
            .render(80)
            .into_iter()
            .map(|s| s.text.into_owned())
            .collect();

        assert!(output.contains('✓'), "Finished bar should show checkmark");
        assert!(
            output.contains("Done"),
            "Finished message should be present"
        );
    }

    // =========================================================================
    // Bar Style Tests
    // =========================================================================

    #[test]
    fn test_progress_bar_styles() {
        let styles = [
            BarStyle::Ascii,
            BarStyle::Block,
            BarStyle::Line,
            BarStyle::Dots,
            BarStyle::Gradient,
        ];

        for style in styles {
            let mut bar = ProgressBar::new().bar_style(style).width(20);
            bar.set_progress(0.5);
            let segments = bar.render(80);
            assert!(!segments.is_empty(), "Style {:?} should render", style);
        }
    }

    #[test]
    fn test_progress_completed_style_applied() {
        let mut bar = ProgressBar::new()
            .completed_style(Style::new().color(Color::parse("green").unwrap()))
            .width(20);
        bar.set_progress(0.5);

        let segments = bar.render(80);

        // Check that completed portion has green color
        let has_green = segments.iter().any(|seg| {
            if let Some(ref style) = seg.style {
                style.color.is_some()
            } else {
                false
            }
        });

        assert!(has_green, "Completed portion should have color style");
    }

    #[test]
    fn test_progress_remaining_style_applied() {
        let mut bar = ProgressBar::new()
            .remaining_style(Style::new().color(Color::parse("gray").unwrap()))
            .width(20);
        bar.set_progress(0.5);

        let segments = bar.render(80);

        // Should have styled segments for the remaining portion
        let has_styled = segments
            .iter()
            .any(|seg| seg.style.as_ref().is_some_and(|s| s.color.is_some()));

        assert!(has_styled, "Remaining portion should have style");
    }

    // =========================================================================
    // Spinner Tests
    // =========================================================================

    #[test]
    fn test_spinner_render_has_style() {
        let spinner = Spinner::dots().style(Style::new().color(Color::parse("cyan").unwrap()));
        let segment = spinner.render();

        assert!(
            segment.style.as_ref().is_some_and(|s| s.color.is_some()),
            "Spinner should have color style"
        );
    }

    #[test]
    fn test_spinner_variants() {
        let spinners = [
            Spinner::dots(),
            Spinner::line(),
            Spinner::simple(),
            Spinner::bounce(),
            Spinner::growing(),
            Spinner::moon(),
            Spinner::clock(),
        ];

        for spinner in spinners {
            let segment = spinner.render();
            assert!(!segment.text.is_empty(), "Spinner should render a frame");
        }
    }

    // =========================================================================
    // ANSI Code Verification Tests
    // =========================================================================

    #[test]
    fn test_progress_with_styled_description_has_style_in_segments() {
        let mut styled_desc = Text::new("Task");
        styled_desc.stylize(0, 4, Style::new().bold());

        let bar = ProgressBar::new().description(styled_desc).width(20);
        let segments = bar.render(80);

        // Check that at least one segment has bold attribute
        let has_bold = segments.iter().any(|seg| {
            seg.style
                .as_ref()
                .is_some_and(|s| s.attributes.contains(Attributes::BOLD))
        });

        assert!(has_bold, "Should have bold style in segments");
    }

    #[test]
    fn test_progress_bar_portion_styles_in_segments() {
        let mut bar = ProgressBar::new()
            .completed_style(Style::new().color(Color::parse("green").unwrap()))
            .remaining_style(Style::new().color(Color::parse("red").unwrap()))
            .width(20);
        bar.set_progress(0.5);

        let segments = bar.render(80);

        // Should have multiple styled segments for the bar
        let styled_count = segments
            .iter()
            .filter(|seg| seg.style.as_ref().is_some_and(|s| s.color.is_some()))
            .count();

        assert!(
            styled_count >= 2,
            "Should have at least 2 styled segments (completed + remaining)"
        );
    }

    // =========================================================================
    // Raw Markup Absence Tests
    // =========================================================================

    #[test]
    fn test_progress_prestyled_description_has_no_raw_markup() {
        // When using pre-styled Text, no raw markup should appear
        let mut text = Text::new("Styled Desc");
        text.stylize(0, 6, Style::new().bold());

        let bar = ProgressBar::new().description(text).width(20);
        let output: String = bar
            .render(80)
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
    // Nested/Complex Style Tests
    // =========================================================================

    #[test]
    fn test_progress_description_with_multiple_styles() {
        // Create text with multiple style regions
        let mut text = Text::new("Bold and Italic");
        text.stylize(0, 4, Style::new().bold()); // "Bold" in bold
        text.stylize(9, 15, Style::new().italic()); // "Italic" in italic

        let bar = ProgressBar::new().description(text).width(20);
        let segments = bar.render(80);

        // Check for bold style
        let has_bold = segments.iter().any(|seg| {
            seg.text.contains("Bold")
                && seg
                    .style
                    .as_ref()
                    .is_some_and(|s| s.attributes.contains(Attributes::BOLD))
        });

        // Check for italic style
        let has_italic = segments.iter().any(|seg| {
            seg.text.contains("Italic")
                && seg
                    .style
                    .as_ref()
                    .is_some_and(|s| s.attributes.contains(Attributes::ITALIC))
        });

        assert!(has_bold, "Should have bold style on 'Bold'");
        assert!(has_italic, "Should have italic style on 'Italic'");
    }

    // =========================================================================
    // Edge Cases
    // =========================================================================

    #[test]
    fn test_progress_narrow_width() {
        let mut bar = ProgressBar::new().description("Long description").width(20);
        bar.set_progress(0.5);

        // Very narrow width - should still render something
        let output: String = bar
            .render(30)
            .into_iter()
            .map(|s| s.text.into_owned())
            .collect();

        assert!(!output.is_empty(), "Should render even with narrow width");
    }

    #[test]
    fn test_progress_too_narrow_for_bar() {
        let mut bar = ProgressBar::new().description("Test").width(20);
        bar.set_progress(0.5);

        // Width so small bar can't fit
        let output: String = bar
            .render(10)
            .into_iter()
            .map(|s| s.text.into_owned())
            .collect();

        // Should still render something (percentage at minimum)
        assert!(!output.is_empty(), "Should render even when bar can't fit");
    }

    #[test]
    fn test_progress_no_description() {
        let mut bar = ProgressBar::new().width(20);
        bar.set_progress(0.5);

        let output: String = bar
            .render(80)
            .into_iter()
            .map(|s| s.text.into_owned())
            .collect();

        assert!(output.contains('['), "Bar should be present");
        assert!(output.contains('%'), "Percentage should be present");
    }

    // =========================================================================
    // Conformance Test Runner
    // =========================================================================

    #[test]
    fn test_all_standard_progress_tests() {
        for test in standard_progress_tests() {
            let output = run_test(test.as_ref());
            assert!(
                !output.is_empty(),
                "Test '{}' produced empty output",
                test.name()
            );
        }
    }
}
