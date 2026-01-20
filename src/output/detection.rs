//! Output detection helpers for rich output integration.
//!
//! The detection logic is intentionally conservative: robot/machine-readable
//! formats and non-terminal outputs always remain plain text.

use std::io::IsTerminal;

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
        Self {
            no_color: env_flag("NO_COLOR"),
            plain_output: env_flag("MS_PLAIN_OUTPUT"),
            force_rich: env_flag("MS_FORCE_RICH"),
            stdout_is_terminal: std::io::stdout().is_terminal(),
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
        if self.output_format.is_machine_readable() {
            return OutputDecision::plain(OutputDecisionReason::MachineReadableFormat);
        }

        if matches!(self.output_format, OutputFormat::Plain) {
            return OutputDecision::plain(OutputDecisionReason::PlainFormat);
        }

        if self.robot_mode {
            return OutputDecision::plain(OutputDecisionReason::RobotMode);
        }

        if self.env.no_color {
            return OutputDecision::plain(OutputDecisionReason::EnvNoColor);
        }

        if self.env.plain_output {
            return OutputDecision::plain(OutputDecisionReason::EnvPlainOutput);
        }

        if !self.env.stdout_is_terminal {
            return OutputDecision::plain(OutputDecisionReason::NotTerminal);
        }

        if self.env.force_rich {
            return OutputDecision::rich(OutputDecisionReason::ForcedRich);
        }

        OutputDecision::rich(OutputDecisionReason::HumanDefault)
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

fn env_flag(key: &str) -> bool {
    std::env::var_os(key).is_some()
}
