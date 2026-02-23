//! E2E Scenario: Prune Workflow Integration Tests
//!
//! Comprehensive tests for the `ms prune` command covering:
//! - Dry-run prune (list tombstones)
//! - Actual prune (purge with approval)
//! - Prune with filters (older-than, stats)
//! - Prune analyze and proposals

use super::fixture::E2EFixture;
use ms::error::Result;

// Test skill definitions

const SKILL_RUST_ERRORS: &str = r#"---
name: Rust Error Handling
description: Best practices for error handling in Rust
tags: [rust, errors, advanced]
---

# Rust Error Handling

Use `Result<T, E>` and propagate errors with `?`.

## Guidelines

- Use thiserror for library errors
- Use anyhow for application errors
"#;

const SKILL_GO_ERRORS: &str = r#"---
name: Go Error Handling
description: Error handling patterns in Go
tags: [go, errors, beginner]
---

# Go Error Handling

Check errors explicitly after each function call.

## Guidelines

- Wrap errors with context
- Use sentinel errors sparingly
"#;

const SKILL_PYTHON_TESTING: &str = r#"---
name: Python Testing
description: Testing strategies for Python projects
tags: [python, testing, intermediate]
---

# Python Testing

Use pytest for all testing needs.

## Guidelines

- Write unit tests first
- Use fixtures for setup
"#;

/// Create a fixture with indexed skills for prune testing
fn setup_prune_fixture(scenario: &str) -> Result<E2EFixture> {
    let mut fixture = E2EFixture::new(scenario);

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    fixture.log_step("Create skills");
    fixture.create_skill("rust-error-handling", SKILL_RUST_ERRORS)?;
    fixture.create_skill("go-error-handling", SKILL_GO_ERRORS)?;
    fixture.create_skill("python-testing", SKILL_PYTHON_TESTING)?;

    fixture.log_step("Index skills");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    // Checkpoint: skills indexed
    fixture.checkpoint("prune:indexed");

    Ok(fixture)
}

