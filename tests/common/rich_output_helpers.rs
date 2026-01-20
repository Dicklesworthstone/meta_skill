//! Rich output test helpers for integration tests.
//!
//! This module provides standalone utilities for testing rich terminal output
//! in integration and e2e tests. Unlike the internal `ms::output::test_utils`,
//! these helpers are designed to work without internal crate dependencies.
//!
//! # Features
//!
//! - Environment variable manipulation with RAII guards
//! - ANSI escape code detection and stripping
//! - Unicode detection (box drawing, emoji)
//! - Command execution with output capture
//! - JSON validation helpers
//! - Golden file testing utilities
//!
//! # Examples
//!
//! ```rust,ignore
//! use common::rich_output_helpers::*;
//!
//! #[test]
//! fn test_robot_mode_has_no_ansi() {
//!     let result = run_ms(&["--robot", "list"]);
//!     result.assert_success();
//!     assert_no_ansi(&result.stdout, "robot mode output");
//! }
//! ```

use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

// =============================================================================
// Environment Variable Manipulation
// =============================================================================

/// RAII guard for temporarily modifying environment variables.
///
/// Automatically restores original values when dropped.
///
/// # Thread Safety
///
/// Environment variable modifications are process-global. Use `#[serial]`
/// from `serial_test` crate when running tests that modify environment
/// variables in parallel.
///
/// # Examples
///
/// ```rust,ignore
/// // Set variables for the scope of the guard
/// let _guard = EnvGuard::new()
///     .set("NO_COLOR", "1")
///     .unset("COLORTERM");
///
/// // Variables are restored when guard drops
/// ```
#[derive(Debug)]
pub struct EnvGuard {
    original: Vec<(String, Option<OsString>)>,
}

impl EnvGuard {
    /// Create a new environment guard.
    #[must_use]
    pub fn new() -> Self {
        Self {
            original: Vec::new(),
        }
    }

    /// Set an environment variable.
    #[must_use]
    pub fn set(mut self, key: &str, value: &str) -> Self {
        self.original.push((key.to_string(), env::var_os(key)));
        // SAFETY: This is test-only code. Tests that modify environment variables
        // should use #[serial] to avoid races between concurrent tests.
        unsafe { env::set_var(key, value) };
        self
    }

    /// Unset an environment variable.
    #[must_use]
    pub fn unset(mut self, key: &str) -> Self {
        self.original.push((key.to_string(), env::var_os(key)));
        // SAFETY: This is test-only code. Tests that modify environment variables
        // should use #[serial] to avoid races between concurrent tests.
        unsafe { env::remove_var(key) };
        self
    }

    /// Set multiple environment variables.
    #[must_use]
    pub fn set_all(mut self, vars: &[(&str, &str)]) -> Self {
        for (k, v) in vars {
            self = self.set(k, v);
        }
        self
    }

    /// Configure for NO_COLOR mode (disables all styling).
    #[must_use]
    pub fn no_color(self) -> Self {
        self.set("NO_COLOR", "1").unset("COLORTERM").unset("MS_FORCE_RICH")
    }

    /// Configure for forced rich output.
    #[must_use]
    pub fn force_rich(self) -> Self {
        self.set("MS_FORCE_RICH", "1")
            .unset("NO_COLOR")
            .unset("MS_PLAIN_OUTPUT")
    }

    /// Configure for CI environment.
    #[must_use]
    pub fn ci_mode(self) -> Self {
        self.set("CI", "true").set("NO_COLOR", "1")
    }

    /// Configure for a specific AI agent.
    #[must_use]
    pub fn agent(self, agent_type: &str) -> Self {
        match agent_type {
            "claude" | "claude_code" => self.set("CLAUDE_CODE", "1"),
            "copilot" => self.set("GITHUB_COPILOT", "1"),
            "cursor" => self.set("CURSOR_AI", "1"),
            "windsurf" => self.set("WINDSURF_AI", "1"),
            "aider" => self.set("AIDER", "1"),
            "amazon_q" => self.set("AMAZON_Q", "1"),
            _ => self.set("AI_AGENT", "1"),
        }
    }
}

