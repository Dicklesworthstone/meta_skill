//! Core types for beads (bd) integration.
//!
//! These types mirror the Go types from beads' `internal/types/types.go`.
//! We implement only the essential subset needed for common CRUD operations.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Issue status in the beads workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueStatus {
    Open,
    InProgress,
    Blocked,
    Deferred,
    Closed,
    Tombstone,
    Pinned,
    Hooked,
}

impl IssueStatus {
    /// Check if the status represents an active (not terminal) state.
    #[must_use] 
    pub const fn is_active(&self) -> bool {
        matches!(
            self,
            Self::Open | Self::InProgress | Self::Blocked | Self::Pinned | Self::Hooked
        )
    }

    /// Check if the status represents a terminal state.
    #[must_use] 
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Closed | Self::Tombstone)
    }
}

impl std::fmt::Display for IssueStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Open => "open",
            Self::InProgress => "in_progress",
            Self::Blocked => "blocked",
            Self::Deferred => "deferred",
            Self::Closed => "closed",
            Self::Tombstone => "tombstone",
            Self::Pinned => "pinned",
            Self::Hooked => "hooked",
        };
        write!(f, "{s}")
    }
}

impl std::str::FromStr for IssueStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "open" => Ok(Self::Open),
            "in_progress" | "in-progress" | "inprogress" => Ok(Self::InProgress),
            "blocked" => Ok(Self::Blocked),
            "deferred" => Ok(Self::Deferred),
            "closed" => Ok(Self::Closed),
            "tombstone" => Ok(Self::Tombstone),
            "pinned" => Ok(Self::Pinned),
            "hooked" => Ok(Self::Hooked),
            _ => Err(format!("unknown issue status: {s}")),
        }
    }
}

/// Issue type classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueType {
    Task,
    Bug,
    Feature,
    Epic,
    Chore,
    Message,
    Gate,
    Agent,
    Role,
    Convoy,
    Event,
    Slot,
    Question,
    Docs,
}

impl std::fmt::Display for IssueType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Task => "task",
            Self::Bug => "bug",
            Self::Feature => "feature",
            Self::Epic => "epic",
            Self::Chore => "chore",
            Self::Message => "message",
            Self::Gate => "gate",
            Self::Agent => "agent",
            Self::Role => "role",
            Self::Convoy => "convoy",
            Self::Event => "event",
            Self::Slot => "slot",
            Self::Question => "question",
            Self::Docs => "docs",
        };
        write!(f, "{s}")
    }
}

impl std::str::FromStr for IssueType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "task" => Ok(Self::Task),
            "bug" => Ok(Self::Bug),
            "feature" => Ok(Self::Feature),
            "epic" => Ok(Self::Epic),
            "chore" => Ok(Self::Chore),
            "message" => Ok(Self::Message),
            "gate" => Ok(Self::Gate),
            "agent" => Ok(Self::Agent),
            "role" => Ok(Self::Role),
            "convoy" => Ok(Self::Convoy),
            "event" => Ok(Self::Event),
            "slot" => Ok(Self::Slot),
            "question" => Ok(Self::Question),
            "docs" => Ok(Self::Docs),
            _ => Err(format!("unknown issue type: {s}")),
        }
    }
}

/// Priority level (0 = critical, 4 = backlog).
pub type Priority = u8;

/// A beads issue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    /// Unique issue ID (e.g., "meta_skill-abc")
    pub id: String,

    /// Issue title
    pub title: String,

    /// Issue description (Markdown)
    #[serde(default)]
    pub description: String,

    /// Current status
    pub status: IssueStatus,

    /// Priority (0-4, lower = higher priority)
    #[serde(default)]
    pub priority: Priority,

    /// Issue type classification
    pub issue_type: IssueType,

    /// Assigned owner (email or username)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,

    /// Assigned worker
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,

    /// Labels/tags
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,

    /// Notes (additional context)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,

    /// Creation timestamp
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateTime<Utc>>,

    /// Creator identity
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,

    /// Last update timestamp
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,

    /// Closed timestamp (if closed)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<DateTime<Utc>>,

    /// Issues that this issue depends on (blockers)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<Dependency>,

    /// Issues that depend on this issue (dependents)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependents: Vec<Dependency>,

    /// Unknown fields captured for forward compatibility
    #[serde(default, skip_serializing_if = "HashMap::is_empty", flatten)]
    pub extra: HashMap<String, JsonValue>,
}

