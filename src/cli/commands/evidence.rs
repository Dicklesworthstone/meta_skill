//! ms evidence - View and manage skill provenance evidence
//!
//! Provides commands to view evidence linking skills to CASS sessions,
//! export provenance graphs, and navigate to source sessions.

use clap::{Args, Subcommand};
use colored::Colorize;

use crate::app::AppContext;
use crate::cli::output::OutputFormat;
use crate::core::{EvidenceLevel, EvidenceRef};
use crate::error::{MsError, Result};
use crate::utils::format::truncate_string;

#[derive(Args, Debug)]
pub struct EvidenceArgs {
    #[command(subcommand)]
    pub command: EvidenceCommand,
}

#[derive(Subcommand, Debug)]
pub enum EvidenceCommand {
    /// Show evidence for a skill
    Show(ShowEvidenceArgs),

    /// List all evidence records
    List(ListEvidenceArgs),

    /// Export provenance graph
    Export(ExportEvidenceArgs),
}

#[derive(Args, Debug)]
pub struct ShowEvidenceArgs {
    /// Skill ID to show evidence for
    pub skill_id: String,

    /// Optional rule ID to filter by
    #[arg(long)]
    pub rule: Option<String>,

    /// Show excerpts (not just pointers)
    #[arg(long)]
    pub excerpts: bool,
}

#[derive(Args, Debug)]
pub struct ListEvidenceArgs {
    /// Limit number of records
    #[arg(long, default_value = "100")]
    pub limit: usize,

    /// Filter by skill ID pattern
    #[arg(long)]
    pub skill: Option<String>,
}

#[derive(Args, Debug)]
pub struct ExportEvidenceArgs {
    /// Output format: json, dot (graphviz)
    #[arg(long, default_value = "json")]
    pub format: String,

    /// Output file (stdout if not specified)
    #[arg(long, short)]
    pub output: Option<String>,

    /// Filter to specific skill
    #[arg(long)]
    pub skill: Option<String>,
}

pub fn run(ctx: &AppContext, args: &EvidenceArgs) -> Result<()> {
    match &args.command {
        EvidenceCommand::Show(show_args) => run_show(ctx, show_args),
        EvidenceCommand::List(list_args) => run_list(ctx, list_args),
        EvidenceCommand::Export(export_args) => run_export(ctx, export_args),
    }
}

fn run_show(ctx: &AppContext, args: &ShowEvidenceArgs) -> Result<()> {
    // Check if skill exists
    let skill = ctx
        .db
        .get_skill(&args.skill_id)?
        .ok_or_else(|| MsError::SkillNotFound(format!("skill not found: {}", args.skill_id)))?;

    // Get evidence
    if let Some(ref rule_id) = args.rule {
        // Show evidence for specific rule
        let evidence = ctx.db.get_rule_evidence(&args.skill_id, rule_id)?;
        if ctx.output_format != OutputFormat::Human {
            show_rule_evidence_robot(&skill.id, rule_id, &evidence)
        } else {
            show_rule_evidence_human(&skill.id, rule_id, &evidence, args.excerpts)
        }
    } else {
        // Show all evidence for skill
        let index = ctx.db.get_evidence(&args.skill_id)?;
        if ctx.output_format != OutputFormat::Human {
            show_evidence_index_robot(&skill.id, &index)
        } else {
            show_evidence_index_human(&skill.id, &skill.name, &index, args.excerpts)
        }
    }
}

fn run_list(ctx: &AppContext, args: &ListEvidenceArgs) -> Result<()> {
    let all_evidence = ctx.db.list_all_evidence()?;

    // Filter by skill pattern if specified
    let filtered: Vec<_> = if let Some(ref pattern) = args.skill {
        all_evidence
            .into_iter()
            .filter(|r| r.skill_id.contains(pattern))
            .take(args.limit)
            .collect()
    } else {
        all_evidence.into_iter().take(args.limit).collect()
    };

    if ctx.output_format != OutputFormat::Human {
        list_evidence_robot(&filtered)
    } else {
        list_evidence_human(&filtered)
    }
}

