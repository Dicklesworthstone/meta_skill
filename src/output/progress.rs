//! Progress visualization system for rich terminal output.
//!
//! This module provides comprehensive progress tracking with:
//! - Determinate progress bars with ETA, speed, and percentage
//! - Indeterminate spinners with multiple animation styles
//! - Multi-progress display for concurrent operations
//! - Automatic agent mode fallback for CI/MCP environments
//! - Thread-safe updates with RAII-style cleanup
//!
//! # Overview
//!
//! The progress system adapts to the terminal environment:
//!
//! - **Rich mode**: Full animation, colors, Unicode characters
//! - **Plain mode**: Simple text progress indicators
//! - **Agent mode**: Periodic status lines or JSON events
//!
//! # Example
//!
//! ```rust,ignore
//! use ms::output::progress::{ProgressBar, ProgressGuard};
//!
//! // Create a progress bar for a known total
//! let mut progress = ProgressBar::new(100)
//!     .message("Processing files")
//!     .show_eta(true)
//!     .show_speed(true);
//!
//! for i in 0..100 {
//!     progress.update(i + 1);
//!     // ... do work ...
//! }
//! progress.finish_with_message("Done!");
//!
//! // Or use the RAII guard for automatic cleanup
//! let guard = ProgressGuard::new("Indexing", 50)?;
//! for i in 0..50 {
//!     guard.update(i + 1);
//! }
//! // Progress is cleared on drop
//! ```

use std::io::{self, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::{Mutex, RwLock};
use rich_rust::renderables::progress::{
    BarStyle as RichBarStyle, ProgressBar as RichProgressBar, Spinner as RichSpinner,
};
use serde::Serialize;

use super::OutputMode;
use super::detection::is_agent_environment;
use super::theme::{ProgressStyle, Theme};

// ============================================================================
// Progress Bar Style
// ============================================================================

/// Bar style variants for the progress bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BarStyle {
    /// Unicode block characters (█▓░) - default
    #[default]
    Block,
    /// Standard ASCII bar using # and -
    Ascii,
    /// Thin line style (━─)
    Line,
    /// Dots style (●○)
    Dots,
    /// Shaded gradient style
    Gradient,
}

impl BarStyle {
    /// Convert to rich_rust BarStyle.
    #[must_use]
    pub const fn to_rich_style(self) -> RichBarStyle {
        match self {
            Self::Block => RichBarStyle::Block,
            Self::Ascii => RichBarStyle::Ascii,
            Self::Line => RichBarStyle::Line,
            Self::Dots => RichBarStyle::Dots,
            Self::Gradient => RichBarStyle::Gradient,
        }
    }

    /// Create from theme progress style.
    #[must_use]
    pub const fn from_theme_style(style: ProgressStyle) -> Self {
        match style {
            ProgressStyle::Block => Self::Block,
            ProgressStyle::Ascii => Self::Ascii,
            ProgressStyle::Line => Self::Line,
            ProgressStyle::Dots => Self::Dots,
        }
    }
}

// ============================================================================
// Spinner Style
// ============================================================================

/// Predefined spinner animation styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpinnerStyle {
    /// Braille dots spinner (⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏) - default
    #[default]
    Dots,
    /// Simple line spinner (|/-\)
    Simple,
    /// Horizontal line spinner (⎺⎻⎼⎽⎼⎻)
    Line,
    /// Bouncing ball spinner (⠁⠂⠄⠂)
    Bounce,
    /// Growing dots spinner (⣾⣽⣻⢿⡿⣟⣯⣷)
    Growing,
    /// Moon phases (🌑🌒🌓🌔🌕🌖🌗🌘)
    Moon,
    /// Clock faces (🕐🕑🕒...)
    Clock,
}

