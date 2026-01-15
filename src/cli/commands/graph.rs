//! ms graph - skill graph analysis via bv.

use std::path::PathBuf;

use clap::{Args, Subcommand};

use crate::app::AppContext;
use crate::error::{MsError, Result};
use crate::graph::bv::{BvClient, run_bv_on_issues, run_bv_on_issues_raw};
use crate::graph::skills::skills_to_issues;

#[derive(Args, Debug)]
pub struct GraphArgs {
    #[command(subcommand)]
    pub command: GraphCommand,

    /// Path to bv binary (default: bv)
    #[arg(long)]
    pub bv_path: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum GraphCommand {
    /// Full graph insights (PageRank, betweenness, cycles)
    Insights(GraphInsightsArgs),
    /// Execution plan with parallel tracks
    Plan(GraphPlanArgs),
    /// Unified triage (best next picks)
    Triage(GraphTriageArgs),
    /// Export dependency graph
    Export(GraphExportArgs),
    /// Show detected cycles
    Cycles(GraphCyclesArgs),
    /// Show top keystone skills (PageRank)
    Keystones(GraphTopArgs),
    /// Show top bottleneck skills (betweenness)
    Bottlenecks(GraphTopArgs),
    /// Label health summary
    Health(GraphHealthArgs),
}

#[derive(Args, Debug, Default)]
pub struct GraphInsightsArgs {}

#[derive(Args, Debug, Default)]
pub struct GraphPlanArgs {}

#[derive(Args, Debug, Default)]
pub struct GraphTriageArgs {}

#[derive(Args, Debug)]
pub struct GraphExportArgs {
    /// Output format: json, dot, mermaid
    #[arg(long, default_value = "json")]
    pub format: String,
}

#[derive(Args, Debug)]
pub struct GraphCyclesArgs {
    /// Max cycles to display
    #[arg(long, default_value = "10")]
    pub limit: usize,
}

#[derive(Args, Debug)]
pub struct GraphTopArgs {
    /// Max items to display
    #[arg(long, default_value = "10")]
    pub limit: usize,
}

#[derive(Args, Debug, Default)]
pub struct GraphHealthArgs {}

pub fn run(ctx: &AppContext, args: &GraphArgs) -> Result<()> {
    let client = if let Some(ref path) = args.bv_path {
        BvClient::with_binary(path)
    } else {
        BvClient::new()
    };

    if !client.is_available() {
        return Err(MsError::NotFound(
            "bv is not available on PATH (install beads_viewer or set --bv-path)".to_string(),
        ));
    }

    let skills = load_all_skills(ctx)?;
    let issues = skills_to_issues(&skills)?;

    match &args.command {
        GraphCommand::Insights(_) => run_insights(ctx, &client, &issues),
        GraphCommand::Plan(_) => run_plan(ctx, &client, &issues),
        GraphCommand::Triage(_) => run_triage(ctx, &client, &issues),
        GraphCommand::Export(export) => run_export(ctx, &client, &issues, export),
        GraphCommand::Cycles(cycles) => run_cycles(ctx, &client, &issues, cycles),
        GraphCommand::Keystones(top) => run_top(ctx, &client, &issues, top, "Keystones"),
        GraphCommand::Bottlenecks(top) => run_top(ctx, &client, &issues, top, "Bottlenecks"),
        GraphCommand::Health(_) => run_health(ctx, &client, &issues),
    }
}

fn load_all_skills(ctx: &AppContext) -> Result<Vec<crate::storage::sqlite::SkillRecord>> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    let limit = 1000usize;
    loop {
        let batch = ctx.db.list_skills(limit, offset)?;
        let count = batch.len();
        if count == 0 {
            break;
        }
        offset += count;
        out.extend(batch);
        if count < limit {
            break;
        }
    }
    Ok(out)
}

