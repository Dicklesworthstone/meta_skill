//! JSON deep-dive scene for demo_showcase.
//!
//! Demonstrates rich_rust's JSON rendering capabilities including:
//! - Pretty-printed JSON with syntax highlighting
//! - Semantic coloring (keys, strings, numbers, booleans, null)
//! - Custom themes
//! - Graceful handling when json feature is disabled

use std::sync::Arc;

use rich_rust::console::Console;
#[cfg(not(feature = "json"))]
use rich_rust::markup::render_or_plain;
#[cfg(not(feature = "json"))]
use rich_rust::renderables::panel::Panel;
use rich_rust::style::Style;

use crate::Config;
use crate::scenes::{Scene, SceneError};

/// JSON deep-dive scene: demonstrates JSON rendering capabilities.
pub struct JsonScene;

impl JsonScene {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Scene for JsonScene {
    fn name(&self) -> &'static str {
        "json"
    }

    fn summary(&self) -> &'static str {
        "JSON deep-dive: pretty-printing, theming, and API payloads."
    }

    fn run(&self, console: &Arc<Console>, cfg: &Config) -> Result<(), SceneError> {
        console.print("[section.title]JSON: API Payload Visualization[/]");
        console.print("");

        #[cfg(feature = "json")]
        {
            let _ = cfg;
            run_json_demo(console);
        }

        #[cfg(not(feature = "json"))]
        {
            run_json_disabled_notice(console, cfg);
        }

        Ok(())
    }
}

/// Run the JSON demo when the feature is enabled.
#[cfg(feature = "json")]
fn run_json_demo(console: &Arc<Console>) {
    console
        .print("[dim]Json renderable provides pretty-printed, syntax-highlighted JSON output.[/]");
    console.print("");

    // Demo 1: API Request payload
    render_request_payload(console);

    console.print("");

    // Demo 2: API Response payload
    render_response_payload(console);

    console.print("");

    // Demo 3: Theme customization
    render_theme_demo(console);

    console.print("");
    console.print(
        "[hint]Json automatically highlights keys, strings, numbers, booleans, and null values.[/]",
    );
}

/// Render a deployment API request payload.
#[cfg(feature = "json")]
fn render_request_payload(console: &Console) {
    use rich_rust::renderables::json::Json;
    use rich_rust::segment::Segment;

    console.print("[brand.accent]API Request: Deploy Service[/]");
    console.print("");
    console.print("[cyan]POST /api/v1/deployments[/]");
    console.print("");

    let request_json = r#"{
  "action": "deploy",
  "service": "nebula-api",
  "version": "2.4.1",
  "environment": "production",
  "config": {
    "replicas": 3,
    "memory_limit": "512Mi",
    "cpu_limit": "500m",
    "health_check": {
      "path": "/health",
      "interval_seconds": 30,
      "timeout_seconds": 5
    }
  },
  "rollout": {
    "strategy": "rolling",
    "max_unavailable": 1,
    "max_surge": 1
  },
  "notify": ["ops@example.com", "dev@example.com"],
  "dry_run": false
}"#;

    if let Ok(json) = Json::from_str(request_json) {
        let json = json.indent(2);
        let mut segments = json.render();
        segments.push(Segment::plain("\n"));
        console.print_segments(&segments);
    }
}

/// Render a deployment API response payload.
#[cfg(feature = "json")]
fn render_response_payload(console: &Console) {
    use rich_rust::renderables::json::Json;
    use rich_rust::segment::Segment;

    console.print("[brand.accent]API Response: Deployment Status[/]");
    console.print("");
    console.print("[green]200 OK[/]");
    console.print("");

    let response_json = r#"{
  "status": "success",
  "deployment_id": "dep-7f3a9b2c",
  "service": "nebula-api",
  "version": "2.4.1",
  "timestamp": "2026-01-27T14:32:18Z",
  "details": {
    "replicas_ready": 3,
    "replicas_total": 3,
    "health_status": "healthy",
    "endpoints": [
      "https://nebula-api.prod.example.com",
      "https://nebula-api-internal.prod.example.com"
    ]
  },
  "metrics": {
    "deploy_duration_ms": 2847,
    "rollout_waves": 2,
    "health_check_passed": true
  },
  "previous_version": "2.4.0",
  "rollback_available": true
}"#;

    if let Ok(json) = Json::from_str(response_json) {
        let json = json.indent(2);
        let mut segments = json.render();
        segments.push(Segment::plain("\n"));
        console.print_segments(&segments);
    }
}

/// Demonstrate JSON theme customization.
#[cfg(feature = "json")]
fn render_theme_demo(console: &Console) {
    use rich_rust::renderables::json::{Json, JsonTheme};
    use rich_rust::segment::Segment;

    console.print("[brand.accent]Custom Theme[/]");
    console.print("");
    console.print("[dim]Customize colors for different JSON elements:[/]");
    console.print("");

    let sample_json = r#"{"error": null, "count": 42, "active": true, "name": "test"}"#;

    // Create a custom warm theme
    let warm_theme = JsonTheme {
        key: Style::parse("bold #ff6b6b").unwrap_or_default(),
        string: Style::parse("#feca57").unwrap_or_default(),
        number: Style::parse("#48dbfb").unwrap_or_default(),
        bool_true: Style::parse("#ff9ff3").unwrap_or_default(),
        bool_false: Style::parse("#ff9ff3").unwrap_or_default(),
        null: Style::parse("dim italic #c8d6e5").unwrap_or_default(),
        bracket: Style::parse("#576574").unwrap_or_default(),
        punctuation: Style::parse("#576574").unwrap_or_default(),
    };

    if let Ok(json) = Json::from_str(sample_json) {
        let json = json.theme(warm_theme);
        let mut segments = json.render();
        segments.push(Segment::plain("\n"));
        console.print_segments(&segments);
    }

    console.print("");
    console.print("[dim]Default theme: [bold blue]keys[/], [green]strings[/], [cyan]numbers[/], [bright_green italic]true[/]/[bright_red italic]false[/], [magenta italic]null[/][/]");
}

/// Show notice when json feature is disabled.
#[cfg(not(feature = "json"))]
fn run_json_disabled_notice(console: &Arc<Console>, cfg: &Config) {
    let content = render_or_plain(
        "[bold]JSON feature not enabled[/]\n\n\
         The JSON renderable requires the [cyan]json[/] feature.\n\n\
         To enable JSON support, build with:\n\n\
         [cyan]cargo build --features json[/]\n\n\
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
    console.print("[dim]When enabled, Json renderable provides:[/]");
    console.print("[dim]  - Pretty-printed output with configurable indentation[/]");
    console.print("[dim]  - Semantic syntax highlighting[/]");
    console.print("[dim]  - Key sorting option[/]");
    console.print("[dim]  - Customizable color themes[/]");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_scene_has_correct_name() {
        let scene = JsonScene::new();
        assert_eq!(scene.name(), "json");
    }

    #[test]
    fn json_scene_runs_without_error() {
        let scene = JsonScene::new();
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
