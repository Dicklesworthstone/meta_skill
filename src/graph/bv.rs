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
    /// Create a new `BvClient` with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            bv_bin: PathBuf::from("bv"),
            work_dir: None,
            env: HashMap::new(),
        }
    }

    /// Create a `BvClient` with a custom binary path.
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
    #[must_use]
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

    // =========================================
    // BvClient Construction Tests
    // =========================================

    #[test]
    fn test_bv_client_new_defaults() {
        let client = BvClient::new();
        assert_eq!(client.bv_bin, PathBuf::from("bv"));
        assert!(client.work_dir.is_none());
        assert!(client.env.is_empty());
    }

    #[test]
    fn test_bv_client_with_binary() {
        let client = BvClient::with_binary("/usr/local/bin/bv");
        assert_eq!(client.bv_bin, PathBuf::from("/usr/local/bin/bv"));
    }

    #[test]
    fn test_bv_client_with_work_dir() {
        let client = BvClient::new().with_work_dir("/tmp/test");
        assert_eq!(client.work_dir, Some(PathBuf::from("/tmp/test")));
    }

    #[test]
    fn test_bv_client_with_env() {
        let client = BvClient::new().with_env("BV_DEBUG", "1");
        assert_eq!(client.env.get("BV_DEBUG"), Some(&"1".to_string()));
    }

    #[test]
    fn test_bv_client_chained_builders() {
        let client = BvClient::with_binary("/custom/bv")
            .with_work_dir("/work")
            .with_env("KEY1", "val1")
            .with_env("KEY2", "val2");

        assert_eq!(client.bv_bin, PathBuf::from("/custom/bv"));
        assert_eq!(client.work_dir, Some(PathBuf::from("/work")));
        assert_eq!(client.env.len(), 2);
        assert_eq!(client.env.get("KEY1"), Some(&"val1".to_string()));
        assert_eq!(client.env.get("KEY2"), Some(&"val2".to_string()));
    }

    #[test]
    fn test_bv_client_clone() {
        let client = BvClient::new().with_env("TEST", "value");
        let cloned = client.clone();
        assert_eq!(cloned.bv_bin, client.bv_bin);
        assert_eq!(cloned.work_dir, client.work_dir);
        assert_eq!(cloned.env, client.env);
    }

    #[test]
    fn test_bv_client_debug() {
        let client = BvClient::new();
        let debug_str = format!("{:?}", client);
        assert!(debug_str.contains("BvClient"));
        assert!(debug_str.contains("bv_bin"));
    }

    // =========================================
    // BvClient::is_available Tests
    // =========================================

    #[test]
    fn test_bv_client_is_available_with_nonexistent_binary() {
        let client = BvClient::with_binary("/nonexistent/path/to/bv_not_here_12345");
        assert!(!client.is_available());
    }

    // =========================================
    // BvClient::run_robot Tests
    // =========================================

    #[test]
    fn test_run_robot_raw_nonexistent_binary() {
        let client = BvClient::with_binary("/nonexistent/bv_binary_12345");
        let temp = tempfile::tempdir().unwrap();
        let result = client.run_robot_raw(&["--version"], temp.path());
        assert!(result.is_err());
    }

    // =========================================
    // write_beads_jsonl Tests
    // =========================================

    #[test]
    fn test_write_beads_jsonl_empty_issues() {
        let temp = tempfile::tempdir().unwrap();
        let issues: Vec<Issue> = vec![];

        let path = write_beads_jsonl(&issues, temp.path()).unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.is_empty());
    }

    #[test]
    fn test_write_beads_jsonl_creates_directory() {
        let temp = tempfile::tempdir().unwrap();
        let beads_dir = temp.path().join(".beads");
        assert!(!beads_dir.exists());

        let issues = vec![sample_issue("test")];
        write_beads_jsonl(&issues, temp.path()).unwrap();

        assert!(beads_dir.exists());
        assert!(beads_dir.is_dir());
    }

    #[test]
    fn test_write_beads_jsonl_single_issue() {
        let temp = tempfile::tempdir().unwrap();
        let issues = vec![sample_issue("single")];

        let path = write_beads_jsonl(&issues, temp.path()).unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        let lines: Vec<_> = content.lines().collect();

        assert_eq!(lines.len(), 1);
        let parsed: Issue = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(parsed.id, "single");
    }

    #[test]
    fn test_write_beads_jsonl_path_is_correct() {
        let temp = tempfile::tempdir().unwrap();
        let issues = vec![sample_issue("test")];

        let path = write_beads_jsonl(&issues, temp.path()).unwrap();
        assert_eq!(path, temp.path().join(".beads").join("beads.jsonl"));
    }

    #[test]
    fn test_write_beads_jsonl_with_dependencies() {
        let temp = tempfile::tempdir().unwrap();
        let mut issue = sample_issue("with-deps");
        issue.dependencies = vec![crate::beads::Dependency {
            id: "dep-1".to_string(),
            title: "Dependency 1".to_string(),
            status: Some(IssueStatus::Open),
            dependency_type: None,
        }];

        let path = write_beads_jsonl(&[issue], temp.path()).unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        let parsed: Issue = serde_json::from_str(&content).unwrap();

        assert_eq!(parsed.dependencies.len(), 1);
        assert_eq!(parsed.dependencies[0].id, "dep-1");
    }

    #[test]
    fn test_write_beads_jsonl_with_all_fields() {
        let temp = tempfile::tempdir().unwrap();
        let mut issue = sample_issue("full");
        issue.title = "Full Issue".to_string();
        issue.description = "A complete issue".to_string();
        issue.status = IssueStatus::InProgress;
        issue.priority = 1;
        issue.issue_type = IssueType::Feature;
        issue.owner = Some("alice".to_string());
        issue.assignee = Some("bob".to_string());
        issue.labels = vec!["urgent".to_string(), "backend".to_string()];
        issue.notes = Some("Some notes".to_string());

        let path = write_beads_jsonl(&[issue.clone()], temp.path()).unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        let parsed: Issue = serde_json::from_str(&content).unwrap();

        assert_eq!(parsed.title, "Full Issue");
        assert_eq!(parsed.description, "A complete issue");
        assert_eq!(parsed.status, IssueStatus::InProgress);
        assert_eq!(parsed.priority, 1);
        assert_eq!(parsed.issue_type, IssueType::Feature);
        assert_eq!(parsed.owner, Some("alice".to_string()));
        assert_eq!(parsed.assignee, Some("bob".to_string()));
        assert!(parsed.labels.contains(&"urgent".to_string()));
        assert_eq!(parsed.notes, Some("Some notes".to_string()));
    }

    // =========================================
    // sample_issue Helper Tests
    // =========================================

    #[test]
    fn test_sample_issue_defaults() {
        let issue = sample_issue("test-id");
        assert_eq!(issue.id, "test-id");
        assert_eq!(issue.title, "Skill test-id");
        assert!(issue.description.is_empty());
        assert_eq!(issue.status, IssueStatus::Open);
        assert_eq!(issue.priority, 2);
        assert_eq!(issue.issue_type, IssueType::Task);
        assert!(issue.owner.is_none());
        assert!(issue.assignee.is_none());
        assert!(issue.labels.is_empty());
        assert!(issue.dependencies.is_empty());
        assert!(issue.dependents.is_empty());
        assert!(issue.extra.is_empty());
    }

    // =========================================
    // run_bv_on_issues Tests (without actual bv)
    // =========================================

    #[test]
    fn test_run_bv_on_issues_raw_nonexistent_bv() {
        let client = BvClient::with_binary("/nonexistent/bv_12345");
        let issues = vec![sample_issue("test")];
        let result = run_bv_on_issues_raw(&client, &issues, &["--version"]);
        assert!(result.is_err());
    }

    // =========================================
    // Issue Status Variants Tests
    // =========================================

    #[test]
    fn test_write_beads_jsonl_all_statuses() {
        let temp = tempfile::tempdir().unwrap();
        let mut issues = vec![];

        for (i, status) in [
            IssueStatus::Open,
            IssueStatus::InProgress,
            IssueStatus::Closed,
        ]
        .iter()
        .enumerate()
        {
            let mut issue = sample_issue(&format!("issue-{i}"));
            issue.status = *status;
            issues.push(issue);
        }

        let path = write_beads_jsonl(&issues, temp.path()).unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        let lines: Vec<_> = content.lines().collect();

        assert_eq!(lines.len(), 3);
        let parsed0: Issue = serde_json::from_str(lines[0]).unwrap();
        let parsed1: Issue = serde_json::from_str(lines[1]).unwrap();
        let parsed2: Issue = serde_json::from_str(lines[2]).unwrap();

        assert_eq!(parsed0.status, IssueStatus::Open);
        assert_eq!(parsed1.status, IssueStatus::InProgress);
        assert_eq!(parsed2.status, IssueStatus::Closed);
    }

    #[test]
    fn test_write_beads_jsonl_all_issue_types() {
        let temp = tempfile::tempdir().unwrap();
        let mut issues = vec![];

        for (i, issue_type) in [
            IssueType::Task,
            IssueType::Bug,
            IssueType::Feature,
            IssueType::Epic,
        ]
        .iter()
        .enumerate()
        {
            let mut issue = sample_issue(&format!("issue-{i}"));
            issue.issue_type = *issue_type;
            issues.push(issue);
        }

        let path = write_beads_jsonl(&issues, temp.path()).unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        let lines: Vec<_> = content.lines().collect();

        assert_eq!(lines.len(), 4);
    }
}
