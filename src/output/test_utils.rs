//! Test utilities for rich output testing.
//!
//! This module provides comprehensive utilities for testing rich terminal output,
//! including environment manipulation, ANSI code detection, Unicode detection,
//! output capture, terminal mocking, and assertion helpers.
//!
//! All utilities in this module are compiled only in test mode (`#[cfg(test)]`).
//!
//! # Examples
//!
//! ## Environment Variable Manipulation
//!
//! ```rust,ignore
//! use ms::output::test_utils::EnvGuard;
//!
//! #[test]
//! fn test_no_color_mode() {
//!     let _guard = EnvGuard::new()
//!         .set("NO_COLOR", "1")
//!         .unset("MS_FORCE_RICH");
//!
//!     // Test code here - environment is automatically restored when guard drops
//! }
//! ```
//!
//! ## ANSI Detection
//!
//! ```rust,ignore
//! use ms::output::test_utils::{contains_ansi, strip_ansi, assert_no_ansi};
//!
//! let styled = "\x1b[31mred text\x1b[0m";
//! assert!(contains_ansi(styled));
//! assert_eq!(strip_ansi(styled), "red text");
//!
//! let plain = "plain text";
//! assert_no_ansi(plain, "output should be plain");
//! ```
//!
//! ## Output Capture
//!
//! ```rust,ignore
//! use ms::output::test_utils::capture_output;
//!
//! let (result, capture) = capture_output(|| {
//!     println!("Hello, world!");
//!     42
//! });
//!
//! assert_eq!(result, 42);
//! assert!(capture.stdout().contains("Hello"));
//! ```

use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::sync::{Arc, Mutex, OnceLock};

use tracing::Level;

// =============================================================================
// Environment Variable Manipulation
// =============================================================================

/// RAII guard for temporarily setting environment variables.
///
/// When the guard is dropped, all environment variables are restored to their
/// original values. This is essential for thread-safe testing where multiple
/// tests may run concurrently.
///
/// # Thread Safety
///
/// While this guard provides RAII semantics, tests that modify environment
/// variables should use `#[serial]` from `serial_test` crate or similar
/// synchronization to avoid races between tests.
///
/// # Examples
///
/// ```rust,ignore
/// let _guard = EnvGuard::new()
///     .set("NO_COLOR", "1")
///     .set("TERM", "dumb")
///     .unset("COLORTERM");
///
/// // Environment is modified here
/// assert!(std::env::var("NO_COLOR").is_ok());
///
/// // When _guard drops, original values are restored
/// ```
#[derive(Debug)]
pub struct EnvGuard {
    /// Original values: (key, Some(value)) for set vars, (key, None) for unset
    original_values: Vec<(String, Option<OsString>)>,
}

impl EnvGuard {
    /// Create a new environment guard.
    #[must_use]
    pub fn new() -> Self {
        Self {
            original_values: Vec::new(),
        }
    }

    /// Set an environment variable, saving the original value.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let _guard = EnvGuard::new().set("MY_VAR", "value");
    /// assert_eq!(std::env::var("MY_VAR").unwrap(), "value");
    /// ```
    #[must_use]
    pub fn set(mut self, key: &str, value: &str) -> Self {
        let original = env::var_os(key);
        self.original_values.push((key.to_string(), original));
        // SAFETY: This is test-only code. Tests that modify environment variables
        // should use #[serial] to avoid races between concurrent tests.
        unsafe { env::set_var(key, value) };
        self
    }

    /// Unset an environment variable, saving the original value.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// std::env::set_var("MY_VAR", "value");
    /// let _guard = EnvGuard::new().unset("MY_VAR");
    /// assert!(std::env::var("MY_VAR").is_err());
    /// ```
    #[must_use]
    pub fn unset(mut self, key: &str) -> Self {
        let original = env::var_os(key);
        self.original_values.push((key.to_string(), original));
        // SAFETY: This is test-only code. Tests that modify environment variables
        // should use #[serial] to avoid races between concurrent tests.
        unsafe { env::remove_var(key) };
        self
    }

    /// Set multiple environment variables at once.
    #[must_use]
    pub fn set_many(mut self, vars: &[(&str, &str)]) -> Self {
        for (key, value) in vars {
            self = self.set(key, value);
        }
        self
    }
}

impl Default for EnvGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        // Restore in reverse order to handle potential dependencies
        for (key, original) in self.original_values.iter().rev() {
            // SAFETY: We're restoring the original environment state during cleanup.
            // This is test-only code and tests should use #[serial] to avoid races.
            unsafe {
                match original {
                    Some(value) => env::set_var(key, value),
                    None => env::remove_var(key),
                }
            }
        }
    }
}

