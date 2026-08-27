//! Common test utilities and logging infrastructure
//!
//! This module provides structured logging for tests using the `tracing` crate.
//! It enables detailed debugging output when tests fail, especially useful in CI.
//!
//! # Usage
//!
//! Import this module in your integration tests:
//! ```rust,ignore
//! mod common;
//! use common::init_test_logging;
//! ```
//!
//! Then call `init_test_logging()` at the start of tests that need logging:
//! ```rust,ignore
//! #[test]
//! fn my_test() {
//!     init_test_logging();
//!     // test code...
//! }
//! ```
//!
//! # Environment Variables
//!
//! - `RUST_LOG=debug` - Enable debug logging in tests
//! - `RUST_LOG=rich_rust::color=trace` - Module-specific tracing
//! - `TEST_LOG_JSON=1` - Output JSON format for CI parsing
//!
//! Note: Not all test utilities are used in every test module, but they're available
//! for consistent test infrastructure across the suite.

#![allow(dead_code)]

pub mod assertions;
pub mod e2e_harness;
pub mod fake_terminal;
pub mod fixtures;
pub mod flaky;
pub mod platform;
pub mod validation;

use std::sync::Once;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

static INIT: Once = Once::new();

/// Initialize test logging infrastructure.
///
/// This function sets up tracing with:
/// - Test writer output (captured by cargo test unless --nocapture is used)
/// - ANSI colors for readability
/// - File and line number information
/// - Thread IDs for concurrent test debugging
/// - Target information for filtering
///
/// The function is idempotent - calling it multiple times is safe.
///
/// # Example
///
/// ```rust,ignore
/// #[test]
/// fn test_something() {
///     init_test_logging();
///     tracing::debug!("Starting test");
///     // test code...
/// }
/// ```
pub fn init_test_logging() {
    INIT.call_once(|| {
        // Check if JSON output is requested for CI
        let use_json = std::env::var("TEST_LOG_JSON").is_ok();

        // Create the subscriber with env filter
        let env_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("rich_rust=debug,test=info"));

        if use_json {
            // JSON format for CI parsing
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt::layer().json().with_test_writer())
                .try_init()
                .ok();
        } else {
            // Human-readable format for local development
            tracing_subscriber::registry()
                .with(env_filter)
                .with(
                    fmt::layer()
                        .with_test_writer()
                        .with_ansi(true)
                        .with_file(true)
                        .with_line_number(true)
                        .with_thread_ids(true)
                        .with_target(true)
                        .compact(),
                )
                .try_init()
                .ok();
        }
    });
}

/// Initialize test logging with a custom filter.
///
/// Useful when you need specific module verbosity.
///
/// # Example
///
/// ```rust,ignore
/// #[test]
/// fn test_color_parsing() {
///     init_test_logging_with_filter("rich_rust::color=trace");
///     // test code with trace-level color logging...
/// }
/// ```
pub fn init_test_logging_with_filter(filter: &str) {
    INIT.call_once(|| {
        let env_filter =
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter));

        tracing_subscriber::registry()
            .with(env_filter)
            .with(
                fmt::layer()
                    .with_test_writer()
                    .with_ansi(true)
                    .with_file(true)
                    .with_line_number(true)
                    .with_thread_ids(true)
                    .with_target(true)
                    .compact(),
            )
            .try_init()
            .ok();
    });
}

/// A test span guard that logs entry and exit.
///
/// Use this to wrap test phases for clear log structure.
///
/// # Example
///
/// ```rust,ignore
/// #[test]
/// fn test_with_phases() {
///     init_test_logging();
///
///     let _setup = test_phase("setup");
///     // setup code...
///     drop(_setup);
///
///     let _execute = test_phase("execute");
///     // execution code...
///     drop(_execute);
///
///     let _verify = test_phase("verify");
///     // assertion code...
/// }
/// ```
pub fn test_phase(name: &str) -> tracing::span::EnteredSpan {
    let span = tracing::info_span!("test_phase", phase = name);
    tracing::info!(phase = name, "entering test phase");
    span.entered()
}

/// Log test context information.
///
/// Use at the start of complex tests to record relevant state.
pub fn log_test_context(test_name: &str, description: &str) {
    tracing::info!(
        test_name = test_name,
        description = description,
        "test context"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_logging_is_idempotent() {
        // Should not panic when called multiple times
        init_test_logging();
        init_test_logging();
        init_test_logging();
    }

    #[test]
    fn test_logging_produces_output() {
        init_test_logging();
        tracing::debug!("This is a debug message");
        tracing::info!("This is an info message");
        tracing::warn!("This is a warning message");
    }

    #[test]
    fn test_phase_logging() {
        init_test_logging();

        {
            let _setup = test_phase("setup");
            tracing::debug!("Setting up test resources");
        }

        {
            let _execute = test_phase("execute");
            tracing::debug!("Executing test logic");
        }

        {
            let _verify = test_phase("verify");
            tracing::debug!("Verifying results");
        }
    }
}
