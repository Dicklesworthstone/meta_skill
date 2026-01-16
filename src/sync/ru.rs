//! RU (Repo Updater) integration for skill repository sync.
//!
//! Wraps the `ru` CLI tool to provide repository synchronization
//! for skill repositories distributed via GitHub.

use std::path::PathBuf;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::{MsError, Result};

#[derive(Debug, Deserialize)]
struct RuSyncOutput {
    #[serde(default)]
    repos: Vec<RuSyncRepo>,
}

#[derive(Debug, Deserialize)]
struct RuSyncRepo {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    repo: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    action: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

/// Exit codes from ru (see AGENTS.md)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuExitCode {
    /// Success - all operations completed
    Ok = 0,
    /// Partial success - some repos had issues
    Partial = 1,
    /// Conflicts detected - manual intervention needed
    Conflicts = 2,
    /// System error - git/network failure
    SystemError = 3,
    /// Bad arguments - invalid CLI usage
    BadArgs = 4,
    /// Interrupted - can resume with --resume
    Interrupted = 5,
}

impl RuExitCode {
    #[must_use] 
    pub const fn from_code(code: i32) -> Self {
        match code {
            0 => Self::Ok,
            1 => Self::Partial,
            2 => Self::Conflicts,
            3 => Self::SystemError,
            4 => Self::BadArgs,
            5 => Self::Interrupted,
            _ => Self::SystemError,
        }
    }

    #[must_use] 
    pub const fn is_success(self) -> bool {
        matches!(self, Self::Ok | Self::Partial)
    }
}

/// Result from an ru sync operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuSyncResult {
    pub exit_code: i32,
    pub cloned: Vec<String>,
    pub pulled: Vec<String>,
    pub conflicts: Vec<RuConflict>,
    pub errors: Vec<RuError>,
    pub skipped: Vec<String>,
}

/// A conflict detected by ru
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuConflict {
    pub repo: String,
    pub reason: String,
    #[serde(default)]
    pub resolution_hint: Option<String>,
}

/// An error reported by ru
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuError {
    pub repo: String,
    pub error: String,
}

/// Repository status from ru
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuRepoStatus {
    pub path: PathBuf,
    pub name: String,
    pub clean: bool,
    pub ahead: u32,
    pub behind: u32,
    #[serde(default)]
    pub branch: Option<String>,
}

/// Options for ru sync
#[derive(Debug, Clone, Default)]
pub struct RuSyncOptions {
    pub dry_run: bool,
    pub clone_only: bool,
    pub pull_only: bool,
    pub autostash: bool,
    pub rebase: bool,
    pub parallel: Option<u32>,
    pub resume: bool,
}

/// Client for interacting with the ru CLI
pub struct RuClient {
    /// Path to ru binary (None = auto-detect)
    ru_path: Option<PathBuf>,
    /// Cached detection result
    available: Option<bool>,
}

impl Default for RuClient {
    fn default() -> Self {
        Self::new()
    }
}

impl RuClient {
    /// Create a new `RuClient` with auto-detection
    #[must_use] 
    pub const fn new() -> Self {
        Self {
            ru_path: None,
            available: None,
        }
    }

    /// Create an `RuClient` with explicit path
    #[must_use] 
    pub const fn with_path(path: PathBuf) -> Self {
        Self {
            ru_path: Some(path),
            available: None,
        }
    }

    /// Check if ru is available
    pub fn is_available(&mut self) -> bool {
        if let Some(available) = self.available {
            return available;
        }

        let result = self.detect_ru();
        self.available = Some(result);
        result
    }

    /// Get the ru binary path
    #[must_use] 
    pub fn ru_path(&self) -> &str {
        self.ru_path
            .as_ref()
            .map_or("ru", |p| p.to_str().unwrap_or("ru"))
    }

    /// Detect if ru is installed and working
    fn detect_ru(&self) -> bool {
        let output = Command::new(self.ru_path())
            .arg("--version")
            .output();

        match output {
            Ok(out) => out.status.success(),
            Err(_) => false,
        }
    }