impl Default for EnvGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in self.original.iter().rev() {
            // SAFETY: We're restoring the original environment state during cleanup.
            // This is test-only code and tests should use #[serial] to avoid races.
            unsafe {
                match value {
                    Some(v) => env::set_var(key, v),
                    None => env::remove_var(key),
                }
            }
        }
    }
}

// =============================================================================
// ANSI Code Utilities
// =============================================================================

/// Regex pattern for ANSI escape sequences.
fn ansi_regex() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"\x1b\[[0-9;]*[A-Za-z@-~]").expect("valid regex"))
}

/// Check if a string contains ANSI escape codes.
///
/// # Examples
///
/// ```rust,ignore
/// assert!(contains_ansi("\x1b[31mred\x1b[0m"));
/// assert!(!contains_ansi("plain text"));
/// ```
#[must_use]
pub fn contains_ansi(s: &str) -> bool {
    s.contains('\x1b') && ansi_regex().is_match(s)
}

/// Strip all ANSI escape codes from a string.
///
/// # Examples
///
/// ```rust,ignore
/// assert_eq!(strip_ansi("\x1b[31mred\x1b[0m"), "red");
/// ```
#[must_use]
pub fn strip_ansi(s: &str) -> String {
    if !s.contains('\x1b') {
        return s.to_string();
    }
    ansi_regex().replace_all(s, "").into_owned()
}

/// Extract all ANSI codes from a string.
#[must_use]
pub fn extract_ansi_codes(s: &str) -> Vec<String> {
    if !s.contains('\x1b') {
        return Vec::new();
    }
    ansi_regex()
        .find_iter(s)
        .map(|m| m.as_str().to_string())
        .collect()
}

/// Assert that a string contains no ANSI codes.
///
/// # Panics
///
/// Panics with detailed error if ANSI codes are found.
pub fn assert_no_ansi(s: &str, context: &str) {
    if contains_ansi(s) {
        let codes = extract_ansi_codes(s);
        panic!(
            "Expected no ANSI codes in {context}\n\
             Found {} sequences: {:?}\n\
             Stripped content: {}",
            codes.len(),
            &codes[..codes.len().min(5)],
            strip_ansi(&s[..s.len().min(300)])
        );
    }
}

/// Assert that a string contains ANSI codes.
///
/// # Panics
///
/// Panics if no ANSI codes are found.
pub fn assert_has_ansi(s: &str, context: &str) {
    if !contains_ansi(s) {
        panic!(
            "Expected ANSI codes in {context}\n\
             Output: {:?}",
            &s[..s.len().min(300)]
        );
    }
}

// =============================================================================
// Unicode Detection
// =============================================================================

/// Check if string contains box drawing characters (U+2500-U+257F).
#[must_use]
pub fn contains_box_drawing(s: &str) -> bool {
    s.chars()
        .any(|c| (0x2500..=0x257F).contains(&(c as u32)))
}

/// Check if string contains emoji characters.
#[must_use]
pub fn contains_emoji(s: &str) -> bool {
    s.chars().any(|c| {
        let cp = c as u32;
        (0x1F600..=0x1F64F).contains(&cp)
            || (0x1F300..=0x1F5FF).contains(&cp)
            || (0x1F680..=0x1F6FF).contains(&cp)
            || (0x1F900..=0x1F9FF).contains(&cp)
            || (0x2600..=0x26FF).contains(&cp)
            || (0x2700..=0x27BF).contains(&cp)
    })
}

/// Check if string contains any rich formatting (ANSI, box drawing, emoji).
#[must_use]
pub fn contains_rich_formatting(s: &str) -> bool {
    contains_ansi(s) || contains_box_drawing(s) || contains_emoji(s)
}

/// Assert output is plain (no ANSI, no box drawing, no emoji).
///
/// # Panics
///
/// Panics if any rich formatting is found.
pub fn assert_plain_output(s: &str, context: &str) {
    if contains_ansi(s) {
        panic!(
            "Expected plain output in {context}, found ANSI codes\n\
             Output: {:?}",
            &s[..s.len().min(300)]
        );
    }
    if contains_box_drawing(s) {
        panic!(
            "Expected plain output in {context}, found box drawing chars\n\
             Output: {:?}",
            &s[..s.len().min(300)]
        );
    }
    if contains_emoji(s) {
        panic!(
            "Expected plain output in {context}, found emoji\n\
             Output: {:?}",
            &s[..s.len().min(300)]
        );
    }
}

