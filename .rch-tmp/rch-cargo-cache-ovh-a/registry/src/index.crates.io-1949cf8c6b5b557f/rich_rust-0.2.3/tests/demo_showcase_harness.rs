//! E2E test harness for demo_showcase binary.
//!
//! Provides utilities for running the demo_showcase binary in integration tests
//! with timeout enforcement, output capture, and diagnostic logging.
//!
//! # Features
//!
//! - Automatic binary location via `CARGO_BIN_EXE_demo_showcase`
//! - Configurable timeouts with hard kill
//! - Stdout/stderr capture
//! - Structured logging of test runs
//! - Diagnostic output on failures
//!
//! # Example
//!
//! ```rust,ignore
//! use demo_showcase_harness::{DemoRunner, RunResult};
//!
//! #[test]
//! fn test_list_scenes() {
//!     let result = DemoRunner::new()
//!         .arg("--list-scenes")
//!         .timeout_secs(10)
//!         .run()
//!         .expect("should run");
//!
//!     assert!(result.success());
//!     assert!(result.stdout.contains("hero"));
//! }
//! ```

#![allow(dead_code)]

use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// Default timeout for demo_showcase runs (30 seconds).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Quick timeout for simple operations (5 seconds).
pub const QUICK_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum bytes to capture from stdout/stderr on failure for diagnostics.
const MAX_DIAGNOSTIC_BYTES: usize = 4096;

/// Maximum lines to show in diagnostic output.
const MAX_DIAGNOSTIC_LINES: usize = 50;

/// Result of running demo_showcase.
#[derive(Debug, Clone)]
pub struct RunResult {
    /// Exit status (None if timed out and killed).
    pub status: Option<ExitStatus>,
    /// Captured stdout.
    pub stdout: String,
    /// Captured stderr.
    pub stderr: String,
    /// Elapsed time.
    pub elapsed: Duration,
    /// Whether the process was killed due to timeout.
    pub timed_out: bool,
    /// Arguments used.
    pub args: Vec<String>,
    /// Environment overrides used.
    pub env_overrides: HashMap<String, String>,
}

impl RunResult {
    /// Check if the run was successful (exit code 0, no timeout).
    #[must_use]
    pub fn success(&self) -> bool {
        !self.timed_out && self.status.is_some_and(|s| s.success())
    }

    /// Get the exit code if available.
    #[must_use]
    pub fn exit_code(&self) -> Option<i32> {
        self.status.and_then(|s| s.code())
    }

    /// Check if stdout contains a string.
    #[must_use]
    pub fn stdout_contains(&self, needle: &str) -> bool {
        self.stdout.contains(needle)
    }

    /// Check if stderr contains a string.
    #[must_use]
    pub fn stderr_contains(&self, needle: &str) -> bool {
        self.stderr.contains(needle)
    }

    /// Get diagnostic summary for logging.
    #[must_use]
    pub fn diagnostic_summary(&self) -> String {
        let status_str = if self.timed_out {
            "TIMEOUT".to_string()
        } else if let Some(code) = self.exit_code() {
            format!("exit={code}")
        } else {
            "killed".to_string()
        };

        format!(
            "{} in {:?} | stdout={} bytes | stderr={} bytes | args={:?}",
            status_str,
            self.elapsed,
            self.stdout.len(),
            self.stderr.len(),
            self.args,
        )
    }

    /// Get truncated diagnostic output for CI logs.
    #[must_use]
    pub fn diagnostic_output(&self) -> String {
        let mut output = String::new();

        output.push_str(&format!(
            "=== Run Summary ===\n{}\n\n",
            self.diagnostic_summary()
        ));

        if !self.stdout.is_empty() {
            output.push_str("=== STDOUT (truncated) ===\n");
            output.push_str(&truncate_output(&self.stdout, MAX_DIAGNOSTIC_LINES));
            output.push('\n');
        }

        if !self.stderr.is_empty() {
            output.push_str("=== STDERR (truncated) ===\n");
            output.push_str(&truncate_output(&self.stderr, MAX_DIAGNOSTIC_LINES));
            output.push('\n');
        }

        output
    }
}

/// Error type for runner operations.
#[derive(Debug)]
pub enum RunError {
    /// Failed to spawn the process.
    SpawnFailed(std::io::Error),
    /// Binary not found.
    BinaryNotFound(String),
    /// Failed to read output.
    OutputReadFailed(std::io::Error),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SpawnFailed(err) => write!(f, "Failed to spawn demo_showcase: {err}"),
            Self::BinaryNotFound(path) => write!(f, "Binary not found: {path}"),
            Self::OutputReadFailed(err) => write!(f, "Failed to read output: {err}"),
        }
    }
}

impl std::error::Error for RunError {}

/// Builder for running demo_showcase with various configurations.
#[derive(Debug, Clone)]
pub struct DemoRunner {
    /// Arguments to pass to the binary.
    args: Vec<String>,
    /// Environment variable overrides.
    env_overrides: HashMap<String, String>,
    /// Timeout duration.
    timeout: Duration,
    /// Working directory (None = current).
    working_dir: Option<PathBuf>,
}

