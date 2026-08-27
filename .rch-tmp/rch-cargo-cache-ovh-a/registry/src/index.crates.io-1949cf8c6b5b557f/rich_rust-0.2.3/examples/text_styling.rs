//! Text styling example demonstrating colors, attributes, and hyperlinks.
//!
//! Run with: `cargo run --example text_styling`

use rich_rust::prelude::*;

fn main() {
    let console = Console::new();
    let width = console.width().min(80);

    // ========================================================================
    // Named Colors
    // ========================================================================
    println!("\n=== Named Colors ===\n");

    console.print("[black]black[/] [red]red[/] [green]green[/] [yellow]yellow[/]");
    console.print("[blue]blue[/] [magenta]magenta[/] [cyan]cyan[/] [white]white[/]");

    // Bright variants
    console.print(
        "[bright_black]bright_black[/] [bright_red]bright_red[/] [bright_green]bright_green[/]",
    );
    console.print(
        "[bright_blue]bright_blue[/] [bright_magenta]bright_magenta[/] [bright_cyan]bright_cyan[/]",
    );

    // ========================================================================
    // Background Colors
    // ========================================================================
    println!("\n=== Background Colors ===\n");

    console.print("[on red]text on red[/] [on green]text on green[/] [on blue]text on blue[/]");
    console.print("[white on black]white on black[/] [black on white]black on white[/]");
    console.print("[bold yellow on magenta]bold yellow on magenta[/]");

    // ========================================================================
    // Hex Colors (Truecolor)
    // ========================================================================
    println!("\n=== Hex Colors (Truecolor) ===\n");

    console.print("[#ff6b6b]Coral red (#ff6b6b)[/]");
    console.print("[#4ecdc4]Teal (#4ecdc4)[/]");
    console.print("[#ffe66d]Sunny yellow (#ffe66d)[/]");
    console.print("[#95e1d3]Mint green (#95e1d3)[/]");
    console.print("[#f38181 on #3d3d3d]Pink on dark gray[/]");

    // ========================================================================
    // RGB Colors
    // ========================================================================
    println!("\n=== RGB Colors ===\n");

    console.print("[rgb(255,99,71)]Tomato rgb(255,99,71)[/]");
    console.print("[rgb(50,205,50)]Lime green rgb(50,205,50)[/]");
    console.print("[rgb(138,43,226)]Blue violet rgb(138,43,226)[/]");

    // ========================================================================
    // 256-Color Palette
    // ========================================================================
    println!("\n=== 256-Color Palette ===\n");

    console.print("[color(196)]Color 196 (red)[/]");
    console.print("[color(46)]Color 46 (green)[/]");
    console.print("[color(21)]Color 21 (blue)[/]");
    console.print("[color(208)]Color 208 (orange)[/]");
    console.print("[color(129)]Color 129 (purple)[/]");

    // ========================================================================
    // Text Attributes
    // ========================================================================
    println!("\n=== Text Attributes ===\n");

    console.print("[bold]Bold text[/]");
    console.print("[dim]Dim/faint text[/]");
    console.print("[italic]Italic text[/]");
    console.print("[underline]Underlined text[/]");
    console.print("[strike]Strikethrough text[/]");
    console.print("[reverse]Reversed colors[/]");
    console.print("[blink]Blinking text (if supported)[/]");

    // ========================================================================
    // Combined Styles
    // ========================================================================
    println!("\n=== Combined Styles ===\n");

    console.print("[bold italic]Bold and italic[/]");
    console.print("[bold underline red]Bold underline red[/]");
    console.print("[italic cyan on #333333]Italic cyan on dark background[/]");
    console.print("[bold dim]Bold and dim (some terminals)[/]");
    console.print("[underline strike green]Underline and strikethrough green[/]");

    // ========================================================================
    // Programmatic Styling
    // ========================================================================
    println!("\n=== Programmatic Styling ===\n");

    // Create styles programmatically
    let warning_style = Style::new()
        .bold()
        .color(Color::parse("yellow").unwrap_or_default());

    let error_style = Style::new()
        .bold()
        .color(Color::parse("white").unwrap_or_default())
        .bgcolor(Color::parse("red").unwrap_or_default());

    let success_style = Style::new()
        .bold()
        .color(Color::parse("#00ff00").unwrap_or_default());

    console.print_styled("Warning: Check your configuration", warning_style);
    console.print_styled("Error: Operation failed!", error_style);
    console.print_styled("Success: All tests passed!", success_style);

    // ========================================================================
    // Hyperlinks (OSC 8)
    // ========================================================================
    println!("\n=== Hyperlinks (Terminal Support Required) ===\n");

    // Create a styled hyperlink
    let link_style = Style::new()
        .underline()
        .color(Color::parse("blue").unwrap_or_default())
        .link("https://github.com/Dicklesworthstone/rich_rust");

    console.print_styled("Visit rich_rust on GitHub", link_style);

    // Link with explicit ID
    let link_style_with_id = Style::new()
        .underline()
        .color(Color::parse("cyan").unwrap_or_default())
        .link_with_id("https://docs.rs/rich_rust", "docs-link");

    console.print_styled("Read the documentation", link_style_with_id);

    // ========================================================================
    // Complex Styled Text
    // ========================================================================
    println!("\n=== Complex Styled Text ===\n");

    // Build text with multiple spans
    let mut complex_text = Text::new("This text has ");
    complex_text.append_styled("multiple", Style::new().bold());
    complex_text.append(" different ");
    complex_text.append_styled(
        "styled",
        Style::new()
            .italic()
            .color(Color::parse("green").unwrap_or_default()),
    );
    complex_text.append(" ");
    complex_text.append_styled(
        "sections",
        Style::new()
            .underline()
            .color(Color::parse("blue").unwrap_or_default()),
    );
    complex_text.append("!");

    // Render each segment with ANSI codes
    for segment in complex_text.render("\n") {
        if let Some(style) = &segment.style {
            let ansi = style.render_ansi(ColorSystem::TrueColor);
            let (prefix, suffix) = &*ansi;
            print!("{}{}{}", prefix, segment.text, suffix);
        } else {
            print!("{}", segment.text);
        }
    }

    // ========================================================================
    // Style Inheritance
    // ========================================================================
    println!("\n=== Style Inheritance ===\n");

    // Styles can be combined
    let base_style = Style::new().bold();
    let red_bold = base_style.combine(&Style::new().color(Color::parse("red").unwrap_or_default()));
    let blue_bold =
        base_style.combine(&Style::new().color(Color::parse("blue").unwrap_or_default()));

    console.print_styled("Red and bold (inherited)", red_bold);
    console.print_styled("Blue and bold (inherited)", blue_bold);

    // ========================================================================
    // Section Divider
    // ========================================================================
    let rule = Rule::with_title("End of Text Styling Demo").style(
        Style::new()
            .bold()
            .color(Color::parse("green").unwrap_or_default()),
    );
    for seg in rule.render(width) {
        print!("{}", seg.text);
    }

    println!();
}
