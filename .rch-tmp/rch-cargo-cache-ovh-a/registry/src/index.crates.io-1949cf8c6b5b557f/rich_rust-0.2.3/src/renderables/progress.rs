//! Progress bar renderable.
//!
//! This module provides progress bar components for displaying task progress
//! in the terminal with various styles and features.

use crate::cells;
use crate::console::{Console, ConsoleOptions};
use crate::filesize::{self, SizeUnit, binary, binary_speed, decimal, decimal_speed};
use crate::renderables::Renderable;
use crate::segment::Segment;
use crate::style::Style;
use crate::text::Text;
use std::time::{Duration, Instant};

/// Bar style variants for the progress bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BarStyle {
    /// Standard ASCII bar using # and -
    Ascii,
    /// Unicode block characters (█▓░)
    #[default]
    Block,
    /// Line style (━╺)
    Line,
    /// Dots style (●○)
    Dots,
    /// Shaded gradient style (█▇▆▅▄▃▂▁░)
    Gradient,
}

impl BarStyle {
    /// Get the completed character for this style.
    #[must_use]
    pub const fn completed_char(&self) -> &'static str {
        match self {
            Self::Ascii => "#",
            Self::Block => "\u{2588}",    // █
            Self::Line => "\u{2501}",     // ━
            Self::Dots => "\u{25CF}",     // ●
            Self::Gradient => "\u{2588}", // █
        }
    }

    /// Get the remaining character for this style.
    #[must_use]
    pub const fn remaining_char(&self) -> &'static str {
        match self {
            Self::Ascii => "-",
            Self::Block => "\u{2591}",    // ░
            Self::Line => "\u{2501}",     // ━
            Self::Dots => "\u{25CB}",     // ○
            Self::Gradient => "\u{2591}", // ░
        }
    }

    /// Get the pulse character for this style (edge of completion).
    #[must_use]
    pub const fn pulse_char(&self) -> &'static str {
        match self {
            Self::Ascii => ">",
            Self::Block => "\u{2593}",    // ▓
            Self::Line => "\u{257A}",     // ╺
            Self::Dots => "\u{25CF}",     // ●
            Self::Gradient => "\u{2593}", // ▓
        }
    }
}

/// Spinner animation frames.
#[derive(Debug, Clone)]
pub struct Spinner {
    /// Animation frames.
    frames: Vec<&'static str>,
    /// Current frame index.
    frame_index: usize,
    /// Style for the spinner.
    style: Style,
}

impl Default for Spinner {
    fn default() -> Self {
        Self::dots()
    }
}

impl Spinner {
    /// Create a dots spinner (⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏).
    #[must_use]
    pub fn dots() -> Self {
        Self {
            frames: vec!["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
            frame_index: 0,
            style: Style::new(),
        }
    }

    /// Create a line spinner (⎺⎻⎼⎽⎼⎻).
    #[must_use]
    pub fn line() -> Self {
        Self {
            frames: vec!["⎺", "⎻", "⎼", "⎽", "⎼", "⎻"],
            frame_index: 0,
            style: Style::new(),
        }
    }

    /// Create a simple spinner (|/-\).
    #[must_use]
    pub fn simple() -> Self {
        Self {
            frames: vec!["|", "/", "-", "\\"],
            frame_index: 0,
            style: Style::new(),
        }
    }

    /// Create a bouncing ball spinner (⠁⠂⠄⠂).
    #[must_use]
    pub fn bounce() -> Self {
        Self {
            frames: vec!["⠁", "⠂", "⠄", "⠂"],
            frame_index: 0,
            style: Style::new(),
        }
    }

    /// Create a growing dots spinner (⣾⣽⣻⢿⡿⣟⣯⣷).
    #[must_use]
    pub fn growing() -> Self {
        Self {
            frames: vec!["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"],
            frame_index: 0,
            style: Style::new(),
        }
    }

    /// Create a moon phase spinner (🌑🌒🌓🌔🌕🌖🌗🌘).
    #[must_use]
    pub fn moon() -> Self {
        Self {
            frames: vec!["🌑", "🌒", "🌓", "🌔", "🌕", "🌖", "🌗", "🌘"],
            frame_index: 0,
            style: Style::new(),
        }
    }

    /// Create a clock spinner (🕐🕑🕒🕓🕔🕕🕖🕗🕘🕙🕚🕛).
    #[must_use]
    pub fn clock() -> Self {
        Self {
            frames: vec![
                "🕐", "🕑", "🕒", "🕓", "🕔", "🕕", "🕖", "🕗", "🕘", "🕙", "🕚", "🕛",
            ],
            frame_index: 0,
            style: Style::new(),
        }
    }

    /// Create a spinner from custom frames.
    #[must_use]
    pub fn custom(frames: Vec<&'static str>) -> Self {
        Self {
            frames,
            frame_index: 0,
            style: Style::new(),
        }
    }

    /// Set the spinner style.
    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Advance to the next frame and return the current frame.
    pub fn next_frame(&mut self) -> &'static str {
        if self.frames.is_empty() {
            return " ";
        }
        let frame = self.frames[self.frame_index];
        self.frame_index = (self.frame_index + 1) % self.frames.len();
        frame
    }

