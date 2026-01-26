//! Real Beads Integration Tests
//!
//! These tests verify BeadsClient behavior using actual `bd` commands in isolated
//! temporary environments. Unlike the mock client tests, these tests:
//!
//! - Execute real `bd` CLI commands
//! - Create actual SQLite databases with WAL
//! - Test concurrent access patterns
//! - Verify data persistence
//!
//! # When to Use These Tests vs MockBeadsClient
//!
//! | Test Scenario | Use |
//! |--------------|-----|
//! | Unit testing code that calls BeadsOperations | `MockBeadsClient` |
//! | Testing error injection and edge cases | `MockBeadsClient` |
//! | Integration testing with real database | These tests |
//! | Testing concurrent access | These tests |
//! | Testing WAL safety | These tests |
//!
//! # Running These Tests
//!
//! ```bash
//! # Run all beads integration tests
//! cargo test --test integration beads_real
//!
//! # Run with verbose output
//! BEADS_TEST_VERBOSE=1 cargo test --test integration beads_real -- --nocapture
//! ```
//!
//! # Requirements
//!
//! - `bd` CLI must be installed and in PATH
//! - Tests are automatically skipped if `bd` is not available

use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;

use ms::beads::{
    BeadsClient, CreateIssueRequest, IssueStatus, IssueType, UpdateIssueRequest, WorkFilter,
};
use tempfile::TempDir;

// =============================================================================
// Test Fixture
// =============================================================================

/// Test environment for real beads integration tests.
///
/// Creates an isolated temporary directory with its own beads database,
/// ensuring tests don't interfere with each other or the user's data.
struct RealBeadsEnv {
    /// Temporary directory (dropped last to clean up)
    #[allow(dead_code)]
    temp_dir: TempDir,
    /// Project directory inside temp
    project_dir: PathBuf,
    /// Path to the test database
    db_path: PathBuf,
    /// Whether bd was successfully initialized
    initialized: bool,
}

impl RealBeadsEnv {
    /// Create a new isolated test environment.
    ///
    /// Returns None if bd is not available.
    fn new(test_name: &str) -> Option<Self> {
        // Check if bd is available
        if !Command::new("bd")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            eprintln!("[{}] SKIP: bd not available", test_name);
            return None;
        }

        let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
        let project_dir = temp_dir.path().join("project");
        std::fs::create_dir(&project_dir).expect("Failed to create project dir");

        let beads_dir = project_dir.join(".beads");
        std::fs::create_dir_all(&beads_dir).expect("Failed to create .beads directory");
        let db_path = beads_dir.join("beads.db");

        // Initialize database using BEADS_DB env var
        let init_status = Command::new("bd")
            .args(["init"])
            .env("BEADS_DB", &db_path)
            .current_dir(&project_dir)
            .output();

        let initialized = match init_status {
            Ok(output) if output.status.success() => true,
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!("[{}] bd init failed: {}", test_name, stderr);
                false
            }
            Err(e) => {
                eprintln!("[{}] Failed to run bd init: {}", test_name, e);
                false
            }
        };

        Some(RealBeadsEnv {
            temp_dir,
            project_dir,
            db_path,
            initialized,
        })
    }

    /// Get a BeadsClient configured for this test environment.
    fn client(&self) -> BeadsClient {
        BeadsClient::new()
            .with_work_dir(&self.project_dir)
            .with_env("BEADS_DB", self.db_path.to_string_lossy())
    }

    /// Get the project directory path.
    fn project_dir(&self) -> PathBuf {
        self.project_dir.clone()
    }

    /// Get the database path.
    fn db_path(&self) -> PathBuf {
        self.db_path.clone()
    }
}

// =============================================================================
// Basic CRUD Operations
// =============================================================================

#[test]
fn test_real_beads_availability() {
    let Some(env) = RealBeadsEnv::new("test_availability") else {
        return;
    };

    let client = env.client();
    assert!(client.is_available(), "bd should be available");
}

#[test]
fn test_real_beads_create_issue() {
    let Some(env) = RealBeadsEnv::new("test_create") else {
        return;
    };
    if !env.initialized {
        eprintln!("SKIP: Test database not initialized");
        return;
    }

    let client = env.client();
    let req = CreateIssueRequest::new("Test Issue")
        .with_type(IssueType::Task)
        .with_priority(2)
        .with_description("Test description");

    let issue = client.create(&req).expect("Create should succeed");

    assert!(!issue.id.is_empty(), "Issue should have an ID");
    assert_eq!(issue.title, "Test Issue");
    assert_eq!(issue.status, IssueStatus::Open);
    assert_eq!(issue.issue_type, IssueType::Task);
}

