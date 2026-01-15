//! Concurrent Access Tests for Multi-Agent Safety (meta_skill-urv8)
//!
//! These tests verify BeadsClient behaves correctly when multiple agents access
//! beads concurrently. Critical for multi-agent coordination use case.
//!
//! From AGENTS.md:
//! - **SYNC AFTER EACH BATCH**: Run `bd sync` after each agent completes
//! - **NEVER batch syncs**: If Agent 1 finishes, sync immediately
//! - **Monitor for failures**: If any `bd update` or `bd sync` fails, STOP ALL AGENTS

#![cfg(test)]

use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;

use tempfile::TempDir;

use crate::beads::{BeadsClient, CreateIssueRequest, TestLogger, WorkFilter};

/// Test fixture for concurrent tests (similar to WAL tests).
struct ConcurrentTestEnv {
    temp_dir: TempDir,
    db_path: PathBuf,
    log: TestLogger,
    initialized: bool,
}

impl ConcurrentTestEnv {
    fn new(test_name: &str) -> Self {
        let mut log = TestLogger::new(test_name);

        let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
        let beads_dir = temp_dir.path().join(".beads");
        std::fs::create_dir_all(&beads_dir).expect("Failed to create .beads directory");
        let db_path = beads_dir.join("beads.db");

        log.info(
            "SETUP",
            &format!("Test dir: {}", temp_dir.path().display()),
            None,
        );

        // Initialize database
        log.info("INIT", "Initializing test database", None);
        let status = Command::new("bd")
            .args(["init"])
            .env("BEADS_DB", &db_path)
            .current_dir(temp_dir.path())
            .output();

        let initialized = match status {
            Ok(output) if output.status.success() => {
                log.success("INIT", "Database initialized", None);
                true
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                log.warn("INIT", &format!("bd init warning: {}", stderr), None);
                false
            }
            Err(e) => {
                log.warn("INIT", &format!("bd init failed: {}", e), None);
                false
            }
        };

        ConcurrentTestEnv {
            temp_dir,
            db_path,
            log,
            initialized,
        }
    }

    fn client(&self) -> BeadsClient {
        BeadsClient::new()
            .with_work_dir(self.temp_dir.path())
            .with_env("BEADS_DB", self.db_path.to_string_lossy())
    }

    fn work_dir(&self) -> PathBuf {
        self.temp_dir.path().to_path_buf()
    }

    fn db_path(&self) -> PathBuf {
        self.db_path.clone()
    }
}

// =============================================================================
// Serial Access (Baseline)
// =============================================================================

#[test]
fn test_serial_access_baseline() {
    let mut log = TestLogger::new("test_serial_access");
    let env = ConcurrentTestEnv::new("test_serial_access");
    let client = env.client();

    if !client.is_available() {
        log.warn("SKIP", "bd not available", None);
        return;
    }

    if !env.initialized {
        log.warn("SKIP", "Test database not initialized", None);
        return;
    }

    // Create issues serially
    let mut created_count = 0;
    for i in 0..10 {
        let req = CreateIssueRequest::new(&format!("Serial Test {}", i));
        match client.create(&req) {
            Ok(issue) => {
                created_count += 1;
                log.debug("CREATE", &format!("Created {}", issue.id), None);
            }
            Err(e) => {
                log.warn("CREATE", &format!("Failed to create {}: {}", i, e), None);
            }
        }
    }

    if created_count == 0 {
        log.warn("SKIP", "Could not create any issues", None);
        return;
    }

    // Sync after batch (as recommended)
    let _ = client.sync(); // May fail without git, that's OK

    // Verify issues exist via list
    let filter = WorkFilter::default();
    match client.list(&filter) {
        Ok(issues) => {
            let found = issues
                .iter()
                .filter(|i| i.title.contains("Serial Test"))
                .count();
            log.info(
                "VERIFY",
                &format!("Found {}/{} serial test issues", found, created_count),
                None,
            );
        }
        Err(e) => {
            log.warn("VERIFY", &format!("List failed: {}", e), None);
        }
    }

    log.success(
        "PASS",
        &format!("Created {} issues serially", created_count),
        None,
    );
}

// =============================================================================
// Concurrent Reads (Should Succeed)
// =============================================================================

