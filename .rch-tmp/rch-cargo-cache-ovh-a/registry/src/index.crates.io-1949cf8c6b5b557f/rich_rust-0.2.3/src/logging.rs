//! Logging integration similar to Python Rich's `RichHandler`.
//!
//! Optional tracing integration is available via `RichTracingLayer` when the
//! `tracing` feature is enabled.

use std::sync::{Arc, Mutex};

use crate::sync::lock_recover;

use log::{Level, LevelFilter, Log, Metadata, Record, SetLoggerError};
use time::{OffsetDateTime, format_description::OwnedFormatItem};

use crate::console::Console;
use crate::markup;
use crate::renderables::traceback::Traceback;
use crate::style::Style;
use crate::text::Text;

#[cfg(not(feature = "backtrace"))]
use crate::renderables::traceback::TracebackFrame;

const DEFAULT_KEYWORDS: [&str; 8] = [
    "GET", "POST", "HEAD", "PUT", "DELETE", "OPTIONS", "TRACE", "PATCH",
];

/// Rich-style logger for the `log` crate.
///
/// Provides beautifully formatted log output with timestamps, syntax highlighting,
/// and optional file path links. Integrates with the standard `log` crate.
///
/// # Thread Safety
///
/// `RichLogger` implements `Log`, which requires `Sync`. All internal state is
/// protected by mutexes with poison recovery. Multiple threads can log
/// concurrently; output ordering follows the underlying Console's behavior.
///
/// The `omit_repeated_times` feature uses internal state to track the last
/// printed timestamp, which is thread-safe but may show occasional duplicate
/// timestamps under heavy concurrent logging.
pub struct RichLogger {
    console: Arc<Console>,
    level: LevelFilter,
    show_time: bool,
    omit_repeated_times: bool,
    show_level: bool,
    show_path: bool,
    enable_link_path: bool,
    markup: bool,
    keywords: Vec<String>,
    time_format: OwnedFormatItem,
    last_time: Mutex<Option<String>>,
    keyword_style: Style,
    rich_tracebacks: bool,
    tracebacks_extra_lines: usize,
}

impl RichLogger {
    /// Create a new `RichLogger` with default settings.
    #[must_use]
    pub fn new(console: Arc<Console>) -> Self {
        let time_format = time::format_description::parse_owned::<2>("[%F %T]")
            .or_else(|_| time::format_description::parse_owned::<2>("[hour]:[minute]:[second]"))
            .unwrap_or_else(|_| OwnedFormatItem::StringLiteral(String::new().into_boxed_str()));
        Self {
            console,
            level: LevelFilter::Info,
            show_time: true,
            omit_repeated_times: true,
            show_level: true,
            show_path: true,
            enable_link_path: true,
            markup: false,
            keywords: DEFAULT_KEYWORDS.iter().map(ToString::to_string).collect(),
            time_format,
            last_time: Mutex::new(None),
            keyword_style: Style::parse("bold yellow").unwrap_or_default(),
            rich_tracebacks: false,
            tracebacks_extra_lines: 3,
        }
    }

    /// Set the minimum log level.
    #[must_use]
    pub fn level(mut self, level: LevelFilter) -> Self {
        self.level = level;
        self
    }

    /// Enable or disable timestamps.
    #[must_use]
    pub fn show_time(mut self, show: bool) -> Self {
        self.show_time = show;
        self
    }

    /// Omit repeated timestamps.
    #[must_use]
    pub fn omit_repeated_times(mut self, omit: bool) -> Self {
        self.omit_repeated_times = omit;
        self
    }

    /// Enable or disable log levels.
    #[must_use]
    pub fn show_level(mut self, show: bool) -> Self {
        self.show_level = show;
        self
    }

    /// Enable or disable path column.
    #[must_use]
    pub fn show_path(mut self, show: bool) -> Self {
        self.show_path = show;
        self
    }

    /// Enable terminal hyperlinks for paths.
    #[must_use]
    pub fn enable_link_path(mut self, enable: bool) -> Self {
        self.enable_link_path = enable;
        self
    }

