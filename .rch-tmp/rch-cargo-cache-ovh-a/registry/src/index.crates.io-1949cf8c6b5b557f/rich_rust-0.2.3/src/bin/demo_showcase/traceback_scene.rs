//! Traceback error scene for demo_showcase.
//!
//! Demonstrates the Traceback renderable with constructed demo frames,
//! showing how rich_rust renders exception information.

use std::sync::Arc;
use std::time::Duration;

use rich_rust::console::Console;
use rich_rust::renderables::traceback::{Traceback, TracebackFrame};

use crate::Config;
use crate::log_pane::LogPane;
use crate::scenes::{Scene, SceneError};
use crate::state::{FailureScenario, LogLevel, LogLine};

/// Traceback error scene: demonstrates exception rendering.
pub struct TracebackScene;

impl TracebackScene {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Scene for TracebackScene {
    fn name(&self) -> &'static str {
        "traceback"
    }

    fn summary(&self) -> &'static str {
        "Controlled error with Traceback + exception panel."
    }

    fn run(&self, console: &Arc<Console>, _cfg: &Config) -> Result<(), SceneError> {
        console.print("[section.title]Traceback: Error Visualization[/]");
        console.print("");
        console.print("[dim]When things go wrong, rich_rust helps you see what happened.[/]");
        console.print("");

        // Show escalating log messages
        render_log_escalation(console);

        console.print("");

        // Show the traceback
        render_traceback(console);

        Ok(())
    }
}

/// Render escalating log messages leading up to the error.
fn render_log_escalation(console: &Console) {
    console.print("[brand.accent]Log Escalation[/]");
    console.print("");

    // Create demo logs that escalate from info -> warn -> error
    let logs = vec![
        LogLine {
            t: Duration::from_secs(45),
            level: LogLevel::Info,
            message: "[deploy] Starting deployment validation...".to_string(),
        },
        LogLine {
            t: Duration::from_secs(46),
            level: LogLevel::Info,
            message: "[deploy] Checking service dependencies...".to_string(),
        },
        LogLine {
            t: Duration::from_secs(47),
            level: LogLevel::Warn,
            message: "[db] Connection pool nearing capacity (8/10)".to_string(),
        },
        LogLine {
            t: Duration::from_secs(48),
            level: LogLevel::Warn,
            message: "[db] Query latency exceeding threshold: 450ms".to_string(),
        },
        LogLine {
            t: Duration::from_secs(49),
            level: LogLevel::Error,
            message: "[db] Connection timeout after 30s - aborting".to_string(),
        },
    ];

    let log_pane = LogPane::new(logs).limit(5);
    console.print_renderable(&log_pane);
}

/// Render the traceback panel.
fn render_traceback(console: &Console) {
    console.print("[brand.accent]Exception Traceback[/]");
    console.print("");

    // Create a failure event directly (no need for full DemoState setup)
    let failure =
        crate::state::FailureEvent::new(FailureScenario::DatabaseTimeout, Duration::from_secs(45));

    // Convert our StackFrames to TracebackFrames
    let frames: Vec<TracebackFrame> = failure
        .stack_frames
        .iter()
        .map(|sf| {
            // Create demo source context for each frame
            let source = generate_source_context(&sf.function);
            TracebackFrame::new(&sf.function, sf.line as usize)
                .filename(&sf.file)
                .source_context(source, (sf.line as usize).saturating_sub(2).max(1))
        })
        .collect();

    let traceback =
        Traceback::new(frames, "DatabaseConnectionError", &failure.message).extra_lines(2);

    // Use the official print_exception API
    console.print_exception(&traceback);

    console.print("");
    console.print(
        "[hint]Tracebacks show the call stack with source context and the exception message.[/]",
    );
}

/// Generate demo source context for a function.
fn generate_source_context(function: &str) -> String {
    // Extract the function name (last part after ::)
    let func_name = function.split("::").last().unwrap_or(function);

    // Generate plausible source lines around the error
    let indent = "    ";
    match func_name {
        "connect" => format!(
            "{indent}let pool = ConnectionPool::new(config)?;\n\
             {indent}pool.set_timeout(Duration::from_secs(30));\n\
             {indent}pool.connect().await?;  // <-- timeout here\n\
             {indent}Ok(pool)"
        ),
        "mark_complete" => format!(
            "{indent}let conn = self.pool.get().await?;\n\
             {indent}conn.execute(UPDATE_QUERY, &[&deployment_id])?;\n\
             {indent}self.notify_completion(deployment_id)?;\n\
             {indent}Ok(())"
        ),
        "run" => format!(
            "{indent}info!(\"Starting stage: {{}}\", self.name);\n\
             {indent}self.pre_checks()?;\n\
             {indent}self.execute_tasks().await?;\n\
             {indent}self.post_checks()?;"
        ),
        _ => format!(
            "{indent}// Function: {func_name}\n\
             {indent}let result = operation()?;\n\
             {indent}process(result)?;\n\
             {indent}Ok(())"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traceback_scene_has_correct_name() {
        let scene = TracebackScene::new();
        assert_eq!(scene.name(), "traceback");
    }

    #[test]
    fn traceback_scene_runs_without_error() {
        let scene = TracebackScene::new();
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
