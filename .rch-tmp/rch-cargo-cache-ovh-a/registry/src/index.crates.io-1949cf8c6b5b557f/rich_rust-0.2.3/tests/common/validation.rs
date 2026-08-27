//! Fixture validation utilities for test infrastructure.
//!
//! This module provides tools for validating fixture consistency,
//! detecting stale fixtures, and ensuring test reproducibility.
//!
//! # Features
//!
//! - **Fixture Consistency**: Verify fixtures produce consistent output
//! - **Snapshot Drift Detection**: Identify snapshots that may need updating
//! - **Cross-Platform Validation**: Check output consistency across platforms
//! - **Determinism Checks**: Ensure fixtures don't depend on random/time values
//!
//! # Example
//!
//! ```rust,ignore
//! use common::validation::*;
//!
//! #[test]
//! fn test_fixture_determinism() {
//!     let validator = FixtureValidator::new();
//!     let report = validator.check_determinism(10, || render_table());
//!     assert!(report.is_deterministic, "{}", report.summary());
//! }
//! ```

#![allow(dead_code)]

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

// =============================================================================
// Fixture Validation
// =============================================================================

/// Report from fixture validation.
#[derive(Debug, Clone)]
pub struct ValidationReport {
    /// Name of the fixture being validated.
    pub fixture_name: String,
    /// Whether the fixture is deterministic.
    pub is_deterministic: bool,
    /// Number of runs performed.
    pub run_count: usize,
    /// Number of unique outputs observed.
    pub unique_outputs: usize,
    /// Sample outputs (for debugging).
    pub sample_outputs: Vec<String>,
    /// Issues found during validation.
    pub issues: Vec<ValidationIssue>,
    /// Hash of the canonical output.
    pub canonical_hash: Option<u64>,
}

impl ValidationReport {
    /// Create a new validation report.
    #[must_use]
    pub fn new(fixture_name: impl Into<String>) -> Self {
        Self {
            fixture_name: fixture_name.into(),
            is_deterministic: true,
            run_count: 0,
            unique_outputs: 0,
            sample_outputs: Vec::new(),
            issues: Vec::new(),
            canonical_hash: None,
        }
    }

    /// Generate a summary of the validation.
    #[must_use]
    pub fn summary(&self) -> String {
        let status = if self.is_deterministic {
            "PASS"
        } else {
            "FAIL"
        };
        let mut summary = format!(
            "Fixture '{}': {} ({} runs, {} unique outputs)",
            self.fixture_name, status, self.run_count, self.unique_outputs
        );

        if !self.issues.is_empty() {
            summary.push_str("\nIssues:");
            for issue in &self.issues {
                summary.push_str(&format!("\n  - {:?}: {}", issue.severity, issue.message));
            }
        }

        summary
    }

    /// Check if validation passed with no issues.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.is_deterministic && self.issues.is_empty()
    }
}

/// Severity level for validation issues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueSeverity {
    /// Informational note.
    Info,
    /// Warning (may indicate a problem).
    Warning,
    /// Error (definitely a problem).
    Error,
}

/// An issue found during validation.
#[derive(Debug, Clone)]
pub struct ValidationIssue {
    /// Severity of the issue.
    pub severity: IssueSeverity,
    /// Description of the issue.
    pub message: String,
    /// Location in output (if applicable).
    pub location: Option<String>,
}

/// Validator for fixture consistency and determinism.
#[derive(Debug, Default)]
pub struct FixtureValidator {
    /// Whether to store sample outputs.
    store_samples: bool,
    /// Maximum samples to store.
    max_samples: usize,
}

impl FixtureValidator {
    /// Create a new fixture validator.
    #[must_use]
    pub fn new() -> Self {
        Self {
            store_samples: true,
            max_samples: 5,
        }
    }

    /// Set whether to store sample outputs.
    #[must_use]
    pub fn store_samples(mut self, store: bool) -> Self {
        self.store_samples = store;
        self
    }

