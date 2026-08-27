//! End-to-end performance regression tests for rich_rust.
//!
//! These tests establish and monitor performance baselines to prevent regressions.
//! Run with: cargo test --test e2e_performance -- --nocapture
//!
//! # Environment Variables
//!
//! - `UPDATE_PERF_BASELINE=1` - Update baselines instead of asserting against them
//! - `PERF_REGRESSION_THRESHOLD=30` - Override default regression threshold (default: 20%)
//! - `RUST_LOG=debug` - Enable detailed logging

mod common;

use common::init_test_logging;
use rich_rust::prelude::*;
use std::time::Instant;

// =============================================================================
// Configuration
// =============================================================================

/// Default regression threshold percentage (50% slower than baseline = failure)
///
/// This threshold is deliberately generous to accommodate CI/shared environments
/// where machine load varies. The goal is to catch major regressions (2x+ slowdowns)
/// while avoiding false positives from load variability.
const DEFAULT_REGRESSION_THRESHOLD: f64 = 50.0;

/// Load baselines from JSON file
fn load_baselines() -> serde_json::Value {
    let content = include_str!("perf_baselines.json");
    serde_json::from_str(content).expect("Failed to parse perf_baselines.json")
}

/// Get baseline value for a specific metric
fn get_baseline_ms(name: &str) -> Option<u64> {
    let baselines = load_baselines();
    baselines["baselines"][name].as_u64()
}

/// Get regression threshold percentage
fn get_regression_threshold() -> f64 {
    std::env::var("PERF_REGRESSION_THRESHOLD")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_REGRESSION_THRESHOLD)
}

/// Check if we should update baselines instead of asserting
fn should_update_baselines() -> bool {
    std::env::var("UPDATE_PERF_BASELINE").is_ok()
}