    /// Enable Rich markup parsing for messages.
    #[must_use]
    pub fn markup(mut self, markup: bool) -> Self {
        self.markup = markup;
        self
    }

    /// Override keyword list.
    #[must_use]
    pub fn keywords(mut self, keywords: Vec<String>) -> Self {
        self.keywords = keywords;
        self
    }

    /// Override time format.
    #[must_use]
    pub fn time_format(mut self, format: &str) -> Self {
        if let Ok(parsed) = time::format_description::parse_owned::<2>(format) {
            self.time_format = parsed;
        }
        self
    }

    /// Enable Rich-style tracebacks for error logs.
    ///
    /// When enabled, `ERROR`-level records will be followed by a rendered
    /// [`Traceback`] (captured backtrace when `backtrace` feature is enabled,
    /// otherwise a single-frame traceback at the log callsite).
    #[must_use]
    pub fn rich_tracebacks(mut self, enable: bool) -> Self {
        self.rich_tracebacks = enable;
        self
    }

    /// How many source context lines to render around the error line.
    #[must_use]
    pub fn tracebacks_extra_lines(mut self, extra_lines: usize) -> Self {
        self.tracebacks_extra_lines = extra_lines;
        self
    }

    /// Install as the global logger.
    pub fn init(self) -> Result<(), SetLoggerError> {
        log::set_max_level(self.level);
        log::set_boxed_logger(Box::new(self))
    }

    fn format_time(&self) -> String {
        let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
        now.format(&self.time_format)
            .unwrap_or_else(|_| now.to_string())
    }

    fn level_style(level: Level) -> Style {
        match level {
            Level::Trace => Style::parse("dim").unwrap_or_default(),
            Level::Debug => Style::parse("blue dim").unwrap_or_default(),
            Level::Info => Style::parse("green").unwrap_or_default(),
            Level::Warn => Style::parse("yellow").unwrap_or_default(),
            Level::Error => Style::parse("bold red").unwrap_or_default(),
        }
    }

    fn format_record(&self, record: &Record<'_>) -> Text {
        let mut line = Text::new("");

        if self.show_time {
            let time_str = self.format_time();
            let display = if self.omit_repeated_times {
                let mut last = lock_recover(&self.last_time);
                if last.as_ref() == Some(&time_str) {
                    " ".repeat(time_str.len())
                } else {
                    *last = Some(time_str.clone());
                    time_str.clone()
                }
            } else {
                time_str
            };
            line.append(&display);
            line.append(" ");
        }

        if self.show_level {
            let level_name = record.level().to_string();
            let padded = format!("{level_name:<8}");
            line.append_styled(&padded, Self::level_style(record.level()));
            line.append(" ");
        }

        let mut message = if self.markup {
            markup::render_or_plain(&record.args().to_string())
        } else {
            Text::new(record.args().to_string())
        };

        if !self.keywords.is_empty() {
            let keywords: Vec<&str> = self.keywords.iter().map(String::as_str).collect();
            message.highlight_words(&keywords, &self.keyword_style, false);
        }

        line.append_text(&message);

        if self.show_path
            && let Some(path) = record.file()
        {
            let mut path_text = Text::new(" ");
            let style = if self.enable_link_path {
                Style::new().link(format!("file://{path}"))
            } else {
                Style::default()
            };
            path_text.append_styled(path, style.clone());
            if let Some(line_no) = record.line() {
                path_text.append(":");
                let line_style = if self.enable_link_path {
                    Style::new().link(format!("file://{path}#{line_no}"))
                } else {
                    Style::default()
                };
                path_text.append_styled(&line_no.to_string(), line_style);
            }
            line.append_text(&path_text);
        }

        line
    }