fn run_export(ctx: &AppContext, args: &ExportEvidenceArgs) -> Result<()> {
    let all_evidence = ctx.db.list_all_evidence()?;

    // Filter by skill if specified
    let filtered: Vec<_> = if let Some(ref skill_id) = args.skill {
        all_evidence
            .into_iter()
            .filter(|r| r.skill_id == *skill_id)
            .collect()
    } else {
        all_evidence
    };

    let output = match args.format.as_str() {
        "json" => export_json(&filtered)?,
        "dot" => export_dot(&filtered)?,
        other => {
            return Err(MsError::Config(format!(
                "unsupported export format: {other} (use json or dot)"
            )));
        }
    };

    if let Some(ref path) = args.output {
        std::fs::write(path, &output)?;
        if ctx.output_format == OutputFormat::Human {
            println!("Exported to: {path}");
        }
    } else {
        println!("{output}");
    }

    Ok(())
}

// =============================================================================
// HUMAN OUTPUT
// =============================================================================

fn show_evidence_index_human(
    skill_id: &str,
    skill_name: &str,
    index: &crate::core::SkillEvidenceIndex,
    show_excerpts: bool,
) -> Result<()> {
    println!("{}", format!("Evidence for: {skill_name}").bold());
    println!("{}", "═".repeat(50));
    println!();

    // Coverage stats
    println!("{}", "Coverage".bold());
    println!("{}", "─".repeat(30).dimmed());
    println!(
        "{}: {}",
        "Rules with evidence".dimmed(),
        index.coverage.rules_with_evidence
    );
    println!(
        "{}: {:.1}%",
        "Avg confidence".dimmed(),
        index.coverage.avg_confidence * 100.0
    );
    println!();

    if index.rules.is_empty() {
        println!("{}", "No evidence recorded for this skill.".dimmed());
        return Ok(());
    }

    // Rules and their evidence
    println!("{}", "Rules".bold());
    println!("{}", "─".repeat(30).dimmed());

    for (rule_id, refs) in &index.rules {
        let ref_count = refs.len();
        let avg_conf: f32 = if refs.is_empty() {
            0.0
        } else {
            refs.iter().map(|r| r.confidence).sum::<f32>() / ref_count as f32
        };

        println!(
            "  {} ({} refs, {:.0}% conf)",
            rule_id.cyan(),
            ref_count,
            avg_conf * 100.0
        );

        for (i, eref) in refs.iter().enumerate() {
            let level_str = match eref.level {
                EvidenceLevel::Pointer => "→",
                EvidenceLevel::Excerpt => "◆",
                EvidenceLevel::Expanded => "●",
            };

            println!(
                "    {} session:{} msgs:{}-{}",
                level_str.dimmed(),
                eref.session_id,
                eref.message_range.0,
                eref.message_range.1
            );

            if show_excerpts {
                if let Some(ref excerpt) = eref.excerpt {
                    let truncated = truncate_string(excerpt, 80);
                    println!("      \"{}\"", truncated.dimmed());
                }
            }

            if i >= 2 && refs.len() > 3 {
                println!(
                    "    {} more...",
                    format!("... {} ", refs.len() - 3).dimmed()
                );
                break;
            }
        }
    }

    println!();
    println!(
        "{}",
        format!(
            "Jump to source: ms evidence show {skill_id} --rule <rule-id>"
        )
        .dimmed()
    );

    Ok(())
}

