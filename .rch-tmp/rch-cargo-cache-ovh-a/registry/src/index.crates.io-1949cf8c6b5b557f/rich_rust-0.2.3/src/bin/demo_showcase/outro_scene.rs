//! Outro scene for demo_showcase.
//!
//! Wraps up the demo with:
//! - Summary of demonstrated features
//! - "Get Started" resources (crates.io, docs.rs, GitHub)
//! - "What Next" suggestions
//! - Thanks message with branding

use std::sync::Arc;

use rich_rust::cells::cell_len;
use rich_rust::console::Console;
use rich_rust::markup;
use rich_rust::renderables::panel::Panel;
use rich_rust::renderables::rule::Rule;
use rich_rust::renderables::table::{Column, Table};
use rich_rust::renderables::tree::{Tree, TreeGuides, TreeNode};
use rich_rust::style::Style;
use rich_rust::text::Text;

use crate::Config;
use crate::scenes::{Scene, SceneError};

/// Outro scene: summary, resources, thanks.
pub struct OutroScene;

impl OutroScene {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Scene for OutroScene {
    fn name(&self) -> &'static str {
        "outro"
    }

    fn summary(&self) -> &'static str {
        "Summary + next steps."
    }

    fn run(&self, console: &Arc<Console>, _cfg: &Config) -> Result<(), SceneError> {
        // Section divider
        let rule = Rule::with_title("Demo Complete")
            .style(Style::parse("brand.accent").unwrap_or_default());
        console.print_renderable(&rule);
        console.print("");

        // Feature summary table
        render_feature_summary(console);

        console.print("");

        // Get Started resources
        render_get_started(console);

        console.print("");

        // What Next tree
        render_what_next(console);

        console.print("");

        // Thanks message
        render_thanks(console);

        Ok(())
    }
}

/// Calculate padding to center content of given visible width within total width.
fn center_padding(content_visible_width: usize, total_width: usize) -> String {
    if content_visible_width >= total_width {
        return String::new();
    }
    let padding = (total_width - content_visible_width) / 2;
    " ".repeat(padding)
}

/// Render a table summarizing demonstrated features.
fn render_feature_summary(console: &Console) {
    console.print("[section.title]Features Demonstrated[/]");
    console.print("");

    let mut table = Table::new();
    table.add_column(Column::new("Feature").style(Style::parse("bold").unwrap_or_default()));
    table.add_column(Column::new("Capability"));
    table.add_column(Column::new("Status").style(Style::parse("dim").unwrap_or_default()));

    // Core features
    // Note: escape brackets with \[ to show literal markup syntax
    table.add_row_markup([
        "[brand.accent]Markup Syntax[/]",
        "`\\[bold red]text\\[/]` for inline styling",
        "[green]Core[/]",
    ]);
    table.add_row_markup([
        "[brand.accent]Tables[/]",
        "Auto-sizing columns, borders, alignment",
        "[green]Core[/]",
    ]);
    table.add_row_markup([
        "[brand.accent]Panels[/]",
        "Boxed content with titles/subtitles",
        "[green]Core[/]",
    ]);
    table.add_row_markup([
        "[brand.accent]Trees[/]",
        "Hierarchical data with guide styles",
        "[green]Core[/]",
    ]);
    table.add_row_markup([
        "[brand.accent]Progress[/]",
        "Bars, spinners, live updates",
        "[green]Core[/]",
    ]);
    table.add_row_markup([
        "[brand.accent]Layout[/]",
        "Split-screen, ratio-based sizing",
        "[green]Core[/]",
    ]);
    table.add_row_markup([
        "[brand.accent]Rules[/]",
        "Horizontal dividers with titles",
        "[green]Core[/]",
    ]);

    // Optional features
    table.add_row_markup([
        "[brand.accent]Syntax Highlighting[/]",
        "100+ languages via syntect",
        "[cyan]--features syntax[/]",
    ]);
    table.add_row_markup([
        "[brand.accent]Markdown[/]",
        "CommonMark + GFM rendering",
        "[cyan]--features markdown[/]",
    ]);
    table.add_row_markup([
        "[brand.accent]JSON[/]",
        "Pretty-print with theme colors",
        "[cyan]--features json[/]",
    ]);
    table.add_row_markup([
        "[brand.accent]Tracing[/]",
        "Structured logging integration",
        "[cyan]--features tracing[/]",
    ]);
    table.add_row_markup([
        "[brand.accent]Export[/]",
        "HTML/SVG capture of output",
        "[green]Core[/]",
    ]);

    console.print_renderable(&table);
}

/// Render call-to-action hyperlinks.
fn render_get_started(console: &Console) {
    console.print("[section.title]Get Started[/]");
    console.print("");

    // Install command
    console.print("  [dim]Add to your project:[/]");
    console.print("  [bold]cargo add rich_rust[/]");
    console.print("");

    // Or with all features
    console.print("  [dim]With all features:[/]");
    console.print("  [bold]cargo add rich_rust --features full[/]");
    console.print("");

    // Resource links
    console.print("  [dim]📖[/] Documentation: [link=https://docs.rs/rich_rust][brand.accent]docs.rs/rich_rust[/][/link]");
    console.print("  [dim]📦[/] Repository:    [link=https://github.com/Dicklesworthstone/rich_rust][brand.accent]github.com/Dicklesworthstone/rich_rust[/][/link]");
    console.print("  [dim]🦀[/] Crates.io:     [link=https://crates.io/crates/rich_rust][brand.accent]crates.io/crates/rich_rust[/][/link]");
}

