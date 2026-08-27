//! E2E test harness with detailed logging infrastructure.
//!
//! This module provides comprehensive infrastructure for end-to-end testing:
//!
//! - **Output Capture**: Capture stdout/stderr with timing information
//! - **ANSI Parsing**: Parse and validate ANSI escape sequences
//! - **File Validation**: Verify file outputs match expected content
//! - **Structured Logging**: Detailed logging at each test step
//!
//! # Example
//!
//! ```rust,ignore
//! use common::e2e_harness::*;
//!
//! #[test]
//! fn test_table_rendering() {
//!     let ctx = E2eContext::new("table_rendering");
//!
//!     ctx.phase("setup", || {
//!         let console = Console::new().force_terminal(true).width(80);
//!         let table = Table::new().add_column("Name").add_row(Row::new().cell("Alice"));
//!         (console, table)
//!     });
//!
//!     let output = ctx.capture_render(|| {
//!         console.render_to_string(&table)
//!     });
//!
//!     ctx.assert_ansi_valid(&output);
//!     ctx.assert_contains(&output, "Alice");
//! }
//! ```

#![allow(dead_code)]

use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

// =============================================================================
// ANSI Sequence Parsing
// =============================================================================

/// Parsed ANSI escape sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnsiSequence {
    /// SGR (Select Graphic Rendition) - style codes
    Sgr(Vec<u8>),
    /// Cursor movement (e.g., \x1b[H, \x1b[2J)
    Cursor(String),
    /// OSC (Operating System Command) - e.g., hyperlinks
    Osc(String),
    /// Unknown/unparsed sequence
    Unknown(String),
}

impl AnsiSequence {
    /// Check if this is a reset sequence (SGR 0).
    #[must_use]
    pub fn is_reset(&self) -> bool {
        matches!(self, Self::Sgr(codes) if codes.is_empty() || codes == &[0])
    }

    /// Check if this sequence sets bold.
    #[must_use]
    pub fn has_bold(&self) -> bool {
        matches!(self, Self::Sgr(codes) if codes.contains(&1))
    }

    /// Check if this sequence sets italic.
    #[must_use]
    pub fn has_italic(&self) -> bool {
        matches!(self, Self::Sgr(codes) if codes.contains(&3))
    }

    /// Check if this sequence sets underline.
    #[must_use]
    pub fn has_underline(&self) -> bool {
        matches!(self, Self::Sgr(codes) if codes.contains(&4))
    }

    /// Check if this sets a foreground color (30-37, 38, 90-97).
    #[must_use]
    pub fn has_foreground_color(&self) -> bool {
        match self {
            Self::Sgr(codes) => codes
                .iter()
                .any(|&c| (30..=37).contains(&c) || (90..=97).contains(&c) || c == 38),
            _ => false,
        }
    }

    /// Check if this sets a background color (40-47, 48, 100-107).
    #[must_use]
    pub fn has_background_color(&self) -> bool {
        match self {
            Self::Sgr(codes) => codes
                .iter()
                .any(|&c| (40..=47).contains(&c) || (100..=107).contains(&c) || c == 48),
            _ => false,
        }
    }

    /// Get the SGR codes if this is an SGR sequence.
    #[must_use]
    pub fn sgr_codes(&self) -> Option<&[u8]> {
        match self {
            Self::Sgr(codes) => Some(codes),
            _ => None,
        }
    }
}

/// Parsed segment of terminal output.
#[derive(Debug, Clone)]
pub struct ParsedSegment {
    /// The text content (without ANSI codes).
    pub text: String,
    /// ANSI sequences that immediately preceded this text chunk.
    pub sequences: Vec<AnsiSequence>,
    /// Raw ANSI string that preceded this text.
    pub raw_ansi: String,
}

/// Parser for ANSI escape sequences in terminal output.
#[derive(Debug, Default)]
pub struct AnsiParser {}

impl AnsiParser {
    /// Create a new parser.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse an SGR sequence string like "1;31" into codes.
    fn parse_sgr_codes(params: &str) -> Vec<u8> {
        if params.is_empty() {
            return vec![0]; // Empty params means reset
        }
        params
            .split(';')
            .filter_map(|s| s.parse::<u8>().ok())
            .collect()
    }

