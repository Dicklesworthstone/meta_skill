//! Dashboard scene for demo_showcase.
//!
//! The centerpiece scene demonstrating rich_rust's Live + Layout capabilities:
//! - Split-screen dashboard with header, main, sidebar, and footer
//! - Real-time pipeline simulation with progress bars
//! - Service health monitoring
//! - Streaming log output
//!
//! This scene runs differently based on interactivity:
//! - Interactive: Live loop with auto-refresh
//! - Non-interactive: Single snapshot render

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use rich_rust::console::Console;
use rich_rust::interactive::Status;
use rich_rust::live::{Live, LiveOptions, VerticalOverflowMethod};
use rich_rust::markup::render_or_plain;
use rich_rust::renderables::Renderable;
use rich_rust::renderables::panel::Panel;
use rich_rust::segment::Segment;
use rich_rust::text::Text;

use crate::Config;
use crate::log_pane::LogPane;
use crate::scenes::{Scene, SceneError};
use crate::simulation::{init_pipeline, run_pipeline};
use crate::state::{
    LogLevel, PipelineStage, ServiceHealth, ServiceInfo, SharedDemoState, StageStatus,
};
use crate::timing::{DemoRng, Timing};

/// Dashboard scene: live split-screen dashboard with pipeline simulation.
pub struct DashboardScene;

impl DashboardScene {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Scene for DashboardScene {
    fn name(&self) -> &'static str {
        "dashboard"
    }

    fn summary(&self) -> &'static str {
        "Live split-screen dashboard (services + pipeline + logs)."
    }

    fn run(&self, console: &Arc<Console>, cfg: &Config) -> Result<(), SceneError> {
        // Brief spinner moment: "Building deployment plan…"
        if let Ok(_status) = Status::new(console, "Building deployment plan…") {
            let duration = if cfg.is_quick() {
                Duration::from_millis(200)
            } else {
                Duration::from_millis(600)
            };
            thread::sleep(duration);
        }

        // Create demo state
        let state = SharedDemoState::new(cfg.run_id(), cfg.seed());
        init_services(&state);
        init_pipeline(&state);

        state.update(|demo| {
            demo.headline = "Nebula Deploy v1.2.3".to_string();
            demo.push_log(LogLevel::Info, "Dashboard initialized");
        });

        // Only use Live mode if both:
        // 1. Interactive mode is allowed (not --no-interactive)
        // 2. The console is actually attached to a terminal
        if cfg.is_interactive() && console.is_terminal() {
            // Live mode: run the simulation with live updates
            run_live_dashboard(console, cfg, &state)?;
        } else {
            // Non-interactive: render a static snapshot
            render_static_dashboard(console, cfg, &state)?;
        }

        Ok(())
    }
}

/// Initialize the service list.
fn init_services(state: &SharedDemoState) {
    state.update(|demo| {
        demo.services = vec![
            ServiceInfo {
                name: "api".to_string(),
                health: ServiceHealth::Ok,
                latency: Duration::from_millis(12),
                version: "1.2.3".to_string(),
            },
            ServiceInfo {
                name: "worker".to_string(),
                health: ServiceHealth::Ok,
                latency: Duration::from_millis(25),
                version: "1.2.3".to_string(),
            },
            ServiceInfo {
                name: "db".to_string(),
                health: ServiceHealth::Ok,
                latency: Duration::from_millis(8),
                version: "13.4".to_string(),
            },
            ServiceInfo {
                name: "cache".to_string(),
                health: ServiceHealth::Warn,
                latency: Duration::from_millis(3),
                version: "7.0".to_string(),
            },
        ];
    });
}