// =============================================================================
// Command Execution
// =============================================================================

/// Result from running the ms command.
#[derive(Debug, Clone)]
pub struct MsCommandResult {
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// Exit code
    pub exit_code: i32,
    /// Whether command succeeded
    pub success: bool,
    /// Execution duration
    pub duration: Duration,
    /// Command that was run (for error messages)
    pub command: String,
}

impl MsCommandResult {
    /// Parse stdout as JSON.
    ///
    /// # Panics
    ///
    /// Panics if stdout is not valid JSON.
    #[must_use]
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.stdout).unwrap_or_else(|e| {
            panic!(
                "Failed to parse stdout as JSON: {e}\n\
                 Command: {}\n\
                 stdout: {}\n\
                 stderr: {}",
                self.command, self.stdout, self.stderr
            );
        })
    }

    /// Try to parse stdout as JSON.
    #[must_use]
    pub fn try_json(&self) -> Option<serde_json::Value> {
        serde_json::from_str(&self.stdout).ok()
    }

    /// Check if stdout contains pattern.
    #[must_use]
    pub fn stdout_contains(&self, pattern: &str) -> bool {
        self.stdout.contains(pattern)
    }

    /// Check if stderr contains pattern.
    #[must_use]
    pub fn stderr_contains(&self, pattern: &str) -> bool {
        self.stderr.contains(pattern)
    }

    /// Assert command succeeded.
    pub fn assert_success(&self) {
        assert!(
            self.success,
            "Command failed: {}\n\
             Exit code: {}\n\
             stdout: {}\n\
             stderr: {}",
            self.command, self.exit_code, self.stdout, self.stderr
        );
    }

    /// Assert command failed.
    pub fn assert_failure(&self) {
        assert!(
            !self.success,
            "Command succeeded but expected failure: {}\n\
             stdout: {}\n\
             stderr: {}",
            self.command, self.stdout, self.stderr
        );
    }

    /// Assert stdout has no ANSI codes.
    pub fn assert_plain_stdout(&self) {
        assert_no_ansi(&self.stdout, &format!("stdout of `{}`", self.command));
    }

    /// Assert stdout has ANSI codes.
    pub fn assert_rich_stdout(&self) {
        assert_has_ansi(&self.stdout, &format!("stdout of `{}`", self.command));
    }

    /// Assert output is robot-friendly (no rich formatting).
    pub fn assert_robot_output(&self) {
        assert_plain_output(
            &self.stdout,
            &format!("robot output of `{}`", self.command),
        );
    }
}

/// Find the ms binary path.
fn find_ms_binary() -> PathBuf {
    // Check MS_BIN env var first
    if let Ok(path) = env::var("MS_BIN") {
        return PathBuf::from(path);
    }

    // Try cargo target directories
    let manifest_dir = env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));

    // Check release first, then debug
    for profile in ["release", "debug"] {
        let path = manifest_dir.join("target").join(profile).join("ms");
        if path.exists() {
            return path;
        }
    }

    // Fallback to debug path (will error at runtime if not found)
    manifest_dir.join("target").join("debug").join("ms")
}

/// Run the ms command with given arguments.
///
/// # Examples
///
/// ```rust,ignore
/// let result = run_ms(&["--version"]);
/// result.assert_success();
/// ```
#[must_use]
pub fn run_ms(args: &[&str]) -> MsCommandResult {
    run_ms_with_env(args, &[])
}

/// Run ms command with custom environment variables.
///
/// # Examples
///
/// ```rust,ignore
/// let result = run_ms_with_env(&["list"], &[("NO_COLOR", "1")]);
/// result.assert_success();
/// result.assert_plain_stdout();
/// ```
#[must_use]
pub fn run_ms_with_env(args: &[&str], env_vars: &[(&str, &str)]) -> MsCommandResult {
    let ms_path = find_ms_binary();
    let command = format!("ms {}", args.join(" "));
    let start = Instant::now();

    let mut cmd = Command::new(&ms_path);
    cmd.args(args);

    for (key, value) in env_vars {
        cmd.env(key, value);
    }

    let output = cmd.output().unwrap_or_else(|e| {
        panic!("Failed to execute {ms_path:?}: {e}");
    });

    MsCommandResult {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
        success: output.status.success(),
        duration: start.elapsed(),
        command,
    }
}

