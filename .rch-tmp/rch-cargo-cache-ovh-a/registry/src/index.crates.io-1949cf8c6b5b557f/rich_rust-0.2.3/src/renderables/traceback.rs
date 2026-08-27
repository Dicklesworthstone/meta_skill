//! Traceback rendering.
//!
//! This provides Rich-style tracebacks comparable to Python Rich's `rich.traceback`.
//!
//! For deterministic tests and Python fixture conformance, you can construct
//! a [`Traceback`] from explicit frames (function name + line number, with
//! optional embedded source context). When the optional `backtrace` feature is
//! enabled, you can also capture real runtime frames via [`Traceback::capture`].
//!
//! # Automatic Capture (requires `backtrace` feature)
//!
//! When the `backtrace` feature is enabled, you can capture the current
//! call stack automatically:
//!
//! ```ignore
//! use rich_rust::renderables::{Traceback, TracebackFrame};
//!
//! // Capture current backtrace
//! let traceback = Traceback::capture("MyError", "something went wrong");
//! console.print_exception(&traceback);
//! ```

use crate::console::{Console, ConsoleOptions};
use crate::markup;
use crate::renderables::Renderable;
use crate::segment::Segment;
use crate::style::Style;
use crate::text::Text;

use super::panel::Panel;

#[cfg(feature = "backtrace")]
use backtrace::Backtrace as BT;

/// A single traceback frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TracebackFrame {
    pub filename: Option<String>,
    pub name: String,
    pub line: usize,
    /// Optional locals for this frame.
    ///
    /// Rust can't generally capture locals automatically; this is an explicit,
    /// deterministic representation for parity with Python Rich when the caller
    /// has locals available.
    pub locals: Option<Vec<(String, String)>>,
    /// Optional source code snippet for this frame.
    ///
    /// When provided, this is used instead of reading from the filesystem.
    /// This enables deterministic testing and rendering without file access.
    /// The snippet should contain lines around the error, with the error line
    /// being at position `line` (1-indexed relative to the start of the snippet's
    /// first line number, specified by `source_first_line`).
    pub source_context: Option<String>,
    /// The line number of the first line in `source_context`.
    /// Defaults to 1 if not specified.
    pub source_first_line: usize,
}

impl TracebackFrame {
    #[must_use]
    pub fn new(name: impl Into<String>, line: usize) -> Self {
        Self {
            filename: None,
            name: name.into(),
            line,
            locals: None,
            source_context: None,
            source_first_line: 1,
        }
    }

    #[must_use]
    pub fn filename(mut self, filename: impl Into<String>) -> Self {
        self.filename = Some(filename.into());
        self
    }

    /// Provide locals for this frame (key/value pairs).
    #[must_use]
    pub fn locals(mut self, locals: impl Into<Vec<(String, String)>>) -> Self {
        self.locals = Some(locals.into());
        self
    }

    /// Provide source context lines directly instead of reading from filesystem.
    ///
    /// This is useful for:
    /// - Deterministic testing without filesystem dependencies
    /// - Rendering tracebacks when source files are not available
    /// - Embedding source snippets from memory
    ///
    /// # Arguments
    /// * `source` - The source code snippet (may contain multiple lines)
    /// * `first_line` - The line number of the first line in the snippet
    ///
    /// # Example
    /// ```
    /// use rich_rust::renderables::TracebackFrame;
    ///
    /// let frame = TracebackFrame::new("my_function", 5)
    ///     .source_context("fn my_function() {\n    let x = 1;\n    return Err(\"oops\");\n}", 3);
    /// ```
    #[must_use]
    pub fn source_context(mut self, source: impl Into<String>, first_line: usize) -> Self {
        self.source_context = Some(source.into());
        self.source_first_line = first_line.max(1);
        self
    }
}

/// A rendered traceback, inspired by Python Rich.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Traceback {
    frames: Vec<TracebackFrame>,
    exception_type: String,
    exception_message: String,
    title: Text,
    extra_lines: usize,
    show_locals: bool,
}

impl Traceback {
    /// Create a new traceback from frames and exception info.
    #[must_use]
    pub fn new(
        frames: impl Into<Vec<TracebackFrame>>,
        exception_type: impl Into<String>,
        exception_message: impl Into<String>,
    ) -> Self {
        Self {
            frames: frames.into(),
            exception_type: exception_type.into(),
            exception_message: exception_message.into(),
            title: markup::render_or_plain(
                "[bold red]Traceback [bold dim red](most recent call last)[/]",
            ),
            extra_lines: 0,
            show_locals: false,
        }
    }

