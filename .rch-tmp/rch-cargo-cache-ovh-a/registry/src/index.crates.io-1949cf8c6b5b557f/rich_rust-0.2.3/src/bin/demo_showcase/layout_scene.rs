//! Layout/composition scene for demo_showcase.
//!
//! Demonstrates rich_rust layout and composition capabilities:
//! - Columns for side-by-side content
//! - Align for horizontal positioning
//! - Padding for spacing and "card" feel

use std::sync::Arc;

use rich_rust::cells::cell_len;
use rich_rust::console::Console;
use rich_rust::markup;
use rich_rust::renderables::align::{Align, AlignMethod};
use rich_rust::renderables::columns::Columns;
use rich_rust::renderables::padding::{Padding, PaddingDimensions};
use rich_rust::renderables::panel::Panel;
use rich_rust::segment::{Segment, split_lines};
use rich_rust::style::Style;

use crate::Config;
use crate::scenes::{Scene, SceneError};

/// Layout/composition scene: demonstrates Columns, Align, and Padding.
pub struct LayoutScene;

impl LayoutScene {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Scene for LayoutScene {
    fn name(&self) -> &'static str {
        "layout"
    }

    fn summary(&self) -> &'static str {
        "Layout tools: Columns, Align, and Padding for polished UI composition."
    }

    fn run(&self, console: &Arc<Console>, cfg: &Config) -> Result<(), SceneError> {
        console.print("[section.title]Layout & Composition: Building Polished UIs[/]");
        console.print("");
        console.print("[dim]Combine Columns, Align, and Padding for professional layouts.[/]");
        console.print("");

        // Demo 1: Alignment showcase
        render_alignment_demo(console);

        console.print("");

        // Demo 2: Columns layout
        render_columns_demo(console);

        console.print("");

        // Demo 3: Padding for card-like containers
        render_padding_demo(console);

        console.print("");

        // Demo 4: Practical composition example
        render_composition_demo(console, cfg);

        Ok(())
    }
}

/// Render alignment demonstration.
fn render_alignment_demo(console: &Console) {
    console.print("[brand.accent]Horizontal Alignment[/]");
    console.print("");

    let width = 50;

    // Left aligned (default)
    let left_segments = Align::from_str("Left-aligned text", width).left().render();
    let left_panel = Panel::new(split_lines(left_segments.into_iter()))
        .width(width + 2)
        .padding(0)
        .border_style(Style::parse("dim").unwrap());
    console.print_renderable(&left_panel);

    // Center aligned
    let center_segments = Align::from_str("Centered text", width).center().render();
    let center_panel = Panel::new(split_lines(center_segments.into_iter()))
        .width(width + 2)
        .padding(0)
        .border_style(Style::parse("dim").unwrap());
    console.print_renderable(&center_panel);

    // Right aligned
    let right_segments = Align::from_str("Right-aligned text", width)
        .right()
        .render();
    let right_panel = Panel::new(split_lines(right_segments.into_iter()))
        .width(width + 2)
        .padding(0)
        .border_style(Style::parse("dim").unwrap());
    console.print_renderable(&right_panel);

    console.print("");

    // Centered hero block - use console width for proper centering
    let hero_lines = [
        "[bold cyan]Nebula Deploy[/]",
        "[dim]Production-ready in minutes[/]",
    ];

    let width = console.width().min(100); // Cap at 100 for readability
    for line in hero_lines {
        let aligned = Align::from_str(line, width).center().render();
        let text: String = aligned.iter().map(|s| s.text.as_ref()).collect();
        console.print(&text);
    }

    console.print("");
    console.print(
        "[hint]Align wraps content to position it left, center, or right within a width.[/]",
    );
}