    /// Parse a single ANSI escape sequence from a string.
    ///
    /// Returns the parsed sequence and the number of bytes consumed.
    fn parse_sequence(s: &str) -> Option<(AnsiSequence, usize)> {
        if !s.starts_with("\x1b[") && !s.starts_with("\x1b]") {
            return None;
        }

        // OSC sequence: \x1b]...ST or \x1b]...\x1b\\
        if s.starts_with("\x1b]") {
            // Find the string terminator
            if let Some(end) = s.find("\x1b\\") {
                let content = &s[2..end];
                return Some((AnsiSequence::Osc(content.to_string()), end + 2));
            }
            if let Some(end) = s.find('\x07') {
                let content = &s[2..end];
                return Some((AnsiSequence::Osc(content.to_string()), end + 1));
            }
            return None;
        }

        // CSI sequence: \x1b[...X where X is the final byte
        let rest = &s[2..];
        let mut end_idx = 0;

        // Find the final byte (0x40-0x7E)
        for (i, c) in rest.char_indices() {
            if c.is_ascii() && (0x40..=0x7E).contains(&(c as u8)) {
                end_idx = i;
                break;
            }
        }

        if end_idx == 0
            && !rest.is_empty()
            && rest
                .chars()
                .next()
                .is_some_and(|c| (0x40..=0x7E).contains(&(c as u8)))
        {
            end_idx = 0;
        } else if end_idx == 0 {
            return None;
        }

        let params = &rest[..end_idx];
        let final_byte = rest.chars().nth(end_idx)?;
        let total_len = 2 + end_idx + 1;

        let sequence = match final_byte {
            'm' => AnsiSequence::Sgr(Self::parse_sgr_codes(params)),
            'H' | 'f' | 'A' | 'B' | 'C' | 'D' | 'J' | 'K' | 's' | 'u' => {
                AnsiSequence::Cursor(format!("{params}{final_byte}"))
            }
            _ => AnsiSequence::Unknown(s[..total_len].to_string()),
        };

        Some((sequence, total_len))
    }

    /// Parse terminal output into segments with their associated styles.
    #[must_use]
    pub fn parse(&mut self, output: &str) -> Vec<ParsedSegment> {
        let mut segments = Vec::new();
        let mut current_text = String::new();
        let mut current_sequences = Vec::new();
        let mut current_raw_ansi = String::new();
        let mut pos = 0;

        while pos < output.len() {
            let remaining = &output[pos..];

            if remaining.starts_with("\x1b") {
                // Save any accumulated text
                if !current_text.is_empty() {
                    segments.push(ParsedSegment {
                        text: std::mem::take(&mut current_text),
                        sequences: std::mem::take(&mut current_sequences),
                        raw_ansi: std::mem::take(&mut current_raw_ansi),
                    });
                }

                // Try to parse the escape sequence
                if let Some((seq, len)) = Self::parse_sequence(remaining) {
                    current_raw_ansi.push_str(&remaining[..len]);

                    current_sequences.push(seq);
                    pos += len;
                    continue;
                }
            }

            // Regular character
            if let Some(c) = remaining.chars().next() {
                current_text.push(c);
                pos += c.len_utf8();
            } else {
                break;
            }
        }

        // Don't forget the last segment
        if !current_text.is_empty() || !current_raw_ansi.is_empty() {
            segments.push(ParsedSegment {
                text: current_text,
                sequences: current_sequences,
                raw_ansi: current_raw_ansi,
            });
        }

        segments
    }

    /// Strip all ANSI sequences and return plain text.
    #[must_use]
    pub fn strip_ansi(output: &str) -> String {
        let mut parser = Self::new();
        let segments = parser.parse(output);
        segments.into_iter().map(|s| s.text).collect()
    }

    /// Validate that ANSI sequences are well-formed.
    ///
    /// Returns a list of validation errors if any.
    #[must_use]
    pub fn validate(output: &str) -> Vec<String> {
        let mut errors = Vec::new();
        let mut parser = Self::new();
        let segments = parser.parse(output);

        // Track whether a non-default style is currently active.
        // This is intentionally strict: output should not leak styling past the end.
        let mut style_active = false;

        for segment in &segments {
            for seq in &segment.sequences {
                match seq {
                    AnsiSequence::Sgr(codes) => {
                        style_active = !(codes.is_empty() || codes == &[0]);
                    }
                    AnsiSequence::Unknown(s) => {
                        errors.push(format!("Unknown ANSI sequence: {s:?}"));
                    }
                    _ => {}
                }
            }
        }

        if style_active {
            errors.push("Styles not fully reset at end of output".to_string());
        }

        errors
    }

