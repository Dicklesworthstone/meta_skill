//! Storage layer for ms
//! 
//! Implements dual persistence: SQLite for queries, Git for audit/versioning.

use std::path::Path;

use crate::error::Result;

pub mod sqlite;
pub mod git;
pub mod migrations;

pub use sqlite::Database;
pub use git::GitArchive;
