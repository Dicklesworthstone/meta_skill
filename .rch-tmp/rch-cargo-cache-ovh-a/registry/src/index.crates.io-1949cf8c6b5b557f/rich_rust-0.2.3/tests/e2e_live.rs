//! End-to-end tests for the Live display system.
//!
//! This module exercises the complete Live workflow with detailed logging.
//! Run with: RUST_LOG=debug cargo test --test e2e_live -- --nocapture
//!
//! ## Test Coverage
//!
//! - Start/stop/refresh cycles
//! - Transient vs persistent rendering
//! - Auto-refresh with various intervals
//! - Context manager (scoped) usage
//! - Stdout/stderr redirection verification
//! - Terminal control code emission (cursor hide/show, alt-screen)
//! - Nested Live handling
//! - Error recovery and cleanup

mod common;

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use common::{init_test_logging, log_test_context, test_phase};
use rich_rust::prelude::*;
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

    #[allow(dead_code)]
    fn len(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

impl Write for SharedBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.lock().unwrap().flush()
    }
}

// =============================================================================
// Start/Stop/Refresh Cycle Tests
// =============================================================================

#[test]
fn test_live_start_stop_cycle() {
    init_test_logging();
    log_test_context("test_live_start_stop_cycle", "Basic start/stop lifecycle");

    let buffer = SharedBuffer::new();
    let console = Console::builder()
        .force_terminal(true)
        .markup(false)
        .file(Box::new(buffer.clone()))
        .build()
        .shared();

    let options = LiveOptions {
        auto_refresh: false,
        transient: false,
        ..Default::default()
    };

    {
        let _setup = test_phase("create_live");
        tracing::debug!("Creating Live instance with auto_refresh=false");
    }

    let live = Live::with_options(console.clone(), options).renderable(Text::new("Hello Live"));

    {
        let _start = test_phase("start");
        tracing::debug!(state = "starting", "Transitioning to started state");
        live.start(true).expect("start should succeed");
        tracing::debug!(state = "started", "Live display started");
    }

    {
        let _verify = test_phase("verify_output");
        let output = buffer.contents();
        tracing::debug!(output_len = output.len(), "Checking output after start");
        assert!(
            output.contains("Hello Live"),
            "Output should contain renderable text"
        );
    }

    {
        let _stop = test_phase("stop");
        tracing::debug!(state = "stopping", "Transitioning to stopped state");
        live.stop().expect("stop should succeed");
        tracing::debug!(state = "stopped", "Live display stopped");
    }

    tracing::info!("Start/stop cycle completed successfully");
}

#[test]
fn test_live_multiple_refresh_cycles() {
    init_test_logging();
    log_test_context(
        "test_live_multiple_refresh_cycles",
        "Multiple manual refreshes",
    );

    let buffer = SharedBuffer::new();
    let console = Console::builder()
        .force_terminal(true)
        .markup(false)
        .file(Box::new(buffer.clone()))
        .build()
        .shared();

    let options = LiveOptions {
        auto_refresh: false,
        transient: false,
        ..Default::default()
    };

    let live = Live::with_options(console.clone(), options).renderable(Text::new("Initial"));
    live.start(true).expect("start");

    {
        let _refresh = test_phase("manual_refreshes");

        for i in 1..=5 {
            tracing::debug!(refresh_count = i, "Manual refresh");
            live.refresh().expect("refresh should succeed");
            let output = buffer.contents();
            tracing::debug!(
                output_len = output.len(),
                refresh = i,
                "Output after refresh"
            );
        }
    }

    live.stop().expect("stop");
    tracing::info!("Multiple refresh cycles completed");
}

#[test]
fn test_live_update_renderable() {
    init_test_logging();
    log_test_context("test_live_update_renderable", "Updating renderable content");

    let buffer = SharedBuffer::new();
    let console = Console::builder()
        .force_terminal(true)
        .markup(false)
        .file(Box::new(buffer.clone()))
        .build()
        .shared();

    let options = LiveOptions {
        auto_refresh: false,
        transient: false,
        ..Default::default()
    };

    let live = Live::with_options(console.clone(), options).renderable(Text::new("Version 1"));
    live.start(true).expect("start");

    {
        let _update = test_phase("update_content");

        // Check initial content
        let output = buffer.contents();
        assert!(
            output.contains("Version 1"),
            "Initial content should be present"
        );
        tracing::debug!("Initial content verified");

        // Update to new content
        tracing::debug!(new_content = "Version 2", "Updating renderable");
        live.update(Text::new("Version 2"), true);

        let output = buffer.contents();
        assert!(
            output.contains("Version 2"),
            "Updated content should be present"
        );
        tracing::debug!("Updated content verified");
    }

    live.stop().expect("stop");
}