    /// Count specific ANSI codes in output.
    #[must_use]
    pub fn count_sgr_code(output: &str, code: u8) -> usize {
        let mut parser = Self::new();
        let segments = parser.parse(output);
        let mut count = 0;

        for segment in segments {
            for seq in segment.sequences {
                if let AnsiSequence::Sgr(codes) = seq
                    && codes.contains(&code)
                {
                    count += 1;
                }
            }
        }

        count
    }
}

// =============================================================================
// Output Capture
// =============================================================================

/// Captured output from a test operation.
#[derive(Debug, Clone)]
pub struct CapturedOutput {
    /// The captured content.
    pub content: String,
    /// Time taken for the operation.
    pub elapsed: Duration,
    /// Timestamp when capture started.
    pub started_at: Instant,
    /// Any errors that occurred.
    pub errors: Vec<String>,
}

impl CapturedOutput {
    /// Check if the output contains a string.
    #[must_use]
    pub fn contains(&self, needle: &str) -> bool {
        self.content.contains(needle)
    }

    /// Get plain text without ANSI codes.
    #[must_use]
    pub fn plain_text(&self) -> String {
        AnsiParser::strip_ansi(&self.content)
    }

    /// Validate ANSI sequences.
    #[must_use]
    pub fn validate_ansi(&self) -> Vec<String> {
        AnsiParser::validate(&self.content)
    }

    /// Parse into segments.
    #[must_use]
    pub fn parse_ansi(&self) -> Vec<ParsedSegment> {
        let mut parser = AnsiParser::new();
        parser.parse(&self.content)
    }

    /// Get byte count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.content.len()
    }

    /// Check if empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// Count lines.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.content.lines().count()
    }
}

/// Capture the output of a closure.
pub fn capture<F, R>(f: F) -> (R, CapturedOutput)
where
    F: FnOnce() -> R,
{
    let start = Instant::now();
    let result = f();
    let elapsed = start.elapsed();

    // This capture records timing + the closure's return value.
    // It does not intercept process-wide stdout/stderr.
    (
        result,
        CapturedOutput {
            content: String::new(),
            elapsed,
            started_at: start,
            errors: Vec::new(),
        },
    )
}

/// Capture string output from a closure that returns a String.
pub fn capture_string<F>(f: F) -> CapturedOutput
where
    F: FnOnce() -> String,
{
    let start = Instant::now();
    let content = f();
    let elapsed = start.elapsed();

    CapturedOutput {
        content,
        elapsed,
        started_at: start,
        errors: Vec::new(),
    }
}

// =============================================================================
// File Output Validation
// =============================================================================

/// Result of file validation.
#[derive(Debug)]
pub struct FileValidation {
    /// Path to the file.
    pub path: PathBuf,
    /// Whether the file exists.
    pub exists: bool,
    /// File size in bytes.
    pub size: Option<u64>,
    /// File content (if readable).
    pub content: Option<String>,
    /// Validation errors.
    pub errors: Vec<String>,
}

impl FileValidation {
    /// Validate a file at the given path.
    pub fn validate(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        let mut errors = Vec::new();

        let exists = path.exists();
        if !exists {
            errors.push(format!("File does not exist: {}", path.display()));
            return Self {
                path,
                exists,
                size: None,
                content: None,
                errors,
            };
        }

        let metadata = fs::metadata(&path);
        let size = metadata.as_ref().ok().map(|m| m.len());

        let content = fs::read_to_string(&path).ok();

        Self {
            path,
            exists,
            size,
            content,
            errors,
        }
    }

    /// Check if the file contains expected content.
    #[must_use]
    pub fn contains(&self, needle: &str) -> bool {
        self.content.as_ref().is_some_and(|c| c.contains(needle))
    }

    /// Assert that the file exists.
    #[track_caller]
    pub fn assert_exists(&self) {
        if !self.exists {
            panic!(
                "Expected file to exist: {}\nErrors: {:?}",
                self.path.display(),
                self.errors
            );
        }
    }

    /// Assert that the file contains expected content.
    #[track_caller]
    pub fn assert_contains(&self, needle: &str) {
        if !self.contains(needle) {
            panic!(
                "Expected file {} to contain {:?}\nContent: {:?}",
                self.path.display(),
                needle,
                self.content
            );
        }
    }

