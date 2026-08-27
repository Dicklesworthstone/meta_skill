//! Live display system for dynamic terminal updates.
//!
//! This module implements Rich-style Live updates with cursor control.

use std::io;
use std::io::{Read, Write};
use std::sync::{
    Arc, Mutex, RwLock,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::ansi::AnsiDecoder;
use crate::console::{Console, ConsoleOptions, RenderHook};
use crate::renderables::Renderable;
use crate::segment::{ControlCode, ControlType, Segment, split_lines};
use crate::style::Style;
use crate::sync::{lock_recover, read_recover, write_recover};
use crate::text::{JustifyMethod, OverflowMethod, Text};

use os_pipe::PipeReader;
use stdio_override::{StderrOverride, StdoutOverride};

/// Vertical overflow handling for Live renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerticalOverflowMethod {
    Crop,
    #[default]
    Ellipsis,
    Visible,
}

/// Configuration for Live.
#[derive(Debug, Clone)]
pub struct LiveOptions {
    pub screen: bool,
    pub auto_refresh: bool,
    pub refresh_per_second: f64,
    pub transient: bool,
    pub redirect_stdout: bool,
    pub redirect_stderr: bool,
    pub vertical_overflow: VerticalOverflowMethod,
}

impl Default for LiveOptions {
    fn default() -> Self {
        Self {
            screen: false,
            auto_refresh: true,
            refresh_per_second: 4.0,
            transient: false,
            redirect_stdout: true,
            redirect_stderr: true,
            vertical_overflow: VerticalOverflowMethod::Ellipsis,
        }
    }
}

/// Live display handle for dynamic terminal updates.
///
/// `Live` provides Rich-style live updates with cursor control, allowing
/// content to be updated in-place without scrolling.
///
/// # Thread Safety
///
/// `Live` is `Send + Sync` and can be cloned to share between threads. The
/// [`update`](Live::update) and [`refresh`](Live::refresh) methods are safe
/// to call concurrently.
///
/// When using auto-refresh, an internal thread handles periodic updates.
/// All internal state is protected by mutexes with poison recovery.
///
/// # Example
///
/// ```rust,ignore
/// use std::sync::Arc;
/// use std::thread;
/// use rich_rust::prelude::*;
///
/// let console = Arc::new(Console::new());
/// let live = Arc::new(Live::new(Arc::clone(&console))
///     .renderable(Text::new("Initial")));
///
/// live.start(true).unwrap();
///
/// // Safe to update from multiple threads
/// let handles: Vec<_> = (0..4).map(|i| {
///     let l = Arc::clone(&live);
///     thread::spawn(move || {
///         l.update(Text::new(format!("From thread {i}")), true);
///     })
/// }).collect();
///
/// for h in handles { h.join().unwrap(); }
/// live.stop().unwrap();
/// ```
#[derive(Clone)]
pub struct Live {
    inner: Arc<LiveInner>,
}

/// Write-only proxy that routes output through the Console during Live.
#[derive(Clone)]
pub struct LiveWriter {
    console: Arc<Console>,
    buffer: Vec<u8>,
    decoder: AnsiDecoder,
}

impl LiveWriter {
    #[must_use]
    pub fn new(console: Arc<Console>) -> Self {
        Self {
            console,
            buffer: Vec::new(),
            decoder: AnsiDecoder::new(),
        }
    }

    fn normalize_trailing_cr(line: &str) -> &str {
        line.strip_suffix('\r').unwrap_or(line)
    }
}

impl io::Write for LiveWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Match Python Rich's FileProxy behavior: buffer until newline, then decode ANSI
        // and print styled output.
        self.buffer.extend_from_slice(buf);

        let mut lines: Vec<Text> = Vec::new();
        while let Some(pos) = self.buffer.iter().position(|&b| b == b'\n') {
            let line_bytes: Vec<u8> = self.buffer.drain(..pos).collect();
            // Drain the newline itself.
            let _ = self.buffer.drain(..1);
            let line = String::from_utf8_lossy(&line_bytes);
            lines.push(
                self.decoder
                    .decode_line(Self::normalize_trailing_cr(line.as_ref())),
            );
        }

        if !lines.is_empty() {
            let sep = Text::new("\n");
            let mut joined = sep.join(lines.iter());
            joined.end = "\n".to_string();
            self.console.print_text(&joined);
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if !self.buffer.is_empty() {
            let remainder = String::from_utf8_lossy(&self.buffer).to_string();
            self.buffer.clear();
            let mut decoded = self
                .decoder
                .decode_line(Self::normalize_trailing_cr(&remainder));
            decoded.end = "\n".to_string();
            self.console.print_text(&decoded);
        }
        Ok(())
    }
}

type RenderableFactory = Arc<dyn Fn() -> Box<dyn Renderable + Send + Sync> + Send + Sync>;

pub(crate) struct LiveInner {
    console: Arc<Console>,
    options: Mutex<LiveOptions>,
    renderable: RwLock<Option<Box<dyn Renderable + Send + Sync>>>,
    get_renderable: Mutex<Option<RenderableFactory>>,
    started: AtomicBool,
    nested: AtomicBool,
    alt_screen_active: AtomicBool,
    refresh_stop: Arc<AtomicBool>,
    refresh_thread: Mutex<Option<JoinHandle<()>>>,
    live_render: Mutex<LiveRender>,
    stdio_redirect: Mutex<Option<StdioRedirect>>,
}

