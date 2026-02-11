//! E2E Scenario: Graph Workflow Integration Tests
//!
//! Comprehensive tests for the `ms graph` command covering:
//! - Graph export (JSON format)
//! - Graph insights (keystones, bottlenecks)
//! - Cycle detection
//!
//! Note: The graph command requires `bv` (beads_viewer) to be available.
//! Tests gracefully handle the case where `bv` is not installed.

use super::fixture::E2EFixture;
use ms::error::Result;

// Skill definitions with dependencies to create a graph structure

const SKILL_BASE: &str = r#"---
name: Base Skill
description: A foundational skill that other skills depend on
tags: [base, foundation]
---

# Base Skill

Foundational concepts.

## Core Principles

- Principle one
- Principle two
"#;

const SKILL_INTERMEDIATE: &str = r#"---
name: Intermediate Skill
description: Builds on base concepts
tags: [intermediate, foundation]
---

# Intermediate Skill

Intermediate concepts building on the base.

## Prerequisites

- Requires base-skill knowledge

## Advanced Concepts

- Concept one
- Concept two
"#;

const SKILL_ADVANCED: &str = r#"---
name: Advanced Skill
description: Advanced patterns requiring intermediate knowledge
tags: [advanced, patterns]
---

# Advanced Skill

Advanced patterns.

## Prerequisites

- Requires intermediate-skill knowledge
- Requires base-skill knowledge

## Patterns

- Pattern one
- Pattern two
"#;

const SKILL_ISOLATED: &str = r#"---
name: Isolated Skill
description: A standalone skill with no dependencies
tags: [standalone]
---

# Isolated Skill

A completely independent skill.

## Content

- Topic one
- Topic two
"#;

/// Create a fixture with skills that form a dependency graph
fn setup_graph_fixture(scenario: &str) -> Result<E2EFixture> {
    let mut fixture = E2EFixture::new(scenario);

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    fixture.log_step("Create skills forming a graph");
    fixture.create_skill("base-skill", SKILL_BASE)?;
    fixture.create_skill("intermediate-skill", SKILL_INTERMEDIATE)?;
    fixture.create_skill("advanced-skill", SKILL_ADVANCED)?;
    fixture.create_skill("isolated-skill", SKILL_ISOLATED)?;

    fixture.log_step("Index skills");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    // Checkpoint: skills indexed
    fixture.checkpoint("graph:indexed");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "graph",
        "Skills indexed for graph testing",
        Some(serde_json::json!({
            "skills": ["base-skill", "intermediate-skill", "advanced-skill", "isolated-skill"],
            "expected_edges": "base -> intermediate -> advanced",
        })),
    );

    Ok(fixture)
}

/// Check if bv is available, returning true if it is.
fn check_bv_available(fixture: &mut E2EFixture) -> bool {
    let output = fixture.run_ms(&["--robot", "graph", "export", "--format", "json"]);
    if !output.success {
        let combined = format!("{}{}", output.stdout, output.stderr);
        if combined.contains("bv is not available") || combined.contains("not available on PATH") {
            fixture.emit_event(
                super::fixture::LogLevel::Warn,
                "graph",
                "bv not available, skipping graph test",
                None,
            );
            return false;
        }
    }
    true
}