    /// Set maximum number of samples to store.
    #[must_use]
    pub fn max_samples(mut self, max: usize) -> Self {
        self.max_samples = max;
        self
    }

    /// Check if a fixture produces deterministic output.
    ///
    /// Runs the fixture multiple times and verifies all outputs are identical.
    pub fn check_determinism<F>(&self, runs: usize, f: F) -> ValidationReport
    where
        F: Fn() -> String,
    {
        let mut report = ValidationReport::new("anonymous");
        let mut outputs: HashMap<u64, usize> = HashMap::new();
        let mut first_output: Option<String> = None;

        for _ in 0..runs {
            let output = f();
            let hash = self.hash_output(&output);

            *outputs.entry(hash).or_insert(0) += 1;

            if first_output.is_none() {
                first_output = Some(output.clone());
                report.canonical_hash = Some(hash);
            }

            if self.store_samples
                && report.sample_outputs.len() < self.max_samples
                && !report.sample_outputs.contains(&output)
            {
                report.sample_outputs.push(output);
            }
        }

        report.run_count = runs;
        report.unique_outputs = outputs.len();
        report.is_deterministic = outputs.len() == 1;

        if !report.is_deterministic {
            report.issues.push(ValidationIssue {
                severity: IssueSeverity::Error,
                message: format!(
                    "Fixture produced {} unique outputs across {} runs",
                    outputs.len(),
                    runs
                ),
                location: None,
            });
        }

        report
    }

    /// Check a fixture with a name.
    pub fn check_named_determinism<F>(&self, name: &str, runs: usize, f: F) -> ValidationReport
    where
        F: Fn() -> String,
    {
        let mut report = self.check_determinism(runs, f);
        report.fixture_name = name.to_string();
        report
    }

    /// Hash output for comparison.
    fn hash_output(&self, output: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        output.hash(&mut hasher);
        hasher.finish()
    }
}

// =============================================================================
// Snapshot Validation
// =============================================================================

/// Information about a snapshot file.
#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    /// Path to the snapshot file.
    pub path: PathBuf,
    /// Snapshot name (derived from filename).
    pub name: String,
    /// Whether this is a pending (.new) snapshot.
    pub is_pending: bool,
    /// Size in bytes.
    pub size: u64,
    /// Content (if loaded).
    pub content: Option<String>,
}

impl SnapshotInfo {
    /// Load snapshot info from a path.
    pub fn from_path(path: impl AsRef<Path>) -> Option<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return None;
        }

        let filename = path.file_name()?.to_str()?;
        let name = filename
            .strip_suffix(".snap.new")
            .or_else(|| filename.strip_suffix(".snap"))?
            .to_string();

        let is_pending = filename.ends_with(".snap.new");
        let metadata = std::fs::metadata(path).ok()?;

        Some(Self {
            path: path.to_path_buf(),
            name,
            is_pending,
            size: metadata.len(),
            content: None,
        })
    }

    /// Load the snapshot content.
    pub fn load_content(&mut self) -> Result<(), std::io::Error> {
        self.content = Some(std::fs::read_to_string(&self.path)?);
        Ok(())
    }
}

/// Validator for snapshot files.
#[derive(Debug, Default)]
pub struct SnapshotValidator {
    /// Root directory for snapshot search.
    root: Option<PathBuf>,
}

impl SnapshotValidator {
    /// Create a new snapshot validator.
    #[must_use]
    pub fn new() -> Self {
        Self { root: None }
    }

    /// Set the root directory for searching.
    #[must_use]
    pub fn root(mut self, path: impl AsRef<Path>) -> Self {
        self.root = Some(path.as_ref().to_path_buf());
        self
    }

    /// Find all snapshot files.
    pub fn find_snapshots(&self) -> Vec<SnapshotInfo> {
        let root = self.root.clone().unwrap_or_else(|| PathBuf::from("."));
        let mut snapshots = Vec::new();

        self.find_snapshots_recursive(&root, &mut snapshots);
        snapshots
    }