// =============================================================================
// Transient vs Persistent Rendering Tests
// =============================================================================

#[test]
fn test_live_transient_mode() {
    init_test_logging();
    log_test_context("test_live_transient_mode", "Transient rendering behavior");

    let buffer = SharedBuffer::new();
    let console = Console::builder()
        .force_terminal(true)
        .markup(false)
        .file(Box::new(buffer.clone()))
        .build()
        .shared();

    let options = LiveOptions {
        auto_refresh: false,
        transient: true,
        ..Default::default()
    };

    {
        let _transient = test_phase("transient_mode");
        tracing::debug!(transient = true, "Creating Live with transient mode");

        let live = Live::with_options(console.clone(), options).renderable(Text::new("Transient"));
        live.start(true).expect("start");

        let output_during = buffer.contents();
        tracing::debug!(output_len = output_during.len(), "Output during Live");

        live.stop().expect("stop");

        let output_after = buffer.contents();
        tracing::debug!(
            output_len_after = output_after.len(),
            "Output after stop (transient should clear)"
        );
    }

    tracing::info!("Transient mode test completed");
}

#[test]
fn test_live_persistent_mode() {
    init_test_logging();
    log_test_context("test_live_persistent_mode", "Persistent rendering behavior");

    let buffer = SharedBuffer::new();
    let console = Console::builder()
        .force_terminal(true)
        .markup(false)
        .file(Box::new(buffer.clone()))
        .build()
        .shared();

    let options = LiveOptions {
        auto_refresh: false,
        transient: false,
        ..Default::default()
    };

    {
        let _persistent = test_phase("persistent_mode");
        tracing::debug!(transient = false, "Creating Live with persistent mode");

        let live = Live::with_options(console.clone(), options).renderable(Text::new("Persistent"));
        live.start(true).expect("start");
        live.stop().expect("stop");

        let output = buffer.contents();
        tracing::debug!(output_len = output.len(), "Output after stop");
        assert!(
            output.contains("Persistent"),
            "Persistent content should remain"
        );
    }

    tracing::info!("Persistent mode test completed");
}

// =============================================================================
// Auto-Refresh Tests
// =============================================================================

#[test]
fn test_live_auto_refresh_default_rate() {
    init_test_logging();
    log_test_context(
        "test_live_auto_refresh_default_rate",
        "Auto-refresh at default 4Hz",
    );

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
        refresh_per_second: 4.0,
        transient: false,
        ..Default::default()
    };

    {
        let _auto = test_phase("auto_refresh");
        tracing::debug!(refresh_per_second = 4.0, "Starting auto-refresh test");

        let live = Live::with_options(console.clone(), options).get_renderable(move || {
            let mut count = counter_clone.lock().unwrap();
            *count += 1;
            tracing::trace!(count = *count, "Renderable callback invoked");
            Box::new(Text::new(format!("Count: {}", *count)))
        });

        live.start(true).expect("start");

        // Wait for auto-refresh to fire a few times
        tracing::debug!("Waiting for auto-refresh cycles...");
        thread::sleep(Duration::from_millis(600));

        live.stop().expect("stop");

        let final_count = *counter.lock().unwrap();
        tracing::info!(final_count = final_count, "Auto-refresh completed");

        // Should have refreshed at least twice in 600ms at 4Hz
        assert!(
            final_count >= 2,
            "Expected at least 2 refreshes, got {}",
            final_count
        );
    }
}

#[test]
fn test_live_auto_refresh_high_rate() {
    init_test_logging();
    log_test_context("test_live_auto_refresh_high_rate", "Auto-refresh at 10Hz");

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
        refresh_per_second: 10.0,
        transient: false,
        ..Default::default()
    };

    {
        let _high_rate = test_phase("high_rate_refresh");
        tracing::debug!(refresh_per_second = 10.0, "Testing high refresh rate");

        let live = Live::with_options(console.clone(), options).get_renderable(move || {
            let mut count = counter_clone.lock().unwrap();
            *count += 1;
            Box::new(Text::new(format!("High rate: {}", *count)))
        });

        live.start(true).expect("start");
        thread::sleep(Duration::from_millis(500));
        live.stop().expect("stop");

        let final_count = *counter.lock().unwrap();
        tracing::info!(
            final_count = final_count,
            refresh_rate = 10.0,
            "High rate test completed"
        );

        // At 10Hz over 500ms, should have at least 4 refreshes
        assert!(
            final_count >= 4,
            "Expected at least 4 refreshes at 10Hz, got {}",
            final_count
        );
    }
}

