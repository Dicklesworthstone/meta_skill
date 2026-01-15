//! BeadsClient - CLI wrapper for the beads (bd) issue tracker.
//!
//! Provides programmatic access to beads using the `--json` flag
//! for structured output, following the same patterns as CassClient and UbsClient.

use std::path::PathBuf;
use std::process::Command;

use crate::error::{MsError, Result};
use crate::security::SafetyGate;

use super::types::{CreateIssueRequest, Issue, IssueStatus, UpdateIssueRequest, WorkFilter};

/// Client for interacting with the beads (bd) issue tracker.
#[derive(Debug, Clone)]
pub struct BeadsClient {
    /// Path to bd binary (default: "bd")
    bd_bin: PathBuf,

    /// Working directory for bd commands (uses current dir if None)
    work_dir: Option<PathBuf>,

    /// Optional safety gate for command execution
    safety: Option<SafetyGate>,
}

impl BeadsClient {
    /// Create a new BeadsClient with default settings.
    pub fn new() -> Self {
        Self {
            bd_bin: PathBuf::from("bd"),
            work_dir: None,
            safety: None,
        }
    }

    /// Create a BeadsClient with a custom binary path.
    pub fn with_binary(binary: impl Into<PathBuf>) -> Self {
        Self {
            bd_bin: binary.into(),
            work_dir: None,
            safety: None,
        }
    }

    /// Set the working directory for bd commands.
    pub fn with_work_dir(mut self, work_dir: impl Into<PathBuf>) -> Self {
        self.work_dir = Some(work_dir.into());
        self
    }

    /// Set the safety gate for command execution.
    pub fn with_safety(mut self, safety: SafetyGate) -> Self {
        self.safety = Some(safety);
        self
    }

    /// Check if beads is available and responsive.
    pub fn is_available(&self) -> bool {
        let mut cmd = Command::new(&self.bd_bin);
        cmd.arg("--version");
        if let Some(ref dir) = self.work_dir {
            cmd.current_dir(dir);
        }
        if let Some(gate) = self.safety.as_ref() {
            let command_str = command_string(&cmd);
            if gate.enforce(&command_str, None).is_err() {
                return false;
            }
        }
        cmd.output().map(|o| o.status.success()).unwrap_or(false)
    }

