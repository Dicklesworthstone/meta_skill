//! ms simulate - Simulate skill execution in a sandbox.

use std::path::PathBuf;

use clap::Args;

use crate::app::AppContext;
use crate::cli::output::OutputFormat;
use crate::cli::output::{HumanLayout, emit_human, emit_json};
use crate::error::{MsError, Result};
use crate::simulation::{ElementStatus, SimulationConfig, SimulationEngine, SimulationReport};
use crate::utils::format::truncate_string;

#[derive(Args, Debug)]
pub struct SimulateArgs {
    /// Skill ID or name to simulate
    pub skill: String,

    /// Path to fixtures directory to preload into the sandbox
    #[arg(long)]
    pub with_fixtures: Option<PathBuf>,

    /// Total simulation timeout (e.g., 30s, 2m)
    #[arg(long)]
    pub timeout: Option<String>,

    /// Per-command timeout (e.g., 10s, 1m)
    #[arg(long)]
    pub command_timeout: Option<String>,

    /// Allow network access during simulation
    #[arg(long)]
    pub allow_network: bool,

    /// Allow file system access outside sandbox (best-effort)
    #[arg(long)]
    pub allow_external_fs: bool,

    /// Write JSON transcript to this path
    #[arg(long)]
    pub record_transcript: Option<PathBuf>,
}

pub fn run(ctx: &AppContext, args: &SimulateArgs) -> Result<()> {
    let mut config = SimulationConfig::default();
    if args.allow_network {
        config.allow_network = true;
    }
    if args.allow_external_fs {
        config.allow_external_fs = true;
    }
    if let Some(raw) = args.timeout.as_deref() {
        config.total_timeout =
            parse_duration(raw).ok_or_else(|| MsError::Config("invalid timeout".to_string()))?;
    }
    if let Some(raw) = args.command_timeout.as_deref() {
        config.command_timeout = parse_duration(raw)
            .ok_or_else(|| MsError::Config("invalid command timeout".to_string()))?;
    }

    let engine = SimulationEngine::new(ctx);
    let report = engine.simulate(&args.skill, args.with_fixtures.as_deref(), config)?;

    if let Some(path) = &args.record_transcript {
        let payload = serde_json::to_string_pretty(&report)
            .map_err(|err| MsError::Config(format!("serialize transcript: {err}")))?;
        std::fs::write(path, payload).map_err(|err| {
            MsError::Config(format!("write transcript {}: {err}", path.display()))
        })?;
    }

    if ctx.output_format != OutputFormat::Human {
        return emit_json(&report);
    }

    render_human(&report);
    Ok(())
}

fn render_human(report: &SimulationReport) {
    let mut layout = HumanLayout::new();
    layout.title("Simulation");
    layout.kv(
        "Skill",
        &format!("{} ({})", report.skill_name, report.skill_id),
    );
    layout.kv("Started", &report.started_at);
    layout.kv("Duration", &format!("{}ms", report.duration_ms));
    layout.blank();

    layout.section("Result");
    layout.bullet(&format!("{:?}", report.result));

    if !report.warnings.is_empty() {
        layout.section("Warnings");
        for warning in &report.warnings {
            layout.bullet(warning);
        }
    }

    layout.section("Elements");
    for element in &report.element_results {
        let status = match element.status {
            ElementStatus::Passed => "PASS",
            ElementStatus::Failed => "FAIL",
            ElementStatus::Skipped => "SKIP",
        };
        layout.push_line(format!(
            "[{}] {} ({}ms)",
            status, element.element, element.duration_ms
        ));
        if let Some(note) = &element.note {
            layout.push_line(format!("  note: {note}"));
        }
        if let Some(stderr) = &element.stderr {
            if element.status == ElementStatus::Failed && !stderr.is_empty() {
                layout.push_line(format!("  stderr: {}", truncate_line(stderr, 200)));
            }
        }
    }

    if !report.fs_changes.created.is_empty()
        || !report.fs_changes.modified.is_empty()
        || !report.fs_changes.deleted.is_empty()
    {
        layout.section("File Changes");
        for path in &report.fs_changes.created {
            layout.push_line(format!("+ {path}"));
        }
        for path in &report.fs_changes.modified {
            layout.push_line(format!("~ {path}"));
        }
        for path in &report.fs_changes.deleted {
            layout.push_line(format!("- {path}"));
        }
    }

    if !report.issues.is_empty() {
        layout.section("Issues");
        for issue in &report.issues {
            layout.push_line(format!(
                "[{:?}] {}: {}",
                issue.severity, issue.element, issue.description
            ));
            if let Some(suggestion) = &issue.suggestion {
                layout.push_line(format!("  suggestion: {suggestion}"));
            }
        }
    }

    emit_human(layout);
}

fn parse_duration(raw: &str) -> Option<std::time::Duration> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (value, suffix) = trimmed.chars().partition::<String, _>(char::is_ascii_digit);
    let value: u64 = value.parse().ok()?;
    match suffix.as_str() {
        "ms" => Some(std::time::Duration::from_millis(value)),
        "s" | "" => Some(std::time::Duration::from_secs(value)),
        "m" => Some(std::time::Duration::from_secs(value * 60)),
        _ => None,
    }
}

fn truncate_line(value: &str, max: usize) -> String {
    truncate_string(value, max)
}