/// Render what-next suggestions as a tree.
fn render_what_next(console: &Console) {
    console.print("[section.title]What Next?[/]");
    console.print("");

    let root = TreeNode::with_icon("🚀", markup::render_or_plain("[bold]Your Next Steps[/]"))
        .child(
            TreeNode::with_icon("📝", markup::render_or_plain("[cyan]Quick Start[/]"))
                .child(TreeNode::with_icon(
                    "1",
                    markup::render_or_plain("Create a Console with `Console::new()`"),
                ))
                .child(TreeNode::with_icon(
                    "2",
                    markup::render_or_plain(
                        "Print styled text with `console.print(\"[bold]Hello[/]\")`",
                    ),
                ))
                .child(TreeNode::with_icon(
                    "3",
                    markup::render_or_plain("Build tables, panels, trees as needed"),
                )),
        )
        .child(
            TreeNode::with_icon("💡", markup::render_or_plain("[cyan]Try the Examples[/]"))
                .child(TreeNode::with_icon(
                    "→",
                    markup::render_or_plain("[dim]cargo run --example basic[/]"),
                ))
                .child(TreeNode::with_icon(
                    "→",
                    markup::render_or_plain("[dim]cargo run --example tables[/]"),
                ))
                .child(TreeNode::with_icon(
                    "→",
                    markup::render_or_plain("[dim]cargo run --example progress[/]"),
                )),
        )
        .child(
            TreeNode::with_icon("🔧", markup::render_or_plain("[cyan]Advanced Features[/]"))
                .child(TreeNode::with_icon(
                    "•",
                    markup::render_or_plain("Custom themes with `Theme::from_style_definitions`"),
                ))
                .child(TreeNode::with_icon(
                    "•",
                    markup::render_or_plain("Live updates with `Live::new(console)`"),
                ))
                .child(TreeNode::with_icon(
                    "•",
                    markup::render_or_plain("Export output with `console.export_html()`"),
                )),
        )
        .child(
            TreeNode::with_icon("📚", markup::render_or_plain("[cyan]Learn More[/]"))
                .child(TreeNode::with_icon(
                    "•",
                    markup::render_or_plain("Read RICH_SPEC.md for detailed behavior"),
                ))
                .child(TreeNode::with_icon(
                    "•",
                    markup::render_or_plain("Check FEATURE_PARITY.md for Python Rich comparison"),
                )),
        );

    let tree = Tree::new(root)
        .guides(TreeGuides::Rounded)
        .guide_style(Style::parse("dim brand.accent").unwrap_or_default());

    console.print_renderable(&tree);
}

/// Render the thanks message with branding.
fn render_thanks(console: &Console) {
    let width = console.width();

    // Build the thanks panel content
    let content_lines = [
        "[bold]Thank you for exploring rich_rust![/]",
        "",
        "[dim]Beautiful terminal output for Rust[/]",
        "[dim]Zero unsafe code • Python Rich compatible • Extensible[/]",
    ];

    let content: Vec<Vec<rich_rust::segment::Segment>> = content_lines
        .iter()
        .map(|line| {
            let text = markup::render_or_plain(line);
            text.render("")
                .into_iter()
                .map(rich_rust::segment::Segment::into_owned)
                .collect()
        })
        .collect();

    let panel = Panel::new(content)
        .title(Text::new("✨ rich_rust ✨"))
        .border_style(Style::parse("brand.title").unwrap_or_default())
        .expand(false);

    console.print_renderable(&panel);

    console.print("");

    // Footer with version and author
    let footer = "Made with Rust by Jeffrey Emanuel";
    let pad = center_padding(cell_len(footer), width);
    console.print(&format!("{pad}[dim]{footer}[/]"));

    let version = "v0.1 • MIT License";
    let pad_ver = center_padding(cell_len(version), width);
    console.print(&format!("{pad_ver}[dim]{version}[/]"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outro_scene_has_correct_name() {
        let scene = OutroScene::new();
        assert_eq!(scene.name(), "outro");
    }

    #[test]
    fn outro_scene_has_summary() {
        let scene = OutroScene::new();
        assert!(!scene.summary().is_empty());
    }

    #[test]
    fn outro_scene_runs_without_error() {
        let scene = OutroScene::new();
        let console = Console::builder()
            .force_terminal(false)
            .markup(true)
            .build()
            .shared();
        let cfg = Config::with_defaults();

        let result = scene.run(&console, &cfg);
        assert!(result.is_ok());
    }

    #[test]
    fn outro_scene_produces_output() {
        let scene = OutroScene::new();
        let console = Console::builder()
            .force_terminal(false)
            .markup(true)
            .width(80)
            .build()
            .shared();
        let cfg = Config::with_defaults();

        console.begin_capture();
        let _ = scene.run(&console, &cfg);
        let segments = console.end_capture();

        // Collect all text into a string for easier assertion
        let output: String = segments.iter().map(|s| s.text.as_ref()).collect();

        // Should contain feature summary
        assert!(
            output.contains("Feature") || output.contains("Markup"),
            "output should contain feature summary"
        );
        // Should contain get started section
        assert!(
            output.contains("cargo add") || output.contains("docs.rs"),
            "output should contain get started info"
        );
        // Should contain thanks
        assert!(
            output.contains("rich_rust") || output.contains("Thank"),
            "output should contain thanks message"
        );
    }
}
