use fsqlite_core::connection::Connection;
use fsqlite_error::FrankenError;
use fsqlite_types::SqliteValue;
use std::error::Error;

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

fn open_wal_db(path: &str) -> TestResult<Connection> {
    let conn = Connection::open(path)?;
    conn.execute("PRAGMA journal_mode = WAL")?;
    Ok(conn)
}

fn setup_table(conn: &Connection) -> TestResult {
    conn.execute("CREATE TABLE t(pk INTEGER PRIMARY KEY, v TEXT)")?;
    Ok(())
}

fn commit_insert(conn: &Connection, pk: i64, value: &str) -> TestResult {
    conn.execute("BEGIN")?;
    conn.execute_with_params(
        "INSERT INTO t(pk, v) VALUES(?1, ?2)",
        &[
            SqliteValue::Integer(pk),
            SqliteValue::Text(value.to_owned().into()),
        ],
    )?;
    conn.execute("COMMIT")?;
    Ok(())
}

fn commit_update(conn: &Connection, pk: i64, value: &str) -> TestResult {
    conn.execute("BEGIN")?;
    conn.execute_with_params(
        "UPDATE t SET v = ?1 WHERE pk = ?2",
        &[
            SqliteValue::Text(value.to_owned().into()),
            SqliteValue::Integer(pk),
        ],
    )?;
    conn.execute("COMMIT")?;
    Ok(())
}

fn commit_delete(conn: &Connection, pk: i64) -> TestResult {
    conn.execute("BEGIN")?;
    conn.execute_with_params("DELETE FROM t WHERE pk = ?1", &[SqliteValue::Integer(pk)])?;
    conn.execute("COMMIT")?;
    Ok(())
}

fn text_at(row: &fsqlite_core::connection::Row, column: usize) -> TestResult<String> {
    match &row.values()[column] {
        SqliteValue::Text(value) => Ok(value.to_string()),
        other => Err(format!("expected text column {column}, got {other:?}").into()),
    }
}

// Regression test for issue #72: prepared-statement plan cache was not
// invalidated at commit time, so a `SELECT ... WHERE pk = ?` fired on the
// same connection immediately after an UPDATE returned stale (pre-UPDATE)
// rows — or zero rows when the UPDATE shifted the row image on a path the
// cached plan no longer matched. Prior to the fix downstream crates
// (beads_rust) worked around this by wrapping re-reads in CTEs; with the
// commit-time `clear_compilation_reuse_caches` call the plain shape works.
#[test]
fn prepared_select_sees_update_from_same_connection_after_cache_warmup() -> TestResult {
    let dir = tempfile::tempdir()?;
    let db_path = dir.path().join("same-conn-update-staleness.db");
    let db_path = db_path.to_string_lossy().into_owned();

    let conn = open_wal_db(&db_path)?;
    setup_table(&conn)?;
    commit_insert(&conn, 1, "before")?;

    // Warm the prepared-statement / planner-directive caches with a read.
    let sql = "SELECT v FROM t WHERE pk = ?1";
    let warm_rows = conn.query_with_params(sql, &[SqliteValue::Integer(1)])?;
    assert_eq!(warm_rows.len(), 1);
    assert_eq!(text_at(&warm_rows[0], 0)?, "before");

    // Commit a write on the same handle.
    commit_update(&conn, 1, "after")?;

    // The critical assertion: a plain `SELECT ... WHERE pk = ?` reissued on
    // the same handle must see the post-UPDATE value. Without
    // clear_compilation_reuse_caches at commit time this returned "before"
    // (or zero rows, depending on which cache was consulted).
    let rows = conn.query_with_params(sql, &[SqliteValue::Integer(1)])?;
    assert_eq!(rows.len(), 1, "post-UPDATE re-read must find the row");
    assert_eq!(text_at(&rows[0], 0)?, "after");

    // Also verify the prepared path explicitly.
    let stmt = conn.prepare(sql)?;
    let prepared_rows = stmt.query_with_params(&[SqliteValue::Integer(1)])?;
    assert_eq!(prepared_rows.len(), 1);
    assert_eq!(text_at(&prepared_rows[0], 0)?, "after");

    Ok(())
}

