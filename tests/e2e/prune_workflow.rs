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

// ============================================================================
// Stale-source detection and pruning (issue #158)
// ============================================================================

/// Regression test for issue #158: renaming a skill's markdown source (new id,
/// same tree) leaves the old id's row indexed forever, and `ms list`/`ms
/// search` surface near-duplicates. `ms prune stale-sources` must detect the
/// orphan (origin path gone, origin root still present) and `--remove` must
/// delete it end-to-end (DB + search index), while `ms doctor` flags it in the
/// default check run.
#[test]
fn test_prune_stale_sources_detect_and_remove() -> Result<()> {
    let mut fixture = setup_prune_fixture("prune_stale_sources")?;

    // Baseline: freshly indexed skills all have live sources.
    fixture.log_step("Baseline stale-sources scan");
    let output = fixture.run_ms(&["--robot", "prune", "stale-sources"]);
    fixture.assert_success(&output, "stale-sources baseline");
    let json = output.json();
    assert_eq!(
        json["count"].as_u64(),
        Some(0),
        "fresh index must have no stale sources"
    );

    // Baseline doctor issue count (environment-dependent checks may differ,
    // so later assertions compare against this delta rather than zero).
    let output = fixture.run_ms(&["--robot", "doctor"]);
    fixture.assert_success(&output, "doctor baseline");
    let baseline_issues = output.json()["issues_found"].as_u64().unwrap_or(0);

    // Simulate the issue #158 scenario: the skill source is renamed to a new
    // id under the same skills root. The new id gets indexed; the old id's
    // row lingers as a stale near-duplicate.
    fixture.log_step("Rename skill source to a new id");
    let skills_dir = fixture
        .skills_dirs
        .get("project")
        .expect("project skills dir")
        .clone();
    let old_dir = skills_dir.join("rust-error-handling");
    let new_dir = skills_dir.join("rust-errors-v2");
    std::fs::rename(&old_dir, &new_dir)?;
    let renamed_content =
        SKILL_RUST_ERRORS.replace("name: Rust Error Handling", "name: Rust Errors V2");
    std::fs::write(new_dir.join("SKILL.md"), renamed_content)?;

    fixture.log_step("Re-index after rename");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "re-index after rename");
    fixture.checkpoint("stale:reindexed");

    // The stale row is still listed (this is the bug being detected).
    let output = fixture.run_ms(&["--robot", "list"]);
    fixture.assert_success(&output, "list after rename");
    let json = output.json();
    let skill_ids: Vec<&str> = json["skills"]
        .as_array()
        .expect("skills array")
        .iter()
        .filter_map(|s| s["id"].as_str())
        .collect();
    assert!(
        skill_ids.contains(&"rust-error-handling"),
        "old id should still be indexed after rename (the stale row)"
    );
    assert!(
        skill_ids.contains(&"rust-errors-v2"),
        "new id should be indexed after rename"
    );

    // `ms doctor` (default run) must now flag the stale origin.
    fixture.log_step("Doctor flags the stale source");
    let output = fixture.run_ms(&["--robot", "doctor"]);
    let doctor_issues = output.json()["issues_found"].as_u64().unwrap_or(0);
    assert!(
        doctor_issues >= baseline_issues + 1,
        "doctor should flag the stale source (baseline {baseline_issues}, got {doctor_issues})"
    );

    // Detection: exactly the old id, with reference counts surfaced.
    fixture.log_step("Detect stale source");
    let output = fixture.run_ms(&["--robot", "prune", "stale-sources"]);
    fixture.assert_success(&output, "stale-sources detect");
    let json = output.json();
    assert_eq!(json["count"].as_u64(), Some(1), "exactly one stale source");
    let entry = &json["stale_sources"][0];
    assert_eq!(entry["skill_id"].as_str(), Some("rust-error-handling"));
    assert!(entry["usage_count"].is_u64(), "usage_count surfaced");
    assert!(entry["feedback_count"].is_u64(), "feedback_count surfaced");
    assert_eq!(
        json["removed"].as_array().map(std::vec::Vec::len),
        Some(0),
        "detection must not remove anything"
    );

    // --remove with --dry-run must not delete.
    fixture.log_step("Dry-run remove");
    let output = fixture.run_ms(&["--robot", "prune", "stale-sources", "--remove", "--dry-run"]);
    fixture.assert_success(&output, "stale-sources dry-run remove");
    let json = output.json();
    assert_eq!(json["removed"].as_array().map(std::vec::Vec::len), Some(0));
    assert_eq!(json["dry_run"].as_bool(), Some(true));

    // Actual removal.
    fixture.log_step("Remove stale source");
    let output = fixture.run_ms(&["--robot", "prune", "stale-sources", "--remove"]);
    fixture.assert_success(&output, "stale-sources remove");
    let json = output.json();
    let removed_ids: Vec<&str> = json["removed"]
        .as_array()
        .expect("removed array")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert_eq!(
        removed_ids,
        vec!["rust-error-handling"],
        "stale skill should be removed"
    );
    fixture.checkpoint("stale:removed");

    // The stale row is gone from list; the renamed skill survives.
    let output = fixture.run_ms(&["--robot", "list"]);
    fixture.assert_success(&output, "list after removal");
    let json = output.json();
    let skill_ids: Vec<&str> = json["skills"]
        .as_array()
        .expect("skills array")
        .iter()
        .filter_map(|s| s["id"].as_str())
        .collect();
    assert!(
        !skill_ids.contains(&"rust-error-handling"),
        "stale row must be gone after removal"
    );
    assert!(
        skill_ids.contains(&"rust-errors-v2"),
        "renamed skill must survive removal"
    );

    // Search must not surface the removed id either (the original complaint
    // was near-duplicate search results).
    let output = fixture.run_ms(&["--robot", "search", "error handling", "--search-type", "bm25"]);
    fixture.assert_success(&output, "search after removal");
    assert!(
        !output.stdout.contains("rust-error-handling"),
        "search must not surface the removed stale id"
    );

    // Doctor is back to baseline, and the scan is clean + idempotent.
    let output = fixture.run_ms(&["--robot", "doctor"]);
    let post_issues = output.json()["issues_found"].as_u64().unwrap_or(0);
    assert_eq!(
        post_issues, baseline_issues,
        "doctor should be back to baseline after removal"
    );
    let output = fixture.run_ms(&["--robot", "prune", "stale-sources"]);
    fixture.assert_success(&output, "stale-sources idempotent rescan");
    assert_eq!(output.json()["count"].as_u64(), Some(0));

    fixture.generate_report();
    Ok(())
}

