//! ms validate - Validate skill specs (optionally with UBS)

use clap::Args;
use serde::Serialize;

use crate::app::AppContext;
use crate::cli::commands::resolve_skill_markdown;
use crate::cli::output::OutputFormat;
use crate::cli::output::{HumanLayout, emit_human, emit_json};
use crate::core::spec_lens::{SpecLens, compile_markdown, parse_markdown};
use crate::core::validation::{ValidationWarning, validate, validate_with_ubs};
use crate::error::{MsError, Result};
use crate::lint::rules::all_rules;
use crate::lint::{RuleCategory, ValidationEngine, ValidationResult};
use crate::quality::ubs::{UbsClient, UbsFinding, UbsResult, UbsSeverity};
use crate::security::SafetyGate;

#[derive(Args, Debug)]
pub struct ValidateArgs {
    /// Skill ID or path to SKILL.md
    pub skill: Option<String>,

    /// Validate all discovered skills
    #[arg(long)]
    pub all: bool,

    /// Run UBS on code blocks
    #[arg(long)]
    pub ubs: bool,

    /// Run 19-rule lint engine across structure, reference, quality, security, and performance
    #[arg(long)]
    pub lint: bool,

    /// Automatically apply available lint fixes
    #[arg(long)]
    pub fix: bool,
}

fn run_lint_engine(
    spec: &mut crate::core::SkillSpec,
    skill_md: &std::path::Path,
    apply_fixes: bool,
) -> Result<(ValidationResult, Option<(usize, bool)>)> {
    let mut engine = ValidationEngine::with_defaults();
    for rule in all_rules() {
        engine.register(rule);
    }
    let mut res = engine.validate(spec);
    let mut fix_info = None;
    if apply_fixes && (!res.passed || res.warning_count() > 0) {
        let fix_res = engine.auto_fix(spec)?;
        let fixed_count = fix_res.fixed_count();
        let all_ok = fix_res.all_succeeded();
        if fixed_count > 0 {
            let updated_md = compile_markdown(spec);
            let _ = std::fs::write(skill_md, updated_md);
            res = engine.validate(spec);
        }
        fix_info = Some((fixed_count, all_ok));
    }
    Ok((res, fix_info))
}

