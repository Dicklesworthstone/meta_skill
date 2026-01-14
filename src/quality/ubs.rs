//! UBS (Ultimate Bug Scanner) integration.

use std::path::{Path, PathBuf};
use std::process::Command;

use regex::Regex;

use crate::error::{MsError, Result};

#[derive(Debug, Clone)]
pub struct UbsClient {
    ubs_path: PathBuf,
}

impl UbsClient {
    pub fn new(ubs_path: Option<PathBuf>) -> Self {
        Self {
            ubs_path: ubs_path.unwrap_or_else(|| PathBuf::from("ubs")),
        }
    }

    pub fn check_files(&self, files: &[PathBuf]) -> Result<UbsResult> {
        if files.is_empty() {
            return Ok(UbsResult::empty());
        }

        let mut cmd = Command::new(&self.ubs_path);
        for file in files {
            cmd.arg(file);
        }
        run_ubs(cmd)
    }

    pub fn check_dir(&self, dir: &Path, only: Option<&str>) -> Result<UbsResult> {
        let mut cmd = Command::new(&self.ubs_path);
        if let Some(lang) = only {
            cmd.arg(format!("--only={lang}"));
        }
        cmd.arg(dir);
        run_ubs(cmd)
    }

    pub fn check_staged(&self, repo_root: &Path) -> Result<UbsResult> {
        let output = Command::new("git")
            .arg("diff")
            .arg("--name-only")
            .arg("--cached")
            .current_dir(repo_root)
            .output()
            .map_err(|err| MsError::Config(format!("git diff: {err}")))?;

        if !output.status.success() {
            return Err(MsError::Config("git diff failed".to_string()));
        }

        let files = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(PathBuf::from)
            .collect::<Vec<_>>();

        if files.is_empty() {
            return Ok(UbsResult::empty());
        }

        self.check_files(&files)
    }
}

#[derive(Debug, Clone)]
pub struct UbsResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub findings: Vec<UbsFinding>,
}

impl UbsResult {
    fn empty() -> Self {
        Self {
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
            findings: Vec::new(),
        }
    }

    pub fn is_clean(&self) -> bool {
        self.exit_code == 0 && self.findings.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct UbsFinding {
    pub category: String,
    pub severity: UbsSeverity,
    pub file: PathBuf,
    pub line: u32,
    pub column: u32,
    pub message: String,
    pub suggested_fix: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum UbsSeverity {
    Critical,
    Important,
    Contextual,
}

fn run_ubs(mut cmd: Command) -> Result<UbsResult> {
    let output = cmd
        .output()
        .map_err(|err| MsError::Config(format!("run ubs: {err}")))?;
    let exit_code = output.status.code().unwrap_or(1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let findings = parse_findings(&stdout);
    Ok(UbsResult {
        exit_code,
        stdout,
        stderr,
        findings,
    })
}

fn parse_findings(output: &str) -> Vec<UbsFinding> {
    let mut findings = Vec::new();
    let mut current_category = String::new();
    let mut current_severity = UbsSeverity::Contextual;
    let mut last_index: Option<usize> = None;

    let issue_re = Regex::new(r"^(?P<file>[^:]+):(?P<line>\d+):(?P<col>\d+)\s*-\s*(?P<msg>.+)$")
        .unwrap();

    for raw_line in output.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if line.contains("Critical") {
            current_severity = UbsSeverity::Critical;
        } else if line.contains("Important") {
            current_severity = UbsSeverity::Important;
        } else if line.contains("Contextual") {
            current_severity = UbsSeverity::Contextual;
        }

        if let Some((left, _)) = line.split_once('(') {
            let trimmed = left.trim().trim_start_matches(|c: char| !c.is_ascii_alphanumeric());
            if !trimmed.is_empty() {
                current_category = trimmed.to_string();
            }
        }

        if let Some(caps) = issue_re.captures(line) {
            let file = caps["file"].to_string();
            let line_num = caps["line"].parse::<u32>().unwrap_or(0);
            let col_num = caps["col"].parse::<u32>().unwrap_or(0);
            let message = caps["msg"].to_string();
            findings.push(UbsFinding {
                category: current_category.clone(),
                severity: current_severity,
                file: PathBuf::from(file),
                line: line_num,
                column: col_num,
                message,
                suggested_fix: None,
            });
            last_index = Some(findings.len() - 1);
            continue;
        }

        if line.to_lowercase().starts_with("suggested fix") || line.to_lowercase().starts_with("fix") {
            if let Some(idx) = last_index {
                findings[idx].suggested_fix = Some(line.to_string());
            }
        }
    }

    findings
}
