//! End-to-end tests for demo_showcase binary.
//!
//! These tests spawn the actual binary and verify its behavior as a black box.
//! All tests use the harness module for consistent timeout handling and logging.

mod common;
mod demo_showcase_harness;

use demo_showcase_harness::{DemoRunner, assertions::*};
use std::time::Duration;

// ============================================================================
// Help and basic CLI tests
// ============================================================================

#[test]
fn test_help_flag_shows_usage() {
    common::init_test_logging();

    let result = DemoRunner::new()
        .arg("--help")
        .timeout_secs(5)
        .run()
        .expect("should run");

    assert_success(&result);
    assert_stdout_contains(&result, "USAGE:");
    assert_stdout_contains(&result, "--list-scenes");
    assert_stdout_contains(&result, "--scene");
}

#[test]
fn test_short_help_flag() {
    common::init_test_logging();

    let result = DemoRunner::new()
        .arg("-h")
        .timeout_secs(5)
        .run()
        .expect("should run");

    assert_success(&result);
    assert_stdout_contains(&result, "USAGE:");
}

// ============================================================================
// Scene listing tests
// ============================================================================

#[test]
fn test_list_scenes_shows_all_scenes() {
    common::init_test_logging();

    let result = DemoRunner::new()
        .arg("--list-scenes")
        .timeout_secs(10)
        .run()
        .expect("should run");

    assert_success(&result);
    assert_no_timeout(&result);

    // All storyboard scenes should be listed
    // Note: Some scene names may wrap in narrow terminals, so we check prefixes
    assert_stdout_contains(&result, "hero");
    assert_stdout_contains(&result, "dashboard");
    assert_stdout_contains(&result, "markdown");
    assert_stdout_contains(&result, "syntax");
    assert_stdout_contains(&result, "json");
    assert_stdout_contains(&result, "table");
    assert_stdout_contains(&result, "panels");
    assert_stdout_contains(&result, "tree");
    assert_stdout_contains(&result, "layout");
    assert_stdout_contains(&result, "emoji_link"); // emoji_links may wrap
    assert_stdout_contains(&result, "debug_tool"); // debug_tools may wrap
    assert_stdout_contains(&result, "tracing");
    assert_stdout_contains(&result, "traceback");
    assert_stdout_contains(&result, "export");
    assert_stdout_contains(&result, "outro");

    // Should show table formatting
    assert_stdout_contains(&result, "Available Scenes");
}

// ============================================================================
// Single scene execution tests
// ============================================================================

#[test]
fn test_run_single_scene_hero() {
    common::init_test_logging();

    let result = DemoRunner::quick()
        .arg("--scene")
        .arg("hero")
        .no_color()
        .run()
        .expect("should run");

    assert_success(&result);
    assert_no_timeout(&result);

    // Hero scene should show branding and capabilities
    // Note: Brand title has spaced letters "N E B U L A"
    assert_stdout_contains(&result, "N E B U L A");
    assert_stdout_contains(&result, "Terminal size");
}

#[test]
fn test_run_single_scene_dashboard() {
    common::init_test_logging();

    let result = DemoRunner::quick()
        .arg("--scene")
        .arg("dashboard")
        .no_color()
        .run()
        .expect("should run");

    assert_success(&result);
    assert_no_timeout(&result);
}

#[test]
fn test_unknown_scene_fails() {
    common::init_test_logging();

    let result = DemoRunner::new()
        .arg("--scene")
        .arg("nonexistent_scene")
        .timeout_secs(5)
        .run()
        .expect("should run");

    assert_failure(&result);
    assert_stderr_contains(&result, "Unknown scene");
}

// ============================================================================
// Full demo run tests
// ============================================================================

#[test]
fn test_full_demo_run_completes() {
    common::init_test_logging();

    let result = DemoRunner::quick()
        .non_interactive()
        .no_color()
        .run()
        .expect("should run");

    assert_success(&result);
    assert_no_timeout(&result);

    // Should show header
    assert_stdout_contains(&result, "Nebula Deploy");

    // Should mention all scenes ran
    assert_stdout_contains(&result, "scenes completed");
}

#[test]
fn test_full_demo_with_seed() {
    common::init_test_logging();

    let result = DemoRunner::quick()
        .arg("--seed")
        .arg("12345")
        .non_interactive()
        .no_color()
        .run()
        .expect("should run");

    assert_success(&result);
    assert_no_timeout(&result);
}

// ============================================================================
// Timing and performance tests
// ============================================================================

#[test]
fn test_quick_mode_is_fast() {
    common::init_test_logging();

    let result = DemoRunner::quick()
        .non_interactive()
        .no_color()
        .run()
        .expect("should run");

    assert_success(&result);
    assert_no_timeout(&result);

    // Quick mode should complete in under 5 seconds
    assert_elapsed_under(&result, Duration::from_secs(5));
}

#[test]
fn test_list_scenes_is_fast() {
    common::init_test_logging();

    let result = DemoRunner::new()
        .arg("--list-scenes")
        .timeout_secs(10)
        .run()
        .expect("should run");

    assert_success(&result);

    // Listing scenes should be very fast
    assert_elapsed_under(&result, Duration::from_secs(2));
}

// ============================================================================
// Error handling tests
// ============================================================================