fn run_insights(ctx: &AppContext, client: &BvClient, issues: &[crate::beads::Issue]) -> Result<()> {
    let value: serde_json::Value = run_bv_on_issues(client, issues, &["--robot-insights"])?;
    if ctx.robot_mode {
        return crate::cli::output::emit_json(&value);
    }
    let cycles = value
        .get("Cycles")
        .and_then(|v| v.as_array())
        .map(|v| v.len())
        .unwrap_or(0);
    let keystones = value
        .get("Keystones")
        .and_then(|v| v.as_array())
        .map(|v| v.len())
        .unwrap_or(0);
    let bottlenecks = value
        .get("Bottlenecks")
        .and_then(|v| v.as_array())
        .map(|v| v.len())
        .unwrap_or(0);
    println!("Graph insights:");
    println!("  cycles: {}", cycles);
    println!("  keystones: {}", keystones);
    println!("  bottlenecks: {}", bottlenecks);
    Ok(())
}

fn run_plan(ctx: &AppContext, client: &BvClient, issues: &[crate::beads::Issue]) -> Result<()> {
    let value: serde_json::Value = run_bv_on_issues(client, issues, &["--robot-plan"])?;
    if ctx.robot_mode {
        return crate::cli::output::emit_json(&value);
    }
    println!("Graph plan:");
    if let Some(summary) = value.get("plan").and_then(|v| v.get("summary")) {
        if let Some(best) = summary.get("highest_impact") {
            println!("  highest_impact: {}", best);
        }
    }
    Ok(())
}

fn run_triage(ctx: &AppContext, client: &BvClient, issues: &[crate::beads::Issue]) -> Result<()> {
    let value: serde_json::Value = run_bv_on_issues(client, issues, &["--robot-triage"])?;
    if ctx.robot_mode {
        return crate::cli::output::emit_json(&value);
    }
    if let Some(recs) = value.get("recommendations").and_then(|v| v.as_array()) {
        if let Some(first) = recs.first() {
            println!("Top recommendation: {}", first);
            return Ok(());
        }
    }
    println!("No recommendations found.");
    Ok(())
}

fn run_export(
    ctx: &AppContext,
    client: &BvClient,
    issues: &[crate::beads::Issue],
    args: &GraphExportArgs,
) -> Result<()> {
    let arg = format!("--graph-format={}", args.format);
    if args.format == "json" {
        let value: serde_json::Value = run_bv_on_issues(client, issues, &["--robot-graph", &arg])?;
        if ctx.robot_mode {
            return crate::cli::output::emit_json(&value);
        }
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }

    let output = run_bv_on_issues_raw(client, issues, &["--robot-graph", &arg])?;
    let graph = String::from_utf8_lossy(&output).to_string();
    if ctx.robot_mode {
        let value = serde_json::json!({
            "status": "ok",
            "format": args.format,
            "graph": graph,
        });
        return crate::cli::output::emit_json(&value);
    }
    println!("{}", graph);
    Ok(())
}

fn run_cycles(
    ctx: &AppContext,
    client: &BvClient,
    issues: &[crate::beads::Issue],
    args: &GraphCyclesArgs,
) -> Result<()> {
    let value: serde_json::Value = run_bv_on_issues(client, issues, &["--robot-insights"])?;
    let cycles = value
        .get("Cycles")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    if ctx.robot_mode {
        let output = serde_json::json!({
            "status": "ok",
            "count": cycles.len(),
            "cycles": cycles,
        });
        return crate::cli::output::emit_json(&output);
    }

    let limit = args.limit.min(cycles.len());
    println!("Cycles (showing {}):", limit);
    for cycle in cycles.iter().take(limit) {
        println!("  {}", cycle);
    }
    Ok(())
}

fn run_top(
    ctx: &AppContext,
    client: &BvClient,
    issues: &[crate::beads::Issue],
    args: &GraphTopArgs,
    key: &str,
) -> Result<()> {
    let value: serde_json::Value = run_bv_on_issues(client, issues, &["--robot-insights"])?;
    let items = value
        .get(key)
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    if ctx.robot_mode {
        let output = serde_json::json!({
            "status": "ok",
            "count": items.len(),
            "items": items,
        });
        return crate::cli::output::emit_json(&output);
    }

    let limit = args.limit.min(items.len());
    println!("{} (showing {}):", key, limit);
    for item in items.iter().take(limit) {
        println!("  {}", item);
    }
    Ok(())
}

fn run_health(ctx: &AppContext, client: &BvClient, issues: &[crate::beads::Issue]) -> Result<()> {
    let value: serde_json::Value = run_bv_on_issues(client, issues, &["--robot-label-health"])?;
    if ctx.robot_mode {
        return crate::cli::output::emit_json(&value);
    }
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
