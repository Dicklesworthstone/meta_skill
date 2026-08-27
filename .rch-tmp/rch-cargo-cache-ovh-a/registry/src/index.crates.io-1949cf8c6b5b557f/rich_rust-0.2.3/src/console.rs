//! Console - the central entry point for styled terminal output.
//!
//! The [`Console`] handles rendering styled content to the terminal,
//! including color detection, width calculation, and ANSI code generation.
//!
//! # Examples
//!
//! ## Basic Printing with Markup
//!
//! ```rust,ignore
//! use rich_rust::Console;
//!
//! let console = Console::new();
//!
//! // Print with markup syntax
//! console.print("[bold red]Error:[/] Something went wrong");
//! console.print("[green]Success![/] Operation completed");
//!
//! // Markup supports colors, attributes, and combinations
//! console.print("[bold italic #ff8800 on blue]Custom styling[/]");
//! ```
//!
//! ## Console Builder
//!
//! ```rust,ignore
//! use rich_rust::console::{Console, ConsoleBuilder};
//! use rich_rust::color::ColorSystem;
//!
//! let console = Console::builder()
//!     .color_system(ColorSystem::EightBit)  // Force 256 colors
//!     .width(80)                            // Fixed width
//!     .markup(true)                         // Enable markup parsing
//!     .build();
//! ```
//!
//! ## Print Options
//!
//! ```rust,ignore
//! use rich_rust::console::{Console, PrintOptions};
//! use rich_rust::style::Style;
//! use rich_rust::text::JustifyMethod;
//!
//! let console = Console::new();
//!
//! let options = PrintOptions::new()
//!     .with_style(Style::new().bold())
//!     .with_justify(JustifyMethod::Center)
//!     .with_markup(true);
//!
//! console.print_with_options("Centered bold text", &options);
//! ```
//!
//! ## Capturing Output
//!
//! ```rust,ignore
//! use rich_rust::Console;
//!
//! let mut console = Console::new();
//!
//! // Start capturing
//! console.begin_capture();
//! console.print("[bold]Hello[/]");
//!
//! // Get captured segments
//! let segments = console.end_capture();
//! for seg in &segments {
//!     println!("Text: {:?}, Style: {:?}", seg.text, seg.style);
//! }
//! ```
//!
//! # Terminal Detection
//!
//! The Console automatically detects terminal capabilities:
//!
//! - **Color system**: `TrueColor` (24-bit), 256 colors, or 16 colors
//! - **Terminal dimensions**: Width and height in character cells
//! - **TTY status**: Whether output is to an interactive terminal
//!
//! You can override these with the builder pattern or by setting explicit values.

use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::io::{self, Write};
use std::sync::{
    Arc, Mutex, Weak,
    atomic::{AtomicBool, Ordering},
};
use time::OffsetDateTime;

use crate::color::{ColorSystem, DEFAULT_TERMINAL_THEME, SVG_EXPORT_THEME, TerminalTheme};
use crate::emoji;
use crate::highlighter::{Highlighter, ReprHighlighter};
use crate::live::LiveInner;
use crate::markup;
use crate::measure::{Measurement, RichMeasure};
use crate::protocol::{RichCast, RichCastOutput};
use crate::renderables::Renderable;
use crate::segment::{ControlCode, ControlType, Segment};
use crate::style::{Attributes, Style, StyleParseError};
use crate::sync::lock_recover;
use crate::terminal;
use crate::text::{JustifyMethod, OverflowMethod, Text};
use crate::theme::{Theme, ThemeStack, ThemeStackError};

/// Console dimensions in cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConsoleDimensions {
    /// Width in cells.
    pub width: usize,
    /// Height in rows.
    pub height: usize,
}

impl Default for ConsoleDimensions {
    fn default() -> Self {
        Self {
            width: 80,
            height: 24,
        }
    }
}

/// Options for rendering.
#[derive(Debug, Clone)]
pub struct ConsoleOptions {
    /// Terminal dimensions.
    pub size: ConsoleDimensions,
    /// Using legacy Windows console.
    pub legacy_windows: bool,
    /// Minimum width constraint.
    pub min_width: usize,
    /// Maximum width constraint.
    pub max_width: usize,
    /// Output is a terminal (vs file/pipe).
    pub is_terminal: bool,
    /// Output encoding.
    pub encoding: String,
    /// Maximum height for rendering.
    pub max_height: usize,
    /// Default justification.
    pub justify: Option<JustifyMethod>,
    /// Default overflow handling.
    pub overflow: Option<OverflowMethod>,
    /// Default `no_wrap` setting.
    pub no_wrap: Option<bool>,
    /// Enable highlighting.
    pub highlight: Option<bool>,
    /// Parse markup in strings.
    pub markup: Option<bool>,
    /// Explicit height override.
    pub height: Option<usize>,
}

impl Default for ConsoleOptions {
    fn default() -> Self {
        Self {
            size: ConsoleDimensions::default(),
            legacy_windows: false,
            min_width: 1,
            max_width: 80,
            is_terminal: true,
            encoding: String::from("utf-8"),
            max_height: usize::MAX,
            justify: None,
            overflow: None,
            no_wrap: None,
            highlight: None,
            markup: None,
            height: None,
        }
    }
}

impl ConsoleOptions {
    /// Create options with a different `max_width`.
    #[must_use]
    pub fn update_width(&self, width: usize) -> Self {
        Self {
            max_width: width.min(self.max_width),
            ..self.clone()
        }
    }

    /// Create options with a different height.
    #[must_use]
    pub fn update_height(&self, height: usize) -> Self {
        Self {
            height: Some(height),
            ..self.clone()
        }
    }

    /// Create options with updated width and height.
    #[must_use]
    pub fn update_dimensions(&self, width: usize, height: usize) -> Self {
        Self {
            size: ConsoleDimensions { width, height },
            max_width: width,
            max_height: height,
            height: Some(height),
            ..self.clone()
        }
    }
}

/// Print options for controlling output.
#[derive(Clone, Default)]
pub struct PrintOptions {
    /// String to separate multiple objects.
    pub sep: String,
    /// String to append at end.
    pub end: String,
    /// Apply style to output.
    pub style: Option<Style>,
    /// Override justification.
    pub justify: Option<JustifyMethod>,
    /// Override overflow handling.
    pub overflow: Option<OverflowMethod>,
    /// Override `no_wrap`.
    pub no_wrap: Option<bool>,
    /// Suppress newline.
    pub no_newline: bool,
    /// Parse markup.
    pub markup: Option<bool>,
    /// Enable/disable highlighting (None = inherit Console setting).
    pub highlight: Option<bool>,
    /// Override the highlighter used when highlighting is enabled.
    pub highlighter: Option<Arc<dyn Highlighter>>,
    /// Override width.
    pub width: Option<usize>,
    /// Crop output to width.
    pub crop: bool,
    /// Soft wrap at width.
    pub soft_wrap: bool,
}

impl PrintOptions {
    /// Create new print options with defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            sep: String::from(" "),
            end: String::from("\n"),
            ..Default::default()
        }
    }

    /// Set markup parsing.
    #[must_use]
    pub fn with_markup(mut self, markup: bool) -> Self {
        self.markup = Some(markup);
        self
    }

    /// Set style.
    #[must_use]
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }

    /// Set the separator between objects.
    #[must_use]
    pub fn with_sep(mut self, sep: impl Into<String>) -> Self {
        self.sep = sep.into();
        self
    }

    /// Set the end string appended after output.
    #[must_use]
    pub fn with_end(mut self, end: impl Into<String>) -> Self {
        self.end = end.into();
        self
    }

    /// Override justification.
    #[must_use]
    pub fn with_justify(mut self, justify: JustifyMethod) -> Self {
        self.justify = Some(justify);
        self
    }

    /// Override overflow handling.
    #[must_use]
    pub fn with_overflow(mut self, overflow: OverflowMethod) -> Self {
        self.overflow = Some(overflow);
        self
    }

    /// Override `no_wrap`.
    #[must_use]
    pub fn with_no_wrap(mut self, no_wrap: bool) -> Self {
        self.no_wrap = Some(no_wrap);
        self
    }

    /// Suppress newline at end.
    #[must_use]
    pub fn with_no_newline(mut self, no_newline: bool) -> Self {
        self.no_newline = no_newline;
        self
    }

    /// Enable/disable highlighting.
    #[must_use]
    pub fn with_highlight(mut self, highlight: bool) -> Self {
        self.highlight = Some(highlight);
        self
    }

    /// Override the highlighter for this print call.
    #[must_use]
    pub fn with_highlighter<H: Highlighter + 'static>(mut self, highlighter: H) -> Self {
        self.highlighter = Some(Arc::new(highlighter));
        self
    }

    /// Override width.
    #[must_use]
    pub fn with_width(mut self, width: usize) -> Self {
        self.width = Some(width);
        self
    }

    /// Crop output to width.
    #[must_use]
    pub fn with_crop(mut self, crop: bool) -> Self {
        self.crop = crop;
        self
    }

    /// Soft wrap at width.
    #[must_use]
    pub fn with_soft_wrap(mut self, soft_wrap: bool) -> Self {
        self.soft_wrap = soft_wrap;
        self
    }
}

impl std::fmt::Debug for PrintOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrintOptions")
            .field("sep", &self.sep)
            .field("end", &self.end)
            .field("style", &self.style)
            .field("justify", &self.justify)
            .field("overflow", &self.overflow)
            .field("no_wrap", &self.no_wrap)
            .field("no_newline", &self.no_newline)
            .field("markup", &self.markup)
            .field("highlight", &self.highlight)
            .field(
                "highlighter",
                &self.highlighter.as_ref().map(|_| "<Highlighter>"),
            )
            .field("width", &self.width)
            .field("crop", &self.crop)
            .field("soft_wrap", &self.soft_wrap)
            .finish()
    }
}

/// Hook for intercepting rendered segments before output.
pub trait RenderHook: Send + Sync {
    fn process(&self, console: &Console, segments: &[Segment<'static>]) -> Vec<Segment<'static>>;
}

/// The main Console for rendering styled output.
///
/// `Console` is the central entry point for all terminal output operations.
/// It handles color detection, terminal dimensions, markup parsing, and
/// ANSI escape code generation.
///
/// # Thread Safety
///
/// `Console` is `Send + Sync` and can be safely shared between threads using
/// `Arc<Console>`. All internal state is protected by mutexes that use poison
/// recovery (see the [`sync`](crate::sync) module).
///
/// When multiple threads print concurrently, their output may interleave at
/// the line level. For strictly ordered output, synchronize at the application
/// level or use a single printing thread.
///
/// # Example
///
/// ```rust,ignore
/// use std::sync::Arc;
/// use std::thread;
/// use rich_rust::Console;
///
/// let console = Arc::new(Console::new());
///
/// let handles: Vec<_> = (0..4).map(|i| {
///     let c = Arc::clone(&console);
///     thread::spawn(move || {
///         c.print(&format!("Hello from thread {i}"));
///     })
/// }).collect();
///
/// for h in handles {
///     h.join().unwrap();
/// }
/// ```
pub struct Console {
    /// Color system to use (None = auto-detect).
    color_system: Option<ColorSystem>,
    /// Force terminal mode.
    force_terminal: Option<bool>,
    /// Tab expansion size.
    tab_size: usize,
    /// Buffer output for export.
    record: AtomicBool,
    /// Parse markup by default.
    markup: bool,
    /// Enable emoji rendering.
    emoji: bool,
    /// Enable syntax highlighting.
    highlight: bool,
    /// Highlighter used when `highlight` is enabled (Python Rich `rich.highlighter` parity).
    highlighter: Arc<dyn Highlighter>,
    /// Theme stack for named styles (Python Rich parity).
    theme_stack: Mutex<ThemeStack>,
    /// Override width.
    width: Option<usize>,
    /// Override height.
    height: Option<usize>,
    /// Use ASCII-safe box characters.
    safe_box: bool,
    /// Output stream (defaults to stdout).
    file: Mutex<Box<dyn Write + Send>>,
    /// Recording buffer.
    buffer: Mutex<Vec<Segment<'static>>>,
    /// Cached terminal detection.
    is_terminal: bool,
    /// Detected/configured color system.
    detected_color_system: Option<ColorSystem>,
    /// Render hooks (Live uses this).
    render_hooks: Mutex<Vec<Arc<dyn RenderHook>>>,
    /// Active Live stack for nested Live handling.
    live_stack: Mutex<Vec<Weak<LiveInner>>>,
}

impl std::fmt::Debug for Console {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Console")
            .field("color_system", &self.color_system)
            .field("force_terminal", &self.force_terminal)
            .field("tab_size", &self.tab_size)
            .field("record", &self.record.load(Ordering::Relaxed))
            .field("markup", &self.markup)
            .field("emoji", &self.emoji)
            .field("highlight", &self.highlight)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("safe_box", &self.safe_box)
            .field("file", &"<dyn Write>")
            .field("buffer_len", &lock_recover(&self.buffer).len())
            .field("is_terminal", &self.is_terminal)
            .field("detected_color_system", &self.detected_color_system)
            .finish_non_exhaustive()
    }
}

impl Default for Console {
    fn default() -> Self {
        Self::new()
    }
}

impl Console {
    /// Create a new console with default settings.
    #[must_use]
    pub fn new() -> Self {
        let is_terminal = terminal::is_terminal();
        let detected_color_system = if is_terminal {
            terminal::detect_color_system()
        } else {
            None
        };

        Self {
            color_system: None,
            force_terminal: None,
            tab_size: 8,
            record: AtomicBool::new(false),
            markup: true,
            emoji: true,
            highlight: true,
            highlighter: Arc::new(ReprHighlighter::default()),
            theme_stack: Mutex::new(ThemeStack::new(Theme::default())),
            width: None,
            height: None,
            safe_box: false,
            file: Mutex::new(Box::new(io::stdout())),
            buffer: Mutex::new(Vec::new()),
            is_terminal,
            detected_color_system,
            render_hooks: Mutex::new(Vec::new()),
            live_stack: Mutex::new(Vec::new()),
        }
    }

