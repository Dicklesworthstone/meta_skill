//! Emoji and hyperlink showcase scene for demo_showcase.
//!
//! Demonstrates rich_rust capabilities for:
//! - Emoji shortcode replacement (`:rocket:`, `:sparkles:`)
//! - The Emoji renderable for individual emojis
//! - OSC8 hyperlinks with graceful fallback

use std::borrow::Cow;
use std::sync::Arc;

use rich_rust::console::Console;
use rich_rust::emoji;
use rich_rust::markup;
use rich_rust::renderables::emoji::Emoji;
use rich_rust::renderables::panel::Panel;

use crate::Config;
use crate::scenes::{Scene, SceneError};

/// Emoji and hyperlink showcase scene.
pub struct EmojiLinksScene;

impl EmojiLinksScene {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Scene for EmojiLinksScene {
    fn name(&self) -> &'static str {
        "emoji_links"
    }

    fn summary(&self) -> &'static str {
        "Emoji shortcodes and terminal hyperlinks for polished output."
    }

    fn run(&self, console: &Arc<Console>, cfg: &Config) -> Result<(), SceneError> {
        console.print("[section.title]Emoji & Hyperlinks: Visual Polish[/]");
        console.print("");
        console.print("[dim]Make your terminal output expressive and interactive.[/]");
        console.print("");

        // Demo 1: Emoji shortcodes
        render_emoji_demo(console);

        console.print("");

        // Demo 2: Hyperlinks
        render_hyperlink_demo(console, cfg);

        console.print("");

        // Demo 3: Combined usage
        render_combined_demo(console, cfg);

        Ok(())
    }
}

/// Render emoji demonstration.
fn render_emoji_demo(console: &Console) {
    console.print("[brand.accent]Emoji Shortcodes[/]");
    console.print("");

    // Shortcode replacement in text
    console.print("[dim]Shortcodes like :rocket: and :sparkles: are automatically replaced:[/]");
    console.print("");
    console.print("  :rocket:  Launch sequence initiated");
    console.print("  :white_check_mark:  All systems nominal");
    console.print("  :warning:  Memory usage elevated");
    console.print("  :x:  Connection failed");
    console.print("  :sparkles:  New feature available");
    console.print("");

    // Common status indicators
    console.print("[dim]Common status indicators:[/]");
    console.print("");
    console.print("  :green_circle: Online    :yellow_circle: Degraded    :red_circle: Offline");
    console.print("  :heavy_check_mark: Pass  :heavy_multiplication_x: Fail  :hourglass: Pending");
    console.print("");

    // The Emoji renderable
    console.print("[dim]The Emoji renderable for individual emojis:[/]");
    console.print("");

    if let Ok(emoji) = Emoji::new("rocket") {
        console.print("  ");
        console.print_renderable(&emoji);
        console.print(" = Emoji::new(\"rocket\")");
    }

    if let Ok(emoji) = Emoji::new("sparkles") {
        console.print("  ");
        console.print_renderable(&emoji);
        console.print(" = Emoji::new(\"sparkles\")");
    }

    console.print("");
    console.print("[hint]Emojis add visual hierarchy and make status clear at a glance.[/]");
}

/// Render hyperlink demonstration.
fn render_hyperlink_demo(console: &Console, cfg: &Config) {
    console.print("[brand.accent]Terminal Hyperlinks (OSC8)[/]");
    console.print("");

    console.print("[dim]Modern terminals support clickable hyperlinks:[/]");
    console.print("");

    // Print links using markup syntax
    console.print("  📖 Documentation: [link=https://docs.rs/rich_rust][cyan underline]docs.rs/rich_rust[/][/link]");
    console.print("  📁 Repository: [link=https://github.com/Dicklesworthstone/rich_rust][cyan underline]github.com/Dicklesworthstone/rich_rust[/][/link]");
    console.print("  📦 Crates.io: [link=https://crates.io/crates/rich_rust][cyan underline]crates.io/crates/rich_rust[/][/link]");

    console.print("");

    // Explain fallback behavior
    let fallback_panel = Panel::from_text(
        "Terminals that don't support OSC8 will show the\n\
         text without the link - no broken escape codes.",
    )
    .title_from_markup("[dim]Graceful Fallback[/]")
    .width(50)
    .safe_box(cfg.is_safe_box());
    console.print_renderable(&fallback_panel);

    console.print("");
    console.print("[hint]Click links in supported terminals (iTerm2, Wezterm, Ghostty, etc.).[/]");
}

/// Render combined usage demonstration.
fn render_combined_demo(console: &Console, cfg: &Config) {
    console.print("[brand.accent]Combining Emoji & Links[/]");
    console.print("");

    // Create a styled notification panel
    // Pre-process content: if emoji rendering is enabled, replace emoji shortcodes, then parse markup.
    let content = ":sparkles: [bold]New Release Available![/] :sparkles:\n\n\
         Version 2.5.0 includes:\n\
         :white_check_mark: Improved table rendering\n\
         :white_check_mark: New panel styles\n\
         :white_check_mark: Better Unicode support\n\n\
         [dim]View release notes:[/] [cyan underline]github.com/releases/v2.5.0[/]";
    let content_with_emoji = if console.emoji() {
        emoji::replace(content, None)
    } else {
        Cow::Borrowed(content)
    };
    let styled_content = markup::render_or_plain(content_with_emoji.as_ref());

    // Process title the same way
    let title = ":bell: [bold]Notification[/]";
    let title_with_emoji = if console.emoji() {
        emoji::replace(title, None)
    } else {
        Cow::Borrowed(title)
    };
    let styled_title = markup::render_or_plain(title_with_emoji.as_ref());

    let notification = Panel::from_rich_text(&styled_content, 50)
        .title(styled_title)
        .width(55)
        .safe_box(cfg.is_safe_box());
    console.print_renderable(&notification);

    console.print("");

    // Quick reference
    console.print("[dim]Quick Reference:[/]");
    console.print("");
    console.print("  Emoji:  console.print(\":rocket: text\")");
    console.print("  Link:   Style::new().link(\"https://...\")");
    console.print("  Both:   \":sparkles: [link=url]text[/]\"");

    console.print("");
    console.print("[hint]Combine emoji and links for rich, interactive CLI experiences.[/]");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emoji_links_scene_has_correct_name() {
        let scene = EmojiLinksScene::new();
        assert_eq!(scene.name(), "emoji_links");
    }

    #[test]
    fn emoji_links_scene_runs_without_error() {
        let scene = EmojiLinksScene::new();
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