impl Live {
    /// Create a Live instance.
    #[must_use]
    pub fn new(console: Arc<Console>) -> Self {
        Self::with_options(console, LiveOptions::default())
    }

    /// Create a Live instance with explicit options.
    #[must_use]
    pub fn with_options(console: Arc<Console>, options: LiveOptions) -> Self {
        assert!(
            options.refresh_per_second > 0.0,
            "refresh_per_second must be > 0"
        );
        let mut options = options;
        if options.screen {
            options.transient = true;
        }
        Self {
            inner: Arc::new(LiveInner {
                console,
                options: Mutex::new(options),
                renderable: RwLock::new(None),
                get_renderable: Mutex::new(None),
                started: AtomicBool::new(false),
                nested: AtomicBool::new(false),
                alt_screen_active: AtomicBool::new(false),
                refresh_stop: Arc::new(AtomicBool::new(false)),
                refresh_thread: Mutex::new(None),
                live_render: Mutex::new(LiveRender::default()),
                stdio_redirect: Mutex::new(None),
            }),
        }
    }

    /// Set a static renderable to display.
    #[must_use]
    pub fn renderable<R>(self, renderable: R) -> Self
    where
        R: Renderable + Send + Sync + 'static,
    {
        *write_recover(&self.inner.renderable) = Some(Box::new(renderable));
        self
    }

    /// Set a callback to provide dynamic renderables.
    #[must_use]
    pub fn get_renderable<F>(self, callback: F) -> Self
    where
        F: Fn() -> Box<dyn Renderable + Send + Sync> + Send + Sync + 'static,
    {
        *lock_recover(&self.inner.get_renderable) = Some(Arc::new(callback));
        self
    }

    /// Start the Live display.
    pub fn start(&self, refresh: bool) -> io::Result<()> {
        if self.inner.started.swap(true, Ordering::SeqCst) {
            return Ok(());
        }

        if !self.inner.console.set_live(&self.inner) {
            self.inner.nested.store(true, Ordering::SeqCst);
            return Ok(());
        }

        let options = self.inner.options();
        if options.screen {
            self.inner.console.set_alt_screen(true)?;
            self.inner.alt_screen_active.store(true, Ordering::SeqCst);
        }

        self.inner.console.show_cursor(false)?;

        // Redirect stdout/stderr (process-wide) so external prints can be routed through Live.
        //
        // This only activates when stdout is actually a TTY (not when force_terminal is used),
        // to avoid interfering with piped output and parallel test harnesses.
        self.inner.maybe_start_stdio_redirect()?;

        self.inner
            .console
            .push_render_hook(Arc::clone(&self.inner) as Arc<dyn RenderHook>);

        if refresh {
            self.refresh()?;
        }

        if options.auto_refresh {
            Arc::clone(&self.inner).start_refresh_thread();
        }

        Ok(())
    }

    /// Stop the Live display.
    pub fn stop(&self) -> io::Result<()> {
        if !self.inner.started.swap(false, Ordering::SeqCst) {
            return Ok(());
        }

        self.inner.stop_refresh_thread();
        self.inner.console.clear_live();

        if self.inner.nested.load(Ordering::SeqCst) {
            return Ok(());
        }

        {
            let mut options = self.inner.options_mut();
            options.vertical_overflow = VerticalOverflowMethod::Visible;
        }

        if !self.inner.alt_screen_active.load(Ordering::SeqCst) && self.inner.console.is_terminal()
        {
            let _ = self.refresh();
            self.inner.console.line();
        }

        self.inner.console.pop_render_hook();
        let _ = self.inner.console.show_cursor(true);

        self.inner.stop_stdio_redirect();

        if self.inner.alt_screen_active.swap(false, Ordering::SeqCst) {
            let _ = self.inner.console.set_alt_screen(false);
        }

        if self.inner.options().transient && !self.inner.alt_screen_active.load(Ordering::SeqCst) {
            let controls = self.inner.live_render_controls_restore();
            let _ = self.inner.console.write_control_codes(controls);
        }

        Ok(())
    }

    /// Update the renderable content.
    pub fn update<R>(&self, renderable: R, refresh: bool)
    where
        R: Renderable + Send + Sync + 'static,
    {
        *write_recover(&self.inner.renderable) = Some(Box::new(renderable));
        if refresh {
            let _ = self.refresh();
        }
    }

    /// Refresh the live display.
    pub fn refresh(&self) -> io::Result<()> {
        self.inner.refresh_display()
    }

    /// Create a stdout proxy writer that routes output through the Console.
    #[must_use]
    pub fn stdout_proxy(&self) -> LiveWriter {
        LiveWriter::new(Arc::clone(&self.inner.console))
    }

