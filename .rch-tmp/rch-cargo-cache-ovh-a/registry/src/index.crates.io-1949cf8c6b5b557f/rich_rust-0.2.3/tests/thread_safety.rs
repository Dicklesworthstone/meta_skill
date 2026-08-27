//! Thread safety tests for rich_rust.
//!
//! This module verifies:
//! 1. All public types are Send + Sync (compile-time verification)
//! 2. Global caches work correctly under concurrent access
//! 3. Parallel rendering operations are safe

use rich_rust::prelude::*;
use std::thread;

// ============================================================================
// COMPILE-TIME SEND + SYNC VERIFICATION
// ============================================================================

/// Helper function to verify a type is Send + Sync at compile time.
fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn test_color_types_are_send_sync() {
    assert_send_sync::<Color>();
    assert_send_sync::<ColorSystem>();
    assert_send_sync::<ColorType>();
    assert_send_sync::<ColorTriplet>();
}

#[test]
fn test_style_types_are_send_sync() {
    assert_send_sync::<Style>();
    assert_send_sync::<Attributes>();
}

#[test]
fn test_segment_is_send_sync() {
    assert_send_sync::<Segment>();
}

#[test]
fn test_text_types_are_send_sync() {
    assert_send_sync::<Text>();
    assert_send_sync::<Span>();
    assert_send_sync::<JustifyMethod>();
    assert_send_sync::<OverflowMethod>();
}

#[test]
fn test_console_types_are_send_sync() {
    // Console is Send + Sync; output stream is guarded by a Mutex.
    assert_send_sync::<Console>();
    assert_send_sync::<ConsoleOptions>();
}

#[test]
fn test_measurement_is_send_sync() {
    assert_send_sync::<Measurement>();
}

#[test]
fn test_box_chars_is_send_sync() {
    assert_send_sync::<BoxChars>();
}

#[test]
fn test_renderable_types_are_send_sync() {
    assert_send_sync::<Align>();
    assert_send_sync::<AlignMethod>();
    assert_send_sync::<VerticalAlignMethod>();
    assert_send_sync::<Columns>();
    assert_send_sync::<Rule>();
    assert_send_sync::<Panel>();
    assert_send_sync::<Table>();
    assert_send_sync::<Layout>();
    assert_send_sync::<Column>();
    assert_send_sync::<Row>();
    assert_send_sync::<Cell>();
    assert_send_sync::<PaddingDimensions>();
    assert_send_sync::<VerticalAlign>();
    assert_send_sync::<ProgressBar>();
    assert_send_sync::<BarStyle>();
    assert_send_sync::<Spinner>();
    assert_send_sync::<Tree>();
    assert_send_sync::<TreeNode>();
    assert_send_sync::<TreeGuides>();
}

// ============================================================================
// CONCURRENT CACHE ACCESS TESTS
// ============================================================================

#[test]
fn test_concurrent_color_parsing() {
    // Spawn multiple threads that all parse colors concurrently
    let handles: Vec<_> = (0..8)
        .map(|i| {
            thread::spawn(move || {
                for j in 0..500 {
                    // Parse various color formats
                    let _ = Color::parse("red").unwrap();
                    let _ = Color::parse("bright_blue").unwrap();
                    let _ = Color::parse("#ff0000").unwrap();
                    let _ = Color::parse("#abc").unwrap();
                    let _ = Color::parse(&format!("color({})", (i * 50 + j) % 256)).unwrap();
                    let _ = Color::parse("rgb(100, 150, 200)").unwrap();
                    let _ = Color::parse("default").unwrap();
                }
            })
        })
        .collect();

    // All threads should complete without panic
    for handle in handles {
        handle
            .join()
            .expect("Thread panicked during concurrent color parsing");
    }
}

#[test]
fn test_concurrent_style_parsing() {
    // Spawn multiple threads that all parse styles concurrently
    let handles: Vec<_> = (0..8)
        .map(|_| {
            thread::spawn(|| {
                for _ in 0..500 {
                    // Parse various style formats
                    let _ = Style::parse("bold").unwrap();
                    let _ = Style::parse("italic red").unwrap();
                    let _ = Style::parse("bold underline green on white").unwrap();
                    let _ = Style::parse("dim cyan").unwrap();
                    let _ = Style::parse("none").unwrap();
                    let _ = Style::parse("reverse").unwrap();
                }
            })
        })
        .collect();

    // All threads should complete without panic
    for handle in handles {
        handle
            .join()
            .expect("Thread panicked during concurrent style parsing");
    }
}

