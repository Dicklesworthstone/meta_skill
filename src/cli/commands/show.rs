//! ms show - Show skill details

use clap::Args;
use colored::Colorize;

use crate::app::AppContext;
use crate::error::{MsError, Result};

#[derive(Args, Debug)]
pub struct ShowArgs {
    /// Skill ID or name to show
    pub skill: String,

    /// Show full spec (not just summary)
    #[arg(long)]
    pub full: bool,

    /// Show metadata only
    #[arg(long)]
    pub meta: bool,

    /// Show dependency graph
    #[arg(long)]
    pub deps: bool,
}

pub fn run(ctx: &AppContext, args: &ShowArgs) -> Result<()> {
    // Try to find skill by ID or name
    let skill = ctx
        .db
        .get_skill(&args.skill)?
        .or_else(|| {
            // Try alias resolution
            ctx.db
                .resolve_alias(&args.skill)
                .ok()
                .flatten()
                .and_then(|res| ctx.db.get_skill(&res.canonical_id).ok().flatten())
        })
        .ok_or_else(|| MsError::SkillNotFound(format!("skill not found: {}", args.skill)))?;

    if ctx.robot_mode {
        show_robot(ctx, &skill, args)
    } else {
        show_human(ctx, &skill, args)
    }
}

fn show_human(
    _ctx: &AppContext,
    skill: &crate::storage::sqlite::SkillRecord,
    args: &ShowArgs,
) -> Result<()> {
    // Header
    println!("{}", skill.name.bold());
    println!("{}", "═".repeat(skill.name.len()));
    println!();

    // ID and version
    println!("{}: {}", "ID".dimmed(), skill.id);
    println!("{}: {}", "Version".dimmed(), skill.version.as_deref().unwrap_or("-"));

    // Author
    if let Some(ref author) = skill.author {
        println!("{}: {}", "Author".dimmed(), author);
    }

    // Layer
    let layer = &skill.source_layer;
    let layer_colored = match layer.as_str() {
        "system" => layer.blue(),
        "global" => layer.green(),
        "project" => layer.yellow(),
        "local" => layer.magenta(),
        _ => layer.normal(),
    };
    println!("{}: {}", "Layer".dimmed(), layer_colored);

    // Source path
    println!("{}: {}", "Source".dimmed(), skill.source_path);

    // Description
    println!();
    if !skill.description.is_empty() {
        println!("{}", skill.description);
    }

    // Deprecation warning
    if skill.is_deprecated {
        println!();
        println!(
            "{} {}",
            "⚠ DEPRECATED:".red().bold(),
            skill.deprecation_reason.as_deref().unwrap_or("No reason provided")
        );
    }

    // Stats
    println!();
    println!("{}", "Stats".bold());
    println!("{}", "─".repeat(40).dimmed());
    println!("{}: {}", "Tokens".dimmed(), skill.token_count);
    println!("{}: {:.2}", "Quality".dimmed(), skill.quality_score);
    println!("{}: {}", "Indexed".dimmed(), format_date(&skill.indexed_at));
    println!("{}: {}", "Modified".dimmed(), format_date(&skill.modified_at));

    // Git info
    if skill.git_remote.is_some() || skill.git_commit.is_some() {
        println!();
        println!("{}", "Provenance".bold());
        println!("{}", "─".repeat(40).dimmed());
        if let Some(ref remote) = skill.git_remote {
            println!("{}: {}", "Remote".dimmed(), remote);
        }
        if let Some(ref commit) = skill.git_commit {
            println!("{}: {}", "Commit".dimmed(), &commit[..commit.len().min(8)]);
        }
        let hash = &skill.content_hash;
        if !hash.is_empty() {
            println!("{}: {}", "Hash".dimmed(), &hash[..hash.len().min(16)]);
        }
    }

    // Metadata JSON (if requested)
    if args.meta || args.full {
        println!();
        println!("{}", "Metadata".bold());
        println!("{}", "─".repeat(40).dimmed());
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&skill.metadata_json) {
            if let Ok(pretty) = serde_json::to_string_pretty(&meta) {
                println!("{}", pretty);
            }
        }
    }

    // Full body (if requested)
    if args.full {
        println!();
        println!("{}", "Body".bold());
        println!("{}", "─".repeat(40).dimmed());
        println!("{}", skill.body);
    }

    // Dependencies (if requested)
    if args.deps {
        println!();
        println!("{}", "Dependencies".bold());
        println!("{}", "─".repeat(40).dimmed());
        // Parse metadata for dependencies
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&skill.metadata_json) {
            if let Some(deps) = meta.get("dependencies").and_then(|d| d.as_array()) {
                if deps.is_empty() {
                    println!("{}", "No dependencies".dimmed());
                } else {
                    for dep in deps {
                        if let Some(dep_str) = dep.as_str() {
                            println!("  → {}", dep_str);
                        }
                    }
                }
            } else {
                println!("{}", "No dependencies".dimmed());
            }
        }
    }

    Ok(())
}

fn show_robot(
    _ctx: &AppContext,
    skill: &crate::storage::sqlite::SkillRecord,
    args: &ShowArgs,
) -> Result<()> {
    let mut output = serde_json::json!({
        "status": "ok",
        "skill": {
            "id": skill.id,
            "name": skill.name,
            "version": skill.version,
            "description": skill.description,
            "author": skill.author,
            "layer": skill.source_layer,
            "source_path": skill.source_path,
            "git_remote": skill.git_remote,
            "git_commit": skill.git_commit,
            "content_hash": skill.content_hash,
            "token_count": skill.token_count,
            "quality_score": skill.quality_score,
            "indexed_at": skill.indexed_at,
            "modified_at": skill.modified_at,
            "is_deprecated": skill.is_deprecated,
            "deprecation_reason": skill.deprecation_reason,
        }
    });

    if args.meta || args.full {
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&skill.metadata_json) {
            output["skill"]["metadata"] = meta;
        }
    }

    if args.full {
        output["skill"]["body"] = serde_json::Value::String(skill.body.clone());
    }

    if args.deps {
        // Parse dependencies from metadata
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&skill.metadata_json) {
            output["skill"]["dependencies"] = meta
                .get("dependencies")
                .cloned()
                .unwrap_or(serde_json::Value::Array(vec![]));
        }
    }

    println!("{}", serde_json::to_string_pretty(&output)?);

    Ok(())
}

fn format_date(datetime: &str) -> String {
    // Try to parse and format nicely
    datetime
        .split('T')
        .next()
        .unwrap_or(datetime)
        .to_string()
}
