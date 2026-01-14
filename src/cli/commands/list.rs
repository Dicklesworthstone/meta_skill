//! ms list - List all indexed skills

use clap::Args;

use crate::app::AppContext;
use crate::error::Result;

#[derive(Args, Debug)]
pub struct ListArgs {
    /// Filter by tags
    #[arg(long, short)]
    pub tags: Vec<String>,

    /// Filter by layer: system, global, project, session
    #[arg(long)]
    pub layer: Option<String>,

    /// Include deprecated skills
    #[arg(long)]
    pub include_deprecated: bool,

    /// Sort by: name, updated, relevance
    #[arg(long, default_value = "name")]
    pub sort: String,
}

pub fn run(_ctx: &AppContext, _args: &ListArgs) -> Result<()> {
    // TODO: Implement list command
    Ok(())
}
