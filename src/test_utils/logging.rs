//! Test logging infrastructure with structured log capture and assertions.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use tracing::Level;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;

/// Global log storage for test assertions.
static LOG_STORAGE: OnceLock<Arc<Mutex<LogStorage>>> = OnceLock::new();

/// Storage for captured log entries.
#[derive(Default)]
pub struct LogStorage {
    entries: VecDeque<LogEntry>,
    max_entries: usize,
}

impl LogStorage {
    #[must_use]
    pub const fn new(max_entries: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            max_entries,
        }
    }

    pub fn push(&mut self, entry: LogEntry) {
        if self.entries.len() >= self.max_entries {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    #[must_use]
    pub const fn entries(&self) -> &VecDeque<LogEntry> {
        &self.entries
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    #[must_use]
    pub fn contains_message(&self, message: &str) -> bool {
        self.entries.iter().any(|e| e.message.contains(message))
    }

    #[must_use]
    pub fn contains_level(&self, level: Level) -> bool {
        self.entries.iter().any(|e| e.level == level)
    }

    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.contains_level(Level::ERROR)
    }

    #[must_use]
    pub fn has_warnings(&self) -> bool {
        self.contains_level(Level::WARN)
    }

    #[must_use]
    pub fn filter_by_level(&self, level: Level) -> Vec<&LogEntry> {
        self.entries.iter().filter(|e| e.level == level).collect()
    }

    #[must_use]
    pub fn filter_by_message(&self, pattern: &str) -> Vec<&LogEntry> {
        self.entries
            .iter()
            .filter(|e| e.message.contains(pattern))
            .collect()
    }
}

/// A captured log entry.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub level: Level,
    pub target: String,
    pub message: String,
    pub timestamp: Instant,
    pub fields: Vec<(String, String)>,
}

impl LogEntry {
    #[must_use]
    pub fn new(level: Level, target: &str, message: &str) -> Self {
        Self {
            level,
            target: target.to_string(),
            message: message.to_string(),
            timestamp: Instant::now(),
            fields: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_field(mut self, key: &str, value: &str) -> Self {
        self.fields.push((key.to_string(), value.to_string()));
        self
    }

    /// Format as JSON for structured output.
    #[must_use]
    pub fn to_json(&self) -> String {
        let fields_json: Vec<String> = self
            .fields
            .iter()
            .map(|(k, v)| format!(r#""{}":"{}""#, k, v.replace('"', "\\\"")))
            .collect();

        let fields_str = if fields_json.is_empty() {
            String::new()
        } else {
            format!(", {}", fields_json.join(", "))
        };

        format!(
            r#"{{"level":"{}","target":"{}","message":"{}"{}}}"#,
            self.level,
            self.target.replace('"', "\\\""),
            self.message.replace('"', "\\\""),
            fields_str
        )
    }
}

/// Get or initialize the global log storage.
pub fn get_log_storage() -> Arc<Mutex<LogStorage>> {
    LOG_STORAGE
        .get_or_init(|| Arc::new(Mutex::new(LogStorage::new(1000))))
        .clone()
}

/// Clear all captured logs.
pub fn clear_logs() {
    if let Ok(mut storage) = get_log_storage().lock() {
        storage.clear();
    }
}

/// Get all captured log entries.
#[must_use]
pub fn get_logs() -> Vec<LogEntry> {
    if let Ok(storage) = get_log_storage().lock() {
        storage.entries().iter().cloned().collect()
    } else {
        Vec::new()
    }
}

/// Check if logs contain a message.
#[must_use]
pub fn logs_contain(message: &str) -> bool {
    if let Ok(storage) = get_log_storage().lock() {
        storage.contains_message(message)
    } else {
        false
    }
}

/// Check if logs have any errors.
#[must_use]
pub fn logs_have_errors() -> bool {
    if let Ok(storage) = get_log_storage().lock() {
        storage.has_errors()
    } else {
        false
    }
}

/// Check if logs have any warnings.
#[must_use]
pub fn logs_have_warnings() -> bool {
    if let Ok(storage) = get_log_storage().lock() {
        storage.has_warnings()
    } else {
        false
    }
}

/// Format logs for display on test failure.
#[must_use]
pub fn format_logs_for_display() -> String {
    let logs = get_logs();
    if logs.is_empty() {
        return String::from("No logs captured");
    }

    let mut output = String::new();
    output.push_str(&format!("Captured {} log entries:\n", logs.len()));
    output.push_str(&"─".repeat(60));
    output.push('\n');

    for entry in logs {
        let level_indicator = match entry.level {
            Level::ERROR => "✗",
            Level::WARN => "⚠",
            Level::INFO => "ℹ",
            Level::DEBUG => "🔍",
            Level::TRACE => "→",
        };

        output.push_str(&format!(
            "{} [{}] {}: {}\n",
            level_indicator, entry.level, entry.target, entry.message
        ));

        for (key, value) in &entry.fields {
            output.push_str(&format!("    {key} = {value}\n"));
        }
    }

    output.push_str(&"─".repeat(60));
    output
}

/// Custom layer for capturing logs during tests.
pub struct TestLogLayer {
    storage: Arc<Mutex<LogStorage>>,
}

impl TestLogLayer {
    pub const fn new(storage: Arc<Mutex<LogStorage>>) -> Self {
        Self { storage }
    }
}

impl<S> tracing_subscriber::Layer<S> for TestLogLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let metadata = event.metadata();
        let level = *metadata.level();
        let target = metadata.target().to_string();

        // Extract message from event
        let mut message = String::new();
        let mut fields = Vec::new();

        struct MessageVisitor<'a> {
            message: &'a mut String,
            fields: &'a mut Vec<(String, String)>,
        }

        impl tracing::field::Visit for MessageVisitor<'_> {
            fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                if field.name() == "message" {
                    *self.message = value.to_string();
                } else {
                    self.fields
                        .push((field.name().to_string(), value.to_string()));
                }
            }

            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                let value_str = format!("{value:?}");
                if field.name() == "message" {
                    *self.message = value_str;
                } else {
                    self.fields.push((field.name().to_string(), value_str));
                }
            }
        }

        let mut visitor = MessageVisitor {
            message: &mut message,
            fields: &mut fields,
        };
        event.record(&mut visitor);

        let mut entry = LogEntry::new(level, &target, &message);
        entry.fields = fields;

        if let Ok(mut storage) = self.storage.lock() {
            storage.push(entry);
        }
    }
}

