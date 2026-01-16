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
    /// Full graph insights (`PageRank`, betweenness, cycles)
    Insights(GraphInsightsArgs),
    /// Execution plan with parallel tracks
    Plan(GraphPlanArgs),
    /// Unified triage (best next picks)
    Triage(GraphTriageArgs),
    /// Export dependency graph
    Export(GraphExportArgs),
    /// Show detected cycles
    Cycles(GraphCyclesArgs),
    /// Show top keystone skills (`PageRank`)
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
    let name_map = skills
        .iter()
        .map(|s| (s.id.clone(), s.name.clone()))
        .collect::<std::collections::HashMap<_, _>>();

    match &args.command {
        GraphCommand::Insights(_) => run_insights(ctx, &client, &issues, &name_map),
        GraphCommand::Plan(_) => run_plan(ctx, &client, &issues),
        GraphCommand::Triage(_) => run_triage(ctx, &client, &issues),
        GraphCommand::Export(export) => run_export(ctx, &client, &issues, export),
        GraphCommand::Cycles(cycles) => run_cycles(ctx, &client, &issues, cycles),
        GraphCommand::Keystones(top) => run_top(ctx, &client, &issues, &name_map, top, "Keystones"),
        GraphCommand::Bottlenecks(top) => {
            run_top(ctx, &client, &issues, &name_map, top, "Bottlenecks")
        }
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

fn run_insights(
    ctx: &AppContext,
    client: &BvClient,
    issues: &[crate::beads::Issue],
    names: &std::collections::HashMap<String, String>,
) -> Result<()> {
    let value: serde_json::Value = run_bv_on_issues(client, issues, &["--robot-insights"])?;
    if ctx.robot_mode {
        return crate::cli::output::emit_json(&value);
    }
    let cycles = value
        .get("Cycles")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let keystones = value
        .get("Keystones")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let bottlenecks = value
        .get("Bottlenecks")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    println!("Graph insights:");
    println!("  cycles: {}", cycles.len());
    println!("  keystones: {}", keystones.len());
    println!("  bottlenecks: {}", bottlenecks.len());

    print_cycles_table(&cycles, names, 5);
    print_metric_table("Keystones", &keystones, names, 10);
    print_metric_table("Bottlenecks", &bottlenecks, names, 10);
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
            println!("  highest_impact: {best}");
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
            println!("Top recommendation: {first}");
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
    println!("{graph}");
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
    println!("Cycles (showing {limit}):");
    for cycle in cycles.iter().take(limit) {
        println!("  {cycle}");
    }
    Ok(())
}

fn run_top(
    ctx: &AppContext,
    client: &BvClient,
    issues: &[crate::beads::Issue],
    names: &std::collections::HashMap<String, String>,
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

    print_metric_table(key, &items, names, args.limit);
    Ok(())
}

struct MetricEntry {
    id: String,
    score: Option<f64>,
    name: Option<String>,
}

fn resolve_metric_items(
    items: &[serde_json::Value],
    names: &std::collections::HashMap<String, String>,
) -> Vec<MetricEntry> {
    let mut out = Vec::new();
    for item in items {
        let mut id = None;
        let mut score = None;
        if let Some(obj) = item.as_object() {
            id = obj.get("id").and_then(|v| v.as_str()).map(std::string::ToString::to_string);
            score = obj.get("value").and_then(serde_json::Value::as_f64);
        } else if let Some(arr) = item.as_array() {
            if let Some(first) = arr.first().and_then(|v| v.as_str()) {
                id = Some(first.to_string());
                score = arr.get(1).and_then(serde_json::Value::as_f64);
            }
        }
        if let Some(id) = id {
            let name = names.get(&id).cloned();
            out.push(MetricEntry { id, score, name });
        }
    }
    out
}

fn print_metric_table(
    title: &str,
    items: &[serde_json::Value],
    names: &std::collections::HashMap<String, String>,
    limit: usize,
) {
    if let Some(table) = format_metric_table(title, items, names, limit) {
        println!();
        println!("{table}");
    }
}

fn print_cycles_table(
    cycles: &[serde_json::Value],
    names: &std::collections::HashMap<String, String>,
    limit: usize,
) {
    if let Some(table) = format_cycles_table(cycles, names, limit) {
        println!();
        println!("{table}");
    }
}

fn format_metric_table(
    title: &str,
    items: &[serde_json::Value],
    names: &std::collections::HashMap<String, String>,
    limit: usize,
) -> Option<String> {
    if items.is_empty() {
        return None;
    }
    let mut resolved = resolve_metric_items(items, names);
    resolved.truncate(limit.min(resolved.len()));
    if resolved.is_empty() {
        return None;
    }

    let mut lines = Vec::new();
    lines.push(format!("{} (showing {}):", title, resolved.len()));
    lines.push(format!(
        "{:>4} {:>10} {:36} {}",
        "Rank", "Score", "Skill ID", "Name"
    ));
    lines.push(format!(
        "{:>4} {:>10} {:36} {}",
        "----", "-----", "--------", "----"
    ));
    for (idx, entry) in resolved.iter().enumerate() {
        let score_str = entry
            .score.map_or_else(|| "-".to_string(), |s| format!("{s:.4}"));
        lines.push(format!(
            "{:>4} {:>10} {:36} {}",
            idx + 1,
            score_str,
            entry.id,
            entry.name.clone().unwrap_or_default()
        ));
    }
    Some(lines.join("\n"))
}

fn format_cycles_table(
    cycles: &[serde_json::Value],
    names: &std::collections::HashMap<String, String>,
    limit: usize,
) -> Option<String> {
    if cycles.is_empty() {
        return None;
    }
    let limit = limit.min(cycles.len());
    let mut lines = Vec::new();
    lines.push(format!("Cycles (showing {limit}):"));
    for (idx, cycle) in cycles.iter().take(limit).enumerate() {
        let chain = cycle
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .map(|id| match names.get(id) {
                        Some(name) => format!("{id} ({name})"),
                        None => id.to_string(),
                    })
                    .collect::<Vec<_>>()
                    .join(" -> ")
            })
            .unwrap_or_else(|| cycle.to_string());
        lines.push(format!("  {:>2}. {}", idx + 1, chain));
    }
    Some(lines.join("\n"))
}

