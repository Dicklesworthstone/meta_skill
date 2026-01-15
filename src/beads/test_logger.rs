//! Structured test logging infrastructure for beads tests.
//!
//! Provides detailed, machine-parseable output for debugging test failures
//! and verifying correct behavior. Especially critical for WAL safety tests.

use std::path::Path;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::MsError;

/// Structured test logger for beads operations.
#[derive(Debug)]
pub struct TestLogger {
    test_name: String,
    log_entries: Vec<LogEntry>,
    start_time: Instant,
    verbose: bool,
}

/// A single log entry with optional details and timing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp_ms: u64,
    pub level: LogLevel,
    pub category: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

/// Log severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
    Success,
}

impl TestLogger {
    /// Create a new test logger for the given test.
    ///
    /// Checks BEADS_TEST_VERBOSE environment variable to enable verbose output.
    pub fn new(test_name: &str) -> Self {
        let verbose = std::env::var("BEADS_TEST_VERBOSE").is_ok();
        let mut logger = TestLogger {
            test_name: test_name.to_string(),
            log_entries: Vec::new(),
            start_time: Instant::now(),
            verbose,
        };
        logger.info("TEST_START", &format!("Starting test: {}", test_name), None);
        logger
    }

    /// Log an info-level message.
    pub fn info(&mut self, category: &str, message: &str, details: Option<serde_json::Value>) {
        self.log(LogLevel::Info, category, message, details, None);
    }

    /// Log a debug-level message.
    pub fn debug(&mut self, category: &str, message: &str, details: Option<serde_json::Value>) {
        self.log(LogLevel::Debug, category, message, details, None);
    }

    /// Log a warning-level message.
    pub fn warn(&mut self, category: &str, message: &str, details: Option<serde_json::Value>) {
        self.log(LogLevel::Warn, category, message, details, None);
    }

    /// Log an error-level message.
    pub fn error(&mut self, category: &str, message: &str, details: Option<serde_json::Value>) {
        self.log(LogLevel::Error, category, message, details, None);
    }

    /// Log a success-level message.
    pub fn success(&mut self, category: &str, message: &str, details: Option<serde_json::Value>) {
        self.log(LogLevel::Success, category, message, details, None);
    }

    /// Log and time an operation.
    ///
    /// Returns the result of the operation.
    pub fn timed<T, F>(&mut self, category: &str, message: &str, f: F) -> T
    where
        F: FnOnce() -> T,
    {
        let start = Instant::now();
        self.debug(category, &format!("Starting: {}", message), None);
        let result = f();
        let duration = start.elapsed();
        self.log(
            LogLevel::Info,
            category,
            &format!(
                "Completed: {} ({:.2}ms)",
                message,
                duration.as_secs_f64() * 1000.0
            ),
            None,
            Some(duration.as_millis() as u64),
        );
        result
    }

    /// Log a bd command execution.
    pub fn log_bd_command(&mut self, args: &[&str], result: &Result<String, MsError>) {
        let cmd_str = format!("bd {}", args.join(" "));

        match result {
            Ok(output) => {
                let preview = if output.len() > 200 {
                    format!("{}...", &output[..200])
                } else {
                    output.clone()
                };

                self.log(
                    LogLevel::Success,
                    "BD_COMMAND",
                    &format!("bd {} succeeded", args.first().unwrap_or(&"?")),
                    Some(serde_json::json!({
                        "command": cmd_str,
                        "args": args,
                        "output_len": output.len(),
                        "output_preview": preview,
                    })),
                    None,
                );
            }
            Err(e) => {
                self.log(
                    LogLevel::Error,
                    "BD_COMMAND",
                    &format!("bd {} failed: {}", args.first().unwrap_or(&"?"), e),
                    Some(serde_json::json!({
                        "command": cmd_str,
                        "args": args,
                        "error": e.to_string(),
                    })),
                    None,
                );
            }
        }
    }

    fn log(
        &mut self,
        level: LogLevel,
        category: &str,
        message: &str,
        details: Option<serde_json::Value>,
        duration_ms: Option<u64>,
    ) {
        let entry = LogEntry {
            timestamp_ms: self.start_time.elapsed().as_millis() as u64,
            level,
            category: category.to_string(),
            message: message.to_string(),
            details,
            duration_ms,
        };

        if self.verbose {
            eprintln!(
                "[{:>8}ms] [{:?}] [{}] {}",
                entry.timestamp_ms, entry.level, entry.category, entry.message
            );
            if let Some(ref d) = entry.details {
                if let Ok(pretty) = serde_json::to_string_pretty(d) {
                    for line in pretty.lines() {
                        eprintln!("            {}", line);
                    }
                }
            }
        }

        self.log_entries.push(entry);
    }

    /// Generate final test report.
    pub fn report(&self) -> TestReport {
        let total_duration = self.start_time.elapsed();
        let errors = self
            .log_entries
            .iter()
            .filter(|e| matches!(e.level, LogLevel::Error))
            .count();
        let warnings = self
            .log_entries
            .iter()
            .filter(|e| matches!(e.level, LogLevel::Warn))
            .count();

        TestReport {
            test_name: self.test_name.clone(),
            total_duration_ms: total_duration.as_millis() as u64,
            entry_count: self.log_entries.len(),
            error_count: errors,
            warning_count: warnings,
            passed: errors == 0,
            entries: self.log_entries.clone(),
        }
    }

