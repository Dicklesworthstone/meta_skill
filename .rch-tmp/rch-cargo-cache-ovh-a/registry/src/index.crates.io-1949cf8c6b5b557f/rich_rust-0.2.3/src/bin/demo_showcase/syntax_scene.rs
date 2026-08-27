//! Syntax highlighting deep-dive scene for demo_showcase.
//!
//! Demonstrates rich_rust's syntax highlighting capabilities including:
//! - Code highlighting for multiple languages (Rust, TOML, YAML)
//! - Line numbers with configurable start line
//! - Theme selection (Solarized, base16-ocean, etc.)
//! - Graceful handling when syntax feature is disabled

use std::sync::Arc;

use rich_rust::console::Console;
#[cfg(not(feature = "syntax"))]
use rich_rust::markup::render_or_plain;
#[cfg(not(feature = "syntax"))]
use rich_rust::renderables::panel::Panel;
#[cfg(not(feature = "syntax"))]
use rich_rust::style::Style;

use crate::Config;
use crate::scenes::{Scene, SceneError};

/// Syntax highlighting deep-dive scene.
pub struct SyntaxScene;

impl SyntaxScene {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Scene for SyntaxScene {
    fn name(&self) -> &'static str {
        "syntax"
    }

    fn summary(&self) -> &'static str {
        "Syntax deep-dive: code highlighting, line numbers, and themes."
    }

    fn run(&self, console: &Arc<Console>, cfg: &Config) -> Result<(), SceneError> {
        console.print("[section.title]Syntax: Code Highlighting[/]");
        console.print("");

        #[cfg(feature = "syntax")]
        {
            let _ = cfg;
            run_syntax_demo(console);
        }

        #[cfg(not(feature = "syntax"))]
        {
            run_syntax_disabled_notice(console, cfg);
        }

        Ok(())
    }
}

/// Run the syntax demo when the feature is enabled.
#[cfg(feature = "syntax")]
fn run_syntax_demo(console: &Arc<Console>) {
    console.print("[dim]Syntax renderable provides highlighting for 100+ languages with configurable themes.[/]");
    console.print("");

    // Demo 1: TOML config file
    render_toml_config(console);

    console.print("");

    // Demo 2: YAML pipeline
    render_yaml_pipeline(console);

    console.print("");

    // Demo 3: Rust code snippet
    render_rust_code(console);

    console.print("");

    // Demo 4: Theme comparison
    render_theme_comparison(console);

    console.print("");
    console.print("[hint]Use .theme() to select from available themes, .line_numbers(true) to show line numbers.[/]");
}

/// Render a TOML deployment config.
#[cfg(feature = "syntax")]
fn render_toml_config(console: &Console) {
    use rich_rust::renderables::syntax::Syntax;
    use rich_rust::segment::Segment;

    console.print("[brand.accent]Deployment Config (TOML)[/]");
    console.print("");

    let toml_code = r#"[deployment]
name = "nebula-api"
version = "2.4.1"
environment = "production"

[deployment.resources]
replicas = 3
memory = "512Mi"
cpu = "500m"

[deployment.health_check]
path = "/health"
interval = "30s"
timeout = "5s"

[deployment.rollout]
strategy = "rolling"
max_unavailable = 1
max_surge = 1"#;

    // Try TOML first, fall back to plain text if syntax not available
    let syntax = Syntax::new(toml_code, "toml")
        .line_numbers(true)
        .theme("base16-ocean.dark");

    match syntax.render(None) {
        Ok(segments) => {
            let mut output = segments;
            output.push(Segment::plain("\n"));
            console.print_segments(&output);
        }
        Err(_) => {
            // TOML syntax not available, use plain text with line numbers
            // Print each line as plain segments to avoid markup parsing
            for (i, line) in toml_code.lines().enumerate() {
                let line_num = format!("{:>2} │ ", i + 1);
                let line_segments = vec![
                    Segment::new(
                        &line_num,
                        Some(rich_rust::style::Style::parse("dim").unwrap_or_default()),
                    )
                    .into_owned(),
                    Segment::plain(line).into_owned(),
                    Segment::plain("\n").into_owned(),
                ];
                console.print_segments(&line_segments);
            }
        }
    }
}

