//! End-to-end export demo for HTML/SVG.
//!
//! This script exercises:
//! - HTML export (inline styles)
//! - HTML export with external stylesheet (generated from inline styles)
//! - SVG export (default rendering)
//! - Multiple content types: text, styled text, table, panel, tree
//!
//! Run:
//!   RUST_LOG=debug cargo run --example e2e_export
//!
//! Output:
//!   Files are written to a temp directory. Logs include the output path.
//!
//! Visual verification guidance:
//! - Open the HTML files in a browser and compare inline vs external CSS output.
//! - Open the SVG file in a browser and verify content renders similarly.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use regex::{Captures, Regex};
use rich_rust::markup;
use rich_rust::prelude::*;
use tracing::{debug, info, warn};
use tracing_subscriber::EnvFilter;

const CELL_WIDTH_PX: usize = 8;
const CELL_HEIGHT_PX: usize = 16;

fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

fn output_dir() -> PathBuf {
    let mut dir = std::env::temp_dir();
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    dir.push(format!("rich_rust_export_e2e_{suffix}"));
    dir
}

fn write_file(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)
}

fn externalize_styles(html: &str) -> (String, String, BTreeMap<String, String>) {
    let re = Regex::new(r#"style="([^"]+)""#).expect("valid regex");
    let mut styles: BTreeMap<String, String> = BTreeMap::new();
    let mut counter = 0usize;

    let html_out = re.replace_all(html, |caps: &Captures| {
        let style = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
        let class = styles.entry(style.to_string()).or_insert_with(|| {
            let class = format!("style-{counter}");
            counter += 1;
            class
        });
        format!("class=\"{class}\"")
    });

    let mut css_lines = Vec::new();
    for (style, class) in &styles {
        css_lines.push(format!(".{class}{{{style}}}"));
    }
    (html_out.into_owned(), css_lines.join("\n"), styles)
}

fn extract_colors(css: &str) -> BTreeSet<String> {
    let mut colors = BTreeSet::new();
    let re = Regex::new(r#"(color|background-color):([^;]+);"#).expect("valid regex");
    for caps in re.captures_iter(css) {
        if let Some(value) = caps.get(2) {
            colors.insert(value.as_str().trim().to_string());
        }
    }
    colors
}

fn build_demo_content(console: &Console) {
    console.print("[bold]Export Demo[/]");
    console.print("Plain text output");
    console.print("[italic cyan]Styled text with markup[/]");

    let rule = Rule::with_title("Table + Panel + Tree");
    console.print_renderable(&rule);

    let mut table = Table::new()
        .title("Status")
        .with_column(Column::new("Service"))
        .with_column(Column::new("State").justify(JustifyMethod::Center));
    table.add_row_markup(["API", "[green]OK[/]"]);
    table.add_row_markup(["Worker", "[yellow]Degraded[/]"]);
    table.add_row_markup(["DB", "[red]Down[/]"]);
    console.print_renderable(&table);

    let content = markup::render_or_plain("[bold magenta]Panel content[/]");
    let panel = Panel::from_rich_text(&content, 40)
        .title(markup::render_or_plain("[bold]Export Panel[/]"))
        .subtitle(markup::render_or_plain("[dim]SVG/HTML[/]"))
        .width(50);
    console.print_renderable(&panel);

    let root = TreeNode::new(markup::render_or_plain("[bold]Root[/]"))
        .child(TreeNode::new(markup::render_or_plain("[green]Leaf A[/]")))
        .child(TreeNode::new(markup::render_or_plain("[yellow]Leaf B[/]")));
    let tree = Tree::new(root);
    console.print_renderable(&tree);
}

fn export_bundle(label: &str, out_dir: &Path) -> std::io::Result<()> {
    let console = Console::builder()
        .width(80)
        .height(24)
        .force_terminal(true)
        .markup(true)
        .build();

    console.begin_capture();
    build_demo_content(&console);

    let html_inline = console.export_html(false);
    let svg = console.export_svg(true);

    let html_path = out_dir.join(format!("{label}_inline.html"));
    let svg_path = out_dir.join(format!("{label}.svg"));

    write_file(&html_path, &html_inline)?;
    write_file(&svg_path, &svg)?;

    info!(
        label,
        html = %html_path.display(),
        svg = %svg_path.display(),
        "exported inline html + svg"
    );

    Ok(())
}

fn export_with_external_css(out_dir: &Path) -> std::io::Result<()> {
    let console = Console::builder()
        .width(80)
        .height(24)
        .force_terminal(true)
        .markup(true)
        .build();

    console.begin_capture();
    build_demo_content(&console);
    let html_inline = console.export_html(true);

    let (html_stripped, css, mapping) = externalize_styles(&html_inline);
    let css_path = out_dir.join("export_styles.css");
    let html_path = out_dir.join("export_external.html");

    let html_with_link = html_stripped.replace(
        "</head>",
        "<link rel=\"stylesheet\" href=\"export_styles.css\"></head>",
    );

    write_file(&css_path, &css)?;
    write_file(&html_path, &html_with_link)?;

    let colors = extract_colors(&css);
    debug!(count = mapping.len(), "inline style blocks extracted");
    debug!(?colors, "css color palette");
    info!(
        html = %html_path.display(),
        css = %css_path.display(),
        "exported html with external css"
    );

    Ok(())
}

fn log_export_constants() {
    info!(
        cell_width_px = CELL_WIDTH_PX,
        cell_height_px = CELL_HEIGHT_PX,
        "export uses fixed cell size for SVG sizing"
    );
}

fn main() -> std::io::Result<()> {
    init_logging();
    log_export_constants();

    let out_dir = output_dir();
    fs::create_dir_all(&out_dir)?;
    info!(path = %out_dir.display(), "export output directory");

    export_bundle("export_default", &out_dir)?;
    export_with_external_css(&out_dir)?;

    warn!(
        "SVG export uses SVG primitives (text/rect/clip paths) with optional terminal-window chrome"
    );

    Ok(())
}