    /// Create a console builder for custom configuration.
    #[must_use]
    pub fn builder() -> ConsoleBuilder {
        ConsoleBuilder::default()
    }

    /// Convert this Console into a shared reference-counted handle.
    #[must_use]
    pub fn shared(self) -> Arc<Self> {
        Arc::new(self)
    }

    /// Get the console width.
    #[must_use]
    pub fn width(&self) -> usize {
        self.width.unwrap_or_else(terminal::get_terminal_width)
    }

    /// Get the console height.
    #[must_use]
    pub fn height(&self) -> usize {
        self.height.unwrap_or_else(terminal::get_terminal_height)
    }

    /// Get the console dimensions.
    #[must_use]
    pub fn size(&self) -> ConsoleDimensions {
        ConsoleDimensions {
            width: self.width(),
            height: self.height(),
        }
    }

    /// Check if this console outputs to a terminal.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        self.force_terminal.unwrap_or(self.is_terminal)
    }

    /// Terminal detection result without `force_terminal` overrides.
    ///
    /// This is used for behaviors that must not affect non-TTY contexts even if
    /// a caller forces terminal rendering (e.g. process-wide stdio redirection).
    #[must_use]
    pub(crate) const fn is_terminal_detected(&self) -> bool {
        self.is_terminal
    }

    /// Get the color system in use.
    #[must_use]
    pub fn color_system(&self) -> Option<ColorSystem> {
        self.color_system.or(self.detected_color_system)
    }

    /// Check if Rich-style emoji code replacement is enabled.
    #[must_use]
    pub const fn emoji(&self) -> bool {
        self.emoji
    }

    /// Check if ASCII-safe box drawing is enabled.
    #[must_use]
    pub const fn safe_box(&self) -> bool {
        self.safe_box
    }

    /// Get a style by theme name or parse a style definition.
    ///
    /// Mirrors Python Rich `Console.get_style()`:
    /// - Check the active theme stack for an exact name match
    /// - Fall back to parsing a style definition
    ///
    /// If parsing fails, this returns an empty style.
    #[must_use]
    pub fn get_style(&self, name: &str) -> Style {
        self.try_get_style(name).unwrap_or_else(|_| Style::new())
    }

    /// Like [`Self::get_style`], but returns an error if the style can't be parsed.
    pub fn try_get_style(&self, name: &str) -> Result<Style, StyleParseError> {
        {
            let stack = lock_recover(&self.theme_stack);
            if let Some(style) = stack.get(name) {
                return Ok(style.clone());
            }
        }
        Style::parse(name)
    }

    /// Push a theme on to the theme stack.
    pub fn push_theme(&self, theme: Theme, inherit: bool) {
        lock_recover(&self.theme_stack).push_theme(theme, inherit);
    }

    /// Pop the current theme from the theme stack.
    pub fn pop_theme(&self) -> Result<(), ThemeStackError> {
        lock_recover(&self.theme_stack).pop_theme()
    }

