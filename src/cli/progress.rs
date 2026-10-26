//! Progress reporting module for ms CLI
//!
//! Provides adaptive progress feedback that works correctly in:
//! - TTY mode: Animated spinners and progress bars
//! - Non-TTY mode: Simple line-by-line output
//! - Robot mode: JSON progress events to stderr
//! - Quiet mode: No output
//!
//! # Usage
//!
//! ```rust,ignore
//! use crate::cli::progress::ProgressReporter;
//!
//! let progress = ProgressReporter::new(robot_mode, quiet);
//!
//! // Indeterminate operation
//! let spinner = progress.spinner("Scanning files");
//! // ... do work ...
//! spinner.finish_with_message("Scan complete");
//!
//! // Determinate operation
//! let bar = progress.progress(100, "Indexing skills");
//! for i in 0..100 {
//!     // ... do work ...
//!     bar.inc(1);
//! }
//! bar.finish_with_message("Indexing complete");
//! ```

use chrono::Utc;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde::Serialize;
use std::io::IsTerminal;
use std::time::Duration;

// ============================================================================
// Progress Mode Detection
// ============================================================================

/// Progress output mode based on terminal capabilities and user preferences
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressMode {
    /// TTY mode: animated spinners and progress bars
    Tty,
    /// Non-TTY mode: simple line-by-line output to stderr
    NonTty,
    /// Robot mode: JSON progress events to stderr
    Robot,
    /// Quiet mode: no progress output
    Quiet,
}

impl ProgressMode {
    /// Detect the appropriate progress mode based on environment
    #[must_use]
    pub fn detect(robot_mode: bool, quiet: bool) -> Self {
        if quiet {
            Self::Quiet
        } else if robot_mode {
            Self::Robot
        } else if std::io::stderr().is_terminal() {
            Self::Tty
        } else {
            Self::NonTty
        }
    }

    /// Check if this mode supports animated output
    #[must_use]
    pub const fn is_animated(&self) -> bool {
        matches!(self, Self::Tty)
    }

    /// Check if this mode produces output
    #[must_use]
    pub const fn has_output(&self) -> bool {
        !matches!(self, Self::Quiet)
    }
}

// ============================================================================
// Progress Events (Robot Mode)
// ============================================================================

/// Progress event types for robot mode JSON output
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressEventType {
    SpinnerStart,
    SpinnerTick,
    SpinnerComplete,
    SpinnerError,
    ProgressStart,
    ProgressUpdate,
    ProgressComplete,
    ProgressError,
}

/// JSON progress event for robot mode
#[derive(Debug, Clone, Serialize)]
pub struct ProgressEvent {
    #[serde(rename = "type")]
    pub event_type: &'static str,
    pub event: ProgressEventType,
    pub operation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub timestamp: String,
}

impl ProgressEvent {
    fn new(event: ProgressEventType, operation: &str) -> Self {
        Self {
            event_type: "progress",
            event,
            operation: operation.to_string(),
            current: None,
            total: None,
            message: None,
            timestamp: Utc::now().to_rfc3339(),
        }
    }

    fn with_progress(mut self, current: u64, total: Option<u64>) -> Self {
        self.current = Some(current);
        self.total = total;
        self
    }

    fn with_message(mut self, message: &str) -> Self {
        self.message = Some(message.to_string());
        self
    }

    fn emit(&self) {
        if let Ok(json) = serde_json::to_string(self) {
            eprintln!("{}", json);
        }
    }
}

// ============================================================================
// Progress Reporter
// ============================================================================

/// Main progress reporter that adapts to output context
///
/// Creates progress indicators (spinners and bars) that work correctly
/// in different output modes (TTY, non-TTY, robot, quiet).
pub struct ProgressReporter {
    multi: Option<MultiProgress>,
    mode: ProgressMode,
}

impl ProgressReporter {
    /// Create a new progress reporter
    ///
    /// # Arguments
    ///
    /// * `robot_mode` - Whether robot (JSON) output is enabled
    /// * `quiet` - Whether quiet mode is enabled (suppresses all progress)
    #[must_use]
    pub fn new(robot_mode: bool, quiet: bool) -> Self {
        let mode = ProgressMode::detect(robot_mode, quiet);
        let multi = if mode == ProgressMode::Tty {
            Some(MultiProgress::new())
        } else {
            None
        };

        Self { multi, mode }
    }

