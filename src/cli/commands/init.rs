//! ms init - Initialize ms in current directory or globally

use clap::Args;

use crate::app::AppContext;
use crate::error::Result;

#[derive(Args, Debug)]
pub struct InitArgs {
    /// Initialize globally (~/.local/share/ms) instead of locally (.ms/)
    #[arg(long)]
    pub global: bool,

    /// Force initialization even if already initialized
    #[arg(long, short)]
    pub force: bool,
}

pub fn run(_ctx: &AppContext, _args: &InitArgs) -> Result<()> {
    // TODO: Implement init command
    Ok(())
}
