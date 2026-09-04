//! Synchronous connection facade over `fsqlite::AsyncConnection`.
//!
//! frankensqlite 0.3 turned the whole `fsqlite::Connection` surface `async`
//! (and its futures are driven by asupersync, not tokio). `ms` is a
//! synchronous CLI whose storage layer — `Database`, `TxManager`,
//! `TombstoneManager`, the resolution cache — is called from ordinary
//! blocking code paths, so propagating `async` through it would have rippled
//! into essentially every command.
//!
//! fsqlite ships the escape hatch for exactly this case: `AsyncConnection`
//! owns the raw connection on a dedicated large-stack worker thread and
//! exposes a complete `*_sync` command surface that channel-hops to it. This
//! module wraps that surface in the small, rusqlite-shaped API the storage
//! layer already speaks (`execute`, `execute_batch`, `query_row`,
//! `query_row_map`, `query_map_collect`), so the call sites are unchanged
//! across the 0.1 → 0.3 upgrade.

use fsqlite::AsyncConnection;
use fsqlite::Row;
use fsqlite::compat::ParamValue;
use fsqlite_error::FrankenError;
use fsqlite_types::value::SqliteValue;

/// A blocking handle to one frankensqlite database.
///
/// Wraps [`AsyncConnection`], whose worker thread owns the engine-side
/// connection for the handle's whole lifetime. Dropping this closes the
/// connection (checkpointing the WAL) and joins that worker, so a `ms`
/// process that exits normally always leaves a checkpointed database behind.
#[derive(Debug)]
pub struct Connection {
    inner: AsyncConnection,
}

impl Connection {
    /// Open (creating if needed) the database at `path`.
    ///
    /// `":memory:"` opens a private in-memory database, as in SQLite.
    pub fn open(path: impl Into<String>) -> Result<Self, FrankenError> {
        Ok(Self {
            inner: AsyncConnection::open_sync(path)?,
        })
    }

    /// Execute a single statement, returning the number of rows it changed.
    pub fn execute(&self, sql: &str) -> Result<usize, FrankenError> {
        self.inner.execute_sync(sql)
    }

    /// Execute zero or more statements, discarding any rows they produce.
    pub fn execute_batch(&self, sql: &str) -> Result<(), FrankenError> {
        self.inner.execute_batch_sync(sql)
    }

    /// Run a query expected to yield exactly one row.
    ///
    /// Returns [`FrankenError::QueryReturnedNoRows`] when nothing matched, so
    /// `fsqlite::compat::OptionalExtension::optional` maps it to `None`.
    pub fn query_row(&self, sql: &str) -> Result<Row, FrankenError> {
        self.inner.query_row_sync(sql)
    }

    /// The rowid of the most recent successful `INSERT` on this connection.
    pub fn last_insert_rowid(&self) -> Result<i64, FrankenError> {
        self.inner.last_insert_rowid_sync()
    }

    /// Begin an explicit transaction.
    pub fn begin_transaction(&self) -> Result<(), FrankenError> {
        self.inner.begin_transaction_sync()
    }

    /// Commit the active explicit transaction.
    pub fn commit_transaction(&self) -> Result<(), FrankenError> {
        self.inner.commit_transaction_sync()
    }

    /// Roll back the active explicit transaction.
    pub fn rollback_transaction(&self) -> Result<(), FrankenError> {
        self.inner.rollback_transaction_sync()
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        // `AsyncConnection`'s own `Drop` deliberately detaches the worker
        // without waiting, because it must never park an arbitrary runtime
        // thread. `ms` is a blocking CLI, so we can afford — and want — the
        // checkpoint-and-join that `close_sync` performs: without it a
        // short-lived `ms` invocation could exit before the worker finished
        // its terminal cleanup. `close_sync` memoizes its outcome and is safe
        // to call on an already-closed connection.
        //
        // A close error here cannot be surfaced (we are in `Drop`) and must
        // not panic: committed data is already durable in the WAL, and the
        // next open recovers it.
        drop(self.inner.close_sync());
    }
}

/// rusqlite-shaped mapping helpers over [`Connection`].
///
/// Mirrors `fsqlite::compat::ConnectionExt` (which is `async` in 0.3) so the
/// storage call sites keep their original shape.
pub trait ConnectionExt {
    /// Run a parameterised query expected to yield exactly one row and map it.
    ///
    /// # Errors
    /// Propagates engine errors, [`FrankenError::QueryReturnedNoRows`] when
    /// nothing matched, and any error returned by `f`.
    fn query_row_map<T, F>(
        &self,
        sql: &str,
        params: &[ParamValue],
        f: F,
    ) -> Result<T, FrankenError>
    where
        F: FnOnce(&Row) -> Result<T, FrankenError>;

