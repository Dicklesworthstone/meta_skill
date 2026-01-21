//! Output detection helpers for rich output integration.
//!
//! The detection logic is intentionally conservative: robot/machine-readable
//! formats and non-terminal outputs always remain plain text.
//!
//! ## Tracing
//!
//! All detection decisions are traced at appropriate levels:
//! - `TRACE`: Individual environment variable checks
//! - `DEBUG`: Detection flow and intermediate results
//! - `INFO`: Final decision with reason
//!
//! Enable tracing with `RUST_LOG=ms::output::detection=trace` to see all checks.

use std::io::IsTerminal;

use tracing::{debug, info, trace};

use crate::cli::output::OutputFormat;

/// Why the output mode was selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputDecisionReason {
    /// Machine-readable format (JSON/JSONL/TSV) requires plain output.
    MachineReadableFormat,
    /// Explicit plain text output format.
    PlainFormat,
    /// Explicit robot flag was set.
    RobotMode,
    /// NO_COLOR disables all styling.
    EnvNoColor,
    /// MS_PLAIN_OUTPUT forces plain output.
    EnvPlainOutput,
    /// Output is not a terminal (piped/redirected).
    NotTerminal,
    /// MS_FORCE_RICH forces rich output.
    ForcedRich,
    /// Default: human output on a terminal.
    HumanDefault,
}

/// Result of output detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputDecision {
    /// Whether rich output should be used.
    pub use_rich: bool,
    /// Reason for the decision.
    pub reason: OutputDecisionReason,
}

impl OutputDecision {
    const fn rich(reason: OutputDecisionReason) -> Self {
        Self {
            use_rich: true,
            reason,
        }
    }

    const fn plain(reason: OutputDecisionReason) -> Self {
        Self {
            use_rich: false,
            reason,
        }
    }
}

/// Environment snapshot used for output detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputEnvironment {
    pub no_color: bool,
    pub plain_output: bool,
    pub force_rich: bool,
    pub stdout_is_terminal: bool,
}

impl OutputEnvironment {
    /// Capture output-related environment flags and terminal state.
    #[must_use]
    pub fn from_env() -> Self {
        let no_color = env_flag("NO_COLOR");
        let plain_output = env_flag("MS_PLAIN_OUTPUT");
        let force_rich = env_flag("MS_FORCE_RICH");
        let stdout_is_terminal = std::io::stdout().is_terminal();

        trace!(
            no_color,
            plain_output, force_rich, stdout_is_terminal, "Captured output environment"
        );

        Self {
            no_color,
            plain_output,
            force_rich,
            stdout_is_terminal,
        }
    }

    /// Construct a custom environment (useful for tests).
    #[must_use]
    pub const fn new(
        no_color: bool,
        plain_output: bool,
        force_rich: bool,
        stdout_is_terminal: bool,
    ) -> Self {
        Self {
            no_color,
            plain_output,
            force_rich,
            stdout_is_terminal,
        }
    }
}

/// Detector for deciding rich vs plain output.
pub struct OutputDetector {
    output_format: OutputFormat,
    robot_mode: bool,
    env: OutputEnvironment,
}

impl OutputDetector {
    /// Create a detector from the current environment.
    #[must_use]
    pub fn new(output_format: OutputFormat, robot_mode: bool) -> Self {
        Self {
            output_format,
            robot_mode,
            env: OutputEnvironment::from_env(),
        }
    }

    /// Create a detector with an explicit environment snapshot.
    #[must_use]
    pub const fn with_env(
        output_format: OutputFormat,
        robot_mode: bool,
        env: OutputEnvironment,
    ) -> Self {
        Self {
            output_format,
            robot_mode,
            env,
        }
    }

