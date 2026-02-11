//! E2E Scenario: Dedup Workflow Integration Tests
//!
//! Comprehensive tests for the `ms dedup` command covering:
//! - Dedup scan detection
//! - Dedup review of a specific pair
//! - Dedup alias creation (resolution)
//! - Dedup scan reporting (status, pair count, filter)

use super::fixture::E2EFixture;
use ms::error::Result;

// Skill definitions intentionally similar to trigger dedup detection

const SKILL_RUST_ERRORS_V1: &str = r#"---
name: Rust Error Handling
description: Best practices for error handling in Rust
tags: [rust, errors, advanced]
---

# Rust Error Handling

Use `Result<T, E>` and propagate errors with `?`.

## Guidelines

- Use thiserror for library errors
- Use anyhow for application errors
- Wrap errors with context
"#;

const SKILL_RUST_ERRORS_V2: &str = r#"---
name: Rust Error Patterns
description: Error handling patterns and best practices in Rust
tags: [rust, errors, patterns, advanced]
---

# Rust Error Patterns

Use `Result<T, E>` and propagate errors with the `?` operator.

## Guidelines

- Prefer thiserror for library error types
- Prefer anyhow for application error types
- Always add context when propagating errors
"#;

const SKILL_GO_TESTING: &str = r#"---
name: Go Testing
description: Testing strategies for Go projects
tags: [go, testing, intermediate]
---

# Go Testing

Use the standard `testing` package.

## Guidelines

- Write table-driven tests
- Use subtests for organization
"#;

/// Create a fixture with near-duplicate skills for dedup testing
fn setup_dedup_fixture(scenario: &str) -> Result<E2EFixture> {
    let mut fixture = E2EFixture::new(scenario);

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    fixture.log_step("Create near-duplicate skills");
    fixture.create_skill("rust-error-handling", SKILL_RUST_ERRORS_V1)?;
    fixture.create_skill("rust-error-patterns", SKILL_RUST_ERRORS_V2)?;
    fixture.create_skill("go-testing", SKILL_GO_TESTING)?;

    fixture.log_step("Index skills");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    // Checkpoint: skills indexed
    fixture.checkpoint("dedup:indexed");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "dedup",
        "Skills indexed for dedup testing",
        Some(serde_json::json!({
            "near_duplicates": ["rust-error-handling", "rust-error-patterns"],
            "distinct": ["go-testing"],
        })),
    );

    Ok(fixture)
}