    /// Run a parameterised query and collect every mapped row.
    ///
    /// # Errors
    /// Propagates engine errors and any error returned by `f`.
    fn query_map_collect<T, F>(
        &self,
        sql: &str,
        params: &[ParamValue],
        f: F,
    ) -> Result<Vec<T>, FrankenError>
    where
        F: FnMut(&Row) -> Result<T, FrankenError>;

    /// Execute a parameterised statement, returning the number of rows changed.
    ///
    /// # Errors
    /// Propagates engine errors.
    fn execute_compat(&self, sql: &str, params: &[ParamValue]) -> Result<usize, FrankenError>;
}

/// Unwrap the `SqliteValue`s the engine binds from the compat wrappers.
fn to_values(params: &[ParamValue]) -> Vec<SqliteValue> {
    params.iter().map(|p| p.0.clone()).collect()
}

impl ConnectionExt for Connection {
    fn query_row_map<T, F>(&self, sql: &str, params: &[ParamValue], f: F) -> Result<T, FrankenError>
    where
        F: FnOnce(&Row) -> Result<T, FrankenError>,
    {
        let row = self
            .inner
            .query_row_with_params_sync(sql, &to_values(params))?;
        f(&row)
    }

    fn query_map_collect<T, F>(
        &self,
        sql: &str,
        params: &[ParamValue],
        mut f: F,
    ) -> Result<Vec<T>, FrankenError>
    where
        F: FnMut(&Row) -> Result<T, FrankenError>,
    {
        let mut mapped = Vec::new();
        // Streams one row at a time through the worker's bounded channel
        // rather than materialising the whole result set twice.
        self.inner
            .query_with_params_for_each_sync(sql, &to_values(params), |row| {
                mapped.push(f(row)?);
                Ok(())
            })?;
        Ok(mapped)
    }

    fn execute_compat(&self, sql: &str, params: &[ParamValue]) -> Result<usize, FrankenError> {
        self.inner
            .execute_with_params_sync(sql, &to_values(params))
    }
}

#[cfg(test)]
mod tests {
    use super::{Connection, ConnectionExt};
    use crate::ms_params;
    use fsqlite::compat::{OptionalExtension, RowExt};

    fn open_memory() -> Connection {
        let conn = Connection::open(":memory:").unwrap();
        conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)")
            .unwrap();
        conn
    }

    #[test]
    fn execute_and_query_row_round_trip() {
        let conn = open_memory();
        assert_eq!(conn.execute("INSERT INTO t (name) VALUES ('a')").unwrap(), 1);
        let n: i64 = conn
            .query_row("SELECT count(*) FROM t")
            .and_then(|row| row.get_typed(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn query_row_map_binds_params() {
        let conn = open_memory();
        conn.execute_compat("INSERT INTO t (name) VALUES (?)", ms_params!["zed"])
            .unwrap();
        let name: String = conn
            .query_row_map("SELECT name FROM t WHERE name = ?", ms_params!["zed"], |r| {
                r.get_typed(0)
            })
            .unwrap();
        assert_eq!(name, "zed");
    }

    #[test]
    fn query_row_map_missing_row_is_optional_none() {
        let conn = open_memory();
        let missing: Option<String> = conn
            .query_row_map("SELECT name FROM t WHERE name = ?", ms_params!["nope"], |r| {
                r.get_typed(0)
            })
            .optional()
            .unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn query_map_collect_returns_every_row_in_order() {
        let conn = open_memory();
        for name in ["a", "b", "c"] {
            conn.execute_compat("INSERT INTO t (name) VALUES (?)", ms_params![name])
                .unwrap();
        }
        let names: Vec<String> = conn
            .query_map_collect("SELECT name FROM t ORDER BY id", &[], |r| r.get_typed(0))
            .unwrap();
        assert_eq!(names, vec!["a", "b", "c"]);
    }

    #[test]
    fn last_insert_rowid_tracks_inserts() {
        let conn = open_memory();
        conn.execute("INSERT INTO t (name) VALUES ('a')").unwrap();
        let first = conn.last_insert_rowid().unwrap();
        conn.execute("INSERT INTO t (name) VALUES ('b')").unwrap();
        assert_eq!(conn.last_insert_rowid().unwrap(), first + 1);
    }

    #[test]
    fn explicit_transaction_rollback_discards_writes() {
        let conn = open_memory();
        conn.begin_transaction().unwrap();
        conn.execute("INSERT INTO t (name) VALUES ('a')").unwrap();
        conn.rollback_transaction().unwrap();
        let n: i64 = conn
            .query_row("SELECT count(*) FROM t")
            .and_then(|row| row.get_typed(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn explicit_transaction_commit_keeps_writes() {
        let conn = open_memory();
        conn.begin_transaction().unwrap();
        conn.execute("INSERT INTO t (name) VALUES ('a')").unwrap();
        conn.commit_transaction().unwrap();
        let n: i64 = conn
            .query_row("SELECT count(*) FROM t")
            .and_then(|row| row.get_typed(0))
            .unwrap();
        assert_eq!(n, 1);
    }
}