    /// Get beads version.
    pub fn version(&self) -> Option<String> {
        let mut cmd = Command::new(&self.bd_bin);
        cmd.arg("--version");
        if let Some(ref dir) = self.work_dir {
            cmd.current_dir(dir);
        }
        let output = cmd.output().ok()?;
        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if version.is_empty() {
                None
            } else {
                Some(version)
            }
        } else {
            None
        }
    }

    /// List all issues matching the filter.
    pub fn list(&self, filter: &WorkFilter) -> Result<Vec<Issue>> {
        let mut args = vec!["list", "--json"];

        // Build filter arguments
        let status_str;
        if let Some(status) = &filter.status {
            status_str = format!("--status={}", status);
            args.push(&status_str);
        }

        let type_str;
        if let Some(issue_type) = &filter.issue_type {
            type_str = format!("--type={}", issue_type);
            args.push(&type_str);
        }

        let assignee_str;
        if let Some(assignee) = &filter.assignee {
            assignee_str = format!("--assignee={}", assignee);
            args.push(&assignee_str);
        }

        let limit_str;
        if let Some(limit) = filter.limit {
            limit_str = format!("--limit={}", limit);
            args.push(&limit_str);
        }

        // Label filters
        let label_args: Vec<String> = filter.labels.iter().map(|l| format!("--label={}", l)).collect();
        for label_arg in &label_args {
            args.push(label_arg);
        }

        let output = self.run_command(&args)?;
        let issues: Vec<Issue> = serde_json::from_slice(&output)
            .map_err(|e| MsError::BeadsUnavailable(format!("failed to parse list output: {}", e)))?;
        Ok(issues)
    }

    /// List issues ready to work (open and unblocked).
    pub fn ready(&self) -> Result<Vec<Issue>> {
        let output = self.run_command(&["ready", "--json"])?;
        let issues: Vec<Issue> = serde_json::from_slice(&output)
            .map_err(|e| MsError::BeadsUnavailable(format!("failed to parse ready output: {}", e)))?;
        Ok(issues)
    }

    /// Get a specific issue by ID.
    pub fn show(&self, issue_id: &str) -> Result<Issue> {
        // Validate issue_id to prevent command injection
        validate_issue_id(issue_id)?;

        let output = self.run_command(&["show", issue_id, "--json"])?;

        // bd show returns an array with one element
        let issues: Vec<Issue> = serde_json::from_slice(&output)
            .map_err(|e| MsError::BeadsUnavailable(format!("failed to parse show output: {}", e)))?;

        issues.into_iter().next().ok_or_else(|| {
            MsError::NotFound(format!("issue not found: {}", issue_id))
        })
    }

    /// Create a new issue.
    pub fn create(&self, req: &CreateIssueRequest) -> Result<Issue> {
        let mut args = vec!["create".to_string()];

        args.push(format!("--title={}", req.title));

        if let Some(ref desc) = req.description {
            args.push(format!("--description={}", desc));
        }

        if let Some(issue_type) = &req.issue_type {
            args.push(format!("--type={}", issue_type));
        }

        if let Some(priority) = req.priority {
            args.push(format!("--priority={}", priority));
        }

        for label in &req.labels {
            args.push(format!("--label={}", label));
        }

        if let Some(ref parent) = req.parent {
            args.push(format!("--parent={}", parent));
        }

        args.push("--json".to_string());

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.run_command(&args_refs)?;

        // bd create returns the created issue
        let issue: Issue = serde_json::from_slice(&output)
            .map_err(|e| MsError::BeadsUnavailable(format!("failed to parse create output: {}", e)))?;
        Ok(issue)
    }

    /// Update an existing issue.
    pub fn update(&self, issue_id: &str, req: &UpdateIssueRequest) -> Result<Issue> {
        validate_issue_id(issue_id)?;

        let mut args = vec!["update".to_string(), issue_id.to_string()];

        if let Some(status) = &req.status {
            args.push(format!("--status={}", status));
        }

        if let Some(ref title) = req.title {
            args.push(format!("--title={}", title));
        }

        if let Some(ref desc) = req.description {
            args.push(format!("--description={}", desc));
        }

        if let Some(priority) = req.priority {
            args.push(format!("--priority={}", priority));
        }

        if let Some(ref assignee) = req.assignee {
            args.push(format!("--assignee={}", assignee));
        }

        if let Some(ref notes) = req.notes {
            args.push(format!("--notes={}", notes));
        }

        for label in &req.add_labels {
            args.push(format!("--add-label={}", label));
        }

        for label in &req.remove_labels {
            args.push(format!("--remove-label={}", label));
        }

        args.push("--json".to_string());

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.run_command(&args_refs)?;

        let issue: Issue = serde_json::from_slice(&output)
            .map_err(|e| MsError::BeadsUnavailable(format!("failed to parse update output: {}", e)))?;
        Ok(issue)
    }

    /// Update just the status of an issue (convenience method).
    pub fn update_status(&self, issue_id: &str, status: IssueStatus) -> Result<Issue> {
        self.update(issue_id, &UpdateIssueRequest::new().with_status(status))
    }

    /// Close an issue.
    pub fn close(&self, issue_id: &str, reason: Option<&str>) -> Result<Issue> {
        validate_issue_id(issue_id)?;

        let mut args = vec!["close".to_string(), issue_id.to_string()];

        if let Some(reason) = reason {
            args.push(format!("--reason={}", reason));
        }

        args.push("--json".to_string());

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.run_command(&args_refs)?;

        let issue: Issue = serde_json::from_slice(&output)
            .map_err(|e| MsError::BeadsUnavailable(format!("failed to parse close output: {}", e)))?;
        Ok(issue)
    }

    /// Close multiple issues at once.
    pub fn close_batch(&self, issue_ids: &[&str]) -> Result<Vec<Issue>> {
        // Validate all issue IDs first
        for id in issue_ids {
            validate_issue_id(id)?;
        }

        let mut args: Vec<String> = vec!["close".to_string()];
        for id in issue_ids {
            args.push(id.to_string());
        }
        args.push("--json".to_string());

        let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let output = self.run_command(&args_refs)?;

        let issues: Vec<Issue> = serde_json::from_slice(&output)
            .map_err(|e| MsError::BeadsUnavailable(format!("failed to parse close output: {}", e)))?;
        Ok(issues)
    }

    /// Add a dependency between issues.
    pub fn add_dependency(&self, issue_id: &str, depends_on: &str) -> Result<()> {
        validate_issue_id(issue_id)?;
        validate_issue_id(depends_on)?;

        self.run_command(&["dep", "add", issue_id, depends_on])?;
        Ok(())
    }

    /// Remove a dependency between issues.
    pub fn remove_dependency(&self, issue_id: &str, depends_on: &str) -> Result<()> {
        validate_issue_id(issue_id)?;
        validate_issue_id(depends_on)?;

        self.run_command(&["dep", "remove", issue_id, depends_on])?;
        Ok(())
    }

    /// Sync beads state with git.
    pub fn sync(&self) -> Result<()> {
        self.run_command(&["sync"])?;
        Ok(())
    }

    /// Check sync status without syncing.
    pub fn sync_status(&self) -> Result<SyncStatus> {
        let output = self.run_command(&["sync", "--status"])?;
        let output_str = String::from_utf8_lossy(&output);

        // Parse the status output
        if output_str.contains("no differences") || output_str.contains("up to date") {
            Ok(SyncStatus::Clean)
        } else if output_str.contains("pending") || output_str.contains("uncommitted") {
            Ok(SyncStatus::Dirty)
        } else {
            Ok(SyncStatus::Unknown)
        }
    }

    /// Run a bd command and return stdout.
    fn run_command(&self, args: &[&str]) -> Result<Vec<u8>> {
        let mut cmd = Command::new(&self.bd_bin);
        cmd.args(args);

        if let Some(ref dir) = self.work_dir {
            cmd.current_dir(dir);
        }

        // Safety gate check
        if let Some(gate) = self.safety.as_ref() {
            let command_str = command_string(&cmd);
            gate.enforce(&command_str, None)?;
        }

        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let exit_code = output.status.code().unwrap_or(-1);
            return Err(classify_beads_error(exit_code, &stderr));
        }

        Ok(output.stdout)
    }
}

