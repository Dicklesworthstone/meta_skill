//! ms hide - Manage hidden skills
//!
//! Hidden skills are excluded from suggestions. Use this to suppress
//! skills that are outdated, irrelevant, or not useful for your workflow.

use clap::{Args, Subcommand};
use colored::Colorize;

use crate::app::AppContext;
use crate::cli::output::OutputFormat;
use crate::error::{MsError, Result};

#[derive(Args, Debug)]
pub struct HideArgs {
    #[command(subcommand)]
    pub command: Option<HideCommand>,

    /// Skill to hide (shortcut for `ms hide add <skill>`)
    pub skill: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum HideCommand {
    /// Hide a skill from suggestions
    Add {
        /// Skill ID or name
        skill: String,
    },

    /// List all hidden skills
    List {
        /// Maximum number of hidden skills to show
        #[arg(long, short = 'n', default_value = "50")]
        limit: usize,

        /// Offset for pagination
        #[arg(long, default_value = "0")]
        offset: usize,
    },
}

pub fn run(ctx: &AppContext, args: &HideArgs) -> Result<()> {
    // Handle positional skill argument as shortcut for add
    if let Some(ref skill) = args.skill {
        if args.command.is_none() {
            return add_hidden(ctx, skill);
        }
    }

    match &args.command {
        Some(HideCommand::Add { skill }) => add_hidden(ctx, skill),
        Some(HideCommand::List { limit, offset }) => list_hidden(ctx, *limit, *offset),
        None => {
            // No subcommand and no positional - show help
            if ctx.output_format != OutputFormat::Human {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "error",
                        "message": "No skill provided. Usage: ms hide <skill> or ms hide <command>"
                    })
                );
            } else {
                println!("{}", "Usage: ms hide <skill>".bold());
                println!();
                println!("Commands:");
                println!("  add <skill>   Hide a skill from suggestions");
                println!("  list          List all hidden skills");
                println!();
                println!("To unhide a skill, use:");
                println!("  ms unhide <skill>");
                println!();
                println!("Examples:");
                println!("  ms hide outdated-skill");
                println!("  ms hide list");
                println!("  ms unhide outdated-skill");
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

fn add_hidden(ctx: &AppContext, skill: &str) -> Result<()> {
    let skill_id = resolve_skill_id(ctx, skill)?;

    // Check if already hidden
    if ctx.db.has_user_preference(&skill_id, "hidden")? {
        if ctx.output_format != OutputFormat::Human {
            println!(
                "{}",
                serde_json::json!({
                    "status": "ok",
                    "skill_id": skill_id,
                    "message": "already hidden"
                })
            );
        } else {
            println!("{} '{}' is already hidden", "!".yellow(), skill_id.cyan());
        }
        return Ok(());
    }

    let record = ctx.db.set_user_preference(&skill_id, "hidden")?;

    if ctx.output_format != OutputFormat::Human {
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
            "{} Hidden '{}' from suggestions",
            "✓".green().bold(),
            skill_id.cyan()
        );
        println!();
        println!(
            "{}",
            "This skill will no longer appear in suggestions.".dimmed()
        );
        println!(
            "{}",
            format!("To unhide, run: ms unhide {}", skill_id).dimmed()
        );
    }

    Ok(())
}

pub fn list_hidden(ctx: &AppContext, limit: usize, offset: usize) -> Result<()> {
    let prefs = ctx.db.list_user_preferences("hidden", limit, offset)?;

    if ctx.output_format != OutputFormat::Human {
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
                "hidden": output
            })
        );
    } else if prefs.is_empty() {
        println!("{}", "No hidden skills".dimmed());
        println!();
        println!("Hide a skill with:");
        println!("  ms hide <skill>");
    } else {
        println!("{} hidden skills:", prefs.len().to_string().bold());
        println!();
        println!("{:40} {}", "SKILL".bold(), "HIDDEN".bold());
        println!("{}", "─".repeat(55).dimmed());

        for pref in &prefs {
            let created = pref
                .created_at
                .split('T')
                .next()
                .unwrap_or(&pref.created_at);

            println!("{:40} {}", pref.skill_id.red(), created.dimmed());
        }

        println!();
        println!("{}", "To unhide a skill: ms unhide <skill>".dimmed());
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
        Hide(HideArgs),
    }

    #[test]
    fn parse_hide_positional() {
        let parsed = TestCli::parse_from(["test", "hide", "outdated-skill"]);
        let TestCommand::Hide(args) = parsed.cmd;
        assert_eq!(args.skill.as_deref(), Some("outdated-skill"));
        assert!(args.command.is_none());
    }

    #[test]
    fn parse_hide_add() {
        let parsed = TestCli::parse_from(["test", "hide", "add", "outdated-skill"]);
        let TestCommand::Hide(args) = parsed.cmd;
        match args.command {
            Some(HideCommand::Add { skill }) => {
                assert_eq!(skill, "outdated-skill");
            }
            _ => panic!("expected add command"),
        }
    }

    #[test]
    fn parse_hide_list() {
        let parsed = TestCli::parse_from(["test", "hide", "list"]);
        let TestCommand::Hide(args) = parsed.cmd;
        match args.command {
            Some(HideCommand::List { limit, offset }) => {
                assert_eq!(limit, 50);
                assert_eq!(offset, 0);
            }
            _ => panic!("expected list command"),
        }
    }

    #[test]
    fn parse_hide_list_with_limit() {
        let parsed = TestCli::parse_from(["test", "hide", "list", "-n", "10", "--offset", "5"]);
        let TestCommand::Hide(args) = parsed.cmd;
        match args.command {
            Some(HideCommand::List { limit, offset }) => {
                assert_eq!(limit, 10);
                assert_eq!(offset, 5);
            }
            _ => panic!("expected list command"),
        }
    }
}
