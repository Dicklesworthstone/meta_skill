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
use crate::cass::mining::{with_secrets_redacted, without_tainted_messages};
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
    /// Session paths or session IDs to mine from
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

/// Return true when `session_ref` looks like a filesystem path rather than a bare session ID.
fn looks_like_session_path(session_ref: &str) -> bool {
    session_ref.contains('/')
        || session_ref.contains('\\')
        || session_ref.ends_with(".jsonl")
        || session_ref.ends_with(".json")
        || std::path::Path::new(session_ref).exists()
}

/// Resolve either a session path or a bare session ID via CASS and export the session.
fn resolve_and_load_session(cass: &CassClient, session_ref: &str) -> Result<crate::cass::Session> {
    if looks_like_session_path(session_ref) {
        return cass.get_session(session_ref);
    }

    let matches_id = |m: &crate::cass::SessionMatch| {
        m.session_id == session_ref
            || std::path::Path::new(&m.path)
                .file_stem()
                .and_then(|s| s.to_str())
                == Some(session_ref)
    };

    for (query, limit) in [(session_ref, 50usize), ("*", 200usize)] {
        if let Ok(hits) = cass.search(query, limit) {
            if let Some(found) = hits.into_iter().find(matches_id) {
                return cass.get_session(&found.path);
            }
        }
    }

    // Fallback: let `cass export` attempt the identifier directly and surface its error.
    cass.get_session(session_ref)
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

    // Load sessions from CASS (honoring config and SafetyGate)
    let cass = CassClient::from_context(ctx);
    let mut sessions = Vec::new();
    let mut load_errors = Vec::new();

    for session_ref in &args.sessions {
        match resolve_and_load_session(&cass, session_ref) {
            Ok(session) => sessions.push(session),
            Err(e) => {
                tracing::warn!(session = %session_ref, error = %e, "failed to load session for antipattern mining");
                load_errors.push(format!("{session_ref}: {e}"));
                if ctx.output_format == OutputFormat::Human {
                    eprintln!(
                        "{}: failed to load {}: {}",
                        "warning".yellow(),
                        session_ref,
                        e
                    );
                }
            }
        }
    }

    if sessions.is_empty() {
        let detail = if load_errors.is_empty() {
            "no valid sessions found".to_string()
        } else {
            format!("no valid sessions found ({})", load_errors.join("; "))
        };
        return Err(MsError::ValidationFailed(detail));
    }

    // Mine anti-patterns
    let sessions: Vec<_> = sessions
        .iter()
        .map(|session| with_secrets_redacted(&without_tainted_messages(session, false)))
        .collect();
    let detector = DefaultDetector::default();
    let anti_patterns = mine_anti_patterns(&sessions, &detector)?;

    // Filter by confidence
    let filtered: Vec<_> = anti_patterns
        .into_iter()
        .filter(|ap| ap.confidence >= args.min_confidence)
        .collect();

    if args.save {
        let mut stored = load_stored_antipatterns(ctx)?;
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
    let candidates: Vec<&AntiPattern> = if args.orphaned {
        crate::antipatterns::find_orphaned(&stored)
    } else {
        stored.iter().collect()
    };
    let filtered: Vec<&AntiPattern> = candidates
        .into_iter()
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
    crate::antipatterns::link_to_pattern(ap, crate::antipatterns::PatternId::new(&args.pattern_id));
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
