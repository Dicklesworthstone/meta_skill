use serde_json::Value;

use super::fixture::{TestFixture, TestSkill};

#[test]
fn test_init_creates_config() {
    let mut fixture = TestFixture::new("test_init_creates_config");

    let output = fixture.run_ms(&["init"]);

    assert!(output.success, "init command failed");
    assert!(fixture.config_path.exists(), "config.toml not created");

    let config_content =
        std::fs::read_to_string(&fixture.config_path).expect("Failed to read config");
    assert!(
        config_content.contains("[skill_paths]"),
        "config missing [skill_paths] section"
    );

    fixture.open_db();
    fixture.verify_db_state(
        |db| {
            let count: i64 = db
                .query_row("SELECT COUNT(*) FROM skills", [], |r| r.get(0))
                .unwrap_or(0);
            count == 0
        },
        "No skills after init",
    );
}

#[test]
fn test_init_idempotent() {
    let mut fixture = TestFixture::new("test_init_idempotent");

    let output1 = fixture.run_ms(&["init"]);
    let output2 = fixture.run_ms(&["init"]);

    assert!(output1.success, "first init failed");
    assert!(output2.success, "second init failed");
    assert!(fixture.config_path.exists());

    fixture.open_db();
    fixture.verify_db_state(
        |db| {
            let count: i64 = db
                .query_row("SELECT COUNT(*) FROM skills", [], |r| r.get(0))
                .unwrap_or(0);
            count == 0
        },
        "No skills after repeated init",
    );
}

#[test]
fn test_index_empty_directory() {
    let mut fixture = TestFixture::new("test_index_empty_directory");

    let output = fixture.run_ms(&["--robot", "index"]);

    assert!(output.success, "index command failed");
    let json: Value = serde_json::from_str(&output.stdout).expect("Invalid JSON output");
    assert_eq!(json["indexed"], Value::from(0));

    fixture.open_db();
    fixture.verify_db_state(
        |db| {
            let count: i64 = db
                .query_row("SELECT COUNT(*) FROM skills", [], |r| r.get(0))
                .unwrap_or(0);
            count == 0
        },
        "Should have 0 skills indexed",
    );
}

#[test]
fn test_index_with_skills() {
    let skills = vec![
        TestSkill::new(
            "rust-error-handling",
            "Best practices for error handling in Rust",
        ),
        TestSkill::new(
            "git-workflow",
            "Standard git branching and merging workflow",
        ),
    ];

    let fixture = TestFixture::with_indexed_skills("test_index_with_skills", &skills);

    fixture.verify_db_state(
        |db| {
            let count: i64 = db
                .query_row("SELECT COUNT(*) FROM skills", [], |r| r.get(0))
                .unwrap_or(0);
            count == 2
        },
        "Should have 2 skills indexed",
    );
}

#[test]
fn test_list_shows_indexed_skills() {
    let skills = vec![
        TestSkill::new("test-skill-1", "First test skill"),
        TestSkill::new("test-skill-2", "Second test skill"),
    ];

    let mut fixture = TestFixture::with_indexed_skills("test_list_shows_indexed_skills", &skills);

    let output = fixture.run_ms(&["list"]);

    assert!(output.success, "list command failed");
    assert!(
        output.stdout.contains("test-skill-1"),
        "Missing skill-1 in output"
    );
    assert!(
        output.stdout.contains("test-skill-2"),
        "Missing skill-2 in output"
    );

    fixture.open_db();
    fixture.verify_db_state(
        |db| {
            let count: i64 = db
                .query_row("SELECT COUNT(*) FROM skills", [], |r| r.get(0))
                .unwrap_or(0);
            count == 2
        },
        "List should not alter indexed skills",
    );
}

#[test]
fn test_show_skill_details() {
    let skills = vec![TestSkill::new(
        "detailed-skill",
        "A skill with detailed information",
    )];

    let mut fixture = TestFixture::with_indexed_skills("test_show_skill_details", &skills);

    let output = fixture.run_ms(&["show", "detailed-skill"]);

    assert!(output.success, "show command failed");
    assert!(output.stdout.contains("detailed-skill"));
    assert!(output.stdout.contains("detailed information"));

    fixture.open_db();
    fixture.verify_db_state(
        |db| {
            let count: i64 = db
                .query_row("SELECT COUNT(*) FROM skills", [], |r| r.get(0))
                .unwrap_or(0);
            count == 1
        },
        "Show should not alter indexed skills",
    );
}

#[test]
fn test_show_nonexistent_skill() {
    let mut fixture = TestFixture::new("test_show_nonexistent_skill");

    let output = fixture.run_ms(&["show", "nonexistent-skill"]);

    assert!(!output.success, "show should fail for nonexistent skill");
    assert!(
        output.stderr.contains("not found") || output.exit_code != 0,
        "expected not found error"
    );

    fixture.open_db();
    fixture.verify_db_state(
        |db| {
            let count: i64 = db
                .query_row("SELECT COUNT(*) FROM skills", [], |r| r.get(0))
                .unwrap_or(0);
            count == 0
        },
        "No skills after failed show",
    );
}

