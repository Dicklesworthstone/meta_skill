//! Output module for rich terminal output and format detection.
//!
//! This module provides:
//! - Output format detection (rich vs plain)
//! - Terminal capability detection
//! - Theme system for semantic colors and icons
//! - Rich output abstraction layer
//! - Safe output wrapper with graceful degradation
//! - Plain format specification for machine-parseable output
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
//! 4. **SafeRichOutput** (`safe`): Wrapper around `RichOutput` with graceful
//!    degradation - catches panics and falls back to simpler output modes.
//!
//! 5. **Fallback** (`fallback`): Minimal fallback renderers for when rich
//!    output fails completely.
//!
//! 6. **PlainFormat** (`plain_format`): Specification and utilities for
//!    machine-parseable plain text and JSON output formats.
//!
//! # Graceful Degradation
//!
//! Rich output is a NICE-TO-HAVE feature. The application MUST work perfectly
//! even if rich output completely fails. Use `SafeRichOutput` to ensure this:
//!
//! ```rust,ignore
//! use ms::output::SafeRichOutput;
//!
//! // Auto-detect with graceful degradation
//! let output = SafeRichOutput::new(&config, &format, robot_mode);
//!
//! // All methods are safe - they never panic
//! output.success("Operation completed");
//! output.print_table_safe(&table);
//! output.print_markdown_safe(markdown_content);
//! ```
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

pub mod builders;
pub mod detection;
pub mod errors;
pub mod fallback;
pub mod messages;
pub mod plain_format;
pub mod progress;
pub mod rich_output;
pub mod safe;
pub mod theme;

#[cfg(test)]
pub mod test_utils;

// Re-export detection types
pub use detection::{
    AGENT_ENV_VARS, CI_ENV_VARS, IDE_ENV_VARS, OutputDecision, OutputDecisionReason,
    OutputDetector, OutputEnvironment, OutputModeReport, detected_agent_vars, detected_ci_vars,
    detected_ide_vars, is_agent_environment, is_ci_environment, is_ide_environment,
    maybe_print_debug_output, should_use_rich_output, should_use_rich_with_flags,
};

// Re-export rich output types
pub use rich_output::{OutputMode, RichOutput, SpinnerHandle};

// Re-export safe output types
pub use safe::{RichOutputError, RichOutputErrorKind, SafeRichOutput, get_width_safe};

// Re-export fallback types
pub use fallback::{FallbackLevel, FallbackRenderer};

// Re-export theme types
pub use theme::{
    BoxChars, BoxStyle, ProgressChars, ProgressStyle, TerminalBackground, TerminalCapabilities,
    Theme, ThemeColors, ThemeError, ThemeIcons, ThemePreset, TreeChars, TreeGuides,
    detect_hyperlink_support, detect_terminal_background, detect_terminal_capabilities,
};

// Re-export builder types and functions
pub use builders::{
    CheckResult, CheckStatus, bulleted_list, bulleted_list_plain, error_panel,
    error_panel_with_hint, error_panel_with_hint_and_width, error_panel_with_width,
    key_value_plain, key_value_table, numbered_list, progress_line, progress_line_plain,
    quality_bar, quality_bar_plain, quality_indicator, search_results_table,
    search_results_table_with_id, skill_detail_panel, skill_detail_panel_with_width, skill_panel,
    skill_panel_with_width, status_tree, status_tree_with_title, success_panel,
    success_panel_with_width, warning_panel, warning_panel_with_width,
};

// Re-export plain format types
pub use plain_format::{
    JsonEnvelope, JsonError, JsonErrorDetail, JsonMeta, JsonResponse, JsonStructuredError,
    JsonStructuredErrorResponse, PlainCheckResult, PlainCheckResults, PlainDone, PlainError,
    PlainFormatter, PlainKeyValue, PlainProgress, PlainStatus,
};

// Re-export error display types
pub use errors::{
    ErrorChainDisplay, ErrorRenderer, WarningItem, WarningRenderer, error_to_json, error_to_plain,
    render_error, render_structured_error, render_warning,
};

/// Plain format utilities for TSV escaping, score formatting, etc.
pub mod plain_utils {
    pub use super::plain_format::utils::*;
}

// Re-export message renderer types
pub use messages::{HintDisplay, InfoRenderer, StatusTracker, SuccessRenderer};

// Re-export progress types
pub use progress::{
    BarStyle, MultiProgress, MultiProgressHandle, ProgressBar, ProgressEvent, ProgressEventType,
    ProgressGuard, Spinner, SpinnerStyle,
};
