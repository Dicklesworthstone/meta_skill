//! ms show - Show skill details

use clap::Args;

use crate::app::AppContext;
use crate::error::Result;

#[derive(Args, Debug)]
pub struct ShowArgs {
    /// Skill ID or name to show
    pub skill: String,

    /// Show full spec (not just summary)
    #[arg(long)]
    pub full: bool,

    /// Show metadata only
    #[arg(long)]
    pub meta: bool,
}

pub fn run(_ctx: &AppContext, _args: &ShowArgs) -> Result<()> {
    // TODO: Implement show command
    Ok(())
}