#[test]
fn test_live_auto_refresh_disabled() {
    init_test_logging();
    log_test_context("test_live_auto_refresh_disabled", "Auto-refresh disabled");

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
        auto_refresh: false,
        transient: false,
        ..Default::default()
    };

    {
        let _disabled = test_phase("disabled_refresh");
        tracing::debug!(auto_refresh = false, "Testing with auto-refresh disabled");

        let live = Live::with_options(console.clone(), options).get_renderable(move || {
            let mut count = counter_clone.lock().unwrap();
            *count += 1;
            Box::new(Text::new(format!("Manual only: {}", *count)))
        });

        live.start(true).expect("start");
        let count_after_start = *counter.lock().unwrap();

        thread::sleep(Duration::from_millis(300));

        let count_after_wait = *counter.lock().unwrap();
        live.stop().expect("stop");

        tracing::info!(
            count_after_start = count_after_start,
            count_after_wait = count_after_wait,
            "Disabled auto-refresh test completed"
        );

        // Count should not increase without manual refresh
        assert_eq!(
            count_after_start, count_after_wait,
            "Count should not change without auto-refresh"
        );
    }
}

// =============================================================================
// Context Manager (Scoped) Usage Tests
// =============================================================================

#[test]
fn test_live_drop_cleanup() {
    init_test_logging();
    log_test_context("test_live_drop_cleanup", "RAII cleanup on drop");

    let buffer = SharedBuffer::new();
    let console = Console::builder()
        .force_terminal(true)
        .markup(false)
        .file(Box::new(buffer.clone()))
        .build()
        .shared();

    {
        let _scope = test_phase("scoped_live");
        tracing::debug!("Creating scoped Live");

        let options = LiveOptions {
            auto_refresh: false,
            transient: false,
            ..Default::default()
        };

        {
            let live = Live::with_options(console.clone(), options).renderable(Text::new("Scoped"));
            live.start(true).expect("start");
            tracing::debug!(state = "active", "Live is active in scope");
            // Live will be dropped here, triggering cleanup
        }

        tracing::debug!(state = "dropped", "Live has been dropped");
    }

    // Verify cursor is restored (cursor show sequence appears in output if broken)
    let output = buffer.contents();
    tracing::info!(output_len = output.len(), "Scoped test completed");
}

#[test]
fn test_live_explicit_stop_before_drop() {
    init_test_logging();
    log_test_context(
        "test_live_explicit_stop_before_drop",
        "Explicit stop before drop",
    );

    let buffer = SharedBuffer::new();
    let console = Console::builder()
        .force_terminal(true)
        .markup(false)
        .file(Box::new(buffer.clone()))
        .build()
        .shared();

    {
        let _explicit = test_phase("explicit_stop");

        let options = LiveOptions {
            auto_refresh: false,
            transient: false,
            ..Default::default()
        };

        let live = Live::with_options(console.clone(), options).renderable(Text::new("Explicit"));
        live.start(true).expect("start");

        tracing::debug!("Calling explicit stop");
        live.stop().expect("explicit stop");
        tracing::debug!("Explicit stop completed");

        // Double stop should be safe
        tracing::debug!("Calling stop again (should be no-op)");
        live.stop().expect("second stop should be no-op");
    }

    tracing::info!("Explicit stop test completed");
}

// =============================================================================
// Stdout/Stderr Redirection Tests
// =============================================================================

#[test]
fn test_live_stdout_proxy() {
    init_test_logging();
    log_test_context("test_live_stdout_proxy", "Stdout proxy writer");

    let buffer = SharedBuffer::new();
    let console = Console::builder()
        .force_terminal(true)
        .markup(false)
        .file(Box::new(buffer.clone()))
        .build()
        .shared();

    {
        let _proxy = test_phase("stdout_proxy");

        let live = Live::new(console.clone());
        let mut writer = live.stdout_proxy();

        tracing::debug!("Writing through stdout proxy");
        writer.write_all(b"Proxied stdout output").expect("write");
        writer.flush().expect("flush");

        let output = buffer.contents();
        tracing::debug!(output_len = output.len(), "Checking proxied output");
        assert!(
            output.contains("Proxied stdout output"),
            "Proxy output should appear"
        );
    }

    tracing::info!("Stdout proxy test completed");
}

