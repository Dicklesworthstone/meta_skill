//! ms test - Run skill tests

use clap::Args;

use crate::app::AppContext;
use crate::error::Result;

#[derive(Args, Debug)]
pub struct TestArgs {
    /// Skill to test (or all if not specified)
    pub skill: Option<String>,

    /// Run only quick tests
    #[arg(long)]
    pub quick: bool,

    /// Show verbose test output
    #[arg(long)]
    pub verbose: bool,
}

pub fn run(_ctx: &AppContext, _args: &TestArgs) -> Result<()> {
    // TODO: Implement test command
    Ok(())
}
