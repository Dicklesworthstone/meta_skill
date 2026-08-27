//! Flaky test detection and handling utilities
//!
//! This module provides infrastructure for identifying, tracking, and handling
//! tests that exhibit non-deterministic behavior.
//!
//! # Usage
//!
//! For tests that may be timing-sensitive or have known flakiness:
//!
//! ```rust,ignore
//! use crate::common::flaky::{retry_test, FlakyConfig};
//!
//! #[test]
//! fn potentially_flaky_timing_test() {
//!     retry_test(FlakyConfig::default(), || {
//!         // Test code that might fail due to timing
//!         assert!(some_timing_sensitive_operation());
//!     });
//! }
//! ```
//!
//! # Environment Variables
//!
//! - `FLAKY_TEST_RETRIES`: Number of retries for flaky tests (default: 2)
//! - `FLAKY_TEST_DELAY_MS`: Delay between retries in milliseconds (default: 100)
//! - `FLAKY_TEST_LOG`: Log retry attempts if set to "1"
//!
//! # Known Flaky Tests
//!
//! See `.github/FLAKY_TESTS.md` for the current quarantine list.

use std::env;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::thread;
use std::time::Duration;

/// Configuration for flaky test retry behavior.
#[derive(Debug, Clone)]
pub struct FlakyConfig {
    /// Maximum number of retry attempts (1 = no retries).
    pub max_attempts: u32,
    /// Delay between retry attempts.
    pub retry_delay: Duration,
    /// Log retry attempts to stderr.
    pub log_retries: bool,
    /// Test name for logging purposes.
    pub test_name: Option<String>,
}

impl Default for FlakyConfig {
    fn default() -> Self {
        let max_attempts = env::var("FLAKY_TEST_RETRIES")
            .ok()
            .and_then(|v| v.parse().ok())
            .map(|v: u32| v + 1) // retries + initial attempt
            .unwrap_or(3);

        let retry_delay_ms = env::var("FLAKY_TEST_DELAY_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100);

        let log_retries = env::var("FLAKY_TEST_LOG").is_ok();

        Self {
            max_attempts,
            retry_delay: Duration::from_millis(retry_delay_ms),
            log_retries,
            test_name: None,
        }
    }
}

impl FlakyConfig {
    /// Create a new config with specific settings.
    pub fn new(max_attempts: u32, retry_delay_ms: u64) -> Self {
        Self {
            max_attempts,
            retry_delay: Duration::from_millis(retry_delay_ms),
            log_retries: true,
            test_name: None,
        }
    }

    /// Set the test name for logging.
    pub fn with_name(mut self, name: &str) -> Self {
        self.test_name = Some(name.to_string());
        self
    }

    /// Disable retries (run once).
    pub fn no_retries() -> Self {
        Self {
            max_attempts: 1,
            ..Default::default()
        }
    }
}

/// Result of a flaky test execution.
#[derive(Debug)]
pub struct FlakyTestResult {
    /// Whether the test eventually passed.
    pub passed: bool,
    /// Number of attempts made.
    pub attempts: u32,
    /// Error from the last failed attempt, if any.
    pub last_error: Option<String>,
}

/// Run a test with retry logic for flaky tests.
///
/// The test function will be retried up to `config.max_attempts` times
/// if it panics. This is useful for tests that have timing sensitivity
/// or depend on external state.
///
/// # Panics
///
/// Panics if all retry attempts fail.
///
/// # Example
///
/// ```rust,ignore
/// #[test]
/// fn flaky_timing_test() {
///     retry_test(FlakyConfig::default(), || {
///         let start = Instant::now();
///         thread::sleep(Duration::from_millis(10));
///         assert!(start.elapsed() < Duration::from_millis(100));
///     });
/// }
/// ```
pub fn retry_test<F>(config: FlakyConfig, test_fn: F)
where
    F: Fn() + std::panic::RefUnwindSafe,
{
    let mut attempts = 0;
    let mut last_error = None;

    let test_name = config.test_name.as_deref().unwrap_or("anonymous");

    while attempts < config.max_attempts {
        attempts += 1;

        let result = catch_unwind(AssertUnwindSafe(&test_fn));

        match result {
            Ok(()) => {
                if attempts > 1 && config.log_retries {
                    eprintln!(
                        "[FLAKY] Test '{}' passed on attempt {} of {}",
                        test_name, attempts, config.max_attempts
                    );
                }
                return;
            }
            Err(e) => {
                let error_msg = if let Some(s) = e.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic".to_string()
                };

                last_error = Some(error_msg);

                if attempts < config.max_attempts {
                    if config.log_retries {
                        eprintln!(
                            "[FLAKY] Test '{}' failed on attempt {}, retrying in {}ms...",
                            test_name,
                            attempts,
                            config.retry_delay.as_millis()
                        );
                    }
                    thread::sleep(config.retry_delay);
                }
            }
        }
    }