    /// Write report to file.
    pub fn write_report(&self, path: &Path) -> std::io::Result<()> {
        let report = self.report();
        let json = serde_json::to_string_pretty(&report)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, json)
    }

    /// Get elapsed time since test start.
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Check if any errors have been logged.
    pub fn has_errors(&self) -> bool {
        self.log_entries
            .iter()
            .any(|e| matches!(e.level, LogLevel::Error))
    }

    /// Get number of entries logged.
    pub fn entry_count(&self) -> usize {
        self.log_entries.len()
    }
}

/// Final test report with summary and all entries.
#[derive(Debug, Serialize, Deserialize)]
pub struct TestReport {
    pub test_name: String,
    pub total_duration_ms: u64,
    pub entry_count: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub passed: bool,
    pub entries: Vec<LogEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logger_creation() {
        let logger = TestLogger::new("test_creation");
        assert_eq!(logger.entry_count(), 1); // TEST_START entry
        assert!(!logger.has_errors());
    }

    #[test]
    fn test_logger_info() {
        let mut logger = TestLogger::new("test_info");
        logger.info("TEST", "Test message", None);
        assert_eq!(logger.entry_count(), 2);

        let report = logger.report();
        assert!(report.passed);
        assert_eq!(report.error_count, 0);
    }

    #[test]
    fn test_logger_with_details() {
        let mut logger = TestLogger::new("test_details");
        logger.info(
            "TEST",
            "With details",
            Some(serde_json::json!({"key": "value"})),
        );

        let report = logger.report();
        assert!(report.entries.last().unwrap().details.is_some());
    }

    #[test]
    fn test_logger_error() {
        let mut logger = TestLogger::new("test_error");
        logger.error("TEST", "An error occurred", None);

        assert!(logger.has_errors());
        let report = logger.report();
        assert!(!report.passed);
        assert_eq!(report.error_count, 1);
    }

    #[test]
    fn test_logger_timed() {
        let mut logger = TestLogger::new("test_timed");

        let result = logger.timed("COMPUTE", "Simple operation", || {
            std::thread::sleep(std::time::Duration::from_millis(10));
            42
        });

        assert_eq!(result, 42);
        // Should have TEST_START + debug (Starting) + info (Completed)
        assert_eq!(logger.entry_count(), 3);

        // Check that timing was recorded
        let report = logger.report();
        let timed_entry = report
            .entries
            .iter()
            .find(|e| e.message.contains("Completed"))
            .unwrap();
        assert!(timed_entry.duration_ms.is_some());
        assert!(timed_entry.duration_ms.unwrap() >= 10);
    }

    #[test]
    fn test_logger_bd_command_success() {
        let mut logger = TestLogger::new("test_bd_success");
        let result: Result<String, MsError> = Ok("Issue created".to_string());
        logger.log_bd_command(&["create", "--title", "Test"], &result);

        let report = logger.report();
        assert!(report.passed);
        let bd_entry = report
            .entries
            .iter()
            .find(|e| e.category == "BD_COMMAND")
            .unwrap();
        assert_eq!(bd_entry.level, LogLevel::Success);
    }

    #[test]
    fn test_logger_bd_command_failure() {
        let mut logger = TestLogger::new("test_bd_failure");
        let result: Result<String, MsError> =
            Err(MsError::NotFound("Issue not found".to_string()));
        logger.log_bd_command(&["show", "nonexistent"], &result);

        assert!(logger.has_errors());
        let report = logger.report();
        assert!(!report.passed);
    }

    #[test]
    fn test_logger_report_serialization() {
        let mut logger = TestLogger::new("test_serialization");
        logger.info("TEST", "Message 1", None);
        logger.warn("TEST", "Warning", None);
        logger.success("TEST", "Done", None);

        let report = logger.report();
        let json = serde_json::to_string_pretty(&report).unwrap();

        // Verify it can be deserialized back
        let parsed: TestReport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.test_name, "test_serialization");
        assert_eq!(parsed.warning_count, 1);
    }

    #[test]
    fn test_log_level_serialization() {
        let levels = [
            (LogLevel::Debug, "\"debug\""),
            (LogLevel::Info, "\"info\""),
            (LogLevel::Warn, "\"warn\""),
            (LogLevel::Error, "\"error\""),
            (LogLevel::Success, "\"success\""),
        ];

        for (level, expected) in levels {
            let json = serde_json::to_string(&level).unwrap();
            assert_eq!(json, expected);
        }
    }

    #[test]
    fn test_log_entry_skip_serializing_none() {
        let entry = LogEntry {
            timestamp_ms: 0,
            level: LogLevel::Info,
            category: "TEST".to_string(),
            message: "Test".to_string(),
            details: None,
            duration_ms: None,
        };

        let json = serde_json::to_string(&entry).unwrap();
        // None fields should not appear in JSON
        assert!(!json.contains("details"));
        assert!(!json.contains("duration_ms"));
    }
}
