//! ms experiment - Manage skill A/B experiments.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use clap::{Args, Subcommand};
use rand::distr::Distribution;
use rand::Rng;
use rand_distr::Beta;
use serde::{Deserialize, Serialize};

use crate::app::AppContext;
use crate::cli::commands::load::{
    CliPackContract, CliPackMode, DepsMode, LoadArgs, build_robot_payload, load_skill,
    output_human as output_load_human,
};
use crate::cli::output::{HumanLayout, emit_json};
use crate::error::{MsError, Result};
use crate::storage::sqlite::ExperimentEventRecord;

#[derive(Args, Debug)]
pub struct ExperimentArgs {
    #[command(subcommand)]
    pub command: ExperimentCommand,
}

#[derive(Subcommand, Debug)]
pub enum ExperimentCommand {
    /// Create a new experiment
    Create(ExperimentCreateArgs),
    /// List experiments
    List(ExperimentListArgs),
    /// Status for the experiment
    Status(ExperimentStatusArgs),
    /// Assign a variant for the experiment
    Assign(ExperimentAssignArgs),
    /// Assign + load a skill with experiment attribution
    Load(ExperimentLoadArgs),
    /// Record outcome metrics for a variant
    Record(ExperimentRecordArgs),
    /// Conclude an experiment
    Conclude(ExperimentConcludeArgs),
}

#[derive(Args, Debug)]
pub struct ExperimentCreateArgs {
    /// Skill ID or name
    pub skill: String,

    /// Experiment scope: skill or slice
    #[arg(long, default_value = "skill")]
    pub scope: String,

    /// Scope identifier (required when scope is slice)
    #[arg(long)]
    pub scope_id: Option<String>,

    /// Variant id or id:name (repeatable)
    #[arg(long, required = true)]
    pub variant: Vec<String>,

    /// Allocation strategy: uniform, weighted, or bandit
    #[arg(long, default_value = "uniform")]
    pub strategy: String,

    /// Variant weight (id=weight). Required for weighted strategy.
    #[arg(long)]
    pub weight: Vec<String>,

    /// Status for the experiment
    #[arg(long, default_value = "running")]
    pub status: String,
}

#[derive(Args, Debug)]
pub struct ExperimentListArgs {
    /// Filter by skill ID or name
    #[arg(long)]
    pub skill: Option<String>,

    /// Limit results
    #[arg(long, default_value = "20")]
    pub limit: usize,

    /// Offset results
    #[arg(long, default_value = "0")]
    pub offset: usize,
}

#[derive(Args, Debug)]
pub struct ExperimentStatusArgs {
    /// Experiment ID
    pub experiment_id: String,

    /// Metric key to analyze (default: task_success)
    #[arg(long)]
    pub metric: Option<String>,
}

#[derive(Args, Debug)]
pub struct ExperimentAssignArgs {
    /// Experiment ID
    pub experiment_id: String,

    /// Metric key to use for bandit selection (default: task_success)
    #[arg(long)]
    pub metric: Option<String>,

    /// JSON context file for assignment
    #[arg(long)]
    pub context: Option<PathBuf>,

    /// Optional session ID
    #[arg(long)]
    pub session: Option<String>,
}

#[derive(Args, Debug)]
pub struct ExperimentLoadArgs {
    /// Experiment ID
    pub experiment_id: String,

    /// Metric key to use for bandit selection (default: task_success)
    #[arg(long)]
    pub metric: Option<String>,

    /// JSON context file for assignment
    #[arg(long)]
    pub context: Option<PathBuf>,

    /// Optional session ID
    #[arg(long)]
    pub session: Option<String>,

    /// Disclosure level (0=minimal, 1=overview, 2=standard, 3=full, 4=complete)
    #[arg(long, short = 'l')]
    pub level: Option<String>,

    /// Token budget for packing (overrides --level)
    #[arg(long)]
    pub pack: Option<usize>,

    /// Pack mode when using --pack
    #[arg(long, value_enum, default_value = "balanced")]
    pub mode: CliPackMode,

    /// Pack contract preset (requires --pack)
    #[arg(long, value_enum)]
    pub contract: Option<CliPackContract>,

    /// Custom pack contract id (requires --pack)
    #[arg(long)]
    pub contract_id: Option<String>,

    /// Max slices per coverage group
    #[arg(long, default_value = "2")]
    pub max_per_group: usize,

    /// Alias for --level full
    #[arg(long)]
    pub full: bool,

    /// Alias for --level complete (includes scripts + references)
    #[arg(long)]
    pub complete: bool,

    /// Dependency loading strategy
    #[arg(long, value_enum, default_value = "auto")]
    pub deps: DepsMode,
}

#[derive(Args, Debug)]
pub struct ExperimentRecordArgs {
    /// Experiment ID
    pub experiment_id: String,

    /// Variant ID
    pub variant_id: String,

    /// Metrics to record (key=value). Repeatable.
    #[arg(long, required = true)]
    pub metric: Vec<String>,