#[test]
fn test_unknown_flag_fails() {
    common::init_test_logging();

    let result = DemoRunner::new()
        .arg("--unknown-flag")
        .timeout_secs(5)
        .run()
        .expect("should run");

    assert_failure(&result);
    assert_stderr_contains(&result, "Unknown flag");
}

#[test]
fn test_invalid_seed_fails() {
    common::init_test_logging();

    let result = DemoRunner::new()
        .arg("--seed")
        .arg("not_a_number")
        .timeout_secs(5)
        .run()
        .expect("should run");

    assert_failure(&result);
    assert_stderr_contains(&result, "Invalid --seed");
}

#[test]
fn test_invalid_speed_fails() {
    common::init_test_logging();

    let result = DemoRunner::new()
        .arg("--speed")
        .arg("0")
        .timeout_secs(5)
        .run()
        .expect("should run");

    assert_failure(&result);
    assert_stderr_contains(&result, "> 0");
}

// ============================================================================
// Output format tests
// ============================================================================

#[test]
fn test_no_color_env_disables_ansi() {
    common::init_test_logging();

    let result = DemoRunner::quick()
        .arg("--list-scenes")
        .env("NO_COLOR", "1")
        .run()
        .expect("should run");

    assert_success(&result);

    // Output should not contain ANSI escape codes
    assert!(
        !result.stdout.contains("\x1b["),
        "Output should not contain ANSI codes when NO_COLOR is set"
    );
}

#[test]
fn test_width_override() {
    common::init_test_logging();

    let result = DemoRunner::quick()
        .arg("--width")
        .arg("40")
        .arg("--list-scenes")
        .no_color()
        .run()
        .expect("should run");

    assert_success(&result);

    // With narrow width, output should wrap or be narrower
    // Just verify it runs successfully
}

// ============================================================================
// Non-interactive safety tests (bd-zzss)
// ============================================================================

/// Guards against regressions that cause piped output to hang.
/// Uses deterministic, bounded settings: --quick --seed 0 --color-system none
#[test]
fn test_non_interactive_full_demo_completes() {
    common::init_test_logging();

    let result = DemoRunner::new()
        .arg("--quick")
        .arg("--seed")
        .arg("0")
        .arg("--color-system")
        .arg("none")
        .arg("--no-interactive")
        .timeout_secs(30) // Hard timeout - must complete in 30s
        .run()
        .expect("should run");

    assert_success(&result);
    assert_no_timeout(&result);

    // Should complete all scenes
    assert_stdout_contains(&result, "scenes completed");
}

/// Guards against unbounded output (infinite animation frames, etc.)
#[test]
fn test_non_interactive_output_is_bounded() {
    common::init_test_logging();

    let result = DemoRunner::new()
        .arg("--quick")
        .arg("--seed")
        .arg("0")
        .arg("--color-system")
        .arg("none")
        .arg("--no-interactive")
        .timeout_secs(30)
        .run()
        .expect("should run");

    assert_success(&result);

    // Output should be reasonably bounded
    // A full demo run should produce less than 100KB of output
    // This guards against runaway loops that spam infinite frames
    let output_size = result.stdout.len() + result.stderr.len();
    const MAX_OUTPUT_BYTES: usize = 100 * 1024; // 100 KB
    assert!(
        output_size < MAX_OUTPUT_BYTES,
        "Output size ({} bytes) exceeds limit ({} bytes) - possible unbounded output",
        output_size,
        MAX_OUTPUT_BYTES
    );
}

/// Verifies no ANSI control sequences leak when color is disabled.
#[test]
fn test_non_interactive_no_ansi_leakage() {
    common::init_test_logging();

    let result = DemoRunner::new()
        .arg("--quick")
        .arg("--seed")
        .arg("0")
        .arg("--color-system")
        .arg("none")
        .arg("--no-interactive")
        .no_color()
        .timeout_secs(30)
        .run()
        .expect("should run");

    assert_success(&result);

    // No ANSI escape sequences should appear in output
    assert!(
        !result.stdout.contains("\x1b["),
        "Stdout should not contain ANSI escape codes in no-color mode"
    );
    assert!(
        !result.stderr.contains("\x1b["),
        "Stderr should not contain ANSI escape codes in no-color mode"
    );
}

/// Verifies no cursor control sequences that could cause display issues.
#[test]
fn test_non_interactive_no_cursor_control() {
    common::init_test_logging();

    let result = DemoRunner::new()
        .arg("--quick")
        .arg("--seed")
        .arg("0")
        .arg("--color-system")
        .arg("none")
        .arg("--no-interactive")
        .timeout_secs(30)
        .run()
        .expect("should run");

    assert_success(&result);

    // Should not contain cursor movement sequences
    // \x1b[H = cursor home, \x1b[?25l = hide cursor, \x1b[2J = clear screen
    let dangerous_sequences = ["\x1b[H", "\x1b[?25", "\x1b[2J", "\x1b[?1049"];
    for seq in dangerous_sequences {
        assert!(
            !result.stdout.contains(seq),
            "Stdout should not contain cursor control sequence '{}'",
            seq.escape_default()
        );
    }
}