// Regression test for the CTE-shaped same-connection variant documented in
// issue #72. beads_rust wrapped `get_issue_from_conn` in a CTE to escape the
// bare fast-path cache. After an UPDATE on the same handle, the CTE shape
// then returned zero rows while the bare path was correct — confirming the
// cache-invalidation bug could shift between query shapes. With commit-time
// cache clear, both shapes see post-UPDATE state.
#[test]
fn cte_wrapped_select_sees_update_from_same_connection_after_cache_warmup() -> TestResult {
    let dir = tempfile::tempdir()?;
    let db_path = dir.path().join("same-conn-cte-update-staleness.db");
    let db_path = db_path.to_string_lossy().into_owned();

    let conn = open_wal_db(&db_path)?;
    setup_table(&conn)?;
    commit_insert(&conn, 1, "before")?;

    let cte_sql = "WITH target(id_value) AS (SELECT ?1)
                   SELECT t.v FROM t, target WHERE t.pk = target.id_value";

    let warm_rows = conn.query_with_params(cte_sql, &[SqliteValue::Integer(1)])?;
    assert_eq!(warm_rows.len(), 1);
    assert_eq!(text_at(&warm_rows[0], 0)?, "before");

    commit_update(&conn, 1, "after")?;

    let rows = conn.query_with_params(cte_sql, &[SqliteValue::Integer(1)])?;
    assert_eq!(
        rows.len(),
        1,
        "CTE-wrapped re-read must still find the row after commit-time cache clear"
    );
    assert_eq!(text_at(&rows[0], 0)?, "after");

    Ok(())
}

#[test]
fn prepared_select_sees_row_inserted_by_other_connection_after_cache_warmup() -> TestResult {
    let dir = tempfile::tempdir()?;
    let db_path = dir.path().join("insert-staleness.db");
    let db_path = db_path.to_string_lossy().into_owned();

    let conn_a = open_wal_db(&db_path)?;
    setup_table(&conn_a)?;
    commit_insert(&conn_a, 1, "one")?;

    let conn_b = open_wal_db(&db_path)?;
    let sql = "SELECT pk, v FROM t WHERE pk = ?1";
    let stmt = conn_b.prepare(sql)?;
    let warm_rows = stmt.query_with_params(&[SqliteValue::Integer(1)])?;
    assert_eq!(warm_rows.len(), 1);
    assert_eq!(text_at(&warm_rows[0], 1)?, "one");

    commit_insert(&conn_a, 99, "ninety-nine")?;

    let rows = stmt.query_with_params(&[SqliteValue::Integer(99)])?;
    assert_eq!(rows.len(), 1);
    assert_eq!(text_at(&rows[0], 1)?, "ninety-nine");

    let reprepared = conn_b.prepare(sql)?;
    let reprepared_rows = reprepared.query_with_params(&[SqliteValue::Integer(99)])?;
    assert_eq!(reprepared_rows.len(), 1);
    assert_eq!(text_at(&reprepared_rows[0], 1)?, "ninety-nine");
    Ok(())
}

#[test]
fn newly_prepared_select_sees_row_inserted_by_other_connection() -> TestResult {
    let dir = tempfile::tempdir()?;
    let db_path = dir.path().join("new-prepare-insert-staleness.db");
    let db_path = db_path.to_string_lossy().into_owned();

    let conn_a = open_wal_db(&db_path)?;
    setup_table(&conn_a)?;
    commit_insert(&conn_a, 1, "one")?;

    let conn_b = open_wal_db(&db_path)?;
    let warm_rows =
        conn_b.query_with_params("SELECT v FROM t WHERE pk = ?1", &[SqliteValue::Integer(1)])?;
    assert_eq!(warm_rows.len(), 1);
    assert_eq!(text_at(&warm_rows[0], 0)?, "one");

    commit_insert(&conn_a, 2, "two")?;

    let stmt = conn_b.prepare("SELECT v FROM t WHERE pk = ?1")?;
    let rows = stmt.query_with_params(&[SqliteValue::Integer(2)])?;
    assert_eq!(
        rows.len(),
        1,
        "prepare() must not mark a stale row image as current via lightweight metadata refresh"
    );
    assert_eq!(text_at(&rows[0], 0)?, "two");
    Ok(())
}

