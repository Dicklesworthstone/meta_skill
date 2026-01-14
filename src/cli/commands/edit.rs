//! ms edit - Edit a skill (structured round-trip)

use clap::Args;

use crate::app::AppContext;
use crate::error::Result;

#[derive(Args, Debug)]
pub struct EditArgs {
    /// Skill ID or name to edit
    pub skill: String,

    /// Editor to use (default: $EDITOR)
    #[arg(long)]
    pub editor: Option<String>,

    /// Edit metadata only
    #[arg(long)]
    pub meta: bool,
}

pub fn run(_ctx: &AppContext, _args: &EditArgs) -> Result<()> {
    // TODO: Implement edit command
    Ok(())
}