impl Default for BeadsClient {
    fn default() -> Self {
        Self::new()
    }
}

impl super::mock::BeadsOperations for BeadsClient {
    fn is_available(&self) -> bool {
        self.is_available()
    }

    fn list(&self, filter: &WorkFilter) -> Result<Vec<Issue>> {
        self.list(filter)
    }

    fn ready(&self) -> Result<Vec<Issue>> {
        self.ready()
    }

    fn show(&self, id: &str) -> Result<Issue> {
        self.show(id)
    }

    fn create(&self, request: &CreateIssueRequest) -> Result<Issue> {
        self.create(request)
    }

    fn update(&self, id: &str, request: &UpdateIssueRequest) -> Result<Issue> {
        self.update(id, request)
    }

    fn update_status(&self, id: &str, status: IssueStatus) -> Result<Issue> {
        self.update_status(id, status)
    }

    fn close(&self, id: &str, reason: Option<&str>) -> Result<Issue> {
        self.close(id, reason)
    }
}

/// Sync status for beads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStatus {
    /// No uncommitted changes
    Clean,
    /// Uncommitted changes exist
    Dirty,
    /// Unknown status
    Unknown,
}

/// Validate an issue ID to prevent command injection.
///
/// Valid issue IDs match the pattern: `project-id` where project is alphanumeric
/// and id is alphanumeric (e.g., "meta_skill-abc123", "proj-7t2").
fn validate_issue_id(id: &str) -> Result<()> {
    if id.is_empty() {
        return Err(MsError::ValidationFailed("issue ID cannot be empty".to_string()));
    }

    // Check for path traversal and shell metacharacters
    if id.contains('/') || id.contains('\\') || id.contains('\0') {
        return Err(MsError::ValidationFailed(
            "issue ID contains invalid characters".to_string(),
        ));
    }

    if id.contains("..") {
        return Err(MsError::ValidationFailed(
            "issue ID contains path traversal sequence".to_string(),
        ));
    }

    // Check for shell metacharacters that could enable injection
    const FORBIDDEN: &[char] = &['|', '&', ';', '$', '`', '(', ')', '{', '}', '<', '>', '!', '*', '?', '[', ']', '#', '~', '\'', '"', '\n', '\r'];
    if id.chars().any(|c| FORBIDDEN.contains(&c)) {
        return Err(MsError::ValidationFailed(
            "issue ID contains shell metacharacters".to_string(),
        ));
    }

    // Must match expected format: word-word or word_word-alphanum
    // Allow alphanumeric, underscore, and hyphen
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(MsError::ValidationFailed(format!(
            "issue ID contains invalid characters: {}",
            id
        )));
    }

    Ok(())
}