impl Issue {
    /// Check if this issue is ready to work (open and not blocked).
    #[must_use] 
    pub fn is_ready(&self) -> bool {
        self.status == IssueStatus::Open && self.dependencies.is_empty()
    }

    /// Check if this issue is in an active (workable) state.
    #[must_use] 
    pub const fn is_active(&self) -> bool {
        self.status.is_active()
    }
}

/// A dependency relationship between issues.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    /// The related issue ID
    pub id: String,

    /// Title of the related issue
    #[serde(default)]
    pub title: String,

    /// Status of the related issue
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<IssueStatus>,

    /// Type of dependency relationship
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dependency_type: Option<DependencyType>,
}

/// Types of dependency relationships.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyType {
    /// This issue blocks another
    Blocks,
    /// This issue is blocked by another
    BlockedBy,
    /// Parent-child relationship
    Parent,
    Child,
    /// Conditional blocking
    ConditionalBlocks,
    /// Waiting for completion
    WaitsFor,
    /// Tracking relationship
    Tracks,
}

/// Request payload for creating a new issue.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreateIssueRequest {
    /// Issue title (required)
    pub title: String,

    /// Issue description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Issue type (default: task)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_type: Option<IssueType>,

    /// Priority (0-4)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<Priority>,

    /// Labels
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,

    /// Parent issue ID (for epics)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

impl CreateIssueRequest {
    /// Create a new request with the given title.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            ..Default::default()
        }
    }

    /// Set the issue type.
    #[must_use] 
    pub const fn with_type(mut self, issue_type: IssueType) -> Self {
        self.issue_type = Some(issue_type);
        self
    }

    /// Set the priority.
    #[must_use] 
    pub const fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = Some(priority);
        self
    }

    /// Set the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add a label.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.labels.push(label.into());
        self
    }

    /// Set the parent issue.
    pub fn with_parent(mut self, parent: impl Into<String>) -> Self {
        self.parent = Some(parent.into());
        self
    }
}

/// Request payload for updating an issue.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateIssueRequest {
    /// New status
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<IssueStatus>,

    /// New title
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// New description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// New priority
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<Priority>,

    /// New assignee
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,

    /// Labels to add
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub add_labels: Vec<String>,

    /// Labels to remove
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove_labels: Vec<String>,

    /// Notes to append
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl UpdateIssueRequest {
    /// Create an empty update request.
    #[must_use] 
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the status.
    #[must_use] 
    pub const fn with_status(mut self, status: IssueStatus) -> Self {
        self.status = Some(status);
        self
    }

    /// Set the assignee.
    pub fn with_assignee(mut self, assignee: impl Into<String>) -> Self {
        self.assignee = Some(assignee.into());
        self
    }

    /// Set the priority.
    #[must_use] 
    pub const fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = Some(priority);
        self
    }

    /// Add notes.
    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        self.notes = Some(notes.into());
        self
    }
}