    /// Use a theme for the duration of the returned guard.
    #[must_use]
    pub fn use_theme(&self, theme: Theme, inherit: bool) -> ThemeGuard<'_> {
        self.push_theme(theme, inherit);
        ThemeGuard { console: self }
    }

    /// Check if colors are enabled.
    #[must_use]
    pub fn is_color_enabled(&self) -> bool {
        self.color_system().is_some()
    }

    /// Get the tab size.
    #[must_use]
    pub const fn tab_size(&self) -> usize {
        self.tab_size
    }

    /// Create console options for rendering.
    #[must_use]
    pub fn options(&self) -> ConsoleOptions {
        ConsoleOptions {
            size: self.size(),
            legacy_windows: false,
            min_width: 1,
            max_width: self.width(),
            is_terminal: self.is_terminal(),
            encoding: String::from("utf-8"),
            max_height: self.height(),
            justify: None,
            overflow: None,
            no_wrap: None,
            highlight: Some(self.highlight),
            markup: Some(self.markup),
            height: None,
        }
    }

    pub(crate) fn apply_highlighter_to_text(&self, options: &ConsoleOptions, text: &mut Text) {
        let highlight_enabled = options.highlight.unwrap_or(self.highlight);
        if highlight_enabled {
            self.highlighter.highlight(self, text);
        }
    }

    /// Measure a renderable via the measurement protocol (Python Rich `Console.measure` parity).
    #[must_use]
    pub fn measure(
        &self,
        renderable: &dyn RichMeasure,
        options: Option<ConsoleOptions>,
    ) -> Measurement {
        let options = options.unwrap_or_else(|| self.options());
        Measurement::get(self, &options, Some(renderable))
    }

    /// Check if the terminal is "dumb".
    #[must_use]
    pub fn is_dumb_terminal(&self) -> bool {
        terminal::is_dumb_terminal()
    }

    /// Check if the console is interactive (TTY and not dumb).
    #[must_use]
    pub fn is_interactive(&self) -> bool {
        self.is_terminal() && !self.is_dumb_terminal()
    }

    pub(crate) fn push_render_hook(&self, hook: Arc<dyn RenderHook>) {
        lock_recover(&self.render_hooks).push(hook);
    }

    pub(crate) fn pop_render_hook(&self) -> Option<Arc<dyn RenderHook>> {
        lock_recover(&self.render_hooks).pop()
    }

    pub(crate) fn set_live(&self, live: &Arc<LiveInner>) -> bool {
        let mut stack = lock_recover(&self.live_stack);
        stack.push(Arc::downgrade(live));
        stack.len() == 1
    }

    pub(crate) fn clear_live(&self) {
        let mut stack = lock_recover(&self.live_stack);
        if !stack.is_empty() {
            stack.pop();
        }
    }

    pub(crate) fn live_stack_snapshot(&self) -> Vec<Arc<LiveInner>> {
        let mut stack = lock_recover(&self.live_stack);
        stack.retain(|entry| entry.strong_count() > 0);
        let mut result = Vec::new();
        for entry in stack.iter() {
            if let Some(live) = entry.upgrade() {
                result.push(live);
            }
        }
        result
    }

    pub(crate) fn write_control_codes(&self, control_codes: Vec<ControlCode>) -> io::Result<()> {
        if control_codes.is_empty() {
            return Ok(());
        }
        let segment = Segment::control(control_codes);
        let mut file = lock_recover(&self.file);
        self.write_segments_raw(&mut *file, &[segment])
    }

    pub(crate) fn swap_file(&self, writer: Box<dyn Write + Send>) -> Box<dyn Write + Send> {
        std::mem::replace(&mut *lock_recover(&self.file), writer)
    }

    /// Show or hide the cursor.
    pub fn show_cursor(&self, show: bool) -> io::Result<()> {
        let control = if show {
            ControlCode::new(ControlType::ShowCursor)
        } else {
            ControlCode::new(ControlType::HideCursor)
        };
        self.write_control_codes(vec![control])
    }

    /// Enable or disable the alternate screen buffer.
    pub fn set_alt_screen(&self, enable: bool) -> io::Result<()> {
        let control = if enable {
            ControlCode::new(ControlType::EnableAltScreen)
        } else {
            ControlCode::new(ControlType::DisableAltScreen)
        };
        self.write_control_codes(vec![control])
    }

    /// Enable recording mode.
    ///
    /// All subsequent console output will be captured to an internal buffer
    /// until [`end_capture`](Self::end_capture) is called.
    pub fn begin_capture(&self) {
        self.record.store(true, Ordering::Relaxed);
        lock_recover(&self.buffer).clear();
    }

    /// End recording and return captured segments.
    ///
    /// Returns all segments captured since [`begin_capture`](Self::begin_capture)
    /// was called, and clears the internal buffer.
    pub fn end_capture(&self) -> Vec<Segment<'static>> {
        self.record.store(false, Ordering::Relaxed);
        std::mem::take(&mut *lock_recover(&self.buffer))
    }

    /// Print styled text to the console.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use rich_rust::Console;
    ///
    /// let console = Console::new();
    /// console.print("[bold red]Hello[/] World!");
    /// ```
    pub fn print(&self, content: &str) {
        self.print_with_options(content, &PrintOptions::new().with_markup(self.markup));
    }

    /// Print a prepared Text object.
    pub fn print_text(&self, text: &Text) {
        let mut file = lock_recover(&self.file);
        let _ = self.print_text_to(&mut *file, text);
    }

    /// Print a prepared Text object to a specific writer.
    pub fn print_text_to<W: Write>(&self, writer: &mut W, text: &Text) -> io::Result<()> {
        let segments: Vec<Segment<'static>> = text
            .render(&text.end)
            .into_iter()
            .map(Segment::into_owned)
            .collect();
        let segments = self.apply_render_hooks(segments);
        self.write_segments_raw(writer, &segments)
    }

    /// Print prepared segments.
    pub fn print_segments(&self, segments: &[Segment<'_>]) {
        let mut file = lock_recover(&self.file);
        let _ = self.print_segments_to(&mut *file, segments);
    }

    /// Print prepared segments to a specific writer.
    pub fn print_segments_to<W: Write>(
        &self,
        writer: &mut W,
        segments: &[Segment<'_>],
    ) -> io::Result<()> {
        let owned: Vec<Segment<'static>> =
            segments.iter().cloned().map(Segment::into_owned).collect();
        let processed = self.apply_render_hooks(owned);
        self.write_segments_raw(writer, &processed)
    }

    /// Print any object implementing the Renderable trait.
    pub fn print_renderable(&self, renderable: &impl Renderable) {
        let options = self.options();
        let segments = renderable.render(self, &options);
        self.print_segments(&segments);
    }

    fn render_rich_cast_segments(
        &self,
        value: &dyn RichCast,
        options: &PrintOptions,
    ) -> Vec<Segment<'static>> {
        match crate::protocol::rich_cast(value) {
            RichCastOutput::Str(text) => self.render_str_segments(&text, options),
            RichCastOutput::Renderable(renderable) => {
                let options = self.options();
                renderable
                    .render(self, &options)
                    .into_iter()
                    .map(Segment::into_owned)
                    .collect()
            }
            RichCastOutput::Castable(renderable) => {
                let options = self.options();
                renderable
                    .render(self, &options)
                    .into_iter()
                    .map(Segment::into_owned)
                    .collect()
            }
        }
    }

    /// Print a value via the protocol casting hook (Python Rich `rich.protocol.rich_cast` parity).
    pub fn print_cast(&self, value: &dyn RichCast) {
        self.print_cast_with_options(value, &PrintOptions::new().with_markup(self.markup));
    }

    /// Print a castable value with custom options (string options apply when the cast yields a string).
    pub fn print_cast_with_options(&self, value: &dyn RichCast, options: &PrintOptions) {
        let mut file = lock_recover(&self.file);
        let _ = self.print_cast_to(&mut *file, value, options);
    }

    /// Print a castable value to a specific writer.
    pub fn print_cast_to<W: Write>(
        &self,
        writer: &mut W,
        value: &dyn RichCast,
        options: &PrintOptions,
    ) -> io::Result<()> {
        let segments = self.render_rich_cast_segments(value, options);
        let segments = self.apply_render_hooks(segments);
        self.write_segments_raw(writer, &segments)
    }

    /// Print an exception / traceback renderable.
    ///
    /// This is a convenience wrapper mirroring Python Rich's `Console.print_exception`.
    pub fn print_exception(&self, traceback: &crate::renderables::Traceback) {
        self.print_renderable(traceback);
    }

    /// Print with custom options.
    pub fn print_with_options(&self, content: &str, options: &PrintOptions) {
        let mut file = lock_recover(&self.file);
        // Keep `Console::print_*` infallible (matches Rich's ergonomics). If callers need
        // I/O error handling they can use `Console::print_to(...)` directly.
        let _ = self.print_to(&mut *file, content, options);
    }

    /// Export rendered text (no ANSI) using default print options.
    #[must_use]
    pub fn export_text(&self, content: &str) -> String {
        self.export_text_with_options(content, &PrintOptions::new().with_markup(self.markup))
    }

    /// Export rendered text (no ANSI) using custom print options.
    #[must_use]
    pub fn export_text_with_options(&self, content: &str, options: &PrintOptions) -> String {
        let segments = self.render_str_segments(content, options);
        Self::segments_to_plain(&segments)
    }

    /// Export a castable value to plain text (no ANSI).
    #[must_use]
    pub fn export_cast_text(&self, value: &dyn RichCast) -> String {
        self.export_cast_text_with_options(value, &PrintOptions::new().with_markup(self.markup))
    }

    /// Export a castable value to plain text (no ANSI) using custom print options.
    #[must_use]
    pub fn export_cast_text_with_options(
        &self,
        value: &dyn RichCast,
        options: &PrintOptions,
    ) -> String {
        let segments = self.render_rich_cast_segments(value, options);
        Self::segments_to_plain(&segments)
    }

    /// Export a renderable to plain text (no ANSI).
    #[must_use]
    pub fn export_renderable_text(&self, renderable: &impl Renderable) -> String {
        let options = self.options();
        let segments = renderable.render(self, &options);
        Self::segments_to_plain(&segments)
    }

    /// Export recorded output to HTML.
    #[must_use]
    pub fn export_html(&self, clear: bool) -> String {
        self.export_html_with_options(&ExportHtmlOptions {
            clear,
            ..ExportHtmlOptions::default()
        })
    }

    /// Export recorded output to SVG.
    #[must_use]
    pub fn export_svg(&self, clear: bool) -> String {
        self.export_svg_with_options(&ExportSvgOptions {
            clear,
            ..ExportSvgOptions::default()
        })
    }

    /// Export recorded output to HTML with Rich-style options.
    ///
    /// Mirrors Python Rich's `Console.export_html(...)` behavior.
    #[must_use]
    pub fn export_html_with_options(&self, options: &ExportHtmlOptions) -> String {
        assert!(
            self.record.load(Ordering::Relaxed),
            "To export console contents call Console::begin_capture() first"
        );
        let segments = self.recorded_segments(options.clear);
        export_segments_to_html_rich(&segments, options)
    }

    /// Export recorded output to SVG with Rich-style options.
    ///
    /// Mirrors Python Rich's `Console.export_svg(...)` behavior.
    #[must_use]
    pub fn export_svg_with_options(&self, options: &ExportSvgOptions) -> String {
        assert!(
            self.record.load(Ordering::Relaxed),
            "To export console contents call Console::begin_capture() first"
        );
        let segments = self.recorded_segments(options.clear);
        export_segments_to_svg_rich(&segments, self.width(), options)
    }

    /// Print to a specific writer.
    pub fn print_to<W: Write>(
        &self,
        writer: &mut W,
        content: &str,
        options: &PrintOptions,
    ) -> io::Result<()> {
        let segments = self.render_str_segments(content, options);
        let segments = self.apply_render_hooks(segments);
        self.write_segments_raw(writer, &segments)
    }

    fn render_str_segments(&self, content: &str, options: &PrintOptions) -> Vec<Segment<'static>> {
        let content = if self.emoji {
            emoji::replace(content, None)
        } else {
            std::borrow::Cow::Borrowed(content)
        };

        // Parse markup if enabled
        let parse_markup = options.markup.unwrap_or(self.markup);
        let mut text = if parse_markup {
            markup::render_or_plain_with_style_resolver(content.as_ref(), |definition| {
                self.get_style(definition)
            })
        } else {
            Text::new(content.as_ref())
        };

        let highlight_enabled = options.highlight.unwrap_or(self.highlight);
        if highlight_enabled {
            let highlighter = options.highlighter.as_ref().unwrap_or(&self.highlighter);
            highlighter.highlight(self, &mut text);
        }

        if let Some(justify) = options.justify {
            text.justify = justify;
        }
        if let Some(overflow) = options.overflow {
            text.overflow = overflow;
        }
        if let Some(no_wrap) = options.no_wrap {
            text.no_wrap = no_wrap;
        }
        if options.crop {
            text.overflow = OverflowMethod::Crop;
        }
        // soft_wrap enables wrapping by overriding text's no_wrap setting
        if options.soft_wrap {
            text.no_wrap = false;
        }

        let width = options.width.or_else(|| {
            if options.justify.is_some()
                || options.overflow.is_some()
                || options.no_wrap.is_some()
                || options.crop
                || options.soft_wrap
            {
                Some(self.width())
            } else {
                None
            }
        });

        let end = if options.no_newline { "" } else { &options.end };
        let mut segments: Vec<Segment<'static>> = if let Some(width) = width {
            let mut rendered = Vec::new();
            let lines = if text.no_wrap {
                text.split_lines()
            } else {
                text.wrap(width)
            };
            let last_index = lines.len().saturating_sub(1);
            let justify = match text.justify {
                JustifyMethod::Default => JustifyMethod::Left,
                other => other,
            };

            for (index, mut line) in lines.into_iter().enumerate() {
                if text.no_wrap && line.cell_len() > width {
                    line.truncate(width, line.overflow, false);
                }

                if matches!(
                    justify,
                    JustifyMethod::Center | JustifyMethod::Right | JustifyMethod::Full
                ) && line.cell_len() < width
                {
                    line.pad(width, justify);
                }

                let line_end = if index == last_index { end } else { "\n" };
                rendered.extend(line.render(line_end).into_iter().map(Segment::into_owned));
            }

            rendered
        } else {
            text.render(end)
                .into_iter()
                .map(Segment::into_owned)
                .collect()
        };

        // Apply any overall style
        if let Some(ref style) = options.style {
            for segment in &mut segments {
                if !segment.is_control() {
                    segment.style = Some(match segment.style {
                        Some(ref s) => style.combine(s),
                        None => style.clone(),
                    });
                }
            }
        }

        segments
    }

    fn segments_to_plain(segments: &[Segment<'_>]) -> String {
        let capacity: usize = segments
            .iter()
            .filter(|segment| !segment.is_control())
            .map(|segment| segment.text.len())
            .sum();
        let mut output = String::with_capacity(capacity);
        for segment in segments {
            if !segment.is_control() {
                output.push_str(segment.text.as_ref());
            }
        }
        output
    }

    fn recorded_segments(&self, clear: bool) -> Vec<Segment<'static>> {
        let mut buffer = lock_recover(&self.buffer);
        let segments = buffer.clone();
        if clear {
            buffer.clear();
        }
        segments
    }

    fn apply_render_hooks(&self, segments: Vec<Segment<'static>>) -> Vec<Segment<'static>> {
        let hooks = lock_recover(&self.render_hooks).clone();
        if hooks.is_empty() {
            return segments;
        }
        let mut current = segments;
        for hook in hooks {
            current = hook.process(self, &current);
        }
        current
    }

    /// Write segments to a writer without invoking render hooks.
    fn write_segments_raw<W: Write>(
        &self,
        writer: &mut W,
        segments: &[Segment<'_>],
    ) -> io::Result<()> {
        if self.record.load(Ordering::Relaxed) {
            lock_recover(&self.buffer).extend(segments.iter().cloned().map(Segment::into_owned));
        }

        let color_system = self.color_system();

        for segment in segments {
            if segment.is_control() {
                self.write_control_segment(writer, segment)?;
                continue;
            }

            // Get ANSI codes for style
            let ansi_codes;
            let (prefix, suffix) = if let Some(ref style) = segment.style {
                if let Some(cs) = color_system {
                    ansi_codes = style.render_ansi(cs);
                    (&ansi_codes.0, &ansi_codes.1)
                } else {
                    static EMPTY: (String, String) = (String::new(), String::new());
                    (&EMPTY.0, &EMPTY.1)
                }
            } else {
                static EMPTY: (String, String) = (String::new(), String::new());
                (&EMPTY.0, &EMPTY.1)
            };

            // Write styled text
            write!(writer, "{prefix}{}{suffix}", segment.text)?;
        }

        writer.flush()
    }

    fn write_control_segment<W: Write>(
        &self,
        writer: &mut W,
        segment: &Segment<'_>,
    ) -> io::Result<()> {
        let Some(ref controls) = segment.control else {
            return Ok(());
        };

        for control in controls {
            match control.control_type {
                crate::segment::ControlType::Bell => {
                    write!(writer, "\x07")?;
                }
                crate::segment::ControlType::CarriageReturn => {
                    write!(writer, "\r")?;
                }
                crate::segment::ControlType::Home => {
                    write!(writer, "\x1b[H")?;
                }
                crate::segment::ControlType::Clear => {
                    write!(writer, "\x1b[2J")?;
                }
                crate::segment::ControlType::ShowCursor => {
                    write!(writer, "\x1b[?25h")?;
                }
                crate::segment::ControlType::HideCursor => {
                    write!(writer, "\x1b[?25l")?;
                }
                crate::segment::ControlType::EnableAltScreen => {
                    write!(writer, "\x1b[?1049h")?;
                }
                crate::segment::ControlType::DisableAltScreen => {
                    write!(writer, "\x1b[?1049l")?;
                }
                crate::segment::ControlType::CursorUp => {
                    let n = control_param(&control.params, 0, 1);
                    write!(writer, "\x1b[{n}A")?;
                }
                crate::segment::ControlType::CursorDown => {
                    let n = control_param(&control.params, 0, 1);
                    write!(writer, "\x1b[{n}B")?;
                }
                crate::segment::ControlType::CursorForward => {
                    let n = control_param(&control.params, 0, 1);
                    write!(writer, "\x1b[{n}C")?;
                }
                crate::segment::ControlType::CursorBackward => {
                    let n = control_param(&control.params, 0, 1);
                    write!(writer, "\x1b[{n}D")?;
                }
                crate::segment::ControlType::CursorMoveToColumn => {
                    // Python Rich expects 0-based columns in ControlCode parameters and
                    // formats with +1 (terminal control sequences are 1-based).
                    let column0 = control_param(&control.params, 0, 0);
                    write!(writer, "\x1b[{}G", column0 + 1)?;
                }
                crate::segment::ControlType::CursorMoveTo => {
                    // Python Rich stores (x, y) 0-based and formats as (y+1; x+1).
                    let x0 = control_param(&control.params, 0, 0);
                    let y0 = control_param(&control.params, 1, 0);
                    write!(writer, "\x1b[{};{}H", y0 + 1, x0 + 1)?;
                }
                crate::segment::ControlType::EraseInLine => {
                    let mode = erase_in_line_mode(&control.params);
                    write!(writer, "\x1b[{mode}K")?;
                }
                crate::segment::ControlType::SetWindowTitle => {
                    let title = control_title(segment, control);
                    write!(writer, "\x1b]0;{title}\x07")?;
                }
            }
        }

        Ok(())
    }

    /// Print a blank line.
    pub fn line(&self) {
        let mut file = lock_recover(&self.file);
        let _ = writeln!(file);
    }

    /// Print a rule (horizontal line).
    pub fn rule(&self, title: Option<&str>) {
        let width = self.width();
        let line_char = if self.safe_box { '-' } else { '\u{2500}' };

        let mut file = lock_recover(&self.file);
        if let Some(title) = title {
            // Ensure title fits within width, accounting for 2 spaces padding
            let max_title_width = width.saturating_sub(2);
            let title_len = crate::cells::cell_len(title);

            let display_title = if title_len > max_title_width {
                let mut t = Text::new(title);
                t.truncate(max_title_width, OverflowMethod::Ellipsis, false);
                t.plain().to_string()
            } else {
                title.to_string()
            };

            let display_len = crate::cells::cell_len(&display_title);
            let available = width.saturating_sub(display_len + 2);
            let left_pad = available / 2;
            let right_pad = available - left_pad;
            let left = line_char.to_string().repeat(left_pad);
            let right = line_char.to_string().repeat(right_pad);
            let _ = writeln!(file, "{left} {display_title} {right}");
        } else {
            let _ = writeln!(file, "{}", line_char.to_string().repeat(width));
        }
    }

    /// Clear the screen.
    pub fn clear(&self) {
        let mut file = lock_recover(&self.file);
        let _ = terminal::control::clear_screen(&mut *file);
    }

    /// Clear the current line.
    pub fn clear_line(&self) {
        let mut file = lock_recover(&self.file);
        let _ = terminal::control::clear_line(&mut *file);
    }

    /// Set the terminal title.
    pub fn set_title(&self, title: &str) {
        let mut file = lock_recover(&self.file);
        let _ = terminal::control::set_title(&mut *file, title);
    }

    /// Ring the terminal bell.
    pub fn bell(&self) {
        let mut file = lock_recover(&self.file);
        let _ = terminal::control::bell(&mut *file);
    }

    /// Print text without parsing markup.
    pub fn print_plain(&self, content: &str) {
        self.print_with_options(content, &PrintOptions::new().with_markup(false));
    }

    /// Print a styled message.
    pub fn print_styled(&self, content: &str, style: Style) {
        self.print_with_options(
            content,
            &PrintOptions::new()
                .with_markup(self.markup)
                .with_style(style),
        );
    }

    /// Print a log message with a level indicator.
    ///
    /// This is a simple version that just shows the level prefix and message.
    /// For timestamps and file/line info, use [`log_with_options`](Self::log_with_options).
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use rich_rust::console::{Console, LogLevel};
    ///
    /// let console = Console::new();
    /// console.log("Starting server", LogLevel::Info);
    /// console.log("Something went wrong", LogLevel::Error);
    /// ```
    pub fn log(&self, message: &str, level: LogLevel) {
        self.log_with_options(message, level, &LogOptions::new());
    }

    /// Print a log message with a level indicator, timestamp, and optional file/line info.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use rich_rust::console::{Console, LogLevel, LogOptions};
    ///
    /// let console = Console::new();
    ///
    /// // With timestamp
    /// let opts = LogOptions::new().with_timestamp(true);
    /// console.log_with_options("Server started", LogLevel::Info, &opts);
    /// // Output: [12:34:56] [INFO] Server started
    ///
    /// // With timestamp and file/line
    /// let opts = LogOptions::new()
    ///     .with_timestamp(true)
    ///     .with_path("src/main.rs", 42);
    /// console.log_with_options("Debug info", LogLevel::Debug, &opts);
    /// // Output: [12:34:56] src/main.rs:42 [DEBUG] Debug info
    /// ```
    pub fn log_with_options(&self, message: &str, level: LogLevel, options: &LogOptions) {
        let (level_prefix, level_style) = match level {
            LogLevel::Debug => ("[DEBUG]", Style::parse("cyan").unwrap_or_default()),
            LogLevel::Info => ("[INFO]", Style::parse("green").unwrap_or_default()),
            LogLevel::Warning => ("[WARNING]", Style::parse("yellow").unwrap_or_default()),
            LogLevel::Error => ("[ERROR]", Style::parse("bold red").unwrap_or_default()),
        };

        {
            let mut file = lock_recover(&self.file);
            // Print timestamp if enabled
            if options.show_timestamp {
                let timestamp = Self::format_timestamp(options.timestamp_format.as_deref());
                let ts_style = Style::parse("dim").unwrap_or_default();
                let _ = self.print_to(
                    &mut *file,
                    &timestamp,
                    &PrintOptions::new().with_markup(false).with_style(ts_style),
                );
                let _ = write!(file, " ");
            }

            // Print file/line info if provided
            if options.file_path.is_some() || options.line_number.is_some() {
                let path_style = Style::parse("magenta").unwrap_or_default();
                let path_info = match (&options.file_path, options.line_number) {
                    (Some(path), Some(line)) => format!("{path}:{line}"),
                    (Some(path), None) => path.clone(),
                    (None, Some(line)) => format!(":{line}"),
                    (None, None) => String::new(),
                };
                if !path_info.is_empty() {
                    let _ = self.print_to(
                        &mut *file,
                        &path_info,
                        &PrintOptions::new()
                            .with_markup(false)
                            .with_style(path_style),
                    );
                    let _ = write!(file, " ");
                }
            }

            // Print level prefix if enabled
            if options.show_level {
                let _ = self.print_to(
                    &mut *file,
                    level_prefix,
                    &PrintOptions::new()
                        .with_markup(false)
                        .with_style(level_style),
                );
                let _ = write!(file, " ");
            }

            // Print the message
            let _ = self.print_to(
                &mut *file,
                message,
                &PrintOptions::new().with_markup(self.markup),
            );
        }
    }

    /// Format the current time as a timestamp string.
    fn format_timestamp(format: Option<&str>) -> String {
        // Prefer local time for parity with typical "console logger" expectations, but
        // fall back to UTC when local offset can't be determined (e.g., sandboxed envs).
        let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());

        match format {
            None => format!(
                "[{:02}:{:02}:{:02}]",
                now.hour(),
                now.minute(),
                now.second()
            ),
            Some(fmt) => Self::format_timestamp_strftime_subset(&now, fmt),
        }
    }

    // Intentionally supports a small, stable subset of strftime:
    // %Y %m %d %H %M %S and %%.
    fn format_timestamp_strftime_subset(now: &OffsetDateTime, fmt: &str) -> String {
        let mut out = String::with_capacity(fmt.len().saturating_add(8));
        let mut it = fmt.chars();

        while let Some(ch) = it.next() {
            if ch != '%' {
                out.push(ch);
                continue;
            }

            let Some(code) = it.next() else {
                out.push('%');
                break;
            };

            match code {
                '%' => out.push('%'),
                'H' => {
                    let _ = write!(out, "{:02}", now.hour());
                }
                'M' => {
                    let _ = write!(out, "{:02}", now.minute());
                }
                'S' => {
                    let _ = write!(out, "{:02}", now.second());
                }
                'Y' => {
                    let _ = write!(out, "{:04}", now.year());
                }
                'm' => {
                    // time::Month implements `From<Month> for u8`.
                    let _ = write!(out, "{:02}", u8::from(now.month()));
                }
                'd' => {
                    let _ = write!(out, "{:02}", now.day());
                }
                other => {
                    // Preserve unknown tokens literally to avoid surprising callers.
                    out.push('%');
                    out.push(other);
                }
            }
        }

        out
    }
}