    /// Create a stderr proxy writer that routes output through the Console.
    #[must_use]
    pub fn stderr_proxy(&self) -> LiveWriter {
        LiveWriter::new(Arc::clone(&self.inner.console))
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

impl LiveInner {
    fn options(&self) -> LiveOptions {
        lock_recover(&self.options).clone()
    }

    fn options_mut(&self) -> std::sync::MutexGuard<'_, LiveOptions> {
        lock_recover(&self.options)
    }

    fn maybe_start_stdio_redirect(&self) -> io::Result<()> {
        let options = self.options();
        if !options.redirect_stdout && !options.redirect_stderr {
            return Ok(());
        }

        // Never override process stdio when stdout isn't a real terminal.
        if !self.console.is_terminal_detected() || self.console.is_dumb_terminal() {
            return Ok(());
        }

        // Idempotent: only one redirect instance per LiveInner.
        if lock_recover(&self.stdio_redirect).is_some() {
            return Ok(());
        }

        let mut redirect = StdioRedirect::start(
            &self.console,
            options.redirect_stdout,
            options.redirect_stderr,
        )?;

        // If we redirected stdout, keep Console writing to the original stdout to avoid recursion.
        // (StdoutOverride itself writes to the pre-redirect stdout.)
        if let Some(stdout) = redirect.stdout_override.clone() {
            let original = self
                .console
                .swap_file(Box::new(OverrideWriter { inner: stdout }));
            redirect.console_original_writer = Some(original);
        }

        *lock_recover(&self.stdio_redirect) = Some(redirect);
        Ok(())
    }

    fn stop_stdio_redirect(&self) {
        let mut slot = lock_recover(&self.stdio_redirect);
        let Some(mut redirect) = slot.take() else {
            return;
        };

        // Restore console writer first so any final output is not forced to original stdout.
        if let Some(original) = redirect.console_original_writer.take() {
            let _ = self.console.swap_file(original);
        }

        redirect.stop();
    }

    fn current_renderable(
        &self,
        console: &Console,
        options: &ConsoleOptions,
    ) -> Vec<Segment<'static>> {
        let callback = lock_recover(&self.get_renderable).clone();
        if let Some(callback) = callback {
            let renderable = callback();
            return renderable
                .render(console, options)
                .into_iter()
                .map(Segment::into_owned)
                .collect();
        }

        {
            let slot = read_recover(&self.renderable);
            if let Some(renderable) = slot.as_ref() {
                return renderable
                    .render(console, options)
                    .into_iter()
                    .map(Segment::into_owned)
                    .collect();
            }
        }

        Vec::new()
    }

    fn render_stack_segments(
        &self,
        console: &Console,
        options: &ConsoleOptions,
    ) -> Vec<Segment<'static>> {
        let lives = console.live_stack_snapshot();
        if lives.is_empty() {
            return self.current_renderable(console, options);
        }

        let mut output = Vec::new();
        for (idx, live) in lives.iter().enumerate() {
            let segments = live.current_renderable(console, options);
            if idx > 0 && !segments.is_empty() {
                output.push(Segment::line());
            }
            output.extend(segments);
        }
        output
    }

    fn live_render_controls_restore(&self) -> Vec<ControlCode> {
        lock_recover(&self.live_render).restore_cursor_controls()
    }

    fn render_live_segments(
        &self,
        render: &mut LiveRender,
        console: &Console,
        options: &ConsoleOptions,
        vertical_overflow: VerticalOverflowMethod,
    ) -> Vec<Segment<'static>> {
        let raw_segments = self.render_stack_segments(console, options);
        let mut lines = split_lines(raw_segments.into_iter());

        let max_height = options.size.height;
        let mut needs_ellipsis = false;
        if max_height > 0 && lines.len() > max_height {
            match vertical_overflow {
                VerticalOverflowMethod::Crop => {
                    lines.truncate(max_height);
                }
                VerticalOverflowMethod::Ellipsis => {
                    if max_height == 1 {
                        lines.truncate(1);
                    } else {
                        lines.truncate(max_height - 1);
                        needs_ellipsis = true;
                    }
                }
                VerticalOverflowMethod::Visible => {}
            }
        }

        if needs_ellipsis {
            let width = options.max_width;
            let mut ellipsis = Text::styled("...", Style::new().dim());
            ellipsis.overflow = OverflowMethod::Crop;
            ellipsis.justify = JustifyMethod::Center;
            ellipsis.pad(width, JustifyMethod::Center);
            let ellipsis_segments = ellipsis
                .render("")
                .into_iter()
                .map(Segment::into_owned)
                .collect();
            lines.push(ellipsis_segments);
        }

        let mut max_width = 0usize;
        for line in &lines {
            let line_width: usize = line.iter().map(Segment::cell_length).sum();
            max_width = max_width.max(line_width);
        }
        render.shape = Some((max_width, lines.len()));

        let mut flattened = Vec::new();
        let last_index = lines.len().saturating_sub(1);
        for (idx, mut line) in lines.into_iter().enumerate() {
            flattened.append(&mut line);
            if idx < last_index {
                flattened.push(Segment::line());
            }
        }
        flattened
    }

    fn start_refresh_thread(self: &Arc<Self>) {
        if self.refresh_stop.load(Ordering::Relaxed) {
            self.refresh_stop.store(false, Ordering::Relaxed);
        }

        let inner = Arc::clone(self);
        let interval = {
            let options = self.options();
            Duration::from_secs_f64(1.0 / options.refresh_per_second)
        };
        let stop = Arc::clone(&self.refresh_stop);

        let handle = thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                thread::sleep(interval);
                if !stop.load(Ordering::Relaxed) {
                    let _ = inner.refresh_display();
                }
            }
        });

        *lock_recover(&self.refresh_thread) = Some(handle);
    }

    fn stop_refresh_thread(&self) {
        self.refresh_stop.store(true, Ordering::Relaxed);
        if let Some(handle) = lock_recover(&self.refresh_thread).take() {
            let _ = handle.join();
        }
    }

    fn refresh_display(&self) -> io::Result<()> {
        if self.nested.load(Ordering::SeqCst) {
            if let Some(parent) = self.console.live_stack_snapshot().first() {
                return parent.refresh_display();
            }
            return Ok(());
        }

        if (self.console.is_terminal() && !self.console.is_dumb_terminal())
            || !self.options().transient
        {
            self.console.print_segments(&[]);
        }
        Ok(())
    }
}