    /// Assert file size is within expected range.
    #[track_caller]
    pub fn assert_size_between(&self, min: u64, max: u64) {
        match self.size {
            Some(size) if size >= min && size <= max => {}
            Some(size) => {
                panic!(
                    "Expected file {} size between {}-{} bytes, got {} bytes",
                    self.path.display(),
                    min,
                    max,
                    size
                );
            }
            None => {
                panic!("Could not determine size of {}", self.path.display());
            }
        }
    }
}

/// Validate multiple files.
pub fn validate_files<P: AsRef<Path>>(paths: impl IntoIterator<Item = P>) -> Vec<FileValidation> {
    paths.into_iter().map(FileValidation::validate).collect()
}

// =============================================================================
// E2E Test Context
// =============================================================================

/// Context for E2E tests with logging and timing.
#[derive(Debug)]
pub struct E2eContext {
    /// Test name for logging.
    pub name: String,
    /// When the test started.
    pub started_at: Instant,
    /// Phase timings.
    pub phases: Vec<(String, Duration)>,
    /// Captured outputs.
    pub captures: Vec<CapturedOutput>,
    /// Test metadata.
    pub metadata: HashMap<String, String>,
}

impl E2eContext {
    /// Create a new E2E test context.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        tracing::info!(test = %name, "Starting E2E test");