    /// Optional session ID
    #[arg(long)]
    pub session: Option<String>,
}

#[derive(Args, Debug)]
pub struct ExperimentConcludeArgs {
    /// Experiment ID
    pub experiment_id: String,

    /// Winner variant ID
    #[arg(long)]
    pub winner: String,
}

#[derive(Serialize)]
struct ExperimentRecordOutput {
    id: String,
    skill_id: String,
    scope: String,
    scope_id: Option<String>,
    status: String,
    started_at: String,
    variants: serde_json::Value,
}

pub fn run(ctx: &AppContext, args: &ExperimentArgs) -> Result<()> {
    match &args.command {
        ExperimentCommand::Create(create) => run_create(ctx, create),
        ExperimentCommand::List(list) => run_list(ctx, list),
        ExperimentCommand::Status(status) => run_status(ctx, status),
        ExperimentCommand::Assign(assign) => run_assign(ctx, assign),
        ExperimentCommand::Load(load) => run_load(ctx, load),
        ExperimentCommand::Record(record) => run_record(ctx, record),
        ExperimentCommand::Conclude(conclude) => run_conclude(ctx, conclude),
    }
}

fn run_create(ctx: &AppContext, args: &ExperimentCreateArgs) -> Result<()> {
    if args.scope != "skill" && args.scope != "slice" {
        return Err(MsError::ValidationFailed(
            "scope must be one of: skill, slice".to_string(),
        ));
    }
    if args.scope == "slice" && args.scope_id.is_none() {
        return Err(MsError::ValidationFailed(
            "--scope-id is required when scope is slice".to_string(),
        ));
    }
    if args.scope == "skill" && args.scope_id.is_some() {
        return Err(MsError::ValidationFailed(
            "--scope-id is only valid when scope is slice".to_string(),
        ));
    }

    let strategy = args.strategy.to_lowercase();
    if !matches!(strategy.as_str(), "uniform" | "weighted" | "bandit") {
        return Err(MsError::ValidationFailed(
            "strategy must be one of: uniform, weighted, bandit".to_string(),
        ));
    }

    let skill_id = resolve_skill_id(ctx, &args.skill)?;

    let (variants_json, allocation_json) =
        build_variants_payload(&args.variant, &strategy, &args.weight)?;

    let record = ctx.db.create_skill_experiment(
        &skill_id,
        &args.scope,
        args.scope_id.as_deref(),
        &variants_json,
        &allocation_json,
        &args.status,
    )?;

    if ctx.robot_mode {
        let payload = serde_json::json!({
            "status": "ok",
            "experiment": record,
        });
        return emit_json(&payload);
    }

    let mut layout = HumanLayout::new();
    layout
        .title("Experiment Created")
        .kv("ID", &record.id)
        .kv("Skill", &record.skill_id)
        .kv("Scope", &record.scope)
        .kv("Scope ID", record.scope_id.as_deref().unwrap_or("-"))
        .kv("Status", &record.status)
        .kv("Started", &record.started_at)
        .kv("Variants", &record.variants_json);
    crate::cli::output::emit_human(layout);
    Ok(())
}

fn run_list(ctx: &AppContext, args: &ExperimentListArgs) -> Result<()> {
    let skill_id = match &args.skill {
        Some(skill) => Some(resolve_skill_id(ctx, skill)?),
        None => None,
    };

    let records = ctx
        .db
        .list_skill_experiments(skill_id.as_deref(), args.limit, args.offset)?;

    if ctx.robot_mode {
        let payload = serde_json::json!({
            "status": "ok",
            "count": records.len(),
            "experiments": records,
        });
        return emit_json(&payload);
    }

    if records.is_empty() {
        println!("No experiments found.");
        return Ok(());
    }

    let mut layout = HumanLayout::new();
    layout.title("Experiments");
    for record in records {
        let variants = serde_json::from_str::<serde_json::Value>(&record.variants_json)
            .unwrap_or_else(|_| serde_json::Value::String(record.variants_json.clone()));
        let output = ExperimentRecordOutput {
            id: record.id.clone(),
            skill_id: record.skill_id.clone(),
            scope: record.scope.clone(),
            scope_id: record.scope_id.clone(),
            status: record.status.clone(),
            started_at: record.started_at.clone(),
            variants,
        };
        layout
            .section(&output.id)
            .kv("Skill", &output.skill_id)
            .kv("Scope", &output.scope)
            .kv("Scope ID", output.scope_id.as_deref().unwrap_or("-"))
            .kv("Status", &output.status)
            .kv("Started", &output.started_at)
            .kv("Variants", &format!("{:?}", output.variants))
            .blank();
    }
    crate::cli::output::emit_human(layout);
    Ok(())
}

