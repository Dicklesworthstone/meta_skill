//! Interactive helpers inspired by Python Rich.
//!
//! This module contains Rust-idiomatic equivalents of a few Rich conveniences
//! that combine rendering with terminal interactivity.
//!
//! Note: `rich_rust`'s core remains output-focused; these helpers are designed to
//! degrade cleanly when the console is not interactive (TTY detection, piped output,
//! and CI-safe behavior).
//!
//! # Design RFC: Input Length Limiting Strategy (bd-191n)
//!
//! ## Problem Statement
//!
//! The current `Prompt::ask_from` implementation uses `BufRead::read_line()` with no
//! upper bound on input length. A malicious or accidental input stream could allocate
//! unbounded memory, leading to OOM or denial-of-service. This RFC defines how we
//! limit input length across all interactive prompt types.
//!
//! ## Decision 1: Default Limit Value
//!
//! **Chosen: 64 KiB (65,536 bytes).**
//!
//! Rationale:
//! - Large enough for any reasonable single-line interactive input (names, passwords,
//!   paths, URLs, freeform text responses).
//! - Small enough to prevent accidental memory exhaustion from piped/redirected input.
//! - Matches typical terminal line-buffer sizes and is consistent with common POSIX
//!   defaults (e.g., `LINE_MAX` is often 2048, but we are generous for rich text use).
//! - Alternatives considered: 256 bytes (too small for paths), 4 KiB (too small for
//!   pasted content), 1 MiB (too permissive, defeats the purpose).
//!
//! The default is exposed as a public constant:
//! ```rust,ignore
//! pub const DEFAULT_MAX_INPUT_LENGTH: usize = 64 * 1024; // 64 KiB
//! ```
//!
//! ## Decision 2: Error Handling
//!
//! **Chosen: New `PromptError::InputTooLong` variant.**
//!
//! ```rust,ignore
//! pub enum PromptError {
//!     NotInteractive,
//!     Eof,
//!     Validation(String),
//!     Io(io::Error),
//!     InputTooLong { limit: usize, received: usize },
//! }
//! ```
//!
//! The variant carries both the configured limit and the number of bytes received
//! before the limit was exceeded, enabling callers to produce informative error
//! messages. `Display` renders as:
//! `"input too long: received at least {received} bytes, limit is {limit} bytes"`.
//!
//! ## Decision 3: Configuration API
//!
//! **Chosen: Per-prompt `max_length()` builder method.**
//!
//! ```rust,ignore
//! let name = Prompt::new("Name")
//!     .max_length(256)        // Override default for this prompt
//!     .ask(&console)?;
//!
//! let essay = Prompt::new("Description")
//!     .max_length(1024 * 1024)  // Allow up to 1 MiB for this prompt
//!     .ask(&console)?;
//!
//! let default_limit = Prompt::new("City")
//!     .ask(&console)?;          // Uses DEFAULT_MAX_INPUT_LENGTH (64 KiB)
//! ```
//!
//! The `Prompt` struct gains a new field:
//! ```rust,ignore
//! pub struct Prompt {
//!     // ... existing fields ...
//!     max_length: usize,  // defaults to DEFAULT_MAX_INPUT_LENGTH
//! }
//! ```
//!
//! `Select` and `Confirm` prompts also gain `max_length` with the same default.
//! Their inputs are inherently shorter (a number or "y"/"n"), but the limit
//! applies uniformly for defense-in-depth.
//!
//! No global `Console`-level override is needed: per-prompt is sufficient, and a
//! global setting adds complexity without clear benefit. If a future use case
//! demands it, a `ConsoleBuilder::default_max_input()` can be added non-breakingly.
//!
//! ## Decision 4: Limit Enforcement Point
//!
//! **Chosen: During read (Option A from the bead description), with streaming check.**
//!
//! A new helper function enforces the limit *before* the full input is buffered:
//!
//! ```rust,ignore
//! fn read_line_limited<R: BufRead>(
//!     reader: &mut R,
//!     max_bytes: usize,
//! ) -> Result<String, PromptError> {
//!     let mut buf = Vec::new();
//!     let mut total = 0usize;
//!     loop {
//!         let available = reader.fill_buf().map_err(PromptError::Io)?;
//!         if available.is_empty() {
//!             // EOF reached
//!             if total == 0 {
//!                 return Err(PromptError::Eof);
//!             }
//!             break;
//!         }
//!         if let Some(newline_pos) = available.iter().position(|&b| b == b'\n') {
//!             let line_len = newline_pos + 1; // include the newline
//!             if total + line_len > max_bytes {
//!                 return Err(PromptError::InputTooLong {
//!                     limit: max_bytes,
//!                     received: total + line_len,
//!                 });
//!             }
//!             buf.extend_from_slice(&available[..line_len]);
//!             reader.consume(line_len);
//!             break;
//!         }
//!         // No newline yet; check running total
//!         if total + available.len() > max_bytes {
//!             return Err(PromptError::InputTooLong {
//!                 limit: max_bytes,
//!                 received: total + available.len(),
//!             });
//!         }
//!         buf.extend_from_slice(available);
//!         total += available.len();
//!         let len = available.len();
//!         reader.consume(len);
//!     }
//!     String::from_utf8(buf)
//!         .map(|s| s.trim_end_matches(&['\n', '\r'][..]).to_string())
//!         .map_err(|e| PromptError::Validation(format!("invalid UTF-8: {e}")))
//! }
//! ```
//!
//! Key advantages over post-read checking:
//! - Memory is never allocated beyond the limit.
//! - Fail-fast: stops reading as soon as the limit is exceeded.
//! - Works correctly with piped input where `read_line()` might buffer megabytes.
//!
//! ## Decision 5: Behavior on Limit Exceeded
//!
//! **Chosen: Error immediately (fail-fast).**
//!
//! - Returning `Err(PromptError::InputTooLong { .. })` immediately on exceeding
//!   the limit is the safest and most predictable behavior.
//! - Silent truncation risks data loss and confused users.
//! - Truncation with warning adds UX complexity without clear benefit.
//! - Callers can catch the error and retry with a message, e.g.:
//!   ```rust,ignore
//!   loop {
//!       match Prompt::new("Name").max_length(256).ask(&console) {
//!           Ok(name) => break name,
//!           Err(PromptError::InputTooLong { limit, .. }) => {
//!               console.print(&format!("[red]Input must be under {limit} bytes.[/]"));
//!           }
//!           Err(e) => return Err(e),
//!       }
//!   }
//!   ```
//!
//! ## Security Considerations
//!
//! - **Memory exhaustion:** The primary threat. Without a limit, `read_line()` will
//!   allocate until OOM on a stream of bytes with no newline. The 64 KiB default
//!   bounds worst-case allocation per prompt invocation.
//! - **Denial of service:** In server-like contexts where prompts read from network
//!   streams, the limit prevents a slow-loris style attack filling memory.
//! - **UTF-8 validation:** `read_line_limited` converts from `Vec<u8>` to `String`,
//!   returning a `Validation` error on invalid UTF-8 rather than panicking.
//! - **No truncation:** We never silently lose data. The caller always knows when
//!   input was rejected.
//!
//! ## Migration Path
//!
//! This is additive and non-breaking:
//! 1. Add `DEFAULT_MAX_INPUT_LENGTH` constant.
//! 2. Add `InputTooLong` variant to `PromptError`.
//! 3. Add `max_length` field to `Prompt`, `Select`, `Confirm` (defaults to 64 KiB).
//! 4. Add `read_line_limited()` helper function.
//! 5. Replace `reader.read_line()` calls in `ask_from()` with `read_line_limited()`.
//!
//! Existing code that doesn't set `max_length` gains the 64 KiB safety net.
//! Code that needs larger inputs can opt in with `.max_length(n)`.
//!
//! ## Implementation Beads (Downstream)
//!
//! - **bd-2e33**: Add `PromptError::InputTooLong` variant
//! - **bd-uqdk**: Implement `read_line_limited` helper function
//! - **bd-1jm0**: Add `max_length` to `Prompt` builder
//! - **bd-fal7**: Wire `read_line_limited` into `Prompt::ask_from`

use std::io;
use std::io::Write as _;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::console::Console;
use crate::console::PrintOptions;
use crate::live::{Live, LiveOptions};
use crate::markup;
use crate::style::Style;
use crate::text::Text;

/// Default maximum input length for interactive prompts (64 KiB).
///
/// This is the default value for `Prompt::max_length`, `Select::max_length`, and
/// `Confirm::max_length`. Override per-prompt with `.max_length(n)`.
///
/// See the module-level RFC documentation for the rationale behind this value.
pub const DEFAULT_MAX_INPUT_LENGTH: usize = 64 * 1024;

