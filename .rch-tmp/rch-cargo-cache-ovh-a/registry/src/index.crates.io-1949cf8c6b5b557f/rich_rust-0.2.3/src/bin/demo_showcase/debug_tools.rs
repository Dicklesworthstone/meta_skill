//! Debug tools scene for demo_showcase.
//!
//! Demonstrates Pretty and Inspect renderables on real demo structs.

use std::sync::Arc;
use std::time::Duration;

use rich_rust::console::Console;
use rich_rust::renderables::pretty::{Inspect, Pretty};

use crate::Config;
use crate::scenes::{Scene, SceneError};
use crate::state::{DemoState, FailureEvent, FailureScenario, ServiceHealth, ServiceInfo};

/// Debug tools scene: Pretty and Inspect demonstration.
pub struct DebugToolsScene;

impl DebugToolsScene {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Scene for DebugToolsScene {
    fn name(&self) -> &'static str {
        "debug_tools"
    }

    fn summary(&self) -> &'static str {
        "Pretty/Inspect + Traceback + RichLogger (+ tracing)."
    }

    fn run(&self, console: &Arc<Console>, _cfg: &Config) -> Result<(), SceneError> {
        console.print("[section.title]Debug Tools: Pretty & Inspect[/]");
        console.print("");
        console.print("[dim]rich_rust provides debugging tools for inspecting Rust values.[/]");
        console.print("");

        // Demo 1: Pretty printing a simple struct
        render_pretty_demo(console);

        console.print("");

        // Demo 2: Inspect a more complex struct
        render_inspect_demo(console);

        console.print("");

        // Demo 3: Failure context inspection
        render_failure_demo(console);

        Ok(())
    }
}

/// Demonstrate Pretty on a simple struct.
fn render_pretty_demo(console: &Console) {
    console.print("[brand.accent]Pretty: Styled Debug Output[/]");
    console.print("");

    // Create a sample service
    let service = ServiceInfo {
        name: "api".to_string(),
        health: ServiceHealth::Ok,
        latency: Duration::from_millis(12),
        version: "1.2.3".to_string(),
    };

    console.print("[dim]ServiceInfo struct:[/]");
    console.print("");

    // Use Pretty to render
    let pretty = Pretty::new(&service);
    console.print_renderable(&pretty);

    console.print("");
    console.print("[hint]Pretty uses the Debug trait to render values with optional wrapping.[/]");
}

/// Demonstrate Inspect with type information.
fn render_inspect_demo(console: &Console) {
    console.print("[brand.accent]Inspect: Value + Type Information[/]");
    console.print("");

    // Create a sample state snapshot
    let state = DemoState::demo_seeded(42, 12345);
    let snapshot = crate::state::DemoStateSnapshot::from(&state);

    console.print("[dim]DemoStateSnapshot (partial):[/]");
    console.print("");

    // Use Inspect to render with type info
    let inspect = Inspect::new(&snapshot);
    console.print_renderable(&inspect);

    console.print("");
    console.print("[hint]Inspect shows the Rust type name and extracts struct fields.[/]");
}

/// Demonstrate inspecting failure context.
fn render_failure_demo(console: &Console) {
    console.print("[brand.accent]Failure Context Inspection[/]");
    console.print("");

    // Create a sample failure event
    let failure = FailureEvent::new(FailureScenario::DatabaseTimeout, Duration::from_secs(45));

    console.print("[dim]FailureEvent (database timeout):[/]");
    console.print("");

    // Pretty print the context
    let context_pretty = Pretty::new(&failure.context);
    console.print_renderable(&context_pretty);

    console.print("");
    console.print("[dim]Stack trace frames:[/]");
    console.print("");

    // Show stack frames
    for frame in &failure.stack_frames {
        console.print(&format!(
            "  [brand.accent]{}[/] at [dim]{}:{}[/]",
            frame.function, frame.file, frame.line
        ));
        for (var, val) in &frame.locals {
            console.print(&format!("    [dim]{var}[/] = [status.info]{val}[/]"));
        }
    }

    console.print("");
    console.print("[hint]Failure events capture context for debugging and display.[/]");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_tools_scene_has_correct_name() {
        let scene = DebugToolsScene::new();
        assert_eq!(scene.name(), "debug_tools");
    }

    #[test]
    fn debug_tools_scene_runs_without_error() {
        let scene = DebugToolsScene::new();
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