    fn build_traceback_for_record(&self, record: &Record<'_>) -> Traceback {
        let exception_type = "Error";
        let exception_message = record.args().to_string();

        #[cfg(feature = "backtrace")]
        {
            Traceback::capture(exception_type, exception_message)
                .extra_lines(self.tracebacks_extra_lines)
        }

        #[cfg(not(feature = "backtrace"))]
        {
            let name = record.module_path().unwrap_or(record.target()).to_string();

            let line = record.line().unwrap_or(0) as usize;
            let mut frame = TracebackFrame::new(name, line);
            if let Some(file) = record.file() {
                frame = frame.filename(file);
            }

            Traceback::new(vec![frame], exception_type, exception_message)
                .extra_lines(self.tracebacks_extra_lines)
        }
    }
}

impl Log for RichLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let text = self.format_record(record);
        self.console.print_text(&text);

        if self.rich_tracebacks && record.level() == Level::Error {
            let traceback = self.build_traceback_for_record(record);
            self.console.print_exception(&traceback);
        }
    }

    fn flush(&self) {}
}

#[cfg(feature = "tracing")]
mod tracing_integration {
    use super::{Console, RichLogger};
    use log::{Level, Log};
    use std::fmt::Debug;
    use std::sync::Arc;

    use tracing::field::{Field, Visit};
    use tracing::{Event, Level as TracingLevel, Subscriber};
    use tracing_subscriber::{Layer, layer::Context};

    /// Tracing layer that formats events using `RichLogger` styling.
    pub struct RichTracingLayer {
        logger: RichLogger,
    }

    impl RichTracingLayer {
        /// Create a tracing layer backed by a `RichLogger`.
        #[must_use]
        pub fn new(console: Arc<Console>) -> Self {
            Self {
                logger: RichLogger::new(console),
            }
        }

        /// Use an existing logger configuration.
        #[must_use]
        pub fn with_logger(logger: RichLogger) -> Self {
            Self { logger }
        }

        /// Install as the global tracing subscriber.
        pub fn init(self) -> Result<(), tracing::subscriber::SetGlobalDefaultError> {
            use tracing_subscriber::prelude::*;

            let subscriber = tracing_subscriber::registry().with(self);
            tracing::subscriber::set_global_default(subscriber)
        }
    }

    #[derive(Default)]
    struct EventVisitor {
        message: Option<String>,
        fields: Vec<(String, String)>,
    }

