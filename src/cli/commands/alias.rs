//! ms alias - Manage skill aliases
//!
//! Aliases allow skills to be referenced by alternate names (legacy IDs,
//! short names, abbreviations). Useful for backward compatibility when
//! renaming or deprecating skills.

use clap::{Args, Subcommand};
use colored::Colorize;

use crate::app::AppContext;
use crate::error::{MsError, Result};

#[derive(Args, Debug)]
pub struct AliasArgs {
    #[command(subcommand)]
    pub command: Option<AliasCommand>,

    /// List all aliases (shortcut for `ms alias list`)
    #[arg(long, short)]
    pub list: bool,
}

#[derive(Subcommand, Debug)]
pub enum AliasCommand {
    /// Add an alias for a skill
    Add {
        /// Alias name (the alternate identifier)
        alias: String,

        /// Target skill ID (the canonical identifier)
        #[arg(long, short)]
        target: String,

        /// Alias type: "short", "legacy", "deprecated", "alternate"
        #[arg(long, short = 'k', default_value = "alternate")]
        kind: String,
    },

    /// Remove an alias
    Remove {
        /// Alias to remove
        alias: String,
    },

    /// Resolve an alias to its canonical skill ID
    Resolve {
        /// Alias to resolve
        alias: String,
    },

    /// List aliases (all or for a specific skill)
    List {
        /// Filter by skill ID
        #[arg(long, short)]
        skill: Option<String>,
    },
}

pub fn run(ctx: &AppContext, args: &AliasArgs) -> Result<()> {
    // Handle --list shortcut
    if args.list {
        return list_aliases(ctx, None);
    }

    match &args.command {
        Some(AliasCommand::Add {
            alias,
            target,
            kind,
        }) => add_alias(ctx, alias, target, kind),
        Some(AliasCommand::Remove { alias }) => remove_alias(ctx, alias),
        Some(AliasCommand::Resolve { alias }) => resolve_alias(ctx, alias),
        Some(AliasCommand::List { skill }) => list_aliases(ctx, skill.as_deref()),
        None => {
            // No subcommand - show help
            if ctx.robot_mode {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "error",
                        "message": "No subcommand provided. Use: add, remove, resolve, list"
                    })
                );
            } else {
                println!("{}", "Usage: ms alias <COMMAND>".bold());
                println!();
                println!("Commands:");
                println!("  add       Add an alias for a skill");
                println!("  remove    Remove an alias");
                println!("  resolve   Resolve an alias to its canonical skill ID");
                println!("  list      List aliases (all or for a specific skill)");
                println!();
                println!("Options:");
                println!("  -l, --list   List all aliases (shortcut for `ms alias list`)");
            }
            Ok(())
        }
    }
}

fn add_alias(ctx: &AppContext, alias: &str, target: &str, kind: &str) -> Result<()> {
    // Validate alias type
    let valid_types = ["short", "legacy", "deprecated", "alternate"];
    if !valid_types.contains(&kind) {
        return Err(MsError::ValidationFailed(format!(
            "Invalid alias type '{}'. Valid types: {}",
            kind,
            valid_types.join(", ")
        )));
    }

    // Check target skill exists
    if ctx.db.get_skill(target)?.is_none() {
        return Err(MsError::SkillNotFound(format!(
            "Target skill '{}' not found",
            target
        )));
    }

    // Check if alias already exists
    if let Some(existing) = ctx.db.resolve_alias(alias)? {
        if existing.canonical_id != target {
            return Err(MsError::ValidationFailed(format!(
                "Alias '{}' already exists pointing to '{}'",
                alias, existing.canonical_id
            )));
        }
    }

    // Add the alias
    let created_at = chrono::Utc::now().to_rfc3339();
    ctx.db.upsert_alias(alias, target, kind, &created_at)?;

    if ctx.robot_mode {
        println!(
            "{}",
            serde_json::json!({
                "status": "ok",
                "alias": alias,
                "target": target,
                "type": kind
            })
        );
    } else {
        println!(
            "{} Added alias '{}' → '{}' (type: {})",
            "✓".green().bold(),
            alias.cyan(),
            target.cyan(),
            kind
        );
    }

    Ok(())
}

