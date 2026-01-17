//! ms validate - Validate skill specs (optionally with UBS)

use clap::Args;
use serde::Serialize;

use crate::app::AppContext;
use crate::cli::output::OutputFormat;
use crate::cli::commands::resolve_skill_markdown;
use crate::cli::output::{HumanLayout, emit_human, emit_json};
use crate::core::spec_lens::parse_markdown;
use crate::core::validation::{ValidationWarning, validate, validate_with_ubs};
use crate::error::{MsError, Result};
use crate::quality::ubs::{UbsClient, UbsFinding, UbsResult, UbsSeverity};
use crate::security::SafetyGate;

#[derive(Args, Debug)]
pub struct ValidateArgs {
    /// Skill ID or path to SKILL.md
    pub skill: String,

    /// Run UBS on code blocks
    #[arg(long)]
    pub ubs: bool,
}

pub fn run(ctx: &AppContext, args: &ValidateArgs) -> Result<()> {
    let skill_md = resolve_skill_markdown(ctx, &args.skill)?;
    let raw = std::fs::read_to_string(&skill_md).map_err(|err| {
        crate::error::MsError::Config(format!("read {}: {err}", skill_md.display()))
    })?;
    let spec = parse_markdown(&raw)?;

    let warnings = validate(&spec)?;
    let ubs_result = if args.ubs {
        let gate = SafetyGate::from_context(ctx);
        let client = UbsClient::new(None).with_safety(gate);
        Some(validate_with_ubs(&spec, &client)?)
    } else {
        None
    };

    if ctx.output_format != OutputFormat::Human {
        let report = build_report(&args.skill, &skill_md, &warnings, ubs_result.as_ref());
        return emit_json(&report);
    }

    let mut layout = HumanLayout::new();
    layout.title("Validation");
    layout.kv("Skill", &args.skill);
    layout.kv("Path", &skill_md.display().to_string());

    if !warnings.is_empty() {
        layout.section("Warnings");
        for warning in &warnings {
            layout.bullet(&format!("{}: {}", warning.field, warning.message));
        }
    }

    if let Some(result) = &ubs_result {
        layout.section("UBS");
        layout.kv("Findings", &result.findings.len().to_string());
        layout.kv("Exit code", &result.exit_code.to_string());
        for finding in &result.findings {
            layout.bullet(&format!(
                "{}:{}:{} {} ({})",
                finding.file.display(),
                finding.line,
                finding.column,
                finding.message,
                severity_label(finding.severity)
            ));
            if let Some(fix) = &finding.suggested_fix {
                layout.bullet(&format!("fix: {fix}"));
            }
        }
    }

    if warnings.is_empty() && ubs_result.as_ref().is_none_or(crate::quality::ubs::UbsResult::is_clean) {
        layout.section("Status");
        layout.bullet("OK");
    }

    emit_human(layout);

    if let Some(result) = ubs_result {
        if !result.is_clean() {
            return Err(MsError::ValidationFailed(
                "UBS findings detected".to_string(),
            ));
        }
    }

    Ok(())
}

#[derive(Serialize)]
struct ValidateReport {
    skill: String,
    path: String,
    warnings: Vec<WarningOutput>,
    ubs: Option<UbsReport>,
    clean: bool,
}

#[derive(Serialize)]
struct WarningOutput {
    field: String,
    message: String,
}

#[derive(Serialize)]
struct UbsReport {
    exit_code: i32,
    findings: usize,
    clean: bool,
    stdout: String,
    stderr: String,
    items: Vec<UbsFindingOutput>,
}

#[derive(Serialize)]
struct UbsFindingOutput {
    category: String,
    severity: String,
    file: String,
    line: u32,
    column: u32,
    message: String,
    suggested_fix: Option<String>,
}

fn build_report(
    skill: &str,
    skill_md: &std::path::Path,
    warnings: &[ValidationWarning],
    ubs_result: Option<&UbsResult>,
) -> ValidateReport {
    let warning_items = warnings
        .iter()
        .map(|warning| WarningOutput {
            field: warning.field.clone(),
            message: warning.message.clone(),
        })
        .collect::<Vec<_>>();
    let ubs = ubs_result.map(|result| UbsReport {
        exit_code: result.exit_code,
        findings: result.findings.len(),
        clean: result.is_clean(),
        stdout: result.stdout.clone(),
        stderr: result.stderr.clone(),
        items: result.findings.iter().map(map_finding).collect::<Vec<_>>(),
    });
    let clean = warning_items.is_empty() && ubs_result.is_none_or(crate::quality::ubs::UbsResult::is_clean);

    ValidateReport {
        skill: skill.to_string(),
        path: skill_md.display().to_string(),
        warnings: warning_items,
        ubs,
        clean,
    }
}

fn map_finding(finding: &UbsFinding) -> UbsFindingOutput {
    UbsFindingOutput {
        category: finding.category.clone(),
        severity: severity_label(finding.severity).to_string(),
        file: finding.file.display().to_string(),
        line: finding.line,
        column: finding.column,
        message: finding.message.clone(),
        suggested_fix: finding.suggested_fix.clone(),
    }
}

const fn severity_label(severity: UbsSeverity) -> &'static str {
    match severity {
        UbsSeverity::Critical => "critical",
        UbsSeverity::Important => "important",
        UbsSeverity::Contextual => "contextual",
    }
}