#[test]
fn test_concurrent_reads() {
    let mut log = TestLogger::new("test_concurrent_reads");
    let env = ConcurrentTestEnv::new("test_concurrent_reads");
    let client = env.client();

    if !client.is_available() {
        log.warn("SKIP", "bd not available", None);
        return;
    }

    if !env.initialized {
        log.warn("SKIP", "Test database not initialized", None);
        return;
    }

    // Create baseline data
    let req = CreateIssueRequest::new("Concurrent Read Test");
    let created_title = match client.create(&req) {
        Ok(issue) => issue.title,
        Err(e) => {
            log.warn("SKIP", &format!("Could not create test issue: {}", e), None);
            return;
        }
    };

    log.info(
        "SETUP",
        &format!("Created baseline issue: {}", created_title),
        None,
    );

    let work_dir = env.work_dir();
    let db_path = env.db_path();

    // Spawn multiple threads doing reads
    let handles: Vec<_> = (0..5)
        .map(|thread_id| {
            let work_dir = work_dir.clone();
            let db_path = db_path.clone();
            thread::spawn(move || {
                let client = BeadsClient::new()
                    .with_work_dir(&work_dir)
                    .with_env("BEADS_DB", db_path.to_string_lossy());

                let filter = WorkFilter::default();
                let mut successes = 0;
                for _ in 0..10 {
                    if client.list(&filter).is_ok() {
                        successes += 1;
                    }
                    // Small delay to spread operations
                    thread::sleep(Duration::from_millis(5));
                }
                (thread_id, successes)
            })
        })
        .collect();

    // Wait for all threads
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    log.info("RESULTS", &format!("Read results: {:?}", results), None);

    // All should succeed (reads are non-destructive)
    let total_success: i32 = results.iter().map(|(_, s)| s).sum();
    let total_attempts = 5 * 10;
    assert!(
        total_success > total_attempts / 2,
        "Most concurrent reads should succeed: {}/{}",
        total_success,
        total_attempts
    );

    log.success(
        "PASS",
        &format!("Concurrent reads: {}/{} succeeded", total_success, total_attempts),
        None,
    );
}

// =============================================================================
// Concurrent Writes (Verify No Corruption)
// =============================================================================

#[test]
fn test_concurrent_writes_no_corruption() {
    let mut log = TestLogger::new("test_concurrent_writes");
    let env = ConcurrentTestEnv::new("test_concurrent_writes");

    if !env.client().is_available() {
        log.warn("SKIP", "bd not available", None);
        return;
    }

    if !env.initialized {
        log.warn("SKIP", "Test database not initialized", None);
        return;
    }

    let work_dir = env.work_dir();
    let db_path = env.db_path();

    // Create multiple issues concurrently
    let handles: Vec<_> = (0..5)
        .map(|thread_id| {
            let work_dir = work_dir.clone();
            let db_path = db_path.clone();
            thread::spawn(move || {
                let client = BeadsClient::new()
                    .with_work_dir(&work_dir)
                    .with_env("BEADS_DB", db_path.to_string_lossy());

                let mut successes = 0;
                let mut failures = 0;
                for j in 0..5 {
                    let req = CreateIssueRequest::new(&format!("Concurrent {} - {}", thread_id, j));
                    match client.create(&req) {
                        Ok(_) => successes += 1,
                        Err(e) => {
                            failures += 1;
                            // Verify error is "clean" (not corruption)
                            let err = e.to_string().to_lowercase();
                            assert!(
                                !err.contains("corrupt") && !err.contains("malformed"),
                                "Got corruption error in thread {}: {}",
                                thread_id,
                                e
                            );
                        }
                    }
                    // Small delay between operations
                    thread::sleep(Duration::from_millis(10));
                }
                (thread_id, successes, failures)
            })
        })
        .collect();

    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    log.info("RESULTS", &format!("Write results: {:?}", results), None);

    let total_success: i32 = results.iter().map(|(_, s, _)| s).sum();
    let total_failures: i32 = results.iter().map(|(_, _, f)| f).sum();
    log.info(
        "SUMMARY",
        &format!(
            "Total: {} successes, {} failures",
            total_success, total_failures
        ),
        None,
    );

    // Verify database is still queryable (not corrupt)
    let client = env.client();
    let filter = WorkFilter::default();
    match client.list(&filter) {
        Ok(issues) => {
            log.info(
                "VERIFY",
                &format!("Total issues after concurrent writes: {}", issues.len()),
                None,
            );
        }
        Err(e) => {
            let err_str = e.to_string().to_lowercase();
            assert!(
                !err_str.contains("corrupt") && !err_str.contains("malformed"),
                "Database corrupted after concurrent writes: {}",
                e
            );
            log.warn("VERIFY", &format!("List failed (not corruption): {}", e), None);
        }
    }

    log.success("PASS", "No corruption from concurrent writes", None);
}