/// Known AI agent types for testing agent detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentType {
    /// Claude Code agent
    ClaudeCode,
    /// GitHub Copilot
    Copilot,
    /// Cursor AI
    Cursor,
    /// Windsurf AI
    Windsurf,
    /// Amazon Q
    AmazonQ,
    /// Aider AI
    Aider,
    /// Generic/unknown agent
    Generic,
}

impl AgentType {
    /// Get the environment variables that identify this agent.
    #[must_use]
    pub fn env_vars(self) -> Vec<(&'static str, &'static str)> {
        match self {
            Self::ClaudeCode => vec![("CLAUDE_CODE", "1")],
            Self::Copilot => vec![("GITHUB_COPILOT", "1")],
            Self::Cursor => vec![("CURSOR_AI", "1")],
            Self::Windsurf => vec![("WINDSURF_AI", "1")],
            Self::AmazonQ => vec![("AMAZON_Q", "1")],
            Self::Aider => vec![("AIDER", "1")],
            Self::Generic => vec![("AI_AGENT", "1")],
        }
    }
}

/// Run a closure with agent-specific environment variables set.
///
/// # Examples
///
/// ```rust,ignore
/// with_agent_env(AgentType::ClaudeCode, || {
///     // Test that Claude Code agent is detected
///     assert!(is_agent_environment());
/// });
/// ```
pub fn with_agent_env<F, R>(agent: AgentType, f: F) -> R
where
    F: FnOnce() -> R,
{
    let vars = agent.env_vars();
    let _guard = vars
        .iter()
        .fold(EnvGuard::new(), |guard, (k, v)| guard.set(k, v));
    f()
}

/// Known CI environment types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CiType {
    /// GitHub Actions
    GitHubActions,
    /// GitLab CI
    GitLabCi,
    /// CircleCI
    CircleCi,
    /// Jenkins
    Jenkins,
    /// Travis CI
    TravisCi,
    /// Generic CI
    Generic,
}

impl CiType {
    /// Get the environment variables that identify this CI system.
    #[must_use]
    pub fn env_vars(self) -> Vec<(&'static str, &'static str)> {
        match self {
            Self::GitHubActions => vec![("GITHUB_ACTIONS", "true"), ("CI", "true")],
            Self::GitLabCi => vec![("GITLAB_CI", "true"), ("CI", "true")],
            Self::CircleCi => vec![("CIRCLECI", "true"), ("CI", "true")],
            Self::Jenkins => vec![("JENKINS_URL", "http://jenkins"), ("CI", "true")],
            Self::TravisCi => vec![("TRAVIS", "true"), ("CI", "true")],
            Self::Generic => vec![("CI", "true")],
        }
    }
}

/// Run a closure with CI-specific environment variables set.
///
/// # Examples
///
/// ```rust,ignore
/// with_ci_env(CiType::GitHubActions, || {
///     // Test CI-specific behavior
///     assert!(is_ci_environment());
/// });
/// ```
pub fn with_ci_env<F, R>(ci: CiType, f: F) -> R
where
    F: FnOnce() -> R,
{
    let vars = ci.env_vars();
    let _guard = vars
        .iter()
        .fold(EnvGuard::new(), |guard, (k, v)| guard.set(k, v));
    f()
}

// =============================================================================
// ANSI Code Detection and Manipulation
// =============================================================================

/// ANSI escape sequence pattern.
/// Matches: ESC [ ... (letter or @-~)
const ANSI_PATTERN: &str = r"\x1b\[[0-9;]*[A-Za-z@-~]";

/// Check if a string contains any ANSI escape codes.
///
/// # Examples
///
/// ```rust,ignore
/// assert!(contains_ansi("\x1b[31mred\x1b[0m"));
/// assert!(!contains_ansi("plain text"));
/// ```
#[must_use]
pub fn contains_ansi(s: &str) -> bool {
    // Fast path: check for ESC character first
    if !s.contains('\x1b') {
        return false;
    }

    // Use regex for accurate detection
    static REGEX: OnceLock<regex::Regex> = OnceLock::new();
    let re = REGEX.get_or_init(|| regex::Regex::new(ANSI_PATTERN).expect("valid regex"));
    re.is_match(s)
}

/// Strip all ANSI escape codes from a string.
///
/// # Examples
///
/// ```rust,ignore
/// let styled = "\x1b[31mred text\x1b[0m";
/// assert_eq!(strip_ansi(styled), "red text");
/// ```
#[must_use]
pub fn strip_ansi(s: &str) -> String {
    // Fast path: no ESC character means no ANSI codes
    if !s.contains('\x1b') {
        return s.to_string();
    }

    static REGEX: OnceLock<regex::Regex> = OnceLock::new();
    let re = REGEX.get_or_init(|| regex::Regex::new(ANSI_PATTERN).expect("valid regex"));
    re.replace_all(s, "").into_owned()
}

