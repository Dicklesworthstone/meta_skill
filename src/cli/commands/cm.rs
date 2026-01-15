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
    /// List playbook rules
    Rules {
        /// Filter by category
        #[arg(long)]
        category: Option<String>,
        /// Maximum number of rules to show
        #[arg(long, default_value = "20")]
        limit: usize,
    },
    /// Find similar rules in the playbook
    Similar {
        /// Query text to match against playbook
        query: String,
        /// Minimum similarity threshold (0.0-1.0)
        #[arg(long, default_value = "0.7")]
        threshold: f32,
    },
    /// Check CM status and availability
    Status,
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
        return Err(MsError::CmUnavailable(
            "cm binary not available".to_string(),
        ));
    }

    match &args.command {
        CmCommand::Context { task } => {
            let context = client.context(task)?;
            if ctx.robot_mode {
                println!("{}", serde_json::to_string(&context).unwrap_or_default());
            } else {
                println!("CM context for: {}", task);
                println!("relevant_bullets: {}", context.relevant_bullets.len());
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

        CmCommand::Rules { category, limit } => {
            let rules = client.get_rules(category.as_deref())?;
            let rules: Vec<_> = rules.into_iter().take(*limit).collect();

            if ctx.robot_mode {
                println!(
                    "{}",
                    serde_json::json!({
                        "rules": rules,
                        "count": rules.len()
                    })
                );
            } else {
                if rules.is_empty() {
                    println!("No rules found");
                } else {
                    println!("Playbook rules ({} shown):\n", rules.len());
                    for rule in &rules {
                        println!("[{}] {} ({})", rule.id, rule.content, rule.category);
                        println!(
                            "    confidence: {:.2}, helpful: {}, harmful: {}",
                            rule.confidence, rule.helpful_count, rule.harmful_count
                        );
                        println!();
                    }
                }
            }
        }

        CmCommand::Similar { query, threshold } => {
            let matches = client.similar(query, Some(*threshold))?;

            if ctx.robot_mode {
                println!(
                    "{}",
                    serde_json::json!({
                        "query": query,
                        "matches": matches
                    })
                );
            } else {
                if matches.is_empty() {
                    println!("No similar rules found for: {}", query);
                } else {
                    println!("Similar rules for: {}\n", query);
                    for m in &matches {
                        println!("[{}] {} (similarity: {:.2})", m.id, m.content, m.similarity);
                    }
                }
            }
        }

        CmCommand::Status => {
            let available = client.is_available();
            if ctx.robot_mode {
                println!(
                    "{}",
                    serde_json::json!({
                        "available": available,
                        "cm_path": ctx.config.cm.cm_path.as_deref().unwrap_or("cm"),
                        "enabled": ctx.config.cm.enabled
                    })
                );
            } else {
                if available {
                    println!("✓ CM is available");
                } else {
                    println!("✗ CM is not available");
                }
                if let Some(path) = &ctx.config.cm.cm_path {
                    println!("  path: {}", path);
                }
            }
        }
    }

    Ok(())
}