    /// Get the current frame without advancing.
    #[must_use]
    pub fn current_frame(&self) -> &'static str {
        if self.frames.is_empty() {
            return " ";
        }
        self.frames[self.frame_index]
    }

    /// Render the current spinner frame as a segment.
    #[must_use]
    pub fn render(&self) -> Segment<'static> {
        Segment::new(self.current_frame(), Some(self.style.clone()))
    }
}

/// A progress bar with percentage, ETA, and customizable appearance.
#[derive(Debug, Clone)]
pub struct ProgressBar {
    /// Current progress (0.0 - 1.0).
    completed: f64,
    /// Total expected count (for ETA calculation).
    total: Option<u64>,
    /// Current count (for ETA calculation).
    current: u64,
    /// Bar width in cells.
    width: usize,
    /// Bar style.
    bar_style: BarStyle,
    /// Style for completed portion.
    completed_style: Style,
    /// Style for remaining portion.
    remaining_style: Style,
    /// Style for the pulse character.
    pulse_style: Style,
    /// Show percentage.
    show_percentage: bool,
    /// Show ETA.
    show_eta: bool,
    /// Show elapsed time.
    show_elapsed: bool,
    /// Show speed (items/sec).
    show_speed: bool,
    /// Task description.
    description: Option<Text>,
    /// Start time for ETA calculation.
    start_time: Option<Instant>,
    /// Whether to show brackets around the bar.
    show_brackets: bool,
    /// Finished message (replaces bar when complete).
    finished_message: Option<String>,
    /// Whether the task is complete.
    is_finished: bool,
    /// Total bytes for file transfer (optional).
    total_bytes: Option<u64>,
    /// Bytes transferred so far.
    transferred_bytes: u64,
    /// Show file size (current/total).
    show_file_size: bool,
    /// Show transfer speed (bytes/sec).
    show_transfer_speed: bool,
    /// Use binary (1024-based) units for file sizes, or decimal (1000-based).
    use_binary_units: bool,
}

impl Default for ProgressBar {
    fn default() -> Self {
        Self {
            completed: 0.0,
            total: None,
            current: 0,
            width: 40,
            bar_style: BarStyle::default(),
            completed_style: Style::new().color_str("green").unwrap_or_default(),
            remaining_style: Style::new().color_str("bright_black").unwrap_or_default(),
            pulse_style: Style::new().color_str("cyan").unwrap_or_default(),
            show_percentage: true,
            show_eta: false,
            show_elapsed: false,
            show_speed: false,
            description: None,
            start_time: None,
            show_brackets: true,
            finished_message: None,
            is_finished: false,
            total_bytes: None,
            transferred_bytes: 0,
            show_file_size: false,
            show_transfer_speed: false,
            use_binary_units: false,
        }
    }
}

impl ProgressBar {
    /// Create a new progress bar.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a progress bar with a known total.
    #[must_use]
    pub fn with_total(total: u64) -> Self {
        Self {
            total: Some(total),
            show_eta: true,
            start_time: Some(Instant::now()),
            ..Self::default()
        }
    }

    /// Set the bar width.
    #[must_use]
    pub fn width(mut self, width: usize) -> Self {
        self.width = width;
        self
    }

    /// Set the bar style.
    #[must_use]
    pub fn bar_style(mut self, style: BarStyle) -> Self {
        self.bar_style = style;
        self
    }

    /// Set the completed portion style.
    #[must_use]
    pub fn completed_style(mut self, style: Style) -> Self {
        self.completed_style = style;
        self
    }

    /// Set the remaining portion style.
    #[must_use]
    pub fn remaining_style(mut self, style: Style) -> Self {
        self.remaining_style = style;
        self
    }

    /// Set the pulse character style.
    #[must_use]
    pub fn pulse_style(mut self, style: Style) -> Self {
        self.pulse_style = style;
        self
    }

    /// Set whether to show percentage.
    #[must_use]
    pub fn show_percentage(mut self, show: bool) -> Self {
        self.show_percentage = show;
        self
    }

    /// Set whether to show ETA.
    #[must_use]
    pub fn show_eta(mut self, show: bool) -> Self {
        self.show_eta = show;
        if show && self.start_time.is_none() {
            self.start_time = Some(Instant::now());
        }
        self
    }

    /// Set whether to show elapsed time.
    #[must_use]
    pub fn show_elapsed(mut self, show: bool) -> Self {
        self.show_elapsed = show;
        if show && self.start_time.is_none() {
            self.start_time = Some(Instant::now());
        }
        self
    }

    /// Set whether to show speed.
    #[must_use]
    pub fn show_speed(mut self, show: bool) -> Self {
        self.show_speed = show;
        if show && self.start_time.is_none() {
            self.start_time = Some(Instant::now());
        }
        self
    }