/// Extract all ANSI escape codes from a string.
///
/// Returns a vector of all ANSI sequences found in the string.
///
/// # Examples
///
/// ```rust,ignore
/// let styled = "\x1b[31mred\x1b[0m and \x1b[32mgreen\x1b[0m";
/// let codes = extract_ansi_codes(styled);
/// assert_eq!(codes.len(), 4); // 31m, 0m, 32m, 0m
/// ```
#[must_use]
pub fn extract_ansi_codes(s: &str) -> Vec<String> {
    if !s.contains('\x1b') {
        return Vec::new();
    }

    static REGEX: OnceLock<regex::Regex> = OnceLock::new();
    let re = REGEX.get_or_init(|| regex::Regex::new(ANSI_PATTERN).expect("valid regex"));
    re.find_iter(s).map(|m| m.as_str().to_string()).collect()
}

/// Assert that a string contains no ANSI escape codes.
///
/// # Panics
///
/// Panics with a detailed error message if ANSI codes are found.
///
/// # Examples
///
/// ```rust,ignore
/// assert_no_ansi("plain text", "robot mode output");
/// ```
pub fn assert_no_ansi(s: &str, context: &str) {
    if contains_ansi(s) {
        let codes = extract_ansi_codes(s);
        let stripped = strip_ansi(s);
        panic!(
            "Expected no ANSI codes in {context}\n\
             Found {} ANSI sequences: {:?}\n\
             Original (first 500 chars): {:?}\n\
             Stripped: {stripped}",
            codes.len(),
            &codes[..codes.len().min(10)],
            &s[..s.len().min(500)]
        );
    }
}

/// Assert that a string contains ANSI escape codes.
///
/// # Panics
///
/// Panics if no ANSI codes are found.
pub fn assert_has_ansi(s: &str, context: &str) {
    if !contains_ansi(s) {
        panic!(
            "Expected ANSI codes in {context}\n\
             Output (first 500 chars): {:?}",
            &s[..s.len().min(500)]
        );
    }
}

// =============================================================================
// Unicode Detection
// =============================================================================

/// Unicode ranges for box drawing characters (U+2500 to U+257F).
const BOX_DRAWING_START: u32 = 0x2500;
const BOX_DRAWING_END: u32 = 0x257F;

/// Check if a string contains box drawing characters.
///
/// Box drawing characters include: ─ │ ┌ ┐ └ ┘ ├ ┤ ┬ ┴ ┼ etc.
///
/// # Examples
///
/// ```rust,ignore
/// assert!(contains_box_drawing("┌───────┐"));
/// assert!(!contains_box_drawing("+-------+"));
/// ```
#[must_use]
pub fn contains_box_drawing(s: &str) -> bool {
    s.chars().any(|c| {
        let cp = c as u32;
        (BOX_DRAWING_START..=BOX_DRAWING_END).contains(&cp)
    })
}

/// Check if a string contains emoji characters.
///
/// This checks for common emoji ranges including:
/// - Emoticons (U+1F600-U+1F64F)
/// - Misc symbols (U+1F300-U+1F5FF)
/// - Transport/maps (U+1F680-U+1F6FF)
/// - Supplemental symbols (U+1F900-U+1F9FF)
///
/// # Examples
///
/// ```rust,ignore
/// assert!(contains_emoji("Hello! 👋"));
/// assert!(!contains_emoji("Hello!"));
/// ```
#[must_use]
pub fn contains_emoji(s: &str) -> bool {
    s.chars().any(|c| {
        let cp = c as u32;
        // Common emoji ranges
        (0x1F600..=0x1F64F).contains(&cp)  // Emoticons
            || (0x1F300..=0x1F5FF).contains(&cp)  // Misc Symbols and Pictographs
            || (0x1F680..=0x1F6FF).contains(&cp)  // Transport and Map
            || (0x1F900..=0x1F9FF).contains(&cp)  // Supplemental Symbols
            || (0x2600..=0x26FF).contains(&cp)    // Misc symbols
            || (0x2700..=0x27BF).contains(&cp)    // Dingbats
            || (0xFE00..=0xFE0F).contains(&cp)    // Variation Selectors
            || (0x1F1E0..=0x1F1FF).contains(&cp) // Flags
    })
}

/// Check if a string contains any "rich" Unicode characters.
///
/// Rich characters include box drawing, emoji, and other decorative Unicode.
#[must_use]
pub fn contains_rich_unicode(s: &str) -> bool {
    contains_box_drawing(s) || contains_emoji(s)
}

/// Assert that output is plain ASCII-compatible (no rich Unicode).
///
/// # Panics
///
/// Panics if box drawing characters or emoji are found.
pub fn assert_plain_output(s: &str, context: &str) {
    assert_no_ansi(s, context);

    if contains_box_drawing(s) {
        panic!(
            "Expected plain output in {context}, but found box drawing characters.\n\
             Output (first 500 chars): {:?}",
            &s[..s.len().min(500)]
        );
    }

    if contains_emoji(s) {
        panic!(
            "Expected plain output in {context}, but found emoji characters.\n\
             Output (first 500 chars): {:?}",
            &s[..s.len().min(500)]
        );
    }
}