#[test]
fn test_concurrent_cell_len_calculation() {
    use rich_rust::cells::cell_len;

    // Spawn multiple threads that all calculate cell lengths concurrently
    let handles: Vec<_> = (0..8)
        .map(|i| {
            thread::spawn(move || {
                for _ in 0..500 {
                    // Calculate cell lengths for various strings
                    let _ = cell_len("Hello, World!");
                    let _ = cell_len("Bold text");
                    let _ = cell_len(&format!("Thread {} testing", i));
                    // Wide characters (CJK)
                    let _ = cell_len("\u{4e2d}\u{6587}"); // Chinese characters
                    let _ = cell_len("\u{65e5}\u{672c}\u{8a9e}"); // Japanese
                    // Emoji
                    let _ = cell_len("\u{1f600}\u{1f601}\u{1f602}");
                    // Empty
                    let _ = cell_len("");
                }
            })
        })
        .collect();

    // All threads should complete without panic
    for handle in handles {
        handle
            .join()
            .expect("Thread panicked during concurrent cell_len calculation");
    }
}

// ============================================================================
// CONCURRENT RENDERING TESTS
// ============================================================================

#[test]
fn test_concurrent_text_rendering() {
    let handles: Vec<_> = (0..4)
        .map(|i| {
            thread::spawn(move || {
                for _ in 0..100 {
                    let text = Text::from(format!("Thread {} [bold]testing[/] rendering", i));
                    let _segments = text.render("\n");

                    // Also test with explicit styles
                    let text2 = Text::styled("Styled text", Style::new().bold());
                    let _segments2 = text2.render("\n");
                }
            })
        })
        .collect();

    for handle in handles {
        handle
            .join()
            .expect("Thread panicked during concurrent text rendering");
    }
}

#[test]
fn test_concurrent_table_rendering() {
    let handles: Vec<_> = (0..4)
        .map(|i| {
            thread::spawn(move || {
                for j in 0..50 {
                    let title = format!("Thread {} Table {}", i, j);
                    let row_val = format!("Row {}", j);
                    let mut table = Table::new()
                        .title(title.as_str())
                        .with_column(Column::new("Name"))
                        .with_column(Column::new("Value"));

                    table.add_row_cells([row_val.as_str(), "Data"]);
                    table.add_row_cells(["Test", "123"]);

                    let _segments = table.render(80);
                }
            })
        })
        .collect();

    for handle in handles {
        handle
            .join()
            .expect("Thread panicked during concurrent table rendering");
    }
}

#[test]
fn test_concurrent_panel_rendering() {
    let handles: Vec<_> = (0..4)
        .map(|i| {
            thread::spawn(move || {
                for j in 0..100 {
                    let content = format!("Thread {} Panel {}", i, j);
                    let panel = Panel::from_text(content.as_str())
                        .title("Test Panel")
                        .width(40);

                    let _segments = panel.render(80);
                }
            })
        })
        .collect();

    for handle in handles {
        handle
            .join()
            .expect("Thread panicked during concurrent panel rendering");
    }
}

#[test]
fn test_concurrent_progress_bar_rendering() {
    let handles: Vec<_> = (0..4)
        .map(|_| {
            thread::spawn(|| {
                for completed in 0..=100 {
                    let mut bar = ProgressBar::new().width(40);
                    bar.set_progress(f64::from(completed) / 100.0);

                    let _segments = bar.render(80);
                }
            })
        })
        .collect();

    for handle in handles {
        handle
            .join()
            .expect("Thread panicked during concurrent progress bar rendering");
    }
}

#[test]
fn test_concurrent_rule_rendering() {
    let handles: Vec<_> = (0..4)
        .map(|i| {
            thread::spawn(move || {
                for j in 0..100 {
                    let title = format!("Thread {} Rule {}", i, j);
                    let rule = Rule::with_title(title.as_str());
                    let _segments = rule.render(80);

                    let rule_plain = Rule::new();
                    let _segments2 = rule_plain.render(80);
                }
            })
        })
        .collect();

    for handle in handles {
        handle
            .join()
            .expect("Thread panicked during concurrent rule rendering");
    }
}