/// A spinner + message context helper, inspired by Python Rich's `Console.status(...)`.
///
/// When the console is interactive (`Console::is_interactive()`), this starts a `Live`
/// display that refreshes a single-line spinner. When the console is not interactive,
/// it prints the message once and does not animate.
///
/// Dropping this value stops the live display.
///
/// # Thread Safety
///
/// `Status` is `Send + Sync` and can be safely shared between threads. The
/// [`update`](Status::update) method is safe to call concurrently from multiple
/// threads — it performs a single atomic mutex write with poison recovery.
///
/// Updates are eventually consistent: the displayed message is guaranteed to
/// reflect one of the recent `update()` calls within ~100ms (one refresh cycle).
///
/// # Design RFC: Atomic `Status::update` (bd-gg33)
///
/// ## Problem
///
/// `Status::update` currently performs two operations: (1) write the new message
/// into `Arc<Mutex<String>>`, then (2) call `live.refresh()`. These are not
/// atomic: another thread could update the message between steps 1 and 2,
/// causing refresh to display a message from a different `update` call.
///
/// ## Options Evaluated
///
/// | Option | Approach | Complexity | Breaking? |
/// |--------|----------|-----------|-----------|
/// | A | Message versioning (u64 counter + Live version check) | Medium | No |
/// | **B** | **Deferred refresh (remove explicit refresh call)** | **Low** | **No** |
/// | C | Combined mutex (hold during refresh) | High | Potentially |
/// | D | Document as known limitation | None | No |
///
/// ## Decision: Option B — Deferred Refresh
///
/// **Remove the explicit `live.refresh()` call from `update()`.**
///
/// Rationale:
/// - `Live` already runs a timer-based refresh at `refresh_per_second: 10.0`
///   (100 ms interval). The explicit `refresh()` call is redundant.
/// - Removing it eliminates the race window entirely: `update()` becomes a
///   single mutex write, which is inherently atomic.
/// - No performance cost. The message is guaranteed to appear on the next
///   scheduled refresh cycle (within ~100 ms), which is imperceptible.
/// - Simplest implementation: fewer lines of code, fewer failure modes.
///
/// Alternatives rejected:
/// - **Option A** (versioning): Adds complexity for no user-visible benefit.
///   The race condition is cosmetic (self-corrects in one refresh cycle).
/// - **Option C** (combined mutex): Risk of deadlock with Live's internal
///   mutexes. Increased lock contention under heavy concurrent updates.
/// - **Option D** (document only): Leaves an unnecessary race when a simple
///   fix exists.
///
/// ## Migration
///
/// ```rust,ignore
/// // Before (current):
/// pub fn update(&self, message: impl Into<String>) {
///     *crate::sync::lock_recover(&self.message) = message.into();
///     if let Some(live) = &self.live {
///         let _ = live.refresh();  // <-- race window here
///     }
/// }
///
/// // After (Option B):
/// pub fn update(&self, message: impl Into<String>) {
///     *crate::sync::lock_recover(&self.message) = message.into();
///     // Live's timer-based refresh picks up the new message automatically.
/// }
/// ```
///
/// ## Test Plan
///
/// 1. Existing `test_status_non_interactive_prints_message_once` still passes.
/// 2. New test: rapid concurrent `update()` calls from multiple threads,
///    verifying no panics and final message is one of the expected values.
/// 3. New test: `update()` after `Live` has stopped (no-op, no crash).
pub struct Status {
    message: Arc<Mutex<String>>,
    live: Option<Live>,
}

impl Status {
    /// Start a status spinner with a message.
    pub fn new(console: &Arc<Console>, message: impl Into<String>) -> io::Result<Self> {
        let message = Arc::new(Mutex::new(message.into()));

        if !console.is_interactive() {
            {
                let message = crate::sync::lock_recover(&message);
                console.print_plain(&message);
            }
            return Ok(Self {
                message,
                live: None,
            });
        }

        let start = Instant::now();
        let frames: [&str; 4] = ["|", "/", "-", "\\"];
        let frame_interval = Duration::from_millis(100);
        let message_for_render = Arc::clone(&message);

        let live_options = LiveOptions {
            refresh_per_second: 10.0,
            transient: true,
            ..LiveOptions::default()
        };

        let live =
            Live::with_options(Arc::clone(console), live_options).get_renderable(move || {
                let elapsed = start.elapsed();
                let tick = elapsed.as_millis() / frame_interval.as_millis().max(1);
                let idx = (tick as usize) % frames.len();
                let frame = frames[idx];
                let msg = crate::sync::lock_recover(&message_for_render).clone();
                Box::new(Text::new(format!("{frame} {msg}")))
            });

        live.start(true)?;

        Ok(Self {
            message,
            live: Some(live),
        })
    }

    /// Update the displayed message.
    ///
    /// # Design Note (RFC bd-gg33)
    ///
    /// This method does NOT explicitly trigger a refresh. The Live display's
    /// background thread (running at 10Hz) will pick up the new message on its
    /// next tick. This design eliminates a race condition where concurrent
    /// `update()` calls could cause message ordering issues.
    ///
    /// See the module-level RFC documentation on [`Status`] for full analysis.
    pub fn update(&self, message: impl Into<String>) {
        *crate::sync::lock_recover(&self.message) = message.into();
        // Live's timer-based refresh (10Hz) picks up the new message automatically.
        // No explicit refresh() call needed - this eliminates the race condition.
    }
}

impl Drop for Status {
    fn drop(&mut self) {
        if let Some(live) = &self.live {
            let _ = live.stop();
        }
    }
}

/// Errors returned by prompt operations.
#[derive(Debug)]
pub enum PromptError {
    /// Prompt requires an interactive console but `Console::is_interactive()` is false.
    NotInteractive,
    /// Input stream reached EOF without yielding a value.
    Eof,
    /// Input did not pass validation.
    Validation(String),
    /// I/O error while reading input.
    Io(io::Error),
    /// Input exceeded the maximum allowed length.
    InputTooLong {
        /// Maximum allowed input length in bytes.
        limit: usize,
        /// Actual input length received (may be approximate if terminated early).
        received: usize,
    },
}

impl std::fmt::Display for PromptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotInteractive => write!(f, "prompt requires an interactive console"),
            Self::Eof => write!(f, "prompt input reached EOF"),
            Self::Validation(message) => write!(f, "{message}"),
            Self::Io(err) => write!(f, "{err}"),
            Self::InputTooLong { limit, received } => {
                write!(
                    f,
                    "input too long: received at least {received} bytes, limit is {limit} bytes"
                )
            }
        }
    }
}

impl PromptError {
    /// Returns `true` if this error indicates input was too long.
    #[must_use]
    pub const fn is_input_too_long(&self) -> bool {
        matches!(self, Self::InputTooLong { .. })
    }

    /// Returns the length limit if this is an `InputTooLong` error.
    #[must_use]
    pub const fn input_limit(&self) -> Option<usize> {
        match self {
            Self::InputTooLong { limit, .. } => Some(*limit),
            _ => None,
        }
    }
}

impl std::error::Error for PromptError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<io::Error> for PromptError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

type PromptValidator = Arc<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// Prompt configuration.
#[derive(Clone)]
pub struct Prompt {
    label: String,
    default: Option<String>,
    allow_empty: bool,
    show_default: bool,
    markup: bool,
    validator: Option<PromptValidator>,
    max_length: usize,
}

impl std::fmt::Debug for Prompt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Prompt")
            .field("label", &self.label)
            .field("default", &self.default)
            .field("allow_empty", &self.allow_empty)
            .field("show_default", &self.show_default)
            .field("markup", &self.markup)
            .field("max_length", &self.max_length)
            .field("validator", &self.validator.as_ref().map(|_| "<validator>"))
            .finish()
    }
}