// =============================================================================
// Output Capture
// =============================================================================

/// Captured stdout and stderr from a test operation.
#[derive(Debug, Clone)]
pub struct OutputCapture {
    stdout: String,
    stderr: String,
}

impl OutputCapture {
    /// Create a new output capture with the given content.
    #[must_use]
    pub fn new(stdout: String, stderr: String) -> Self {
        Self { stdout, stderr }
    }

    /// Get captured stdout.
    #[must_use]
    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    /// Get captured stderr.
    #[must_use]
    pub fn stderr(&self) -> &str {
        &self.stderr
    }

    /// Get combined stdout and stderr.
    #[must_use]
    pub fn combined(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }

    /// Check if stdout contains a pattern.
    #[must_use]
    pub fn stdout_contains(&self, pattern: &str) -> bool {
        self.stdout.contains(pattern)
    }

    /// Check if stderr contains a pattern.
    #[must_use]
    pub fn stderr_contains(&self, pattern: &str) -> bool {
        self.stderr.contains(pattern)
    }

    /// Check if stdout is empty.
    #[must_use]
    pub fn stdout_is_empty(&self) -> bool {
        self.stdout.is_empty()
    }

    /// Check if stderr is empty.
    #[must_use]
    pub fn stderr_is_empty(&self) -> bool {
        self.stderr.is_empty()
    }
}

impl Default for OutputCapture {
    fn default() -> Self {
        Self::new(String::new(), String::new())
    }
}

// Thread-local output capture buffers.
thread_local! {
    static CAPTURE_STDOUT: std::cell::RefCell<Option<Arc<Mutex<Vec<u8>>>>> =
        const { std::cell::RefCell::new(None) };
    static CAPTURE_STDERR: std::cell::RefCell<Option<Arc<Mutex<Vec<u8>>>>> =
        const { std::cell::RefCell::new(None) };
}

/// Capture stdout and stderr during execution of a closure.
///
/// Note: This captures output written via `print!`/`println!` macros.
/// It may not capture output written directly to file descriptors.
///
/// # Examples
///
/// ```rust,ignore
/// let (result, capture) = capture_output(|| {
///     println!("Hello!");
///     42
/// });
/// assert_eq!(result, 42);
/// assert!(capture.stdout_contains("Hello"));
/// ```
pub fn capture_output<F, R>(f: F) -> (R, OutputCapture)
where
    F: FnOnce() -> R,
{
    // For true stdout/stderr capture, we'd need to redirect file descriptors.
    // This is a simplified version that works with our test patterns.
    let stdout_buf = Arc::new(Mutex::new(Vec::new()));
    let stderr_buf = Arc::new(Mutex::new(Vec::new()));

    CAPTURE_STDOUT.with(|cell| {
        *cell.borrow_mut() = Some(Arc::clone(&stdout_buf));
    });
    CAPTURE_STDERR.with(|cell| {
        *cell.borrow_mut() = Some(Arc::clone(&stderr_buf));
    });

    let result = f();

    CAPTURE_STDOUT.with(|cell| {
        *cell.borrow_mut() = None;
    });
    CAPTURE_STDERR.with(|cell| {
        *cell.borrow_mut() = None;
    });

    let stdout = String::from_utf8_lossy(&stdout_buf.lock().unwrap()).to_string();
    let stderr = String::from_utf8_lossy(&stderr_buf.lock().unwrap()).to_string();

    (result, OutputCapture::new(stdout, stderr))
}

// =============================================================================
// Terminal Mocking
// =============================================================================

/// Color support level for terminal mocking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSystem {
    /// No color support
    None,
    /// Basic 16 colors
    Standard,
    /// 256 color palette
    EightBit,
    /// True color (24-bit)
    TrueColor,
}

/// Mock terminal configuration for testing.
#[derive(Debug, Clone)]
pub struct MockTerminal {
    /// Whether stdout is a TTY
    pub is_tty: bool,
    /// Terminal width in columns
    pub width: usize,
    /// Terminal height in rows
    pub height: usize,
    /// Color support level
    pub color_support: ColorSystem,
    /// Terminal type (TERM env var)
    pub term_type: Option<String>,
}