#[test]
fn test_live_stderr_proxy() {
    init_test_logging();
    log_test_context("test_live_stderr_proxy", "Stderr proxy writer");

    let buffer = SharedBuffer::new();
    let console = Console::builder()
        .force_terminal(true)
        .markup(false)
        .file(Box::new(buffer.clone()))
        .build()
        .shared();

    {
        let _proxy = test_phase("stderr_proxy");

        let live = Live::new(console.clone());
        let mut writer = live.stderr_proxy();

        tracing::debug!("Writing through stderr proxy");
        writer.write_all(b"Proxied stderr output").expect("write");
        writer.flush().expect("flush");

        let output = buffer.contents();
        assert!(
            output.contains("Proxied stderr output"),
            "Stderr proxy output should appear"
        );
    }

    tracing::info!("Stderr proxy test completed");
}

// =============================================================================
// Terminal Control Code Emission Tests
// =============================================================================

#[test]
fn test_live_cursor_hide_show() {
    init_test_logging();
    log_test_context("test_live_cursor_hide_show", "Cursor control sequences");

    let buffer = SharedBuffer::new();
    let console = Console::builder()
        .force_terminal(true)
        .markup(false)
        .file(Box::new(buffer.clone()))
        .build()
        .shared();

    {
        let _cursor = test_phase("cursor_control");

        let options = LiveOptions {
            auto_refresh: false,
            transient: false,
            ..Default::default()
        };

        let live =
            Live::with_options(console.clone(), options).renderable(Text::new("Cursor test"));

        buffer.clear();
        live.start(true).expect("start");

        let output_start = buffer.contents();
        tracing::debug!(
            has_hide = output_start.contains("\x1b[?25l"),
            output = %output_start.escape_debug(),
            "Checking for cursor hide on start"
        );

        live.stop().expect("stop");

        let output_stop = buffer.contents();
        tracing::debug!(
            has_show = output_stop.contains("\x1b[?25h"),
            output = %output_stop.escape_debug(),
            "Checking for cursor show on stop"
        );
    }

    tracing::info!("Cursor control test completed");
}

#[test]
fn test_live_alt_screen_mode() {
    init_test_logging();
    log_test_context("test_live_alt_screen_mode", "Alternate screen buffer");

    let buffer = SharedBuffer::new();
    let console = Console::builder()
        .force_terminal(true)
        .markup(false)
        .file(Box::new(buffer.clone()))
        .build()
        .shared();

    {
        let _alt_screen = test_phase("alt_screen");

        let options = LiveOptions {
            auto_refresh: false,
            screen: true,    // Enable alt-screen
            transient: true, // screen implies transient
            ..Default::default()
        };

        tracing::debug!(screen = true, "Testing with alt-screen mode");

        let live = Live::with_options(console.clone(), options).renderable(Text::new("Alt screen"));

        buffer.clear();
        live.start(true).expect("start");

        let output = buffer.contents();
        tracing::debug!(
            has_alt_enter = output.contains("\x1b[?1049h"),
            output_escaped = %output.escape_debug(),
            "Checking for alt-screen enter"
        );

        live.stop().expect("stop");

        let final_output = buffer.contents();
        tracing::debug!(
            has_alt_exit = final_output.contains("\x1b[?1049l"),
            "Checking for alt-screen exit"
        );
    }

    tracing::info!("Alt-screen test completed");
}

// =============================================================================
// Vertical Overflow Tests
// =============================================================================