impl SpinnerStyle {
    /// Create a rich_rust Spinner with this style.
    #[must_use]
    pub fn to_rich_spinner(self) -> RichSpinner {
        match self {
            Self::Dots => RichSpinner::dots(),
            Self::Simple => RichSpinner::simple(),
            Self::Line => RichSpinner::line(),
            Self::Bounce => RichSpinner::bounce(),
            Self::Growing => RichSpinner::growing(),
            Self::Moon => RichSpinner::moon(),
            Self::Clock => RichSpinner::clock(),
        }
    }

    /// Get the frames for this spinner style.
    #[must_use]
    pub fn frames(self) -> Vec<&'static str> {
        match self {
            Self::Dots => vec!["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
            Self::Simple => vec!["|", "/", "-", "\\"],
            Self::Line => vec!["⎺", "⎻", "⎼", "⎽", "⎼", "⎻"],
            Self::Bounce => vec!["⠁", "⠂", "⠄", "⠂"],
            Self::Growing => vec!["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"],
            Self::Moon => vec!["🌑", "🌒", "🌓", "🌔", "🌕", "🌖", "🌗", "🌘"],
            Self::Clock => vec![
                "🕐", "🕑", "🕒", "🕓", "🕔", "🕕", "🕖", "🕗", "🕘", "🕙", "🕚", "🕛",
            ],
        }
    }

    /// Get ASCII fallback frames.
    #[must_use]
    pub fn ascii_frames() -> Vec<&'static str> {
        vec!["-", "\\", "|", "/"]
    }
}

// ============================================================================
// Progress Bar
// ============================================================================

/// A determinate progress bar with percentage, ETA, and customizable appearance.
///
/// This wraps rich_rust's ProgressBar with additional features:
/// - Theme integration for consistent styling
/// - Agent mode fallback with periodic status output
/// - Thread-safe updates
/// - RAII-style cleanup via `ProgressGuard`
///
/// # Example
///
/// ```rust,ignore
/// let mut bar = ProgressBar::new(100)
///     .message("Downloading")
///     .show_eta(true)
///     .show_speed(true);
///
/// for i in 0..100 {
///     bar.update(i + 1);
///     // ... do work ...
/// }
/// bar.finish();
/// ```
pub struct ProgressBar {
    /// The underlying rich_rust progress bar.
    inner: RichProgressBar,
    /// Current progress value.
    current: AtomicU64,
    /// Total expected count.
    total: u64,
    /// Progress message/description.
    message: String,
    /// Whether we're in agent mode.
    agent_mode: bool,
    /// Last time we emitted an agent status.
    last_agent_emit: Mutex<Instant>,
    /// Minimum interval between agent status emissions.
    agent_emit_interval: Duration,
    /// Whether the progress bar is finished.
    finished: AtomicBool,
    /// Start time for elapsed/ETA calculations.
    start_time: Instant,
    /// Terminal width for rendering.
    width: usize,
    /// Whether to use rich output.
    use_rich: bool,
    /// Theme for styling.
    theme: Theme,
    /// Output mode.
    output_mode: OutputMode,
}

impl ProgressBar {
    /// Create a new progress bar with a known total.
    ///
    /// # Arguments
    /// * `total` - The total number of items to process
    #[must_use]
    pub fn new(total: u64) -> Self {
        let agent_mode = is_agent_environment();
        let theme = Theme::default();
        let bar_style = BarStyle::from_theme_style(theme.progress_style);

        let inner = RichProgressBar::with_total(total)
            .bar_style(bar_style.to_rich_style())
            .show_percentage(true)
            .show_eta(false)
            .show_speed(false)
            .width(40);

        Self {
            inner,
            current: AtomicU64::new(0),
            total,
            message: String::new(),
            agent_mode,
            last_agent_emit: Mutex::new(Instant::now()),
            agent_emit_interval: Duration::from_secs(2),
            finished: AtomicBool::new(false),
            start_time: Instant::now(),
            width: terminal_width(),
            use_rich: !agent_mode,
            theme,
            output_mode: if agent_mode {
                OutputMode::Plain
            } else {
                OutputMode::Rich
            },
        }
    }

