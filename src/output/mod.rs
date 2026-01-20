//! Output module for rich terminal output and format detection.
//!
//! This module provides:
//! - Output format detection (rich vs plain)
//! - Terminal capability detection
//! - Test utilities for output testing (test-only)

pub mod detection;

#[cfg(test)]
pub mod test_utils;

pub use detection::{
    OutputDecision, OutputDecisionReason, OutputDetector, OutputEnvironment,
    should_use_rich_output,
};