fn control_param(params: &[i32], index: usize, default: i32) -> i32 {
    params
        .get(index)
        .copied()
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn erase_in_line_mode(params: &[i32]) -> i32 {
    if let Some(value) = params.first().copied()
        && (0..=2).contains(&value)
    {
        return value;
    }
    2
}

fn control_title(segment: &Segment<'_>, control: &crate::segment::ControlCode) -> String {
    let raw_title = if !segment.text.is_empty() {
        segment.text.to_string()
    } else if !control.params.is_empty() {
        let mut title = String::with_capacity(control.params.len());
        for param in &control.params {
            if let Ok(byte) = u8::try_from(*param) {
                title.push(byte as char);
            }
        }
        title
    } else {
        String::new()
    };

    // Sanitize title to prevent terminal injection:
    // Remove control characters that could break or escape the OSC sequence
    raw_title
        .chars()
        .filter(|c| {
            // Allow printable characters only, excluding control chars
            // BEL (\x07) terminates OSC, ESC (\x1b) starts new sequences
            !c.is_control()
        })
        .collect()
}

// ============================================================================
// HTML/SVG Export (Python Rich parity)
// ============================================================================

/// Default HTML export template (Rich 13.9.4).
pub const CONSOLE_HTML_FORMAT: &str = "<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"UTF-8\">\n<style>\n{stylesheet}\nbody {\n    color: {foreground};\n    background-color: {background};\n}\n</style>\n</head>\n<body>\n    <pre style=\"font-family:Menlo,'DejaVu Sans Mono',consolas,'Courier New',monospace\"><code style=\"font-family:inherit\">{code}</code></pre>\n</body>\n</html>\n";

/// Default SVG export template (Rich 13.9.4).
pub const CONSOLE_SVG_FORMAT: &str = "<svg class=\"rich-terminal\" viewBox=\"0 0 {width} {height}\" xmlns=\"http://www.w3.org/2000/svg\">\n    <!-- Generated with Rich https://www.textualize.io -->\n    <style>\n\n    @font-face {\n        font-family: \"Fira Code\";\n        src: local(\"FiraCode-Regular\"),\n                url(\"https://cdnjs.cloudflare.com/ajax/libs/firacode/6.2.0/woff2/FiraCode-Regular.woff2\") format(\"woff2\"),\n                url(\"https://cdnjs.cloudflare.com/ajax/libs/firacode/6.2.0/woff/FiraCode-Regular.woff\") format(\"woff\");\n        font-style: normal;\n        font-weight: 400;\n    }\n    @font-face {\n        font-family: \"Fira Code\";\n        src: local(\"FiraCode-Bold\"),\n                url(\"https://cdnjs.cloudflare.com/ajax/libs/firacode/6.2.0/woff2/FiraCode-Bold.woff2\") format(\"woff2\"),\n                url(\"https://cdnjs.cloudflare.com/ajax/libs/firacode/6.2.0/woff/FiraCode-Bold.woff\") format(\"woff\");\n        font-style: bold;\n        font-weight: 700;\n    }\n\n    .{unique_id}-matrix {\n        font-family: Fira Code, monospace;\n        font-size: {char_height}px;\n        line-height: {line_height}px;\n        font-variant-east-asian: full-width;\n    }\n\n    .{unique_id}-title {\n        font-size: 18px;\n        font-weight: bold;\n        font-family: arial;\n    }\n\n    {styles}\n    </style>\n\n    <defs>\n    <clipPath id=\"{unique_id}-clip-terminal\">\n      <rect x=\"0\" y=\"0\" width=\"{terminal_width}\" height=\"{terminal_height}\" />\n    </clipPath>\n    {lines}\n    </defs>\n\n    {chrome}\n    <g transform=\"translate({terminal_x}, {terminal_y})\" clip-path=\"url(#{unique_id}-clip-terminal)\">\n    {backgrounds}\n    <g class=\"{unique_id}-matrix\">\n    {matrix}\n    </g>\n    </g>\n</svg>\n";

/// Options for controlling HTML export.
#[derive(Debug, Clone)]
pub struct ExportHtmlOptions {
    pub theme: TerminalTheme,
    pub clear: bool,
    /// Optional template override. If `None`, uses [`CONSOLE_HTML_FORMAT`].
    pub code_format: Option<String>,
    pub inline_styles: bool,
}

impl Default for ExportHtmlOptions {
    fn default() -> Self {
        Self {
            theme: DEFAULT_TERMINAL_THEME,
            clear: true,
            code_format: None,
            inline_styles: false,
        }
    }
}

/// Options for controlling SVG export.
#[derive(Debug, Clone)]
pub struct ExportSvgOptions {
    pub title: String,
    pub theme: TerminalTheme,
    pub clear: bool,
    /// Optional template override. If `None`, uses [`CONSOLE_SVG_FORMAT`].
    pub code_format: Option<String>,
    pub font_aspect_ratio: f64,
    pub unique_id: Option<String>,
}

impl Default for ExportSvgOptions {
    fn default() -> Self {
        Self {
            title: "Rich".to_string(),
            theme: SVG_EXPORT_THEME,
            clear: true,
            code_format: None,
            font_aspect_ratio: 0.61,
            unique_id: None,
        }
    }
}

fn export_segments_to_html_rich(segments: &[Segment<'_>], options: &ExportHtmlOptions) -> String {
    let theme = options.theme;
    let render_code_format = options
        .code_format
        .as_deref()
        .unwrap_or(CONSOLE_HTML_FORMAT);

    let simplified = crate::segment::simplify(segments.iter().cloned());

    let mut fragments: Vec<String> = Vec::new();
    let mut stylesheet = String::new();

    if options.inline_styles {
        for segment in simplified {
            if segment.is_control() {
                continue;
            }
            let mut text = escape_html_rich(segment.text.as_ref());
            if let Some(style) = &segment.style {
                let rule = style.get_html_style(theme);
                if let Some(link) = &style.link {
                    text = format!("<a href=\"{link}\">{text}</a>");
                }
                if !rule.is_empty() {
                    text = format!("<span style=\"{rule}\">{text}</span>");
                }
            }
            fragments.push(text);
        }
    } else {
        let mut rules_to_no: HashMap<String, usize> = HashMap::new();
        let mut rules_in_order: Vec<String> = Vec::new();

        let mut get_no = |rule: &str| -> usize {
            if let Some(n) = rules_to_no.get(rule) {
                *n
            } else {
                let n = rules_in_order.len() + 1;
                rules_in_order.push(rule.to_string());
                rules_to_no.insert(rule.to_string(), n);
                n
            }
        };

        for segment in simplified {
            if segment.is_control() {
                continue;
            }
            let mut text = escape_html_rich(segment.text.as_ref());
            if let Some(style) = &segment.style {
                let rule = style.get_html_style(theme);
                let style_no = get_no(&rule);
                if let Some(link) = &style.link {
                    text = format!("<a class=\"r{style_no}\" href=\"{link}\">{text}</a>");
                } else {
                    text = format!("<span class=\"r{style_no}\">{text}</span>");
                }
            }
            fragments.push(text);
        }

        let mut stylesheet_rules: Vec<String> = Vec::new();
        for (idx, rule) in rules_in_order.iter().enumerate() {
            let style_no = idx + 1;
            if !rule.is_empty() {
                stylesheet_rules.push(format!(".r{style_no} {{{rule}}}"));
            }
        }
        stylesheet = stylesheet_rules.join("\n");
    }

    let code = fragments.join("");
    let foreground = theme.foreground_color.hex();
    let background = theme.background_color.hex();
    apply_template(
        render_code_format,
        &[
            ("code", &code),
            ("stylesheet", &stylesheet),
            ("foreground", &foreground),
            ("background", &background),
        ],
    )
}

#[expect(
    clippy::cast_precision_loss,
    reason = "SVG export uses f64 coordinates; console widths/heights are small in practice"
)]
fn export_segments_to_svg_rich(
    segments: &[Segment<'_>],
    console_width: usize,
    options: &ExportSvgOptions,
) -> String {
    use crate::cells::cell_len;

    let theme = options.theme;
    let code_format = options.code_format.as_deref().unwrap_or(CONSOLE_SVG_FORMAT);

    let width = console_width;
    let char_height = 20.0_f64;
    let char_width = char_height * options.font_aspect_ratio;
    let line_height = char_height * 1.22;

    let margin_top = 1.0_f64;
    let margin_right = 1.0_f64;
    let margin_bottom = 1.0_f64;
    let margin_left = 1.0_f64;

    let padding_top = 40.0_f64;
    let padding_right = 8.0_f64;
    let padding_bottom = 8.0_f64;
    let padding_left = 8.0_f64;

    let padding_width = padding_left + padding_right;
    let padding_height = padding_top + padding_bottom;
    let margin_width = margin_left + margin_right;
    let margin_height = margin_top + margin_bottom;

    let mut style_cache: HashMap<Style, String> = HashMap::new();
    let mut get_svg_style = |style: &Style| -> String {
        if let Some(cached) = style_cache.get(style) {
            return cached.clone();
        }
        let css = style.get_svg_style(theme);
        style_cache.insert(style.clone(), css.clone());
        css
    };

    let mut text_backgrounds: Vec<String> = Vec::new();
    let mut text_group: Vec<String> = Vec::new();

    let mut classes_to_no: HashMap<String, usize> = HashMap::new();
    let mut classes_in_order: Vec<String> = Vec::new();
    let mut get_class_no = |rules: &str| -> usize {
        if let Some(n) = classes_to_no.get(rules) {
            *n
        } else {
            let n = classes_in_order.len() + 1;
            classes_in_order.push(rules.to_string());
            classes_to_no.insert(rules.to_string(), n);
            n
        }
    };

    let escape_text = |text: &str| -> String { escape_html_rich(text).replace(' ', "&#160;") };

    let segments: Vec<Segment<'static>> = segments
        .iter()
        .cloned()
        .map(Segment::into_owned)
        .filter(|seg| !seg.is_control())
        .collect();

    let unique_id = options.unique_id.clone().unwrap_or_else(|| {
        let mut repr = String::new();
        for seg in &segments {
            if seg.is_control() {
                continue;
            }
            let _ = FmtWrite::write_fmt(
                &mut repr,
                format_args!(
                    "Segment(text={:?},style={:?},control={:?})",
                    seg.text, seg.style, seg.control
                ),
            );
        }
        repr.push_str(&options.title);
        let checksum = adler32(repr.as_bytes());
        format!("terminal-{checksum}")
    });

    let mut y_last = 0usize;
    let mut lines = crate::segment::split_lines(segments.into_iter());
    lines = lines
        .into_iter()
        .map(|line| crate::segment::adjust_line_length(line, width, None, false))
        .collect();

    let default_style = Style::default();

    for (y, line) in lines.iter().enumerate() {
        y_last = y;
        let mut x_cells = 0usize;
        for segment in line {
            if segment.is_control() {
                continue;
            }

            let text = segment.text.as_ref();
            let style = segment.style.as_ref().unwrap_or(&default_style);
            let rules = get_svg_style(style);
            let class_no = get_class_no(&rules);
            let class_name = format!("r{class_no}");

            let (has_background, background_hex) = if style.attributes.contains(Attributes::REVERSE)
            {
                let bg = match &style.color {
                    None => theme.foreground_color,
                    Some(c) => c.get_truecolor_with_theme(theme, true),
                };
                (true, bg.hex())
            } else {
                let has_bg = style.bgcolor.as_ref().is_some_and(|c| !c.is_default());
                let bg = match &style.bgcolor {
                    None => theme.background_color,
                    Some(c) => c.get_truecolor_with_theme(theme, false),
                };
                (has_bg, bg.hex())
            };

            let text_length = cell_len(text);
            if has_background {
                text_backgrounds.push(format!(
                    "<rect fill=\"{background_hex}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" shape-rendering=\"crispEdges\"/>",
                    (x_cells as f64) * char_width,
                    (y as f64) * line_height + 1.5,
                    char_width * (text_length as f64),
                    line_height + 0.25
                ));
            }

            let all_spaces = text.chars().all(|ch| ch == ' ');
            if !all_spaces {
                let text_len_chars = text.chars().count();
                text_group.push(format!(
                    "<text class=\"{unique_id}-{class_name}\" x=\"{}\" y=\"{}\" textLength=\"{}\" clip-path=\"url(#{unique_id}-line-{y})\">{}</text>",
                    (x_cells as f64) * char_width,
                    (y as f64) * line_height + char_height,
                    char_width * (text_len_chars as f64),
                    escape_text(text)
                ));
            }

            x_cells = x_cells.saturating_add(cell_len(text));
        }
    }

    let mut lines_defs = String::new();
    if y_last > 0 {
        for line_no in 0..y_last {
            let offset = (line_no as f64) * line_height + 1.5;
            let _ = FmtWrite::write_fmt(
                &mut lines_defs,
                format_args!(
                    "<clipPath id=\"{unique_id}-line-{line_no}\">\n    <rect x=\"0\" y=\"{offset}\" width=\"{}\" height=\"{}\"/>\n            </clipPath>",
                    char_width * (width as f64),
                    line_height + 0.25
                ),
            );
        }
    }

    let mut styles = String::new();
    for (idx, css) in classes_in_order.iter().enumerate() {
        let rule_no = idx + 1;
        let _ = FmtWrite::write_fmt(
            &mut styles,
            format_args!(".{unique_id}-r{rule_no} {{ {css} }}\n"),
        );
    }

    let backgrounds = text_backgrounds.join("");
    let matrix = text_group.join("");

    let outer_terminal_width = ((width as f64) * char_width + padding_width).ceil();
    let outer_terminal_height = ((y_last as f64) + 1.0) * line_height + padding_height;

    let mut chrome = format!(
        "<rect fill=\"{}\" stroke=\"rgba(255,255,255,0.35)\" stroke-width=\"1\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"8\"/>",
        theme.background_color.hex(),
        margin_left,
        margin_top,
        outer_terminal_width,
        outer_terminal_height
    );

    if !options.title.is_empty() {
        let title_fill = theme.foreground_color.hex();
        let title_x = outer_terminal_width / 2.0;
        let title_y = margin_top + char_height + 6.0;
        let _ = FmtWrite::write_fmt(
            &mut chrome,
            format_args!(
                "<text class=\"{unique_id}-title\" fill=\"{title_fill}\" text-anchor=\"middle\" x=\"{title_x}\" y=\"{title_y}\">{}</text>",
                escape_text(&options.title)
            ),
        );
    }
    chrome.push_str(
        "\n            <g transform=\"translate(26,22)\">\n            <circle cx=\"0\" cy=\"0\" r=\"7\" fill=\"#ff5f57\"/>\n            <circle cx=\"22\" cy=\"0\" r=\"7\" fill=\"#febc2e\"/>\n            <circle cx=\"44\" cy=\"0\" r=\"7\" fill=\"#28c840\"/>\n            </g>\n        ",
    );

    let char_width_s = char_width.to_string();
    let char_height_s = char_height.to_string();
    let line_height_s = line_height.to_string();
    let terminal_width_s = (char_width * (width as f64) - 1.0).to_string();
    let terminal_height_s = (((y_last as f64) + 1.0) * line_height - 1.0).to_string();
    let width_s = (outer_terminal_width + margin_width).to_string();
    let height_s = (outer_terminal_height + margin_height).to_string();
    let terminal_translate_x = (margin_left + padding_left).to_string();
    let terminal_translate_y = (margin_top + padding_top).to_string();

    apply_template(
        code_format,
        &[
            ("unique_id", &unique_id),
            ("char_width", &char_width_s),
            ("char_height", &char_height_s),
            ("line_height", &line_height_s),
            ("terminal_width", &terminal_width_s),
            ("terminal_height", &terminal_height_s),
            ("width", &width_s),
            ("height", &height_s),
            ("terminal_x", &terminal_translate_x),
            ("terminal_y", &terminal_translate_y),
            ("styles", &styles),
            ("chrome", &chrome),
            ("backgrounds", &backgrounds),
            ("matrix", &matrix),
            ("lines", &lines_defs),
        ],
    )
}

fn apply_template(template: &str, vars: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (key, value) in vars {
        out = out.replace(&format!("{{{key}}}"), value);
    }
    out
}

fn escape_html_rich(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn adler32(bytes: &[u8]) -> u32 {
    const MOD_ADLER: u32 = 65521;
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in bytes {
        a = (a + u32::from(byte)) % MOD_ADLER;
        b = (b + a) % MOD_ADLER;
    }
    (b << 16) | a
}

/// Log level for `console.log()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
}

/// Options for controlling log output format.
///
/// # Examples
///
/// ```rust,ignore
/// use rich_rust::console::{Console, LogLevel, LogOptions};
///
/// let console = Console::new();
///
/// // Log with timestamp
/// let opts = LogOptions::new().with_timestamp(true);
/// console.log_with_options("Something happened", LogLevel::Info, &opts);
///
/// // Log with file/line info
/// let opts = LogOptions::new()
///     .with_timestamp(true)
///     .with_path("src/main.rs", 42);
/// console.log_with_options("Debug info", LogLevel::Debug, &opts);
/// ```
#[derive(Debug, Clone)]
pub struct LogOptions {
    /// Whether to show a timestamp.
    pub show_timestamp: bool,
    /// Custom timestamp format (strftime-like subset).
    ///
    /// Supported codes: `%Y` `%m` `%d` `%H` `%M` `%S` and `%%`.
    /// Unknown codes are preserved literally.
    ///
    /// If None, uses default format: `"[HH:MM:SS]"`.
    pub timestamp_format: Option<String>,
    /// File path (e.g., "src/main.rs").
    pub file_path: Option<String>,
    /// Line number within the file.
    pub line_number: Option<u32>,
    /// Whether to show the log level prefix.
    pub show_level: bool,
    /// Whether to highlight keywords in the message.
    pub highlight: bool,
}

impl Default for LogOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl LogOptions {
    /// Create new log options with default values.
    #[must_use]
    pub fn new() -> Self {
        Self {
            show_timestamp: false,
            timestamp_format: None,
            file_path: None,
            line_number: None,
            show_level: true,
            highlight: false,
        }
    }

    /// Enable or disable timestamp display.
    #[must_use]
    pub fn with_timestamp(mut self, show: bool) -> Self {
        self.show_timestamp = show;
        self
    }

    /// Set a custom timestamp format.
    ///
    /// Simple format using: `%H` (hour), `%M` (minute), `%S` (second),
    /// `%Y` (year), `%m` (month), `%d` (day).
    #[must_use]
    pub fn with_timestamp_format(mut self, format: impl Into<String>) -> Self {
        self.timestamp_format = Some(format.into());
        self
    }

    /// Set the file path and line number for caller info.
    #[must_use]
    pub fn with_path(mut self, file: impl Into<String>, line: u32) -> Self {
        self.file_path = Some(file.into());
        self.line_number = Some(line);
        self
    }

    /// Set just the file path (without line number).
    #[must_use]
    pub fn with_file(mut self, file: impl Into<String>) -> Self {
        self.file_path = Some(file.into());
        self
    }

    /// Set just the line number.
    #[must_use]
    pub fn with_line(mut self, line: u32) -> Self {
        self.line_number = Some(line);
        self
    }

    /// Enable or disable level prefix display.
    #[must_use]
    pub fn with_level(mut self, show: bool) -> Self {
        self.show_level = show;
        self
    }

    /// Enable or disable keyword highlighting.
    #[must_use]
    pub fn with_highlight(mut self, highlight: bool) -> Self {
        self.highlight = highlight;
        self
    }
}

/// RAII guard returned by [`Console::use_theme`].
pub struct ThemeGuard<'a> {
    console: &'a Console,
}