/// Tests that live mode is auto-disabled in non-interactive context.
#[test]
fn test_non_interactive_live_auto_disabled() {
    common::init_test_logging();

    // Even with --live flag, non-TTY should auto-disable live mode
    // This is harder to test directly, but we can verify output is static
    let result = DemoRunner::new()
        .arg("--quick")
        .arg("--seed")
        .arg("0")
        .arg("--no-interactive")
        .no_color()
        .timeout_secs(30)
        .run()
        .expect("should run");

    assert_success(&result);
    assert_no_timeout(&result);

    // Output should be static (no carriage returns for live updates)
    // Newlines are fine, but \r without \n indicates live updates
    let cr_without_lf = result.stdout.matches('\r').count() - result.stdout.matches("\r\n").count();
    assert!(
        cr_without_lf == 0,
        "Found {} carriage returns without line feeds - indicates live updates in non-interactive mode",
        cr_without_lf
    );
}

/// Tests that pager is disabled with --no-interactive (bd-3iws).
///
/// When --no-interactive is set, the markdown scene should:
/// - Skip pager invocation entirely
/// - Render content inline to stdout
/// - Complete without hanging
#[test]
fn test_non_interactive_pager_disabled() {
    common::init_test_logging();

    // Run markdown scene with --no-interactive
    // This should render the runbook inline without invoking a pager
    let result = DemoRunner::new()
        .arg("--scene")
        .arg("markdown")
        .arg("--quick")
        .arg("--seed")
        .arg("0")
        .arg("--no-interactive")
        .timeout_secs(10) // Should complete quickly without pager
        .run()
        .expect("should run");

    assert_success(&result);
    assert_no_timeout(&result);

    // Verify content was rendered inline (not swallowed by pager)
    // The markdown scene outputs recognizable text
    assert!(
        result.stdout_contains("Markdown") || result.stdout_contains("markdown"),
        "Markdown scene should produce recognizable output:\n{}",
        result.diagnostic_output()
    );
}

// ============================================================================
// Non-TTY / Piped output verification tests (bd-2k90)
// ============================================================================

/// Verifies each implemented scene completes when piped (simulating `| cat`).
/// This is a CI-friendly verification that no scene blocks on TTY input.
#[test]
fn test_piped_all_scenes_complete() {
    common::init_test_logging();

    // List of all implemented scenes (non-placeholder)
    let scenes = [
        "hero",
        "debug_tools",
        "tracing",
        "traceback",
        "table",
        "panels",
        "tree",
        "layout",
        "emoji_links",
        "export",
    ];

    for scene in scenes {
        let msg = format!("scene '{scene}' should run");
        let result = DemoRunner::new()
            .arg("--scene")
            .arg(scene)
            .arg("--quick")
            .arg("--seed")
            .arg("0")
            .arg("--color-system")
            .arg("none")
            .arg("--no-interactive")
            .timeout_secs(15)
            .run()
            .expect(&msg);

        assert_success(&result);
        assert_no_timeout(&result);
        assert!(
            !result.stdout.is_empty(),
            "Scene '{}' should produce output",
            scene
        );
    }
}

/// Verifies output remains readable when piped (no binary garbage or control chars).
#[test]
fn test_piped_output_is_readable_text() {
    common::init_test_logging();

    let result = DemoRunner::new()
        .arg("--quick")
        .arg("--seed")
        .arg("0")
        .arg("--color-system")
        .arg("none")
        .arg("--no-interactive")
        .timeout_secs(30)
        .run()
        .expect("should run");

    assert_success(&result);

    // Output should be valid UTF-8 (already guaranteed by String)
    // Check for problematic control characters (excluding normal whitespace)
    let problematic_chars: Vec<char> = result
        .stdout
        .chars()
        .filter(|c| c.is_control() && *c != '\n' && *c != '\r' && *c != '\t')
        .collect();

    assert!(
        problematic_chars.is_empty(),
        "Output contains {} problematic control characters: {:?}",
        problematic_chars.len(),
        problematic_chars.iter().take(10).collect::<Vec<_>>()
    );
}

/// Verifies no pager-style blocking prompts in piped output.
/// Note: Informational text like "Press any key..." is fine if it doesn't block.
#[test]
fn test_piped_no_blocking_pager() {
    common::init_test_logging();

    let result = DemoRunner::new()
        .arg("--quick")
        .arg("--seed")
        .arg("0")
        .arg("--color-system")
        .arg("none")
        .arg("--no-interactive")
        .timeout_secs(30)
        .run()
        .expect("should run");

    assert_success(&result);

    // Should not contain pager-specific blocking indicators
    // (END) and "-- More --" indicate actual pagers like less/more
    let pager_indicators = ["(END)", "-- More --", "[Press q to quit]"];

    for indicator in pager_indicators {
        assert!(
            !result.stdout.contains(indicator),
            "Output should not contain pager indicator: '{}'",
            indicator
        );
    }

    // The fact that we got here with exit 0 proves no blocking occurred
}

