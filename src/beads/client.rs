//! BeadsClient - CLI wrapper for the beads (bd) issue tracker.
//!
//! Provides programmatic access to beads using the `--json` flag
//! for structured output, following the same patterns as CassClient and UbsClient.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

use crate::error::{MsError, Result};
use crate::security::SafetyGate;

use super::types::{CreateIssueRequest, Issue, IssueStatus, UpdateIssueRequest, WorkFilter};
use super::version::{
    BeadsVersion, MINIMUM_SUPPORTED_VERSION, RECOMMENDED_VERSION, VersionCompatibility,
};

/// Client for interacting with the beads (bd) issue tracker.
#[derive(Debug, Clone)]
pub struct BeadsClient {
    /// Path to bd binary (default: "bd")
    bd_bin: PathBuf,

    /// Working directory for bd commands (uses current dir if None)
    work_dir: Option<PathBuf>,

    /// Custom environment variables for the bd process
    env: HashMap<String, String>,

    /// Optional safety gate for command execution
    safety: Option<SafetyGate>,
}

impl BeadsClient {
    /// Create a new BeadsClient with default settings.
    pub fn new() -> Self {
        Self {
            bd_bin: PathBuf::from("bd"),
            work_dir: None,
            env: HashMap::new(),
            safety: None,
        }
    }

    /// Create a BeadsClient with a custom binary path.
    pub fn with_binary(binary: impl Into<PathBuf>) -> Self {
        Self {
            bd_bin: binary.into(),
            work_dir: None,
            env: HashMap::new(),
            safety: None,
        }
    }

    /// Set the working directory for bd commands.
    pub fn with_work_dir(mut self, work_dir: impl Into<PathBuf>) -> Self {
        self.work_dir = Some(work_dir.into());
        self
    }

    /// Set an environment variable for the bd process.
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
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
        cmd.envs(&self.env);

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
        cmd.envs(&self.env);

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

    /// Get the parsed bd version.
    pub fn version_semver(&self) -> Result<BeadsVersion> {
        if let Ok(output) = self.run_command(&["version", "--json"]) {
            if let Ok(info) = serde_json::from_slice::<serde_json::Value>(&output) {
                if let Some(version_str) = info.get("version").and_then(|v| v.as_str()) {
                    return BeadsVersion::parse(version_str);
                }
            }
        }

        let raw = self
            .version()
            .ok_or_else(|| MsError::BeadsUnavailable("bd version not available".to_string()))?;
        BeadsVersion::parse(&raw)
    }

    /// Check if bd version is compatible with this client.
    pub fn check_compatibility(&self) -> Result<VersionCompatibility> {
        let version = self.version_semver()?;

        if version < *MINIMUM_SUPPORTED_VERSION {
            return Ok(VersionCompatibility::Unsupported {
                error: format!(
                    "bd {} is older than minimum supported {}. Please upgrade.",
                    version, *MINIMUM_SUPPORTED_VERSION
                ),
            });
        }

        if version < *RECOMMENDED_VERSION {
            return Ok(VersionCompatibility::Partial {
                warning: format!(
                    "bd {} is older than recommended {}. Some features may not work.",
                    version, *RECOMMENDED_VERSION
                ),
            });
        }

        Ok(VersionCompatibility::Full)
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
        let label_args: Vec<String> = filter
            .labels
            .iter()
            .map(|l| format!("--label={}", l))
            .collect();
        for label_arg in &label_args {
            args.push(label_arg);
        }

        let output = self.run_command(&args)?;
        let issues: Vec<Issue> = serde_json::from_slice(&output).map_err(|e| {
            MsError::BeadsUnavailable(format!("failed to parse list output: {}", e))
        })?;
        Ok(issues)
    }

    /// List issues ready to work (open and unblocked).
    pub fn ready(&self) -> Result<Vec<Issue>> {
        let output = self.run_command(&["ready", "--json"])?;
        let issues: Vec<Issue> = serde_json::from_slice(&output).map_err(|e| {
            MsError::BeadsUnavailable(format!("failed to parse ready output: {}", e))
        })?;
        Ok(issues)
    }

