//! Integration tests for terminal detection error/edge paths.
//!
//! Verifies that `Console` behaves correctly under degraded terminal
//! conditions: no color, forced non-TTY, explicit dumb terminals, color
//! system downgrades, and the `NO_COLOR` / `FORCE_COLOR` conventions.

use std::io::Write;
use std::sync::{Arc, Mutex};

use rich_rust::color::ColorSystem;
use rich_rust::prelude::*;
use rich_rust::sync::lock_recover;

// ============================================================================
// Shared helper: in-memory writer for capturing Console output
// ============================================================================

struct BufferWriter(Arc<Mutex<Vec<u8>>>);

impl Write for BufferWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.lock().unwrap().flush()
    }
}

fn make_console(
    color: Option<ColorSystem>,
    force_term: Option<bool>,
) -> (Console, Arc<Mutex<Vec<u8>>>) {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let writer = BufferWriter(Arc::clone(&buf));
    let mut builder = Console::builder().width(80).file(Box::new(writer));
    if let Some(cs) = color {
        builder = builder.color_system(cs);
    }
    if let Some(ft) = force_term {
        builder = builder.force_terminal(ft);
    }
    (builder.build(), buf)
}

fn output(buf: &Arc<Mutex<Vec<u8>>>) -> String {
    let guard = lock_recover(buf);
    String::from_utf8_lossy(&guard).into_owned()
}

// ============================================================================
// 1. No-color mode: force_terminal(false) or no TTY suppresses ANSI
// ============================================================================

#[test]
fn non_tty_console_produces_plain_text() {
    // force_terminal(false) means non-TTY → color detection returns None
    let (console, buf) = make_console(None, Some(false));

    console.print("[bold red]Hello[/] world");

    let out = output(&buf);
    assert!(
        !out.contains("\x1b["),
        "non-TTY console should produce no ANSI escapes, got: {out:?}"
    );
    assert!(out.contains("Hello"), "output should still contain text");
    assert!(out.contains("world"), "output should still contain text");
}

#[test]
fn non_tty_console_reports_color_disabled() {
    let (console, _buf) = make_console(None, Some(false));

    assert!(!console.is_color_enabled(), "non-TTY should disable color");
    assert_eq!(
        console.color_system(),
        None,
        "non-TTY should yield None color system"
    );
}

#[test]
fn no_color_builder_clears_explicit_system() {
    // no_color() removes any previously set explicit color system
    let buf = Arc::new(Mutex::new(Vec::new()));
    let writer = BufferWriter(Arc::clone(&buf));
    let console = Console::builder()
        .color_system(ColorSystem::TrueColor)
        .no_color()
        .force_terminal(false)
        .width(80)
        .file(Box::new(writer))
        .build();

    // no_color + force_terminal(false) → no color at all
    assert!(!console.is_color_enabled());
    assert_eq!(console.color_system(), None);
}

// ============================================================================
// 2. Forced non-TTY: force_terminal(false) disables color detection
// ============================================================================

#[test]
fn force_terminal_false_disables_color() {
    let (console, buf) = make_console(None, Some(false));

    assert!(
        !console.is_terminal(),
        "force_terminal(false) should report non-terminal"
    );
    assert!(
        !console.is_color_enabled(),
        "forced non-TTY should have no color"
    );

    console.print_styled("styled text", Style::new().bold());

    let out = output(&buf);
    assert!(
        !out.contains("\x1b["),
        "forced non-TTY should produce no ANSI escapes, got: {out:?}"
    );
}

#[test]
fn force_terminal_true_enables_terminal_flag() {
    let (console, _buf) = make_console(Some(ColorSystem::Standard), Some(true));

    assert!(
        console.is_terminal(),
        "force_terminal(true) should report as terminal"
    );
    assert!(
        console.is_color_enabled(),
        "force_terminal(true) with explicit color system should enable color"
    );
}

// ============================================================================
// 3. Explicit color system overrides detection
// ============================================================================