/// Run the dashboard with Live updates.
fn run_live_dashboard(
    console: &Arc<Console>,
    cfg: &Config,
    state: &SharedDemoState,
) -> Result<(), SceneError> {
    let timing = Timing::new(cfg.speed(), cfg.is_quick());
    let mut rng = DemoRng::new(cfg.seed());

    // Clone state for the get_renderable closure
    let state_for_render = state.clone();
    let safe_box = cfg.is_safe_box();

    let options = LiveOptions {
        screen: false,
        auto_refresh: true,
        refresh_per_second: 10.0,
        transient: false, // Keep dashboard visible after stop()
        redirect_stdout: false,
        redirect_stderr: false,
        vertical_overflow: VerticalOverflowMethod::Ellipsis,
    };

    let live = Live::with_options(Arc::clone(console), options).get_renderable(move || {
        let snapshot = state_for_render.snapshot();
        Box::new(DashboardRenderable::new(&snapshot, safe_box))
    });

    live.start(true)?;

    // Run the pipeline simulation
    let success = run_pipeline(state, &timing, &mut rng, true);

    // Final update
    state.update(|demo| {
        if success {
            demo.headline = "Pipeline completed successfully!".to_string();
            demo.push_log(LogLevel::Info, "All stages complete");
        } else {
            demo.headline = "Pipeline failed".to_string();
        }
    });

    // Brief pause to show final state
    timing.sleep(Duration::from_millis(500));

    live.stop()?;

    // Print final summary
    console.print("");
    let snapshot = state.snapshot();
    if success {
        console.print("[bold green]Pipeline completed successfully[/]");
    } else {
        console.print("[bold red]Pipeline failed[/]");
    }
    console.print(&format!(
        "[dim]Completed in {:.1}s[/]",
        snapshot.elapsed.as_secs_f64()
    ));

    Ok(())
}

/// Render a static snapshot of the dashboard (non-interactive mode).
fn render_static_dashboard(
    console: &Arc<Console>,
    cfg: &Config,
    state: &SharedDemoState,
) -> Result<(), SceneError> {
    // Simulate some initial state for the snapshot
    state.update(|demo| {
        demo.headline = "Nebula Deploy v1.2.3 (snapshot)".to_string();

        // Set some stages to show activity
        if !demo.pipeline.is_empty() {
            demo.pipeline[0].status = StageStatus::Done;
            demo.pipeline[0].progress = 1.0;
        }
        if demo.pipeline.len() > 1 {
            demo.pipeline[1].status = StageStatus::Running;
            demo.pipeline[1].progress = 0.42;
            demo.pipeline[1].eta = Some(Duration::from_secs(8));
        }

        // Add some demo logs
        demo.push_log(LogLevel::Info, "Starting deployment pipeline");
        demo.push_log(LogLevel::Debug, "Loading configuration from deploy.toml");
        demo.push_log(LogLevel::Info, "[LINT] Starting");
        demo.push_log(LogLevel::Info, "[LINT] Completed");
        demo.push_log(LogLevel::Info, "[BUILD] Starting");
        demo.push_log(LogLevel::Debug, "Compiling 127 crates...");
    });

    let snapshot = state.snapshot();
    let renderable = DashboardRenderable::new(&snapshot, cfg.is_safe_box());
    console.print_renderable(&renderable);

    console.print("");
    console.print("[hint]Run with --no-interactive false for live updates.[/]");

    Ok(())
}

/// A renderable that combines the full dashboard layout.
///
/// Stores the data needed to render, but builds panels on-demand
/// during rendering to avoid lifetime issues with Panel.
struct DashboardRenderable {
    headline: String,
    run_id: u64,
    seed: u64,
    pipeline: Vec<PipelineStage>,
    services: Vec<ServiceInfo>,
    logs: Vec<crate::state::LogLine>,
    safe_box: bool,
}

impl DashboardRenderable {
    fn new(snapshot: &crate::state::DemoStateSnapshot, safe_box: bool) -> Self {
        Self {
            headline: snapshot.headline.clone(),
            run_id: snapshot.run_id,
            seed: snapshot.seed,
            pipeline: snapshot.pipeline.clone(),
            services: snapshot.services.clone(),
            logs: snapshot.logs.clone(),
            safe_box,
        }
    }

