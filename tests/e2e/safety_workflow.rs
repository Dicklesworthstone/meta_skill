//! E2E Scenario: Safety Workflow
//!
//! Tests the safety policy enforcement lifecycle:
//! status → check → log
//!
//! This is a P1 E2E test that exercises the safety gate workflow.
//! Note: Full DCG integration tests require DCG to be installed.

use super::fixture::E2EFixture;
use ms::error::Result;

/// Test the safety status command.
#[test]
fn test_safety_status() -> Result<()> {
    let mut fixture = E2EFixture::new("safety_status");

    // ==========================================
    // Step 1: Initialize
    // ==========================================
    fixture.log_step("Initialize ms directory");
    let output = fixture.init();
    fixture.assert_success(&output, "init");
    fixture.checkpoint("post_init");

    // ==========================================
    // Step 2: Check safety status
    // ==========================================
    fixture.log_step("Check safety gate status");
    let output = fixture.run_ms(&["--robot", "safety", "status"]);
    fixture.assert_success(&output, "safety status");

    let json = output.json();
    println!("[SAFETY] Status: {:?}", json);

    // Status should include DCG availability info
    assert!(
        json.get("dcg_available").is_some() || json.get("dcg_version").is_some(),
        "Status should report DCG availability"
    );
    fixture.checkpoint("post_status");

    fixture.generate_report();
    Ok(())
}

/// Test safety check for safe commands.
#[test]
fn test_safety_check_safe_command() -> Result<()> {
    let mut fixture = E2EFixture::new("safety_check_safe");

    fixture.log_step("Initialize");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    // ==========================================
    // Check a safe command
    // ==========================================
    fixture.log_step("Check safe command: ls -la");
    let output = fixture.run_ms(&["--robot", "safety", "check", "ls -la"]);

    // The check should complete (might not be allowed if DCG not installed)
    println!("[SAFETY] Check result: {:?}", output.stdout);

    let json = output.json();
    // Should have command info in output
    if let Some(cmd) = json.get("command") {
        println!("[SAFETY] Command checked: {:?}", cmd);
    }
    if let Some(allowed) = json.get("allowed") {
        println!("[SAFETY] Allowed: {:?}", allowed);
    }

    fixture.generate_report();
    Ok(())
}

/// Test safety check for potentially dangerous commands.
#[test]
fn test_safety_check_dangerous_command() -> Result<()> {
    let mut fixture = E2EFixture::new("safety_check_dangerous");

    fixture.log_step("Initialize");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    // ==========================================
    // Check a potentially dangerous command
    // ==========================================
    fixture.log_step("Check dangerous command: rm -rf /");
    let output = fixture.run_ms(&["--robot", "safety", "check", "rm -rf /"]);

    // The check should complete
    println!("[SAFETY] Check result: {:?}", output.stdout);

    let json = output.json();
    // If DCG is available, this should be blocked
    if let Some(allowed) = json.get("allowed") {
        if allowed == &serde_json::Value::Bool(false) {
            println!("[SAFETY] Command correctly blocked");
            // Should have a reason
            if let Some(reason) = json.get("reason") {
                println!("[SAFETY] Reason: {:?}", reason);
            }
        }
    }

    fixture.generate_report();
    Ok(())
}

/// Test safety log viewing.
#[test]
fn test_safety_log() -> Result<()> {
    let mut fixture = E2EFixture::new("safety_log");

    fixture.log_step("Initialize");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    // ==========================================
    // View safety log
    // ==========================================
    fixture.log_step("View safety log");
    let output = fixture.run_ms(&["--robot", "safety", "log"]);
    fixture.assert_success(&output, "safety log");

    let json = output.json();
    println!("[SAFETY] Log: {:?}", json);

    // Should have events array (may be empty)
    if let Some(events) = json.get("events") {
        if let Some(arr) = events.as_array() {
            println!("[SAFETY] Events count: {}", arr.len());
        }
    }

    fixture.generate_report();
    Ok(())
}

/// Test safety log with filters.
#[test]
fn test_safety_log_filters() -> Result<()> {
    let mut fixture = E2EFixture::new("safety_log_filters");

    fixture.log_step("Initialize");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    // ==========================================
    // View safety log with limit
    // ==========================================
    fixture.log_step("View safety log with limit");
    let output = fixture.run_ms(&["--robot", "safety", "log", "--limit", "5"]);
    fixture.assert_success(&output, "safety log with limit");

    let json = output.json();
    if let Some(events) = json.get("events").and_then(|e| e.as_array()) {
        assert!(events.len() <= 5, "Should have at most 5 events");
        println!("[SAFETY] Events with limit: {}", events.len());
    }

    // ==========================================
    // View safety log with blocked-only filter
    // ==========================================
    fixture.log_step("View blocked-only events");
    let output = fixture.run_ms(&["--robot", "safety", "log", "--blocked-only"]);
    fixture.assert_success(&output, "safety log blocked-only");

    let json = output.json();
    println!("[SAFETY] Blocked-only log: {:?}", json);

    fixture.generate_report();
    Ok(())
}