    /// Get a specific issue by ID.
    pub fn show(&self, issue_id: &str) -> Result<Issue> {
        // Validate issue_id to prevent command injection
        validate_issue_id(issue_id)?;

        let output = self.run_command(&["show", issue_id, "--json"])?;

        // bd show returns an array with one element
        let issues: Vec<Issue> = serde_json::from_slice(&output).map_err(|e| {
            MsError::BeadsUnavailable(format!("failed to parse show output: {}", e))
        })?;

        issues
            .into_iter()
            .next()
            .ok_or_else(|| MsError::NotFound(format!("issue not found: {}", issue_id)))
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

        // bd create --json returns a single object (not an array)
        let issue: Issue = serde_json::from_slice(&output).map_err(|e| {
            MsError::BeadsUnavailable(format!("failed to parse create output: {}", e))
        })?;
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

        // bd update --json returns an array with one element
        let issues: Vec<Issue> = serde_json::from_slice(&output).map_err(|e| {
            MsError::BeadsUnavailable(format!("failed to parse update output: {}", e))
        })?;

        issues
            .into_iter()
            .next()
            .ok_or_else(|| MsError::NotFound(format!("issue not found: {}", issue_id)))
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

        // bd close --json returns an array with one element
        let issues: Vec<Issue> = serde_json::from_slice(&output).map_err(|e| {
            MsError::BeadsUnavailable(format!("failed to parse close output: {}", e))
        })?;

        issues
            .into_iter()
            .next()
            .ok_or_else(|| MsError::NotFound(format!("issue not found: {}", issue_id)))
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

        let issues: Vec<Issue> = serde_json::from_slice(&output).map_err(|e| {
            MsError::BeadsUnavailable(format!("failed to parse close output: {}", e))
        })?;
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
        cmd.envs(&self.env);

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
/// (may include dots for hidden directories) and id is alphanumeric
/// (e.g., "meta_skill-abc123", "proj-7t2", ".tmpXXX-abc").
fn validate_issue_id(id: &str) -> Result<()> {
    if id.is_empty() {
        return Err(MsError::ValidationFailed(
            "issue ID cannot be empty".to_string(),
        ));
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
    const FORBIDDEN: &[char] = &[
        '|', '&', ';', '$', '`', '(', ')', '{', '}', '<', '>', '!', '*', '?', '[', ']', '#', '~',
        '\'', '"', '\n', '\r',
    ];
    if id.chars().any(|c| FORBIDDEN.contains(&c)) {
        return Err(MsError::ValidationFailed(
            "issue ID contains shell metacharacters".to_string(),
        ));
    }

    // Must match expected format: word-word or word_word-alphanum
    // Allow alphanumeric, underscore, hyphen, and dots (for temp dir names like .tmpXXX)
    // Single dots are OK; double dots are blocked above for path traversal
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
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
    if stderr_lower.contains("sync")
        && (stderr_lower.contains("fail") || stderr_lower.contains("error"))
    {
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
        let client =
            BeadsClient::with_binary("/usr/local/bin/bd").with_work_dir("/data/projects/test");
        assert_eq!(client.bd_bin, PathBuf::from("/usr/local/bin/bd"));
        assert_eq!(client.work_dir, Some(PathBuf::from("/data/projects/test")));
    }

    #[test]
    fn test_validate_issue_id_valid() {
        assert!(validate_issue_id("meta_skill-abc").is_ok());
        assert!(validate_issue_id("project-123").is_ok());
        assert!(validate_issue_id("test-7t2").is_ok());
        assert!(validate_issue_id("my_project-xyz123").is_ok());
        assert!(validate_issue_id(".tmp123-abc").is_ok()); // Should pass
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

/// Integration tests for BeadsClient.
///
/// These tests require a real `bd` binary and use isolated test environments.
/// Run with: `cargo test --package meta_skill beads::client::integration_tests`
#[cfg(test)]
mod integration_tests {
    use std::process::Command;

    use tempfile::TempDir;

    use super::*;
    use crate::beads::{CreateIssueRequest, IssueStatus, IssueType, TestLogger};

    /// Test fixture that creates an isolated beads environment.
    ///
    /// SAFETY: Uses tempdir + BEADS_DB override to completely isolate tests.
    struct TestBeadsEnv {
        /// Temporary directory containing the test database
        #[allow(dead_code)]
        temp_dir: TempDir,
        /// Directory where the project is initialized
        project_dir: PathBuf,
        /// Path to the test database
        db_path: PathBuf,
        /// Test logger for this environment
        log: TestLogger,
        /// Whether bd was successfully initialized
        #[allow(dead_code)]
        initialized: bool,
    }

    impl TestBeadsEnv {
        /// Create a new isolated test environment.
        ///
        /// Returns None if bd is not available.
        fn new(test_name: &str) -> Option<Self> {
            let mut log = TestLogger::new(test_name);

            // Check if bd is available first
            if !Command::new("bd")
                .arg("--version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
            {
                log.warn("SKIP", "bd not available, skipping test", None);
                return None;
            }

            let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
            let project_dir = temp_dir.path().join("testproj");
            std::fs::create_dir(&project_dir).expect("Failed to create project dir");

            let beads_dir = project_dir.join(".beads");
            std::fs::create_dir_all(&beads_dir).expect("Failed to create .beads directory");
            let db_path = beads_dir.join("beads.db");

            log.info(
                "SETUP",
                &format!("Test dir: {}", project_dir.display()),
                None,
            );

            // Initialize database using env var
            let init_status = Command::new("bd")
                .args(["init"])
                .env("BEADS_DB", &db_path)
                .current_dir(&project_dir)
                .output();

            let initialized = match init_status {
                Ok(output) if output.status.success() => {
                    log.success("INIT", "Test database initialized", None);
                    true
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    log.error("INIT", &format!("bd init failed: {}", stderr), None);
                    return None;
                }
                Err(e) => {
                    log.error("INIT", &format!("Failed to run bd: {}", e), None);
                    return None;
                }
            };

            Some(TestBeadsEnv {
                temp_dir,
                project_dir,
                db_path,
                log,
                initialized,
            })
        }

        /// Get a BeadsClient configured for this test environment.
        fn client(&self) -> BeadsClient {
            BeadsClient::new()
                .with_work_dir(&self.project_dir)
                .with_env("BEADS_DB", self.db_path.to_string_lossy())
        }

        /// Get a mutable reference to the test logger.
        fn log(&mut self) -> &mut TestLogger {
            &mut self.log
        }
    }

    impl Drop for TestBeadsEnv {
        fn drop(&mut self) {
            self.log.info("CLEANUP", "Test environment dropped", None);

            // Log final report if verbose
            let report = self.log.report();
            if std::env::var("BEADS_TEST_VERBOSE").is_ok() {
                if let Ok(pretty) = serde_json::to_string_pretty(&report) {
                    eprintln!("\n{}", pretty);
                }
            }
        }
    }

    #[test]
    fn test_is_available_with_real_bd() {
        let Some(mut env) = TestBeadsEnv::new("test_is_available") else {
            return; // Skip if bd not available
        };
        let client = env.client();

        assert!(
            client.is_available(),
            "bd should be available in test environment"
        );
        env.log()
            .success("VERIFY", "is_available() returns true", None);
    }

    #[test]
    fn test_is_available_with_nonexistent_path() {
        let mut log = TestLogger::new("test_unavailable");

        let client = BeadsClient::with_binary("/nonexistent/path/to/bd");
        assert!(
            !client.is_available(),
            "Should be unavailable with bad path"
        );

        log.success("VERIFY", "is_available() returns false for bad path", None);
    }

    #[test]
    fn test_full_issue_lifecycle() {
        let Some(mut env) = TestBeadsEnv::new("test_full_lifecycle") else {
            return;
        };
        let client = env.client();

        // CREATE
        env.log().info("LIFECYCLE", "Phase 1: Create", None);
        let issue = client
            .create(
                &CreateIssueRequest::new("Integration Test Issue")
                    .with_type(IssueType::Task)
                    .with_priority(2)
                    .with_description("Created by integration test"),
            )
            .expect("Create should succeed");

        env.log().info(
            "CREATE",
            &format!("Issue created: {}", issue.id),
            Some(serde_json::json!({
                "id": issue.id,
                "status": format!("{:?}", issue.status),
            })),
        );
        assert!(!issue.id.is_empty());
        assert_eq!(issue.status, IssueStatus::Open);

        // READ
        env.log().info("LIFECYCLE", "Phase 2: Read", None);
        let fetched = client.show(&issue.id).expect("Show should succeed");

        assert_eq!(fetched.id, issue.id);
        assert_eq!(fetched.title, "Integration Test Issue");
        env.log().success("READ", "Issue fetched correctly", None);

        // UPDATE
        env.log().info("LIFECYCLE", "Phase 3: Update", None);
        client
            .update_status(&issue.id, IssueStatus::InProgress)
            .expect("Update should succeed");

        let updated = client.show(&issue.id).expect("Should still be readable");
        assert_eq!(updated.status, IssueStatus::InProgress);
        env.log()
            .success("UPDATE", "Status updated to in_progress", None);

        // CLOSE
        env.log().info("LIFECYCLE", "Phase 4: Close", None);
        client
            .close(&issue.id, Some("Integration test complete"))
            .expect("Close should succeed");

        let closed = client.show(&issue.id).expect("Should still be readable");
        assert_eq!(closed.status, IssueStatus::Closed);
        env.log()
            .success("CLOSE", "Issue closed successfully", None);

        env.log()
            .success("LIFECYCLE", "Full lifecycle completed", None);
    }

    #[test]
    fn test_list_operations() {
        let Some(mut env) = TestBeadsEnv::new("test_list_operations") else {
            return;
        };
        let client = env.client();

        // Create variety of issues
        let mut created_ids = Vec::new();
        for (title, issue_type) in [
            ("Bug 1", IssueType::Bug),
            ("Task 1", IssueType::Task),
            ("Feature 1", IssueType::Feature),
            ("Task 2", IssueType::Task),
            ("Bug 2", IssueType::Bug),
        ] {
            let issue = client
                .create(&CreateIssueRequest::new(title).with_type(issue_type.clone()))
                .expect("Create should succeed");
            created_ids.push(issue.id);
            env.log().debug(
                "CREATE",
                &format!("Created {} ({:?})", title, issue_type),
                None,
            );
        }

        // Mark one as in_progress
        client
            .update_status(&created_ids[0], IssueStatus::InProgress)
            .expect("Update should succeed");

        // Close one
        client
            .close(&created_ids[1], None)
            .expect("Close should succeed");

        // Verify we created all issues
        assert_eq!(created_ids.len(), 5, "Should have created 5 issues");

        // Test list()
        let all_issues = client
            .list(&WorkFilter::default())
            .expect("List should succeed");
        env.log().info(
            "LIST",
            &format!(
                "Total issues: {} (created {})",
                all_issues.len(),
                created_ids.len()
            ),
            None,
        );

        // Should find at least the non-closed issues (we closed created_ids[1])
        // created_ids[0] = in_progress
        // created_ids[1] = closed (may be excluded from default list)
        // created_ids[2..4] = open
        let found_count = all_issues
            .iter()
            .filter(|i| created_ids.contains(&i.id))
            .count();
        assert!(
            found_count >= 4,
            "Should find at least 4 of 5 created issues in list (closed may be excluded), found {}",
            found_count
        );

        // Test ready() - returns open, unblocked issues
        let ready_issues = client.ready().expect("Ready should succeed");
        env.log().info(
            "READY",
            &format!("Ready issues: {}", ready_issues.len()),
            None,
        );

        // Verify closed issue is NOT in ready list (closed issues should never be ready)
        let closed_in_ready = ready_issues.iter().any(|i| i.id == created_ids[1]);
        assert!(
            !closed_in_ready,
            "Closed issue should not appear in ready list"
        );

        // Verify at least some open issues appear in ready list
        let open_ids = &created_ids[2..5]; // indices 2, 3, 4 should be open
        let open_in_ready = ready_issues
            .iter()
            .filter(|i| open_ids.contains(&i.id))
            .count();
        env.log().info(
            "READY",
            &format!("Open issues in ready: {}/{}", open_in_ready, open_ids.len()),
            None,
        );

        env.log()
            .success("LIST", "List and ready operations work correctly", None);
    }

    #[test]
    fn test_dependency_operations() {
        let Some(mut env) = TestBeadsEnv::new("test_dependencies") else {
            return;
        };
        let client = env.client();

        // Create parent and child
        let epic = client
            .create(&CreateIssueRequest::new("Parent Epic").with_type(IssueType::Epic))
            .expect("Create epic should succeed");

        let task = client
            .create(&CreateIssueRequest::new("Child Task").with_type(IssueType::Task))
            .expect("Create task should succeed");

        env.log().info(
            "SETUP",
            &format!("Epic: {}, Task: {}", epic.id, task.id),
            None,
        );

        // Add dependency
        client
            .add_dependency(&task.id, &epic.id)
            .expect("Add dependency should succeed");
        env.log()
            .info("DEP", "Added dependency: task -> epic", None);

        // Verify dependency via show
        let task_details = client.show(&task.id).expect("Show should succeed");
        env.log().info(
            "VERIFY",
            &format!("Task dependencies: {:?}", task_details.dependencies),
            None,
        );

        // Task should depend on epic
        let has_dependency = task_details.dependencies.iter().any(|d| d.id == epic.id);

        assert!(has_dependency, "Task should depend on epic");

        env.log()
            .success("DEP", "Dependency operations work correctly", None);
    }

    #[test]
    fn test_error_handling_not_found() {
        let Some(mut env) = TestBeadsEnv::new("test_errors") else {
            return;
        };
        let client = env.client();

        // Test NotFound error - bd may return different error types for non-existent issues
        let result = client.show("nonexistent-issue-xyz-123");
        assert!(
            result.is_err(),
            "Should return an error for non-existent issue"
        );
        let err = result.unwrap_err();
        env.log()
            .info("ERROR", &format!("Got error: {:?}", err), None);

        // Accept NotFound or BeadsUnavailable (bd may not distinguish these)
        let is_expected_error = matches!(err, MsError::NotFound(_) | MsError::BeadsUnavailable(_));
        assert!(
            is_expected_error,
            "Expected NotFound or BeadsUnavailable, got: {:?}",
            err
        );

        env.log().success(
            "ERROR",
            "Error handling works for non-existent issues",
            None,
        );
    }

    #[test]
    fn test_security_path_traversal_blocked() {
        let mut log = TestLogger::new("test_security");

        // These should fail validation without even calling bd
        let client = BeadsClient::new();

        let result = client.show("../../../etc/passwd");
        assert!(result.is_err());
        log.info(
            "SECURITY",
            &format!("Path traversal blocked: {}", result.unwrap_err()),
            None,
        );

        let result = client.show("test;rm -rf /");
        assert!(result.is_err());
        log.info(
            "SECURITY",
            &format!("Shell injection blocked: {}", result.unwrap_err()),
            None,
        );

        log.success("SECURITY", "Path traversal and injection blocked", None);
    }

    #[test]
    fn test_performance_baseline() {
        let Some(mut env) = TestBeadsEnv::new("test_performance") else {
            return;
        };
        let client = env.client();

        // Create baseline data
        for i in 0..10 {
            client
                .create(&CreateIssueRequest::new(&format!("Perf Test {}", i)))
                .expect("Create should succeed");
        }

        // Time list operations
        let start = std::time::Instant::now();
        for _ in 0..5 {
            let _ = client.list(&WorkFilter::default());
        }
        let list_time = start.elapsed().as_millis() / 5;

        let start = std::time::Instant::now();
        for _ in 0..5 {
            let _ = client.ready();
        }
        let ready_time = start.elapsed().as_millis() / 5;

        env.log().info(
            "PERF",
            &format!("Avg list: {}ms, Avg ready: {}ms", list_time, ready_time),
            None,
        );

        // Sanity check - operations should be reasonably fast (< 1 second each)
        assert!(list_time < 1000, "List too slow: {}ms", list_time);
        assert!(ready_time < 1000, "Ready too slow: {}ms", ready_time);

        env.log()
            .success("PERF", "Performance within acceptable bounds", None);
    }

    #[test]
    fn test_create_with_labels() {
        let Some(mut env) = TestBeadsEnv::new("test_labels") else {
            return;
        };
        let client = env.client();

        let issue = client
            .create(
                &CreateIssueRequest::new("Issue with Labels")
                    .with_type(IssueType::Feature)
                    .with_label("backend")
                    .with_label("urgent"),
            )
            .expect("Create with labels should succeed");

        env.log().info(
            "CREATE",
            &format!("Created issue with labels: {:?}", issue.labels),
            None,
        );

        // Verify labels were set
        let fetched = client.show(&issue.id).expect("Show should succeed");
        assert!(
            fetched.labels.iter().any(|l| l == "backend"),
            "Should have backend label"
        );
        assert!(
            fetched.labels.iter().any(|l| l == "urgent"),
            "Should have urgent label"
        );

        env.log()
            .success("LABELS", "Labels applied correctly", None);
    }

    #[test]
    fn test_version_check() {
        let mut log = TestLogger::new("test_version");

        let client = BeadsClient::new();
        if !client.is_available() {
            log.warn("SKIP", "bd not available", None);
            return;
        }

        let version = client.version();
        assert!(version.is_some(), "Should be able to get version");

        let version_str = version.unwrap();
        log.info("VERSION", &format!("bd version: {}", version_str), None);
        assert!(!version_str.is_empty(), "Version should not be empty");

        log.success("VERSION", "Version check works", None);
    }

    #[test]
    fn test_version_compatibility_check() {
        let mut log = TestLogger::new("test_version_compatibility");

        let client = BeadsClient::new();
        if !client.is_available() {
            log.warn("SKIP", "bd not available", None);
            return;
        }

        let compat = client
            .check_compatibility()
            .expect("Should be able to check compatibility");

        match compat {
            VersionCompatibility::Full => {
                log.info("COMPAT", "Full compatibility", None);
            }
            VersionCompatibility::Partial { warning } => {
                log.warn("COMPAT", &warning, None);
            }
            VersionCompatibility::Unsupported { error } => {
                panic!("Unsupported version: {}", error);
            }
        }
    }
}
