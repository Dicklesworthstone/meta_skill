//! E2E Scenario: Experiment/A-B Testing Workflow Integration Tests
//!
//! Covers experiment creation, variant definition, traffic allocation,
//! metrics collection, result analysis, and experiment conclusion.

use super::fixture::E2EFixture;
use ms::error::Result;

// =============================================================================
// SKILL DEFINITIONS
// =============================================================================

const SKILL_FOR_EXPERIMENT: &str = r#"---
name: Rust Error Handling
description: Best practices for Rust error handling
tags: [rust, errors]
---

# Rust Error Handling

Use `thiserror` for library errors and `anyhow` for application errors.

## Rules

- Prefer `Result` over panicking
- Use `?` operator for propagation
"#;

const SKILL_ALT: &str = r#"---
name: Python Testing
description: Python testing patterns
tags: [python, testing]
---

# Python Testing

Use pytest for testing Python projects.

## Rules

- Write tests alongside source files
- Use fixtures for setup
"#;

// =============================================================================
// HELPER: Setup fixture with skills
// =============================================================================

fn setup_experiment_fixture(scenario: &str) -> Result<E2EFixture> {
    let mut fixture = E2EFixture::new(scenario);

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    fixture.log_step("Create skills for experiment testing");
    fixture.create_skill("rust-error-handling", SKILL_FOR_EXPERIMENT)?;
    fixture.create_skill("python-testing", SKILL_ALT)?;

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
        skills.len() >= 2,
        "Expected at least 2 skills indexed, got {}",
        skills.len()
    );

    Ok(fixture)
}

/// Create an experiment and return its ID.
fn create_experiment(fixture: &mut E2EFixture) -> Result<String> {
    let output = fixture.run_ms(&[
        "--robot",
        "experiment",
        "create",
        "rust-error-handling",
        "--variant",
        "control",
        "--variant",
        "concise",
    ]);
    fixture.assert_success(&output, "experiment create");

    let json = output.json();
    let experiment_id = json["experiment"]["id"]
        .as_str()
        .expect("experiment should have id")
        .to_string();
    println!(
        "[VERIFY] Created experiment: {}",
        experiment_id
    );
    Ok(experiment_id)
}

// =============================================================================
// TEST: Create new experiment
// =============================================================================

#[test]
fn test_experiment_create() -> Result<()> {
    let mut fixture = setup_experiment_fixture("experiment_create")?;

    fixture.log_step("checkpoint:experiment:setup");
    fixture.checkpoint("experiment_setup");

    fixture.log_step("Create experiment with two variants");
    let output = fixture.run_ms(&[
        "--robot",
        "experiment",
        "create",
        "rust-error-handling",
        "--variant",
        "control",
        "--variant",
        "concise",
    ]);
    fixture.assert_success(&output, "experiment create");

    fixture.log_step("checkpoint:experiment:create");

    let json = output.json();
    assert_eq!(
        json["status"].as_str().unwrap_or(""),
        "ok",
        "Create should succeed"
    );

    let experiment = &json["experiment"];
    assert!(
        experiment["id"].as_str().is_some(),
        "Experiment should have an ID"
    );
    assert_eq!(
        experiment["skill_id"].as_str().unwrap_or(""),
        "rust-error-handling",
        "Experiment should reference the skill"
    );
    assert_eq!(
        experiment["scope"].as_str().unwrap_or(""),
        "skill",
        "Default scope should be skill"
    );
    assert_eq!(
        experiment["status"].as_str().unwrap_or(""),
        "running",
        "Default status should be running"
    );

    println!(
        "[VERIFY] Experiment created: id={}, skill={}",
        experiment["id"].as_str().unwrap_or("?"),
        experiment["skill_id"].as_str().unwrap_or("?")
    );

    fixture.log_step("checkpoint:experiment:teardown");
    fixture.checkpoint("experiment_create_done");

    Ok(())
}

// =============================================================================
// TEST: Define variants with strategies
// =============================================================================

