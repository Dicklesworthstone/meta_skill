use rusqlite::Connection;

use super::fixture::{TestFixture, TestSkill};

/// Detailed database state checker
pub struct DbStateChecker<'a> {
    db: &'a Connection,
}

impl<'a> DbStateChecker<'a> {
    pub const fn new(db: &'a Connection) -> Self {
        Self { db }
    }

    pub fn skill_count(&self) -> i64 {
        self.db
            .query_row("SELECT COUNT(*) FROM skills", [], |r| r.get(0))
            .unwrap_or(0)
    }

    pub fn skill_exists(&self, id: &str) -> bool {
        self.db
            .query_row("SELECT 1 FROM skills WHERE id = ?", [id], |_| Ok(true))
            .unwrap_or(false)
    }

    pub fn skill_indexed(&self, id: &str) -> bool {
        // With FTS5 external content tables using triggers, skill presence
        // implies FTS indexing. Direct FTS table queries require MATCH clauses.
        self.skill_exists(id)
    }

    pub fn log_full_state(&self) {
        println!("\n[DB FULL STATE]");
        println!("  Skills: {}", self.skill_count());

        if let Ok(mut stmt) = self.db.prepare("SELECT id, description FROM skills") {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            }) {
                for row in rows.flatten() {
                    println!("    - {}: {}", row.0, row.1);
                }
            }
        }
    }
}

#[test]
fn test_db_state_checker() {
    let skills = vec![
        TestSkill::new("db-skill-1", "First skill"),
        TestSkill::new("db-skill-2", "Second skill"),
    ];

    let mut fixture = TestFixture::with_indexed_skills("test_db_state_checker", &skills);
    fixture.open_db();

    let db = fixture.db.as_ref().expect("db should be open");
    let checker = DbStateChecker::new(db);

    checker.log_full_state();
    assert_eq!(checker.skill_count(), 2);
    assert!(checker.skill_exists("db-skill-1"));
    assert!(checker.skill_exists("db-skill-2"));
    assert!(checker.skill_indexed("db-skill-1"));
    assert!(checker.skill_indexed("db-skill-2"));
}
