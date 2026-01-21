//! E2E Scenario: Rich Output Integration (plain + machine-readable modes)
//!
//! These tests focus on ensuring agent/CI/robot modes remain plain and parseable.

use super::fixture::E2EFixture;
use crate::common::{assert_plain_output, assert_valid_json};
use ms::error::Result;

const SKILL_SAMPLE: &str = r#"---
name: Output Sample
description: Sample skill for output tests
tags: [output, sample]
---

# Output Sample

Minimal content for list/search/show output tests.
"#;

fn setup_fixture(scenario: &str) -> Result<E2EFixture> {
    let mut fixture = E2EFixture::new(scenario);

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    fixture.log_step("Configure skill paths");
    let output = fixture.run_ms(&["--robot", "config", "skill_paths.project", r#"[\"./skills\"]"#]);
    fixture.assert_success(&output, "config skill_paths.project");

    fixture.log_step("Create sample skill");
    fixture.create_skill("output-sample", SKILL_SAMPLE)?;

    fixture.log_step("Index skills");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    Ok(fixture)
}

#[test]
fn test_agent_env_forces_plain_output() -> Result<()> {
    let mut fixture = setup_fixture("rich_output_agent_plain")?;

    fixture.log_step("List with agent environment");
    let output = fixture.run_ms_with_env(&["list"], &[("CLAUDE_CODE", "1")]);
    fixture.assert_success(&output, "list (agent)");
    assert_plain_output(&output.stdout, "agent mode list stdout");
    assert_plain_output(&output.stderr, "agent mode list stderr");

    Ok(())
}

#[test]
fn test_multiple_agent_envs_plain_output() -> Result<()> {
    let mut fixture = setup_fixture("rich_output_multiple_agents")?;

    for (name, env_var) in [
        ("cursor", "CURSOR_AI"),
        ("codex", "OPENAI_CODEX"),
        ("aider", "AIDER_MODE"),
        ("generic", "AGENT_MODE"),
    ] {
        fixture.log_step(&format!("List with agent env {name}"));
        let output = fixture.run_ms_with_env(&["list"], &[(env_var, "1")]);
        fixture.assert_success(&output, &format!("list {name}"));
        assert_plain_output(&output.stdout, &format!("agent {name} list stdout"));
    }

    Ok(())
}

#[test]
fn test_robot_flag_emits_valid_json() -> Result<()> {
    let mut fixture = setup_fixture("rich_output_robot_json")?;

    fixture.log_step("List with --robot");
    let output = fixture.run_ms(&["--robot", "list"]);
    fixture.assert_success(&output, "list --robot");
    let json = output.json();
    assert!(json.get("status").is_some(), "robot output should have status");
    assert_plain_output(&output.stdout, "robot mode list stdout");

    Ok(())
}

#[test]
fn test_no_color_env_disables_rich_output() -> Result<()> {
    let mut fixture = setup_fixture("rich_output_no_color")?;

    fixture.log_step("List with NO_COLOR");
    let output = fixture.run_ms_with_env(&["list"], &[("NO_COLOR", "1")]);
    fixture.assert_success(&output, "list NO_COLOR");
    assert_plain_output(&output.stdout, "NO_COLOR list stdout");

    Ok(())
}

#[test]
fn test_ci_env_disables_rich_output() -> Result<()> {
    let mut fixture = setup_fixture("rich_output_ci")?;

    fixture.log_step("List with CI=true");
    let output = fixture.run_ms_with_env(&["list"], &[("CI", "true")]);
    fixture.assert_success(&output, "list CI");
    assert_plain_output(&output.stdout, "CI list stdout");

    Ok(())
}

#[test]
fn test_tsv_output_is_plain_and_tabbed() -> Result<()> {
    let mut fixture = setup_fixture("rich_output_tsv")?;

    fixture.log_step("List with -O tsv");
    let output = fixture.run_ms(&["-O", "tsv", "list"]);
    fixture.assert_success(&output, "list tsv");
    assert_plain_output(&output.stdout, "tsv list stdout");
    assert!(
        output.stdout.contains('\t'),
        "tsv output should contain tab delimiters"
    );

    Ok(())
}

#[test]
fn test_json_output_is_valid() -> Result<()> {
    let mut fixture = setup_fixture("rich_output_json")?;

    fixture.log_step("List with -O json");
    let output = fixture.run_ms(&["-O", "json", "list"]);
    fixture.assert_success(&output, "list json");
    let _json = assert_valid_json(&output.stdout);
    assert_plain_output(&output.stdout, "json list stdout");

    Ok(())
}

#[test]
fn test_machine_readable_overrides_force_rich() -> Result<()> {
    let mut fixture = setup_fixture("rich_output_force_rich_machine")?;

    fixture.log_step("List with -O json and MS_FORCE_RICH");
    let output = fixture.run_ms_with_env(&["-O", "json", "list"], &[("MS_FORCE_RICH", "1")]);
    fixture.assert_success(&output, "list json force rich");
    let _json = assert_valid_json(&output.stdout);
    assert_plain_output(&output.stdout, "json list with force rich stdout");

    Ok(())
}

#[test]
fn test_error_output_plain_for_agent() -> Result<()> {
    let mut fixture = setup_fixture("rich_output_agent_error")?;

    fixture.log_step("Show nonexistent skill with agent env");
    let output = fixture.run_ms_with_env(&["show", "missing-skill"], &[("CLAUDE_CODE", "1")]);
    assert!(!output.success, "expected error for missing skill");
    assert_plain_output(&output.stderr, "agent error stderr");

    Ok(())
}
