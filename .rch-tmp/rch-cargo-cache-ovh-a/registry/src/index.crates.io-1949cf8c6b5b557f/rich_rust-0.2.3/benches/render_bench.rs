//! Benchmarks for rich_rust rendering.

use criterion::{Criterion, criterion_group, criterion_main};
use rich_rust::cells::cell_len;
use rich_rust::color::Color;
use rich_rust::prelude::*;
use rich_rust::style::Style;
use rich_rust::text::Text;
use std::hint::black_box;

fn benchmark_text_render(c: &mut Criterion) {
    let mut text = Text::new("Hello, World! This is a test string for benchmarking.");
    text.stylize(0, 5, Style::new().bold());
    text.stylize(7, 12, Style::new().italic());

    c.bench_function("text_render", |b| {
        b.iter(|| {
            black_box(text.render(""));
        });
    });
}

fn benchmark_text_wrap(c: &mut Criterion) {
    let text = Text::new(
        "This is a longer string that needs to be wrapped to fit within a certain width. It contains multiple words and should demonstrate the wrapping algorithm.",
    );

    c.bench_function("text_wrap_80", |b| {
        b.iter(|| {
            black_box(text.wrap(80));
        });
    });

    c.bench_function("text_wrap_40", |b| {
        b.iter(|| {
            black_box(text.wrap(40));
        });
    });
}

fn benchmark_style_parse(c: &mut Criterion) {
    c.bench_function("style_parse_simple", |b| {
        b.iter(|| black_box(Style::parse("bold red")));
    });

    c.bench_function("style_parse_complex", |b| {
        b.iter(|| black_box(Style::parse("bold italic underline red on blue")));
    });
}

fn benchmark_style_render(c: &mut Criterion) {
    use rich_rust::color::ColorSystem;

    let simple_style = Style::new().bold();
    let complex_style = Style::new()
        .bold()
        .italic()
        .color(Color::from_rgb(255, 100, 50))
        .bgcolor(Color::from_rgb(0, 50, 100));
    let link_style = Style::new().bold().link("https://example.com/page");
    let text = "Hello, World!";

    c.bench_function("style_render_simple", |b| {
        b.iter(|| {
            black_box(simple_style.render(text, ColorSystem::TrueColor));
        });
    });

    c.bench_function("style_render_complex", |b| {
        b.iter(|| {
            black_box(complex_style.render(text, ColorSystem::TrueColor));
        });
    });

    c.bench_function("style_render_with_link", |b| {
        b.iter(|| {
            black_box(link_style.render(text, ColorSystem::TrueColor));
        });
    });

    c.bench_function("style_make_ansi_codes", |b| {
        b.iter(|| black_box(complex_style.make_ansi_codes(ColorSystem::TrueColor)));
    });

    // Test buffer reuse pattern
    c.bench_function("style_make_ansi_codes_into", |b| {
        let mut buffer = String::with_capacity(64);
        b.iter(|| {
            buffer.clear();
            complex_style.make_ansi_codes_into(ColorSystem::TrueColor, &mut buffer);
            black_box(buffer.len())
        });
    });
}

fn benchmark_color_parse(c: &mut Criterion) {
    c.bench_function("color_parse_named", |b| {
        b.iter(|| black_box(Color::parse("red")));
    });

    c.bench_function("color_parse_hex", |b| {
        b.iter(|| black_box(Color::parse("#ff5733")));
    });

    c.bench_function("color_parse_rgb", |b| {
        b.iter(|| black_box(Color::parse("rgb(255, 87, 51)")));
    });

    c.bench_function("color_parse_indexed", |b| {
        b.iter(|| black_box(Color::parse("color(196)")));
    });
}

fn benchmark_cell_len(c: &mut Criterion) {
    let ascii = "Hello, World!";
    let cjk = "你好世界こんにちは";
    let emoji = "Hello 👋🌍🎉 World";
    let mixed = "Hello 你好 👋 World こんにちは";
    let long_ascii = "a".repeat(100);

    c.bench_function("cell_len_ascii_short", |b| {
        b.iter(|| black_box(cell_len(ascii)));
    });

    c.bench_function("cell_len_cjk", |b| {
        b.iter(|| black_box(cell_len(cjk)));
    });

    c.bench_function("cell_len_emoji", |b| {
        b.iter(|| black_box(cell_len(emoji)));
    });

    c.bench_function("cell_len_mixed", |b| {
        b.iter(|| black_box(cell_len(mixed)));
    });

    c.bench_function("cell_len_long_ascii", |b| {
        b.iter(|| black_box(cell_len(&long_ascii)));
    });
}

