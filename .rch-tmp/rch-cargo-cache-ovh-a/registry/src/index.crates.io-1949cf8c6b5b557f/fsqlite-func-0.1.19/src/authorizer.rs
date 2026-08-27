//! Authorizer callback trait (§9.4).
//!
//! The authorizer is consulted during **statement compilation** (not execution)
//! to allow or deny access to database objects and operations. This enables
//! sandboxing untrusted SQL.
//!
//! The [`Authorizer`] trait mirrors SQLite's `sqlite3_set_authorizer` API:
//! each callback receives an [`AuthAction`] code plus up to four optional
//! string parameters providing context (table name, column name, database
//! name, trigger name).

/// SQL operation being authorized.
///
/// Covers all DDL, DML, DROP, and miscellaneous operations that SQLite
/// authorizes during statement compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthAction {
    // -- DDL (8) --
    /// CREATE INDEX (arg1=index name, arg2=table name)
    CreateIndex,
    /// CREATE TABLE (arg1=table name, arg2=None)
    CreateTable,
    /// CREATE TEMP INDEX (arg1=index name, arg2=table name)
    CreateTempIndex,
    /// CREATE TEMP TABLE (arg1=table name, arg2=None)
    CreateTempTable,
    /// CREATE TEMP TRIGGER (arg1=trigger name, arg2=table name)
    CreateTempTrigger,
    /// CREATE TEMP VIEW (arg1=view name, arg2=None)
    CreateTempView,
    /// CREATE TRIGGER (arg1=trigger name, arg2=table name)
    CreateTrigger,
    /// CREATE VIEW (arg1=view name, arg2=None)
    CreateView,

    // -- DML (5) --
    /// DELETE (arg1=table name, arg2=None)
    Delete,
    /// INSERT (arg1=table name, arg2=None)
    Insert,
    /// SELECT (arg1=None, arg2=None)
    Select,
    /// UPDATE (arg1=table name, arg2=column name)
    Update,
    /// READ (arg1=table name, arg2=column name)
    Read,

    // -- DROP (8) --
    /// DROP INDEX (arg1=index name, arg2=table name)
    DropIndex,
    /// DROP TABLE (arg1=table name, arg2=None)
    DropTable,
    /// DROP TEMP INDEX (arg1=index name, arg2=table name)
    DropTempIndex,
    /// DROP TEMP TABLE (arg1=table name, arg2=None)
    DropTempTable,
    /// DROP TEMP TRIGGER (arg1=trigger name, arg2=table name)
    DropTempTrigger,
    /// DROP TEMP VIEW (arg1=view name, arg2=None)
    DropTempView,
    /// DROP TRIGGER (arg1=trigger name, arg2=table name)
    DropTrigger,
    /// DROP VIEW (arg1=view name, arg2=None)
    DropView,

    // -- Miscellaneous (12) --
    /// PRAGMA (arg1=pragma name, arg2=pragma arg or None)
    Pragma,
    /// Transaction control (arg1=operation e.g. "BEGIN", arg2=None)
    Transaction,
    /// ATTACH (arg1=filename, arg2=None)
    Attach,
    /// DETACH (arg1=database name, arg2=None)
    Detach,
    /// ALTER TABLE (arg1=database name, arg2=table name)
    AlterTable,
    /// REINDEX (arg1=index name, arg2=None)
    Reindex,
    /// ANALYZE (arg1=table name, arg2=None)
    Analyze,
    /// CREATE VIRTUAL TABLE (arg1=table name, arg2=module name)
    CreateVtable,
    /// DROP VIRTUAL TABLE (arg1=table name, arg2=module name)
    DropVtable,
    /// Function invocation (arg1=None, arg2=function name)
    Function,
    /// SAVEPOINT (arg1=operation e.g. "BEGIN"/"RELEASE"/"ROLLBACK", arg2=name)
    Savepoint,
    /// Recursive query (arg1=None, arg2=None)
    Recursive,
}

/// Result of an authorization check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthResult {
    /// Allow the operation to proceed.
    Ok,
    /// Deny the operation with an authorization error.
    Deny,
    /// Silently replace the result with NULL (for Read) or skip the
    /// operation where semantics permit.
    Ignore,
}

/// Statement authorizer callback (§9.4).
///
/// Called during SQL **compilation** (not execution) to approve or deny each
/// operation. Used for sandboxing untrusted SQL.
///
/// # Parameters
///
/// - `action`: The type of SQL operation being authorized.
/// - `arg1`/`arg2`: Context-dependent string parameters (table name, column
///   name, index name, etc.). The meaning depends on the [`AuthAction`].
/// - `db_name`: The database name (e.g. `"main"`, `"temp"`, attached name).
/// - `trigger`: If the operation originates inside a trigger, this is the
///   trigger name. `None` for top-level SQL.
pub trait Authorizer: Send + Sync {
    /// Return allow/deny/ignore for the given action.
    fn authorize(
        &self,
        action: AuthAction,
        arg1: Option<&str>,
        arg2: Option<&str>,
        db_name: Option<&str>,
        trigger: Option<&str>,
    ) -> AuthResult;
}

// Keep the old names as type aliases for a smooth transition in
// downstream code that may reference them. Since we don't care about
// backwards compatibility (AGENTS.md), these exist purely for the
// re-export in lib.rs to compile.

/// Alias for [`AuthAction`] (legacy name).
pub type AuthorizerAction = AuthAction;

/// Alias for [`AuthResult`] (legacy name).
pub type AuthorizerDecision = AuthResult;

#[cfg(test)]
mod tests {
    use super::*;

    // -- Read-only sandboxing authorizer --

    struct ReadOnlyAuthorizer;

