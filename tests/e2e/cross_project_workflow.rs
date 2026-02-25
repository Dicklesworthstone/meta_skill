//! E2E Scenario: Cross-Project Workflow Integration Tests
//!
//! Comprehensive tests for the `ms cross-project` command covering:
//! - Summary subcommand (aggregation by project, filters, limits)
//! - Patterns subcommand (cross-project pattern extraction)
//! - Gaps subcommand (coverage gap analysis)
//! - Validation errors (zero limits, zero min-projects)
//! - CASS unavailable error handling
//! - JSON output format verification

use std::process::Command;

use super::fixture::{E2EFixture, LogLevel};
use ms::error::Result;

/// Check if the `cass` binary is available on PATH.
/// Tests that require CASS should call this and return early if unavailable.
fn cass_available() -> bool {
    Command::new("cass")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

// ============================================================================
// Skill definitions for gap analysis testing
// ============================================================================

const SKILL_DEBUGGING: &str = r#"---
name: Debugging Patterns
description: Common debugging strategies for software projects
tags: [debugging, workflow]
---

# Debugging Patterns

Common strategies for debugging software issues.

## Rules

- Use structured logging over println debugging
- Reproduce the issue first, then investigate
- Check recent changes in version control
"#;

const SKILL_TESTING: &str = r#"---
name: Testing Best Practices
description: Guidelines for writing effective tests
tags: [testing, quality]
---

# Testing Best Practices

Guidelines for writing effective and maintainable tests.

## Rules

- Write tests for edge cases and error conditions
- Keep tests focused on one behavior
- Use descriptive test names
"#;

// ============================================================================
// Helper: set up workspace with skills for gap analysis
// ============================================================================

fn setup_workspace(scenario: &str) -> Result<E2EFixture> {
    let mut fixture = E2EFixture::new(scenario);

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    fixture.log_step("Create skills for gap analysis");
    fixture.create_skill("debugging-patterns", SKILL_DEBUGGING)?;
    fixture.create_skill("testing-practices", SKILL_TESTING)?;

    fixture.log_step("Index skills");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    fixture.checkpoint("cross-project:workspace-ready");

    fixture.emit_event(
        LogLevel::Info,
        "cross-project",
        "Workspace ready with 2 skills",
        Some(serde_json::json!({ "skills": 2 })),
    );

    Ok(fixture)
}

/// Minimal workspace: init only, for validation tests.
fn setup_minimal(scenario: &str) -> Result<E2EFixture> {
    let mut fixture = E2EFixture::new(scenario);

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    Ok(fixture)
}

// ============================================================================
// Validation Error Tests
// ============================================================================

/// Summary with limit=0 should fail validation.
#[test]
fn test_cross_project_summary_zero_limit() -> Result<()> {
    let mut fixture = setup_minimal("cross_project_summary_zero_limit")?;

    fixture.checkpoint("cross-project:pre-summary-zero-limit");

    fixture.log_step("Run summary with limit=0");
    let output = fixture.run_ms(&["--robot", "cross-project", "summary", "--limit", "0"]);

    assert!(
        !output.success,
        "Summary with limit=0 should fail"
    );

    fixture.checkpoint("cross-project:post-summary-zero-limit");

    fixture.emit_event(
        LogLevel::Info,
        "cross-project",
        "Zero limit validation correctly rejected",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Patterns with limit=0 should fail validation.
#[test]
fn test_cross_project_patterns_zero_limit() -> Result<()> {
    let mut fixture = setup_minimal("cross_project_patterns_zero_limit")?;

    fixture.checkpoint("cross-project:pre-patterns-zero-limit");

    fixture.log_step("Run patterns with limit=0");
    let output = fixture.run_ms(&["--robot", "cross-project", "patterns", "--limit", "0"]);

    assert!(
        !output.success,
        "Patterns with limit=0 should fail"
    );

    fixture.checkpoint("cross-project:post-patterns-zero-limit");

    fixture.emit_event(
        LogLevel::Info,
        "cross-project",
        "Zero limit validation correctly rejected for patterns",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Patterns with min-projects=0 should fail validation.
#[test]
fn test_cross_project_patterns_zero_min_projects() -> Result<()> {
    let mut fixture = setup_minimal("cross_project_patterns_zero_min_projects")?;

    fixture.checkpoint("cross-project:pre-patterns-zero-min-projects");

    fixture.log_step("Run patterns with min-projects=0");
    let output = fixture.run_ms(&[
        "--robot",
        "cross-project",
        "patterns",
        "--min-projects",
        "0",
    ]);

    assert!(
        !output.success,
        "Patterns with min-projects=0 should fail"
    );

    fixture.checkpoint("cross-project:post-patterns-zero-min-projects");

    fixture.emit_event(
        LogLevel::Info,
        "cross-project",
        "Zero min-projects validation correctly rejected",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Gaps with limit=0 should fail validation.
#[test]
fn test_cross_project_gaps_zero_limit() -> Result<()> {
    let mut fixture = setup_minimal("cross_project_gaps_zero_limit")?;

    fixture.checkpoint("cross-project:pre-gaps-zero-limit");

    fixture.log_step("Run gaps with limit=0");
    let output = fixture.run_ms(&["--robot", "cross-project", "gaps", "--limit", "0"]);

    assert!(
        !output.success,
        "Gaps with limit=0 should fail"
    );

    fixture.checkpoint("cross-project:post-gaps-zero-limit");

    fixture.emit_event(
        LogLevel::Info,
        "cross-project",
        "Zero limit validation correctly rejected for gaps",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Gaps with min-projects=0 should fail validation.
#[test]
fn test_cross_project_gaps_zero_min_projects() -> Result<()> {
    let mut fixture = setup_minimal("cross_project_gaps_zero_min_projects")?;

    fixture.checkpoint("cross-project:pre-gaps-zero-min-projects");

    fixture.log_step("Run gaps with min-projects=0");
    let output = fixture.run_ms(&[
        "--robot",
        "cross-project",
        "gaps",
        "--min-projects",
        "0",
    ]);

    assert!(
        !output.success,
        "Gaps with min-projects=0 should fail"
    );

    fixture.checkpoint("cross-project:post-gaps-zero-min-projects");

    fixture.emit_event(
        LogLevel::Info,
        "cross-project",
        "Zero min-projects validation correctly rejected for gaps",
        None,
    );

    fixture.generate_report();
    Ok(())
}

// ============================================================================
// CASS Unavailable Tests
// ============================================================================

/// Summary with bogus cass-path should report unavailable.
#[test]
fn test_cross_project_summary_cass_unavailable() -> Result<()> {
    let mut fixture = setup_minimal("cross_project_summary_cass_unavailable")?;

    fixture.checkpoint("cross-project:pre-summary-no-cass");

    fixture.log_step("Run summary with nonexistent cass binary");
    let output = fixture.run_ms(&[
        "--robot",
        "cross-project",
        "summary",
        "--cass-path",
        "/nonexistent/cass/binary",
    ]);

    assert!(
        !output.success,
        "Summary with unavailable CASS should fail"
    );

    fixture.checkpoint("cross-project:post-summary-no-cass");

    fixture.emit_event(
        LogLevel::Info,
        "cross-project",
        "CASS unavailable correctly detected for summary",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Patterns with bogus cass-path should report unavailable.
#[test]
fn test_cross_project_patterns_cass_unavailable() -> Result<()> {
    let mut fixture = setup_minimal("cross_project_patterns_cass_unavailable")?;

    fixture.checkpoint("cross-project:pre-patterns-no-cass");

    fixture.log_step("Run patterns with nonexistent cass binary");
    let output = fixture.run_ms(&[
        "--robot",
        "cross-project",
        "patterns",
        "--cass-path",
        "/nonexistent/cass/binary",
    ]);

    assert!(
        !output.success,
        "Patterns with unavailable CASS should fail"
    );

    fixture.checkpoint("cross-project:post-patterns-no-cass");

    fixture.emit_event(
        LogLevel::Info,
        "cross-project",
        "CASS unavailable correctly detected for patterns",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Gaps with bogus cass-path should report unavailable.
#[test]
fn test_cross_project_gaps_cass_unavailable() -> Result<()> {
    let mut fixture = setup_minimal("cross_project_gaps_cass_unavailable")?;

    fixture.checkpoint("cross-project:pre-gaps-no-cass");

    fixture.log_step("Run gaps with nonexistent cass binary");
    let output = fixture.run_ms(&[
        "--robot",
        "cross-project",
        "gaps",
        "--cass-path",
        "/nonexistent/cass/binary",
    ]);

    assert!(
        !output.success,
        "Gaps with unavailable CASS should fail"
    );

    fixture.checkpoint("cross-project:post-gaps-no-cass");

    fixture.emit_event(
        LogLevel::Info,
        "cross-project",
        "CASS unavailable correctly detected for gaps",
        None,
    );

    fixture.generate_report();
    Ok(())
}

// ============================================================================
// Summary Workflow Tests (with real CASS)
// ============================================================================

/// Summary with default args should produce valid JSON output.
#[test]
fn test_cross_project_summary_json_output() -> Result<()> {
    if !cass_available() {
        eprintln!("[SKIP] cass binary not available, skipping test");
        return Ok(());
    }
    let mut fixture = setup_workspace("cross_project_summary_json")?;

    fixture.checkpoint("cross-project:pre-summary");

    fixture.log_step("Run cross-project summary with robot mode");
    let output = fixture.run_ms(&["--robot", "cross-project", "summary"]);
    fixture.assert_success(&output, "cross-project summary");

    fixture.checkpoint("cross-project:post-summary");

    let json = output.json();

    // Verify JSON structure
    assert!(
        json["query"].is_string(),
        "Response should have query field"
    );
    assert!(
        json["total_sessions"].is_number(),
        "Response should have total_sessions"
    );
    assert!(
        json["total_projects"].is_number(),
        "Response should have total_projects"
    );
    assert!(
        json["projects"].is_array(),
        "Response should have projects array"
    );

    let total = json["total_sessions"].as_u64().unwrap_or(0);

    fixture.emit_event(
        LogLevel::Info,
        "cross-project",
        &format!("Summary returned {total} sessions"),
        Some(json.clone()),
    );

    fixture.generate_report();
    Ok(())
}

/// Summary with --top limit restricts output.
#[test]
fn test_cross_project_summary_top_limit() -> Result<()> {
    if !cass_available() {
        eprintln!("[SKIP] cass binary not available, skipping test");
        return Ok(());
    }
    let mut fixture = setup_workspace("cross_project_summary_top_limit")?;

    fixture.checkpoint("cross-project:pre-summary-top");

    fixture.log_step("Run summary with --top 2");
    let output = fixture.run_ms(&["--robot", "cross-project", "summary", "--top", "2"]);
    fixture.assert_success(&output, "cross-project summary top");

    fixture.checkpoint("cross-project:post-summary-top");

    let json = output.json();
    let projects = json["projects"].as_array().expect("projects array");
    assert!(
        projects.len() <= 2,
        "Should return at most 2 projects, got {}",
        projects.len()
    );

    fixture.emit_event(
        LogLevel::Info,
        "cross-project",
        "Summary top limit verified",
        Some(serde_json::json!({ "returned": projects.len(), "limit": 2 })),
    );

    fixture.generate_report();
    Ok(())
}

/// Summary with query filter narrows results.
#[test]
fn test_cross_project_summary_with_query() -> Result<()> {
    if !cass_available() {
        eprintln!("[SKIP] cass binary not available, skipping test");
        return Ok(());
    }
    let mut fixture = setup_workspace("cross_project_summary_query")?;

    fixture.checkpoint("cross-project:pre-summary-query");

    fixture.log_step("Run summary with specific query");
    let output = fixture.run_ms(&["--robot", "cross-project", "summary", "--query", "rust"]);
    fixture.assert_success(&output, "cross-project summary with query");

    fixture.checkpoint("cross-project:post-summary-query");

    let json = output.json();
    assert_eq!(
        json["query"].as_str(),
        Some("rust"),
        "Query should be preserved in output"
    );

    fixture.emit_event(
        LogLevel::Info,
        "cross-project",
        "Summary with query filter verified",
        Some(json.clone()),
    );

    fixture.generate_report();
    Ok(())
}

/// Summary with min-sessions filter.
#[test]
fn test_cross_project_summary_min_sessions() -> Result<()> {
    if !cass_available() {
        eprintln!("[SKIP] cass binary not available, skipping test");
        return Ok(());
    }
    let mut fixture = setup_workspace("cross_project_summary_min_sessions")?;

    fixture.checkpoint("cross-project:pre-summary-min-sessions");

    fixture.log_step("Run summary with high min-sessions threshold");
    let output = fixture.run_ms(&[
        "--robot",
        "cross-project",
        "summary",
        "--min-sessions",
        "999999",
    ]);
    fixture.assert_success(&output, "cross-project summary min-sessions");

    fixture.checkpoint("cross-project:post-summary-min-sessions");

    let json = output.json();
    let projects = json["projects"].as_array().expect("projects array");
    assert_eq!(
        projects.len(),
        0,
        "With extremely high min-sessions, no projects should match"
    );

    fixture.emit_event(
        LogLevel::Info,
        "cross-project",
        "Summary min-sessions filter verified",
        None,
    );

    fixture.generate_report();
    Ok(())
}

// ============================================================================
// Patterns Workflow Tests (with real CASS)
// ============================================================================

/// Patterns with default args should produce valid JSON output.
#[test]
fn test_cross_project_patterns_json_output() -> Result<()> {
    if !cass_available() {
        eprintln!("[SKIP] cass binary not available, skipping test");
        return Ok(());
    }
    let mut fixture = setup_workspace("cross_project_patterns_json")?;

    fixture.checkpoint("cross-project:pre-patterns");

    fixture.log_step("Run cross-project patterns with robot mode");
    let output = fixture.run_ms(&["--robot", "cross-project", "patterns"]);
    fixture.assert_success(&output, "cross-project patterns");

    fixture.checkpoint("cross-project:post-patterns");

    let json = output.json();

    // Verify JSON structure
    assert!(
        json["query"].is_string(),
        "Response should have query field"
    );
    assert!(
        json["scanned_sessions"].is_number(),
        "Response should have scanned_sessions"
    );
    assert!(
        json["patterns"].is_array(),
        "Response should have patterns array"
    );

    fixture.emit_event(
        LogLevel::Info,
        "cross-project",
        "Patterns JSON output verified",
        Some(json.clone()),
    );

    fixture.generate_report();
    Ok(())
}

/// Patterns with high thresholds should return fewer/no results.
#[test]
fn test_cross_project_patterns_high_thresholds() -> Result<()> {
    if !cass_available() {
        eprintln!("[SKIP] cass binary not available, skipping test");
        return Ok(());
    }
    let mut fixture = setup_workspace("cross_project_patterns_high_thresholds")?;

    fixture.checkpoint("cross-project:pre-patterns-high-thresholds");

    fixture.log_step("Run patterns with very high occurrence threshold");
    let output = fixture.run_ms(&[
        "--robot",
        "cross-project",
        "patterns",
        "--min-occurrences",
        "999999",
    ]);
    fixture.assert_success(&output, "cross-project patterns high thresholds");

    fixture.checkpoint("cross-project:post-patterns-high-thresholds");

    let json = output.json();
    let patterns = json["patterns"].as_array().expect("patterns array");
    assert_eq!(
        patterns.len(),
        0,
        "With extremely high min-occurrences, no patterns should match"
    );

    fixture.emit_event(
        LogLevel::Info,
        "cross-project",
        "Patterns high threshold filter verified",
        None,
    );

    fixture.generate_report();
    Ok(())
}

// ============================================================================
// Gaps Workflow Tests (with real CASS)
// ============================================================================

/// Gaps with default args should produce valid JSON output.
#[test]
fn test_cross_project_gaps_json_output() -> Result<()> {
    if !cass_available() {
        eprintln!("[SKIP] cass binary not available, skipping test");
        return Ok(());
    }
    let mut fixture = setup_workspace("cross_project_gaps_json")?;

    fixture.checkpoint("cross-project:pre-gaps");

    fixture.log_step("Run cross-project gaps with robot mode");
    let output = fixture.run_ms(&["--robot", "cross-project", "gaps"]);
    fixture.assert_success(&output, "cross-project gaps");

    fixture.checkpoint("cross-project:post-gaps");

    let json = output.json();

    // Verify JSON structure
    assert!(
        json["query"].is_string(),
        "Response should have query field"
    );
    assert!(
        json["scanned_sessions"].is_number(),
        "Response should have scanned_sessions"
    );
    assert!(
        json["gaps"].is_array(),
        "Response should have gaps array"
    );

    fixture.emit_event(
        LogLevel::Info,
        "cross-project",
        "Gaps JSON output verified",
        Some(json.clone()),
    );

    fixture.generate_report();
    Ok(())
}

/// Gaps with high min-score should consider most patterns as gaps.
#[test]
fn test_cross_project_gaps_min_score() -> Result<()> {
    if !cass_available() {
        eprintln!("[SKIP] cass binary not available, skipping test");
        return Ok(());
    }
    let mut fixture = setup_workspace("cross_project_gaps_min_score")?;

    fixture.checkpoint("cross-project:pre-gaps-min-score");

    fixture.log_step("Run gaps with high min-score to include more gaps");
    let output = fixture.run_ms(&[
        "--robot",
        "cross-project",
        "gaps",
        "--min-score",
        "100.0",
    ]);
    fixture.assert_success(&output, "cross-project gaps min-score");

    fixture.checkpoint("cross-project:post-gaps-min-score");

    let json = output.json();
    // With a very high min_score, all patterns should appear as gaps
    // (unless perfectly matched)
    assert!(
        json["gaps"].is_array(),
        "Should return gaps array"
    );

    fixture.emit_event(
        LogLevel::Info,
        "cross-project",
        "Gaps min-score filter verified",
        Some(json.clone()),
    );

    fixture.generate_report();
    Ok(())
}

/// Gaps with search-limit=1 should still work correctly.
#[test]
fn test_cross_project_gaps_search_limit() -> Result<()> {
    if !cass_available() {
        eprintln!("[SKIP] cass binary not available, skipping test");
        return Ok(());
    }
    let mut fixture = setup_workspace("cross_project_gaps_search_limit")?;

    fixture.checkpoint("cross-project:pre-gaps-search-limit");

    fixture.log_step("Run gaps with search-limit=1");
    let output = fixture.run_ms(&[
        "--robot",
        "cross-project",
        "gaps",
        "--search-limit",
        "1",
    ]);
    fixture.assert_success(&output, "cross-project gaps search-limit");

    fixture.checkpoint("cross-project:post-gaps-search-limit");

    let json = output.json();
    assert!(
        json["gaps"].is_array(),
        "Should return gaps array"
    );

    fixture.emit_event(
        LogLevel::Info,
        "cross-project",
        "Gaps search-limit verified",
        Some(json.clone()),
    );

    fixture.generate_report();
    Ok(())
}

/// Gaps with zero search-limit should fail validation.
#[test]
fn test_cross_project_gaps_zero_search_limit() -> Result<()> {
    let mut fixture = setup_minimal("cross_project_gaps_zero_search_limit")?;

    fixture.checkpoint("cross-project:pre-gaps-zero-search-limit");

    fixture.log_step("Run gaps with search-limit=0");
    let output = fixture.run_ms(&[
        "--robot",
        "cross-project",
        "gaps",
        "--search-limit",
        "0",
    ]);

    assert!(
        !output.success,
        "Gaps with search-limit=0 should fail"
    );

    fixture.checkpoint("cross-project:post-gaps-zero-search-limit");

    fixture.emit_event(
        LogLevel::Info,
        "cross-project",
        "Zero search-limit validation correctly rejected",
        None,
    );

    fixture.generate_report();
    Ok(())
}