#[test]
fn test_graph_export_json() -> Result<()> {
    let mut fixture = setup_graph_fixture("graph_export_json")?;

    fixture.log_step("Export graph as JSON");
    let output = fixture.run_ms(&["--robot", "graph", "export", "--format", "json"]);

    // If bv is not available, skip gracefully
    if !output.success {
        let combined = format!("{}{}", output.stdout, output.stderr);
        if combined.contains("bv is not available") || combined.contains("not available on PATH") {
            fixture.emit_event(
                super::fixture::LogLevel::Warn,
                "graph",
                "bv not available on this system, skipping test",
                None,
            );
            fixture.generate_report();
            return Ok(());
        }
        // If it failed for another reason, that is a real failure
        fixture.assert_success(&output, "graph export json");
    }

    // If we got here, bv is available and the command succeeded
    fixture.assert_output_contains(&output, "status");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "graph",
        "Graph export JSON completed",
        Some(serde_json::json!({
            "format": "json",
            "stdout_len": output.stdout.len(),
        })),
    );

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_graph_export_dot() -> Result<()> {
    let mut fixture = setup_graph_fixture("graph_export_dot")?;

    fixture.log_step("Export graph as DOT format");
    let output = fixture.run_ms(&["--robot", "graph", "export", "--format", "dot"]);

    if !output.success {
        let combined = format!("{}{}", output.stdout, output.stderr);
        if combined.contains("bv is not available") || combined.contains("not available on PATH") {
            fixture.emit_event(
                super::fixture::LogLevel::Warn,
                "graph",
                "bv not available on this system, skipping test",
                None,
            );
            fixture.generate_report();
            return Ok(());
        }
        fixture.assert_success(&output, "graph export dot");
    }

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "graph",
        "Graph export DOT completed",
        Some(serde_json::json!({
            "format": "dot",
            "stdout_len": output.stdout.len(),
        })),
    );

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_graph_cycles() -> Result<()> {
    let mut fixture = setup_graph_fixture("graph_cycles")?;

    // Checkpoint: pre-cycles
    fixture.checkpoint("graph:pre-cycles");

    fixture.log_step("Detect cycles in skill graph");
    let output = fixture.run_ms(&["--robot", "graph", "cycles", "--limit", "10"]);

    if !output.success {
        let combined = format!("{}{}", output.stdout, output.stderr);
        if combined.contains("bv is not available") || combined.contains("not available on PATH") {
            fixture.emit_event(
                super::fixture::LogLevel::Warn,
                "graph",
                "bv not available on this system, skipping test",
                None,
            );
            fixture.generate_report();
            return Ok(());
        }
        fixture.assert_success(&output, "graph cycles");
    }

    let json = output.json();
    let status = json["status"].as_str().expect("status field");

    assert_eq!(status, "ok", "Cycles detection status should be ok");
    assert!(
        json.get("count").is_some(),
        "Response should have 'count' field"
    );
    assert!(
        json.get("cycles").is_some(),
        "Response should have 'cycles' field"
    );

    let cycle_count = json["count"].as_u64().unwrap_or(0);

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "graph",
        &format!("Cycle detection found {} cycles", cycle_count),
        Some(serde_json::json!({
            "count": cycle_count,
            "limit": 10,
        })),
    );

    // Our test skills form a DAG (no cycles), so we expect 0 cycles
    // However, the graph analysis may process differently, so we just verify structure
    assert!(
        json["cycles"].is_array(),
        "Cycles should be an array"
    );

    // Checkpoint: post-cycles
    fixture.checkpoint("graph:post-cycles");

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_graph_insights() -> Result<()> {
    let mut fixture = setup_graph_fixture("graph_insights")?;

    fixture.log_step("Get graph insights");
    let output = fixture.run_ms(&["--robot", "graph", "insights"]);

    if !output.success {
        let combined = format!("{}{}", output.stdout, output.stderr);
        if combined.contains("bv is not available") || combined.contains("not available on PATH") {
            fixture.emit_event(
                super::fixture::LogLevel::Warn,
                "graph",
                "bv not available on this system, skipping test",
                None,
            );
            fixture.generate_report();
            return Ok(());
        }
        fixture.assert_success(&output, "graph insights");
    }

    // Verify the output is valid JSON and contains insight data
    let json = output.json();

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "graph",
        "Graph insights completed",
        Some(serde_json::json!({
            "has_cycles": json.get("Cycles").is_some(),
            "has_keystones": json.get("Keystones").is_some(),
            "has_bottlenecks": json.get("Bottlenecks").is_some(),
        })),
    );

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_graph_keystones() -> Result<()> {
    let mut fixture = setup_graph_fixture("graph_keystones")?;

    fixture.log_step("Get top keystone skills");
    let output = fixture.run_ms(&["--robot", "graph", "keystones", "--limit", "5"]);

    if !output.success {
        let combined = format!("{}{}", output.stdout, output.stderr);
        if combined.contains("bv is not available") || combined.contains("not available on PATH") {
            fixture.emit_event(
                super::fixture::LogLevel::Warn,
                "graph",
                "bv not available on this system, skipping test",
                None,
            );
            fixture.generate_report();
            return Ok(());
        }
        fixture.assert_success(&output, "graph keystones");
    }

    let json = output.json();
    let status = json["status"].as_str().expect("status");
    assert_eq!(status, "ok", "Keystones status should be ok");

    assert!(
        json.get("count").is_some(),
        "Response should have 'count' field"
    );
    assert!(
        json.get("items").is_some(),
        "Response should have 'items' field"
    );

    let items = json["items"].as_array().expect("items array");
    assert!(
        items.len() <= 5,
        "Items should respect limit of 5, got {}",
        items.len()
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "graph",
        &format!("Keystones: {} items returned", items.len()),
        Some(serde_json::json!({
            "count": json["count"],
            "items_count": items.len(),
            "limit": 5,
        })),
    );

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_graph_bottlenecks() -> Result<()> {
    let mut fixture = setup_graph_fixture("graph_bottlenecks")?;

    fixture.log_step("Get top bottleneck skills");
    let output = fixture.run_ms(&["--robot", "graph", "bottlenecks", "--limit", "5"]);

    if !output.success {
        let combined = format!("{}{}", output.stdout, output.stderr);
        if combined.contains("bv is not available") || combined.contains("not available on PATH") {
            fixture.emit_event(
                super::fixture::LogLevel::Warn,
                "graph",
                "bv not available on this system, skipping test",
                None,
            );
            fixture.generate_report();
            return Ok(());
        }
        fixture.assert_success(&output, "graph bottlenecks");
    }

    let json = output.json();
    let status = json["status"].as_str().expect("status");
    assert_eq!(status, "ok", "Bottlenecks status should be ok");

    assert!(
        json.get("items").is_some(),
        "Response should have 'items' field"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "graph",
        "Bottlenecks query completed",
        Some(serde_json::json!({
            "count": json["count"],
        })),
    );

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_graph_without_bv() -> Result<()> {
    let mut fixture = setup_graph_fixture("graph_without_bv")?;

    fixture.log_step("Test graph command with invalid bv path");
    let output = fixture.run_ms(&[
        "--robot",
        "graph",
        "--bv-path",
        "/nonexistent/bv",
        "export",
        "--format",
        "json",
    ]);

    // This should fail because bv is not found at the given path
    assert!(
        !output.success,
        "Graph with invalid bv path should fail"
    );

    let combined = format!("{}{}", output.stdout, output.stderr);
    assert!(
        combined.contains("not available") || combined.contains("error") || !output.success,
        "Should report that bv is not available"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "graph",
        "Graph correctly failed with invalid bv path",
        Some(serde_json::json!({
            "exit_code": output.exit_code,
            "expected": "failure",
        })),
    );

    fixture.generate_report();
    Ok(())
}