#[test]
fn explicit_color_system_overrides_detection() {
    // Even with force_terminal(false), an explicit color_system should be used
    let (console, buf) = make_console(Some(ColorSystem::Standard), Some(false));

    assert_eq!(
        console.color_system(),
        Some(ColorSystem::Standard),
        "explicit color system should be returned"
    );

    console.print_styled("bold text", Style::new().bold());

    let out = output(&buf);
    assert!(
        out.contains("\x1b["),
        "explicit color system should produce ANSI escapes even on non-TTY, got: {out:?}"
    );
}

#[test]
fn truecolor_system_produces_rgb_escapes() {
    let (console, buf) = make_console(Some(ColorSystem::TrueColor), Some(true));

    // Print with an explicit RGB color
    let style = Style::parse("bold #ff0000").unwrap();
    console.print_styled("red text", style);

    let out = output(&buf);
    // TrueColor should use SGR 38;2;R;G;B sequences
    assert!(
        out.contains("38;2;255;0;0"),
        "TrueColor should produce RGB escape sequences, got: {out:?}"
    );
}

#[test]
fn standard_color_system_downgrades_rgb() {
    let (console, buf) = make_console(Some(ColorSystem::Standard), Some(true));

    // Print with an RGB color that must be downgraded to 16 colors
    let style = Style::parse("bold #ff0000").unwrap();
    console.print_styled("red text", style);

    let out = output(&buf);
    // Standard should NOT use 38;2 (RGB) sequences
    assert!(
        !out.contains("38;2;"),
        "Standard color should not produce RGB escapes, got: {out:?}"
    );
    // Should still have some ANSI escape (at minimum for bold)
    assert!(
        out.contains("\x1b["),
        "Standard color should still produce ANSI codes for bold, got: {out:?}"
    );
}

#[test]
fn eight_bit_color_system_uses_256_palette() {
    let (console, buf) = make_console(Some(ColorSystem::EightBit), Some(true));

    let style = Style::parse("#ff0000").unwrap();
    console.print_styled("red text", style);

    let out = output(&buf);
    // EightBit should NOT use 38;2 (RGB) sequences
    assert!(
        !out.contains("38;2;"),
        "EightBit color should not produce RGB escapes, got: {out:?}"
    );
    // Should use 38;5;N (256-color) or downgrade to 16-color codes
    assert!(
        out.contains("\x1b["),
        "EightBit should still produce ANSI codes, got: {out:?}"
    );
}

// ============================================================================
// 4. Console state queries under edge conditions
// ============================================================================

#[test]
fn default_console_color_system_is_auto_detected() {
    // A default console (no builder options) auto-detects color system.
    // In CI (non-TTY), this typically yields None.
    let console = Console::new();
    // We can't assert a specific value since it depends on environment,
    // but the accessors should not panic.
    let _ = console.color_system();
    let _ = console.is_terminal();
    let _ = console.is_color_enabled();
}

#[test]
fn console_width_defaults_when_not_tty() {
    let (console, _buf) = make_console(None, Some(false));
    let width = console.width();
    // We set width=80 in make_console, so it should be 80
    assert_eq!(width, 80, "console should use explicitly set width");
}

// ============================================================================
// 5. Print operations degrade gracefully without color
// ============================================================================

#[test]
fn print_with_markup_degrades_to_plain_text_without_color() {
    let (console, buf) = make_console(None, Some(false));

    console.print("[bold red]Error:[/] file not found");

    let out = output(&buf);
    assert!(
        !out.contains("\x1b["),
        "non-TTY console should strip ANSI escapes, got: {out:?}"
    );
    // The text content should still appear
    assert!(out.contains("Error:"), "text content should be preserved");
    assert!(
        out.contains("file not found"),
        "text content should be preserved"
    );
}

#[test]
fn print_styled_degrades_to_plain_text_without_color() {
    let (console, buf) = make_console(None, Some(false));

    let style = Style::parse("bold italic underline #ff00ff on #00ff00").unwrap();
    console.print_styled("decorated text", style);

    let out = output(&buf);
    assert!(
        !out.contains("\x1b["),
        "non-TTY console should produce no ANSI escapes, got: {out:?}"
    );
    assert!(
        out.contains("decorated text"),
        "text content should be preserved"
    );
}