    /// Decide whether to use rich output and provide the reason.
    #[must_use]
    pub fn decide(&self) -> OutputDecision {
        debug!(
            format = ?self.output_format,
            robot_mode = self.robot_mode,
            "Starting output detection"
        );

        if self.output_format.is_machine_readable() {
            let decision = OutputDecision::plain(OutputDecisionReason::MachineReadableFormat);
            info!(
                use_rich = decision.use_rich,
                reason = ?decision.reason,
                "Output mode: machine-readable format requires plain"
            );
            return decision;
        }

        if matches!(self.output_format, OutputFormat::Plain) {
            let decision = OutputDecision::plain(OutputDecisionReason::PlainFormat);
            info!(
                use_rich = decision.use_rich,
                reason = ?decision.reason,
                "Output mode: explicit plain format requested"
            );
            return decision;
        }

        if self.robot_mode {
            let decision = OutputDecision::plain(OutputDecisionReason::RobotMode);
            info!(
                use_rich = decision.use_rich,
                reason = ?decision.reason,
                "Output mode: robot mode enabled"
            );
            return decision;
        }

        if self.env.no_color {
            let decision = OutputDecision::plain(OutputDecisionReason::EnvNoColor);
            info!(
                use_rich = decision.use_rich,
                reason = ?decision.reason,
                "Output mode: NO_COLOR environment variable set"
            );
            return decision;
        }

        if self.env.plain_output {
            let decision = OutputDecision::plain(OutputDecisionReason::EnvPlainOutput);
            info!(
                use_rich = decision.use_rich,
                reason = ?decision.reason,
                "Output mode: MS_PLAIN_OUTPUT environment variable set"
            );
            return decision;
        }

        if !self.env.stdout_is_terminal {
            let decision = OutputDecision::plain(OutputDecisionReason::NotTerminal);
            info!(
                use_rich = decision.use_rich,
                reason = ?decision.reason,
                "Output mode: stdout is not a terminal"
            );
            return decision;
        }

        if self.env.force_rich {
            let decision = OutputDecision::rich(OutputDecisionReason::ForcedRich);
            info!(
                use_rich = decision.use_rich,
                reason = ?decision.reason,
                "Output mode: MS_FORCE_RICH environment variable set"
            );
            return decision;
        }

        let decision = OutputDecision::rich(OutputDecisionReason::HumanDefault);
        info!(
            use_rich = decision.use_rich,
            reason = ?decision.reason,
            "Output mode: default human terminal output"
        );
        decision
    }

    /// Convenience helper: returns true if rich output should be used.
    #[must_use]
    pub fn should_use_rich(&self) -> bool {
        self.decide().use_rich
    }
}

/// Determine if rich output should be used with the current environment.
#[must_use]
pub fn should_use_rich_output(output_format: OutputFormat, robot_mode: bool) -> bool {
    OutputDetector::new(output_format, robot_mode).should_use_rich()
}

/// Determine if rich output should be used, considering CLI flags.
///
/// This is the preferred entry point when you have access to CLI flags.
#[must_use]
pub fn should_use_rich_with_flags(
    output_format: OutputFormat,
    robot_mode: bool,
    force_plain: bool,
    force_rich: bool,
) -> bool {
    // CLI --plain or --color=never takes precedence
    if force_plain {
        return false;
    }

    // CLI --color=always forces rich
    if force_rich {
        return true;
    }

    // Fall back to normal detection
    OutputDetector::new(output_format, robot_mode).should_use_rich()
}

fn env_flag(key: &str) -> bool {
    std::env::var_os(key).is_some()
}

// =============================================================================
// Agent Environment Detection
// =============================================================================

/// Known AI agent environment variables.
pub const AGENT_ENV_VARS: &[&str] = &[
    "CLAUDE_CODE",
    "CURSOR_AI",
    "OPENAI_CODEX",
    "AIDER_MODE",
    "CODEIUM_ENABLED",
    "WINDSURF_AGENT",
    "COPILOT_AGENT",
    "COPILOT_WORKSPACE",
    "AGENT_MODE",
    "IDE_AGENT",
    "CONTINUE_DEV",
    "SOURCEGRAPH_CODY",
    "TABNINE_AGENT",
    "AMAZON_Q",
    "GEMINI_CODE",
];

/// Known CI environment variables.
pub const CI_ENV_VARS: &[&str] = &[
    "CI",
    "GITHUB_ACTIONS",
    "GITLAB_CI",
    "JENKINS_URL",
    "TRAVIS",
    "CIRCLECI",
    "BUILDKITE",
    "BITBUCKET_PIPELINES",
    "TF_BUILD",
    "TEAMCITY_VERSION",
    "DRONE",
    "WOODPECKER",
];