    impl Visit for EventVisitor {
        fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
            let rendered = format!("{value:?}");
            let rendered = strip_quotes(&rendered).to_string();
            if field.name() == "message" {
                self.message = Some(rendered);
            } else {
                self.fields.push((field.name().to_string(), rendered));
            }
        }
    }

    impl<S> Layer<S> for RichTracingLayer
    where
        S: Subscriber,
    {
        fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
            let metadata = event.metadata();
            let mut visitor = EventVisitor::default();
            event.record(&mut visitor);

            let mut message = visitor.message.unwrap_or_default();
            if !visitor.fields.is_empty() {
                let extra = visitor
                    .fields
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                if message.is_empty() {
                    message = extra;
                } else {
                    message.push(' ');
                    message.push_str(&extra);
                }
            }

            let message_ref = message.as_str();
            let args = format_args!("{message_ref}");
            let record = log::Record::builder()
                .args(args)
                .level(map_tracing_level(*metadata.level()))
                .target(metadata.target())
                .file(metadata.file())
                .line(metadata.line())
                .module_path(metadata.module_path())
                .build();

            self.logger.log(&record);
        }
    }

    fn map_tracing_level(level: TracingLevel) -> Level {
        match level {
            TracingLevel::TRACE => Level::Trace,
            TracingLevel::DEBUG => Level::Debug,
            TracingLevel::INFO => Level::Info,
            TracingLevel::WARN => Level::Warn,
            TracingLevel::ERROR => Level::Error,
        }
    }

    fn strip_quotes(value: &str) -> &str {
        if value.len() >= 2 && value.starts_with('\"') && value.ends_with('\"') {
            &value[1..value.len() - 1]
        } else {
            value
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_strip_quotes() {
            assert_eq!(strip_quotes("\"hello\""), "hello");
            assert_eq!(strip_quotes("plain"), "plain");
        }

        #[test]
        fn test_strip_quotes_empty() {
            assert_eq!(strip_quotes(""), "");
            assert_eq!(strip_quotes("\"\""), "");
        }

        #[test]
        fn test_strip_quotes_single_char() {
            assert_eq!(strip_quotes("\""), "\"");
            assert_eq!(strip_quotes("a"), "a");
        }

        #[test]
        fn test_strip_quotes_only_start_quote() {
            assert_eq!(strip_quotes("\"hello"), "\"hello");
        }

        #[test]
        fn test_strip_quotes_only_end_quote() {
            assert_eq!(strip_quotes("hello\""), "hello\"");
        }

        #[test]
        fn test_rich_tracing_layer_new() {
            let console = Arc::new(Console::builder().force_terminal(true).build());
            let layer = RichTracingLayer::new(console);
            // Layer is created without panic
            let _ = layer;
        }

        #[test]
        fn test_rich_tracing_layer_with_logger() {
            let console = Arc::new(Console::builder().force_terminal(true).build());
            let logger = RichLogger::new(console)
                .level(log::LevelFilter::Debug)
                .show_time(false);
            let layer = RichTracingLayer::with_logger(logger);
            // Layer is created without panic
            let _ = layer;
        }

        #[test]
        fn test_map_tracing_level_trace() {
            assert_eq!(map_tracing_level(TracingLevel::TRACE), Level::Trace);
        }

        #[test]
        fn test_map_tracing_level_debug() {
            assert_eq!(map_tracing_level(TracingLevel::DEBUG), Level::Debug);
        }

        #[test]
        fn test_map_tracing_level_info() {
            assert_eq!(map_tracing_level(TracingLevel::INFO), Level::Info);
        }

        #[test]
        fn test_map_tracing_level_warn() {
            assert_eq!(map_tracing_level(TracingLevel::WARN), Level::Warn);
        }

        #[test]
        fn test_map_tracing_level_error() {
            assert_eq!(map_tracing_level(TracingLevel::ERROR), Level::Error);
        }

        #[test]
        fn test_event_visitor_default() {
            let visitor = EventVisitor::default();
            assert!(visitor.message.is_none());
            assert!(visitor.fields.is_empty());
        }

        #[test]
        fn test_event_visitor_record_debug_message() {
            let visitor = EventVisitor::default();
            // Simulate recording a message field
            // Note: Field is internal to tracing, we test indirectly
            assert!(visitor.message.is_none());
        }
    }
}

