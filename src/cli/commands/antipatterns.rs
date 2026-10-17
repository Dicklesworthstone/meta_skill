//! ms antipatterns - Mine and manage anti-patterns from CASS sessions
//!
//! Provides commands to mine anti-patterns from sessions, view details,
//! and link anti-patterns to positive patterns they constrain.

use clap::{Args, Subcommand};
use colored::{Color, Colorize};

use crate::antipatterns::{
    AntiPattern, AntiPatternSeverity, DefaultDetector, format_anti_patterns, mine_anti_patterns,
};
use crate::app::AppContext;
use crate::cass::CassClient;
use crate::cli::output::OutputFormat;
use crate::error::{MsError, Result};

#[derive(Args, Debug)]
pub struct AntiPatternsArgs {
    #[command(subcommand)]
    pub command: AntiPatternsCommand,
}

#[derive(Subcommand, Debug)]
pub enum AntiPatternsCommand {
    /// Mine anti-patterns from CASS sessions
    Mine(MineArgs),

    /// Show details of an anti-pattern
    Show(ShowArgs),

    /// List all mined anti-patterns
    List(ListArgs),

    /// Link an anti-pattern to the positive pattern it constrains
    Link(LinkArgs),
}

#[derive(Args, Debug)]
pub struct MineArgs {
    /// Session IDs to mine from
    pub sessions: Vec<String>,

    /// Output format: text, json
    #[arg(long, default_value = "text")]
    pub format: String,

    /// Minimum confidence threshold (0.0 - 1.0)
    #[arg(long, default_value = "0.3")]
    pub min_confidence: f32,

    /// Save results to database
    #[arg(long)]
    pub save: bool,
}

#[derive(Args, Debug)]
pub struct ShowArgs {
    /// Anti-pattern ID to show
    pub id: String,

    /// Show full evidence details
    #[arg(long)]
    pub evidence: bool,
}

#[derive(Args, Debug)]
pub struct ListArgs {
    /// Limit number of records
    #[arg(long, default_value = "50")]
    pub limit: usize,

    /// Filter by severity (advisory, warning, blocking)
    #[arg(long)]
    pub severity: Option<String>,

    /// Show only orphaned anti-patterns (no positive counterpart)
    #[arg(long)]
    pub orphaned: bool,
}

#[derive(Args, Debug)]
pub struct LinkArgs {
    /// Anti-pattern ID
    pub anti_pattern_id: String,

    /// Positive pattern ID to link to
    pub pattern_id: String,
}

pub fn run(ctx: &AppContext, args: &AntiPatternsArgs) -> Result<()> {
    match &args.command {
        AntiPatternsCommand::Mine(mine_args) => run_mine(ctx, mine_args),
        AntiPatternsCommand::Show(show_args) => run_show(ctx, show_args),
        AntiPatternsCommand::List(list_args) => run_list(ctx, list_args),
        AntiPatternsCommand::Link(link_args) => run_link(ctx, link_args),
    }
}

fn antipatterns_store_path(ctx: &AppContext) -> std::path::PathBuf {
    ctx.ms_root.join("antipatterns.json")
}

fn load_stored_antipatterns(ctx: &AppContext) -> Result<Vec<AntiPattern>> {
    let path = antipatterns_store_path(ctx);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(&path)?;
    let items: Vec<AntiPattern> = serde_json::from_str(&content)
        .map_err(|e| MsError::Config(format!("failed to parse antipatterns store: {e}")))?;
    Ok(items)
}

fn save_stored_antipatterns(ctx: &AppContext, items: &[AntiPattern]) -> Result<()> {
    let path = antipatterns_store_path(ctx);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(items)?;
    std::fs::write(&path, content)?;
    Ok(())
}

fn run_mine(ctx: &AppContext, args: &MineArgs) -> Result<()> {
    if args.sessions.is_empty() {
        return Err(MsError::ValidationFailed(
            "at least one session ID is required".to_string(),
        ));
    }

    // Load sessions from CASS
    let cass = if let Some(ref cass_path) = ctx.config.cass.cass_path {
        CassClient::with_binary(cass_path)
    } else {
        CassClient::new()
    };
    let mut sessions = Vec::new();

    for session_id in &args.sessions {
        match cass.get_session(session_id) {
            Ok(session) => sessions.push(session),
            Err(e) => {
                if ctx.output_format == OutputFormat::Human {
                    eprintln!(
                        "{}: failed to load {}: {}",
                        "warning".yellow(),
                        session_id,
                        e
                    );
                }
            }
        }
    }

    if sessions.is_empty() {
        return Err(MsError::ValidationFailed(
            "no valid sessions found".to_string(),
        ));
    }

    // Mine anti-patterns
    let detector = DefaultDetector::default();
    let anti_patterns = mine_anti_patterns(&sessions, &detector)?;

    // Filter by confidence
    let filtered: Vec<_> = anti_patterns
        .into_iter()
        .filter(|ap| ap.confidence >= args.min_confidence)
        .collect();

    if args.save {
        let mut stored = load_stored_antipatterns(ctx).unwrap_or_default();
        for ap in &filtered {
            if let Some(existing) = stored.iter_mut().find(|item| item.id == ap.id) {
                *existing = ap.clone();
            } else {
                stored.push(ap.clone());
            }
        }
        save_stored_antipatterns(ctx, &stored)?;
    }

    // Output
    if ctx.output_format != OutputFormat::Human || args.format == "json" {
        output_mine_robot(&filtered)
    } else {
        output_mine_human(&filtered)
    }
}