#[test]
fn test_search_finds_matching_skills() {
    let skills = vec![
        TestSkill::new("rust-async", "Asynchronous programming patterns in Rust"),
        TestSkill::new("python-async", "Async/await patterns in Python"),
        TestSkill::new("git-basics", "Basic git commands and workflow"),
    ];

    let mut fixture =
        TestFixture::with_indexed_skills("test_search_finds_matching_skills", &skills);

    let output = fixture.run_ms(&["search", "async"]);

    assert!(output.success, "search command failed");
    assert!(
        output.stdout.contains("rust-async"),
        "Missing rust-async in results"
    );
    assert!(
        output.stdout.contains("python-async"),
        "Missing python-async in results"
    );
    assert!(
        !output.stdout.contains("git-basics"),
        "git-basics should not match 'async'"
    );

    fixture.open_db();
    fixture.verify_db_state(
        |db| {
            let count: i64 = db
                .query_row("SELECT COUNT(*) FROM skills", [], |r| r.get(0))
                .unwrap_or(0);
            count == 3
        },
        "Search should not alter indexed skills",
    );
}

// =============================================================================
// Build Command Tests
// =============================================================================

#[test]
fn test_build_requires_source() {
    let fixture = TestFixture::new("test_build_requires_source");

    // Build without --from-cass should show help
    let output = fixture.run_ms(&["build"]);

    // Should succeed but show interactive help since no source specified
    assert!(output.success, "build command should show help");
    assert!(
        output.stdout.contains("Usage:") || output.stdout.contains("--from-cass"),
        "Should show usage information"
    );
}

#[test]
fn test_build_guided_and_auto_mutually_exclusive() {
    let fixture = TestFixture::new("test_build_guided_and_auto_mutually_exclusive");
    let init = fixture.init();
    assert!(init.success, "init failed");

    // --guided and --auto should fail
    let output = fixture.run_ms(&["build", "--guided", "--auto", "--from-cass", "test"]);

    assert!(
        !output.success,
        "guided and auto should be mutually exclusive"
    );
    assert!(
        output.stderr.contains("mutually exclusive") || output.stderr.contains("error"),
        "Should report mutual exclusivity"
    );
}

#[test]
fn test_build_auto_requires_from_cass() {
    let fixture = TestFixture::new("test_build_auto_requires_from_cass");
    let init = fixture.init();
    assert!(init.success, "init failed");

    // --auto without --from-cass should fail
    let output = fixture.run_ms(&["--robot", "build", "--auto"]);

    assert!(
        !output.success,
        "auto build without --from-cass should fail"
    );

    let json: Value = serde_json::from_str(&output.stdout).unwrap_or_default();
    assert!(
        json.get("error").is_some()
            || output.stderr.contains("--from-cass")
            || output.stderr.contains("required"),
        "Should report missing --from-cass"
    );
}

#[test]
fn test_build_resolve_uncertainties_empty() {
    let fixture = TestFixture::new("test_build_resolve_uncertainties_empty");
    let init = fixture.init();
    assert!(init.success, "init failed");

    // Resolve uncertainties with no queue
    let output = fixture.run_ms(&["--robot", "build", "--resolve-uncertainties"]);

    assert!(output.success, "resolve-uncertainties should succeed");

    let json: Value = serde_json::from_str(&output.stdout).expect("Invalid JSON output");

    // Should report queue status
    assert!(
        json.get("status").is_some(),
        "Should have status field: {}",
        output.stdout
    );
}

#[test]
fn test_build_resume_nonexistent() {
    let fixture = TestFixture::new("test_build_resume_nonexistent");
    let init = fixture.init();
    assert!(init.success, "init failed");

    // Resume with nonexistent checkpoint
    let output = fixture.run_ms(&["--robot", "build", "--resume", "nonexistent-session-id"]);

    assert!(
        output.success,
        "resume should handle missing checkpoint gracefully"
    );

    let json: Value = serde_json::from_str(&output.stdout).expect("Invalid JSON output");

    assert!(
        json.get("error").is_some() || json.get("status").is_some(),
        "Should report error or status: {}",
        output.stdout
    );
}

#[test]
fn test_build_auto_no_cass_available() {
    let fixture = TestFixture::new("test_build_auto_no_cass_available");
    let init = fixture.init();
    assert!(init.success, "init failed");

    // Auto build with a query - CASS is not available so should report error
    let output = fixture.run_ms(&["--robot", "build", "--auto", "--from-cass", "test-query"]);

    // May succeed or fail depending on CASS availability
    // The important thing is it doesn't crash and produces structured output
    // Output may be multiple JSON objects separated by newlines
    let has_output = output.stdout.contains("status") || output.stdout.contains("error");

    assert!(
        has_output || !output.stderr.is_empty(),
        "Should produce some output"
    );
}

#[test]
fn test_build_safety_warning_flags() {
    let fixture = TestFixture::new("test_build_safety_warning_flags");
    let init = fixture.init();
    assert!(init.success, "init failed");

    // In robot mode, safety warnings should be bypassed
    // This tests that --no-redact and --no-injection-filter don't cause issues
    let output = fixture.run_ms(&[
        "--robot",
        "build",
        "--auto",
        "--from-cass",
        "test",
        "--no-redact",
        "--no-injection-filter",
    ]);

    // Should produce structured output even with safety flags
    // Output may be multiple JSON objects separated by newlines
    let has_json = output.stdout.contains('{') && output.stdout.contains('}');

    assert!(
        has_json,
        "Should produce JSON output even with safety flags bypassed"
    );
}
