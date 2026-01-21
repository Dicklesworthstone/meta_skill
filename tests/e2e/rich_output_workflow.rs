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

fn fetch_skill_id(fixture: &mut E2EFixture) -> String {
    fixture.log_step("Fetch skill id via --robot list");
    let output = fixture.run_ms(&["--robot", "list"]);
    fixture.assert_success(&output, "list --robot");
    let json = output.json();
    let skills = json
        .get("skills")
        .and_then(|v| v.as_array())
        .expect("skills array in list output");
    let first = skills
        .first()
        .and_then(|v| v.get("id"))
        .and_then(|v| v.as_str())
        .expect("skill id in list output");
    first.to_string()
}

fn assert_jsonl(output: &str) -> usize {
    let lines: Vec<_> = output.lines().filter(|l| !l.trim().is_empty()).collect();
    for line in &lines {
        let _: serde_json::Value =
            serde_json::from_str(line).expect("jsonl line should be valid JSON");
    }
    lines.len()
}

#[test]
fn test_agent_env_robot_json_is_plain() -> Result<()> {
    let mut fixture = setup_fixture("rich_output_agent_robot_json")?;

    fixture.log_step("List with --robot and agent environment");
    let output = fixture.run_ms_with_env(&["--robot", "list"], &[("CLAUDE_CODE", "1")]);
    fixture.assert_success(&output, "list --robot (agent)");
    let json = output.json();
    assert!(json.get("status").is_some(), "robot output should have status");
    assert_plain_output(&output.stdout, "agent robot list stdout");

    Ok(())
}

