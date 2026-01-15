//! bv integration helpers for skill graph analysis.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::de::DeserializeOwned;

use crate::beads::Issue;
use crate::error::{MsError, Result};

/// Client for interacting with the bv CLI in robot mode.
#[derive(Debug, Clone)]
pub struct BvClient {
    /// Path to bv binary (default: "bv")
    bv_bin: PathBuf,

    /// Working directory for bv commands (uses current dir if None)
    work_dir: Option<PathBuf>,

    /// Custom environment variables for the bv process
    env: HashMap<String, String>,
}

impl BvClient {
    /// Create a new BvClient with default settings.
    pub fn new() -> Self {
        Self {
            bv_bin: PathBuf::from("bv"),
            work_dir: None,
            env: HashMap::new(),
        }
    }

    /// Create a BvClient with a custom binary path.
    pub fn with_binary(binary: impl Into<PathBuf>) -> Self {
        Self {
            bv_bin: binary.into(),
            work_dir: None,
            env: HashMap::new(),
        }
    }

    /// Set the working directory for bv commands.
    pub fn with_work_dir(mut self, work_dir: impl Into<PathBuf>) -> Self {
        self.work_dir = Some(work_dir.into());
        self
    }

    /// Set an environment variable for the bv process.
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Check if bv is available and responsive.
    pub fn is_available(&self) -> bool {
        let mut cmd = Command::new(&self.bv_bin);
        cmd.arg("--version");
        if let Some(ref dir) = self.work_dir {
            cmd.current_dir(dir);
        }
        cmd.envs(&self.env);
        cmd.output().map(|o| o.status.success()).unwrap_or(false)
    }

    /// Run a bv robot command and parse JSON output.
    pub fn run_robot<T: DeserializeOwned>(&self, args: &[&str], root: &Path) -> Result<T> {
        let output = self.run_robot_raw(args, root)?;
        serde_json::from_slice(&output).map_err(MsError::from)
    }

    /// Run a bv robot command and return raw stdout bytes.
    pub fn run_robot_raw(&self, args: &[&str], root: &Path) -> Result<Vec<u8>> {
        let mut cmd = Command::new(&self.bv_bin);
        cmd.args(args).current_dir(root).envs(&self.env);
        let output = cmd.output().map_err(MsError::from)?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MsError::Config(format!(
                "bv command failed ({}): {}",
                output.status,
                stderr.trim()
            )));
        }
        Ok(output.stdout)
    }
}

/// Write issues to a .beads/beads.jsonl file under the given root.
pub fn write_beads_jsonl(issues: &[Issue], root: &Path) -> Result<PathBuf> {
    let beads_dir = root.join(".beads");
    std::fs::create_dir_all(&beads_dir)
        .map_err(|err| MsError::Config(format!("create {}: {err}", beads_dir.display())))?;

    let jsonl_path = beads_dir.join("beads.jsonl");
    let mut lines = Vec::with_capacity(issues.len());
    for issue in issues {
        let line = serde_json::to_string(issue)?;
        lines.push(line);
    }
    std::fs::write(&jsonl_path, lines.join("\n"))
        .map_err(|err| MsError::Config(format!("write {}: {err}", jsonl_path.display())))?;
    Ok(jsonl_path)
}

/// Helper to run bv on a temporary beads JSONL generated from the provided issues.
pub fn run_bv_on_issues<T: DeserializeOwned>(
    client: &BvClient,
    issues: &[Issue],
    args: &[&str],
) -> Result<T> {
    let temp = tempfile::tempdir().map_err(MsError::from)?;
    write_beads_jsonl(issues, temp.path())?;
    client.run_robot(args, temp.path())
}

/// Helper to run bv on a temporary beads JSONL and return raw stdout.
pub fn run_bv_on_issues_raw(client: &BvClient, issues: &[Issue], args: &[&str]) -> Result<Vec<u8>> {
    let temp = tempfile::tempdir().map_err(MsError::from)?;
    write_beads_jsonl(issues, temp.path())?;
    client.run_robot_raw(args, temp.path())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beads::{IssueStatus, IssueType};

    fn sample_issue(id: &str) -> Issue {
        Issue {
            id: id.to_string(),
            title: format!("Skill {id}"),
            description: String::new(),
            status: IssueStatus::Open,
            priority: 2,
            issue_type: IssueType::Task,
            owner: None,
            assignee: None,
            labels: Vec::new(),
            notes: None,
            created_at: None,
            created_by: None,
            updated_at: None,
            closed_at: None,
            dependencies: Vec::new(),
            dependents: Vec::new(),
            extra: HashMap::new(),
        }
    }

    #[test]
    fn test_write_beads_jsonl() {
        let temp = tempfile::tempdir().unwrap();
        let issues = vec![sample_issue("skill-a"), sample_issue("skill-b")];

        let path = write_beads_jsonl(&issues, temp.path()).unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        let mut lines = content.lines();

        let first: Issue = serde_json::from_str(lines.next().unwrap()).unwrap();
        assert_eq!(first.id, "skill-a");

        let second: Issue = serde_json::from_str(lines.next().unwrap()).unwrap();
        assert_eq!(second.id, "skill-b");
    }
}