    /// Set the task description.
    ///
    /// Passing a `&str` uses `Text::new()` and does **NOT** parse markup.
    /// For styled descriptions, pass a pre-styled `Text` (e.g. from
    /// [`crate::markup::render_or_plain`]).
    #[must_use]
    pub fn description(mut self, desc: impl Into<Text>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set whether to show brackets around the bar.
    #[must_use]
    pub fn show_brackets(mut self, show: bool) -> Self {
        self.show_brackets = show;
        self
    }

    /// Set the finished message.
    ///
    /// This takes a plain string and does **NOT** parse markup.
    #[must_use]
    pub fn finished_message(mut self, msg: impl Into<String>) -> Self {
        self.finished_message = Some(msg.into());
        self
    }

    /// Update progress directly (0.0 - 1.0).
    pub fn set_progress(&mut self, progress: f64) {
        self.completed = progress.clamp(0.0, 1.0);
        if self.completed >= 1.0 {
            self.is_finished = true;
        }
    }

    /// Update progress with current/total counts.
    pub fn update(&mut self, current: u64) {
        self.current = current;
        if let Some(total) = self.total
            && total > 0
        {
            #[allow(clippy::cast_precision_loss)]
            {
                self.completed = (current as f64) / (total as f64);
            }
            self.completed = self.completed.clamp(0.0, 1.0);
        }
        if self.completed >= 1.0 {
            self.is_finished = true;
        }
    }

    /// Advance progress by a delta.
    pub fn advance(&mut self, delta: u64) {
        self.update(self.current + delta);
    }

    /// Mark the progress bar as finished.
    pub fn finish(&mut self) {
        self.completed = 1.0;
        self.is_finished = true;
    }

    /// Get the current progress (0.0 - 1.0).
    #[must_use]
    pub fn progress(&self) -> f64 {
        self.completed
    }

    /// Check if the progress bar is finished.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.is_finished
    }

    /// Get the elapsed time since start.
    #[must_use]
    pub fn elapsed(&self) -> Option<Duration> {
        self.start_time.map(|start| start.elapsed())
    }

    /// Calculate estimated time remaining.
    #[must_use]
    pub fn eta(&self) -> Option<Duration> {
        if self.completed <= 0.0 || self.completed >= 1.0 {
            return None;
        }

        let elapsed = self.elapsed()?;
        let elapsed_secs = elapsed.as_secs_f64();
        if elapsed_secs < 0.1 {
            return None; // Not enough data
        }

        let remaining_ratio = (1.0 - self.completed) / self.completed;
        let eta_secs = elapsed_secs * remaining_ratio;

        Some(Duration::from_secs_f64(eta_secs))
    }

    /// Calculate items per second.
    #[must_use]
    pub fn speed(&self) -> Option<f64> {
        let elapsed = self.elapsed()?;
        let elapsed_secs = elapsed.as_secs_f64();
        if elapsed_secs < 0.1 {
            return None;
        }

        #[allow(clippy::cast_precision_loss)]
        Some((self.current as f64) / elapsed_secs)
    }

    // -------------------------------------------------------------------------
    // File Size / Transfer Progress Methods
    // -------------------------------------------------------------------------

    /// Create a progress bar for file transfers with a known total size.
    ///
    /// This automatically enables file size display and transfer speed.
    ///
    /// # Example
    ///
    /// ```rust
    /// use rich_rust::renderables::ProgressBar;
    ///
    /// let mut bar = ProgressBar::for_download(1_000_000); // 1 MB download
    /// bar.update_bytes(500_000); // 500 KB transferred
    /// ```
    #[must_use]
    pub fn for_download(total_bytes: u64) -> Self {
        Self {
            total_bytes: Some(total_bytes),
            total: Some(total_bytes),
            show_file_size: true,
            show_transfer_speed: true,
            show_percentage: true,
            show_eta: true,
            start_time: Some(Instant::now()),
            ..Self::default()
        }
    }

    /// Set the total bytes for file transfer progress.
    #[must_use]
    pub fn total_bytes(mut self, bytes: u64) -> Self {
        self.total_bytes = Some(bytes);
        self.total = Some(bytes);
        self
    }

    /// Set whether to show file size (current/total bytes).
    #[must_use]
    pub fn show_file_size(mut self, show: bool) -> Self {
        self.show_file_size = show;
        self
    }

    /// Set whether to show transfer speed (bytes/sec).
    #[must_use]
    pub fn show_transfer_speed(mut self, show: bool) -> Self {
        self.show_transfer_speed = show;
        if show && self.start_time.is_none() {
            self.start_time = Some(Instant::now());
        }
        self
    }

    /// Set whether to use binary (1024-based: KiB, MiB) or decimal (1000-based: KB, MB) units.
    ///
    /// By default, decimal units are used.
    #[must_use]
    pub fn use_binary_units(mut self, use_binary: bool) -> Self {
        self.use_binary_units = use_binary;
        self
    }

    /// Update the transferred bytes and recalculate progress.
    pub fn update_bytes(&mut self, bytes: u64) {
        self.transferred_bytes = bytes;
        self.current = bytes;
        if let Some(total) = self.total_bytes
            && total > 0
        {
            #[allow(clippy::cast_precision_loss)]
            {
                self.completed = (bytes as f64) / (total as f64);
            }
            self.completed = self.completed.clamp(0.0, 1.0);
        }
        if self.completed >= 1.0 {
            self.is_finished = true;
        }
    }

    /// Advance the transferred bytes by a delta.
    pub fn advance_bytes(&mut self, delta: u64) {
        self.update_bytes(self.transferred_bytes + delta);
    }

    /// Get the current transferred bytes.
    #[must_use]
    pub fn transferred_bytes(&self) -> u64 {
        self.transferred_bytes
    }

    /// Get the total bytes (if set).
    #[must_use]
    pub fn total_bytes_value(&self) -> Option<u64> {
        self.total_bytes
    }