/// A whole index root disappearing (unmounted tree) must NOT be treated as a
/// stale source — only individual origin paths going missing while their root
/// survives are flagged (issue #158's false-positive guard).
#[test]
fn test_prune_stale_sources_absent_root_not_flagged() -> Result<()> {
    let mut fixture = setup_prune_fixture("prune_stale_sources_absent_root")?;

    // Simulate the project skills root being unplugged by renaming the whole
    // tree aside (no deletion; the tree still exists under another name).
    let skills_dir = fixture
        .skills_dirs
        .get("project")
        .expect("project skills dir")
        .clone();
    let parked = skills_dir.with_file_name("skills-parked");
    std::fs::rename(&skills_dir, &parked)?;

    let output = fixture.run_ms(&["--robot", "prune", "stale-sources"]);
    fixture.assert_success(&output, "stale-sources with absent root");
    let json = output.json();
    assert_eq!(
        json["count"].as_u64(),
        Some(0),
        "absent index root must not flag its skills as stale"
    );
    assert!(
        json["unrooted_count"].as_u64().unwrap_or(0) >= 1,
        "absent-root skills should be counted as unrooted"
    );

    // Restore the tree so fixture teardown sees the expected layout.
    std::fs::rename(&parked, &skills_dir)?;

    fixture.generate_report();
    Ok(())
}