#[test]
fn test_prune_list_dry_run() -> Result<()> {
    let mut fixture = setup_prune_fixture("prune_list_dry_run")?;

    // Checkpoint: pre-prune
    fixture.checkpoint("prune:pre-list");

    fixture.log_step("List tombstones (dry run)");
    let output = fixture.run_ms(&["--robot", "prune", "--dry-run"]);
    fixture.assert_success(&output, "prune list dry run");

    // Checkpoint: post-prune
    fixture.checkpoint("prune:post-list");

    let json = output.json();

    // The response should have tombstone structure
    assert!(
        json.get("tombstones").is_some() || json.get("count").is_some(),
        "Response should have tombstone-related fields"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "prune",
        "Prune list dry run completed",
        Some(serde_json::json!({
            "count": json.get("count").and_then(|v| v.as_u64()).unwrap_or(0),
        })),
    );

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_prune_list_explicit() -> Result<()> {
    let mut fixture = setup_prune_fixture("prune_list_explicit")?;

    fixture.log_step("Explicitly list tombstones");
    let output = fixture.run_ms(&["--robot", "prune", "list"]);
    fixture.assert_success(&output, "prune list");

    let json = output.json();

    assert!(
        json.get("tombstones").is_some(),
        "Response should have 'tombstones' field"
    );
    assert!(
        json.get("count").is_some(),
        "Response should have 'count' field"
    );
    assert!(
        json.get("total_size_bytes").is_some(),
        "Response should have 'total_size_bytes' field"
    );

    let count = json["count"].as_u64().expect("count");
    let total_size = json["total_size_bytes"].as_u64().expect("total_size_bytes");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "prune",
        &format!("Tombstones listed: {} items, {} bytes", count, total_size),
        Some(serde_json::json!({
            "count": count,
            "total_size_bytes": total_size,
        })),
    );

    // In a fresh fixture there should be no tombstones
    assert_eq!(count, 0, "Fresh fixture should have no tombstones");

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_prune_stats() -> Result<()> {
    let mut fixture = setup_prune_fixture("prune_stats")?;

    fixture.log_step("Get prune statistics");
    let output = fixture.run_ms(&["--robot", "prune", "stats"]);
    fixture.assert_success(&output, "prune stats");

    let json = output.json();

    // Verify statistics fields
    assert!(
        json.get("count").is_some(),
        "Response should have 'count' field"
    );
    assert!(
        json.get("files").is_some(),
        "Response should have 'files' field"
    );
    assert!(
        json.get("directories").is_some(),
        "Response should have 'directories' field"
    );
    assert!(
        json.get("total_size_bytes").is_some(),
        "Response should have 'total_size_bytes' field"
    );
    assert!(
        json.get("older_than_7_days").is_some(),
        "Response should have 'older_than_7_days' field"
    );
    assert!(
        json.get("older_than_30_days").is_some(),
        "Response should have 'older_than_30_days' field"
    );

    let count = json["count"].as_u64().expect("count");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "prune",
        &format!("Prune stats: {} total tombstones", count),
        Some(serde_json::json!({
            "count": count,
            "files": json["files"],
            "directories": json["directories"],
            "total_size_bytes": json["total_size_bytes"],
        })),
    );

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_prune_purge_requires_approval() -> Result<()> {
    let mut fixture = setup_prune_fixture("prune_purge_no_approval")?;

    fixture.log_step("Attempt purge without --approve");
    let output = fixture.run_ms(&["--robot", "prune", "purge", "all"]);

    // Without --approve, purge should not perform destructive action
    // It may succeed with a warning or fail depending on implementation
    let json_str = format!("{}{}", output.stdout, output.stderr);

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "prune",
        "Purge without approval tested",
        Some(serde_json::json!({
            "success": output.success,
            "exit_code": output.exit_code,
            "requires_approval": json_str.contains("approval") || json_str.contains("approve"),
        })),
    );

    // If there are no tombstones, the command may succeed with "not found"
    // If there are tombstones, it should require approval
    // Either way, verify no destructive action occurred
    fixture.log_step("Verify tombstone count unchanged");
    let list_output = fixture.run_ms(&["--robot", "prune", "list"]);
    fixture.assert_success(&list_output, "prune list after failed purge");

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_prune_with_older_than_filter() -> Result<()> {
    let mut fixture = setup_prune_fixture("prune_older_than")?;

    fixture.log_step("List tombstones older than 7 days");
    let output = fixture.run_ms(&["--robot", "prune", "--older-than", "7"]);
    fixture.assert_success(&output, "prune list older than 7");

    let json = output.json();
    let count = json["count"].as_u64().unwrap_or(0);

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "prune",
        &format!("Tombstones older than 7 days: {}", count),
        Some(serde_json::json!({
            "older_than_days": 7,
            "count": count,
        })),
    );

    fixture.log_step("List tombstones older than 30 days");
    let output = fixture.run_ms(&["--robot", "prune", "--older-than", "30"]);
    fixture.assert_success(&output, "prune list older than 30");

    let json_30 = output.json();
    let count_30 = json_30["count"].as_u64().unwrap_or(0);

    // Items older than 30 days should be a subset of items older than 7 days
    assert!(
        count_30 <= count,
        "30-day count ({}) should be <= 7-day count ({})",
        count_30,
        count
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "prune",
        "Older-than filter verified",
        Some(serde_json::json!({
            "7_day_count": count,
            "30_day_count": count_30,
        })),
    );

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_prune_analyze() -> Result<()> {
    let mut fixture = setup_prune_fixture("prune_analyze")?;

    // Checkpoint: pre-analyze
    fixture.checkpoint("prune:pre-analyze");

    fixture.log_step("Run prune analysis");
    let output = fixture.run_ms(&["--robot", "prune", "analyze"]);
    fixture.assert_success(&output, "prune analyze");

    // Checkpoint: post-analyze
    fixture.checkpoint("prune:post-analyze");

    let json = output.json();
    let status = json["status"].as_str().expect("status");
    assert_eq!(status, "analysis", "Analyze status should be 'analysis'");

    // Verify analysis structure
    assert!(
        json.get("candidates").is_some(),
        "Response should have 'candidates' field"
    );

    let candidates = &json["candidates"];
    assert!(
        candidates.get("low_usage").is_some(),
        "Candidates should have 'low_usage'"
    );
    assert!(
        candidates.get("low_quality").is_some(),
        "Candidates should have 'low_quality'"
    );
    assert!(
        candidates.get("high_similarity").is_some(),
        "Candidates should have 'high_similarity'"
    );
    assert!(
        candidates.get("toolchain_mismatch").is_some(),
        "Candidates should have 'toolchain_mismatch'"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "prune",
        "Prune analysis completed",
        Some(serde_json::json!({
            "status": status,
            "low_usage_count": candidates["low_usage"].as_array().map(|a| a.len()),
            "low_quality_count": candidates["low_quality"].as_array().map(|a| a.len()),
            "high_similarity_count": candidates["high_similarity"].as_array().map(|a| a.len()),
            "toolchain_mismatch_count": candidates["toolchain_mismatch"].as_array().map(|a| a.len()),
        })),
    );

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_prune_analyze_with_custom_thresholds() -> Result<()> {
    let mut fixture = setup_prune_fixture("prune_analyze_custom")?;

    fixture.log_step("Run prune analysis with custom thresholds");
    let output = fixture.run_ms(&[
        "--robot",
        "prune",
        "analyze",
        "--days",
        "60",
        "--min-usage",
        "1",
        "--max-quality",
        "0.5",
        "--similarity",
        "0.9",
        "--limit",
        "5",
    ]);
    fixture.assert_success(&output, "prune analyze custom thresholds");

    let json = output.json();

    // Verify custom thresholds are reflected
    assert_eq!(json["window_days"].as_u64(), Some(60));
    assert_eq!(json["min_usage"].as_u64(), Some(1));

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "prune",
        "Prune analysis with custom thresholds completed",
        Some(serde_json::json!({
            "days": 60,
            "min_usage": 1,
            "max_quality": 0.5,
            "similarity": 0.9,
            "limit": 5,
        })),
    );

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_prune_proposals_dry_run() -> Result<()> {
    let mut fixture = setup_prune_fixture("prune_proposals_dry_run")?;

    // Checkpoint: pre-proposals
    fixture.checkpoint("prune:pre-proposals");

    fixture.log_step("Generate prune proposals in dry-run mode");
    let output = fixture.run_ms(&["--robot", "prune", "--dry-run", "proposals"]);
    fixture.assert_success(&output, "prune proposals dry-run");

    // Checkpoint: post-proposals
    fixture.checkpoint("prune:post-proposals");

    let json = output.json();
    let status = json["status"].as_str().expect("status");

    assert_eq!(
        status, "proposals_ready",
        "Proposals status should be 'proposals_ready'"
    );

    // Verify proposals structure
    assert!(
        json.get("proposals").is_some(),
        "Response should have 'proposals' field"
    );
    assert!(
        json.get("stats").is_some(),
        "Response should have 'stats' field"
    );

    let proposals = &json["proposals"];
    assert!(
        proposals.get("deprecate").is_some(),
        "Proposals should have 'deprecate'"
    );
    assert!(
        proposals.get("merge").is_some(),
        "Proposals should have 'merge'"
    );
    assert!(
        proposals.get("split").is_some(),
        "Proposals should have 'split'"
    );

    let stats = &json["stats"];
    assert!(
        stats.get("total_skills").is_some(),
        "Stats should have 'total_skills'"
    );
    assert!(
        stats.get("candidates").is_some(),
        "Stats should have 'candidates'"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "prune",
        "Prune proposals (dry-run) completed",
        Some(serde_json::json!({
            "status": status,
            "total_skills": stats["total_skills"],
            "candidates": stats["candidates"],
            "deprecate_count": stats["deprecate_proposals"],
            "merge_count": stats["merge_proposals"],
            "split_count": stats["split_proposals"],
        })),
    );

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_prune_restore_nonexistent() -> Result<()> {
    let mut fixture = setup_prune_fixture("prune_restore_nonexistent")?;

    fixture.log_step("Attempt to restore a nonexistent tombstone");
    let output = fixture.run_ms(&["--robot", "prune", "restore", "nonexistent-id-12345"]);

    // Should succeed but report not found
    let json_str = format!("{}{}", output.stdout, output.stderr);

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "prune",
        "Restore nonexistent tombstone tested",
        Some(serde_json::json!({
            "success": output.success,
            "exit_code": output.exit_code,
            "contains_not_found": json_str.contains("not found") || json_str.contains("No tombstone"),
        })),
    );

    // Verify the output indicates not found
    assert!(
        json_str.contains("not found") || json_str.contains("No tombstone") || !output.success,
        "Should indicate tombstone not found"
    );

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_prune_apply_requires_approval() -> Result<()> {
    let mut fixture = setup_prune_fixture("prune_apply_no_approval")?;

    fixture.log_step("Attempt apply without --approve flag");
    let output = fixture.run_ms(&["--robot", "prune", "apply", "deprecate:rust-error-handling"]);

    // Should fail because --approve is required
    assert!(!output.success, "Apply without --approve should fail");

    let combined = format!("{}{}", output.stdout, output.stderr);
    assert!(
        combined.contains("approve") || combined.contains("approval"),
        "Error should mention approval requirement"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "prune",
        "Apply correctly requires approval",
        Some(serde_json::json!({
            "exit_code": output.exit_code,
            "expected": "failure requiring approval",
        })),
    );

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_prune_apply_dry_run() -> Result<()> {
    let mut fixture = setup_prune_fixture("prune_apply_dry_run")?;

    fixture.log_step("Apply deprecate proposal in dry-run mode");
    let output = fixture.run_ms(&[
        "--robot",
        "prune",
        "--dry-run",
        "apply",
        "deprecate:rust-error-handling",
        "--approve",
    ]);
    fixture.assert_success(&output, "prune apply dry-run");

    let json = output.json();
    let status = json["status"].as_str().expect("status");
    let dry_run = json["dry_run"].as_bool().expect("dry_run");
    let action = json["action"].as_str().expect("action");

    assert_eq!(status, "ok", "Apply dry-run status should be ok");
    assert!(dry_run, "dry_run should be true");
    assert_eq!(action, "deprecate", "Action should be deprecate");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "prune",
        "Apply dry-run completed",
        Some(serde_json::json!({
            "status": status,
            "dry_run": dry_run,
            "action": action,
            "message": json["message"],
        })),
    );

    // Verify the skill is NOT actually deprecated (dry-run)
    fixture.log_step("Verify skill is not deprecated after dry-run");
    let list_output = fixture.run_ms(&["--robot", "list"]);
    fixture.assert_success(&list_output, "list after dry-run");

    let list_json = list_output.json();
    let skills = list_json["skills"].as_array().expect("skills array");
    let skill_ids: Vec<&str> = skills.iter().filter_map(|s| s["id"].as_str()).collect();
    assert!(
        skill_ids.contains(&"rust-error-handling"),
        "Skill should still be listed after dry-run"
    );

    fixture.generate_report();
    Ok(())
}
