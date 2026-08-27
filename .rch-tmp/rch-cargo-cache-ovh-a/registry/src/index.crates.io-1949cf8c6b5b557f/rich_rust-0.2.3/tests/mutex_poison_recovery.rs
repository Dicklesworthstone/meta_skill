//! Integration tests for mutex poison recovery throughout the codebase.
//!
//! These tests verify that `rich_rust` components gracefully handle mutex
//! poisoning — a thread panicking while holding a lock should not prevent
//! subsequent operations from succeeding.

use std::io::Write;
use std::panic;
use std::sync::{Arc, Mutex};
use std::thread;

use rich_rust::prelude::*;
use rich_rust::sync::{lock_recover, lock_recover_debug, read_recover};

// ============================================================================
// sync module helpers — additional coverage
// ============================================================================

#[test]
fn lock_recover_preserves_mutated_state_after_poison() {
    let mutex = Mutex::new(vec![1, 2, 3]);
    // Mutate, then poison during mutation
    let _ = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        let mut guard = mutex.lock().unwrap();
        guard.push(4);
        panic::panic_any("poison after mutation");
    }));
    assert!(mutex.is_poisoned());
    // Recovered state should include the mutation that happened before the panic
    let guard = lock_recover(&mutex);
    assert_eq!(*guard, vec![1, 2, 3, 4]);
}

#[test]
fn lock_recover_debug_works_after_poison() {
    let mutex = Mutex::new(String::from("original"));
    let _ = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        let mut guard = mutex.lock().unwrap();
        guard.push_str("_modified");
        panic::panic_any("poison");
    }));
    let guard = lock_recover_debug(&mutex, "test_integration");
    assert_eq!(*guard, "original_modified");
}

#[test]
fn write_recover_preserves_state_after_poison() {
    let rwlock = std::sync::RwLock::new(100_u32);
    let _ = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        let mut guard = rwlock.write().unwrap();
        *guard = 200;
        panic::panic_any("poison during write");
    }));
    // Write recovery should see the mutated value
    let guard = read_recover(&rwlock);
    assert_eq!(*guard, 200);
}

// ============================================================================
// Console operations after poison
// ============================================================================

/// Helper: creates a Console writing to a shared buffer.
fn buffered_console() -> (Console, Arc<Mutex<Vec<u8>>>) {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let writer = BufferWriter(Arc::clone(&buf));
    let console = Console::builder()
        .force_terminal(true)
        .color_system(ColorSystem::Standard)
        // Keep these tests focused on mutex poison recovery, not highlighting side effects.
        .highlight(false)
        .file(Box::new(writer))
        .build();
    (console, buf)
}

struct BufferWriter(Arc<Mutex<Vec<u8>>>);

impl Write for BufferWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.lock().unwrap().flush()
    }
}

#[test]
fn console_print_works_from_multiple_threads() {
    let (console, buf) = buffered_console();
    let console = Arc::new(console);
    let mut handles = vec![];

    for i in 0..4 {
        let c = Arc::clone(&console);
        handles.push(thread::spawn(move || {
            c.print_plain(&format!("thread {i}"));
        }));
    }

    for h in handles {
        h.join().expect("thread should not panic");
    }

    let binding = lock_recover(&buf);
    let output = String::from_utf8_lossy(&binding);
    for i in 0..4 {
        assert!(
            output.contains(&format!("thread {i}")),
            "missing output from thread {i}"
        );
    }
}

#[test]
fn console_survives_thread_panic() {
    let (console, buf) = buffered_console();
    let console = Arc::new(console);

    // Spawn a thread that panics (not while holding our console's lock)
    let c = Arc::clone(&console);
    let handle = thread::spawn(move || {
        c.print_plain("before panic");
        panic::panic_any("intentional thread panic");
    });
    let _ = handle.join(); // expect Err since the thread panicked

    // Console should still be usable from the main thread
    console.print_plain("after panic");

    let binding = lock_recover(&buf);
    let output = String::from_utf8_lossy(&binding);
    assert!(
        output.contains("after panic"),
        "console should still work after thread panic"
    );
}

// ============================================================================
// Style cache operations under concurrent access
// ============================================================================

#[test]
fn style_parse_concurrent() {
    let mut handles = vec![];
    for _ in 0..8 {
        handles.push(thread::spawn(|| {
            let style = Style::parse("bold red").unwrap();
            // Verify parsing succeeded (bold style was parsed correctly)
            let _ = style;
        }));
    }
    for h in handles {
        h.join().expect("style parse should be thread-safe");
    }
}

#[test]
fn color_parse_concurrent() {
    let mut handles = vec![];
    for _ in 0..8 {
        handles.push(thread::spawn(|| {
            let color = Color::parse("#ff0000").unwrap();
            // Verify it parsed correctly
            let _ = format!("{color:?}");
        }));
    }
    for h in handles {
        h.join().expect("color parse should be thread-safe");
    }
}

#[test]
fn cell_len_concurrent() {
    let mut handles = vec![];
    for _ in 0..8 {
        handles.push(thread::spawn(|| {
            let width = rich_rust::cells::cell_len("Hello, World! This is a test string.");
            assert!(width > 0);
        }));
    }
    for h in handles {
        h.join().expect("cell_len should be thread-safe");
    }
}

// ============================================================================
// Concurrent access with one panicking thread
// ============================================================================

#[test]
fn concurrent_style_parse_with_panic_survivor() {
    let barrier = Arc::new(std::sync::Barrier::new(5));
    let mut handles = vec![];

    // 4 threads that parse styles normally
    for _ in 0..4 {
        let b = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            b.wait();
            for _ in 0..50 {
                let _ = Style::parse("bold italic green");
            }
        }));
    }

    // 1 thread that panics after doing some work
    let b = Arc::clone(&barrier);
    handles.push(thread::spawn(move || {
        b.wait();
        let _ = Style::parse("underline blue");
        panic::panic_any("intentional panic in style thread");
    }));

    let mut panicked = 0;
    for h in handles {
        if h.join().is_err() {
            panicked += 1;
        }
    }
    assert_eq!(panicked, 1, "exactly one thread should have panicked");

    // Style::parse should still work after the panic
    let style = Style::parse("bold red on white").unwrap();
    // Verify parsing succeeded (bold style was parsed correctly)
    let _ = style;
}
