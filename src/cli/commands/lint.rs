//! ms lint - Lint skill specifications for issues
//!
//! Provides comprehensive linting of skill files with configurable rules,
//! multiple output formats, and auto-fix capabilities.

use std::path::PathBuf;

use clap::{Args, ValueEnum};
use console::style;
use serde::Serialize;

use crate::app::AppContext;
use crate::cli::output::OutputFormat;
use crate::cli::commands::{discover_skill_markdowns, resolve_skill_markdown};
use crate::cli::output::{emit_human, emit_json, HumanLayout};
use crate::core::spec_lens::parse_markdown;
use crate::error::{MsError, Result};
use crate::lint::diagnostic::{RuleCategory, Severity};
use crate::lint::rules::all_rules;
use crate::lint::{ValidationConfig, ValidationEngine, ValidationResult};

/// Output format for lint results
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum LintFormat {
    /// Human-readable output with colors
    #[default]
    Human,
    /// JSON format for programmatic consumption
    Json,
    /// SARIF format for GitHub integration
    Sarif,
    /// `JUnit` XML format for CI systems
    Junit,
}

#[derive(Args, Debug)]
pub struct LintArgs {
    /// Skill path, ID, or directory to lint
    #[arg(value_name = "PATH")]
    pub path: Option<String>,

    /// Lint all indexed skills
    #[arg(long, conflicts_with = "path")]
    pub all: bool,

    /// Only run specific rules (comma-separated IDs)
    #[arg(long, value_delimiter = ',')]
    pub rules: Option<Vec<String>>,

    /// Skip specific rules (comma-separated IDs)
    #[arg(long, value_delimiter = ',')]
    pub skip: Option<Vec<String>>,

    /// Output format
    #[arg(long, short, value_enum, default_value_t = LintFormat::Human)]
    pub format: LintFormat,

    /// Apply auto-fixes where available
    #[arg(long)]
    pub fix: bool,

    /// Treat warnings as errors
    #[arg(long)]
    pub strict: bool,

    /// Stop after N errors
    #[arg(long)]
    pub max_errors: Option<usize>,

    /// Show detailed rule documentation
    #[arg(long)]
    pub explain: Option<String>,

    /// List all available rules
    #[arg(long)]
    pub list_rules: bool,
}

pub fn run(ctx: &AppContext, args: &LintArgs) -> Result<()> {
    // Handle --explain
    if let Some(rule_id) = &args.explain {
        return explain_rule(ctx, rule_id);
    }

    // Handle --list-rules
    if args.list_rules {
        return list_rules(ctx);
    }

    // Collect paths to lint
    let paths = if args.all {
        discover_skill_markdowns(ctx)?
    } else if let Some(path) = &args.path {
        vec![resolve_skill_markdown(ctx, path)?]
    } else {
        // Default to current directory
        let current = std::env::current_dir()
            .map_err(|e| MsError::Config(format!("cannot get current dir: {e}")))?;
        let skill_md = current.join("SKILL.md");
        if skill_md.exists() {
            vec![skill_md]
        } else {
            return Err(MsError::Config(
                "No SKILL.md found. Specify a path or use --all".into(),
            ));
        }
    };

    if paths.is_empty() {
        return Err(MsError::Config("No skills found to lint".into()));
    }

    // Build validation config
    let mut config = ValidationConfig::new();
    if args.strict {
        config = config.strict();
    }
    if let Some(max) = args.max_errors {
        config = config.with_max_errors(max);
    }

    // Disable skipped rules
    if let Some(skip) = &args.skip {
        for rule_id in skip {
            config = config.disable_rule(rule_id);
        }
    }

    // Build engine with all rules
    let mut engine = ValidationEngine::new(config);

    // Register rules (filtered if --rules specified)
    let rules_filter: Option<std::collections::HashSet<&str>> = args
        .rules
        .as_ref()
        .map(|r| r.iter().map(std::string::String::as_str).collect());

    for rule in all_rules() {
        if let Some(ref filter) = rules_filter {
            if !filter.contains(rule.id()) {
                continue;
            }
        }
        engine.register(rule);
    }

    // Lint all paths
    let mut all_results = Vec::new();
    let mut total_errors = 0;
    let mut total_warnings = 0;
    let mut total_fixed = 0;

    for path in &paths {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| MsError::Config(format!("read {}: {e}", path.display())))?;

        let mut spec = parse_markdown(&raw)?;
        let result = engine.validate(&spec);

        total_errors += result.error_count();
        total_warnings += result.warning_count();

        // Apply fixes if requested
        let fixes_applied = if args.fix && !result.passed {
            let fix_result = engine.auto_fix(&mut spec)?;
            if fix_result.fixed_count() > 0 {
                // Write back the fixed spec
                let fixed_content = crate::core::spec_lens::compile_markdown(&spec);
                std::fs::write(path, &fixed_content)
                    .map_err(|e| MsError::Config(format!("write {}: {e}", path.display())))?;
                total_fixed += fix_result.fixed_count();
            }
            Some(fix_result.fixed_count())
        } else {
            None
        };

        all_results.push(LintFileResult {
            path: path.clone(),
            result,
            fixes_applied,
        });
    }

    // Output based on format
    match args.format {
        LintFormat::Human => output_human(ctx, &all_results, total_errors, total_warnings, total_fixed),
        LintFormat::Json => output_json(&all_results)?,
        LintFormat::Sarif => output_sarif(&all_results)?,
        LintFormat::Junit => output_junit(&all_results)?,
    }

    // Exit with appropriate code
    if total_errors > 0 {
        Err(MsError::ValidationFailed(format!(
            "{total_errors} error(s) found"
        )))
    } else {
        Ok(())
    }
}