    /// Calculate transfer speed in bytes per second.
    #[must_use]
    pub fn transfer_speed(&self) -> Option<f64> {
        let elapsed = self.elapsed()?;
        let elapsed_secs = elapsed.as_secs_f64();
        if elapsed_secs < 0.1 {
            return None;
        }

        #[allow(clippy::cast_precision_loss)]
        Some((self.transferred_bytes as f64) / elapsed_secs)
    }

    /// Format the current file size as a human-readable string.
    #[must_use]
    pub fn format_file_size(&self) -> String {
        if self.use_binary_units {
            binary(self.transferred_bytes)
        } else {
            decimal(self.transferred_bytes)
        }
    }

    /// Format the total file size as a human-readable string.
    #[must_use]
    pub fn format_total_size(&self) -> Option<String> {
        self.total_bytes.map(|total| {
            if self.use_binary_units {
                binary(total)
            } else {
                decimal(total)
            }
        })
    }

    /// Format the transfer speed as a human-readable string.
    #[must_use]
    pub fn format_transfer_speed(&self) -> Option<String> {
        self.transfer_speed().map(|speed| {
            if self.use_binary_units {
                binary_speed(speed)
            } else {
                decimal_speed(speed)
            }
        })
    }

    /// Format a duration as a human-readable string.
    #[must_use]
    fn format_duration(duration: Duration) -> String {
        let total_secs = duration.as_secs();
        if total_secs < 60 {
            format!("{total_secs}s")
        } else if total_secs < 3600 {
            let mins = total_secs / 60;
            let secs = total_secs % 60;
            format!("{mins}:{secs:02}")
        } else {
            let hours = total_secs / 3600;
            let mins = (total_secs % 3600) / 60;
            let secs = total_secs % 60;
            format!("{hours}:{mins:02}:{secs:02}")
        }
    }

    /// Render the progress bar to segments for a given width.
    #[must_use]
    pub fn render(&self, available_width: usize) -> Vec<Segment<'static>> {
        let mut segments = Vec::new();

        // If finished and has a finished message, show that
        if self.is_finished
            && let Some(ref msg) = self.finished_message
        {
            let style = Style::new().color_str("green").unwrap_or_default();
            segments.push(Segment::new(format!("✓ {msg}"), Some(style)));
            segments.push(Segment::line());
            return segments;
        }

        // Description
        let mut used_width = 0;
        if let Some(ref desc) = self.description {
            let mut desc_text = desc.clone();
            desc_text.append(" ");
            let desc_width = desc_text.cell_len();
            segments.extend(
                desc_text
                    .render("")
                    .into_iter()
                    .map(super::super::segment::Segment::into_owned),
            );
            used_width += desc_width;
        }

        // Calculate bar width
        let mut suffix_parts: Vec<String> = Vec::new();

        if self.show_percentage {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let pct = (self.completed * 100.0) as u32;
            suffix_parts.push(format!("{pct:3}%"));
        }

        if self.show_elapsed
            && let Some(elapsed) = self.elapsed()
        {
            suffix_parts.push(Self::format_duration(elapsed));
        }

        if self.show_eta
            && !self.is_finished
            && let Some(eta) = self.eta()
        {
            suffix_parts.push(format!("ETA {}", Self::format_duration(eta)));
        }

        if self.show_speed
            && let Some(speed) = self.speed()
        {
            if speed >= 1.0 {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let speed_int = speed as u64;
                suffix_parts.push(format!("{speed_int}/s"));
            } else {
                suffix_parts.push(format!("{speed:.2}/s"));
            }
        }

        // File size display (e.g., "1.5 MB / 10.0 MB")
        if self.show_file_size {
            let current_size = self.format_file_size();
            if let Some(total_size) = self.format_total_size() {
                suffix_parts.push(format!("{current_size}/{total_size}"));
            } else {
                suffix_parts.push(current_size);
            }
        }

        // Transfer speed display (e.g., "1.5 MB/s")
        if self.show_transfer_speed
            && let Some(speed_str) = self.format_transfer_speed()
        {
            suffix_parts.push(speed_str);
        }

        let suffix = if suffix_parts.is_empty() {
            String::new()
        } else {
            format!(" {}", suffix_parts.join(" "))
        };
        let suffix_width = cells::cell_len(&suffix);

        let bracket_width = if self.show_brackets { 2 } else { 0 };
        let bar_width = available_width
            .saturating_sub(used_width)
            .saturating_sub(suffix_width)
            .saturating_sub(bracket_width)
            .min(self.width);

        if bar_width < 3 {
            // Not enough space for a bar, just show percentage
            if self.show_percentage {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let pct = (self.completed * 100.0) as u32;
                segments.push(Segment::new(format!("{pct}%"), None));
            }
            segments.push(Segment::line());
            return segments;
        }

        // Render the bar
        if self.show_brackets {
            segments.push(Segment::new("[", None));
        }

        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let completed_width = ((self.completed * bar_width as f64).floor() as usize).min(bar_width);
        let remaining_width = bar_width.saturating_sub(completed_width);

        // Completed portion
        if completed_width > 0 {
            let completed_chars = self.bar_style.completed_char().repeat(completed_width);
            segments.push(Segment::new(
                completed_chars,
                Some(self.completed_style.clone()),
            ));
        }

        // Pulse character (at the edge)
        // Show pulse if we have remaining space and we are active (progress > 0 and < 1)
        // or if we have calculated some completion but still have space.
        let show_pulse = remaining_width > 0 && self.completed > 0.0 && self.completed < 1.0;