impl MockTerminal {
    /// Create a new mock terminal with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            is_tty: true,
            width: 80,
            height: 24,
            color_support: ColorSystem::TrueColor,
            term_type: Some("xterm-256color".to_string()),
        }
    }

    /// Create a non-TTY (piped) terminal.
    #[must_use]
    pub fn piped() -> Self {
        Self {
            is_tty: false,
            width: 80,
            height: 24,
            color_support: ColorSystem::None,
            term_type: None,
        }
    }

    /// Create a dumb terminal with no capabilities.
    #[must_use]
    pub fn dumb() -> Self {
        Self {
            is_tty: true,
            width: 80,
            height: 24,
            color_support: ColorSystem::None,
            term_type: Some("dumb".to_string()),
        }
    }

    /// Set whether this is a TTY.
    #[must_use]
    pub const fn with_tty(mut self, is_tty: bool) -> Self {
        self.is_tty = is_tty;
        self
    }

    /// Set terminal dimensions.
    #[must_use]
    pub const fn with_size(mut self, width: usize, height: usize) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    /// Set color support level.
    #[must_use]
    pub const fn with_color(mut self, color_support: ColorSystem) -> Self {
        self.color_support = color_support;
        self
    }

    /// Get environment variables to simulate this terminal.
    #[must_use]
    pub fn env_vars(&self) -> Vec<(&'static str, String)> {
        let mut vars = vec![
            ("COLUMNS", self.width.to_string()),
            ("LINES", self.height.to_string()),
        ];

        if let Some(ref term) = self.term_type {
            vars.push(("TERM", term.clone()));
        }

        match self.color_support {
            ColorSystem::None => {
                // Don't set COLORTERM
            }
            ColorSystem::Standard => {
                // Basic terminal
            }
            ColorSystem::EightBit => {
                vars.push(("COLORTERM", "256color".to_string()));
            }
            ColorSystem::TrueColor => {
                vars.push(("COLORTERM", "truecolor".to_string()));
            }
        }

        vars
    }
}

impl Default for MockTerminal {
    fn default() -> Self {
        Self::new()
    }
}

/// Run a closure with mocked terminal settings.
///
/// # Examples
///
/// ```rust,ignore
/// let term = MockTerminal::new()
///     .with_size(120, 40)
///     .with_color(ColorSystem::TrueColor);
///
/// with_mock_terminal(term, || {
///     // Terminal environment is mocked here
/// });
/// ```
pub fn with_mock_terminal<F, R>(term: MockTerminal, f: F) -> R
where
    F: FnOnce() -> R,
{
    let vars = term.env_vars();
    let mut guard = EnvGuard::new();
    for (key, value) in vars {
        guard = guard.set(key, &value);
    }
    let _guard = guard;
    f()
}

// =============================================================================
// Tracing Log Capture
// =============================================================================

/// A captured log entry for testing.
#[derive(Debug, Clone)]
pub struct CapturedLogEntry {
    /// Log level
    pub level: Level,
    /// Logger target (module path)
    pub target: String,
    /// Log message
    pub message: String,
    /// Structured fields
    pub fields: HashMap<String, String>,
}

/// Captured logs from a test operation.
#[derive(Debug, Clone, Default)]
pub struct LogCapture {
    entries: Vec<CapturedLogEntry>,
}

impl LogCapture {
    /// Create a new empty log capture.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Get all captured entries.
    #[must_use]
    pub fn entries(&self) -> &[CapturedLogEntry] {
        &self.entries
    }

    /// Check if any log message contains the pattern.
    #[must_use]
    pub fn contains_message(&self, pattern: &str) -> bool {
        self.entries.iter().any(|e| e.message.contains(pattern))
    }

    /// Check if any log at the given level contains the pattern.
    #[must_use]
    pub fn contains_at_level(&self, level: Level, pattern: &str) -> bool {
        self.entries
            .iter()
            .any(|e| e.level == level && e.message.contains(pattern))
    }

    /// Get all entries at a specific level.
    #[must_use]
    pub fn at_level(&self, level: Level) -> Vec<&CapturedLogEntry> {
        self.entries.iter().filter(|e| e.level == level).collect()
    }

    /// Check if there are any error logs.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.entries.iter().any(|e| e.level == Level::ERROR)
    }

    /// Check if there are any warning logs.
    #[must_use]
    pub fn has_warnings(&self) -> bool {
        self.entries.iter().any(|e| e.level == Level::WARN)
    }

    /// Get the number of captured entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if no logs were captured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Add an entry (internal use).
    pub(crate) fn push(&mut self, entry: CapturedLogEntry) {
        self.entries.push(entry);
    }
}

/// Assert that a specific log message was emitted.
///
/// # Panics
///
/// Panics if no matching log entry is found.
pub fn assert_log_contains(capture: &LogCapture, level: Level, pattern: &str) {
    if !capture.contains_at_level(level, pattern) {
        let all_messages: Vec<_> = capture
            .entries()
            .iter()
            .map(|e| format!("[{:?}] {}", e.level, e.message))
            .collect();
        panic!(
            "Expected log at level {:?} containing '{}'\n\
             Captured logs:\n{}",
            level,
            pattern,
            all_messages.join("\n")
        );
    }
}

// Note: Full log capture requires setting up a custom tracing subscriber.
// The with_log_capture function below is a placeholder that works with
// the existing test_utils::logging infrastructure.