#[test]
fn test_experiment_variants() -> Result<()> {
    let mut fixture = setup_experiment_fixture("experiment_variants")?;

    fixture.log_step("checkpoint:experiment:setup");

    // Create with named variants using id:name format
    fixture.log_step("Create experiment with named variants");
    let output = fixture.run_ms(&[
        "--robot",
        "experiment",
        "create",
        "rust-error-handling",
        "--variant",
        "control:Full Detail",
        "--variant",
        "compact:Compact Version",
    ]);
    fixture.assert_success(&output, "experiment create with named variants");

    fixture.log_step("checkpoint:experiment:configure");

    let json = output.json();
    let variants_str = json["experiment"]["variants_json"]
        .as_str()
        .unwrap_or("{}");
    println!("[VERIFY] Variants JSON: {}", variants_str);

    // Verify the variants are stored
    let experiment_id = json["experiment"]["id"]
        .as_str()
        .expect("experiment id");

    // Check status shows variants
    fixture.log_step("Get experiment status to verify variants");
    let output = fixture.run_ms(&["--robot", "experiment", "status", experiment_id]);
    fixture.assert_success(&output, "experiment status");

    let json = output.json();
    assert_eq!(
        json["status"].as_str().unwrap_or(""),
        "ok",
        "Status should be ok"
    );

    // Verify variants exist in the output
    if let Some(variants) = json.get("variants").and_then(|v| v.as_array()) {
        assert!(
            variants.len() >= 2,
            "Should have at least 2 variant stats"
        );
        for v in variants {
            let id = v["id"].as_str().unwrap_or("unknown");
            println!("[VERIFY] Variant: {}", id);
        }
    }

    fixture.log_step("checkpoint:experiment:teardown");
    fixture.checkpoint("experiment_variants_done");

    Ok(())
}

// =============================================================================
// TEST: Traffic allocation strategies
// =============================================================================

#[test]
fn test_experiment_traffic() -> Result<()> {
    let mut fixture = setup_experiment_fixture("experiment_traffic")?;

    fixture.log_step("checkpoint:experiment:setup");

    // Create with weighted strategy
    fixture.log_step("Create experiment with weighted strategy");
    let output = fixture.run_ms(&[
        "--robot",
        "experiment",
        "create",
        "rust-error-handling",
        "--variant",
        "control",
        "--variant",
        "treatment",
        "--strategy",
        "weighted",
        "--weight",
        "control=0.7",
        "--weight",
        "treatment=0.3",
    ]);
    fixture.assert_success(&output, "experiment create weighted");

    fixture.log_step("checkpoint:experiment:configure");

    let json = output.json();
    let experiment_id = json["experiment"]["id"]
        .as_str()
        .expect("experiment id")
        .to_string();

    // Assign variant - should respect weights
    fixture.log_step("Assign variant with weighted allocation");
    let output = fixture.run_ms(&["--robot", "experiment", "assign", &experiment_id]);
    fixture.assert_success(&output, "experiment assign");

    let json = output.json();
    let variant = json["variant"]["id"].as_str().unwrap_or("unknown");
    println!("[VERIFY] Assigned variant: {}", variant);
    assert!(
        variant == "control" || variant == "treatment",
        "Assigned variant should be one of the defined variants"
    );

    fixture.log_step("checkpoint:experiment:teardown");
    fixture.checkpoint("experiment_traffic_done");

    Ok(())
}

// =============================================================================
// TEST: Start experiment and assign variants
// =============================================================================

#[test]
fn test_experiment_start() -> Result<()> {
    let mut fixture = setup_experiment_fixture("experiment_start")?;

    fixture.log_step("checkpoint:experiment:setup");

    let experiment_id = create_experiment(&mut fixture)?;

    fixture.log_step("checkpoint:experiment:start");

    // Run multiple assignments to simulate traffic
    fixture.log_step("Run multiple variant assignments");
    for i in 0..3 {
        let output = fixture.run_ms(&["--robot", "experiment", "assign", &experiment_id]);
        fixture.assert_success(&output, &format!("experiment assign {}", i));

        let json = output.json();
        let variant = json["variant"]["id"].as_str().unwrap_or("unknown");
        println!("[VERIFY] Assignment {}: variant={}", i, variant);
    }

    // Check status shows assignments
    fixture.log_step("Verify assignments in status");
    let output = fixture.run_ms(&["--robot", "experiment", "status", &experiment_id]);
    fixture.assert_success(&output, "experiment status after assignments");

    let json = output.json();
    if let Some(variants) = json.get("variants").and_then(|v| v.as_array()) {
        let total_assignments: u64 = variants
            .iter()
            .map(|v| v["assignments"].as_u64().unwrap_or(0))
            .sum();
        println!(
            "[VERIFY] Total assignments: {}",
            total_assignments
        );
        assert!(
            total_assignments >= 3,
            "Should have at least 3 assignments"
        );
    }

    fixture.log_step("checkpoint:experiment:teardown");
    fixture.checkpoint("experiment_start_done");

    Ok(())
}

