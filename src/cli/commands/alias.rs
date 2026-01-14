//! ms alias - Manage skill aliases

use clap::Args;

use crate::app::AppContext;
use crate::error::Result;

#[derive(Args, Debug)]
pub struct AliasArgs {
    /// Alias name
    pub name: Option<String>,

    /// Target skill ID
    #[arg(long)]
    pub target: Option<String>,

    /// Remove the alias
    #[arg(long)]
    pub remove: bool,

    /// List all aliases
    #[arg(long)]
    pub list: bool,
}

pub fn run(_ctx: &AppContext, _args: &AliasArgs) -> Result<()> {
    // TODO: Implement alias command
    Ok(())
}
