//! ms quality - Compute skill quality scores

use clap::Args;
use serde::Serialize;

use crate::app::AppContext;
use crate::cli::output::{HumanLayout, emit_json};
use crate::core::spec_lens::parse_markdown;
use crate::error::{MsError, Result};
use crate::quality::{QualityContext, QualityScorer};

#[derive(Args, Debug)]
pub struct QualityArgs {
    /// Skill to score
    pub skill: Option<String>,

    /// Score all skills
    #[arg(long)]
    pub all: bool,

    /// Update stored `quality_score` in `SQLite`
    #[arg(long)]
    pub update: bool,
}

#[derive(Serialize)]
struct QualityOutput {
    skill_id: String,
    quality_score: f32,
    breakdown: QualityBreakdownOutput,
    issues: Vec<String>,
    suggestions: Vec<String>,
}

#[derive(Serialize)]
struct QualityBreakdownOutput {
    structure: f32,
    content: f32,
    evidence: f32,
    usage: f32,
    toolchain: f32,
    freshness: f32,
}

pub fn run(ctx: &AppContext, args: &QualityArgs) -> Result<()> {
    if !args.all && args.skill.is_none() {
        return Err(MsError::Config("missing skill (or use --all)".to_string()));
    }

    let scorer = QualityScorer::with_defaults();
    let mut outputs = Vec::new();

    let skill_files = if args.all {
        crate::cli::commands::discover_skill_markdowns(ctx)?
    } else {
        vec![crate::cli::commands::resolve_skill_markdown(
            ctx,
            args.skill.as_ref().unwrap(),
        )?]
    };

    for skill_md in skill_files {
        let raw = std::fs::read_to_string(&skill_md)?;
        let spec = parse_markdown(&raw)
            .map_err(|err| MsError::InvalidSkill(format!("{}: {err}", skill_md.display())))?;
        let skill_id = spec.metadata.id.clone();

        let (usage_count, evidence_count, modified_at) =
            if let Ok(Some(record)) = ctx.db.get_skill(&skill_id) {
                let usage = ctx.db.count_skill_usage(&skill_id).ok();
                let evidence = ctx.db.count_skill_evidence(&skill_id).ok();
                let modified = parse_modified_at(&record.modified_at);
                (usage, evidence, modified)
            } else {
                (None, None, None)
            };

        let context = QualityContext {
            usage_count,
            evidence_count,
            modified_at,
            toolchain_match: true,
        };

        let score = scorer.score_spec(&spec, &context);

        if args.update {
            ctx.db
                .update_skill_quality(&skill_id, f64::from(score.overall))?;
        }

        let output = QualityOutput {
            skill_id: skill_id.clone(),
            quality_score: score.overall,
            breakdown: QualityBreakdownOutput {
                structure: score.breakdown.structure,
                content: score.breakdown.content,
                evidence: score.breakdown.evidence,
                usage: score.breakdown.usage,
                toolchain: score.breakdown.toolchain,
                freshness: score.breakdown.freshness,
            },
            issues: score
                .issues
                .iter()
                .map(|issue| format!("{issue:?}"))
                .collect(),
            suggestions: score.suggestions.clone(),
        };
        outputs.push(output);
    }

    if ctx.robot_mode {
        let payload = serde_json::json!({
            "status": "ok",
            "count": outputs.len(),
            "results": outputs,
        });
        emit_json(&payload)
    } else {
        let mut layout = HumanLayout::new();
        layout.title("Skill Quality");
        for output in outputs {
            layout
                .section(&output.skill_id)
                .kv("Quality", &format!("{:.2}", output.quality_score))
                .kv("Structure", &format!("{:.2}", output.breakdown.structure))
                .kv("Content", &format!("{:.2}", output.breakdown.content))
                .kv("Evidence", &format!("{:.2}", output.breakdown.evidence))
                .kv("Usage", &format!("{:.2}", output.breakdown.usage))
                .kv("Toolchain", &format!("{:.2}", output.breakdown.toolchain))
                .kv("Freshness", &format!("{:.2}", output.breakdown.freshness))
                .blank();
            if !output.issues.is_empty() {
                layout.bullet("Issues:");
                for issue in &output.issues {
                    layout.push_line(format!("  - {issue}"));
                }
                layout.blank();
            }
            if !output.suggestions.is_empty() {
                layout.bullet("Suggestions:");
                for suggestion in &output.suggestions {
                    layout.push_line(format!("  - {suggestion}"));
                }
                layout.blank();
            }
        }
        crate::cli::output::emit_human(layout);
        Ok(())
    }
}

fn parse_modified_at(raw: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(raw) {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S") {
        return Some(chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
            dt,
            chrono::Utc,
        ));
    }
    None
}
