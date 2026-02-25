//! Criterion benchmarks for rich output performance.
//!
//! Performance targets:
//! - Detection: < 1ms (cached: < 1us)
//! - Simple styled string: < 100us
//! - Table (100 rows): < 10ms
//! - Panel: < 1ms
//! - Progress update: < 1ms
//! - Hyperlink formatting: < 10us

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;

use ms::output::{
    OutputDecision, OutputDecisionReason, OutputDetector, OutputEnvironment, RichOutput,
    Theme, detect_terminal_capabilities,
};
use ms::output::messages::{HintDisplay, InfoRenderer, StatusTracker, SuccessRenderer};
use ms::output::builders::{
    error_panel, quality_bar, quality_bar_plain, search_results_table,
    skill_panel_with_width, success_panel, success_panel_with_width, warning_panel,
};
use ms::cli::output::OutputFormat;

// =============================================================================
// Detection Benchmarks
// =============================================================================

fn detection_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("detection");

    // Detection with explicit environment (no env var lookups)
    group.bench_function("detect_with_env", |b| {
        let env = OutputEnvironment::new(false, false, false, true);
        b.iter(|| {
            let detector = OutputDetector::with_env(
                black_box(OutputFormat::Human),
                black_box(false),
                black_box(env),
            );
            detector.decide()
        });
    });

    // Detection from live environment (includes env var lookups)
    group.bench_function("detect_from_env", |b| {
        b.iter(|| {
            OutputDetector::new(
                black_box(OutputFormat::Human),
                black_box(false),
            )
            .decide()
        });
    });

    // Terminal capabilities detection (color system, unicode, hyperlinks)
    group.bench_function("terminal_capabilities", |b| {
        b.iter(|| detect_terminal_capabilities());
    });

    // RichOutput construction (plain)
    group.bench_function("richoutput_plain", |b| {
        b.iter(|| RichOutput::plain());
    });

    // OutputDecision to RichOutput
    group.bench_function("richoutput_from_decision", |b| {
        let decision = OutputDecision {
            use_rich: false,
            reason: OutputDecisionReason::PlainFormat,
        };
        b.iter(|| RichOutput::from_detection(black_box(&decision)));
    });

    group.finish();
}

// =============================================================================
// Rendering Benchmarks
// =============================================================================

fn rendering_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("rendering");

    let plain = RichOutput::plain();

    // Format styled string (plain mode - no actual styling)
    group.bench_function("format_styled_plain", |b| {
        b.iter(|| plain.format_styled(black_box("Hello, world!"), black_box("bold green")));
    });

    // Format success message
    group.bench_function("format_success_plain", |b| {
        b.iter(|| plain.format_success(black_box("Operation completed")));
    });

    // Format error message
    group.bench_function("format_error_plain", |b| {
        b.iter(|| plain.format_error(black_box("Something went wrong")));
    });

    // Format key-value pair
    group.bench_function("format_key_value_plain", |b| {
        b.iter(|| plain.format_key_value(black_box("Found"), black_box("42 skills")));
    });

    // Batch format: 1000 styled strings
    group.bench_function("batch_1000_styled_plain", |b| {
        b.iter(|| {
            for _ in 0..1000 {
                let _ = plain.format_styled(black_box("test string"), black_box("bold"));
            }
        });
    });

    group.finish();
}

// =============================================================================
// Hyperlink Benchmarks
// =============================================================================

fn hyperlink_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("hyperlinks");

    let plain = RichOutput::plain();

    // Format hyperlink (plain mode - no OSC 8)
    group.bench_function("format_hyperlink_plain", |b| {
        b.iter(|| {
            plain.format_hyperlink(
                black_box("click here"),
                black_box("https://example.com"),
            )
        });
    });

    // Format file hyperlink (plain mode)
    group.bench_function("format_file_hyperlink_plain", |b| {
        let path = std::path::Path::new("/usr/local/bin/ms");
        b.iter(|| {
            plain.format_file_hyperlink(black_box("ms"), black_box(path))
        });
    });

    // Format hyperlink same text/url (short-circuit path)
    group.bench_function("format_hyperlink_same_url", |b| {
        b.iter(|| {
            plain.format_hyperlink(
                black_box("https://example.com"),
                black_box("https://example.com"),
            )
        });
    });

    group.finish();
}

// =============================================================================
// Builder Benchmarks
// =============================================================================

fn builder_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("builders");

    // Success panel
    group.bench_function("success_panel", |b| {
        b.iter(|| {
            success_panel(
                black_box("Created"),
                black_box("Skill saved successfully"),
            )
        });
    });

    group.bench_function("success_panel_80w", |b| {
        b.iter(|| {
            success_panel_with_width(
                black_box("Created"),
                black_box("Skill saved successfully"),
                black_box(80),
            )
        });
    });

    // Error panel
    group.bench_function("error_panel", |b| {
        b.iter(|| {
            error_panel(
                black_box("Not Found"),
                black_box("Skill 'foo' not found in any layer"),
            )
        });
    });

    // Warning panel
    group.bench_function("warning_panel", |b| {
        b.iter(|| {
            warning_panel(
                black_box("Deprecated"),
                black_box("This format will be removed in v2"),
            )
        });
    });

    // Quality bar
    group.bench_function("quality_bar", |b| {
        b.iter(|| quality_bar(black_box(0.85), black_box(20)));
    });

    group.bench_function("quality_bar_plain", |b| {
        b.iter(|| quality_bar_plain(black_box(0.85), black_box(20)));
    });

    // Skill panel
    group.bench_function("skill_panel_80w", |b| {
        b.iter(|| {
            skill_panel_with_width(
                black_box("Debug Rust Builds"),
                black_box("Diagnose Rust compiler errors and build failures"),
                black_box("project"),
                black_box(80),
            )
        });
    });

    group.finish();
}

