//! ms pre-commit - run UBS on staged files

use std::path::PathBuf;

use clap::Args;

use crate::app::AppContext;
use crate::cli::output;
use crate::error::{MsError, Result};
use crate::quality::ubs::UbsClient;

#[derive(Args, Debug)]
pub struct PreCommitArgs {
    /// Git repository root (default: current directory)
    #[arg(long)]
    pub repo: Option<PathBuf>,

    /// Limit UBS to a specific language (e.g. "go", "rust")
    #[arg(long)]
    pub only: Option<String>,
}

pub fn run(ctx: &AppContext, args: &PreCommitArgs) -> Result<()> {
    let repo = args
        .repo
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let client = UbsClient::new(None);
    let result = if let Some(lang) = args.only.as_deref() {
        client.check_dir(&repo, Some(lang))?
    } else {
        client.check_staged(&repo)?
    };

    if ctx.robot_mode {
        let report = PreCommitReport {
            exit_code: result.exit_code,
            findings: result.findings.len(),
            clean: result.is_clean(),
            stdout: result.stdout,
            stderr: result.stderr,
        };
        return output::emit_json(&report);
    }

    if result.is_clean() {
        println!("UBS: no findings.");
        return Ok(());
    }

    println!("UBS: {} finding(s).", result.findings.len());
    for finding in result.findings.iter() {
        println!(
            "- {}:{}:{} {}",
            finding.file.display(),
            finding.line,
            finding.column,
            finding.message
        );
        if let Some(fix) = &finding.suggested_fix {
            println!("  {}", fix);
        }
    }

    Err(MsError::ValidationFailed(
        "UBS findings detected".to_string(),
    ))
}

#[derive(serde::Serialize)]
struct PreCommitReport {
    exit_code: i32,
    findings: usize,
    clean: bool,
    stdout: String,
    stderr: String,
}
