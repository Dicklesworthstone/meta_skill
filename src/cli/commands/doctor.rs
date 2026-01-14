//! ms doctor - Health checks and repairs

use clap::Args;

use crate::app::AppContext;
use crate::error::Result;

#[derive(Args, Debug)]
pub struct DoctorArgs {
    /// Attempt to fix issues automatically
    #[arg(long)]
    pub fix: bool,

    /// Show verbose output
    #[arg(long)]
    pub verbose: bool,
}

pub fn run(_ctx: &AppContext, _args: &DoctorArgs) -> Result<()> {
    // TODO: Implement doctor command
    Ok(())
}
