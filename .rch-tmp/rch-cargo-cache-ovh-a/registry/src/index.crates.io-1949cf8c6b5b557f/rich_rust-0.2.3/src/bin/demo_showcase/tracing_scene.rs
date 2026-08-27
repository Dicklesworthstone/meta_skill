//! Tracing integration showcase scene for demo_showcase.
//!
//! Demonstrates rich_rust's tracing integration capabilities including:
//! - RichTracingLayer for beautiful span/event output
//! - Nested spans with timing
//! - Event levels (info, warn, error)
//! - Graceful handling when tracing feature is disabled

use std::sync::Arc;

use rich_rust::console::Console;

use crate::Config;
use crate::scenes::{Scene, SceneError};

/// Tracing showcase scene: demonstrates RichTracingLayer integration.
pub struct TracingScene;

impl TracingScene {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Scene for TracingScene {
    fn name(&self) -> &'static str {
        "tracing"
    }

    fn summary(&self) -> &'static str {
        "Tracing integration: spans, events, and structured logging."
    }

    fn run(&self, console: &Arc<Console>, cfg: &Config) -> Result<(), SceneError> {
        console.print("[section.title]Tracing: Structured Observability[/]");
        console.print("");

        #[cfg(feature = "tracing")]
        {
            let _ = cfg;
            run_tracing_demo(console);
        }

        #[cfg(not(feature = "tracing"))]
        {
            run_tracing_disabled_notice(console, cfg);
        }

        Ok(())
    }
}

/// Run the tracing demo when the feature is enabled.
#[cfg(feature = "tracing")]
fn run_tracing_demo(console: &Arc<Console>) {
    use rich_rust::logging::RichTracingLayer;
    use tracing::{info, info_span, warn};
    use tracing_subscriber::prelude::*;

    console
        .print("[dim]RichTracingLayer provides beautiful output for tracing spans and events.[/]");
    console.print("");

    // Install the tracing layer
    let layer = RichTracingLayer::new(console.clone());
    let subscriber = tracing_subscriber::registry().with(layer);

    // Use the subscriber for this demo only
    tracing::subscriber::with_default(subscriber, || {
        // Demo 1: Basic span with events
        console.print("[brand.accent]Request Processing[/]");
        console.print("");

        let request_span = info_span!("http_request", method = "POST", path = "/api/deploy");
        let _guard = request_span.enter();

        info!("Received deployment request");
        info!(
            version = "2.4.1",
            region = "us-west-2",
            "Starting deployment"
        );

        // Nested span for deployment phase
        {
            let deploy_span = info_span!("deploy_phase", stage = "rollout");
            let _deploy_guard = deploy_span.enter();

            info!("Pulling container image");
            info!(replicas = 3, "Scaling deployment");
            warn!(memory_percent = 85, "Memory usage elevated");
            info!("Health checks passing");
        }

        info!(duration_ms = 2150, "Deployment complete");
    });

    console.print("");
    console.print(
        "[hint]Spans show hierarchical structure; events appear with their parent context.[/]",
    );
}

/// Show notice when tracing feature is disabled.
#[cfg(not(feature = "tracing"))]
fn run_tracing_disabled_notice(console: &Arc<Console>, cfg: &Config) {
    use rich_rust::renderables::panel::Panel;
    use rich_rust::style::Style;

    let notice = Panel::from_text(
        "[bold]Tracing feature not enabled[/]\n\n\
         The tracing integration requires the `tracing` feature.\n\n\
         To enable tracing support, build with:\n\n\
         [cyan]cargo build --features full,tracing[/]\n\n\
         Or add to your Cargo.toml:\n\n\
         [cyan]rich_rust = { features = [\"tracing\"] }[/]",
    )
    .title("[yellow]Feature Required[/]")
    .border_style(Style::parse("yellow").unwrap_or_default())
    .padding((1, 2))
    .width(60)
    .safe_box(cfg.is_safe_box());

    console.print_renderable(&notice);

    console.print("");
    console.print("[dim]When enabled, RichTracingLayer provides:[/]");
    console.print("[dim]  - Colorized span enter/exit with timing[/]");
    console.print("[dim]  - Structured event fields[/]");
    console.print("[dim]  - Hierarchical indentation[/]");
    console.print("[dim]  - Level-based styling (INFO, WARN, ERROR)[/]");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracing_scene_has_correct_name() {
        let scene = TracingScene::new();
        assert_eq!(scene.name(), "tracing");
    }

    #[test]
    fn tracing_scene_runs_without_error() {
        let scene = TracingScene::new();
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
