//! ms search - Search for skills

use clap::Args;

use crate::app::AppContext;
use crate::error::Result;

#[derive(Args, Debug)]
pub struct SearchArgs {
    /// Search query
    pub query: String,

    /// Maximum number of results
    #[arg(long, short, default_value = "10")]
    pub limit: usize,

    /// Filter by tags
    #[arg(long, short)]
    pub tags: Vec<String>,

    /// Search type: hybrid (default), bm25, semantic
    #[arg(long, default_value = "hybrid")]
    pub search_type: String,
}

pub fn run(_ctx: &AppContext, _args: &SearchArgs) -> Result<()> {
    // TODO: Implement search command
    Ok(())
}
