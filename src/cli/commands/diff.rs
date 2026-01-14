//! ms diff - Semantic diff between skills

use clap::Args;

use crate::app::AppContext;
use crate::error::Result;

#[derive(Args, Debug)]
pub struct DiffArgs {
    /// First skill
    pub skill_a: String,

    /// Second skill
    pub skill_b: String,

    /// Show only structural differences
    #[arg(long)]
    pub structure_only: bool,

    /// Output format: text, json
    #[arg(long, default_value = "text")]
    pub format: String,
}

pub fn run(_ctx: &AppContext, _args: &DiffArgs) -> Result<()> {
    // TODO: Implement diff command
    Ok(())
}
