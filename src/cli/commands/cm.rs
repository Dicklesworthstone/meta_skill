//! CM (cass-memory) commands.

use clap::{Args, Subcommand};

use crate::app::AppContext;
use crate::cm::CmClient;
use crate::error::{MsError, Result};
use crate::security::SafetyGate;

#[derive(Args, Debug)]
pub struct CmArgs {
    #[command(subcommand)]
    pub command: CmCommand,
}

#[derive(Subcommand, Debug)]
pub enum CmCommand {
    /// Fetch CM context for a task query
    Context {
        /// Task or query string
        task: String,
    },
}

pub fn run(ctx: &AppContext, args: &CmArgs) -> Result<()> {
    if !ctx.config.cm.enabled {
        if ctx.robot_mode {
            println!(
                "{}",
                serde_json::json!({
                    "status": "disabled",
                    "message": "cm integration disabled (cm.enabled=false)"
                })
            );
        } else {
            println!("cm integration disabled (cm.enabled=false)");
        }
        return Ok(());
    }

    let mut client = CmClient::from_config(&ctx.config.cm);
    if let Ok(gate) = SafetyGate::from_env() {
        client = client.with_safety(gate);
    }

    if !client.is_available() {
        return Err(MsError::CmUnavailable("cm binary not available".to_string()));
    }

    match &args.command {
        CmCommand::Context { task } => {
            let context = client.context(task)?;
            if ctx.robot_mode {
                println!(
                    "{}",
                    serde_json::to_string(&context).unwrap_or_default()
                );
            } else {
                println!("CM context for: {}", task);
                println!(
                    "relevant_bullets: {}",
                    context.relevant_bullets.len()
                );
                println!("anti_patterns: {}", context.anti_patterns.len());
                println!("history_snippets: {}", context.history_snippets.len());
                if !context.suggested_cass_queries.is_empty() {
                    println!("suggested_cass_queries:");
                    for q in &context.suggested_cass_queries {
                        println!("- {}", q);
                    }
                }
            }
        }
    }

    Ok(())
}
