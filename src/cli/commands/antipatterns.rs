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

    // Output
    if ctx.output_format != OutputFormat::Human || args.format == "json" {
        output_mine_robot(&filtered)
    } else {
        output_mine_human(&filtered)
    }
}

fn run_show(ctx: &AppContext, args: &ShowArgs) -> Result<()> {
    // For now, we don't have persistent storage, so show is a placeholder
    // In a full implementation, this would query the database
    if ctx.output_format != OutputFormat::Human {
        let output = serde_json::json!({
            "status": "error",
            "message": "anti-pattern storage not yet implemented"
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!(
            "{}",
            "Anti-pattern storage not yet implemented. Use 'mine' to extract from sessions."
                .yellow()
        );
        println!("Requested ID: {}", args.id.cyan());
    }
    Ok(())
}

fn run_list(ctx: &AppContext, args: &ListArgs) -> Result<()> {
    // Placeholder - would query database in full implementation
    if ctx.output_format != OutputFormat::Human {
        let output = serde_json::json!({
            "status": "ok",
            "count": 0,
            "anti_patterns": []
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", "Anti-Patterns".bold());
        println!("{}", "═".repeat(50));
        println!();
        println!(
            "{}",
            "No anti-patterns in database. Use 'mine' to extract from sessions.".dimmed()
        );
        if args.orphaned {
            println!("{}", "(--orphaned filter active)".dimmed());
        }
        if let Some(ref sev) = args.severity {
            println!("{}", format!("(--severity {sev} filter active)").dimmed());
        }
    }
    Ok(())
}

fn run_link(_ctx: &AppContext, args: &LinkArgs) -> Result<()> {
    // Placeholder - would update database in full implementation
    println!(
        "{}",
        format!(
            "Link {} → {} (storage not yet implemented)",
            args.anti_pattern_id, args.pattern_id
        )
        .yellow()
    );
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