    /// Create an indeterminate progress bar (no total).
    #[must_use]
    pub fn indeterminate() -> Self {
        let mut bar = Self::new(0);
        bar.inner = RichProgressBar::new()
            .bar_style(RichBarStyle::Block)
            .show_percentage(false);
        bar
    }

    /// Set the progress message/description.
    #[must_use]
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self.inner = std::mem::take(&mut self.inner).description(self.message.as_str());
        self
    }

    /// Set whether to show ETA.
    #[must_use]
    pub fn show_eta(mut self, show: bool) -> Self {
        self.inner = std::mem::take(&mut self.inner).show_eta(show);
        self
    }

    /// Set whether to show speed (items/sec).
    #[must_use]
    pub fn show_speed(mut self, show: bool) -> Self {
        self.inner = std::mem::take(&mut self.inner).show_speed(show);
        self
    }

    /// Set whether to show elapsed time.
    #[must_use]
    pub fn show_elapsed(mut self, show: bool) -> Self {
        self.inner = std::mem::take(&mut self.inner).show_elapsed(show);
        self
    }

    /// Set the bar width.
    #[must_use]
    pub fn width(mut self, width: usize) -> Self {
        self.width = width;
        self.inner = std::mem::take(&mut self.inner).width(width);
        self
    }

    /// Set the bar style.
    #[must_use]
    pub fn style(mut self, style: BarStyle) -> Self {
        self.inner = std::mem::take(&mut self.inner).bar_style(style.to_rich_style());
        self
    }

    /// Set the theme.
    #[must_use]
    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        let bar_style = BarStyle::from_theme_style(self.theme.progress_style);
        self.inner = std::mem::take(&mut self.inner).bar_style(bar_style.to_rich_style());
        self
    }

    /// Set the agent mode emission interval.
    #[must_use]
    pub fn agent_emit_interval(mut self, interval: Duration) -> Self {
        self.agent_emit_interval = interval;
        self
    }

    /// Force agent mode behavior.
    #[must_use]
    pub fn force_agent_mode(mut self, agent_mode: bool) -> Self {
        self.agent_mode = agent_mode;
        self.use_rich = !agent_mode;
        self.output_mode = if agent_mode {
            OutputMode::Plain
        } else {
            OutputMode::Rich
        };
        self
    }

    /// Update the progress to a specific value.
    pub fn update(&mut self, current: u64) {
        self.current.store(current, Ordering::Relaxed);
        self.inner.update(current);

        if self.agent_mode {
            self.maybe_emit_agent_status();
        } else {
            self.render();
        }
    }

    /// Advance progress by a delta.
    pub fn advance(&mut self, delta: u64) {
        let current = self.current.fetch_add(delta, Ordering::Relaxed) + delta;
        self.inner.update(current);

        if self.agent_mode {
            self.maybe_emit_agent_status();
        } else {
            self.render();
        }
    }

    /// Set the progress as a fraction (0.0 - 1.0).
    pub fn set_progress(&mut self, progress: f64) {
        self.inner.set_progress(progress);
        if self.total > 0 {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let current = (progress * self.total as f64) as u64;
            self.current.store(current, Ordering::Relaxed);
        }

        if self.agent_mode {
            self.maybe_emit_agent_status();
        } else {
            self.render();
        }
    }

    /// Finish the progress bar.
    pub fn finish(&mut self) {
        self.finished.store(true, Ordering::Relaxed);
        self.inner.finish();
        self.render_finish();
    }

    /// Finish the progress bar with a message.
    pub fn finish_with_message(&mut self, message: &str) {
        self.finished.store(true, Ordering::Relaxed);
        self.inner = std::mem::take(&mut self.inner).finished_message(message);
        self.inner.finish();
        self.render_finish();
    }

    /// Get the current progress (0.0 - 1.0).
    #[must_use]
    pub fn progress(&self) -> f64 {
        self.inner.progress()
    }

    /// Get the elapsed time.
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Check if finished.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Relaxed)
    }

    // ========================================================================
    // Rendering
    // ========================================================================

    fn render(&self) {
        if self.output_mode == OutputMode::Json {
            return;
        }

        let plain = self.inner.render_plain(self.width);

        if self.use_rich {
            eprint!("\r{plain}");
        } else {
            // Plain mode: simple percentage
            let current = self.current.load(Ordering::Relaxed);
            if self.total > 0 {
                let pct = (current * 100) / self.total;
                eprint!("\r{} {pct}%", self.message);
            } else {
                eprint!("\r{} ...", self.message);
            }
        }
        let _ = io::stderr().flush();
    }

    fn render_finish(&self) {
        if self.output_mode == OutputMode::Json {
            return;
        }

        // Clear the progress line
        let width = self.width;
        eprint!("\r{:width$}\r", "");
        let _ = io::stderr().flush();

        if self.agent_mode {
            // Emit final status
            let current = self.current.load(Ordering::Relaxed);
            eprintln!(
                "[DONE] {} - {}/{} (100%)",
                self.message, current, self.total
            );
        }
    }

    fn maybe_emit_agent_status(&self) {
        let mut last_emit = self.last_agent_emit.lock();
        let now = Instant::now();

        if now.duration_since(*last_emit) >= self.agent_emit_interval {
            *last_emit = now;
            drop(last_emit);

            let current = self.current.load(Ordering::Relaxed);
            if self.total > 0 {
                let pct = (current * 100) / self.total;
                let elapsed = self.elapsed();
                let elapsed_secs = elapsed.as_secs();

                // Calculate ETA
                let eta = if current > 0 && pct < 100 {
                    let rate = current as f64 / elapsed.as_secs_f64();
                    let remaining = (self.total - current) as f64 / rate;
                    format!(" ETA: {}s", remaining as u64)
                } else {
                    String::new()
                };

                eprintln!(
                    "[PROGRESS] {} - {}/{} ({pct}%) elapsed: {elapsed_secs}s{eta}",
                    self.message, current, self.total
                );
            } else {
                eprintln!("[PROGRESS] {} - working...", self.message);
            }
        }
    }
}

