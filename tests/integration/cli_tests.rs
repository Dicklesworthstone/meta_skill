use serde_json::Value;

use super::fixture::{TestFixture, TestSkill};

#[test]
fn test_init_creates_config() {
    let mut fixture = TestFixture::new("test_init_creates_config");

    let output = fixture.run_ms(&["init"]);

    assert!(output.success, "init command failed");
    assert!(fixture.config_path.exists(), "config.toml not created");

    let config_content = std::fs::read_to_string(&fixture.config_path)
        .expect("Failed to read config");
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
        TestSkill::new("rust-error-handling", "Best practices for error handling in Rust"),
        TestSkill::new("git-workflow", "Standard git branching and merging workflow"),
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
    assert!(output.stdout.contains("test-skill-1"), "Missing skill-1 in output");
    assert!(output.stdout.contains("test-skill-2"), "Missing skill-2 in output");

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

    let mut fixture = TestFixture::with_indexed_skills("test_search_finds_matching_skills", &skills);

    let output = fixture.run_ms(&["search", "async"]);

    assert!(output.success, "search command failed");
    assert!(output.stdout.contains("rust-async"), "Missing rust-async in results");
    assert!(output.stdout.contains("python-async"), "Missing python-async in results");
    assert!(!output.stdout.contains("git-basics"), "git-basics should not match 'async'");

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