// =============================================================================
// TEST: Collect metrics
// =============================================================================

#[test]
fn test_experiment_metrics() -> Result<()> {
    let mut fixture = setup_experiment_fixture("experiment_metrics")?;

    fixture.log_step("checkpoint:experiment:setup");

    let experiment_id = create_experiment(&mut fixture)?;

    fixture.log_step("checkpoint:experiment:start");

    // Record metrics for control variant
    fixture.log_step("Record success metric for control");
    let output = fixture.run_ms(&[
        "--robot",
        "experiment",
        "record",
        &experiment_id,
        "control",
        "--metric",
        "task_success=true",
    ]);
    fixture.assert_success(&output, "record control success");

    fixture.log_step("checkpoint:experiment:collect");

    let json = output.json();
    assert_eq!(
        json["status"].as_str().unwrap_or(""),
        "ok",
        "Recording should succeed"
    );

    // Record metrics for concise variant
    fixture.log_step("Record failure metric for concise");
    let output = fixture.run_ms(&[
        "--robot",
        "experiment",
        "record",
        &experiment_id,
        "concise",
        "--metric",
        "task_success=false",
    ]);
    fixture.assert_success(&output, "record concise failure");

    // Record more metrics for statistical significance
    fixture.log_step("Record additional metrics");
    for _ in 0..3 {
        let output = fixture.run_ms(&[
            "--robot",
            "experiment",
            "record",
            &experiment_id,
            "control",
            "--metric",
            "task_success=true",
        ]);
        fixture.assert_success(&output, "record control additional");
    }

    for _ in 0..2 {
        let output = fixture.run_ms(&[
            "--robot",
            "experiment",
            "record",
            &experiment_id,
            "concise",
            "--metric",
            "task_success=true",
        ]);
        fixture.assert_success(&output, "record concise additional");
    }

    fixture.log_step("checkpoint:experiment:teardown");
    fixture.checkpoint("experiment_metrics_done");

    Ok(())
}

// =============================================================================
// TEST: Analyze results
// =============================================================================

#[test]
fn test_experiment_analyze() -> Result<()> {
    let mut fixture = setup_experiment_fixture("experiment_analyze")?;

    fixture.log_step("checkpoint:experiment:setup");

    let experiment_id = create_experiment(&mut fixture)?;

    // Record enough data for analysis
    fixture.log_step("Record data for analysis");
    for _ in 0..5 {
        let output = fixture.run_ms(&[
            "--robot",
            "experiment",
            "record",
            &experiment_id,
            "control",
            "--metric",
            "task_success=true",
        ]);
        fixture.assert_success(&output, "record control success");
    }

    for _ in 0..3 {
        let output = fixture.run_ms(&[
            "--robot",
            "experiment",
            "record",
            &experiment_id,
            "concise",
            "--metric",
            "task_success=true",
        ]);
        fixture.assert_success(&output, "record concise success");
    }
    for _ in 0..2 {
        let output = fixture.run_ms(&[
            "--robot",
            "experiment",
            "record",
            &experiment_id,
            "concise",
            "--metric",
            "task_success=false",
        ]);
        fixture.assert_success(&output, "record concise failure");
    }

    // Analyze via status command
    fixture.log_step("checkpoint:experiment:analyze");
    let output = fixture.run_ms(&[
        "--robot",
        "experiment",
        "status",
        &experiment_id,
        "--metric",
        "task_success",
    ]);
    fixture.assert_success(&output, "experiment status with analysis");

    let json = output.json();

    // Verify analysis structure
    if let Some(variants) = json.get("variants").and_then(|v| v.as_array()) {
        for v in variants {
            let id = v["id"].as_str().unwrap_or("unknown");
            let outcomes = v["outcomes"].as_u64().unwrap_or(0);
            let success_rate = v["success_rate"].as_f64().unwrap_or(-1.0);
            println!(
                "[VERIFY] Variant {}: outcomes={}, success_rate={:.2}",
                id, outcomes, success_rate
            );
        }
    }

    // Check for analysis section
    if let Some(analysis) = json.get("analysis") {
        println!(
            "[VERIFY] Analysis: {}",
            serde_json::to_string_pretty(analysis).unwrap_or_default()
        );
        // Analysis should have recommendation
        assert!(
            analysis.get("recommendation").is_some(),
            "Analysis should include a recommendation"
        );
    }

    fixture.log_step("checkpoint:experiment:verify");
    fixture.log_step("checkpoint:experiment:teardown");
    fixture.checkpoint("experiment_analyze_done");

    Ok(())
}