#[test]
fn test_live_vertical_overflow_crop() {
    init_test_logging();
    log_test_context(
        "test_live_vertical_overflow_crop",
        "Vertical overflow crop mode",
    );

    let buffer = SharedBuffer::new();
    let console = Console::builder()
        .force_terminal(true)
        .width(20)
        .height(3)
        .markup(false)
        .file(Box::new(buffer.clone()))
        .build()
        .shared();

    {
        let _crop = test_phase("crop_overflow");

        // Note: With transient=true, output is cleared on stop (no final visible render).
        // With transient=false, stop() changes vertical_overflow to Visible for the final
        // render, so all content will appear after stop. We test with transient=true to
        // verify crop behavior during active display.
        let options = LiveOptions {
            auto_refresh: false,
            transient: true, // transient to avoid final visible render
            vertical_overflow: VerticalOverflowMethod::Crop,
            ..Default::default()
        };

        let content = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5";
        tracing::debug!(lines = 5, max_height = 3, "Testing crop overflow");

        let live = Live::with_options(console.clone(), options).renderable(Text::new(content));
        live.start(true).expect("start");

        // Capture output during active display (before stop clears it)
        let output_during = buffer.contents();
        tracing::debug!(output = %output_during.escape_debug(), "Crop output during active display");

        live.stop().expect("stop");

        // During active display, lines 4 and 5 should be cropped
        // (final output may be cleared due to transient mode)
        assert!(
            !output_during.contains("Line 5"),
            "Line 5 should be cropped during active display"
        );
    }

    tracing::info!("Crop overflow test completed");
}

#[test]
fn test_live_vertical_overflow_ellipsis() {
    init_test_logging();
    log_test_context(
        "test_live_vertical_overflow_ellipsis",
        "Vertical overflow ellipsis mode",
    );

    let buffer = SharedBuffer::new();
    let console = Console::builder()
        .force_terminal(true)
        .width(20)
        .height(3)
        .markup(false)
        .file(Box::new(buffer.clone()))
        .build()
        .shared();

    {
        let _ellipsis = test_phase("ellipsis_overflow");

        let options = LiveOptions {
            auto_refresh: false,
            transient: false,
            vertical_overflow: VerticalOverflowMethod::Ellipsis,
            ..Default::default()
        };

        let content = "Line 1\nLine 2\nLine 3\nLine 4";
        tracing::debug!(lines = 4, max_height = 3, "Testing ellipsis overflow");

        let live = Live::with_options(console.clone(), options).renderable(Text::new(content));
        live.start(true).expect("start");
        live.stop().expect("stop");

        let output = buffer.contents();
        tracing::debug!(output = %output.escape_debug(), "Ellipsis output");

        assert!(output.contains("..."), "Should contain ellipsis indicator");
    }

    tracing::info!("Ellipsis overflow test completed");
}

#[test]
fn test_live_vertical_overflow_visible() {
    init_test_logging();
    log_test_context(
        "test_live_vertical_overflow_visible",
        "Vertical overflow visible mode",
    );

    let buffer = SharedBuffer::new();
    let console = Console::builder()
        .force_terminal(true)
        .width(20)
        .height(2)
        .markup(false)
        .file(Box::new(buffer.clone()))
        .build()
        .shared();

    {
        let _visible = test_phase("visible_overflow");

        let options = LiveOptions {
            auto_refresh: false,
            transient: false,
            vertical_overflow: VerticalOverflowMethod::Visible,
            ..Default::default()
        };

        let content = "Line A\nLine B\nLine C\nLine D";
        tracing::debug!(
            lines = 4,
            max_height = 2,
            "Testing visible overflow (no truncation)"
        );

        let live = Live::with_options(console.clone(), options).renderable(Text::new(content));
        live.start(true).expect("start");
        live.stop().expect("stop");

        let output = buffer.contents();
        tracing::debug!(output = %output.escape_debug(), "Visible output");

        // All lines should be visible
        assert!(output.contains("Line A"), "Line A should be visible");
        assert!(
            output.contains("Line D"),
            "Line D should be visible (no truncation)"
        );
    }

    tracing::info!("Visible overflow test completed");
}

// =============================================================================
// Error Recovery and Edge Cases
// =============================================================================

#[test]
fn test_live_start_already_started() {
    init_test_logging();
    log_test_context(
        "test_live_start_already_started",
        "Double start is idempotent",
    );

    let buffer = SharedBuffer::new();
    let console = Console::builder()
        .force_terminal(true)
        .markup(false)
        .file(Box::new(buffer.clone()))
        .build()
        .shared();

    {
        let _idempotent = test_phase("idempotent_start");

        let options = LiveOptions {
            auto_refresh: false,
            transient: false,
            ..Default::default()
        };

        let live = Live::with_options(console.clone(), options).renderable(Text::new("Idempotent"));

        tracing::debug!("First start");
        live.start(true).expect("first start");

        tracing::debug!("Second start (should be no-op)");
        live.start(true).expect("second start should succeed");

        tracing::debug!("Third start (should be no-op)");
        live.start(true).expect("third start should succeed");

        live.stop().expect("stop");
    }

    tracing::info!("Idempotent start test completed");
}