/// Run ms in robot mode (--robot flag).
#[must_use]
pub fn run_ms_robot(args: &[&str]) -> MsCommandResult {
    let mut full_args = vec!["--robot"];
    full_args.extend(args);
    run_ms(&full_args)
}

/// Run ms with NO_COLOR set.
#[must_use]
pub fn run_ms_no_color(args: &[&str]) -> MsCommandResult {
    run_ms_with_env(args, &[("NO_COLOR", "1")])
}

/// Run ms with forced rich output.
#[must_use]
pub fn run_ms_force_rich(args: &[&str]) -> MsCommandResult {
    run_ms_with_env(args, &[("MS_FORCE_RICH", "1")])
}

// =============================================================================
// JSON Validation
// =============================================================================

/// Assert string is valid JSON and return parsed value.
///
/// # Panics
///
/// Panics with helpful error message if JSON is invalid.
#[must_use]
pub fn assert_valid_json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap_or_else(|e| {
        let line = e.line();
        let lines: Vec<_> = s.lines().collect();
        let context = if line > 0 && line <= lines.len() {
            let start = line.saturating_sub(2);
            let end = (line + 1).min(lines.len());
            lines[start..end]
                .iter()
                .enumerate()
                .map(|(i, l)| format!("{}: {l}", start + i + 1))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            s[..s.len().min(500)].to_string()
        };

        panic!("Invalid JSON at line {line}: {e}\nContext:\n{context}");
    })
}

/// Assert JSON has expected keys.
pub fn assert_json_has_keys(json: &serde_json::Value, keys: &[&str]) {
    let obj = json.as_object().expect("Expected JSON object");
    for key in keys {
        assert!(
            obj.contains_key(*key),
            "Missing key '{}' in JSON. Keys: {:?}",
            key,
            obj.keys().collect::<Vec<_>>()
        );
    }
}

/// Assert JSON value equals expected.
pub fn assert_json_eq(json: &serde_json::Value, path: &str, expected: &serde_json::Value) {
    let actual = json.pointer(path).unwrap_or_else(|| {
        panic!("JSON path '{path}' not found in {json}");
    });
    assert_eq!(
        actual, expected,
        "JSON mismatch at '{path}':\n  expected: {expected}\n  actual: {actual}"
    );
}

// =============================================================================
// Golden File Testing
// =============================================================================

/// Compare output against golden file.
///
/// Set `UPDATE_GOLDEN=1` to update golden files.
///
/// # Panics
///
/// Panics if output doesn't match golden file.
pub fn assert_golden(output: &str, path: &str) {
    use std::fs;
    use std::path::Path;

    let golden_path = Path::new(path);

    if env::var("UPDATE_GOLDEN").is_ok() {
        if let Some(parent) = golden_path.parent() {
            fs::create_dir_all(parent).expect("Failed to create golden dir");
        }
        fs::write(golden_path, output).expect("Failed to write golden file");
        eprintln!("Updated golden file: {path}");
        return;
    }

    let expected = fs::read_to_string(golden_path).unwrap_or_else(|_| {
        panic!(
            "Golden file not found: {path}\n\
             Run with UPDATE_GOLDEN=1 to create.\n\
             Actual output:\n{output}"
        );
    });

    if output != expected {
        let diff_line = output
            .lines()
            .zip(expected.lines())
            .position(|(a, b)| a != b)
            .map(|i| i + 1)
            .unwrap_or(0);

        panic!(
            "Output doesn't match golden file: {path}\n\
             First diff at line {diff_line}\n\
             Run with UPDATE_GOLDEN=1 to update."
        );
    }
}

/// Strip ANSI and compare against golden file.
///
/// Useful for testing content without worrying about styling changes.
pub fn assert_golden_stripped(output: &str, path: &str) {
    assert_golden(&strip_ansi(output), path);
}

