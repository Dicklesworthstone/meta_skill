use rusqlite::Connection;

use super::fixture::{TestFixture, TestSkill};

#[test]
fn test_full_workflow_smoke() {
    let fixture = TestFixture::new("full_workflow_smoke");
    let init = fixture.init();
    assert!(init.success, "init failed: {}", init.stderr);

    fixture.add_skill(&TestSkill::new(
        "workflow-skill",
        "Skill for end-to-end workflow smoke test.",
    ));

    let index = fixture.run_ms(&["--robot", "index"]);
    assert!(index.success, "index failed: {}", index.stderr);

    let show = fixture.run_ms(&["--robot", "show", "workflow-skill"]);
    assert!(show.success, "show failed: {}", show.stderr);

    let search = fixture.run_ms(&["--robot", "search", "workflow"]);
    assert!(search.success, "search failed: {}", search.stderr);

    let conn = Connection::open(fixture.db_path()).expect("open db");
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM skills", [], |row| row.get(0))
        .expect("query skills count");
    assert_eq!(count, 1);
}