struct LintFileResult {
    path: PathBuf,
    result: ValidationResult,
    fixes_applied: Option<usize>,
}

fn explain_rule(ctx: &AppContext, rule_id: &str) -> Result<()> {
    let rules = all_rules();
    let rule = rules
        .iter()
        .find(|r| r.id() == rule_id)
        .ok_or_else(|| MsError::NotFound(format!("Rule '{rule_id}' not found")))?;

    if ctx.output_format != OutputFormat::Human {
        let info = RuleInfo {
            id: rule.id().to_string(),
            name: rule.name().to_string(),
            description: rule.description().to_string(),
            category: format!("{}", rule.category()),
            default_severity: format!("{}", rule.default_severity()),
            can_fix: rule.can_fix(),
        };
        emit_json(&info)?;
    } else {
        let mut layout = HumanLayout::new();
        layout.title(&format!("Rule: {}", rule.id()));
        layout.kv("Name", rule.name());
        layout.kv("Category", &format!("{}", rule.category()));
        layout.kv("Severity", &format!("{}", rule.default_severity()));
        layout.kv("Auto-fix", if rule.can_fix() { "Yes" } else { "No" });
        layout.blank();
        layout.section("Description");
        layout.push_line(rule.description());
        emit_human(layout);
    }

    Ok(())
}

fn list_rules(ctx: &AppContext) -> Result<()> {
    let mut engine = ValidationEngine::with_defaults();
    for rule in all_rules() {
        engine.register(rule);
    }

    let rules = engine.list_rules();

    if ctx.output_format != OutputFormat::Human {
        let infos: Vec<RuleInfo> = rules
            .iter()
            .map(|r| RuleInfo {
                id: r.id.clone(),
                name: r.name.clone(),
                description: r.description.clone(),
                category: format!("{}", r.category),
                default_severity: format!("{}", r.default_severity),
                can_fix: r.can_fix,
            })
            .collect();
        emit_json(&infos)?;
    } else {
        let mut layout = HumanLayout::new();
        layout.title("Available Lint Rules");

        // Group by category
        let categories = [
            (RuleCategory::Structure, "Structure"),
            (RuleCategory::Reference, "Reference"),
            (RuleCategory::Security, "Security"),
            (RuleCategory::Quality, "Quality"),
            (RuleCategory::Performance, "Performance"),
        ];

        for (cat, cat_name) in categories {
            let cat_rules: Vec<_> = rules.iter().filter(|r| r.category == cat).collect();
            if cat_rules.is_empty() {
                continue;
            }

            layout.section(cat_name);
            for rule in cat_rules {
                let fix_badge = if rule.can_fix { " [fixable]" } else { "" };
                layout.bullet(&format!(
                    "{} - {} ({}){fix_badge}",
                    rule.id, rule.name, rule.default_severity
                ));
            }
            layout.blank();
        }

        emit_human(layout);
    }

    Ok(())
}

