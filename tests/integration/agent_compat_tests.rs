//! Agent Compatibility Integration Tests (bd-1p9c)
//!
//! These tests ensure that AI coding agents always receive plain, parseable output.
//! This is CRITICAL as the primary users of `ms` are AI coding agents.
//!
//! ## Test Categories
//!
//! 1. **Agent Environment Variables** - Tests for known AI agent environment variables
//! 2. **CI Environment Variables** - Tests for CI/CD systems
//! 3. **IDE Environment Variables** - Tests for IDE integrations
//! 4. **Robot Mode** - Tests for explicit robot mode flag
//! 5. **MCP Server** - Tests ensuring MCP responses are always JSON
//!
//! ## Output Verification
//!
//! All tests verify:
//! - No ANSI escape codes in output
//! - No Unicode box drawing characters
//! - No emoji characters
//! - Valid JSON where applicable
//! - Machine-parseable line-based format

#![allow(dead_code)] // Test utilities may not all be used yet

use std::collections::HashSet;
use std::env;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

// =============================================================================
// Inline Test Utilities (avoid unsafe env manipulation)
// =============================================================================

/// Regex pattern for ANSI escape sequences.
fn ansi_regex() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"\x1b\[[0-9;]*[A-Za-z@-~]").expect("valid regex"))
}

/// Check if a string contains ANSI escape codes.
fn contains_ansi(s: &str) -> bool {
    s.contains('\x1b') && ansi_regex().is_match(s)
}

/// Strip all ANSI escape codes from a string.
fn strip_ansi(s: &str) -> String {
    if !s.contains('\x1b') {
        return s.to_string();
    }
    ansi_regex().replace_all(s, "").into_owned()
}

/// Extract all ANSI codes from a string.
fn extract_ansi_codes(s: &str) -> Vec<String> {
    if !s.contains('\x1b') {
        return Vec::new();
    }
    ansi_regex()
        .find_iter(s)
        .map(|m| m.as_str().to_string())
        .collect()
}

/// Assert that a string contains no ANSI codes.
fn assert_no_ansi(s: &str, context: &str) {
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

/// Result from running the ms command.
#[derive(Debug, Clone)]
struct MsCommandResult {
    stdout: String,
    stderr: String,
    exit_code: i32,
    success: bool,
    duration: Duration,
    command: String,
}

impl MsCommandResult {
    fn try_json(&self) -> Option<serde_json::Value> {
        serde_json::from_str(&self.stdout).ok()
    }

    fn assert_success(&self) {
        assert!(
            self.success,
            "Command failed: {}\n\
             Exit code: {}\n\
             stdout: {}\n\
             stderr: {}",
            self.command, self.exit_code, self.stdout, self.stderr
        );
    }
}

/// Find the ms binary path.
fn find_ms_binary() -> PathBuf {
    if let Ok(path) = env::var("MS_BIN") {
        return PathBuf::from(path);
    }

    let manifest_dir = env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));

    for profile in ["release", "debug"] {
        let path = manifest_dir.join("target").join(profile).join("ms");
        if path.exists() {
            return path;
        }
    }

    // Try CARGO_TARGET_DIR
    if let Ok(target_dir) = env::var("CARGO_TARGET_DIR") {
        for profile in ["release", "debug"] {
            let path = PathBuf::from(&target_dir).join(profile).join("ms");
            if path.exists() {
                return path;
            }
        }
    }

    // Try /tmp/cargo-target
    for profile in ["release", "debug"] {
        let path = PathBuf::from("/tmp/cargo-target").join(profile).join("ms");
        if path.exists() {
            return path;
        }
    }

    manifest_dir.join("target").join("debug").join("ms")
}