// =============================================================================
// Table Benchmarks
// =============================================================================

fn table_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("tables");

    // Small table (5 rows)
    let small_results: Vec<(&str, f32, &str, &str)> = (0..5)
        .map(|i| {
            match i {
                0 => ("debug-rust", 0.95, "project", "Debug Rust builds"),
                1 => ("test-runner", 0.88, "org", "Run test suites"),
                2 => ("git-workflow", 0.82, "global", "Git workflow helpers"),
                3 => ("perf-tuning", 0.75, "project", "Performance optimization"),
                _ => ("code-review", 0.71, "org", "Code review checklist"),
            }
        })
        .collect();

    group.bench_function("search_table_5_rows", |b| {
        b.iter(|| search_results_table(black_box(&small_results), black_box(120)));
    });

    // Medium table (100 rows)
    let medium_results: Vec<(&str, f32, &str, &str)> = (0..100)
        .map(|_| ("skill-name", 0.85_f32, "project", "A skill description"))
        .collect();

    group.throughput(Throughput::Elements(100));
    group.bench_function("search_table_100_rows", |b| {
        b.iter(|| search_results_table(black_box(&medium_results), black_box(120)));
    });

    // Large table (1000 rows)
    let large_results: Vec<(&str, f32, &str, &str)> = (0..1000)
        .map(|_| ("skill-name", 0.85_f32, "project", "A skill description"))
        .collect();

    group.throughput(Throughput::Elements(1000));
    group.bench_function("search_table_1000_rows", |b| {
        b.iter(|| search_results_table(black_box(&large_results), black_box(120)));
    });

    group.finish();
}

// =============================================================================
// Message Renderer Benchmarks
// =============================================================================

fn message_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("messages");

    let plain = RichOutput::plain();

    // SuccessRenderer
    group.bench_function("success_renderer_simple", |b| {
        b.iter(|| {
            SuccessRenderer::new(black_box(&plain), black_box("Skill created"))
                .render();
        });
    });

    group.bench_function("success_renderer_with_steps", |b| {
        b.iter(|| {
            SuccessRenderer::new(black_box(&plain), black_box("Import complete"))
                .next_step("Run `ms list` to see skills")
                .next_step("Run `ms search` to find them")
                .detail("5 skills imported from org layer")
                .render();
        });
    });

    // InfoRenderer
    group.bench_function("info_renderer_simple", |b| {
        b.iter(|| {
            InfoRenderer::new(black_box(&plain), black_box("Indexing 42 skills"))
                .render();
        });
    });

    group.bench_function("info_renderer_with_context", |b| {
        b.iter(|| {
            InfoRenderer::new(black_box(&plain), black_box("Search complete"))
                .context("Results", "12")
                .context("Time", "45ms")
                .render();
        });
    });

    // HintDisplay
    group.bench_function("hint_display", |b| {
        b.iter(|| {
            HintDisplay::new(black_box(&plain), black_box("Use --explain for details"))
                .render();
        });
    });

    // StatusTracker
    group.bench_function("status_tracker_3_steps", |b| {
        b.iter(|| {
            let mut tracker = StatusTracker::new(black_box(&plain), black_box("Import"));
            tracker.step("Parsing");
            tracker.step("Validating");
            tracker.step("Writing");
            tracker.complete("Done");
        });
    });

    group.finish();
}

// =============================================================================
// Theme Benchmarks
// =============================================================================

fn theme_benchmarks(c: &mut Criterion) {
    let mut group = c.benchmark_group("theme");

    // Theme construction
    group.bench_function("theme_default", |b| {
        b.iter(|| Theme::default());
    });

    // Theme auto-detect
    group.bench_function("theme_auto_detect", |b| {
        b.iter(|| Theme::auto_detect());
    });

    // Theme with ASCII fallback
    group.bench_function("theme_ascii_fallback", |b| {
        b.iter(|| Theme::default().with_ascii_fallback());
    });

    // Theme adapt for terminal
    group.bench_function("theme_adapt_terminal", |b| {
        let caps = detect_terminal_capabilities();
        b.iter(|| {
            Theme::default().adapted_for_terminal(black_box(&caps))
        });
    });

    group.finish();
}

// =============================================================================
// Criterion Groups
// =============================================================================

criterion_group!(
    benches,
    detection_benchmarks,
    rendering_benchmarks,
    hyperlink_benchmarks,
    builder_benchmarks,
    table_benchmarks,
    message_benchmarks,
    theme_benchmarks,
);

criterion_main!(benches);