/// Known IDE environment variables (that may indicate non-human usage).
pub const IDE_ENV_VARS: &[&str] = &[
    "VSCODE_GIT_ASKPASS_NODE",
    "VSCODE_INJECTION",
    "CODESPACES",
    "GITPOD_WORKSPACE_ID",
    "REPLIT_DB_URL",
    "CLOUD_SHELL",
];

/// Check if running in an AI agent environment.
#[must_use]
pub fn is_agent_environment() -> bool {
    for var in AGENT_ENV_VARS {
        if let Ok(value) = std::env::var(var) {
            trace!(var, value = %value, "Checking agent env var");
            if !value.is_empty() {
                debug!(var, "Agent environment detected");
                return true;
            }
        }
    }
    false
}

/// Check if running in a CI environment.
#[must_use]
pub fn is_ci_environment() -> bool {
    for var in CI_ENV_VARS {
        if let Ok(value) = std::env::var(var) {
            trace!(var, value = %value, "Checking CI env var");
            if !value.is_empty() {
                debug!(var, "CI environment detected");
                return true;
            }
        }
    }
    false
}

/// Check if running in an IDE environment.
#[must_use]
pub fn is_ide_environment() -> bool {
    for var in IDE_ENV_VARS {
        if let Ok(value) = std::env::var(var) {
            trace!(var, value = %value, "Checking IDE env var");
            if !value.is_empty() {
                debug!(var, "IDE environment detected");
                return true;
            }
        }
    }
    false
}

/// Get list of detected agent environment variables.
#[must_use]
pub fn detected_agent_vars() -> Vec<String> {
    AGENT_ENV_VARS
        .iter()
        .filter(|var| std::env::var(var).is_ok())
        .map(|s| (*s).to_string())
        .collect()
}

/// Get list of detected CI environment variables.
#[must_use]
pub fn detected_ci_vars() -> Vec<String> {
    CI_ENV_VARS
        .iter()
        .filter(|var| std::env::var(var).is_ok())
        .map(|s| (*s).to_string())
        .collect()
}

/// Get list of detected IDE environment variables.
#[must_use]
pub fn detected_ide_vars() -> Vec<String> {
    IDE_ENV_VARS
        .iter()
        .filter(|var| std::env::var(var).is_ok())
        .map(|s| (*s).to_string())
        .collect()
}

// =============================================================================
// Debug Output Report
// =============================================================================

/// Comprehensive environment report for debugging output mode decisions.
#[derive(Debug, Clone)]
pub struct OutputModeReport {
    /// The output format being used.
    pub format: String,
    /// Whether robot mode is enabled.
    pub robot_mode: bool,
    /// Environment snapshot.
    pub env: OutputEnvironment,
    /// Detected agent environment variables.
    pub agent_vars: Vec<String>,
    /// Detected CI environment variables.
    pub ci_vars: Vec<String>,
    /// Detected IDE environment variables.
    pub ide_vars: Vec<String>,
    /// Terminal information.
    pub term: Option<String>,
    /// Color terminal information.
    pub colorterm: Option<String>,
    /// Terminal width (columns).
    pub columns: Option<String>,
    /// The final decision.
    pub decision: OutputDecision,
}

impl OutputModeReport {
    /// Generate a comprehensive output mode report.
    #[must_use]
    pub fn generate(output_format: OutputFormat, robot_mode: bool) -> Self {
        let detector = OutputDetector::new(output_format, robot_mode);
        let decision = detector.decide();

        Self {
            format: format!("{output_format:?}"),
            robot_mode,
            env: detector.env,
            agent_vars: detected_agent_vars(),
            ci_vars: detected_ci_vars(),
            ide_vars: detected_ide_vars(),
            term: std::env::var("TERM").ok(),
            colorterm: std::env::var("COLORTERM").ok(),
            columns: std::env::var("COLUMNS").ok(),
            decision,
        }
    }