        if show_pulse {
            // Replace first remaining char with pulse
            let remaining_after_pulse = remaining_width.saturating_sub(1);
            segments.push(Segment::new(
                self.bar_style.pulse_char(),
                Some(self.pulse_style.clone()),
            ));

            if remaining_after_pulse > 0 {
                let remaining_chars = self
                    .bar_style
                    .remaining_char()
                    .repeat(remaining_after_pulse);
                segments.push(Segment::new(
                    remaining_chars,
                    Some(self.remaining_style.clone()),
                ));
            }
        } else if remaining_width > 0 {
            let remaining_chars = self.bar_style.remaining_char().repeat(remaining_width);
            segments.push(Segment::new(
                remaining_chars,
                Some(self.remaining_style.clone()),
            ));
        }

        if self.show_brackets {
            segments.push(Segment::new("]", None));
        }

        // Suffix (percentage, ETA, etc.)
        if !suffix.is_empty() {
            segments.push(Segment::new(suffix, None));
        }

        segments.push(Segment::line());
        segments
    }

    /// Render the progress bar as a plain string.
    #[must_use]
    pub fn render_plain(&self, width: usize) -> String {
        self.render(width)
            .into_iter()
            .map(|seg| seg.text.into_owned())
            .collect()
    }
}

impl Renderable for ProgressBar {
    fn render<'a>(&'a self, _console: &Console, options: &ConsoleOptions) -> Vec<Segment<'a>> {
        self.render(options.max_width).into_iter().collect()
    }
}

/// Create an ASCII-style progress bar.
#[must_use]
pub fn ascii_bar() -> ProgressBar {
    ProgressBar::new().bar_style(BarStyle::Ascii)
}

/// Create a line-style progress bar.
#[must_use]
pub fn line_bar() -> ProgressBar {
    ProgressBar::new().bar_style(BarStyle::Line)
}

/// Create a dots-style progress bar.
#[must_use]
pub fn dots_bar() -> ProgressBar {
    ProgressBar::new().bar_style(BarStyle::Dots)
}

/// Create a gradient-style progress bar.
#[must_use]
pub fn gradient_bar() -> ProgressBar {
    ProgressBar::new().bar_style(BarStyle::Gradient)
}

// =============================================================================
// Standalone File Size and Transfer Speed Columns
// =============================================================================

/// A renderable that displays a file size in human-readable format.
#[derive(Debug, Clone)]
pub struct FileSizeColumn {
    size: u64,
    unit: SizeUnit,
    precision: usize,
    style: Style,
}

impl FileSizeColumn {
    #[must_use]
    pub fn new(size: u64) -> Self {
        Self {
            size,
            unit: SizeUnit::Decimal,
            precision: 1,
            style: Style::new().color_str("green").unwrap_or_default(),
        }
    }

    #[must_use]
    pub fn unit(mut self, unit: SizeUnit) -> Self {
        self.unit = unit;
        self
    }

    #[must_use]
    pub fn precision(mut self, precision: usize) -> Self {
        self.precision = precision;
        self
    }

    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn set_size(&mut self, size: u64) {
        self.size = size;
    }

    #[must_use]
    pub fn size(&self) -> u64 {
        self.size
    }

    #[must_use]
    pub fn render_plain(&self) -> String {
        #[allow(clippy::cast_possible_wrap)]
        filesize::format_size(self.size as i64, self.unit, self.precision)
    }

    #[must_use]
    pub fn render(&self) -> Vec<Segment<'static>> {
        vec![Segment::new(self.render_plain(), Some(self.style.clone()))]
    }
}

impl Default for FileSizeColumn {
    fn default() -> Self {
        Self::new(0)
    }
}

/// A renderable that displays a total file size.
#[derive(Debug, Clone)]
pub struct TotalFileSizeColumn {
    inner: FileSizeColumn,
}

impl TotalFileSizeColumn {
    #[must_use]
    pub fn new(size: u64) -> Self {
        Self {
            inner: FileSizeColumn::new(size),
        }
    }

    #[must_use]
    pub fn unit(mut self, unit: SizeUnit) -> Self {
        self.inner = self.inner.unit(unit);
        self
    }

    #[must_use]
    pub fn precision(mut self, precision: usize) -> Self {
        self.inner = self.inner.precision(precision);
        self
    }

    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.inner = self.inner.style(style);
        self
    }

    #[must_use]
    pub fn render_plain(&self) -> String {
        self.inner.render_plain()
    }

    #[must_use]
    pub fn render(&self) -> Vec<Segment<'static>> {
        self.inner.render()
    }
}

impl Default for TotalFileSizeColumn {
    fn default() -> Self {
        Self::new(0)
    }
}

/// A renderable that displays download progress as "current/total unit".
#[derive(Debug, Clone)]
pub struct DownloadColumn {
    current: u64,
    total: u64,
    unit: SizeUnit,
    precision: usize,
    current_style: Style,
    separator_style: Style,
    total_style: Style,
}

impl DownloadColumn {
    #[must_use]
    pub fn new(current: u64, total: u64) -> Self {
        let green_style = Style::new().color_str("green").unwrap_or_default();
        Self {
            current,
            total,
            unit: SizeUnit::Decimal,
            precision: 1,
            current_style: green_style.clone(),
            separator_style: Style::new(),
            total_style: green_style,
        }
    }

