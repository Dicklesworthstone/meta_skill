//! ms experiment - Manage skill A/B experiments.

use clap::{Args, Subcommand};
use serde::Serialize;

use crate::app::AppContext;
use crate::cli::output::{HumanLayout, emit_json};
use crate::error::{MsError, Result};

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
    }
}

fn run_create(ctx: &AppContext, args: &ExperimentCreateArgs) -> Result<()> {
    if args.scope == "slice" && args.scope_id.is_none() {
        return Err(MsError::ValidationFailed(
            "--scope-id is required when scope is slice".to_string(),
        ));
    }

    let skill_id = resolve_skill_id(ctx, &args.skill)?;

    let (variants_json, allocation_json) = build_variants_payload(&args.variant)?;

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

fn build_variants_payload(variants: &[String]) -> Result<(String, String)> {
    if variants.is_empty() {
        return Err(MsError::ValidationFailed(
            "at least one --variant is required".to_string(),
        ));
    }

    let weight = 1.0 / (variants.len() as f64);
    let mut items = Vec::new();
    let mut weights = serde_json::Map::new();
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
        items.push(serde_json::json!({
            "id": id,
            "name": name,
            "weight": weight,
        }));
        weights.insert(id.to_string(), serde_json::json!(weight));
    }

    let variants_json = serde_json::to_string(&items)
        .map_err(|err| MsError::Serialization(format!("variants serialize: {err}")))?;
    let allocation_json = serde_json::json!({
        "strategy": "uniform",
        "weights": weights,
    })
    .to_string();

    Ok((variants_json, allocation_json))
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
    fn build_variants_payload_validation() {
        let empty: Vec<String> = Vec::new();
        assert!(build_variants_payload(&empty).is_err());

        let variants = vec!["a".to_string(), "b:Beta".to_string()];
        let (variants_json, allocation_json) = build_variants_payload(&variants).unwrap();
        assert!(variants_json.contains("\"id\":\"a\""));
        assert!(variants_json.contains("\"name\":\"Beta\""));
        assert!(allocation_json.contains("\"a\""));
    }
}