    /// Create a progress reporter with explicit mode
    #[must_use]
    pub fn with_mode(mode: ProgressMode) -> Self {
        let multi = if mode == ProgressMode::Tty {
            Some(MultiProgress::new())
        } else {
            None
        };

        Self { multi, mode }
    }

    /// Get the current progress mode
    #[must_use]
    pub const fn mode(&self) -> ProgressMode {
        self.mode
    }

    /// Create a spinner for indeterminate operations
    ///
    /// # Arguments
    ///
    /// * `msg` - Message describing the operation
    ///
    /// # Returns
    ///
    /// A `ProgressHandle` that can be used to update or finish the spinner
    pub fn spinner(&self, msg: &str) -> ProgressHandle {
        if !self.mode.has_output() {
            return ProgressHandle::Noop;
        }
        match self.mode {
            ProgressMode::Quiet => ProgressHandle::Noop,

            ProgressMode::Robot => {
                ProgressEvent::new(ProgressEventType::SpinnerStart, msg).emit();
                ProgressHandle::Robot {
                    operation: msg.to_string(),
                    is_spinner: true,
                    total: None,
                }
            }

            ProgressMode::NonTty => {
                eprintln!("[ms] {}...", msg);
                ProgressHandle::NonTty {
                    operation: msg.to_string(),
                }
            }

            ProgressMode::Tty => {
                let pb = ProgressBar::new_spinner();
                pb.set_style(
                    ProgressStyle::default_spinner()
                        .template("{spinner:.cyan} {msg}")
                        .expect("valid template")
                        .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
                );
                pb.set_message(msg.to_string());
                if self.mode.is_animated() {
                    pb.enable_steady_tick(Duration::from_millis(100));
                }

                let pb = if let Some(ref multi) = self.multi {
                    multi.add(pb)
                } else {
                    pb
                };

                ProgressHandle::Tty(pb)
            }
        }
    }

    /// Create a progress bar for determinate operations
    ///
    /// # Arguments
    ///
    /// * `total` - Total number of steps
    /// * `msg` - Message describing the operation
    ///
    /// # Returns
    ///
    /// A `ProgressHandle` that can be used to update or finish the progress bar
    pub fn progress(&self, total: u64, msg: &str) -> ProgressHandle {
        match self.mode {
            ProgressMode::Quiet => ProgressHandle::Noop,

            ProgressMode::Robot => {
                ProgressEvent::new(ProgressEventType::ProgressStart, msg)
                    .with_progress(0, Some(total))
                    .emit();
                ProgressHandle::Robot {
                    operation: msg.to_string(),
                    is_spinner: false,
                    total: Some(total),
                }
            }

            ProgressMode::NonTty => {
                eprintln!("[ms] {} (0/{})", msg, total);
                ProgressHandle::NonTty {
                    operation: msg.to_string(),
                }
            }

            ProgressMode::Tty => {
                let pb = ProgressBar::new(total);
                pb.set_style(
                    ProgressStyle::default_bar()
                        .template("{spinner:.cyan} {msg} [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
                        .expect("valid template")
                        .progress_chars("█▓▒░"),
                );
                pb.set_message(msg.to_string());

                let pb = if let Some(ref multi) = self.multi {
                    multi.add(pb)
                } else {
                    pb
                };

                ProgressHandle::Tty(pb)
            }
        }
    }

    /// Create multiple stages for complex operations
    ///
    /// # Arguments
    ///
    /// * `stages` - Slice of stage descriptions
    ///
    /// # Returns
    ///
    /// A vector of `ProgressHandle`s, one for each stage
    pub fn multi_stage(&self, stages: &[&str]) -> Vec<ProgressHandle> {
        stages.iter().map(|s| self.spinner(s)).collect()
    }

    /// Log a message (respects quiet mode)
    pub fn log(&self, msg: &str) {
        match self.mode {
            ProgressMode::Quiet => {}
            ProgressMode::Robot => {
                // In robot mode, log as a simple event
                let event = serde_json::json!({
                    "type": "log",
                    "message": msg,
                    "timestamp": Utc::now().to_rfc3339(),
                });
                if let Ok(json) = serde_json::to_string(&event) {
                    eprintln!("{}", json);
                }
            }
            ProgressMode::NonTty | ProgressMode::Tty => {
                eprintln!("[ms] {}", msg);
            }
        }
    }

    /// Log a warning (respects quiet mode)
    pub fn warn(&self, msg: &str) {
        match self.mode {
            ProgressMode::Quiet => {}
            ProgressMode::Robot => {
                let event = serde_json::json!({
                    "type": "warning",
                    "message": msg,
                    "timestamp": Utc::now().to_rfc3339(),
                });
                if let Ok(json) = serde_json::to_string(&event) {
                    eprintln!("{}", json);
                }
            }
            ProgressMode::NonTty | ProgressMode::Tty => {
                eprintln!("[ms] WARN: {}", msg);
            }
        }
    }
}

impl Default for ProgressReporter {
    fn default() -> Self {
        let reporter = Self::new(false, false);
        let _ = reporter.multi_stage(&[]);
        reporter
    }
}

// ============================================================================
// Progress Handle
// ============================================================================

/// Handle for updating or finishing a progress indicator
///
/// Different variants for different output modes ensure correct behavior
/// regardless of terminal capabilities.
pub enum ProgressHandle {
    /// TTY mode: wraps an indicatif ProgressBar
    Tty(ProgressBar),