fn output_human(
    _ctx: &AppContext,
    results: &[LintFileResult],
    total_errors: usize,
    total_warnings: usize,
    total_fixed: usize,
) {
    let mut layout = HumanLayout::new();

    for file_result in results {
        let path_str = file_result.path.display().to_string();

        if file_result.result.diagnostics.is_empty() {
            layout.push_line(format!(
                "{} {}",
                style("✓").green(),
                style(&path_str).dim()
            ));
            continue;
        }

        layout.push_line(format!(
            "{} {}",
            style("✗").red(),
            style(&path_str).bold()
        ));

        for diag in &file_result.result.diagnostics {
            let severity_icon = match diag.severity {
                Severity::Error => style("error").red().bold(),
                Severity::Warning => style("warning").yellow(),
                Severity::Info => style("info").blue(),
            };

            let location = diag
                .span
                .as_ref()
                .map(|s| format!("{}:{}", s.start_line, s.start_col))
                .unwrap_or_default();

            layout.push_line(format!(
                "  {} {} {} {}",
                severity_icon,
                style(&diag.rule_id).dim(),
                diag.message,
                style(&location).dim()
            ));

            if let Some(suggestion) = &diag.suggestion {
                layout.push_line(format!(
                    "    {} {}",
                    style("hint:").cyan(),
                    suggestion
                ));
            }
        }

        if let Some(fixes) = file_result.fixes_applied {
            if fixes > 0 {
                layout.push_line(format!(
                    "  {} {} fix(es) applied",
                    style("✓").green(),
                    fixes
                ));
            }
        }

        layout.blank();
    }

    // Summary
    layout.section("Summary");
    layout.kv("Files", &results.len().to_string());
    layout.kv("Errors", &total_errors.to_string());
    layout.kv("Warnings", &total_warnings.to_string());
    if total_fixed > 0 {
        layout.kv("Fixed", &total_fixed.to_string());
    }

    emit_human(layout);
}

fn output_json(results: &[LintFileResult]) -> Result<()> {
    let report = JsonReport {
        files: results
            .iter()
            .map(|r| JsonFileReport {
                path: r.path.display().to_string(),
                passed: r.result.passed,
                error_count: r.result.error_count(),
                warning_count: r.result.warning_count(),
                diagnostics: r
                    .result
                    .diagnostics
                    .iter()
                    .map(|d| JsonDiagnostic {
                        rule_id: d.rule_id.clone(),
                        severity: format!("{}", d.severity),
                        message: d.message.clone(),
                        category: format!("{}", d.category),
                        span: d.span.as_ref().map(|s| JsonSpan {
                            start_line: s.start_line,
                            start_col: s.start_col,
                            end_line: s.end_line,
                            end_col: s.end_col,
                        }),
                        suggestion: d.suggestion.clone(),
                        fix_available: d.fix_available,
                    })
                    .collect(),
                fixes_applied: r.fixes_applied,
            })
            .collect(),
        summary: JsonSummary {
            total_files: results.len(),
            total_errors: results.iter().map(|r| r.result.error_count()).sum(),
            total_warnings: results.iter().map(|r| r.result.warning_count()).sum(),
            passed: results.iter().all(|r| r.result.passed),
        },
    };

    emit_json(&report)
}