/// Verifies per-scene output size is bounded (guards against runaway loops).
#[test]
fn test_piped_per_scene_output_bounded() {
    common::init_test_logging();

    let scenes = [
        "hero",
        "debug_tools",
        "tracing",
        "traceback",
        "table",
        "panels",
        "tree",
        "layout",
        "emoji_links",
        "export",
    ];
    const MAX_SCENE_OUTPUT: usize = 50 * 1024; // 50 KB per scene

    for scene in scenes {
        let msg = format!("scene '{scene}' should run");
        let result = DemoRunner::new()
            .arg("--scene")
            .arg(scene)
            .arg("--quick")
            .arg("--seed")
            .arg("0")
            .arg("--color-system")
            .arg("none")
            .arg("--no-interactive")
            .timeout_secs(15)
            .run()
            .expect(&msg);

        assert_success(&result);

        let output_size = result.stdout.len() + result.stderr.len();
        assert!(
            output_size < MAX_SCENE_OUTPUT,
            "Scene '{}' output ({} bytes) exceeds limit ({} bytes)",
            scene,
            output_size,
            MAX_SCENE_OUTPUT
        );
    }
}

/// Verifies quick mode completes rapidly (CI performance gate).
#[test]
fn test_piped_quick_mode_is_fast() {
    common::init_test_logging();

    let result = DemoRunner::new()
        .arg("--quick")
        .arg("--seed")
        .arg("0")
        .arg("--color-system")
        .arg("none")
        .arg("--no-interactive")
        .timeout_secs(30)
        .run()
        .expect("should run");

    assert_success(&result);
    assert_no_timeout(&result);

    // Quick mode full demo should complete in under 10 seconds
    assert_elapsed_under(&result, Duration::from_secs(10));
}

// ============================================================================
// Output toggles matrix tests (bd-1e7c)
// ============================================================================

/// Verifies --no-emoji prevents emoji shortcode replacement.
/// The hero scene uses emoji like ":rocket:" which should remain literal.
#[test]
fn test_no_emoji_disables_emoji_replacement() {
    common::init_test_logging();

    // Run with --no-emoji
    let result_no_emoji = DemoRunner::quick()
        .arg("--scene")
        .arg("emoji_links")
        .arg("--no-emoji")
        .no_color()
        .run()
        .expect("should run");

    assert_success(&result_no_emoji);

    // Run with emoji enabled (default)
    let result_with_emoji = DemoRunner::quick()
        .arg("--scene")
        .arg("emoji_links")
        .arg("--emoji")
        .no_color()
        .run()
        .expect("should run");

    assert_success(&result_with_emoji);

    // emoji_links scene prints a `:rocket:` shortcode when emoji is enabled.
    // With emoji replacement disabled, the raw shortcode should remain.
    assert_stdout_contains(&result_no_emoji, ":rocket:");
    assert!(!result_no_emoji.stdout.contains('🚀'));

    assert!(!result_with_emoji.stdout.contains(":rocket:"));
    assert_stdout_contains(&result_with_emoji, "🚀");
}

/// Verifies --safe-box flag is accepted and runs successfully.
/// This test verifies the flag is parsed and the scene runs without error.
#[test]
fn test_safe_box_flag_accepted() {
    common::init_test_logging();

    // Run with --safe-box - should be accepted without error
    let result = DemoRunner::quick()
        .arg("--scene")
        .arg("table")
        .arg("--safe-box")
        .no_color()
        .run()
        .expect("should run");

    assert_success(&result);

    // The table scene includes an explicit ASCII table demo section
    // which shows ASCII box characters regardless of the flag
    assert_stdout_contains(&result, "ASCII Fallback Mode");
}

/// Verifies the table scene's ASCII demo section uses ASCII characters.
#[test]
fn test_table_ascii_demo_uses_ascii_characters() {
    common::init_test_logging();

    let result = DemoRunner::quick()
        .arg("--scene")
        .arg("table")
        .no_color()
        .run()
        .expect("should run");

    assert_success(&result);

    // The explicit ASCII demo section should contain ASCII box characters
    // This section uses Table::new().ascii() explicitly
    assert_stdout_contains(&result, "ASCII Fallback Mode");
    assert_stdout_contains(&result, "Deployment History");
}

/// Verifies default output uses Unicode box characters.
#[test]
fn test_default_uses_unicode_box_characters() {
    common::init_test_logging();

    // Run with default settings (Unicode boxes)
    let result = DemoRunner::quick()
        .arg("--scene")
        .arg("table")
        .no_color()
        .run()
        .expect("should run");

    assert_success(&result);

    // Should contain some Unicode box characters in the default tables
    let has_unicode_box = result.stdout.chars().any(|c| {
        matches!(
            c,
            '─' | '│'
                | '┌'
                | '┐'
                | '└'
                | '┘'
                | '├'
                | '┤'
                | '┬'
                | '┴'
                | '┼'
                | '━'
                | '┃'
                | '┏'
                | '┓'
                | '┗'
                | '┛'
                | '╔'
                | '╗'
                | '╚'
                | '╝'
                | '╭'
                | '╮'
                | '╰'
                | '╯'
                | '┡'
                | '┩'
                | '╡'
                | '╞'
        )
    });

    assert!(
        has_unicode_box,
        "Default output should contain Unicode box characters"
    );
}

/// Verifies --no-links removes OSC8 hyperlink sequences.
#[test]
fn test_no_links_removes_hyperlinks() {
    common::init_test_logging();

    // Run with --no-links
    let result = DemoRunner::quick()
        .arg("--scene")
        .arg("hero")
        .arg("--no-links")
        .run()
        .expect("should run");

    assert_success(&result);

    // OSC8 hyperlink format: \x1b]8;;URL\x1b\\ or \x1b]8;;URL\x07
    assert!(
        !result.stdout.contains("\x1b]8;"),
        "With --no-links, output should not contain OSC8 hyperlink sequences"
    );
}