#[test]
fn test_concurrent_tree_rendering() {
    let handles: Vec<_> = (0..4)
        .map(|i| {
            thread::spawn(move || {
                for j in 0..50 {
                    let root = TreeNode::new(format!("Root {} {}", i, j))
                        .child(TreeNode::new("Child 1"))
                        .child(TreeNode::new("Child 2"));

                    let tree = Tree::new(root);
                    let _segments = tree.render();
                }
            })
        })
        .collect();

    for handle in handles {
        handle
            .join()
            .expect("Thread panicked during concurrent tree rendering");
    }
}

// ============================================================================
// MIXED CONCURRENT OPERATIONS
// ============================================================================

#[test]
fn test_mixed_concurrent_operations() {
    // This test exercises multiple subsystems concurrently to detect any
    // cross-subsystem thread safety issues

    let handles: Vec<_> = (0..12)
        .map(|i| {
            thread::spawn(move || {
                match i % 6 {
                    0 => {
                        // Color parsing
                        for _ in 0..200 {
                            let _ = Color::parse("red").unwrap();
                            let _ = Color::parse("#ff0000").unwrap();
                        }
                    }
                    1 => {
                        // Style parsing
                        for _ in 0..200 {
                            let _ = Style::parse("bold red").unwrap();
                        }
                    }
                    2 => {
                        // Text rendering
                        for _ in 0..100 {
                            let text = Text::from("[bold]Hello[/]");
                            let _ = text.render("\n");
                        }
                    }
                    3 => {
                        // Table rendering
                        for _ in 0..50 {
                            let mut table = Table::new().with_column(Column::new("A"));
                            table.add_row_cells(["value"]);
                            let _ = table.render(80);
                        }
                    }
                    4 => {
                        // Panel rendering
                        for _ in 0..100 {
                            let panel = Panel::from_text("content");
                            let _ = panel.render(80);
                        }
                    }
                    5 => {
                        // Cell length calculation
                        use rich_rust::cells::cell_len;
                        for _ in 0..200 {
                            let _ = cell_len("test string");
                        }
                    }
                    _ => unreachable!(),
                }
            })
        })
        .collect();

    for handle in handles {
        handle
            .join()
            .expect("Thread panicked during mixed concurrent operations");
    }
}

// ============================================================================
// CACHE CONSISTENCY TESTS
// ============================================================================