#[test]
fn test_real_beads_full_lifecycle() {
    let Some(env) = RealBeadsEnv::new("test_lifecycle") else {
        return;
    };
    if !env.initialized {
        return;
    }

    let client = env.client();

    // CREATE
    let issue = client
        .create(&CreateIssueRequest::new("Lifecycle Test Issue"))
        .expect("Create should succeed");

    // READ
    let fetched = client.show(&issue.id).expect("Show should succeed");
    assert_eq!(fetched.id, issue.id);

    // UPDATE
    client
        .update_status(&issue.id, IssueStatus::InProgress)
        .expect("Update should succeed");

    let updated = client.show(&issue.id).expect("Show updated should succeed");
    assert_eq!(updated.status, IssueStatus::InProgress);

    // CLOSE
    client
        .close(&issue.id, Some("Test complete"))
        .expect("Close should succeed");

    let closed = client.show(&issue.id).expect("Show closed should succeed");
    assert_eq!(closed.status, IssueStatus::Closed);
}

#[test]
fn test_real_beads_list_and_ready() {
    let Some(env) = RealBeadsEnv::new("test_list_ready") else {
        return;
    };
    if !env.initialized {
        return;
    }

    let client = env.client();

    // Create multiple issues
    let mut created_ids = Vec::new();
    for i in 0..5 {
        let req = CreateIssueRequest::new(&format!("List Test {}", i));
        if let Ok(issue) = client.create(&req) {
            created_ids.push(issue.id);
        }
    }

    assert!(!created_ids.is_empty(), "Should create at least one issue");

    // Close one issue
    if created_ids.len() >= 2 {
        let _ = client.close(&created_ids[1], None);
    }

    // List all
    let all = client
        .list(&WorkFilter::default())
        .expect("List should work");
    assert!(!all.is_empty(), "Should find some issues");

    // Ready should only return open issues
    let ready = client.ready().expect("Ready should work");
    for issue in &ready {
        assert!(
            !issue.status.is_terminal(),
            "Ready should not include closed issues"
        );
    }
}

#[test]
fn test_real_beads_update_multiple_fields() {
    let Some(env) = RealBeadsEnv::new("test_update_fields") else {
        return;
    };
    if !env.initialized {
        return;
    }

    let client = env.client();

    let issue = client
        .create(&CreateIssueRequest::new("Update Fields Test"))
        .expect("Create should succeed");

    let update = UpdateIssueRequest::new()
        .with_status(IssueStatus::InProgress)
        .with_priority(1)
        .with_notes("Working on it");

    let updated = client
        .update(&issue.id, &update)
        .expect("Update should succeed");

    assert_eq!(updated.status, IssueStatus::InProgress);
    assert_eq!(updated.priority, 1);
}

// =============================================================================
// Dependency Operations
// =============================================================================

#[test]
fn test_real_beads_dependencies() {
    let Some(env) = RealBeadsEnv::new("test_dependencies") else {
        return;
    };
    if !env.initialized {
        return;
    }

    let client = env.client();

    // Create parent and child
    let epic = client
        .create(&CreateIssueRequest::new("Epic").with_type(IssueType::Epic))
        .expect("Create epic should succeed");

    let task = client
        .create(&CreateIssueRequest::new("Task").with_type(IssueType::Task))
        .expect("Create task should succeed");

    // Add dependency: task depends on epic
    client
        .add_dependency(&task.id, &epic.id)
        .expect("Add dependency should succeed");

    // Verify via show
    let task_details = client.show(&task.id).expect("Show should succeed");
    let has_dep = task_details.dependencies.iter().any(|d| d.id == epic.id);
    assert!(has_dep, "Task should depend on epic");
}

// =============================================================================
// Error Handling
// =============================================================================

#[test]
fn test_real_beads_not_found_error() {
    let Some(env) = RealBeadsEnv::new("test_not_found") else {
        return;
    };

    let client = env.client();
    let result = client.show("nonexistent-issue-xyz-12345");

    assert!(result.is_err(), "Should error for nonexistent issue");
}

#[test]
fn test_real_beads_security_validation() {
    let client = BeadsClient::new();

    // Path traversal should be blocked
    assert!(client.show("../../../etc/passwd").is_err());

    // Shell injection should be blocked
    assert!(client.show("test; rm -rf /").is_err());
    assert!(client.show("test$(whoami)").is_err());
    assert!(client.show("test|cat").is_err());
}

// =============================================================================
// Concurrent Access Tests
// =============================================================================

