//! UBS (Ultimate Bug Scanner) integration.

use std::path::{Path, PathBuf};
use std::process::Command;

use regex::Regex;

use crate::error::{MsError, Result};
use crate::security::SafetyGate;

#[derive(Debug, Clone)]
pub struct UbsClient {
    ubs_path: PathBuf,
    safety: Option<SafetyGate>,
}

impl UbsClient {
    pub fn new(ubs_path: Option<PathBuf>) -> Self {
        Self {
            ubs_path: ubs_path.unwrap_or_else(|| PathBuf::from("ubs")),
            safety: None,
        }
    }

    pub fn with_safety(mut self, safety: SafetyGate) -> Self {
        self.safety = Some(safety);
        self
    }

    pub fn check_files(&self, files: &[PathBuf]) -> Result<UbsResult> {
        if files.is_empty() {
            return Ok(UbsResult::empty());
        }

        let mut cmd = Command::new(&self.ubs_path);
        for file in files {
            cmd.arg(file);
        }
        run_ubs(cmd, self.safety.as_ref())
    }

    pub fn check_dir(&self, dir: &Path, only: Option<&str>) -> Result<UbsResult> {
        let mut cmd = Command::new(&self.ubs_path);
        if let Some(lang) = only {
            cmd.arg(format!("--only={lang}"));
        }
        cmd.arg(dir);
        run_ubs(cmd, self.safety.as_ref())
    }

    pub fn check_staged(&self, repo_root: &Path) -> Result<UbsResult> {
        let mut git_cmd = Command::new("git");
        git_cmd
            .arg("diff")
            .arg("--name-only")
            .arg("--cached")
            .current_dir(repo_root);
        if let Some(gate) = self.safety.as_ref() {
            let command_str = command_string(&git_cmd);
            gate.enforce(&command_str, None)?;
        }
        let output = git_cmd.output()
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

fn run_ubs(mut cmd: Command, gate: Option<&SafetyGate>) -> Result<UbsResult> {
    if let Some(gate) = gate {
        let command_str = command_string(&cmd);
        gate.enforce(&command_str, None)?;
    }
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

fn command_string(cmd: &Command) -> String {
    let program = cmd.get_program().to_string_lossy().to_string();
    let args = cmd
        .get_args()
        .map(|arg| arg.to_string_lossy())
        .collect::<Vec<_>>();
    if args.is_empty() {
        program
    } else {
        format!("{program} {}", args.join(" "))
    }
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
