//! Mock `BeadsClient` for testing.
//!
//! Provides a `BeadsOperations` trait and `MockBeadsClient` implementation
//! for unit testing code that depends on beads operations without spawning
//! subprocesses or requiring bd to be installed.
//!
//! # Usage
//!
//! ```rust,ignore
//! use meta_skill::beads::{MockBeadsClient, BeadsOperations, Issue, IssueStatus};
//!
//! // Create a mock client
//! let mock = MockBeadsClient::new();
//!
//! // Pre-populate with issues
//! mock.insert_issue(Issue { /* ... */ });
//!
//! // Inject errors for testing error handling
//! mock.inject_error(ErrorInjection::All(BeadsErrorKind::Unavailable));
//! ```

use std::cell::RefCell;
use std::collections::HashMap;

use chrono::Utc;

use crate::error::{MsError, Result};

use super::types::{
    CreateIssueRequest, Issue, IssueStatus, IssueType, UpdateIssueRequest, WorkFilter,
};

/// Operations available on beads.
///
/// This trait abstracts beads operations for dependency injection in tests.
pub trait BeadsOperations {
    /// Check if beads is available and responsive.
    fn is_available(&self) -> bool;

    /// List all issues matching the filter.
    fn list(&self, filter: &WorkFilter) -> Result<Vec<Issue>>;

    /// List issues ready to work (open and unblocked).
    fn ready(&self) -> Result<Vec<Issue>>;

    /// Get a specific issue by ID.
    fn show(&self, id: &str) -> Result<Issue>;

    /// Create a new issue.
    fn create(&self, request: &CreateIssueRequest) -> Result<Issue>;

    /// Update an existing issue.
    fn update(&self, id: &str, request: &UpdateIssueRequest) -> Result<Issue>;

    /// Update just the status of an issue.
    fn update_status(&self, id: &str, status: IssueStatus) -> Result<Issue>;

    /// Close an issue.
    fn close(&self, id: &str, reason: Option<&str>) -> Result<Issue>;
}

/// Kind of error to inject for testing.
#[derive(Debug, Clone)]
pub enum BeadsErrorKind {
    /// Beads is unavailable.
    Unavailable,
    /// Issue not found.
    NotFound,
    /// Validation failed.
    ValidationFailed,
    /// Transaction failed (e.g., database locked).
    TransactionFailed,
    /// Custom error message.
    Custom(String),
}

impl BeadsErrorKind {
    /// Convert to an `MsError` with the given context.
    fn to_error(&self, context: &str) -> MsError {
        match self {
            Self::Unavailable => MsError::BeadsUnavailable(format!("mock error: {context}")),
            Self::NotFound => MsError::NotFound(format!("mock error: {context}")),
            Self::ValidationFailed => MsError::ValidationFailed(format!("mock error: {context}")),
            Self::TransactionFailed => MsError::TransactionFailed(format!("mock error: {context}")),
            Self::Custom(msg) => MsError::BeadsUnavailable(msg.clone()),
        }
    }
}

/// Error injection configuration for testing.
#[derive(Debug, Clone)]
pub enum ErrorInjection {
    /// Fail all operations with this error.
    All(BeadsErrorKind),

    /// Fail a specific operation (by name) with this error.
    Operation(String, BeadsErrorKind),

    /// Fail operations on a specific issue ID with this error.
    IssueId(String, BeadsErrorKind),
}

/// Mock `BeadsClient` for testing.
///
/// Provides an in-memory implementation of beads operations that can be
/// used in unit tests without spawning subprocesses or requiring bd.
#[derive(Debug)]
pub struct MockBeadsClient {
    /// Whether to report as available.
    available: bool,

    /// In-memory issue store.
    issues: RefCell<HashMap<String, Issue>>,

    /// Counter for generating IDs.
    next_id: RefCell<u32>,

    /// Project prefix for generated IDs.
    project_prefix: String,

    /// Inject errors for specific operations.
    error_on: RefCell<Option<ErrorInjection>>,
}

