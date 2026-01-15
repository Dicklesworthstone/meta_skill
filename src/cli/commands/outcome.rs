//! ms outcome - Record implicit success/failure outcomes.

use clap::Args;

use crate::app::AppContext;
use crate::cli::output::{HumanLayout, emit_json};
use crate::error::{MsError, Result};

#[derive(Args, Debug)]
pub struct OutcomeArgs {
    /// Skill ID or name
    pub skill: String,

    /// Mark latest usage as success
    #[arg(long)]
    pub success: bool,

    /// Mark latest usage as failure
    #[arg(long)]
    pub failure: bool,
}

pub fn run(ctx: &AppContext, args: &OutcomeArgs) -> Result<()> {
    if args.success == args.failure {
        return Err(MsError::ValidationFailed(
            "provide exactly one of --success or --failure".to_string(),
        ));
    }

    let skill_id = resolve_skill_id(ctx, &args.skill)?;
    let success = args.success;
    ctx.db.record_skill_outcome(&skill_id, success)?;

    if ctx.robot_mode {
        let payload = serde_json::json!({
            "status": "ok",
            "skill_id": skill_id,
            "success": success,
        });
        return emit_json(&payload);
    }

    let mut layout = HumanLayout::new();
    layout
        .title("Outcome Recorded")
        .kv("Skill", &skill_id)
        .kv("Outcome", if success { "success" } else { "failure" });
    crate::cli::output::emit_human(layout);
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
        Outcome(OutcomeArgs),
    }

    #[test]
    fn parse_outcome_success() {
        let parsed = TestCli::parse_from(["test", "outcome", "skill-1", "--success"]);
        let TestCommand::Outcome(args) = parsed.cmd;
        assert_eq!(args.skill, "skill-1");
        assert!(args.success);
        assert!(!args.failure);
    }

    #[test]
    fn parse_outcome_failure() {
        let parsed = TestCli::parse_from(["test", "outcome", "skill-1", "--failure"]);
        let TestCommand::Outcome(args) = parsed.cmd;
        assert_eq!(args.skill, "skill-1");
        assert!(!args.success);
        assert!(args.failure);
    }
}