/// Test safety check with session ID for audit.
#[test]
fn test_safety_check_with_session() -> Result<()> {
    let mut fixture = E2EFixture::new("safety_check_session");

    fixture.log_step("Initialize");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    // ==========================================
    // Check command with session ID
    // ==========================================
    fixture.log_step("Check command with session ID");
    let output = fixture.run_ms(&[
        "--robot",
        "safety",
        "check",
        "echo hello",
        "--session-id",
        "test-session-123",
    ]);

    println!("[SAFETY] Check with session: {:?}", output.stdout);

    // Verify session can be used to filter log
    fixture.log_step("Filter log by session");
    let output = fixture.run_ms(&["--robot", "safety", "log", "--session", "test-session-123"]);
    fixture.assert_success(&output, "safety log by session");

    let json = output.json();
    println!("[SAFETY] Session-filtered log: {:?}", json);

    fixture.generate_report();
    Ok(())
}

/// Test safety check with dry-run mode.
#[test]
fn test_safety_check_dry_run() -> Result<()> {
    let mut fixture = E2EFixture::new("safety_check_dry_run");

    fixture.log_step("Initialize");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    // ==========================================
    // Check command in dry-run mode
    // ==========================================
    fixture.log_step("Check command in dry-run mode");
    let output = fixture.run_ms(&["--robot", "safety", "check", "ls -la", "--dry-run"]);

    println!("[SAFETY] Dry-run check: {:?}", output.stdout);

    // Dry-run should not log the event
    // We can't easily verify this without checking log counts

    fixture.generate_report();
    Ok(())
}

/// Test complete safety workflow.
#[test]
fn test_safety_workflow_complete() -> Result<()> {
    let mut fixture = E2EFixture::new("safety_workflow_complete");

    // ==========================================
    // Step 1: Initialize
    // ==========================================
    fixture.log_step("Initialize ms directory");
    let output = fixture.init();
    fixture.assert_success(&output, "init");
    fixture.checkpoint("post_init");

    // ==========================================
    // Step 2: Check status
    // ==========================================
    fixture.log_step("Check safety status");
    let output = fixture.run_ms(&["--robot", "safety", "status"]);
    fixture.assert_success(&output, "status");

    let status_json = output.json();
    let dcg_available = status_json
        .get("dcg_available")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    println!("[SAFETY] DCG available: {}", dcg_available);
    fixture.checkpoint("post_status");

    // ==========================================
    // Step 3: Check various commands
    // ==========================================
    fixture.log_step("Check multiple commands");

    // Safe command
    let output = fixture.run_ms(&[
        "--robot",
        "safety",
        "check",
        "cat /etc/hostname",
        "--session-id",
        "workflow-test",
    ]);
    println!("[SAFETY] cat check: {:?}", output.stdout);

    // Another safe command
    let output = fixture.run_ms(&[
        "--robot",
        "safety",
        "check",
        "pwd",
        "--session-id",
        "workflow-test",
    ]);
    println!("[SAFETY] pwd check: {:?}", output.stdout);

    // Potentially dangerous command
    let output = fixture.run_ms(&[
        "--robot",
        "safety",
        "check",
        "chmod 777 /",
        "--session-id",
        "workflow-test",
    ]);
    println!("[SAFETY] chmod check: {:?}", output.stdout);

    fixture.checkpoint("post_checks");

    // ==========================================
    // Step 4: View log
    // ==========================================
    fixture.log_step("View safety log");
    let output = fixture.run_ms(&["--robot", "safety", "log"]);
    fixture.assert_success(&output, "view log");

    let json = output.json();
    if let Some(events) = json.get("events").and_then(|e| e.as_array()) {
        println!("[SAFETY] Total events: {}", events.len());
    }
    fixture.checkpoint("post_log");

    // ==========================================
    // Step 5: Filter log by session
    // ==========================================
    fixture.log_step("Filter log by session");
    let output = fixture.run_ms(&["--robot", "safety", "log", "--session", "workflow-test"]);
    fixture.assert_success(&output, "filter log");

    let json = output.json();
    println!("[SAFETY] Session events: {:?}", json);
    fixture.checkpoint("post_filter");

    fixture.generate_report();
    Ok(())
}
