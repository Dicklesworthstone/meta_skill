//! E2E test binary for beads integration
//!
//! Run with: cargo run --bin `test_beads_e2e`

use std::process::Command;
use std::time::Instant;

use tempfile::TempDir;

use ms::beads::{BeadsClient, CreateIssueRequest, IssueStatus, IssueType, WorkFilter};
use ms::{MsError, Result};

fn main() -> Result<()> {
    println!("=== Beads Integration E2E Test ===");

    // Step 1: Verify bd availability
    let probe = BeadsClient::new();
    if !probe.is_available() {
        return Err(MsError::BeadsUnavailable(
            "bd binary not available".to_string(),
        ));
    }
    println!("[1/9] bd available");

    // Step 2: Create isolated test environment
    let temp_dir = TempDir::new()
        .map_err(|e| MsError::AssertionFailed(format!("failed to create temp dir: {e}")))?;
    let beads_dir = temp_dir.path().join(".beads");
    std::fs::create_dir_all(&beads_dir)?;
    let db_path = beads_dir.join("beads.db");

    let init_output = Command::new("bd")
        .args(["init"])
        .current_dir(temp_dir.path())
        .env("BEADS_DB", &db_path)
        .output()
        .map_err(|e| MsError::AssertionFailed(format!("bd init failed: {e}")))?;
    if !init_output.status.success() {
        let stderr = String::from_utf8_lossy(&init_output.stderr);
        return Err(MsError::AssertionFailed(format!(
            "bd init failed: {stderr}"
        )));
    }
    println!("[2/9] database initialized: {}", db_path.display());

    let client = BeadsClient::new()
        .with_env("BEADS_DB", db_path.to_string_lossy())
        .with_work_dir(temp_dir.path());

    // Step 3: Create issues
    let epic = client.create(
        &CreateIssueRequest::new("E2E Test Epic")
            .with_type(IssueType::Epic)
            .with_priority(1),
    )?;
    let task1 = client.create(
        &CreateIssueRequest::new("E2E Task 1")
            .with_type(IssueType::Task)
            .with_priority(2),
    )?;
    let task2 = client.create(
        &CreateIssueRequest::new("E2E Task 2")
            .with_type(IssueType::Task)
            .with_priority(2),
    )?;
    println!(
        "[3/9] created epic {} and tasks {}, {}",
        epic.id, task1.id, task2.id
    );

    // Step 4: Dependency management
    client.add_dependency(&task1.id, &epic.id)?;
    client.add_dependency(&task2.id, &epic.id)?;
    let epic_details = client.show(&epic.id)?;
    if epic_details.dependents.len() < 2 {
        return Err(MsError::AssertionFailed(format!(
            "expected at least 2 dependents, got {}",
            epic_details.dependents.len()
        )));
    }
    println!("[4/9] dependencies added");

    // Step 5: Issue lifecycle
    client.update_status(&task1.id, IssueStatus::InProgress)?;
    let updated = client.show(&task1.id)?;
    if updated.status != IssueStatus::InProgress {
        return Err(MsError::AssertionFailed(format!(
            "expected in_progress, got {:?}",
            updated.status
        )));
    }
    client.close(&task1.id, Some("E2E test complete"))?;
    let closed = client.show(&task1.id)?;
    if closed.status != IssueStatus::Closed {
        return Err(MsError::AssertionFailed(format!(
            "expected closed, got {:?}",
            closed.status
        )));
    }
    println!("[5/9] lifecycle complete");

    // Step 6: Ready list + open list
    let open = client.list(&WorkFilter {
        status: Some(IssueStatus::Open),
        ..Default::default()
    })?;
    let ready = client.ready()?;
    println!(
        "[6/9] open issues: {}, ready issues: {}",
        open.len(),
        ready.len()
    );

    // Step 7: Error recovery (expected failure)
    if client.show("does-not-exist").is_ok() {
        return Err(MsError::AssertionFailed(
            "expected show() to fail for missing issue".to_string(),
        ));
    }
    println!("[7/9] missing issue handled");

    // Step 8: Sync operation (graceful if no git)
    match client.sync() {
        Ok(()) => println!("[8/9] sync completed"),
        Err(err) => println!("[8/9] sync skipped/failed: {err}"),
    }

    // Step 9: Performance check
    let start = Instant::now();
    for _ in 0..5 {
        let _ = client.list(&WorkFilter {
            status: Some(IssueStatus::Open),
            ..Default::default()
        })?;
    }
    let avg_ms = start.elapsed().as_millis() / 5;
    if avg_ms >= 500 {
        println!("[WARN] list performance slower than expected: {avg_ms}ms");
    }
    println!("[9/9] average list time: {avg_ms}ms");

    println!("=== All E2E Tests Passed ===");
    Ok(())
}
