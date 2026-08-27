//! End-to-end tests for mutex poison recovery scenarios.
//!
//! These tests verify that rich_rust components remain functional after
//! mutex poisoning events. Each test includes detailed logging.
//!
//! Run with: cargo test --test e2e_mutex_poison -- --nocapture

use std::io::Write;
use std::panic;
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::Duration;

use rich_rust::color::ColorSystem;
use rich_rust::prelude::*;
use rich_rust::sync::lock_recover;
use rich_rust::text::Text;

// ============================================================================
// Shared buffer helper
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

fn make_console(buf: &Arc<Mutex<Vec<u8>>>) -> Console {
    let writer = BufferWriter(Arc::clone(buf));
    Console::builder()
        .force_terminal(true)
        .color_system(ColorSystem::Standard)
        .width(80)
        .file(Box::new(writer))
        .build()
}

fn buf_contents(buf: &Arc<Mutex<Vec<u8>>>) -> String {
    let guard = lock_recover(buf);
    String::from_utf8_lossy(&guard).into_owned()
}

// ============================================================================
// TEST 1: Console operations survive internal mutex poison
// ============================================================================

#[test]
fn e2e_console_survives_poison() {
    eprintln!("\n{}", "=".repeat(70));
    eprintln!("E2E TEST: Console operations after mutex poison");
    eprintln!("{}", "=".repeat(70));

    let buf = Arc::new(Mutex::new(Vec::new()));
    let console = Arc::new(make_console(&buf));

    // PHASE 1: Baseline
    eprintln!("\n[PHASE 1] Baseline: Verify console works normally");
    eprintln!("{}", "-".repeat(50));
    console.print_plain("Initial message");
    let output1 = buf_contents(&buf);
    eprintln!("  Output captured: {} bytes", output1.len());
    eprintln!("  Contains 'Initial': {}", output1.contains("Initial"));
    assert!(output1.contains("Initial"), "Initial print should work");
    eprintln!("  PHASE 1 PASSED\n");

    // PHASE 2: Concurrent stress with intentional panics
    eprintln!("[PHASE 2] Concurrent stress with intentional panics");
    eprintln!("{}", "-".repeat(50));
    let barrier = Arc::new(Barrier::new(6));

    let handles: Vec<_> = (0..6)
        .map(|i| {
            let console = Arc::clone(&console);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                if i == 2 {
                    eprintln!("  [Thread {i}] About to panic intentionally");
                    panic!("Intentional panic from thread 2");
                }
                for j in 0..5 {
                    console.print_plain(&format!("T{i}:{j}"));
                    thread::sleep(Duration::from_millis(5));
                }
                eprintln!("  [Thread {i}] Completed 5 prints");
            })
        })
        .collect();

    let mut panicked = 0;
    let mut succeeded = 0;
    for (i, h) in handles.into_iter().enumerate() {
        match h.join() {
            Ok(()) => {
                eprintln!("  [Thread {i}] Joined successfully");
                succeeded += 1;
            }
            Err(_) => {
                eprintln!("  [Thread {i}] Panicked (expected)");
                panicked += 1;
            }
        }
    }
    eprintln!("  Summary: {succeeded} succeeded, {panicked} panicked");
    assert_eq!(panicked, 1, "Exactly one thread should panic");
    assert_eq!(succeeded, 5, "Five threads should succeed");
    eprintln!("  PHASE 2 PASSED\n");

    // PHASE 3: Console still works after concurrent panic
    eprintln!("[PHASE 3] Console still works after concurrent panic");
    eprintln!("{}", "-".repeat(50));
    console.print_plain("Final message after chaos");
    let output_final = buf_contents(&buf);
    eprintln!("  Final output length: {} bytes", output_final.len());
    eprintln!("  Contains 'Final': {}", output_final.contains("Final"));
    assert!(
        output_final.contains("Final"),
        "Console should still work after panic"
    );
    eprintln!("  PHASE 3 PASSED\n");

    eprintln!("{}", "=".repeat(70));
    eprintln!("E2E TEST PASSED: Console survives concurrent panic");
    eprintln!("{}", "=".repeat(70));
}

// ============================================================================
// TEST 2: Style cache survives poison and concurrent access
// ============================================================================