    #[must_use]
    pub fn unit(mut self, unit: SizeUnit) -> Self {
        self.unit = unit;
        self
    }

    #[must_use]
    pub fn precision(mut self, precision: usize) -> Self {
        self.precision = precision;
        self
    }

    #[must_use]
    pub fn current_style(mut self, style: Style) -> Self {
        self.current_style = style;
        self
    }

    #[must_use]
    pub fn total_style(mut self, style: Style) -> Self {
        self.total_style = style;
        self
    }

    pub fn set_current(&mut self, current: u64) {
        self.current = current;
    }

    pub fn set_total(&mut self, total: u64) {
        self.total = total;
    }

    #[must_use]
    pub fn current(&self) -> u64 {
        self.current
    }

    #[must_use]
    pub fn total(&self) -> u64 {
        self.total
    }

    #[must_use]
    pub fn render_plain(&self) -> String {
        #[allow(clippy::cast_possible_wrap)]
        let current_str = filesize::format_size(self.current as i64, self.unit, self.precision);
        #[allow(clippy::cast_possible_wrap)]
        let total_str = filesize::format_size(self.total as i64, self.unit, self.precision);
        let parts: Vec<&str> = total_str.rsplitn(2, ' ').collect();
        if parts.len() == 2 {
            let unit_str = parts[0];
            let total_value = parts[1];
            let current_parts: Vec<&str> = current_str.rsplitn(2, ' ').collect();
            let current_value = if current_parts.len() == 2 {
                current_parts[1]
            } else {
                &current_str
            };
            format!("{current_value}/{total_value} {unit_str}")
        } else {
            format!("{current_str}/{total_str}")
        }
    }

    #[must_use]
    pub fn render(&self) -> Vec<Segment<'static>> {
        #[allow(clippy::cast_possible_wrap)]
        let current_str = filesize::format_size(self.current as i64, self.unit, self.precision);
        #[allow(clippy::cast_possible_wrap)]
        let total_str = filesize::format_size(self.total as i64, self.unit, self.precision);
        let parts: Vec<&str> = total_str.rsplitn(2, ' ').collect();
        if parts.len() == 2 {
            let unit_str = parts[0];
            let total_value = parts[1];
            let current_parts: Vec<&str> = current_str.rsplitn(2, ' ').collect();
            let current_value = if current_parts.len() == 2 {
                current_parts[1]
            } else {
                &current_str
            };
            vec![
                Segment::new(current_value.to_string(), Some(self.current_style.clone())),
                Segment::new("/", Some(self.separator_style.clone())),
                Segment::new(
                    format!("{total_value} {unit_str}"),
                    Some(self.total_style.clone()),
                ),
            ]
        } else {
            vec![
                Segment::new(current_str, Some(self.current_style.clone())),
                Segment::new("/", Some(self.separator_style.clone())),
                Segment::new(total_str, Some(self.total_style.clone())),
            ]
        }
    }
}

impl Default for DownloadColumn {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

/// A renderable that displays a transfer speed in human-readable format.
#[derive(Debug, Clone)]
pub struct TransferSpeedColumn {
    speed: f64,
    unit: SizeUnit,
    precision: usize,
    style: Style,
}

impl TransferSpeedColumn {
    #[must_use]
    pub fn new(speed: f64) -> Self {
        Self {
            speed,
            unit: SizeUnit::Decimal,
            precision: 1,
            style: Style::new().color_str("red").unwrap_or_default(),
        }
    }

    #[must_use]
    pub fn from_transfer(bytes: u64, duration: Duration) -> Self {
        let secs = duration.as_secs_f64();
        #[allow(clippy::cast_precision_loss)]
        let speed = if secs > 0.0 { bytes as f64 / secs } else { 0.0 };
        Self::new(speed)
    }

    #[must_use]
    pub fn unit(mut self, unit: SizeUnit) -> Self {
        self.unit = unit;
        self
    }

    #[must_use]
    pub fn precision(mut self, precision: usize) -> Self {
        self.precision = precision;
        self
    }

    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn set_speed(&mut self, speed: f64) {
        self.speed = speed;
    }

    pub fn update_from_transfer(&mut self, bytes: u64, duration: Duration) {
        let secs = duration.as_secs_f64();
        #[allow(clippy::cast_precision_loss)]
        {
            self.speed = if secs > 0.0 { bytes as f64 / secs } else { 0.0 };
        }
    }

    #[must_use]
    pub fn speed(&self) -> f64 {
        self.speed
    }

    #[must_use]
    pub fn render_plain(&self) -> String {
        filesize::format_speed(self.speed, self.unit, self.precision)
    }

    #[must_use]
    pub fn render(&self) -> Vec<Segment<'static>> {
        vec![Segment::new(self.render_plain(), Some(self.style.clone()))]
    }
}