#[test]
fn prepared_select_sees_row_updated_by_other_connection_after_cache_warmup() -> TestResult {
    let dir = tempfile::tempdir()?;
    let db_path = dir.path().join("update-staleness.db");
    let db_path = db_path.to_string_lossy().into_owned();

    let conn_a = open_wal_db(&db_path)?;
    setup_table(&conn_a)?;
    commit_insert(&conn_a, 1, "before")?;

    let conn_b = open_wal_db(&db_path)?;
    let sql = "SELECT v FROM t WHERE pk = ?1";
    let stmt = conn_b.prepare(sql)?;
    let warm_rows = stmt.query_with_params(&[SqliteValue::Integer(1)])?;
    assert_eq!(warm_rows.len(), 1);
    assert_eq!(text_at(&warm_rows[0], 0)?, "before");

    commit_update(&conn_a, 1, "after")?;

    let rows = stmt.query_with_params(&[SqliteValue::Integer(1)])?;
    assert_eq!(rows.len(), 1);
    assert_eq!(text_at(&rows[0], 0)?, "after");

    let reprepared = conn_b.prepare(sql)?;
    let reprepared_rows = reprepared.query_with_params(&[SqliteValue::Integer(1)])?;
    assert_eq!(reprepared_rows.len(), 1);
    assert_eq!(text_at(&reprepared_rows[0], 0)?, "after");
    Ok(())
}

#[test]
fn prepared_select_stops_returning_row_deleted_by_other_connection_after_cache_warmup() -> TestResult
{
    let dir = tempfile::tempdir()?;
    let db_path = dir.path().join("delete-staleness.db");
    let db_path = db_path.to_string_lossy().into_owned();

    let conn_a = open_wal_db(&db_path)?;
    setup_table(&conn_a)?;
    commit_insert(&conn_a, 1, "doomed")?;

    let conn_b = open_wal_db(&db_path)?;
    let sql = "SELECT v FROM t WHERE pk = ?1";
    let stmt = conn_b.prepare(sql)?;
    let warm_rows = stmt.query_with_params(&[SqliteValue::Integer(1)])?;
    assert_eq!(warm_rows.len(), 1);
    assert_eq!(text_at(&warm_rows[0], 0)?, "doomed");

    commit_delete(&conn_a, 1)?;

    let rows = stmt.query_with_params(&[SqliteValue::Integer(1)])?;
    assert!(
        rows.is_empty(),
        "deleted row must not be returned through stale cursor state: {rows:?}"
    );

    let reprepared = conn_b.prepare(sql)?;
    let reprepared_rows = reprepared.query_with_params(&[SqliteValue::Integer(1)])?;
    assert!(
        reprepared_rows.is_empty(),
        "deleted row must not be returned from prepared cache: {reprepared_rows:?}"
    );
    Ok(())
}

#[test]
fn stale_prepared_statement_rejects_schema_change_from_other_connection() -> TestResult {
    let dir = tempfile::tempdir()?;
    let db_path = dir.path().join("schema-staleness.db");
    let db_path = db_path.to_string_lossy().into_owned();

    let conn_a = open_wal_db(&db_path)?;
    setup_table(&conn_a)?;
    commit_insert(&conn_a, 1, "one")?;

    let conn_b = open_wal_db(&db_path)?;
    let stmt = conn_b.prepare("SELECT v FROM t WHERE pk = ?1")?;
    let warm_rows = stmt.query_with_params(&[SqliteValue::Integer(1)])?;
    assert_eq!(warm_rows.len(), 1);
    assert_eq!(text_at(&warm_rows[0], 0)?, "one");

    conn_a.execute("ALTER TABLE t ADD COLUMN extra TEXT DEFAULT 'fresh'")?;

    let schema_result = stmt.query_with_params(&[SqliteValue::Integer(1)]);
    assert!(
        matches!(schema_result, Err(FrankenError::SchemaChanged)),
        "stale prepared statement should reject schema change, got {schema_result:?}"
    );

    let fresh_stmt = conn_b.prepare("SELECT extra FROM t WHERE pk = ?1")?;
    let fresh_rows = fresh_stmt.query_with_params(&[SqliteValue::Integer(1)])?;
    assert_eq!(fresh_rows.len(), 1);
    assert_eq!(text_at(&fresh_rows[0], 0)?, "fresh");
    Ok(())
}
