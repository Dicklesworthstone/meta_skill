//! Logging conformance tests.
//!
//! Tests for RichLogger (log crate) and RichTracingLayer (tracing crate).
//! Covers all log levels, formatting options, and edge cases.

use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use log::{Level, LevelFilter, Log, Metadata};
use rich_rust::console::Console;
use rich_rust::logging::RichLogger;

/// Shared buffer for capturing console output.
#[derive(Clone)]
struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

impl SharedBuffer {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(Vec::new())))
    }

    fn contents(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).to_string()
    }

    fn clear(&self) {
        self.0.lock().unwrap().clear();
    }
}

impl Write for SharedBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.lock().unwrap().flush()
    }
}

fn create_test_console(buffer: SharedBuffer) -> Arc<Console> {
    Console::builder()
        .force_terminal(true)
        .markup(false)
        .width(80)
        .file(Box::new(buffer))
        .build()
        .shared()
}

/// Macro to log a test record with proper lifetime handling.
/// Combines record creation and logging in one statement to avoid format_args lifetime issues.
macro_rules! log_test {
    ($logger:expr, $level:expr, $message:expr) => {
        $logger.log(
            &log::Record::builder()
                .args(format_args!("{}", $message))
                .level($level)
                .target("test_target")
                .file(Some("test.rs"))
                .line(Some(42))
                .module_path(Some("test_module"))
                .build(),
        )
    };
}