impl Drop for ProgressBar {
    fn drop(&mut self) {
        if !self.is_finished() {
            // Clear the progress line on drop
            let width = self.width;
            eprint!("\r{:width$}\r", "");
            let _ = io::stderr().flush();
        }
    }
}

// ============================================================================
// Spinner
// ============================================================================

/// An indeterminate spinner for operations with unknown duration.
///
/// # Example
///
/// ```rust,ignore
/// let mut spinner = Spinner::new()
///     .message("Connecting to server")
///     .style(SpinnerStyle::Dots);
///
/// // Use a background thread or tick manually
/// for _ in 0..10 {
///     spinner.tick();
///     std::thread::sleep(Duration::from_millis(100));
/// }
/// spinner.finish_with_message("Connected!");
/// ```
pub struct Spinner {
    /// Animation frames.
    frames: Vec<&'static str>,
    /// Current frame index.
    frame_index: usize,
    /// Spinner message.
    message: String,
    /// Whether we're in agent mode.
    agent_mode: bool,
    /// Whether the spinner is running.
    running: AtomicBool,
    /// Start time.
    start_time: Instant,
    /// Last tick time (for agent mode rate limiting).
    last_emit: Mutex<Instant>,
    /// Agent emit interval.
    agent_emit_interval: Duration,
    /// Terminal width.
    width: usize,
    /// Whether to use rich output.
    use_rich: bool,
}

impl Spinner {
    /// Create a new spinner with default style.
    #[must_use]
    pub fn new() -> Self {
        let agent_mode = is_agent_environment();

        Self {
            frames: if agent_mode {
                SpinnerStyle::ascii_frames()
            } else {
                SpinnerStyle::default().frames()
            },
            frame_index: 0,
            message: String::new(),
            agent_mode,
            running: AtomicBool::new(true),
            start_time: Instant::now(),
            last_emit: Mutex::new(Instant::now()),
            agent_emit_interval: Duration::from_secs(5),
            width: terminal_width(),
            use_rich: !agent_mode,
        }
    }