impl Default for TransferSpeedColumn {
    fn default() -> Self {
        Self::new(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Attributes;

    #[test]
    fn test_progress_bar_new() {
        let bar = ProgressBar::new();
        assert!((bar.progress() - 0.0).abs() < f64::EPSILON);
        assert!(!bar.is_finished());
    }

    #[test]
    fn test_progress_bar_with_total() {
        let mut bar = ProgressBar::with_total(100);
        bar.update(50);
        assert!((bar.progress() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_progress_bar_set_progress() {
        let mut bar = ProgressBar::new();
        bar.set_progress(0.75);
        assert!((bar.progress() - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn test_progress_bar_advance() {
        let mut bar = ProgressBar::with_total(10);
        bar.advance(3);
        assert!((bar.progress() - 0.3).abs() < f64::EPSILON);
        bar.advance(2);
        assert!((bar.progress() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_progress_bar_finish() {
        let mut bar = ProgressBar::new();
        bar.finish();
        assert!(bar.is_finished());
        assert!((bar.progress() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_progress_bar_render() {
        let mut bar = ProgressBar::new().width(20).show_brackets(true);
        bar.set_progress(0.5);
        let segments = bar.render(80);
        assert_ne!(segments.len(), 0);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains('['));
        assert!(text.contains(']'));
        assert!(text.contains('%'));
    }

    #[test]
    fn test_progress_bar_render_plain() {
        let mut bar = ProgressBar::new().width(10).show_brackets(false);
        bar.set_progress(0.5);
        let plain = bar.render_plain(40);
        assert_ne!(plain, "");
    }

    #[test]
    fn test_progress_bar_styles() {
        for style in [
            BarStyle::Ascii,
            BarStyle::Block,
            BarStyle::Line,
            BarStyle::Dots,
        ] {
            let mut bar = ProgressBar::new().bar_style(style).width(10);
            bar.set_progress(0.5);
            let segments = bar.render(40);
            assert_ne!(segments.len(), 0);
        }
    }

    #[test]
    fn test_progress_bar_with_description() {
        let mut bar = ProgressBar::new().description("Downloading").width(20);
        bar.set_progress(0.5);
        let plain = bar.render_plain(80);
        assert!(plain.contains("Downloading"));
    }

    #[test]
    fn test_progress_bar_description_preserves_spans() {
        let mut desc = Text::new("Download");
        desc.stylize(0, 8, Style::new().bold());
        let bar = ProgressBar::new().description(desc).width(20);
        let segments = bar.render(80);
        let has_bold = segments.iter().any(|seg| {
            seg.text.contains("Download")
                && seg
                    .style
                    .as_ref()
                    .is_some_and(|style| style.attributes.contains(Attributes::BOLD))
        });
        assert!(has_bold, "description should preserve span styles");
    }

    #[test]
    fn test_progress_bar_finished_message() {
        let mut bar = ProgressBar::new().finished_message("Done!").width(20);
        bar.finish();
        let plain = bar.render_plain(80);
        assert!(plain.contains("Done!"));
        assert!(plain.contains('✓'));
    }

    #[test]
    fn test_spinner_next_frame() {
        let mut spinner = Spinner::simple();
        assert_eq!(spinner.next_frame(), "|");
        assert_eq!(spinner.next_frame(), "/");
        assert_eq!(spinner.next_frame(), "-");
        assert_eq!(spinner.next_frame(), "\\");
        assert_eq!(spinner.next_frame(), "|"); // Wraps around
    }

    #[test]
    fn test_spinner_current_frame() {
        let spinner = Spinner::simple();
        assert_eq!(spinner.current_frame(), "|");
        assert_eq!(spinner.current_frame(), "|"); // Doesn't advance
    }

    #[test]
    fn test_spinner_render() {
        let spinner = Spinner::dots();
        let segment = spinner.render();
        assert!(!segment.text.is_empty());
    }

    #[test]
    fn test_bar_style_chars() {
        assert_eq!(BarStyle::Ascii.completed_char(), "#");
        assert_eq!(BarStyle::Ascii.remaining_char(), "-");
        assert_eq!(BarStyle::Block.completed_char(), "\u{2588}");
        assert_eq!(BarStyle::Block.remaining_char(), "\u{2591}");
    }

    #[test]
    fn test_ascii_bar() {
        let mut bar = ascii_bar();
        bar.set_progress(0.5);
        let plain = bar.render_plain(40);
        assert!(plain.contains('#'));
        assert!(plain.contains('-'));
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(ProgressBar::format_duration(Duration::from_secs(30)), "30s");
        assert_eq!(
            ProgressBar::format_duration(Duration::from_secs(90)),
            "1:30"
        );
        assert_eq!(
            ProgressBar::format_duration(Duration::from_secs(3661)),
            "1:01:01"
        );
    }

    #[test]
    fn test_progress_clamp() {
        let mut bar = ProgressBar::new();
        bar.set_progress(-0.5);
        assert!((bar.progress() - 0.0).abs() < f64::EPSILON);
        bar.set_progress(1.5);
        assert!((bar.progress() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_update_clamps_progress() {
        let mut bar = ProgressBar::with_total(10);
        bar.update(15);
        assert!((bar.progress() - 1.0).abs() < f64::EPSILON);
        assert!(bar.is_finished());
    }

    // -------------------------------------------------------------------------
    // File Size / Transfer Progress Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_for_download() {
        let bar = ProgressBar::for_download(1_000_000);
        assert_eq!(bar.total_bytes_value(), Some(1_000_000));
        assert!(bar.show_file_size);
        assert!(bar.show_transfer_speed);
    }

    #[test]
    fn test_update_bytes() {
        let mut bar = ProgressBar::for_download(1_000_000);
        bar.update_bytes(500_000);
        assert_eq!(bar.transferred_bytes(), 500_000);
        assert!((bar.progress() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_advance_bytes() {
        let mut bar = ProgressBar::for_download(1_000_000);
        bar.advance_bytes(250_000);
        bar.advance_bytes(250_000);
        assert_eq!(bar.transferred_bytes(), 500_000);
        assert!((bar.progress() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_format_file_size_decimal() {
        let mut bar = ProgressBar::for_download(10_000_000);
        bar.update_bytes(1_500_000);
        assert_eq!(bar.format_file_size(), "1.5 MB");
        assert_eq!(bar.format_total_size(), Some("10.0 MB".to_string()));
    }

    #[test]
    fn test_format_file_size_binary() {
        let mut bar = ProgressBar::for_download(10_485_760) // 10 MiB
            .use_binary_units(true);
        bar.update_bytes(1_572_864); // 1.5 MiB
        assert_eq!(bar.format_file_size(), "1.5 MiB");
        assert_eq!(bar.format_total_size(), Some("10.0 MiB".to_string()));
    }

    #[test]
    fn test_render_with_file_size() {
        let mut bar = ProgressBar::for_download(10_000_000)
            .width(20)
            .show_percentage(false)
            .show_eta(false);
        bar.update_bytes(5_000_000);
        let plain = bar.render_plain(100);
        // Should contain file size info
        assert!(plain.contains("MB") || plain.contains("bytes"));
    }

    #[test]
    fn test_total_bytes_builder() {
        let bar = ProgressBar::new()
            .total_bytes(2_000_000)
            .show_file_size(true)
            .show_transfer_speed(true);
        assert_eq!(bar.total_bytes_value(), Some(2_000_000));
        assert!(bar.show_file_size);
        assert!(bar.show_transfer_speed);
    }

    #[test]
    fn test_use_binary_units() {
        let bar = ProgressBar::for_download(1024).use_binary_units(true);
        assert!(bar.use_binary_units);

        let bar_decimal = ProgressBar::for_download(1000).use_binary_units(false);
        assert!(!bar_decimal.use_binary_units);
    }

    #[test]
    fn test_download_finishes_at_100() {
        let mut bar = ProgressBar::for_download(1_000_000);
        bar.update_bytes(1_000_000);
        assert!(bar.is_finished());
        assert!((bar.progress() - 1.0).abs() < f64::EPSILON);
    }

    // =========================================================================
    // Standalone Column Tests
    // =========================================================================

    #[test]
    fn test_file_size_column_decimal() {
        let column = FileSizeColumn::new(1_500_000);
        assert_eq!(column.render_plain(), "1.5 MB");
        assert_eq!(column.size(), 1_500_000);
    }

    #[test]
    fn test_file_size_column_binary() {
        let column = FileSizeColumn::new(1_048_576).unit(SizeUnit::Binary);
        assert_eq!(column.render_plain(), "1.0 MiB");
    }

    #[test]
    fn test_file_size_column_precision() {
        let column = FileSizeColumn::new(1_234_567).precision(2);
        assert_eq!(column.render_plain(), "1.23 MB");
    }

    #[test]
    fn test_file_size_column_set_size() {
        let mut column = FileSizeColumn::new(1000);
        column.set_size(2_000_000);
        assert_eq!(column.size(), 2_000_000);
        assert_eq!(column.render_plain(), "2.0 MB");
    }

    #[test]
    fn test_total_file_size_column() {
        let column = TotalFileSizeColumn::new(10_000_000);
        assert_eq!(column.render_plain(), "10.0 MB");
    }

    #[test]
    fn test_download_column() {
        let column = DownloadColumn::new(1_500_000, 10_000_000);
        assert_eq!(column.render_plain(), "1.5/10.0 MB");
        assert_eq!(column.current(), 1_500_000);
        assert_eq!(column.total(), 10_000_000);
    }

    #[test]
    fn test_download_column_binary() {
        let column = DownloadColumn::new(1_048_576, 10_485_760).unit(SizeUnit::Binary);
        assert_eq!(column.render_plain(), "1.0/10.0 MiB");
    }

    #[test]
    fn test_download_column_update() {
        let mut column = DownloadColumn::new(0, 1000);
        column.set_current(500);
        assert_eq!(column.current(), 500);
        column.set_total(2000);
        assert_eq!(column.total(), 2000);
    }

    #[test]
    fn test_transfer_speed_column() {
        let column = TransferSpeedColumn::new(1_500_000.0);
        assert_eq!(column.render_plain(), "1.5 MB/s");
        assert!((column.speed() - 1_500_000.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_transfer_speed_column_binary() {
        let column = TransferSpeedColumn::new(1_048_576.0).unit(SizeUnit::Binary);
        assert_eq!(column.render_plain(), "1.0 MiB/s");
    }

    #[test]
    fn test_transfer_speed_from_transfer() {
        let column = TransferSpeedColumn::from_transfer(1_000_000, Duration::from_secs(1));
        assert!((column.speed() - 1_000_000.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_transfer_speed_update() {
        let mut column = TransferSpeedColumn::new(0.0);
        column.set_speed(5_000_000.0);
        assert!((column.speed() - 5_000_000.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_column_default_impls() {
        assert_eq!(FileSizeColumn::default().size(), 0);
        assert_eq!(TotalFileSizeColumn::default().render_plain(), "0 bytes");
        assert_eq!(DownloadColumn::default().current(), 0);
        assert!((TransferSpeedColumn::default().speed() - 0.0).abs() < f64::EPSILON);
    }
}