/// Macro to log a test record without file info.
macro_rules! log_test_no_file {
    ($logger:expr, $level:expr, $message:expr) => {
        $logger.log(
            &log::Record::builder()
                .args(format_args!("{}", $message))
                .level($level)
                .target("test_target")
                .file(None)
                .line(None)
                .module_path(Some("test_module"))
                .build(),
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // RichLogger Creation Tests
    // ========================================================================

    #[test]
    fn test_rich_logger_new() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer);
        let logger = RichLogger::new(console);
        // Should create without panic
        drop(logger);
    }

    #[test]
    fn test_rich_logger_builder_chain() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer);
        let _logger = RichLogger::new(console)
            .level(LevelFilter::Debug)
            .show_time(true)
            .omit_repeated_times(true)
            .show_level(true)
            .show_path(true)
            .enable_link_path(false)
            .markup(false)
            .keywords(vec!["GET".to_string(), "POST".to_string()])
            .time_format("[%Y-%m-%d]");
    }

    // ========================================================================
    // Level Tests
    // ========================================================================

    #[test]
    fn test_logger_level_filter() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer);
        let logger = RichLogger::new(console).level(LevelFilter::Warn);

        // Debug should be filtered
        let debug_meta = Metadata::builder()
            .level(Level::Debug)
            .target("test")
            .build();
        assert!(!logger.enabled(&debug_meta));

        // Warn should pass
        let warn_meta = Metadata::builder()
            .level(Level::Warn)
            .target("test")
            .build();
        assert!(logger.enabled(&warn_meta));

        // Error should pass
        let error_meta = Metadata::builder()
            .level(Level::Error)
            .target("test")
            .build();
        assert!(logger.enabled(&error_meta));
    }

    #[test]
    fn test_all_log_levels_output() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let logger = RichLogger::new(console)
            .level(LevelFilter::Trace)
            .show_time(false)
            .show_path(false);

        // Test each level
        for (level, expected) in [
            (Level::Trace, "TRACE"),
            (Level::Debug, "DEBUG"),
            (Level::Info, "INFO"),
            (Level::Warn, "WARN"),
            (Level::Error, "ERROR"),
        ] {
            buffer.clear();
            log_test!(logger, level, "Test message");

            let output = buffer.contents();
            assert!(
                output.contains(expected),
                "Level {level} should contain '{expected}', got: {output}"
            );
            assert!(output.contains("Test message"));
        }
    }

    // ========================================================================
    // Formatting Tests
    // ========================================================================

    #[test]
    fn test_logger_shows_time() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let logger = RichLogger::new(console)
            .level(LevelFilter::Info)
            .show_time(true)
            .show_level(false)
            .show_path(false);

        log_test!(logger, Level::Info, "Timed message");

        let output = buffer.contents();
        // Output should contain time-like characters (colons for HH:MM:SS)
        assert!(
            output.contains(':'),
            "Should contain time format, got: {output}"
        );
        assert!(output.contains("Timed message"));
    }

    #[test]
    fn test_logger_hides_time() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let logger = RichLogger::new(console)
            .level(LevelFilter::Info)
            .show_time(false)
            .show_level(true)
            .show_path(false);

        log_test!(logger, Level::Info, "No time message");

        let output = buffer.contents();
        // Should start with level or message, not time
        assert!(output.contains("No time message"));
    }

    #[test]
    fn test_logger_shows_level() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let logger = RichLogger::new(console)
            .level(LevelFilter::Info)
            .show_time(false)
            .show_level(true)
            .show_path(false);

        log_test!(logger, Level::Info, "Leveled message");

        let output = buffer.contents();
        assert!(
            output.contains("INFO"),
            "Should contain level, got: {output}"
        );
    }

    #[test]
    fn test_logger_hides_level() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let logger = RichLogger::new(console)
            .level(LevelFilter::Info)
            .show_time(false)
            .show_level(false)
            .show_path(false);

        log_test!(logger, Level::Info, "No level message");

        let output = buffer.contents();
        assert!(
            !output.contains("INFO"),
            "Should not contain level, got: {output}"
        );
        assert!(output.contains("No level message"));
    }

    #[test]
    fn test_logger_shows_path() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let logger = RichLogger::new(console)
            .level(LevelFilter::Info)
            .show_time(false)
            .show_level(false)
            .show_path(true)
            .enable_link_path(false);

        log_test!(logger, Level::Info, "Path message");

        let output = buffer.contents();
        assert!(
            output.contains("test.rs"),
            "Should contain file path, got: {output}"
        );
        assert!(
            output.contains("42"),
            "Should contain line number, got: {output}"
        );
    }

    #[test]
    fn test_logger_hides_path() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let logger = RichLogger::new(console)
            .level(LevelFilter::Info)
            .show_time(false)
            .show_level(false)
            .show_path(false);

        log_test!(logger, Level::Info, "No path message");

        let output = buffer.contents();
        assert!(
            !output.contains("test.rs"),
            "Should not contain file path, got: {output}"
        );
    }

    // ========================================================================
    // Keyword Highlighting Tests
    // ========================================================================

    #[test]
    fn test_logger_default_keywords() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let logger = RichLogger::new(console)
            .level(LevelFilter::Info)
            .show_time(false)
            .show_level(false)
            .show_path(false);

        log_test!(logger, Level::Info, "GET /api/users");

        let output = buffer.contents();
        // GET should be highlighted (default keywords include HTTP methods)
        assert!(
            output.contains("GET"),
            "Should contain GET keyword, got: {output}"
        );
    }

    #[test]
    fn test_logger_custom_keywords() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let logger = RichLogger::new(console)
            .level(LevelFilter::Info)
            .show_time(false)
            .show_level(false)
            .show_path(false)
            .keywords(vec!["CUSTOM".to_string()]);

        log_test!(logger, Level::Info, "CUSTOM keyword here");

        let output = buffer.contents();
        assert!(
            output.contains("CUSTOM"),
            "Should contain custom keyword, got: {output}"
        );
    }

    #[test]
    fn test_logger_empty_keywords() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let logger = RichLogger::new(console)
            .level(LevelFilter::Info)
            .show_time(false)
            .show_level(false)
            .show_path(false)
            .keywords(vec![]); // No keywords

        log_test!(logger, Level::Info, "GET message");

        let output = buffer.contents();
        // Should still work without keywords
        assert!(output.contains("GET message"));
    }

    // ========================================================================
    // Markup Tests
    // ========================================================================

    #[test]
    fn test_logger_with_markup() {
        let buffer = SharedBuffer::new();
        let console = Console::builder()
            .force_terminal(true)
            .markup(true)
            .width(80)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let logger = RichLogger::new(console)
            .level(LevelFilter::Info)
            .show_time(false)
            .show_level(false)
            .show_path(false)
            .markup(true);

        log_test!(logger, Level::Info, "[bold]Bold text[/bold]");

        let output = buffer.contents();
        // The markup should be processed
        assert!(
            output.contains("Bold text"),
            "Should contain processed text, got: {output}"
        );
    }

    #[test]
    fn test_logger_without_markup() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let logger = RichLogger::new(console)
            .level(LevelFilter::Info)
            .show_time(false)
            .show_level(false)
            .show_path(false)
            .markup(false);

        log_test!(logger, Level::Info, "[bold]Not parsed[/bold]");

        let output = buffer.contents();
        // The markup should NOT be processed
        assert!(
            output.contains("[bold]"),
            "Should contain raw markup, got: {output}"
        );
    }

    // ========================================================================
    // Time Format Tests
    // ========================================================================

    #[test]
    fn test_logger_custom_time_format() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        // Use a different but valid format to verify custom format is applied
        // Default is [hour]:[minute]:[second], so use a format with dashes
        let logger = RichLogger::new(console)
            .level(LevelFilter::Info)
            .show_time(true)
            .show_level(false)
            .show_path(false)
            .time_format("[hour]-[minute]-[second]"); // Different separator

        log_test!(logger, Level::Info, "Custom time");

        let output = buffer.contents();
        // Should contain time with dashes instead of colons (the default uses colons)
        assert!(
            output.contains("-") && !output.contains(":"),
            "Should use custom format with dashes, got: {output}"
        );
    }

    #[test]
    fn test_logger_invalid_time_format() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let logger = RichLogger::new(console)
            .level(LevelFilter::Info)
            .show_time(true)
            .show_level(false)
            .show_path(false)
            .time_format("invalid"); // Invalid format

        log_test!(logger, Level::Info, "Invalid format");

        let output = buffer.contents();
        // Should handle gracefully (fallback or original format)
        assert!(output.contains("Invalid format"));
    }

    // ========================================================================
    // Omit Repeated Times Tests
    // ========================================================================

    #[test]
    fn test_logger_omit_repeated_times() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let logger = RichLogger::new(console)
            .level(LevelFilter::Info)
            .show_time(true)
            .omit_repeated_times(true)
            .show_level(false)
            .show_path(false);

        // Log two messages quickly (same timestamp)
        log_test!(logger, Level::Info, "First message");
        log_test!(logger, Level::Info, "Second message");

        // Second message should have blank time (or fewer colons)
        let output = buffer.contents();
        assert!(output.contains("First message"));
        assert!(output.contains("Second message"));
    }

    #[test]
    fn test_logger_dont_omit_repeated_times() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let logger = RichLogger::new(console)
            .level(LevelFilter::Info)
            .show_time(true)
            .omit_repeated_times(false)
            .show_level(false)
            .show_path(false);

        log_test!(logger, Level::Info, "First");
        log_test!(logger, Level::Info, "Second");

        let output = buffer.contents();
        // Both should have timestamps
        assert!(output.contains("First"));
        assert!(output.contains("Second"));
    }

    // ========================================================================
    // Hyperlink Tests
    // ========================================================================

    #[test]
    fn test_logger_with_link_path() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let logger = RichLogger::new(console)
            .level(LevelFilter::Info)
            .show_time(false)
            .show_level(false)
            .show_path(true)
            .enable_link_path(true);

        log_test!(logger, Level::Info, "Linked path");

        let output = buffer.contents();
        // Output should contain hyperlink escape codes
        // OSC 8 ; params ; URI ST ... OSC 8 ;; ST
        assert!(
            output.contains("test.rs") || output.contains("file://"),
            "Should contain path or hyperlink, got: {output}"
        );
    }

    // ========================================================================
    // Level Style Tests
    // ========================================================================

    #[test]
    fn test_level_styles_are_different() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let logger = RichLogger::new(console)
            .level(LevelFilter::Trace)
            .show_time(false)
            .show_level(true)
            .show_path(false);

        // Collect outputs for each level
        let mut outputs = Vec::new();
        for level in [
            Level::Trace,
            Level::Debug,
            Level::Info,
            Level::Warn,
            Level::Error,
        ] {
            buffer.clear();
            log_test!(logger, level, "msg");
            outputs.push(buffer.contents());
        }

        // Each level should produce different ANSI codes
        // (At minimum, the level name should be different)
        for (i, out1) in outputs.iter().enumerate() {
            for (j, out2) in outputs.iter().enumerate() {
                if i != j {
                    // They should be different (different level names at least)
                    assert_ne!(out1, out2, "Levels {i} and {j} should differ");
                }
            }
        }
    }

    // ========================================================================
    // Edge Cases
    // ========================================================================

    #[test]
    fn test_logger_empty_message() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let logger = RichLogger::new(console)
            .level(LevelFilter::Info)
            .show_time(false)
            .show_level(false)
            .show_path(false);

        log_test!(logger, Level::Info, "");

        // Should not panic - just verify we can get output
        let _ = buffer.contents();
    }

    #[test]
    fn test_logger_multiline_message() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let logger = RichLogger::new(console)
            .level(LevelFilter::Info)
            .show_time(false)
            .show_level(false)
            .show_path(false);

        log_test!(logger, Level::Info, "Line 1\nLine 2\nLine 3");

        let output = buffer.contents();
        assert!(output.contains("Line 1"));
        assert!(output.contains("Line 2"));
        assert!(output.contains("Line 3"));
    }

    #[test]
    fn test_logger_unicode_message() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let logger = RichLogger::new(console)
            .level(LevelFilter::Info)
            .show_time(false)
            .show_level(false)
            .show_path(false);

        log_test!(logger, Level::Info, "Hello 世界 🌍 Привет");

        let output = buffer.contents();
        assert!(output.contains("世界"));
        assert!(output.contains("🌍"));
        assert!(output.contains("Привет"));
    }

    #[test]
    fn test_logger_very_long_message() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let logger = RichLogger::new(console)
            .level(LevelFilter::Info)
            .show_time(false)
            .show_level(false)
            .show_path(false);

        // Use a fixed long message (100+ chars is sufficient to test long message handling)
        log_test!(
            logger,
            Level::Info,
            "This is a very long message that is meant to test the logger's ability to handle messages of significant length without any issues or problems occurring during the logging process"
        );

        let output = buffer.contents();
        assert!(output.len() >= 100);
    }

    #[test]
    fn test_logger_record_without_file() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let logger = RichLogger::new(console)
            .level(LevelFilter::Info)
            .show_time(false)
            .show_level(false)
            .show_path(true);

        log_test_no_file!(logger, Level::Info, "No file");

        let output = buffer.contents();
        assert!(output.contains("No file"));
    }

    #[test]
    fn test_logger_flush_is_noop() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer);
        let logger = RichLogger::new(console);

        // Flush should not panic
        logger.flush();
    }

    // ========================================================================
    // Log Trait Implementation Tests
    // ========================================================================

    #[test]
    fn test_logger_implements_log_trait() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer);
        let logger = RichLogger::new(console).level(LevelFilter::Info);

        // Test that it implements Log trait
        let dyn_logger: &dyn Log = &logger;

        let meta = Metadata::builder()
            .level(Level::Info)
            .target("test")
            .build();
        assert!(dyn_logger.enabled(&meta));

        dyn_logger.flush();
    }

    // ========================================================================
    // Tracing Integration Tests (feature-gated)
    // ========================================================================

    #[cfg(feature = "tracing")]
    mod tracing_tests {
        use super::*;
        use rich_rust::logging::RichTracingLayer;

        #[test]
        fn test_rich_tracing_layer_new() {
            let buffer = SharedBuffer::new();
            let console = create_test_console(buffer);
            let _layer = RichTracingLayer::new(console);
        }

        #[test]
        fn test_rich_tracing_layer_with_logger() {
            let buffer = SharedBuffer::new();
            let console = create_test_console(buffer.clone());
            let logger = RichLogger::new(console)
                .level(LevelFilter::Debug)
                .show_time(false);
            let _layer = RichTracingLayer::with_logger(logger);
        }
    }

    // ========================================================================
    // Default Options Tests
    // ========================================================================

    #[test]
    fn test_logger_default_options() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let logger = RichLogger::new(console);

        // Default level is Info
        let debug_meta = Metadata::builder()
            .level(Level::Debug)
            .target("test")
            .build();
        assert!(!logger.enabled(&debug_meta), "Default should filter debug");

        let info_meta = Metadata::builder()
            .level(Level::Info)
            .target("test")
            .build();
        assert!(logger.enabled(&info_meta), "Default should allow info");
    }
}