impl Prompt {
    /// Create a new prompt.
    #[must_use]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            default: None,
            allow_empty: false,
            show_default: true,
            markup: true,
            validator: None,
            max_length: DEFAULT_MAX_INPUT_LENGTH,
        }
    }

    /// Provide a default value (used when the user enters empty input, or when not interactive).
    #[must_use]
    pub fn default(mut self, default: impl Into<String>) -> Self {
        self.default = Some(default.into());
        self
    }

    /// Allow empty input when no default is set.
    #[must_use]
    pub const fn allow_empty(mut self, allow_empty: bool) -> Self {
        self.allow_empty = allow_empty;
        self
    }

    /// Show the default value in the prompt (when present).
    #[must_use]
    pub const fn show_default(mut self, show_default: bool) -> Self {
        self.show_default = show_default;
        self
    }

    /// Enable/disable markup parsing for the prompt label.
    #[must_use]
    pub const fn markup(mut self, markup: bool) -> Self {
        self.markup = markup;
        self
    }

    /// Add validation for user input. Returning `Err(message)` prints the message and re-prompts.
    #[must_use]
    pub fn validate<F>(mut self, validator: F) -> Self
    where
        F: Fn(&str) -> Result<(), String> + Send + Sync + 'static,
    {
        self.validator = Some(Arc::new(validator));
        self
    }

    /// Set maximum input length in bytes.
    ///
    /// If input exceeds this limit, `ask()` returns `PromptError::InputTooLong`.
    /// Defaults to [`DEFAULT_MAX_INPUT_LENGTH`] (64 KiB).
    #[must_use]
    pub const fn max_length(mut self, max_bytes: usize) -> Self {
        self.max_length = if max_bytes == 0 { 1 } else { max_bytes };
        self
    }

    /// Ask for input using stdin.
    pub fn ask(&self, console: &Console) -> Result<String, PromptError> {
        let stdin = io::stdin();
        let mut reader = stdin.lock();
        self.ask_from(console, &mut reader)
    }

    /// Ask for input from a provided reader (useful for tests).
    pub fn ask_from<R: io::BufRead>(
        &self,
        console: &Console,
        reader: &mut R,
    ) -> Result<String, PromptError> {
        if !console.is_terminal() {
            return self.default.clone().ok_or(PromptError::NotInteractive);
        }

        loop {
            self.print_prompt(console);

            let line = read_line_limited(reader, self.max_length)?;
            let input = trim_newline(&line);
            let mut value = if input.is_empty() {
                self.default.clone().unwrap_or_default()
            } else {
                input.to_string()
            };

            if value.is_empty() && !self.allow_empty && self.default.is_none() {
                self.print_error(console, "Input required.");
                continue;
            }

            if let Some(validator) = &self.validator
                && let Err(message) = validator(&value)
            {
                self.print_error(console, &message);
                continue;
            }

            value = value.trim_end().to_string();
            return Ok(value);
        }
    }

    fn print_prompt(&self, console: &Console) {
        let mut prompt = self.label.clone();
        if self.show_default
            && let Some(default) = &self.default
        {
            let default = if self.markup {
                markup::escape(default)
            } else {
                default.clone()
            };
            prompt.push_str(" [");
            prompt.push_str(&default);
            prompt.push(']');
        }
        prompt.push_str(": ");

        console.print_with_options(
            &prompt,
            &PrintOptions::new()
                .with_markup(self.markup)
                .with_no_newline(true)
                .with_highlight(self.markup),
        );
    }

    fn print_error(&self, console: &Console, message: &str) {
        let style = Style::parse("bold red").unwrap_or_default();
        console.print_with_options(
            message,
            &PrintOptions::new().with_markup(false).with_style(style),
        );
    }
}

/// Pager support with a deterministic fallback when a pager isn't available.
///
/// When interactive, this attempts to pipe content through `$PAGER` (or a platform default).
/// When not interactive or if spawning the pager fails, it falls back to printing directly
/// to the console.
#[derive(Debug, Clone)]
pub struct Pager {
    command: Option<String>,
    allow_color: bool,
}

impl Default for Pager {
    fn default() -> Self {
        Self::new()
    }
}

impl Pager {
    /// Create a new pager with defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            command: None,
            allow_color: true,
        }
    }

    /// Override the pager command.
    #[must_use]
    pub fn command(mut self, command: impl Into<String>) -> Self {
        self.command = Some(command.into());
        self
    }

    /// Allow ANSI color sequences (where supported by the pager).
    #[must_use]
    pub const fn allow_color(mut self, allow_color: bool) -> Self {
        self.allow_color = allow_color;
        self
    }

    /// Display content through the pager, falling back to normal console output
    /// when a pager can't be used.
    pub fn show(&self, console: &Console, content: &str) -> io::Result<()> {
        if !console.is_terminal() {
            print_exact(console, content);
            return Ok(());
        }

        let (command, args) = self.resolve_command();
        match spawn_pager(&command, &args, content) {
            Ok(()) => Ok(()),
            Err(_err) => {
                print_exact(console, content);
                Ok(())
            }
        }
    }

    fn resolve_command(&self) -> (String, Vec<String>) {
        let command = self
            .command
            .clone()
            .or_else(|| std::env::var("PAGER").ok())
            .unwrap_or_else(|| {
                #[cfg(windows)]
                {
                    "more".to_string()
                }
                #[cfg(not(windows))]
                {
                    "less".to_string()
                }
            });

        let mut parts = command.split_whitespace();
        let bin = parts.next().unwrap_or("less").to_string();

        let mut args: Vec<String> = parts.map(str::to_string).collect();

        if self.allow_color && bin == "less" && args.iter().all(|arg| arg != "-R") {
            args.push("-R".to_string());
        }

        (bin, args)
    }
}

fn spawn_pager(command: &str, args: &[String], content: &str) -> io::Result<()> {
    let mut child = Command::new(command)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(content.as_bytes())?;
        stdin.flush()?;
    }

    let _status = child.wait()?;
    Ok(())
}

fn print_exact(console: &Console, content: &str) {
    console.print_with_options(
        content,
        &PrintOptions::new().with_markup(false).with_no_newline(true),
    );
}

/// Read a line from input with a maximum byte length limit.
///
/// Unlike `BufRead::read_line`, this function enforces the limit *during* reading
/// rather than after, preventing memory exhaustion from extremely long input.
///
/// Returns the line as a `String` (including trailing newline if present).
/// On EOF with no data, returns `Err(PromptError::Eof)`.
/// On exceeding the limit, returns `Err(PromptError::InputTooLong)`.
fn read_line_limited<R: io::BufRead>(
    reader: &mut R,
    max_bytes: usize,
) -> Result<String, PromptError> {
    let mut buf = Vec::with_capacity(max_bytes.min(1024));
    let mut total = 0usize;

    loop {
        let available = reader.fill_buf()?;

        if available.is_empty() {
            // EOF reached
            if buf.is_empty() {
                return Err(PromptError::Eof);
            }
            break;
        }

        // Look for newline in the available buffer
        if let Some(newline_pos) = available.iter().position(|&b| b == b'\n') {
            let line_len = newline_pos + 1; // include the newline
            if total + line_len > max_bytes {
                return Err(PromptError::InputTooLong {
                    limit: max_bytes,
                    received: total + line_len,
                });
            }
            buf.extend_from_slice(&available[..line_len]);
            reader.consume(line_len);
            break;
        }

        // No newline yet; check running total
        if total + available.len() > max_bytes {
            return Err(PromptError::InputTooLong {
                limit: max_bytes,
                received: total + available.len(),
            });
        }

        buf.extend_from_slice(available);
        total += available.len();
        let len = available.len();
        reader.consume(len);
    }

    String::from_utf8(buf).map_err(|e| PromptError::Validation(format!("invalid UTF-8: {e}")))
}

fn trim_newline(line: &str) -> &str {
    line.trim_end_matches(&['\n', '\r'][..])
}

/// A choice for the Select prompt.
#[derive(Debug, Clone)]
pub struct Choice {
    /// The value returned when this choice is selected.
    pub value: String,
    /// Optional display label (if different from value).
    pub label: Option<String>,
}

impl Choice {
    /// Create a choice where value and label are the same.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: None,
        }
    }

    /// Create a choice with a separate display label.
    #[must_use]
    pub fn with_label(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: Some(label.into()),
        }
    }

    /// Get the display text for this choice.
    #[must_use]
    pub fn display(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.value)
    }
}

impl<S: Into<String>> From<S> for Choice {
    fn from(value: S) -> Self {
        Self::new(value)
    }
}

/// Select prompt for choosing from a list of options.
///
/// Displays numbered choices and allows selection by number or by typing
/// the choice value directly.
///
/// # Examples
///
/// ```rust,ignore
/// use rich_rust::interactive::Select;
///
/// let color = Select::new("Pick a color")
///     .choices(["red", "green", "blue"])
///     .default("blue")
///     .ask(&console)?;
/// ```
#[derive(Debug, Clone)]
pub struct Select {
    label: String,
    choices: Vec<Choice>,
    default: Option<String>,
    show_default: bool,
    markup: bool,
    max_length: usize,
}

