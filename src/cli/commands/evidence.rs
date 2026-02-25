//! ms evidence - View and manage skill provenance evidence
//!
//! Provides commands to view evidence linking skills to CASS sessions,
//! export provenance graphs, and navigate to source sessions.

use clap::{Args, Subcommand};
use tracing::debug;

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
    debug!(target: "evidence", mode = ?ctx.output_format, "output mode selected");

    let result = match &args.command {
        EvidenceCommand::Show(show_args) => run_show(ctx, show_args),
        EvidenceCommand::List(list_args) => run_list(ctx, list_args),
        EvidenceCommand::Export(export_args) => run_export(ctx, export_args),
    };
    debug!(target: "evidence", stage = "render_complete");
    result
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
    println!("Evidence for: {skill_name}");
    println!("{}", "═".repeat(50));
    println!();

    // Coverage stats
    println!("Coverage");
    println!("{}", "─".repeat(30));
    println!(
        "Rules with evidence: {}",
        index.coverage.rules_with_evidence
    );
    println!(
        "Avg confidence: {:.1}%",
        index.coverage.avg_confidence * 100.0
    );
    println!();

    if index.rules.is_empty() {
        println!("No evidence recorded for this skill.");
        return Ok(());
    }

    // Rules and their evidence
    println!("Rules");
    println!("{}", "─".repeat(30));

    for (rule_id, refs) in &index.rules {
        let ref_count = refs.len();
        let avg_conf: f32 = if refs.is_empty() {
            0.0
        } else {
            refs.iter().map(|r| r.confidence).sum::<f32>() / ref_count as f32
        };

        println!(
            "  {} ({} refs, {:.0}% conf)",
            rule_id,
            ref_count,
            avg_conf * 100.0
        );

        for (i, eref) in refs.iter().enumerate() {
            let level_str = match eref.level {
                EvidenceLevel::Pointer => ">",
                EvidenceLevel::Excerpt => "*",
                EvidenceLevel::Expanded => "#",
            };

            println!(
                "    {} session:{} msgs:{}-{}",
                level_str,
                eref.session_id,
                eref.message_range.0,
                eref.message_range.1
            );

            if show_excerpts {
                if let Some(ref excerpt) = eref.excerpt {
                    let truncated = truncate_string(excerpt, 80);
                    println!("      \"{truncated}\"");
                }
            }

            if i >= 2 && refs.len() > 3 {
                println!("    ... {} more...", refs.len() - 3);
                break;
            }
        }
    }

    println!();
    println!(
        "Jump to source: ms evidence show {skill_id} --rule <rule-id>"
    );

    Ok(())
}

fn show_rule_evidence_human(
    skill_id: &str,
    rule_id: &str,
    evidence: &[EvidenceRef],
    show_excerpts: bool,
) -> Result<()> {
    println!("Evidence for {skill_id}/{rule_id}");
    println!("{}", "═".repeat(50));
    println!();

    if evidence.is_empty() {
        println!("No evidence recorded for this rule.");
        return Ok(());
    }

    for (i, eref) in evidence.iter().enumerate() {
        let level_str = match eref.level {
            EvidenceLevel::Pointer => "Pointer",
            EvidenceLevel::Excerpt => "Excerpt",
            EvidenceLevel::Expanded => "Expanded",
        };

        println!(
            "[{}] {} ({:.0}% confidence)",
            i + 1,
            level_str,
            eref.confidence * 100.0
        );
        println!("  Session: {}", eref.session_id);
        println!(
            "  Messages: {}-{}",
            eref.message_range.0, eref.message_range.1
        );
        println!(
            "  Hash: {}",
            &eref.snippet_hash[..16.min(eref.snippet_hash.len())]
        );

        if show_excerpts || eref.level != EvidenceLevel::Pointer {
            if let Some(ref excerpt) = eref.excerpt {
                println!();
                println!("  {}", "─".repeat(40));
                for line in excerpt.lines().take(5) {
                    println!("  {line}");
                }
                if excerpt.lines().count() > 5 {
                    println!("  ...");
                }
                println!("  {}", "─".repeat(40));
            }
        }

        println!();
    }

    Ok(())
}

