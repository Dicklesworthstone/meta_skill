//! ms browse - Interactive skill browser TUI
//!
//! Provides a full-featured terminal interface for browsing, searching,
//! and previewing skills interactively.

use clap::Args;

use crate::app::AppContext;
use crate::cli::output::OutputFormat;
use crate::error::{MsError, Result};
use crate::tui::run_browse_tui;

#[derive(Args, Debug)]
pub struct BrowseArgs {
    /// Initial search query
    #[arg(long)]
    pub query: Option<String>,

    /// Filter by layer (base, org, project, user)
    #[arg(long)]
    pub layer: Option<String>,

    /// Filter by tags (comma-separated)
    #[arg(long, short = 't')]
    pub tags: Option<String>,

    /// Minimum quality score (0.0-1.0)
    #[arg(long)]
    pub min_quality: Option<f64>,
}

pub fn run(ctx: &AppContext, args: &BrowseArgs) -> Result<()> {
    // Cannot run TUI in robot mode
    if ctx.output_format != OutputFormat::Human {
        return Err(MsError::ValidationFailed(
            "browse command requires interactive terminal (cannot use --robot or --output-format)"
                .to_string(),
        ));
    }

    // Build initial query from args
    let mut initial_query_parts: Vec<String> = Vec::new();

    if let Some(ref query) = args.query {
        initial_query_parts.push(query.clone());
    }

    if let Some(ref layer) = args.layer {
        initial_query_parts.push(format!("layer:{}", layer));
    }

    if let Some(ref tags) = args.tags {
        for tag in tags.split(',') {
            let tag = tag.trim();
            if !tag.is_empty() {
                initial_query_parts.push(format!("tag:{}", tag));
            }
        }
    }

    if let Some(min_quality) = args.min_quality {
        initial_query_parts.push(format!("quality:>{}", min_quality));
    }

    // Run the TUI and capture any loaded skill
    let result = run_browse_tui(&ctx.db)?;

    // If a skill was selected, output its content
    if let Some(skill_id) = result {
        let skill = ctx
            .db
            .get_skill(&skill_id)?
            .ok_or_else(|| MsError::SkillNotFound(skill_id.clone()))?;

        // Output the skill body
        println!("{}", skill.body);
    }

    Ok(())
}