/// Render columns layout demonstration.
fn render_columns_demo(console: &Console) {
    console.print("[brand.accent]Multi-Column Layout[/]");
    console.print("");

    // Feature cards in columns
    let features = [
        "Tables", "Panels", "Trees", "Progress", "Syntax", "Markdown",
    ];

    // Use max_width to limit expansion on very wide terminals (300+ columns)
    // while still allowing a polished look with some expansion
    let cols = Columns::from_strings(&features)
        .column_count(3)
        .gutter(4)
        .equal_width(true)
        .align(AlignMethod::Center)
        .max_width(100);

    console.print_renderable(&cols);
    console.print("");

    // Descriptive cards (longer content)
    let cards = [
        "Tables: Structured data",
        "Panels: Bordered content",
        "Trees: Hierarchical views",
        "Progress: Live updates",
    ];

    // Use max_width to limit expansion on very wide terminals
    let card_cols = Columns::from_strings(&cards)
        .column_count(2)
        .gutter(4)
        .equal_width(true)
        .max_width(100);

    console.print_renderable(&card_cols);

    console.print("");
    console.print(
        "[hint]Columns arrange items in newspaper-style layout with configurable gutters.[/]",
    );
}

/// Render padding demonstration.
fn render_padding_demo(console: &Console) {
    console.print("[brand.accent]Padding for Visual Hierarchy[/]");
    console.print("");

    // Show different padding styles
    console
        .print("[dim]CSS-style padding: (vertical, horizontal) or (top, right, bottom, left)[/]");
    console.print("");

    // No padding
    let content_no_pad = vec![vec![Segment::new("No padding", None)]];
    let no_pad = Padding::new(content_no_pad, PaddingDimensions::zero(), 20);
    let no_pad_panel = Panel::new(no_pad.render())
        .width(22)
        .padding(0)
        .border_style(Style::parse("dim").unwrap());
    console.print_renderable(&no_pad_panel);

    // Symmetric padding
    let content_sym = vec![vec![Segment::new("Padding (1, 2)", None)]];
    let sym_pad = Padding::new(content_sym, (1, 2), 24);
    let sym_pad_panel = Panel::new(sym_pad.render())
        .width(26)
        .padding(0)
        .border_style(Style::parse("dim").unwrap());
    console.print_renderable(&sym_pad_panel);

    console.print("");

    // Card-like padding with background
    let title_text = markup::render_or_plain("[bold]Feature Card[/]");
    let title_segments: Vec<Segment> = title_text
        .render("")
        .into_iter()
        .map(Segment::into_owned)
        .collect();
    let content_card = vec![
        title_segments,
        vec![Segment::new("", None)],
        vec![Segment::new("Add spacing and structure", None)],
        vec![Segment::new("to make content stand out.", None)],
    ];
    let card_pad = Padding::new(content_card, (1, 3), 40);
    let card_pad_panel = Panel::new(card_pad.render())
        .width(42)
        .padding(0)
        .border_style(Style::parse("dim").unwrap());
    console.print_renderable(&card_pad_panel);

    console.print("");
    console.print("[hint]Padding creates breathing room around content for a polished look.[/]");
}