    /// Override the title shown in the traceback panel.
    #[must_use]
    pub fn title(mut self, title: impl Into<Text>) -> Self {
        self.title = title.into();
        self
    }

    #[must_use]
    pub fn extra_lines(mut self, extra_lines: usize) -> Self {
        self.extra_lines = extra_lines;
        self
    }

    #[must_use]
    pub fn show_locals(mut self, show: bool) -> Self {
        self.show_locals = show;
        self
    }

    /// Push a frame (builder-style).
    pub fn push_frame(&mut self, frame: TracebackFrame) {
        self.frames.push(frame);
    }

    /// Capture the current call stack and create a traceback.
    ///
    /// This is the primary way to create a Traceback from an actual runtime
    /// error. It captures the current backtrace and converts it to frames.
    ///
    /// # Arguments
    /// * `exception_type` - The type/name of the exception (e.g., `PanicError`)
    /// * `exception_message` - The error message
    ///
    /// # Example
    /// ```ignore
    /// let traceback = Traceback::capture("ConnectionError", "failed to connect");
    /// console.print_exception(&traceback);
    /// ```
    ///
    /// Requires the `backtrace` feature.
    #[cfg(feature = "backtrace")]
    #[must_use]
    pub fn capture(
        exception_type: impl Into<String>,
        exception_message: impl Into<String>,
    ) -> Self {
        let bt = BT::new();
        Self::from_backtrace(&bt, exception_type, exception_message)
    }

    /// Create a Traceback from an existing `backtrace::Backtrace`.
    ///
    /// This is useful when you have a backtrace from a panic handler or
    /// error type that provides its own backtrace.
    ///
    /// # Arguments
    /// * `bt` - The backtrace to convert
    /// * `exception_type` - The type/name of the exception
    /// * `exception_message` - The error message
    ///
    /// Requires the `backtrace` feature.
    #[cfg(feature = "backtrace")]
    #[must_use]
    pub fn from_backtrace(
        bt: &BT,
        exception_type: impl Into<String>,
        exception_message: impl Into<String>,
    ) -> Self {
        let frames = Self::parse_backtrace(bt);
        Self::new(frames, exception_type, exception_message)
    }

    /// Parse a backtrace into `TracebackFrame` list.
    ///
    /// Filters out runtime/std frames to show only relevant user code.
    #[cfg(feature = "backtrace")]
    fn parse_backtrace(bt: &BT) -> Vec<TracebackFrame> {
        let mut frames = Vec::new();
        let mut seen_user_code = false;

        for frame in bt.frames() {
            // Get symbols for this frame
            let symbols: Vec<_> = {
                let mut syms = Vec::new();
                backtrace::resolve(frame.ip(), |symbol| {
                    syms.push((
                        symbol.name().map(|n| n.to_string()),
                        symbol.filename().map(std::path::Path::to_path_buf),
                        symbol.lineno(),
                    ));
                });
                syms
            };

            for (name, filename, lineno) in symbols {
                let Some(name) = name else {
                    continue;
                };

                // Filter out internal/runtime frames
                if Self::is_internal_frame(&name) {
                    // Once we've seen user code, internal frames mark the end
                    if seen_user_code {
                        continue;
                    }
                    continue;
                }

                seen_user_code = true;

                let mut frame =
                    TracebackFrame::new(Self::demangle_name(&name), lineno.unwrap_or(0) as usize);

                if let Some(ref path) = filename {
                    frame = frame.filename(path.display().to_string());
                }

                frames.push(frame);
            }
        }

        // Reverse so most recent call is last (like Python)
        frames.reverse();
        frames
    }

    /// Check if a frame name is internal/runtime that should be filtered.
    #[cfg(feature = "backtrace")]
    fn is_internal_frame(name: &str) -> bool {
        // Filter common runtime prefixes
        let internal_prefixes = [
            "std::",
            "core::",
            "alloc::",
            "backtrace::",
            "log::",
            "tracing::",
            "tracing_subscriber::",
            "rich_rust::logging::",
            "<alloc::",
            "<core::",
            "<std::",
            "rust_begin_unwind",
            "__rust_",
            "_start",
            "__libc_",
            "clone",
        ];

        for prefix in internal_prefixes {
            if name.starts_with(prefix) {
                return true;
            }
        }

        // Filter Traceback's own capture functions
        if name.contains("Traceback::capture") || name.contains("Traceback::from_backtrace") {
            return true;
        }

        false
    }

