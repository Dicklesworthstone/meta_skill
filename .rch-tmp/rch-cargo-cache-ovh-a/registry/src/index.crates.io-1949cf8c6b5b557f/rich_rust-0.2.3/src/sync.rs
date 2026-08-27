//! # Synchronization Utilities
//!
//! This module provides consistent mutex and `RwLock` handling throughout `rich_rust`.
//!
//! ## Design RFC: Mutex Poison Handling (bd-33rg)
//!
//! ### Background
//!
//! Rust's standard library mutexes become "poisoned" when a thread panics while
//! holding the lock. By default, subsequent `lock()` attempts return `Err(PoisonError)`.
//!
//! ### Problem Statement
//!
//! The `rich_rust` codebase previously used inconsistent patterns:
//!
//! 1. `if let Ok(...) = mutex.lock()` - Silently ignores poison (production code)
//! 2. `.lock().unwrap()` - Panics on poison (test code)
//! 3. `.lock().expect("msg")` - Panics with message (some tests)
//!
//! This inconsistency made it difficult to:
//! - Debug poison-related issues
//! - Understand recovery behavior
//! - Maintain consistent error handling
//!
//! ### Chosen Strategy: Recover with Debug Logging
//!
//! We chose **Option B** from the RFC:
//!
//! - **Production (release builds)**: Silently recover from poison
//! - **Debug builds**: Log a warning when recovering from poison
//!
//! #### Rationale
//!
//! 1. **Caches are safe to recover**: Style/cell/color caches can use stale data
//! 2. **Output buffers are non-critical**: Garbled output is better than crashes
//! 3. **Config is self-healing**: Theme/options are read-mostly
//! 4. **Progress is paramount**: For a terminal output library, continuing
//!    to produce output is more important than perfect consistency
//!
//! ### Usage Guidelines
//!
//! | Scenario | Function | When to Use |
//! |----------|----------|-------------|
//! | Production code | [`lock_recover`] | All mutex access in non-test code |
//! | Need context | [`lock_recover_debug`] | When debugging poison sources |
//! | `RwLock` read | [`read_recover`] | All `RwLock` read access |
//! | `RwLock` write | [`write_recover`] | All `RwLock` write access |
//! | Test code | `.lock().unwrap()` | Tests should fail fast on poison |
//!
//! ### Examples
//!
//! ```rust
//! use std::sync::Mutex;
//! use rich_rust::sync::lock_recover;
//!
//! let data = Mutex::new(vec![1, 2, 3]);
//!
//! // Always succeeds, even if mutex was poisoned by a panicking thread
//! let guard = lock_recover(&data);
//! println!("Data: {:?}", *guard);
//! ```
//!
//! ### Security Considerations
//!
//! Poison recovery means we may access data that was being modified when a
//! thread panicked. For `rich_rust`, this is acceptable because:
//!
//! 1. We don't handle sensitive data (passwords, keys, etc.)
//! 2. The worst case is visual corruption, not data loss
//! 3. Users can restart the program if output is corrupted
//!
//! ### Performance
//!
//! - `lock_recover` adds zero overhead in release builds
//! - Debug logging only triggers on actual poison (rare)
//! - All functions are `#[inline]` for zero-cost abstraction

use std::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Lock a mutex, recovering from poison if necessary.
///
/// # Behavior
///
/// - If the mutex is not poisoned, returns the guard normally
/// - If the mutex is poisoned (a thread panicked while holding it),
///   recovers the data and returns the guard anyway
///
/// # When to Use
///
/// Use this for all mutex access in production code. The function always
/// succeeds, making it impossible to accidentally ignore a poisoned mutex.
///
/// # Example
///
/// ```rust
/// use std::sync::Mutex;
/// use rich_rust::sync::lock_recover;
///
/// let mutex = Mutex::new(42);
/// let guard = lock_recover(&mutex);
/// assert_eq!(*guard, 42);
/// ```
///
/// # Panics
///
/// This function never panics. It always recovers from poison.
#[inline]
pub fn lock_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Lock a mutex with context logging on poison recovery (debug builds only).
///
/// Same as [`lock_recover`] but prints a warning in debug builds to help
/// track down the source of poison. In release builds, this is identical
/// to `lock_recover` with no overhead.
///
/// # Arguments
///
/// * `mutex` - The mutex to lock
/// * `context` - A string describing where this lock is being acquired
///   (e.g., "`Console::print`", "`Style::parse` cache")
///
/// # Example
///
/// ```rust
/// use std::sync::Mutex;
/// use rich_rust::sync::lock_recover_debug;
///
/// let mutex = Mutex::new("hello");
/// let guard = lock_recover_debug(&mutex, "my_function");
/// assert_eq!(*guard, "hello");
/// ```
#[inline]
#[allow(unused_variables)]
pub fn lock_recover_debug<'a, T>(mutex: &'a Mutex<T>, context: &str) -> MutexGuard<'a, T> {
    mutex.lock().unwrap_or_else(|e| {
        #[cfg(debug_assertions)]
        eprintln!("[rich_rust::sync] mutex poison recovered at: {context}");
        e.into_inner()
    })
}