/// Initialize test logging with the specified level.
///
/// Call this at the start of your test to enable log capture.
/// Returns a guard that will print logs on drop if the test failed.
#[must_use]
pub fn init_test_logging(level: &str) -> TestLoggingGuard {
    let storage = get_log_storage();

    // Clear previous logs
    if let Ok(mut s) = storage.lock() {
        s.clear();
    }

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    let test_layer = TestLogLayer::new(storage);

    let subscriber = tracing_subscriber::registry().with(filter).with(test_layer);

    // Try to set as global subscriber (will fail if already set, which is OK)
    let _ = tracing::subscriber::set_global_default(subscriber);

    TestLoggingGuard {
        start_time: Instant::now(),
        test_name: String::new(),
        print_on_failure: true,
    }
}

/// Guard that optionally prints logs when a test fails.
pub struct TestLoggingGuard {
    start_time: Instant,
    test_name: String,
    print_on_failure: bool,
}

impl TestLoggingGuard {
    #[must_use]
    pub fn with_name(mut self, name: &str) -> Self {
        self.test_name = name.to_string();
        self
    }

    #[must_use]
    pub const fn no_print_on_failure(mut self) -> Self {
        self.print_on_failure = false;
        self
    }

    #[must_use]
    pub fn elapsed(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }
}

impl Drop for TestLoggingGuard {
    fn drop(&mut self) {
        if self.print_on_failure && std::thread::panicking() {
            eprintln!("\n{}", "═".repeat(60));
            if !self.test_name.is_empty() {
                eprintln!("TEST FAILED: {}", self.test_name);
            }
            eprintln!("Duration: {:?}", self.elapsed());
            eprintln!("{}", format_logs_for_display());
            eprintln!("{}\n", "═".repeat(60));
        }
    }
}

// =============================================================================
// Assertion Macros
// =============================================================================

/// Assert that a log entry with the specified level and message exists.
#[macro_export]
macro_rules! assert_log_contains {
    ($level:expr, $message:expr) => {{
        let logs = $crate::test_utils::logging::get_logs();
        let found = logs
            .iter()
            .any(|e| e.level == $level && e.message.contains($message));
        assert!(
            found,
            "Expected log with level {} containing '{}'\nCaptured logs:\n{}",
            $level,
            $message,
            $crate::test_utils::logging::format_logs_for_display()
        );
    }};
}

/// Assert that a log entry matches the specified pattern.
#[macro_export]
macro_rules! assert_log_matches {
    ($pattern:expr) => {{
        let pattern = regex::Regex::new($pattern).expect("Invalid regex pattern");
        let logs = $crate::test_utils::logging::get_logs();
        let found = logs.iter().any(|e| pattern.is_match(&e.message));
        assert!(
            found,
            "Expected log matching pattern '{}'\nCaptured logs:\n{}",
            $pattern,
            $crate::test_utils::logging::format_logs_for_display()
        );
    }};
}

/// Assert that no error logs were recorded.
#[macro_export]
macro_rules! assert_no_errors {
    () => {{
        let has_errors = $crate::test_utils::logging::logs_have_errors();
        assert!(
            !has_errors,
            "Expected no errors but found some:\n{}",
            $crate::test_utils::logging::format_logs_for_display()
        );
    }};
}