    fn find_snapshots_recursive(&self, dir: &Path, snapshots: &mut Vec<SnapshotInfo>) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_dir() {
                // Skip target directory and hidden directories
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name != "target" && !name.starts_with('.') {
                    self.find_snapshots_recursive(&path, snapshots);
                }
            } else if let Some(name) = path.file_name().and_then(|n| n.to_str())
                && (name.ends_with(".snap") || name.ends_with(".snap.new"))
                && let Some(info) = SnapshotInfo::from_path(&path)
            {
                snapshots.push(info);
            }
        }
    }

    /// Find pending (uncommitted) snapshots.
    pub fn find_pending(&self) -> Vec<SnapshotInfo> {
        self.find_snapshots()
            .into_iter()
            .filter(|s| s.is_pending)
            .collect()
    }

    /// Generate a report of snapshot status.
    #[must_use]
    pub fn report(&self) -> SnapshotReport {
        let snapshots = self.find_snapshots();
        let pending: Vec<_> = snapshots.iter().filter(|s| s.is_pending).collect();
        let accepted: Vec<_> = snapshots.iter().filter(|s| !s.is_pending).collect();

        SnapshotReport {
            total: snapshots.len(),
            pending_count: pending.len(),
            accepted_count: accepted.len(),
            pending_names: pending.iter().map(|s| s.name.clone()).collect(),
            by_directory: self.group_by_directory(&snapshots),
        }
    }

    fn group_by_directory(&self, snapshots: &[SnapshotInfo]) -> HashMap<String, usize> {
        let mut by_dir = HashMap::new();

        for snapshot in snapshots {
            if let Some(parent) = snapshot.path.parent() {
                let dir = parent.display().to_string();
                *by_dir.entry(dir).or_insert(0) += 1;
            }
        }

        by_dir
    }
}

/// Report on snapshot status.
#[derive(Debug, Clone)]
pub struct SnapshotReport {
    /// Total number of snapshots.
    pub total: usize,
    /// Number of pending (new) snapshots.
    pub pending_count: usize,
    /// Number of accepted snapshots.
    pub accepted_count: usize,
    /// Names of pending snapshots.
    pub pending_names: Vec<String>,
    /// Snapshots grouped by directory.
    pub by_directory: HashMap<String, usize>,
}

impl SnapshotReport {
    /// Generate a human-readable summary.
    #[must_use]
    pub fn summary(&self) -> String {
        let mut summary = format!(
            "Snapshot Summary: {} total ({} accepted, {} pending)",
            self.total, self.accepted_count, self.pending_count
        );

        if !self.pending_names.is_empty() {
            summary.push_str("\n\nPending snapshots:");
            for name in &self.pending_names {
                summary.push_str(&format!("\n  - {name}"));
            }
        }

        if !self.by_directory.is_empty() {
            summary.push_str("\n\nBy directory:");
            let mut dirs: Vec<_> = self.by_directory.iter().collect();
            dirs.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
            for (dir, count) in dirs {
                summary.push_str(&format!("\n  {dir}: {count}"));
            }
        }

        summary
    }

    /// Check if there are pending snapshots.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        self.pending_count > 0
    }
}

// =============================================================================
// Content Validation
// =============================================================================

/// Validate that output contains expected structural elements.
pub fn validate_table_structure(output: &str) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    // Check for box-drawing characters or ASCII table elements
    let has_horizontal = output.contains('─') || output.contains('-');
    let has_vertical = output.contains('│') || output.contains('|');
    let has_corners = output.contains('┌')
        || output.contains('┐')
        || output.contains('└')
        || output.contains('┘')
        || output.contains('+');

    if !has_horizontal {
        issues.push(ValidationIssue {
            severity: IssueSeverity::Warning,
            message: "No horizontal table elements found".to_string(),
            location: None,
        });
    }

    if !has_vertical {
        issues.push(ValidationIssue {
            severity: IssueSeverity::Warning,
            message: "No vertical table elements found".to_string(),
            location: None,
        });
    }

    if !has_corners {
        issues.push(ValidationIssue {
            severity: IssueSeverity::Warning,
            message: "No corner elements found".to_string(),
            location: None,
        });
    }

    issues
}