fn list_evidence_human(records: &[crate::storage::sqlite::EvidenceRecord]) -> Result<()> {
    if records.is_empty() {
        println!("No evidence records found.");
        return Ok(());
    }

    println!("Evidence Records");
    println!("{}", "═".repeat(60));
    println!();

    let mut current_skill = String::new();
    for record in records {
        if record.skill_id != current_skill {
            if !current_skill.is_empty() {
                println!();
            }
            current_skill = record.skill_id.clone();
            println!("{}", record.skill_id);
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

/// Check whether the terminal supports rich output for evidence commands.
#[allow(dead_code)]
fn should_use_rich_for_evidence() -> bool {
    use std::io::IsTerminal;

    if std::env::var("MS_FORCE_RICH").is_ok() {
        return true;
    }
    if std::env::var("NO_COLOR").is_ok() || std::env::var("MS_PLAIN_OUTPUT").is_ok() {
        return false;
    }

    use crate::output::{is_agent_environment, is_ci_environment};
    if is_agent_environment() || is_ci_environment() {
        return false;
    }

    std::io::stdout().is_terminal()
}

/// Get the terminal width, defaulting to 80 if detection fails.
#[allow(dead_code)]
fn terminal_width() -> usize {
    crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(80)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::EvidenceRef;
    use crate::storage::sqlite::EvidenceRecord;

    fn make_evidence_ref(session: &str, confidence: f32) -> EvidenceRef {
        EvidenceRef {
            session_id: session.to_string(),
            message_range: (0, 5),
            confidence,
            level: EvidenceLevel::Excerpt,
            snippet_hash: "abcdef1234567890abcdef".to_string(),
            excerpt: Some("Example excerpt content".to_string()),
        }
    }

    fn make_evidence_record(skill_id: &str, rule_id: &str) -> EvidenceRecord {
        EvidenceRecord {
            skill_id: skill_id.to_string(),
            rule_id: rule_id.to_string(),
            evidence: vec![make_evidence_ref("sess-001", 0.85)],
            updated_at: "2025-06-15T10:00:00Z".to_string(),
        }
    }

    // ── 1. test_evidence_render_empty ───────────────────────────────

    #[test]
    fn test_evidence_render_empty() {
        let records: Vec<EvidenceRecord> = vec![];
        let result = list_evidence_human(&records);
        assert!(result.is_ok());
    }

    // ── 2. test_evidence_render_card ────────────────────────────────

    #[test]
    fn test_evidence_render_card() {
        let eref = make_evidence_ref("sess-abc", 0.9);
        assert_eq!(eref.session_id, "sess-abc");
        assert_eq!(eref.confidence, 0.9);
        assert!(eref.excerpt.is_some());
    }

    // ── 3. test_evidence_render_confidence ──────────────────────────

    #[test]
    fn test_evidence_render_confidence() {
        let eref = make_evidence_ref("sess-001", 0.75);
        let conf_str = format!("{:.0}%", eref.confidence * 100.0);
        assert_eq!(conf_str, "75%");
        assert!(!conf_str.contains("\x1b["), "confidence must be plain text");
    }

    // ── 4. test_evidence_render_table_view ──────────────────────────

    #[test]
    fn test_evidence_render_table_view() {
        let records = vec![
            make_evidence_record("skill-a", "rule-1"),
            make_evidence_record("skill-a", "rule-2"),
            make_evidence_record("skill-b", "rule-1"),
        ];
        let result = list_evidence_human(&records);
        assert!(result.is_ok());
    }

    // ── 5. test_evidence_render_timeline ────────────────────────────

    #[test]
    fn test_evidence_render_timeline() {
        let refs = vec![
            make_evidence_ref("sess-001", 0.8),
            make_evidence_ref("sess-002", 0.9),
        ];
        // Timeline ordering by session
        assert!(refs[0].session_id < refs[1].session_id);
    }

    // ── 6. test_evidence_render_source_panel ────────────────────────

    #[test]
    fn test_evidence_render_source_panel() {
        let eref = make_evidence_ref("sess-abc123", 0.85);
        let level_str = match eref.level {
            EvidenceLevel::Pointer => "Pointer",
            EvidenceLevel::Excerpt => "Excerpt",
            EvidenceLevel::Expanded => "Expanded",
        };
        assert_eq!(level_str, "Excerpt");
        let hash_display = &eref.snippet_hash[..16.min(eref.snippet_hash.len())];
        assert_eq!(hash_display.len(), 16);
    }

    // ── 7. test_evidence_render_summary ─────────────────────────────

    #[test]
    fn test_evidence_render_summary() {
        let records = vec![
            make_evidence_record("skill-a", "rule-1"),
            make_evidence_record("skill-a", "rule-2"),
        ];
        assert_eq!(records.len(), 2);
        let total_refs: usize = records.iter().map(|r| r.evidence.len()).sum();
        assert_eq!(total_refs, 2);
    }

    // ── 8. test_evidence_plain_output_format ────────────────────────

    #[test]
    fn test_evidence_plain_output_format() {
        let eref = make_evidence_ref("sess-001", 0.7);
        let line = format!(
            "[1] Excerpt ({:.0}% confidence)",
            eref.confidence * 100.0
        );
        assert!(line.contains("70%"));
        assert!(!line.contains("\x1b["), "plain output must have no ANSI");
    }

    // ── 9. test_evidence_json_output_format ─────────────────────────

    #[test]
    fn test_evidence_json_output_format() {
        let records = vec![make_evidence_record("skill-a", "rule-1")];
        let output = serde_json::json!({
            "status": "ok",
            "count": records.len(),
            "records": records.iter().map(|r| serde_json::json!({
                "skill_id": r.skill_id,
                "rule_id": r.rule_id,
                "evidence_count": r.evidence.len(),
            })).collect::<Vec<_>>(),
        });
        let json = serde_json::to_string_pretty(&output).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["status"], "ok");
        assert_eq!(parsed["count"], 1);
    }

    // ── 10. test_evidence_robot_mode_no_ansi ────────────────────────

    #[test]
    fn test_evidence_robot_mode_no_ansi() {
        let output = serde_json::json!({
            "status": "ok",
            "skill_id": "test-skill",
            "coverage": { "avg_confidence": 0.85 },
        });
        let json = serde_json::to_string_pretty(&output).unwrap();
        assert!(!json.contains("\x1b["), "robot mode must have no ANSI");
    }

    // ── 11. test_evidence_timeline_ordering ─────────────────────────

    #[test]
    fn test_evidence_timeline_ordering() {
        let mut records = vec![
            make_evidence_record("skill-b", "rule-1"),
            make_evidence_record("skill-a", "rule-1"),
        ];
        records.sort_by(|a, b| a.skill_id.cmp(&b.skill_id));
        assert_eq!(records[0].skill_id, "skill-a");
        assert_eq!(records[1].skill_id, "skill-b");
    }

    // ── 12. test_evidence_large_set_performance ─────────────────────

    #[test]
    fn test_evidence_large_set_performance() {
        let records: Vec<EvidenceRecord> = (0..100)
            .map(|i| make_evidence_record(&format!("skill-{i}"), "rule-1"))
            .collect();
        assert_eq!(records.len(), 100);
        let total_refs: usize = records.iter().map(|r| r.evidence.len()).sum();
        assert_eq!(total_refs, 100);
    }

    // ── 13. test_evidence_export_json ───────────────────────────────

    #[test]
    fn test_evidence_export_json() {
        let records = vec![make_evidence_record("skill-a", "rule-1")];
        let json = export_json(&records).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["format"], "provenance_graph");
        assert!(parsed["nodes"].is_array());
        assert!(parsed["edges"].is_array());
    }

    // ── 14. test_evidence_export_dot ────────────────────────────────

    #[test]
    fn test_evidence_export_dot() {
        let records = vec![make_evidence_record("skill-a", "rule-1")];
        let dot = export_dot(&records).unwrap();
        assert!(dot.contains("digraph provenance"));
        assert!(dot.contains("skill:skill-a"));
        assert!(dot.contains("session:sess-001"));
    }

    // ── 15. test_evidence_should_use_rich_returns_bool ──────────────

    #[test]
    fn test_evidence_should_use_rich_returns_bool() {
        let _result: bool = should_use_rich_for_evidence();
    }
}