impl Default for DemoRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl DemoRunner {
    /// Create a new runner with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            args: Vec::new(),
            env_overrides: HashMap::new(),
            timeout: DEFAULT_TIMEOUT,
            working_dir: None,
        }
    }

    /// Create a runner configured for quick tests (shorter timeout, --quick flag).
    #[must_use]
    pub fn quick() -> Self {
        Self::new().arg("--quick").timeout(QUICK_TIMEOUT)
    }

    /// Add an argument.
    #[must_use]
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Add multiple arguments.
    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Set an environment variable.
    #[must_use]
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_overrides.insert(key.into(), value.into());
        self
    }

    /// Set the timeout duration.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the timeout in seconds.
    #[must_use]
    pub fn timeout_secs(self, secs: u64) -> Self {
        self.timeout(Duration::from_secs(secs))
    }

    /// Set the working directory.
    #[must_use]
    pub fn working_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    /// Disable colors (set NO_COLOR=1).
    #[must_use]
    pub fn no_color(self) -> Self {
        self.env("NO_COLOR", "1")
    }

    /// Force terminal mode.
    #[must_use]
    pub fn force_terminal(self) -> Self {
        self.arg("--force-terminal")
    }

    /// Set a fixed terminal width.
    #[must_use]
    pub fn width(self, width: usize) -> Self {
        self.arg("--width").arg(width.to_string())
    }

    /// Run in non-interactive mode.
    #[must_use]
    pub fn non_interactive(self) -> Self {
        self.arg("--no-interactive")
            .arg("--no-live")
            .arg("--no-screen")
    }

    /// Get the path to the demo_showcase binary.
    fn binary_path() -> Result<PathBuf, RunError> {
        // Try CARGO_BIN_EXE first (set during cargo test)
        if let Ok(path) = std::env::var("CARGO_BIN_EXE_demo_showcase") {
            let path = PathBuf::from(path);
            if path.exists() {
                return Ok(path);
            }
        }

        // Fallback to target/debug or target/release
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."));

        for profile in ["debug", "release"] {
            let path = manifest_dir
                .join("target")
                .join(profile)
                .join("demo_showcase");
            if path.exists() {
                return Ok(path);
            }
            // Windows
            let path_exe = path.with_extension("exe");
            if path_exe.exists() {
                return Ok(path_exe);
            }
        }

        Err(RunError::BinaryNotFound(
            "demo_showcase binary not found. Run `cargo build --bin demo_showcase` first.".into(),
        ))
    }

    /// Run the demo_showcase binary and capture output.
    pub fn run(&self) -> Result<RunResult, RunError> {
        let binary = Self::binary_path()?;

        // Log the run
        tracing::info!(
            binary = %binary.display(),
            args = ?self.args,
            env = ?self.env_overrides,
            timeout = ?self.timeout,
            "Starting demo_showcase run"
        );

        let start = Instant::now();

        // Build command
        let mut cmd = Command::new(&binary);
        cmd.args(&self.args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Apply environment overrides
        for (key, value) in &self.env_overrides {
            cmd.env(key, value);
        }

        // Set working directory if specified
        if let Some(ref dir) = self.working_dir {
            cmd.current_dir(dir);
        }

        // Spawn process
        let mut child = cmd.spawn().map_err(RunError::SpawnFailed)?;

        // Wait with timeout
        let (status, timed_out) = wait_with_timeout(&mut child, self.timeout);

        let elapsed = start.elapsed();

        // Capture output
        let stdout = read_output(child.stdout.take()).map_err(RunError::OutputReadFailed)?;
        let stderr = read_output(child.stderr.take()).map_err(RunError::OutputReadFailed)?;

        let result = RunResult {
            status,
            stdout,
            stderr,
            elapsed,
            timed_out,
            args: self.args.clone(),
            env_overrides: self.env_overrides.clone(),
        };

        // Log result
        if result.success() {
            tracing::info!(
                elapsed = ?elapsed,
                stdout_bytes = result.stdout.len(),
                stderr_bytes = result.stderr.len(),
                "demo_showcase completed successfully"
            );
        } else {
            tracing::warn!(
                elapsed = ?elapsed,
                timed_out = timed_out,
                exit_code = ?result.exit_code(),
                stdout_bytes = result.stdout.len(),
                stderr_bytes = result.stderr.len(),
                "demo_showcase failed or timed out"
            );

            // Log diagnostic output at debug level
            tracing::debug!(diagnostic = %result.diagnostic_output(), "Run diagnostics");
        }

        Ok(result)
    }
}

/// Wait for a child process with timeout, killing if necessary.
fn wait_with_timeout(child: &mut Child, timeout: Duration) -> (Option<ExitStatus>, bool) {
    let start = Instant::now();
    let poll_interval = Duration::from_millis(50);

    loop {
        // Check if process has exited
        match child.try_wait() {
            Ok(Some(status)) => return (Some(status), false),
            Ok(None) => {
                // Still running, check timeout
                if start.elapsed() >= timeout {
                    tracing::warn!(pid = ?child.id(), "Process timed out, killing");
                    let _ = child.kill();
                    // Wait for the killed process
                    let status = child.wait().ok();
                    return (status, true);
                }
                std::thread::sleep(poll_interval);
            }
            Err(err) => {
                tracing::error!(error = %err, "Error checking process status");
                return (None, false);
            }
        }
    }
}