    /// Sync all configured repositories
    pub fn sync(&mut self, options: &RuSyncOptions) -> Result<RuSyncResult> {
        if !self.is_available() {
            return Err(MsError::Config(
                "ru is not available; install from /data/projects/repo_updater".to_string(),
            ));
        }

        let mut cmd = Command::new(self.ru_path());
        cmd.arg("sync")
            .arg("--json")
            .arg("--non-interactive");

        if options.dry_run {
            cmd.arg("--dry-run");
        }
        if options.clone_only {
            cmd.arg("--clone-only");
        }
        if options.pull_only {
            cmd.arg("--pull-only");
        }
        if options.autostash {
            cmd.arg("--autostash");
        }
        if options.rebase {
            cmd.arg("--rebase");
        }
        if let Some(parallel) = options.parallel {
            cmd.arg("-j").arg(parallel.to_string());
        }
        if options.resume {
            cmd.arg("--resume");
        }

        let output = cmd.output().map_err(|err| {
            MsError::Config(format!("failed to execute ru sync: {err}"))
        })?;

        let exit_code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let mut result = parse_sync_output(&stdout, exit_code);

        // Ensure exit code matches actual process status
        result.exit_code = exit_code;

        if !output.status.success() && !stderr.trim().is_empty() {
            result.errors.push(RuError {
                repo: "unknown".to_string(),
                error: stderr.trim().to_string(),
            });
        }

        Ok(result)
    }