/// Acquire a read lock on an `RwLock`, recovering from poison if necessary.
///
/// # Behavior
///
/// - If the `RwLock` is not poisoned, returns the read guard normally
/// - If poisoned (a thread panicked while holding a write lock),
///   recovers the data and returns the guard anyway
///
/// # Example
///
/// ```rust
/// use std::sync::RwLock;
/// use rich_rust::sync::read_recover;
///
/// let rwlock = RwLock::new(42);
/// let guard = read_recover(&rwlock);
/// assert_eq!(*guard, 42);
/// ```
#[inline]
pub fn read_recover<T>(rwlock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    rwlock
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Acquire a write lock on an `RwLock`, recovering from poison if necessary.
///
/// # Behavior
///
/// - If the `RwLock` is not poisoned, returns the write guard normally
/// - If poisoned, recovers the data and returns the guard anyway
///
/// # Example
///
/// ```rust
/// use std::sync::RwLock;
/// use rich_rust::sync::write_recover;
///
/// let rwlock = RwLock::new(42);
/// let mut guard = write_recover(&rwlock);
/// *guard = 100;
/// ```
#[inline]
pub fn write_recover<T>(rwlock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    rwlock
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::{self, AssertUnwindSafe};
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_lock_recover_normal_operation() {
        println!("[TEST] lock_recover with healthy mutex");
        let mutex = Mutex::new(42);
        let guard = lock_recover(&mutex);
        println!("[TEST] Acquired lock, value = {}", *guard);
        assert_eq!(*guard, 42);
        println!("[TEST] PASS: lock_recover works on healthy mutex");
    }

    #[test]
    fn test_lock_recover_after_poison() {
        println!("[TEST] lock_recover after poisoning mutex");
        let mutex = Mutex::new(42);

        println!("[TEST] Step 1: Poisoning mutex by panicking while holding lock...");
        let _ = panic::catch_unwind(AssertUnwindSafe(|| {
            let _guard = mutex.lock().unwrap();
            println!("[TEST] Inside panic scope, about to panic");
            panic!("intentional panic to poison mutex");
        }));

        println!("[TEST] Step 2: Verifying mutex is actually poisoned...");
        assert!(mutex.lock().is_err(), "Mutex should be poisoned");
        println!("[TEST] Confirmed: mutex.lock() returns Err (poisoned)");

        println!("[TEST] Step 3: Testing recovery...");
        let guard = lock_recover(&mutex);
        println!("[TEST] Recovery successful, value = {}", *guard);
        assert_eq!(*guard, 42);
        println!("[TEST] PASS: lock_recover recovers from poison");
    }

    #[test]
    fn test_lock_recover_debug_context() {
        println!("[TEST] lock_recover_debug with context");
        let mutex = Mutex::new("hello");
        let guard = lock_recover_debug(&mutex, "test_debug_context");
        println!("[TEST] Value: {}", *guard);
        assert_eq!(*guard, "hello");
        println!("[TEST] PASS: lock_recover_debug works");
    }

    #[test]
    fn test_lock_recover_debug_after_poison() {
        println!("[TEST] lock_recover_debug after poison (should log in debug)");
        let mutex = Mutex::new(99);

        let _ = panic::catch_unwind(AssertUnwindSafe(|| {
            let _guard = mutex.lock().unwrap();
            panic!("intentional panic");
        }));

        // In debug builds, this should print a warning
        let guard = lock_recover_debug(&mutex, "test_poison_debug");
        assert_eq!(*guard, 99);
        println!("[TEST] PASS: lock_recover_debug recovers with logging");
    }

    #[test]
    fn test_read_recover_normal() {
        println!("[TEST] read_recover with healthy RwLock");
        let rwlock = RwLock::new(42);
        let guard = read_recover(&rwlock);
        println!("[TEST] Read value = {}", *guard);
        assert_eq!(*guard, 42);
        println!("[TEST] PASS: read_recover works on healthy RwLock");
    }

    #[test]
    fn test_read_recover_after_write_poison() {
        println!("[TEST] read_recover after write poison");
        let rwlock = RwLock::new(42);

        println!("[TEST] Poisoning via write lock...");
        let _ = panic::catch_unwind(AssertUnwindSafe(|| {
            let _guard = rwlock.write().unwrap();
            panic!("intentional panic during write");
        }));

        println!("[TEST] Attempting read recovery...");
        let guard = read_recover(&rwlock);
        println!("[TEST] Read recovered, value = {}", *guard);
        assert_eq!(*guard, 42);
        println!("[TEST] PASS: read_recover works after write poison");
    }

    #[test]
    fn test_write_recover_normal() {
        println!("[TEST] write_recover with healthy RwLock");
        let rwlock = RwLock::new(42);
        {
            let mut guard = write_recover(&rwlock);
            *guard = 100;
            println!("[TEST] Modified value to 100");
        }
        let guard = read_recover(&rwlock);
        println!("[TEST] Verified new value = {}", *guard);
        assert_eq!(*guard, 100);
        println!("[TEST] PASS: write_recover works on healthy RwLock");
    }

    #[test]
    fn test_write_recover_after_read_poison() {
        println!("[TEST] write_recover after read poison (edge case)");
        let rwlock = RwLock::new(42);

        println!("[TEST] Poisoning via read lock...");
        let _ = panic::catch_unwind(AssertUnwindSafe(|| {
            let _guard = rwlock.read().unwrap();
            panic!("intentional panic during read");
        }));

        println!("[TEST] Attempting write recovery...");
        let mut guard = write_recover(&rwlock);
        *guard = 200;
        println!("[TEST] Write recovered, set value to 200");
        drop(guard);

        let read_guard = read_recover(&rwlock);
        println!("[TEST] Verified new value = {}", *read_guard);
        assert_eq!(*read_guard, 200);
        println!("[TEST] PASS: write_recover works after read poison");
    }

    #[test]
    fn test_multiple_recoveries_same_mutex() {
        println!("[TEST] Multiple sequential recoveries on same mutex");
        let mutex = Mutex::new(0);

        // Poison the mutex multiple times and verify we can always recover
        for i in 1..=3 {
            println!("[TEST] Iteration {i}: poisoning mutex...");

            // Set value BEFORE poisoning to ensure it's visible
            {
                let mut guard = lock_recover(&mutex);
                *guard = i;
                println!("[TEST] Iteration {i}: set value to {}", *guard);
            }

            // Now poison by panicking while holding lock
            let _ = panic::catch_unwind(AssertUnwindSafe(|| {
                let _guard = mutex.lock().unwrap();
                println!("[TEST] Iteration {i}: about to panic");
                panic!("intentional panic #{i}");
            }));

            println!("[TEST] Iteration {i}: recovering...");
            let guard = lock_recover(&mutex);
            println!("[TEST] Iteration {i}: recovered value = {}", *guard);
            // Value should be i since we set it before poisoning
            assert_eq!(*guard, i);
        }
        println!("[TEST] PASS: Multiple recoveries work correctly");
    }

    #[test]
    fn test_concurrent_access_after_poison() {
        println!("[TEST] Concurrent access after poison");

        let mutex = Arc::new(Mutex::new(0));

        // Poison the mutex
        {
            let m = Arc::clone(&mutex);
            let _ = panic::catch_unwind(AssertUnwindSafe(move || {
                let _guard = m.lock().unwrap();
                panic!("poison it");
            }));
        }

        // Spawn multiple threads that all recover
        let handles: Vec<_> = (0..4)
            .map(|i| {
                let m = Arc::clone(&mutex);
                thread::spawn(move || {
                    let mut guard = lock_recover(&m);
                    *guard += 1;
                    println!("[Thread {i}] Incremented to {}", *guard);
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        let final_val = *lock_recover(&mutex);
        println!("[TEST] Final value after 4 increments: {final_val}");
        assert_eq!(final_val, 4);
        println!("[TEST] PASS: Concurrent recovery works");
    }
}