/// Assert performance is within threshold of baseline
fn assert_perf_within_threshold(name: &str, elapsed_ms: u128) {
    let baseline = match get_baseline_ms(name) {
        Some(b) => b,
        None => {
            tracing::warn!(metric = name, "No baseline found, skipping assertion");
            return;
        }
    };

    let threshold = get_regression_threshold();
    let max_allowed = (baseline as f64 * (1.0 + threshold / 100.0)) as u128;
    let percent_of_baseline = (elapsed_ms as f64 / baseline as f64) * 100.0;

    tracing::info!(
        metric = name,
        elapsed_ms = elapsed_ms,
        baseline_ms = baseline,
        percent_of_baseline = format!("{:.1}%", percent_of_baseline),
        threshold = format!("{}%", threshold),
        "Performance measurement"
    );

    if should_update_baselines() {
        tracing::info!(
            metric = name,
            new_value = elapsed_ms,
            "Baseline update requested (UPDATE_PERF_BASELINE=1)"
        );
        return;
    }

    assert!(
        elapsed_ms <= max_allowed,
        "Performance regression detected for '{}': {}ms > {}ms ({}% of baseline, threshold: {}%)",
        name,
        elapsed_ms,
        max_allowed,
        percent_of_baseline as u64,
        threshold
    );
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Create a large table for performance testing
fn create_large_table(rows: usize, cols: usize) -> Table {
    let mut table = Table::new();

    // Add columns
    for i in 0..cols {
        table = table.with_column(Column::new(format!("Column {}", i + 1)));
    }

    // Add rows
    for r in 0..rows {
        let cells: Vec<String> = (0..cols)
            .map(|c| format!("Row {} Col {}", r + 1, c + 1))
            .collect();
        table.add_row_cells(cells);
    }

    table
}

/// Generate random-ish text for testing
fn generate_text(length: usize) -> String {
    let words = [
        "the",
        "quick",
        "brown",
        "fox",
        "jumps",
        "over",
        "lazy",
        "dog",
        "Lorem",
        "ipsum",
        "dolor",
        "sit",
        "amet",
        "consectetur",
        "adipiscing",
        "elit",
        "sed",
        "do",
        "eiusmod",
        "tempor",
        "incididunt",
        "ut",
        "labore",
    ];

    let mut result = String::with_capacity(length);
    let mut word_idx = 0;

    while result.len() < length {
        if !result.is_empty() {
            result.push(' ');
        }
        result.push_str(words[word_idx % words.len()]);
        word_idx += 1;
    }

    result.truncate(length);
    result
}

/// Generate markup text with varying complexity
fn generate_markup(count: usize, nested: bool) -> String {
    let mut markup = String::new();

    for i in 0..count {
        if nested {
            markup.push_str(&format!(
                "[bold][red]Item {}[/red] with [italic]nested[/italic] styles[/bold] ",
                i + 1
            ));
        } else {
            markup.push_str(&format!("[bold]Item {}[/bold] ", i + 1));
        }
    }

    markup
}

// =============================================================================
// Table Performance Tests
// =============================================================================

#[test]
fn perf_large_table_100x10() {
    init_test_logging();
    tracing::info!("Starting performance test: large table 100x10");

    let table = create_large_table(100, 10);

    let start = Instant::now();
    let segments = table.render(200);
    let elapsed = start.elapsed();

    // Verify rendering produced output
    let output: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(
        output.contains("Column 1") && output.contains("Row 1 Col 1"),
        "Rendered table should include headers and cell values"
    );

    tracing::info!(
        rows = 100,
        columns = 10,
        segment_count = segments.len(),
        output_len = output.len(),
        elapsed_ms = elapsed.as_millis(),
        "Large table 100x10 rendered"
    );

    assert_perf_within_threshold("large_table_100x10_ms", elapsed.as_millis());
}

#[test]
fn perf_large_table_500x20() {
    init_test_logging();
    tracing::info!("Starting performance test: large table 500x20");

    let table = create_large_table(500, 20);

    let start = Instant::now();
    let segments = table.render(300);
    let elapsed = start.elapsed();

    let output: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(
        output.contains("Column 1") && output.contains("Row 1 Col 1"),
        "Rendered table should include headers and cell values"
    );

    tracing::info!(
        rows = 500,
        columns = 20,
        segment_count = segments.len(),
        elapsed_ms = elapsed.as_millis(),
        "Large table 500x20 rendered"
    );

    assert_perf_within_threshold("large_table_500x20_ms", elapsed.as_millis());
}

// =============================================================================
// Color Parsing Performance Tests
// =============================================================================

#[test]
fn perf_color_parse_10000() {
    init_test_logging();
    tracing::info!("Starting performance test: color parsing 10000 unique colors");

    // Generate unique colors to test parsing without cache hits
    let colors: Vec<String> = (0..10000)
        .map(|i| format!("#{:06x}", i % 0xFFFFFF))
        .collect();

    let start = Instant::now();
    let mut parsed_count = 0;

    for color_str in &colors {
        if Color::parse(color_str).is_ok() {
            parsed_count += 1;
        }
    }

    let elapsed = start.elapsed();

    tracing::info!(
        color_count = colors.len(),
        parsed_count = parsed_count,
        elapsed_ms = elapsed.as_millis(),
        "Color parsing complete"
    );

    assert_eq!(parsed_count, 10000, "All colors should parse successfully");
    assert_perf_within_threshold("color_parse_10000_ms", elapsed.as_millis());
}

#[test]
fn perf_color_parse_10000_cached() {
    init_test_logging();
    tracing::info!("Starting performance test: color parsing 10000 with cache hits");

    // Use a small set of colors repeatedly to test cache performance
    let base_colors = [
        "red", "green", "blue", "yellow", "cyan", "magenta", "#ff0000", "#00ff00",
    ];

    // Warm up the cache
    for color in &base_colors {
        let _ = Color::parse(color);
    }

    let start = Instant::now();
    let mut parsed_count = 0;

    for i in 0..10000 {
        let color_str = base_colors[i % base_colors.len()];
        if Color::parse(color_str).is_ok() {
            parsed_count += 1;
        }
    }

    let elapsed = start.elapsed();

    tracing::info!(
        iteration_count = 10000,
        unique_colors = base_colors.len(),
        parsed_count = parsed_count,
        elapsed_ms = elapsed.as_millis(),
        "Cached color parsing complete"
    );

    assert_eq!(parsed_count, 10000, "All colors should parse successfully");
    assert_perf_within_threshold("color_parse_10000_cached_ms", elapsed.as_millis());
}

// =============================================================================
// Text Wrapping Performance Tests
// =============================================================================

#[test]
fn perf_text_wrap_10000_chars() {
    init_test_logging();
    tracing::info!("Starting performance test: text wrap 10000 chars");

    let text_content = generate_text(10000);
    let text = Text::new(&text_content);

    let start = Instant::now();
    let wrapped = text.wrap(80);
    let elapsed = start.elapsed();

    tracing::info!(
        input_chars = text_content.len(),
        output_lines = wrapped.len(),
        elapsed_ms = elapsed.as_millis(),
        "Text wrapping complete"
    );

    assert!(
        !wrapped.is_empty() && wrapped.iter().all(|line| line.cell_len() <= 80),
        "Wrapped lines should be non-empty and respect the width constraint"
    );
    assert_perf_within_threshold("text_wrap_10000_chars_ms", elapsed.as_millis());
}

#[test]
fn perf_text_wrap_50000_chars() {
    init_test_logging();
    tracing::info!("Starting performance test: text wrap 50000 chars");

    let text_content = generate_text(50000);
    let text = Text::new(&text_content);

    let start = Instant::now();
    let wrapped = text.wrap(80);
    let elapsed = start.elapsed();

    tracing::info!(
        input_chars = text_content.len(),
        output_lines = wrapped.len(),
        elapsed_ms = elapsed.as_millis(),
        "Large text wrapping complete"
    );

    assert!(
        !wrapped.is_empty() && wrapped.iter().all(|line| line.cell_len() <= 80),
        "Wrapped lines should be non-empty and respect the width constraint"
    );
    assert_perf_within_threshold("text_wrap_50000_chars_ms", elapsed.as_millis());
}

// =============================================================================
// Markup Parsing Performance Tests
// =============================================================================

#[test]
fn perf_markup_parse_simple_1000() {
    init_test_logging();
    tracing::info!("Starting performance test: markup parse 1000 simple items");

    let markup = generate_markup(1000, false);

    let start = Instant::now();
    let result = rich_rust::markup::render(&markup);
    let elapsed = start.elapsed();

    let text = result.expect("Markup should parse successfully");

    tracing::info!(
        markup_len = markup.len(),
        result_chars = text.plain().len(),
        elapsed_ms = elapsed.as_millis(),
        "Simple markup parsing complete"
    );

    assert_perf_within_threshold("markup_parse_simple_1000_ms", elapsed.as_millis());
}

#[test]
fn perf_markup_parse_nested_1000() {
    init_test_logging();
    tracing::info!("Starting performance test: markup parse 1000 nested items");

    let markup = generate_markup(1000, true);

    let start = Instant::now();
    let result = rich_rust::markup::render(&markup);
    let elapsed = start.elapsed();

    let text = result.expect("Nested markup should parse successfully");

    tracing::info!(
        markup_len = markup.len(),
        result_chars = text.plain().len(),
        elapsed_ms = elapsed.as_millis(),
        "Nested markup parsing complete"
    );

    assert_perf_within_threshold("markup_parse_nested_1000_ms", elapsed.as_millis());
}

// =============================================================================
// Segment Operations Performance Tests
// =============================================================================

#[test]
fn perf_segment_merge_10000() {
    init_test_logging();
    tracing::info!("Starting performance test: segment merge 10000");

    // Create many segments
    let style = Style::new().bold();
    let segments: Vec<Segment> = (0..10000)
        .map(|i| Segment::new(format!("Seg{} ", i), Some(style.clone())))
        .collect();

    let start = Instant::now();

    // Merge consecutive segments with same style
    let mut simplified: Vec<Segment> = Vec::new();
    for seg in segments {
        if let Some(last) = simplified.last_mut()
            && last.style == seg.style
        {
            last.text.to_mut().push_str(&seg.text);
            continue;
        }
        simplified.push(seg);
    }

    let elapsed = start.elapsed();

    tracing::info!(
        input_count = 10000,
        merged_count = simplified.len(),
        elapsed_ms = elapsed.as_millis(),
        "Segment merging complete"
    );

    assert_perf_within_threshold("segment_merge_10000_ms", elapsed.as_millis());
}

// =============================================================================
// Style Operations Performance Tests
// =============================================================================

#[test]
fn perf_style_combine_10000() {
    init_test_logging();
    tracing::info!("Starting performance test: style combine 10000");

    let base_styles = [
        Style::new().bold(),
        Style::new().italic(),
        Style::new().underline(),
        Style::new().color(Color::parse("red").unwrap()),
        Style::new().bgcolor(Color::parse("blue").unwrap()),
    ];

    let start = Instant::now();
    let mut result = Style::default();

    for i in 0..10000 {
        let style = &base_styles[i % base_styles.len()];
        result = result.combine(style);
    }

    let elapsed = start.elapsed();

    tracing::info!(
        iterations = 10000,
        final_bold = result.attributes.contains(Attributes::BOLD),
        elapsed_ms = elapsed.as_millis(),
        "Style combining complete"
    );

    assert_perf_within_threshold("style_combine_10000_ms", elapsed.as_millis());
}

// =============================================================================
// Memory Stress Test
// =============================================================================

#[test]
fn perf_memory_stress_large_document() {
    init_test_logging();
    tracing::info!("Starting performance test: memory stress with large document");

    // Create a large document with multiple elements
    let _console = Console::new(); // For future use
    let width = 120;

    // Multiple tables
    let mut all_segments: Vec<Segment> = Vec::new();

    for table_num in 0..10 {
        let table = create_large_table(50, 5).title(format!("Table {}", table_num + 1));

        all_segments.extend(table.render(width));
    }

    // Multiple panels
    for _ in 0..10 {
        let content = format!("Panel content {}", "X".repeat(100));
        let panel = Panel::from_text(&content).width(50).expand(false);
        let segments: Vec<Segment> = panel
            .render(80)
            .into_iter()
            .map(|s| s.into_owned())
            .collect();
        all_segments.extend(segments);
    }

    // Multiple rules
    for rule_num in 0..30 {
        let title = format!("Section {}", rule_num + 1);
        let rule = Rule::with_title(title); // Takes ownership
        all_segments.extend(rule.render(width));
    }

    let total_text_len: usize = all_segments.iter().map(|s| s.text.len()).sum();

    tracing::info!(
        segment_count = all_segments.len(),
        total_text_bytes = total_text_len,
        tables = 10,
        panels = 20,
        rules = 30,
        "Large document rendering complete"
    );

    // Verify reasonable bounds
    assert!(
        all_segments.len() < 1_000_000,
        "Segment count should be bounded"
    );
    assert!(total_text_len < 10_000_000, "Total text should be bounded");
}

// =============================================================================
// Baseline Summary Test
// =============================================================================

#[test]
fn perf_print_baseline_summary() {
    init_test_logging();

    let baselines = load_baselines();
    let threshold = get_regression_threshold();

    tracing::info!(
        version = baselines["version"].as_str().unwrap_or("unknown"),
        regression_threshold = format!("{}%", threshold),
        "Performance baseline configuration"
    );

    if let Some(baseline_map) = baselines["baselines"].as_object() {
        for (name, value) in baseline_map {
            let metric_name: &str = name.as_str();
            let baseline_val: u64 = value.as_u64().unwrap_or(0);
            let max_allowed: u64 = (baseline_val as f64 * (1.0 + threshold / 100.0)) as u64;
            tracing::info!(
                metric = metric_name,
                baseline_ms = baseline_val,
                max_allowed_ms = max_allowed,
                "Baseline"
            );
        }
    }
}