    /// Simplify/demangle a function name for display.
    #[cfg(feature = "backtrace")]
    fn demangle_name(name: &str) -> String {
        // The backtrace crate already demangles, but we can simplify further
        let name = name.to_string();

        // Remove hash suffixes like ::h1234567890abcdef
        if let Some(pos) = name.rfind("::h")
            && name[pos + 3..].chars().all(|c| c.is_ascii_hexdigit())
        {
            return name[..pos].to_string();
        }

        name
    }

    /// Get source for a frame, preferring provided context over filesystem.
    ///
    /// Returns `Some((source, first_line))` if source is available,
    /// `None` if no source can be obtained.
    fn get_frame_source(&self, frame: &TracebackFrame) -> Option<(String, usize)> {
        // Priority 1: Use provided source context
        if let Some(ref source) = frame.source_context {
            return Some((source.clone(), frame.source_first_line));
        }

        // Priority 2: Read from filesystem if filename is provided
        if let Some(ref filename) = frame.filename
            && let Ok(source) = std::fs::read_to_string(filename)
        {
            return Some((source, 1));
        }

        None
    }
}

impl Renderable for Traceback {
    fn render<'a>(&'a self, _console: &Console, options: &ConsoleOptions) -> Vec<Segment<'a>> {
        let width = options.max_width.max(1);

        // Define styles for traceback components
        let file_style = Style::parse("cyan").ok();
        let lineno_style = Style::parse("bright_blue").ok();
        let func_style = Style::parse("bright_green").ok();
        let separator_style = None;
        let dim_style = Style::parse("dim").ok();
        let error_line_style = Style::parse("bold").ok();
        let exception_type_style = Style::parse("bold bright_red").ok();
        let exception_msg_style = None;

        let mut content_lines: Vec<Vec<Segment<'static>>> = Vec::new();
        for frame in &self.frames {
            // Try to get source: first from provided context, then from filesystem
            let source_result = self.get_frame_source(frame);

            if let Some((source, first_line)) = source_result {
                // Render frame header with location info using styled segments
                if let Some(filename) = frame.filename.as_deref() {
                    content_lines.push(vec![
                        Segment::new(filename.to_string(), file_style.clone()),
                        Segment::new(":", separator_style.clone()),
                        Segment::new(frame.line.to_string(), lineno_style.clone()),
                        Segment::new(" in ", separator_style.clone()),
                        Segment::new(frame.name.clone(), func_style.clone()),
                    ]);
                } else {
                    content_lines.push(vec![
                        Segment::new("in ", separator_style.clone()),
                        Segment::new(frame.name.clone(), func_style.clone()),
                        Segment::new(":", separator_style.clone()),
                        Segment::new(frame.line.to_string(), lineno_style.clone()),
                    ]);
                }
                content_lines.push(vec![Segment::new(String::new(), None)]);

                // Render source context with line numbers
                let source_lines: Vec<&str> = source.lines().collect();
                let last_line = first_line + source_lines.len().saturating_sub(1);

                // Calculate which lines to show based on extra_lines
                let start = frame.line.saturating_sub(self.extra_lines).max(first_line);
                let end = (frame.line + self.extra_lines).min(last_line);

                if start <= end && frame.line >= first_line && frame.line <= last_line {
                    let line_number_width = end.to_string().len() + 5;

                    for line_no in start..=end {
                        let source_idx = line_no.saturating_sub(first_line);
                        if source_idx < source_lines.len() {
                            let code = source_lines[source_idx];
                            let is_error_line = line_no == frame.line;
                            let indicator = if is_error_line { "❱" } else { " " };
                            let line_style = if is_error_line {
                                error_line_style.clone()
                            } else {
                                None
                            };

                            // Style for the indicator: bold red for error line, dim otherwise
                            let indicator_style = if is_error_line {
                                exception_type_style.clone() // reuse bold red
                            } else {
                                dim_style.clone()
                            };

                            content_lines.push(vec![
                                Segment::new(indicator.to_string(), indicator_style),
                                Segment::new(" ", None),
                                Segment::new(
                                    format!("{line_no:<line_number_width$}"),
                                    lineno_style.clone(),
                                ),
                                Segment::new(code.to_string(), line_style),
                            ]);
                        }
                    }
                }

                if self.show_locals
                    && let Some(locals) = frame.locals.as_ref()
                    && !locals.is_empty()
                {
                    let locals_key_style = Style::parse("dim").ok();
                    let locals_value_style = Style::parse("cyan").ok();
                    content_lines.push(vec![Segment::new(String::new(), None)]);
                    content_lines.push(vec![Segment::new("locals:", dim_style.clone())]);
                    for (k, v) in locals {
                        content_lines.push(vec![
                            Segment::new("  ", None),
                            Segment::new(format!("{k}="), locals_key_style.clone()),
                            Segment::new(v.clone(), locals_value_style.clone()),
                        ]);
                    }
                }

                continue;
            }

            // Fallback: no source available, just show frame info
            content_lines.push(vec![
                Segment::new("in ", separator_style.clone()),
                Segment::new(frame.name.clone(), func_style.clone()),
                Segment::new(":", separator_style.clone()),
                Segment::new(frame.line.to_string(), lineno_style.clone()),
            ]);

            if self.show_locals
                && let Some(locals) = frame.locals.as_ref()
                && !locals.is_empty()
            {
                let locals_key_style = Style::parse("dim").ok();
                let locals_value_style = Style::parse("cyan").ok();
                content_lines.push(vec![Segment::new(String::new(), None)]);
                content_lines.push(vec![Segment::new("locals:", dim_style.clone())]);
                for (k, v) in locals {
                    content_lines.push(vec![
                        Segment::new("  ", None),
                        Segment::new(format!("{k}="), locals_key_style.clone()),
                        Segment::new(v.clone(), locals_value_style.clone()),
                    ]);
                }
            }
        }

        let panel = Panel::new(content_lines)
            .title(self.title.clone())
            .border_style(Style::parse("red").unwrap_or_default())
            .width(width);
        let mut segments: Vec<Segment<'static>> = panel.render(width);

        // Exception info with styling
        segments.push(Segment::new(
            format!("{}: ", self.exception_type),
            exception_type_style.clone(),
        ));
        segments.push(Segment::new(
            self.exception_message.clone(),
            exception_msg_style.clone(),
        ));
        segments.push(Segment::line());

        segments.into_iter().collect()
    }
}