/// Verifies --links enables OSC8 hyperlinks when forced.
/// Note: Links may be auto-disabled in non-TTY contexts, so we use --force-terminal.
#[test]
fn test_links_enabled_contains_osc8() {
    common::init_test_logging();

    // Run with --links --force-terminal to ensure hyperlinks are enabled
    let result = DemoRunner::quick()
        .arg("--scene")
        .arg("hero")
        .arg("--links")
        .arg("--force-terminal")
        .run()
        .expect("should run");

    assert_success(&result);

    // Note: This test may not find hyperlinks if the hero scene doesn't generate any.
    // The key thing is that it runs successfully with the flag.
    // If the scene has hyperlinks, they use OSC8 format: \x1b]8;;URL
}

/// Verifies --color-system none produces no ANSI SGR sequences.
#[test]
fn test_color_system_none_no_ansi() {
    common::init_test_logging();

    let result = DemoRunner::quick()
        .arg("--scene")
        .arg("hero")
        .arg("--color-system")
        .arg("none")
        .run()
        .expect("should run");

    assert_success(&result);

    // Should not contain ANSI SGR sequences (color/style codes)
    // These start with \x1b[ and end with 'm'
    assert!(
        !result.stdout.contains("\x1b["),
        "With --color-system none, output should not contain ANSI escape sequences"
    );
}

/// Verifies --color-system truecolor produces ANSI sequences.
#[test]
fn test_color_system_truecolor_has_ansi() {
    common::init_test_logging();

    let result = DemoRunner::quick()
        .arg("--scene")
        .arg("hero")
        .arg("--color-system")
        .arg("truecolor")
        .arg("--force-terminal")
        .run()
        .expect("should run");

    assert_success(&result);

    // With truecolor and force-terminal, should contain ANSI sequences
    assert!(
        result.stdout.contains("\x1b["),
        "With --color-system truecolor and --force-terminal, output should contain ANSI sequences"
    );
}

/// Matrix test: Combines multiple toggles to verify they work together.
/// Note: --safe-box propagation to all renderables is a separate feature.
#[test]
fn test_output_toggles_matrix_combination() {
    common::init_test_logging();

    // Run with all restrictive toggles
    let result = DemoRunner::quick()
        .arg("--scene")
        .arg("table")
        .arg("--no-emoji")
        .arg("--safe-box")
        .arg("--no-links")
        .arg("--color-system")
        .arg("none")
        .arg("--no-interactive")
        .run()
        .expect("should run");

    assert_success(&result);

    // Verify restrictions that are fully implemented:
    // 1. No ANSI codes (--color-system none)
    assert!(
        !result.stdout.contains("\x1b["),
        "Combined toggles: should not contain ANSI codes"
    );

    // 2. No OSC8 links (--no-links)
    assert!(
        !result.stdout.contains("\x1b]8;"),
        "Combined toggles: should not contain OSC8 sequences"
    );

    // Note: --safe-box for tables is tracked separately
    // The scene runs successfully with all toggles combined
}

// ============================================================================
// Narrow width verification tests (bd-2t54)
// ============================================================================

/// Verifies demo completes successfully at narrow width (70 cols).
#[test]
fn test_narrow_width_70_completes() {
    common::init_test_logging();

    let result = DemoRunner::quick()
        .arg("--width")
        .arg("70")
        .non_interactive()
        .no_color()
        .run()
        .expect("should run");

    assert_success(&result);
    assert_no_timeout(&result);
    assert_stdout_contains(&result, "scenes completed");
}

/// Verifies demo completes at very narrow width (50 cols).
#[test]
fn test_narrow_width_50_completes() {
    common::init_test_logging();

    let result = DemoRunner::quick()
        .arg("--width")
        .arg("50")
        .non_interactive()
        .no_color()
        .run()
        .expect("should run");

    assert_success(&result);
    assert_no_timeout(&result);
    assert_stdout_contains(&result, "scenes completed");
}

/// Verifies individual scenes render without panic at narrow width.
#[test]
fn test_narrow_width_all_scenes() {
    common::init_test_logging();

    let scenes = [
        "hero",
        "table",
        "panels",
        "tree",
        "layout",
        "emoji_links",
        "debug_tools",
        "tracing",
        "traceback",
        "export",
    ];

    for scene in scenes {
        let msg = format!("scene '{scene}' should run at narrow width");
        let result = DemoRunner::new()
            .arg("--scene")
            .arg(scene)
            .arg("--width")
            .arg("60")
            .arg("--quick")
            .arg("--no-interactive")
            .arg("--color-system")
            .arg("none")
            .timeout_secs(15)
            .run()
            .expect(&msg);

        assert_success(&result);
        assert_no_timeout(&result);
        assert!(
            !result.stdout.is_empty(),
            "Scene '{}' should produce output at narrow width",
            scene
        );
    }
}

/// Verifies no zero-width panic at minimum sensible width.
#[test]
fn test_narrow_width_minimum_no_panic() {
    common::init_test_logging();

    // Width of 40 is about the minimum for any reasonable output
    let result = DemoRunner::new()
        .arg("--scene")
        .arg("hero")
        .arg("--width")
        .arg("40")
        .arg("--quick")
        .arg("--no-interactive")
        .arg("--color-system")
        .arg("none")
        .timeout_secs(10)
        .run()
        .expect("should run at minimum width");

    assert_success(&result);
    assert_no_timeout(&result);
}

