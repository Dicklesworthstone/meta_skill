//! Database migrations

use rusqlite::Connection;

use crate::error::{MsError, Result};

const MIGRATIONS: [&str; 3] = [
    include_str!("../../migrations/001_initial_schema.sql"),
    include_str!("../../migrations/002_add_fts.sql"),
    include_str!("../../migrations/003_add_vectors.sql"),
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
            MsError::TransactionFailed(format!(
                "migration {} failed: {err}",
                target_version
            ))
        })?;
        conn.pragma_update(None, "user_version", &target_version)
            .map_err(|err| {
                MsError::TransactionFailed(format!(
                    "failed to set user_version {}: {err}",
                    target_version
                ))
            })?;
    }

    Ok(SCHEMA_VERSION)
}