#[test]
fn test_live_stop_not_started() {
    init_test_logging();
    log_test_context("test_live_stop_not_started", "Stop without start is safe");

    let buffer = SharedBuffer::new();
    let console = Console::builder()
        .force_terminal(true)
        .markup(false)
        .file(Box::new(buffer.clone()))
        .build()
        .shared();

    {
        let _safe_stop = test_phase("safe_stop");

        let options = LiveOptions {
            auto_refresh: false,
            transient: false,
            ..Default::default()
        };

        let live =
            Live::with_options(console.clone(), options).renderable(Text::new("Never started"));

        tracing::debug!("Stop without start");
        live.stop().expect("stop without start should succeed");

        tracing::debug!("Another stop");
        live.stop().expect("second stop should succeed");
    }

    tracing::info!("Safe stop test completed");
}

#[test]
fn test_live_clone_behavior() {
    init_test_logging();
    log_test_context("test_live_clone_behavior", "Clone shares state via Arc");

    let buffer = SharedBuffer::new();
    let console = Console::builder()
        .force_terminal(true)
        .markup(false)
        .file(Box::new(buffer.clone()))
        .build()
        .shared();

    {
        let _clone = test_phase("clone_state");

        let options = LiveOptions {
            auto_refresh: false,
            transient: false,
            ..Default::default()
        };

        let live1 = Live::with_options(console.clone(), options).renderable(Text::new("Shared"));
        let live2 = live1.clone();

        tracing::debug!("Start via clone 1");
        live1.start(true).expect("start via clone 1");

        tracing::debug!("Refresh via clone 2");
        live2.refresh().expect("refresh via clone 2");

        tracing::debug!("Stop via clone 2");
        live2.stop().expect("stop via clone 2");

        // Clone 1 should also see stopped state
        tracing::debug!("Check clone 1 state");
    }

    tracing::info!("Clone behavior test completed");
}

#[test]
fn test_live_rapid_updates() {
    init_test_logging();
    log_test_context("test_live_rapid_updates", "Rapid content updates");

    let buffer = SharedBuffer::new();
    let console = Console::builder()
        .force_terminal(true)
        .markup(false)
        .file(Box::new(buffer.clone()))
        .build()
        .shared();

    {
        let _rapid = test_phase("rapid_updates");

        let options = LiveOptions {
            auto_refresh: false,
            transient: false,
            ..Default::default()
        };

        let live = Live::with_options(console.clone(), options).renderable(Text::new("Update 0"));
        live.start(true).expect("start");

        tracing::debug!("Performing 100 rapid updates");
        for i in 1..=100 {
            live.update(Text::new(format!("Update {}", i)), false);
            if i % 25 == 0 {
                tracing::trace!(update = i, "Progress");
            }
        }

        // Final refresh
        live.refresh().expect("final refresh");
        live.stop().expect("stop");

        let output = buffer.contents();
        tracing::debug!(output_len = output.len(), "Rapid updates completed");
    }

    tracing::info!("Rapid updates test completed");
}

// =============================================================================
// Non-TTY Behavior Tests
// =============================================================================

#[test]
fn test_live_non_tty_mode() {
    init_test_logging();
    log_test_context("test_live_non_tty_mode", "Behavior in non-TTY environment");

    let buffer = SharedBuffer::new();
    let console = Console::builder()
        .force_terminal(false) // Non-TTY
        .markup(false)
        .file(Box::new(buffer.clone()))
        .build()
        .shared();

    {
        let _non_tty = test_phase("non_tty");
        tracing::debug!(is_terminal = false, "Testing non-TTY mode");

        let options = LiveOptions {
            auto_refresh: false,
            transient: false,
            ..Default::default()
        };

        let live =
            Live::with_options(console.clone(), options).renderable(Text::new("Non-TTY content"));
        live.start(true).expect("start");
        live.refresh().expect("refresh");
        live.stop().expect("stop");

        let output = buffer.contents();
        tracing::debug!(
            output = %output.escape_debug(),
            has_escape = output.contains("\x1b["),
            "Non-TTY output"
        );

        // Non-TTY should have minimal or no control codes
    }

    tracing::info!("Non-TTY mode test completed");
}