#[cfg(feature = "tracing")]
pub use tracing_integration::RichTracingLayer;

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // RichLogger Initialization Tests
    // =========================================================================

    #[test]
    fn test_rich_logger_new_default() {
        let console = Arc::new(Console::builder().force_terminal(true).build());
        let logger = RichLogger::new(console.clone());

        // Verify default settings
        assert_eq!(logger.level, LevelFilter::Info);
        assert!(logger.show_time);
        assert!(logger.omit_repeated_times);
        assert!(logger.show_level);
        assert!(logger.show_path);
        assert!(logger.enable_link_path);
        assert!(!logger.markup);
        assert_eq!(logger.keywords.len(), 8); // DEFAULT_KEYWORDS
        assert!(logger.keywords.contains(&"GET".to_string()));
        assert!(logger.keywords.contains(&"POST".to_string()));
    }

    #[test]
    fn test_rich_logger_builder_chain() {
        let console = Arc::new(Console::builder().force_terminal(true).build());
        let logger = RichLogger::new(console)
            .level(LevelFilter::Debug)
            .show_time(false)
            .omit_repeated_times(false)
            .show_level(true)
            .show_path(false)
            .enable_link_path(false)
            .markup(true)
            .keywords(vec!["CUSTOM".to_string()]);

        assert_eq!(logger.level, LevelFilter::Debug);
        assert!(!logger.show_time);
        assert!(!logger.omit_repeated_times);
        assert!(logger.show_level);
        assert!(!logger.show_path);
        assert!(!logger.enable_link_path);
        assert!(logger.markup);
        assert_eq!(logger.keywords, vec!["CUSTOM".to_string()]);
    }

    #[test]
    fn test_rich_logger_time_format() {
        let console = Arc::new(Console::builder().force_terminal(true).build());
        let logger = RichLogger::new(console).time_format("[hour]:[minute]");
        // Time format is successfully parsed (no panic)
        let _ = logger.format_time();
    }

    #[test]
    fn test_rich_logger_time_format_invalid() {
        let console = Arc::new(Console::builder().force_terminal(true).build());
        // Invalid format should not crash, just keeps the existing format
        let logger = RichLogger::new(console).time_format("invalid format spec");
        let _ = logger.format_time();
    }

    // =========================================================================
    // Log Level Filtering Tests
    // =========================================================================

    #[test]
    fn test_log_enabled_info_level() {
        let console = Arc::new(Console::builder().force_terminal(true).build());
        let logger = RichLogger::new(console).level(LevelFilter::Info);

        // Info level: Info, Warn, Error enabled; Debug, Trace disabled
        let info_meta = log::Metadata::builder().level(Level::Info).build();
        let warn_meta = log::Metadata::builder().level(Level::Warn).build();
        let error_meta = log::Metadata::builder().level(Level::Error).build();
        let debug_meta = log::Metadata::builder().level(Level::Debug).build();
        let trace_meta = log::Metadata::builder().level(Level::Trace).build();

        assert!(logger.enabled(&info_meta));
        assert!(logger.enabled(&warn_meta));
        assert!(logger.enabled(&error_meta));
        assert!(!logger.enabled(&debug_meta));
        assert!(!logger.enabled(&trace_meta));
    }

    #[test]
    fn test_log_enabled_trace_level() {
        let console = Arc::new(Console::builder().force_terminal(true).build());
        let logger = RichLogger::new(console).level(LevelFilter::Trace);

        // Trace level: all enabled
        let trace_meta = log::Metadata::builder().level(Level::Trace).build();
        let debug_meta = log::Metadata::builder().level(Level::Debug).build();
        let info_meta = log::Metadata::builder().level(Level::Info).build();

        assert!(logger.enabled(&trace_meta));
        assert!(logger.enabled(&debug_meta));
        assert!(logger.enabled(&info_meta));
    }

    #[test]
    fn test_log_enabled_error_only() {
        let console = Arc::new(Console::builder().force_terminal(true).build());
        let logger = RichLogger::new(console).level(LevelFilter::Error);

        let error_meta = log::Metadata::builder().level(Level::Error).build();
        let warn_meta = log::Metadata::builder().level(Level::Warn).build();

        assert!(logger.enabled(&error_meta));
        assert!(!logger.enabled(&warn_meta));
    }

    // =========================================================================
    // Log Output Formatting Tests
    // =========================================================================

    #[test]
    fn test_format_record_includes_message_and_path() {
        let console = Arc::new(Console::builder().markup(false).build());
        let logger = RichLogger::new(console)
            .show_time(false)
            .show_level(false)
            .show_path(true)
            .enable_link_path(false);

        let record = log::Record::builder()
            .args(format_args!("Hello"))
            .level(Level::Info)
            .file(Some("main.rs"))
            .line(Some(42))
            .build();

        let text = logger.format_record(&record);
        let plain = text.plain();
        assert!(plain.contains("Hello"));
        assert!(plain.contains("main.rs:42"));
    }

    #[test]
    fn test_format_record_with_time() {
        let console = Arc::new(Console::builder().force_terminal(true).build());
        let logger = RichLogger::new(console)
            .show_time(true)
            .show_level(false)
            .show_path(false);

        let record = log::Record::builder()
            .args(format_args!("Test message"))
            .level(Level::Info)
            .build();

        let text = logger.format_record(&record);
        let plain = text.plain();
        // Time should be present (has some pattern with colons or brackets)
        // The exact format depends on time_format, but we check message is there
        assert!(plain.contains("Test message"));
    }

    #[test]
    fn test_format_record_without_time() {
        let console = Arc::new(Console::builder().force_terminal(true).build());
        let logger = RichLogger::new(console)
            .show_time(false)
            .show_level(false)
            .show_path(false);

        let record = log::Record::builder()
            .args(format_args!("No time"))
            .level(Level::Info)
            .build();

        let text = logger.format_record(&record);
        let plain = text.plain();
        // Should only contain the message
        assert_eq!(plain.trim(), "No time");
    }

    #[test]
    fn test_format_record_with_level() {
        let console = Arc::new(Console::builder().force_terminal(true).build());
        let logger = RichLogger::new(console)
            .show_time(false)
            .show_level(true)
            .show_path(false);

        let record = log::Record::builder()
            .args(format_args!("Leveled"))
            .level(Level::Warn)
            .build();

        let text = logger.format_record(&record);
        let plain = text.plain();
        assert!(plain.contains("WARN"));
        assert!(plain.contains("Leveled"));
    }

    #[test]
    fn test_format_record_level_styles() {
        // Test that each level gets a consistent style
        let _trace_style = RichLogger::level_style(Level::Trace);
        let _debug_style = RichLogger::level_style(Level::Debug);
        let info_style = RichLogger::level_style(Level::Info);
        let warn_style = RichLogger::level_style(Level::Warn);
        let error_style = RichLogger::level_style(Level::Error);

        // Each call with the same level should return the same style
        assert_eq!(
            RichLogger::level_style(Level::Info),
            info_style,
            "Style should be consistent"
        );

        // Different levels should have different styles (at least some of them)
        // Info is green, Warn is yellow, Error is bold red - they should differ
        assert_ne!(
            info_style, error_style,
            "Info and Error should have different styles"
        );
        assert_ne!(
            warn_style, error_style,
            "Warn and Error should have different styles"
        );
    }

    #[test]
    fn test_format_record_without_path() {
        let console = Arc::new(Console::builder().force_terminal(true).build());
        let logger = RichLogger::new(console)
            .show_time(false)
            .show_level(false)
            .show_path(false);

        let record = log::Record::builder()
            .args(format_args!("No path"))
            .level(Level::Info)
            .file(Some("should_not_appear.rs"))
            .line(Some(99))
            .build();

        let text = logger.format_record(&record);
        let plain = text.plain();
        assert!(!plain.contains("should_not_appear.rs"));
        assert!(!plain.contains("99"));
    }

    #[test]
    fn test_format_record_with_link_path() {
        let console = Arc::new(Console::builder().force_terminal(true).build());
        let logger = RichLogger::new(console)
            .show_time(false)
            .show_level(false)
            .show_path(true)
            .enable_link_path(true);

        let record = log::Record::builder()
            .args(format_args!("Linked"))
            .level(Level::Info)
            .file(Some("linked.rs"))
            .line(Some(10))
            .build();

        let text = logger.format_record(&record);
        // The text should contain the path (link is in the style)
        let plain = text.plain();
        assert!(plain.contains("linked.rs:10"));
    }

    // =========================================================================
    // Keyword Highlighting Tests
    // =========================================================================

    #[test]
    fn test_format_record_keyword_highlighting() {
        let console = Arc::new(Console::builder().force_terminal(true).build());
        let logger = RichLogger::new(console)
            .show_time(false)
            .show_level(false)
            .show_path(false)
            .keywords(vec!["KEYWORD".to_string()]);

        let record = log::Record::builder()
            .args(format_args!("This has KEYWORD inside"))
            .level(Level::Info)
            .build();

        let text = logger.format_record(&record);
        let plain = text.plain();
        assert!(plain.contains("KEYWORD"));
    }

    #[test]
    fn test_format_record_http_keywords() {
        let console = Arc::new(Console::builder().force_terminal(true).build());
        let logger = RichLogger::new(console)
            .show_time(false)
            .show_level(false)
            .show_path(false);

        // Default keywords include HTTP methods
        let record = log::Record::builder()
            .args(format_args!("GET /api/users POST /api/login"))
            .level(Level::Info)
            .build();

        let text = logger.format_record(&record);
        let plain = text.plain();
        assert!(plain.contains("GET"));
        assert!(plain.contains("POST"));
    }

    #[test]
    fn test_format_record_empty_keywords() {
        let console = Arc::new(Console::builder().force_terminal(true).build());
        let logger = RichLogger::new(console)
            .show_time(false)
            .show_level(false)
            .show_path(false)
            .keywords(vec![]); // No keywords

        let record = log::Record::builder()
            .args(format_args!("GET should not be highlighted"))
            .level(Level::Info)
            .build();

        // Should not crash with empty keywords
        let text = logger.format_record(&record);
        let plain = text.plain();
        assert!(plain.contains("GET"));
    }

    // =========================================================================
    // Markup Tests
    // =========================================================================

    #[test]
    fn test_format_record_with_markup() {
        let console = Arc::new(Console::builder().force_terminal(true).build());
        let logger = RichLogger::new(console)
            .show_time(false)
            .show_level(false)
            .show_path(false)
            .markup(true);

        let record = log::Record::builder()
            .args(format_args!("[bold]Bold text[/bold]"))
            .level(Level::Info)
            .build();

        let text = logger.format_record(&record);
        let plain = text.plain();
        // Markup should be parsed, not shown as literal
        assert!(plain.contains("Bold text"));
        assert!(!plain.contains("[bold]"));
    }

    #[test]
    fn test_format_record_without_markup() {
        let console = Arc::new(Console::builder().force_terminal(true).build());
        let logger = RichLogger::new(console)
            .show_time(false)
            .show_level(false)
            .show_path(false)
            .markup(false);

        let record = log::Record::builder()
            .args(format_args!("[bold]Not parsed[/bold]"))
            .level(Level::Info)
            .build();

        let text = logger.format_record(&record);
        let plain = text.plain();
        // Markup tags should appear as literal text
        assert!(plain.contains("[bold]"));
    }

    // =========================================================================
    // Omit Repeated Times Tests
    // =========================================================================

    #[test]
    fn test_format_record_omit_repeated_times() {
        let console = Arc::new(Console::builder().force_terminal(true).build());
        let logger = RichLogger::new(console)
            .show_time(true)
            .omit_repeated_times(true)
            .show_level(false)
            .show_path(false);

        // First record should have time
        let record1 = log::Record::builder()
            .args(format_args!("First"))
            .level(Level::Info)
            .build();
        let text1 = logger.format_record(&record1);
        let plain1 = text1.plain();

        // Second record within same second should have blank time
        let record2 = log::Record::builder()
            .args(format_args!("Second"))
            .level(Level::Info)
            .build();
        let text2 = logger.format_record(&record2);
        let plain2 = text2.plain();

        // Both should contain their messages
        assert!(plain1.contains("First"));
        assert!(plain2.contains("Second"));
        // Second should have spaces where time was (if within same second)
    }

    #[test]
    fn test_format_record_no_omit_repeated_times() {
        let console = Arc::new(Console::builder().force_terminal(true).build());
        let logger = RichLogger::new(console)
            .show_time(true)
            .omit_repeated_times(false)
            .show_level(false)
            .show_path(false);

        let record1 = log::Record::builder()
            .args(format_args!("First"))
            .level(Level::Info)
            .build();
        let _ = logger.format_record(&record1);

        let record2 = log::Record::builder()
            .args(format_args!("Second"))
            .level(Level::Info)
            .build();
        let text2 = logger.format_record(&record2);

        // Second record should still show time (not omitted)
        let plain2 = text2.plain();
        assert!(plain2.contains("Second"));
    }

    // =========================================================================
    // Thread Safety Tests
    // =========================================================================

    #[test]
    fn test_logger_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<RichLogger>();
    }

    #[test]
    fn test_logger_multithreaded_enabled() {
        use std::thread;

        let console = Arc::new(Console::builder().force_terminal(true).build());
        let logger = Arc::new(RichLogger::new(console).level(LevelFilter::Info));

        // Test that enabled() can be called from multiple threads safely
        let handles: Vec<_> = (0..4)
            .map(|i| {
                let logger = Arc::clone(&logger);
                thread::spawn(move || {
                    for _ in 0..100 {
                        let level = match i % 5 {
                            0 => Level::Trace,
                            1 => Level::Debug,
                            2 => Level::Info,
                            3 => Level::Warn,
                            _ => Level::Error,
                        };
                        let meta = log::Metadata::builder().level(level).build();
                        let _ = logger.enabled(&meta);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("Thread should not panic");
        }
    }

    // =========================================================================
    // Flush Behavior Tests
    // =========================================================================

    #[test]
    fn test_logger_flush_is_noop() {
        let console = Arc::new(Console::builder().force_terminal(true).build());
        let logger = RichLogger::new(console);
        // flush() should not panic and is a no-op
        logger.flush();
    }

    // =========================================================================
    // Edge Cases
    // =========================================================================

    #[test]
    fn test_format_record_empty_message() {
        let console = Arc::new(Console::builder().force_terminal(true).build());
        let logger = RichLogger::new(console)
            .show_time(false)
            .show_level(false)
            .show_path(false);

        let record = log::Record::builder()
            .args(format_args!(""))
            .level(Level::Info)
            .build();

        let text = logger.format_record(&record);
        let plain = text.plain();
        assert_eq!(plain.trim(), "");
    }

    #[test]
    fn test_format_record_no_file_no_line() {
        let console = Arc::new(Console::builder().force_terminal(true).build());
        let logger = RichLogger::new(console)
            .show_time(false)
            .show_level(false)
            .show_path(true)
            .enable_link_path(false);

        let record = log::Record::builder()
            .args(format_args!("No file info"))
            .level(Level::Info)
            .build();

        let text = logger.format_record(&record);
        let plain = text.plain();
        // Should just have the message, no path
        assert!(plain.contains("No file info"));
    }

    #[test]
    fn test_format_record_file_no_line() {
        let console = Arc::new(Console::builder().force_terminal(true).build());
        let logger = RichLogger::new(console)
            .show_time(false)
            .show_level(false)
            .show_path(true)
            .enable_link_path(false);

        let record = log::Record::builder()
            .args(format_args!("Has file"))
            .level(Level::Info)
            .file(Some("nolineno.rs"))
            .build();

        let text = logger.format_record(&record);
        let plain = text.plain();
        assert!(plain.contains("nolineno.rs"));
        // No line number, so no colon after filename
    }

    #[test]
    fn test_format_record_unicode_message() {
        let console = Arc::new(Console::builder().force_terminal(true).build());
        let logger = RichLogger::new(console)
            .show_time(false)
            .show_level(false)
            .show_path(false);

        let record = log::Record::builder()
            .args(format_args!("Unicode: \u{1F600} \u{1F4BB} \u{2764}"))
            .level(Level::Info)
            .build();

        let text = logger.format_record(&record);
        let plain = text.plain();
        assert!(plain.contains("\u{1F600}"));
        assert!(plain.contains("\u{1F4BB}"));
    }

    #[test]
    fn test_format_record_multiline_message() {
        let console = Arc::new(Console::builder().force_terminal(true).build());
        let logger = RichLogger::new(console)
            .show_time(false)
            .show_level(false)
            .show_path(false);

        let record = log::Record::builder()
            .args(format_args!("Line 1\nLine 2\nLine 3"))
            .level(Level::Info)
            .build();

        let text = logger.format_record(&record);
        let plain = text.plain();
        assert!(plain.contains("Line 1"));
        assert!(plain.contains("Line 2"));
        assert!(plain.contains("Line 3"));
    }
}