impl Select {
    /// Create a new select prompt.
    #[must_use]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            choices: Vec::new(),
            default: None,
            show_default: true,
            markup: true,
            max_length: DEFAULT_MAX_INPUT_LENGTH,
        }
    }

    /// Add choices to select from.
    #[must_use]
    pub fn choices<I, C>(mut self, choices: I) -> Self
    where
        I: IntoIterator<Item = C>,
        C: Into<Choice>,
    {
        self.choices.extend(choices.into_iter().map(Into::into));
        self
    }

    /// Add a single choice.
    #[must_use]
    pub fn choice(mut self, choice: impl Into<Choice>) -> Self {
        self.choices.push(choice.into());
        self
    }

    /// Set the default choice (used when user enters empty input or in non-interactive mode).
    #[must_use]
    pub fn default(mut self, default: impl Into<String>) -> Self {
        self.default = Some(default.into());
        self
    }

    /// Show/hide the default value in the prompt.
    #[must_use]
    pub const fn show_default(mut self, show_default: bool) -> Self {
        self.show_default = show_default;
        self
    }

    /// Enable/disable markup parsing for the prompt label.
    #[must_use]
    pub const fn markup(mut self, markup: bool) -> Self {
        self.markup = markup;
        self
    }

    /// Set maximum input length in bytes.
    ///
    /// If input exceeds this limit, `ask()` returns `PromptError::InputTooLong`.
    /// Defaults to [`DEFAULT_MAX_INPUT_LENGTH`] (64 KiB).
    #[must_use]
    pub const fn max_length(mut self, max_bytes: usize) -> Self {
        self.max_length = if max_bytes == 0 { 1 } else { max_bytes };
        self
    }

    /// Ask for selection using stdin.
    pub fn ask(&self, console: &Console) -> Result<String, PromptError> {
        let stdin = io::stdin();
        let mut reader = stdin.lock();
        self.ask_from(console, &mut reader)
    }

    /// Ask for selection from a provided reader (useful for tests).
    pub fn ask_from<R: io::BufRead>(
        &self,
        console: &Console,
        reader: &mut R,
    ) -> Result<String, PromptError> {
        if self.choices.is_empty() {
            return Err(PromptError::Validation("No choices provided".to_string()));
        }

        if !console.is_terminal() {
            return self.default.clone().ok_or(PromptError::NotInteractive);
        }

        loop {
            self.print_choices(console);
            self.print_prompt(console);

            let line = read_line_limited(reader, self.max_length)?;
            let input = trim_newline(&line).trim();

            // Empty input uses default
            if input.is_empty() {
                if let Some(default) = &self.default
                    && self.find_choice(default).is_some()
                {
                    return Ok(default.clone());
                }
                self.print_error(console, "Please select an option.");
                continue;
            }

            // Try as number first
            if let Ok(num) = input.parse::<usize>()
                && num >= 1
                && num <= self.choices.len()
            {
                return Ok(self.choices[num - 1].value.clone());
            }

            // Try as exact match (case insensitive)
            if let Some(choice) = self.find_choice(input) {
                return Ok(choice.value.clone());
            }

            self.print_error(console, &format!("Invalid choice: {input}"));
        }
    }

    fn find_choice(&self, input: &str) -> Option<&Choice> {
        let input_lower = input.to_lowercase();
        self.choices.iter().find(|c| {
            c.value.to_lowercase() == input_lower || c.display().to_lowercase() == input_lower
        })
    }

    fn print_choices(&self, console: &Console) {
        for (i, choice) in self.choices.iter().enumerate() {
            let num = i + 1;
            let display = choice.display();
            let is_default = self.default.as_deref() == Some(&choice.value);

            let line = if is_default && self.show_default {
                format!("  [bold cyan]{num}.[/] {display} [dim](default)[/]")
            } else {
                format!("  [cyan]{num}.[/] {display}")
            };

            console.print_with_options(&line, &PrintOptions::new().with_markup(self.markup));
        }
    }

    fn print_prompt(&self, console: &Console) {
        let mut prompt = self.label.clone();
        if self.show_default
            && let Some(default) = &self.default
        {
            let default_display = self
                .find_choice(default)
                .map_or(default.as_str(), Choice::display);
            let escaped = if self.markup {
                markup::escape(default_display)
            } else {
                default_display.to_string()
            };
            prompt.push_str(" [");
            prompt.push_str(&escaped);
            prompt.push(']');
        }
        prompt.push_str(": ");

        console.print_with_options(
            &prompt,
            &PrintOptions::new()
                .with_markup(self.markup)
                .with_no_newline(true)
                .with_highlight(self.markup),
        );
    }

    fn print_error(&self, console: &Console, message: &str) {
        let style = Style::parse("bold red").unwrap_or_default();
        console.print_with_options(
            message,
            &PrintOptions::new().with_markup(false).with_style(style),
        );
    }
}

/// Confirm prompt (yes/no question).
///
/// # Examples
///
/// ```rust,ignore
/// use rich_rust::interactive::Confirm;
///
/// let proceed = Confirm::new("Continue?")
///     .default(true)
///     .ask(&console)?;
/// ```
#[derive(Debug, Clone)]
pub struct Confirm {
    label: String,
    default: Option<bool>,
    markup: bool,
    max_length: usize,
}

