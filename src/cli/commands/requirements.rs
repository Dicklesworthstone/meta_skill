//! ms requirements - Check environment requirements
//!
//! Checks for the presence of external tools and dependencies
//! that ms interacts with (git, cass, bd, etc.).

use std::path::PathBuf;
use std::process::Command;

use clap::Args;
use colored::Colorize;
use which::which;

use crate::app::AppContext;
use crate::cli::output::OutputFormat;
use crate::error::Result;

#[derive(Args, Debug)]
pub struct RequirementsArgs {
    /// Skill to check requirements for (optional, checks specific skill deps)
    pub skill: Option<String>,

    /// Output format: text, json
    #[arg(long, default_value = "text")]
    pub format: String,

    /// Check all indexed skills
    #[arg(long)]
    pub all: bool,
}

#[derive(Debug, serde::Serialize)]
struct RequirementCheck {
    name: String,
    bin: String,
    path: Option<PathBuf>,
    version: Option<String>,
    required: bool,
    present: bool,
    error: Option<String>,
}

impl RequirementCheck {
    fn new(name: &str, bin: &str, required: bool) -> Self {
        Self {
            name: name.to_string(),
            bin: bin.to_string(),
            path: None,
            version: None,
            required,
            present: false,
            error: None,
        }
    }

    fn check(&mut self) {
        match which(&self.bin) {
            Ok(path) => {
                self.path = Some(path);
                self.present = true;

                // Try to get version
                if let Ok(output) = Command::new(&self.bin).arg("--version").output() {
                    if output.status.success() {
                        let v = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        // Clean up version string (e.g. "git version 2.34.1" -> "2.34.1")
                        let clean_v = if v.to_lowercase().starts_with(&self.bin)
                            || v.to_lowercase().starts_with("git version")
                        {
                            v.split_whitespace().last().unwrap_or(&v).to_string()
                        } else {
                            v
                        };
                        self.version = Some(clean_v);
                    }
                }
            }
            Err(e) => {
                self.present = false;
                self.error = Some(e.to_string());
            }
        }
    }
}

pub fn run(ctx: &AppContext, args: &RequirementsArgs) -> Result<()> {
    if let Some(skill_ref) = &args.skill {
        return run_skill_requirements(ctx, args, skill_ref);
    }

    if args.all {
        return run_all_skill_requirements(ctx, args);
    }

    let mut checks = vec![
        RequirementCheck::new("Git Version Control", "git", true),
        RequirementCheck::new("CASS (Context Aware Semantic Search)", "cass", false),
        RequirementCheck::new("Beads Issue Tracker", "bd", false),
        RequirementCheck::new("Ripgrep", "rg", false),
        RequirementCheck::new("Tar Archiver", "tar", true),
    ];

    for check in &mut checks {
        check.check();
    }

    if ctx.output_format != OutputFormat::Human || args.format == "json" {
        return crate::cli::output::emit_json(&checks);
    }

    println!("{}", "System Requirements Check".bold());
    println!("{}", "─".repeat(60));
    println!();

    let mut all_passed = true;

    for check in &checks {
        let status = if check.present {
            "✓".green()
        } else if check.required {
            all_passed = false;
            "✗".red()
        } else {
            "-".yellow()
        };

        let path_str = check.path.as_ref().map_or_else(
            || "Not found".dimmed().to_string(),
            |p| p.to_string_lossy().to_string(),
        );

        let version_str = check
            .version
            .as_ref()
            .map(|v| format!("(v{v})"))
            .unwrap_or_default();

        println!("{} {} {}", status, check.name.bold(), version_str);
        println!("    Bin:  {}", check.bin.cyan());
        println!("    Path: {path_str}");
        if !check.present && !check.required {
            println!("    Note: Optional dependency");
        }
        println!();
    }

    if all_passed {
        println!("\n{} All required dependencies found.", "Success:".green());
    } else {
        println!("\n{} Missing required dependencies.", "Error:".red());
    }

    Ok(())
}

fn collect_available_skills_and_capabilities(
    ctx: &AppContext,
) -> Result<(
    Vec<crate::storage::sqlite::SkillRecord>,
    std::collections::HashSet<String>,
)> {
    let mut records = Vec::new();
    let mut available = std::collections::HashSet::new();
    let mut offset = 0usize;
    let limit = 200usize;

    loop {
        let batch = ctx.db.list_skills(limit, offset)?;
        if batch.is_empty() {
            break;
        }
        offset += batch.len();
        for record in batch {
            available.insert(record.id.clone());
            if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&record.metadata_json) {
                if let Some(provides) = meta.get("provides").and_then(|v| v.as_array()) {
                    for cap in provides.iter().filter_map(|v| v.as_str()) {
                        available.insert(cap.to_string());
                    }
                }
            }
            records.push(record);
        }
    }

    Ok((records, available))
}