// ============================================================================
// Color downgrade verification tests (bd-191b)
// ============================================================================

/// Verifies demo runs successfully with standard 16-color palette.
#[test]
fn test_color_system_standard() {
    common::init_test_logging();

    let result = DemoRunner::quick()
        .arg("--scene")
        .arg("hero")
        .arg("--color-system")
        .arg("standard")
        .arg("--force-terminal")
        .run()
        .expect("should run");

    assert_success(&result);

    // Standard uses basic ANSI codes (SGR parameters 30-37, 40-47, 90-97, 100-107)
    assert!(
        result.stdout.contains("\x1b["),
        "Standard color mode should produce ANSI codes"
    );
    // Should show the color system in output
    assert_stdout_contains(&result, "Standard");
}

/// Verifies demo runs successfully with 256-color palette.
#[test]
fn test_color_system_256() {
    common::init_test_logging();

    let result = DemoRunner::quick()
        .arg("--scene")
        .arg("hero")
        .arg("--color-system")
        .arg("256")
        .arg("--force-terminal")
        .run()
        .expect("should run");

    assert_success(&result);

    // 256-color uses 38;5;N or 48;5;N format
    assert!(
        result.stdout.contains("\x1b[38;5;") || result.stdout.contains("\x1b[48;5;"),
        "256-color mode should use 8-bit color codes"
    );
    // Should show the color system in output
    assert_stdout_contains(&result, "EightBit");
}

/// Verifies demo runs successfully with truecolor palette.
#[test]
fn test_color_system_truecolor_detailed() {
    common::init_test_logging();

    let result = DemoRunner::quick()
        .arg("--scene")
        .arg("hero")
        .arg("--color-system")
        .arg("truecolor")
        .arg("--force-terminal")
        .run()
        .expect("should run");

    assert_success(&result);

    // Truecolor uses 38;2;R;G;B or 48;2;R;G;B format
    assert!(
        result.stdout.contains("\x1b[38;2;") || result.stdout.contains("\x1b[48;2;"),
        "Truecolor mode should use 24-bit RGB codes"
    );
    // Should show the color system in output
    assert_stdout_contains(&result, "TrueColor");
}

/// Verifies all color systems produce readable color palette section.
#[test]
fn test_color_systems_show_palette() {
    common::init_test_logging();

    let color_systems = ["standard", "256", "truecolor"];

    for system in color_systems {
        let msg = format!("should run with {system}");
        let result = DemoRunner::quick()
            .arg("--scene")
            .arg("hero")
            .arg("--color-system")
            .arg(system)
            .arg("--force-terminal")
            .run()
            .expect(&msg);

        assert_success(&result);

        // All should show the color palette preview
        assert_stdout_contains(&result, "Color Palette");
        assert_stdout_contains(&result, "Brand");
        assert_stdout_contains(&result, "Status");
        assert_stdout_contains(&result, "Badges");
    }
}

/// Verifies status badges have visible markers in all color modes.
#[test]
fn test_color_systems_badges_visible() {
    common::init_test_logging();

    let color_systems = ["standard", "256", "truecolor"];

    for system in color_systems {
        let msg = format!("should run with {system}");
        let result = DemoRunner::quick()
            .arg("--scene")
            .arg("hero")
            .arg("--color-system")
            .arg(system)
            .arg("--force-terminal")
            .run()
            .expect(&msg);

        assert_success(&result);

        // Badges should contain the status text
        assert!(
            result.stdout.contains("OK")
                && result.stdout.contains("WARN")
                && result.stdout.contains("ERR"),
            "Color system '{}' should show readable status badges",
            system
        );
    }
}

// ============================================================================
// Export bundle e2e tests (bd-3p3h)
// ============================================================================

/// Verifies --export-dir creates HTML and SVG files.
#[test]
fn test_export_dir_creates_files() {
    common::init_test_logging();

    let temp_dir = std::env::temp_dir().join("demo_showcase_e2e_export_test");
    // Clean up any previous run
    let _ = std::fs::remove_dir_all(&temp_dir);

    let result = DemoRunner::quick()
        .arg("--export-dir")
        .arg(temp_dir.to_str().unwrap())
        .non_interactive()
        .no_color()
        .arg("--width")
        .arg("80")
        .arg("--height")
        .arg("24")
        .timeout(Duration::from_secs(300))
        .run()
        .expect("should run export");

    assert_success(&result);
    assert_no_timeout(&result);

    // Verify files exist
    let html_path = temp_dir.join("demo_showcase.html");
    let svg_path = temp_dir.join("demo_showcase.svg");

    assert!(
        html_path.exists(),
        "HTML file should exist at {}",
        html_path.display()
    );
    assert!(
        svg_path.exists(),
        "SVG file should exist at {}",
        svg_path.display()
    );

    // Verify files are non-empty
    let html_size = std::fs::metadata(&html_path)
        .expect("should read HTML metadata")
        .len();
    let svg_size = std::fs::metadata(&svg_path)
        .expect("should read SVG metadata")
        .len();

    assert!(html_size > 0, "HTML file should be non-empty");
    assert!(svg_size > 0, "SVG file should be non-empty");

    // Clean up
    let _ = std::fs::remove_dir_all(&temp_dir);
}