fn remove_alias(ctx: &AppContext, alias: &str) -> Result<()> {
    let removed = ctx.db.delete_alias(alias)?;

    if ctx.robot_mode {
        println!(
            "{}",
            serde_json::json!({
                "status": if removed { "ok" } else { "not_found" },
                "alias": alias,
                "removed": removed
            })
        );
    } else if removed {
        println!("{} Removed alias '{}'", "✓".green().bold(), alias.cyan());
    } else {
        println!("{} Alias '{}' not found", "!".yellow(), alias);
    }

    Ok(())
}

fn resolve_alias(ctx: &AppContext, alias: &str) -> Result<()> {
    let resolution = ctx.db.resolve_alias(alias)?;

    if ctx.robot_mode {
        if let Some(ref res) = resolution {
            println!(
                "{}",
                serde_json::json!({
                    "status": "ok",
                    "alias": alias,
                    "canonical_id": res.canonical_id,
                    "type": res.alias_type
                })
            );
        } else {
            println!(
                "{}",
                serde_json::json!({
                    "status": "not_found",
                    "alias": alias
                })
            );
        }
    } else if let Some(res) = resolution {
        println!(
            "{} → {} ({})",
            alias.cyan(),
            res.canonical_id.green(),
            res.alias_type.dimmed()
        );
    } else {
        // Try as skill ID directly
        if ctx.db.get_skill(alias)?.is_some() {
            println!("{} is a canonical skill ID (not an alias)", alias.cyan());
        } else {
            println!("{} No skill or alias found for '{}'", "!".yellow(), alias);
        }
    }

    Ok(())
}

fn list_aliases(ctx: &AppContext, skill_id: Option<&str>) -> Result<()> {
    let aliases = ctx.db.list_aliases(skill_id)?;

    if ctx.robot_mode {
        let output: Vec<serde_json::Value> = aliases
            .iter()
            .map(|a| {
                serde_json::json!({
                    "alias": a.alias,
                    "skill_id": a.skill_id,
                    "type": a.alias_type,
                    "created_at": a.created_at
                })
            })
            .collect();

        println!(
            "{}",
            serde_json::json!({
                "status": "ok",
                "count": aliases.len(),
                "aliases": output
            })
        );
    } else if aliases.is_empty() {
        if let Some(sid) = skill_id {
            println!("{} No aliases for skill '{}'", "!".yellow(), sid);
        } else {
            println!("{}", "No aliases defined".dimmed());
            println!();
            println!("Add an alias with:");
            println!("  ms alias add <alias> --target <skill-id>");
        }
    } else {
        if let Some(sid) = skill_id {
            println!(
                "{} aliases for '{}':",
                aliases.len().to_string().bold(),
                sid.cyan()
            );
        } else {
            println!("{} aliases:", aliases.len().to_string().bold());
        }
        println!();
        println!(
            "{:30} {:30} {:12} {}",
            "ALIAS".bold(),
            "SKILL".bold(),
            "TYPE".bold(),
            "CREATED".bold()
        );
        println!("{}", "─".repeat(85).dimmed());

        for alias in &aliases {
            let type_colored = match alias.alias_type.as_str() {
                "deprecated" => alias.alias_type.red(),
                "legacy" => alias.alias_type.yellow(),
                "short" => alias.alias_type.green(),
                _ => alias.alias_type.normal(),
            };

            let created = alias
                .created_at
                .split('T')
                .next()
                .unwrap_or(&alias.created_at);

            println!(
                "{:30} {:30} {:12} {}",
                alias.alias,
                alias.skill_id,
                type_colored,
                created.dimmed()
            );
        }
    }

    Ok(())
}
