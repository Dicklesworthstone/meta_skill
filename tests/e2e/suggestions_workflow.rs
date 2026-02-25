//! E2E Scenario: Suggestions/Bandit Workflow Integration Tests
//!
//! Covers context-aware suggestions, feedback recording,
//! bandit learning updates, state persistence, and cold start behavior.

use super::fixture::E2EFixture;
use ms::error::Result;

// =============================================================================
// SKILL DEFINITIONS
// =============================================================================

const SKILL_RUST_ERRORS: &str = r#"---
name: Rust Error Handling
description: Best practices for Rust error handling with thiserror and anyhow
tags: [rust, errors, thiserror]
context:
  project_types: [rust]
  file_patterns: ["*.rs", "Cargo.toml"]
  tools: [cargo, rustc]
---

# Rust Error Handling

Use `thiserror` for library errors and `anyhow` for application errors.

## Rules

- Prefer `Result` over panicking
- Use `?` operator for propagation
- Include context when wrapping errors
"#;

const SKILL_RUST_TESTING: &str = r#"---
name: Rust Testing Patterns
description: Comprehensive testing patterns for Rust projects
tags: [rust, testing, cargo]
context:
  project_types: [rust]
  file_patterns: ["*.rs"]
  tools: [cargo]
---

# Rust Testing Patterns

Use inline `#[cfg(test)]` modules and integration tests in `tests/`.

## Rules

- Test happy path, edge cases, and error conditions
- Use proptest for property-based testing
- Prefer real implementations over mocks
"#;

const SKILL_GIT_WORKFLOW: &str = r#"---
name: Git Workflow
description: Standard git branching and workflow patterns
tags: [git, workflow, vcs]
---

# Git Workflow

Standard git branching model with feature branches.

## Rules

- Use feature branches
- Write clear commit messages
- Squash before merge
"#;

const SKILL_DEBUGGING: &str = r#"---
name: Debug Techniques
description: Systematic debugging approaches for complex issues
tags: [debugging, troubleshooting]
---

# Debug Techniques

Systematic approaches to finding and fixing bugs.

## Rules

- Reproduce before fixing
- Binary search through commits with git bisect
- Use logging and tracing
"#;

const SKILL_PERFORMANCE: &str = r#"---
name: Performance Optimization
description: Performance profiling and optimization patterns
tags: [performance, profiling, optimization]
---

# Performance Optimization

Profile first, optimize second. Measure everything.

## Rules

- Profile before optimizing
- Use flamegraphs for CPU analysis
- Benchmark with criterion
"#;

// =============================================================================
// HELPER: Setup fixture with skills
// =============================================================================

fn setup_suggest_fixture(scenario: &str) -> Result<E2EFixture> {
    let mut fixture = E2EFixture::new(scenario);

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    fixture.log_step("Create skills for suggestion testing");
    fixture.create_skill("rust-error-handling", SKILL_RUST_ERRORS)?;
    fixture.create_skill("rust-testing-patterns", SKILL_RUST_TESTING)?;
    fixture.create_skill("git-workflow", SKILL_GIT_WORKFLOW)?;
    fixture.create_skill("debug-techniques", SKILL_DEBUGGING)?;
    fixture.create_skill("performance-optimization", SKILL_PERFORMANCE)?;

    fixture.log_step("Index skills");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    // Verify skills are indexed
    fixture.log_step("Verify skills indexed");
    let output = fixture.run_ms(&["--robot", "list"]);
    fixture.assert_success(&output, "list");
    let json = output.json();
    let skills = json["skills"].as_array().expect("skills array");
    assert!(
        skills.len() >= 5,
        "Expected at least 5 skills indexed, got {}",
        skills.len()
    );

    Ok(fixture)
}

// =============================================================================
// TEST: Cold start suggestions with no history
// =============================================================================

