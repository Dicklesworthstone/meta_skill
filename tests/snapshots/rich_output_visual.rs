//! Visual regression tests for rich output components.
//!
//! These tests capture the rendered output of builders, panels, tables,
//! quality indicators, and message renderers as insta snapshots. Any change
//! to rendering will be caught as a snapshot diff, requiring explicit approval.
//!
//! All tests use plain mode to avoid terminal-dependent ANSI codes.

use insta::assert_snapshot;

use ms::output::builders::{
    bulleted_list, bulleted_list_plain, error_panel_with_hint_and_width, error_panel_with_width,
    key_value_plain, key_value_table, numbered_list, progress_line, progress_line_plain,
    quality_bar, quality_bar_plain, quality_indicator, search_results_table,
    search_results_table_with_id, skill_detail_panel_with_width, skill_panel_with_width,
    success_panel_with_width, warning_panel_with_width,
};

// =============================================================================
// Success Panel Visual Tests
// =============================================================================

#[test]
fn visual_success_panel_simple() {
    let output = success_panel_with_width("Created", "Skill saved to project layer", 60);
    assert_snapshot!("success_panel_simple", output);
}

#[test]
fn visual_success_panel_long_message() {
    let output = success_panel_with_width(
        "Import Complete",
        "Successfully imported 42 skills from the organization archive into the local project layer with full validation",
        80,
    );
    assert_snapshot!("success_panel_long_message", output);
}

#[test]
fn visual_success_panel_narrow() {
    let output = success_panel_with_width("OK", "Done", 40);
    assert_snapshot!("success_panel_narrow", output);
}

// =============================================================================
// Error Panel Visual Tests
// =============================================================================

#[test]
fn visual_error_panel_simple() {
    let output = error_panel_with_width("Not Found", "Skill 'foo' not found in any layer", 60);
    assert_snapshot!("error_panel_simple", output);
}

#[test]
fn visual_error_panel_with_hint() {
    let output = error_panel_with_hint_and_width(
        "Validation Failed",
        "Missing required field: description",
        "Add a 'description' field to the YAML frontmatter",
        60,
    );
    assert_snapshot!("error_panel_with_hint", output);
}

// =============================================================================
// Warning Panel Visual Tests
// =============================================================================

#[test]
fn visual_warning_panel() {
    let output = warning_panel_with_width(
        "Deprecated",
        "The --legacy flag will be removed in v2.0",
        60,
    );
    assert_snapshot!("warning_panel", output);
}

// =============================================================================
// Skill Panel Visual Tests
// =============================================================================

#[test]
fn visual_skill_panel() {
    let output = skill_panel_with_width(
        "Debug Rust Builds",
        "Diagnose Rust compiler errors and build failures with structured debugging steps",
        "project",
        80,
    );
    assert_snapshot!("skill_panel", output);
}

#[test]
fn visual_skill_detail_panel() {
    let output = skill_detail_panel_with_width(
        "Debug Rust Builds",
        "Diagnose Rust compiler errors and build failures",
        "project",
        0.92,
        "Tags: rust, debugging, build\nSource: template/debugging",
        80,
    );
    assert_snapshot!("skill_detail_panel", output);
}

#[test]
fn visual_skill_detail_panel_low_quality() {
    let output = skill_detail_panel_with_width(
        "Git Workflow",
        "Standard git workflow for feature branches",
        "org",
        0.35,
        "Tags: git, workflow",
        80,
    );
    assert_snapshot!("skill_detail_panel_low_quality", output);
}

// =============================================================================
// Search Results Table Visual Tests
// =============================================================================

#[test]
fn visual_search_results_empty() {
    let results: Vec<(&str, f32, &str, &str)> = vec![];
    let table = search_results_table(&results, 80);
    let output = table.render_plain(80);
    assert_snapshot!("search_results_empty", output);
}

#[test]
fn visual_search_results_single() {
    let results = vec![("debug-rust", 0.95_f32, "project", "Debug Rust builds and compiler errors")];
    let table = search_results_table(&results, 80);
    let output = table.render_plain(80);
    assert_snapshot!("search_results_single", output);
}

#[test]
fn visual_search_results_multiple() {
    let results = vec![
        ("debug-rust", 0.95, "project", "Debug Rust builds and compiler errors"),
        ("test-runner", 0.88, "org", "Run test suites efficiently"),
        ("git-workflow", 0.82, "global", "Git workflow helpers for feature branches"),
        ("perf-tuning", 0.75, "project", "Performance optimization and profiling"),
        ("code-review", 0.71, "org", "Code review checklist and guidelines"),
    ];
    let table = search_results_table(&results, 100);
    let output = table.render_plain(100);
    assert_snapshot!("search_results_multiple", output);
}

#[test]
fn visual_search_results_with_id() {
    let results = vec![
        ("debug-rust", "Debug Rust Builds", 0.95_f32, "project"),
        ("test-runner", "Run Test Suites", 0.88, "org"),
    ];
    let table = search_results_table_with_id(&results, 100);
    let output = table.render_plain(100);
    assert_snapshot!("search_results_with_id", output);
}

// =============================================================================
// Quality Bar Visual Tests
// =============================================================================

#[test]
fn visual_quality_bar_zero() {
    let output = quality_bar(0.0, 20);
    assert_snapshot!("quality_bar_zero", output);
}

