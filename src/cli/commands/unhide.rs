//! ms unhide - Unhide a skill
//!
//! Removes a skill from the hidden list, allowing it to appear in suggestions again.

use clap::Args;
use colored::Colorize;

use crate::app::AppContext;
use crate::error::{MsError, Result};

#[derive(Args, Debug)]
pub struct UnhideArgs {
    /// Skill ID or name to unhide
    pub skill: String,
}

pub fn run(ctx: &AppContext, args: &UnhideArgs) -> Result<()> {
    let skill_id = resolve_skill_id(ctx, &args.skill)?;

    let removed = ctx.db.remove_user_preference(&skill_id, "hidden")?;

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
            "{} Unhidden '{}' - it will appear in suggestions again",
            "✓".green().bold(),
            skill_id.cyan()
        );
    } else {
        println!(
            "{} '{}' was not hidden",
            "!".yellow(),
            skill_id.cyan()
        );
    }

    Ok(())
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
        Unhide(UnhideArgs),
    }

    #[test]
    fn parse_unhide() {
        let parsed = TestCli::parse_from(["test", "unhide", "outdated-skill"]);
        let TestCommand::Unhide(args) = parsed.cmd;
        assert_eq!(args.skill, "outdated-skill");
    }
}