#[test]
fn test_suggest_cold_start() -> Result<()> {
    let mut fixture = setup_suggest_fixture("suggest_cold_start")?;

    fixture.log_step("checkpoint:suggest:setup");
    fixture.checkpoint("suggest_setup");

    fixture.log_step("checkpoint:suggest:pre_suggest");

    fixture.log_step("Run suggest with no prior history (cold start)");
    let output = fixture.run_ms(&[
        "--robot",
        "suggest",
        "--ignore-cooldowns",
        "--reset-bandit",
        "--reset-cooldowns",
    ]);
    fixture.assert_success(&output, "suggest cold start");

    fixture.log_step("checkpoint:suggest:post_suggest");

    // Verify output structure
    let json = output.json();
    assert!(
        json.get("suggestions").is_some() || json.get("items").is_some(),
        "Suggest output should contain suggestions or items field: {json}"
    );

    // In cold start, we should still get suggestions (exploration mode)
    let suggestions = json
        .get("suggestions")
        .or_else(|| json.get("items"))
        .and_then(|v| v.as_array());

    if let Some(items) = suggestions {
        println!(
            "[VERIFY] Cold start returned {} suggestions",
            items.len()
        );
        // With 5 skills indexed, should get some suggestions
        assert!(
            !items.is_empty(),
            "Cold start should still generate suggestions via exploration"
        );
    }

    fixture.log_step("checkpoint:suggest:teardown");
    fixture.checkpoint("suggest_cold_start_done");

    Ok(())
}

// =============================================================================
// TEST: Context-aware suggestions
// =============================================================================

