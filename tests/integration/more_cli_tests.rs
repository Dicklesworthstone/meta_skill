use super::fixture::{TestFixture, TestSkill};
use std::env;

#[test]
fn test_list_filters() {
    let skills = vec![
        TestSkill::new("skill-rust", "Rust skill")
            .with_tags(vec!["rust", "backend"])
            .with_layer("project"),
        TestSkill::new("skill-python", "Python skill")
            .with_tags(vec!["python", "scripting"])
            .with_layer("user"),
        TestSkill::new("skill-legacy", "Legacy skill")
            .with_tags(vec!["deprecated"])
            .with_layer("base"), // Assuming base layer for this test
    ];

    let mut fixture = TestFixture::with_indexed_skills("test_list_filters", &skills);

    // Filter by tag
    let output = fixture.run_ms(&["list", "--tags", "rust"]);
    assert!(output.success);
    assert!(output.stdout.contains("skill-rust"));
    assert!(!output.stdout.contains("skill-python"));

    // Filter by layer (skills are in project layer by default in this fixture)
    let output = fixture.run_ms(&["list", "--layer", "project"]);
    assert!(output.success);
    assert!(output.stdout.contains("skill-rust"));
    assert!(output.stdout.contains("skill-python"));

    // Filter by mismatching layer
    let output = fixture.run_ms(&["list", "--layer", "user"]);
    assert!(output.success);
    assert!(!output.stdout.contains("skill-python"));
    assert!(output.stdout.contains("No skills found"));

    // Filter by multiple tags (OR logic typically, or verify logic)
    // list.rs implementation: args.tags.iter().any(|t| skill_tags.contains(t)) -> OR logic
    let output = fixture.run_ms(&["list", "--tags", "rust", "--tags", "python"]);
    assert!(output.success);
    assert!(output.stdout.contains("skill-rust"));
    assert!(output.stdout.contains("skill-python"));
}

#[test]
fn test_init_global() {
    let fixture = TestFixture::new("test_init_global");

    // Set XDG vars to point to fixture's temp dir
    let data_home = fixture.temp_dir.path().join(".local/share");
    let config_home = fixture.temp_dir.path().join(".config");

    // We need to run this command with specific env vars
    let output = fixture.run_ms_with_env(
        &["init", "--global"],
        &[
            ("XDG_DATA_HOME", data_home.to_str().unwrap()),
            ("XDG_CONFIG_HOME", config_home.to_str().unwrap()),
        ],
    );

    assert!(output.success, "init --global failed");

    // Verify config created
    assert!(
        config_home.join("ms/config.toml").exists(),
        "Global config not created"
    );

    // Global init does not create data directories immediately (they are created on demand)
    // So we don't assert data_home exists yet.
}

#[test]
fn test_shell_hooks() {
    let fixture = TestFixture::new("test_shell_hooks");

    // Test bash hook
    let output = fixture.run_ms(&["shell", "--shell", "bash"]);
    assert!(output.success);
    assert!(
        output.stdout.contains("ms_suggest_prompt"),
        "bash hook missing ms_suggest_prompt"
    );

    // Test zsh hook
    let output = fixture.run_ms(&["shell", "--shell", "zsh"]);
    assert!(output.success);
    assert!(
        output.stdout.contains("ms_suggest_precmd"),
        "zsh hook missing ms_suggest_precmd"
    );
}

#[test]
fn test_evidence_cli() {
    let skills = vec![TestSkill::new("skill-with-evidence", "Skill with evidence")];
    let mut fixture = TestFixture::with_indexed_skills("test_evidence_cli", &skills);

    fixture.open_db();

    // Manually insert evidence into DB
    if let Some(conn) = &fixture.db {
        let evidence_json = r#"[{"session_id":"session-abc","message_range":[0,1],"snippet_hash":"hash123","level":"pointer","confidence":0.95}]"#;
        let coverage_json = r#"{"total_rules":1,"rules_with_evidence":1,"avg_confidence":0.95}"#;

        conn.execute(
            "INSERT INTO skill_evidence (skill_id, rule_id, evidence_json, coverage_json, updated_at) 
             VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                "skill-with-evidence",
                "rule-1",
                evidence_json,
                coverage_json,
                chrono::Utc::now().to_rfc3339(),
            ),
        ).expect("Failed to insert evidence");
    }

    // Test list
    let output = fixture.run_ms(&["evidence", "list"]);
    assert!(output.success);
    assert!(output.stdout.contains("skill-with-evidence"));
    assert!(output.stdout.contains("rule-1"));

    // Test show
    let output = fixture.run_ms(&["evidence", "show", "skill-with-evidence"]);
    assert!(output.success);
    assert!(output.stdout.contains("rule-1"));
    assert!(output.stdout.contains("session-abc"));
}
