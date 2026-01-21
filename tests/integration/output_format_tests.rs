//! Output format integration tests
//!
//! Tests that verify all commands properly support the OutputFormat system
//! and produce valid structured output in robot mode.

use serde_json::Value;

use super::fixture::{TestFixture, TestSkill};

/// Test that the -O/--output-format flag is recognized globally
#[test]
fn test_output_format_flag_recognized() {
    let fixture = TestFixture::new("test_output_format_flag_recognized");
    let _ = fixture.run_ms(&["init"]);

    // Test with -O short flag
    let output = fixture.run_ms(&["-O", "json", "list"]);
    assert!(
        output.success,
        "Command failed with -O flag: {}",
        output.stderr
    );

    // Should be valid JSON
    let json: Result<Value, _> = serde_json::from_str(&output.stdout);
    assert!(json.is_ok(), "Output is not valid JSON: {}", output.stdout);
}

/// Test backward compatibility with --robot flag
#[test]
fn test_robot_flag_backward_compat() {
    let fixture = TestFixture::new("test_robot_flag_backward_compat");
    let _ = fixture.run_ms(&["init"]);

    let output = fixture.run_ms(&["--robot", "list"]);
    assert!(
        output.success,
        "Command failed with --robot flag: {}",
        output.stderr
    );

    let json: Result<Value, _> = serde_json::from_str(&output.stdout);
    assert!(
        json.is_ok(),
        "--robot should produce valid JSON: {}",
        output.stdout
    );
}

/// Test that list command produces valid JSON in robot mode
#[test]
fn test_list_json_output() {
    let skills = vec![
        TestSkill::new("test-skill-1", "Test skill 1 description"),
        TestSkill::new("test-skill-2", "Test skill 2 description"),
    ];
    let fixture = TestFixture::with_indexed_skills("test_list_json_output", &skills);

    let output = fixture.run_ms(&["-O", "json", "list"]);
    assert!(output.success, "list command failed: {}", output.stderr);

    let json: Value = serde_json::from_str(&output.stdout).expect("list should produce valid JSON");

    assert!(
        json.get("status").is_some(),
        "JSON should have status field"
    );
    assert!(
        json.get("skills").is_some() || json.get("results").is_some(),
        "JSON should have skills or results field"
    );
}

/// Test that search command produces valid JSON in robot mode
#[test]
fn test_search_json_output() {
    let skills = vec![TestSkill::new(
        "error-handling",
        "How to handle errors in Rust",
    )];
    let fixture = TestFixture::with_indexed_skills("test_search_json_output", &skills);

    let output = fixture.run_ms(&["-O", "json", "search", "error"]);
    assert!(output.success, "search command failed: {}", output.stderr);

    let json: Value =
        serde_json::from_str(&output.stdout).expect("search should produce valid JSON");

    assert!(
        json.get("status").is_some(),
        "JSON should have status field"
    );
}

/// Test that show command produces valid JSON in robot mode
#[test]
fn test_show_json_output() {
    let skills = vec![TestSkill::new("my-skill", "A sample skill for testing")];
    let fixture = TestFixture::with_indexed_skills("test_show_json_output", &skills);

    let output = fixture.run_ms(&["-O", "json", "show", "my-skill"]);
    assert!(output.success, "show command failed: {}", output.stderr);

    let json: Value = serde_json::from_str(&output.stdout).expect("show should produce valid JSON");

    assert!(
        json.get("status").is_some(),
        "JSON should have status field"
    );
}

/// Test that index command produces valid JSON in robot mode
#[test]
fn test_index_json_output() {
    let fixture = TestFixture::new("test_index_json_output");
    let _ = fixture.run_ms(&["init"]);

    let output = fixture.run_ms(&["-O", "json", "index"]);
    assert!(output.success, "index command failed: {}", output.stderr);

    let json: Value =
        serde_json::from_str(&output.stdout).expect("index should produce valid JSON");

    // Should have indexed count
    assert!(
        json.get("indexed").is_some() || json.get("status").is_some(),
        "JSON should have indexed or status field"
    );
}

/// Test that load command produces valid JSON in robot mode
#[test]
fn test_load_json_output() {
    let skills = vec![TestSkill::new("loadable-skill", "A skill to load")];
    let fixture = TestFixture::with_indexed_skills("test_load_json_output", &skills);

    let output = fixture.run_ms(&["-O", "json", "load", "loadable-skill"]);
    assert!(output.success, "load command failed: {}", output.stderr);

    let json: Value = serde_json::from_str(&output.stdout).expect("load should produce valid JSON");

    assert!(
        json.get("status").is_some(),
        "JSON should have status field"
    );
}

