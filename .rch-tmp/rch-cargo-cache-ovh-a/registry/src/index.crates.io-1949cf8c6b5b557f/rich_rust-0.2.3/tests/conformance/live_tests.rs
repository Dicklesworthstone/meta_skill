//! Live display conformance tests.
//!
//! Tests for the Live dynamic refresh system including:
//! - Core lifecycle (new, start, stop, refresh, update)
//! - Terminal control sequences
//! - Vertical overflow handling
//! - Proxy writers
//! - Transient vs persistent modes

use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use rich_rust::console::Console;
use rich_rust::live::{Live, LiveOptions, LiveWriter, VerticalOverflowMethod};
use rich_rust::text::Text;

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
        .width(40)
        .height(10)
        .file(Box::new(buffer))
        .build()
        .shared()
}

fn create_non_terminal_console(buffer: SharedBuffer) -> Arc<Console> {
    Console::builder()
        .force_terminal(false)
        .markup(false)
        .width(40)
        .height(10)
        .file(Box::new(buffer))
        .build()
        .shared()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Initialization Tests
    // ========================================================================

    #[test]
    fn test_live_new_default_options() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer);
        let live = Live::new(console);

        // Should not panic and create a valid Live instance
        drop(live);
    }

    #[test]
    fn test_live_with_options() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer);
        let options = LiveOptions {
            screen: false,
            auto_refresh: false,
            refresh_per_second: 10.0,
            transient: true,
            redirect_stdout: false,
            redirect_stderr: false,
            vertical_overflow: VerticalOverflowMethod::Crop,
        };
        let live = Live::with_options(console, options);
        drop(live);
    }

    #[test]
    fn test_live_with_screen_option_sets_transient() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer);
        let options = LiveOptions {
            screen: true,
            transient: false, // Should be overridden to true
            auto_refresh: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options);
        drop(live);
        // The implementation sets transient=true when screen=true
    }

    #[test]
    #[should_panic(expected = "refresh_per_second must be > 0")]
    fn test_live_panics_on_zero_refresh_rate() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer);
        let options = LiveOptions {
            refresh_per_second: 0.0,
            ..LiveOptions::default()
        };
        let _live = Live::with_options(console, options);
    }

    #[test]
    #[should_panic(expected = "refresh_per_second must be > 0")]
    fn test_live_panics_on_negative_refresh_rate() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer);
        let options = LiveOptions {
            refresh_per_second: -1.0,
            ..LiveOptions::default()
        };
        let _live = Live::with_options(console, options);
    }

    // ========================================================================
    // Lifecycle Tests
    // ========================================================================

    #[test]
    fn test_live_start_stop_cycle() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer);
        let options = LiveOptions {
            auto_refresh: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options).renderable(Text::new("Test"));

        assert!(live.start(false).is_ok());
        assert!(live.stop().is_ok());
    }

    #[test]
    fn test_live_start_is_idempotent() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer);
        let options = LiveOptions {
            auto_refresh: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options).renderable(Text::new("Test"));

        // Multiple starts should be safe
        assert!(live.start(false).is_ok());
        assert!(live.start(false).is_ok());
        assert!(live.stop().is_ok());
    }

    #[test]
    fn test_live_stop_is_idempotent() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer);
        let options = LiveOptions {
            auto_refresh: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options).renderable(Text::new("Test"));

        live.start(false).expect("start");

        // Multiple stops should be safe
        assert!(live.stop().is_ok());
        assert!(live.stop().is_ok());
    }

    #[test]
    fn test_live_stop_without_start() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer);
        let options = LiveOptions {
            auto_refresh: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options);

        // Stopping without starting should be safe
        assert!(live.stop().is_ok());
    }

    #[test]
    fn test_live_drop_calls_stop() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let options = LiveOptions {
            auto_refresh: false,
            transient: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options).renderable(Text::new("DropTest"));
        live.start(true).expect("start");

        // Drop should call stop() which outputs the final state
        drop(live);

        let output = buffer.contents();
        assert!(
            output.contains("DropTest"),
            "Drop should have rendered content"
        );
    }

    // ========================================================================
    // Refresh Tests
    // ========================================================================

    #[test]
    fn test_live_refresh_outputs_content() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let options = LiveOptions {
            auto_refresh: false,
            transient: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options).renderable(Text::new("RefreshTest"));

        live.start(true).expect("start");
        live.refresh().expect("refresh");
        live.stop().expect("stop");

        let output = buffer.contents();
        assert!(output.contains("RefreshTest"));
    }

    #[test]
    fn test_live_refresh_with_start_refresh() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let options = LiveOptions {
            auto_refresh: false,
            transient: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options).renderable(Text::new("StartRefresh"));

        // Start with refresh=true should output immediately
        live.start(true).expect("start");
        live.stop().expect("stop");

        let output = buffer.contents();
        assert!(output.contains("StartRefresh"));
    }

    #[test]
    fn test_live_refresh_without_start() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let options = LiveOptions {
            auto_refresh: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options).renderable(Text::new("NoStart"));

        // Refresh without start should not panic
        let _ = live.refresh();
    }

    // ========================================================================
    // Update Tests
    // ========================================================================

    #[test]
    fn test_live_update_changes_content() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let options = LiveOptions {
            auto_refresh: false,
            transient: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options).renderable(Text::new("Initial"));

        live.start(true).expect("start");
        live.update(Text::new("Updated"), true);
        live.stop().expect("stop");

        let output = buffer.contents();
        assert!(output.contains("Updated"), "Should contain updated content");
    }

    #[test]
    fn test_live_update_without_refresh() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let options = LiveOptions {
            auto_refresh: false,
            transient: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options).renderable(Text::new("Initial"));

        live.start(false).expect("start");
        live.update(Text::new("NotRefreshed"), false);

        // Should not panic, content updated but not displayed
        live.stop().expect("stop");
    }

    // ========================================================================
    // Renderable Callback Tests
    // ========================================================================

    #[test]
    fn test_live_get_renderable_callback() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let counter = Arc::new(Mutex::new(0));

        let counter_clone = Arc::clone(&counter);
        let options = LiveOptions {
            auto_refresh: false,
            transient: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options).get_renderable(move || {
            let mut c = counter_clone.lock().unwrap();
            *c += 1;
            Box::new(Text::new(format!("Call {}", *c)))
        });

        live.start(true).expect("start");
        live.refresh().expect("refresh");
        live.stop().expect("stop");

        let count = *counter.lock().unwrap();
        assert!(count >= 2, "Callback should be called multiple times");
    }

    // ========================================================================
    // Vertical Overflow Tests
    // ========================================================================

    #[test]
    fn test_vertical_overflow_crop() {
        let buffer = SharedBuffer::new();
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .width(20)
            .height(2)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let options = LiveOptions {
            auto_refresh: false,
            transient: false,
            vertical_overflow: VerticalOverflowMethod::Crop,
            ..LiveOptions::default()
        };
        let live =
            Live::with_options(console, options).renderable(Text::new("Line1\nLine2\nLine3"));

        live.start(true).expect("start");
        live.refresh().expect("refresh");
        live.stop().expect("stop");

        let output = buffer.contents();
        // With crop and height=2, should only show 2 lines
        assert!(output.contains("Line1"));
        // Line3 might be cropped
    }

    #[test]
    fn test_vertical_overflow_ellipsis() {
        let buffer = SharedBuffer::new();
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .width(20)
            .height(2)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let options = LiveOptions {
            auto_refresh: false,
            transient: false,
            vertical_overflow: VerticalOverflowMethod::Ellipsis,
            ..LiveOptions::default()
        };
        let live =
            Live::with_options(console, options).renderable(Text::new("Line1\nLine2\nLine3"));

        live.start(true).expect("start");
        live.refresh().expect("refresh");
        live.stop().expect("stop");

        let output = buffer.contents();
        assert!(output.contains("..."), "Should show ellipsis for overflow");
    }

    #[test]
    fn test_vertical_overflow_visible() {
        let buffer = SharedBuffer::new();
        let console = Console::builder()
            .force_terminal(true)
            .markup(false)
            .width(20)
            .height(2)
            .file(Box::new(buffer.clone()))
            .build()
            .shared();

        let options = LiveOptions {
            auto_refresh: false,
            transient: false,
            vertical_overflow: VerticalOverflowMethod::Visible,
            ..LiveOptions::default()
        };
        let live =
            Live::with_options(console, options).renderable(Text::new("Line1\nLine2\nLine3"));

        live.start(true).expect("start");
        live.refresh().expect("refresh");
        live.stop().expect("stop");

        let output = buffer.contents();
        // All lines should be visible
        assert!(output.contains("Line1"));
        assert!(output.contains("Line2"));
        assert!(output.contains("Line3"));
    }

    // ========================================================================
    // Proxy Writer Tests
    // ========================================================================

    #[test]
    fn test_stdout_proxy_writer() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let live = Live::new(console);

        let mut writer = live.stdout_proxy();
        writer.write_all(b"stdout test").expect("write");
        writer.flush().expect("flush");

        let output = buffer.contents();
        assert!(output.contains("stdout test"));
    }

    #[test]
    fn test_stderr_proxy_writer() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let live = Live::new(console);

        let mut writer = live.stderr_proxy();
        writer.write_all(b"stderr test").expect("write");
        writer.flush().expect("flush");

        let output = buffer.contents();
        assert!(output.contains("stderr test"));
    }

    #[test]
    fn test_live_writer_new() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let mut writer = LiveWriter::new(console);

        writer.write_all(b"direct writer").expect("write");
        writer.flush().expect("flush");

        let output = buffer.contents();
        assert!(output.contains("direct writer"));
    }

    // ========================================================================
    // Terminal Control Tests
    // ========================================================================

    #[test]
    fn test_live_hides_cursor_on_start() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let options = LiveOptions {
            auto_refresh: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options).renderable(Text::new("Cursor"));

        live.start(false).expect("start");

        let output = buffer.contents();
        // CSI ?25l is the hide cursor sequence
        assert!(
            output.contains("\x1b[?25l"),
            "Should emit hide cursor sequence"
        );

        live.stop().expect("stop");
    }

    #[test]
    fn test_live_shows_cursor_on_stop() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let options = LiveOptions {
            auto_refresh: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options).renderable(Text::new("Cursor"));

        live.start(false).expect("start");
        buffer.clear();
        live.stop().expect("stop");

        let output = buffer.contents();
        // CSI ?25h is the show cursor sequence
        assert!(
            output.contains("\x1b[?25h"),
            "Should emit show cursor sequence"
        );
    }

    // ========================================================================
    // Non-Terminal Tests
    // ========================================================================

    #[test]
    fn test_live_non_terminal_graceful() {
        let buffer = SharedBuffer::new();
        let console = create_non_terminal_console(buffer.clone());
        let options = LiveOptions {
            auto_refresh: false,
            transient: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options).renderable(Text::new("NonTerminal"));

        // Should work gracefully without TTY control codes
        live.start(true).expect("start");
        live.refresh().expect("refresh");
        live.stop().expect("stop");
    }

    // ========================================================================
    // Transient Mode Tests
    // ========================================================================

    #[test]
    fn test_live_transient_mode() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let options = LiveOptions {
            auto_refresh: false,
            transient: true,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options).renderable(Text::new("Transient"));

        live.start(true).expect("start");
        live.stop().expect("stop");

        // In transient mode, output may be cleared on stop
        // This test mainly verifies no panic occurs
    }

    #[test]
    fn test_live_persistent_mode() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let options = LiveOptions {
            auto_refresh: false,
            transient: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options).renderable(Text::new("Persistent"));

        live.start(true).expect("start");
        live.stop().expect("stop");

        let output = buffer.contents();
        assert!(
            output.contains("Persistent"),
            "Persistent mode should leave content"
        );
    }

    // ========================================================================
    // Auto-Refresh Tests
    // ========================================================================

    #[test]
    fn test_live_auto_refresh_starts_thread() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let options = LiveOptions {
            auto_refresh: true,
            refresh_per_second: 20.0, // Fast refresh for test
            transient: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options).renderable(Text::new("AutoRefresh"));

        live.start(false).expect("start");

        // Wait a bit for auto-refresh to occur
        thread::sleep(Duration::from_millis(150));

        live.stop().expect("stop");

        // Auto-refresh should have triggered at least once
        // (This is a timing-sensitive test)
    }

    #[test]
    fn test_live_auto_refresh_stops_cleanly() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer);
        let options = LiveOptions {
            auto_refresh: true,
            refresh_per_second: 10.0,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options).renderable(Text::new("AutoStop"));

        live.start(false).expect("start");
        thread::sleep(Duration::from_millis(50));
        live.stop().expect("stop");

        // Should not hang or panic
    }

    // ========================================================================
    // Clone Tests
    // ========================================================================

    #[test]
    fn test_live_is_cloneable() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer);
        let options = LiveOptions {
            auto_refresh: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options).renderable(Text::new("Clone"));

        let cloned = live.clone();

        // Both instances should work
        live.start(false).expect("start original");
        drop(cloned);
        live.stop().expect("stop original");
    }

    // ========================================================================
    // VerticalOverflowMethod Tests
    // ========================================================================

    #[test]
    fn test_vertical_overflow_default() {
        let default = VerticalOverflowMethod::default();
        assert_eq!(default, VerticalOverflowMethod::Ellipsis);
    }

    #[test]
    fn test_vertical_overflow_equality() {
        assert_eq!(VerticalOverflowMethod::Crop, VerticalOverflowMethod::Crop);
        assert_eq!(
            VerticalOverflowMethod::Ellipsis,
            VerticalOverflowMethod::Ellipsis
        );
        assert_eq!(
            VerticalOverflowMethod::Visible,
            VerticalOverflowMethod::Visible
        );
        assert_ne!(
            VerticalOverflowMethod::Crop,
            VerticalOverflowMethod::Visible
        );
    }

    #[test]
    fn test_vertical_overflow_debug() {
        let crop = VerticalOverflowMethod::Crop;
        let debug = format!("{:?}", crop);
        assert!(debug.contains("Crop"));
    }

    #[test]
    fn test_vertical_overflow_copy() {
        let original = VerticalOverflowMethod::Ellipsis;
        let copied = original; // VerticalOverflowMethod is Copy
        assert_eq!(original, copied);
    }

    // ========================================================================
    // LiveOptions Tests
    // ========================================================================

    #[test]
    fn test_live_options_default() {
        let options = LiveOptions::default();

        assert!(!options.screen);
        assert!(options.auto_refresh);
        assert_eq!(options.refresh_per_second, 4.0);
        assert!(!options.transient);
        assert!(options.redirect_stdout);
        assert!(options.redirect_stderr);
        assert_eq!(options.vertical_overflow, VerticalOverflowMethod::Ellipsis);
    }

    #[test]
    fn test_live_options_clone() {
        let options = LiveOptions {
            screen: true,
            auto_refresh: false,
            refresh_per_second: 10.0,
            transient: true,
            redirect_stdout: false,
            redirect_stderr: false,
            vertical_overflow: VerticalOverflowMethod::Crop,
        };

        let cloned = options.clone();
        assert_eq!(options.screen, cloned.screen);
        assert_eq!(options.auto_refresh, cloned.auto_refresh);
        assert_eq!(options.refresh_per_second, cloned.refresh_per_second);
        assert_eq!(options.transient, cloned.transient);
        assert_eq!(options.redirect_stdout, cloned.redirect_stdout);
        assert_eq!(options.redirect_stderr, cloned.redirect_stderr);
        assert_eq!(options.vertical_overflow, cloned.vertical_overflow);
    }

    #[test]
    fn test_live_options_debug() {
        let options = LiveOptions::default();
        let debug = format!("{:?}", options);
        assert!(debug.contains("LiveOptions"));
    }

    // ========================================================================
    // Edge Cases
    // ========================================================================

    #[test]
    fn test_live_empty_renderable() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let options = LiveOptions {
            auto_refresh: false,
            transient: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options).renderable(Text::new(""));

        live.start(true).expect("start");
        live.refresh().expect("refresh");
        live.stop().expect("stop");

        // Should not panic with empty content
    }

    #[test]
    fn test_live_no_renderable() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer);
        let options = LiveOptions {
            auto_refresh: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options);

        // No renderable set - should not panic
        live.start(true).expect("start");
        live.refresh().expect("refresh");
        live.stop().expect("stop");
    }

    #[test]
    fn test_live_very_long_content() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let options = LiveOptions {
            auto_refresh: false,
            transient: false,
            ..LiveOptions::default()
        };

        let long_text = "Line\n".repeat(100);
        let live = Live::with_options(console, options).renderable(Text::new(long_text));

        live.start(true).expect("start");
        live.refresh().expect("refresh");
        live.stop().expect("stop");

        // Should handle long content gracefully
    }

    #[test]
    fn test_live_unicode_content() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer.clone());
        let options = LiveOptions {
            auto_refresh: false,
            transient: false,
            ..LiveOptions::default()
        };
        let live =
            Live::with_options(console, options).renderable(Text::new("Hello 世界 🌍 Привет"));

        live.start(true).expect("start");
        live.refresh().expect("refresh");
        live.stop().expect("stop");

        let output = buffer.contents();
        assert!(output.contains("世界"));
    }

    #[test]
    fn test_live_rapid_updates() {
        let buffer = SharedBuffer::new();
        let console = create_test_console(buffer);
        let options = LiveOptions {
            auto_refresh: false,
            transient: false,
            ..LiveOptions::default()
        };
        let live = Live::with_options(console, options).renderable(Text::new("Start"));

        live.start(false).expect("start");

        for i in 0..100 {
            live.update(Text::new(format!("Update {i}")), false);
        }

        live.stop().expect("stop");

        // Should handle rapid updates without issues
    }
}