fn run_status(ctx: &AppContext, args: &ExperimentStatusArgs) -> Result<()> {
    let record = get_experiment(ctx, &args.experiment_id)?;
    let variants = parse_variants_json(&record.variants_json)?;
    let events = ctx.db.list_skill_experiment_events(&record.id)?;
    let metric = resolve_metric_key(args.metric.as_deref(), &events)
        .unwrap_or_else(|| "task_success".to_string());
    let stats = compute_variant_stats(&variants, &events, &metric);
    let analysis = compute_experiment_analysis(&stats);

    if ctx.robot_mode {
        let payload = serde_json::json!({
            "status": "ok",
            "experiment": record,
            "metric": metric,
            "variants": stats,
            "analysis": analysis,
        });
        return emit_json(&payload);
    }

    let mut layout = HumanLayout::new();
    layout
        .title("Experiment Status")
        .kv("ID", &record.id)
        .kv("Skill", &record.skill_id)
        .kv("Scope", &record.scope)
        .kv("Scope ID", record.scope_id.as_deref().unwrap_or("-"))
        .kv("Status", &record.status)
        .kv("Metric", &metric)
        .kv("Started", &record.started_at)
        .blank();

    for stat in &stats {
        let rate = if stat.outcomes > 0 {
            format!("{:.2}%", stat.success_rate * 100.0)
        } else {
            "-".to_string()
        };
        layout
            .section(&stat.id)
            .kv("Name", stat.name.as_deref().unwrap_or("-"))
            .kv("Assignments", &stat.assignments.to_string())
            .kv("Outcomes", &stat.outcomes.to_string())
            .kv("Successes", &stat.successes.to_string())
            .kv("Success rate", &rate)
            .blank();
    }

    if let Some(analysis) = analysis {
        layout
            .section("Analysis")
            .kv(
                "Significance",
                &analysis
                    .significance
                    .map(|s| format!("{:.2}", s))
                    .unwrap_or_else(|| "-".to_string()),
            )
            .kv(
                "p-value",
                &analysis
                    .p_value
                    .map(|p| format!("{:.4}", p))
                    .unwrap_or_else(|| "-".to_string()),
            )
            .kv("Recommendation", &analysis.recommendation);
    }

    crate::cli::output::emit_human(layout);
    Ok(())
}

fn run_assign(ctx: &AppContext, args: &ExperimentAssignArgs) -> Result<()> {
    let record = get_experiment(ctx, &args.experiment_id)?;
    let context_json = match &args.context {
        Some(path) => Some(read_json_file(path)?),
        None => None,
    };
    let events = ctx.db.list_skill_experiment_events(&record.id)?;
    let selection = select_variant_for_experiment(&record, args.metric.as_deref(), &events)?;
    let event = record_assignment_event(
        ctx,
        &record.id,
        &selection.variant.id,
        context_json.as_deref(),
        args.session.as_deref(),
    )?;

    if ctx.robot_mode {
        let payload = serde_json::json!({
            "status": "ok",
            "experiment_id": record.id,
            "metric": selection.metric,
            "variant": selection.variant,
            "event": event,
        });
        return emit_json(&payload);
    }

    let mut layout = HumanLayout::new();
    layout
        .title("Variant Assigned")
        .kv("Experiment", &record.id)
        .kv("Metric", &selection.metric)
        .kv("Variant", &selection.variant.id)
        .kv("Name", selection.variant.name.as_deref().unwrap_or("-"))
        .kv("Strategy", &selection.allocation.strategy);
    crate::cli::output::emit_human(layout);
    Ok(())
}

fn run_load(ctx: &AppContext, args: &ExperimentLoadArgs) -> Result<()> {
    let record = get_experiment(ctx, &args.experiment_id)?;
    let context_json = match &args.context {
        Some(path) => Some(read_json_file(path)?),
        None => None,
    };
    let events = ctx.db.list_skill_experiment_events(&record.id)?;
    let selection = select_variant_for_experiment(&record, args.metric.as_deref(), &events)?;

    let load_args = LoadArgs {
        skill: record.skill_id.clone(),
        level: args.level.clone(),
        pack: args.pack,
        mode: args.mode,
        contract: args.contract,
        contract_id: args.contract_id.clone(),
        max_per_group: args.max_per_group,
        full: args.full,
        complete: args.complete,
        deps: args.deps,
        experiment_id: Some(record.id.clone()),
        variant_id: Some(selection.variant.id.clone()),
    };

    let load_result = load_skill(ctx, &load_args)?;
    let event = record_assignment_event(
        ctx,
        &record.id,
        &selection.variant.id,
        context_json.as_deref(),
        args.session.as_deref(),
    )?;

    if ctx.robot_mode {
        let mut payload = build_robot_payload(&load_result, &load_args);
        if let Some(data) = payload.as_object_mut() {
            data.insert(
                "experiment".to_string(),
                serde_json::json!({
                    "id": record.id,
                    "metric": selection.metric,
                    "variant": selection.variant,
                    "event": event,
                }),
            );
        }
        emit_json(&payload)?;
        return Ok(());
    }

    let mut layout = HumanLayout::new();
    layout
        .title("Assigned + Loading")
        .kv("Experiment", &record.id)
        .kv("Metric", &selection.metric)
        .kv("Variant", &selection.variant.id)
        .kv("Name", selection.variant.name.as_deref().unwrap_or("-"))
        .kv("Strategy", &selection.allocation.strategy);
    crate::cli::output::emit_human(layout);

    output_load_human(ctx, &load_result, &load_args)
}