/// Render panels side by side by combining their rendered lines horizontally.
fn render_panels_side_by_side(
    console: &Console,
    panels: &[Panel<'_>],
    panel_width: usize,
    gutter: usize,
) {
    // Render each panel to plain text and split into lines
    let rendered: Vec<Vec<String>> = panels
        .iter()
        .map(|p| {
            let text = p.render_plain(panel_width);
            text.lines().map(String::from).collect()
        })
        .collect();

    // Find max height (number of lines)
    let max_lines = rendered.iter().map(|r| r.len()).max().unwrap_or(0);

    // Find width of each panel (by measuring actual rendered content)
    let widths: Vec<usize> = rendered
        .iter()
        .map(|lines| lines.iter().map(|l| cell_len(l)).max().unwrap_or(0))
        .collect();

    // Build combined lines
    for line_idx in 0..max_lines {
        let mut combined = String::new();
        for (panel_idx, lines) in rendered.iter().enumerate() {
            if panel_idx > 0 {
                combined.push_str(&" ".repeat(gutter));
            }
            let line = lines.get(line_idx).map(|s| s.as_str()).unwrap_or("");
            let line_width = cell_len(line);
            let this_panel_width = widths[panel_idx];
            combined.push_str(line);
            // Pad to panel width for alignment
            if line_width < this_panel_width {
                combined.push_str(&" ".repeat(this_panel_width - line_width));
            }
        }
        console.print(&combined);
    }
}

/// Render practical composition demonstration.
fn render_composition_demo(console: &Console, cfg: &Config) {
    console.print("[brand.accent]Composition: Putting It Together[/]");
    console.print("");

    // Create a multi-card layout using panels side by side
    let card1_content = markup::render_or_plain(
        "[bold green]Production[/]\n\n\
         Status: Healthy\n\
         Uptime: 99.9%\n\
         Latency: 12ms",
    );
    let card1 = Panel::from_rich_text(&card1_content, 26)
        .title_from_markup("[green]us-west-2[/]")
        .width(28)
        .safe_box(cfg.is_safe_box());

    let card2_content = markup::render_or_plain(
        "[bold green]Production[/]\n\n\
         Status: Healthy\n\
         Uptime: 99.8%\n\
         Latency: 45ms",
    );
    let card2 = Panel::from_rich_text(&card2_content, 26)
        .title_from_markup("[green]eu-west-1[/]")
        .width(28)
        .safe_box(cfg.is_safe_box());

    let card3_content = markup::render_or_plain(
        "[bold yellow]Degraded[/]\n\n\
         Status: Elevated\n\
         Uptime: 98.5%\n\
         Latency: 120ms",
    );
    let card3 = Panel::from_rich_text(&card3_content, 26)
        .title_from_markup("[yellow]ap-south-1[/]")
        .width(28)
        .safe_box(cfg.is_safe_box());

    // Render cards side by side
    render_panels_side_by_side(console, &[card1, card2, card3], 28, 2);

    console.print("");

    // Centered summary
    let summary = Align::from_str(
        "[bold]3 regions | 99.4% avg uptime | 59ms avg latency[/]",
        80,
    )
    .center()
    .render();
    let summary_text: String = summary.iter().map(|s| s.text.as_ref()).collect();
    console.print(&summary_text);

    console.print("");
    console.print("[hint]Combine layout primitives to create dashboard-quality output.[/]");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_scene_has_correct_name() {
        let scene = LayoutScene::new();
        assert_eq!(scene.name(), "layout");
    }

    #[test]
    fn layout_scene_runs_without_error() {
        let scene = LayoutScene::new();
        let console = Console::builder()
            .force_terminal(false)
            .markup(true)
            .build()
            .shared();
        let cfg = Config::with_defaults();

        let result = scene.run(&console, &cfg);
        assert!(result.is_ok());
    }

    /// Test that layout scene doesn't have excessive whitespace on wide terminals.
    /// Regression test for bd-2tvq: columns shouldn't stretch across 400+ columns.
    #[test]
    fn layout_scene_no_excessive_whitespace_on_wide_terminal() {
        let scene = LayoutScene::new();
        let console = Console::builder()
            .force_terminal(false)
            .markup(true)
            .width(400) // Very wide terminal
            .build()
            .shared();
        let cfg = Config::with_defaults();

        console.begin_capture();
        let result = scene.run(&console, &cfg);
        assert!(result.is_ok());

        let segments = console.end_capture();
        let output: String = segments.iter().map(|s| s.text.as_ref()).collect();

        // Track which section we're in
        // Skip sections that intentionally demonstrate padding/alignment with fixed widths
        let mut in_multi_column_section = false;
        let mut excessive_found = false;

        for line in output.lines() {
            // Detect section changes
            if line.contains("Multi-Column Layout") {
                in_multi_column_section = true;
                continue;
            }
            if line.contains("Padding for Visual Hierarchy") {
                in_multi_column_section = false;
                continue;
            }

            // Only check the multi-column section for excessive whitespace
            // The alignment demo intentionally uses fixed-width padding
            if in_multi_column_section && !line.trim().is_empty() {
                let mut space_count = 0;
                for ch in line.chars() {
                    if ch == ' ' {
                        space_count += 1;
                        // 50+ consecutive spaces indicates columns stretched too much
                        if space_count >= 50 {
                            excessive_found = true;
                            break;
                        }
                    } else {
                        space_count = 0;
                    }
                }
                // Early exit once we find a problem
                if excessive_found {
                    break;
                }
            }
        }

        assert!(
            !excessive_found,
            "Multi-column section has excessive whitespace (50+ spaces between columns)"
        );
    }
}