fn benchmark_table_render(c: &mut Criterion) {
    // Small table: 3x3
    let mut small_table = Table::new();
    small_table = small_table
        .with_column(Column::new("A"))
        .with_column(Column::new("B"))
        .with_column(Column::new("C"));
    small_table.add_row_cells(["1", "2", "3"]);
    small_table.add_row_cells(["4", "5", "6"]);
    small_table.add_row_cells(["7", "8", "9"]);

    c.bench_function("table_render_3x3", |b| {
        b.iter(|| {
            let segments: Vec<_> = black_box(small_table.render(80));
            black_box(segments)
        });
    });

    // Medium table: 10x5
    let mut medium_table = Table::new();
    medium_table = medium_table
        .with_column(Column::new("Name"))
        .with_column(Column::new("Age"))
        .with_column(Column::new("City"))
        .with_column(Column::new("Country"))
        .with_column(Column::new("Score"));
    for i in 0..10 {
        medium_table.add_row_cells([
            format!("User{i}"),
            format!("{}", 20 + i),
            "New York".to_string(),
            "USA".to_string(),
            format!("{}", 80 + i),
        ]);
    }

    c.bench_function("table_render_10x5", |b| {
        b.iter(|| {
            let segments: Vec<_> = black_box(medium_table.render(120));
            black_box(segments)
        });
    });
}

fn benchmark_panel_render(c: &mut Criterion) {
    let panel = Panel::from_text("This is a panel with some content inside.")
        .title("Title")
        .subtitle("Subtitle")
        .width(60);

    c.bench_function("panel_render", |b| {
        b.iter(|| {
            let segments: Vec<_> = black_box(panel.render(80));
            black_box(segments)
        });
    });
}

// =============================================================================
// Conformance Test Benchmarks
// =============================================================================
// These benchmarks reuse the conformance test cases for consistent performance
// baselines. See tests/conformance/ for test definitions.

fn benchmark_conformance_text(c: &mut Criterion) {
    use rich_rust::markup;
    use rich_rust::segment::Segment;

    // Plain text
    c.bench_function("conformance_text_plain", |b| {
        b.iter(|| {
            let text = markup::render_or_plain("Hello, World!");
            let segments: Vec<Segment<'static>> = text
                .render("")
                .into_iter()
                .map(Segment::into_owned)
                .collect();
            black_box(segments)
        });
    });

    // Styled text
    c.bench_function("conformance_text_styled", |b| {
        b.iter(|| {
            let text =
                markup::render_or_plain("[bold]Bold [italic]and italic[/italic] text[/bold]");
            let segments: Vec<Segment<'static>> = text
                .render("")
                .into_iter()
                .map(Segment::into_owned)
                .collect();
            black_box(segments)
        });
    });

    // Colored text
    c.bench_function("conformance_text_colored", |b| {
        b.iter(|| {
            let text = markup::render_or_plain("[red]Red[/] and [green]Green[/]");
            let segments: Vec<Segment<'static>> = text
                .render("")
                .into_iter()
                .map(Segment::into_owned)
                .collect();
            black_box(segments)
        });
    });
}

fn benchmark_conformance_rule(c: &mut Criterion) {
    use rich_rust::renderables::rule::Rule;

    c.bench_function("conformance_rule_simple", |b| {
        let rule = Rule::new();
        b.iter(|| black_box(rule.render(40)));
    });

    c.bench_function("conformance_rule_with_title", |b| {
        let rule = Rule::with_title("Section");
        b.iter(|| black_box(rule.render(40)));
    });
}

// =============================================================================
// Tree Rendering Benchmarks
// =============================================================================

