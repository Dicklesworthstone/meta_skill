//! E2E Scenario: Cross-Project Workflow Integration Tests
//!
//! Comprehensive tests for the `ms cross-project` command covering:
//! - Summary subcommand (aggregation by project, filters, limits, `--include-unknown`)
//! - Patterns subcommand (cross-project pattern extraction, thresholds, `--include-unknown`)
//! - Gaps subcommand (coverage gap analysis, `--min-score`, `--search-limit`, `--include-unknown`)
//! - Validation errors (zero limits, zero min-projects, zero search-limit)
//! - CASS unavailable error handling
//! - JSON output format verification
//!
//! All CASS-backed tests run against a deterministic stand-in `cass` script
//! passed via `MS_CASS_PATH`, so the suite never spawns `cass` from `PATH` and
//! never reads the caller's real agent session history.

use std::fs;
use std::path::PathBuf;

use serde_json::json;

use super::fixture::{CommandOutput, E2EFixture, LogLevel};
use ms::error::Result;

// ============================================================================
// Deterministic Stand-In CASS Fixture
// ============================================================================

/// Build a deterministic stand-in `cass` executable inside `fixture.root`
/// serving 5 synthetic sessions across 3 named projects plus 1 session without
/// a `project` field:
///
/// - `/sessions/alpha-1.jsonl` (`proj-alpha`): `kubectl rollout status deploy/api`
/// - `/sessions/beta-1.jsonl`  (`proj-beta`):  `kubectl rollout status deploy/worker`
/// - `/sessions/gamma-1.jsonl` (`proj-gamma`): `kubectl rollout status deploy/gateway`
/// - `/sessions/alpha-2.jsonl` (`proj-alpha`): `helm upgrade --install api ./chart`
/// - `/sessions/unknown-1.jsonl` (no `project`): `helm upgrade --install jobs ./chart`
///
/// This layout satisfies default `--min-occurrences 2` / `--min-projects 2` for
/// `Commands: kubectl` even with `--limit 2`, exercises `--top` truncation (3 -> 2),
/// query narrowing (`--query rust` returns only `proj-alpha` and `proj-beta`),
/// `--min-sessions` filtering (`proj-alpha` has 2 sessions; others have 1), and
/// `--include-unknown` (`Commands: helm` only crosses 2 projects when `unknown`
/// is included alongside `proj-alpha`).
fn stand_in_cass(fixture: &E2EFixture) -> Result<PathBuf> {
    let data_dir = fixture.root.join("stand_in_cass_data");
    fs::create_dir_all(&data_dir)?;

    let sessions: &[(&str, &str, &str)] = &[
        (
            "alpha-1.json",
            "Checking API deployment rollout status.",
            "kubectl rollout status deploy/api",
        ),
        (
            "beta-1.json",
            "Checking worker deployment rollout status.",
            "kubectl rollout status deploy/worker",
        ),
        (
            "gamma-1.json",
            "Checking gateway deployment rollout status.",
            "kubectl rollout status deploy/gateway",
        ),
        (
            "alpha-2.json",
            "Upgrading API helm release.",
            "helm upgrade --install api ./chart",
        ),
        (
            "unknown-1.json",
            "Upgrading jobs helm release.",
            "helm upgrade --install jobs ./chart",
        ),
    ];

    for (filename, content, command) in sessions {
        let payload = json!([
            {
                "role": "assistant",
                "content": content,
                "tool_calls": [{
                    "id": "1",
                    "name": "Bash",
                    "arguments": { "command": command }
                }]
            },
            {
                "role": "user",
                "content": "Looks good, verified."
            }
        ]);
        fs::write(data_dir.join(filename), payload.to_string())?;
    }

    let hit_alpha_1 = json!({
        "source_path": "/sessions/alpha-1.jsonl",
        "project": "proj-alpha",
        "timestamp": "2026-01-10T10:00:00Z"
    });
    let hit_beta_1 = json!({
        "source_path": "/sessions/beta-1.jsonl",
        "project": "proj-beta",
        "timestamp": "2026-01-12T10:00:00Z"
    });
    let hit_gamma_1 = json!({
        "source_path": "/sessions/gamma-1.jsonl",
        "project": "proj-gamma",
        "timestamp": "2026-01-14T10:00:00Z"
    });
    let hit_alpha_2 = json!({
        "source_path": "/sessions/alpha-2.jsonl",
        "project": "proj-alpha",
        "timestamp": "2026-01-15T10:00:00Z"
    });
    let hit_unknown_1 = json!({
        "source_path": "/sessions/unknown-1.jsonl",
        "timestamp": "2026-01-16T10:00:00Z"
    });

    let search_all_1 = json!({ "hits": [&hit_alpha_1] });
    let search_all_2 = json!({ "hits": [&hit_alpha_1, &hit_beta_1] });
    let search_all_5 = json!({
        "hits": [&hit_alpha_1, &hit_beta_1, &hit_gamma_1, &hit_alpha_2, &hit_unknown_1]
    });
    let search_rust = json!({ "hits": [&hit_alpha_1, &hit_beta_1] });

    fs::write(data_dir.join("search_1.json"), search_all_1.to_string())?;
    fs::write(data_dir.join("search_2.json"), search_all_2.to_string())?;
    fs::write(data_dir.join("search_all.json"), search_all_5.to_string())?;
    fs::write(data_dir.join("search_rust.json"), search_rust.to_string())?;

    let cass = fixture.root.join("stand-in-cass");
    let script = format!(
        r#"#!/bin/sh
DATA_DIR='{data}'
case "$1" in
  --version)
    echo 'cass 0.6.0'
    ;;
  search)
    QUERY="$2"
    LIMIT="100"
    shift 2
    while [ $# -gt 0 ]; do
      if [ "$1" = "--limit" ] && [ $# -ge 2 ]; then
        LIMIT="$2"
        shift 2
      else
        shift
      fi
    done
    if [ "$QUERY" = "rust" ]; then
      cat "$DATA_DIR/search_rust.json"
    elif [ "$LIMIT" = "1" ]; then
      cat "$DATA_DIR/search_1.json"
    elif [ "$LIMIT" = "2" ]; then
      cat "$DATA_DIR/search_2.json"
    else
      cat "$DATA_DIR/search_all.json"
    fi
    ;;
  export)
    case "$2" in
      /sessions/alpha-1.jsonl)   cat "$DATA_DIR/alpha-1.json" ;;
      /sessions/beta-1.jsonl)    cat "$DATA_DIR/beta-1.json" ;;
      /sessions/gamma-1.jsonl)   cat "$DATA_DIR/gamma-1.json" ;;
      /sessions/alpha-2.jsonl)   cat "$DATA_DIR/alpha-2.json" ;;
      /sessions/unknown-1.jsonl) cat "$DATA_DIR/unknown-1.json" ;;
      *) echo "unknown session: $2" >&2; exit 1 ;;
    esac
    ;;
  *)
    echo "unsupported subcommand: $1" >&2
    exit 1
    ;;