fn run_record(ctx: &AppContext, args: &ExperimentRecordArgs) -> Result<()> {
    let record = get_experiment(ctx, &args.experiment_id)?;
    let variants = parse_variants_json(&record.variants_json)?;
    if !variants.iter().any(|v| v.id == args.variant_id) {
        return Err(MsError::ValidationFailed(format!(
            "unknown variant id: {}",
            args.variant_id
        )));
    }

    let metrics_json = parse_metric_pairs(&args.metric)?;
    let event = ctx.db.record_skill_experiment_event(
        &record.id,
        &args.variant_id,
        "outcome",
        Some(&metrics_json),
        None,
        args.session.as_deref(),
    )?;

    if ctx.robot_mode {
        let payload = serde_json::json!({
            "status": "ok",
            "event": event,
        });
        return emit_json(&payload);
    }

    let mut layout = HumanLayout::new();
    layout
        .title("Outcome Recorded")
        .kv("Experiment", &record.id)
        .kv("Variant", &args.variant_id)
        .kv("Metrics", &metrics_json);
    crate::cli::output::emit_human(layout);
    Ok(())
}

fn run_conclude(ctx: &AppContext, args: &ExperimentConcludeArgs) -> Result<()> {
    let record = get_experiment(ctx, &args.experiment_id)?;
    let variants = parse_variants_json(&record.variants_json)?;
    if !variants.iter().any(|v| v.id == args.winner) {
        return Err(MsError::ValidationFailed(format!(
            "unknown winner variant id: {}",
            args.winner
        )));
    }

    ctx.db
        .update_skill_experiment_status(&record.id, "concluded")?;

    let metrics_json = serde_json::json!({
        "winner": args.winner,
    })
    .to_string();
    let event = ctx.db.record_skill_experiment_event(
        &record.id,
        &args.winner,
        "conclude",
        Some(&metrics_json),
        None,
        None,
    )?;

    if ctx.robot_mode {
        let payload = serde_json::json!({
            "status": "ok",
            "experiment_id": record.id,
            "winner": args.winner,
            "event": event,
        });
        return emit_json(&payload);
    }

    let mut layout = HumanLayout::new();
    layout
        .title("Experiment Concluded")
        .kv("Experiment", &record.id)
        .kv("Winner", &args.winner);
    crate::cli::output::emit_human(layout);
    Ok(())
}