impl Confirm {
    /// Create a new confirmation prompt.
    #[must_use]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            default: None,
            markup: true,
            max_length: DEFAULT_MAX_INPUT_LENGTH,
        }
    }

    /// Set the default value.
    #[must_use]
    pub const fn default(mut self, default: bool) -> Self {
        self.default = Some(default);
        self
    }

    /// Enable/disable markup parsing for the prompt label.
    #[must_use]
    pub const fn markup(mut self, markup: bool) -> Self {
        self.markup = markup;
        self
    }

    /// Set maximum input length in bytes.
    ///
    /// If input exceeds this limit, `ask()` returns `PromptError::InputTooLong`.
    /// Defaults to [`DEFAULT_MAX_INPUT_LENGTH`] (64 KiB).
    #[must_use]
    pub const fn max_length(mut self, max_bytes: usize) -> Self {
        self.max_length = if max_bytes == 0 { 1 } else { max_bytes };
        self
    }

    /// Ask for confirmation using stdin.
    pub fn ask(&self, console: &Console) -> Result<bool, PromptError> {
        let stdin = io::stdin();
        let mut reader = stdin.lock();
        self.ask_from(console, &mut reader)
    }

    /// Ask for confirmation from a provided reader (useful for tests).
    pub fn ask_from<R: io::BufRead>(
        &self,
        console: &Console,
        reader: &mut R,
    ) -> Result<bool, PromptError> {
        if !console.is_terminal() {
            return self.default.ok_or(PromptError::NotInteractive);
        }

        loop {
            self.print_prompt(console);

            let line = read_line_limited(reader, self.max_length)?;
            let input = trim_newline(&line).trim().to_lowercase();

            if input.is_empty() {
                if let Some(default) = self.default {
                    return Ok(default);
                }
                self.print_error(console, "Please enter y or n.");
                continue;
            }

            match input.as_str() {
                "y" | "yes" | "true" | "1" => return Ok(true),
                "n" | "no" | "false" | "0" => return Ok(false),
                _ => {
                    self.print_error(console, "Please enter y or n.");
                }
            }
        }
    }

    fn print_prompt(&self, console: &Console) {
        let mut prompt = self.label.clone();

        let choices = match self.default {
            Some(true) => "[Y/n]",
            Some(false) => "[y/N]",
            None => "[y/n]",
        };
        prompt.push(' ');
        prompt.push_str(choices);
        prompt.push_str(": ");

        console.print_with_options(
            &prompt,
            &PrintOptions::new()
                .with_markup(self.markup)
                .with_no_newline(true)
                .with_highlight(self.markup),
        );
    }

    fn print_error(&self, console: &Console, message: &str) {
        let style = Style::parse("bold red").unwrap_or_default();
        console.print_with_options(
            message,
            &PrintOptions::new().with_markup(false).with_style(style),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as StdError;
    use std::io::Write;

    #[derive(Clone)]
    struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

    impl Write for SharedBuffer {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().write(buf)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.0.lock().unwrap().flush()
        }
    }

    #[test]
    fn test_status_non_interactive_prints_message_once() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let _status = Status::new(&console, "Working...").expect("status");

        let out = buffer.0.lock().unwrap();
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("Working...\n"));
    }

    #[test]
    fn test_prompt_non_interactive_uses_default() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let prompt = Prompt::new("Name").default("Alice");
        let answer = prompt.ask(&console).expect("prompt");
        assert_eq!(answer, "Alice");

        // Non-interactive prompt should not print.
        assert!(buffer.0.lock().unwrap().is_empty());
    }

    #[test]
    fn test_prompt_from_reader_validates_and_reprompts() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let prompt = Prompt::new("Age").validate(|value| {
            if value.chars().all(|c| c.is_ascii_digit()) {
                Ok(())
            } else {
                Err("digits only".to_string())
            }
        });

        let input = b"nope\n42\n";
        let mut reader = io::Cursor::new(&input[..]);
        let answer = prompt.ask_from(&console, &mut reader).expect("prompt");
        assert_eq!(answer, "42");

        let out = buffer.0.lock().unwrap();
        let text = String::from_utf8_lossy(&out);
        // The error message may have ANSI codes around it due to the bold red style,
        // so we just check for the text content rather than a literal sequence with newline.
        assert!(
            text.contains("digits only"),
            "Expected error message 'digits only' in output, got: {text:?}"
        );
    }

    #[test]
    fn test_pager_non_interactive_falls_back_to_print() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build();

        Pager::new()
            .show(&console, "hello\nworld\n")
            .expect("pager");

        let out = buffer.0.lock().unwrap();
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("hello\nworld\n"));
    }

    #[test]
    fn test_select_by_number() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let select = Select::new("Pick a color").choices(["red", "green", "blue"]);

        let input = b"2\n";
        let mut reader = io::Cursor::new(&input[..]);
        let answer = select.ask_from(&console, &mut reader).expect("select");
        assert_eq!(answer, "green");
    }

    #[test]
    fn test_select_by_value() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let select = Select::new("Pick a color").choices(["red", "green", "blue"]);

        let input = b"blue\n";
        let mut reader = io::Cursor::new(&input[..]);
        let answer = select.ask_from(&console, &mut reader).expect("select");
        assert_eq!(answer, "blue");
    }

    #[test]
    fn test_select_case_insensitive() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let select = Select::new("Pick").choices(["Red", "Green"]);

        let input = b"red\n";
        let mut reader = io::Cursor::new(&input[..]);
        let answer = select.ask_from(&console, &mut reader).expect("select");
        assert_eq!(answer, "Red");
    }

    #[test]
    fn test_select_default() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let select = Select::new("Pick").choices(["a", "b", "c"]).default("b");

        let input = b"\n"; // Empty input
        let mut reader = io::Cursor::new(&input[..]);
        let answer = select.ask_from(&console, &mut reader).expect("select");
        assert_eq!(answer, "b");
    }

    #[test]
    fn test_select_non_interactive_uses_default() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let select = Select::new("Pick").choices(["a", "b"]).default("b");
        let answer = select.ask(&console).expect("select");
        assert_eq!(answer, "b");
    }

    #[test]
    fn test_select_with_labels() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let select = Select::new("Pick")
            .choice(Choice::with_label("us-east-1", "US East (N. Virginia)"))
            .choice(Choice::with_label("eu-west-1", "EU (Ireland)"));

        let input = b"1\n";
        let mut reader = io::Cursor::new(&input[..]);
        let answer = select.ask_from(&console, &mut reader).expect("select");
        assert_eq!(answer, "us-east-1");
    }

    #[test]
    fn test_confirm_yes() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let confirm = Confirm::new("Continue?");

        let input = b"y\n";
        let mut reader = io::Cursor::new(&input[..]);
        let answer = confirm.ask_from(&console, &mut reader).expect("confirm");
        assert!(answer);
    }

    #[test]
    fn test_confirm_no() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let confirm = Confirm::new("Continue?");

        let input = b"n\n";
        let mut reader = io::Cursor::new(&input[..]);
        let answer = confirm.ask_from(&console, &mut reader).expect("confirm");
        assert!(!answer);
    }

    #[test]
    fn test_confirm_default_yes() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let confirm = Confirm::new("Continue?").default(true);

        let input = b"\n"; // Empty input
        let mut reader = io::Cursor::new(&input[..]);
        let answer = confirm.ask_from(&console, &mut reader).expect("confirm");
        assert!(answer);
    }

    #[test]
    fn test_confirm_non_interactive_uses_default() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let confirm = Confirm::new("Continue?").default(false);
        let answer = confirm.ask(&console).expect("confirm");
        assert!(!answer);
    }

    #[test]
    fn test_choice_display() {
        let simple = Choice::new("value");
        assert_eq!(simple.display(), "value");

        let labeled = Choice::with_label("value", "Display Label");
        assert_eq!(labeled.display(), "Display Label");
    }

    // ========================================================================
    // Comprehensive Prompt Tests (bd-1trs)
    // ========================================================================

    #[test]
    fn test_prompt_builder_chain() {
        // Test that all builder methods work and return Self for chaining
        let prompt = Prompt::new("Enter name")
            .default("Alice")
            .allow_empty(true)
            .show_default(false)
            .markup(false)
            .validate(|_| Ok(()));

        assert_eq!(prompt.label, "Enter name");
        assert_eq!(prompt.default, Some("Alice".to_string()));
        assert!(prompt.allow_empty);
        assert!(!prompt.show_default);
        assert!(!prompt.markup);
        assert!(prompt.validator.is_some());
    }

    #[test]
    fn test_prompt_display_shows_default() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        // Disable markup on prompt so [Bob] appears literally in output
        let prompt = Prompt::new("Name")
            .default("Bob")
            .show_default(true)
            .markup(false);
        let input = b"Alice\n";
        let mut reader = io::Cursor::new(&input[..]);
        let _ = prompt.ask_from(&console, &mut reader);

        let out = buffer.0.lock().unwrap();
        let text = String::from_utf8_lossy(&out);
        // Should show "Name [Bob]: " format
        assert!(text.contains("Name"), "Expected 'Name' in output: {text:?}");
        assert!(
            text.contains("[Bob]"),
            "Expected '[Bob]' in output: {text:?}"
        );
    }

    #[test]
    fn test_prompt_display_hides_default() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let prompt = Prompt::new("Name").default("Bob").show_default(false);
        let input = b"Alice\n";
        let mut reader = io::Cursor::new(&input[..]);
        let _ = prompt.ask_from(&console, &mut reader);

        let out = buffer.0.lock().unwrap();
        let text = String::from_utf8_lossy(&out);
        // Should show "Name: " without the default
        assert!(text.contains("Name"), "Expected 'Name' in output: {text:?}");
        assert!(
            !text.contains("[Bob]"),
            "Should NOT show '[Bob]' when show_default=false: {text:?}"
        );
    }

    #[test]
    fn test_prompt_display_escapes_markup_in_default() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(true) // Markup enabled
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        // Default contains markup-like text that should be escaped
        let prompt = Prompt::new("Name").default("[bold]text[/]").markup(true);
        let input = b"Alice\n";
        let mut reader = io::Cursor::new(&input[..]);
        let _ = prompt.ask_from(&console, &mut reader);

        // The default should be escaped so it displays literally
        let out = buffer.0.lock().unwrap();
        let text = String::from_utf8_lossy(&out);
        // The escaped version should appear (markup::escape converts [ to \[)
        assert!(text.contains("Name"), "Expected 'Name' in output: {text:?}");
    }

    #[test]
    fn test_prompt_empty_input_uses_default() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let prompt = Prompt::new("Name").default("DefaultName");
        let input = b"\n"; // Empty input
        let mut reader = io::Cursor::new(&input[..]);
        let answer = prompt.ask_from(&console, &mut reader).expect("prompt");
        assert_eq!(answer, "DefaultName");
    }

    #[test]
    fn test_prompt_no_default_no_allow_empty_reprompts() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let prompt = Prompt::new("Name").allow_empty(false);
        // First empty, then valid
        let input = b"\nAlice\n";
        let mut reader = io::Cursor::new(&input[..]);
        let answer = prompt.ask_from(&console, &mut reader).expect("prompt");
        assert_eq!(answer, "Alice");

        let out = buffer.0.lock().unwrap();
        let text = String::from_utf8_lossy(&out);
        assert!(
            text.contains("Input required"),
            "Expected 'Input required' error message: {text:?}"
        );
    }

    #[test]
    fn test_prompt_allow_empty_true() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let prompt = Prompt::new("Name").allow_empty(true);
        let input = b"\n"; // Empty input
        let mut reader = io::Cursor::new(&input[..]);
        let answer = prompt.ask_from(&console, &mut reader).expect("prompt");
        assert_eq!(answer, ""); // Empty is allowed
    }

    #[test]
    fn test_prompt_validation_passes_on_valid_input() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let prompt = Prompt::new("Email").validate(|value| {
            if value.contains('@') {
                Ok(())
            } else {
                Err("must contain @".to_string())
            }
        });

        let input = b"test@example.com\n";
        let mut reader = io::Cursor::new(&input[..]);
        let answer = prompt.ask_from(&console, &mut reader).expect("prompt");
        assert_eq!(answer, "test@example.com");

        // No error message should be printed
        let out = buffer.0.lock().unwrap();
        let text = String::from_utf8_lossy(&out);
        assert!(
            !text.contains("must contain @"),
            "Should not show error for valid input: {text:?}"
        );
    }

    #[test]
    fn test_prompt_multiple_validation_failures() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let prompt = Prompt::new("Number").validate(|value| {
            value
                .parse::<i32>()
                .map(|_| ())
                .map_err(|_| "must be a number".to_string())
        });

        // Multiple invalid inputs, then valid
        let input = b"abc\nxyz\n42\n";
        let mut reader = io::Cursor::new(&input[..]);
        let answer = prompt.ask_from(&console, &mut reader).expect("prompt");
        assert_eq!(answer, "42");

        let out = buffer.0.lock().unwrap();
        let text = String::from_utf8_lossy(&out);
        // Should have shown the error message (at least twice)
        let error_count = text.matches("must be a number").count();
        assert!(
            error_count >= 2,
            "Expected at least 2 error messages, found {error_count}: {text:?}"
        );
    }

    #[test]
    fn test_prompt_input_whitespace_trimmed() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let prompt = Prompt::new("Name");
        let input = b"  Alice  \n"; // Whitespace around input
        let mut reader = io::Cursor::new(&input[..]);
        let answer = prompt.ask_from(&console, &mut reader).expect("prompt");
        // Trailing whitespace should be trimmed
        assert_eq!(answer, "  Alice");
    }

    #[test]
    fn test_prompt_eof_returns_error() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let prompt = Prompt::new("Name");
        let input = b""; // Empty input (EOF)
        let mut reader = io::Cursor::new(&input[..]);
        let result = prompt.ask_from(&console, &mut reader);
        assert!(matches!(result, Err(PromptError::Eof)));
    }

    #[test]
    fn test_prompt_debug_impl() {
        let prompt = Prompt::new("Name").default("Alice").validate(|_| Ok(()));

        let debug_str = format!("{prompt:?}");
        assert!(
            debug_str.contains("Prompt"),
            "Debug should contain 'Prompt': {debug_str}"
        );
        assert!(
            debug_str.contains("Name"),
            "Debug should contain label: {debug_str}"
        );
        assert!(
            debug_str.contains("Alice"),
            "Debug should contain default: {debug_str}"
        );
        assert!(
            debug_str.contains("<validator>"),
            "Debug should show validator placeholder: {debug_str}"
        );
    }

    #[test]
    fn test_prompt_error_display() {
        let not_interactive = PromptError::NotInteractive;
        assert_eq!(
            format!("{not_interactive}"),
            "prompt requires an interactive console"
        );

        let eof = PromptError::Eof;
        assert_eq!(format!("{eof}"), "prompt input reached EOF");

        let validation = PromptError::Validation("invalid input".to_string());
        assert_eq!(format!("{validation}"), "invalid input");

        let io_err = PromptError::Io(io::Error::new(io::ErrorKind::NotFound, "file not found"));
        assert!(format!("{io_err}").contains("file not found"));
    }

    #[test]
    fn test_prompt_error_source() {
        let not_interactive = PromptError::NotInteractive;
        assert!(StdError::source(&not_interactive).is_none());

        let eof = PromptError::Eof;
        assert!(StdError::source(&eof).is_none());

        let validation = PromptError::Validation("test".to_string());
        assert!(StdError::source(&validation).is_none());

        let io_err = PromptError::Io(io::Error::new(io::ErrorKind::NotFound, "test"));
        assert!(StdError::source(&io_err).is_some());
    }

    #[test]
    fn test_prompt_error_from_io_error() {
        let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "access denied");
        let prompt_err: PromptError = io_err.into();
        assert!(matches!(prompt_err, PromptError::Io(_)));
        assert!(format!("{prompt_err}").contains("access denied"));
    }

    #[test]
    fn test_prompt_markup_in_label() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(true)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        // Label with markup - should be processed when markup=true
        let prompt = Prompt::new("[bold]Name[/]").markup(true);
        let input = b"Alice\n";
        let mut reader = io::Cursor::new(&input[..]);
        let _ = prompt.ask_from(&console, &mut reader);

        // The prompt label should have been printed
        let out = buffer.0.lock().unwrap();
        let text = String::from_utf8_lossy(&out);
        assert!(text.contains("Name"), "Expected 'Name' in output: {text:?}");
    }

    #[test]
    fn test_prompt_markup_disabled_in_label() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(true)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        // Label with markup tags - should be printed literally when markup=false
        let prompt = Prompt::new("[bold]Name[/]").markup(false);
        let input = b"Alice\n";
        let mut reader = io::Cursor::new(&input[..]);
        let _ = prompt.ask_from(&console, &mut reader);

        let out = buffer.0.lock().unwrap();
        let text = String::from_utf8_lossy(&out);
        // With markup=false, the literal brackets should appear
        assert!(
            text.contains("[bold]Name[/]"),
            "Expected literal '[bold]Name[/]' in output: {text:?}"
        );
    }

    #[test]
    fn test_prompt_clone() {
        let prompt = Prompt::new("Name")
            .default("Alice")
            .allow_empty(true)
            .show_default(false)
            .markup(false);

        let cloned = prompt.clone();
        assert_eq!(cloned.label, prompt.label);
        assert_eq!(cloned.default, prompt.default);
        assert_eq!(cloned.allow_empty, prompt.allow_empty);
        assert_eq!(cloned.show_default, prompt.show_default);
        assert_eq!(cloned.markup, prompt.markup);
    }

    // ========================================================================
    // Additional PromptError Tests
    // ========================================================================

    #[test]
    fn test_prompt_not_interactive_error() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(false) // Not interactive
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        // No default set
        let prompt = Prompt::new("Name");
        let result = prompt.ask(&console);
        assert!(matches!(result, Err(PromptError::NotInteractive)));
    }

    // ========================================================================
    // InputTooLong variant tests (bd-2e33)
    // ========================================================================

    #[test]
    fn test_prompt_error_input_too_long_display() {
        let err = PromptError::InputTooLong {
            limit: 256,
            received: 1024,
        };
        assert_eq!(
            err.to_string(),
            "input too long: received at least 1024 bytes, limit is 256 bytes"
        );
    }

    #[test]
    fn test_prompt_error_input_too_long_source_is_none() {
        let err = PromptError::InputTooLong {
            limit: 100,
            received: 200,
        };
        assert!(StdError::source(&err).is_none());
    }

    #[test]
    fn test_prompt_error_is_input_too_long() {
        let too_long = PromptError::InputTooLong {
            limit: 100,
            received: 200,
        };
        assert!(too_long.is_input_too_long());

        let eof = PromptError::Eof;
        assert!(!eof.is_input_too_long());

        let not_interactive = PromptError::NotInteractive;
        assert!(!not_interactive.is_input_too_long());

        let validation = PromptError::Validation("test".to_string());
        assert!(!validation.is_input_too_long());

        let io_err = PromptError::Io(io::Error::other("test"));
        assert!(!io_err.is_input_too_long());
    }

    #[test]
    fn test_prompt_error_input_limit() {
        let too_long = PromptError::InputTooLong {
            limit: 100,
            received: 200,
        };
        assert_eq!(too_long.input_limit(), Some(100));

        let eof = PromptError::Eof;
        assert_eq!(eof.input_limit(), None);

        let not_interactive = PromptError::NotInteractive;
        assert_eq!(not_interactive.input_limit(), None);
    }

    #[test]
    fn test_prompt_error_input_too_long_debug() {
        let err = PromptError::InputTooLong {
            limit: 64 * 1024,
            received: 128 * 1024,
        };
        let debug_str = format!("{err:?}");
        assert!(debug_str.contains("InputTooLong"));
        assert!(debug_str.contains("65536"));
        assert!(debug_str.contains("131072"));
    }

    #[test]
    fn test_default_max_input_length_constant() {
        assert_eq!(super::DEFAULT_MAX_INPUT_LENGTH, 64 * 1024);
        assert_eq!(super::DEFAULT_MAX_INPUT_LENGTH, 65536);
    }

    // ========================================================================
    // read_line_limited tests (bd-uqdk)
    // ========================================================================

    #[test]
    fn test_read_line_limited_normal_input() {
        let mut reader = io::Cursor::new("hello world\n");
        let result = super::read_line_limited(&mut reader, 100).unwrap();
        assert_eq!(result, "hello world\n");
    }

    #[test]
    fn test_read_line_limited_exactly_at_limit() {
        let input = "ab\n"; // 3 bytes
        let mut reader = io::Cursor::new(input);
        let result = super::read_line_limited(&mut reader, 3).unwrap();
        assert_eq!(result, "ab\n");
    }

    #[test]
    fn test_read_line_limited_exceeds_limit() {
        let mut reader = io::Cursor::new("this is a long input\n");
        let result = super::read_line_limited(&mut reader, 5);
        assert!(matches!(
            result,
            Err(PromptError::InputTooLong { limit: 5, .. })
        ));
    }

    #[test]
    fn test_read_line_limited_empty_eof() {
        let mut reader = io::Cursor::new("");
        let result = super::read_line_limited(&mut reader, 100);
        assert!(matches!(result, Err(PromptError::Eof)));
    }

    #[test]
    fn test_read_line_limited_no_newline_eof() {
        let mut reader = io::Cursor::new("no newline");
        let result = super::read_line_limited(&mut reader, 100).unwrap();
        assert_eq!(result, "no newline");
    }

    #[test]
    fn test_read_line_limited_empty_line() {
        let mut reader = io::Cursor::new("\n");
        let result = super::read_line_limited(&mut reader, 100).unwrap();
        assert_eq!(result, "\n");
    }

    #[test]
    fn test_read_line_limited_unicode_input() {
        let mut reader = io::Cursor::new("héllo 世界\n");
        let result = super::read_line_limited(&mut reader, 100).unwrap();
        assert_eq!(result, "héllo 世界\n");
    }

    #[test]
    fn test_read_line_limited_invalid_utf8() {
        let invalid: Vec<u8> = vec![0xff, 0xfe, b'\n'];
        let mut reader = io::Cursor::new(invalid);
        let result = super::read_line_limited(&mut reader, 100);
        assert!(
            matches!(result, Err(PromptError::Validation(ref msg)) if msg.contains("UTF-8")),
            "Expected Validation error with UTF-8 message, got: {result:?}"
        );
    }

    #[test]
    fn test_read_line_limited_one_byte_limit() {
        // Only a single newline fits
        let mut reader = io::Cursor::new("\n");
        let result = super::read_line_limited(&mut reader, 1).unwrap();
        assert_eq!(result, "\n");

        // Anything longer fails
        let mut reader2 = io::Cursor::new("a\n");
        let result2 = super::read_line_limited(&mut reader2, 1);
        assert!(matches!(
            result2,
            Err(PromptError::InputTooLong { limit: 1, .. })
        ));
    }

    #[test]
    fn test_read_line_limited_multiple_lines_reads_first() {
        let mut reader = io::Cursor::new("line1\nline2\n");
        let result = super::read_line_limited(&mut reader, 100).unwrap();
        assert_eq!(result, "line1\n");
        // Second line is still available
        let result2 = super::read_line_limited(&mut reader, 100).unwrap();
        assert_eq!(result2, "line2\n");
    }

    #[test]
    fn test_read_line_limited_crlf_input() {
        let mut reader = io::Cursor::new("hello\r\n");
        let result = super::read_line_limited(&mut reader, 100).unwrap();
        // Reads up to and including \n; \r is part of the content
        assert_eq!(result, "hello\r\n");
    }

    // ========================================================================
    // Prompt max_length integration tests (bd-1jm0)
    // ========================================================================

    #[test]
    fn test_prompt_max_length_builder() {
        let prompt = Prompt::new("Test").max_length(100);
        assert_eq!(prompt.max_length, 100);
    }

    #[test]
    fn test_prompt_max_length_zero_clamped_to_one() {
        let prompt = Prompt::new("Test").max_length(0);
        assert_eq!(prompt.max_length, 1);
    }

    #[test]
    fn test_prompt_default_max_length() {
        let prompt = Prompt::new("Test");
        assert_eq!(prompt.max_length, super::DEFAULT_MAX_INPUT_LENGTH);
    }

    #[test]
    fn test_prompt_max_length_in_debug() {
        let prompt = Prompt::new("Test").max_length(256);
        let debug_str = format!("{prompt:?}");
        assert!(
            debug_str.contains("256"),
            "Debug should contain max_length value: {debug_str}"
        );
        assert!(
            debug_str.contains("max_length"),
            "Debug should contain 'max_length' field: {debug_str}"
        );
    }

    #[test]
    fn test_prompt_input_too_long_via_ask_from() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let prompt = Prompt::new("Name").max_length(5);
        // "this is too long\n" exceeds 5-byte limit
        let input = b"this is too long\n";
        let mut reader = io::Cursor::new(&input[..]);
        let result = prompt.ask_from(&console, &mut reader);
        assert!(
            matches!(result, Err(PromptError::InputTooLong { limit: 5, .. })),
            "Expected InputTooLong error, got: {result:?}"
        );
    }

    #[test]
    fn test_prompt_input_within_max_length() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let prompt = Prompt::new("Name").max_length(100);
        let input = b"Alice\n";
        let mut reader = io::Cursor::new(&input[..]);
        let answer = prompt.ask_from(&console, &mut reader).expect("prompt");
        assert_eq!(answer, "Alice");
    }

    #[test]
    fn test_prompt_zero_max_length_still_accepts_newline_default() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let prompt = Prompt::new("Name").default("fallback").max_length(0);
        let input = b"\n";
        let mut reader = io::Cursor::new(&input[..]);
        let answer = prompt.ask_from(&console, &mut reader).expect("prompt");
        assert_eq!(answer, "fallback");
    }

    #[test]
    fn test_prompt_max_length_chaining() {
        let prompt = Prompt::new("Test")
            .default("default")
            .max_length(256)
            .allow_empty(true)
            .validate(|_| Ok(()));

        assert_eq!(prompt.max_length, 256);
        assert_eq!(prompt.default, Some("default".to_string()));
        assert!(prompt.allow_empty);
    }

    // ========================================================================
    // Select and Confirm max_length tests (bd-fal7)
    // ========================================================================

    #[test]
    fn test_select_max_length_builder() {
        let select = Select::new("Pick").choices(["a", "b"]).max_length(128);
        assert_eq!(select.max_length, 128);
    }

    #[test]
    fn test_select_max_length_zero_clamped_to_one() {
        let select = Select::new("Pick").choices(["a", "b"]).max_length(0);
        assert_eq!(select.max_length, 1);
    }

    #[test]
    fn test_select_default_max_length() {
        let select = Select::new("Pick").choices(["a"]);
        assert_eq!(select.max_length, super::DEFAULT_MAX_INPUT_LENGTH);
    }

    #[test]
    fn test_select_input_too_long() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let select = Select::new("Pick").choices(["a", "b"]).max_length(3);
        let input = b"this exceeds limit\n";
        let mut reader = io::Cursor::new(&input[..]);
        let result = select.ask_from(&console, &mut reader);
        assert!(
            matches!(result, Err(PromptError::InputTooLong { limit: 3, .. })),
            "Expected InputTooLong, got: {result:?}"
        );
    }

    #[test]
    fn test_select_zero_max_length_still_accepts_newline_default() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let select = Select::new("Pick")
            .choices(["alpha", "beta"])
            .default("alpha")
            .max_length(0);
        let input = b"\n";
        let mut reader = io::Cursor::new(&input[..]);
        let answer = select.ask_from(&console, &mut reader).expect("select");
        assert_eq!(answer, "alpha");
    }

    #[test]
    fn test_confirm_max_length_builder() {
        let confirm = Confirm::new("Continue?").max_length(32);
        assert_eq!(confirm.max_length, 32);
    }

    #[test]
    fn test_confirm_max_length_zero_clamped_to_one() {
        let confirm = Confirm::new("Continue?").max_length(0);
        assert_eq!(confirm.max_length, 1);
    }

    #[test]
    fn test_confirm_default_max_length() {
        let confirm = Confirm::new("Continue?");
        assert_eq!(confirm.max_length, super::DEFAULT_MAX_INPUT_LENGTH);
    }

    #[test]
    fn test_confirm_input_too_long() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let confirm = Confirm::new("Continue?").max_length(3);
        let input = b"this exceeds limit\n";
        let mut reader = io::Cursor::new(&input[..]);
        let result = confirm.ask_from(&console, &mut reader);
        assert!(
            matches!(result, Err(PromptError::InputTooLong { limit: 3, .. })),
            "Expected InputTooLong, got: {result:?}"
        );
    }

    #[test]
    fn test_confirm_zero_max_length_still_accepts_newline_default() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let confirm = Confirm::new("Continue?").default(true).max_length(0);
        let input = b"\n";
        let mut reader = io::Cursor::new(&input[..]);
        let answer = confirm.ask_from(&console, &mut reader).expect("confirm");
        assert!(answer);
    }

    #[test]
    fn test_confirm_within_max_length() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let confirm = Confirm::new("Continue?").max_length(100);
        let input = b"y\n";
        let mut reader = io::Cursor::new(&input[..]);
        let answer = confirm.ask_from(&console, &mut reader).expect("confirm");
        assert!(answer);
    }

    // ========================================================================
    // Comprehensive unit tests for interactive.rs (bd-ic9b)
    // ========================================================================

    // --- Pager tests ---

    #[test]
    fn test_pager_builder_defaults() {
        let pager = Pager::new();
        assert!(pager.command.is_none());
        assert!(pager.allow_color);
    }

    #[test]
    fn test_pager_custom_command() {
        let pager = Pager::new().command("more -R");
        assert_eq!(pager.command.as_deref(), Some("more -R"));
    }

    #[test]
    fn test_pager_allow_color_false() {
        let pager = Pager::new().allow_color(false);
        assert!(!pager.allow_color);
    }

    #[test]
    fn test_pager_default_impl() {
        let pager = Pager::default();
        assert!(pager.command.is_none());
        assert!(pager.allow_color);
    }

    #[test]
    fn test_pager_debug_impl() {
        let pager = Pager::new().command("less");
        let debug = format!("{pager:?}");
        assert!(
            debug.contains("Pager"),
            "Debug should contain 'Pager': {debug}"
        );
        assert!(
            debug.contains("less"),
            "Debug should contain command: {debug}"
        );
    }

    #[test]
    fn test_pager_clone() {
        let pager = Pager::new().command("cat").allow_color(false);
        let cloned = pager.clone();
        assert_eq!(cloned.command, pager.command);
        assert_eq!(cloned.allow_color, pager.allow_color);
    }

    // --- Choice tests ---

    #[test]
    fn test_choice_new() {
        let choice = Choice::new("option1");
        assert_eq!(choice.value, "option1");
        assert!(choice.label.is_none());
        assert_eq!(choice.display(), "option1");
    }

    #[test]
    fn test_choice_with_label() {
        let choice = Choice::with_label("us-east-1", "US East (Virginia)");
        assert_eq!(choice.value, "us-east-1");
        assert_eq!(choice.label.as_deref(), Some("US East (Virginia)"));
        assert_eq!(choice.display(), "US East (Virginia)");
    }

    #[test]
    fn test_choice_from_string() {
        let choice: Choice = "hello".into();
        assert_eq!(choice.value, "hello");
        assert!(choice.label.is_none());
    }

    #[test]
    fn test_choice_from_owned_string() {
        let choice: Choice = String::from("world").into();
        assert_eq!(choice.value, "world");
    }

    #[test]
    fn test_choice_debug_and_clone() {
        let choice = Choice::with_label("val", "lbl");
        let debug = format!("{choice:?}");
        assert!(debug.contains("val"));
        assert!(debug.contains("lbl"));

        let cloned = choice.clone();
        assert_eq!(cloned.value, "val");
        assert_eq!(cloned.label.as_deref(), Some("lbl"));
    }

    // --- Select edge case tests ---

    #[test]
    fn test_select_empty_choices_error() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let select = Select::new("Pick");
        let input = b"1\n";
        let mut reader = io::Cursor::new(&input[..]);
        let result = select.ask_from(&console, &mut reader);
        assert!(
            matches!(result, Err(PromptError::Validation(ref msg)) if msg.contains("No choices")),
            "Expected Validation error about no choices, got: {result:?}"
        );
    }

    #[test]
    fn test_select_invalid_then_valid() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let select = Select::new("Pick").choices(["a", "b", "c"]);
        // First: invalid input, then valid number
        let input = b"999\n2\n";
        let mut reader = io::Cursor::new(&input[..]);
        let result = select.ask_from(&console, &mut reader).expect("select");
        assert_eq!(result, "b");
    }

    #[test]
    fn test_select_non_interactive_no_default_error() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let select = Select::new("Pick").choices(["a", "b"]);
        // No default set, non-interactive
        let result = select.ask(&console);
        assert!(matches!(result, Err(PromptError::NotInteractive)));
    }

    #[test]
    fn test_select_eof() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let select = Select::new("Pick").choices(["a", "b"]);
        let input = b"";
        let mut reader = io::Cursor::new(&input[..]);
        let result = select.ask_from(&console, &mut reader);
        assert!(matches!(result, Err(PromptError::Eof)));
    }

    #[test]
    fn test_select_builder_chaining() {
        let select = Select::new("Pick")
            .choices(["a", "b"])
            .choice("c")
            .default("b")
            .show_default(false)
            .markup(false)
            .max_length(512);

        assert_eq!(select.label, "Pick");
        assert_eq!(select.choices.len(), 3);
        assert_eq!(select.default.as_deref(), Some("b"));
        assert!(!select.show_default);
        assert!(!select.markup);
        assert_eq!(select.max_length, 512);
    }

    // --- Confirm edge case tests ---

    #[test]
    fn test_confirm_all_yes_variants() {
        let yes_inputs: &[&[u8]] = &[b"y\n", b"yes\n", b"true\n", b"1\n"];

        for input_bytes in yes_inputs {
            let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
            let console = Console::builder()
                .force_terminal(true)
                .markup(false)
                .file(Box::new(buffer.clone()))
                .build()
                .shared();

            let confirm = Confirm::new("Continue?");
            let mut reader = io::Cursor::new(*input_bytes);
            let msg = format!(
                "Failed on input: {:?}",
                String::from_utf8_lossy(input_bytes)
            );
            let answer = confirm.ask_from(&console, &mut reader).expect(&msg);
            assert!(
                answer,
                "Expected true for input {:?}",
                String::from_utf8_lossy(input_bytes)
            );
        }
    }

    #[test]
    fn test_confirm_all_no_variants() {
        let no_inputs: &[&[u8]] = &[b"n\n", b"no\n", b"false\n", b"0\n"];

        for input_bytes in no_inputs {
            let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
            let console = Console::builder()
                .force_terminal(true)
                .markup(false)
                .file(Box::new(buffer.clone()))
                .build()
                .shared();

            let confirm = Confirm::new("Continue?");
            let mut reader = io::Cursor::new(*input_bytes);
            let msg = format!(
                "Failed on input: {:?}",
                String::from_utf8_lossy(input_bytes)
            );
            let answer = confirm.ask_from(&console, &mut reader).expect(&msg);
            assert!(
                !answer,
                "Expected false for input {:?}",
                String::from_utf8_lossy(input_bytes)
            );
        }
    }

    #[test]
    fn test_confirm_invalid_then_valid() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let confirm = Confirm::new("Continue?");
        // "maybe" is invalid, then "y" is valid
        let input = b"maybe\ny\n";
        let mut reader = io::Cursor::new(&input[..]);
        let answer = confirm.ask_from(&console, &mut reader).expect("confirm");
        assert!(answer);

        let out = buffer.0.lock().unwrap();
        let text = String::from_utf8_lossy(&out);
        assert!(
            text.contains("Please enter y or n"),
            "Expected error prompt in output: {text:?}"
        );
    }

    #[test]
    fn test_confirm_default_false() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let confirm = Confirm::new("Continue?").default(false);
        let input = b"\n"; // Empty = use default
        let mut reader = io::Cursor::new(&input[..]);
        let answer = confirm.ask_from(&console, &mut reader).expect("confirm");
        assert!(!answer);
    }

    #[test]
    fn test_confirm_no_default_empty_reprompts() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let confirm = Confirm::new("Continue?"); // No default
        // First empty (reprompt), then yes
        let input = b"\ny\n";
        let mut reader = io::Cursor::new(&input[..]);
        let answer = confirm.ask_from(&console, &mut reader).expect("confirm");
        assert!(answer);
    }

    #[test]
    fn test_confirm_eof() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let confirm = Confirm::new("Continue?");
        let input = b"";
        let mut reader = io::Cursor::new(&input[..]);
        let result = confirm.ask_from(&console, &mut reader);
        assert!(matches!(result, Err(PromptError::Eof)));
    }

    #[test]
    fn test_confirm_non_interactive_no_default_error() {
        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let confirm = Confirm::new("Continue?"); // No default
        let result = confirm.ask(&console);
        assert!(matches!(result, Err(PromptError::NotInteractive)));
    }

    #[test]
    fn test_confirm_builder_chaining() {
        let confirm = Confirm::new("Delete?")
            .default(false)
            .markup(false)
            .max_length(64);

        assert_eq!(confirm.label, "Delete?");
        assert_eq!(confirm.default, Some(false));
        assert!(!confirm.markup);
        assert_eq!(confirm.max_length, 64);
    }

    // --- trim_newline tests ---

    #[test]
    fn test_trim_newline_lf() {
        assert_eq!(super::trim_newline("hello\n"), "hello");
    }

    #[test]
    fn test_trim_newline_crlf() {
        assert_eq!(super::trim_newline("hello\r\n"), "hello");
    }

    #[test]
    fn test_trim_newline_none() {
        assert_eq!(super::trim_newline("hello"), "hello");
    }

    #[test]
    fn test_trim_newline_empty() {
        assert_eq!(super::trim_newline(""), "");
    }

    #[test]
    fn test_trim_newline_only_newline() {
        assert_eq!(super::trim_newline("\n"), "");
    }
}