fn benchmark_tree_render(c: &mut Criterion) {
    use rich_rust::renderables::tree::{Tree, TreeNode};

    // Simple tree: 3 children
    let simple_root = TreeNode::new("Root")
        .child(TreeNode::new("Child 1"))
        .child(TreeNode::new("Child 2"))
        .child(TreeNode::new("Child 3"));
    let simple_tree = Tree::new(simple_root);

    c.bench_function("tree_render_simple", |b| {
        b.iter(|| {
            let segments: Vec<_> = black_box(simple_tree.render());
            black_box(segments)
        });
    });

    // Deep tree: 4 levels of nesting (build using builder pattern)
    let deep_root = TreeNode::new("Root")
        .child(
            TreeNode::new("L1-0")
                .child(
                    TreeNode::new("L2-0")
                        .child(TreeNode::new("L3-0"))
                        .child(TreeNode::new("L3-1")),
                )
                .child(
                    TreeNode::new("L2-1")
                        .child(TreeNode::new("L3-0"))
                        .child(TreeNode::new("L3-1")),
                )
                .child(
                    TreeNode::new("L2-2")
                        .child(TreeNode::new("L3-0"))
                        .child(TreeNode::new("L3-1")),
                ),
        )
        .child(
            TreeNode::new("L1-1")
                .child(
                    TreeNode::new("L2-0")
                        .child(TreeNode::new("L3-0"))
                        .child(TreeNode::new("L3-1")),
                )
                .child(
                    TreeNode::new("L2-1")
                        .child(TreeNode::new("L3-0"))
                        .child(TreeNode::new("L3-1")),
                )
                .child(
                    TreeNode::new("L2-2")
                        .child(TreeNode::new("L3-0"))
                        .child(TreeNode::new("L3-1")),
                ),
        )
        .child(
            TreeNode::new("L1-2")
                .child(
                    TreeNode::new("L2-0")
                        .child(TreeNode::new("L3-0"))
                        .child(TreeNode::new("L3-1")),
                )
                .child(
                    TreeNode::new("L2-1")
                        .child(TreeNode::new("L3-0"))
                        .child(TreeNode::new("L3-1")),
                )
                .child(
                    TreeNode::new("L2-2")
                        .child(TreeNode::new("L3-0"))
                        .child(TreeNode::new("L3-1")),
                ),
        );
    let deep_tree = Tree::new(deep_root);

    c.bench_function("tree_render_deep", |b| {
        b.iter(|| {
            let segments: Vec<_> = black_box(deep_tree.render());
            black_box(segments)
        });
    });
}

// =============================================================================
// Markup Parsing Benchmarks
// =============================================================================

fn benchmark_markup_parse(c: &mut Criterion) {
    use rich_rust::markup;

    // Simple markup
    c.bench_function("markup_parse_simple", |b| {
        b.iter(|| black_box(markup::render_or_plain("[bold]Hello[/bold]")));
    });

    // Complex nested markup
    c.bench_function("markup_parse_nested", |b| {
        b.iter(|| {
            black_box(markup::render_or_plain(
                "[bold][red]Error:[/red] [italic]Something went wrong[/italic][/bold]",
            ))
        });
    });

    // Long markup with many tags
    let long_markup = (0..20)
        .map(|i| format!("[bold]Item {i}[/bold] "))
        .collect::<String>();

    c.bench_function("markup_parse_long", |b| {
        b.iter(|| black_box(markup::render_or_plain(&long_markup)));
    });

    // Plain text (no markup)
    c.bench_function("markup_parse_plain", |b| {
        b.iter(|| {
            black_box(markup::render_or_plain(
                "Just plain text with no markup at all",
            ))
        });
    });
}

// =============================================================================
// Color Downgrade Benchmarks
// =============================================================================

fn benchmark_color_downgrade(c: &mut Criterion) {
    use rich_rust::color::ColorSystem;

    let truecolor = Color::from_rgb(255, 128, 64);

    c.bench_function("color_downgrade_to_256", |b| {
        b.iter(|| black_box(truecolor.downgrade(ColorSystem::EightBit)));
    });

    c.bench_function("color_downgrade_to_16", |b| {
        b.iter(|| black_box(truecolor.downgrade(ColorSystem::Standard)));
    });
}

// =============================================================================
// Large Input Stress Tests
// =============================================================================

fn benchmark_stress_large_text(c: &mut Criterion) {
    // 10KB of text
    let large_text = "Lorem ipsum dolor sit amet. ".repeat(400);
    let text = Text::new(&large_text);

    c.bench_function("stress_text_render_10kb", |b| {
        b.iter(|| black_box(text.render("")));
    });

    c.bench_function("stress_text_wrap_10kb", |b| {
        b.iter(|| black_box(text.wrap(80)));
    });
}

fn benchmark_stress_large_table(c: &mut Criterion) {
    // 50x10 table
    let mut large_table = Table::new();
    for col in 0..10 {
        large_table = large_table.with_column(Column::new(format!("Col{col}")));
    }
    for row in 0..50 {
        let cells: Vec<String> = (0..10).map(|col| format!("R{row}C{col}")).collect();
        large_table.add_row_cells(cells);
    }

    c.bench_function("stress_table_50x10", |b| {
        b.iter(|| {
            let segments: Vec<_> = black_box(large_table.render(200));
            black_box(segments)
        });
    });
}

criterion_group!(
    benches,
    benchmark_text_render,
    benchmark_text_wrap,
    benchmark_style_parse,
    benchmark_style_render,
    benchmark_color_parse,
    benchmark_cell_len,
    benchmark_table_render,
    benchmark_panel_render,
    benchmark_conformance_text,
    benchmark_conformance_rule,
    benchmark_tree_render,
    benchmark_markup_parse,
    benchmark_color_downgrade,
    benchmark_stress_large_text,
    benchmark_stress_large_table,
);
criterion_main!(benches);
