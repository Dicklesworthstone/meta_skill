//! ms test - Run skill tests

use clap::Args;

use crate::app::AppContext;
use crate::cli::output::{HumanLayout, emit_json};
use crate::error::Result;
use crate::testing::{SkillTestRunner, TestOptions, TestStatus};

#[derive(Args, Debug)]
pub struct TestArgs {
    /// Skill to test (or all if not specified)
    pub skill: Option<String>,

    /// Run tests for all skills
    #[arg(long)]
    pub all: bool,

    /// Run a specific test by name
    #[arg(long)]
    pub test: Option<String>,

    /// Only run tests with these tags (comma-separated)
    #[arg(long)]
    pub tags: Option<String>,

    /// Skip tests with these tags (comma-separated)
    #[arg(long)]
    pub exclude_tags: Option<String>,

    /// Override default timeout (e.g., 30s, 2m)
    #[arg(long)]
    pub timeout: Option<String>,

    /// Stop on first failure
    #[arg(long)]
    pub fail_fast: bool,
}

pub fn run(ctx: &AppContext, args: &TestArgs) -> Result<()> {
    let options = TestOptions {
        verbose: ctx.verbosity > 0,
        fail_fast: args.fail_fast,
        test_name: args.test.clone(),
        include_tags: parse_tags(args.tags.as_deref()),
        exclude_tags: parse_tags(args.exclude_tags.as_deref()),
        timeout_override: args.timeout.as_deref().and_then(parse_duration),
    };

    let runner = SkillTestRunner::new(ctx, options);

    let reports = if args.all || args.skill.is_none() {
        runner.run_all()?
    } else {
        vec![runner.run_for_skill(args.skill.as_ref().unwrap())?]
    };

    if ctx.robot_mode {
        let status = if reports.iter().any(|r| !r.success()) {
            "partial"
        } else {
            "ok"
        };
        let payload = serde_json::json!({
            "status": status,
            "count": reports.len(),
            "reports": reports,
        });
        emit_json(&payload)
    } else {
        render_human(&reports);
        Ok(())
    }
}

fn render_human(reports: &[crate::testing::SkillTestReport]) {
    let mut layout = HumanLayout::new();
    layout.title("Skill Tests");

    for report in reports {
        let status = if report.failed == 0 { "PASS" } else { "FAIL" };
        layout
            .section(&format!("{} ({})", report.skill_id, status))
            .kv("Tests", &report.tests_run.to_string())
            .kv("Passed", &report.passed.to_string())
            .kv("Failed", &report.failed.to_string())
            .kv("Skipped", &report.skipped.to_string())
            .kv("Duration", &format!("{}ms", report.duration_ms))
            .blank();

        for result in &report.results {
            let line = match result.status {
                TestStatus::Passed => format!("[PASS] {}", result.name),
                TestStatus::Failed => format!("[FAIL] {}", result.name),
                TestStatus::Skipped => format!("[SKIP] {}", result.name),
                TestStatus::Timeout => format!("[TIME] {}", result.name),
            };
            layout.push_line(line);
            if !result.failures.is_empty() {
                for failure in &result.failures {
                    layout.push_line(format!("  - {}", failure));
                }
            }
        }
        layout.blank();
    }

    crate::cli::output::emit_human(layout);
}

fn parse_tags(raw: Option<&str>) -> Vec<String> {
    raw.unwrap_or("")
        .split(',')
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
        .collect()
}

fn parse_duration(raw: &str) -> Option<std::time::Duration> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (value, suffix) = trimmed
        .chars()
        .partition::<String, _>(|c| c.is_ascii_digit());
    let value: u64 = value.parse().ok()?;
    match suffix.as_str() {
        "ms" => Some(std::time::Duration::from_millis(value)),
        "s" | "" => Some(std::time::Duration::from_secs(value)),
        "m" => Some(std::time::Duration::from_secs(value * 60)),
        _ => None,
    }
}