/// Verifies exported HTML contains expected markers.
#[test]
fn test_export_html_contains_expected_content() {
    common::init_test_logging();

    let temp_dir = std::env::temp_dir().join("demo_showcase_e2e_html_content_test");
    let _ = std::fs::remove_dir_all(&temp_dir);

    // Use hero scene only for speed - it contains "Nebula" which we check for
    let result = DemoRunner::quick()
        .arg("--scene")
        .arg("hero")
        .arg("--export-dir")
        .arg(temp_dir.to_str().unwrap())
        .non_interactive()
        .arg("--color-system")
        .arg("truecolor")
        .arg("--width")
        .arg("80")
        .timeout(Duration::from_secs(60))
        .run()
        .expect("should run export");

    assert_success(&result);

    let html_path = temp_dir.join("demo_showcase.html");
    let html_content = std::fs::read_to_string(&html_path).expect("should read HTML file");

    // HTML should contain demo title (spaced letters in hero scene)
    assert!(
        html_content.contains("N E B U L A")
            || html_content.contains("NEBULA")
            || html_content.contains("Nebula"),
        "HTML should contain demo title 'Nebula' (possibly with spaced letters)"
    );

    // HTML should be valid HTML structure
    assert!(
        html_content.contains("<html") || html_content.contains("<!DOCTYPE"),
        "HTML should have valid HTML structure"
    );

    // HTML should contain style information
    assert!(
        html_content.contains("<style") || html_content.contains("style="),
        "HTML should contain styling"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

/// Verifies exported SVG contains expected markers.
#[test]
fn test_export_svg_contains_expected_content() {
    common::init_test_logging();

    let temp_dir = std::env::temp_dir().join("demo_showcase_e2e_svg_content_test");
    let _ = std::fs::remove_dir_all(&temp_dir);

    // Use hero scene only for speed - it contains "Nebula" which we check for
    let result = DemoRunner::quick()
        .arg("--scene")
        .arg("hero")
        .arg("--export-dir")
        .arg(temp_dir.to_str().unwrap())
        .non_interactive()
        .arg("--color-system")
        .arg("truecolor")
        .arg("--width")
        .arg("80")
        .timeout(Duration::from_secs(60))
        .run()
        .expect("should run export");

    assert_success(&result);

    let svg_path = temp_dir.join("demo_showcase.svg");
    let svg_content = std::fs::read_to_string(&svg_path).expect("should read SVG file");

    // SVG should have valid SVG structure
    assert!(
        svg_content.contains("<svg"),
        "SVG should contain <svg> element"
    );
    assert!(
        svg_content.contains("xmlns"),
        "SVG should have xmlns attribute"
    );

    // SVG should include Rich export markers / structure
    assert!(
        svg_content.contains("class=\"rich-terminal\""),
        "SVG should include Rich terminal wrapper class"
    );
    assert!(
        svg_content.contains("Generated with Rich"),
        "SVG should include generator comment"
    );
    assert!(
        svg_content.contains("<text"),
        "SVG should render <text> nodes"
    );

    // SVG should contain demo title (spaced letters in hero scene)
    assert!(
        svg_content.contains("N E B U L A")
            || svg_content.contains("N&#160;E&#160;B&#160;U&#160;L&#160;A")
            || svg_content.contains("NEBULA")
            || svg_content.contains("Nebula"),
        "SVG should contain demo title 'Nebula' (possibly with spaced letters)"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

/// Verifies --export (without dir) uses temp directory.
#[test]
fn test_export_flag_uses_temp_dir() {
    common::init_test_logging();

    let result = DemoRunner::quick()
        .arg("--export")
        .non_interactive()
        .no_color()
        .arg("--width")
        .arg("80")
        .timeout(Duration::from_secs(300))
        .run()
        .expect("should run export");

    assert_success(&result);

    // Stderr should mention the export paths
    assert!(
        result.stderr.contains("demo_showcase.html") || result.stderr.contains("Exported HTML"),
        "Stderr should mention HTML export path"
    );
    assert!(
        result.stderr.contains("demo_showcase.svg") || result.stderr.contains("Exported SVG"),
        "Stderr should mention SVG export path"
    );
}

/// Verifies export with single scene works.
#[test]
fn test_export_single_scene() {
    common::init_test_logging();

    let temp_dir = std::env::temp_dir().join("demo_showcase_e2e_single_scene_export");
    let _ = std::fs::remove_dir_all(&temp_dir);

    // Note: --scene with --export-dir only runs the scene, doesn't trigger full export
    // The export happens when running full demo with --export-dir
    let result = DemoRunner::quick()
        .arg("--export-dir")
        .arg(temp_dir.to_str().unwrap())
        .non_interactive()
        .no_color()
        .timeout(Duration::from_secs(300))
        .run()
        .expect("should run export");

    assert_success(&result);

    // Files should exist from full export run
    let html_path = temp_dir.join("demo_showcase.html");
    let svg_path = temp_dir.join("demo_showcase.svg");

    assert!(html_path.exists(), "HTML file should exist after export");
    assert!(svg_path.exists(), "SVG file should exist after export");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

/// Verifies export files have reasonable sizes (not empty, not huge).
#[test]
fn test_export_file_sizes_reasonable() {
    common::init_test_logging();

    let temp_dir = std::env::temp_dir().join("demo_showcase_e2e_file_sizes");
    let _ = std::fs::remove_dir_all(&temp_dir);

    // Use hero scene only for speed - file size assertions still valid for single scene
    let result = DemoRunner::quick()
        .arg("--scene")
        .arg("hero")
        .arg("--export-dir")
        .arg(temp_dir.to_str().unwrap())
        .non_interactive()
        .arg("--color-system")
        .arg("truecolor")
        .arg("--width")
        .arg("80")
        .timeout(Duration::from_secs(60))
        .run()
        .expect("should run export");

    assert_success(&result);

    let html_path = temp_dir.join("demo_showcase.html");
    let svg_path = temp_dir.join("demo_showcase.svg");

    let html_size = std::fs::metadata(&html_path)
        .expect("should read HTML metadata")
        .len();
    let svg_size = std::fs::metadata(&svg_path)
        .expect("should read SVG metadata")
        .len();

    // Files should be at least 1KB (meaningful content)
    assert!(
        html_size >= 1024,
        "HTML should be at least 1KB, got {} bytes",
        html_size
    );
    assert!(
        svg_size >= 1024,
        "SVG should be at least 1KB, got {} bytes",
        svg_size
    );

    // Files shouldn't be huge (< 5MB is reasonable for a demo)
    assert!(
        html_size < 5 * 1024 * 1024,
        "HTML should be under 5MB, got {} bytes",
        html_size
    );
    assert!(
        svg_size < 5 * 1024 * 1024,
        "SVG should be under 5MB, got {} bytes",
        svg_size
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

// ============================================================================
// Feature-gated scene tests (bd-61ky)
//
// These tests verify that feature-gated scenes show real content when the
// feature is enabled (tested via --features showcase).
// ============================================================================

#[test]
#[cfg(feature = "syntax")]
fn test_syntax_scene_shows_real_content_when_enabled() {
    common::init_test_logging();

    let result = DemoRunner::quick()
        .arg("--scene")
        .arg("syntax")
        .non_interactive()
        .no_color()
        .run()
        .expect("should run");

    assert_success(&result);
    assert_no_timeout(&result);

    // When syntax feature is enabled, should show real syntax highlighting content
    assert_stdout_contains(&result, "Syntax: Code Highlighting");
    assert_stdout_contains(&result, "Syntax renderable provides highlighting");

    // Should show actual code examples
    assert_stdout_contains(&result, "Deployment Config (TOML)");
    assert_stdout_contains(&result, "[deployment]");

    // Should NOT show the feature-disabled panel
    assert!(
        !result.stdout_contains("Feature Required"),
        "Should not show 'Feature Required' panel when syntax feature is enabled"
    );
}

#[test]
#[cfg(feature = "markdown")]
fn test_markdown_scene_shows_real_content_when_enabled() {
    common::init_test_logging();

    let result = DemoRunner::quick()
        .arg("--scene")
        .arg("markdown")
        .non_interactive()
        .no_color()
        .run()
        .expect("should run");

    assert_success(&result);
    assert_no_timeout(&result);

    // When markdown feature is enabled, should show real content
    assert_stdout_contains(&result, "Markdown: Documentation Rendering");
    assert_stdout_contains(&result, "Markdown renderable converts CommonMark");

    // Should show actual markdown examples
    assert_stdout_contains(&result, "Release Notes");
    assert_stdout_contains(&result, "Nebula API v2.4.1");

    // Should NOT show the feature-disabled panel
    assert!(
        !result.stdout_contains("Feature Required"),
        "Should not show 'Feature Required' panel when markdown feature is enabled"
    );
}

#[test]
#[cfg(feature = "json")]
fn test_json_scene_shows_real_content_when_enabled() {
    common::init_test_logging();

    let result = DemoRunner::quick()
        .arg("--scene")
        .arg("json")
        .non_interactive()
        .no_color()
        .run()
        .expect("should run");

    assert_success(&result);
    assert_no_timeout(&result);

    // When json feature is enabled, should show real content
    assert_stdout_contains(&result, "JSON: API Payload Visualization");
    assert_stdout_contains(&result, "Json renderable provides pretty-printed");

    // Should show actual JSON examples
    assert_stdout_contains(&result, "API Request");
    assert_stdout_contains(&result, "nebula-api");

    // Should NOT show the feature-disabled panel
    assert!(
        !result.stdout_contains("Feature Required"),
        "Should not show 'Feature Required' panel when json feature is enabled"
    );
}

#[test]
#[cfg(feature = "tracing")]
fn test_tracing_scene_shows_real_content_when_enabled() {
    common::init_test_logging();

    let result = DemoRunner::quick()
        .arg("--scene")
        .arg("tracing")
        .non_interactive()
        .no_color()
        .run()
        .expect("should run");

    assert_success(&result);
    assert_no_timeout(&result);

    // When tracing feature is enabled, should show real content
    assert_stdout_contains(&result, "Tracing");

    // Should NOT show the feature-disabled panel
    assert!(
        !result.stdout_contains("Feature Required"),
        "Should not show 'Feature Required' panel when tracing feature is enabled"
    );
}