    /// Set the spinner style.
    #[must_use]
    pub fn style(mut self, style: SpinnerStyle) -> Self {
        self.frames = if self.agent_mode {
            SpinnerStyle::ascii_frames()
        } else {
            style.frames()
        };
        self
    }

    /// Set custom frames.
    #[must_use]
    pub fn frames(mut self, frames: Vec<&'static str>) -> Self {
        self.frames = frames;
        self
    }

    /// Set the spinner message.
    #[must_use]
    pub fn message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    /// Set the agent mode emission interval.
    #[must_use]
    pub fn agent_emit_interval(mut self, interval: Duration) -> Self {
        self.agent_emit_interval = interval;
        self
    }

    /// Force agent mode behavior.
    #[must_use]
    pub fn force_agent_mode(mut self, agent_mode: bool) -> Self {
        self.agent_mode = agent_mode;
        self.use_rich = !agent_mode;
        if agent_mode {
            self.frames = SpinnerStyle::ascii_frames();
        }
        self
    }

    /// Advance to the next frame and render.
    pub fn tick(&mut self) {
        if !self.running.load(Ordering::Relaxed) {
            return;
        }

        self.frame_index = (self.frame_index + 1) % self.frames.len().max(1);

        if self.agent_mode {
            self.maybe_emit_agent_status();
        } else {
            self.render();
        }
    }

    /// Get the current frame.
    #[must_use]
    pub fn current_frame(&self) -> &str {
        if self.frames.is_empty() {
            " "
        } else {
            self.frames[self.frame_index]
        }
    }

    /// Update the message while spinning.
    pub fn set_message(&mut self, message: impl Into<String>) {
        self.message = message.into();
        if !self.agent_mode {
            self.render();
        }
    }

    /// Stop the spinner.
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        self.clear();
    }

    /// Finish with a message.
    pub fn finish_with_message(&mut self, message: &str) {
        self.running.store(false, Ordering::Relaxed);
        self.clear();

        let elapsed = self.elapsed();
        if self.agent_mode {
            eprintln!("[DONE] {message} (elapsed: {:.1}s)", elapsed.as_secs_f64());
        } else {
            eprintln!("✓ {message}");
        }
    }

    /// Check if running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Get elapsed time.
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    // ========================================================================
    // Rendering
    // ========================================================================

    fn render(&self) {
        let frame = self.current_frame();
        eprint!("\r{frame} {}", self.message);
        let _ = io::stderr().flush();
    }

    fn clear(&self) {
        let width = self.width;
        eprint!("\r{:width$}\r", "");
        let _ = io::stderr().flush();
    }

    fn maybe_emit_agent_status(&self) {
        let mut last_emit = self.last_emit.lock();
        let now = Instant::now();

        if now.duration_since(*last_emit) >= self.agent_emit_interval {
            *last_emit = now;
            drop(last_emit);

            let elapsed = self.elapsed();
            eprintln!(
                "[STATUS] {} - elapsed: {:.1}s",
                self.message,
                elapsed.as_secs_f64()
            );
        }
    }
}

impl Default for Spinner {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        if self.running.load(Ordering::Relaxed) {
            self.clear();
        }
    }
}

// ============================================================================
// Multi-Progress
// ============================================================================

/// Coordinates multiple concurrent progress bars.
///
/// This manages a collection of progress bars that are displayed together,
/// updating them without visual artifacts.
///
/// # Example
///
/// ```rust,ignore
/// let multi = MultiProgress::new();
///
/// let bar1 = multi.add(ProgressBar::new(100).message("Download"));
/// let bar2 = multi.add(ProgressBar::new(50).message("Extract"));
///
/// // Update bars from different threads
/// bar1.update(50);
/// bar2.update(25);
///
/// multi.finish_all();
/// ```
pub struct MultiProgress {
    /// The progress bars being tracked.
    bars: RwLock<Vec<Arc<Mutex<ProgressBar>>>>,
    /// Whether we're in agent mode.
    agent_mode: bool,
    /// Whether the multi-progress is finished.
    finished: AtomicBool,
    /// Terminal width.
    width: usize,
}

