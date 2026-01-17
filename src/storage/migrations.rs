//! Database migrations

use rusqlite::Connection;

use crate::error::{MsError, Result};

const MIGRATIONS: [&str; 12] = [
    include_str!("../../migrations/001_initial_schema.sql"),
    include_str!("../../migrations/002_add_fts.sql"),
    include_str!("../../migrations/003_add_vectors.sql"),
    include_str!("../../migrations/004_add_acip_quarantine.sql"),
    include_str!("../../migrations/005_add_acip_quarantine_reviews.sql"),
    include_str!("../../migrations/006_add_embedding_metadata.sql"),
    include_str!("../../migrations/007_add_session_quality.sql"),
    include_str!("../../migrations/008_add_skill_experiment_events.sql"),
    include_str!("../../migrations/009_add_skill_feedback.sql"),
    include_str!("../../migrations/010_add_resolution_cache.sql"),
    include_str!("../../migrations/011_add_user_preferences.sql"),
    include_str!("../../migrations/012_add_resolution_warnings.sql"),
];

pub const SCHEMA_VERSION: u32 = MIGRATIONS.len() as u32;

/// Run all migrations on the database
pub fn run_migrations(conn: &Connection) -> Result<u32> {
    let current_version: u32 = conn
        .query_row("PRAGMA user_version;", [], |row| row.get(0))
        .map_err(|err| MsError::TransactionFailed(err.to_string()))?;

    for (idx, sql) in MIGRATIONS.iter().enumerate() {
        let target_version = (idx + 1) as u32;
        if current_version >= target_version {
            continue;
        }

        conn.execute_batch(sql).map_err(|err| {
            MsError::TransactionFailed(format!("migration {target_version} failed: {err}"))
        })?;
        conn.pragma_update(None, "user_version", target_version)
            .map_err(|err| {
                MsError::TransactionFailed(format!(
                    "failed to set user_version {target_version}: {err}"
                ))
            })?;
    }

    Ok(SCHEMA_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get_user_version(conn: &Connection) -> u32 {
        conn.query_row("PRAGMA user_version;", [], |row| row.get(0))
            .unwrap()
    }

    // =========================================================================
    // SCHEMA_VERSION tests
    // =========================================================================

    #[test]
    fn schema_version_matches_migrations_count() {
        assert_eq!(SCHEMA_VERSION, MIGRATIONS.len() as u32);
    }

    #[test]
    fn schema_version_is_12() {
        assert_eq!(SCHEMA_VERSION, 12);
    }

    // =========================================================================
    // MIGRATIONS constant tests
    // =========================================================================

    #[test]
    fn migrations_are_not_empty() {
        for (idx, sql) in MIGRATIONS.iter().enumerate() {
            assert!(!sql.trim().is_empty(), "Migration {} is empty", idx + 1);
        }
    }

    #[test]
    fn migrations_contain_sql() {
        // Each migration should contain some SQL keywords
        for (idx, sql) in MIGRATIONS.iter().enumerate() {
            let lower = sql.to_lowercase();
            let has_sql = lower.contains("create")
                || lower.contains("alter")
                || lower.contains("insert")
                || lower.contains("drop");
            assert!(
                has_sql,
                "Migration {} doesn't appear to contain SQL",
                idx + 1
            );
        }
    }

    // =========================================================================
    // run_migrations tests
    // =========================================================================

    #[test]
    fn run_migrations_on_empty_database() {
        let conn = Connection::open_in_memory().unwrap();
        assert_eq!(get_user_version(&conn), 0);

        let result = run_migrations(&conn).unwrap();
        assert_eq!(result, SCHEMA_VERSION);
        assert_eq!(get_user_version(&conn), SCHEMA_VERSION);
    }

    #[test]
    fn run_migrations_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();

        // Run migrations twice
        let result1 = run_migrations(&conn).unwrap();
        let result2 = run_migrations(&conn).unwrap();

        // Both should return the same version
        assert_eq!(result1, SCHEMA_VERSION);
        assert_eq!(result2, SCHEMA_VERSION);
        assert_eq!(get_user_version(&conn), SCHEMA_VERSION);
    }

    #[test]
    fn run_migrations_multiple_times() {
        let conn = Connection::open_in_memory().unwrap();

        // Run migrations 5 times
        for _ in 0..5 {
            let result = run_migrations(&conn).unwrap();
            assert_eq!(result, SCHEMA_VERSION);
        }
    }

    #[test]
    fn run_migrations_already_at_latest() {
        let conn = Connection::open_in_memory().unwrap();

        // Run migrations to bring database to latest version
        run_migrations(&conn).unwrap();

        // Set version to latest (it should already be, but explicitly confirm)
        conn.pragma_update(None, "user_version", &SCHEMA_VERSION)
            .unwrap();

        // Running again should be a no-op
        let result = run_migrations(&conn).unwrap();
        assert_eq!(result, SCHEMA_VERSION);
    }

    #[test]
    fn run_migrations_noop_when_fully_migrated() {
        let conn = Connection::open_in_memory().unwrap();

        // Run initial migrations
        run_migrations(&conn).unwrap();
        assert_eq!(get_user_version(&conn), SCHEMA_VERSION);

        // Running again should be a no-op
        let result = run_migrations(&conn).unwrap();
        assert_eq!(result, SCHEMA_VERSION);
    }

    #[test]
    fn run_migrations_creates_skills_table() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        // Verify skills table exists
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='skills'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn run_migrations_creates_tables() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        // Count the number of tables created
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        // Should have created multiple tables
        assert!(count >= 3, "Expected at least 3 tables, got {count}");
    }

    #[test]
    fn run_migrations_creates_indexes() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        // Count the number of indexes created
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='index'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        // Should have created some indexes
        assert!(count >= 1, "Expected at least 1 index, got {count}");
    }
}