/// Render a YAML CI/CD pipeline.
#[cfg(feature = "syntax")]
fn render_yaml_pipeline(console: &Console) {
    use rich_rust::renderables::syntax::Syntax;
    use rich_rust::segment::Segment;

    console.print("[brand.accent]CI/CD Pipeline (YAML)[/]");
    console.print("");

    let yaml_code = r#"name: deploy-nebula
on:
  push:
    branches: [main]

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Build container
        run: docker build -t nebula-api .
      - name: Run tests
        run: cargo test --release
      - name: Deploy to production
        if: github.ref == 'refs/heads/main'
        run: |
          kubectl apply -f k8s/
          kubectl rollout status deployment/nebula-api"#;

    let syntax = Syntax::new(yaml_code, "yaml")
        .line_numbers(true)
        .theme("Solarized (dark)");

    if let Ok(segments) = syntax.render(None) {
        let mut output = segments;
        output.push(Segment::plain("\n"));
        console.print_segments(&output);
    }
}

/// Render a Rust code snippet.
#[cfg(feature = "syntax")]
fn render_rust_code(console: &Console) {
    use rich_rust::renderables::syntax::Syntax;
    use rich_rust::segment::Segment;

    console.print("[brand.accent]Request Handler (Rust)[/]");
    console.print("");

    let rust_code = r#"/// Handle deployment requests
async fn deploy_handler(
    State(ctx): State<AppContext>,
    Json(req): Json<DeployRequest>,
) -> Result<Json<DeployResponse>, ApiError> {
    // Validate the deployment config
    req.validate()?;

    // Start the deployment
    let deployment = ctx.deployer
        .deploy(&req.service, &req.version)
        .await?;

    // Return the deployment status
    Ok(Json(DeployResponse {
        status: "success".into(),
        deployment_id: deployment.id,
        timestamp: Utc::now(),
    }))
}"#;

    let syntax = Syntax::new(rust_code, "rust")
        .line_numbers(true)
        .start_line(42) // Show as excerpt from larger file
        .theme("base16-ocean.dark");

    if let Ok(segments) = syntax.render(None) {
        let mut output = segments;
        output.push(Segment::plain("\n"));
        console.print_segments(&output);
    }

    console.print("[dim italic]Lines 42-60 of src/handlers/deploy.rs[/]");
}

/// Demonstrate different themes.
#[cfg(feature = "syntax")]
fn render_theme_comparison(console: &Console) {
    use rich_rust::renderables::syntax::Syntax;
    use rich_rust::segment::Segment;

    console.print("[brand.accent]Theme Comparison[/]");
    console.print("");
    console.print("[dim]The same code with different themes:[/]");
    console.print("");

    let sample_code = r#"fn greet(name: &str) -> String {
    format!("Hello, {}!", name)
}"#;

    let themes = [
        ("base16-ocean.dark", "Base16 Ocean (Dark)"),
        ("Solarized (dark)", "Solarized Dark"),
        ("InspiredGitHub", "Inspired GitHub"),
    ];

    for (theme_id, theme_name) in themes {
        console.print(&format!("[dim]{theme_name}:[/]"));

        let syntax = Syntax::new(sample_code, "rust").theme(theme_id);

        if let Ok(segments) = syntax.render(None) {
            let mut output = segments;
            output.push(Segment::plain("\n"));
            console.print_segments(&output);
        }
        console.print("");
    }
}

/// Show notice when syntax feature is disabled.
#[cfg(not(feature = "syntax"))]
fn run_syntax_disabled_notice(console: &Arc<Console>, cfg: &Config) {
    let content = render_or_plain(
        "[bold]Syntax feature not enabled[/]\n\n\
         The Syntax renderable requires the [cyan]syntax[/] feature.\n\n\
         To enable syntax highlighting, build with:\n\n\
         [cyan]cargo build --features syntax[/]\n\n\
         Or enable all content features:\n\n\
         [cyan]cargo build --features full[/]\n\n\
         Or run the full showcase:\n\n\
         [cyan]cargo run --bin demo_showcase --features showcase[/]",
    );
    let title = render_or_plain("[yellow]Feature Required[/]");
    let notice = Panel::from_rich_text(&content, 56)
        .title(title)
        .border_style(Style::parse("yellow").unwrap_or_default())
        .padding((1, 2))
        .width(60)
        .safe_box(cfg.is_safe_box());

    console.print_renderable(&notice);

    console.print("");
    console.print("[dim]When enabled, Syntax renderable provides:[/]");
    console.print("[dim]  - Highlighting for 100+ languages[/]");
    console.print("[dim]  - Multiple color themes[/]");
    console.print("[dim]  - Line numbers with custom start line[/]");
    console.print("[dim]  - Indentation guides[/]");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syntax_scene_has_correct_name() {
        let scene = SyntaxScene::new();
        assert_eq!(scene.name(), "syntax");
    }

    #[test]
    fn syntax_scene_runs_without_error() {
        let scene = SyntaxScene::new();
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
