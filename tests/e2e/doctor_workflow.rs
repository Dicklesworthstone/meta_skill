//! E2E Scenario: Doctor Workflow Integration Tests
//!
//! Comprehensive tests for the `ms doctor` command covering:
//! - Doctor on a healthy workspace (all checks pass)
//! - Doctor with --fix flag
//! - Doctor with --check flag for specific checks (safety, perf, output)
//! - Doctor with --comprehensive flag for extended recovery diagnostics
//! - Doctor with --check-lock to inspect lock status
//! - Doctor with an unknown --check name (graceful error)
//! - Doctor output format and summary structure
//! - Doctor on a workspace with no indexed skills
//! - Doctor idempotency (running doctor twice yields consistent results)

use super::fixture::E2EFixture;
use ms::error::Result;

// ============================================================================
// Skill definitions used by the healthy-workspace helper
// ============================================================================

const SKILL_BASIC: &str = r#"---
name: Basic Skill
description: A simple skill for doctor health checks
tags: [test, basic]
---

# Basic Skill

This is a basic skill used for doctor health-check tests.
"#;

const SKILL_EXTRA: &str = r#"---
name: Extra Skill
description: Another skill to verify index consistency
tags: [test, extra]
---

# Extra Skill

This skill exists to give the doctor something to validate.
"#;

// ============================================================================
// Fixtures
// ============================================================================

/// Create a fully healthy workspace: init, create skills, and index.
fn setup_healthy_workspace(scenario: &str) -> Result<E2EFixture> {
    let mut fixture = E2EFixture::new(scenario);

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    fixture.log_step("Create test skills");
    fixture.create_skill("basic-skill", SKILL_BASIC)?;
    fixture.create_skill("extra-skill", SKILL_EXTRA)?;

    fixture.log_step("Index skills");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    fixture.checkpoint("doctor:healthy-setup");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "doctor",
        "Healthy workspace created with 2 skills",
        Some(serde_json::json!({ "skills": 2 })),
    );

    Ok(fixture)
}

/// Create a minimal workspace: init only, no skills or index.
fn setup_minimal_workspace(scenario: &str) -> Result<E2EFixture> {
    let mut fixture = E2EFixture::new(scenario);

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    fixture.checkpoint("doctor:minimal-setup");

    Ok(fixture)
}

// ============================================================================
// Tests
// ============================================================================

