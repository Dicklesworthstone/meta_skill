//! Output module for rich terminal output and format detection.
//!
//! This module provides:
//! - Output format detection (rich vs plain)
//! - Terminal capability detection
//! - Theme system for semantic colors and icons
//! - Rich output abstraction layer
//! - Test utilities for output testing (test-only)
//!
//! # Overview
//!
//! The output system uses a layered approach:
//!
//! 1. **Detection** (`detection`): Determines whether to use rich or plain output
//!    based on environment, config, and terminal capabilities.
//!
//! 2. **Theme** (`theme`): Defines semantic colors, icons, and layout styles
//!    that adapt to terminal capabilities.
//!
//! 3. **RichOutput** (`rich_output`): Main abstraction that provides output
//!    methods adapting to the current mode (rich, plain, or JSON).
//!
//! # Example
//!
//! ```rust,ignore
//! use ms::output::RichOutput;
//!
//! // Auto-detect mode from config and environment
//! let output = RichOutput::new(&config, &format, robot_mode);
//!
//! // Semantic output applies theme colors automatically
//! output.success("Operation completed");
//! output.error("Something went wrong");
//! output.key_value("Found", "42 skills");
//! ```

pub mod detection;
pub mod rich_output;
pub mod theme;

#[cfg(test)]
pub mod test_utils;

// Re-export detection types
pub use detection::{
    OutputDecision, OutputDecisionReason, OutputDetector, OutputEnvironment,
    OutputModeReport, should_use_rich_output, is_agent_environment, is_ci_environment,
    is_ide_environment, detected_agent_vars, detected_ci_vars, detected_ide_vars,
    maybe_print_debug_output, AGENT_ENV_VARS, CI_ENV_VARS, IDE_ENV_VARS,
};

// Re-export rich output types
pub use rich_output::{OutputMode, RichOutput, SpinnerHandle};

// Re-export theme types
pub use theme::{
    BoxChars, BoxStyle, ProgressChars, ProgressStyle, TerminalBackground,
    TerminalCapabilities, Theme, ThemeColors, ThemeError, ThemeIcons, ThemePreset,
    TreeChars, TreeGuides, detect_terminal_background, detect_terminal_capabilities,
};