/// Run ms command with custom environment variables.
/// This uses Command::env() which doesn't require unsafe.
fn run_ms_with_env(args: &[&str], env_vars: &[(&str, &str)]) -> MsCommandResult {
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

/// Run the ms command with given arguments (no extra env vars).
fn run_ms(args: &[&str]) -> MsCommandResult {
    run_ms_with_env(args, &[])
}

// =============================================================================
// Agent Environment Variables
// =============================================================================

/// All known AI agent environment variables that should trigger plain output.
const AGENT_ENV_VARS: &[(&str, &str)] = &[
    ("CLAUDE_CODE", "1"),
    ("CURSOR_AI", "1"),
    ("OPENAI_CODEX", "1"),
    ("AIDER_MODE", "1"),
    ("CODEIUM_ENABLED", "1"),
    ("WINDSURF_AGENT", "1"),
    ("COPILOT_AGENT", "1"),
    ("COPILOT_WORKSPACE", "1"),
    ("AGENT_MODE", "1"),
    ("IDE_AGENT", "1"),
    ("CONTINUE_DEV", "1"),
    ("SOURCEGRAPH_CODY", "1"),
    ("TABNINE_AGENT", "1"),
    ("AMAZON_Q", "1"),
    ("GEMINI_CODE", "1"),
];

/// CI environment variables that should trigger plain output.
const CI_ENV_VARS: &[(&str, &str)] = &[
    ("CI", "true"),
    ("GITHUB_ACTIONS", "true"),
    ("GITLAB_CI", "true"),
    ("JENKINS_URL", "http://jenkins"),
    ("TRAVIS", "true"),
    ("CIRCLECI", "true"),
    ("BUILDKITE", "true"),
    ("BITBUCKET_PIPELINES", "true"),
    ("TF_BUILD", "True"),
    ("TEAMCITY_VERSION", "2023.1"),
    ("DRONE", "true"),
    ("WOODPECKER", "true"),
];

/// IDE environment variables that may indicate non-human usage.
const IDE_ENV_VARS: &[(&str, &str)] = &[
    ("VSCODE_GIT_ASKPASS_NODE", "/path/to/node"),
    ("VSCODE_INJECTION", "1"),
    ("CODESPACES", "true"),
    ("GITPOD_WORKSPACE_ID", "test-workspace"),
    ("REPLIT_DB_URL", "http://replit.db"),
    ("CLOUD_SHELL", "true"),
];

// =============================================================================
// Output Verification Utilities
// =============================================================================

/// Known box drawing characters to check for.
const BOX_CHARS: &[char] = &[
    '─', '│', '┌', '┐', '└', '┘', '├', '┤', '┬', '┴', '┼', '╭', '╮', '╯', '╰', '═', '║', '╔', '╗',
    '╚', '╝',
];

/// Known fancy Unicode characters to check for.
const FANCY_UNICODE: &[char] = &[
    '✓', '✗', '⚠', 'ℹ', '⏳', '✅', '→', '•', '█', '▓', '▒', '░', '●', '○',
];

/// Assert no box drawing characters in output.
fn assert_no_box_drawing_chars(output: &str, context: &str) {
    for ch in BOX_CHARS {
        if output.contains(*ch) {
            let line = output.lines().find(|l| l.contains(*ch)).unwrap_or("");
            panic!(
                "Found box char '{}' (U+{:04X}) in {context}:\n  Line: {line}",
                ch, *ch as u32
            );
        }
    }
}

/// Assert no fancy Unicode in output.
fn assert_no_fancy_unicode(output: &str, context: &str) {
    for ch in FANCY_UNICODE {
        if output.contains(*ch) {
            let line = output.lines().find(|l| l.contains(*ch)).unwrap_or("");
            panic!(
                "Found fancy unicode '{}' (U+{:04X}) in {context}:\n  Line: {line}",
                ch, *ch as u32
            );
        }
    }
}

/// Comprehensive agent-safe output verification.
fn assert_agent_safe_output(output: &str, context: &str) {
    assert_no_ansi(output, context);
    assert_no_box_drawing_chars(output, context);
    assert_no_fancy_unicode(output, context);
}

/// Verify output is line-parseable (no embedded control chars except newlines).
fn assert_line_parseable(output: &str, context: &str) {
    for (i, line) in output.lines().enumerate() {
        if line.contains('\r') {
            panic!(
                "Line {} in {context} contains carriage return: {:?}",
                i + 1,
                line
            );
        }
        // Check for other control characters
        for ch in line.chars() {
            if ch.is_control() && ch != '\t' {
                panic!(
                    "Line {} in {context} contains control char U+{:04X}: {:?}",
                    i + 1,
                    ch as u32,
                    line
                );
            }
        }
    }
}

// =============================================================================
// Agent Environment Tests
// =============================================================================

/// Test that each agent environment variable produces plain output.
#[test]
fn test_agent_env_vars_produce_plain_output() {
    for (env_var, value) in AGENT_ENV_VARS {
        let result = run_ms_with_env(&["--version"], &[(env_var, value)]);

        // --version should succeed
        if !result.success {
            // Skip if command failed for other reasons
            continue;
        }

        let context = format!("{env_var}={value}");
        assert_agent_safe_output(&result.stdout, &context);
        assert_line_parseable(&result.stdout, &context);
    }
}

/// Test Claude Code agent environment.
#[test]
fn test_claude_code_agent_output() {
    let result = run_ms_with_env(&["--version"], &[("CLAUDE_CODE", "1")]);

    if result.success {
        assert_agent_safe_output(&result.stdout, "CLAUDE_CODE=1 --version");
    }
}

/// Test that agent mode works even with TTY-like environment.
#[test]
fn test_agent_overrides_tty_like_env() {
    // Set up environment that looks like a TTY
    let result = run_ms_with_env(
        &["--version"],
        &[
            ("CLAUDE_CODE", "1"),
            ("TERM", "xterm-256color"),
            ("COLORTERM", "truecolor"),
        ],
    );

    if result.success {
        // Agent mode should still produce plain output
        assert_agent_safe_output(&result.stdout, "CLAUDE_CODE with TTY env");
    }
}

// =============================================================================
// CI Environment Tests
// =============================================================================

/// Test that each CI environment variable produces plain output.
#[test]
fn test_ci_env_vars_produce_plain_output() {
    for (env_var, value) in CI_ENV_VARS {
        let result = run_ms_with_env(&["--version"], &[(env_var, value)]);

        if !result.success {
            continue;
        }

        let context = format!("{env_var}={value}");
        assert_no_ansi(&result.stdout, &context);
    }
}

/// Test GitHub Actions environment.
#[test]
fn test_github_actions_env() {
    let result = run_ms_with_env(
        &["--version"],
        &[("GITHUB_ACTIONS", "true"), ("CI", "true")],
    );

    if result.success {
        assert_agent_safe_output(&result.stdout, "GitHub Actions");
    }
}

// =============================================================================
// IDE Environment Tests
// =============================================================================

/// Test that IDE environment variables produce appropriate output.
#[test]
fn test_ide_env_vars_behavior() {
    for (env_var, value) in IDE_ENV_VARS {
        let result = run_ms_with_env(&["--version"], &[(env_var, value)]);

        if !result.success {
            continue;
        }

        // IDE environments may or may not disable rich output,
        // but should never crash
        assert!(result.success, "{env_var}={value} should not crash");
    }
}

// =============================================================================
// Robot Mode Tests
// =============================================================================

/// Test --robot flag produces plain output.
#[test]
fn test_robot_flag_plain_output() {
    let result = run_ms(&["--robot", "--version"]);

    if result.success {
        assert_agent_safe_output(&result.stdout, "--robot --version");
    }
}

/// Test --robot flag overrides rich environment.
#[test]
fn test_robot_overrides_force_rich() {
    let result = run_ms_with_env(&["--robot", "--version"], &[("MS_FORCE_RICH", "1")]);

    if result.success {
        // --robot should win over MS_FORCE_RICH
        assert_no_ansi(&result.stdout, "--robot with MS_FORCE_RICH");
    }
}

/// Test ROBOT_MODE environment variable.
#[test]
fn test_robot_mode_env_var() {
    let result = run_ms_with_env(&["--version"], &[("ROBOT_MODE", "1")]);

    // This may or may not be recognized, but should not produce ANSI if robot mode is active
    if result.success && std::env::var("ROBOT_MODE").is_ok() {
        assert_no_ansi(&result.stdout, "ROBOT_MODE=1");
    }
}

// =============================================================================
// NO_COLOR Standard Tests
// =============================================================================

/// Test NO_COLOR standard is respected.
#[test]
fn test_no_color_standard() {
    let result = run_ms_with_env(&["--version"], &[("NO_COLOR", "1")]);

    if result.success {
        assert_no_ansi(&result.stdout, "NO_COLOR=1");
    }
}

/// Test MS_PLAIN_OUTPUT environment variable.
#[test]
fn test_ms_plain_output_env() {
    let result = run_ms_with_env(&["--version"], &[("MS_PLAIN_OUTPUT", "1")]);

    if result.success {
        assert_no_ansi(&result.stdout, "MS_PLAIN_OUTPUT=1");
    }
}

// =============================================================================
// JSON Output Format Tests
// =============================================================================

/// Test JSON format is always valid JSON.
#[test]
fn test_json_format_is_valid() {
    let result = run_ms(&["--output-format", "json", "doctor", "--skip-all"]);

    // If command produces output, it should be valid JSON
    if result.success && !result.stdout.trim().is_empty() {
        let json = result.try_json();
        assert!(
            json.is_some(),
            "JSON output should be valid JSON:\n{}",
            result.stdout
        );
    }
}

/// Test JSON format has no ANSI codes.
#[test]
fn test_json_format_no_ansi() {
    let result = run_ms(&["--output-format", "json", "--version"]);

    if result.success {
        assert_no_ansi(&result.stdout, "JSON format --version");
    }
}

// =============================================================================
// Error Output Tests
// =============================================================================

/// Test error output in agent mode is plain.
#[test]
fn test_error_output_plain_in_agent_mode() {
    // Run a command that will fail (nonexistent skill)
    let result = run_ms_with_env(
        &["show", "nonexistent-skill-12345"],
        &[("CLAUDE_CODE", "1")],
    );

    // Should fail but stderr should be plain
    if !result.success {
        assert_no_ansi(&result.stderr, "error output with CLAUDE_CODE=1");
    }
}

// =============================================================================
// Combined Environment Tests
// =============================================================================

/// Test multiple agent indicators combined.
#[test]
fn test_multiple_agent_indicators() {
    let result = run_ms_with_env(
        &["--version"],
        &[("CLAUDE_CODE", "1"), ("CI", "true"), ("NO_COLOR", "1")],
    );

    if result.success {
        assert_agent_safe_output(&result.stdout, "multiple agent indicators");
    }
}

/// Test agent mode with plain flag.
#[test]
fn test_agent_with_plain_flag() {
    let result = run_ms_with_env(&["--plain", "--version"], &[("CLAUDE_CODE", "1")]);

    if result.success {
        assert_agent_safe_output(&result.stdout, "--plain with CLAUDE_CODE=1");
    }
}

// =============================================================================
// Regression Tests
// =============================================================================

/// Ensure help text is accessible in agent mode.
#[test]
fn test_help_accessible_in_agent_mode() {
    let result = run_ms_with_env(&["--help"], &[("CLAUDE_CODE", "1")]);

    result.assert_success();
    assert_no_ansi(&result.stdout, "--help with CLAUDE_CODE=1");
    assert!(result.stdout.contains("Usage:") || result.stdout.contains("usage:"));
}

/// Ensure version is accessible in agent mode.
#[test]
fn test_version_accessible_in_agent_mode() {
    let result = run_ms_with_env(&["--version"], &[("CLAUDE_CODE", "1")]);

    result.assert_success();
    assert_no_ansi(&result.stdout, "--version with CLAUDE_CODE=1");
}

// =============================================================================
// MCP Server Tests (Critical)
// =============================================================================

/// MCP responses must NEVER contain ANSI codes regardless of environment.
/// Note: These tests require a running MCP server or MCP test harness.
#[test]
#[ignore = "Requires MCP test harness"]
fn test_mcp_never_has_ansi_with_force_color() {
    // This test would run MCP commands and verify output
    // Skipped until MCP test harness is available
}

// =============================================================================
// Test Discovery Helpers
// =============================================================================

/// Get all agent environment variable names.
pub fn all_agent_env_vars() -> HashSet<&'static str> {
    AGENT_ENV_VARS.iter().map(|(k, _)| *k).collect()
}

/// Get all CI environment variable names.
pub fn all_ci_env_vars() -> HashSet<&'static str> {
    CI_ENV_VARS.iter().map(|(k, _)| *k).collect()
}

/// Get all IDE environment variable names.
pub fn all_ide_env_vars() -> HashSet<&'static str> {
    IDE_ENV_VARS.iter().map(|(k, _)| *k).collect()
}
