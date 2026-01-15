//! Beads (bd) integration module.
//!
//! This module provides programmatic access to the beads issue tracker,
//! following the same patterns as other flywheel tools (CASS, UBS, DCG).
//!
//! # Usage
//!
//! ```rust,ignore
//! use meta_skill::beads::{BeadsClient, CreateIssueRequest, IssueType};
//!
//! let client = BeadsClient::new();
//!
//! // Check availability
//! if client.is_available() {
//!     // List ready issues
//!     let ready = client.ready()?;
//!
//!     // Create a new issue
//!     let issue = client.create(
//!         CreateIssueRequest::new("Fix authentication bug")
//!             .with_type(IssueType::Bug)
//!             .with_priority(1)
//!     )?;
//!
//!     // Update status
//!     client.update_status(&issue.id, IssueStatus::InProgress)?;
//! }
//! ```
//!
//! # Testing with MockBeadsClient
//!
//! For testing code that depends on beads operations, use `MockBeadsClient`:
//!
//! ```rust,ignore
//! use meta_skill::beads::{MockBeadsClient, BeadsOperations, test_issue};
//!
//! let mock = MockBeadsClient::new()
//!     .with_issues(vec![test_issue("test-1", "Test issue")]);
//!
//! // Test your code using mock.show(), mock.create(), etc.
//! ```

mod client;
mod mock;
pub mod test_logger;
mod types;

#[cfg(test)]
mod concurrent_tests;
#[cfg(test)]
mod wal_safety_tests;

pub use client::{BeadsClient, SyncStatus};
pub use mock::{test_issue, BeadsErrorKind, BeadsOperations, ErrorInjection, MockBeadsClient};
pub use test_logger::{LogEntry, LogLevel, TestLogger, TestReport};
pub use types::{
    CreateIssueRequest, Dependency, DependencyType, Issue, IssueStatus, IssueType, Priority,
    UpdateIssueRequest, WorkFilter,
};