fn show_rule_evidence_human(
    skill_id: &str,
    rule_id: &str,
    evidence: &[EvidenceRef],
    show_excerpts: bool,
) -> Result<()> {
    println!(
        "{}",
        format!("Evidence for {skill_id}/{rule_id}").bold()
    );
    println!("{}", "═".repeat(50));
    println!();

    if evidence.is_empty() {
        println!("{}", "No evidence recorded for this rule.".dimmed());
        return Ok(());
    }

    for (i, eref) in evidence.iter().enumerate() {
        let level_str = match eref.level {
            EvidenceLevel::Pointer => "Pointer",
            EvidenceLevel::Excerpt => "Excerpt",
            EvidenceLevel::Expanded => "Expanded",
        };

        println!(
            "{} {} ({})",
            format!("[{}]", i + 1).cyan(),
            level_str,
            format!("{:.0}% confidence", eref.confidence * 100.0).dimmed()
        );
        println!("  {}: {}", "Session".dimmed(), eref.session_id);
        println!(
            "  {}: {}-{}",
            "Messages".dimmed(),
            eref.message_range.0,
            eref.message_range.1
        );
        println!(
            "  {}: {}",
            "Hash".dimmed(),
            &eref.snippet_hash[..16.min(eref.snippet_hash.len())]
        );

        if show_excerpts || eref.level != EvidenceLevel::Pointer {
            if let Some(ref excerpt) = eref.excerpt {
                println!();
                println!("  {}", "─".repeat(40).dimmed());
                for line in excerpt.lines().take(5) {
                    println!("  {line}");
                }
                if excerpt.lines().count() > 5 {
                    println!("  {}", "...".dimmed());
                }
                println!("  {}", "─".repeat(40).dimmed());
            }
        }

        println!();
    }

    Ok(())
}

fn list_evidence_human(records: &[crate::storage::sqlite::EvidenceRecord]) -> Result<()> {
    if records.is_empty() {
        println!("{}", "No evidence records found.".dimmed());
        return Ok(());
    }

    println!("{}", "Evidence Records".bold());
    println!("{}", "═".repeat(60));
    println!();

    let mut current_skill = String::new();
    for record in records {
        if record.skill_id != current_skill {
            if !current_skill.is_empty() {
                println!();
            }
            current_skill = record.skill_id.clone();
            println!("{}", record.skill_id.cyan().bold());
        }

        let ref_count = record.evidence.len();
        let avg_conf: f32 = if record.evidence.is_empty() {
            0.0
        } else {
            record.evidence.iter().map(|e| e.confidence).sum::<f32>() / ref_count as f32
        };

        println!(
            "  {} {} refs, {:.0}% avg conf",
            record.rule_id,
            ref_count,
            avg_conf * 100.0
        );
    }

    println!();
    println!("Total: {} rule-evidence mappings", records.len());

    Ok(())
}

// =============================================================================
// ROBOT OUTPUT
// =============================================================================

