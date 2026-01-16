//! ms favorite - Manage favorite skills
//!
//! Favorites are skills that should always be boosted in suggestions.
//! Use this to prioritize skills you frequently use.

use clap::{Args, Subcommand};
use colored::Colorize;

use crate::app::AppContext;
use crate::error::{MsError, Result};

#[derive(Args, Debug)]
pub struct FavoriteArgs {
    #[command(subcommand)]
    pub command: Option<FavoriteCommand>,

    /// Skill to add to favorites (shortcut for `ms favorite add <skill>`)
    pub skill: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum FavoriteCommand {
    /// Add a skill to favorites
    Add {
        /// Skill ID or name
        skill: String,
    },

    /// Remove a skill from favorites
    Remove {
        /// Skill ID or name
        skill: String,
    },

    /// List all favorite skills
    List {
        /// Maximum number of favorites to show
        #[arg(long, short = 'n', default_value = "50")]
        limit: usize,

        /// Offset for pagination
        #[arg(long, default_value = "0")]
        offset: usize,
    },
}

pub fn run(ctx: &AppContext, args: &FavoriteArgs) -> Result<()> {
    // Handle positional skill argument as shortcut for add
    if let Some(ref skill) = args.skill {
        if args.command.is_none() {
            return add_favorite(ctx, skill);
        }
    }

    match &args.command {
        Some(FavoriteCommand::Add { skill }) => add_favorite(ctx, skill),
        Some(FavoriteCommand::Remove { skill }) => remove_favorite(ctx, skill),
        Some(FavoriteCommand::List { limit, offset }) => list_favorites(ctx, *limit, *offset),
        None => {
            // No subcommand and no positional - show help
            if ctx.robot_mode {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "error",
                        "message": "No skill provided. Usage: ms favorite <skill> or ms favorite <command>"
                    })
                );
            } else {
                println!("{}", "Usage: ms favorite <skill>".bold());
                println!();
                println!("Commands:");
                println!("  add <skill>      Add a skill to favorites");
                println!("  remove <skill>   Remove a skill from favorites");
                println!("  list             List all favorite skills");
                println!();
                println!("Examples:");
                println!("  ms favorite rust-error-handling");
                println!("  ms favorite list");
                println!("  ms favorite remove rust-error-handling");
            }
            Ok(())
        }
    }
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

fn add_favorite(ctx: &AppContext, skill: &str) -> Result<()> {
    let skill_id = resolve_skill_id(ctx, skill)?;

    // Check if already a favorite
    if ctx.db.has_user_preference(&skill_id, "favorite")? {
        if ctx.robot_mode {
            println!(
                "{}",
                serde_json::json!({
                    "status": "ok",
                    "skill_id": skill_id,
                    "message": "already a favorite"
                })
            );
        } else {
            println!(
                "{} '{}' is already a favorite",
                "!".yellow(),
                skill_id.cyan()
            );
        }
        return Ok(());
    }

    let record = ctx.db.set_user_preference(&skill_id, "favorite")?;

    if ctx.robot_mode {
        println!(
            "{}",
            serde_json::json!({
                "status": "ok",
                "skill_id": record.skill_id,
                "preference_type": record.preference_type,
                "created_at": record.created_at
            })
        );
    } else {
        println!(
            "{} Added '{}' to favorites",
            "✓".green().bold(),
            skill_id.cyan()
        );
        println!();
        println!(
            "{}",
            "This skill will be boosted in suggestions.".dimmed()
        );
    }

    Ok(())
}

fn remove_favorite(ctx: &AppContext, skill: &str) -> Result<()> {
    let skill_id = resolve_skill_id(ctx, skill)?;

    let removed = ctx.db.remove_user_preference(&skill_id, "favorite")?;

    if ctx.robot_mode {
        println!(
            "{}",
            serde_json::json!({
                "status": if removed { "ok" } else { "not_found" },
                "skill_id": skill_id,
                "removed": removed
            })
        );
    } else if removed {
        println!(
            "{} Removed '{}' from favorites",
            "✓".green().bold(),
            skill_id.cyan()
        );
    } else {
        println!(
            "{} '{}' was not in favorites",
            "!".yellow(),
            skill_id.cyan()
        );
    }

    Ok(())
}

fn list_favorites(ctx: &AppContext, limit: usize, offset: usize) -> Result<()> {
    let prefs = ctx.db.list_user_preferences("favorite", limit, offset)?;

    if ctx.robot_mode {
        let output: Vec<serde_json::Value> = prefs
            .iter()
            .map(|p| {
                serde_json::json!({
                    "skill_id": p.skill_id,
                    "created_at": p.created_at
                })
            })
            .collect();

        println!(
            "{}",
            serde_json::json!({
                "status": "ok",
                "count": prefs.len(),
                "favorites": output
            })
        );
    } else if prefs.is_empty() {
        println!("{}", "No favorite skills".dimmed());
        println!();
        println!("Add a favorite with:");
        println!("  ms favorite <skill>");
    } else {
        println!("{} favorite skills:", prefs.len().to_string().bold());
        println!();
        println!("{:40} {}", "SKILL".bold(), "ADDED".bold());
        println!("{}", "─".repeat(55).dimmed());

        for pref in &prefs {
            let created = pref
                .created_at
                .split('T')
                .next()
                .unwrap_or(&pref.created_at);

            println!("{:40} {}", pref.skill_id.cyan(), created.dimmed());
        }
    }

    Ok(())
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
        Favorite(FavoriteArgs),
    }

    #[test]
    fn parse_favorite_positional() {
        let parsed = TestCli::parse_from(["test", "favorite", "rust-error-handling"]);
        let TestCommand::Favorite(args) = parsed.cmd;
        assert_eq!(args.skill.as_deref(), Some("rust-error-handling"));
        assert!(args.command.is_none());
    }

    #[test]
    fn parse_favorite_add() {
        let parsed = TestCli::parse_from(["test", "favorite", "add", "rust-error-handling"]);
        let TestCommand::Favorite(args) = parsed.cmd;
        match args.command {
            Some(FavoriteCommand::Add { skill }) => {
                assert_eq!(skill, "rust-error-handling");
            }
            _ => panic!("expected add command"),
        }
    }

    #[test]
    fn parse_favorite_remove() {
        let parsed = TestCli::parse_from(["test", "favorite", "remove", "rust-error-handling"]);
        let TestCommand::Favorite(args) = parsed.cmd;
        match args.command {
            Some(FavoriteCommand::Remove { skill }) => {
                assert_eq!(skill, "rust-error-handling");
            }
            _ => panic!("expected remove command"),
        }
    }

    #[test]
    fn parse_favorite_list() {
        let parsed = TestCli::parse_from(["test", "favorite", "list"]);
        let TestCommand::Favorite(args) = parsed.cmd;
        match args.command {
            Some(FavoriteCommand::List { limit, offset }) => {
                assert_eq!(limit, 50);
                assert_eq!(offset, 0);
            }
            _ => panic!("expected list command"),
        }
    }

    #[test]
    fn parse_favorite_list_with_limit() {
        let parsed = TestCli::parse_from(["test", "favorite", "list", "-n", "10", "--offset", "5"]);
        let TestCommand::Favorite(args) = parsed.cmd;
        match args.command {
            Some(FavoriteCommand::List { limit, offset }) => {
                assert_eq!(limit, 10);
                assert_eq!(offset, 5);
            }
            _ => panic!("expected list command"),
        }
    }
}