impl MultiProgress {
    /// Create a new multi-progress coordinator.
    #[must_use]
    pub fn new() -> Self {
        Self {
            bars: RwLock::new(Vec::new()),
            agent_mode: is_agent_environment(),
            finished: AtomicBool::new(false),
            width: terminal_width(),
        }
    }

    /// Add a progress bar and return a handle to it.
    pub fn add(&self, bar: ProgressBar) -> MultiProgressHandle {
        let bar = Arc::new(Mutex::new(bar));
        self.bars.write().push(Arc::clone(&bar));

        MultiProgressHandle {
            bar,
            _agent_mode: self.agent_mode,
        }
    }

    /// Create and add a new progress bar.
    pub fn add_bar(&self, total: u64, message: &str) -> MultiProgressHandle {
        let bar = ProgressBar::new(total).message(message);
        self.add(bar)
    }

    /// Finish all progress bars.
    pub fn finish_all(&self) {
        self.finished.store(true, Ordering::Relaxed);

        let bars = self.bars.read();
        for bar in bars.iter() {
            let mut bar = bar.lock();
            if !bar.is_finished() {
                bar.finish();
            }
        }

        // Clear all lines
        if !self.agent_mode {
            let count = bars.len();
            for _ in 0..count {
                eprint!("\r{:width$}\r\x1b[A", "", width = self.width);
            }
            eprint!("\r{:width$}\r", "", width = self.width);
            let _ = io::stderr().flush();
        }
    }

    /// Get the number of active progress bars.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bars.read().len()
    }

    /// Check if empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bars.read().is_empty()
    }

    /// Render all progress bars.
    pub fn render(&self) {
        if self.agent_mode || self.finished.load(Ordering::Relaxed) {
            return;
        }

        let bars = self.bars.read();
        for (i, bar) in bars.iter().enumerate() {
            let bar = bar.lock();
            let plain = bar.inner.render_plain(self.width);

            if i > 0 {
                eprint!("\n");
            }
            eprint!("\r{plain}");
        }

        // Move cursor back up
        let count = bars.len();
        if count > 1 {
            eprint!("\x1b[{}A", count - 1);
        }
        let _ = io::stderr().flush();
    }
}

impl Default for MultiProgress {
    fn default() -> Self {
        Self::new()
    }
}

/// Handle to a progress bar within a `MultiProgress`.
pub struct MultiProgressHandle {
    bar: Arc<Mutex<ProgressBar>>,
    /// Reserved for future use (e.g., agent-specific output formatting)
    _agent_mode: bool,
}

impl MultiProgressHandle {
    /// Update the progress.
    pub fn update(&self, current: u64) {
        self.bar.lock().update(current);
    }

    /// Advance the progress.
    pub fn advance(&self, delta: u64) {
        self.bar.lock().advance(delta);
    }

    /// Finish the progress bar.
    pub fn finish(&self) {
        self.bar.lock().finish();
    }

    /// Finish with a message.
    pub fn finish_with_message(&self, message: &str) {
        self.bar.lock().finish_with_message(message);
    }

    /// Get the current progress.
    #[must_use]
    pub fn progress(&self) -> f64 {
        self.bar.lock().progress()
    }

    /// Check if finished.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.bar.lock().is_finished()
    }
}

// ============================================================================
// Progress Guard (RAII)
// ============================================================================