#[test]
fn visual_quality_bar_half() {
    let output = quality_bar(0.5, 20);
    assert_snapshot!("quality_bar_half", output);
}

#[test]
fn visual_quality_bar_full() {
    let output = quality_bar(1.0, 20);
    assert_snapshot!("quality_bar_full", output);
}

#[test]
fn visual_quality_bar_plain_half() {
    let output = quality_bar_plain(0.5, 20);
    assert_snapshot!("quality_bar_plain_half", output);
}

#[test]
fn visual_quality_indicator_low() {
    let output = quality_indicator(0.2);
    assert_snapshot!("quality_indicator_low", output);
}

#[test]
fn visual_quality_indicator_medium() {
    let output = quality_indicator(0.6);
    assert_snapshot!("quality_indicator_medium", output);
}

#[test]
fn visual_quality_indicator_high() {
    let output = quality_indicator(0.95);
    assert_snapshot!("quality_indicator_high", output);
}

// =============================================================================
// Progress Line Visual Tests
// =============================================================================

#[test]
fn visual_progress_line_start() {
    let output = progress_line(0, 100, "Processing", 60);
    assert_snapshot!("progress_line_start", output);
}

#[test]
fn visual_progress_line_middle() {
    let output = progress_line(50, 100, "Processing", 60);
    assert_snapshot!("progress_line_middle", output);
}

#[test]
fn visual_progress_line_complete() {
    let output = progress_line(100, 100, "Done", 60);
    assert_snapshot!("progress_line_complete", output);
}

#[test]
fn visual_progress_line_plain_middle() {
    let output = progress_line_plain(50, 100, "Processing", 60);
    assert_snapshot!("progress_line_plain_middle", output);
}

// =============================================================================
// List Visual Tests
// =============================================================================

#[test]
fn visual_bulleted_list() {
    let items = &["First item", "Second item", "Third item"];
    let output = bulleted_list(items);
    assert_snapshot!("bulleted_list", output);
}

#[test]
fn visual_bulleted_list_plain() {
    let items = &["Alpha", "Beta", "Gamma"];
    let output = bulleted_list_plain(items);
    assert_snapshot!("bulleted_list_plain", output);
}

#[test]
fn visual_numbered_list() {
    let items = &["Parse input", "Validate schema", "Write to database"];
    let output = numbered_list(items);
    assert_snapshot!("numbered_list", output);
}

// =============================================================================
// Key-Value Visual Tests
// =============================================================================

#[test]
fn visual_key_value_table() {
    let pairs = &[
        ("Name", "Debug Rust Builds"),
        ("Layer", "project"),
        ("Tags", "rust, debugging, build"),
        ("Score", "0.92"),
    ];
    let table = key_value_table(pairs);
    let output = table.render_plain(60);
    assert_snapshot!("key_value_table", output);
}

#[test]
fn visual_key_value_plain() {
    let pairs = &[
        ("Name", "Debug Rust Builds"),
        ("Layer", "project"),
        ("Tags", "rust, debugging, build"),
    ];
    let output = key_value_plain(pairs, ": ");
    assert_snapshot!("key_value_plain", output);
}

// =============================================================================
// Hyperlink Format Visual Tests
// =============================================================================

#[test]
fn visual_hyperlink_plain_mode() {
    let output = ms::output::RichOutput::plain();
    let result = output.format_hyperlink("Documentation", "https://example.com/docs");
    assert_snapshot!("hyperlink_plain", result);
}

#[test]
fn visual_hyperlink_same_url() {
    let output = ms::output::RichOutput::plain();
    let result = output.format_hyperlink("https://example.com", "https://example.com");
    assert_snapshot!("hyperlink_same_url", result);
}

#[test]
fn visual_file_hyperlink_plain() {
    let output = ms::output::RichOutput::plain();
    let result = output.format_file_hyperlink("main.rs", std::path::Path::new("/src/main.rs"));
    assert_snapshot!("file_hyperlink_plain", result);
}

// =============================================================================
// Message Renderer Visual Tests
// =============================================================================

#[test]
fn visual_success_renderer_plain() {
    use ms::output::messages::SuccessRenderer;
    let output = ms::output::RichOutput::plain();
    // Capture stdout by building the formatted parts
    let _renderer = SuccessRenderer::new(&output, "Skill created successfully");
    // The renderer prints to stdout, so we just verify it doesn't panic
    // For snapshot purposes, test the underlying format methods
    let formatted = output.format_success("Skill created successfully");
    assert_snapshot!("success_renderer_format", formatted);
}

#[test]
fn visual_info_format_plain() {
    let output = ms::output::RichOutput::plain();
    let formatted = output.format_info("Indexing 42 skills");
    assert_snapshot!("info_format_plain", formatted);
}

#[test]
fn visual_error_format_plain() {
    let output = ms::output::RichOutput::plain();
    let formatted = output.format_error("Skill not found");
    assert_snapshot!("error_format_plain", formatted);
}

#[test]
fn visual_warning_format_plain() {
    let output = ms::output::RichOutput::plain();
    let formatted = output.format_warning("Deprecated feature");
    assert_snapshot!("warning_format_plain", formatted);
}

#[test]
fn visual_key_value_format_plain() {
    let output = ms::output::RichOutput::plain();
    let formatted = output.format_key_value("Layer", "project");
    assert_snapshot!("key_value_format_plain", formatted);
}