/// Validate that output contains expected panel structure.
pub fn validate_panel_structure(output: &str) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    let lines: Vec<&str> = output.lines().collect();

    if lines.is_empty() {
        issues.push(ValidationIssue {
            severity: IssueSeverity::Error,
            message: "Panel output is empty".to_string(),
            location: None,
        });
        return issues;
    }

    // Check first and last lines have corner characters
    let first = lines.first().unwrap_or(&"");
    let last = lines.last().unwrap_or(&"");

    let has_top_border = first.contains('┌') || first.contains('╭') || first.contains('+');
    let has_bottom_border = last.contains('└') || last.contains('╰') || last.contains('+');

    if !has_top_border {
        issues.push(ValidationIssue {
            severity: IssueSeverity::Warning,
            message: "Panel missing top border".to_string(),
            location: Some("line 1".to_string()),
        });
    }

    if !has_bottom_border {
        issues.push(ValidationIssue {
            severity: IssueSeverity::Warning,
            message: "Panel missing bottom border".to_string(),
            location: Some(format!("line {}", lines.len())),
        });
    }

    issues
}

/// Validate tree structure in output.
pub fn validate_tree_structure(output: &str) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    let has_tree_chars = output.contains('├')
        || output.contains('└')
        || output.contains('│')
        || output.contains('|');

    if !has_tree_chars {
        issues.push(ValidationIssue {
            severity: IssueSeverity::Warning,
            message: "No tree guide characters found".to_string(),
            location: None,
        });
    }

    issues
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixture_validator_deterministic() {
        let validator = FixtureValidator::new();
        let counter = std::cell::Cell::new(0);

        let report = validator.check_determinism(10, || {
            counter.set(counter.get() + 1);
            "constant output".to_string()
        });

        assert!(report.is_deterministic);
        assert_eq!(report.unique_outputs, 1);
        assert_eq!(report.run_count, 10);
    }

    #[test]
    fn test_fixture_validator_non_deterministic() {
        let validator = FixtureValidator::new();
        let counter = std::cell::Cell::new(0);

        let report = validator.check_determinism(10, || {
            let n = counter.get();
            counter.set(n + 1);
            format!("output {n}")
        });

        assert!(!report.is_deterministic);
        assert!(report.unique_outputs > 1);
    }

    #[test]
    fn test_validation_report_summary() {
        let mut report = ValidationReport::new("test_fixture");
        report.run_count = 10;
        report.unique_outputs = 1;
        report.is_deterministic = true;

        let summary = report.summary();
        assert!(summary.contains("test_fixture"));
        assert!(summary.contains("PASS"));
        assert!(summary.contains("10 runs"));
    }

    #[test]
    fn test_snapshot_info_from_path() {
        // Create a temp file for testing
        let temp_dir = std::env::temp_dir();
        let snap_path = temp_dir.join("test_snapshot.snap");
        std::fs::write(&snap_path, "test content").unwrap();

        let info = SnapshotInfo::from_path(&snap_path).unwrap();
        assert_eq!(info.name, "test_snapshot");
        assert!(!info.is_pending);

        // Cleanup
        std::fs::remove_file(&snap_path).ok();
    }

    #[test]
    fn test_validate_table_structure() {
        let valid_table = "┌──────┬──────┐\n│ A    │ B    │\n└──────┴──────┘";
        let issues = validate_table_structure(valid_table);
        assert!(issues.is_empty());

        let invalid_table = "just text";
        let issues = validate_table_structure(invalid_table);
        assert!(!issues.is_empty());
    }

    #[test]
    fn test_validate_panel_structure() {
        let valid_panel = "┌────┐\n│ Hi │\n└────┘";
        let issues = validate_panel_structure(valid_panel);
        assert!(issues.is_empty());

        let invalid_panel = "no borders here";
        let issues = validate_panel_structure(invalid_panel);
        assert!(!issues.is_empty());
    }
}