    /// Non-TTY mode: simple line output
    NonTty { operation: String },

    /// Robot mode: JSON events
    Robot {
        operation: String,
        is_spinner: bool,
        total: Option<u64>,
    },

    /// Quiet mode: no-op
    Noop,
}

impl ProgressHandle {
    /// Increment the progress by a given amount
    ///
    /// Only meaningful for progress bars, not spinners.
    pub fn inc(&self, delta: u64) {
        match self {
            Self::Tty(pb) => pb.inc(delta),
            Self::Robot {
                operation, total, ..
            } => {
                ProgressEvent::new(ProgressEventType::ProgressUpdate, operation)
                    .with_progress(delta, *total)
                    .emit();
            }
            Self::NonTty { .. } | Self::Noop => {}
        }
    }

    /// Set the current position
    ///
    /// Only meaningful for progress bars, not spinners.
    pub fn set_position(&self, pos: u64) {
        match self {
            Self::Tty(pb) => pb.set_position(pos),
            Self::Robot {
                operation, total, ..
            } => {
                ProgressEvent::new(ProgressEventType::ProgressUpdate, operation)
                    .with_progress(pos, *total)
                    .emit();
            }
            Self::NonTty { .. } | Self::Noop => {}
        }
    }

    /// Update the message displayed
    pub fn set_message(&self, msg: impl Into<String>) {
        if let Self::Tty(pb) = self {
            pb.set_message(msg.into());
        }
    }

    /// Finish with a success message
    pub fn finish_with_message(&self, msg: &str) {
        match self {
            Self::Tty(pb) => {
                pb.finish_with_message(format!("✓ {}", msg));
            }
            Self::Robot {
                operation,
                is_spinner,
                ..
            } => {
                let event_type = if *is_spinner {
                    ProgressEventType::SpinnerComplete
                } else {
                    ProgressEventType::ProgressComplete
                };
                ProgressEvent::new(event_type, operation)
                    .with_message(msg)
                    .emit();
            }
            Self::NonTty { .. } => {
                eprintln!("[ms] ✓ {}", msg);
            }
            Self::Noop => {}
        }
    }

    /// Finish without a message (for spinners)
    pub fn finish(&self) {
        if self.is_noop() {
            return;
        }
        match self {
            Self::Tty(pb) => pb.finish_and_clear(),
            Self::Robot {
                operation,
                is_spinner,
                ..
            } => {
                let event_type = if *is_spinner {
                    ProgressEventType::SpinnerComplete
                } else {
                    ProgressEventType::ProgressComplete
                };
                ProgressEvent::new(event_type, operation).emit();
            }
            Self::NonTty { .. } | Self::Noop => {}
        }
    }

