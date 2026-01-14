//! ms load - Load a skill with progressive disclosure

use clap::Args;

use crate::app::AppContext;
use crate::error::Result;

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

pub fn run(_ctx: &AppContext, _args: &LoadArgs) -> Result<()> {
    // TODO: Implement load command
    Ok(())
}