/// RAII-style progress tracking with automatic cleanup.
///
/// The progress bar is automatically cleared when the guard is dropped,
/// even on panic or early return.
///
/// # Example
///
/// ```rust,ignore
/// fn process_files(files: &[Path]) -> Result<()> {
///     let guard = ProgressGuard::new("Processing", files.len() as u64)?;
///
///     for (i, file) in files.iter().enumerate() {
///         guard.update((i + 1) as u64);
///         process_file(file)?;  // Progress is cleared even if this errors
///     }
///
///     guard.finish_with_message("All files processed");
///     Ok(())
/// }
/// ```
pub struct ProgressGuard {
    bar: ProgressBar,
}

impl ProgressGuard {
    /// Create a new progress guard.
    pub fn new(message: &str, total: u64) -> Self {
        let bar = ProgressBar::new(total).message(message);
        Self { bar }
    }

    /// Create with a custom progress bar.
    pub fn with_bar(bar: ProgressBar) -> Self {
        Self { bar }
    }

    /// Update the progress.
    pub fn update(&mut self, current: u64) {
        self.bar.update(current);
    }

    /// Advance the progress.
    pub fn advance(&mut self, delta: u64) {
        self.bar.advance(delta);
    }

    /// Set progress as a fraction.
    pub fn set_progress(&mut self, progress: f64) {
        self.bar.set_progress(progress);
    }

    /// Finish with a success message.
    pub fn finish_with_message(mut self, message: &str) {
        self.bar.finish_with_message(message);
    }

    /// Get the underlying progress bar.
    pub fn bar(&self) -> &ProgressBar {
        &self.bar
    }

    /// Get a mutable reference to the underlying progress bar.
    pub fn bar_mut(&mut self) -> &mut ProgressBar {
        &mut self.bar
    }
}

impl Drop for ProgressGuard {
    fn drop(&mut self) {
        // The ProgressBar's Drop impl will clear the line
    }
}

// ============================================================================
// Agent Progress Events (JSON)
// ============================================================================

/// JSON-serializable progress event for agent mode.
#[derive(Debug, Clone, Serialize)]
pub struct ProgressEvent {
    /// Event type.
    #[serde(rename = "type")]
    pub event_type: ProgressEventType,
    /// Progress message/description.
    pub message: String,
    /// Current count (if determinate).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<u64>,
    /// Total count (if determinate).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
    /// Progress percentage (0-100).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percentage: Option<u32>,
    /// Elapsed time in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elapsed_secs: Option<f64>,
    /// Estimated time remaining in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub eta_secs: Option<f64>,
    /// Processing speed (items/sec).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
}

/// Progress event types.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProgressEventType {
    /// Progress has started.
    Start,
    /// Progress update.
    Progress,
    /// Progress completed successfully.
    Complete,
    /// Progress failed.
    Error,
}

impl ProgressEvent {
    /// Create a start event.
    #[must_use]
    pub fn start(message: impl Into<String>, total: Option<u64>) -> Self {
        Self {
            event_type: ProgressEventType::Start,
            message: message.into(),
            current: Some(0),
            total,
            percentage: Some(0),
            elapsed_secs: Some(0.0),
            eta_secs: None,
            speed: None,
        }
    }

    /// Create a progress event.
    #[must_use]
    pub fn progress(
        message: impl Into<String>,
        current: u64,
        total: u64,
        elapsed: Duration,
    ) -> Self {
        let pct = if total > 0 {
            ((current * 100) / total) as u32
        } else {
            0
        };

        let elapsed_secs = elapsed.as_secs_f64();
        let speed = if elapsed_secs > 0.0 {
            Some(current as f64 / elapsed_secs)
        } else {
            None
        };

        let eta_secs = if current > 0 && current < total {
            let rate = current as f64 / elapsed_secs;
            Some((total - current) as f64 / rate)
        } else {
            None
        };

        Self {
            event_type: ProgressEventType::Progress,
            message: message.into(),
            current: Some(current),
            total: Some(total),
            percentage: Some(pct),
            elapsed_secs: Some(elapsed_secs),
            eta_secs,
            speed,
        }
    }

