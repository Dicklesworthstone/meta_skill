//! Basic example demonstrating rich_rust functionality.

use rich_rust::prelude::*;

fn main() {
    // Create a console
    let console = Console::new();

    // Print styled text using markup
    console.print("[bold]Hello[/bold], [red]World[/red]!");

    // Print with different colors
    console.print("[green]Success![/] [yellow]Warning[/] [red]Error[/]");

    // Print with multiple styles
    console.print("[bold italic blue]Bold italic blue text[/]");

    // Create styled text programmatically
    let mut text = Text::new("Programmatic ");
    let style = Style::parse("bold magenta").unwrap_or_default();
    text.append_styled("styled", style);
    text.append(" text!");

    // Render to segments and display
    for segment in text.render("\n") {
        print!("{}", segment.text);
    }

    // Print a rule
    console.rule(Some("Section Title"));

    // Print plain text (no markup parsing)
    console.print_plain("[This is not markup - brackets are literal]");

    // Print with explicit style
    console.print_styled("Explicitly styled text", Style::new().bold().italic());
}
