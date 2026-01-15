//! Tests for the TestFixture enhancements

use std::time::Duration;

use super::fixture::{
    CommandOutput, TestBundle, TestFixture, TestSkill, sample_bundles, sample_skills,
};

// Re-export macros for use in this test file
use crate::{
    assert_command_success, assert_exit_code, assert_file_contains, assert_file_exists,
    assert_stderr_contains, assert_stdout_contains,
};

#[test]
fn test_fixture_creates_directory_structure() {
    let fixture = TestFixture::new("test_fixture_creates_directory_structure");

    assert!(fixture.root.exists(), "Root should exist");
    assert!(fixture.skills_dir.exists(), "Skills dir should exist");
}

#[test]
fn test_fixture_with_sample_skills() {
    let fixture = TestFixture::with_sample_skills("test_fixture_with_sample_skills");

    // Check that sample skills were added
    let rust_error_skill = fixture.skills_dir.join("rust-error-handling");
    assert!(
        rust_error_skill.exists(),
        "rust-error-handling skill should exist"
    );

    let git_skill = fixture.skills_dir.join("git-workflow");
    assert!(git_skill.exists(), "git-workflow skill should exist");
}

#[test]
fn test_fixture_with_sample_bundles() {
    let fixture = TestFixture::with_sample_bundles("test_fixture_with_sample_bundles");

    // Check that bundles directory was created
    let bundles_dir = fixture.root.join("bundles");
    assert!(bundles_dir.exists(), "Bundles dir should exist");

    // Check for rust-patterns bundle
    let rust_bundle = bundles_dir.join("rust-patterns");
    assert!(rust_bundle.exists(), "rust-patterns bundle should exist");
}

#[test]
fn test_dump_directory_tree() {
    let fixture = TestFixture::new("test_dump_directory_tree");

    // Add a skill to create some structure
    fixture.add_skill(&TestSkill::new("test-skill", "A test skill"));

    let tree = fixture.dump_directory_tree();

    // Tree should contain the root directory name and skills
    assert!(tree.contains("skills"), "Tree should show skills directory");
    assert!(tree.contains("test-skill"), "Tree should show test-skill");
}

#[test]
fn test_command_output_helpers() {
    let output = CommandOutput {
        success: true,
        exit_code: 0,
        stdout: "hello world".to_string(),
        stderr: "warning: something".to_string(),
        elapsed: Duration::from_millis(100),
    };

    assert!(output.stdout_contains("hello"));
    assert!(output.stderr_contains("warning"));
    assert!(!output.stdout_contains("missing"));

    // Test assert methods
    output.assert_success();
    output.assert_exit_code(0);
}

#[test]
fn test_command_output_json_parsing() {
    let output = CommandOutput {
        success: true,
        exit_code: 0,
        stdout: r#"{"name": "test", "value": 42}"#.to_string(),
        stderr: String::new(),
        elapsed: Duration::from_millis(50),
    };

    let json = output.json();
    assert_eq!(json["name"], "test");
    assert_eq!(json["value"], 42);

    let try_json = output.try_json();
    assert!(try_json.is_some());
}

#[test]
fn test_test_bundle_creation() {
    let bundle = TestBundle::new("my-bundle", "A test bundle");

    assert_eq!(bundle.name, "my-bundle");
    assert!(bundle.manifest.contains("my-bundle"));
    assert!(bundle.manifest.contains("A test bundle"));
    assert!(bundle.skills.is_empty());
}

#[test]
fn test_test_bundle_with_skills() {
    let bundle = TestBundle::with_skills(
        "skill-bundle",
        "Bundle with skills",
        vec![
            ("skill1", "# Skill 1\n\nContent"),
            ("skill2", "# Skill 2\n\nContent"),
        ],
    );

    assert_eq!(bundle.name, "skill-bundle");
    assert_eq!(bundle.skills.len(), 2);
    assert!(bundle.manifest.contains("skill1"));
    assert!(bundle.manifest.contains("skill2"));
}

#[test]
fn test_sample_bundles() {
    let rust = sample_bundles::rust_patterns();
    assert_eq!(rust.name, "rust-patterns");
    assert!(!rust.skills.is_empty());

    let testing = sample_bundles::testing_patterns();
    assert_eq!(testing.name, "testing-patterns");

    let empty = sample_bundles::empty_bundle();
    assert!(empty.skills.is_empty());
}

#[test]
fn test_sample_skills() {
    let error_handling = sample_skills::rust_error_handling();
    assert_eq!(error_handling.name, "rust-error-handling");
    assert!(error_handling.content.contains("Result"));

    let git = sample_skills::git_workflow();
    assert!(git.content.contains("Git"));

    let all = sample_skills::all();
    assert!(all.len() >= 3);
}

#[test]
fn test_assertion_macros() {
    // Create a temp file for testing
    let temp_dir = tempfile::TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.txt");
    std::fs::write(&test_file, "hello world\nfoo bar").unwrap();

    // Test assert_file_exists!
    assert_file_exists!(&test_file);

    // Test assert_file_contains!
    assert_file_contains!(&test_file, "hello");
    assert_file_contains!(&test_file, "foo bar");
}

#[test]
fn test_command_assertion_macros() {
    let success_output = CommandOutput {
        success: true,
        exit_code: 0,
        stdout: "operation completed successfully".to_string(),
        stderr: "debug: processing".to_string(),
        elapsed: Duration::from_millis(10),
    };

    assert_command_success!(success_output);
    assert_exit_code!(success_output, 0);
    assert_stdout_contains!(success_output, "completed");
    assert_stderr_contains!(success_output, "debug");
}

#[test]
fn test_fixture_timing() {
    let fixture = TestFixture::new("test_fixture_timing");

    // Wait a bit
    std::thread::sleep(Duration::from_millis(10));

    let elapsed = fixture.elapsed();
    assert!(elapsed >= Duration::from_millis(10));
}

#[test]
fn test_add_bundle_to_fixture() {
    let fixture = TestFixture::new("test_add_bundle_to_fixture");

    let bundle = TestBundle::with_skills(
        "test-bundle",
        "Test bundle",
        vec![("bundled-skill", "# Bundled Skill\n\nContent here")],
    );

    fixture.add_bundle(&bundle);

    let bundle_dir = fixture.root.join("bundles").join("test-bundle");
    assert!(bundle_dir.exists());

    let manifest = bundle_dir.join("bundle.json");
    assert!(manifest.exists());

    let skill_file = bundle_dir
        .join("skills")
        .join("bundled-skill")
        .join("SKILL.md");
    assert!(skill_file.exists());
}