#[test]
fn test_dedup_scan() -> Result<()> {
    let mut fixture = setup_dedup_fixture("dedup_scan")?;

    // Checkpoint: pre-scan
    fixture.checkpoint("dedup:pre-scan");

    fixture.log_step("Run dedup scan");
    let output = fixture.run_ms(&["--robot", "dedup", "scan"]);
    fixture.assert_success(&output, "dedup scan");

    // Checkpoint: post-scan
    fixture.checkpoint("dedup:post-scan");

    let json = output.json();
    let status = json["status"].as_str().expect("status field");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "dedup",
        &format!("Dedup scan status: {}", status),
        Some(serde_json::json!({
            "status": status,
            "total_pairs": json["total_pairs"],
            "displayed_pairs": json["displayed_pairs"],
        })),
    );

    assert_eq!(status, "ok", "Dedup scan status should be ok");

    // Verify the pairs array exists (may be empty if embeddings are not computed)
    assert!(
        json.get("pairs").is_some(),
        "Response should have 'pairs' field"
    );
    assert!(
        json.get("total_pairs").is_some(),
        "Response should have 'total_pairs' field"
    );

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_dedup_scan_with_threshold() -> Result<()> {
    let mut fixture = setup_dedup_fixture("dedup_scan_threshold")?;

    fixture.log_step("Run dedup scan with low threshold");
    let output = fixture.run_ms(&["--robot", "dedup", "scan", "--threshold", "0.5"]);
    fixture.assert_success(&output, "dedup scan low threshold");

    let json_low = output.json();
    let low_total = json_low["total_pairs"].as_u64().unwrap_or(0);

    fixture.log_step("Run dedup scan with high threshold");
    let output = fixture.run_ms(&["--robot", "dedup", "scan", "--threshold", "0.99"]);
    fixture.assert_success(&output, "dedup scan high threshold");

    let json_high = output.json();
    let high_total = json_high["total_pairs"].as_u64().unwrap_or(0);

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "dedup",
        &format!(
            "Threshold comparison: low={}, high={}",
            low_total, high_total
        ),
        Some(serde_json::json!({
            "low_threshold": 0.5,
            "low_pairs": low_total,
            "high_threshold": 0.99,
            "high_pairs": high_total,
        })),
    );

    // A lower threshold should find at least as many pairs as a higher one
    assert!(
        low_total >= high_total,
        "Lower threshold ({}) should find >= pairs than higher threshold ({})",
        low_total,
        high_total
    );

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_dedup_review() -> Result<()> {
    let mut fixture = setup_dedup_fixture("dedup_review")?;

    fixture.log_step("Review a specific pair of skills");
    let output = fixture.run_ms(&[
        "--robot",
        "dedup",
        "review",
        "rust-error-handling",
        "rust-error-patterns",
    ]);
    fixture.assert_success(&output, "dedup review");

    let json = output.json();
    let status = json["status"].as_str().expect("status field");

    // Verify the review response structure
    assert_eq!(status, "ok", "Review status should be ok");
    assert!(
        json.get("skill_a").is_some(),
        "Response should have 'skill_a'"
    );
    assert!(
        json.get("skill_b").is_some(),
        "Response should have 'skill_b'"
    );

    let skill_a = &json["skill_a"];
    let skill_b = &json["skill_b"];

    assert_eq!(
        skill_a["id"].as_str(),
        Some("rust-error-handling"),
        "Skill A should be rust-error-handling"
    );
    assert_eq!(
        skill_b["id"].as_str(),
        Some("rust-error-patterns"),
        "Skill B should be rust-error-patterns"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "dedup",
        "Dedup review completed",
        Some(serde_json::json!({
            "skill_a": skill_a["id"],
            "skill_b": skill_b["id"],
        })),
    );

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_dedup_alias() -> Result<()> {
    let mut fixture = setup_dedup_fixture("dedup_alias")?;

    // Checkpoint: pre-alias
    fixture.checkpoint("dedup:pre-alias");

    fixture.log_step("Create alias for duplicate skill");
    let output = fixture.run_ms(&[
        "--robot",
        "dedup",
        "alias",
        "rust-error-handling",
        "rust-errors",
    ]);
    fixture.assert_success(&output, "dedup alias");

    // Checkpoint: post-alias
    fixture.checkpoint("dedup:post-alias");

    let json = output.json();
    let status = json["status"].as_str().expect("status");
    let action = json["action"].as_str().expect("action");

    assert_eq!(status, "ok", "Alias status should be ok");
    assert_eq!(action, "alias", "Action should be alias");
    assert_eq!(
        json["canonical"]["id"].as_str(),
        Some("rust-error-handling"),
        "Canonical should be rust-error-handling"
    );
    assert_eq!(
        json["alias"].as_str(),
        Some("rust-errors"),
        "Alias should be rust-errors"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "dedup",
        "Alias created successfully",
        Some(serde_json::json!({
            "canonical": "rust-error-handling",
            "alias": "rust-errors",
        })),
    );

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_dedup_scan_reporting() -> Result<()> {
    let mut fixture = setup_dedup_fixture("dedup_scan_reporting")?;

    fixture.log_step("Run dedup scan with limit");
    let output = fixture.run_ms(&["--robot", "dedup", "scan", "--limit", "5"]);
    fixture.assert_success(&output, "dedup scan with limit");

    let json = output.json();

    // Verify response structure for reporting
    assert!(
        json.get("status").is_some(),
        "Response should have 'status'"
    );
    assert!(
        json.get("total_pairs").is_some(),
        "Response should have 'total_pairs'"
    );
    assert!(
        json.get("displayed_pairs").is_some(),
        "Response should have 'displayed_pairs'"
    );
    assert!(
        json.get("pairs").is_some(),
        "Response should have 'pairs'"
    );

    let displayed = json["displayed_pairs"].as_u64().unwrap_or(0);
    assert!(
        displayed <= 5,
        "Displayed pairs should respect limit of 5, got {}",
        displayed
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "dedup",
        "Dedup scan reporting verified",
        Some(serde_json::json!({
            "total_pairs": json["total_pairs"],
            "displayed_pairs": displayed,
            "limit": 5,
        })),
    );

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_dedup_review_nonexistent_skill() -> Result<()> {
    let mut fixture = setup_dedup_fixture("dedup_review_nonexistent")?;

    fixture.log_step("Review with a nonexistent skill");
    let output = fixture.run_ms(&[
        "--robot",
        "dedup",
        "review",
        "rust-error-handling",
        "nonexistent-skill",
    ]);

    // This should fail because the skill does not exist
    assert!(
        !output.success,
        "Reviewing a nonexistent skill should fail"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "dedup",
        "Nonexistent skill review correctly failed",
        Some(serde_json::json!({
            "exit_code": output.exit_code,
            "expected": "failure",
        })),
    );

    fixture.generate_report();
    Ok(())
}
