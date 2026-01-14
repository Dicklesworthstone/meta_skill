//! ms fmt - Format skill files

use clap::Args;

use crate::app::AppContext;
use crate::error::Result;

#[derive(Args, Debug)]
pub struct FmtArgs {
    /// Skills to format (default: all)
    pub skills: Vec<String>,

    /// Check formatting without modifying
    #[arg(long)]
    pub check: bool,

    /// Show diff instead of modifying
    #[arg(long)]
    pub diff: bool,
}

pub fn run(_ctx: &AppContext, _args: &FmtArgs) -> Result<()> {
    // TODO: Implement fmt command
    Ok(())
}