/// Read all output from an optional reader.
fn read_output<R: Read>(reader: Option<R>) -> std::io::Result<String> {
    let Some(mut reader) = reader else {
        return Ok(String::new());
    };

    let mut buf = Vec::new();
    reader.read_to_end(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// Truncate output to a maximum number of lines, showing head and tail.
fn truncate_output(output: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = output.lines().collect();

    if lines.len() <= max_lines {
        return output.to_string();
    }

    let head_count = max_lines / 2;
    let tail_count = max_lines - head_count;
    let skipped = lines.len() - max_lines;

    let mut result = String::new();
    for line in lines.iter().take(head_count) {
        result.push_str(line);
        result.push('\n');
    }
    result.push_str(&format!("... ({skipped} lines skipped) ...\n"));
    for line in lines.iter().skip(lines.len() - tail_count) {
        result.push_str(line);
        result.push('\n');
    }

    result
}

/// Assertion helpers for test results.
pub mod assertions {
    use super::RunResult;

    /// Assert that the run was successful.
    #[track_caller]
    pub fn assert_success(result: &RunResult) {
        if !result.success() {
            panic!(
                "Expected successful run, but got:\n{}",
                result.diagnostic_output()
            );
        }
    }

    /// Assert that the run failed (non-zero exit or timeout).
    #[track_caller]
    pub fn assert_failure(result: &RunResult) {
        if result.success() {
            panic!(
                "Expected failed run, but got success:\n{}",
                result.diagnostic_output()
            );
        }
    }

    /// Assert that stdout contains the given string.
    #[track_caller]
    pub fn assert_stdout_contains(result: &RunResult, needle: &str) {
        if !result.stdout_contains(needle) {
            panic!(
                "Expected stdout to contain {needle:?}, but it didn't:\n{}",
                result.diagnostic_output()
            );
        }
    }

    /// Assert that stderr contains the given string.
    #[track_caller]
    pub fn assert_stderr_contains(result: &RunResult, needle: &str) {
        if !result.stderr_contains(needle) {
            panic!(
                "Expected stderr to contain {needle:?}, but it didn't:\n{}",
                result.diagnostic_output()
            );
        }
    }

    /// Assert that the run did not time out.
    #[track_caller]
    pub fn assert_no_timeout(result: &RunResult) {
        if result.timed_out {
            panic!(
                "Expected run to complete without timeout:\n{}",
                result.diagnostic_output()
            );
        }
    }

    /// Assert that the run completed within a duration.
    #[track_caller]
    pub fn assert_elapsed_under(result: &RunResult, max: std::time::Duration) {
        if result.elapsed > max {
            panic!(
                "Expected run to complete in {:?}, but took {:?}:\n{}",
                max,
                result.elapsed,
                result.diagnostic_output()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_output_short() {
        let output = "line1\nline2\nline3";
        let result = truncate_output(output, 10);
        assert_eq!(result, output.to_string());
    }

    #[test]
    fn test_truncate_output_long() {
        let lines: Vec<String> = (1..=100).map(|i| format!("line {i}")).collect();
        let output = lines.join("\n");
        let result = truncate_output(&output, 10);

        assert!(result.contains("line 1"));
        assert!(result.contains("line 5"));
        assert!(result.contains("skipped"));
        assert!(result.contains("line 96"));
        assert!(result.contains("line 100"));
    }

    #[test]
    fn test_runner_builder() {
        let runner = DemoRunner::new()
            .arg("--list-scenes")
            .env("NO_COLOR", "1")
            .timeout_secs(10);

        assert_eq!(runner.args, vec!["--list-scenes"]);
        assert_eq!(runner.env_overrides.get("NO_COLOR"), Some(&"1".to_string()));
        assert_eq!(runner.timeout, Duration::from_secs(10));
    }

    #[test]
    fn test_quick_runner() {
        let runner = DemoRunner::quick();
        assert!(runner.args.contains(&"--quick".to_string()));
        assert_eq!(runner.timeout, QUICK_TIMEOUT);
    }

    #[test]
    fn test_run_result_success_check() {
        use std::os::unix::process::ExitStatusExt;

        let result = RunResult {
            status: Some(ExitStatus::from_raw(0)),
            stdout: String::new(),
            stderr: String::new(),
            elapsed: Duration::from_millis(100),
            timed_out: false,
            args: vec![],
            env_overrides: HashMap::new(),
        };

        assert!(result.success());
    }

    #[test]
    fn test_run_result_timeout_not_success() {
        let result = RunResult {
            status: None,
            stdout: String::new(),
            stderr: String::new(),
            elapsed: Duration::from_secs(30),
            timed_out: true,
            args: vec![],
            env_overrides: HashMap::new(),
        };

        assert!(!result.success());
    }
}