/// Convert a Command to a string representation.
fn command_string(cmd: &Command) -> String {
    let program = cmd.get_program().to_string_lossy().to_string();
    let args = cmd
        .get_args()
        .map(|arg| arg.to_string_lossy())
        .collect::<Vec<_>>();
    if args.is_empty() {
        program
    } else {
        format!("{} {}", program, args.join(" "))
    }
}

/// Classify beads errors into actionable categories.
fn classify_beads_error(exit_code: i32, stderr: &str) -> MsError {
    let stderr_lower = stderr.to_lowercase();

    // Not found errors
    if stderr_lower.contains("not found") || stderr_lower.contains("no such") {
        return MsError::NotFound(stderr.to_string());
    }

    // Database locked errors (transient, retriable)
    if stderr_lower.contains("database") && stderr_lower.contains("locked") {
        return MsError::TransactionFailed(format!("beads database locked: {}", stderr));
    }

    // Sync errors
    if stderr_lower.contains("sync") && (stderr_lower.contains("fail") || stderr_lower.contains("error")) {
        return MsError::TransactionFailed(format!("beads sync failed: {}", stderr));
    }

    // Validation errors
    if stderr_lower.contains("invalid") || stderr_lower.contains("validation") {
        return MsError::ValidationFailed(stderr.to_string());
    }

    // Default: beads unavailable
    MsError::BeadsUnavailable(format!(
        "beads command failed (exit {}): {}",
        exit_code, stderr
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beads_client_creation() {
        let client = BeadsClient::new();
        assert_eq!(client.bd_bin, PathBuf::from("bd"));
    }

    #[test]
    fn test_beads_client_builder() {
        let client = BeadsClient::with_binary("/usr/local/bin/bd")
            .with_work_dir("/data/projects/test");
        assert_eq!(client.bd_bin, PathBuf::from("/usr/local/bin/bd"));
        assert_eq!(client.work_dir, Some(PathBuf::from("/data/projects/test")));
    }

    #[test]
    fn test_validate_issue_id_valid() {
        assert!(validate_issue_id("meta_skill-abc").is_ok());
        assert!(validate_issue_id("project-123").is_ok());
        assert!(validate_issue_id("test-7t2").is_ok());
        assert!(validate_issue_id("my_project-xyz123").is_ok());
    }

    #[test]
    fn test_validate_issue_id_empty() {
        assert!(validate_issue_id("").is_err());
    }

    #[test]
    fn test_validate_issue_id_path_traversal() {
        assert!(validate_issue_id("../etc/passwd").is_err());
        assert!(validate_issue_id("test/../foo").is_err());
        assert!(validate_issue_id("/etc/passwd").is_err());
        assert!(validate_issue_id("test\\foo").is_err());
    }

    #[test]
    fn test_validate_issue_id_shell_injection() {
        assert!(validate_issue_id("test; rm -rf /").is_err());
        assert!(validate_issue_id("test|cat /etc/passwd").is_err());
        assert!(validate_issue_id("test$(whoami)").is_err());
        assert!(validate_issue_id("test`whoami`").is_err());
        assert!(validate_issue_id("test & echo hi").is_err());
    }

    #[test]
    fn test_error_classification_not_found() {
        let err = classify_beads_error(1, "Issue not found: xyz");
        assert!(matches!(err, MsError::NotFound(_)));
    }

    #[test]
    fn test_error_classification_database_locked() {
        let err = classify_beads_error(1, "Database is locked");
        assert!(matches!(err, MsError::TransactionFailed(_)));
    }

    #[test]
    fn test_error_classification_sync_failed() {
        let err = classify_beads_error(1, "Sync failed: network error");
        assert!(matches!(err, MsError::TransactionFailed(_)));
    }

    #[test]
    fn test_error_classification_generic() {
        let err = classify_beads_error(42, "Unknown error");
        assert!(matches!(err, MsError::BeadsUnavailable(_)));
    }
}