    fn render_pipeline_text(stages: &[PipelineStage]) -> Text {
        let mut lines = Vec::new();

        for stage in stages {
            let status_badge = match stage.status {
                StageStatus::Pending => "[dim]O[/]",
                StageStatus::Running => "[bold yellow]*[/]",
                StageStatus::Done => "[bold green]v[/]",
                StageStatus::Failed => "[bold red]x[/]",
            };

            let progress = if stage.status == StageStatus::Running {
                let pct = (stage.progress * 100.0).round() as u32;
                let filled = (pct as usize) / 5;
                let empty = 20 - filled;
                let bar = "#".repeat(filled);
                let rest = "-".repeat(empty);
                format!(" [cyan][{bar}{rest}][/] {pct:>3}%")
            } else if stage.status == StageStatus::Done {
                " [green][####################][/] 100%".to_string()
            } else {
                " [dim][--------------------][/]   0%".to_string()
            };

            let eta = stage
                .eta
                .map(|d| format!(" [dim]({}s)[/]", d.as_secs()))
                .unwrap_or_default();

            lines.push(format!(
                "{} [bold]{:<12}[/]{}{}",
                status_badge, stage.name, progress, eta
            ));
        }

        render_or_plain(&lines.join("\n"))
    }

    fn render_services_text(services: &[ServiceInfo]) -> Text {
        let mut lines = Vec::new();

        for svc in services {
            let health_badge = match svc.health {
                ServiceHealth::Ok => "[green]OK[/]",
                ServiceHealth::Warn => "[yellow]WARN[/]",
                ServiceHealth::Err => "[red]ERR[/]",
            };

            let latency = if svc.latency.as_millis() > 0 {
                format!("{}ms", svc.latency.as_millis())
            } else {
                "-".to_string()
            };

            lines.push(format!(
                "{:<6} [bold]{:<8}[/] [dim]v{}[/]  {}",
                health_badge, svc.name, svc.version, latency
            ));
        }

        render_or_plain(&lines.join("\n"))
    }
}

impl Renderable for DashboardRenderable {
    fn render<'a>(
        &'a self,
        _console: &Console,
        options: &rich_rust::console::ConsoleOptions,
    ) -> Vec<Segment<'a>> {
        let mut segments = Vec::new();
        let max_width = options.max_width;

        // Header
        let header = render_or_plain(&format!(
            "[bold cyan]{}[/]  [dim]Run #{}  Seed: {}[/]",
            self.headline, self.run_id, self.seed
        ));
        segments.extend(header.render("").into_iter().map(Segment::into_owned));
        segments.push(Segment::new("\n\n".to_string(), None));

        // Pipeline panel
        let pipeline_text = Self::render_pipeline_text(&self.pipeline);
        let pipeline_panel = Panel::from_rich_text(&pipeline_text, 60)
            .title_from_markup("[bold]Pipeline[/]")
            .safe_box(self.safe_box);
        segments.extend(
            pipeline_panel
                .render(max_width)
                .into_iter()
                .map(Segment::into_owned),
        );
        segments.push(Segment::new("\n".to_string(), None));

        // Services panel
        let services_text = Self::render_services_text(&self.services);
        let services_panel = Panel::from_rich_text(&services_text, 30)
            .title_from_markup("[bold]Services[/]")
            .safe_box(self.safe_box);
        segments.extend(
            services_panel
                .render(max_width)
                .into_iter()
                .map(Segment::into_owned),
        );
        segments.push(Segment::new("\n".to_string(), None));

        // Logs panel
        let log_pane = LogPane::from_snapshot(&self.logs, 8);
        let logs_text = log_pane.as_text();
        let logs_panel = Panel::from_rich_text(&logs_text, 80)
            .title_from_markup("[bold]Logs[/]")
            .safe_box(self.safe_box);
        segments.extend(
            logs_panel
                .render(max_width)
                .into_iter()
                .map(Segment::into_owned),
        );

        segments
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dashboard_scene_has_correct_name() {
        let scene = DashboardScene::new();
        assert_eq!(scene.name(), "dashboard");
    }

    #[test]
    fn dashboard_scene_runs_without_error() {
        let scene = DashboardScene::new();
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
    fn dashboard_renderable_creates_without_panic() {
        let state = SharedDemoState::new(1, 42);
        init_services(&state);
        init_pipeline(&state);
        let snapshot = state.snapshot();
        let _ = DashboardRenderable::new(&snapshot, false);
    }
}
