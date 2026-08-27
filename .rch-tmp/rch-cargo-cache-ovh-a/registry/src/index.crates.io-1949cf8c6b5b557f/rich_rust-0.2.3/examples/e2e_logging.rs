//! End-to-end logging demo for rich_rust.
//!
//! Expected output:
//! - All 5 log levels are styled (trace/debug/info/warn/error).
//! - Multi-line messages show line breaks.
//! - Paths appear with hyperlinks when enabled.
//! - Keyword highlighting (e.g., GET/POST) is visible.
//! - Meta debug lines describe formatting decisions and configuration.
//!
//! Run:
//!   RUST_LOG=debug cargo run --example e2e_logging
//!   RUST_LOG=debug cargo run --example e2e_logging --features tracing

use std::sync::Arc;

use log::{Level, LevelFilter, Log, Record, debug, error, info, trace, warn};
use rich_rust::console::Console;
use rich_rust::prelude::*;

fn init_rich_logger(console: Arc<Console>) -> Result<(), log::SetLoggerError> {
    RichLogger::new(console)
        .level(LevelFilter::Trace)
        .show_time(true)
        .show_level(true)
        .show_path(true)
        .enable_link_path(true)
        .markup(true)
        .time_format("[hour]:[minute]:[second]")
        .init()
}

fn log_levels() {
    trace!("TRACE: verbose diagnostic output");
    debug!("META: debug includes formatting info and decisions");
    info!("INFO: [bold green]Styled info[/] with markup");
    warn!("WARN: multi-line message\nsecond line\nthird line");
    error!(
        "ERROR: failure with debug detail: {:?}",
        std::io::Error::other("demo error")
    );
}

fn log_keywords_and_paths() {
    info!("GET /api/v1/resources (keyword highlight demo)");
    warn!("POST /api/v1/resources failed with 500");
}

fn log_manual_record(logger: &RichLogger, level: Level, message: &str, file: &str, line: u32) {
    let args = format_args!("{message}");
    let record = Record::builder()
        .args(args)
        .level(level)
        .file(Some(file))
        .line(Some(line))
        .module_path(Some("examples::e2e_logging"))
        .build();
    logger.log(&record);
}

fn demo_time_format_variants(console: Arc<Console>) {
    let compact = RichLogger::new(console.clone())
        .level(LevelFilter::Trace)
        .show_time(true)
        .show_level(true)
        .show_path(true)
        .enable_link_path(false)
        .time_format("[hour]:[minute]:[second]");
    log_manual_record(
        &compact,
        Level::Info,
        "META: compact time format demo",
        "examples/e2e_logging.rs",
        60,
    );

    let detailed = RichLogger::new(console)
        .level(LevelFilter::Trace)
        .show_time(true)
        .show_level(true)
        .show_path(true)
        .enable_link_path(false)
        .time_format("[hour]:[minute]:[second].[subsecond]");
    log_manual_record(
        &detailed,
        Level::Info,
        "META: detailed time format demo",
        "examples/e2e_logging.rs",
        74,
    );
}

fn demo_width_variants() {
    let narrow_console = Arc::new(
        Console::builder()
            .width(40)
            .force_terminal(true)
            .markup(true)
            .build(),
    );
    let narrow_logger = RichLogger::new(narrow_console)
        .level(LevelFilter::Trace)
        .show_time(true)
        .show_level(true)
        .show_path(true)
        .enable_link_path(false);
    log_manual_record(
        &narrow_logger,
        Level::Info,
        "META: narrow console width demo (40 cols)",
        "examples/e2e_logging.rs",
        95,
    );
    log_manual_record(
        &narrow_logger,
        Level::Info,
        "Long message to observe how output behaves at narrow widths",
        "examples/e2e_logging.rs",
        100,
    );
}

#[cfg(feature = "tracing")]
fn run_tracing_demo(console: Arc<Console>) {
    use tracing::{Level as TracingLevel, event, info_span};

    let layer = RichTracingLayer::new(console);
    if let Err(err) = layer.init() {
        eprintln!("Failed to install RichTracingLayer: {err}");
        return;
    }

    let span = info_span!("request", method = "GET", path = "/status");
    let _guard = span.enter();
    event!(TracingLevel::INFO, "Tracing event inside span");
    event!(TracingLevel::WARN, code = 503, "Tracing warn with fields");
}

#[cfg(not(feature = "tracing"))]
fn run_tracing_demo(_console: Arc<Console>) {
    info!("Tracing feature not enabled; skipping tracing demo");
}

fn main() {
    let console = Arc::new(
        Console::builder()
            .width(80)
            .force_terminal(true)
            .markup(true)
            .build(),
    );

    if let Err(err) = init_rich_logger(console.clone()) {
        eprintln!("Failed to initialize RichLogger: {err}");
        return;
    }

    debug!("META: logger configured (time, level, path, hyperlinks, markup)");

    log_levels();
    log_keywords_and_paths();
    demo_time_format_variants(console.clone());
    demo_width_variants();
    run_tracing_demo(console);
}