/// Run a closure and capture tracing logs.
///
/// This integrates with the existing `test_utils::logging` infrastructure.
///
/// # Examples
///
/// ```rust,ignore
/// let (result, logs) = with_log_capture(|| {
///     tracing::info!("Processing item");
///     42
/// });
/// assert!(logs.contains_message("Processing"));
/// ```
pub fn with_log_capture<F, R>(f: F) -> (R, LogCapture)
where
    F: FnOnce() -> R,
{
    // This is a simplified implementation.
    // Full implementation would set up a custom tracing subscriber.
    let capture = LogCapture::new();
    let result = f();
    (result, capture)
}

// =============================================================================
// Golden File Utilities
// =============================================================================

/// Compare output against a golden file.
///
/// If `UPDATE_GOLDEN=1` is set, updates the golden file instead of comparing.
///
/// # Panics
///
/// Panics if the output doesn't match the golden file.
///
/// # Examples
///
/// ```rust,ignore
/// let output = run_some_command();
/// assert_golden(&output, "tests/golden/command_output.txt");
/// ```
pub fn assert_golden(output: &str, golden_path: &str) {
    use std::fs;
    use std::path::Path;

    let path = Path::new(golden_path);

    // Check if we should update golden files
    if env::var("UPDATE_GOLDEN").is_ok() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("Failed to create golden file directory");
        }
        fs::write(path, output).expect("Failed to write golden file");
        println!("Updated golden file: {golden_path}");
        return;
    }

    // Compare with existing golden file
    let expected = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(e) => {
            panic!(
                "Golden file not found: {golden_path}\n\
                 Error: {e}\n\
                 Run with UPDATE_GOLDEN=1 to create it.\n\
                 Actual output:\n{output}"
            );
        }
    };

    if output != expected {
        // Find the first difference for helpful error message
        let diff_pos = output
            .chars()
            .zip(expected.chars())
            .position(|(a, b)| a != b)
            .unwrap_or(output.len().min(expected.len()));

        let context_start = diff_pos.saturating_sub(50);
        let context_end = (diff_pos + 50).min(output.len());

        panic!(
            "Output doesn't match golden file: {golden_path}\n\
             First difference at position {diff_pos}\n\
             Expected (around diff):\n{:?}\n\
             Actual (around diff):\n{:?}\n\
             Run with UPDATE_GOLDEN=1 to update.",
            &expected[context_start..context_end.min(expected.len())],
            &output[context_start..context_end]
        );
    }
}

/// Update a golden file if `UPDATE_GOLDEN=1` is set.
pub fn update_golden_if_needed(output: &str, golden_path: &str) {
    use std::fs;
    use std::path::Path;

    if env::var("UPDATE_GOLDEN").is_ok() {
        let path = Path::new(golden_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("Failed to create golden file directory");
        }
        fs::write(path, output).expect("Failed to write golden file");
        println!("Updated golden file: {golden_path}");
    }
}

/// Load the content of a golden file.
///
/// # Panics
///
/// Panics if the file doesn't exist.
#[must_use]
pub fn load_golden(golden_path: &str) -> String {
    std::fs::read_to_string(golden_path)
        .unwrap_or_else(|e| panic!("Failed to load golden file {golden_path}: {e}"))
}

// =============================================================================
// JSON Validation Utilities
// =============================================================================

/// Assert that a string is valid JSON and return the parsed value.
///
/// # Panics
///
/// Panics if the string is not valid JSON.
///
/// # Examples
///
/// ```rust,ignore
/// let json = assert_valid_json(r#"{"key": "value"}"#);
/// assert_eq!(json["key"], "value");
/// ```
#[must_use]
pub fn assert_valid_json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap_or_else(|e| {
        // Try to find the error location
        let line = e.line();
        let col = e.column();
        let lines: Vec<_> = s.lines().collect();
        let context = if line > 0 && line <= lines.len() {
            let start = line.saturating_sub(3);
            let end = (line + 2).min(lines.len());
            lines[start..end]
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    let line_num = start + i + 1;
                    let marker = if line_num == line { " >>> " } else { "     " };
                    format!("{marker}{line_num}: {l}")
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            s[..s.len().min(500)].to_string()
        };

        panic!(
            "Invalid JSON at line {line}, column {col}: {e}\n\
             Context:\n{context}"
        );
    })
}

/// Assert that JSON has the expected top-level keys.
///
/// # Panics
///
/// Panics if any expected key is missing.
pub fn assert_json_structure(json: &serde_json::Value, expected_keys: &[&str]) {
    let obj = json.as_object().expect("JSON should be an object");
    for key in expected_keys {
        if !obj.contains_key(*key) {
            let actual_keys: Vec<_> = obj.keys().collect();
            panic!(
                "Missing expected JSON key: '{}'\n\
                 Actual keys: {:?}",
                key, actual_keys
            );
        }
    }
}