    /// Create a complete event.
    #[must_use]
    pub fn complete(message: impl Into<String>, elapsed: Duration) -> Self {
        Self {
            event_type: ProgressEventType::Complete,
            message: message.into(),
            current: None,
            total: None,
            percentage: Some(100),
            elapsed_secs: Some(elapsed.as_secs_f64()),
            eta_secs: None,
            speed: None,
        }
    }

    /// Emit this event to stderr as JSON.
    pub fn emit(&self) {
        if let Ok(json) = serde_json::to_string(self) {
            eprintln!("{json}");
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Get the terminal width, defaulting to 80.
fn terminal_width() -> usize {
    crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(80)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_bar_new() {
        let bar = ProgressBar::new(100);
        assert_eq!(bar.total, 100);
        assert!(!bar.is_finished());
    }

    #[test]
    fn test_progress_bar_update() {
        let mut bar = ProgressBar::new(100).force_agent_mode(true);
        bar.update(50);
        assert!((bar.progress() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_progress_bar_advance() {
        let mut bar = ProgressBar::new(100).force_agent_mode(true);
        bar.advance(25);
        bar.advance(25);
        assert!((bar.progress() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_progress_bar_finish() {
        let mut bar = ProgressBar::new(100).force_agent_mode(true);
        bar.finish();
        assert!(bar.is_finished());
        assert!((bar.progress() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_spinner_new() {
        let spinner = Spinner::new();
        assert!(spinner.is_running());
    }

    #[test]
    fn test_spinner_tick() {
        let mut spinner = Spinner::new().force_agent_mode(true);
        let frame1 = spinner.current_frame().to_string();
        spinner.tick();
        let frame2 = spinner.current_frame().to_string();
        // Frames should differ after tick (unless only 1 frame)
        if spinner.frames.len() > 1 {
            assert_ne!(frame1, frame2);
        }
    }

    #[test]
    fn test_spinner_stop() {
        let mut spinner = Spinner::new().force_agent_mode(true);
        assert!(spinner.is_running());
        spinner.stop();
        assert!(!spinner.is_running());
    }

    #[test]
    fn test_multi_progress() {
        let multi = MultiProgress::new();
        assert!(multi.is_empty());

        let _handle1 = multi.add(ProgressBar::new(100).force_agent_mode(true));
        let _handle2 = multi.add(ProgressBar::new(50).force_agent_mode(true));

        assert_eq!(multi.len(), 2);
    }

    #[test]
    fn test_bar_style_conversion() {
        assert_eq!(
            BarStyle::Block.to_rich_style(),
            rich_rust::renderables::progress::BarStyle::Block
        );
        assert_eq!(
            BarStyle::Ascii.to_rich_style(),
            rich_rust::renderables::progress::BarStyle::Ascii
        );
    }

    #[test]
    fn test_spinner_style_frames() {
        let frames = SpinnerStyle::Simple.frames();
        assert_eq!(frames, vec!["|", "/", "-", "\\"]);
    }

    #[test]
    fn test_progress_event_start() {
        let event = ProgressEvent::start("Test", Some(100));
        assert_eq!(event.event_type, ProgressEventType::Start);
        assert_eq!(event.message, "Test");
        assert_eq!(event.total, Some(100));
    }

    #[test]
    fn test_progress_event_progress() {
        let event = ProgressEvent::progress("Test", 50, 100, Duration::from_secs(5));
        assert_eq!(event.event_type, ProgressEventType::Progress);
        assert_eq!(event.percentage, Some(50));
        assert!(event.speed.is_some());
    }

    #[test]
    fn test_progress_event_complete() {
        let event = ProgressEvent::complete("Done", Duration::from_secs(10));
        assert_eq!(event.event_type, ProgressEventType::Complete);
        assert_eq!(event.percentage, Some(100));
    }

    #[test]
    fn test_progress_guard() {
        let mut guard = ProgressGuard::new("Test", 10);
        guard.update(5);
        assert!((guard.bar().progress() - 0.5).abs() < f64::EPSILON);
    }
}
