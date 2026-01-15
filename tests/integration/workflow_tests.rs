use serde_json::Value;

use super::fixture::{TestFixture, TestSkill};

fn parse_json(output: &str) -> Value {
    serde_json::from_str(output).expect("stdout should be valid JSON")
}

#[test]
fn test_full_workflow() {
    let mut fixture = TestFixture::new("test_full_workflow");

    let init = fixture.run_ms(&["--robot", "init"]);
    assert!(init.success, "init failed: {}", init.stderr);

    let skill = TestSkill::new("workflow-skill", "Skill for workflow testing");
    fixture.add_skill(&skill);

    let index = fixture.run_ms(&["--robot", "index"]);
    assert!(index.success, "index failed: {}", index.stderr);
    let index_json = parse_json(&index.stdout);
    assert_eq!(index_json["status"], "ok");

    let list = fixture.run_ms(&["--robot", "list"]);
    assert!(list.success, "list failed: {}", list.stderr);
    let list_json = parse_json(&list.stdout);
    assert!(
        list_json["skills"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["id"] == "workflow-skill")
    );

    let show = fixture.run_ms(&["--robot", "show", "workflow-skill"]);
    assert!(show.success, "show failed: {}", show.stderr);
    let show_json = parse_json(&show.stdout);
    assert_eq!(show_json["status"], "ok");

    let search = fixture.run_ms(&["--robot", "search", "workflow"]);
    assert!(search.success, "search failed: {}", search.stderr);
    let search_json = parse_json(&search.stdout);
    assert_eq!(search_json["status"], "ok");
    assert!(
        search_json["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["id"] == "workflow-skill")
    );

    fixture.open_db();
    fixture.verify_db_state(
        |db| {
            let count: i64 = db
                .query_row("SELECT COUNT(*) FROM skills", [], |r| r.get(0))
                .unwrap_or(0);
            count == 1
        },
        "Workflow should leave 1 skill indexed",
    );
}

#[test]
fn test_error_recovery_flow() {
    let mut fixture = TestFixture::new("test_error_recovery_flow");

    let init = fixture.run_ms(&["--robot", "init"]);
    assert!(init.success, "init failed: {}", init.stderr);

    let show = fixture.run_ms(&["show", "missing-skill"]);
    assert!(!show.success, "show should fail for missing skill");

    let skill = TestSkill::new("recovery-skill", "Skill for recovery test");
    fixture.add_skill(&skill);

    let index = fixture.run_ms(&["--robot", "index"]);
    assert!(index.success, "index failed after error: {}", index.stderr);

    let show_ok = fixture.run_ms(&["show", "recovery-skill"]);
    assert!(show_ok.success, "show should succeed after recovery");
}