// =============================================================================
// Test Scenario Helpers
// =============================================================================

/// Test scenario configuration.
#[derive(Debug, Clone)]
pub struct OutputScenario {
    /// Scenario name
    pub name: &'static str,
    /// Arguments to pass to ms
    pub args: Vec<&'static str>,
    /// Environment variables to set
    pub env: Vec<(&'static str, &'static str)>,
    /// Whether rich output is expected
    pub expect_rich: bool,
}

impl OutputScenario {
    /// Create a scenario expecting plain output.
    #[must_use]
    pub fn plain(name: &'static str, args: &[&'static str]) -> Self {
        Self {
            name,
            args: args.to_vec(),
            env: Vec::new(),
            expect_rich: false,
        }
    }

    /// Create a scenario expecting rich output.
    #[must_use]
    pub fn rich(name: &'static str, args: &[&'static str]) -> Self {
        Self {
            name,
            args: args.to_vec(),
            env: Vec::new(),
            expect_rich: true,
        }
    }

    /// Add environment variables.
    #[must_use]
    pub fn with_env(mut self, env: &[(&'static str, &'static str)]) -> Self {
        self.env = env.to_vec();
        self
    }

    /// Run the scenario and verify output.
    pub fn run(&self) -> MsCommandResult {
        let result = run_ms_with_env(&self.args, &self.env);

        if self.expect_rich {
            if !contains_ansi(&result.stdout) && result.success {
                panic!(
                    "Scenario '{}': Expected rich output but got plain\n\
                     stdout: {:?}",
                    self.name,
                    &result.stdout[..result.stdout.len().min(300)]
                );
            }
        } else if contains_ansi(&result.stdout) {
            panic!(
                "Scenario '{}': Expected plain output but got rich\n\
                 ANSI codes: {:?}\n\
                 Stripped: {}",
                self.name,
                extract_ansi_codes(&result.stdout)[..5.min(extract_ansi_codes(&result.stdout).len())]
                    .to_vec(),
                strip_ansi(&result.stdout[..result.stdout.len().min(300)])
            );
        }

        result
    }
}

/// Common test scenarios for output format testing.
pub fn standard_output_scenarios() -> Vec<OutputScenario> {
    vec![
        // Plain output scenarios
        OutputScenario::plain("robot_flag", &["--robot", "list"])
            .with_env(&[]),
        OutputScenario::plain("no_color_env", &["list"])
            .with_env(&[("NO_COLOR", "1")]),
        OutputScenario::plain("plain_output_env", &["list"])
            .with_env(&[("MS_PLAIN_OUTPUT", "1")]),
        OutputScenario::plain("json_format", &["--output", "json", "list"])
            .with_env(&[]),
        // Rich output scenarios (when terminal is available)
        OutputScenario::rich("force_rich", &["list"])
            .with_env(&[("MS_FORCE_RICH", "1")]),
    ]
}

// =============================================================================
// Tests for the helpers themselves
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_guard() {
        let key = "TEST_RICH_OUTPUT_HELPER";
        // SAFETY: Test-only code, tests should use #[serial]
        unsafe { env::remove_var(key) };

        {
            let _guard = EnvGuard::new().set(key, "value");
            assert_eq!(env::var(key).unwrap(), "value");
        }

        assert!(env::var(key).is_err(), "Should be unset after drop");
    }

    #[test]
    fn test_contains_ansi() {
        assert!(contains_ansi("\x1b[31mred\x1b[0m"));
        assert!(!contains_ansi("plain"));
    }

    #[test]
    fn test_strip_ansi() {
        assert_eq!(strip_ansi("\x1b[31mred\x1b[0m"), "red");
        assert_eq!(strip_ansi("plain"), "plain");
    }

    #[test]
    fn test_contains_box_drawing() {
        assert!(contains_box_drawing("┌─┐"));
        assert!(!contains_box_drawing("+-+"));
    }

    #[test]
    fn test_contains_emoji() {
        assert!(contains_emoji("test 🎉"));
        assert!(!contains_emoji("test"));
    }

    #[test]
    fn test_assert_valid_json() {
        let json = assert_valid_json(r#"{"key": "value"}"#);
        assert_eq!(json["key"], "value");
    }
}