// =============================================================================
// TEST: Conclude experiment with winner
// =============================================================================

#[test]
fn test_experiment_conclude() -> Result<()> {
    let mut fixture = setup_experiment_fixture("experiment_conclude")?;

    fixture.log_step("checkpoint:experiment:setup");

    let experiment_id = create_experiment(&mut fixture)?;

    // Record some data
    fixture.log_step("Record data before concluding");
    let output = fixture.run_ms(&[
        "--robot",
        "experiment",
        "record",
        &experiment_id,
        "control",
        "--metric",
        "task_success=true",
    ]);
    fixture.assert_success(&output, "record data");

    // Conclude the experiment
    fixture.log_step("checkpoint:experiment:conclude");
    let output = fixture.run_ms(&[
        "--robot",
        "experiment",
        "conclude",
        &experiment_id,
        "--winner",
        "control",
    ]);
    fixture.assert_success(&output, "experiment conclude");

    let json = output.json();
    assert_eq!(
        json["status"].as_str().unwrap_or(""),
        "ok",
        "Conclude should succeed"
    );
    assert_eq!(
        json["winner"].as_str().unwrap_or(""),
        "control",
        "Winner should be control"
    );
    assert_eq!(
        json["experiment_id"].as_str().unwrap_or(""),
        experiment_id,
        "Should reference correct experiment"
    );

    // Verify status is now concluded
    fixture.log_step("checkpoint:experiment:verify");
    let output = fixture.run_ms(&["--robot", "experiment", "status", &experiment_id]);
    fixture.assert_success(&output, "experiment status after conclude");

    let json = output.json();
    let status = json["experiment"]["status"].as_str().unwrap_or("");
    assert_eq!(status, "concluded", "Status should be concluded");

    println!("[VERIFY] Experiment concluded with winner=control");

    fixture.log_step("checkpoint:experiment:teardown");
    fixture.checkpoint("experiment_conclude_done");

    Ok(())
}

// =============================================================================
// TEST: List experiments
// =============================================================================

#[test]
fn test_experiment_list() -> Result<()> {
    let mut fixture = setup_experiment_fixture("experiment_list")?;

    fixture.log_step("checkpoint:experiment:setup");

    // Create multiple experiments
    fixture.log_step("Create first experiment");
    let output = fixture.run_ms(&[
        "--robot",
        "experiment",
        "create",
        "rust-error-handling",
        "--variant",
        "control",
        "--variant",
        "compact",
    ]);
    fixture.assert_success(&output, "experiment create 1");

    fixture.log_step("Create second experiment");
    let output = fixture.run_ms(&[
        "--robot",
        "experiment",
        "create",
        "python-testing",
        "--variant",
        "a",
        "--variant",
        "b",
        "--variant",
        "c",
    ]);
    fixture.assert_success(&output, "experiment create 2");

    // List all experiments
    fixture.log_step("List all experiments");
    let output = fixture.run_ms(&["--robot", "experiment", "list"]);
    fixture.assert_success(&output, "experiment list all");

    let json = output.json();
    assert_eq!(
        json["status"].as_str().unwrap_or(""),
        "ok",
        "List should succeed"
    );

    let count = json["count"].as_u64().unwrap_or(0);
    assert!(count >= 2, "Should have at least 2 experiments, got {}", count);

    if let Some(experiments) = json["experiments"].as_array() {
        for exp in experiments {
            let id = exp["id"].as_str().unwrap_or("?");
            let skill = exp["skill_id"].as_str().unwrap_or("?");
            let status = exp["status"].as_str().unwrap_or("?");
            println!(
                "[VERIFY] Experiment: id={}, skill={}, status={}",
                id, skill, status
            );
        }
    }

    // Filter by skill
    fixture.log_step("List experiments filtered by skill");
    let output = fixture.run_ms(&[
        "--robot",
        "experiment",
        "list",
        "--skill",
        "rust-error-handling",
    ]);
    fixture.assert_success(&output, "experiment list filtered");

    let json = output.json();
    let filtered_count = json["count"].as_u64().unwrap_or(0);
    assert_eq!(
        filtered_count, 1,
        "Filtered list should have exactly 1 experiment for rust-error-handling"
    );

    fixture.log_step("checkpoint:experiment:teardown");
    fixture.checkpoint("experiment_list_done");

    Ok(())
}

