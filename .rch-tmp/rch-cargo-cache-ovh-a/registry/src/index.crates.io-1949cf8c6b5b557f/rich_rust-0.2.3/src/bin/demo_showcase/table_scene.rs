//! Table showcase scene for demo_showcase.
//!
//! Demonstrates rich_rust Table capabilities including:
//! - Header styling and column justification
//! - Width constraints (min/max)
//! - Markup cells with status badges
//! - Numeric alignment
//! - ASCII fallback mode

use std::sync::Arc;

use rich_rust::console::Console;
use rich_rust::renderables::table::{Column, Table};
use rich_rust::style::Style;
use rich_rust::text::JustifyMethod;

use crate::Config;
use crate::scenes::{Scene, SceneError};
use crate::state::{ServiceHealth, ServiceInfo};

/// Table showcase scene: demonstrates Table rendering capabilities.
pub struct TableScene;

impl TableScene {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Scene for TableScene {
    fn name(&self) -> &'static str {
        "table"
    }

    fn summary(&self) -> &'static str {
        "Table showcase: styles, alignment, badges, and ASCII fallback."
    }

    fn run(&self, console: &Arc<Console>, _cfg: &Config) -> Result<(), SceneError> {
        console.print("[section.title]Tables: Structured Data Display[/]");
        console.print("");
        console.print("[dim]Tables organize data with headers, alignment, and styling.[/]");
        console.print("");

        // Demo 1: Service status table with badges
        render_service_table(console);

        console.print("");

        // Demo 2: Metrics table with numeric alignment
        render_metrics_table(console);

        console.print("");

        // Demo 3: ASCII fallback demonstration
        render_ascii_table(console);

        Ok(())
    }
}

/// Render a service status table with status badges.
fn render_service_table(console: &Console) {
    console.print("[brand.accent]Service Status Table[/]");
    console.print("");

    let services = sample_services();

    let mut table = Table::new().title("Services");

    // Configure columns with different styles and alignments
    table.add_column(
        Column::new("Service")
            .style(Style::parse("bold cyan").unwrap_or_default())
            .min_width(10),
    );
    table.add_column(Column::new("Region").justify(JustifyMethod::Center));
    table.add_column(Column::new("Version").justify(JustifyMethod::Center));
    table.add_column(
        Column::new("Latency")
            .justify(JustifyMethod::Right)
            .style(Style::parse("dim").unwrap_or_default()),
    );
    table.add_column(Column::new("Status").justify(JustifyMethod::Center));

    // Add rows with markup for status badges
    for svc in &services {
        let latency = format!("{}ms", svc.latency.as_millis());
        let status = match svc.health {
            ServiceHealth::Ok => "[bold green]OK[/]",
            ServiceHealth::Warn => "[bold yellow]WARN[/]",
            ServiceHealth::Err => "[bold red]ERR[/]",
        };

        table.add_row_markup([&svc.name, "us-west-2", &svc.version, &latency, status]);
    }

    console.print_renderable(&table);

    console.print("");
    console.print("[hint]Status badges use markup for semantic coloring.[/]");
}

/// Render a metrics table demonstrating numeric alignment.
fn render_metrics_table(console: &Console) {
    console.print("[brand.accent]Metrics Table (Numeric Alignment)[/]");
    console.print("");

    let mut table = Table::new().title("System Metrics");

    table.add_column(
        Column::new("Metric")
            .style(Style::parse("bold").unwrap_or_default())
            .min_width(15),
    );
    table.add_column(
        Column::new("Current")
            .justify(JustifyMethod::Right)
            .min_width(10),
    );
    table.add_column(
        Column::new("Average")
            .justify(JustifyMethod::Right)
            .min_width(10),
    );
    table.add_column(
        Column::new("Peak")
            .justify(JustifyMethod::Right)
            .min_width(10),
    );
    table.add_column(Column::new("Status").justify(JustifyMethod::Center));

    // Add metrics data with consistent numeric formatting
    let metrics = [
        ("CPU Usage", "42%", "38%", "89%", "[green]Normal[/]"),
        ("Memory", "6.2 GB", "5.8 GB", "7.9 GB", "[green]Normal[/]"),
        (
            "Disk I/O",
            "145 MB/s",
            "98 MB/s",
            "312 MB/s",
            "[yellow]Elevated[/]",
        ),
        (
            "Network",
            "1.2 Gbps",
            "0.8 Gbps",
            "2.1 Gbps",
            "[green]Normal[/]",
        ),
        ("Connections", "847", "623", "1,204", "[green]Normal[/]"),
    ];

    for (metric, current, avg, peak, status) in metrics {
        table.add_row_markup([metric, current, avg, peak, status]);
    }

    console.print_renderable(&table);

    console.print("");
    console.print("[hint]Right-justified columns align numeric values for easy comparison.[/]");
}

/// Render an ASCII table demonstrating fallback mode.
fn render_ascii_table(console: &Console) {
    console.print("[brand.accent]ASCII Fallback Mode[/]");
    console.print("");
    console.print("[dim]For terminals without Unicode support:[/]");
    console.print("");

    let mut table = Table::new().title("Deployment History").ascii();

    table.add_column(Column::new("ID").style(Style::parse("cyan").unwrap_or_default()));
    table.add_column(Column::new("Time").justify(JustifyMethod::Center));
    table.add_column(Column::new("Duration").justify(JustifyMethod::Right));
    table.add_column(Column::new("Result").justify(JustifyMethod::Center));

    let deployments = [
        ("d-7f3a2b", "14:32:05", "2m 15s", "[green]Success[/]"),
        ("d-8e2c1a", "13:45:22", "1m 48s", "[green]Success[/]"),
        ("d-6d4b9c", "12:18:41", "3m 02s", "[red]Rollback[/]"),
        ("d-5a3e8f", "11:05:33", "2m 31s", "[green]Success[/]"),
    ];

    for (id, time, duration, result) in deployments {
        table.add_row_markup([id, time, duration, result]);
    }

    console.print_renderable(&table);

    console.print("");
    console.print("[hint]ASCII mode uses +, -, and | for borders (works everywhere).[/]");
}

/// Generate sample service data.
fn sample_services() -> Vec<ServiceInfo> {
    use std::time::Duration;

    vec![
        ServiceInfo {
            name: "api".to_string(),
            health: ServiceHealth::Ok,
            latency: Duration::from_millis(12),
            version: "2.4.1".to_string(),
        },
        ServiceInfo {
            name: "auth".to_string(),
            health: ServiceHealth::Ok,
            latency: Duration::from_millis(8),
            version: "1.9.0".to_string(),
        },
        ServiceInfo {
            name: "db".to_string(),
            health: ServiceHealth::Warn,
            latency: Duration::from_millis(45),
            version: "3.2.0".to_string(),
        },
        ServiceInfo {
            name: "cache".to_string(),
            health: ServiceHealth::Ok,
            latency: Duration::from_millis(2),
            version: "1.0.5".to_string(),
        },
        ServiceInfo {
            name: "worker".to_string(),
            health: ServiceHealth::Ok,
            latency: Duration::from_millis(23),
            version: "2.1.3".to_string(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_scene_has_correct_name() {
        let scene = TableScene::new();
        assert_eq!(scene.name(), "table");
    }

    #[test]
    fn table_scene_runs_without_error() {
        let scene = TableScene::new();
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
    fn sample_services_returns_valid_data() {
        let services = sample_services();
        assert!(!services.is_empty());
        for svc in &services {
            assert!(!svc.name.is_empty());
            assert!(!svc.version.is_empty());
        }
    }
}