/// Filter parameters for listing issues.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkFilter {
    /// Filter by status
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<IssueStatus>,

    /// Filter by type
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_type: Option<IssueType>,

    /// Filter by assignee
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,

    /// Filter by labels (all must match)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,

    /// Maximum priority to include (0-4)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_priority: Option<Priority>,

    /// Limit results
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // IssueStatus tests
    // =========================================================================

    #[test]
    fn test_issue_status_roundtrip() {
        let statuses = [
            IssueStatus::Open,
            IssueStatus::InProgress,
            IssueStatus::Blocked,
            IssueStatus::Deferred,
            IssueStatus::Closed,
            IssueStatus::Tombstone,
            IssueStatus::Pinned,
            IssueStatus::Hooked,
        ];

        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let parsed: IssueStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, parsed);
        }
    }

    #[test]
    fn test_issue_type_roundtrip() {
        let types = [
            IssueType::Task,
            IssueType::Bug,
            IssueType::Feature,
            IssueType::Epic,
            IssueType::Chore,
            IssueType::Question,
            IssueType::Docs,
        ];

        for issue_type in types {
            let json = serde_json::to_string(&issue_type).unwrap();
            let parsed: IssueType = serde_json::from_str(&json).unwrap();
            assert_eq!(issue_type, parsed);
        }
    }

    #[test]
    fn test_issue_status_from_str() {
        assert_eq!("open".parse::<IssueStatus>().unwrap(), IssueStatus::Open);
        assert_eq!(
            "in_progress".parse::<IssueStatus>().unwrap(),
            IssueStatus::InProgress
        );
        assert_eq!(
            "in-progress".parse::<IssueStatus>().unwrap(),
            IssueStatus::InProgress
        );
        assert_eq!(
            "closed".parse::<IssueStatus>().unwrap(),
            IssueStatus::Closed
        );
    }

    #[test]
    fn test_issue_status_from_str_case_insensitive() {
        assert_eq!("OPEN".parse::<IssueStatus>().unwrap(), IssueStatus::Open);
        assert_eq!(
            "InProgress".parse::<IssueStatus>().unwrap(),
            IssueStatus::InProgress
        );
        assert_eq!(
            "BLOCKED".parse::<IssueStatus>().unwrap(),
            IssueStatus::Blocked
        );
    }

    #[test]
    fn test_issue_status_from_str_all_variants() {
        assert_eq!("open".parse::<IssueStatus>().unwrap(), IssueStatus::Open);
        assert_eq!(
            "blocked".parse::<IssueStatus>().unwrap(),
            IssueStatus::Blocked
        );
        assert_eq!(
            "deferred".parse::<IssueStatus>().unwrap(),
            IssueStatus::Deferred
        );
        assert_eq!(
            "closed".parse::<IssueStatus>().unwrap(),
            IssueStatus::Closed
        );
        assert_eq!(
            "tombstone".parse::<IssueStatus>().unwrap(),
            IssueStatus::Tombstone
        );
        assert_eq!(
            "pinned".parse::<IssueStatus>().unwrap(),
            IssueStatus::Pinned
        );
        assert_eq!(
            "hooked".parse::<IssueStatus>().unwrap(),
            IssueStatus::Hooked
        );
    }

    #[test]
    fn test_issue_status_from_str_invalid() {
        assert!("unknown".parse::<IssueStatus>().is_err());
        assert!("".parse::<IssueStatus>().is_err());
        assert!("in progress".parse::<IssueStatus>().is_err());
    }

    #[test]
    fn test_issue_status_from_str_inprogress_variants() {
        // All variants of in_progress should work
        assert_eq!(
            "in_progress".parse::<IssueStatus>().unwrap(),
            IssueStatus::InProgress
        );
        assert_eq!(
            "in-progress".parse::<IssueStatus>().unwrap(),
            IssueStatus::InProgress
        );
        assert_eq!(
            "inprogress".parse::<IssueStatus>().unwrap(),
            IssueStatus::InProgress
        );
    }

    #[test]
    fn test_issue_status_serialization_snake_case() {
        // Verify snake_case serialization for JSON
        assert_eq!(
            serde_json::to_string(&IssueStatus::InProgress).unwrap(),
            "\"in_progress\""
        );
        assert_eq!(
            serde_json::to_string(&IssueStatus::Open).unwrap(),
            "\"open\""
        );
    }

    #[test]
    fn test_issue_status_display() {
        assert_eq!(IssueStatus::Open.to_string(), "open");
        assert_eq!(IssueStatus::InProgress.to_string(), "in_progress");
        assert_eq!(IssueStatus::Blocked.to_string(), "blocked");
        assert_eq!(IssueStatus::Deferred.to_string(), "deferred");
        assert_eq!(IssueStatus::Closed.to_string(), "closed");
        assert_eq!(IssueStatus::Tombstone.to_string(), "tombstone");
        assert_eq!(IssueStatus::Pinned.to_string(), "pinned");
        assert_eq!(IssueStatus::Hooked.to_string(), "hooked");
    }

    #[test]
    fn test_issue_status_is_active_pinned_hooked() {
        // Pinned and Hooked should be active
        assert!(IssueStatus::Pinned.is_active());
        assert!(IssueStatus::Hooked.is_active());
    }

    #[test]
    fn test_issue_status_deferred_not_active() {
        // Deferred is not considered active
        assert!(!IssueStatus::Deferred.is_active());
    }

    #[test]
    fn test_issue_status_is_active() {
        assert!(IssueStatus::Open.is_active());
        assert!(IssueStatus::InProgress.is_active());
        assert!(IssueStatus::Blocked.is_active());
        assert!(!IssueStatus::Closed.is_active());
        assert!(!IssueStatus::Tombstone.is_active());
    }

    #[test]
    fn test_issue_status_is_terminal() {
        assert!(IssueStatus::Closed.is_terminal());
        assert!(IssueStatus::Tombstone.is_terminal());
        assert!(!IssueStatus::Open.is_terminal());
        assert!(!IssueStatus::InProgress.is_terminal());
    }

    #[test]
    fn test_issue_status_terminal_not_active() {
        // Terminal statuses should not be active
        assert!(!IssueStatus::Closed.is_active());
        assert!(!IssueStatus::Tombstone.is_active());
    }

    // =========================================================================
    // IssueType tests
    // =========================================================================

    #[test]
    fn test_issue_type_serialization_snake_case() {
        assert_eq!(serde_json::to_string(&IssueType::Bug).unwrap(), "\"bug\"");
        assert_eq!(serde_json::to_string(&IssueType::Task).unwrap(), "\"task\"");
    }

    #[test]
    fn test_issue_type_display() {
        assert_eq!(IssueType::Task.to_string(), "task");
        assert_eq!(IssueType::Bug.to_string(), "bug");
        assert_eq!(IssueType::Feature.to_string(), "feature");
        assert_eq!(IssueType::Epic.to_string(), "epic");
        assert_eq!(IssueType::Chore.to_string(), "chore");
        assert_eq!(IssueType::Message.to_string(), "message");
        assert_eq!(IssueType::Gate.to_string(), "gate");
        assert_eq!(IssueType::Agent.to_string(), "agent");
        assert_eq!(IssueType::Role.to_string(), "role");
        assert_eq!(IssueType::Convoy.to_string(), "convoy");
        assert_eq!(IssueType::Event.to_string(), "event");
        assert_eq!(IssueType::Slot.to_string(), "slot");
        assert_eq!(IssueType::Question.to_string(), "question");
        assert_eq!(IssueType::Docs.to_string(), "docs");
    }

    #[test]
    fn test_issue_type_from_str() {
        assert_eq!("task".parse::<IssueType>().unwrap(), IssueType::Task);
        assert_eq!("bug".parse::<IssueType>().unwrap(), IssueType::Bug);
        assert_eq!("feature".parse::<IssueType>().unwrap(), IssueType::Feature);
        assert_eq!("epic".parse::<IssueType>().unwrap(), IssueType::Epic);
        assert_eq!("chore".parse::<IssueType>().unwrap(), IssueType::Chore);
        assert_eq!("message".parse::<IssueType>().unwrap(), IssueType::Message);
        assert_eq!("gate".parse::<IssueType>().unwrap(), IssueType::Gate);
        assert_eq!("agent".parse::<IssueType>().unwrap(), IssueType::Agent);
        assert_eq!("role".parse::<IssueType>().unwrap(), IssueType::Role);
        assert_eq!("convoy".parse::<IssueType>().unwrap(), IssueType::Convoy);
        assert_eq!("event".parse::<IssueType>().unwrap(), IssueType::Event);
        assert_eq!("slot".parse::<IssueType>().unwrap(), IssueType::Slot);
        assert_eq!(
            "question".parse::<IssueType>().unwrap(),
            IssueType::Question
        );
        assert_eq!("docs".parse::<IssueType>().unwrap(), IssueType::Docs);
    }

    #[test]
    fn test_issue_type_from_str_case_insensitive() {
        assert_eq!("BUG".parse::<IssueType>().unwrap(), IssueType::Bug);
        assert_eq!("Feature".parse::<IssueType>().unwrap(), IssueType::Feature);
        assert_eq!("TASK".parse::<IssueType>().unwrap(), IssueType::Task);
    }

    #[test]
    fn test_issue_type_from_str_invalid() {
        assert!("unknown".parse::<IssueType>().is_err());
        assert!("".parse::<IssueType>().is_err());
        assert!("feature_request".parse::<IssueType>().is_err());
    }

    // =========================================================================
    // DependencyType tests
    // =========================================================================

    #[test]
    fn test_dependency_type_serialization() {
        assert_eq!(
            serde_json::to_string(&DependencyType::Blocks).unwrap(),
            "\"blocks\""
        );
        assert_eq!(
            serde_json::to_string(&DependencyType::BlockedBy).unwrap(),
            "\"blocked_by\""
        );
        assert_eq!(
            serde_json::to_string(&DependencyType::ConditionalBlocks).unwrap(),
            "\"conditional_blocks\""
        );
    }

    #[test]
    fn test_dependency_type_roundtrip() {
        let types = [
            DependencyType::Blocks,
            DependencyType::BlockedBy,
            DependencyType::Parent,
            DependencyType::Child,
            DependencyType::ConditionalBlocks,
            DependencyType::WaitsFor,
            DependencyType::Tracks,
        ];

        for dep_type in types {
            let json = serde_json::to_string(&dep_type).unwrap();
            let parsed: DependencyType = serde_json::from_str(&json).unwrap();
            assert_eq!(dep_type, parsed);
        }
    }

    // =========================================================================
    // Dependency struct tests
    // =========================================================================

    #[test]
    fn test_dependency_serialization() {
        let dep = Dependency {
            id: "test-456".to_string(),
            title: "Blocker issue".to_string(),
            status: Some(IssueStatus::Open),
            dependency_type: Some(DependencyType::Blocks),
        };

        let json = serde_json::to_string(&dep).unwrap();
        let parsed: Dependency = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "test-456");
        assert_eq!(parsed.title, "Blocker issue");
        assert_eq!(parsed.status, Some(IssueStatus::Open));
        assert_eq!(parsed.dependency_type, Some(DependencyType::Blocks));
    }

    #[test]
    fn test_dependency_minimal() {
        let json = r#"{"id": "dep-1"}"#;
        let dep: Dependency = serde_json::from_str(json).unwrap();
        assert_eq!(dep.id, "dep-1");
        assert_eq!(dep.title, ""); // Default
        assert!(dep.status.is_none());
        assert!(dep.dependency_type.is_none());
    }

    // =========================================================================
    // Issue struct tests
    // =========================================================================

    #[test]
    fn test_issue_deserialization() {
        let json = r#"{
            "id": "test-123",
            "title": "Test Issue",
            "description": "A test issue",
            "status": "open",
            "priority": 2,
            "issue_type": "task"
        }"#;

        let issue: Issue = serde_json::from_str(json).unwrap();
        assert_eq!(issue.id, "test-123");
        assert_eq!(issue.title, "Test Issue");
        assert_eq!(issue.status, IssueStatus::Open);
        assert_eq!(issue.priority, 2);
        assert_eq!(issue.issue_type, IssueType::Task);
    }

    #[test]
    fn test_issue_with_optional_fields() {
        let json = r#"{
            "id": "test-456",
            "title": "Full Issue",
            "description": "Full description",
            "status": "in_progress",
            "priority": 1,
            "issue_type": "feature",
            "owner": "owner@example.com",
            "assignee": "worker@example.com",
            "labels": ["urgent", "backend"],
            "notes": "Some notes here",
            "created_by": "creator@example.com"
        }"#;

        let issue: Issue = serde_json::from_str(json).unwrap();
        assert_eq!(issue.id, "test-456");
        assert_eq!(issue.owner, Some("owner@example.com".to_string()));
        assert_eq!(issue.assignee, Some("worker@example.com".to_string()));
        assert_eq!(issue.labels, vec!["urgent", "backend"]);
        assert_eq!(issue.notes, Some("Some notes here".to_string()));
        assert_eq!(issue.created_by, Some("creator@example.com".to_string()));
    }

    #[test]
    fn test_issue_default_values() {
        let json = r#"{
            "id": "test-min",
            "title": "Minimal Issue",
            "status": "open",
            "issue_type": "task"
        }"#;

        let issue: Issue = serde_json::from_str(json).unwrap();
        assert_eq!(issue.description, ""); // Default
        assert_eq!(issue.priority, 0); // Default
        assert!(issue.labels.is_empty()); // Default
        assert!(issue.owner.is_none());
        assert!(issue.assignee.is_none());
        assert!(issue.dependencies.is_empty());
        assert!(issue.dependents.is_empty());
        assert!(issue.extra.is_empty());
    }

    #[test]
    fn test_issue_unknown_fields_captured() {
        let json = r#"{
            "id": "test-123",
            "title": "Test",
            "status": "open",
            "issue_type": "task",
            "priority": 2,
            "unknown_future_field": "some value",
            "another_new_field": { "nested": true }
        }"#;

        let issue: Issue = serde_json::from_str(json).unwrap();
        assert_eq!(issue.id, "test-123");
        assert!(issue.extra.contains_key("unknown_future_field"));
        assert!(issue.extra.contains_key("another_new_field"));
    }

    #[test]
    fn test_issue_is_ready() {
        let ready_issue = Issue {
            id: "test-1".to_string(),
            title: "Ready".to_string(),
            description: String::new(),
            status: IssueStatus::Open,
            priority: 0,
            issue_type: IssueType::Task,
            owner: None,
            assignee: None,
            labels: vec![],
            notes: None,
            created_at: None,
            created_by: None,
            updated_at: None,
            closed_at: None,
            dependencies: vec![],
            dependents: vec![],
            extra: HashMap::new(),
        };
        assert!(ready_issue.is_ready());
    }

    #[test]
    fn test_issue_not_ready_has_dependencies() {
        let blocked_issue = Issue {
            id: "test-2".to_string(),
            title: "Blocked".to_string(),
            description: String::new(),
            status: IssueStatus::Open,
            priority: 0,
            issue_type: IssueType::Task,
            owner: None,
            assignee: None,
            labels: vec![],
            notes: None,
            created_at: None,
            created_by: None,
            updated_at: None,
            closed_at: None,
            dependencies: vec![Dependency {
                id: "blocker-1".to_string(),
                title: "Blocker".to_string(),
                status: None,
                dependency_type: None,
            }],
            dependents: vec![],
            extra: HashMap::new(),
        };
        assert!(!blocked_issue.is_ready());
    }

    #[test]
    fn test_issue_not_ready_wrong_status() {
        let in_progress_issue = Issue {
            id: "test-3".to_string(),
            title: "In Progress".to_string(),
            description: String::new(),
            status: IssueStatus::InProgress,
            priority: 0,
            issue_type: IssueType::Task,
            owner: None,
            assignee: None,
            labels: vec![],
            notes: None,
            created_at: None,
            created_by: None,
            updated_at: None,
            closed_at: None,
            dependencies: vec![],
            dependents: vec![],
            extra: HashMap::new(),
        };
        assert!(!in_progress_issue.is_ready());
    }

    #[test]
    fn test_issue_is_active() {
        let json = r#"{
            "id": "test-active",
            "title": "Active Issue",
            "status": "in_progress",
            "issue_type": "task"
        }"#;
        let issue: Issue = serde_json::from_str(json).unwrap();
        assert!(issue.is_active());
    }

    #[test]
    fn test_issue_not_active_when_closed() {
        let json = r#"{
            "id": "test-closed",
            "title": "Closed Issue",
            "status": "closed",
            "issue_type": "task"
        }"#;
        let issue: Issue = serde_json::from_str(json).unwrap();
        assert!(!issue.is_active());
    }

    #[test]
    fn test_issue_with_dependencies() {
        let json = r#"{
            "id": "test-deps",
            "title": "Issue with deps",
            "status": "blocked",
            "issue_type": "task",
            "dependencies": [
                {"id": "dep-1", "title": "First blocker"},
                {"id": "dep-2", "title": "Second blocker", "status": "open"}
            ],
            "dependents": [
                {"id": "child-1", "title": "Child issue"}
            ]
        }"#;
        let issue: Issue = serde_json::from_str(json).unwrap();
        assert_eq!(issue.dependencies.len(), 2);
        assert_eq!(issue.dependents.len(), 1);
        assert_eq!(issue.dependencies[0].id, "dep-1");
        assert_eq!(issue.dependencies[1].status, Some(IssueStatus::Open));
    }

    // =========================================================================
    // CreateIssueRequest tests
    // =========================================================================

    #[test]
    fn test_create_issue_request_new() {
        let req = CreateIssueRequest::new("Test Title");
        assert_eq!(req.title, "Test Title");
        assert!(req.description.is_none());
        assert!(req.issue_type.is_none());
        assert!(req.priority.is_none());
        assert!(req.labels.is_empty());
        assert!(req.parent.is_none());
    }

    #[test]
    fn test_create_issue_request_builder() {
        let req = CreateIssueRequest::new("Test Issue")
            .with_type(IssueType::Bug)
            .with_priority(1)
            .with_description("Bug description")
            .with_label("urgent");

        assert_eq!(req.title, "Test Issue");
        assert_eq!(req.issue_type, Some(IssueType::Bug));
        assert_eq!(req.priority, Some(1));
        assert_eq!(req.description, Some("Bug description".to_string()));
        assert!(req.labels.contains(&"urgent".to_string()));
    }

    #[test]
    fn test_create_issue_request_multiple_labels() {
        let req = CreateIssueRequest::new("Multi-label")
            .with_label("label1")
            .with_label("label2")
            .with_label("label3");

        assert_eq!(req.labels.len(), 3);
        assert!(req.labels.contains(&"label1".to_string()));
        assert!(req.labels.contains(&"label2".to_string()));
        assert!(req.labels.contains(&"label3".to_string()));
    }

    #[test]
    fn test_create_issue_request_with_parent() {
        let req = CreateIssueRequest::new("Child Issue")
            .with_type(IssueType::Task)
            .with_parent("epic-123");

        assert_eq!(req.parent, Some("epic-123".to_string()));
    }

    #[test]
    fn test_create_issue_request_serialization() {
        let req = CreateIssueRequest::new("Serialization Test")
            .with_type(IssueType::Feature)
            .with_priority(2);

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"title\":\"Serialization Test\""));
        assert!(json.contains("\"issue_type\":\"feature\""));
        assert!(json.contains("\"priority\":2"));
    }

    // =========================================================================
    // UpdateIssueRequest tests
    // =========================================================================

    #[test]
    fn test_update_issue_request_new() {
        let req = UpdateIssueRequest::new();
        assert!(req.status.is_none());
        assert!(req.title.is_none());
        assert!(req.description.is_none());
        assert!(req.priority.is_none());
        assert!(req.assignee.is_none());
        assert!(req.add_labels.is_empty());
        assert!(req.remove_labels.is_empty());
        assert!(req.notes.is_none());
    }

    #[test]
    fn test_update_issue_request_builder() {
        let req = UpdateIssueRequest::new()
            .with_status(IssueStatus::InProgress)
            .with_assignee("agent@example.com")
            .with_notes("Started work");

        assert_eq!(req.status, Some(IssueStatus::InProgress));
        assert_eq!(req.assignee, Some("agent@example.com".to_string()));
        assert_eq!(req.notes, Some("Started work".to_string()));
    }

    #[test]
    fn test_update_issue_request_with_priority() {
        let req = UpdateIssueRequest::new().with_priority(0);

        assert_eq!(req.priority, Some(0));
    }

    #[test]
    fn test_update_issue_request_serialization() {
        let req = UpdateIssueRequest::new()
            .with_status(IssueStatus::Closed)
            .with_priority(1);

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"status\":\"closed\""));
        assert!(json.contains("\"priority\":1"));
    }

    #[test]
    fn test_update_issue_request_add_remove_labels() {
        let mut req = UpdateIssueRequest::new();
        req.add_labels = vec!["new-label".to_string()];
        req.remove_labels = vec!["old-label".to_string()];

        assert_eq!(req.add_labels.len(), 1);
        assert_eq!(req.remove_labels.len(), 1);
        assert!(req.add_labels.contains(&"new-label".to_string()));
        assert!(req.remove_labels.contains(&"old-label".to_string()));
    }

    // =========================================================================
    // WorkFilter tests
    // =========================================================================

    #[test]
    fn test_work_filter_default() {
        let filter = WorkFilter::default();
        assert!(filter.status.is_none());
        assert!(filter.issue_type.is_none());
        assert!(filter.assignee.is_none());
        assert!(filter.labels.is_empty());
        assert!(filter.max_priority.is_none());
        assert!(filter.limit.is_none());
    }

    #[test]
    fn test_work_filter_with_status() {
        let filter = WorkFilter {
            status: Some(IssueStatus::Open),
            ..Default::default()
        };
        assert_eq!(filter.status, Some(IssueStatus::Open));
    }

    #[test]
    fn test_work_filter_with_type() {
        let filter = WorkFilter {
            issue_type: Some(IssueType::Bug),
            ..Default::default()
        };
        assert_eq!(filter.issue_type, Some(IssueType::Bug));
    }

    #[test]
    fn test_work_filter_with_assignee() {
        let filter = WorkFilter {
            assignee: Some("developer@example.com".to_string()),
            ..Default::default()
        };
        assert_eq!(filter.assignee, Some("developer@example.com".to_string()));
    }

    #[test]
    fn test_work_filter_with_labels() {
        let filter = WorkFilter {
            labels: vec!["urgent".to_string(), "backend".to_string()],
            ..Default::default()
        };
        assert_eq!(filter.labels.len(), 2);
    }

    #[test]
    fn test_work_filter_with_max_priority() {
        let filter = WorkFilter {
            max_priority: Some(2),
            ..Default::default()
        };
        assert_eq!(filter.max_priority, Some(2));
    }

    #[test]
    fn test_work_filter_with_limit() {
        let filter = WorkFilter {
            limit: Some(10),
            ..Default::default()
        };
        assert_eq!(filter.limit, Some(10));
    }

    #[test]
    fn test_work_filter_serialization() {
        let filter = WorkFilter {
            status: Some(IssueStatus::InProgress),
            issue_type: Some(IssueType::Task),
            max_priority: Some(2),
            limit: Some(50),
            ..Default::default()
        };

        let json = serde_json::to_string(&filter).unwrap();
        assert!(json.contains("\"status\":\"in_progress\""));
        assert!(json.contains("\"issue_type\":\"task\""));
        assert!(json.contains("\"max_priority\":2"));
        assert!(json.contains("\"limit\":50"));
    }

    #[test]
    fn test_work_filter_roundtrip() {
        let filter = WorkFilter {
            status: Some(IssueStatus::Blocked),
            issue_type: Some(IssueType::Feature),
            assignee: Some("agent".to_string()),
            labels: vec!["p0".to_string()],
            max_priority: Some(1),
            limit: Some(20),
        };

        let json = serde_json::to_string(&filter).unwrap();
        let parsed: WorkFilter = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.status, Some(IssueStatus::Blocked));
        assert_eq!(parsed.issue_type, Some(IssueType::Feature));
        assert_eq!(parsed.assignee, Some("agent".to_string()));
        assert_eq!(parsed.labels, vec!["p0".to_string()]);
        assert_eq!(parsed.max_priority, Some(1));
        assert_eq!(parsed.limit, Some(20));
    }
}