    /// Get status of all repositories without making changes
    pub fn status(&mut self) -> Result<Vec<RuRepoStatus>> {
        if !self.is_available() {
            return Err(MsError::Config(
                "ru is not available; install from /data/projects/repo_updater".to_string(),
            ));
        }

        let output = Command::new(self.ru_path())
            .args(["status", "--no-fetch", "--json"])
            .output()
            .map_err(|err| MsError::Config(format!("failed to execute ru status: {err}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MsError::Config(format!(
                "ru status failed: {}",
                stderr.trim()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let statuses: Vec<RuRepoStatus> = serde_json::from_str(&stdout).unwrap_or_default();
        Ok(statuses)
    }

    /// List all configured repository paths
    pub fn list_paths(&mut self) -> Result<Vec<PathBuf>> {
        if !self.is_available() {
            return Err(MsError::Config(
                "ru is not available; install from /data/projects/repo_updater".to_string(),
            ));
        }

        let output = Command::new(self.ru_path())
            .args(["list", "--paths"])
            .output()
            .map_err(|err| MsError::Config(format!("failed to execute ru list: {err}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MsError::Config(format!(
                "ru list failed: {}",
                stderr.trim()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let paths: Vec<PathBuf> = stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(PathBuf::from)
            .collect();

        Ok(paths)
    }

    /// Run ru doctor to check system health
    pub fn doctor(&mut self) -> Result<bool> {
        if !self.is_available() {
            return Err(MsError::Config(
                "ru is not available; install from /data/projects/repo_updater".to_string(),
            ));
        }

        let output = Command::new(self.ru_path())
            .arg("doctor")
            .output()
            .map_err(|err| MsError::Config(format!("failed to execute ru doctor: {err}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MsError::Config(format!(
                "ru doctor failed: {}",
                stderr.trim()
            )));
        }

        Ok(true)
    }
}

fn parse_sync_output(stdout: &str, exit_code: i32) -> RuSyncResult {
    let parsed = serde_json::from_str::<RuSyncOutput>(stdout);
    if let Ok(output) = parsed {
        let mut result = RuSyncResult {
            exit_code,
            cloned: Vec::new(),
            pulled: Vec::new(),
            conflicts: Vec::new(),
            errors: Vec::new(),
            skipped: Vec::new(),
        };

        for repo in output.repos {
            let name = repo
                .repo
                .or(repo.name)
                .or(repo.path.as_ref().and_then(|p| {
                    std::path::Path::new(p)
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                }))
                .unwrap_or_else(|| "unknown".to_string());
            let status = repo.status.unwrap_or_else(|| repo.action.unwrap_or_default());
            let message = repo.message.unwrap_or_default();

            match status.as_str() {
                "cloned" | "clone" => result.cloned.push(name),
                "updated" | "pull" => result.pulled.push(name),
                "current" | "skip" => result.skipped.push(name),
                "conflict" => result.conflicts.push(RuConflict {
                    repo: name,
                    reason: if message.is_empty() {
                        "conflict".to_string()
                    } else {
                        message
                    },
                    resolution_hint: None,
                }),
                "failed" | "fail" => result.errors.push(RuError {
                    repo: name,
                    error: if message.is_empty() {
                        "failed".to_string()
                    } else {
                        message
                    },
                }),
                _ => result.skipped.push(name),
            }
        }

        if result.errors.is_empty() && result.conflicts.is_empty() && stdout.trim().is_empty() {
            result.errors.push(RuError {
                repo: "unknown".to_string(),
                error: "ru returned empty output".to_string(),
            });
        }

        return result;
    }

    RuSyncResult {
        exit_code,
        cloned: Vec::new(),
        pulled: Vec::new(),
        conflicts: Vec::new(),
        errors: vec![RuError {
            repo: "unknown".to_string(),
            error: stdout.to_string(),
        }],
        skipped: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_from_code_maps_correctly() {
        assert_eq!(RuExitCode::from_code(0), RuExitCode::Ok);
        assert_eq!(RuExitCode::from_code(1), RuExitCode::Partial);
        assert_eq!(RuExitCode::from_code(2), RuExitCode::Conflicts);
        assert_eq!(RuExitCode::from_code(3), RuExitCode::SystemError);
        assert_eq!(RuExitCode::from_code(4), RuExitCode::BadArgs);
        assert_eq!(RuExitCode::from_code(5), RuExitCode::Interrupted);
        assert_eq!(RuExitCode::from_code(99), RuExitCode::SystemError);
    }

    #[test]
    fn exit_code_is_success() {
        assert!(RuExitCode::Ok.is_success());
        assert!(RuExitCode::Partial.is_success());
        assert!(!RuExitCode::Conflicts.is_success());
        assert!(!RuExitCode::SystemError.is_success());
    }

    #[test]
    fn ru_sync_options_default() {
        let opts = RuSyncOptions::default();
        assert!(!opts.dry_run);
        assert!(!opts.clone_only);
        assert!(!opts.pull_only);
        assert!(!opts.autostash);
        assert!(!opts.rebase);
        assert!(opts.parallel.is_none());
        assert!(!opts.resume);
    }

    #[test]
    fn ru_client_default_path() {
        let client = RuClient::new();
        assert_eq!(client.ru_path(), "ru");
    }

    #[test]
    fn ru_client_with_explicit_path() {
        let client = RuClient::with_path(PathBuf::from("/usr/local/bin/ru"));
        assert_eq!(client.ru_path(), "/usr/local/bin/ru");
    }

    #[test]
    fn parse_sync_output_maps_repo_statuses() {
        let sample = r#"
        {
          "version": "1.2.0",
          "repos": [
            {"name":"alpha","path":"/data/projects/alpha","action":"pull","status":"updated","message":""},
            {"name":"beta","path":"/data/projects/beta","action":"clone","status":"cloned","message":""},
            {"name":"gamma","path":"/data/projects/gamma","action":"skip","status":"current","message":"Already up to date"},
            {"name":"delta","path":"/data/projects/delta","action":"pull","status":"conflict","message":"merge conflict"},
            {"name":"epsilon","path":"/data/projects/epsilon","action":"pull","status":"failed","message":"auth failed"}
          ]
        }
        "#;
        let result = parse_sync_output(sample, 2);
        assert_eq!(result.cloned, vec!["beta".to_string()]);
        assert_eq!(result.pulled, vec!["alpha".to_string()]);
        assert_eq!(result.skipped, vec!["gamma".to_string()]);
        assert_eq!(result.conflicts.len(), 1);
        assert_eq!(result.conflicts[0].repo, "delta");
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].repo, "epsilon");
    }

    #[test]
    fn parse_sync_output_uses_repo_fallbacks_and_actions() {
        let sample = r#"
        {
          "repos": [
            {"repo":"alpha","status":"updated"},
            {"name":"beta","status":"cloned"},
            {"path":"/data/projects/gamma","status":"current"},
            {"repo":"delta","action":"fail","message":"auth failed"}
          ]
        }
        "#;
        let result = parse_sync_output(sample, 1);
        assert_eq!(result.pulled, vec!["alpha".to_string()]);
        assert_eq!(result.cloned, vec!["beta".to_string()]);
        assert_eq!(result.skipped, vec!["gamma".to_string()]);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].repo, "delta");
        assert_eq!(result.errors[0].error, "auth failed");
    }

    #[test]
    fn parse_sync_output_unknown_status_defaults_to_skipped() {
        let sample = r#"
        {
          "repos": [
            {"repo":"alpha","status":"weird","message":""}
          ]
        }
        "#;
        let result = parse_sync_output(sample, 0);
        assert_eq!(result.skipped, vec!["alpha".to_string()]);
        assert!(result.errors.is_empty());
        assert!(result.conflicts.is_empty());
    }

    #[test]
    fn parse_sync_output_invalid_json_records_error() {
        let sample = "not json";
        let result = parse_sync_output(sample, 3);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].repo, "unknown");
        assert_eq!(result.errors[0].error, "not json");
    }
}