pub fn run(ctx: &AppContext, args: &ValidateArgs) -> Result<()> {
    if !args.all && args.skill.is_none() {
        return Err(MsError::Config("missing skill (or use --all)".to_string()));
    }

    if args.all {
        let skill_files = crate::cli::commands::discover_skill_markdowns(ctx)?;
        let mut reports = Vec::new();
        let mut ubs_failed = false;

        for skill_md in skill_files {
            let raw = std::fs::read_to_string(&skill_md).map_err(|err| {
                crate::error::MsError::Config(format!("read {}: {err}", skill_md.display()))
            })?;
            let mut spec = parse_markdown(&raw)?;
            let roundtrip_ok = SpecLens.verify_roundtrip(&spec).is_ok();
            let mut warnings = validate(&spec)?;
            if !roundtrip_ok {
                warnings.push(ValidationWarning {
                    field: "roundtrip".to_string(),
                    message: "Markdown compilation roundtrip did not preserve spec semantics"
                        .to_string(),
                });
            }
            let (lint_res, fix_info) = run_lint_engine(&mut spec, &skill_md, args.fix)?;
            let ubs_result = if args.ubs {
                let gate = SafetyGate::from_context(ctx);
                let client = UbsClient::new(None).with_safety(gate);
                let res = validate_with_ubs(&spec, &client)?;
                if !res.is_clean() {
                    ubs_failed = true;
                }
                Some(res)
            } else {
                None
            };
            reports.push(build_report(
                &spec.metadata.id,
                &skill_md,
                &warnings,
                ubs_result.as_ref(),
                roundtrip_ok,
                &lint_res,
                fix_info,
                args.lint,
            ));
        }

        if ctx.output_format != OutputFormat::Human {
            let clean = reports.iter().all(|r| r.clean);
            return emit_json(&serde_json::json!({
                "status": if clean { "ok" } else { "warnings" },
                "count": reports.len(),
                "clean": clean,
                "results": reports,
            }));
        }

        let mut layout = HumanLayout::new();
        layout.title("Validation (All Skills)");
        layout.kv("Total Skills", &reports.len().to_string());
        layout.kv(
            "Clean",
            &reports.iter().filter(|r| r.clean).count().to_string(),
        );
        for r in &reports {
            layout.section(&r.skill);
            layout.kv("Path", &r.path);
            layout.kv("Clean", if r.clean { "yes" } else { "no" });
            layout.kv("Roundtrip OK", if r.roundtrip_ok { "yes" } else { "no" });
            for w in &r.warnings {
                layout.bullet(&format!("{}: {}", w.field, w.message));
            }
        }
        emit_human(layout);
        if ubs_failed {
            return Err(MsError::ValidationFailed(
                "UBS findings detected".to_string(),
            ));
        }
        return Ok(());
    }

    let skill_arg = args.skill.as_deref().unwrap_or_default();
    let skill_md = resolve_skill_markdown(ctx, skill_arg)?;
    let raw = std::fs::read_to_string(&skill_md).map_err(|err| {
        crate::error::MsError::Config(format!("read {}: {err}", skill_md.display()))
    })?;
    let mut spec = parse_markdown(&raw)?;
    let roundtrip_ok = SpecLens.verify_roundtrip(&spec).is_ok();

    let mut warnings = validate(&spec)?;
    if !roundtrip_ok {
        warnings.push(ValidationWarning {
            field: "roundtrip".to_string(),
            message: "Markdown compilation roundtrip did not preserve spec semantics".to_string(),
        });
    }
    let (lint_res, fix_info) = run_lint_engine(&mut spec, &skill_md, args.fix)?;
    let ubs_result = if args.ubs {
        let gate = SafetyGate::from_context(ctx);
        let client = UbsClient::new(None).with_safety(gate);
        Some(validate_with_ubs(&spec, &client)?)
    } else {
        None
    };

    if ctx.output_format != OutputFormat::Human {
        let report = build_report(
            skill_arg,
            &skill_md,
            &warnings,
            ubs_result.as_ref(),
            roundtrip_ok,
            &lint_res,
            fix_info,
            args.lint,
        );
        return emit_json(&report);
    }

    let mut layout = HumanLayout::new();
    layout.title("Validation");
    layout.kv("Skill", skill_arg);
    layout.kv("Path", &skill_md.display().to_string());
    layout.kv("Roundtrip OK", if roundtrip_ok { "yes" } else { "no" });
    if args.lint || args.fix {
        layout.kv("Lint Errors", &lint_res.error_count().to_string());
        layout.kv("Lint Warnings", &lint_res.warning_count().to_string());
        if let Some((fixed, all_ok)) = fix_info {
            layout.kv("Fixes Applied", &fixed.to_string());
            layout.kv("All Fixes Succeeded", if all_ok { "yes" } else { "no" });
        }
    }

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

    if warnings.is_empty()
        && ubs_result
            .as_ref()
            .is_none_or(crate::quality::ubs::UbsResult::is_clean)
    {
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
    roundtrip_ok: bool,
    clean: bool,
    lint_errors: usize,
    lint_warnings: usize,
    lint_infos: usize,
    lint_by_category: std::collections::BTreeMap<String, usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fixes_applied: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    all_fixes_succeeded: Option<bool>,
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
    roundtrip_ok: bool,
    lint_res: &ValidationResult,
    fix_info: Option<(usize, bool)>,
    strict_lint: bool,
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
    let clean = roundtrip_ok
        && warning_items.is_empty()
        && ubs_result.is_none_or(crate::quality::ubs::UbsResult::is_clean)
        && (!strict_lint || lint_res.passed);

    let mut lint_by_category = std::collections::BTreeMap::new();
    for cat in [
        RuleCategory::Structure,
        RuleCategory::Reference,
        RuleCategory::Quality,
        RuleCategory::Security,
        RuleCategory::Performance,
    ] {
        let count = lint_res.by_category(cat).count();
        if count > 0 {
            lint_by_category.insert(cat.to_string(), count);
        }
    }

    ValidateReport {
        skill: skill.to_string(),
        path: skill_md.display().to_string(),
        warnings: warning_items,
        ubs,
        roundtrip_ok,
        clean,
        lint_errors: lint_res.error_count(),
        lint_warnings: lint_res.warning_count(),
        lint_infos: lint_res.infos().count(),
        lint_by_category,
        fixes_applied: fix_info.map(|(f, _)| f),
        all_fixes_succeeded: fix_info.map(|(_, ok)| ok),
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
