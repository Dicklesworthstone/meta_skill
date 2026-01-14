use rusqlite::Connection;

use super::fixture::{TestFixture, TestSkill};

#[test]
fn test_db_state_after_index() {
    let skills = vec![
        TestSkill::new("db-state-skill", "Skill for db state verification."),
    ];
    let fixture = TestFixture::with_indexed_skills("db_state_after_index", &skills);

    let conn = Connection::open(fixture.db_path()).expect("open db");
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM skills", [], |row| row.get(0))
        .expect("query skills count");
    assert_eq!(count, 1);

    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM skills WHERE id = ?",
            ["db-state-skill"],
            |row| row.get(0),
        )
        .expect("query skill existence");
    assert_eq!(exists, 1);
}
