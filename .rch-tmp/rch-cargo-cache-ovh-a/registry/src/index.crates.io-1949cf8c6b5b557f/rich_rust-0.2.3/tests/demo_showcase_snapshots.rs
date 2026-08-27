//! Snapshot tests for demo_showcase scenes.
//!
//! These tests capture the expected output of various scenes and detect
//! visual regressions using insta snapshots.
//!
//! ## Running Tests
//!
//! ```bash
//! # Run all snapshot tests
//! cargo test --test demo_showcase_snapshots
//!
//! # Update snapshots when intentional changes are made
//! cargo insta test --accept -- --test demo_showcase_snapshots
//!
//! # Review pending snapshots interactively
//! cargo insta review
//! ```

mod common;
mod demo_showcase_harness;

use demo_showcase_harness::{DemoRunner, assertions::*};

/// Strip ANSI escape codes for text-only snapshot comparison.
fn strip_ansi(s: &str) -> String {
    let ansi_regex = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    ansi_regex.replace_all(s, "").to_string()
}

/// Normalize output for stable snapshots:
/// - Strip ANSI codes
/// - Normalize line endings
/// - Trim trailing whitespace from lines
/// - Filter out lines containing time-varying values (elapsed, timestamps)
fn normalize_for_snapshot(s: &str) -> String {
    // Regex to match microsecond timestamp values like "12.313µs"
    let timestamp_regex = regex::Regex::new(r"\d+\.\d+µs").unwrap();

    strip_ansi(s)
        .lines()
        .filter(|line| !line.contains("elapsed"))
        .filter(|line| !timestamp_regex.is_match(line))
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

// ============================================================================
// Debug Tools Scene Snapshots
// ============================================================================

#[test]
fn snapshot_debug_tools_scene() {
    common::init_test_logging();

    let result = DemoRunner::new()
        .arg("--scene")
        .arg("debug_tools")
        .arg("--quick")
        .arg("--seed")
        .arg("42")
        .arg("--color-system")
        .arg("none")
        .arg("--no-interactive")
        .timeout_secs(15)
        .run()
        .expect("should run");

    assert_success(&result);
    assert_no_timeout(&result);

    let normalized = normalize_for_snapshot(&result.stdout);
    insta::assert_snapshot!("debug_tools_scene", normalized);
}

// ============================================================================
// Dashboard Scene Snapshots
// ============================================================================

/// Snapshot for dashboard scene in non-interactive mode.
///
/// This test captures the dashboard output with:
/// - Pipeline progress display
/// - Services status panel
/// - Log stream panel
#[test]
fn snapshot_dashboard_scene() {
    common::init_test_logging();

    let result = DemoRunner::new()
        .arg("--scene")
        .arg("dashboard")
        .arg("--quick")
        .arg("--seed")
        .arg("42")
        .arg("--color-system")
        .arg("none")
        .arg("--no-interactive")
        .timeout_secs(15)
        .run()
        .expect("should run");

    assert_success(&result);
    assert_no_timeout(&result);

    let normalized = normalize_for_snapshot(&result.stdout);
    insta::assert_snapshot!("dashboard_scene", normalized);
}

// ============================================================================
// Traceback Scene Snapshots
// ============================================================================

#[test]
fn snapshot_traceback_scene() {
    common::init_test_logging();

    let result = DemoRunner::new()
        .arg("--scene")
        .arg("traceback")
        .arg("--quick")
        .arg("--seed")
        .arg("42")
        .arg("--color-system")
        .arg("none")
        .arg("--no-interactive")
        .timeout_secs(15)
        .run()
        .expect("should run");

    assert_success(&result);
    assert_no_timeout(&result);

    let normalized = normalize_for_snapshot(&result.stdout);
    insta::assert_snapshot!("traceback_scene", normalized);
}

// ============================================================================
// Table Scene Snapshots
// ============================================================================

#[test]
fn snapshot_table_scene() {
    common::init_test_logging();

    let result = DemoRunner::new()
        .arg("--scene")
        .arg("table")
        .arg("--quick")
        .arg("--seed")
        .arg("42")
        .arg("--color-system")
        .arg("none")
        .arg("--no-interactive")
        .timeout_secs(15)
        .run()
        .expect("should run");

    assert_success(&result);
    assert_no_timeout(&result);

    let normalized = normalize_for_snapshot(&result.stdout);
    insta::assert_snapshot!("table_scene", normalized);
}

// ============================================================================
// Hero Scene Snapshots
// ============================================================================

#[test]
fn snapshot_hero_scene() {
    common::init_test_logging();

    let result = DemoRunner::new()
        .arg("--scene")
        .arg("hero")
        .arg("--quick")
        .arg("--seed")
        .arg("42")
        .arg("--color-system")
        .arg("none")
        .arg("--no-interactive")
        .timeout_secs(15)
        .run()
        .expect("should run");

    assert_success(&result);
    assert_no_timeout(&result);

    let normalized = normalize_for_snapshot(&result.stdout);
    insta::assert_snapshot!("hero_scene", normalized);
}

// ============================================================================
// Scene List Snapshot
// ============================================================================

#[test]
fn snapshot_scene_list() {
    common::init_test_logging();

    let result = DemoRunner::new()
        .arg("--list-scenes")
        .arg("--color-system")
        .arg("none")
        .timeout_secs(10)
        .run()
        .expect("should run");

    assert_success(&result);

    let normalized = normalize_for_snapshot(&result.stdout);
    insta::assert_snapshot!("scene_list", normalized);
}

// ============================================================================
// Content Scene Snapshots (JSON, Markdown, Syntax)
// ============================================================================

/// Snapshot for JSON scene with feature enabled.
///
/// This test captures the JSON deep-dive output including:
/// - API request/response payloads
/// - Custom theme demonstration
/// - Pretty-printed JSON with syntax highlighting
#[test]
#[cfg(feature = "showcase")]
fn snapshot_json_scene() {
    common::init_test_logging();

    let result = DemoRunner::new()
        .arg("--scene")
        .arg("json")
        .arg("--quick")
        .arg("--seed")
        .arg("42")
        .arg("--color-system")
        .arg("none")
        .arg("--no-interactive")
        .timeout_secs(15)
        .run()
        .expect("should run");

    assert_success(&result);
    assert_no_timeout(&result);

    let normalized = normalize_for_snapshot(&result.stdout);
    insta::assert_snapshot!("json_scene", normalized);
}

/// Snapshot for Markdown scene with feature enabled.
///
/// This test captures the Markdown deep-dive output including:
/// - Release notes with headings, lists, code fences
/// - Runbook excerpt with deployment instructions
/// - CommonMark + GFM rendering
#[test]
#[cfg(feature = "showcase")]
fn snapshot_markdown_scene() {
    common::init_test_logging();

    let result = DemoRunner::new()
        .arg("--scene")
        .arg("markdown")
        .arg("--quick")
        .arg("--seed")
        .arg("42")
        .arg("--color-system")
        .arg("none")
        .arg("--no-interactive")
        .timeout_secs(15)
        .run()
        .expect("should run");

    assert_success(&result);
    assert_no_timeout(&result);

    let normalized = normalize_for_snapshot(&result.stdout);
    insta::assert_snapshot!("markdown_scene", normalized);
}

/// Snapshot for Syntax scene with feature enabled.
///
/// This test captures the Syntax deep-dive output including:
/// - TOML config with line numbers
/// - YAML CI/CD pipeline
/// - Rust code snippet
/// - Theme comparison demo
#[test]
#[cfg(feature = "showcase")]
fn snapshot_syntax_scene() {
    common::init_test_logging();

    let result = DemoRunner::new()
        .arg("--scene")
        .arg("syntax")
        .arg("--quick")
        .arg("--seed")
        .arg("42")
        .arg("--color-system")
        .arg("none")
        .arg("--no-interactive")
        .timeout_secs(15)
        .run()
        .expect("should run");

    assert_success(&result);
    assert_no_timeout(&result);

    let normalized = normalize_for_snapshot(&result.stdout);
    insta::assert_snapshot!("syntax_scene", normalized);
}
