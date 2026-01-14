//! ms load - Load a skill with progressive disclosure

use clap::Args;

use crate::app::AppContext;
use crate::error::{MsError, Result};

#[derive(Args, Debug)]
pub struct LoadArgs {
    /// Skill ID or name to load
    pub skill: String,

    /// Disclosure level: minimal, overview, standard, full, complete
    #[arg(long, default_value = "standard")]
    pub level: String,

    /// Token budget for packing
    #[arg(long)]
    pub pack: Option<usize>,

    /// Include dependencies
    #[arg(long, default_value = "true")]
    pub deps: bool,
}

pub fn run(ctx: &AppContext, args: &LoadArgs) -> Result<()> {
    // Look up the skill (basic implementation)
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

    // For now, just output the skill body (basic implementation)
    // TODO: Implement progressive disclosure levels and packing
    if ctx.robot_mode {
        let output = serde_json::json!({
            "status": "ok",
            "skill_id": skill.id,
            "name": skill.name,
            "body": skill.body,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", skill.body);
    }

    Ok(())
}
