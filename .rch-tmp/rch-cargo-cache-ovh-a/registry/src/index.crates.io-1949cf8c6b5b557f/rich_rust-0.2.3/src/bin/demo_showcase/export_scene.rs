//! Export scene for demo_showcase.
//!
//! Demonstrates the export functionality and shows viewing instructions.
//! When running in export mode, displays the export summary.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use rich_rust::r#box::{DOUBLE, ROUNDED};
use rich_rust::console::Console;
use rich_rust::interactive::Status;
use rich_rust::markup::render_or_plain;
use rich_rust::renderables::panel::Panel;
use rich_rust::style::Style;

use crate::Config;
use crate::scenes::{Scene, SceneError};

/// Export showcase scene: demonstrates export capabilities.
pub struct ExportScene;

impl ExportScene {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Scene for ExportScene {
    fn name(&self) -> &'static str {
        "export"
    }

    fn summary(&self) -> &'static str {
        "Export HTML/SVG bundle with viewing instructions."
    }

    fn run(&self, console: &Arc<Console>, cfg: &Config) -> Result<(), SceneError> {
        console.print("[section.title]Export: Sharing Terminal Output[/]");
        console.print("");
        console.print("[dim]rich_rust can export terminal output to HTML and SVG for sharing.[/]");
        console.print("");

        // Show export formats
        render_export_formats(console, cfg);

        console.print("");

        // Show usage instructions
        render_usage_instructions(console);

        console.print("");

        // If we're in export mode, show what will be exported
        if cfg.is_export() {
            // Brief spinner moment: "Generating export bundle…"
            if let Ok(_status) = Status::new(console, "Generating export bundle…") {
                let duration = if cfg.is_quick() {
                    Duration::from_millis(200)
                } else {
                    Duration::from_millis(500)
                };
                thread::sleep(duration);
            }
            render_export_summary(console, cfg);
            console.print("");
        }

        Ok(())
    }
}

/// Render export format descriptions.
fn render_export_formats(console: &Console, cfg: &Config) {
    console.print("[brand.accent]Available Export Formats[/]");
    console.print("");

    // HTML format panel
    let html_content = render_or_plain(
        "[bold]HTML Export[/]\n\n\
         Generates a standalone HTML file with inline or external CSS.\n\
         - Colors and styles preserved\n\
         - Works in any modern browser\n\
         - Easy to share via email or hosting\n\n\
         [dim]Use `--export` or `--export-dir <path>`[/]",
    );
    let html_title = render_or_plain("[cyan]demo_showcase.html[/]");

    let html_panel = Panel::from_rich_text(&html_content, 76)
        .title(html_title)
        .box_style(&ROUNDED)
        .border_style(Style::parse("cyan").unwrap_or_default())
        .safe_box(cfg.is_safe_box());

    console.print_renderable(&html_panel);

    console.print("");

    // SVG format panel
    let svg_content = render_or_plain(
        "[bold]SVG Export[/]\n\n\
         Generates a scalable vector graphic with embedded fonts.\n\
         - Perfect for documentation\n\
         - Scales to any size without pixelation\n\
         - Rendered with SVG primitives (text, rects, clip paths)\n\
         - Optional terminal-window chrome (Rich-style)\n\n\
         [dim]Note: View in a browser or any SVG-capable viewer[/]",
    );
    let svg_title = render_or_plain("[magenta]demo_showcase.svg[/]");

    let svg_panel = Panel::from_rich_text(&svg_content, 76)
        .title(svg_title)
        .box_style(&ROUNDED)
        .border_style(Style::parse("magenta").unwrap_or_default())
        .safe_box(cfg.is_safe_box());

    console.print_renderable(&svg_panel);
}

/// Render usage instructions.
fn render_usage_instructions(console: &Console) {
    console.print("[brand.accent]How to Export[/]");
    console.print("");

    let instructions = r#"[bold]Quick Export (temp directory):[/]
  demo_showcase --export

[bold]Export to specific directory:[/]
  demo_showcase --export-dir ./output

[bold]Export single scene:[/]
  demo_showcase --scene hero --export-dir ./output

[bold]Recommended flags for clean export:[/]
  demo_showcase --export-dir ./output \
    --no-interactive \
    --color-system truecolor \
    --width 100 \
    --quick"#;

    console.print(instructions);
}

/// Render export summary when in export mode.
fn render_export_summary(console: &Console, cfg: &Config) {
    console.print("[brand.accent]Export Summary[/]");
    console.print("");

    if let Some(export_dir) = cfg.export_dir() {
        let html_path = export_dir.join("demo_showcase.html");
        let svg_path = export_dir.join("demo_showcase.svg");

        let summary = render_or_plain(&format!(
            "[bold green]Files will be written to:[/]\n\n\
             [cyan]HTML:[/] {}\n\
             [magenta]SVG:[/]  {}\n\n\
             [dim]Open the HTML file in your browser to view the output.\n\
             The SVG can be embedded in documentation or presentations.[/]",
            html_path.display(),
            svg_path.display()
        ));
        let summary_title = render_or_plain("[bold]Export Complete[/]");

        let summary_panel = Panel::from_rich_text(&summary, 76)
            .title(summary_title)
            .box_style(&DOUBLE)
            .border_style(Style::parse("green").unwrap_or_default())
            .safe_box(cfg.is_safe_box());

        console.print_renderable(&summary_panel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_scene_has_correct_name() {
        let scene = ExportScene::new();
        assert_eq!(scene.name(), "export");
    }

    #[test]
    fn export_scene_runs_without_error() {
        let scene = ExportScene::new();
        let console = Console::builder()
            .force_terminal(false)
            .markup(true)
            .build()
            .shared();
        let cfg = Config::with_defaults();

        let result = scene.run(&console, &cfg);
        assert!(result.is_ok());
    }
}
