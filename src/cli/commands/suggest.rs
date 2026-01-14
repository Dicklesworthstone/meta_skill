use clap::Args;

use crate::app::AppContext;
use crate::error::Result;

#[derive(Args, Debug)]
pub struct SuggestArgs {
    /// Working directory context
    #[arg(long)]
    pub cwd: Option<String>,

    /// Budget for packed output
    #[arg(long)]
    pub budget: Option<usize>,
}

pub fn run(_ctx: &AppContext, _args: &SuggestArgs) -> Result<()> {
    Ok(())
}
