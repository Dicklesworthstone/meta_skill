//! Panel showcase scene for demo_showcase.
//!
//! Demonstrates rich_rust Panel capabilities including:
//! - Different box styles (rounded, square, heavy, double, ASCII)
//! - Titles and subtitles
//! - Padding and spacing
//! - Nested panels

use std::sync::Arc;

use rich_rust::r#box::{DOUBLE, HEAVY};
use rich_rust::console::Console;
use rich_rust::markup;
use rich_rust::renderables::panel::Panel;
use rich_rust::style::Style;
use rich_rust::text::JustifyMethod;

use crate::Config;
use crate::scenes::{Scene, SceneError};

/// Panel showcase scene: demonstrates Panel rendering capabilities.
pub struct PanelScene;

impl PanelScene {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Scene for PanelScene {
    fn name(&self) -> &'static str {
        "panels"
    }

    fn summary(&self) -> &'static str {
        "Panel showcase: box styles, titles, padding, and nesting."
    }

    fn run(&self, console: &Arc<Console>, cfg: &Config) -> Result<(), SceneError> {
        console.print("[section.title]Panels: Bordered Content Containers[/]");
        console.print("");
        console.print("[dim]Panels wrap content with decorative borders and titles.[/]");
        console.print("");

        // Demo 1: Box style showcase
        render_box_styles(console, cfg);

        console.print("");

        // Demo 2: Titles and subtitles
        render_titled_panels(console, cfg);

        console.print("");

        // Demo 3: Practical usage examples
        render_practical_panels(console, cfg);

        Ok(())
    }
}

/// Render different box styles side by side.
fn render_box_styles(console: &Console, cfg: &Config) {
    console.print("[brand.accent]Box Styles[/]");
    console.print("");

    // Rounded (default)
    let rounded = Panel::from_text("Rounded corners - the default style")
        .title("Rounded")
        .width(40)
        .safe_box(cfg.is_safe_box());
    console.print_renderable(&rounded);
    console.print("");

    // Square
    let square = Panel::from_text("Sharp corners for a technical look")
        .title("Square")
        .square()
        .width(40)
        .safe_box(cfg.is_safe_box());
    console.print_renderable(&square);
    console.print("");

    // Heavy
    let heavy = Panel::from_text("Bold borders for emphasis")
        .title("Heavy")
        .box_style(&HEAVY)
        .width(40)
        .safe_box(cfg.is_safe_box());
    console.print_renderable(&heavy);
    console.print("");

    // Double
    let double = Panel::from_text("Classic double-line borders")
        .title("Double")
        .box_style(&DOUBLE)
        .width(40)
        .safe_box(cfg.is_safe_box());
    console.print_renderable(&double);
    console.print("");

    // ASCII (for compatibility)
    let ascii = Panel::from_text("Works in any terminal")
        .title("ASCII")
        .ascii()
        .width(40);
    console.print_renderable(&ascii);

    console.print("");
    console.print("[hint]Choose box styles based on terminal support and visual weight.[/]");
}

/// Render panels with titles and subtitles.
fn render_titled_panels(console: &Console, cfg: &Config) {
    console.print("[brand.accent]Titles and Subtitles[/]");
    console.print("");

    // Title only
    let titled = Panel::from_text("A panel with just a title")
        .title("Simple Title")
        .width(50)
        .safe_box(cfg.is_safe_box());
    console.print_renderable(&titled);
    console.print("");

    // Title and subtitle
    let with_subtitle =
        Panel::from_text("Subtitles appear at the bottom\nand can provide additional context")
            .title("Main Title")
            .subtitle("Subtitle goes here")
            .width(50)
            .safe_box(cfg.is_safe_box());
    console.print_renderable(&with_subtitle);
    console.print("");

    // Styled title with alignment
    let styled = Panel::from_text("Titles can be styled and aligned")
        .title_from_markup("[bold cyan]Styled Title[/]")
        .title_align(JustifyMethod::Center)
        .subtitle_from_markup("[dim]Right-aligned subtitle[/]")
        .subtitle_align(JustifyMethod::Right)
        .border_style(Style::parse("cyan").unwrap_or_default())
        .width(50)
        .safe_box(cfg.is_safe_box());
    console.print_renderable(&styled);

    console.print("");
    console.print(
        "[hint]Titles support markup for styling; alignment options: left, center, right.[/]",
    );
}

/// Render practical panel usage examples.
fn render_practical_panels(console: &Console, cfg: &Config) {
    console.print("[brand.accent]Practical Examples[/]");
    console.print("");

    // Status panel
    let status_content = markup::render_or_plain(
        "[bold green]Deployment successful[/]\n\n\
         Version: 2.4.1\n\
         Region: us-west-2\n\
         Duration: 2m 15s",
    );
    let status = Panel::from_rich_text(&status_content, 40)
        .title_from_markup("[green]Status[/]")
        .border_style(Style::parse("green").unwrap_or_default())
        .padding((1, 2))
        .safe_box(cfg.is_safe_box());
    console.print_renderable(&status);
    console.print("");

    // Warning panel
    let warning_content = markup::render_or_plain(
        "[yellow]Memory usage is at 85%[/]\n\n\
         Consider scaling up the worker\n\
         pool or optimizing queries.",
    );
    let warning = Panel::from_rich_text(&warning_content, 40)
        .title_from_markup("[bold yellow]Warning[/]")
        .border_style(Style::parse("yellow").unwrap_or_default())
        .box_style(&HEAVY)
        .padding((1, 2))
        .safe_box(cfg.is_safe_box());
    console.print_renderable(&warning);
    console.print("");

    // Tip panel (light borders with blue styling)
    let tip_content = markup::render_or_plain(
        "Use --quick for faster iteration\n\
         Use --seed 42 for reproducible output",
    );
    let tip = Panel::from_rich_text(&tip_content, 45)
        .title_from_markup("[bold blue]Tip[/]")
        .border_style(Style::parse("blue dim").unwrap_or_default())
        .safe_box(cfg.is_safe_box());
    console.print_renderable(&tip);
    console.print("");

    // Quote panel with subtle rounded borders
    let quote_content = markup::render_or_plain(
        "[italic]The best error message is the one that\n\
         never shows up.[/]\n\n\
         [dim]— Thomas Fuchs[/]",
    );
    let quote = Panel::from_rich_text(&quote_content, 45)
        .border_style(Style::parse("dim").unwrap_or_default())
        .padding((0, 1))
        .safe_box(cfg.is_safe_box());
    console.print_renderable(&quote);

    console.print("");
    console.print("[hint]Combine padding, colors, and box styles to create semantic meaning.[/]");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_scene_has_correct_name() {
        let scene = PanelScene::new();
        assert_eq!(scene.name(), "panels");
    }

    #[test]
    fn panel_scene_runs_without_error() {
        let scene = PanelScene::new();
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
