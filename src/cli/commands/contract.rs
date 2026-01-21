//! ms contract - Manage pack contracts

use std::collections::HashMap;

use clap::{Args, Subcommand};

use crate::app::AppContext;
use crate::cli::output::OutputFormat;
use crate::cli::output::{HumanLayout, emit_human, emit_robot, robot_ok};
use crate::core::pack_contracts::{
    add_custom_contract, builtin_contracts, custom_contracts_path, load_custom_contracts,
};
use crate::core::skill::PackContract;
use crate::error::{MsError, Result};

#[derive(Args, Debug)]
pub struct ContractArgs {
    #[command(subcommand)]
    pub command: ContractCommand,
}

#[derive(Subcommand, Debug)]
pub enum ContractCommand {
    /// Create a custom pack contract
    Create(CreateArgs),
    /// List built-in and custom contracts
    List(ListArgs),
}

#[derive(Args, Debug)]
pub struct CreateArgs {
    /// Contract id (unique)
    pub id: String,

    /// Description of the contract
    #[arg(long)]
    pub description: Option<String>,

    /// Required coverage group (repeatable or comma-separated)
    #[arg(long = "required")]
    pub required_groups: Vec<String>,

    /// Mandatory slice id (repeatable or comma-separated)
    #[arg(long = "mandatory")]
    pub mandatory_slices: Vec<String>,

    /// Max slices per coverage group
    #[arg(long)]
    pub max_per_group: Option<usize>,

    /// Coverage group weights (format: group:weight)
    #[arg(long = "group-weight")]
    pub group_weights: Vec<String>,

    /// Tag weights (format: tag:weight)
    #[arg(long = "tag-weight")]
    pub tag_weights: Vec<String>,
}

#[derive(Args, Debug)]
pub struct ListArgs {
    /// Include built-in presets
    #[arg(long, default_value_t = true)]
    pub include_builtin: bool,

    /// Include custom contracts
    #[arg(long, default_value_t = true)]
    pub include_custom: bool,
}

pub fn run(ctx: &AppContext, args: &ContractArgs) -> Result<()> {
    match &args.command {
        ContractCommand::Create(cmd) => create_contract(ctx, cmd),
        ContractCommand::List(cmd) => list_contracts(ctx, cmd),
    }
}

fn create_contract(ctx: &AppContext, args: &CreateArgs) -> Result<()> {
    let required_groups = split_values(&args.required_groups);
    let mandatory_slices = split_values(&args.mandatory_slices);
    let group_weights = parse_weight_entries(&args.group_weights)?;
    let tag_weights = parse_weight_entries(&args.tag_weights)?;

    let contract = PackContract {
        id: args.id.trim().to_string(),
        description: args
            .description
            .clone()
            .unwrap_or_else(|| "Custom pack contract".to_string()),
        required_groups,
        mandatory_slices,
        max_per_group: args.max_per_group,
        group_weights: if group_weights.is_empty() {
            None
        } else {
            Some(group_weights)
        },
        tag_weights: if tag_weights.is_empty() {
            None
        } else {
            Some(tag_weights)
        },
    };

    let path = custom_contracts_path(&ctx.ms_root);
    add_custom_contract(&path, contract.clone())?;

    if ctx.output_format != OutputFormat::Human {
        emit_robot(&robot_ok(serde_json::json!({
            "created": contract,
            "path": path.display().to_string(),
        })))
    } else {
        let mut layout = HumanLayout::new();
        layout
            .title("Contract Created")
            .kv("Id", &contract.id)
            .kv("Path", &path.display().to_string())
            .kv("Required Groups", &join_or_none(&contract.required_groups))
            .kv(
                "Mandatory Slices",
                &join_or_none(&contract.mandatory_slices),
            );
        if let Some(max) = contract.max_per_group {
            layout.kv("Max Per Group", &max.to_string());
        }
        emit_human(layout);
        Ok(())
    }
}

fn list_contracts(ctx: &AppContext, args: &ListArgs) -> Result<()> {
    let path = custom_contracts_path(&ctx.ms_root);
    let mut builtins = if args.include_builtin {
        builtin_contracts()
    } else {
        Vec::new()
    };
    let mut customs = if args.include_custom {
        load_custom_contracts(&path)?
    } else {
        Vec::new()
    };

    builtins.sort_by(|a, b| a.id.cmp(&b.id));
    customs.sort_by(|a, b| a.id.cmp(&b.id));

    if ctx.output_format != OutputFormat::Human {
        emit_robot(&robot_ok(serde_json::json!({
            "builtins": builtins,
            "custom": customs,
            "path": path.display().to_string(),
        })))
    } else {
        let mut layout = HumanLayout::new();
        layout.title("Pack Contracts");
        if args.include_builtin {
            layout.section("Built-in");
            if builtins.is_empty() {
                layout.push_line("(none)");
            }
            for contract in &builtins {
                render_contract(&mut layout, contract, "builtin");
            }
        }
        if args.include_custom {
            layout.section("Custom");
            if customs.is_empty() {
                layout.push_line("(none)");
            }
            for contract in &customs {
                render_contract(&mut layout, contract, "custom");
            }
            if args.include_custom {
                layout
                    .blank()
                    .kv("Custom Path", &path.display().to_string());
            }
        }
        emit_human(layout);
        Ok(())
    }
}

fn render_contract(layout: &mut HumanLayout, contract: &PackContract, source: &str) {
    layout
        .blank()
        .kv("Id", &contract.id)
        .kv("Source", source)
        .kv("Description", &contract.description)
        .kv("Required", &join_or_none(&contract.required_groups))
        .kv("Mandatory", &join_or_none(&contract.mandatory_slices));
    if let Some(max) = contract.max_per_group {
        layout.kv("Max Per Group", &max.to_string());
    }
}

fn split_values(values: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        for item in value.split(',') {
            let trimmed = item.trim();
            if !trimmed.is_empty() {
                out.push(trimmed.to_string());
            }
        }
    }
    out
}

fn parse_weight_entries(entries: &[String]) -> Result<HashMap<String, f32>> {
    let mut out = HashMap::new();
    for entry in entries {
        let (key, value) = entry.split_once(':').ok_or_else(|| {
            MsError::ValidationFailed(format!("weight must be key:value ({entry})",))
        })?;
        let key = key.trim();
        if key.is_empty() {
            return Err(MsError::ValidationFailed(
                "weight key cannot be empty".to_string(),
            ));
        }
        let weight: f32 = value
            .trim()
            .parse()
            .map_err(|_| MsError::ValidationFailed(format!("invalid weight for {key}: {value}")))?;
        if weight < 0.0 {
            return Err(MsError::ValidationFailed(format!(
                "weight must be >= 0 for {key}"
            )));
        }
        let key = key.to_lowercase();
        if out.insert(key.clone(), weight).is_some() {
            return Err(MsError::ValidationFailed(format!(
                "duplicate weight entry for {key}"
            )));
        }
    }
    Ok(out)
}

fn join_or_none(items: &[String]) -> String {
    if items.is_empty() {
        "-".to_string()
    } else {
        items.join(", ")
    }
}