impl MockBeadsClient {
    /// Create a new mock that reports as available.
    #[must_use]
    pub fn new() -> Self {
        Self {
            available: true,
            issues: RefCell::new(HashMap::new()),
            next_id: RefCell::new(1),
            project_prefix: "mock".to_string(),
            error_on: RefCell::new(None),
        }
    }

    /// Create a mock that reports as unavailable.
    #[must_use]
    pub fn unavailable() -> Self {
        Self {
            available: false,
            ..Self::new()
        }
    }

    /// Set the project prefix for generated issue IDs.
    pub fn with_project_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.project_prefix = prefix.into();
        self
    }

    /// Pre-populate with issues for testing.
    pub fn with_issues(self, issues: Vec<Issue>) -> Self {
        {
            let mut store = self.issues.borrow_mut();
            for issue in issues {
                store.insert(issue.id.clone(), issue);
            }
        }
        self
    }

    /// Insert a single issue into the mock store.
    pub fn insert_issue(&self, issue: Issue) {
        let mut store = self.issues.borrow_mut();
        store.insert(issue.id.clone(), issue);
    }

    /// Get all issues in the mock store.
    pub fn get_all_issues(&self) -> Vec<Issue> {
        let store = self.issues.borrow();
        store.values().cloned().collect()
    }

    /// Inject an error for testing error handling.
    pub fn inject_error(&self, injection: ErrorInjection) {
        let mut error_on = self.error_on.borrow_mut();
        *error_on = Some(injection);
    }

    /// Clear any injected errors.
    pub fn clear_errors(&self) {
        let mut error_on = self.error_on.borrow_mut();
        *error_on = None;
    }

    /// Check if an error should be returned for this operation.
    fn check_error(&self, op: &str, id: Option<&str>) -> Result<()> {
        let error_on = self.error_on.borrow();
        if let Some(injection) = error_on.as_ref() {
            match injection {
                ErrorInjection::All(kind) => {
                    return Err(kind.to_error(op));
                }
                ErrorInjection::Operation(target_op, kind) if target_op == op => {
                    return Err(kind.to_error(op));
                }
                ErrorInjection::IssueId(target_id, kind) if Some(target_id.as_str()) == id => {
                    return Err(kind.to_error(&format!("{op}: {target_id}")));
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Generate a new issue ID.
    fn generate_id(&self) -> String {
        let mut next_id = self.next_id.borrow_mut();
        let id = format!("{}-{}", self.project_prefix, *next_id);
        *next_id += 1;
        id
    }
}

impl Default for MockBeadsClient {
    fn default() -> Self {
        Self::new()
    }
}

impl BeadsOperations for MockBeadsClient {
    fn is_available(&self) -> bool {
        self.available
    }

    fn list(&self, filter: &WorkFilter) -> Result<Vec<Issue>> {
        self.check_error("list", None)?;

        let store = self.issues.borrow();
        let mut issues: Vec<Issue> = store
            .values()
            .filter(|issue| {
                // Filter by status
                if let Some(ref status) = filter.status {
                    if &issue.status != status {
                        return false;
                    }
                }

                // Filter by type
                if let Some(ref issue_type) = filter.issue_type {
                    if &issue.issue_type != issue_type {
                        return false;
                    }
                }

                // Filter by assignee
                if let Some(ref assignee) = filter.assignee {
                    if issue.assignee.as_ref() != Some(assignee) {
                        return false;
                    }
                }

                // Filter by max priority
                if let Some(max_priority) = filter.max_priority {
                    if issue.priority > max_priority {
                        return false;
                    }
                }

                // Filter by labels (all must match)
                for label in &filter.labels {
                    if !issue.labels.contains(label) {
                        return false;
                    }
                }

                true
            })
            .cloned()
            .collect();

        // Apply limit
        if let Some(limit) = filter.limit {
            issues.truncate(limit);
        }

        Ok(issues)
    }

    fn ready(&self) -> Result<Vec<Issue>> {
        self.check_error("ready", None)?;

        let store = self.issues.borrow();
        Ok(store
            .values()
            .filter(|issue| issue.is_ready())
            .cloned()
            .collect())
    }

    fn show(&self, id: &str) -> Result<Issue> {
        self.check_error("show", Some(id))?;

        let store = self.issues.borrow();
        store
            .get(id)
            .cloned()
            .ok_or_else(|| MsError::NotFound(format!("issue not found: {id}")))
    }

    fn create(&self, request: &CreateIssueRequest) -> Result<Issue> {
        self.check_error("create", None)?;

        let id = self.generate_id();
        let now = Utc::now();

        let issue = Issue {
            id: id.clone(),
            title: request.title.clone(),
            description: request.description.clone().unwrap_or_default(),
            status: IssueStatus::Open,
            priority: request.priority.unwrap_or(2),
            issue_type: request.issue_type.unwrap_or(IssueType::Task),
            owner: None,
            assignee: None,
            labels: request.labels.clone(),
            notes: None,
            created_at: Some(now),
            created_by: Some("mock".to_string()),
            updated_at: Some(now),
            closed_at: None,
            dependencies: vec![],
            dependents: vec![],
            extra: HashMap::new(),
        };

        let mut store = self.issues.borrow_mut();
        store.insert(id, issue.clone());

        Ok(issue)
    }

    fn update(&self, id: &str, request: &UpdateIssueRequest) -> Result<Issue> {
        self.check_error("update", Some(id))?;

        let mut store = self.issues.borrow_mut();
        let issue = store
            .get_mut(id)
            .ok_or_else(|| MsError::NotFound(format!("issue not found: {id}")))?;

        // Apply updates
        if let Some(ref status) = request.status {
            issue.status = *status;
            if status.is_terminal() {
                issue.closed_at = Some(Utc::now());
            }
        }

        if let Some(ref title) = request.title {
            issue.title = title.clone();
        }

        if let Some(ref description) = request.description {
            issue.description = description.clone();
        }

        if let Some(priority) = request.priority {
            issue.priority = priority;
        }

        if let Some(ref assignee) = request.assignee {
            issue.assignee = Some(assignee.clone());
        }

        if let Some(ref notes) = request.notes {
            issue.notes = Some(notes.clone());
        }

        // Add labels
        for label in &request.add_labels {
            if !issue.labels.contains(label) {
                issue.labels.push(label.clone());
            }
        }

        // Remove labels
        issue.labels.retain(|l| !request.remove_labels.contains(l));

        issue.updated_at = Some(Utc::now());

        Ok(issue.clone())
    }

    fn update_status(&self, id: &str, status: IssueStatus) -> Result<Issue> {
        self.update(id, &UpdateIssueRequest::new().with_status(status))
    }

    fn close(&self, id: &str, reason: Option<&str>) -> Result<Issue> {
        self.check_error("close", Some(id))?;

        let request = if let Some(reason) = reason {
            UpdateIssueRequest::new()
                .with_status(IssueStatus::Closed)
                .with_notes(reason)
        } else {
            UpdateIssueRequest::new().with_status(IssueStatus::Closed)
        };

        self.update(id, &request)
    }
}

/// Helper to create a test issue with minimal fields.
#[must_use]
pub fn test_issue(id: &str, title: &str) -> Issue {
    Issue {
        id: id.to_string(),
        title: title.to_string(),
        description: String::new(),
        status: IssueStatus::Open,
        priority: 2,
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // MockBeadsClient construction tests
    // =========================================================================

    #[test]
    fn mock_client_new_is_available() {
        let mock = MockBeadsClient::new();
        assert!(mock.is_available());
    }

    #[test]
    fn mock_client_unavailable_returns_false() {
        let mock = MockBeadsClient::unavailable();
        assert!(!mock.is_available());
    }

    #[test]
    fn mock_client_default_is_available() {
        let mock = MockBeadsClient::default();
        assert!(mock.is_available());
    }

    #[test]
    fn mock_client_with_project_prefix() {
        let mock = MockBeadsClient::new().with_project_prefix("test");
        let issue = mock.create(&CreateIssueRequest::new("Test")).unwrap();
        assert!(issue.id.starts_with("test-"));
    }

    // =========================================================================
    // Pre-population tests
    // =========================================================================

    #[test]
    fn mock_client_with_issues() {
        let issue = test_issue("pre-1", "Pre-existing issue");
        let mock = MockBeadsClient::new().with_issues(vec![issue]);

        let found = mock.show("pre-1").unwrap();
        assert_eq!(found.title, "Pre-existing issue");
    }

    #[test]
    fn mock_client_insert_issue() {
        let mock = MockBeadsClient::new();
        mock.insert_issue(test_issue("inserted-1", "Inserted issue"));

        let found = mock.show("inserted-1").unwrap();
        assert_eq!(found.title, "Inserted issue");
    }

    #[test]
    fn mock_client_get_all_issues() {
        let mock = MockBeadsClient::new().with_issues(vec![
            test_issue("all-1", "Issue 1"),
            test_issue("all-2", "Issue 2"),
        ]);

        let all = mock.get_all_issues();
        assert_eq!(all.len(), 2);
    }

    // =========================================================================
    // Error injection tests
    // =========================================================================

    #[test]
    fn mock_client_inject_error_all() {
        let mock = MockBeadsClient::new();
        mock.inject_error(ErrorInjection::All(BeadsErrorKind::Unavailable));

        assert!(mock.ready().is_err());
        assert!(mock.show("any").is_err());
        assert!(mock.create(&CreateIssueRequest::new("Test")).is_err());
    }

    #[test]
    fn mock_client_inject_error_operation() {
        let mock = MockBeadsClient::new();
        mock.inject_error(ErrorInjection::Operation(
            "create".to_string(),
            BeadsErrorKind::Unavailable,
        ));

        // Create should fail
        assert!(mock.create(&CreateIssueRequest::new("Test")).is_err());

        // Other operations should work
        assert!(mock.ready().is_ok());
    }

    #[test]
    fn mock_client_inject_error_issue_id() {
        let mock = MockBeadsClient::new().with_issues(vec![
            test_issue("good-1", "Good issue"),
            test_issue("bad-1", "Bad issue"),
        ]);

        mock.inject_error(ErrorInjection::IssueId(
            "bad-1".to_string(),
            BeadsErrorKind::NotFound,
        ));

        // Good issue should work
        assert!(mock.show("good-1").is_ok());

        // Bad issue should fail
        assert!(mock.show("bad-1").is_err());
    }

    #[test]
    fn mock_client_clear_errors() {
        let mock = MockBeadsClient::new();
        mock.inject_error(ErrorInjection::All(BeadsErrorKind::Unavailable));

        assert!(mock.ready().is_err());

        mock.clear_errors();

        assert!(mock.ready().is_ok());
    }

    #[test]
    fn mock_client_inject_custom_error() {
        let mock = MockBeadsClient::new();
        mock.inject_error(ErrorInjection::All(BeadsErrorKind::Custom(
            "Custom failure message".to_string(),
        )));

        let err = mock.ready().unwrap_err();
        let err_str = err.to_string();
        assert!(err_str.contains("Custom failure message"));
    }

    #[test]
    fn mock_client_inject_validation_error() {
        let mock = MockBeadsClient::new();
        mock.inject_error(ErrorInjection::Operation(
            "create".to_string(),
            BeadsErrorKind::ValidationFailed,
        ));

        let err = mock.create(&CreateIssueRequest::new("Test")).unwrap_err();
        assert!(matches!(err, MsError::ValidationFailed(_)));
    }

    #[test]
    fn mock_client_inject_transaction_error() {
        let mock = MockBeadsClient::new().with_issues(vec![test_issue("tx-1", "Test")]);

        mock.inject_error(ErrorInjection::IssueId(
            "tx-1".to_string(),
            BeadsErrorKind::TransactionFailed,
        ));

        let err = mock.show("tx-1").unwrap_err();
        assert!(matches!(err, MsError::TransactionFailed(_)));
    }

    // =========================================================================
    // BeadsOperations implementation tests
    // =========================================================================

    #[test]
    fn mock_client_list_all() {
        let mock = MockBeadsClient::new().with_issues(vec![
            test_issue("list-1", "Issue 1"),
            test_issue("list-2", "Issue 2"),
        ]);

        let result = mock.list(&WorkFilter::default()).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn mock_client_list_filter_by_status() {
        let mut open_issue = test_issue("open-1", "Open");
        open_issue.status = IssueStatus::Open;

        let mut closed_issue = test_issue("closed-1", "Closed");
        closed_issue.status = IssueStatus::Closed;

        let mock = MockBeadsClient::new().with_issues(vec![open_issue, closed_issue]);

        let filter = WorkFilter {
            status: Some(IssueStatus::Open),
            ..Default::default()
        };

        let result = mock.list(&filter).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "open-1");
    }

    #[test]
    fn mock_client_list_filter_by_type() {
        let mut bug = test_issue("bug-1", "Bug");
        bug.issue_type = IssueType::Bug;

        let mut feature = test_issue("feat-1", "Feature");
        feature.issue_type = IssueType::Feature;

        let mock = MockBeadsClient::new().with_issues(vec![bug, feature]);

        let filter = WorkFilter {
            issue_type: Some(IssueType::Bug),
            ..Default::default()
        };

        let result = mock.list(&filter).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "bug-1");
    }

    #[test]
    fn mock_client_list_filter_by_assignee() {
        let mut assigned = test_issue("assigned-1", "Assigned");
        assigned.assignee = Some("developer@example.com".to_string());

        let unassigned = test_issue("unassigned-1", "Unassigned");

        let mock = MockBeadsClient::new().with_issues(vec![assigned, unassigned]);

        let filter = WorkFilter {
            assignee: Some("developer@example.com".to_string()),
            ..Default::default()
        };

        let result = mock.list(&filter).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "assigned-1");
    }

    #[test]
    fn mock_client_list_filter_by_priority() {
        let mut p0 = test_issue("p0-1", "Critical");
        p0.priority = 0;

        let mut p2 = test_issue("p2-1", "Normal");
        p2.priority = 2;

        let mut p4 = test_issue("p4-1", "Backlog");
        p4.priority = 4;

        let mock = MockBeadsClient::new().with_issues(vec![p0, p2, p4]);

        let filter = WorkFilter {
            max_priority: Some(2),
            ..Default::default()
        };

        let result = mock.list(&filter).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn mock_client_list_filter_by_labels() {
        let mut with_label = test_issue("label-1", "Has label");
        with_label.labels = vec!["urgent".to_string()];

        let without_label = test_issue("label-2", "No label");

        let mock = MockBeadsClient::new().with_issues(vec![with_label, without_label]);

        let filter = WorkFilter {
            labels: vec!["urgent".to_string()],
            ..Default::default()
        };

        let result = mock.list(&filter).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "label-1");
    }

    #[test]
    fn mock_client_list_filter_by_multiple_labels() {
        let mut has_both = test_issue("both-1", "Has both");
        has_both.labels = vec!["urgent".to_string(), "backend".to_string()];

        let mut has_one = test_issue("one-1", "Has one");
        has_one.labels = vec!["urgent".to_string()];

        let mock = MockBeadsClient::new().with_issues(vec![has_both, has_one]);

        let filter = WorkFilter {
            labels: vec!["urgent".to_string(), "backend".to_string()],
            ..Default::default()
        };

        let result = mock.list(&filter).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "both-1");
    }

    #[test]
    fn mock_client_list_with_limit() {
        let mock = MockBeadsClient::new().with_issues(vec![
            test_issue("lim-1", "Issue 1"),
            test_issue("lim-2", "Issue 2"),
            test_issue("lim-3", "Issue 3"),
        ]);

        let filter = WorkFilter {
            limit: Some(2),
            ..Default::default()
        };

        let result = mock.list(&filter).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn mock_client_ready_returns_open_unblocked() {
        let mut open = test_issue("ready-1", "Ready");
        open.status = IssueStatus::Open;

        let mut in_progress = test_issue("inprog-1", "In Progress");
        in_progress.status = IssueStatus::InProgress;

        let mock = MockBeadsClient::new().with_issues(vec![open, in_progress]);

        let result = mock.ready().unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "ready-1");
    }

    #[test]
    fn mock_client_show_existing() {
        let mock = MockBeadsClient::new().with_issues(vec![test_issue("show-1", "Test")]);

        let issue = mock.show("show-1").unwrap();
        assert_eq!(issue.title, "Test");
    }

    #[test]
    fn mock_client_show_not_found() {
        let mock = MockBeadsClient::new();

        let result = mock.show("nonexistent");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), MsError::NotFound(_)));
    }

    #[test]
    fn mock_client_create_generates_id() {
        let mock = MockBeadsClient::new();

        let issue = mock.create(&CreateIssueRequest::new("New Issue")).unwrap();

        assert!(!issue.id.is_empty());
        assert!(issue.id.starts_with("mock-"));
    }

    #[test]
    fn mock_client_create_sequential_ids() {
        let mock = MockBeadsClient::new();

        let issue1 = mock.create(&CreateIssueRequest::new("First")).unwrap();
        let issue2 = mock.create(&CreateIssueRequest::new("Second")).unwrap();

        assert_ne!(issue1.id, issue2.id);
    }

    #[test]
    fn mock_client_create_sets_defaults() {
        let mock = MockBeadsClient::new();

        let issue = mock.create(&CreateIssueRequest::new("Test")).unwrap();

        assert_eq!(issue.status, IssueStatus::Open);
        assert_eq!(issue.issue_type, IssueType::Task);
        assert_eq!(issue.priority, 2);
        assert!(issue.created_at.is_some());
    }

    #[test]
    fn mock_client_create_with_all_fields() {
        let mock = MockBeadsClient::new();

        let request = CreateIssueRequest::new("Full Issue")
            .with_type(IssueType::Bug)
            .with_priority(1)
            .with_description("Detailed description")
            .with_label("urgent");

        let issue = mock.create(&request).unwrap();

        assert_eq!(issue.title, "Full Issue");
        assert_eq!(issue.issue_type, IssueType::Bug);
        assert_eq!(issue.priority, 1);
        assert_eq!(issue.description, "Detailed description");
        assert!(issue.labels.contains(&"urgent".to_string()));
    }

    #[test]
    fn mock_client_create_persists_issue() {
        let mock = MockBeadsClient::new();

        let created = mock
            .create(&CreateIssueRequest::new("Persist Test"))
            .unwrap();

        // Should be able to retrieve it
        let fetched = mock.show(&created.id).unwrap();
        assert_eq!(fetched.title, "Persist Test");
    }

    #[test]
    fn mock_client_update_status() {
        let mock = MockBeadsClient::new().with_issues(vec![test_issue("upd-1", "Test")]);

        let updated = mock
            .update_status("upd-1", IssueStatus::InProgress)
            .unwrap();

        assert_eq!(updated.status, IssueStatus::InProgress);

        // Verify persisted
        let fetched = mock.show("upd-1").unwrap();
        assert_eq!(fetched.status, IssueStatus::InProgress);
    }

    #[test]
    fn mock_client_update_multiple_fields() {
        let mock = MockBeadsClient::new().with_issues(vec![test_issue("upd-2", "Original")]);

        let request = UpdateIssueRequest::new()
            .with_status(IssueStatus::InProgress)
            .with_priority(0)
            .with_assignee("developer@example.com")
            .with_notes("Working on it");

        let updated = mock.update("upd-2", &request).unwrap();

        assert_eq!(updated.status, IssueStatus::InProgress);
        assert_eq!(updated.priority, 0);
        assert_eq!(updated.assignee, Some("developer@example.com".to_string()));
        assert_eq!(updated.notes, Some("Working on it".to_string()));
    }

    #[test]
    fn mock_client_update_add_labels() {
        let mock = MockBeadsClient::new().with_issues(vec![test_issue("label-upd", "Test")]);

        let mut request = UpdateIssueRequest::new();
        request.add_labels = vec!["new-label".to_string()];

        let updated = mock.update("label-upd", &request).unwrap();

        assert!(updated.labels.contains(&"new-label".to_string()));
    }

    #[test]
    fn mock_client_update_remove_labels() {
        let mut issue = test_issue("label-rm", "Test");
        issue.labels = vec!["old-label".to_string()];

        let mock = MockBeadsClient::new().with_issues(vec![issue]);

        let mut request = UpdateIssueRequest::new();
        request.remove_labels = vec!["old-label".to_string()];

        let updated = mock.update("label-rm", &request).unwrap();

        assert!(!updated.labels.contains(&"old-label".to_string()));
    }

    #[test]
    fn mock_client_update_not_found() {
        let mock = MockBeadsClient::new();

        let result = mock.update(
            "nonexistent",
            &UpdateIssueRequest::new().with_status(IssueStatus::Closed),
        );

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), MsError::NotFound(_)));
    }

    #[test]
    fn mock_client_update_sets_closed_at() {
        let mock = MockBeadsClient::new().with_issues(vec![test_issue("close-at", "Test")]);

        let updated = mock.update_status("close-at", IssueStatus::Closed).unwrap();

        assert!(updated.closed_at.is_some());
    }

    #[test]
    fn mock_client_close() {
        let mock = MockBeadsClient::new().with_issues(vec![test_issue("close-1", "Test")]);

        let closed = mock.close("close-1", Some("Done")).unwrap();

        assert_eq!(closed.status, IssueStatus::Closed);
        assert!(closed.closed_at.is_some());
    }

    #[test]
    fn mock_client_close_without_reason() {
        let mock = MockBeadsClient::new().with_issues(vec![test_issue("close-2", "Test")]);

        let closed = mock.close("close-2", None).unwrap();

        assert_eq!(closed.status, IssueStatus::Closed);
    }

    #[test]
    fn mock_client_close_not_found() {
        let mock = MockBeadsClient::new();

        let result = mock.close("nonexistent", None);

        assert!(result.is_err());
    }

    #[test]
    fn mock_client_close_with_reason_sets_notes() {
        let mock = MockBeadsClient::new().with_issues(vec![test_issue("close-reason", "Test")]);

        let closed = mock
            .close("close-reason", Some("Completed successfully"))
            .unwrap();

        assert_eq!(closed.notes, Some("Completed successfully".to_string()));
    }

    // =========================================================================
    // test_issue helper tests
    // =========================================================================

    #[test]
    fn test_issue_creates_minimal_issue() {
        let issue = test_issue("test-1", "Test Title");

        assert_eq!(issue.id, "test-1");
        assert_eq!(issue.title, "Test Title");
        assert_eq!(issue.status, IssueStatus::Open);
        assert_eq!(issue.issue_type, IssueType::Task);
        assert!(issue.labels.is_empty());
        assert!(issue.dependencies.is_empty());
    }

    // =========================================================================
    // BeadsErrorKind tests
    // =========================================================================

    #[test]
    fn beads_error_kind_to_error_unavailable() {
        let err = BeadsErrorKind::Unavailable.to_error("test context");
        assert!(matches!(err, MsError::BeadsUnavailable(_)));
    }

    #[test]
    fn beads_error_kind_to_error_not_found() {
        let err = BeadsErrorKind::NotFound.to_error("test context");
        assert!(matches!(err, MsError::NotFound(_)));
    }

    #[test]
    fn beads_error_kind_to_error_validation() {
        let err = BeadsErrorKind::ValidationFailed.to_error("test context");
        assert!(matches!(err, MsError::ValidationFailed(_)));
    }

    #[test]
    fn beads_error_kind_to_error_transaction() {
        let err = BeadsErrorKind::TransactionFailed.to_error("test context");
        assert!(matches!(err, MsError::TransactionFailed(_)));
    }

    #[test]
    fn beads_error_kind_to_error_custom() {
        let err = BeadsErrorKind::Custom("my message".to_string()).to_error("ignored");
        assert!(matches!(err, MsError::BeadsUnavailable(_)));
        assert!(err.to_string().contains("my message"));
    }
}