esac
"#,
        data = data_dir.display()
    );
    fs::write(&cass, script)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&cass, fs::Permissions::from_mode(0o755))?;
    }

    Ok(cass)
}

/// Run `ms` against the deterministic stand-in `cass` binary via `MS_CASS_PATH`.
fn run_ms_with_stand_in_cass(fixture: &mut E2EFixture, args: &[&str]) -> Result<CommandOutput> {
    let cass = stand_in_cass(fixture)?;
    let cass_str = cass.to_str().expect("stand-in cass path is valid UTF-8");
    Ok(fixture.run_ms_with_env(args, &[("MS_CASS_PATH", cass_str)]))
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

const SKILL_KUBECTL: &str = r#"---
name: Kubectl Rollout Operations
description: Operating Kubernetes rollouts and commands with kubectl
tags: [kubectl, commands, kubernetes]
---

# Kubectl Rollout Operations

Guidelines for verifying Kubernetes rollouts with kubectl commands.

## Rules

- Always verify deployment rollout status with kubectl rollout status
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

    assert!(!output.success, "Summary with limit=0 should fail");

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

    assert!(!output.success, "Patterns with limit=0 should fail");

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

    assert!(!output.success, "Patterns with min-projects=0 should fail");

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

    assert!(!output.success, "Gaps with limit=0 should fail");

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
    let output = fixture.run_ms(&["--robot", "cross-project", "gaps", "--min-projects", "0"]);

    assert!(!output.success, "Gaps with min-projects=0 should fail");

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

    assert!(!output.success, "Summary with unavailable CASS should fail");

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

    assert!(!output.success, "Gaps with unavailable CASS should fail");

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
// Summary Workflow Tests (deterministic stand-in CASS)
// ============================================================================

/// Summary with default args aggregates project counts and excludes unknown hits.
#[test]
fn test_cross_project_summary_json_output() -> Result<()> {
    let mut fixture = setup_workspace("cross_project_summary_json")?;

    fixture.checkpoint("cross-project:pre-summary");

    fixture.log_step("Run cross-project summary with robot mode");
    let output = run_ms_with_stand_in_cass(&mut fixture, &["--robot", "cross-project", "summary"])?;
    fixture.assert_success(&output, "cross-project summary");

    fixture.checkpoint("cross-project:post-summary");

    let json = output.json();
    assert_eq!(json["query"].as_str(), Some("*"));
    assert_eq!(json["total_sessions"].as_u64(), Some(4), "{json}");
    assert_eq!(json["total_projects"].as_u64(), Some(3), "{json}");

    let projects = json["projects"].as_array().expect("projects array");
    assert_eq!(projects.len(), 3, "{json}");
    assert_eq!(projects[0]["project"].as_str(), Some("proj-alpha"));
    assert_eq!(projects[0]["sessions"].as_u64(), Some(2));
    assert!(
        projects
            .iter()
            .all(|p| p["project"].as_str() != Some("unknown")),
        "unknown project must be excluded without --include-unknown: {json}"
    );

    fixture.emit_event(
        LogLevel::Info,
        "cross-project",
        "Summary aggregated 4 sessions across 3 projects",
        Some(json.clone()),
    );

    fixture.generate_report();
    Ok(())
}

/// Summary with --top limit restricts output from 3 projects down to 2.
#[test]
fn test_cross_project_summary_top_limit() -> Result<()> {
    let mut fixture = setup_workspace("cross_project_summary_top_limit")?;

    fixture.checkpoint("cross-project:pre-summary-top");

    fixture.log_step("Run summary with --top 2");
    let output = run_ms_with_stand_in_cass(
        &mut fixture,
        &["--robot", "cross-project", "summary", "--top", "2"],
    )?;
    fixture.assert_success(&output, "cross-project summary top");

    fixture.checkpoint("cross-project:post-summary-top");

    let json = output.json();
    let projects = json["projects"].as_array().expect("projects array");
    assert_eq!(projects.len(), 2, "Should truncate 3 projects to 2: {json}");
    assert_eq!(json["total_projects"].as_u64(), Some(2), "{json}");
    assert_eq!(projects[0]["project"].as_str(), Some("proj-alpha"));
    assert_eq!(projects[0]["sessions"].as_u64(), Some(2));

    fixture.emit_event(
        LogLevel::Info,
        "cross-project",
        "Summary top limit verified",
        Some(serde_json::json!({ "returned": projects.len(), "limit": 2 })),
    );

    fixture.generate_report();
    Ok(())
}

/// Summary with query filter narrows results to matching projects.
#[test]
fn test_cross_project_summary_with_query() -> Result<()> {
    let mut fixture = setup_workspace("cross_project_summary_query")?;

    fixture.checkpoint("cross-project:pre-summary-query");

    fixture.log_step("Run summary with specific query");
    let output = run_ms_with_stand_in_cass(
        &mut fixture,
        &["--robot", "cross-project", "summary", "--query", "rust"],
    )?;
    fixture.assert_success(&output, "cross-project summary with query");

    fixture.checkpoint("cross-project:post-summary-query");

    let json = output.json();
    assert_eq!(
        json["query"].as_str(),
        Some("rust"),
        "Query should be preserved in output"
    );
    assert_eq!(json["total_sessions"].as_u64(), Some(2), "{json}");
    assert_eq!(json["total_projects"].as_u64(), Some(2), "{json}");
    let names: Vec<&str> = json["projects"]
        .as_array()
        .expect("projects array")
        .iter()
        .filter_map(|p| p["project"].as_str())
        .collect();
    assert!(names.contains(&"proj-alpha"), "{json}");
    assert!(names.contains(&"proj-beta"), "{json}");
    assert!(!names.contains(&"proj-gamma"), "{json}");

    fixture.emit_event(
        LogLevel::Info,
        "cross-project",
        "Summary with query filter verified",
        Some(json.clone()),
    );

    fixture.generate_report();
    Ok(())
}

/// Summary with min-sessions filter excludes low-volume projects.
#[test]
fn test_cross_project_summary_min_sessions() -> Result<()> {
    let mut fixture = setup_workspace("cross_project_summary_min_sessions")?;

    fixture.checkpoint("cross-project:pre-summary-min-sessions");

    fixture.log_step("Run summary with min-sessions=2");
    let two_out = run_ms_with_stand_in_cass(
        &mut fixture,
        &["--robot", "cross-project", "summary", "--min-sessions", "2"],
    )?;
    fixture.assert_success(&two_out, "cross-project summary min-sessions 2");
    let two_json = two_out.json();
    let two_projects = two_json["projects"].as_array().expect("projects array");
    assert_eq!(two_projects.len(), 1, "{two_json}");
    assert_eq!(two_projects[0]["project"].as_str(), Some("proj-alpha"));
    assert_eq!(two_projects[0]["sessions"].as_u64(), Some(2));

    fixture.log_step("Run summary with high min-sessions threshold");
    let output = run_ms_with_stand_in_cass(
        &mut fixture,
        &[
            "--robot",
            "cross-project",
            "summary",
            "--min-sessions",
            "999999",
        ],
    )?;
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

/// Summary with --include-unknown includes sessions lacking a `project` field.
#[test]
fn test_cross_project_summary_include_unknown() -> Result<()> {
    let mut fixture = setup_workspace("cross_project_summary_include_unknown")?;

    let output = run_ms_with_stand_in_cass(
        &mut fixture,
        &["--robot", "cross-project", "summary", "--include-unknown"],
    )?;
    fixture.assert_success(&output, "cross-project summary include-unknown");

    let json = output.json();
    assert_eq!(json["total_sessions"].as_u64(), Some(5), "{json}");
    assert_eq!(json["total_projects"].as_u64(), Some(4), "{json}");
    let projects = json["projects"].as_array().expect("projects array");
    let unknown = projects
        .iter()
        .find(|p| p["project"].as_str() == Some("unknown"))
        .unwrap_or_else(|| panic!("expected 'unknown' project in summary: {json}"));
    assert_eq!(unknown["sessions"].as_u64(), Some(1), "{json}");

    fixture.generate_report();
    Ok(())
}

// ============================================================================
// Patterns Workflow Tests (deterministic stand-in CASS)
// ============================================================================

/// Patterns with default thresholds aggregates shared cross-project patterns.
#[test]
fn test_cross_project_patterns_json_output() -> Result<()> {
    let mut fixture = setup_workspace("cross_project_patterns_json")?;

    fixture.checkpoint("cross-project:pre-patterns");

    fixture.log_step("Run cross-project patterns with robot mode");
    let output = run_ms_with_stand_in_cass(
        &mut fixture,
        &["--robot", "cross-project", "patterns", "--limit", "2"],
    )?;
    fixture.assert_success(&output, "cross-project patterns");

    fixture.checkpoint("cross-project:post-patterns");

    let json = output.json();
    assert_eq!(json["query"].as_str(), Some("*"));
    assert_eq!(json["scanned_sessions"].as_u64(), Some(2), "{json}");
    let patterns = json["patterns"].as_array().expect("patterns array");
    assert!(
        !patterns.is_empty(),
        "Expected at least one aggregated cross-project pattern row: {json}"
    );
    let kubectl = patterns
        .iter()
        .find(|p| p["label"].as_str().unwrap_or("").contains("kubectl"))
        .unwrap_or_else(|| panic!("Expected kubectl command pattern row: {json}"));
    assert_eq!(kubectl["occurrences"].as_u64(), Some(2), "{json}");
    let proj_names: Vec<&str> = kubectl["projects"]
        .as_array()
        .expect("pattern projects array")
        .iter()
        .filter_map(|p| p["project"].as_str())
        .collect();
    assert!(proj_names.contains(&"proj-alpha"), "{json}");
    assert!(proj_names.contains(&"proj-beta"), "{json}");

    fixture.emit_event(
        LogLevel::Info,
        "cross-project",
        "Patterns JSON output verified with real aggregated rows",
        Some(json.clone()),
    );

    fixture.generate_report();
    Ok(())
}

/// Patterns with high thresholds returns 0 matching rows while still scanning sessions.
#[test]
fn test_cross_project_patterns_high_thresholds() -> Result<()> {
    let mut fixture = setup_workspace("cross_project_patterns_high_thresholds")?;

    fixture.checkpoint("cross-project:pre-patterns-high-thresholds");

    fixture.log_step("Run patterns with very high occurrence threshold");
    let output = run_ms_with_stand_in_cass(
        &mut fixture,
        &[
            "--robot",
            "cross-project",
            "patterns",
            "--limit",
            "2",
            "--min-occurrences",
            "999999",
        ],
    )?;
    fixture.assert_success(&output, "cross-project patterns high thresholds");

    fixture.checkpoint("cross-project:post-patterns-high-thresholds");

    let json = output.json();
    assert_eq!(json["scanned_sessions"].as_u64(), Some(2), "{json}");
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

/// Patterns with --include-unknown aggregates hits lacking a `project` field under `"unknown"`.
#[test]
fn test_cross_project_patterns_include_unknown() -> Result<()> {
    let mut fixture = setup_workspace("cross_project_patterns_include_unknown")?;

    // Baseline without --include-unknown: helm only appears in proj-alpha (1 project < 2).
    let default_out =
        run_ms_with_stand_in_cass(&mut fixture, &["--robot", "cross-project", "patterns"])?;
    fixture.assert_success(&default_out, "patterns default");
    let default_json = default_out.json();
    assert_eq!(
        default_json["scanned_sessions"].as_u64(),
        Some(4),
        "{default_json}"
    );
    assert!(
        default_json["patterns"]
            .as_array()
            .expect("patterns")
            .iter()
            .all(|p| !p["label"].as_str().unwrap_or("").contains("helm")),
        "helm should not cross 2 projects without --include-unknown: {default_json}"
    );

    // With --include-unknown: unknown-1.jsonl joins alpha-2.jsonl so helm crosses 2 projects.
    let inc_out = run_ms_with_stand_in_cass(
        &mut fixture,
        &["--robot", "cross-project", "patterns", "--include-unknown"],
    )?;
    fixture.assert_success(&inc_out, "patterns include-unknown");
    let inc_json = inc_out.json();
    assert_eq!(inc_json["scanned_sessions"].as_u64(), Some(5), "{inc_json}");
    let helm = inc_json["patterns"]
        .as_array()
        .expect("patterns")
        .iter()
        .find(|p| p["label"].as_str().unwrap_or("").contains("helm"))
        .unwrap_or_else(|| panic!("expected helm pattern with --include-unknown: {inc_json}"));
    assert_eq!(helm["occurrences"].as_u64(), Some(2), "{inc_json}");
    let projects: Vec<&str> = helm["projects"]
        .as_array()
        .expect("projects")
        .iter()
        .filter_map(|p| p["project"].as_str())
        .collect();
    assert!(projects.contains(&"proj-alpha"), "{inc_json}");
    assert!(projects.contains(&"unknown"), "{inc_json}");

    fixture.generate_report();
    Ok(())
}

// ============================================================================
// Gaps Workflow Tests (deterministic stand-in CASS)
// ============================================================================

/// Gaps with default args reports uncovered cross-project pattern rows.
#[test]
fn test_cross_project_gaps_json_output() -> Result<()> {
    let mut fixture = setup_workspace("cross_project_gaps_json")?;

    fixture.checkpoint("cross-project:pre-gaps");

    fixture.log_step("Run cross-project gaps with robot mode");
    let output = run_ms_with_stand_in_cass(
        &mut fixture,
        &["--robot", "cross-project", "gaps", "--limit", "2"],
    )?;
    fixture.assert_success(&output, "cross-project gaps");

    fixture.checkpoint("cross-project:post-gaps");

    let json = output.json();
    assert_eq!(json["query"].as_str(), Some("*"));
    assert_eq!(json["scanned_sessions"].as_u64(), Some(2), "{json}");
    let gaps = json["gaps"].as_array().expect("gaps array");
    assert!(
        !gaps.is_empty(),
        "Expected uncovered kubectl pattern to appear as a gap row: {json}"
    );
    let kubectl_gap = gaps
        .iter()
        .find(|g| g["label"].as_str().unwrap_or("").contains("kubectl"))
        .unwrap_or_else(|| panic!("Expected kubectl gap row: {json}"));
    assert_eq!(kubectl_gap["occurrences"].as_u64(), Some(2), "{json}");
    assert_eq!(
        kubectl_gap["projects"].as_array().map(Vec::len),
        Some(2),
        "{json}"
    );

    fixture.emit_event(
        LogLevel::Info,
        "cross-project",
        "Gaps JSON output verified with real gap rows",
        Some(json.clone()),
    );

    fixture.generate_report();
    Ok(())
}

/// Gaps with high min-score treats matched patterns as gaps when below threshold.
#[test]
fn test_cross_project_gaps_min_score() -> Result<()> {
    let mut fixture = setup_workspace("cross_project_gaps_min_score")?;
    fixture.create_skill("kubectl-rollout", SKILL_KUBECTL)?;
    let idx = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&idx, "reindex with kubectl skill");

    fixture.checkpoint("cross-project:pre-gaps-min-score");

    // At default --min-score 0.0, kubectl-rollout covers `Commands: kubectl`.
    let covered_out = run_ms_with_stand_in_cass(
        &mut fixture,
        &["--robot", "cross-project", "gaps", "--limit", "2"],
    )?;
    fixture.assert_success(&covered_out, "cross-project gaps default min-score");
    let covered_json = covered_out.json();
    assert_eq!(
        covered_json["gaps"].as_array().map(Vec::len),
        Some(0),
        "kubectl skill should cover kubectl pattern at min-score 0.0: {covered_json}"
    );

    fixture.log_step("Run gaps with high min-score to surface weakly covered patterns");
    let output = run_ms_with_stand_in_cass(
        &mut fixture,
        &[
            "--robot",
            "cross-project",
            "gaps",
            "--limit",
            "2",
            "--min-score",
            "100.0",
        ],
    )?;
    fixture.assert_success(&output, "cross-project gaps min-score");

    fixture.checkpoint("cross-project:post-gaps-min-score");

    let json = output.json();
    let gaps = json["gaps"].as_array().expect("gaps array");
    assert_eq!(
        gaps.len(),
        1,
        "High min-score must surface kubectl pattern as a gap: {json}"
    );
    assert!(
        gaps[0]["label"].as_str().unwrap_or("").contains("kubectl"),
        "{json}"
    );
    assert_eq!(
        gaps[0]["best_match"]["skill_id"].as_str(),
        Some("kubectl-rollout-operations"),
        "{json}"
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

/// Gaps with search-limit=1 aggregates gap rows cleanly.
#[test]
fn test_cross_project_gaps_search_limit() -> Result<()> {
    let mut fixture = setup_workspace("cross_project_gaps_search_limit")?;

    fixture.checkpoint("cross-project:pre-gaps-search-limit");

    fixture.log_step("Run gaps with search-limit=1");
    let output = run_ms_with_stand_in_cass(
        &mut fixture,
        &[
            "--robot",
            "cross-project",
            "gaps",
            "--limit",
            "2",
            "--search-limit",
            "1",
        ],
    )?;
    fixture.assert_success(&output, "cross-project gaps search-limit");

    fixture.checkpoint("cross-project:post-gaps-search-limit");

    let json = output.json();
    assert_eq!(json["scanned_sessions"].as_u64(), Some(2), "{json}");
    let gaps = json["gaps"].as_array().expect("gaps array");
    assert!(
        gaps.iter()
            .any(|g| g["label"].as_str().unwrap_or("").contains("kubectl")),
        "Expected kubectl gap row with search-limit=1: {json}"
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

/// Gaps with --include-unknown surfaces uncovered patterns shared with unknown-project sessions.
#[test]
fn test_cross_project_gaps_include_unknown() -> Result<()> {
    let mut fixture = setup_workspace("cross_project_gaps_include_unknown")?;

    // Baseline without --include-unknown: helm does not meet min-projects=2.
    let default_out =
        run_ms_with_stand_in_cass(&mut fixture, &["--robot", "cross-project", "gaps"])?;
    fixture.assert_success(&default_out, "gaps default");
    let default_json = default_out.json();
    assert!(
        default_json["gaps"]
            .as_array()
            .expect("gaps")
            .iter()
            .all(|g| !g["label"].as_str().unwrap_or("").contains("helm")),
        "helm gap should be absent without --include-unknown: {default_json}"
    );

    // With --include-unknown: helm crosses proj-alpha + unknown and surfaces as a gap.
    let inc_out = run_ms_with_stand_in_cass(
        &mut fixture,
        &["--robot", "cross-project", "gaps", "--include-unknown"],
    )?;
    fixture.assert_success(&inc_out, "gaps include-unknown");
    let inc_json = inc_out.json();
    assert_eq!(inc_json["scanned_sessions"].as_u64(), Some(5), "{inc_json}");
    let helm_gap = inc_json["gaps"]
        .as_array()
        .expect("gaps")
        .iter()
        .find(|g| g["label"].as_str().unwrap_or("").contains("helm"))
        .unwrap_or_else(|| panic!("expected helm gap with --include-unknown: {inc_json}"));
    assert_eq!(helm_gap["occurrences"].as_u64(), Some(2), "{inc_json}");
    let projects: Vec<&str> = helm_gap["projects"]
        .as_array()
        .expect("projects")
        .iter()
        .filter_map(|p| p["project"].as_str())
        .collect();
    assert!(projects.contains(&"proj-alpha"), "{inc_json}");
    assert!(projects.contains(&"unknown"), "{inc_json}");

    fixture.generate_report();
    Ok(())
}

/// Gaps with zero search-limit should fail validation.
#[test]
fn test_cross_project_gaps_zero_search_limit() -> Result<()> {
    let mut fixture = setup_minimal("cross_project_gaps_zero_search_limit")?;

    fixture.checkpoint("cross-project:pre-gaps-zero-search-limit");

    fixture.log_step("Run gaps with search-limit=0");
    let output = fixture.run_ms(&["--robot", "cross-project", "gaps", "--search-limit", "0"]);

    assert!(!output.success, "Gaps with search-limit=0 should fail");

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