impl Drop for ThemeGuard<'_> {
    fn drop(&mut self) {
        let _ = self.console.pop_theme();
    }
}

/// Builder for creating a Console with custom settings.
#[derive(Default)]
pub struct ConsoleBuilder {
    color_system: Option<ColorSystem>,
    force_terminal: Option<bool>,
    tab_size: Option<usize>,
    markup: Option<bool>,
    emoji: Option<bool>,
    highlight: Option<bool>,
    highlighter: Option<Arc<dyn Highlighter>>,
    width: Option<usize>,
    height: Option<usize>,
    safe_box: Option<bool>,
    theme: Option<Theme>,
    file: Option<Box<dyn Write + Send>>,
}

impl std::fmt::Debug for ConsoleBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConsoleBuilder")
            .field("color_system", &self.color_system)
            .field("force_terminal", &self.force_terminal)
            .field("tab_size", &self.tab_size)
            .field("markup", &self.markup)
            .field("emoji", &self.emoji)
            .field("highlight", &self.highlight)
            .field(
                "highlighter",
                &self.highlighter.as_ref().map(|_| "<Highlighter>"),
            )
            .field("width", &self.width)
            .field("height", &self.height)
            .field("safe_box", &self.safe_box)
            .field("theme", &self.theme.as_ref().map(|_| "<Theme>"))
            .field("file", &self.file.as_ref().map(|_| "<dyn Write>"))
            .finish()
    }
}

impl ConsoleBuilder {
    /// Set the color system.
    #[must_use]
    pub fn color_system(mut self, system: ColorSystem) -> Self {
        self.color_system = Some(system);
        self
    }

    /// Disable colors.
    #[must_use]
    pub fn no_color(mut self) -> Self {
        self.color_system = None;
        self
    }

    /// Force terminal mode.
    #[must_use]
    pub fn force_terminal(mut self, force: bool) -> Self {
        self.force_terminal = Some(force);
        self
    }

    /// Set tab size.
    #[must_use]
    pub fn tab_size(mut self, size: usize) -> Self {
        self.tab_size = Some(size);
        self
    }

    /// Enable/disable markup parsing.
    #[must_use]
    pub fn markup(mut self, enabled: bool) -> Self {
        self.markup = Some(enabled);
        self
    }

    /// Enable/disable emoji.
    #[must_use]
    pub fn emoji(mut self, enabled: bool) -> Self {
        self.emoji = Some(enabled);
        self
    }

    /// Enable/disable highlighting.
    #[must_use]
    pub fn highlight(mut self, enabled: bool) -> Self {
        self.highlight = Some(enabled);
        self
    }