#[test]
fn test_real_beads_concurrent_reads() {
    let Some(env) = RealBeadsEnv::new("test_concurrent_reads") else {
        return;
    };
    if !env.initialized {
        return;
    }

    let client = env.client();

    // Create baseline data
    let _ = client.create(&CreateIssueRequest::new("Concurrent Read Test"));

    let project_dir = env.project_dir();
    let db_path = env.db_path();

    // Spawn multiple reader threads
    let handles: Vec<_> = (0..5)
        .map(|_| {
            let project_dir = project_dir.clone();
            let db_path = db_path.clone();
            thread::spawn(move || {
                let client = BeadsClient::new()
                    .with_work_dir(&project_dir)
                    .with_env("BEADS_DB", db_path.to_string_lossy());

                let mut successes = 0;
                for _ in 0..10 {
                    if client.list(&WorkFilter::default()).is_ok() {
                        successes += 1;
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                successes
            })
        })
        .collect();

    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let total_success: i32 = results.iter().sum();

    // Most reads should succeed
    assert!(
        total_success > 25,
        "Most concurrent reads should succeed: {}/50",
        total_success
    );
}

#[test]
fn test_real_beads_concurrent_writes_no_corruption() {
    let Some(env) = RealBeadsEnv::new("test_concurrent_writes") else {
        return;
    };
    if !env.initialized {
        return;
    }

    let project_dir = env.project_dir();
    let db_path = env.db_path();

    // Spawn multiple writer threads
    let handles: Vec<_> = (0..5)
        .map(|thread_id| {
            let project_dir = project_dir.clone();
            let db_path = db_path.clone();
            thread::spawn(move || {
                let client = BeadsClient::new()
                    .with_work_dir(&project_dir)
                    .with_env("BEADS_DB", db_path.to_string_lossy());

                let mut successes = 0;
                for j in 0..5 {
                    let req = CreateIssueRequest::new(&format!("Concurrent {} - {}", thread_id, j));
                    match client.create(&req) {
                        Ok(_) => successes += 1,
                        Err(e) => {
                            let err = e.to_string().to_lowercase();
                            // Verify error is NOT corruption
                            assert!(
                                !err.contains("corrupt") && !err.contains("malformed"),
                                "Got corruption error: {}",
                                e
                            );
                        }
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                successes
            })
        })
        .collect();

    let _results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // Verify database is still queryable (not corrupt)
    let client = env.client();
    let result = client.list(&WorkFilter::default());
    match result {
        Ok(issues) => {
            assert!(issues.len() > 0, "Should have some issues after writes");
        }
        Err(e) => {
            let err_str = e.to_string().to_lowercase();
            assert!(
                !err_str.contains("corrupt") && !err_str.contains("malformed"),
                "Database corrupted: {}",
                e
            );
        }
    }
}

#[test]
fn test_real_beads_interleaved_read_write() {
    let Some(env) = RealBeadsEnv::new("test_interleaved") else {
        return;
    };
    if !env.initialized {
        return;
    }

    let project_dir = env.project_dir();
    let db_path = env.db_path();

    // Writer thread
    let write_dir = project_dir.clone();
    let write_db = db_path.clone();
    let writer = thread::spawn(move || {
        let client = BeadsClient::new()
            .with_work_dir(&write_dir)
            .with_env("BEADS_DB", write_db.to_string_lossy());

        for i in 0..10 {
            let req = CreateIssueRequest::new(&format!("Interleaved {}", i));
            let _ = client.create(&req);
            thread::sleep(Duration::from_millis(20));
        }
    });

    // Reader thread
    let read_dir = project_dir.clone();
    let read_db = db_path.clone();
    let reader = thread::spawn(move || {
        let client = BeadsClient::new()
            .with_work_dir(&read_dir)
            .with_env("BEADS_DB", read_db.to_string_lossy());

        let mut successes = 0;
        for _ in 0..20 {
            if client.list(&WorkFilter::default()).is_ok() {
                successes += 1;
            }
            thread::sleep(Duration::from_millis(10));
        }
        successes
    });

    writer.join().unwrap();
    let read_success = reader.join().unwrap();

    // Reader should mostly succeed
    assert!(
        read_success > 10,
        "Reader should succeed most of the time: {}/20",
        read_success
    );

    // Verify no corruption
    let client = env.client();
    let result = client.list(&WorkFilter::default());
    assert!(result.is_ok(), "Database should be queryable after test");
}

// =============================================================================
// WAL Safety Tests
// =============================================================================

#[test]
fn test_real_beads_wal_integrity() {
    let Some(env) = RealBeadsEnv::new("test_wal_integrity") else {
        return;
    };
    if !env.initialized {
        return;
    }

    let client = env.client();
    let db_dir = env.project_dir().join(".beads");

    // Run operations that trigger WAL activity
    for i in 0..5 {
        let req = CreateIssueRequest::new(&format!("WAL Test {}", i));
        if let Ok(issue) = client.create(&req) {
            let _ = client.update_status(&issue.id, IssueStatus::InProgress);
        }
    }

    // Verify database is still queryable
    let result = client.list(&WorkFilter::default());
    assert!(result.is_ok(), "Database should remain queryable");

    // Check WAL files exist (if WAL mode is enabled)
    let wal_path = db_dir.join("beads.db-wal");
    if wal_path.exists() {
        let wal_size = std::fs::metadata(&wal_path).map(|m| m.len()).unwrap_or(0);
        assert!(
            wal_size < 100_000_000,
            "WAL file should not grow unboundedly"
        );
    }
}

#[test]
fn test_real_beads_rapid_creates() {
    let Some(env) = RealBeadsEnv::new("test_rapid_creates") else {
        return;
    };
    if !env.initialized {
        return;
    }

    let client = env.client();

    // Rapid fire creation
    let mut created_ids = Vec::new();
    for i in 0..20 {
        let req = CreateIssueRequest::new(&format!("Rapid {}", i));
        if let Ok(issue) = client.create(&req) {
            created_ids.push(issue.id);
        }
    }

    assert!(!created_ids.is_empty(), "Should create some issues");

    // Verify data integrity
    let result = client.list(&WorkFilter::default());
    assert!(
        result.is_ok(),
        "Database should be queryable after rapid creates"
    );
}

#[test]
fn test_real_beads_sync_persistence() {
    let Some(env) = RealBeadsEnv::new("test_sync_persistence") else {
        return;
    };
    if !env.initialized {
        return;
    }

    let client = env.client();

    // Create issues
    for i in 0..3 {
        let req = CreateIssueRequest::new(&format!("Sync Test {}", i));
        let _ = client.create(&req);
    }

    // Call sync (may fail without git, that's OK)
    let _ = client.sync();

    // Verify data persists with a fresh client
    let fresh_client = env.client();
    let issues = fresh_client
        .list(&WorkFilter::default())
        .expect("Fresh client should be able to list");

    let found = issues
        .iter()
        .filter(|i| i.title.contains("Sync Test"))
        .count();
    assert!(found > 0, "Should find persisted issues");
}

// =============================================================================
// Labels and Filtering
// =============================================================================

#[test]
fn test_real_beads_labels() {
    let Some(env) = RealBeadsEnv::new("test_labels") else {
        return;
    };
    if !env.initialized {
        return;
    }

    let client = env.client();

    let issue = client
        .create(
            &CreateIssueRequest::new("Label Test")
                .with_label("backend")
                .with_label("urgent"),
        )
        .expect("Create with labels should succeed");

    let fetched = client.show(&issue.id).expect("Show should succeed");
    assert!(fetched.labels.iter().any(|l| l == "backend"));
    assert!(fetched.labels.iter().any(|l| l == "urgent"));
}

#[test]
fn test_real_beads_filter_by_status() {
    let Some(env) = RealBeadsEnv::new("test_filter_status") else {
        return;
    };
    if !env.initialized {
        return;
    }

    let client = env.client();

    // Create open and closed issues
    let open_issue = client
        .create(&CreateIssueRequest::new("Open Issue"))
        .expect("Create should succeed");

    let closed_issue = client
        .create(&CreateIssueRequest::new("Closed Issue"))
        .expect("Create should succeed");

    let _ = client.close(&closed_issue.id, None);

    // Filter by open status
    let open_filter = WorkFilter {
        status: Some(IssueStatus::Open),
        ..Default::default()
    };

    let open_issues = client.list(&open_filter).expect("List should work");

    // Should find the open one, not the closed one
    let found_open = open_issues.iter().any(|i| i.id == open_issue.id);
    let found_closed = open_issues.iter().any(|i| i.id == closed_issue.id);

    assert!(found_open, "Should find open issue in open filter");
    assert!(!found_closed, "Should NOT find closed issue in open filter");
}

// =============================================================================
// Version Compatibility
// =============================================================================

#[test]
fn test_real_beads_version() {
    let client = BeadsClient::new();

    if !client.is_available() {
        return;
    }

    let version = client.version();
    assert!(version.is_some(), "Should get version string");

    let version_str = version.unwrap();
    assert!(!version_str.is_empty(), "Version should not be empty");
}