// =============================================================================
// TEST: Full A/B testing lifecycle
// =============================================================================

#[test]
fn test_experiment_full_lifecycle() -> Result<()> {
    let mut fixture = setup_experiment_fixture("experiment_full_lifecycle")?;

    fixture.log_step("checkpoint:experiment:setup");

    // 1. Create experiment
    fixture.log_step("checkpoint:experiment:create");
    let experiment_id = create_experiment(&mut fixture)?;

    // 2. Assign variants multiple times
    fixture.log_step("checkpoint:experiment:start");
    let mut assignment_counts: std::collections::HashMap<String, u32> =
        std::collections::HashMap::new();
    for i in 0..6 {
        let output = fixture.run_ms(&["--robot", "experiment", "assign", &experiment_id]);
        fixture.assert_success(&output, &format!("assign {}", i));

        let json = output.json();
        let variant = json["variant"]["id"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        *assignment_counts.entry(variant).or_insert(0) += 1;
    }

    println!("[VERIFY] Assignment distribution: {:?}", assignment_counts);

    // 3. Record outcomes
    fixture.log_step("checkpoint:experiment:collect");
    // Control: 4 success, 1 failure
    for _ in 0..4 {
        let output = fixture.run_ms(&[
            "--robot",
            "experiment",
            "record",
            &experiment_id,
            "control",
            "--metric",
            "task_success=true",
        ]);
        fixture.assert_success(&output, "record control success");
    }
    let output = fixture.run_ms(&[
        "--robot",
        "experiment",
        "record",
        &experiment_id,
        "control",
        "--metric",
        "task_success=false",
    ]);
    fixture.assert_success(&output, "record control failure");

    // Concise: 2 success, 3 failure
    for _ in 0..2 {
        let output = fixture.run_ms(&[
            "--robot",
            "experiment",
            "record",
            &experiment_id,
            "concise",
            "--metric",
            "task_success=true",
        ]);
        fixture.assert_success(&output, "record concise success");
    }
    for _ in 0..3 {
        let output = fixture.run_ms(&[
            "--robot",
            "experiment",
            "record",
            &experiment_id,
            "concise",
            "--metric",
            "task_success=false",
        ]);
        fixture.assert_success(&output, "record concise failure");
    }

    // 4. Analyze
    fixture.log_step("checkpoint:experiment:analyze");
    let output = fixture.run_ms(&[
        "--robot",
        "experiment",
        "status",
        &experiment_id,
        "--metric",
        "task_success",
    ]);
    fixture.assert_success(&output, "experiment status analysis");

    let json = output.json();
    if let Some(variants) = json.get("variants").and_then(|v| v.as_array()) {
        for v in variants {
            let id = v["id"].as_str().unwrap_or("?");
            let rate = v["success_rate"].as_f64().unwrap_or(-1.0);
            println!("[VERIFY] {} success_rate={:.2}", id, rate);
        }
    }

    // 5. Conclude
    fixture.log_step("checkpoint:experiment:conclude");
    let output = fixture.run_ms(&[
        "--robot",
        "experiment",
        "conclude",
        &experiment_id,
        "--winner",
        "control",
    ]);
    fixture.assert_success(&output, "experiment conclude");

    // 6. Verify final state
    fixture.log_step("checkpoint:experiment:verify");
    let output = fixture.run_ms(&["--robot", "experiment", "status", &experiment_id]);
    fixture.assert_success(&output, "final status check");

    let json = output.json();
    assert_eq!(
        json["experiment"]["status"].as_str().unwrap_or(""),
        "concluded",
        "Final status should be concluded"
    );

    println!("[VERIFY] Full A/B lifecycle complete for experiment {}", experiment_id);

    fixture.log_step("checkpoint:experiment:teardown");
    fixture.checkpoint("experiment_lifecycle_done");

    Ok(())
}