#[test]
fn test_multiple_agent_envs_robot_json_plain() -> Result<()> {
    let mut fixture = setup_fixture("rich_output_multiple_agents_robot")?;

    for (name, env_var) in [
        ("cursor", "CURSOR_AI"),
        ("codex", "OPENAI_CODEX"),
        ("aider", "AIDER_MODE"),
        ("generic", "AGENT_MODE"),
    ] {
        fixture.log_step(&format!("List with --robot and agent env {name}"));
        let output = fixture.run_ms_with_env(&["--robot", "list"], &[(env_var, "1")]);
        fixture.assert_success(&output, &format!("list --robot {name}"));
        let json = output.json();
        assert!(json.get("status").is_some(), "robot output should have status");
        assert_plain_output(&output.stdout, &format!("agent {name} robot list stdout"));
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
fn test_no_color_env_robot_json() -> Result<()> {
    let mut fixture = setup_fixture("rich_output_no_color_robot")?;

    fixture.log_step("List with --robot and NO_COLOR");
    let output = fixture.run_ms_with_env(&["--robot", "list"], &[("NO_COLOR", "1")]);
    fixture.assert_success(&output, "list --robot NO_COLOR");
    let json = output.json();
    assert!(json.get("status").is_some(), "robot output should have status");
    assert_plain_output(&output.stdout, "NO_COLOR robot list stdout");

    Ok(())
}

#[test]
fn test_ci_env_robot_json() -> Result<()> {
    let mut fixture = setup_fixture("rich_output_ci_robot")?;

    fixture.log_step("List with --robot and CI=true");
    let output = fixture.run_ms_with_env(&["--robot", "list"], &[("CI", "true")]);
    fixture.assert_success(&output, "list --robot CI");
    let json = output.json();
    assert!(json.get("status").is_some(), "robot output should have status");
    assert_plain_output(&output.stdout, "CI robot list stdout");

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
fn test_list_jsonl_plain() -> Result<()> {
    let mut fixture = setup_fixture("rich_output_list_jsonl")?;

    fixture.log_step("List with -O jsonl");
    let output = fixture.run_ms(&["-O", "jsonl", "list"]);
    fixture.assert_success(&output, "list jsonl");
    let count = assert_jsonl(&output.stdout);
    assert!(count > 0, "expected jsonl list output lines");
    assert_plain_output(&output.stdout, "list jsonl stdout");

    Ok(())
}

#[test]
fn test_search_jsonl_plain() -> Result<()> {
    let mut fixture = setup_fixture("rich_output_search_jsonl")?;

    fixture.log_step("Search with -O jsonl");
    let output = fixture.run_ms(&[
        "-O",
        "jsonl",
        "search",
        "Output Sample",
        "--search-type",
        "bm25",
    ]);
    fixture.assert_success(&output, "search jsonl");
    let count = assert_jsonl(&output.stdout);
    assert!(count > 0, "expected jsonl search output lines");
    assert_plain_output(&output.stdout, "search jsonl stdout");

    Ok(())
}

#[test]
fn test_suggest_jsonl_plain() -> Result<()> {
    let mut fixture = setup_fixture("rich_output_suggest_jsonl")?;

    fixture.log_step("Suggest with -O jsonl");
    let output = fixture.run_ms(&["-O", "jsonl", "suggest", "--limit", "1"]);
    fixture.assert_success(&output, "suggest jsonl");
    let _count = assert_jsonl(&output.stdout);
    assert_plain_output(&output.stdout, "suggest jsonl stdout");

    Ok(())
}

#[test]
fn test_search_robot_json_plain() -> Result<()> {
    let mut fixture = setup_fixture("rich_output_search_robot")?;

    fixture.log_step("Search with --robot (bm25)");
    let output = fixture.run_ms(&[
        "--robot",
        "search",
        "Output Sample",
        "--search-type",
        "bm25",
    ]);
    fixture.assert_success(&output, "search --robot bm25");
    let json = assert_valid_json(&output.stdout);
    assert_plain_output(&output.stdout, "search robot stdout");
    let count = json.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
    assert!(count > 0, "expected search results in robot output");

    Ok(())
}

#[test]
fn test_show_robot_json_plain() -> Result<()> {
    let mut fixture = setup_fixture("rich_output_show_robot")?;
    let skill_id = fetch_skill_id(&mut fixture);

    fixture.log_step("Show with --robot");
    let output = fixture.run_ms(&["--robot", "show", &skill_id]);
    fixture.assert_success(&output, "show --robot");
    let json = assert_valid_json(&output.stdout);
    assert_plain_output(&output.stdout, "show robot stdout");
    assert!(
        json.get("skill")
            .and_then(|v| v.get("id"))
            .and_then(|v| v.as_str())
            .is_some(),
        "expected skill.id in show output"
    );

    Ok(())
}

#[test]
fn test_load_robot_json_plain() -> Result<()> {
    let mut fixture = setup_fixture("rich_output_load_robot")?;
    let skill_id = fetch_skill_id(&mut fixture);

    fixture.log_step("Load with --robot");
    let output = fixture.run_ms(&["--robot", "load", &skill_id, "--level", "overview"]);
    fixture.assert_success(&output, "load --robot");
    let _json = assert_valid_json(&output.stdout);
    assert_plain_output(&output.stdout, "load robot stdout");

    Ok(())
}

#[test]
fn test_suggest_robot_json_plain() -> Result<()> {
    let mut fixture = setup_fixture("rich_output_suggest_robot")?;

    fixture.log_step("Suggest with --robot");
    let output = fixture.run_ms(&["--robot", "suggest", "--limit", "1"]);
    fixture.assert_success(&output, "suggest --robot");
    let _json = assert_valid_json(&output.stdout);
    assert_plain_output(&output.stdout, "suggest robot stdout");

    Ok(())
}

#[test]
fn test_evidence_list_robot_json_plain() -> Result<()> {
    let mut fixture = setup_fixture("rich_output_evidence_list_robot")?;

    fixture.log_step("Evidence list with --robot");
    let output = fixture.run_ms(&["--robot", "evidence", "list", "--limit", "5"]);
    fixture.assert_success(&output, "evidence list --robot");
    let _json = assert_valid_json(&output.stdout);
    assert_plain_output(&output.stdout, "evidence list robot stdout");

    Ok(())
}

#[test]
fn test_evidence_show_robot_json_plain() -> Result<()> {
    let mut fixture = setup_fixture("rich_output_evidence_show_robot")?;
    let skill_id = fetch_skill_id(&mut fixture);

    fixture.log_step("Evidence show with --robot");
    let output = fixture.run_ms(&["--robot", "evidence", "show", &skill_id]);
    fixture.assert_success(&output, "evidence show --robot");
    let _json = assert_valid_json(&output.stdout);
    assert_plain_output(&output.stdout, "evidence show robot stdout");

    Ok(())
}

#[test]
fn test_search_invalid_layer_robot_error_json() -> Result<()> {
    let mut fixture = setup_fixture("rich_output_search_invalid_layer")?;

    fixture.log_step("Search with invalid layer and --robot");
    let output = fixture.run_ms(&["--robot", "search", "Output", "--layer", "nonsense"]);
    fixture.assert_success(&output, "search invalid layer");
    let json = assert_valid_json(&output.stdout);
    assert!(
        json.get("status").and_then(|v| v.as_str()) == Some("error"),
        "expected error status in search invalid layer"
    );
    assert_plain_output(&output.stdout, "search invalid layer stdout");

    Ok(())
}

#[test]
fn test_load_missing_skill_robot_error_json() -> Result<()> {
    let mut fixture = setup_fixture("rich_output_load_missing_skill")?;

    fixture.log_step("Load missing skill with --robot");
    let output = fixture.run_ms(&["--robot", "load", "missing-skill"]);
    assert!(!output.success, "expected missing skill failure");
    let _json = assert_valid_json(&output.stdout);
    assert_plain_output(&output.stdout, "load missing skill stdout");

    Ok(())
}

#[test]
fn test_evidence_export_invalid_format_robot_error_json() -> Result<()> {
    let mut fixture = setup_fixture("rich_output_evidence_invalid_format")?;

    fixture.log_step("Evidence export with invalid format and --robot");
    let output = fixture.run_ms(&["--robot", "evidence", "export", "--format", "bad"]);
    assert!(!output.success, "expected invalid format failure");
    let _json = assert_valid_json(&output.stdout);
    assert_plain_output(&output.stdout, "evidence export invalid format stdout");

    Ok(())
}

#[test]
fn test_error_output_plain_for_agent() -> Result<()> {
    let mut fixture = setup_fixture("rich_output_agent_error")?;

    fixture.log_step("Show nonexistent skill with --robot and agent env");
    let output = fixture.run_ms_with_env(
        &["--robot", "show", "missing-skill"],
        &[("CLAUDE_CODE", "1")],
    );
    assert!(!output.success, "expected error for missing skill");
    let _json = assert_valid_json(&output.stdout);
    assert_plain_output(&output.stdout, "agent error stdout");

    Ok(())
}