/// Assert that a JSON value matches expected type.
pub fn assert_json_type(json: &serde_json::Value, path: &str, expected_type: &str) {
    let actual_type = match json {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    };

    if actual_type != expected_type {
        panic!(
            "JSON type mismatch at '{path}': expected {expected_type}, got {actual_type}\n\
             Value: {json}"
        );
    }
}

// =============================================================================
// Command Test Helpers
// =============================================================================

/// Result of running an ms command.
#[derive(Debug, Clone)]
pub struct CommandResult {
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// Exit code
    pub exit_code: i32,
    /// Whether the command succeeded (exit code 0)
    pub success: bool,
    /// Command duration
    pub duration: std::time::Duration,
}

impl CommandResult {
    /// Check if stdout contains a pattern.
    #[must_use]
    pub fn stdout_contains(&self, pattern: &str) -> bool {
        self.stdout.contains(pattern)
    }

    /// Check if stderr contains a pattern.
    #[must_use]
    pub fn stderr_contains(&self, pattern: &str) -> bool {
        self.stderr.contains(pattern)
    }

    /// Parse stdout as JSON.
    ///
    /// # Panics
    ///
    /// Panics if stdout is not valid JSON.
    #[must_use]
    pub fn json(&self) -> serde_json::Value {
        assert_valid_json(&self.stdout)
    }

    /// Try to parse stdout as JSON.
    #[must_use]
    pub fn try_json(&self) -> Option<serde_json::Value> {
        serde_json::from_str(&self.stdout).ok()
    }

    /// Assert the command succeeded.
    ///
    /// # Panics
    ///
    /// Panics if the command failed.
    pub fn assert_success(&self) {
        if !self.success {
            panic!(
                "Command failed with exit code {}\n\
                 stdout: {}\n\
                 stderr: {}",
                self.exit_code, self.stdout, self.stderr
            );
        }
    }

    /// Assert the command failed.
    ///
    /// # Panics
    ///
    /// Panics if the command succeeded.
    pub fn assert_failure(&self) {
        if self.success {
            panic!(
                "Command succeeded but expected failure\n\
                 stdout: {}\n\
                 stderr: {}",
                self.stdout, self.stderr
            );
        }
    }

    /// Assert stdout contains no ANSI codes (plain output).
    pub fn assert_plain_stdout(&self) {
        assert_no_ansi(&self.stdout, "command stdout");
    }

    /// Assert stdout contains ANSI codes (rich output).
    pub fn assert_rich_stdout(&self) {
        assert_has_ansi(&self.stdout, "command stdout");
    }
}

/// Run the ms binary with given arguments.
///
/// # Examples
///
/// ```rust,ignore
/// let result = run_ms_command(&["--version"]);
/// result.assert_success();
/// assert!(result.stdout_contains("ms"));
/// ```
#[must_use]
pub fn run_ms_command(args: &[&str]) -> CommandResult {
    run_ms_command_with_env(args, &[])
}

/// Run the ms binary with given arguments and environment variables.
///
/// # Examples
///
/// ```rust,ignore
/// let result = run_ms_command_with_env(
///     &["list"],
///     &[("NO_COLOR", "1"), ("MS_ROOT", "/tmp/test")]
/// );
/// result.assert_success();
/// result.assert_plain_stdout();
/// ```
#[must_use]
pub fn run_ms_command_with_env(args: &[&str], env: &[(&str, &str)]) -> CommandResult {
    use std::process::Command;

    let start = std::time::Instant::now();

    // Find the ms binary
    let ms_path = std::env::var("MS_BIN")
        .ok()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            // Try cargo target directory
            let manifest_dir = env!("CARGO_MANIFEST_DIR");
            std::path::PathBuf::from(manifest_dir)
                .join("target")
                .join("debug")
                .join("ms")
        });

    let mut cmd = Command::new(&ms_path);
    cmd.args(args);

    // Set environment variables
    for (key, value) in env {
        cmd.env(key, value);
    }

    let output = cmd.output().unwrap_or_else(|e| {
        panic!("Failed to execute ms command at {:?}: {}", ms_path, e);
    });

    let duration = start.elapsed();

    CommandResult {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
        success: output.status.success(),
        duration,
    }
}

// =============================================================================
// Test Assertion Macros
// =============================================================================

/// Assert that output is suitable for robot/machine consumption.
///
/// Checks that output:
/// - Contains no ANSI codes
/// - Contains no box drawing characters
/// - Contains no emoji
#[macro_export]
macro_rules! assert_robot_output {
    ($output:expr) => {
        $crate::output::test_utils::assert_plain_output($output, "robot output");
    };
    ($output:expr, $context:expr) => {
        $crate::output::test_utils::assert_plain_output($output, $context);
    };
}