fn show_evidence_index_robot(
    skill_id: &str,
    index: &crate::core::SkillEvidenceIndex,
) -> Result<()> {
    let output = serde_json::json!({
        "status": "ok",
        "skill_id": skill_id,
        "coverage": {
            "total_rules": index.coverage.total_rules,
            "rules_with_evidence": index.coverage.rules_with_evidence,
            "avg_confidence": index.coverage.avg_confidence,
        },
        "rules": index.rules,
    });
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn show_rule_evidence_robot(skill_id: &str, rule_id: &str, evidence: &[EvidenceRef]) -> Result<()> {
    let output = serde_json::json!({
        "status": "ok",
        "skill_id": skill_id,
        "rule_id": rule_id,
        "evidence_count": evidence.len(),
        "evidence": evidence,
    });
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn list_evidence_robot(records: &[crate::storage::sqlite::EvidenceRecord]) -> Result<()> {
    let output = serde_json::json!({
        "status": "ok",
        "count": records.len(),
        "records": records.iter().map(|r| serde_json::json!({
            "skill_id": r.skill_id,
            "rule_id": r.rule_id,
            "evidence_count": r.evidence.len(),
            "updated_at": r.updated_at,
        })).collect::<Vec<_>>(),
    });
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

// =============================================================================
// EXPORT FORMATS
// =============================================================================

fn export_json(records: &[crate::storage::sqlite::EvidenceRecord]) -> Result<String> {
    let graph = serde_json::json!({
        "format": "provenance_graph",
        "version": "1.0",
        "nodes": build_nodes(records),
        "edges": build_edges(records),
    });
    serde_json::to_string_pretty(&graph)
        .map_err(|e| MsError::Serialization(format!("JSON export failed: {e}")))
}

fn export_dot(records: &[crate::storage::sqlite::EvidenceRecord]) -> Result<String> {
    let mut dot = String::new();
    dot.push_str("digraph provenance {\n");
    dot.push_str("  rankdir=LR;\n");
    dot.push_str("  node [shape=box];\n\n");

    // Collect unique skills and sessions
    let mut skills = std::collections::HashSet::new();
    let mut sessions = std::collections::HashSet::new();

    for record in records {
        skills.insert(&record.skill_id);
        for eref in &record.evidence {
            sessions.insert(&eref.session_id);
        }
    }

    // Skill nodes (blue)
    dot.push_str("  // Skills\n");
    for skill in &skills {
        dot.push_str(&format!(
            "  \"skill:{skill}\" [label=\"{skill}\" color=blue style=filled fillcolor=lightblue];\n"
        ));
    }

    // Session nodes (green)
    dot.push_str("\n  // Sessions\n");
    for session in &sessions {
        let short_id = truncate_string(session, 12);
        dot.push_str(&format!(
            "  \"session:{session}\" [label=\"{short_id}\" color=green style=filled fillcolor=lightgreen];\n"
        ));
    }

    // Edges
    dot.push_str("\n  // Evidence links\n");
    for record in records {
        for eref in &record.evidence {
            dot.push_str(&format!(
                "  \"session:{}\" -> \"skill:{}\" [label=\"{} ({:.0}%)\" fontsize=10];\n",
                eref.session_id,
                record.skill_id,
                record.rule_id,
                eref.confidence * 100.0
            ));
        }
    }

    dot.push_str("}\n");
    Ok(dot)
}

fn build_nodes(records: &[crate::storage::sqlite::EvidenceRecord]) -> Vec<serde_json::Value> {
    let mut nodes = Vec::new();
    let mut seen_skills = std::collections::HashSet::new();
    let mut seen_sessions = std::collections::HashSet::new();

    for record in records {
        // Add skill node
        if seen_skills.insert(&record.skill_id) {
            nodes.push(serde_json::json!({
                "id": format!("skill:{}", record.skill_id),
                "type": "skill",
                "label": record.skill_id,
            }));
        }

        // Add rule node
        nodes.push(serde_json::json!({
            "id": format!("rule:{}:{}", record.skill_id, record.rule_id),
            "type": "rule",
            "label": record.rule_id,
            "parent_skill": record.skill_id,
        }));

        // Add session nodes
        for eref in &record.evidence {
            if seen_sessions.insert(&eref.session_id) {
                nodes.push(serde_json::json!({
                    "id": format!("session:{}", eref.session_id),
                    "type": "session",
                    "label": eref.session_id,
                }));
            }
        }
    }

    nodes
}

fn build_edges(records: &[crate::storage::sqlite::EvidenceRecord]) -> Vec<serde_json::Value> {
    let mut edges = Vec::new();

    for record in records {
        // skill -> rule edge
        edges.push(serde_json::json!({
            "from": format!("skill:{}", record.skill_id),
            "to": format!("rule:{}:{}", record.skill_id, record.rule_id),
            "type": "contains",
        }));

        // rule -> session edges
        for eref in &record.evidence {
            edges.push(serde_json::json!({
                "from": format!("rule:{}:{}", record.skill_id, record.rule_id),
                "to": format!("session:{}", eref.session_id),
                "type": "evidence",
                "confidence": eref.confidence,
                "message_range": [eref.message_range.0, eref.message_range.1],
            }));
        }
    }

    edges
}