    /// Set the console's default highlighter.
    #[must_use]
    pub fn highlighter<H: Highlighter + 'static>(mut self, highlighter: H) -> Self {
        self.highlighter = Some(Arc::new(highlighter));
        self
    }

    /// Set console width.
    #[must_use]
    pub fn width(mut self, width: usize) -> Self {
        self.width = Some(width);
        self
    }

    /// Set console height.
    #[must_use]
    pub fn height(mut self, height: usize) -> Self {
        self.height = Some(height);
        self
    }

    /// Use ASCII-safe box characters.
    #[must_use]
    pub fn safe_box(mut self, safe: bool) -> Self {
        self.safe_box = Some(safe);
        self
    }

    /// Set the initial console theme.
    #[must_use]
    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = Some(theme);
        self
    }

    /// Set the output stream.
    #[must_use]
    pub fn file(mut self, writer: Box<dyn Write + Send>) -> Self {
        self.file = Some(writer);
        self
    }

    /// Build the console.
    #[must_use]
    pub fn build(self) -> Console {
        let mut console = Console::new();

        if let Some(cs) = self.color_system {
            console.color_system = Some(cs);
        }
        if let Some(ft) = self.force_terminal {
            console.force_terminal = Some(ft);
            if console.color_system.is_none() {
                console.detected_color_system = if ft {
                    terminal::detect_color_system_forced(true)
                } else {
                    None
                };
            }
        }
        if let Some(ts) = self.tab_size {
            console.tab_size = ts;
        }
        if let Some(m) = self.markup {
            console.markup = m;
        }
        if let Some(e) = self.emoji {
            console.emoji = e;
        }
        if let Some(h) = self.highlight {
            console.highlight = h;
        }
        if let Some(highlighter) = self.highlighter {
            console.highlighter = highlighter;
        }
        if let Some(w) = self.width {
            console.width = Some(w);
        }
        if let Some(h) = self.height {
            console.height = Some(h);
        }
        if let Some(sb) = self.safe_box {
            console.safe_box = sb;
        }
        if let Some(theme) = self.theme {
            console.theme_stack = Mutex::new(ThemeStack::new(theme));
        }
        if let Some(f) = self.file {
            console.file = Mutex::new(f);
        }

        console
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::highlighter::NullHighlighter;

    #[test]
    fn test_console_new() {
        let console = Console::new();
        assert!(console.width() > 0);
        assert!(console.height() > 0);
    }

    #[test]
    fn test_console_builder() {
        let console = Console::builder()
            .width(100)
            .height(50)
            .markup(false)
            .build();

        assert_eq!(console.width(), 100);
        assert_eq!(console.height(), 50);
        assert!(!console.markup);
    }

    #[test]
    fn test_console_default_highlighter_applies_when_enabled() {
        let console = Console::builder().markup(false).build();
        let opts = PrintOptions::new().with_markup(false).with_no_newline(true);
        let segments = console.render_str_segments("True", &opts);
        let expected = console.get_style("repr.bool_true");
        assert!(segments.iter().any(|s| s.style.as_ref() == Some(&expected)));
    }

    #[test]
    fn test_console_highlight_override_off_disables_highlighter() {
        let console = Console::builder().markup(false).build();
        let opts = PrintOptions::new()
            .with_markup(false)
            .with_no_newline(true)
            .with_highlight(false);
        let segments = console.render_str_segments("True", &opts);
        let expected = console.get_style("repr.bool_true");
        assert!(!segments.iter().any(|s| s.style.as_ref() == Some(&expected)));
    }

    #[test]
    fn test_console_builder_highlighter_override() {
        let console = Console::builder()
            .markup(false)
            .highlighter(NullHighlighter)
            .build();
        let opts = PrintOptions::new().with_markup(false).with_no_newline(true);
        let segments = console.render_str_segments("True", &opts);
        let expected = console.get_style("repr.bool_true");
        assert!(!segments.iter().any(|s| s.style.as_ref() == Some(&expected)));
    }

    #[test]
    fn test_console_print_options_highlighter_override() {
        let console = Console::builder().markup(false).build();
        let opts = PrintOptions::new()
            .with_markup(false)
            .with_no_newline(true)
            .with_highlight(true)
            .with_highlighter(NullHighlighter);
        let segments = console.render_str_segments("True", &opts);
        let expected = console.get_style("repr.bool_true");
        assert!(!segments.iter().any(|s| s.style.as_ref() == Some(&expected)));
    }

    #[test]
    fn test_console_options() {
        let console = Console::builder().width(80).build();
        let options = console.options();

        assert_eq!(options.max_width, 80);
        assert_eq!(options.size.width, 80);
    }

    #[test]
    fn test_print_options() {
        let options = PrintOptions::new()
            .with_markup(true)
            .with_style(Style::new().bold());

        assert_eq!(options.markup, Some(true));
        assert!(options.style.is_some());
    }

    #[test]
    fn test_capture() {
        let console = Console::new();
        console.begin_capture();

        console.print_plain("capture test");
        let segments = console.end_capture();
        let captured: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(captured.contains("capture test"));
    }

    #[test]
    fn test_capture_collects_segments() {
        use std::sync::{Arc, Mutex};

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

        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .width(40)
            .markup(false)
            .file(Box::new(buffer))
            .build();

        console.begin_capture();
        console.print_plain("Hello");
        let segments = console.end_capture();

        let captured: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(captured.contains("Hello"));
    }

    #[test]
    fn test_print_exception_renders_traceback() {
        use crate::renderables::{Traceback, TracebackFrame};

        let console = Console::builder().width(60).markup(false).build();
        console.begin_capture();

        let traceback = Traceback::new(
            vec![
                TracebackFrame::new("<module>", 14),
                TracebackFrame::new("level1", 11),
            ],
            "ErrorType",
            "boom",
        );

        console.print_exception(&traceback);
        let segments = console.end_capture();
        let captured: String = segments.iter().map(|s| s.text.as_ref()).collect();

        assert!(captured.contains("Traceback (most recent call last)"));
        assert!(captured.contains("in <module>:14"));
        assert!(captured.contains("ErrorType: boom"));
    }

    #[test]
    fn test_dimensions() {
        let dims = ConsoleDimensions::default();
        assert_eq!(dims.width, 80);
        assert_eq!(dims.height, 24);
    }

    #[test]
    fn test_custom_output_stream() {
        use std::sync::{Arc, Mutex};

        // Thread-safe buffer that implements Write + Send
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

        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .width(80)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build();

        console.print_plain("Hello, World!");

        let output = buffer.0.lock().unwrap();
        let text = String::from_utf8_lossy(&output);
        assert!(
            text.contains("Hello, World!"),
            "Expected 'Hello, World!' in output, got: {text}"
        );
    }

    #[test]
    fn test_print_plain_disables_markup() {
        use std::sync::{Arc, Mutex};

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

        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .markup(true)
            .file(Box::new(buffer.clone()))
            .build();

        console.print_plain("[bold]Hello[/]");

        let output = buffer.0.lock().unwrap();
        let text = String::from_utf8_lossy(&output);
        assert!(
            text.contains("[bold]Hello[/]"),
            "Expected literal markup in output, got: {text}"
        );
        assert!(
            !text.contains("\x1b["),
            "Did not expect ANSI sequences in output, got: {text}"
        );
    }

    #[test]
    fn test_export_text_defaults() {
        let console = Console::builder().markup(true).build();
        let output = console.export_text("[bold]Hello[/]");
        assert_eq!(output, "Hello\n");
    }

    #[test]
    fn test_export_text_respects_markup_setting() {
        let console = Console::builder().markup(false).build();
        let output = console.export_text("[bold]Hello[/]");
        assert_eq!(output, "[bold]Hello[/]\n");
    }

    #[test]
    fn test_export_text_replaces_emoji_codes_by_default() {
        let console = Console::builder().markup(false).build();
        let output = console.export_text("hi :smile:");
        assert_eq!(output, "hi 😄\n");
    }

    #[test]
    fn test_export_text_does_not_replace_emoji_codes_when_disabled() {
        let console = Console::builder().markup(false).emoji(false).build();
        let output = console.export_text("hi :smile:");
        assert_eq!(output, "hi :smile:\n");
    }

    #[test]
    fn test_export_text_with_options_no_newline() {
        let console = Console::new();
        let mut options = PrintOptions::new().with_markup(false);
        options.no_newline = true;
        let output = console.export_text_with_options("Hello", &options);
        assert_eq!(output, "Hello");
    }

    #[test]
    fn test_export_renderable_text() {
        use crate::renderables::Rule;

        let console = Console::builder().width(20).build();
        let rule = Rule::with_title("Title");
        let output = console.export_renderable_text(&rule);
        assert!(output.contains("Title"));
        assert!(output.ends_with('\n'));
    }

    #[test]
    fn test_export_html_svg_capture() {
        use std::sync::{Arc, Mutex};

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

        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .markup(false)
            .file(Box::new(buffer))
            .build();

        console.begin_capture();
        console.print_plain("Hello");

        let html = console.export_html(false);
        assert!(html.contains("<pre"));
        assert!(html.contains("Hello"));

        let svg = console.export_svg(true);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("Hello"));

        let cleared = console.export_html(false);
        assert!(!cleared.contains("Hello"));
    }

    #[test]
    fn test_escape_html_entities() {
        let escaped = escape_html_rich("<>&\"'");
        assert_eq!(escaped, "&lt;&gt;&amp;&quot;'");
    }

    #[test]
    fn test_style_html_rule_basic_attributes() {
        use crate::color::Color;

        let style = Style::new()
            .color(Color::from_rgb(255, 0, 0))
            .bgcolor(Color::from_rgb(0, 0, 255))
            .bold()
            .italic()
            .underline()
            .strike();
        let css = style.get_html_style(DEFAULT_TERMINAL_THEME);

        assert!(css.contains("color: #ff0000"));
        assert!(css.contains("background-color: #0000ff"));
        assert!(css.contains("font-weight: bold"));
        assert!(css.contains("font-style: italic"));
        assert!(css.contains("text-decoration: underline"));
        assert!(css.contains("text-decoration: line-through"));
    }

    #[test]
    fn test_style_html_rule_reverse_swaps_colors() {
        use crate::color::Color;

        let style = Style::new()
            .color(Color::from_rgb(10, 20, 30))
            .bgcolor(Color::from_rgb(200, 210, 220))
            .reverse();
        let css = style.get_html_style(DEFAULT_TERMINAL_THEME);

        assert!(css.contains("color: #c8d2dc"));
        assert!(css.contains("background-color: #0a141e"));
    }

    #[test]
    fn test_export_html_body_links_and_spans() {
        let link_style = Style::new().link("https://example.com").bold();
        let segments = vec![
            Segment::new("Link", Some(link_style)),
            Segment::new(" ", None),
            Segment::new("Plain", None),
        ];

        let opts = ExportHtmlOptions {
            inline_styles: true,
            code_format: Some("{code}".to_string()),
            ..ExportHtmlOptions::default()
        };
        let html = export_segments_to_html_rich(&segments, &opts);
        assert!(html.contains("href=\"https://example.com\""));
        assert!(html.contains("font-weight: bold"));
        assert!(html.contains("Plain"));
    }

    #[test]
    fn test_export_html_escapes_text() {
        let segments = vec![Segment::plain("<tag> & \"quote\"")];
        let opts = ExportHtmlOptions {
            inline_styles: true,
            code_format: Some("{code}".to_string()),
            ..ExportHtmlOptions::default()
        };
        let html = export_segments_to_html_rich(&segments, &opts);
        assert!(html.contains("&lt;tag&gt;"));
        assert!(html.contains("&amp;"));
        assert!(html.contains("&quot;"));
    }

    #[test]
    fn test_export_html_skips_control_segments() {
        use crate::segment::{ControlCode, ControlType};

        let segments = vec![
            Segment::control(vec![ControlCode::new(ControlType::Bell)]),
            Segment::new("Hi", None),
        ];
        let opts = ExportHtmlOptions {
            inline_styles: true,
            code_format: Some("{code}".to_string()),
            ..ExportHtmlOptions::default()
        };
        let html = export_segments_to_html_rich(&segments, &opts);
        assert!(html.contains("Hi"));
        assert!(!html.contains("Bell"));
    }

    #[test]
    fn test_export_svg_dimensions() {
        let segments = vec![Segment::plain("AB"), Segment::line(), Segment::plain("C")];
        let opts = ExportSvgOptions {
            code_format: Some("{width}x{height}".to_string()),
            ..ExportSvgOptions::default()
        };
        let svg = export_segments_to_svg_rich(&segments, 2, &opts);
        assert!(svg.contains('x'));
    }

    #[test]
    fn test_export_svg_includes_text() {
        let segments = vec![Segment::plain("Hello")];
        let opts = ExportSvgOptions {
            code_format: Some("{matrix}".to_string()),
            ..ExportSvgOptions::default()
        };
        let svg = export_segments_to_svg_rich(&segments, 10, &opts);
        assert!(svg.contains("Hello"));
    }

    #[test]
    fn test_export_html_document_structure() {
        let segments = vec![Segment::plain("Hello")];
        let opts = ExportHtmlOptions::default();
        let html = export_segments_to_html_rich(&segments, &opts);
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<meta charset=\"UTF-8\">"));
        assert!(html.contains("<body>"));
        assert!(html.contains("</html>"));
    }

    #[test]
    fn test_export_html_includes_renderable_content() {
        use crate::renderables::{Column, Panel, Table, Tree, TreeNode};

        let console = Console::builder().width(30).build();
        console.begin_capture();

        let mut table = Table::new().with_column(Column::new("Col"));
        table.add_row_cells(["Cell"]);
        console.print_renderable(&table);

        let panel = Panel::from_text("Panel").width(10);
        console.print_renderable(&panel);

        let root = TreeNode::new("Root").child(TreeNode::new("Leaf"));
        let tree = Tree::new(root);
        console.print_renderable(&tree);

        let html = console.export_html(true);
        assert!(html.contains("Col"));
        assert!(html.contains("Cell"));
        assert!(html.contains("Panel"));
        assert!(html.contains("Root"));
        assert!(html.contains("Leaf"));
    }

    #[test]
    fn test_print_options_justify_uses_console_width() {
        let console = Console::builder().width(10).markup(false).build();
        let mut output = Vec::new();
        let mut options = PrintOptions::new().with_justify(JustifyMethod::Center);
        options.no_newline = true;

        console
            .print_to(&mut output, "Hi", &options)
            .expect("failed to render");

        let text = String::from_utf8(output).expect("invalid utf8");
        assert_eq!(text, "    Hi    ");
    }

    #[test]
    fn test_print_options_width_wraps() {
        let console = Console::builder().width(80).markup(false).build();
        let mut output = Vec::new();
        let mut options = PrintOptions::new();
        options.width = Some(4);

        console
            .print_to(&mut output, "Hello", &options)
            .expect("failed to render");

        let text = String::from_utf8(output).expect("invalid utf8");
        assert_eq!(text, "Hell\no\n");
    }

    #[test]
    fn test_print_options_no_wrap_ellipsis() {
        let console = Console::builder().width(80).markup(false).build();
        let mut output = Vec::new();
        let mut options = PrintOptions::new()
            .with_no_wrap(true)
            .with_overflow(OverflowMethod::Ellipsis);
        options.width = Some(4);
        options.no_newline = true;

        console
            .print_to(&mut output, "Hello", &options)
            .expect("failed to render");

        let text = String::from_utf8(output).expect("invalid utf8");
        assert_eq!(text, "H...");
    }

    #[test]
    fn test_custom_output_stream_line() {
        use std::sync::{Arc, Mutex};

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

        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .width(80)
            .file(Box::new(buffer.clone()))
            .build();

        console.line();

        let output = buffer.0.lock().unwrap();
        let text = String::from_utf8_lossy(&output);
        assert_eq!(text, "\n", "Expected single newline, got: {text:?}");
    }

    // ========== ConsoleBuilder Tests ==========

    #[test]
    fn test_console_builder_color_system() {
        let console = Console::builder()
            .color_system(ColorSystem::TrueColor)
            .build();
        assert_eq!(console.color_system(), Some(ColorSystem::TrueColor));
    }

    #[test]
    fn test_console_builder_no_color() {
        let console = Console::builder().no_color().build();
        assert_eq!(console.color_system, None);
    }

    #[test]
    fn test_console_builder_force_terminal() {
        let console = Console::builder().force_terminal(true).build();
        assert!(console.is_terminal());
    }

    #[test]
    fn test_console_builder_tab_size() {
        let console = Console::builder().tab_size(4).build();
        assert_eq!(console.tab_size(), 4);
    }

    #[test]
    fn test_console_builder_emoji() {
        let console = Console::builder().emoji(false).build();
        assert!(!console.emoji);
    }

    #[test]
    fn test_console_builder_highlight() {
        let console = Console::builder().highlight(false).build();
        assert!(!console.highlight);
    }

    #[test]
    fn test_console_builder_safe_box() {
        let console = Console::builder().safe_box(true).build();
        assert!(console.safe_box);
    }

    #[test]
    fn test_console_builder_all_options() {
        let console = Console::builder()
            .color_system(ColorSystem::EightBit)
            .force_terminal(true)
            .tab_size(2)
            .markup(false)
            .emoji(false)
            .highlight(false)
            .width(120)
            .height(40)
            .safe_box(true)
            .build();

        assert_eq!(console.color_system(), Some(ColorSystem::EightBit));
        assert!(console.is_terminal());
        assert_eq!(console.tab_size(), 2);
        assert!(!console.markup);
        assert!(!console.emoji);
        assert!(!console.highlight);
        assert_eq!(console.width(), 120);
        assert_eq!(console.height(), 40);
        assert!(console.safe_box);
    }

    // ========== Console Size Tests ==========

    #[test]
    fn test_console_size_returns_dimensions() {
        let console = Console::builder().width(100).height(50).build();
        let size = console.size();
        assert_eq!(size.width, 100);
        assert_eq!(size.height, 50);
    }

    #[test]
    fn test_console_default_dimensions() {
        let console = Console::new();
        // Default should be reasonable terminal size
        assert!(console.width() >= 40);
        assert!(console.height() >= 10);
    }

    // ========== PrintOptions Tests ==========

    #[test]
    fn test_print_options_default() {
        let options = PrintOptions::new();
        assert_eq!(options.markup, None);
        assert!(options.style.is_none());
        assert_eq!(options.sep, " ");
        assert_eq!(options.end, "\n");
        assert_eq!(options.no_wrap, None);
        assert!(!options.no_newline);
        assert_eq!(options.highlight, None);
    }

    #[test]
    fn test_print_options_with_sep() {
        let options = PrintOptions::new().with_sep(", ");
        assert_eq!(options.sep, ", ");
    }

    #[test]
    fn test_print_options_with_end() {
        let options = PrintOptions::new().with_end("\r\n");
        assert_eq!(options.end, "\r\n");
    }

    #[test]
    fn test_print_options_with_overflow() {
        let options = PrintOptions::new().with_overflow(OverflowMethod::Crop);
        assert_eq!(options.overflow, Some(OverflowMethod::Crop));
    }

    #[test]
    fn test_print_options_with_crop() {
        let options = PrintOptions::new().with_crop(true);
        assert!(options.crop);
    }

    #[test]
    fn test_print_options_with_soft_wrap() {
        let options = PrintOptions::new().with_soft_wrap(true);
        assert!(options.soft_wrap);
    }

    #[test]
    fn test_print_options_chained() {
        let style = Style::new().bold().italic();
        let options = PrintOptions::new()
            .with_markup(false)
            .with_style(style.clone())
            .with_sep(" | ")
            .with_end("")
            .with_justify(JustifyMethod::Right)
            .with_overflow(OverflowMethod::Ellipsis)
            .with_no_wrap(true)
            .with_no_newline(true)
            .with_highlight(true)
            .with_width(40)
            .with_crop(true)
            .with_soft_wrap(true);

        assert_eq!(options.markup, Some(false));
        assert!(options.style.is_some());
        assert_eq!(options.sep, " | ");
        assert_eq!(options.end, "");
        assert_eq!(options.justify, Some(JustifyMethod::Right));
        assert_eq!(options.overflow, Some(OverflowMethod::Ellipsis));
        assert_eq!(options.no_wrap, Some(true));
        assert!(options.no_newline);
        assert_eq!(options.highlight, Some(true));
        assert_eq!(options.width, Some(40));
        assert!(options.crop);
        assert!(options.soft_wrap);
    }

    // ========== ConsoleOptions Tests ==========

    #[test]
    fn test_console_options_update_width() {
        let console = Console::builder().width(100).build();
        let options = console.options();
        // update_width clamps to the new width (min of current and new)
        let updated = options.update_width(80);
        assert_eq!(updated.max_width, 80);
    }

    #[test]
    fn test_console_options_update_height() {
        let console = Console::builder().height(24).build();
        let options = console.options();
        // update_height sets the height in the options
        let updated = options.update_height(50);
        assert_eq!(updated.height, Some(50));
    }

    // ========== Color System Tests ==========

    #[test]
    fn test_console_is_color_enabled_with_system() {
        let console = Console::builder()
            .color_system(ColorSystem::Standard)
            .build();
        assert!(console.is_color_enabled());
    }

    #[test]
    fn test_console_is_color_enabled_no_color() {
        let console = Console::builder().no_color().build();
        assert!(!console.is_color_enabled());
    }

    // ========== Capture Mode Tests ==========

    #[test]
    fn test_capture_empty() {
        let console = Console::new();
        console.begin_capture();
        let segments = console.end_capture();
        assert_eq!(segments.len(), 0);
    }

    #[test]
    fn test_capture_with_styled_text() {
        use std::sync::{Arc, Mutex};

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

        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .width(80)
            .markup(true)
            .color_system(ColorSystem::TrueColor)
            .file(Box::new(buffer))
            .build();

        console.begin_capture();
        console.print("[bold]Test[/]");
        let segments = console.end_capture();

        // Should have captured at least one segment
        assert_ne!(segments.len(), 0);
    }

    #[test]
    fn test_capture_multiple_prints() {
        use std::sync::{Arc, Mutex};

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

        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .width(80)
            .markup(false)
            .file(Box::new(buffer))
            .build();

        console.begin_capture();
        console.print_plain("First");
        console.print_plain("Second");
        let segments = console.end_capture();

        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("First"));
        assert!(text.contains("Second"));
    }

    // ========== Print Method Tests ==========

    #[test]
    fn test_print_text_direct() {
        use std::sync::{Arc, Mutex};

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

        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .width(80)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build();

        let text = Text::new("Direct text");
        console.print_text(&text);

        let output = buffer.0.lock().unwrap();
        let result = String::from_utf8_lossy(&output);
        assert!(result.contains("Direct text"));
    }

    #[test]
    fn test_print_styled() {
        use std::sync::{Arc, Mutex};

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

        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .width(80)
            .color_system(ColorSystem::TrueColor)
            .file(Box::new(buffer.clone()))
            .build();

        console.print_styled("Styled", Style::new().bold());

        let output = buffer.0.lock().unwrap();
        let result = String::from_utf8_lossy(&output);
        assert!(result.contains("Styled"));
        // Should contain ANSI codes for bold
        assert!(result.contains("\x1b["));
    }

    #[test]
    fn test_print_to_writer() {
        let console = Console::builder().width(80).markup(false).build();
        let mut output = Vec::new();
        let options = PrintOptions::new();

        console
            .print_to(&mut output, "Writer test", &options)
            .expect("failed to print");

        let text = String::from_utf8(output).expect("invalid utf8");
        assert!(text.contains("Writer test"));
    }

    #[test]
    fn test_print_segments() {
        use std::sync::{Arc, Mutex};

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

        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .width(80)
            .file(Box::new(buffer.clone()))
            .build();

        let segments = vec![Segment::plain("Hello "), Segment::plain("World")];
        console.print_segments(&segments);

        let output = buffer.0.lock().unwrap();
        let result = String::from_utf8_lossy(&output);
        assert!(result.contains("Hello "));
        assert!(result.contains("World"));
    }

    // ========== Rule Method Tests ==========

    #[test]
    fn test_rule_without_title() {
        use std::sync::{Arc, Mutex};

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

        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .width(20)
            .file(Box::new(buffer.clone()))
            .build();

        console.rule(None);

        let output = buffer.0.lock().unwrap();
        let result = String::from_utf8_lossy(&output);
        // Rule should contain horizontal line characters
        assert!(result.contains('─') || result.contains('-'));
    }

    #[test]
    fn test_rule_with_title() {
        use std::sync::{Arc, Mutex};

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

        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .width(40)
            .file(Box::new(buffer.clone()))
            .build();

        console.rule(Some("Section"));

        let output = buffer.0.lock().unwrap();
        let result = String::from_utf8_lossy(&output);
        assert!(result.contains("Section"));
    }

    // ========== Log Method Tests ==========

    #[test]
    fn test_log_debug() {
        use std::sync::{Arc, Mutex};

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

        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .width(80)
            .file(Box::new(buffer.clone()))
            .build();

        console.log("Debug message", LogLevel::Debug);

        let output = buffer.0.lock().unwrap();
        let result = String::from_utf8_lossy(&output);
        assert!(result.contains("Debug message"));
    }

    #[test]
    fn test_log_info() {
        use std::sync::{Arc, Mutex};

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

        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .width(80)
            .file(Box::new(buffer.clone()))
            .build();

        console.log("Info message", LogLevel::Info);

        let output = buffer.0.lock().unwrap();
        let result = String::from_utf8_lossy(&output);
        assert!(result.contains("Info message"));
    }

    #[test]
    fn test_log_warning() {
        use std::sync::{Arc, Mutex};

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

        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .width(80)
            .file(Box::new(buffer.clone()))
            .build();

        console.log("Warning message", LogLevel::Warning);

        let output = buffer.0.lock().unwrap();
        let result = String::from_utf8_lossy(&output);
        assert!(result.contains("Warning message"));
    }

    #[test]
    fn test_log_error() {
        use std::sync::{Arc, Mutex};

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

        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .width(80)
            .file(Box::new(buffer.clone()))
            .build();

        console.log("Error message", LogLevel::Error);

        let output = buffer.0.lock().unwrap();
        let result = String::from_utf8_lossy(&output);
        assert!(result.contains("Error message"));
    }

    // ========== Log with Options Tests ==========

    #[test]
    fn test_log_with_timestamp() {
        use std::sync::{Arc, Mutex};

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

        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .width(80)
            .file(Box::new(buffer.clone()))
            .build();

        let opts = LogOptions::new().with_timestamp(true);
        console.log_with_options("Test message", LogLevel::Info, &opts);

        let output = buffer.0.lock().unwrap();
        let result = String::from_utf8_lossy(&output);
        // Should contain timestamp format [HH:MM:SS]
        assert!(result.contains('['));
        assert!(result.contains(']'));
        assert!(result.contains(':'));
        assert!(result.contains("Test message"));
    }

    #[test]
    fn test_log_with_file_path() {
        use std::sync::{Arc, Mutex};

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

        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .width(80)
            .file(Box::new(buffer.clone()))
            .build();

        let opts = LogOptions::new().with_path("src/main.rs", 42);
        console.log_with_options("Debug info", LogLevel::Debug, &opts);

        let output = buffer.0.lock().unwrap();
        let result = String::from_utf8_lossy(&output);
        assert!(result.contains("src/main.rs"));
        assert!(result.contains("42"));
        assert!(result.contains("Debug info"));
    }

    #[test]
    fn test_log_with_timestamp_and_path() {
        use std::sync::{Arc, Mutex};

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

        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .width(80)
            .file(Box::new(buffer.clone()))
            .build();

        let opts = LogOptions::new()
            .with_timestamp(true)
            .with_path("test.rs", 100);
        console.log_with_options("Combined test", LogLevel::Warning, &opts);

        let output = buffer.0.lock().unwrap();
        let result = String::from_utf8_lossy(&output);
        assert!(result.contains('[')); // timestamp bracket
        assert!(result.contains("test.rs"));
        assert!(result.contains("100"));
        assert!(result.contains("Combined test"));
    }

    #[test]
    fn test_log_without_level() {
        use std::sync::{Arc, Mutex};

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

        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .width(80)
            .file(Box::new(buffer.clone()))
            .build();

        let opts = LogOptions::new().with_level(false);
        console.log_with_options("No level prefix", LogLevel::Info, &opts);

        let output = buffer.0.lock().unwrap();
        let result = String::from_utf8_lossy(&output);
        assert!(!result.contains("[INFO]"));
        assert!(result.contains("No level prefix"));
    }

    #[test]
    fn test_log_options_default() {
        let opts = LogOptions::default();
        assert!(!opts.show_timestamp);
        assert!(opts.timestamp_format.is_none());
        assert!(opts.file_path.is_none());
        assert!(opts.line_number.is_none());
        assert!(opts.show_level);
        assert!(!opts.highlight);
    }

    #[test]
    fn test_log_options_builder() {
        let opts = LogOptions::new()
            .with_timestamp(true)
            .with_timestamp_format("%Y-%m-%d %H:%M:%S")
            .with_file("test.rs")
            .with_line(123)
            .with_level(false)
            .with_highlight(true);

        assert!(opts.show_timestamp);
        assert_eq!(opts.timestamp_format, Some("%Y-%m-%d %H:%M:%S".to_string()));
        assert_eq!(opts.file_path, Some("test.rs".to_string()));
        assert_eq!(opts.line_number, Some(123));
        assert!(!opts.show_level);
        assert!(opts.highlight);
    }

    #[test]
    fn test_format_timestamp_default() {
        let ts = Console::format_timestamp(None);
        // Default format: [HH:MM:SS]
        assert!(ts.starts_with('['));
        assert!(ts.ends_with(']'));
        assert_eq!(ts.matches(':').count(), 2);
    }

    #[test]
    fn test_format_timestamp_custom() {
        let ts = Console::format_timestamp(Some("%H-%M-%S"));
        // Custom format: HH-MM-SS
        assert_eq!(ts.matches('-').count(), 2);
        assert!(!ts.contains(':'));
    }

    #[test]
    fn test_format_timestamp_custom_with_date_tokens() {
        let ts = Console::format_timestamp(Some("%Y-%m-%d %H:%M:%S"));
        // We don't assert wall-clock values; we only assert the substitutions happened.
        assert_eq!(ts.len(), "0000-00-00 00:00:00".len());
        assert_eq!(ts.matches('-').count(), 2);
        assert_eq!(ts.matches(':').count(), 2);
    }

    // ========== Markup Integration Tests ==========

    #[test]
    fn test_markup_enabled() {
        use std::sync::{Arc, Mutex};

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

        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .width(80)
            .markup(true)
            .color_system(ColorSystem::TrueColor)
            .file(Box::new(buffer.clone()))
            .build();

        console.print("[bold]Bold text[/]");

        let output = buffer.0.lock().unwrap();
        let result = String::from_utf8_lossy(&output);
        // Should contain ANSI codes, not literal [bold]
        assert!(!result.contains("[bold]"));
        assert!(result.contains("\x1b["));
    }

    #[test]
    fn test_markup_disabled() {
        use std::sync::{Arc, Mutex};

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

        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .width(80)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build();

        console.print("[bold]Literal markup[/]");

        let output = buffer.0.lock().unwrap();
        let result = String::from_utf8_lossy(&output);
        // Should contain literal markup tags
        assert!(result.contains("[bold]"));
    }

    // ========== Width Constraint Tests ==========

    #[test]
    fn test_print_with_width_constraint() {
        let console = Console::builder().width(80).markup(false).build();
        let mut output = Vec::new();
        let mut options = PrintOptions::new();
        options.width = Some(10);

        console
            .print_to(
                &mut output,
                "This is a long text that should wrap",
                &options,
            )
            .expect("failed to print");

        let text = String::from_utf8(output).expect("invalid utf8");
        // Text should be wrapped at width 10
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines.len() > 1);
    }

    #[test]
    fn test_justify_left() {
        let console = Console::builder().width(20).markup(false).build();
        let mut output = Vec::new();
        let mut options = PrintOptions::new().with_justify(JustifyMethod::Left);
        options.no_newline = true;

        console
            .print_to(&mut output, "Left", &options)
            .expect("failed to print");

        let text = String::from_utf8(output).expect("invalid utf8");
        assert!(text.starts_with("Left"));
    }

    #[test]
    fn test_justify_right() {
        let console = Console::builder().width(20).markup(false).build();
        let mut output = Vec::new();
        let mut options = PrintOptions::new().with_justify(JustifyMethod::Right);
        options.no_newline = true;

        console
            .print_to(&mut output, "Right", &options)
            .expect("failed to print");

        let text = String::from_utf8(output).expect("invalid utf8");
        assert!(text.ends_with("Right"));
        assert_eq!(text.len(), 20);
    }

    // ========== ConsoleDimensions Tests ==========

    #[test]
    fn test_console_dimensions_default() {
        let dims = ConsoleDimensions::default();
        assert_eq!(dims.width, 80);
        assert_eq!(dims.height, 24);
    }

    #[test]
    fn test_console_dimensions_custom() {
        let dims = ConsoleDimensions {
            width: 120,
            height: 40,
        };
        assert_eq!(dims.width, 120);
        assert_eq!(dims.height, 40);
    }

    // ========== PrintOptions Default Trait ==========

    #[test]
    fn test_print_options_implements_default() {
        // Default::default() uses derived defaults (empty strings)
        // PrintOptions::new() sets explicit defaults (sep=" ", end="\n")
        let default_options = PrintOptions::default();
        assert_eq!(default_options.sep, "");
        assert_eq!(default_options.end, "");

        // new() provides the typical defaults
        let new_options = PrintOptions::new();
        assert_eq!(new_options.sep, " ");
        assert_eq!(new_options.end, "\n");
    }

    // ========== Edge Case Tests ==========

    #[test]
    fn test_print_empty_string() {
        use std::sync::{Arc, Mutex};

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

        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .width(80)
            .file(Box::new(buffer.clone()))
            .build();

        console.print_plain("");

        let output = buffer.0.lock().unwrap();
        let result = String::from_utf8_lossy(&output);
        // Should only have newline
        assert_eq!(result.trim(), "");
    }

    #[test]
    fn test_print_unicode() {
        use std::sync::{Arc, Mutex};

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

        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .width(80)
            .file(Box::new(buffer.clone()))
            .build();

        console.print_plain("Hello 世界 🌍");

        let output = buffer.0.lock().unwrap();
        let result = String::from_utf8_lossy(&output);
        assert!(result.contains("世界"));
        assert!(result.contains("🌍"));
    }

    #[test]
    fn test_print_with_newlines() {
        use std::sync::{Arc, Mutex};

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

        let buffer = SharedBuffer(Arc::new(Mutex::new(Vec::new())));
        let console = Console::builder()
            .width(80)
            .file(Box::new(buffer.clone()))
            .build();

        console.print_plain("Line 1\nLine 2\nLine 3");

        let output = buffer.0.lock().unwrap();
        let result = String::from_utf8_lossy(&output);
        let lines: Vec<&str> = result.lines().collect();
        assert!(lines.len() >= 3);
    }

    #[test]
    fn test_overflow_crop() {
        let console = Console::builder().width(80).markup(false).build();
        let mut output = Vec::new();
        let mut options = PrintOptions::new()
            .with_no_wrap(true)
            .with_overflow(OverflowMethod::Crop);
        options.width = Some(5);
        options.no_newline = true;

        console
            .print_to(&mut output, "Hello World", &options)
            .expect("failed to print");

        let text = String::from_utf8(output).expect("invalid utf8");
        assert_eq!(text, "Hello");
    }

    // ========================================================================
    // Console I/O Error Path Tests (bd-3761)
    // ========================================================================

    /// A writer that always fails on write
    struct FailingWriter;

    impl Write for FailingWriter {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "write failed"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// A writer that fails on flush
    struct FlushFailingWriter {
        buffer: Vec<u8>,
    }

    impl FlushFailingWriter {
        fn new() -> Self {
            Self { buffer: Vec::new() }
        }
    }

    impl Write for FlushFailingWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.buffer.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("flush failed: disk full"))
        }
    }

    /// A writer that fails after N bytes
    struct LimitedWriter {
        limit: usize,
        written: usize,
    }

    impl LimitedWriter {
        fn new(limit: usize) -> Self {
            Self { limit, written: 0 }
        }
    }

    impl Write for LimitedWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.written >= self.limit {
                return Err(io::Error::new(io::ErrorKind::WriteZero, "buffer full"));
            }
            let available = self.limit - self.written;
            let to_write = buf.len().min(available);
            self.written += to_write;
            Ok(to_write)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// A writer that tracks operations for verification
    struct TrackingWriter {
        writes: Arc<Mutex<Vec<usize>>>,
        flushes: Arc<Mutex<usize>>,
    }

    impl TrackingWriter {
        fn new() -> Self {
            Self {
                writes: Arc::new(Mutex::new(Vec::new())),
                flushes: Arc::new(Mutex::new(0)),
            }
        }

        fn write_count(&self) -> usize {
            self.writes.lock().unwrap().len()
        }

        #[allow(dead_code)]
        fn flush_count(&self) -> usize {
            *self.flushes.lock().unwrap()
        }

        fn total_bytes(&self) -> usize {
            self.writes.lock().unwrap().iter().sum()
        }
    }

    impl Clone for TrackingWriter {
        fn clone(&self) -> Self {
            Self {
                writes: Arc::clone(&self.writes),
                flushes: Arc::clone(&self.flushes),
            }
        }
    }

    impl Write for TrackingWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.writes.lock().unwrap().push(buf.len());
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            *self.flushes.lock().unwrap() += 1;
            Ok(())
        }
    }

    #[test]
    fn test_io_write_failure() {
        // Test that write errors are properly propagated via print_to
        let console = Console::builder().width(80).markup(false).build();

        let mut failing_writer = FailingWriter;
        let result = console.print_to(&mut failing_writer, "Hello", &PrintOptions::new());

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    }

    #[test]
    fn test_io_write_partial() {
        // Test writer that accepts only partial writes
        let console = Console::builder().width(80).markup(false).build();

        let mut limited = LimitedWriter::new(5);
        let _result = console.print_to(&mut limited, "Hello World!", &PrintOptions::new());

        // May succeed partially or fail depending on implementation
        // The writer should have accepted at least some bytes
        assert!(limited.written > 0);
    }

    #[test]
    fn test_io_flush_failure() {
        // Test that flush errors are handled
        let mut writer = FlushFailingWriter::new();

        // Write should succeed
        let write_result = writer.write(b"Hello");
        assert!(write_result.is_ok());
        assert_eq!(write_result.unwrap(), 5);

        // Flush should fail
        let flush_result = writer.flush();
        assert!(flush_result.is_err());
        let err = flush_result.unwrap_err();
        assert!(err.to_string().contains("flush failed"));
    }

    #[test]
    fn test_io_write_segments_to_failing() {
        let console = Console::builder().width(80).markup(false).build();

        // Create segments
        let segments = vec![
            Segment::plain("Hello "),
            Segment::styled("World", Style::new().bold()),
        ];

        let mut failing_writer = FailingWriter;
        let result = console.print_segments_to(&mut failing_writer, &segments);

        assert!(result.is_err());
    }

    #[test]
    fn test_io_print_text_to_failing() {
        let console = Console::builder().width(80).markup(false).build();

        let text = Text::new("Hello World");
        let mut failing_writer = FailingWriter;
        let result = console.print_text_to(&mut failing_writer, &text);

        assert!(result.is_err());
    }

    #[test]
    fn test_io_write_tracking() {
        // Verify writes are actually occurring
        let tracking = TrackingWriter::new();
        let console = Console::builder()
            .width(80)
            .markup(false)
            .file(Box::new(tracking.clone()))
            .build();

        console.print_plain("Line 1");
        console.print_plain("Line 2");

        // Should have multiple writes
        assert!(tracking.write_count() >= 2, "Expected writes to occur");
        assert!(tracking.total_bytes() > 0, "Expected bytes written");
    }

    #[test]
    fn test_io_empty_write() {
        // Writing empty content should not cause errors
        let console = Console::builder().width(80).markup(false).build();

        let mut output = Vec::new();
        let result = console.print_to(&mut output, "", &PrintOptions::new().with_no_newline(true));

        assert!(result.is_ok());
        // Empty string with no_newline should produce empty output
        assert!(output.is_empty() || output == b"\n");
    }

    #[test]
    fn test_io_large_write() {
        // Test with a large string to ensure no buffer issues
        let console = Console::builder().width(1000).markup(false).build();

        let large_content = "x".repeat(10000);
        let mut output = Vec::new();
        let result = console.print_to(&mut output, &large_content, &PrintOptions::new());

        assert!(result.is_ok());
        // Should contain all the content plus newline
        assert!(output.len() >= 10000);
    }

    #[test]
    fn test_io_control_code_write_failure() {
        // Test that control code writes handle errors
        // Note: This tests internal behavior, so we use print_segments_to
        let console = Console::builder().width(80).markup(false).build();

        // Create a segment with control codes
        let segments = vec![Segment {
            text: std::borrow::Cow::Borrowed(""),
            style: None,
            control: Some(vec![ControlCode::new(ControlType::Home)]),
        }];

        let mut failing_writer = FailingWriter;
        let result = console.print_segments_to(&mut failing_writer, &segments);

        // Should handle the error (either succeed because control codes are skipped
        // in non-terminal mode, or fail gracefully)
        // The important thing is no panic
        let _ = result;
    }

    #[test]
    fn test_control_cursor_move_to_column_is_zero_based() {
        let console = Console::builder()
            .width(80)
            .markup(false)
            .force_terminal(true)
            .build();
        let segments = vec![Segment::control(vec![ControlCode::with_params_vec(
            ControlType::CursorMoveToColumn,
            vec![0],
        )])];
        let mut output = Vec::new();
        console
            .print_segments_to(&mut output, &segments)
            .expect("print_segments_to");

        assert_eq!(String::from_utf8(output).expect("utf8 output"), "\x1b[1G");
    }

    #[test]
    fn test_control_cursor_move_to_is_zero_based_xy() {
        let console = Console::builder()
            .width(80)
            .markup(false)
            .force_terminal(true)
            .build();
        let segments = vec![Segment::control(vec![ControlCode::with_params_vec(
            ControlType::CursorMoveTo,
            vec![3, 4],
        )])];
        let mut output = Vec::new();
        console
            .print_segments_to(&mut output, &segments)
            .expect("print_segments_to");

        assert_eq!(String::from_utf8(output).expect("utf8 output"), "\x1b[5;4H");
    }

    #[test]
    fn test_control_set_window_title_emits_empty_title_sequence() {
        let console = Console::builder()
            .width(80)
            .markup(false)
            .force_terminal(true)
            .build();
        let segments = vec![Segment {
            text: std::borrow::Cow::Borrowed(""),
            style: None,
            control: Some(vec![ControlCode::new(ControlType::SetWindowTitle)]),
        }];
        let mut output = Vec::new();
        console
            .print_segments_to(&mut output, &segments)
            .expect("print_segments_to");

        assert_eq!(
            String::from_utf8(output).expect("utf8 output"),
            "\x1b]0;\x07"
        );
    }

    #[test]
    fn test_io_error_types() {
        // Create writers with different error types
        struct NotFoundWriter;
        impl Write for NotFoundWriter {
            fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::NotFound, "file not found"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        struct PermissionWriter;
        impl Write for PermissionWriter {
            fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "access denied",
                ))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        // Verify different error types are preserved
        let console = Console::builder().width(80).markup(false).build();
        let mut not_found = NotFoundWriter;
        let result1 = console.print_to(&mut not_found, "test", &PrintOptions::new());
        assert!(matches!(
            result1.as_ref().map_err(std::io::Error::kind),
            Err(io::ErrorKind::NotFound)
        ));

        let mut permission = PermissionWriter;
        let result2 = console.print_to(&mut permission, "test", &PrintOptions::new());
        assert!(matches!(
            result2.as_ref().map_err(std::io::Error::kind),
            Err(io::ErrorKind::PermissionDenied)
        ));
    }

    #[test]
    fn test_io_concurrent_writes() {
        use std::thread;

        struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

        impl Clone for SharedBuffer {
            fn clone(&self) -> Self {
                Self(Arc::clone(&self.0))
            }
        }

        impl Write for SharedBuffer {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        // Test thread-safe writes to shared buffer
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let shared = SharedBuffer(Arc::clone(&buffer));
        let console = Console::builder()
            .width(80)
            .markup(false)
            .file(Box::new(shared))
            .build()
            .shared();

        // Spawn multiple threads writing concurrently
        let mut handles = vec![];
        for i in 0..4 {
            let console_clone = Arc::clone(&console);
            let handle = thread::spawn(move || {
                console_clone.print_plain(&format!("Thread {i}"));
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().expect("thread panicked");
        }

        // Verify all writes completed
        let output = buffer.lock().unwrap();
        let text = String::from_utf8_lossy(&output);
        // All 4 threads should have written something
        assert!(text.contains("Thread"), "Expected thread output");
    }

    #[test]
    fn test_io_interrupted_write() {
        // Test handling of interrupted writes (EINTR-like scenario)
        struct InterruptedWriter {
            attempts: Arc<Mutex<usize>>,
            succeed_after: usize,
        }

        impl Write for InterruptedWriter {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                let mut attempts = self.attempts.lock().unwrap();
                *attempts += 1;
                if *attempts <= self.succeed_after {
                    Err(io::Error::new(io::ErrorKind::Interrupted, "interrupted"))
                } else {
                    Ok(buf.len())
                }
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let console = Console::builder().width(80).markup(false).build();

        // Writer that returns Interrupted initially
        let attempts = Arc::new(Mutex::new(0));
        let mut writer = InterruptedWriter {
            attempts: Arc::clone(&attempts),
            succeed_after: 0, // Succeed on first try
        };

        let result = console.print_to(&mut writer, "test", &PrintOptions::new());
        assert!(
            result.is_ok()
                || result.as_ref().map_err(std::io::Error::kind) == Err(io::ErrorKind::Interrupted)
        );
    }
}
