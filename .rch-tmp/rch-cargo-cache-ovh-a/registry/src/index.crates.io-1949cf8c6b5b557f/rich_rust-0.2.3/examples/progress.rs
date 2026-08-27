//! Progress bar and spinner example demonstrating various styles and features.
//!
//! Run with: `cargo run --example progress`

use rich_rust::prelude::*;

fn main() {
    let console = Console::new();
    let width = console.width().min(80);

    // ========================================================================
    // Basic Progress Bar
    // ========================================================================
    println!("\n=== Basic Progress Bar ===\n");

    let mut bar = ProgressBar::new().width(40);
    bar.set_progress(0.75);
    for seg in bar.render(width) {
        print!("{}", seg.text);
    }

    // ========================================================================
    // Progress Bar Styles
    // ========================================================================
    println!("=== Progress Bar Styles ===\n");

    // Block style (default)
    println!("Block (default):");
    let mut block_bar = ProgressBar::new().width(40).bar_style(BarStyle::Block);
    block_bar.set_progress(0.6);
    for seg in block_bar.render(width) {
        print!("{}", seg.text);
    }

    // ASCII style
    println!("ASCII:");
    let mut ascii_style_bar = ProgressBar::new().width(40).bar_style(BarStyle::Ascii);
    ascii_style_bar.set_progress(0.6);
    for seg in ascii_style_bar.render(width) {
        print!("{}", seg.text);
    }

    // Line style
    println!("Line:");
    let mut line_style_bar = ProgressBar::new().width(40).bar_style(BarStyle::Line);
    line_style_bar.set_progress(0.6);
    for seg in line_style_bar.render(width) {
        print!("{}", seg.text);
    }

    // Dots style
    println!("Dots:");
    let mut dots_style_bar = ProgressBar::new().width(40).bar_style(BarStyle::Dots);
    dots_style_bar.set_progress(0.6);
    for seg in dots_style_bar.render(width) {
        print!("{}", seg.text);
    }

    // Gradient style
    println!("Gradient:");
    let mut gradient_style_bar = ProgressBar::new().width(40).bar_style(BarStyle::Gradient);
    gradient_style_bar.set_progress(0.6);
    for seg in gradient_style_bar.render(width) {
        print!("{}", seg.text);
    }

    // ========================================================================
    // Progress Bar with Description
    // ========================================================================
    println!("=== Progress Bar with Description ===\n");

    let mut described_bar = ProgressBar::new().width(30).description("Downloading");
    described_bar.set_progress(0.45);
    for seg in described_bar.render(width) {
        print!("{}", seg.text);
    }

    let mut upload_bar = ProgressBar::new().width(30).description("Uploading files");
    upload_bar.set_progress(0.82);
    for seg in upload_bar.render(width) {
        print!("{}", seg.text);
    }

    // ========================================================================
    // Progress Bar with Custom Styling
    // ========================================================================
    println!("=== Custom Styled Progress Bars ===\n");

    let mut custom_bar = ProgressBar::new()
        .width(40)
        .completed_style(Style::parse("bold magenta").unwrap_or_default())
        .remaining_style(Style::parse("dim").unwrap_or_default())
        .pulse_style(Style::parse("bold yellow").unwrap_or_default());
    custom_bar.set_progress(0.5);
    for seg in custom_bar.render(width) {
        print!("{}", seg.text);
    }

    // Different color scheme
    let mut ocean_bar = ProgressBar::new()
        .width(40)
        .completed_style(Style::parse("bold cyan").unwrap_or_default())
        .remaining_style(Style::parse("blue").unwrap_or_default())
        .pulse_style(Style::parse("bold white").unwrap_or_default());
    ocean_bar.set_progress(0.7);
    for seg in ocean_bar.render(width) {
        print!("{}", seg.text);
    }

    // ========================================================================
    // Progress Bar without Brackets
    // ========================================================================
    println!("=== Progress Bar without Brackets ===\n");

    let mut no_brackets = ProgressBar::new().width(40).show_brackets(false);
    no_brackets.set_progress(0.65);
    for seg in no_brackets.render(width) {
        print!("{}", seg.text);
    }

    // ========================================================================
    // Progress Bar with Finished Message
    // ========================================================================
    println!("=== Progress Bar with Finished Message ===\n");

    // In progress
    println!("In progress:");
    let mut task_bar = ProgressBar::new()
        .width(30)
        .description("Processing")
        .finished_message("All files processed!");
    task_bar.set_progress(0.5);
    for seg in task_bar.render(width) {
        print!("{}", seg.text);
    }

    // Finished
    println!("Finished:");
    let mut finished_bar = ProgressBar::new()
        .width(30)
        .description("Processing")
        .finished_message("All files processed!");
    finished_bar.finish();
    for seg in finished_bar.render(width) {
        print!("{}", seg.text);
    }

    // ========================================================================
    // Progress States
    // ========================================================================
    println!("=== Progress States (0%, 25%, 50%, 75%, 100%) ===\n");

    for pct in [0.0, 0.25, 0.5, 0.75, 1.0] {
        let mut state_bar = ProgressBar::new().width(30);
        state_bar.set_progress(pct);
        for seg in state_bar.render(width) {
            print!("{}", seg.text);
        }
    }

    // ========================================================================
    // Multiple Progress Bars (simulating concurrent tasks)
    // ========================================================================
    println!("=== Multiple Progress Bars ===\n");

    let tasks = [
        ("Task 1", 0.90),
        ("Task 2", 0.65),
        ("Task 3", 0.30),
        ("Task 4", 0.10),
    ];

    for (name, progress) in tasks {
        let mut task_progress = ProgressBar::new().width(25).description(name);
        task_progress.set_progress(progress);
        for seg in task_progress.render(width) {
            print!("{}", seg.text);
        }
    }

    // ========================================================================
    // Spinners
    // ========================================================================
    println!("=== Spinner Types ===\n");

    // Dots spinner (default)
    println!("Dots spinner:");
    let mut dots_spinner = Spinner::dots();
    for _ in 0..10 {
        let frame = dots_spinner.next_frame();
        print!("{} ", frame);
    }
    println!("\n");

    // Line spinner
    println!("Line spinner:");
    let mut line_spinner = Spinner::line();
    for _ in 0..12 {
        let frame = line_spinner.next_frame();
        print!("{} ", frame);
    }
    println!("\n");

    // Simple spinner
    println!("Simple spinner:");
    let mut simple_spinner = Spinner::simple();
    for _ in 0..8 {
        let frame = simple_spinner.next_frame();
        print!("{} ", frame);
    }
    println!("\n");

    // Bounce spinner
    println!("Bounce spinner:");
    let mut bounce_spinner = Spinner::bounce();
    for _ in 0..8 {
        let frame = bounce_spinner.next_frame();
        print!("{} ", frame);
    }
    println!("\n");

    // Growing spinner
    println!("Growing spinner:");
    let mut growing_spinner = Spinner::growing();
    for _ in 0..16 {
        let frame = growing_spinner.next_frame();
        print!("{} ", frame);
    }
    println!("\n");

    // Moon phase spinner
    println!("Moon phase spinner:");
    let mut moon_spinner = Spinner::moon();
    for _ in 0..16 {
        let frame = moon_spinner.next_frame();
        print!("{} ", frame);
    }
    println!("\n");

    // Clock spinner
    println!("Clock spinner:");
    let mut clock_spinner = Spinner::clock();
    for _ in 0..24 {
        let frame = clock_spinner.next_frame();
        print!("{} ", frame);
    }
    println!("\n");

    // ========================================================================
    // Styled Spinner
    // ========================================================================
    println!("=== Styled Spinner ===\n");

    let styled_spinner = Spinner::dots().style(Style::parse("bold cyan").unwrap_or_default());
    let segment = styled_spinner.render();
    if let Some(style) = &segment.style {
        let ansi = style.render_ansi(ColorSystem::TrueColor);
        let (prefix, suffix) = &*ansi;
        print!("{}Loading {}{}", prefix, segment.text, suffix);
    }
    println!("\n");

    // ========================================================================
    // Custom Spinner
    // ========================================================================
    println!("=== Custom Spinner ===\n");

    let mut custom_spinner = Spinner::custom(vec![
        "[    ]", "[=   ]", "[==  ]", "[=== ]", "[====]", "[ ===]", "[  ==]", "[   =]",
    ]);
    for _ in 0..16 {
        let frame = custom_spinner.next_frame();
        print!("{} ", frame);
    }
    println!("\n");

    // ========================================================================
    // Combined: Spinner + Progress Bar
    // ========================================================================
    println!("=== Combined: Spinner + Progress Bar ===\n");

    let spinner = Spinner::dots();
    let spinner_frame = spinner.current_frame();
    let mut combined_bar = ProgressBar::new().width(30);
    combined_bar.set_progress(0.33);

    print!("{} Processing: ", spinner_frame);
    for seg in combined_bar.render(width - 15) {
        print!("{}", seg.text);
    }

    // ========================================================================
    // Section Divider
    // ========================================================================
    let rule = Rule::with_title("End of Progress Demo")
        .style(Style::parse("bold green").unwrap_or_default());
    for seg in rule.render(width) {
        print!("{}", seg.text);
    }

    println!();
}