fn run_show(ctx: &AppContext, args: &ShowArgs) -> Result<()> {
    let stored = load_stored_antipatterns(ctx)?;
    let Some(ap) = stored.iter().find(|ap| ap.id.0 == args.id) else {
        return Err(MsError::NotFound(format!(
            "anti-pattern not found: {}",
            args.id
        )));
    };

    if ctx.output_format != OutputFormat::Human {
        let mut obj = serde_json::json!({
            "status": "ok",
            "id": ap.id.0,
            "rule": ap.rule.statement,
            "severity": format!("{:?}", ap.rule.severity),
            "confidence": ap.confidence,
            "constrains": ap.constrains.as_ref().map(|p| &p.0),
            "orphaned": ap.is_orphaned(),
            "rationale": ap.rule.rationale,
            "instead": ap.rule.instead,
            "failure_modes": ap.failure_modes,
            "evidence_count": ap.evidence.len(),
        });
        if args.evidence {
            obj["evidence"] = serde_json::to_value(&ap.evidence)?;
        }
        println!("{}", serde_json::to_string_pretty(&obj)?);
    } else {
        println!("{} {}", "Anti-Pattern:".bold(), ap.id.0.cyan());
        println!("  {}: {}", "Rule".dimmed(), ap.rule.statement);
        println!("  {}: {:?}", "Severity".dimmed(), ap.rule.severity);
        println!("  {}: {:.0}%", "Confidence".dimmed(), ap.confidence * 100.0);
        if let Some(ref pat) = ap.constrains {
            println!("  {}: {}", "Constrains".dimmed(), pat.0.green());
        } else {
            println!("  {}: {}", "Constrains".dimmed(), "(orphaned)".yellow());
        }
        if let Some(ref rationale) = ap.rule.rationale {
            println!("  {}: {}", "Why".dimmed(), rationale);
        }
        if let Some(ref instead) = ap.rule.instead {
            println!("  {}: {}", "Instead".dimmed(), instead.green());
        }
        if args.evidence && !ap.evidence.is_empty() {
            println!("\n  {}:", "Evidence".bold());
            for ev in &ap.evidence {
                println!("    - [{}] {}", ev.session_id.0, ev.incident.description);
            }
        }
    }
    Ok(())
}