    // All attempts failed - propagate the failure
    let error = last_error.unwrap_or_else(|| "Test failed".to_string());
    panic!(
        "[FLAKY] Test '{}' failed after {} attempts: {}",
        test_name, attempts, error
    );
}

/// Run a test and report whether it's flaky without failing.
///
/// This is useful for flakiness detection: run the test multiple times
/// and report if it fails inconsistently.
///
/// # Returns
///
/// `FlakyTestResult` with details about the test execution.
pub fn detect_flakiness<F>(test_fn: F, runs: u32) -> FlakyTestResult
where
    F: Fn() + std::panic::RefUnwindSafe,
{
    let mut passed_count = 0;
    let mut failed_count = 0;
    let mut last_error = None;

    for _ in 0..runs {
        let result = catch_unwind(AssertUnwindSafe(&test_fn));
        match result {
            Ok(()) => passed_count += 1,
            Err(e) => {
                failed_count += 1;
                let error_msg = if let Some(s) = e.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic".to_string()
                };
                last_error = Some(error_msg);
            }
        }
    }

    // Test is flaky if it has mixed results
    let is_flaky = passed_count > 0 && failed_count > 0;

    if is_flaky {
        eprintln!(
            "[FLAKY DETECTED] Test passed {}/{} runs, failed {}/{}",
            passed_count, runs, failed_count, runs
        );
    }

    FlakyTestResult {
        passed: passed_count == runs,
        attempts: runs,
        last_error,
    }
}

/// Marker for tests that are known to be flaky.
///
/// Use this macro to document and handle known flaky tests:
///
/// ```rust,ignore
/// #[test]
/// fn known_flaky_test() {
///     known_flaky!("ISSUE-123", "Timing-sensitive live display test");
///     // test code...
/// }
/// ```
#[macro_export]
macro_rules! known_flaky {
    ($issue:expr, $reason:expr) => {
        // Log if in verbose mode
        if std::env::var("FLAKY_TEST_LOG").is_ok() {
            eprintln!(
                "[KNOWN FLAKY] {} - {} (see {})",
                module_path!(),
                $reason,
                $issue
            );
        }
    };
}

/// Log random seed for reproducibility in property tests.
///
/// When proptest fails, this helps reproduce the failure.
pub fn log_random_seed(test_name: &str) {
    if let Ok(seed) = env::var("PROPTEST_SEED") {
        eprintln!("[SEED] {}: PROPTEST_SEED={}", test_name, seed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn test_retry_passes_on_first_attempt() {
        let config = FlakyConfig::new(3, 10);
        let counter = AtomicU32::new(0);

        retry_test(config, || {
            counter.fetch_add(1, Ordering::SeqCst);
            // Test passes (no panic)
        });

        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_retry_succeeds_after_failures() {
        let config = FlakyConfig::new(3, 10).with_name("test_retry");
        let counter = AtomicU32::new(0);

        retry_test(config, || {
            let count = counter.fetch_add(1, Ordering::SeqCst);
            // Fail first two attempts
            if count < 2 {
                panic!("Simulated failure");
            }
        });

        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[test]
    #[should_panic(expected = "[FLAKY]")]
    fn test_retry_fails_after_all_attempts() {
        let config = FlakyConfig::new(2, 10);

        retry_test(config, || {
            panic!("Always fails");
        });
    }

    #[test]
    fn test_detect_flakiness_consistent_pass() {
        let result = detect_flakiness(|| { /* passes */ }, 5);
        assert!(result.passed);
        assert_eq!(result.attempts, 5);
        assert!(result.last_error.is_none());
    }

    #[test]
    fn test_detect_flakiness_consistent_fail() {
        let result = detect_flakiness(|| panic!("fail"), 3);
        assert!(!result.passed);
        assert_eq!(result.attempts, 3);
        assert!(result.last_error.is_some());
    }

    #[test]
    fn test_flaky_config_from_env() {
        // Default config should have reasonable values
        let config = FlakyConfig::default();
        assert!(config.max_attempts >= 1);
        assert!(config.retry_delay.as_millis() > 0);
    }

    #[test]
    fn test_no_retries_config() {
        let config = FlakyConfig::no_retries();
        assert_eq!(config.max_attempts, 1);
    }
}