#[derive(Clone)]
struct OverrideWriter<T> {
    inner: Arc<Mutex<T>>,
}

impl<T: Write> Write for OverrideWriter<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        lock_recover(&*self.inner).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        lock_recover(&*self.inner).flush()
    }
}

struct StdioRedirect {
    stop: Arc<AtomicBool>,
    stdout_override: Option<Arc<Mutex<StdoutOverride>>>,
    stderr_override: Option<Arc<Mutex<StderrOverride>>>,
    stdout_reader: Option<JoinHandle<()>>,
    stderr_reader: Option<JoinHandle<()>>,
    pump: Option<JoinHandle<()>>,
    console_original_writer: Option<Box<dyn Write + Send>>,
}

impl StdioRedirect {
    fn start(
        console: &Arc<Console>,
        redirect_stdout: bool,
        redirect_stderr: bool,
    ) -> io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));

        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();

        let pump_console = Arc::clone(console);
        let pump_stop = Arc::clone(&stop);
        let pump = thread::spawn(move || {
            // Stream UTF-8 safely: keep any incomplete tail between chunks.
            let mut carry: Vec<u8> = Vec::new();

            while !pump_stop.load(Ordering::Relaxed) {
                let Ok(mut chunk) = rx.recv() else {
                    break;
                };
                if chunk.is_empty() {
                    continue;
                }

                if !carry.is_empty() {
                    carry.append(&mut chunk);
                    chunk = std::mem::take(&mut carry);
                }

                match String::from_utf8(chunk) {
                    Ok(s) => {
                        if !s.is_empty() {
                            pump_console.print_plain(&s);
                        }
                    }
                    Err(e) => {
                        let valid = e.utf8_error().valid_up_to();
                        let bytes = e.into_bytes();
                        let (ok_bytes, rest) = bytes.split_at(valid);
                        if !ok_bytes.is_empty() {
                            let s = String::from_utf8_lossy(ok_bytes);
                            pump_console.print_plain(&s);
                        }
                        carry.extend_from_slice(rest);
                    }
                }
            }

            if !carry.is_empty() {
                let s = String::from_utf8_lossy(&carry);
                pump_console.print_plain(&s);
            }
        });

        let mut stdout_override = None;
        let mut stderr_override = None;
        let mut stdout_reader = None;
        let mut stderr_reader = None;

        if redirect_stdout {
            let (reader, writer) = os_pipe::pipe()?;
            let guard = StdoutOverride::from_io(writer)?;
            let guard = Arc::new(Mutex::new(guard));
            stdout_override = Some(Arc::clone(&guard));
            stdout_reader = Some(Self::start_reader_thread(
                reader,
                tx.clone(),
                Arc::clone(&stop),
            ));
        }

        if redirect_stderr {
            let (reader, writer) = os_pipe::pipe()?;
            let guard = StderrOverride::from_io(writer)?;
            let guard = Arc::new(Mutex::new(guard));
            stderr_override = Some(Arc::clone(&guard));
            stderr_reader = Some(Self::start_reader_thread(
                reader,
                tx.clone(),
                Arc::clone(&stop),
            ));
        }

        Ok(Self {
            stop,
            stdout_override,
            stderr_override,
            stdout_reader,
            stderr_reader,
            pump: Some(pump),
            console_original_writer: None,
        })
    }

    fn start_reader_thread(
        mut reader: PipeReader,
        tx: std::sync::mpsc::Sender<Vec<u8>>,
        stop: Arc<AtomicBool>,
    ) -> JoinHandle<()> {
        thread::spawn(move || {
            let mut buf = [0u8; 8192];
            while !stop.load(Ordering::Relaxed) {
                match reader.read(&mut buf) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        let _ = tx.send(buf[..n].to_vec());
                    }
                    Err(_) => break,
                }
            }
        })
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);

        // Drop overrides first: this restores stdio and closes the pipe writer ends, unblocking readers.
        //
        // Drop stderr before stdout if it was created after stdout (out-of-order drops panic).
        self.stderr_override.take();
        self.stdout_override.take();

        if let Some(h) = self.stdout_reader.take() {
            let _ = h.join();
        }
        if let Some(h) = self.stderr_reader.take() {
            let _ = h.join();
        }

        // Close the channel so the pump exits.
        if let Some(h) = self.pump.take() {
            let _ = h.join();
        }
    }
}