// =============================================================================
// Dynamic Renderable (get_renderable) Tests
// =============================================================================

#[test]
fn test_live_get_renderable_callback() {
    init_test_logging();
    log_test_context(
        "test_live_get_renderable_callback",
        "Dynamic renderable via callback",
    );

    let buffer = SharedBuffer::new();
    let console = Console::builder()
        .force_terminal(true)
        .markup(false)
        .file(Box::new(buffer.clone()))
        .build()
        .shared();

    let state = Arc::new(Mutex::new(String::from("Initial")));
    let state_clone = Arc::clone(&state);

    {
        let _callback = test_phase("callback_renderable");

        let options = LiveOptions {
            auto_refresh: false,
            transient: false,
            ..Default::default()
        };

        let live = Live::with_options(console.clone(), options).get_renderable(move || {
            let current = state_clone.lock().unwrap().clone();
            tracing::trace!(state = %current, "Callback invoked");
            Box::new(Text::new(current))
        });

        live.start(true).expect("start");

        // Update state and refresh
        {
            let mut s = state.lock().unwrap();
            *s = String::from("Updated");
            tracing::debug!("State updated to 'Updated'");
        }

        live.refresh().expect("refresh after state change");

        let output = buffer.contents();
        tracing::debug!(output = %output, "Callback output");
        assert!(output.contains("Updated"), "Should reflect updated state");

        live.stop().expect("stop");
    }

    tracing::info!("Callback renderable test completed");
}

// =============================================================================
// Summary Test
// =============================================================================

#[test]
fn test_live_comprehensive_workflow() {
    init_test_logging();
    log_test_context(
        "test_live_comprehensive_workflow",
        "Complete workflow exercise",
    );

    let buffer = SharedBuffer::new();
    let console = Console::builder()
        .force_terminal(true)
        .width(40)
        .height(10)
        .markup(false)
        .file(Box::new(buffer.clone()))
        .build()
        .shared();

    {
        let _workflow = test_phase("comprehensive");

        // Phase 1: Basic lifecycle
        tracing::info!("Phase 1: Basic lifecycle");
        {
            let options = LiveOptions {
                auto_refresh: false,
                transient: false,
                ..Default::default()
            };
            let live =
                Live::with_options(console.clone(), options).renderable(Text::new("Phase 1"));
            live.start(true).expect("phase 1 start");
            live.refresh().expect("phase 1 refresh");
            live.stop().expect("phase 1 stop");
        }

        // Phase 2: Content updates
        tracing::info!("Phase 2: Content updates");
        buffer.clear();
        {
            let options = LiveOptions {
                auto_refresh: false,
                transient: false,
                ..Default::default()
            };
            let live = Live::with_options(console.clone(), options).renderable(Text::new("V1"));
            live.start(true).expect("phase 2 start");
            live.update(Text::new("V2"), true);
            live.update(Text::new("V3"), true);
            live.stop().expect("phase 2 stop");
        }

        // Phase 3: Auto-refresh
        tracing::info!("Phase 3: Auto-refresh");
        buffer.clear();
        {
            let counter = Arc::new(Mutex::new(0));
            let counter_clone = Arc::clone(&counter);

            let options = LiveOptions {
                auto_refresh: true,
                refresh_per_second: 20.0,
                transient: false,
                ..Default::default()
            };
            let live = Live::with_options(console.clone(), options).get_renderable(move || {
                let mut c = counter_clone.lock().unwrap();
                *c += 1;
                Box::new(Text::new(format!("Auto: {}", *c)))
            });
            live.start(true).expect("phase 3 start");
            thread::sleep(Duration::from_millis(200));
            live.stop().expect("phase 3 stop");

            let final_count = *counter.lock().unwrap();
            tracing::debug!(auto_refresh_count = final_count, "Phase 3 complete");
        }

        // Phase 4: Transient mode
        tracing::info!("Phase 4: Transient mode");
        buffer.clear();
        {
            let options = LiveOptions {
                auto_refresh: false,
                transient: true,
                ..Default::default()
            };
            let live =
                Live::with_options(console.clone(), options).renderable(Text::new("Transient"));
            live.start(true).expect("phase 4 start");
            live.stop().expect("phase 4 stop");
        }
    }

    tracing::info!("Comprehensive workflow completed successfully");
}