fn build_spec_from_record(
    record: &crate::storage::sqlite::SkillRecord,
) -> Result<crate::core::skill::SkillSpec> {
    if let Some(err) = crate::core::spec_lens::frontmatter_error(&record.body) {
        return Err(crate::error::MsError::ValidationFailed(format!(
            "failed to parse skill '{}' frontmatter: {err}",
            record.id
        )));
    }
    let mut spec = crate::core::spec_lens::parse_markdown(&record.body).map_err(|e| {
        crate::error::MsError::ValidationFailed(format!(
            "failed to parse skill '{}': {e}",
            record.id
        ))
    })?;
    if spec.metadata.id.is_empty() {
        spec.metadata.id.clone_from(&record.id);
    }
    if spec.metadata.name.is_empty() {
        spec.metadata.name.clone_from(&record.name);
    }
    if !record.metadata_json.trim().is_empty() {
        let meta: serde_json::Value = serde_json::from_str(&record.metadata_json).map_err(|e| {
            crate::error::MsError::ValidationFailed(format!(
                "failed to parse skill '{}' metadata_json: {e}",
                record.id
            ))
        })?;
        if spec.metadata.requires.is_empty() {
            if let Some(reqs) = meta.get("requires").and_then(|v| v.as_array()) {
                spec.metadata.requires = reqs
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();
            }
        }
        if spec.metadata.provides.is_empty() {
            if let Some(provs) = meta.get("provides").and_then(|v| v.as_array()) {
                spec.metadata.provides = provs
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();
            }
        }
    }
    Ok(spec)
}

fn run_skill_requirements(
    ctx: &AppContext,
    args: &RequirementsArgs,
    skill_ref: &str,
) -> Result<()> {
    let record = if let Some(s) = ctx.db.get_skill(skill_ref)? {
        s
    } else if let Some(alias) = ctx.db.resolve_alias(skill_ref)? {
        ctx.db.get_skill(&alias.canonical_id)?.ok_or_else(|| {
            crate::error::MsError::SkillNotFound(format!("skill not found: {skill_ref}"))
        })?
    } else {
        return Err(crate::error::MsError::SkillNotFound(format!(
            "skill not found: {skill_ref}"
        )));
    };

    let (_, available) = collect_available_skills_and_capabilities(ctx)?;
    let spec = build_spec_from_record(&record)?;
    let report = crate::core::requirements::check_skill_requirements(&spec, &available);

    if ctx.output_format != OutputFormat::Human || args.format == "json" {
        return crate::cli::output::emit_json(&report);
    }

    print_skill_report(&report);
    Ok(())
}

fn run_all_skill_requirements(ctx: &AppContext, args: &RequirementsArgs) -> Result<()> {
    let (records, available) = collect_available_skills_and_capabilities(ctx)?;
    if records.is_empty() {
        return Err(crate::error::MsError::ValidationFailed(
            "no indexed skills found to check requirements".to_string(),
        ));
    }

    let mut reports = Vec::with_capacity(records.len());
    let mut parse_errors = Vec::new();

    for record in &records {
        match build_spec_from_record(record) {
            Ok(spec) => {
                reports.push(crate::core::requirements::check_skill_requirements(
                    &spec, &available,
                ));
            }
            Err(err) => {
                let err_msg = err.to_string();
                parse_errors.push(serde_json::json!({
                    "skill_id": record.id,
                    "error": err_msg,
                }));
                reports.push(crate::core::requirements::SkillRequirementReport {
                    skill_id: record.id.clone(),
                    skill_name: record.name.clone(),
                    satisfied: false,
                    checks: vec![crate::core::requirements::RequirementCheck {
                        name: "skill-markdown".to_string(),
                        category: "parse".to_string(),
                        required: true,
                        satisfied: false,
                        message: Some(err_msg),
                    }],
                });
            }
        }
    }

    let all_satisfied = parse_errors.is_empty() && reports.iter().all(|r| r.satisfied);

    if ctx.output_format != OutputFormat::Human || args.format == "json" {
        let status = if !parse_errors.is_empty() {
            "error"
        } else if all_satisfied {
            "ok"
        } else {
            "unsatisfied"
        };
        crate::cli::output::emit_json(&serde_json::json!({
            "status": status,
            "total_skills": reports.len(),
            "satisfied_skills": reports.iter().filter(|r| r.satisfied).count(),
            "parse_error_count": parse_errors.len(),
            "parse_errors": parse_errors,
            "skills": reports,
        }))?;
        if !parse_errors.is_empty() {
            return Err(crate::error::MsError::ReportedValidationFailed(format!(
                "failed to parse {} indexed skill(s) during requirements check",
                parse_errors.len()
            )));
        }
        return Ok(());
    }

    println!("{}", "All Indexed Skills Requirements Check".bold());
    println!("{}", "─".repeat(60));
    println!();
    for report in &reports {
        print_skill_report(report);
        println!();
    }
    if !parse_errors.is_empty() {
        return Err(crate::error::MsError::ValidationFailed(format!(
            "failed to parse {} indexed skill(s) during requirements check",
            parse_errors.len()
        )));
    }
    Ok(())
}

fn print_skill_report(report: &crate::core::requirements::SkillRequirementReport) {
    let status = if report.satisfied {
        "✓".green()
    } else {
        "✗".red()
    };
    println!(
        "{} {} ({})",
        status,
        report.skill_name.bold(),
        report.skill_id.cyan()
    );
    if report.checks.is_empty() {
        println!("    No explicit requirements declared.");
        return;
    }
    for check in &report.checks {
        let icon = if check.satisfied {
            "✓".green()
        } else if check.required {
            "✗".red()
        } else {
            "-".yellow()
        };
        let msg = check.message.as_deref().unwrap_or("");
        println!("    {icon} [{}] {}: {msg}", check.category, check.name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Parser, Subcommand};

    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: TestCommand,
    }

    #[derive(Subcommand)]
    enum TestCommand {
        Requirements(RequirementsArgs),
    }

    #[test]
    fn parse_requirements_defaults() {
        let parsed = TestCli::parse_from(["test", "requirements"]);
        let TestCommand::Requirements(args) = parsed.cmd;
        assert!(args.skill.is_none());
        assert_eq!(args.format, "text");
        assert!(!args.all);
    }
}