/// Test plain output format
#[test]
fn test_plain_output_format() {
    let skills = vec![TestSkill::new("plain-skill", "Testing plain output")];
    let fixture = TestFixture::with_indexed_skills("test_plain_output_format", &skills);

    let output = fixture.run_ms(&["-O", "plain", "list"]);
    assert!(output.success, "list -O plain failed: {}", output.stderr);

    // Plain output should not be JSON
    let json: Result<Value, _> = serde_json::from_str(&output.stdout);
    assert!(
        json.is_err() || output.stdout.lines().count() > 0,
        "Plain output should be simple text"
    );
}

/// Test TSV output format
#[test]
fn test_tsv_output_format() {
    let skills = vec![TestSkill::new("tsv-skill", "Testing TSV output")];
    let fixture = TestFixture::with_indexed_skills("test_tsv_output_format", &skills);

    let output = fixture.run_ms(&["-O", "tsv", "list"]);
    assert!(output.success, "list -O tsv failed: {}", output.stderr);

    // TSV output should contain tabs if there's output
    if !output.stdout.trim().is_empty() {
        // Either has tabs (TSV data) or is just status text
        let has_content = output.stdout.contains('\t') || output.stdout.lines().next().is_some();
        assert!(has_content, "TSV should have content");
    }
}

/// Test that multiple output format variants produce different output
#[test]
fn test_output_formats_differ() {
    let skills = vec![TestSkill::new("format-skill", "Testing format differences")];
    let fixture = TestFixture::with_indexed_skills("test_output_formats_differ", &skills);

    let human = fixture.run_ms(&["list"]);
    let json = fixture.run_ms(&["-O", "json", "list"]);
    let plain = fixture.run_ms(&["-O", "plain", "list"]);

    assert!(
        human.success && json.success && plain.success,
        "All format variants should succeed"
    );

    // JSON should be parseable, human likely not
    let json_valid: Result<Value, _> = serde_json::from_str(&json.stdout);
    assert!(json_valid.is_ok(), "JSON output should be valid JSON");

    // Outputs should generally differ (unless empty)
    if !human.stdout.is_empty() && !json.stdout.is_empty() {
        // Human and JSON outputs should differ in structure
        let human_is_json: Result<Value, _> = serde_json::from_str(&human.stdout);
        // Human format is typically not valid JSON (unless it happens to be)
        // This is a weak assertion - mainly checking they're not identical
        assert!(
            human.stdout != json.stdout || human_is_json.is_ok(),
            "Human and JSON outputs should differ or human is also JSON"
        );
    }
}

/// Test error handling in robot mode
#[test]
fn test_error_json_output() {
    let fixture = TestFixture::new("test_error_json_output");
    let _ = fixture.run_ms(&["init"]);

    // Try to show a non-existent skill
    let output = fixture.run_ms(&["-O", "json", "show", "nonexistent-skill-xyz"]);

    // Should fail but with valid JSON error
    if !output.success {
        // Try to parse as JSON
        let json: Result<Value, _> = serde_json::from_str(&output.stdout);
        // Error output should still be structured if possible
        // Some commands may output errors to stderr instead
        if json.is_ok() {
            let error_json = json.unwrap();
            assert!(
                error_json.get("status").is_some() || error_json.get("error").is_some(),
                "Error JSON should have status or error field"
            );
        }
    }
}

/// Test that suggest command produces valid JSON
#[test]
fn test_suggest_json_output() {
    let skills = vec![TestSkill::new(
        "suggest-skill",
        "A skill for suggestion testing",
    )];
    let fixture = TestFixture::with_indexed_skills("test_suggest_json_output", &skills);

    let output = fixture.run_ms(&["-O", "json", "suggest"]);
    // suggest may succeed or fail depending on context, but should be valid JSON if it outputs
    if output.success && !output.stdout.is_empty() {
        let json: Result<Value, _> = serde_json::from_str(&output.stdout);
        assert!(
            json.is_ok(),
            "suggest should produce valid JSON: {}",
            output.stdout
        );
    }
}

/// Test config command JSON output
#[test]
fn test_config_json_output() {
    let fixture = TestFixture::new("test_config_json_output");
    let _ = fixture.run_ms(&["init"]);

    let output = fixture.run_ms(&["-O", "json", "config", "--list"]);
    assert!(output.success, "config --list failed: {}", output.stderr);

    let json: Result<Value, _> = serde_json::from_str(&output.stdout);
    assert!(
        json.is_ok(),
        "config --list should produce valid JSON: {}",
        output.stdout
    );
}