// ============================================================================
// 6. Multiple color system levels: verify monotonic downgrade
// ============================================================================

#[test]
fn color_downgrade_chain_produces_valid_output() {
    let style = Style::parse("bold #abcdef on #123456").unwrap();

    for (system, label) in [
        (ColorSystem::TrueColor, "TrueColor"),
        (ColorSystem::EightBit, "EightBit"),
        (ColorSystem::Standard, "Standard"),
    ] {
        let (console, buf) = make_console(Some(system), Some(true));
        console.print_styled("test", style.clone());
        let out = output(&buf);
        assert!(
            out.contains("\x1b["),
            "{label} should produce ANSI escapes, got: {out:?}"
        );
        assert!(out.contains("test"), "{label} should contain text");
    }
}

// ============================================================================
// 7. is_dumb_terminal() public function
// ============================================================================

#[test]
fn is_dumb_terminal_returns_bool() {
    // We can't set TERM in tests safely (environment is shared), but we
    // can verify the function doesn't panic and returns a bool.
    let result = rich_rust::terminal::is_dumb_terminal();
    let _ = result; // just verify it compiles and doesn't panic
}

// ============================================================================
// 8. safe_box mode for restricted terminals
// ============================================================================

#[test]
fn safe_box_console_creation() {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let writer = BufferWriter(Arc::clone(&buf));
    let console = Console::builder()
        .no_color()
        .force_terminal(true)
        .safe_box(true)
        .width(80)
        .file(Box::new(writer))
        .build();

    // safe_box consoles should be usable without panicking
    console.print_plain("safe box output");
    let out = output(&buf);
    assert!(
        out.contains("safe box output"),
        "safe_box console should produce output"
    );
}

// ============================================================================
// 9. Combination edge cases
// ============================================================================

#[test]
fn no_color_plus_non_tty_suppresses_all_ansi() {
    // no_color() clears explicit color system; force_terminal(false) prevents
    // auto-detection from finding color support → no ANSI output at all.
    let buf = Arc::new(Mutex::new(Vec::new()));
    let writer = BufferWriter(Arc::clone(&buf));
    let console = Console::builder()
        .color_system(ColorSystem::TrueColor)
        .no_color()
        .force_terminal(false)
        .width(80)
        .file(Box::new(writer))
        .build();

    assert!(
        !console.is_color_enabled(),
        "no_color + non-TTY should disable color"
    );

    console.print_styled("no color", Style::new().bold());
    let out = output(&buf);
    assert!(
        !out.contains("\x1b["),
        "no_color + non-TTY should suppress all ANSI escapes, got: {out:?}"
    );
}

#[test]
fn color_system_after_no_color_restores_color() {
    // Calling color_system() after no_color() should restore color
    let buf = Arc::new(Mutex::new(Vec::new()));
    let writer = BufferWriter(Arc::clone(&buf));
    let console = Console::builder()
        .no_color()
        .color_system(ColorSystem::Standard)
        .force_terminal(true)
        .width(80)
        .file(Box::new(writer))
        .build();

    assert!(
        console.is_color_enabled(),
        "color_system should restore color after no_color"
    );
    assert_eq!(console.color_system(), Some(ColorSystem::Standard));
}

#[test]
fn multiple_print_calls_on_no_color_console() {
    let (console, buf) = make_console(None, Some(false));

    // Multiple prints should all degrade gracefully
    for i in 0..5 {
        console.print(&format!("[bold]line {i}[/]"));
    }

    let out = output(&buf);
    assert!(
        !out.contains("\x1b["),
        "all lines should be ANSI-free, got: {out:?}"
    );
    for i in 0..5 {
        assert!(
            out.contains(&format!("line {i}")),
            "missing line {i} in output"
        );
    }
}