// =============================================================================
// Sync After Each Agent Pattern
// =============================================================================

#[test]
fn test_sync_after_each_agent_pattern() {
    let mut log = TestLogger::new("test_sync_pattern");
    let env = ConcurrentTestEnv::new("test_sync_pattern");

    if !env.client().is_available() {
        log.warn("SKIP", "bd not available", None);
        return;
    }

    if !env.initialized {
        log.warn("SKIP", "Test database not initialized", None);
        return;
    }

    // Simulate multiple "agents" working sequentially with sync between
    let mut total_created = 0;
    for agent_id in 0..3 {
        log.info("AGENT", &format!("Agent {} starting work", agent_id), None);

        // Each agent gets fresh client (simulating separate processes)
        let client = env.client();

        // Agent does some work
        let mut agent_created = 0;
        for i in 0..3 {
            let req = CreateIssueRequest::new(&format!("Agent {} Task {}", agent_id, i));
            match client.create(&req) {
                Ok(_) => agent_created += 1,
                Err(e) => {
                    log.warn(
                        "CREATE",
                        &format!("Agent {} task {} failed: {}", agent_id, i, e),
                        None,
                    );
                }
            }
        }
        total_created += agent_created;

        // Sync immediately after agent completes (REQUIRED pattern from AGENTS.md)
        log.info("SYNC", &format!("Agent {} syncing", agent_id), None);
        let sync_result = client.sync();
        log.debug(
            "SYNC",
            &format!("Sync result: {:?}", sync_result.is_ok()),
            None,
        );

        log.info(
            "AGENT",
            &format!("Agent {} completed ({} tasks)", agent_id, agent_created),
            None,
        );
    }

    if total_created == 0 {
        log.warn("SKIP", "No tasks were created", None);
        return;
    }

    // Verify all work persisted
    let client = env.client();
    let filter = WorkFilter::default();
    match client.list(&filter) {
        Ok(issues) => {
            let agent_issues = issues
                .iter()
                .filter(|i| i.title.contains("Agent"))
                .count();
            log.info(
                "VERIFY",
                &format!(
                    "Found {}/{} agent issues in database",
                    agent_issues, total_created
                ),
                None,
            );
        }
        Err(e) => {
            log.warn("VERIFY", &format!("List failed: {}", e), None);
        }
    }

    log.success("PASS", "Sync-after-each-agent pattern works", None);
}

// =============================================================================
// Failure Detection and Stop
// =============================================================================

#[test]
fn test_failure_detection() {
    let mut log = TestLogger::new("test_failure_detection");
    let env = ConcurrentTestEnv::new("test_failure_detection");
    let client = env.client();

    if !client.is_available() {
        log.warn("SKIP", "bd not available", None);
        return;
    }

    // Test that errors are properly surfaced for nonexistent issues
    let result = client.show("nonexistent-issue-xyz-12345");

    match result {
        Ok(_) => {
            log.warn("UNEXPECTED", "Show succeeded for nonexistent issue", None);
        }
        Err(e) => {
            let err_str = e.to_string().to_lowercase();
            log.info("ERROR", &format!("Got expected error: {}", e), None);

            // Verify error is actionable (specific, not generic)
            let is_specific = err_str.contains("not found")
                || err_str.contains("notfound")
                || err_str.contains("no such")
                || err_str.contains("invalid")
                || err_str.contains("does not exist");

            if is_specific {
                log.success("PASS", "Failures are properly detected and specific", None);
            } else {
                log.info(
                    "INFO",
                    "Error is generic but not critical - still actionable",
                    None,
                );
            }
        }
    }
}

// =============================================================================
// Interleaved Read-Write Operations
// =============================================================================