/// Assert that no warning logs were recorded.
#[macro_export]
macro_rules! assert_no_warnings {
    () => {{
        let has_warnings = $crate::test_utils::logging::logs_have_warnings();
        assert!(
            !has_warnings,
            "Expected no warnings but found some:\n{}",
            $crate::test_utils::logging::format_logs_for_display()
        );
    }};
}

// =============================================================================
// Legacy TestLogger (kept for backward compatibility)
// =============================================================================

/// Basic test logger with timing and formatted output.
///
/// For new code, prefer using `init_test_logging()` which provides
/// structured logging with capture and assertions.
pub struct TestLogger {
    test_name: String,
    start_time: Instant,
}

impl TestLogger {
    #[must_use]
    pub fn new(test_name: &str) -> Self {
        let separator = "=".repeat(60);
        println!("\n{separator}");
        println!("[TEST START] {test_name}");
        println!("{separator}");
        Self {
            test_name: test_name.to_string(),
            start_time: Instant::now(),
        }
    }

    pub fn log_input<T: std::fmt::Debug>(&self, name: &str, value: &T) {
        println!("[INPUT] {name}: {value:?}");
    }

    pub fn log_expected<T: std::fmt::Debug>(&self, value: &T) {
        println!("[EXPECTED] {value:?}");
    }

    pub fn log_actual<T: std::fmt::Debug>(&self, value: &T) {
        println!("[ACTUAL] {value:?}");
    }

    pub fn pass(&self) {
        let elapsed = self.start_time.elapsed();
        println!("[RESULT] PASSED in {elapsed:?}");
        println!("{}\n", "=".repeat(60));
    }

    pub fn fail(&self, reason: &str) {
        let elapsed = self.start_time.elapsed();
        println!("[RESULT] FAILED in {elapsed:?}");
        println!("[REASON] {reason}");
        println!("{}\n", "=".repeat(60));
    }

    #[allow(dead_code)]
    #[must_use]
    pub fn test_name(&self) -> &str {
        &self.test_name
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_entry_creation() {
        let entry = LogEntry::new(Level::INFO, "test::module", "Test message");
        assert_eq!(entry.level, Level::INFO);
        assert_eq!(entry.target, "test::module");
        assert_eq!(entry.message, "Test message");
    }

    #[test]
    fn test_log_entry_with_fields() {
        let entry = LogEntry::new(Level::DEBUG, "test", "Debug message")
            .with_field("key1", "value1")
            .with_field("key2", "value2");

        assert_eq!(entry.fields.len(), 2);
        assert_eq!(entry.fields[0], ("key1".to_string(), "value1".to_string()));
    }

    #[test]
    fn test_log_entry_to_json() {
        let entry =
            LogEntry::new(Level::ERROR, "test::json", "Error occurred").with_field("code", "500");

        let json = entry.to_json();
        assert!(json.contains("\"level\":\"ERROR\""));
        assert!(json.contains("\"target\":\"test::json\""));
        assert!(json.contains("\"message\":\"Error occurred\""));
        assert!(json.contains("\"code\":\"500\""));
    }

    #[test]
    fn test_log_storage_operations() {
        let mut storage = LogStorage::new(10);

        storage.push(LogEntry::new(Level::INFO, "test", "Info message"));
        storage.push(LogEntry::new(Level::ERROR, "test", "Error message"));

        assert_eq!(storage.entries().len(), 2);
        assert!(storage.contains_message("Info message"));
        assert!(storage.has_errors());
        assert!(!storage.has_warnings());
    }

    #[test]
    fn test_log_storage_max_entries() {
        let mut storage = LogStorage::new(3);

        for i in 0..5 {
            storage.push(LogEntry::new(
                Level::INFO,
                "test",
                &format!("Message {}", i),
            ));
        }

        assert_eq!(storage.entries().len(), 3);
        // First two should be dropped
        assert!(!storage.contains_message("Message 0"));
        assert!(!storage.contains_message("Message 1"));
        assert!(storage.contains_message("Message 4"));
    }

    #[test]
    fn test_log_storage_filter_by_level() {
        let mut storage = LogStorage::new(10);
        storage.push(LogEntry::new(Level::INFO, "test", "Info 1"));
        storage.push(LogEntry::new(Level::ERROR, "test", "Error 1"));
        storage.push(LogEntry::new(Level::INFO, "test", "Info 2"));

        let info_logs = storage.filter_by_level(Level::INFO);
        assert_eq!(info_logs.len(), 2);

        let error_logs = storage.filter_by_level(Level::ERROR);
        assert_eq!(error_logs.len(), 1);
    }

    #[test]
    fn test_format_logs_empty() {
        clear_logs();
        let formatted = format_logs_for_display();
        assert!(formatted.contains("No logs captured"));
    }

    #[test]
    fn test_test_logger_basic() {
        let logger = TestLogger::new("test_basic");
        logger.log_input("value", &42);
        logger.log_expected(&"expected");
        logger.log_actual(&"actual");
        logger.pass();
        assert_eq!(logger.test_name(), "test_basic");
    }
}