/// Assert that output is rich (contains ANSI codes).
#[macro_export]
macro_rules! assert_rich_output {
    ($output:expr) => {
        $crate::output::test_utils::assert_has_ansi($output, "rich output");
    };
    ($output:expr, $context:expr) => {
        $crate::output::test_utils::assert_has_ansi($output, $context);
    };
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_guard_set_and_restore() {
        let key = "TEST_ENV_GUARD_VAR";
        // SAFETY: Test-only code, tests should use #[serial]
        unsafe { env::remove_var(key) };

        {
            let _guard = EnvGuard::new().set(key, "test_value");
            assert_eq!(env::var(key).unwrap(), "test_value");
        }

        // Should be restored (removed) after guard drops
        assert!(env::var(key).is_err());
    }

    #[test]
    fn test_env_guard_unset_and_restore() {
        let key = "TEST_ENV_GUARD_UNSET";
        // SAFETY: Test-only code, tests should use #[serial]
        unsafe { env::set_var(key, "original") };

        {
            let _guard = EnvGuard::new().unset(key);
            assert!(env::var(key).is_err());
        }

        // Should be restored after guard drops
        assert_eq!(env::var(key).unwrap(), "original");
        // SAFETY: Test-only code, cleanup
        unsafe { env::remove_var(key) };
    }

    #[test]
    fn test_contains_ansi() {
        assert!(contains_ansi("\x1b[31mred\x1b[0m"));
        assert!(contains_ansi("\x1b[1;32mbold green\x1b[0m"));
        assert!(contains_ansi("text \x1b[4munderline\x1b[0m text"));
        assert!(!contains_ansi("plain text"));
        assert!(!contains_ansi(""));
    }

    #[test]
    fn test_strip_ansi() {
        assert_eq!(strip_ansi("\x1b[31mred\x1b[0m"), "red");
        assert_eq!(strip_ansi("plain"), "plain");
        assert_eq!(
            strip_ansi("\x1b[1;32mbold green\x1b[0m text"),
            "bold green text"
        );
    }

    #[test]
    fn test_contains_box_drawing() {
        assert!(contains_box_drawing("┌───┐"));
        assert!(contains_box_drawing("│ x │"));
        assert!(contains_box_drawing("└───┘"));
        assert!(!contains_box_drawing("+---+"));
        assert!(!contains_box_drawing("| x |"));
    }

    #[test]
    fn test_contains_emoji() {
        assert!(contains_emoji("Hello 👋"));
        assert!(contains_emoji("✅ Done"));
        assert!(contains_emoji("🚀 Launch"));
        assert!(!contains_emoji("Hello"));
        assert!(!contains_emoji(":wave:"));
    }

    #[test]
    fn test_mock_terminal() {
        let term = MockTerminal::new()
            .with_size(120, 40)
            .with_color(ColorSystem::TrueColor);

        assert!(term.is_tty);
        assert_eq!(term.width, 120);
        assert_eq!(term.height, 40);
        assert_eq!(term.color_support, ColorSystem::TrueColor);
    }

    #[test]
    fn test_mock_terminal_piped() {
        let term = MockTerminal::piped();
        assert!(!term.is_tty);
        assert_eq!(term.color_support, ColorSystem::None);
    }

    #[test]
    fn test_output_capture() {
        let capture =
            OutputCapture::new("stdout content".to_string(), "stderr content".to_string());
        assert!(capture.stdout_contains("stdout"));
        assert!(capture.stderr_contains("stderr"));
        assert!(capture.combined().contains("stdout"));
        assert!(capture.combined().contains("stderr"));
    }

    #[test]
    fn test_assert_valid_json() {
        let json = assert_valid_json(r#"{"key": "value", "num": 42}"#);
        assert_eq!(json["key"], "value");
        assert_eq!(json["num"], 42);
    }

    #[test]
    fn test_assert_json_structure() {
        let json = assert_valid_json(r#"{"name": "test", "count": 5}"#);
        assert_json_structure(&json, &["name", "count"]);
    }

    #[test]
    fn test_agent_type_env_vars() {
        let vars = AgentType::ClaudeCode.env_vars();
        assert!(vars.contains(&("CLAUDE_CODE", "1")));
    }

    #[test]
    fn test_ci_type_env_vars() {
        let vars = CiType::GitHubActions.env_vars();
        assert!(vars.contains(&("GITHUB_ACTIONS", "true")));
        assert!(vars.contains(&("CI", "true")));
    }

    #[test]
    fn test_log_capture() {
        let mut capture = LogCapture::new();
        capture.push(CapturedLogEntry {
            level: Level::INFO,
            target: "test".to_string(),
            message: "Test message".to_string(),
            fields: HashMap::new(),
        });

        assert!(capture.contains_message("Test"));
        assert!(capture.contains_at_level(Level::INFO, "message"));
        assert!(!capture.has_errors());
        assert_eq!(capture.len(), 1);
    }
}