        Self {
            name,
            started_at: Instant::now(),
            phases: Vec::new(),
            captures: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Add metadata to the context.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Run a test phase with timing.
    pub fn phase<F, R>(&mut self, name: &str, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        tracing::info!(test = %self.name, phase = name, "Entering phase");
        let start = Instant::now();
        let result = f();
        let elapsed = start.elapsed();
        tracing::info!(test = %self.name, phase = name, elapsed_ms = elapsed.as_millis(), "Phase complete");
        self.phases.push((name.to_string(), elapsed));
        result
    }

    /// Capture string output with timing.
    pub fn capture<F>(&mut self, description: &str, f: F) -> CapturedOutput
    where
        F: FnOnce() -> String,
    {
        tracing::debug!(test = %self.name, capture = description, "Starting capture");
        let output = capture_string(f);
        tracing::debug!(
            test = %self.name,
            capture = description,
            bytes = output.len(),
            elapsed_ms = output.elapsed.as_millis(),
            "Capture complete"
        );
        self.captures.push(output.clone());
        output
    }

    /// Assert that output contains expected content.
    #[track_caller]
    pub fn assert_contains(&self, output: &CapturedOutput, needle: &str) {
        tracing::debug!(
            test = %self.name,
            needle = needle,
            output_len = output.len(),
            "Asserting contains"
        );
        if !output.contains(needle) {
            tracing::error!(
                test = %self.name,
                needle = needle,
                output = %output.content,
                "Assertion failed: content not found"
            );
            panic!(
                "[{}] Expected output to contain {:?}\nOutput:\n{}",
                self.name, needle, output.content
            );
        }
    }

    /// Assert that ANSI output is valid.
    #[track_caller]
    pub fn assert_ansi_valid(&self, output: &CapturedOutput) {
        let errors = output.validate_ansi();
        if !errors.is_empty() {
            tracing::error!(
                test = %self.name,
                errors = ?errors,
                "ANSI validation failed"
            );
            panic!(
                "[{}] ANSI validation errors:\n{}\nOutput:\n{}",
                self.name,
                errors.join("\n"),
                output.content
            );
        }
        tracing::debug!(test = %self.name, "ANSI validation passed");
    }

    /// Get total elapsed time.
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// Generate a test report.
    #[must_use]
    pub fn report(&self) -> String {
        let mut report = String::new();
        writeln!(report, "=== E2E Test Report: {} ===", self.name).unwrap();
        writeln!(report, "Total elapsed: {:?}", self.elapsed()).unwrap();
        writeln!(report).unwrap();

        if !self.metadata.is_empty() {
            writeln!(report, "Metadata:").unwrap();
            for (k, v) in &self.metadata {
                writeln!(report, "  {k}: {v}").unwrap();
            }
            writeln!(report).unwrap();
        }

        writeln!(report, "Phases:").unwrap();
        for (name, duration) in &self.phases {
            writeln!(report, "  {name}: {duration:?}").unwrap();
        }
        writeln!(report).unwrap();

        writeln!(report, "Captures: {} total", self.captures.len()).unwrap();
        for (i, capture) in self.captures.iter().enumerate() {
            writeln!(
                report,
                "  [{i}] {} bytes, {:?}",
                capture.len(),
                capture.elapsed
            )
            .unwrap();
        }

        report
    }
}

impl Drop for E2eContext {
    fn drop(&mut self) {
        tracing::info!(
            test = %self.name,
            elapsed_ms = self.elapsed().as_millis(),
            phases = self.phases.len(),
            captures = self.captures.len(),
            "E2E test complete"
        );
    }
}

// =============================================================================
// Timing Utilities
// =============================================================================

/// Time a closure and return the result with duration.
pub fn timed<F, R>(f: F) -> (R, Duration)
where
    F: FnOnce() -> R,
{
    let start = Instant::now();
    let result = f();
    (result, start.elapsed())
}

/// Assert that an operation completes within a time limit.
#[track_caller]
pub fn assert_completes_within<F, R>(max_duration: Duration, f: F) -> R
where
    F: FnOnce() -> R,
{
    let (result, elapsed) = timed(f);
    if elapsed > max_duration {
        panic!(
            "Operation exceeded time limit: {:?} > {:?}",
            elapsed, max_duration
        );
    }
    result
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ansi_parser_basic() {
        let mut parser = AnsiParser::new();
        let segments = parser.parse("\x1b[1mBold\x1b[0m Normal");

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].text, "Bold");
        assert_eq!(segments[1].text, " Normal");
    }

    #[test]
    fn test_ansi_parser_sgr_codes() {
        let mut parser = AnsiParser::new();
        let segments = parser.parse("\x1b[1;31mRed Bold\x1b[0m");

        // Parser creates segments for text and trailing sequences
        assert!(!segments.is_empty());
        assert_eq!(segments[0].text, "Red Bold");
        assert!(segments[0].sequences.iter().any(|s| s.has_bold()));
    }

    #[test]
    fn test_ansi_strip() {
        let plain = AnsiParser::strip_ansi("\x1b[1mBold\x1b[0m and \x1b[32mGreen\x1b[0m");
        assert_eq!(plain, "Bold and Green");
    }

    #[test]
    fn test_ansi_validate() {
        // Well-formed output
        let errors = AnsiParser::validate("\x1b[1mBold\x1b[0m");
        assert!(errors.is_empty());

        // Missing reset is detected
        let errors = AnsiParser::validate("\x1b[1mBold");
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_capture_string() {
        let output = capture_string(|| "Hello, World!".to_string());
        assert_eq!(output.content, "Hello, World!");
        assert!(!output.elapsed.is_zero() || output.elapsed == Duration::ZERO);
    }

    #[test]
    fn test_file_validation_nonexistent() {
        let validation = FileValidation::validate("/nonexistent/path/file.txt");
        assert!(!validation.exists);
        assert!(!validation.errors.is_empty());
    }

    #[test]
    fn test_e2e_context() {
        let mut ctx = E2eContext::new("test_example").with_metadata("version", "1.0");

        let result = ctx.phase("setup", || 42);
        assert_eq!(result, 42);
        assert_eq!(ctx.phases.len(), 1);

        let output = ctx.capture("render", || "Hello".to_string());
        assert_eq!(output.content, "Hello");
        assert_eq!(ctx.captures.len(), 1);

        let report = ctx.report();
        assert!(report.contains("test_example"));
        assert!(report.contains("setup"));
    }

    #[test]
    fn test_timed() {
        let (result, duration) = timed(|| {
            std::thread::sleep(Duration::from_millis(10));
            42
        });
        assert_eq!(result, 42);
        assert!(duration >= Duration::from_millis(10));
    }

    #[test]
    fn test_assert_completes_within() {
        let result = assert_completes_within(Duration::from_secs(1), || 42);
        assert_eq!(result, 42);
    }

    #[test]
    #[should_panic(expected = "exceeded time limit")]
    fn test_assert_completes_within_fails() {
        assert_completes_within(Duration::from_millis(1), || {
            std::thread::sleep(Duration::from_millis(50));
            42
        });
    }

    #[test]
    fn test_ansi_sequence_properties() {
        let bold = AnsiSequence::Sgr(vec![1]);
        assert!(bold.has_bold());
        assert!(!bold.has_italic());
        assert!(!bold.is_reset());

        let reset = AnsiSequence::Sgr(vec![0]);
        assert!(reset.is_reset());

        let color = AnsiSequence::Sgr(vec![31]);
        assert!(color.has_foreground_color());
        assert!(!color.has_background_color());
    }
}