#[test]
fn test_color_cache_consistency() {
    // Verify that concurrent cache access returns consistent results
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let failed = Arc::new(AtomicBool::new(false));

    let handles: Vec<_> = (0..8)
        .map(|_| {
            let failed = Arc::clone(&failed);
            thread::spawn(move || {
                for _ in 0..1000 {
                    let color1 = Color::parse("bright_red").unwrap();
                    let color2 = Color::parse("bright_red").unwrap();

                    // Both should return the same color number
                    if color1.number != color2.number {
                        failed.store(true, Ordering::SeqCst);
                        return;
                    }
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(
        !failed.load(Ordering::SeqCst),
        "Cache returned inconsistent results"
    );
}

#[test]
fn test_style_cache_consistency() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let failed = Arc::new(AtomicBool::new(false));

    let handles: Vec<_> = (0..8)
        .map(|_| {
            let failed = Arc::clone(&failed);
            thread::spawn(move || {
                for _ in 0..1000 {
                    let style1 = Style::parse("bold italic red").unwrap();
                    let style2 = Style::parse("bold italic red").unwrap();

                    // Both should return equivalent styles
                    if style1 != style2 {
                        failed.store(true, Ordering::SeqCst);
                        return;
                    }
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(
        !failed.load(Ordering::SeqCst),
        "Style cache returned inconsistent results"
    );
}

// ============================================================================
// MUTEX POISON RECOVERY TESTS (bd-34us)
// ============================================================================

/// Test that library operations continue working after a thread panics
/// during style parsing (which uses global style/color caches).
#[test]
fn test_style_operations_survive_thread_panic() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    // Pre-warm caches
    let _ = Style::parse("bold red").unwrap();
    let _ = Color::parse("blue").unwrap();

    let panic_done = Arc::new(AtomicBool::new(false));
    let panic_done_clone = Arc::clone(&panic_done);

    // Spawn a thread that panics while doing style operations
    let panic_handle = thread::spawn(move || {
        let _ = Style::parse("bold").unwrap();
        panic_done_clone.store(true, Ordering::SeqCst);
        panic!("intentional panic during style operations");
    });

    // Wait for the panic thread to complete (it will return Err)
    let _ = panic_handle.join();
    assert!(panic_done.load(Ordering::SeqCst), "panic thread ran");

    // Verify the library still works after the panic
    let style = Style::parse("bold italic red on white").unwrap();
    // Style was parsed successfully - verify it's not the null/default style
    assert_ne!(style, Style::default());

    let color = Color::parse("#ff0000").unwrap();
    // Color parsed successfully
    let _ = color;

    // More operations to verify caches are functional
    for _ in 0..100 {
        let _ = Style::parse("dim cyan underline").unwrap();
        let _ = Color::parse("bright_green").unwrap();
    }
}

/// Test that cell_len continues working after a thread panics during
/// cell width calculations.
#[test]
fn test_cell_len_survives_thread_panic() {
    use rich_rust::cells::cell_len;

    // Pre-warm cache
    let _ = cell_len("Hello, World!");

    // Spawn a thread that panics while doing cell_len
    let panic_handle = thread::spawn(|| {
        let _ = cell_len("test");
        panic!("intentional panic during cell_len");
    });

    let _ = panic_handle.join();

    // Verify cell_len still works
    assert_eq!(cell_len("Hello"), 5);
    assert_eq!(cell_len(""), 0);
    assert_eq!(cell_len("中文"), 4); // CJK characters are width 2

    // Batch operations to stress-test post-panic cache
    for i in 0..100 {
        let s = format!("test string number {i}");
        let len = cell_len(&s);
        assert!(len > 0);
    }
}

/// Test that rendering operations survive a thread panic during rendering.
#[test]
fn test_rendering_survives_thread_panic() {
    // Spawn a thread that panics mid-render
    let panic_handle = thread::spawn(|| {
        let text = Text::from("[bold]Hello[/]");
        let _segments = text.render("\n");
        panic!("intentional panic after rendering");
    });

    let _ = panic_handle.join();

    // All rendering should still work
    let text = Text::from("[bold red]After panic[/]");
    let segments = text.render("\n");
    assert!(!segments.is_empty());

    let mut table = Table::new().with_column(Column::new("Name"));
    table.add_row_cells(["Post-panic data"]);
    let segments = table.render(80);
    assert!(!segments.is_empty());

    let panel = Panel::from_text("Recovered content");
    let segments = panel.render(60);
    assert!(!segments.is_empty());
}

/// Test concurrent access where some threads panic and others continue.
/// This is the most realistic scenario: in a multi-threaded application,
/// one task crashes but the rest should keep running.
#[test]
fn test_concurrent_access_with_panicking_threads() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let success_count = Arc::new(AtomicUsize::new(0));

    let handles: Vec<_> = (0..12)
        .map(|i| {
            let success_count = Arc::clone(&success_count);
            thread::spawn(move || {
                // Threads 0 and 1 will panic; the rest should succeed
                if i < 2 {
                    let _ = Style::parse("bold").unwrap();
                    panic!("intentional panic in thread {i}");
                }

                // Remaining threads do normal work
                for _ in 0..100 {
                    let _ = Style::parse("bold red").unwrap();
                    let _ = Color::parse("#00ff00").unwrap();

                    let text = Text::from("Concurrent rendering");
                    let _ = text.render("\n");
                }

                success_count.fetch_add(1, Ordering::SeqCst);
            })
        })
        .collect();

    let mut panics = 0;
    for handle in handles {
        match handle.join() {
            Ok(()) => {}
            Err(_) => panics += 1,
        }
    }

    // Exactly 2 threads should have panicked
    assert_eq!(panics, 2);
    // The remaining 10 threads should have completed successfully
    assert_eq!(success_count.load(Ordering::SeqCst), 10);
}

/// Test that Status spinner (which uses Arc<Mutex<String>>) handles
/// poison recovery via the sync module.
#[test]
fn test_status_mutex_poison_recovery() {
    use rich_rust::sync::lock_recover;
    use std::sync::{Arc, Mutex};

    // Simulate what Status does internally: Arc<Mutex<String>>
    let message = Arc::new(Mutex::new("Working...".to_string()));

    // Poison the mutex
    let msg_clone = Arc::clone(&message);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = msg_clone.lock().unwrap();
        panic!("intentional panic to poison Status-like mutex");
    }));

    assert!(message.is_poisoned());

    // Recover using the sync helper (as Status does)
    let guard = lock_recover(&message);
    assert_eq!(*guard, "Working...");
    drop(guard);

    // Update via recover (as Status::update does)
    *lock_recover(&message) = "Still working!".to_string();
    assert_eq!(*lock_recover(&message), "Still working!");
}