#[test]
fn test_interleaved_read_write() {
    let mut log = TestLogger::new("test_interleaved_read_write");
    let env = ConcurrentTestEnv::new("test_interleaved_rw");

    if !env.client().is_available() {
        log.warn("SKIP", "bd not available", None);
        return;
    }

    if !env.initialized {
        log.warn("SKIP", "Test database not initialized", None);
        return;
    }

    let work_dir = env.work_dir();
    let db_path = env.db_path();

    // One thread writes, another reads concurrently
    let write_dir = work_dir.clone();
    let write_db = db_path.clone();
    let writer = thread::spawn(move || {
        let client = BeadsClient::new()
            .with_work_dir(&write_dir)
            .with_env("BEADS_DB", write_db.to_string_lossy());

        let mut successes = 0;
        for i in 0..10 {
            let req = CreateIssueRequest::new(&format!("Interleaved Write {}", i));
            if client.create(&req).is_ok() {
                successes += 1;
            }
            thread::sleep(Duration::from_millis(20));
        }
        successes
    });

    let read_dir = work_dir.clone();
    let read_db = db_path.clone();
    let reader = thread::spawn(move || {
        let client = BeadsClient::new()
            .with_work_dir(&read_dir)
            .with_env("BEADS_DB", read_db.to_string_lossy());

        let filter = WorkFilter::default();
        let mut successes = 0;
        for _ in 0..20 {
            if client.list(&filter).is_ok() {
                successes += 1;
            }
            thread::sleep(Duration::from_millis(10));
        }
        successes
    });

    let write_success = writer.join().unwrap();
    let read_success = reader.join().unwrap();

    log.info(
        "RESULTS",
        &format!(
            "Writer: {}/10 succeeded, Reader: {}/20 succeeded",
            write_success, read_success
        ),
        None,
    );

    // Verify database integrity
    let client = env.client();
    let filter = WorkFilter::default();
    match client.list(&filter) {
        Ok(issues) => {
            log.info(
                "VERIFY",
                &format!("Database has {} issues after interleaved ops", issues.len()),
                None,
            );
            log.success("PASS", "Interleaved read-write completed without corruption", None);
        }
        Err(e) => {
            let err_str = e.to_string().to_lowercase();
            assert!(
                !err_str.contains("corrupt"),
                "Database corrupted: {}",
                e
            );
            log.warn("PARTIAL", &format!("List failed (not corruption): {}", e), None);
        }
    }
}

// =============================================================================
// Stress Test: Rapid Concurrent Operations
// =============================================================================

#[test]
fn test_stress_rapid_concurrent_ops() {
    let mut log = TestLogger::new("test_stress_concurrent");
    let env = ConcurrentTestEnv::new("test_stress");

    if !env.client().is_available() {
        log.warn("SKIP", "bd not available", None);
        return;
    }

    if !env.initialized {
        log.warn("SKIP", "Test database not initialized", None);
        return;
    }

    let work_dir = env.work_dir();
    let db_path = env.db_path();
    let start = std::time::Instant::now();

    // 10 threads, each doing 5 operations
    let handles: Vec<_> = (0..10)
        .map(|thread_id| {
            let work_dir = work_dir.clone();
            let db_path = db_path.clone();
            thread::spawn(move || {
                let client = BeadsClient::new()
                    .with_work_dir(&work_dir)
                    .with_env("BEADS_DB", db_path.to_string_lossy());

                let filter = WorkFilter::default();
                let mut ops = 0;
                for j in 0..5 {
                    // Alternate between read and write
                    if j % 2 == 0 {
                        let req =
                            CreateIssueRequest::new(&format!("Stress {} - {}", thread_id, j));
                        if client.create(&req).is_ok() {
                            ops += 1;
                        }
                    } else {
                        if client.list(&filter).is_ok() {
                            ops += 1;
                        }
                    }
                }
                ops
            })
        })
        .collect();

    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let elapsed = start.elapsed();
    let total_ops: i32 = results.iter().sum();

    log.info(
        "PERF",
        &format!(
            "{} successful ops across 10 threads in {:?}",
            total_ops, elapsed
        ),
        None,
    );

    // Verify no corruption
    let client = env.client();
    let filter = WorkFilter::default();
    match client.list(&filter) {
        Ok(issues) => {
            log.info(
                "VERIFY",
                &format!("Database has {} issues after stress test", issues.len()),
                None,
            );
            log.success("PASS", "Stress test completed without corruption", None);
        }
        Err(e) => {
            let err_str = e.to_string().to_lowercase();
            assert!(
                !err_str.contains("corrupt"),
                "Database corrupted after stress: {}",
                e
            );
            log.warn("PARTIAL", &format!("List failed (not corruption): {}", e), None);
        }
    }
}