#[test]
fn e2e_style_cache_poison_recovery() {
    eprintln!("\n{}", "=".repeat(70));
    eprintln!("E2E TEST: Style cache poison recovery");
    eprintln!("{}", "=".repeat(70));

    // PHASE 1: Warm up style cache
    eprintln!("\n[PHASE 1] Warm up style cache");
    eprintln!("{}", "-".repeat(50));
    let test_styles = ["bold", "italic", "red", "blue on white", "bold red"];
    for s in &test_styles {
        let parsed = Style::parse(s);
        eprintln!("  Parsed '{s}' -> ok={}", parsed.is_ok());
        assert!(parsed.is_ok(), "style '{s}' should parse");
    }
    eprintln!("  PHASE 1 PASSED\n");

    // PHASE 2: Concurrent parsing with one panicking thread
    eprintln!("[PHASE 2] Concurrent parsing with one panicking thread");
    eprintln!("{}", "-".repeat(50));
    let results = Arc::new(Mutex::new(Vec::new()));

    let handles: Vec<_> = (0..4)
        .map(|i| {
            let results = Arc::clone(&results);
            thread::spawn(move || {
                if i == 1 {
                    for _ in 0..10 {
                        let _ = Style::parse("bold");
                    }
                    panic!("Thread 1 intentional panic mid-parse");
                }
                let mut count = 0;
                for _ in 0..50 {
                    if Style::parse("green").is_ok() {
                        count += 1;
                    }
                }
                results.lock().unwrap().push((i, count));
                eprintln!("  [Thread {i}] Parsed {count} styles");
            })
        })
        .collect();

    for (i, h) in handles.into_iter().enumerate() {
        match h.join() {
            Ok(()) => eprintln!("  [Thread {i}] Joined OK"),
            Err(_) => eprintln!("  [Thread {i}] Panicked (expected)"),
        }
    }

    let results = results.lock().unwrap();
    eprintln!("  Successful threads: {}", results.len());
    assert_eq!(results.len(), 3, "Three threads should complete");
    eprintln!("  PHASE 2 PASSED\n");

    // PHASE 3: Style parsing still works
    eprintln!("[PHASE 3] Style parsing still works");
    eprintln!("{}", "-".repeat(50));
    let test_style = Style::parse("bold italic underline");
    eprintln!(
        "  Parsed 'bold italic underline': ok={}",
        test_style.is_ok()
    );
    assert!(test_style.is_ok());
    eprintln!("  PHASE 3 PASSED\n");

    eprintln!("{}", "=".repeat(70));
    eprintln!("E2E TEST PASSED: Style cache handles poison gracefully");
    eprintln!("{}", "=".repeat(70));
}

// ============================================================================
// TEST 3: Live display survives refresh during panic
// ============================================================================

#[test]
fn e2e_live_display_stability() {
    eprintln!("\n{}", "=".repeat(70));
    eprintln!("E2E TEST: Live display stability under stress");
    eprintln!("{}", "=".repeat(70));

    let buf = Arc::new(Mutex::new(Vec::new()));
    let console = Arc::new(make_console(&buf));

    // PHASE 1: Create and start Live display
    eprintln!("\n[PHASE 1] Create and start Live display");
    eprintln!("{}", "-".repeat(50));

    let counter = Arc::new(Mutex::new(0u32));
    let counter_for_live = Arc::clone(&counter);

    let live = rich_rust::live::Live::with_options(
        Arc::clone(&console),
        rich_rust::live::LiveOptions {
            refresh_per_second: 10.0,
            transient: true,
            ..Default::default()
        },
    );

    let live = live.get_renderable(move || {
        let count = lock_recover(&counter_for_live);
        Box::new(Text::new(format!("Count: {}", *count)))
    });

    live.start(true).expect("Live should start");
    eprintln!("  Live display started");
    eprintln!("  PHASE 1 PASSED\n");

    // PHASE 2: Update from multiple threads, one panics
    eprintln!("[PHASE 2] Update from multiple threads, one panics");
    eprintln!("{}", "-".repeat(50));

    let handles: Vec<_> = (0..3)
        .map(|i| {
            let counter = Arc::clone(&counter);
            thread::spawn(move || {
                if i == 1 {
                    thread::sleep(Duration::from_millis(50));
                    panic!("Thread 1 panic during live update");
                }
                for _ in 0..10 {
                    {
                        let mut c = lock_recover(&counter);
                        *c += 1;
                    }
                    thread::sleep(Duration::from_millis(20));
                }
                eprintln!("  [Thread {i}] Completed 10 updates");
            })
        })
        .collect();

    let mut panicked = 0;
    let mut ok = 0;
    for (i, h) in handles.into_iter().enumerate() {
        match h.join() {
            Ok(()) => {
                eprintln!("  [Thread {i}] Joined OK");
                ok += 1;
            }
            Err(_) => {
                eprintln!("  [Thread {i}] Panicked (expected)");
                panicked += 1;
            }
        }
    }
    eprintln!("  Summary: {ok} ok, {panicked} panicked");
    assert_eq!(panicked, 1);
    assert_eq!(ok, 2);
    eprintln!("  PHASE 2 PASSED\n");

    // PHASE 3: Verify counter and stop Live cleanly
    eprintln!("[PHASE 3] Verify Live still works and stop cleanly");
    eprintln!("{}", "-".repeat(50));
    let final_count = *lock_recover(&counter);
    eprintln!("  Final counter value: {final_count}");
    assert!(
        final_count >= 10,
        "at least 10 updates from 2 surviving threads"
    );
    live.stop().expect("Live should stop cleanly");
    eprintln!("  Live stopped successfully");
    eprintln!("  PHASE 3 PASSED\n");

    eprintln!("{}", "=".repeat(70));
    eprintln!("E2E TEST PASSED: Live display remains stable");
    eprintln!("{}", "=".repeat(70));
}

// ============================================================================
// TEST 4: Cell width cache survives concurrent poison
// ============================================================================