/// Convenience helper mirroring Python Rich's `Console.print_exception`.
///
/// Rust doesn't have a standardized structured backtrace API across error
/// types, so this helper prints a provided [`Traceback`] renderable. For automatic
/// capture, use [`Traceback::capture`] (feature `backtrace`).
pub fn print_exception(console: &Console, traceback: &Traceback) {
    console.print_exception(traceback);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_to_text(traceback: &Traceback, width: usize) -> String {
        let console = Console::new();
        let options = ConsoleOptions {
            max_width: width,
            ..Default::default()
        };
        let segments = traceback.render(&console, &options);
        segments.iter().map(|s| s.text.as_ref()).collect()
    }

    #[test]
    fn frame_without_source_shows_minimal_info() {
        let frame = TracebackFrame::new("my_func", 42);
        let traceback = Traceback::new(vec![frame], "Error", "something went wrong");

        let output = render_to_text(&traceback, 80);
        assert!(output.contains("my_func"));
        assert!(output.contains("42"));
        assert!(output.contains("Error: something went wrong"));
    }

    #[test]
    fn frame_with_source_context_renders_code() {
        let source = "fn main() {\n    let x = 1;\n    return Err(\"oops\");\n    let y = 2;\n}";
        let frame = TracebackFrame::new("main", 3).source_context(source, 1);
        let traceback = Traceback::new(vec![frame], "PanicError", "oops").extra_lines(1);

        let output = render_to_text(&traceback, 80);

        // Should show the error line with indicator
        assert!(output.contains("❱"));
        assert!(output.contains("return Err"));

        // Should show context lines (extra_lines=1)
        assert!(output.contains("let x = 1"));
        assert!(output.contains("let y = 2"));

        // Should show exception info
        assert!(output.contains("PanicError: oops"));
    }

    #[test]
    fn source_context_with_offset_first_line() {
        // Simulating a snippet from lines 10-14 of a larger file
        let source = "    fn helper() {\n        do_thing();\n        crash_here();\n    }\n";
        let frame = TracebackFrame::new("helper", 12).source_context(source, 10);
        let traceback = Traceback::new(vec![frame], "Error", "crashed").extra_lines(1);

        let output = render_to_text(&traceback, 80);

        // Should show line 12 with indicator
        assert!(output.contains("❱"));
        assert!(output.contains("12"));
        assert!(output.contains("crash_here"));

        // Should show context (lines 11 and 13)
        assert!(output.contains("11"));
        assert!(output.contains("do_thing"));
    }

    #[test]
    fn source_context_takes_priority_over_filename() {
        // Even if filename is set, source_context should be used
        let source = "custom source line";
        let frame = TracebackFrame::new("func", 1)
            .filename("/nonexistent/file.rs")
            .source_context(source, 1);
        let traceback = Traceback::new(vec![frame], "Error", "test");

        let output = render_to_text(&traceback, 80);

        // Should render the provided source, not try to read file
        assert!(output.contains("custom source line"));
        // Should still show filename in header
        assert!(output.contains("/nonexistent/file.rs"));
    }

    #[test]
    fn extra_lines_zero_shows_only_error_line() {
        let source = "line1\nline2\nline3\nline4\nline5";
        let frame = TracebackFrame::new("func", 3).source_context(source, 1);
        let traceback = Traceback::new(vec![frame], "Error", "test").extra_lines(0);

        let output = render_to_text(&traceback, 80);

        // Should show only line 3
        assert!(output.contains("line3"));
        assert!(output.contains("❱"));
        // Should not show other lines
        assert!(!output.contains("line1"));
        assert!(!output.contains("line5"));
    }

    #[test]
    fn multiple_frames_with_source_context() {
        let frame1 =
            TracebackFrame::new("outer", 2).source_context("fn outer() {\n    inner();\n}", 1);
        let frame2 =
            TracebackFrame::new("inner", 2).source_context("fn inner() {\n    fail();\n}", 1);

        let traceback = Traceback::new(vec![frame1, frame2], "PanicError", "boom");

        let output = render_to_text(&traceback, 80);

        // Both frames should be rendered
        assert!(output.contains("outer"));
        assert!(output.contains("inner"));
        assert!(output.contains("PanicError: boom"));
    }

    #[test]
    fn frame_builder_methods() {
        let frame = TracebackFrame::new("test", 10)
            .filename("src/lib.rs")
            .source_context("test code", 5);

        assert_eq!(frame.name, "test");
        assert_eq!(frame.line, 10);
        assert_eq!(frame.filename, Some("src/lib.rs".to_string()));
        assert_eq!(frame.source_context, Some("test code".to_string()));
        assert_eq!(frame.source_first_line, 5);
    }

    #[test]
    fn frame_locals_builder_and_rendering() {
        let frame = TracebackFrame::new("func", 1)
            .source_context("line1\nline2\nline3", 1)
            .locals(vec![
                ("user".to_string(), "\"alice\"".to_string()),
                ("retries".to_string(), "3".to_string()),
            ]);

        let traceback = Traceback::new(vec![frame], "Error", "msg")
            .extra_lines(0)
            .show_locals(true);

        let output = render_to_text(&traceback, 80);
        assert!(output.contains("locals:"));
        assert!(output.contains("user="));
        assert!(output.contains("alice"));
        assert!(output.contains("retries="));
    }

    #[test]
    fn source_first_line_minimum_is_one() {
        let frame = TracebackFrame::new("test", 1).source_context("code", 0);
        assert_eq!(frame.source_first_line, 1);
    }

    // =========================================================================
    // TracebackFrame Creation Tests (bd-201u)
    // =========================================================================

    #[test]
    fn test_frame_new_basic() {
        let frame = TracebackFrame::new("my_function", 42);
        assert_eq!(frame.name, "my_function");
        assert_eq!(frame.line, 42);
        assert!(frame.filename.is_none());
        assert!(frame.source_context.is_none());
        assert_eq!(frame.source_first_line, 1);
    }

    #[test]
    fn test_frame_new_with_string_type() {
        let name = String::from("owned_name");
        let frame = TracebackFrame::new(name, 100);
        assert_eq!(frame.name, "owned_name");
        assert_eq!(frame.line, 100);
    }

    #[test]
    fn test_frame_filename_builder() {
        let frame = TracebackFrame::new("func", 1).filename("src/main.rs");
        assert_eq!(frame.filename, Some("src/main.rs".to_string()));
    }

    #[test]
    fn test_frame_filename_with_owned_string() {
        let path = String::from("/path/to/file.rs");
        let frame = TracebackFrame::new("func", 1).filename(path);
        assert_eq!(frame.filename, Some("/path/to/file.rs".to_string()));
    }

    #[test]
    fn test_frame_source_context_builder() {
        let frame = TracebackFrame::new("func", 5).source_context("let x = 1;", 5);
        assert_eq!(frame.source_context, Some("let x = 1;".to_string()));
        assert_eq!(frame.source_first_line, 5);
    }

    #[test]
    fn test_frame_source_context_multiline() {
        let source = "fn foo() {\n    bar();\n}";
        let frame = TracebackFrame::new("foo", 2).source_context(source, 1);
        assert!(frame.source_context.unwrap().contains("bar()"));
    }

    #[test]
    fn test_frame_clone() {
        let frame = TracebackFrame::new("func", 10)
            .filename("test.rs")
            .source_context("code", 5);
        let cloned = frame.clone();
        assert_eq!(frame, cloned);
    }

    #[test]
    fn test_frame_eq() {
        let frame1 = TracebackFrame::new("func", 10);
        let frame2 = TracebackFrame::new("func", 10);
        assert_eq!(frame1, frame2);
    }

    #[test]
    fn test_frame_ne_different_name() {
        let frame1 = TracebackFrame::new("func_a", 10);
        let frame2 = TracebackFrame::new("func_b", 10);
        assert_ne!(frame1, frame2);
    }

    #[test]
    fn test_frame_ne_different_line() {
        let frame1 = TracebackFrame::new("func", 10);
        let frame2 = TracebackFrame::new("func", 20);
        assert_ne!(frame1, frame2);
    }

    #[test]
    fn test_frame_debug() {
        let frame = TracebackFrame::new("test", 1);
        let debug = format!("{frame:?}");
        assert!(debug.contains("TracebackFrame"));
        assert!(debug.contains("test"));
    }

    #[test]
    fn test_frame_chain_builder_pattern() {
        let frame = TracebackFrame::new("handler", 50)
            .filename("src/handlers/api.rs")
            .source_context("async fn handler() -> Result<()>", 50);

        assert_eq!(frame.name, "handler");
        assert_eq!(frame.line, 50);
        assert_eq!(frame.filename, Some("src/handlers/api.rs".to_string()));
        assert!(frame.source_context.is_some());
    }

    #[test]
    fn test_frame_empty_source_context() {
        let frame = TracebackFrame::new("func", 1).source_context("", 1);
        assert_eq!(frame.source_context, Some(String::new()));
    }

    // =========================================================================
    // Traceback Creation Tests
    // =========================================================================

    #[test]
    fn test_traceback_new_empty_frames() {
        let traceback = Traceback::new(Vec::new(), "Error", "message");
        assert_eq!(traceback.frames.len(), 0);
        assert_eq!(traceback.exception_type, "Error");
        assert_eq!(traceback.exception_message, "message");
    }

    #[test]
    fn test_traceback_new_from_vec() {
        let frames = vec![
            TracebackFrame::new("a", 1),
            TracebackFrame::new("b", 2),
            TracebackFrame::new("c", 3),
        ];
        let traceback = Traceback::new(frames, "TestError", "test");
        assert_eq!(traceback.frames.len(), 3);
    }

    #[test]
    fn test_traceback_title_builder() {
        let traceback = Traceback::new(vec![], "Error", "msg").title("Custom Title");
        // Just verify the method doesn't panic; title is stored internally
        let output = render_to_text(&traceback, 80);
        assert!(output.contains("Custom Title"));
    }

    #[test]
    fn test_traceback_extra_lines_builder() {
        let source = "a\nb\nc\nd\ne";
        let frame = TracebackFrame::new("func", 3).source_context(source, 1);
        let traceback = Traceback::new(vec![frame], "Error", "test").extra_lines(2);

        let output = render_to_text(&traceback, 80);
        // With extra_lines=2, should show lines 1-5 (all of them)
        assert!(output.contains('a'));
        assert!(output.contains('e'));
    }

    #[test]
    fn test_traceback_push_frame() {
        let mut traceback = Traceback::new(vec![], "Error", "test");
        assert_eq!(traceback.frames.len(), 0);

        traceback.push_frame(TracebackFrame::new("added", 1));
        assert_eq!(traceback.frames.len(), 1);
        assert_eq!(traceback.frames[0].name, "added");
    }

    #[test]
    fn test_traceback_clone() {
        let traceback = Traceback::new(vec![TracebackFrame::new("func", 1)], "Error", "message");
        let cloned = traceback.clone();
        assert_eq!(traceback.exception_type, cloned.exception_type);
        assert_eq!(traceback.exception_message, cloned.exception_message);
        assert_eq!(traceback.frames.len(), cloned.frames.len());
    }

    #[test]
    fn test_traceback_eq() {
        let tb1 = Traceback::new(vec![TracebackFrame::new("f", 1)], "E", "m");
        let tb2 = Traceback::new(vec![TracebackFrame::new("f", 1)], "E", "m");
        assert_eq!(tb1, tb2);
    }

    // =========================================================================
    // Rendering Tests (bd-21eb)
    // =========================================================================

    #[test]
    fn test_render_single_frame_no_source() {
        let traceback = Traceback::new(
            vec![TracebackFrame::new("single_func", 99)],
            "SingleError",
            "single message",
        );
        let output = render_to_text(&traceback, 80);

        assert!(output.contains("single_func"));
        assert!(output.contains("99"));
        assert!(output.contains("SingleError: single message"));
    }

    #[test]
    fn test_render_multi_frame_order() {
        let traceback = Traceback::new(
            vec![
                TracebackFrame::new("first", 1),
                TracebackFrame::new("second", 2),
                TracebackFrame::new("third", 3),
            ],
            "Error",
            "test",
        );
        let output = render_to_text(&traceback, 80);

        // All frames should appear
        assert!(output.contains("first"));
        assert!(output.contains("second"));
        assert!(output.contains("third"));
    }

    #[test]
    fn test_render_exception_display_bold_red() {
        let traceback = Traceback::new(vec![], "ValueError", "invalid value");
        let console = Console::new();
        let options = ConsoleOptions {
            max_width: 80,
            ..Default::default()
        };
        let segments = traceback.render(&console, &options);

        // Find the exception type segment
        let exception_seg = segments.iter().find(|s| s.text.contains("ValueError"));
        assert!(exception_seg.is_some());
    }

    #[test]
    fn test_render_source_context_line_numbers() {
        let source = "line1\nline2\nline3";
        let frame = TracebackFrame::new("func", 2).source_context(source, 1);
        let traceback = Traceback::new(vec![frame], "Error", "test").extra_lines(1);

        let output = render_to_text(&traceback, 80);

        // Should show line numbers
        assert!(output.contains('1'));
        assert!(output.contains('2'));
        assert!(output.contains('3'));
    }

    #[test]
    fn test_render_error_line_indicator() {
        let source = "before\nerror_line\nafter";
        let frame = TracebackFrame::new("func", 2).source_context(source, 1);
        let traceback = Traceback::new(vec![frame], "Error", "test").extra_lines(1);

        let output = render_to_text(&traceback, 80);

        // Error indicator should be present
        assert!(output.contains("❱"));
        // Error line content should be shown
        assert!(output.contains("error_line"));
    }

    #[test]
    fn test_render_width_constraint_narrow() {
        let traceback = Traceback::new(
            vec![TracebackFrame::new(
                "very_long_function_name_that_might_wrap",
                123_456,
            )],
            "LongError",
            "a very long error message that might need to be handled",
        );

        // Narrow width should still render without panic
        let output = render_to_text(&traceback, 40);
        assert!(output.contains("LongError"));
    }

    #[test]
    fn test_render_width_constraint_minimum() {
        let traceback = Traceback::new(vec![], "Error", "msg");

        // Even width=1 should work (max(1) in implementation)
        let output = render_to_text(&traceback, 1);
        assert_ne!(output, "");
    }

    #[test]
    fn test_render_with_filename_in_header() {
        let frame = TracebackFrame::new("main", 10)
            .filename("src/main.rs")
            .source_context("fn main() { }", 10);
        let traceback = Traceback::new(vec![frame], "Error", "test");

        let output = render_to_text(&traceback, 80);
        assert!(output.contains("src/main.rs"));
        assert!(output.contains("main"));
        assert!(output.contains("10"));
    }

    #[test]
    fn test_render_without_filename_fallback() {
        let frame = TracebackFrame::new("anonymous", 5).source_context("some_code()", 5);
        let traceback = Traceback::new(vec![frame], "Error", "test");

        let output = render_to_text(&traceback, 80);
        // Should render with "in func:line" format when no filename
        assert!(output.contains("anonymous"));
        assert!(output.contains('5'));
    }

    #[test]
    fn test_render_default_title() {
        let traceback = Traceback::new(vec![], "Error", "test");
        let output = render_to_text(&traceback, 80);

        assert!(output.contains("Traceback"));
    }

    #[test]
    fn test_render_custom_title() {
        let traceback = Traceback::new(vec![], "Error", "test").title("Exception occurred!");
        let output = render_to_text(&traceback, 80);

        assert!(output.contains("Exception occurred!"));
    }

    #[test]
    fn test_render_long_source_line_truncation() {
        let long_line = "x".repeat(200);
        let frame = TracebackFrame::new("func", 1).source_context(&long_line, 1);
        let traceback = Traceback::new(vec![frame], "Error", "test");

        // Should render without panic even with very long lines
        let output = render_to_text(&traceback, 60);
        assert_ne!(output, "");
    }

    #[test]
    fn test_render_unicode_in_source() {
        let source = "let emoji = '🚀';";
        let frame = TracebackFrame::new("func", 1).source_context(source, 1);
        let traceback = Traceback::new(vec![frame], "Error", "test");

        let output = render_to_text(&traceback, 80);
        assert!(output.contains("🚀"));
    }

    #[test]
    fn test_render_empty_exception_message() {
        let traceback = Traceback::new(vec![], "Error", "");
        let output = render_to_text(&traceback, 80);

        assert!(output.contains("Error:"));
    }

    #[test]
    fn test_render_special_chars_in_exception() {
        let traceback = Traceback::new(vec![], "Error<T>", "msg with \"quotes\" & <brackets>");
        let output = render_to_text(&traceback, 80);

        assert!(output.contains("Error<T>"));
        assert!(output.contains("quotes"));
    }

    #[cfg(feature = "backtrace")]
    mod backtrace_tests {
        use super::*;

        fn inner_function() -> Traceback {
            Traceback::capture("TestError", "test message")
        }

        fn outer_function() -> Traceback {
            inner_function()
        }

        #[test]
        fn capture_creates_traceback_with_frames() {
            let traceback = outer_function();

            // Should have at least some frames (our functions)
            assert!(!traceback.frames.is_empty(), "should capture frames");

            // Exception info should be set
            assert_eq!(traceback.exception_type, "TestError");
            assert_eq!(traceback.exception_message, "test message");
        }

        #[test]
        fn capture_filters_internal_frames() {
            let traceback = Traceback::capture("Error", "test");

            // Should not contain std/core frames
            for frame in &traceback.frames {
                assert!(
                    !frame.name.starts_with("std::"),
                    "should filter std:: frames: {}",
                    frame.name
                );
                assert!(
                    !frame.name.starts_with("core::"),
                    "should filter core:: frames: {}",
                    frame.name
                );
            }
        }

        #[test]
        fn is_internal_frame_detects_runtime() {
            assert!(Traceback::is_internal_frame("std::rt::lang_start"));
            assert!(Traceback::is_internal_frame("core::ops::function::FnOnce"));
            assert!(Traceback::is_internal_frame("__libc_start_main"));
            assert!(!Traceback::is_internal_frame("main"));
            assert!(!Traceback::is_internal_frame("my_crate::my_function"));
            assert!(!Traceback::is_internal_frame("app::handler::process"));
        }

        #[test]
        fn demangle_removes_hash_suffix() {
            assert_eq!(
                Traceback::demangle_name("my_crate::func::h1234567890abcdef"),
                "my_crate::func"
            );
            assert_eq!(Traceback::demangle_name("my_crate::func"), "my_crate::func");
        }

        #[test]
        fn capture_renders_without_panic() {
            let traceback = Traceback::capture("PanicError", "something went wrong");
            let output = render_to_text(&traceback, 100);

            assert!(output.contains("PanicError: something went wrong"));
            assert!(output.contains("Traceback"));
        }

        #[test]
        fn is_internal_frame_detects_backtrace_frames() {
            assert!(Traceback::is_internal_frame("backtrace::capture"));
            assert!(Traceback::is_internal_frame("backtrace::Backtrace::new"));
        }

        #[test]
        fn is_internal_frame_detects_alloc_frames() {
            assert!(Traceback::is_internal_frame("<alloc::boxed::Box<F,A>"));
            assert!(Traceback::is_internal_frame("alloc::vec::Vec::push"));
        }

        #[test]
        fn is_internal_frame_detects_rust_internals() {
            assert!(Traceback::is_internal_frame("rust_begin_unwind"));
            assert!(Traceback::is_internal_frame("__rust_start_panic"));
            assert!(Traceback::is_internal_frame("_start"));
            assert!(Traceback::is_internal_frame("clone"));
        }

        #[test]
        fn is_internal_frame_filters_own_capture() {
            assert!(Traceback::is_internal_frame(
                "rich_rust::renderables::traceback::Traceback::capture"
            ));
            assert!(Traceback::is_internal_frame(
                "rich_rust::renderables::traceback::Traceback::from_backtrace"
            ));
        }

        #[test]
        fn demangle_preserves_non_hash_suffix() {
            assert_eq!(
                Traceback::demangle_name("module::func::inner"),
                "module::func::inner"
            );
            assert_eq!(
                Traceback::demangle_name("crate::module::Type::method"),
                "crate::module::Type::method"
            );
        }

        #[test]
        fn demangle_handles_various_suffixes() {
            // Short hex suffix also gets removed (behavior of implementation)
            assert_eq!(Traceback::demangle_name("func::habcd"), "func");
            // Non-hex suffix preserved
            assert_eq!(Traceback::demangle_name("func::hnotahex"), "func::hnotahex");
        }

        #[test]
        fn from_backtrace_creates_traceback() {
            let bt = backtrace::Backtrace::new();
            let traceback = Traceback::from_backtrace(&bt, "BTError", "from backtrace");

            assert_eq!(traceback.exception_type, "BTError");
            assert_eq!(traceback.exception_message, "from backtrace");
        }
    }
}