impl RenderHook for LiveInner {
    fn process(&self, console: &Console, segments: &[Segment<'static>]) -> Vec<Segment<'static>> {
        let options = console.options();
        let overflow = self.options().vertical_overflow;

        let mut render = lock_recover(&self.live_render);

        let mut output = Vec::new();
        if console.is_interactive() {
            if self.alt_screen_active.load(Ordering::SeqCst) {
                output.push(Segment::control(vec![ControlCode::new(ControlType::Home)]));
            } else {
                let controls = render.position_cursor_controls();
                if !controls.is_empty() {
                    output.push(Segment::control(controls));
                }
            }
            output.extend_from_slice(segments);
            let live_segments = self.render_live_segments(&mut render, console, &options, overflow);
            output.extend(live_segments);
            output
        } else if !self.options().transient {
            output.extend_from_slice(segments);
            let live_segments = self.render_live_segments(&mut render, console, &options, overflow);
            output.extend(live_segments);
            output
        } else {
            segments.to_vec()
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct LiveRender {
    shape: Option<(usize, usize)>,
}

impl LiveRender {
    fn position_cursor_controls(&self) -> Vec<ControlCode> {
        let Some((_, height)) = self.shape else {
            return Vec::new();
        };
        if height == 0 {
            return Vec::new();
        }

        let mut controls = Vec::new();
        controls.push(ControlCode::new(ControlType::CarriageReturn));
        controls.push(ControlCode::with_params_vec(
            ControlType::EraseInLine,
            vec![2],
        ));

        if height > 1 {
            for _ in 0..(height - 1) {
                controls.push(ControlCode::with_params_vec(ControlType::CursorUp, vec![1]));
                controls.push(ControlCode::with_params_vec(
                    ControlType::EraseInLine,
                    vec![2],
                ));
            }
        }

        controls
    }

    fn restore_cursor_controls(&self) -> Vec<ControlCode> {
        let Some((_, height)) = self.shape else {
            return Vec::new();
        };
        if height == 0 {
            return Vec::new();
        }

        let mut controls = Vec::new();
        controls.push(ControlCode::new(ControlType::CarriageReturn));
        for _ in 0..height {
            controls.push(ControlCode::with_params_vec(ControlType::CursorUp, vec![1]));
            controls.push(ControlCode::with_params_vec(
                ControlType::EraseInLine,
                vec![2],
            ));
        }
        controls
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::ColorSystem;
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    #[derive(Clone)]
    struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

    impl SharedBuffer {
        fn new() -> Self {
            Self(Arc::new(Mutex::new(Vec::new())))
        }

        fn text(&self) -> String {
            let buf = self.0.lock().unwrap();
            String::from_utf8_lossy(&buf).to_string()
        }

        #[allow(dead_code)]
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

    fn make_test_console(buffer: SharedBuffer) -> Arc<Console> {
        Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer))
            .build()
            .shared()
    }

    // =========================================================================
    // LiveOptions Tests
    // =========================================================================

    #[test]
    fn test_live_options_default() {
        let options = LiveOptions::default();
        assert!(!options.screen);
        assert!(options.auto_refresh);
        assert!((options.refresh_per_second - 4.0).abs() < f64::EPSILON);
        assert!(!options.transient);
        assert!(options.redirect_stdout);
        assert!(options.redirect_stderr);
        assert_eq!(options.vertical_overflow, VerticalOverflowMethod::Ellipsis);
    }

    #[test]
    fn test_live_options_custom() {
        let options = LiveOptions {
            screen: true,
            auto_refresh: false,
            refresh_per_second: 10.0,
            transient: true,
            redirect_stdout: false,
            redirect_stderr: false,
            vertical_overflow: VerticalOverflowMethod::Crop,
        };
        assert!(options.screen);
        assert!(!options.auto_refresh);
        assert!((options.refresh_per_second - 10.0).abs() < f64::EPSILON);
        assert!(options.transient);
        assert!(!options.redirect_stdout);
        assert!(!options.redirect_stderr);
        assert_eq!(options.vertical_overflow, VerticalOverflowMethod::Crop);
    }

    // =========================================================================
    // VerticalOverflowMethod Tests
    // =========================================================================

    #[test]
    fn test_vertical_overflow_default() {
        let method = VerticalOverflowMethod::default();
        assert_eq!(method, VerticalOverflowMethod::Ellipsis);
    }

    #[test]
    fn test_vertical_overflow_variants() {
        let crop = VerticalOverflowMethod::Crop;
        let ellipsis = VerticalOverflowMethod::Ellipsis;
        let visible = VerticalOverflowMethod::Visible;

        assert_ne!(crop, ellipsis);
        assert_ne!(ellipsis, visible);
        assert_ne!(crop, visible);
    }

    // =========================================================================
    // Live Creation Tests
    // =========================================================================

    #[test]
    fn test_live_new() {
        let buffer = SharedBuffer::new();
        let console = make_test_console(buffer);
        let live = Live::new(console);
        // Should create without panic
        assert!(!live.inner.started.load(Ordering::SeqCst));
    }

    #[test]
    fn test_live_with_options() {
        let buffer = SharedBuffer::new();
        let console = make_test_console(buffer);
        let options = LiveOptions {
            auto_refresh: false,
            refresh_per_second: 2.0,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options);
        // Check options were applied
        let stored = live.inner.options();
        assert!(!stored.auto_refresh);
        assert!((stored.refresh_per_second - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_live_screen_enables_transient() {
        let buffer = SharedBuffer::new();
        let console = make_test_console(buffer);
        let options = LiveOptions {
            screen: true,
            transient: false, // Should be set to true when screen is true
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options);
        let stored = live.inner.options();
        assert!(
            stored.transient,
            "transient should be true when screen is true"
        );
    }

    #[test]
    #[should_panic(expected = "refresh_per_second must be > 0")]
    fn test_live_zero_refresh_rate_panics() {
        let buffer = SharedBuffer::new();
        let console = make_test_console(buffer);
        let options = LiveOptions {
            refresh_per_second: 0.0,
            ..LiveOptions::default()
        };
        let _live = Live::with_options(console, options);
    }

    #[test]
    #[should_panic(expected = "refresh_per_second must be > 0")]
    fn test_live_negative_refresh_rate_panics() {
        let buffer = SharedBuffer::new();
        let console = make_test_console(buffer);
        let options = LiveOptions {
            refresh_per_second: -1.0,
            ..LiveOptions::default()
        };
        let _live = Live::with_options(console, options);
    }

    // =========================================================================
    // Live Renderable Tests
    // =========================================================================

    #[test]
    fn test_live_renderable_builder() {
        let buffer = SharedBuffer::new();
        let console = make_test_console(buffer);
        let live = Live::new(console).renderable(Text::new("Content"));
        // Check that renderable was set
        let slot = live.inner.renderable.read().unwrap();
        assert!(slot.is_some());
    }

    #[test]
    fn test_live_get_renderable_callback() {
        let buffer = SharedBuffer::new();
        let console = make_test_console(buffer);
        let counter = Arc::new(Mutex::new(0));
        let counter_clone = Arc::clone(&counter);

        let live = Live::new(console).get_renderable(move || {
            let mut c = counter_clone.lock().unwrap();
            *c += 1;
            Box::new(Text::new(format!("Count: {}", *c)))
        });

        // Check that callback was set
        let slot = live.inner.get_renderable.lock().unwrap();
        assert!(slot.is_some());
    }

    #[test]
    fn test_live_refresh_outputs_renderable() {
        let buffer = SharedBuffer::new();
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();
        let options = LiveOptions {
            auto_refresh: false,
            screen: false,
            transient: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options).renderable(Text::new("Hello"));
        live.start(true).expect("start");
        let _ = live.refresh();
        live.stop().expect("stop");

        let text = buffer.text();
        assert!(text.contains("Hello"), "output missing: {text}");
    }

    // =========================================================================
    // Live Start/Stop Lifecycle Tests
    // =========================================================================

    #[test]
    fn test_live_start_stop() {
        let buffer = SharedBuffer::new();
        let console = make_test_console(buffer);
        let options = LiveOptions {
            auto_refresh: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options);

        assert!(!live.inner.started.load(Ordering::SeqCst));
        live.start(false).expect("start should succeed");
        assert!(live.inner.started.load(Ordering::SeqCst));
        live.stop().expect("stop should succeed");
        assert!(!live.inner.started.load(Ordering::SeqCst));
    }

    #[test]
    fn test_live_start_idempotent() {
        let buffer = SharedBuffer::new();
        let console = make_test_console(buffer);
        let options = LiveOptions {
            auto_refresh: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options);

        live.start(false).expect("first start");
        live.start(false).expect("second start should be no-op");
        live.start(false).expect("third start should be no-op");
        assert!(live.inner.started.load(Ordering::SeqCst));
        live.stop().expect("stop");
    }

    #[test]
    fn test_live_stop_idempotent() {
        let buffer = SharedBuffer::new();
        let console = make_test_console(buffer);
        let options = LiveOptions {
            auto_refresh: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options);

        live.start(false).expect("start");
        live.stop().expect("first stop");
        live.stop().expect("second stop should be no-op");
        live.stop().expect("third stop should be no-op");
        assert!(!live.inner.started.load(Ordering::SeqCst));
    }

    #[test]
    fn test_live_drop_stops() {
        let buffer = SharedBuffer::new();
        let console = make_test_console(buffer);
        let options = LiveOptions {
            auto_refresh: false,
            ..LiveOptions::default()
        };

        {
            let live = Live::with_options(console.clone(), options);
            live.start(false).expect("start");
            assert!(live.inner.started.load(Ordering::SeqCst));
            // Drop live here
        }
        // After drop, the Live should have stopped
    }

    // =========================================================================
    // Live Update Tests
    // =========================================================================

    #[test]
    fn test_live_update_renderable() {
        let buffer = SharedBuffer::new();
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();
        let options = LiveOptions {
            auto_refresh: false,
            screen: false,
            transient: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options).renderable(Text::new("First"));
        live.start(true).expect("start");

        // Update to new content
        live.update(Text::new("Second"), true);

        live.stop().expect("stop");

        let text = buffer.text();
        assert!(
            text.contains("Second"),
            "should contain updated content: {text}"
        );
    }

    #[test]
    fn test_live_update_without_refresh() {
        let buffer = SharedBuffer::new();
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();
        let options = LiveOptions {
            auto_refresh: false,
            screen: false,
            transient: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options).renderable(Text::new("Initial"));
        live.start(false).expect("start");

        // Update without refresh
        live.update(Text::new("Updated"), false);

        // Manually refresh
        let _ = live.refresh();

        live.stop().expect("stop");
    }

    // =========================================================================
    // Vertical Overflow Tests
    // =========================================================================

    #[test]
    fn test_live_vertical_overflow_ellipsis() {
        let buffer = SharedBuffer::new();
        let console = Console::builder()
            .force_terminal(true)
            .width(10)
            .height(2)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();
        let options = LiveOptions {
            auto_refresh: false,
            screen: false,
            transient: false,
            vertical_overflow: VerticalOverflowMethod::Ellipsis,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options).renderable(Text::new("a\nb\nc"));
        live.start(true).expect("start");
        let _ = live.refresh();
        live.stop().expect("stop");

        let text = buffer.text();
        assert!(text.contains("..."), "expected ellipsis, got: {text}");
    }

    #[test]
    fn test_live_vertical_overflow_crop() {
        let buffer = SharedBuffer::new();
        let console = Console::builder()
            .force_terminal(true)
            .width(20)
            .height(2)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();
        let options = LiveOptions {
            auto_refresh: false,
            screen: false,
            transient: false,
            vertical_overflow: VerticalOverflowMethod::Crop,
            ..LiveOptions::default()
        };
        let live =
            Live::with_options(console, options).renderable(Text::new("line1\nline2\nline3"));
        live.start(true).expect("start");
        let _ = live.refresh();
        live.stop().expect("stop");

        let text = buffer.text();
        // With crop, should not have ellipsis
        assert!(
            !text.contains("..."),
            "crop should not add ellipsis: {text}"
        );
    }

    #[test]
    fn test_live_vertical_overflow_visible() {
        let buffer = SharedBuffer::new();
        let console = Console::builder()
            .force_terminal(true)
            .width(20)
            .height(2)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();
        let options = LiveOptions {
            auto_refresh: false,
            screen: false,
            transient: false,
            vertical_overflow: VerticalOverflowMethod::Visible,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options)
            .renderable(Text::new("visible1\nvisible2\nvisible3"));
        live.start(true).expect("start");
        let _ = live.refresh();
        live.stop().expect("stop");

        // All lines should be visible
        let text = buffer.text();
        // No truncation or ellipsis
        assert!(
            !text.contains("..."),
            "visible should not add ellipsis: {text}"
        );
    }

    // =========================================================================
    // LiveWriter Tests
    // =========================================================================

    #[test]
    fn test_live_writer_proxy() {
        let buffer = SharedBuffer::new();
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();
        let live = Live::new(console.clone());
        let mut writer = live.stdout_proxy();
        let _ = writer.write_all(b"proxy output");

        let text = buffer.text();
        assert!(
            !text.contains("proxy output"),
            "LiveWriter should buffer until newline or flush"
        );

        let _ = writer.write_all(b"\n");
        let text = buffer.text();
        assert!(text.contains("proxy output"));
    }

    #[test]
    fn test_live_writer_stderr_proxy() {
        let buffer = SharedBuffer::new();
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();
        let live = Live::new(console.clone());
        let mut writer = live.stderr_proxy();
        let _ = writer.write_all(b"stderr content");

        let text = buffer.text();
        assert!(
            !text.contains("stderr content"),
            "LiveWriter should buffer until newline or flush"
        );

        let _ = writer.write_all(b"\n");
        let text = buffer.text();
        assert!(text.contains("stderr content"));
    }

    #[test]
    fn test_live_writer_decodes_ansi_sgr() {
        let buffer = SharedBuffer::new();
        let console = Console::builder()
            .force_terminal(true)
            .color_system(ColorSystem::Standard)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();
        let live = Live::new(console.clone());
        let mut writer = live.stdout_proxy();
        let _ = writer.write_all(b"\x1b[31mred\x1b[0m\n");

        let text = buffer.text();
        assert!(text.contains("red"));
        assert!(
            text.contains("\x1b["),
            "expected ANSI output when terminal is forced"
        );
    }

    #[test]
    fn test_live_writer_flush() {
        let buffer = SharedBuffer::new();
        let console = make_test_console(buffer);
        let live = Live::new(console);
        let mut writer = live.stdout_proxy();

        // Flush should not panic
        writer.flush().expect("flush should succeed");
    }

    #[test]
    fn test_live_writer_write_returns_length() {
        let buffer = SharedBuffer::new();
        let console = make_test_console(buffer);
        let live = Live::new(console);
        let mut writer = live.stdout_proxy();

        let data = b"test data";
        let written = writer.write(data).expect("write");
        assert_eq!(written, data.len());
    }

    #[test]
    fn test_live_writer_crlf_newline_preserves_text() {
        let buffer = SharedBuffer::new();
        let console = make_test_console(buffer.clone());
        let live = Live::new(console);
        let mut writer = live.stdout_proxy();

        writer
            .write_all(b"crlf content\r\n")
            .expect("write_all should succeed");

        let text = buffer.text();
        assert!(text.contains("crlf content"));
    }

    #[test]
    fn test_live_writer_flush_with_trailing_cr_preserves_text() {
        let buffer = SharedBuffer::new();
        let console = make_test_console(buffer.clone());
        let live = Live::new(console);
        let mut writer = live.stdout_proxy();

        writer
            .write_all(b"flush content\r")
            .expect("write_all should succeed");
        writer.flush().expect("flush should succeed");

        let text = buffer.text();
        assert!(text.contains("flush content"));
    }

    // =========================================================================
    // LiveRender Tests
    // =========================================================================

    #[test]
    fn test_live_render_default() {
        let render = LiveRender::default();
        assert!(render.shape.is_none());
    }

    #[test]
    fn test_live_render_position_cursor_no_shape() {
        let render = LiveRender::default();
        let controls = render.position_cursor_controls();
        assert_eq!(controls.len(), 0);
    }

    #[test]
    fn test_live_render_position_cursor_zero_height() {
        let render = LiveRender {
            shape: Some((10, 0)),
        };
        let controls = render.position_cursor_controls();
        assert_eq!(controls.len(), 0);
    }

    #[test]
    fn test_live_render_position_cursor_single_line() {
        let render = LiveRender {
            shape: Some((10, 1)),
        };
        let controls = render.position_cursor_controls();
        // Should have CarriageReturn and EraseInLine
        assert_ne!(controls.len(), 0);
        assert_eq!(controls.len(), 2);
    }

    #[test]
    fn test_live_render_position_cursor_multiple_lines() {
        let render = LiveRender {
            shape: Some((10, 3)),
        };
        let controls = render.position_cursor_controls();
        // CR + EraseLine + (CursorUp + EraseLine) * 2
        // Total: 2 + 2*2 = 6
        assert_eq!(controls.len(), 6);
    }

    #[test]
    fn test_live_render_restore_cursor_no_shape() {
        let render = LiveRender::default();
        let controls = render.restore_cursor_controls();
        assert_eq!(controls.len(), 0);
    }

    #[test]
    fn test_live_render_restore_cursor_zero_height() {
        let render = LiveRender {
            shape: Some((10, 0)),
        };
        let controls = render.restore_cursor_controls();
        assert_eq!(controls.len(), 0);
    }

    #[test]
    fn test_live_render_restore_cursor_with_height() {
        let render = LiveRender {
            shape: Some((10, 2)),
        };
        let controls = render.restore_cursor_controls();
        // CR + (CursorUp + EraseLine) * height
        // Total: 1 + 2*2 = 5
        assert_eq!(controls.len(), 5);
    }

    // =========================================================================
    // Thread Safety Tests
    // =========================================================================

    #[test]
    fn test_live_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Live>();
    }

    #[test]
    fn test_live_clone() {
        let buffer = SharedBuffer::new();
        let console = make_test_console(buffer);
        let live = Live::new(console);
        let _cloned = live.clone();
        // Should not panic, clones share inner state
    }

    #[test]
    fn test_live_concurrent_updates() {
        let buffer = SharedBuffer::new();
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();
        let options = LiveOptions {
            auto_refresh: false,
            screen: false,
            transient: false,
            ..LiveOptions::default()
        };
        let live = Arc::new(Live::with_options(console, options));
        live.start(false).expect("start");

        let handles: Vec<_> = (0..4)
            .map(|i| {
                let live = Arc::clone(&live);
                thread::spawn(move || {
                    for j in 0..10 {
                        live.update(Text::new(format!("Thread {i} update {j}")), false);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("thread should not panic");
        }

        live.stop().expect("stop");
    }

    // =========================================================================
    // Auto-refresh Tests
    // =========================================================================

    #[test]
    fn test_live_auto_refresh_disabled() {
        let buffer = SharedBuffer::new();
        let console = make_test_console(buffer.clone());
        let options = LiveOptions {
            auto_refresh: false,
            refresh_per_second: 100.0, // High rate to detect if running
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options).renderable(Text::new("Static"));
        live.start(true).expect("start");

        // Sleep a bit
        thread::sleep(Duration::from_millis(50));

        let _text_before = buffer.text();
        thread::sleep(Duration::from_millis(50));
        // With auto_refresh disabled, no background thread should be running
        // (Content should be the same unless manually refreshed)

        live.stop().expect("stop");
    }

    #[test]
    fn test_live_auto_refresh_enabled() {
        let buffer = SharedBuffer::new();
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();
        let counter = Arc::new(Mutex::new(0));
        let counter_clone = Arc::clone(&counter);

        let options = LiveOptions {
            auto_refresh: true,
            refresh_per_second: 20.0, // 50ms intervals
            screen: false,
            transient: false,
            ..LiveOptions::default()
        };

        let live = Live::with_options(console, options).get_renderable(move || {
            let mut c = counter_clone.lock().unwrap();
            *c += 1;
            Box::new(Text::new(format!("Refresh count: {}", *c)))
        });

        live.start(true).expect("start");
        thread::sleep(Duration::from_millis(200)); // Should trigger several refreshes
        live.stop().expect("stop");

        // Counter should have been incremented multiple times
        let final_count = *counter.lock().unwrap();
        assert!(
            final_count >= 2,
            "expected multiple refreshes, got {final_count}",
        );
    }

    // =========================================================================
    // Edge Cases
    // =========================================================================

    #[test]
    fn test_live_empty_renderable() {
        let buffer = SharedBuffer::new();
        let console = make_test_console(buffer.clone());
        let options = LiveOptions {
            auto_refresh: false,
            screen: false,
            transient: false,
            ..LiveOptions::default()
        };
        // No renderable set
        let live = Live::with_options(console, options);
        live.start(true).expect("start");
        let _ = live.refresh();
        live.stop().expect("stop");
        // Should not panic with no renderable
    }

    #[test]
    fn test_live_refresh_before_start() {
        let buffer = SharedBuffer::new();
        let console = make_test_console(buffer);
        let live = Live::new(console).renderable(Text::new("Content"));

        // Refresh before start should not panic
        let _ = live.refresh();
    }

    #[test]
    fn test_live_refresh_after_stop() {
        let buffer = SharedBuffer::new();
        let console = make_test_console(buffer);
        let options = LiveOptions {
            auto_refresh: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options).renderable(Text::new("Content"));
        live.start(false).expect("start");
        live.stop().expect("stop");

        // Refresh after stop should not panic
        let _ = live.refresh();
    }
}