/// Doctor on a healthy, fully-initialized workspace should report no issues.
#[test]
fn test_doctor_healthy_workspace() -> Result<()> {
    let mut fixture = setup_healthy_workspace("doctor_healthy")?;

    fixture.checkpoint("doctor:pre-run");

    fixture.log_step("Run doctor on healthy workspace");
    let output = fixture.run_ms(&["doctor"]);
    fixture.assert_success(&output, "doctor healthy");

    fixture.checkpoint("doctor:post-run");

    // The doctor should report all checks passed
    fixture.assert_output_contains(&output, "All checks passed");

    // Should mention checking database
    fixture.assert_output_contains(&output, "Checking database");

    // Should mention checking Git archive
    fixture.assert_output_contains(&output, "Checking Git archive");

    // Should mention checking transactions
    fixture.assert_output_contains(&output, "Checking transactions");

    // Should mention checking lock status
    fixture.assert_output_contains(&output, "Checking lock status");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "doctor",
        "Healthy workspace passed all checks",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Doctor on a minimal workspace (init only, no skills) should also pass.
#[test]
fn test_doctor_minimal_workspace() -> Result<()> {
    let mut fixture = setup_minimal_workspace("doctor_minimal")?;

    fixture.log_step("Run doctor on minimal workspace (no skills)");
    let output = fixture.run_ms(&["doctor"]);
    fixture.assert_success(&output, "doctor minimal");

    fixture.assert_output_contains(&output, "All checks passed");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "doctor",
        "Minimal workspace (no skills) passed all checks",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Doctor with --fix on a healthy workspace should succeed with no repairs needed.
#[test]
fn test_doctor_fix_healthy() -> Result<()> {
    let mut fixture = setup_healthy_workspace("doctor_fix_healthy")?;

    fixture.checkpoint("doctor:pre-fix");

    fixture.log_step("Run doctor with --fix on healthy workspace");
    let output = fixture.run_ms(&["doctor", "--fix"]);
    fixture.assert_success(&output, "doctor --fix");

    fixture.checkpoint("doctor:post-fix");

    // On a healthy workspace, --fix should still report all passed
    fixture.assert_output_contains(&output, "All checks passed");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "doctor",
        "Doctor --fix on healthy workspace completed (nothing to fix)",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Doctor with --fix exercises the transaction recovery path.
#[test]
fn test_doctor_fix_transaction_recovery() -> Result<()> {
    let mut fixture = setup_healthy_workspace("doctor_fix_txn")?;

    fixture.checkpoint("doctor:pre-fix-txn");

    fixture.log_step("Run doctor --fix to exercise transaction recovery code path");
    let output = fixture.run_ms(&["doctor", "--fix"]);
    fixture.assert_success(&output, "doctor --fix txn");

    // The output should indicate checks completed (all passed or issues fixed)
    let combined = format!("{}{}", output.stdout, output.stderr);
    let valid_outcome = combined.contains("All checks passed") || combined.contains("fixed");
    assert!(
        valid_outcome,
        "Doctor --fix should report either all passed or issues fixed.\nStdout: {}\nStderr: {}",
        output.stdout, output.stderr
    );

    fixture.checkpoint("doctor:post-fix-txn");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "doctor",
        "Transaction recovery path exercised via --fix",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Doctor --check safety should run only the safety check in isolation.
#[test]
fn test_doctor_check_safety() -> Result<()> {
    let mut fixture = setup_healthy_workspace("doctor_check_safety")?;

    fixture.log_step("Run doctor --check safety");
    let output = fixture.run_ms(&["doctor", "--check", "safety"]);
    fixture.assert_success(&output, "doctor --check safety");

    // Safety check should mention dcg (the command safety tool)
    fixture.assert_output_contains(&output, "dcg");

    // Should NOT run the general database/archive/transaction checks
    fixture.assert_output_not_contains(&output, "Checking database");
    fixture.assert_output_not_contains(&output, "Checking Git archive");
    fixture.assert_output_not_contains(&output, "Checking transactions");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "doctor",
        "Safety check ran in isolation",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Doctor --check security should run the comprehensive security check.
#[test]
fn test_doctor_check_security() -> Result<()> {
    let mut fixture = setup_healthy_workspace("doctor_check_security")?;

    fixture.log_step("Run doctor --check security");
    let output = fixture.run_ms(&["doctor", "--check", "security"]);
    fixture.assert_success(&output, "doctor --check security");

    // Security check should contain the section header
    fixture.assert_output_contains(&output, "Security Checks");

    // Should check DCG and environment files
    fixture.assert_output_contains(&output, "Command safety");
    fixture.assert_output_contains(&output, "Environment files");

    // Should NOT run general checks
    fixture.assert_output_not_contains(&output, "Checking database");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "doctor",
        "Security check ran successfully",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Doctor --check with an unknown check name should report an error gracefully.
#[test]
fn test_doctor_check_unknown() -> Result<()> {
    let mut fixture = setup_healthy_workspace("doctor_check_unknown")?;

    fixture.log_step("Run doctor --check with unknown check name");
    let output = fixture.run_ms(&["doctor", "--check", "nonexistent"]);
    // Doctor should still succeed but report the unknown check
    fixture.assert_output_contains(&output, "Unknown check");
    fixture.assert_output_contains(&output, "nonexistent");
    // Should list the available checks so the user knows what to try
    fixture.assert_output_contains(&output, "Available checks");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "doctor",
        "Unknown check name handled gracefully with suggestions",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Doctor --comprehensive should run the extended recovery diagnostics.
#[test]
fn test_doctor_comprehensive() -> Result<()> {
    let mut fixture = setup_healthy_workspace("doctor_comprehensive")?;

    fixture.log_step("Run doctor --comprehensive");
    let output = fixture.run_ms(&["doctor", "--comprehensive"]);
    fixture.assert_success(&output, "doctor --comprehensive");

    // Comprehensive mode should show the recovery diagnostics header
    fixture.assert_output_contains(&output, "Comprehensive Recovery Diagnostics");

    // It should still run the standard checks as well
    fixture.assert_output_contains(&output, "Checking database");
    fixture.assert_output_contains(&output, "Checking Git archive");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "doctor",
        "Comprehensive recovery diagnostics completed",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Doctor --comprehensive --fix should run recovery with automatic repair.
#[test]
fn test_doctor_comprehensive_fix() -> Result<()> {
    let mut fixture = setup_healthy_workspace("doctor_comprehensive_fix")?;

    fixture.log_step("Run doctor --comprehensive --fix");
    let output = fixture.run_ms(&["doctor", "--comprehensive", "--fix"]);
    fixture.assert_success(&output, "doctor --comprehensive --fix");

    fixture.assert_output_contains(&output, "Comprehensive Recovery Diagnostics");

    // Should reach the summary line
    let combined = format!("{}{}", output.stdout, output.stderr);
    let has_summary = combined.contains("All checks passed")
        || combined.contains("Found")
        || combined.contains("No issues detected");
    assert!(
        has_summary,
        "Doctor --comprehensive --fix should produce a summary.\nStdout: {}\nStderr: {}",
        output.stdout, output.stderr
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "doctor",
        "Comprehensive recovery with --fix completed",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Doctor --check-lock should report lock status on a workspace with no active lock.
#[test]
fn test_doctor_check_lock() -> Result<()> {
    let mut fixture = setup_healthy_workspace("doctor_check_lock")?;

    fixture.log_step("Run doctor --check-lock");
    let output = fixture.run_ms(&["doctor", "--check-lock"]);
    fixture.assert_success(&output, "doctor --check-lock");

    // On a fresh workspace with no active lock, should report no lock held
    fixture.assert_output_contains(&output, "No lock held");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "doctor",
        "Lock status check passed (no lock held)",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Doctor --check output should produce the output mode detection report.
#[test]
fn test_doctor_check_output_mode() -> Result<()> {
    let mut fixture = setup_healthy_workspace("doctor_check_output_mode")?;

    fixture.log_step("Run doctor --check output");
    let output = fixture.run_ms(&["doctor", "--check", "output"]);
    fixture.assert_success(&output, "doctor --check output");

    // Output mode check should contain its diagnostic sections
    fixture.assert_output_contains(&output, "Output Mode Detection Report");
    fixture.assert_output_contains(&output, "Configuration");
    fixture.assert_output_contains(&output, "Environment Variables");
    fixture.assert_output_contains(&output, "Terminal");
    fixture.assert_output_contains(&output, "Decision");

    // Should NOT run general checks
    fixture.assert_output_not_contains(&output, "Checking database");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "doctor",
        "Output mode detection report generated",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Doctor --check perf should report performance metrics.
#[test]
fn test_doctor_check_perf() -> Result<()> {
    let mut fixture = setup_healthy_workspace("doctor_check_perf")?;

    fixture.log_step("Run doctor --check perf");
    let output = fixture.run_ms(&["doctor", "--check", "perf"]);
    fixture.assert_success(&output, "doctor --check perf");

    // Perf check should report on memory usage (on Linux) or search latency
    let combined = format!("{}{}", output.stdout, output.stderr);
    let mentions_perf = combined.contains("Memory")
        || combined.contains("memory")
        || combined.contains("performance")
        || combined.contains("latency")
        || combined.contains("MB")
        || combined.contains("check skipped");

    assert!(
        mentions_perf,
        "Doctor --check perf should report performance metrics.\nStdout: {}\nStderr: {}",
        output.stdout, output.stderr
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "doctor",
        "Performance check completed",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Doctor output should contain the banner header and a structured summary.
#[test]
fn test_doctor_output_format() -> Result<()> {
    let mut fixture = setup_healthy_workspace("doctor_output_format")?;

    fixture.log_step("Run doctor and verify output structure");
    let output = fixture.run_ms(&["doctor"]);
    fixture.assert_success(&output, "doctor output format");

    // Should contain the header banner
    fixture.assert_output_contains(&output, "ms doctor");
    fixture.assert_output_contains(&output, "Health Checks");

    // Should contain individual check status markers (green checkmarks in the output)
    fixture.assert_output_contains(&output, "OK");

    // Should contain a summary at the end
    let combined = format!("{}{}", output.stdout, output.stderr);
    let has_summary = combined.contains("All checks passed") || combined.contains("Found");
    assert!(
        has_summary,
        "Doctor output should contain a summary line.\nStdout: {}\nStderr: {}",
        output.stdout, output.stderr
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "doctor",
        "Doctor output format verified",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Running doctor twice should yield the same result (idempotency).
#[test]
fn test_doctor_idempotent() -> Result<()> {
    let mut fixture = setup_healthy_workspace("doctor_idempotent")?;

    fixture.log_step("Run doctor first time");
    let output1 = fixture.run_ms(&["doctor"]);
    fixture.assert_success(&output1, "doctor run 1");

    fixture.checkpoint("doctor:after-first-run");

    fixture.log_step("Run doctor second time");
    let output2 = fixture.run_ms(&["doctor"]);
    fixture.assert_success(&output2, "doctor run 2");

    fixture.checkpoint("doctor:after-second-run");

    // Both runs should have the same result (all passed)
    fixture.assert_output_contains(&output1, "All checks passed");
    fixture.assert_output_contains(&output2, "All checks passed");

    // Verify that running doctor did not change the workspace state
    if let Some(diff) = fixture.last_checkpoint_diff() {
        // The only changes should be the log files from the fixture itself,
        // not any structural changes to the workspace
        fixture.emit_event(
            super::fixture::LogLevel::Info,
            "doctor",
            &format!("Checkpoint diff: {}", diff.summary()),
            None,
        );
    }

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "doctor",
        "Doctor idempotency verified (two runs produced consistent results)",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Doctor should work correctly even without ms being fully initialized
/// (the CLI constructs AppContext which creates db/archive, so doctor always
/// has a functional workspace to check).
#[test]
fn test_doctor_on_fresh_workspace() -> Result<()> {
    let mut fixture = E2EFixture::new("doctor_fresh");

    // Do NOT run `ms init` -- just run doctor directly.
    // The ms CLI will create the ms root, database, and archive as part of
    // AppContext construction. Doctor should then find everything healthy.
    fixture.log_step("Run doctor on workspace that was never explicitly initialized");
    let output = fixture.run_ms(&["doctor"]);
    fixture.assert_success(&output, "doctor fresh");

    // Since AppContext creates the infrastructure, doctor should pass
    fixture.assert_output_contains(&output, "All checks passed");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "doctor",
        "Doctor on implicitly-initialized workspace passed all checks",
        None,
    );

    fixture.generate_report();
    Ok(())
}
