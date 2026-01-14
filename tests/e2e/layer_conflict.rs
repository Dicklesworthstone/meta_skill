//! E2E Scenario: Multi-Layer Skill Conflict Resolution
//!
//! Tests the behavior when the same skill exists in multiple layers
//! and verifies the correct resolution based on layer priority.
//!
//! Note: Full multi-layer support is a future feature. These tests
//! currently focus on project-layer behavior with placeholders for
//! when multi-layer is fully implemented.

use super::fixture::E2EFixture;
use ms::error::Result;

/// Test basic skill loading from project layer.
#[test]
fn test_project_layer_skill_loading() -> Result<()> {
    let mut fixture = E2EFixture::new("project_layer_skill_loading");

    // Step 1: Initialize
    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");
    fixture.checkpoint("post_init");

    // Step 2: Create skill in project layer
    fixture.log_step("Create skill in project layer");
    fixture.create_skill_in_layer(
        "project-skill",
        r#"---
name: Project Skill
description: A skill in the project layer
tags: [test, project]
---

# Project Layer Skill

This skill exists in the project layer.

## Content

Project-specific content here.
"#,
        "project",
    )?;
    fixture.checkpoint("skill_created");

    // Step 3: Index
    fixture.log_step("Index skills");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");
    fixture.checkpoint("post_index");

    // Step 4: Load skill
    // Note: skill ID is derived from the H1 heading "Project Layer Skill" -> "project-layer-skill"
    fixture.log_step("Load skill");
    let output = fixture.run_ms(&["--robot", "load", "project-layer-skill"]);
    fixture.assert_success(&output, "load");
    // Load command returns skill body content, just verify it succeeded
    fixture.checkpoint("post_load");

    fixture.generate_report();
    Ok(())
}

/// Test multiple project-layer skills.
#[test]
fn test_multiple_project_skills() -> Result<()> {
    let mut fixture = E2EFixture::new("multiple_project_skills");

    fixture.log_step("Initialize");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    // Create multiple skills in project layer
    fixture.log_step("Create skills in project layer");

    fixture.create_skill_in_layer(
        "skill-alpha",
        r#"---
name: Skill Alpha
description: First test skill
tags: [test, alpha]
---

# Skill Alpha

First skill for testing.
"#,
        "project",
    )?;

    fixture.create_skill_in_layer(
        "skill-beta",
        r#"---
name: Skill Beta
description: Second test skill
tags: [test, beta]
---

# Skill Beta

Second skill for testing.
"#,
        "project",
    )?;

    fixture.create_skill_in_layer(
        "skill-gamma",
        r#"---
name: Skill Gamma
description: Third test skill
tags: [test, gamma]
---

# Skill Gamma

Third skill for testing.
"#,
        "project",
    )?;

    fixture.checkpoint("skills_created");

    // Index all skills
    fixture.log_step("Index all skills");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    // List should show all three
    fixture.log_step("List all skills");
    let output = fixture.run_ms(&["--robot", "list"]);
    fixture.assert_success(&output, "list");

    let json = output.json();
    let skills = json["skills"].as_array().expect("skills array");
    assert!(skills.len() >= 3, "Should have at least 3 skills indexed");

    // Load each skill
    for name in &["skill-alpha", "skill-beta", "skill-gamma"] {
        let output = fixture.run_ms(&["--robot", "load", name]);
        fixture.assert_success(&output, &format!("load {}", name));
    }

    fixture.generate_report();
    Ok(())
}

/// Placeholder test for multi-layer priority (future feature).
#[test]
#[ignore = "Multi-layer priority not yet implemented"]
fn test_layer_priority_resolution() -> Result<()> {
    let mut fixture = E2EFixture::new("layer_priority_resolution");

    fixture.log_step("Initialize");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    // When multi-layer is implemented:
    // 1. Create same skill in global and project layers
    // 2. Index both layers
    // 3. Verify project layer takes precedence

    fixture.log_step("Create skill in global layer");
    fixture.create_skill_in_layer(
        "layered-skill",
        r#"---
name: Layered Skill (Global)
description: Global version
tags: [global]
---

# Layered Skill - GLOBAL

Global version content.
"#,
        "global",
    )?;

    fixture.log_step("Create skill in project layer");
    fixture.create_skill_in_layer(
        "layered-skill",
        r#"---
name: Layered Skill (Project)
description: Project version
tags: [project]
---

# Layered Skill - PROJECT

Project version content (should take precedence).
"#,
        "project",
    )?;

    // When implemented, index would see both and resolve priority
    fixture.log_step("Index with multi-layer support");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    // When implemented, load should return project version
    fixture.log_step("Load layered skill");
    let output = fixture.run_ms(&["--robot", "load", "layered-skill"]);
    fixture.assert_success(&output, "load");
    // fixture.assert_output_contains(&output, "PROJECT");

    fixture.generate_report();
    Ok(())
}
