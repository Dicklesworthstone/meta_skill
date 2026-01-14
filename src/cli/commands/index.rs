//! ms index - Index skills from configured paths

use clap::Args;

use crate::app::AppContext;
use crate::error::Result;

#[derive(Args, Debug)]
pub struct IndexArgs {
    /// Paths to index (overrides config)
    #[arg(value_name = "PATH")]
    pub paths: Vec<String>,

    /// Watch for changes and re-index automatically
    #[arg(long)]
    pub watch: bool,

    /// Force full re-index
    #[arg(long, short)]
    pub force: bool,
}

pub fn run(_ctx: &AppContext, _args: &IndexArgs) -> Result<()> {
    // TODO: Implement index command
    Ok(())
}