fn run_list(ctx: &AppContext, args: &ListArgs) -> Result<()> {
    let stored = load_stored_antipatterns(ctx)?;
    let filtered: Vec<&AntiPattern> = stored
        .iter()
        .filter(|ap| !args.orphaned || ap.is_orphaned())
        .filter(|ap| {
            if let Some(ref sev) = args.severity {
                let sev_lower = sev.to_ascii_lowercase();
                match ap.rule.severity {
                    AntiPatternSeverity::Advisory => sev_lower == "advisory",
                    AntiPatternSeverity::Warning => sev_lower == "warning",
                    AntiPatternSeverity::Blocking => sev_lower == "blocking",
                }
            } else {
                true
            }
        })
        .take(args.limit)
        .collect();

    if ctx.output_format != OutputFormat::Human {
        let output = serde_json::json!({
            "status": "ok",
            "count": filtered.len(),
            "anti_patterns": filtered.iter().map(|ap| {
                serde_json::json!({
                    "id": ap.id.0,
                    "rule": ap.rule.statement,
                    "severity": format!("{:?}", ap.rule.severity),
                    "confidence": ap.confidence,
                    "constrains": ap.constrains.as_ref().map(|p| &p.0),
                    "orphaned": ap.is_orphaned(),
                })
            }).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", "Anti-Patterns".bold());
        println!("{}", "═".repeat(50));
        println!();
        if filtered.is_empty() {
            println!(
                "{}",
                "No anti-patterns in database. Use 'mine --save' to extract from sessions."
                    .dimmed()
            );
        } else {
            let owned: Vec<AntiPattern> = filtered.into_iter().cloned().collect();
            output_mine_human(&owned)?;
        }
    }
    Ok(())
}

fn run_link(ctx: &AppContext, args: &LinkArgs) -> Result<()> {
    let mut stored = load_stored_antipatterns(ctx)?;
    let Some(ap) = stored.iter_mut().find(|ap| ap.id.0 == args.anti_pattern_id) else {
        return Err(MsError::NotFound(format!(
            "anti-pattern not found: {}",
            args.anti_pattern_id
        )));
    };
    ap.constrains = Some(crate::antipatterns::PatternId::new(&args.pattern_id));
    ap.updated_at = chrono::Utc::now();
    save_stored_antipatterns(ctx, &stored)?;

    if ctx.output_format != OutputFormat::Human {
        let output = serde_json::json!({
            "status": "ok",
            "anti_pattern_id": args.anti_pattern_id,
            "pattern_id": args.pattern_id,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!(
            "{} Linked {} → {}",
            "✓".green(),
            args.anti_pattern_id.cyan(),
            args.pattern_id.green()
        );
    }
    Ok(())
}

// =============================================================================
// HUMAN OUTPUT
// =============================================================================

fn output_mine_human(anti_patterns: &[AntiPattern]) -> Result<()> {
    if anti_patterns.is_empty() {
        println!("{}", "No anti-patterns detected.".dimmed());
        return Ok(());
    }

    println!("{}", "Mined Anti-Patterns".bold());
    println!("{}", "═".repeat(60));
    println!();

    for (i, ap) in anti_patterns.iter().enumerate() {
        let severity_color = match ap.rule.severity {
            AntiPatternSeverity::Advisory => Color::Blue,
            AntiPatternSeverity::Warning => Color::Yellow,
            AntiPatternSeverity::Blocking => Color::Red,
        };

        println!(
            "{} {}",
            format!("[{}]", i + 1).cyan(),
            format!("{:?}", ap.rule.severity)
                .to_uppercase()
                .color(severity_color)
        );
        println!("  {}: {}", "Rule".dimmed(), ap.rule.statement);

        if let Some(ref rationale) = ap.rule.rationale {
            println!("  {}: {}", "Why".dimmed(), rationale);
        }

        if let Some(ref instead) = ap.rule.instead {
            println!("  {}: {}", "Instead".dimmed(), instead.green());
        }

        println!(
            "  {}: {:.0}% ({} evidence sources)",
            "Confidence".dimmed(),
            ap.confidence * 100.0,
            ap.evidence.len()
        );

        if !ap.failure_modes.is_empty() {
            println!("  {}:", "Failure modes".dimmed());
            for fm in ap.failure_modes.iter().take(3) {
                println!("    - {}", fm.description);
            }
        }

        if ap.is_orphaned() {
            println!(
                "  {} {}",
                "⚠".yellow(),
                "No positive pattern linked".yellow()
            );
        }

        println!();
    }

    let section = format_anti_patterns(anti_patterns);
    println!(
        "{}",
        format!(
            "Total: {} anti-patterns ({} formatted)",
            anti_patterns.len(),
            section.patterns.len()
        )
        .dimmed()
    );

    Ok(())
}

// =============================================================================
// ROBOT OUTPUT
// =============================================================================

fn output_mine_robot(anti_patterns: &[AntiPattern]) -> Result<()> {
    let section = format_anti_patterns(anti_patterns);

    let output = serde_json::json!({
        "status": "ok",
        "count": anti_patterns.len(),
        "formatted_count": section.patterns.len(),
        "anti_patterns": anti_patterns.iter().map(|ap| {
            serde_json::json!({
                "id": ap.id.0,
                "rule": ap.rule.statement,
                "severity": format!("{:?}", ap.rule.severity),
                "confidence": ap.confidence,
                "evidence_count": ap.evidence.len(),
                "failure_modes": ap.failure_modes.iter().map(|fm| fm.description.clone()).collect::<Vec<_>>(),
                "orphaned": ap.is_orphaned(),
                "rationale": ap.rule.rationale,
                "instead": ap.rule.instead,
            })
        }).collect::<Vec<_>>(),
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_color_mapping() {
        // Just verify severity variants are handled
        let severities = [
            AntiPatternSeverity::Advisory,
            AntiPatternSeverity::Warning,
            AntiPatternSeverity::Blocking,
        ];

        for sev in severities {
            let _ = match sev {
                AntiPatternSeverity::Advisory => Color::Blue,
                AntiPatternSeverity::Warning => Color::Yellow,
                AntiPatternSeverity::Blocking => Color::Red,
            };
        }
    }
}