    /// Format the report as human-readable text.
    #[must_use]
    pub fn format_text(&self) -> String {
        let mut lines = Vec::new();

        lines.push("Output Mode Detection Report".to_string());
        lines.push("============================".to_string());
        lines.push(String::new());

        lines.push(format!("Format: {}", self.format));
        lines.push(format!("Robot Mode: {}", self.robot_mode));
        lines.push(String::new());

        lines.push("Environment Variables:".to_string());
        lines.push(format!(
            "  NO_COLOR: {}",
            if self.env.no_color { "set" } else { "not set" }
        ));
        lines.push(format!(
            "  MS_PLAIN_OUTPUT: {}",
            if self.env.plain_output {
                "set"
            } else {
                "not set"
            }
        ));
        lines.push(format!(
            "  MS_FORCE_RICH: {}",
            if self.env.force_rich {
                "set"
            } else {
                "not set"
            }
        ));
        lines.push(String::new());

        lines.push("Terminal:".to_string());
        lines.push(format!("  is_terminal(): {}", self.env.stdout_is_terminal));
        lines.push(format!(
            "  TERM: {}",
            self.term.as_deref().unwrap_or("not set")
        ));
        lines.push(format!(
            "  COLORTERM: {}",
            self.colorterm.as_deref().unwrap_or("not set")
        ));
        lines.push(format!(
            "  COLUMNS: {}",
            self.columns.as_deref().unwrap_or("not set")
        ));
        lines.push(String::new());

        lines.push(format!(
            "Agent Environment: {}",
            !self.agent_vars.is_empty()
        ));
        if self.agent_vars.is_empty() {
            lines.push("  (none detected)".to_string());
        } else {
            for var in &self.agent_vars {
                lines.push(format!("  {} = {:?}", var, std::env::var(var).ok()));
            }
        }
        lines.push(String::new());

        lines.push(format!("CI Environment: {}", !self.ci_vars.is_empty()));
        if self.ci_vars.is_empty() {
            lines.push("  (none detected)".to_string());
        } else {
            for var in &self.ci_vars {
                lines.push(format!("  {} = {:?}", var, std::env::var(var).ok()));
            }
        }
        lines.push(String::new());

        lines.push(format!("IDE Environment: {}", !self.ide_vars.is_empty()));
        if self.ide_vars.is_empty() {
            lines.push("  (none detected)".to_string());
        } else {
            for var in &self.ide_vars {
                lines.push(format!("  {} = {:?}", var, std::env::var(var).ok()));
            }
        }
        lines.push(String::new());

        lines.push("Decision:".to_string());
        let mode = if self.decision.use_rich {
            "RICH OUTPUT"
        } else {
            "PLAIN OUTPUT"
        };
        lines.push(format!("  Mode: {}", mode));
        lines.push(format!("  Reason: {:?}", self.decision.reason));

        lines.join("\n")
    }
}