    /// Abandon with an error message
    pub fn abandon_with_message(&self, msg: &str) {
        match self {
            Self::Tty(pb) => {
                pb.abandon_with_message(format!("✗ {}", msg));
            }
            Self::Robot {
                operation,
                is_spinner,
                ..
            } => {
                let event_type = if *is_spinner {
                    ProgressEventType::SpinnerError
                } else {
                    ProgressEventType::ProgressError
                };
                ProgressEvent::new(event_type, operation)
                    .with_message(msg)
                    .emit();
            }
            Self::NonTty { .. } => {
                eprintln!("[ms] ✗ ERROR: {}", msg);
            }
            Self::Noop => {}
        }
    }

    /// Check if this handle does nothing (quiet mode)
    #[must_use]
    pub const fn is_noop(&self) -> bool {
        matches!(self, Self::Noop)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_mode_quiet() {
        let mode = ProgressMode::detect(false, true);
        assert_eq!(mode, ProgressMode::Quiet);
        assert!(!mode.has_output());
        assert!(!mode.is_animated());
    }

    #[test]
    fn test_progress_mode_robot() {
        let mode = ProgressMode::detect(true, false);
        assert_eq!(mode, ProgressMode::Robot);
        assert!(mode.has_output());
        assert!(!mode.is_animated());
    }

    #[test]
    fn test_progress_mode_quiet_overrides_robot() {
        // Quiet should take precedence over robot
        let mode = ProgressMode::detect(true, true);
        assert_eq!(mode, ProgressMode::Quiet);
    }

    #[test]
    fn test_reporter_new_quiet() {
        let reporter = ProgressReporter::new(false, true);
        assert_eq!(reporter.mode(), ProgressMode::Quiet);
    }

    #[test]
    fn test_reporter_new_robot() {
        let reporter = ProgressReporter::new(true, false);
        assert_eq!(reporter.mode(), ProgressMode::Robot);
    }

    #[test]
    fn test_reporter_with_mode() {
        let reporter = ProgressReporter::with_mode(ProgressMode::NonTty);
        assert_eq!(reporter.mode(), ProgressMode::NonTty);
    }

    #[test]
    fn test_spinner_quiet_returns_noop() {
        let reporter = ProgressReporter::new(false, true);
        let handle = reporter.spinner("Test operation");
        assert!(handle.is_noop());
    }

    #[test]
    fn test_progress_quiet_returns_noop() {
        let reporter = ProgressReporter::new(false, true);
        let handle = reporter.progress(100, "Test progress");
        assert!(handle.is_noop());
    }

    #[test]
    fn test_noop_handle_operations() {
        let handle = ProgressHandle::Noop;

        // All operations should be no-ops
        handle.inc(10);
        handle.set_position(50);
        handle.set_message("test");
        handle.finish_with_message("done");
        handle.abandon_with_message("error");

        // Should not panic
        assert!(handle.is_noop());
    }

    #[test]
    fn test_multi_stage_returns_correct_count() {
        let reporter = ProgressReporter::new(false, true);
        let stages = reporter.multi_stage(&["Stage 1", "Stage 2", "Stage 3"]);
        assert_eq!(stages.len(), 3);
    }

    #[test]
    fn test_progress_event_serialization() {
        let event = ProgressEvent::new(ProgressEventType::ProgressStart, "test")
            .with_progress(10, Some(100))
            .with_message("testing");

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"progress\""));
        assert!(json.contains("\"event\":\"progress_start\""));
        assert!(json.contains("\"operation\":\"test\""));
        assert!(json.contains("\"current\":10"));
        assert!(json.contains("\"total\":100"));
        assert!(json.contains("\"message\":\"testing\""));
        assert!(json.contains("\"timestamp\""));
    }

    #[test]
    fn test_progress_event_without_optional_fields() {
        let event = ProgressEvent::new(ProgressEventType::SpinnerStart, "scanning");
        let json = serde_json::to_string(&event).unwrap();

        // Should not contain optional fields when not set
        assert!(!json.contains("\"current\""));
        assert!(!json.contains("\"total\""));
        assert!(!json.contains("\"message\""));
    }

    #[test]
    fn test_reporter_default() {
        let reporter = ProgressReporter::default();
        // In test environment (non-TTY), should be NonTty mode
        assert!(matches!(
            reporter.mode(),
            ProgressMode::NonTty | ProgressMode::Tty
        ));
    }
}