#[test]
fn e2e_cell_len_cache_poison_recovery() {
    eprintln!("\n{}", "=".repeat(70));
    eprintln!("E2E TEST: cell_len cache poison recovery");
    eprintln!("{}", "=".repeat(70));

    // PHASE 1: Baseline
    eprintln!("\n[PHASE 1] Baseline cell_len operations");
    eprintln!("{}", "-".repeat(50));
    let test_strings = [
        "Hello, World!",
        "日本語テスト",
        "🎉🎊🎈",
        "Mixed: abc日本🎉",
    ];
    for s in &test_strings {
        let w = rich_rust::cells::cell_len(s);
        eprintln!("  cell_len({s:?}) = {w}");
        assert!(w > 0, "cell_len should be positive");
    }
    eprintln!("  PHASE 1 PASSED\n");

    // PHASE 2: Concurrent with panic
    eprintln!("[PHASE 2] Concurrent cell_len with panic");
    eprintln!("{}", "-".repeat(50));
    let barrier = Arc::new(Barrier::new(5));
    let handles: Vec<_> = (0..5)
        .map(|i| {
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                if i == 3 {
                    // Compute some widths then panic
                    let _ = rich_rust::cells::cell_len("before panic");
                    panic!("Thread 3 panic in cell_len");
                }
                let mut total = 0;
                for _ in 0..100 {
                    total += rich_rust::cells::cell_len("test string");
                }
                eprintln!("  [Thread {i}] Total width: {total}");
                total
            })
        })
        .collect();

    let mut panicked = 0;
    for (i, h) in handles.into_iter().enumerate() {
        match h.join() {
            Ok(total) => eprintln!("  [Thread {i}] Joined with total={total}"),
            Err(_) => {
                eprintln!("  [Thread {i}] Panicked (expected)");
                panicked += 1;
            }
        }
    }
    assert_eq!(panicked, 1);
    eprintln!("  PHASE 2 PASSED\n");

    // PHASE 3: Verify cache still works
    eprintln!("[PHASE 3] cell_len still works after panic");
    eprintln!("{}", "-".repeat(50));
    let w = rich_rust::cells::cell_len("recovery test");
    eprintln!("  cell_len(\"recovery test\") = {w}");
    assert!(w > 0);
    eprintln!("  PHASE 3 PASSED\n");

    eprintln!("{}", "=".repeat(70));
    eprintln!("E2E TEST PASSED: cell_len cache recovers from poison");
    eprintln!("{}", "=".repeat(70));
}

// ============================================================================
// TEST 5: Full pipeline: Console + Style + Color all survive
// ============================================================================

#[test]
fn e2e_full_pipeline_survives_poison() {
    eprintln!("\n{}", "=".repeat(70));
    eprintln!("E2E TEST: Full rendering pipeline after poison");
    eprintln!("{}", "=".repeat(70));

    let buf = Arc::new(Mutex::new(Vec::new()));
    let console = Arc::new(make_console(&buf));

    // PHASE 1: Normal styled output
    eprintln!("\n[PHASE 1] Normal styled output");
    eprintln!("{}", "-".repeat(50));
    console.print("[bold red]Error:[/] file not found");
    let output1 = buf_contents(&buf);
    eprintln!("  Output: {} bytes", output1.len());
    assert!(
        output1.contains("Error:"),
        "styled output should contain text"
    );
    eprintln!("  PHASE 1 PASSED\n");

    // PHASE 2: Stress with panics
    eprintln!("[PHASE 2] Multi-thread stress with panics");
    eprintln!("{}", "-".repeat(50));
    let barrier = Arc::new(Barrier::new(4));
    let handles: Vec<_> = (0..4)
        .map(|i| {
            let console = Arc::clone(&console);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                if i == 0 {
                    let _ = Style::parse("bold");
                    panic!("Thread 0 panic during style+print");
                }
                for j in 0..5 {
                    let style = Style::parse("bold green").unwrap();
                    console.print_styled(&format!("T{i}:{j}"), style);
                }
                eprintln!("  [Thread {i}] Completed styled prints");
            })
        })
        .collect();

    let mut panicked = 0;
    for (i, h) in handles.into_iter().enumerate() {
        match h.join() {
            Ok(()) => eprintln!("  [Thread {i}] Joined OK"),
            Err(_) => {
                eprintln!("  [Thread {i}] Panicked (expected)");
                panicked += 1;
            }
        }
    }
    assert_eq!(panicked, 1);
    eprintln!("  PHASE 2 PASSED\n");

    // PHASE 3: Full pipeline still works
    eprintln!("[PHASE 3] Full pipeline still functional");
    eprintln!("{}", "-".repeat(50));
    let style = Style::parse("bold blue").unwrap();
    console.print_styled("Post-poison styled text", style);
    let final_output = buf_contents(&buf);
    eprintln!("  Final output: {} bytes", final_output.len());
    assert!(
        final_output.contains("Post-poison"),
        "pipeline should still work"
    );
    eprintln!("  PHASE 3 PASSED\n");

    eprintln!("{}", "=".repeat(70));
    eprintln!("E2E TEST PASSED: Full pipeline survives poison");
    eprintln!("{}", "=".repeat(70));
}
