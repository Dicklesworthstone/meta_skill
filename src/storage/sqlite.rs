//! SQLite database layer

use std::path::Path;

use rusqlite::Connection;

use crate::error::Result;
use crate::storage::migrations;

/// SQLite database wrapper for skill registry
pub struct Database {
    conn: Connection,
    schema_version: u32,
}

impl Database {
    /// Open database at the given path
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let conn = Connection::open(path)?;

        Self::configure_pragmas(&conn)?;
        let schema_version = migrations::run_migrations(&conn)?;

        Ok(Self {
            conn,
            schema_version,
        })
    }
    
    /// Get a reference to the connection
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Current schema version after migrations.
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    fn configure_pragmas(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA cache_size = -64000;
             PRAGMA mmap_size = 268435456;
             PRAGMA temp_store = MEMORY;
             PRAGMA foreign_keys = ON;",
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_database_creation_and_schema_version() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Database::open(&db_path).unwrap();
        assert!(db_path.exists());
        assert_eq!(db.schema_version(), migrations::SCHEMA_VERSION);
    }

    #[test]
    fn test_wal_mode_enabled() {
        let dir = tempdir().unwrap();
        let db = Database::open(dir.path().join("test.db")).unwrap();
        let mode: String = db
            .conn()
            .query_row("PRAGMA journal_mode;", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode.to_lowercase(), "wal");
    }

    #[test]
    fn test_all_tables_created() {
        let dir = tempdir().unwrap();
        let db = Database::open(dir.path().join("test.db")).unwrap();
        let tables = [
            "skills",
            "skill_aliases",
            "skills_fts",
            "skill_embeddings",
            "skill_packs",
            "skill_slices",
            "skill_evidence",
            "skill_rules",
            "uncertainty_queue",
            "redaction_reports",
            "injection_reports",
            "command_safety_events",
            "skill_usage",
            "skill_usage_events",
            "rule_outcomes",
            "ubs_reports",
            "cm_rule_links",
            "cm_sync_state",
            "skill_experiments",
            "skill_reservations",
            "skill_dependencies",
            "skill_capabilities",
            "build_sessions",
            "config",
            "tx_log",
            "cass_fingerprints",
        ];

        for table in tables {
            let exists: i32 = db
                .conn()
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "Table {} should exist", table);
        }
    }
}