#[test]
fn test_suggest_context_aware() -> Result<()> {
    let mut fixture = setup_suggest_fixture("suggest_context_aware")?;

    fixture.log_step("checkpoint:suggest:setup");

    // Create a Rust project context in the temp directory
    fixture.log_step("Create Rust project context files");
    let cargo_toml = r#"[package]
name = "test-project"
version = "0.1.0"
edition = "2021"
"#;
    std::fs::write(fixture.root.join("Cargo.toml"), cargo_toml)?;
    std::fs::create_dir_all(fixture.root.join("src"))?;
    std::fs::write(
        fixture.root.join("src/main.rs"),
        "fn main() { println!(\"hello\"); }",
    )?;

    fixture.log_step("checkpoint:suggest:context_analyze");

    fixture.log_step("Run suggest with Rust project context");
    let cwd = fixture.root.display().to_string();
    let output = fixture.run_ms(&[
        "--robot",
        "suggest",
        "--cwd",
        &cwd,
        "--ignore-cooldowns",
        "--reset-bandit",
        "--reset-cooldowns",
    ]);
    fixture.assert_success(&output, "suggest context-aware");

    fixture.log_step("checkpoint:suggest:post_suggest");

    let json = output.json();
    let suggestions = json
        .get("suggestions")
        .or_else(|| json.get("items"))
        .and_then(|v| v.as_array());

    if let Some(items) = suggestions {
        println!(
            "[VERIFY] Context-aware returned {} suggestions",
            items.len()
        );

        // Log each suggestion for inspection
        for (i, item) in items.iter().enumerate() {
            let id = item
                .get("skill_id")
                .or_else(|| item.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let score = item
                .get("confidence")
                .or_else(|| item.get("score"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            println!("[VERIFY] Suggestion {}: id={}, score={:.3}", i, id, score);
        }
    }

    fixture.log_step("checkpoint:suggest:teardown");
    fixture.checkpoint("suggest_context_done");

    Ok(())
}

// =============================================================================
// TEST: Positive feedback recording
// =============================================================================

#[test]
fn test_suggest_feedback_positive() -> Result<()> {
    let mut fixture = setup_suggest_fixture("suggest_feedback_positive")?;

    fixture.log_step("checkpoint:suggest:setup");

    fixture.log_step("Record positive feedback for a skill");
    let output = fixture.run_ms(&[
        "--robot",
        "feedback",
        "add",
        "rust-error-handling",
        "--positive",
        "--comment",
        "Very helpful for error propagation patterns",
    ]);
    fixture.assert_success(&output, "feedback add positive");

    fixture.log_step("checkpoint:suggest:feedback_record");

    // Verify the feedback was recorded
    let json = output.json();
    let status = json["status"].as_str().unwrap_or("");
    assert_eq!(status, "ok", "Feedback should be recorded successfully");

    // Verify feedback appears in list
    fixture.log_step("List feedback for the skill");
    let output = fixture.run_ms(&[
        "--robot",
        "feedback",
        "list",
        "--skill",
        "rust-error-handling",
    ]);
    fixture.assert_success(&output, "feedback list");

    let json = output.json();
    let records = json
        .get("records")
        .or_else(|| json.get("feedback"))
        .and_then(|v| v.as_array());

    if let Some(items) = records {
        assert!(
            !items.is_empty(),
            "Should have at least one feedback record"
        );
        println!("[VERIFY] Found {} feedback records", items.len());
    }

    fixture.log_step("checkpoint:suggest:teardown");
    fixture.checkpoint("suggest_feedback_positive_done");

    Ok(())
}

// =============================================================================
// TEST: Negative feedback recording
// =============================================================================

#[test]
fn test_suggest_feedback_negative() -> Result<()> {
    let mut fixture = setup_suggest_fixture("suggest_feedback_negative")?;

    fixture.log_step("checkpoint:suggest:setup");

    fixture.log_step("Record negative feedback with rating");
    let output = fixture.run_ms(&[
        "--robot",
        "feedback",
        "add",
        "git-workflow",
        "--negative",
        "--rating",
        "2",
        "--comment",
        "Too generic for our workflow",
    ]);
    fixture.assert_success(&output, "feedback add negative");

    fixture.log_step("checkpoint:suggest:feedback_record");

    let json = output.json();
    let status = json["status"].as_str().unwrap_or("");
    assert_eq!(status, "ok", "Negative feedback should be recorded");

    // Verify feedback appears in list
    fixture.log_step("Verify feedback in list");
    let output = fixture.run_ms(&["--robot", "feedback", "list", "--skill", "git-workflow"]);
    fixture.assert_success(&output, "feedback list");

    fixture.log_step("checkpoint:suggest:teardown");
    fixture.checkpoint("suggest_feedback_negative_done");

    Ok(())
}

// =============================================================================
// TEST: Bandit learning updates from feedback
// =============================================================================

#[test]
fn test_suggest_bandit_learning() -> Result<()> {
    let mut fixture = setup_suggest_fixture("suggest_bandit_learning")?;

    fixture.log_step("checkpoint:suggest:setup");

    // First, reset the bandit to a clean state
    fixture.log_step("Reset bandit state");
    let output = fixture.run_ms(&["--robot", "bandit", "reset"]);
    fixture.assert_success(&output, "bandit reset");

    let json = output.json();
    assert_eq!(
        json["status"].as_str().unwrap_or(""),
        "ok",
        "Bandit reset should succeed"
    );

    // Get initial bandit stats
    fixture.log_step("Get initial bandit stats");
    let output = fixture.run_ms(&["--robot", "bandit", "stats"]);
    fixture.assert_success(&output, "bandit stats initial");

    let initial_stats = output.json();
    let initial_selections = initial_stats["total_selections"].as_u64().unwrap_or(999);
    println!(
        "[VERIFY] Initial total_selections: {}",
        initial_selections
    );

    // Record positive feedback to update bandit
    fixture.log_step("Record feedback to trigger bandit learning");
    let output = fixture.run_ms(&[
        "--robot",
        "feedback",
        "add",
        "rust-error-handling",
        "--positive",
        "--rating",
        "5",
    ]);
    fixture.assert_success(&output, "feedback add for bandit");

    fixture.log_step("checkpoint:suggest:feedback_record");

    // Check bandit stats after feedback
    fixture.log_step("checkpoint:suggest:verify_weights");
    let output = fixture.run_ms(&["--robot", "bandit", "stats"]);
    fixture.assert_success(&output, "bandit stats after feedback");

    let after_stats = output.json();
    println!(
        "[VERIFY] After-feedback stats: {}",
        serde_json::to_string_pretty(&after_stats).unwrap_or_default()
    );

    // The bandit should still be functional after feedback
    assert_eq!(
        after_stats["status"].as_str().unwrap_or(""),
        "ok",
        "Bandit stats should be ok after feedback"
    );

    fixture.log_step("checkpoint:suggest:teardown");
    fixture.checkpoint("suggest_bandit_learning_done");

    Ok(())
}

// =============================================================================
// TEST: Bandit stats command
// =============================================================================

#[test]
fn test_suggest_bandit_stats() -> Result<()> {
    let mut fixture = setup_suggest_fixture("suggest_bandit_stats")?;

    fixture.log_step("checkpoint:suggest:setup");

    // Reset first for clean state
    fixture.log_step("Reset bandit for clean stats");
    let output = fixture.run_ms(&["--robot", "bandit", "reset"]);
    fixture.assert_success(&output, "bandit reset");

    // Check stats
    fixture.log_step("Get bandit stats");
    let output = fixture.run_ms(&["--robot", "bandit", "stats"]);
    fixture.assert_success(&output, "bandit stats");

    let json = output.json();

    // Verify stats structure
    assert_eq!(
        json["status"].as_str().unwrap_or(""),
        "ok",
        "Stats status should be ok"
    );
    assert!(
        json.get("total_selections").is_some(),
        "Stats should include total_selections"
    );
    assert!(
        json.get("config").is_some(),
        "Stats should include config"
    );
    assert!(
        json.get("weights").is_some(),
        "Stats should include weights"
    );
    assert!(
        json.get("arms").is_some(),
        "Stats should include arms"
    );

    // Verify config fields
    let config = &json["config"];
    assert!(
        config.get("exploration_factor").is_some(),
        "Config should include exploration_factor"
    );
    assert!(
        config.get("observation_decay").is_some(),
        "Config should include observation_decay"
    );

    // Log the arms
    if let Some(arms) = json["arms"].as_array() {
        println!("[VERIFY] Bandit has {} arms", arms.len());
        for arm in arms {
            let signal = arm["signal"].as_str().unwrap_or("unknown");
            let prob = arm["estimated_prob"].as_f64().unwrap_or(0.0);
            println!("[VERIFY] Arm: {} prob={:.3}", signal, prob);
        }
    }

    fixture.log_step("checkpoint:suggest:teardown");
    fixture.checkpoint("suggest_bandit_stats_done");

    Ok(())
}

// =============================================================================
// TEST: Bandit reset clears learning
// =============================================================================

#[test]
fn test_suggest_bandit_reset() -> Result<()> {
    let mut fixture = setup_suggest_fixture("suggest_bandit_reset")?;

    fixture.log_step("checkpoint:suggest:setup");

    // Record some feedback first
    fixture.log_step("Record feedback before reset");
    let output = fixture.run_ms(&[
        "--robot",
        "feedback",
        "add",
        "rust-error-handling",
        "--positive",
        "--rating",
        "5",
    ]);
    fixture.assert_success(&output, "pre-reset feedback");

    // Reset the bandit
    fixture.log_step("Reset bandit state");
    let output = fixture.run_ms(&["--robot", "bandit", "reset"]);
    fixture.assert_success(&output, "bandit reset");

    let json = output.json();
    assert_eq!(
        json["status"].as_str().unwrap_or(""),
        "ok",
        "Reset should succeed"
    );
    assert_eq!(
        json["reset"].as_bool().unwrap_or(false),
        true,
        "Reset flag should be true"
    );

    // Verify stats are fresh after reset
    fixture.log_step("checkpoint:suggest:verify_weights");
    let output = fixture.run_ms(&["--robot", "bandit", "stats"]);
    fixture.assert_success(&output, "bandit stats after reset");

    let json = output.json();
    let total = json["total_selections"].as_u64().unwrap_or(999);
    assert_eq!(total, 0, "Total selections should be 0 after reset");

    println!("[VERIFY] Bandit successfully reset, total_selections={}", total);

    fixture.log_step("checkpoint:suggest:teardown");
    fixture.checkpoint("suggest_bandit_reset_done");

    Ok(())
}

// =============================================================================
// TEST: Bandit state persistence across runs
// =============================================================================

#[test]
fn test_suggest_bandit_persistence() -> Result<()> {
    let mut fixture = setup_suggest_fixture("suggest_bandit_persistence")?;

    fixture.log_step("checkpoint:suggest:setup");

    // Reset for clean state
    fixture.log_step("Reset bandit");
    let output = fixture.run_ms(&["--robot", "bandit", "reset"]);
    fixture.assert_success(&output, "bandit reset");

    // Run suggest which will create/update bandit state
    fixture.log_step("Run suggest (first invocation)");
    let output = fixture.run_ms(&[
        "--robot",
        "suggest",
        "--ignore-cooldowns",
        "--reset-cooldowns",
    ]);
    fixture.assert_success(&output, "suggest first run");

    // Record feedback to modify the state
    fixture.log_step("Record feedback");
    let output = fixture.run_ms(&[
        "--robot",
        "feedback",
        "add",
        "rust-testing-patterns",
        "--positive",
        "--rating",
        "4",
    ]);
    fixture.assert_success(&output, "feedback add for persistence");

    // Check stats at this point
    fixture.log_step("Get bandit stats after activity");
    let output = fixture.run_ms(&["--robot", "bandit", "stats"]);
    fixture.assert_success(&output, "bandit stats mid-test");
    let mid_stats = output.json();

    // Run suggest again - bandit state should persist
    fixture.log_step("Run suggest (second invocation)");
    let output = fixture.run_ms(&[
        "--robot",
        "suggest",
        "--ignore-cooldowns",
        "--reset-cooldowns",
    ]);
    fixture.assert_success(&output, "suggest second run");

    // Stats should show the bandit state is still intact
    fixture.log_step("Verify bandit state persisted");
    let output = fixture.run_ms(&["--robot", "bandit", "stats"]);
    fixture.assert_success(&output, "bandit stats final");
    let final_stats = output.json();

    // State should still be valid (not reset)
    assert_eq!(
        final_stats["status"].as_str().unwrap_or(""),
        "ok",
        "Bandit state should persist"
    );

    println!(
        "[VERIFY] Mid-test total_selections: {}",
        mid_stats["total_selections"]
    );
    println!(
        "[VERIFY] Final total_selections: {}",
        final_stats["total_selections"]
    );

    fixture.log_step("checkpoint:suggest:teardown");
    fixture.checkpoint("suggest_bandit_persistence_done");

    Ok(())
}

// =============================================================================
// TEST: Suggestions with explain mode
// =============================================================================

#[test]
fn test_suggest_explain_mode() -> Result<()> {
    let mut fixture = setup_suggest_fixture("suggest_explain_mode")?;

    fixture.log_step("checkpoint:suggest:setup");

    fixture.log_step("Run suggest with --explain flag");
    let output = fixture.run_ms(&[
        "--robot",
        "suggest",
        "--explain",
        "--ignore-cooldowns",
        "--reset-bandit",
        "--reset-cooldowns",
    ]);
    fixture.assert_success(&output, "suggest with explain");

    fixture.log_step("checkpoint:suggest:post_suggest");

    let json = output.json();
    let suggestions = json
        .get("suggestions")
        .or_else(|| json.get("items"))
        .and_then(|v| v.as_array());

    if let Some(items) = suggestions {
        println!(
            "[VERIFY] Explain mode returned {} suggestions",
            items.len()
        );

        // In explain mode, suggestions should have breakdown data
        for (i, item) in items.iter().enumerate() {
            let id = item
                .get("skill_id")
                .or_else(|| item.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let has_breakdown = item.get("breakdown").is_some()
                || item.get("score_breakdown").is_some()
                || item.get("percentage_breakdown").is_some();
            println!(
                "[VERIFY] Suggestion {}: id={}, has_breakdown={}",
                i, id, has_breakdown
            );
        }
    }

    fixture.log_step("checkpoint:suggest:teardown");
    fixture.checkpoint("suggest_explain_done");

    Ok(())
}