    impl Authorizer for ReadOnlyAuthorizer {
        fn authorize(
            &self,
            action: AuthAction,
            _arg1: Option<&str>,
            _arg2: Option<&str>,
            _db_name: Option<&str>,
            _trigger: Option<&str>,
        ) -> AuthResult {
            match action {
                AuthAction::Read | AuthAction::Select => AuthResult::Ok,
                _ => AuthResult::Deny,
            }
        }
    }

    #[test]
    fn test_authorizer_allow_select() {
        let auth = ReadOnlyAuthorizer;
        assert_eq!(
            auth.authorize(AuthAction::Select, None, None, Some("main"), None),
            AuthResult::Ok
        );
    }

    #[test]
    fn test_authorizer_deny_insert() {
        let auth = ReadOnlyAuthorizer;
        assert_eq!(
            auth.authorize(AuthAction::Insert, Some("users"), None, Some("main"), None),
            AuthResult::Deny
        );
    }

    #[test]
    fn test_authorizer_ignore_read() {
        // An authorizer that hides a specific column by returning Ignore.
        struct ColumnHider;

        impl Authorizer for ColumnHider {
            fn authorize(
                &self,
                action: AuthAction,
                _arg1: Option<&str>,
                arg2: Option<&str>,
                _db_name: Option<&str>,
                _trigger: Option<&str>,
            ) -> AuthResult {
                if action == AuthAction::Read && arg2 == Some("secret") {
                    return AuthResult::Ignore;
                }
                AuthResult::Ok
            }
        }

        let auth = ColumnHider;
        // Reading the secret column -> Ignore (NULL replacement)
        assert_eq!(
            auth.authorize(
                AuthAction::Read,
                Some("users"),
                Some("secret"),
                Some("main"),
                None
            ),
            AuthResult::Ignore
        );
        // Reading a normal column -> Ok
        assert_eq!(
            auth.authorize(
                AuthAction::Read,
                Some("users"),
                Some("name"),
                Some("main"),
                None
            ),
            AuthResult::Ok
        );
    }

    #[test]
    fn test_authorizer_trigger_context() {
        struct TriggerAwareAuthorizer;

        impl Authorizer for TriggerAwareAuthorizer {
            fn authorize(
                &self,
                _action: AuthAction,
                _arg1: Option<&str>,
                _arg2: Option<&str>,
                _db_name: Option<&str>,
                trigger: Option<&str>,
            ) -> AuthResult {
                if trigger.is_some() {
                    return AuthResult::Deny;
                }
                AuthResult::Ok
            }
        }

        let auth = TriggerAwareAuthorizer;
        // Top-level SQL -> allowed
        assert_eq!(
            auth.authorize(AuthAction::Insert, Some("t"), None, Some("main"), None),
            AuthResult::Ok
        );
        // Inside trigger -> denied
        assert_eq!(
            auth.authorize(
                AuthAction::Insert,
                Some("t"),
                None,
                Some("main"),
                Some("trg_audit")
            ),
            AuthResult::Deny
        );
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn test_auth_action_all_variants() {
        // Verify every AuthAction variant can be constructed and matched.
        let actions = [
            AuthAction::CreateIndex,
            AuthAction::CreateTable,
            AuthAction::CreateTempIndex,
            AuthAction::CreateTempTable,
            AuthAction::CreateTempTrigger,
            AuthAction::CreateTempView,
            AuthAction::CreateTrigger,
            AuthAction::CreateView,
            AuthAction::Delete,
            AuthAction::Insert,
            AuthAction::Select,
            AuthAction::Update,
            AuthAction::Read,
            AuthAction::DropIndex,
            AuthAction::DropTable,
            AuthAction::DropTempIndex,
            AuthAction::DropTempTable,
            AuthAction::DropTempTrigger,
            AuthAction::DropTempView,
            AuthAction::DropTrigger,
            AuthAction::DropView,
            AuthAction::Pragma,
            AuthAction::Transaction,
            AuthAction::Attach,
            AuthAction::Detach,
            AuthAction::AlterTable,
            AuthAction::Reindex,
            AuthAction::Analyze,
            AuthAction::CreateVtable,
            AuthAction::DropVtable,
            AuthAction::Function,
            AuthAction::Savepoint,
            AuthAction::Recursive,
        ];

        // All 33 variants are constructible.
        assert_eq!(actions.len(), 33);

        // Each is pattern-matchable (exhaustive).
        for a in &actions {
            let _ = match a {
                AuthAction::CreateIndex
                | AuthAction::CreateTable
                | AuthAction::CreateTempIndex
                | AuthAction::CreateTempTable
                | AuthAction::CreateTempTrigger
                | AuthAction::CreateTempView
                | AuthAction::CreateTrigger
                | AuthAction::CreateView
                | AuthAction::Delete
                | AuthAction::Insert
                | AuthAction::Select
                | AuthAction::Update
                | AuthAction::Read
                | AuthAction::DropIndex
                | AuthAction::DropTable
                | AuthAction::DropTempIndex
                | AuthAction::DropTempTable
                | AuthAction::DropTempTrigger
                | AuthAction::DropTempView
                | AuthAction::DropTrigger
                | AuthAction::DropView
                | AuthAction::Pragma
                | AuthAction::Transaction
                | AuthAction::Attach
                | AuthAction::Detach
                | AuthAction::AlterTable
                | AuthAction::Reindex
                | AuthAction::Analyze
                | AuthAction::CreateVtable
                | AuthAction::DropVtable
                | AuthAction::Function
                | AuthAction::Savepoint
                | AuthAction::Recursive => true,
            };
        }
    }

    #[test]
    fn test_authorizer_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ReadOnlyAuthorizer>();
    }
}