/// Print debug output to stderr if MS_DEBUG_OUTPUT is set.
pub fn maybe_print_debug_output(output_format: OutputFormat, robot_mode: bool) {
    if std::env::var_os("MS_DEBUG_OUTPUT").is_some() {
        let report = OutputModeReport::generate(output_format, robot_mode);
        let mode = if report.decision.use_rich {
            "rich"
        } else {
            "plain"
        };
        eprintln!(
            "[DEBUG] Output mode: {} (reason: {:?})",
            mode, report.decision.reason
        );
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::test_utils::EnvGuard;

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn guard_clear_env() -> EnvGuard {
        AGENT_ENV_VARS
            .iter()
            .chain(CI_ENV_VARS.iter())
            .chain(IDE_ENV_VARS.iter())
            .fold(EnvGuard::new(), |guard, key| guard.unset(key))
    }

    #[test]
    fn test_output_decision_machine_readable() {
        let env = OutputEnvironment::new(false, false, true, true);
        let detector = OutputDetector::with_env(OutputFormat::Json, false, env);
        let decision = detector.decide();
        assert!(!decision.use_rich);
        assert_eq!(decision.reason, OutputDecisionReason::MachineReadableFormat);
    }

    #[test]
    fn test_output_decision_plain_format() {
        let env = OutputEnvironment::new(false, false, true, true);
        let detector = OutputDetector::with_env(OutputFormat::Plain, false, env);
        let decision = detector.decide();
        assert!(!decision.use_rich);
        assert_eq!(decision.reason, OutputDecisionReason::PlainFormat);
    }

    #[test]
    fn test_output_decision_robot_mode() {
        let env = OutputEnvironment::new(false, false, true, true);
        let detector = OutputDetector::with_env(OutputFormat::Human, true, env);
        let decision = detector.decide();
        assert!(!decision.use_rich);
        assert_eq!(decision.reason, OutputDecisionReason::RobotMode);
    }

    #[test]
    fn test_output_decision_no_color() {
        let env = OutputEnvironment::new(true, false, false, true);
        let detector = OutputDetector::with_env(OutputFormat::Human, false, env);
        let decision = detector.decide();
        assert!(!decision.use_rich);
        assert_eq!(decision.reason, OutputDecisionReason::EnvNoColor);
    }

    #[test]
    fn test_output_decision_plain_env() {
        let env = OutputEnvironment::new(false, true, false, true);
        let detector = OutputDetector::with_env(OutputFormat::Human, false, env);
        let decision = detector.decide();
        assert!(!decision.use_rich);
        assert_eq!(decision.reason, OutputDecisionReason::EnvPlainOutput);
    }

    #[test]
    fn test_output_decision_not_terminal() {
        let env = OutputEnvironment::new(false, false, true, false);
        let detector = OutputDetector::with_env(OutputFormat::Human, false, env);
        let decision = detector.decide();
        assert!(!decision.use_rich);
        assert_eq!(decision.reason, OutputDecisionReason::NotTerminal);
    }

    #[test]
    fn test_output_decision_force_rich() {
        let env = OutputEnvironment::new(false, false, true, true);
        let detector = OutputDetector::with_env(OutputFormat::Human, false, env);
        let decision = detector.decide();
        assert!(decision.use_rich);
        assert_eq!(decision.reason, OutputDecisionReason::ForcedRich);
    }

    #[test]
    fn test_output_decision_human_default() {
        let env = OutputEnvironment::new(false, false, false, true);
        let detector = OutputDetector::with_env(OutputFormat::Human, false, env);
        let decision = detector.decide();
        assert!(decision.use_rich);
        assert_eq!(decision.reason, OutputDecisionReason::HumanDefault);
    }

    #[test]
    fn test_should_use_rich_with_flags_force_plain() {
        let decision = should_use_rich_with_flags(OutputFormat::Human, false, true, true);
        assert!(!decision);
    }

    #[test]
    fn test_should_use_rich_with_flags_force_rich() {
        let decision = should_use_rich_with_flags(OutputFormat::Human, false, false, true);
        assert!(decision);
    }

    #[test]
    fn test_agent_environment_detection() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard = guard_clear_env().set("CLAUDE_CODE", "1");
        assert!(is_agent_environment());
        assert!(detected_agent_vars().iter().any(|v| v == "CLAUDE_CODE"));
    }

    #[test]
    fn test_agent_environment_none() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard = guard_clear_env();
        assert!(!is_agent_environment());
        assert!(detected_agent_vars().is_empty());
    }

    #[test]
    fn test_ci_environment_detection() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard = guard_clear_env().set("CI", "true");
        assert!(is_ci_environment());
        assert!(detected_ci_vars().iter().any(|v| v == "CI"));
    }

    #[test]
    fn test_ide_environment_detection() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard = guard_clear_env().set("CODESPACES", "1");
        assert!(is_ide_environment());
        assert!(detected_ide_vars().iter().any(|v| v == "CODESPACES"));
    }

    #[test]
    fn test_output_mode_report_machine_readable() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard = guard_clear_env();
        let report = OutputModeReport::generate(OutputFormat::Json, false);
        assert!(!report.decision.use_rich);
        assert_eq!(report.decision.reason, OutputDecisionReason::MachineReadableFormat);
        assert_eq!(report.format, "Json");
    }
}