fn run_health(ctx: &AppContext, client: &BvClient, issues: &[crate::beads::Issue]) -> Result<()> {
    let value: serde_json::Value = run_bv_on_issues(client, issues, &["--robot-label-health"])?;
    if ctx.robot_mode {
        return crate::cli::output::emit_json(&value);
    }
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_metric_table_renders() {
        let items = vec![
            serde_json::json!({"id": "skill-a", "value": 0.12345}),
            serde_json::json!(["skill-b", 0.9]),
        ];
        let names = std::collections::HashMap::from([
            ("skill-a".to_string(), "Skill A".to_string()),
            ("skill-b".to_string(), "Skill B".to_string()),
        ]);

        let table = format_metric_table("Keystones", &items, &names, 1).unwrap();
        assert!(table.contains("Keystones (showing 1):"));
        assert!(table.contains("skill-a"));
        assert!(table.contains("Skill A"));
        assert!(table.contains("0.1235"));
    }

    #[test]
    fn format_cycles_table_renders() {
        let cycles = vec![serde_json::json!(["skill-a", "skill-b"])];
        let names = std::collections::HashMap::from([
            ("skill-a".to_string(), "Skill A".to_string()),
            ("skill-b".to_string(), "Skill B".to_string()),
        ]);

        let table = format_cycles_table(&cycles, &names, 5).unwrap();
        assert!(table.contains("Cycles (showing 1):"));
        assert!(table.contains("skill-a (Skill A) -> skill-b (Skill B)"));
    }

    #[test]
    fn format_tables_empty() {
        let names = std::collections::HashMap::<String, String>::new();
        assert!(format_metric_table("Keystones", &[], &names, 5).is_none());
        assert!(format_cycles_table(&[], &names, 5).is_none());
    }
}
