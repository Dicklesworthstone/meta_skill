//! Storage layer for ms
//!
//! Implements dual persistence: `SQLite` for queries, Git for audit/versioning.

pub mod git;
pub mod migrations;
pub mod sqlite;
pub mod tombstone;
pub mod tx;

pub use git::GitArchive;
pub use sqlite::{Database, SkillRecord};
pub use tombstone::{PurgeResult, RestoreResult, TombstoneManager, TombstoneRecord};
pub use tx::{GlobalLock, RecoveryReport, TxManager, TxPhase, TxRecord};
