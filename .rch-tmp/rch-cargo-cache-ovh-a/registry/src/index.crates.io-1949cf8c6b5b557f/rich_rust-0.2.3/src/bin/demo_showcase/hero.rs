//! Hero scene for demo_showcase.
//!
//! Introduces the Nebula Deploy brand and demonstrates rich_rust capabilities.
//! Content: branded title, capability detection panel, palette preview, hyperlink CTAs.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use rich_rust::cells::cell_len;
use rich_rust::console::Console;
use rich_rust::interactive::Status;
use rich_rust::renderables::Renderable;
use rich_rust::renderables::panel::Panel;
use rich_rust::renderables::table::{Column, Table};
use rich_rust::style::Style;
use rich_rust::text::Text;

use crate::Config;
use crate::scenes::{Scene, SceneError};

/// Maximum content width for readable output on wide terminals.
const MAX_CONTENT_WIDTH: usize = 120;

/// Hero scene: branding, capabilities, palette preview.
pub struct HeroScene;

impl HeroScene {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Scene for HeroScene {
    fn name(&self) -> &'static str {
        "hero"
    }

    fn summary(&self) -> &'static str {
        "Introduce Nebula Deploy and the visual brand."
    }

    fn run(&self, console: &Arc<Console>, cfg: &Config) -> Result<(), SceneError> {
        // Brief spinner moment: "Validating environment…"
        if let Ok(_status) = Status::new(console, "Validating environment…") {
            // Hold the spinner briefly in quick mode, longer in normal mode
            let duration = if cfg.is_quick() {
                Duration::from_millis(200)
            } else {
                Duration::from_millis(800)
            };
            thread::sleep(duration);
            // Status is dropped here, stopping the spinner
        }

        // Big branded title
        render_brand_title(console);

        console.print("");

        // Capability panel
        render_capabilities_panel(console);

        console.print("");

        // Palette preview
        render_palette_preview(console);

        console.print("");

        // Hyperlink CTAs
        render_ctas(console);

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

/// Print a renderable centered within the console width.
///
/// For very wide terminals (200+ columns), we add left padding to center
/// the content visually rather than letting it left-align at column 0.
/// This preserves ANSI styling by working with segments directly.
fn print_centered_renderable<R: Renderable>(console: &Console, renderable: &R) {
    use rich_rust::segment::Segment;

    let terminal_width = console.width();

    // For wide terminals, calculate padding to center content
    if terminal_width > MAX_CONTENT_WIDTH {
        let left_pad = (terminal_width - MAX_CONTENT_WIDTH) / 2;
        let indent = " ".repeat(left_pad);

        // Render at the constrained width
        let options = console
            .options()
            .update_dimensions(MAX_CONTENT_WIDTH, console.height());
        let segments = renderable.render(console, &options);

        // Build output segments with indentation, preserving styles
        let mut output_segments: Vec<Segment<'static>> = Vec::new();
        let mut at_line_start = true;

        for seg in segments {
            if seg.text.contains('\n') {
                // Split segment on newlines, preserving style for each part
                for (i, part) in seg.text.split('\n').enumerate() {
                    if i > 0 {
                        // After a newline, add the newline and mark next as line start
                        output_segments.push(Segment::new("\n".to_string(), None));
                        at_line_start = true;
                    }
                    if !part.is_empty() {
                        if at_line_start {
                            output_segments.push(Segment::new(indent.clone(), None));
                            at_line_start = false;
                        }
                        output_segments.push(Segment::new(part.to_string(), seg.style.clone()));
                    }
                }
            } else {
                if at_line_start && !seg.text.is_empty() {
                    output_segments.push(Segment::new(indent.clone(), None));
                    at_line_start = false;
                }
                output_segments.push(seg.into_owned());
            }
        }

        console.print_segments(&output_segments);
    } else {
        // Normal width - just print directly
        console.print_renderable(renderable);
    }
}

/// Render the big branded title with tagline.
fn render_brand_title(console: &Console) {
    let width = console.width();

    // Use compact layout for narrow terminals
    if width < 50 {
        // Narrow layout: simple centered text using a panel
        let title_text = "✦ NEBULA DEPLOY ✦";
        let title_visible_width = cell_len(title_text);
        let box_width = title_visible_width + 4; // │ + space + title + space + │
        let inner_width = box_width - 2;

        let top = format!("┌{}┐", "─".repeat(inner_width));
        let mid_content = format!(" {} ", title_text);
        let bot = format!("└{}┘", "─".repeat(inner_width));

        let pad = center_padding(box_width, width);

        console.print(&format!("{pad}[brand.title]{top}[/]"));
        console.print(&format!(
            "{pad}[brand.title]│[/][bold #a78bfa]{mid_content}[/][brand.title]│[/]"
        ));
        console.print(&format!("{pad}[brand.title]{bot}[/]"));
        console.print("");

        let subtitle = "Beautiful terminal output";
        let pad_sub = center_padding(cell_len(subtitle), width);
        console.print(&format!("{pad_sub}[brand.subtitle]{subtitle}[/]"));

        let powered = "powered by rich_rust";
        let pad_pow = center_padding(cell_len(powered), width);
        console.print(&format!("{pad_pow}[brand.muted]{powered}[/]"));
    } else {
        // Full-width layout with spaced letters - use box width based on content
        let title_text = "✦  N E B U L A   D E P L O Y  ✦";
        let title_visible_width = cell_len(title_text);
        let inner_padding = 4; // padding on each side inside the box
        let box_width = title_visible_width + (inner_padding * 2) + 2; // content + padding + borders
        let inner_width = box_width - 2;

        let top = format!("╭{}╮", "─".repeat(inner_width));
        let empty_line = format!("│{}│", " ".repeat(inner_width));
        let bot = format!("╰{}╯", "─".repeat(inner_width));

        // Centered title within the box
        let title_inner_pad = " ".repeat(inner_padding);
        let title_line_content = format!("{title_inner_pad}{title_text}{title_inner_pad}");

        let pad = center_padding(box_width, width);

        console.print(&format!("{pad}[brand.title]{top}[/]"));
        console.print(&format!("{pad}[brand.title]{empty_line}[/]"));
        console.print(&format!(
            "{pad}[brand.title]│[/][bold #a78bfa]{title_line_content}[/][brand.title]│[/]"
        ));
        console.print(&format!("{pad}[brand.title]{empty_line}[/]"));
        console.print(&format!("{pad}[brand.title]{bot}[/]"));
        console.print("");

        let subtitle = "Beautiful terminal output for Rust";
        let pad_sub = center_padding(cell_len(subtitle), width);
        console.print(&format!("{pad_sub}[brand.subtitle]{subtitle}[/]"));

        let powered = "powered by rich_rust";
        let pad_pow = center_padding(cell_len(powered), width);
        console.print(&format!("{pad_pow}[brand.muted]{powered}[/]"));
    }
}

/// Render the capabilities detection panel.
fn render_capabilities_panel(console: &Console) {
    let width = console.width();
    let height = console.height();
    let is_terminal = console.is_terminal();
    let is_interactive = console.is_interactive();
    let color_system = console.color_system();
    let emoji_enabled = true; // Default for demo

    // Format color system name
    let color_name = match color_system {
        Some(cs) => format!("{cs:?}"),
        None => "None (no color)".to_string(),
    };

    // Build capability lines
    let lines = [
        format!(
            "[dim]Terminal size:[/] [brand.accent]{width}[/] × [brand.accent]{height}[/] cells"
        ),
        format!("[dim]Color system:[/]  [brand.accent]{color_name}[/]"),
        format!(
            "[dim]Is terminal:[/]   {}",
            if is_terminal {
                "[status.ok]yes[/]"
            } else {
                "[status.warn]no (piped)[/]"
            }
        ),
        format!(
            "[dim]Interactive:[/]   {}",
            if is_interactive {
                "[status.ok]yes[/]"
            } else {
                "[status.warn]no[/]"
            }
        ),
        format!(
            "[dim]Emoji:[/]         {}",
            if emoji_enabled {
                "[status.ok]enabled[/] ✨"
            } else {
                "[status.warn]disabled[/]"
            }
        ),
    ];

    // Create panel content
    let content: Vec<Vec<rich_rust::segment::Segment>> = lines
        .iter()
        .map(|line| {
            let text = rich_rust::markup::render_or_plain(line);
            text.render("")
                .into_iter()
                .map(rich_rust::segment::Segment::into_owned)
                .collect()
        })
        .collect();

    let panel = Panel::new(content)
        .title(Text::new("Environment Detection"))
        .border_style(Style::parse("dim #38bdf8").unwrap_or_default())
        .expand(false);

    print_centered_renderable(console, &panel);
}

/// Render the color palette preview.
fn render_palette_preview(console: &Console) {
    let mut table = Table::new().title("Color Palette");
    table.add_column(Column::new("Category").style(Style::parse("dim").unwrap_or_default()));
    table.add_column(Column::new("Preview"));

    // Brand colors
    table.add_row_markup([
        "Brand",
        "[#a78bfa]████[/] [#c4b5fd]████[/] [#38bdf8]████[/]",
    ]);

    // Status colors
    table.add_row_markup([
        "Status",
        "[green]████[/] [yellow]████[/] [red]████[/] [cyan]████[/]",
    ]);

    // Badges
    table.add_row_markup([
        "Badges",
        "[bold white on green] OK [/] [bold black on yellow] WARN [/] [bold white on red] ERR [/]",
    ]);

    // Dim/muted
    table.add_row_markup(["Muted", "[dim #94a3b8]████[/] [dim #64748b]████[/]"]);

    print_centered_renderable(console, &table);
}

/// Render call-to-action hyperlinks.
fn render_ctas(console: &Console) {
    console.print("[section.title]Get Started[/]");
    console.print("");

    // Documentation link
    console.print("  [dim]📖[/] Documentation: [link=https://docs.rs/rich_rust][brand.accent]docs.rs/rich_rust[/][/link]");

    // Repository link
    console.print("  [dim]📦[/] Repository:    [link=https://github.com/Dicklesworthstone/rich_rust][brand.accent]github.com/Dicklesworthstone/rich_rust[/][/link]");

    // Crates.io link
    console.print("  [dim]🦀[/] Crates.io:     [link=https://crates.io/crates/rich_rust][brand.accent]crates.io/crates/rich_rust[/][/link]");

    console.print("");
    console.print("[hint]Press any key to continue, or run with --scene <name> to jump to a specific demo.[/]");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hero_scene_has_correct_name() {
        let scene = HeroScene::new();
        assert_eq!(scene.name(), "hero");
    }

    #[test]
    fn hero_scene_has_summary() {
        let scene = HeroScene::new();
        assert!(!scene.summary().is_empty());
    }

    #[test]
    fn hero_scene_runs_without_error() {
        let scene = HeroScene::new();
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
    fn hero_scene_produces_output() {
        let scene = HeroScene::new();
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

        // Should contain brand title (N E B U L A with spaces in the hero)
        assert!(
            output.contains("N E B U L A") || output.contains("D E P L O Y"),
            "output should contain brand title"
        );
        // Should contain capability info
        assert!(
            output.contains("Terminal size") || output.contains("Color system"),
            "output should contain capability info"
        );
    }
}