fn output_sarif(results: &[LintFileResult]) -> Result<()> {
    let sarif = SarifReport {
        schema: "https://json.schemastore.org/sarif-2.1.0.json".to_string(),
        version: "2.1.0".to_string(),
        runs: vec![SarifRun {
            tool: SarifTool {
                driver: SarifDriver {
                    name: "ms lint".to_string(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    information_uri: "https://github.com/anthropics/ms".to_string(),
                    rules: all_rules()
                        .iter()
                        .map(|r| SarifRule {
                            id: r.id().to_string(),
                            name: r.name().to_string(),
                            short_description: SarifMessage {
                                text: r.description().to_string(),
                            },
                            default_configuration: SarifConfiguration {
                                level: match r.default_severity() {
                                    Severity::Error => "error".to_string(),
                                    Severity::Warning => "warning".to_string(),
                                    Severity::Info => "note".to_string(),
                                },
                            },
                        })
                        .collect(),
                },
            },
            results: results
                .iter()
                .flat_map(|r| {
                    r.result.diagnostics.iter().map(|d| SarifResult {
                        rule_id: d.rule_id.clone(),
                        level: match d.severity {
                            Severity::Error => "error".to_string(),
                            Severity::Warning => "warning".to_string(),
                            Severity::Info => "note".to_string(),
                        },
                        message: SarifMessage {
                            text: d.message.clone(),
                        },
                        locations: vec![SarifLocation {
                            physical_location: SarifPhysicalLocation {
                                artifact_location: SarifArtifactLocation {
                                    uri: r.path.display().to_string(),
                                },
                                region: d.span.as_ref().map(|s| SarifRegion {
                                    start_line: s.start_line,
                                    start_column: s.start_col,
                                    end_line: s.end_line,
                                    end_column: s.end_col,
                                }),
                            },
                        }],
                    })
                })
                .collect(),
        }],
    };

    emit_json(&sarif)
}

fn output_junit(results: &[LintFileResult]) -> Result<()> {
    let total_tests: usize = results.iter().map(|r| r.result.diagnostics.len().max(1)).sum();
    let total_failures: usize = results.iter().map(|r| r.result.error_count()).sum();

    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str(&format!(
        "<testsuites tests=\"{total_tests}\" failures=\"{total_failures}\">\n"
    ));

    for file_result in results {
        let path_str = file_result.path.display().to_string();
        let test_count = file_result.result.diagnostics.len().max(1);
        let failure_count = file_result.result.error_count();

        xml.push_str(&format!(
            "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{}\">\n",
            escape_xml(&path_str),
            test_count,
            failure_count
        ));

        if file_result.result.diagnostics.is_empty() {
            xml.push_str(&format!(
                "    <testcase name=\"{}\" classname=\"lint\"/>\n",
                escape_xml(&path_str)
            ));
        } else {
            for diag in &file_result.result.diagnostics {
                xml.push_str(&format!(
                    "    <testcase name=\"{}\" classname=\"{}\">\n",
                    escape_xml(&diag.rule_id),
                    escape_xml(&path_str)
                ));

                if diag.severity == Severity::Error {
                    xml.push_str(&format!(
                        "      <failure message=\"{}\" type=\"{}\">{}</failure>\n",
                        escape_xml(&diag.message),
                        escape_xml(&diag.rule_id),
                        escape_xml(&diag.message)
                    ));
                }

                xml.push_str("    </testcase>\n");
            }
        }

        xml.push_str("  </testsuite>\n");
    }

    xml.push_str("</testsuites>\n");
    println!("{xml}");

    Ok(())
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

// JSON output types

#[derive(Serialize)]
struct RuleInfo {
    id: String,
    name: String,
    description: String,
    category: String,
    default_severity: String,
    can_fix: bool,
}

#[derive(Serialize)]
struct JsonReport {
    files: Vec<JsonFileReport>,
    summary: JsonSummary,
}

#[derive(Serialize)]
struct JsonFileReport {
    path: String,
    passed: bool,
    error_count: usize,
    warning_count: usize,
    diagnostics: Vec<JsonDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fixes_applied: Option<usize>,
}

#[derive(Serialize)]
struct JsonDiagnostic {
    rule_id: String,
    severity: String,
    message: String,
    category: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    span: Option<JsonSpan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggestion: Option<String>,
    fix_available: bool,
}

#[derive(Serialize)]
struct JsonSpan {
    start_line: usize,
    start_col: usize,
    end_line: usize,
    end_col: usize,
}

#[derive(Serialize)]
struct JsonSummary {
    total_files: usize,
    total_errors: usize,
    total_warnings: usize,
    passed: bool,
}

// SARIF output types

#[derive(Serialize)]
struct SarifReport {
    #[serde(rename = "$schema")]
    schema: String,
    version: String,
    runs: Vec<SarifRun>,
}

#[derive(Serialize)]
struct SarifRun {
    tool: SarifTool,
    results: Vec<SarifResult>,
}

#[derive(Serialize)]
struct SarifTool {
    driver: SarifDriver,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifDriver {
    name: String,
    version: String,
    information_uri: String,
    rules: Vec<SarifRule>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifRule {
    id: String,
    name: String,
    short_description: SarifMessage,
    default_configuration: SarifConfiguration,
}

#[derive(Serialize)]
struct SarifConfiguration {
    level: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifResult {
    rule_id: String,
    level: String,
    message: SarifMessage,
    locations: Vec<SarifLocation>,
}

#[derive(Serialize)]
struct SarifMessage {
    text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifLocation {
    physical_location: SarifPhysicalLocation,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifPhysicalLocation {
    artifact_location: SarifArtifactLocation,
    #[serde(skip_serializing_if = "Option::is_none")]
    region: Option<SarifRegion>,
}

#[derive(Serialize)]
struct SarifArtifactLocation {
    uri: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SarifRegion {
    start_line: usize,
    start_column: usize,
    end_line: usize,
    end_column: usize,
}