fn build_variants_payload(
    variants: &[String],
    strategy: &str,
    weights: &[String],
) -> Result<(String, String)> {
    if variants.is_empty() {
        return Err(MsError::ValidationFailed(
            "at least one --variant is required".to_string(),
        ));
    }

    let weight_map = parse_weight_specs(weights)?;
    if strategy == "weighted" && weight_map.is_empty() {
        return Err(MsError::ValidationFailed(
            "--weight is required when strategy is weighted".to_string(),
        ));
    }
    if strategy == "uniform" && !weight_map.is_empty() {
        return Err(MsError::ValidationFailed(
            "--weight is only supported with weighted/bandit strategies".to_string(),
        ));
    }

    let mut seen: HashSet<String> = HashSet::new();
    let mut variants_out = Vec::new();
    let mut weights_out: HashMap<String, f64> = HashMap::new();

    for variant in variants {
        let (id, name) = match variant.split_once(':') {
            Some((id, name)) => (id.trim(), name.trim()),
            None => (variant.as_str(), variant.as_str()),
        };
        if id.is_empty() {
            return Err(MsError::ValidationFailed(
                "variant id cannot be empty".to_string(),
            ));
        }
        if !seen.insert(id.to_string()) {
            return Err(MsError::ValidationFailed(format!(
                "duplicate variant id: {}",
                id
            )));
        }
        let weight = weight_map.get(id).copied();
        variants_out.push(ExperimentVariant {
            id: id.to_string(),
            name: Some(name.to_string()),
            weight,
        });
        if let Some(value) = weight {
            weights_out.insert(id.to_string(), value);
        }
    }

    if !weight_map.is_empty() {
        for (id, _) in &weight_map {
            if !seen.contains(id) {
                return Err(MsError::ValidationFailed(format!(
                    "weight provided for unknown variant id: {}",
                    id
                )));
            }
        }
    }

    apply_weights(strategy, &mut variants_out, &mut weights_out)?;

    let variants_json = serde_json::to_string(&variants_out)
        .map_err(|err| MsError::Serialization(format!("variants serialize: {err}")))?;
    let allocation_json = serde_json::json!({
        "strategy": strategy,
        "weights": weights_out,
    })
    .to_string();

    Ok((variants_json, allocation_json))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExperimentVariant {
    id: String,
    name: Option<String>,
    weight: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
struct AllocationConfig {
    strategy: String,
    #[serde(default)]
    weights: HashMap<String, f64>,
}

#[derive(Debug, Serialize)]
struct VariantStats {
    id: String,
    name: Option<String>,
    assignments: u64,
    outcomes: u64,
    successes: u64,
    success_rate: f64,
}

#[derive(Debug, Serialize)]
struct ExperimentAnalysis {
    p_value: Option<f64>,
    significance: Option<f64>,
    confidence_interval: Option<[f64; 2]>,
    recommendation: String,
}

struct AssignmentSelection {
    metric: String,
    variant: ExperimentVariant,
    allocation: AllocationConfig,
}

fn select_variant_for_experiment(
    record: &crate::storage::sqlite::ExperimentRecord,
    metric: Option<&str>,
    events: &[ExperimentEventRecord],
) -> Result<AssignmentSelection> {
    let variants = parse_variants_json(&record.variants_json)?;
    let allocation = parse_allocation_json(&record.allocation_json)?;
    let metric = resolve_metric_key(metric, events).unwrap_or_else(|| "task_success".to_string());
    let selected = select_variant(&variants, &allocation, events, &metric)?;

    Ok(AssignmentSelection {
        metric,
        variant: selected,
        allocation,
    })
}

fn record_assignment_event(
    ctx: &AppContext,
    experiment_id: &str,
    variant_id: &str,
    context_json: Option<&str>,
    session_id: Option<&str>,
) -> Result<ExperimentEventRecord> {
    ctx.db.record_skill_experiment_event(
        experiment_id,
        variant_id,
        "assign",
        None,
        context_json,
        session_id,
    )
}

fn parse_weight_specs(weights: &[String]) -> Result<HashMap<String, f64>> {
    let mut map = HashMap::new();
    for spec in weights {
        let (id, value) = spec
            .split_once('=')
            .ok_or_else(|| MsError::ValidationFailed(format!("invalid weight: {spec}")))?;
        let id = id.trim();
        let value: f64 = value.trim().parse().map_err(|_| {
            MsError::ValidationFailed(format!("invalid weight value for {id}: {value}"))
        })?;
        if id.is_empty() {
            return Err(MsError::ValidationFailed(
                "weight variant id cannot be empty".to_string(),
            ));
        }
        if value < 0.0 {
            return Err(MsError::ValidationFailed(format!(
                "weight must be >= 0 for {}",
                id
            )));
        }
        if map.insert(id.to_string(), value).is_some() {
            return Err(MsError::ValidationFailed(format!(
                "duplicate weight for {}",
                id
            )));
        }
    }
    Ok(map)
}

fn apply_weights(
    strategy: &str,
    variants: &mut [ExperimentVariant],
    weights_out: &mut HashMap<String, f64>,
) -> Result<()> {
    let count = variants.len();
    if count == 0 {
        return Ok(());
    }

    if weights_out.is_empty() {
        let uniform = 1.0 / (count as f64);
        for variant in variants.iter_mut() {
            variant.weight = Some(uniform);
            weights_out.insert(variant.id.clone(), uniform);
        }
        return Ok(());
    }

    let sum: f64 = weights_out.values().sum();
    if sum <= 0.0 {
        return Err(MsError::ValidationFailed(
            "weights must sum to a positive value".to_string(),
        ));
    }

    for variant in variants.iter_mut() {
        if let Some(weight) = weights_out.get(&variant.id).copied() {
            variant.weight = Some(weight / sum);
        } else if strategy == "weighted" {
            return Err(MsError::ValidationFailed(format!(
                "missing weight for variant {}",
                variant.id
            )));
        } else {
            variant.weight = Some(0.0);
        }
    }

    for value in weights_out.values_mut() {
        *value /= sum;
    }

    Ok(())
}

fn parse_variants_json(json: &str) -> Result<Vec<ExperimentVariant>> {
    serde_json::from_str::<Vec<ExperimentVariant>>(json)
        .map_err(|err| MsError::Serialization(format!("variants parse: {err}")))
}

fn parse_allocation_json(json: &str) -> Result<AllocationConfig> {
    serde_json::from_str::<AllocationConfig>(json)
        .map_err(|err| MsError::Serialization(format!("allocation parse: {err}")))
}

fn read_json_file(path: &PathBuf) -> Result<String> {
    let contents = std::fs::read_to_string(path)?;
    let value: serde_json::Value = serde_json::from_str(&contents)?;
    Ok(value.to_string())
}

fn parse_metric_pairs(pairs: &[String]) -> Result<String> {
    let mut map = serde_json::Map::new();
    for pair in pairs {
        let (key, raw) = pair
            .split_once('=')
            .ok_or_else(|| MsError::ValidationFailed(format!("invalid metric: {pair}")))?;
        let key = key.trim();
        if key.is_empty() {
            return Err(MsError::ValidationFailed(
                "metric key cannot be empty".to_string(),
            ));
        }
        let value = parse_metric_value(raw.trim());
        map.insert(key.to_string(), value);
    }
    Ok(serde_json::Value::Object(map).to_string())
}

fn parse_metric_value(raw: &str) -> serde_json::Value {
    match raw {
        "true" => serde_json::Value::Bool(true),
        "false" => serde_json::Value::Bool(false),
        _ => {
            if let Ok(number) = raw.parse::<i64>() {
                serde_json::Value::Number(number.into())
            } else if let Ok(number) = raw.parse::<f64>() {
                serde_json::Number::from_f64(number)
                    .map(serde_json::Value::Number)
                    .unwrap_or_else(|| serde_json::Value::String(raw.to_string()))
            } else {
                serde_json::Value::String(raw.to_string())
            }
        }
    }
}

fn resolve_metric_key(
    explicit: Option<&str>,
    events: &[ExperimentEventRecord],
) -> Option<String> {
    if let Some(metric) = explicit {
        return Some(metric.to_string());
    }

    let preferred = ["task_success", "explicit_feedback", "success"];
    for key in preferred {
        if events.iter().any(|event| metric_present(event, key)) {
            return Some(key.to_string());
        }
    }

    for event in events.iter().filter(|event| event.event_type == "outcome") {
        if let Some(metrics_json) = event.metrics_json.as_deref() {
            if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(metrics_json) {
                if let Some(key) = map.keys().next() {
                    return Some(key.to_string());
                }
            }
        }
    }

    None
}

fn metric_present(event: &ExperimentEventRecord, key: &str) -> bool {
    if event.event_type != "outcome" {
        return false;
    }
    let Some(metrics_json) = event.metrics_json.as_deref() else {
        return false;
    };
    if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(metrics_json) {
        return map.contains_key(key);
    }
    false
}

fn compute_variant_stats(
    variants: &[ExperimentVariant],
    events: &[ExperimentEventRecord],
    metric: &str,
) -> Vec<VariantStats> {
    let mut stats = Vec::new();
    for variant in variants {
        let assignments = events
            .iter()
            .filter(|event| event.variant_id == variant.id && event.event_type == "assign")
            .count() as u64;
        let mut outcomes = 0u64;
        let mut successes = 0u64;
        for event in events.iter().filter(|event| {
            event.variant_id == variant.id && event.event_type == "outcome"
        }) {
            let Some(metrics_json) = event.metrics_json.as_deref() else {
                continue;
            };
            if let Ok(metrics) = serde_json::from_str::<serde_json::Value>(metrics_json) {
                if let Some(success) = metric_success(&metrics, metric) {
                    outcomes += 1;
                    if success {
                        successes += 1;
                    }
                }
            }
        }
        let success_rate = if outcomes > 0 {
            successes as f64 / outcomes as f64
        } else {
            0.0
        };
        stats.push(VariantStats {
            id: variant.id.clone(),
            name: variant.name.clone(),
            assignments,
            outcomes,
            successes,
            success_rate,
        });
    }
    stats
}

fn metric_success(metrics: &serde_json::Value, key: &str) -> Option<bool> {
    let value = metrics.get(key)?;
    match value {
        serde_json::Value::Bool(value) => Some(*value),
        serde_json::Value::Number(num) => num.as_f64().map(|v| v > 0.5),
        serde_json::Value::String(value) => match value.to_lowercase().as_str() {
            "true" | "yes" | "success" => Some(true),
            "false" | "no" | "failure" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn compute_experiment_analysis(stats: &[VariantStats]) -> Option<ExperimentAnalysis> {
    let mut candidates: Vec<&VariantStats> = stats.iter().filter(|s| s.outcomes > 0).collect();
    if candidates.len() < 2 {
        return Some(ExperimentAnalysis {
            p_value: None,
            significance: None,
            confidence_interval: None,
            recommendation: "Not enough outcome data yet.".to_string(),
        });
    }

    candidates.sort_by(|a, b| {
        b.success_rate
            .partial_cmp(&a.success_rate)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.outcomes.cmp(&a.outcomes))
    });

    let best = candidates[0];
    let runner_up = candidates[1];
    let (p_value, ci) = two_proportion_test(
        best.successes,
        best.outcomes,
        runner_up.successes,
        runner_up.outcomes,
    );

    let (p_value, confidence_interval) = match (p_value, ci) {
        (Some(p), Some(ci)) => (Some(p), Some(ci)),
        _ => (None, None),
    };

    let significance = p_value.map(|p| (1.0 - p).clamp(0.0, 1.0));
    let recommendation = if let Some(p) = p_value {
        if p <= 0.05 {
            format!("{} appears better than {}.", best.id, runner_up.id)
        } else {
            "No significant difference yet. Keep running the experiment.".to_string()
        }
    } else {
        "Not enough outcome data yet.".to_string()
    };

    Some(ExperimentAnalysis {
        p_value,
        significance,
        confidence_interval,
        recommendation,
    })
}

fn two_proportion_test(
    succ_a: u64,
    total_a: u64,
    succ_b: u64,
    total_b: u64,
) -> (Option<f64>, Option<[f64; 2]>) {
    if total_a == 0 || total_b == 0 {
        return (None, None);
    }
    let a = succ_a as f64;
    let b = succ_b as f64;
    let n1 = total_a as f64;
    let n2 = total_b as f64;
    let p1 = a / n1;
    let p2 = b / n2;
    let pooled = (a + b) / (n1 + n2);
    let se = (pooled * (1.0 - pooled) * (1.0 / n1 + 1.0 / n2)).sqrt();
    if se == 0.0 {
        return (None, None);
    }
    let z = (p1 - p2) / se;
    let p_value = 2.0 * (1.0 - normal_cdf(z.abs()));

    let se_diff = (p1 * (1.0 - p1) / n1 + p2 * (1.0 - p2) / n2).sqrt();
    let diff = p1 - p2;
    let ci = [diff - 1.96 * se_diff, diff + 1.96 * se_diff];
    (Some(p_value), Some(ci))
}

fn normal_cdf(z: f64) -> f64 {
    // Abramowitz-Stegun approximation for standard normal CDF.
    let t = 1.0 / (1.0 + 0.2316419 * z);
    let d = 0.3989423 * (-0.5 * z * z).exp();
    let prob = d
        * t
        * (0.3193815
            + t * (-0.3565638 + t * (1.781478 + t * (-1.821256 + t * 1.330274))));
    1.0 - prob
}

fn select_variant(
    variants: &[ExperimentVariant],
    allocation: &AllocationConfig,
    events: &[ExperimentEventRecord],
    metric: &str,
) -> Result<ExperimentVariant> {
    if variants.is_empty() {
        return Err(MsError::ValidationFailed(
            "experiment has no variants".to_string(),
        ));
    }

    match allocation.strategy.as_str() {
        "uniform" => select_uniform(variants),
        "weighted" => select_weighted(variants, &allocation.weights),
        "bandit" => select_bandit(variants, events, metric),
        other => Err(MsError::ValidationFailed(format!(
            "unknown allocation strategy: {}",
            other
        ))),
    }
}

fn select_uniform(variants: &[ExperimentVariant]) -> Result<ExperimentVariant> {
    let mut rng = rand::rng();
    let idx = rng.random_range(0..variants.len());
    Ok(variants[idx].clone())
}

fn select_weighted(
    variants: &[ExperimentVariant],
    weights: &HashMap<String, f64>,
) -> Result<ExperimentVariant> {
    let mut total = 0.0f64;
    let mut entries = Vec::new();
    for variant in variants {
        let weight = weights.get(&variant.id).copied().unwrap_or(0.0);
        if weight > 0.0 {
            total += weight;
        }
        entries.push((variant, weight.max(0.0)));
    }

    if total <= 0.0 {
        return select_uniform(variants);
    }

    let mut rng = rand::rng();
    let mut target = rng.random_range(0.0..total);
    for (variant, weight) in entries {
        if weight <= 0.0 {
            continue;
        }
        if target <= weight {
            return Ok(variant.clone());
        }
        target -= weight;
    }

    Ok(variants.last().cloned().unwrap())
}

fn select_bandit(
    variants: &[ExperimentVariant],
    events: &[ExperimentEventRecord],
    metric: &str,
) -> Result<ExperimentVariant> {
    let stats = compute_variant_stats(variants, events, metric);
    let mut rng = rand::rng();
    let mut best: Option<(f64, &ExperimentVariant)> = None;
    for variant in variants {
        let stat = stats.iter().find(|s| s.id == variant.id);
        let successes = stat.map(|s| s.successes).unwrap_or(0) as f64;
        let outcomes = stat.map(|s| s.outcomes).unwrap_or(0) as f64;
        let failures = (outcomes - successes).max(0.0);
        let alpha = 1.0 + successes;
        let beta = 1.0 + failures;
        let dist = Beta::new(alpha, beta).map_err(|_| {
            MsError::ValidationFailed("invalid bandit beta distribution".to_string())
        })?;
        let sample = dist.sample(&mut rng);
        if best.as_ref().map(|(v, _)| sample > *v).unwrap_or(true) {
            best = Some((sample, variant));
        }
    }

    Ok(best
        .map(|(_, variant)| variant.clone())
        .unwrap_or_else(|| variants[0].clone()))
}

fn get_experiment(ctx: &AppContext, id: &str) -> Result<crate::storage::sqlite::ExperimentRecord> {
    ctx.db
        .get_skill_experiment(id)?
        .ok_or_else(|| MsError::NotFound(format!("experiment not found: {id}")))
}

fn resolve_skill_id(ctx: &AppContext, input: &str) -> Result<String> {
    if let Some(skill) = ctx.db.get_skill(input)? {
        return Ok(skill.id);
    }
    if let Ok(Some(alias)) = ctx.db.resolve_alias(input) {
        if let Some(skill) = ctx.db.get_skill(&alias.canonical_id)? {
            return Ok(skill.id);
        }
    }
    Err(MsError::SkillNotFound(format!("skill not found: {input}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Parser, Subcommand};

    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: TestCommand,
    }

    #[derive(Subcommand)]
    enum TestCommand {
        Experiment(ExperimentArgs),
    }

    #[test]
    fn parse_experiment_create_defaults() {
        let parsed = TestCli::parse_from([
            "test",
            "experiment",
            "create",
            "skill-1",
            "--variant",
            "v1",
            "--variant",
            "v2",
        ]);
        let TestCommand::Experiment(args) = parsed.cmd;
        match args.command {
            ExperimentCommand::Create(create) => {
                assert_eq!(create.skill, "skill-1");
                assert_eq!(create.scope, "skill");
                assert!(create.scope_id.is_none());
                assert_eq!(create.strategy, "uniform");
                assert!(create.weight.is_empty());
                assert_eq!(create.status, "running");
                assert_eq!(create.variant, vec!["v1".to_string(), "v2".to_string()]);
            }
            _ => panic!("expected create"),
        }
    }

    #[test]
    fn parse_experiment_list_defaults() {
        let parsed = TestCli::parse_from(["test", "experiment", "list"]);
        let TestCommand::Experiment(args) = parsed.cmd;
        match args.command {
            ExperimentCommand::List(list) => {
                assert!(list.skill.is_none());
                assert_eq!(list.limit, 20);
                assert_eq!(list.offset, 0);
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn parse_experiment_status() {
        let parsed = TestCli::parse_from([
            "test",
            "experiment",
            "status",
            "exp-1",
            "--metric",
            "task_success",
        ]);
        let TestCommand::Experiment(args) = parsed.cmd;
        match args.command {
            ExperimentCommand::Status(status) => {
                assert_eq!(status.experiment_id, "exp-1");
                assert_eq!(status.metric.as_deref(), Some("task_success"));
            }
            _ => panic!("expected status"),
        }
    }

    #[test]
    fn parse_experiment_assign() {
        let parsed = TestCli::parse_from(["test", "experiment", "assign", "exp-1"]);
        let TestCommand::Experiment(args) = parsed.cmd;
        match args.command {
            ExperimentCommand::Assign(assign) => {
                assert_eq!(assign.experiment_id, "exp-1");
                assert!(assign.metric.is_none());
                assert!(assign.context.is_none());
            }
            _ => panic!("expected assign"),
        }
    }

    #[test]
    fn parse_experiment_record() {
        let parsed = TestCli::parse_from([
            "test",
            "experiment",
            "record",
            "exp-1",
            "variant-a",
            "--metric",
            "task_success=true",
        ]);
        let TestCommand::Experiment(args) = parsed.cmd;
        match args.command {
            ExperimentCommand::Record(record) => {
                assert_eq!(record.experiment_id, "exp-1");
                assert_eq!(record.variant_id, "variant-a");
                assert_eq!(record.metric, vec!["task_success=true".to_string()]);
            }
            _ => panic!("expected record"),
        }
    }

    #[test]
    fn parse_experiment_conclude() {
        let parsed = TestCli::parse_from([
            "test",
            "experiment",
            "conclude",
            "exp-1",
            "--winner",
            "control",
        ]);
        let TestCommand::Experiment(args) = parsed.cmd;
        match args.command {
            ExperimentCommand::Conclude(conclude) => {
                assert_eq!(conclude.experiment_id, "exp-1");
                assert_eq!(conclude.winner, "control");
            }
            _ => panic!("expected conclude"),
        }
    }

    #[test]
    fn parse_experiment_load_defaults() {
        let parsed = TestCli::parse_from(["test", "experiment", "load", "exp-1"]);
        let TestCommand::Experiment(args) = parsed.cmd;
        match args.command {
            ExperimentCommand::Load(load) => {
                assert_eq!(load.experiment_id, "exp-1");
                assert!(load.metric.is_none());
                assert!(load.context.is_none());
                assert!(load.pack.is_none());
                assert!(matches!(load.mode, CliPackMode::Balanced));
                assert!(load.contract.is_none());
                assert!(load.contract_id.is_none());
                assert_eq!(load.max_per_group, 2);
                assert!(!load.full);
                assert!(!load.complete);
                assert!(matches!(load.deps, DepsMode::Auto));
            }
            _ => panic!("expected load"),
        }
    }

    #[test]
    fn build_variants_payload_validation() {
        let empty: Vec<String> = Vec::new();
        assert!(build_variants_payload(&empty, "uniform", &[]).is_err());

        let variants = vec!["a".to_string(), "b:Beta".to_string()];
        let (variants_json, allocation_json) =
            build_variants_payload(&variants, "uniform", &[]).unwrap();
        assert!(variants_json.contains("\"id\":\"a\""));
        assert!(variants_json.contains("\"name\":\"Beta\""));
        assert!(allocation_json.contains("\"a\""));

        let weights = vec!["a=0.7".to_string(), "b=0.3".to_string()];
        let (_, allocation_json) =
            build_variants_payload(&variants, "weighted", &weights).unwrap();
        assert!(allocation_json.contains("\"strategy\":\"weighted\""));
    }
}
