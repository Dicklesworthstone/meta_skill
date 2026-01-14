//! Error recovery and resilience for ms operations.
//!
//! This module provides:
//! - Retry utilities with exponential backoff and jitter
//! - Failure mode enumeration and diagnosis
//! - Recovery handlers for DB, Git, index, and cache
//! - Checkpoint support for resumable long-running operations

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::error::{MsError, Result};
use crate::storage::{Database, GitArchive};

/// Configuration for retry behavior with exponential backoff.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (including the initial attempt).
    pub max_attempts: u32,
    /// Initial delay between retries.
    pub initial_delay: Duration,
    /// Maximum delay cap.
    pub max_delay: Duration,
    /// Multiplier for exponential backoff (e.g., 2.0 doubles delay each retry).
    pub backoff_multiplier: f64,
    /// Jitter factor (0.0-1.0) to add randomness and prevent thundering herd.
    pub jitter_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 2.0,
            jitter_factor: 0.1,
        }
    }
}

impl RetryConfig {
    /// Create a config for aggressive retries (quick recovery).
    pub fn aggressive() -> Self {
        Self {
            max_attempts: 5,
            initial_delay: Duration::from_millis(50),
            max_delay: Duration::from_secs(2),
            backoff_multiplier: 1.5,
            jitter_factor: 0.2,
        }
    }

    /// Create a config for patient retries (long-running ops).
    pub fn patient() -> Self {
        Self {
            max_attempts: 10,
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            jitter_factor: 0.15,
        }
    }

    /// Calculate delay for a given attempt number (0-indexed).
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let base_delay = self.initial_delay.as_secs_f64()
            * self.backoff_multiplier.powi(attempt as i32);
        let capped_delay = base_delay.min(self.max_delay.as_secs_f64());

        // Add jitter
        let jitter = if self.jitter_factor > 0.0 {
            let jitter_range = capped_delay * self.jitter_factor;
            // Simple deterministic jitter based on attempt number
            let jitter_offset = (attempt as f64 * 0.618033988749895) % 1.0;
            jitter_range * (jitter_offset - 0.5) * 2.0
        } else {
            0.0
        };

        Duration::from_secs_f64((capped_delay + jitter).max(0.0))
    }
}

/// Categories of failures that can occur in ms operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FailureMode {
    /// SQLite database errors (corruption, lock contention, WAL issues).
    Database,
    /// Git archive errors (missing objects, index lock, corrupted refs).
    GitArchive,
    /// Search index errors (Tantivy corruption, schema mismatch).
    SearchIndex,
    /// Cache inconsistency (stale entries, missing files).
    Cache,
    /// Transaction failures (incomplete 2PC, orphaned records).
    Transaction,
    /// Lock-related issues (stale locks, deadlocks).
    Lock,
    /// Configuration errors (missing/invalid config).
    Config,
}

impl FailureMode {
    /// Whether this failure mode is typically recoverable.
    pub fn is_recoverable(&self) -> bool {
        match self {
            FailureMode::Database => true,  // Often can recover via WAL checkpoint
            FailureMode::GitArchive => true, // Can rebuild/fsck
            FailureMode::SearchIndex => true, // Can rebuild from source
            FailureMode::Cache => true,      // Always rebuildable
            FailureMode::Transaction => true, // 2PC recovery
            FailureMode::Lock => true,       // Can break stale locks
            FailureMode::Config => false,    // Needs user intervention
        }
    }

    /// Human-readable description of the failure mode.
    pub fn description(&self) -> &'static str {
        match self {
            FailureMode::Database => "SQLite database issue",
            FailureMode::GitArchive => "Git archive issue",
            FailureMode::SearchIndex => "Search index issue",
            FailureMode::Cache => "Cache inconsistency",
            FailureMode::Transaction => "Incomplete transaction",
            FailureMode::Lock => "Lock contention or stale lock",
            FailureMode::Config => "Configuration problem",
        }
    }
}

/// A specific issue detected during diagnosis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryIssue {
    /// The category of failure.
    pub mode: FailureMode,
    /// Severity level (1=critical, 2=major, 3=minor).
    pub severity: u8,
    /// Human-readable description of the issue.
    pub description: String,
    /// Whether automatic recovery is possible.
    pub auto_recoverable: bool,
    /// Suggested fix if not auto-recoverable.
    pub suggested_fix: Option<String>,
}

impl RecoveryIssue {
    pub fn new(mode: FailureMode, severity: u8, description: impl Into<String>) -> Self {
        Self {
            mode,
            severity,
            description: description.into(),
            auto_recoverable: mode.is_recoverable(),
            suggested_fix: None,
        }
    }

    pub fn with_fix(mut self, fix: impl Into<String>) -> Self {
        self.suggested_fix = Some(fix.into());
        self
    }

    pub fn not_auto_recoverable(mut self) -> Self {
        self.auto_recoverable = false;
        self
    }
}

/// Report from diagnosis or recovery operations.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RecoveryReport {
    /// Issues found during diagnosis.
    pub issues: Vec<RecoveryIssue>,
    /// Number of issues successfully fixed.
    pub fixed: usize,
    /// Number of issues that couldn't be fixed automatically.
    pub unfixed: usize,
    /// Transaction rollbacks performed.
    pub rolled_back: usize,
    /// Transactions completed during recovery.
    pub completed: usize,
    /// Orphaned files cleaned up.
    pub orphaned_files: usize,
    /// Cache entries invalidated.
    pub cache_invalidated: usize,
    /// Duration of the recovery operation.
    pub duration: Option<Duration>,
}

impl RecoveryReport {
    /// Whether any recovery work was performed.
    pub fn had_work(&self) -> bool {
        self.fixed > 0
            || self.rolled_back > 0
            || self.completed > 0
            || self.orphaned_files > 0
            || self.cache_invalidated > 0
    }

    /// Whether there are critical issues remaining.
    pub fn has_critical_issues(&self) -> bool {
        self.issues.iter().any(|i| i.severity == 1 && !i.auto_recoverable)
    }

    /// Summary string suitable for logging.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.fixed > 0 {
            parts.push(format!("fixed {}", self.fixed));
        }
        if self.unfixed > 0 {
            parts.push(format!("{} unfixed", self.unfixed));
        }
        if self.rolled_back > 0 {
            parts.push(format!("rolled back {}", self.rolled_back));
        }
        if self.completed > 0 {
            parts.push(format!("completed {}", self.completed));
        }
        if self.orphaned_files > 0 {
            parts.push(format!("cleaned {} orphans", self.orphaned_files));
        }
        if parts.is_empty() {
            "no issues found".to_string()
        } else {
            parts.join(", ")
        }
    }
}

/// Central coordinator for recovery operations.
pub struct RecoveryManager {
    ms_root: PathBuf,
    db: Option<Arc<Database>>,
    git: Option<Arc<GitArchive>>,
    retry_config: RetryConfig,
}

impl RecoveryManager {
    /// Create a new recovery manager.
    pub fn new(ms_root: impl AsRef<Path>) -> Self {
        Self {
            ms_root: ms_root.as_ref().to_path_buf(),
            db: None,
            git: None,
            retry_config: RetryConfig::default(),
        }
    }

    /// Set the database connection.
    pub fn with_db(mut self, db: Arc<Database>) -> Self {
        self.db = Some(db);
        self
    }

    /// Set the git archive.
    pub fn with_git(mut self, git: Arc<GitArchive>) -> Self {
        self.git = Some(git);
        self
    }

    /// Set a custom retry configuration.
    pub fn with_retry_config(mut self, config: RetryConfig) -> Self {
        self.retry_config = config;
        self
    }

    /// Run diagnostics and return a report of issues found.
    pub fn diagnose(&self) -> Result<RecoveryReport> {
        let mut report = RecoveryReport::default();
        let start = Instant::now();

        // Check database
        self.check_database(&mut report)?;

        // Check git archive
        self.check_git_archive(&mut report)?;

        // Check locks
        self.check_locks(&mut report)?;

        // Check transactions
        self.check_transactions(&mut report)?;

        // Check search index
        self.check_search_index(&mut report)?;

        // Check cache
        self.check_cache(&mut report)?;

        report.duration = Some(start.elapsed());
        Ok(report)
    }

    /// Attempt to recover from detected issues.
    pub fn recover(&self, fix: bool) -> Result<RecoveryReport> {
        let mut report = self.diagnose()?;
        let start = Instant::now();

        if !fix {
            return Ok(report);
        }

        // Recover transactions first (most critical)
        self.recover_transactions(&mut report)?;

        // Break stale locks
        self.recover_locks(&mut report)?;

        // Rebuild search index if needed
        self.recover_search_index(&mut report)?;

        // Clean up cache
        self.recover_cache(&mut report)?;

        report.duration = Some(start.elapsed());
        Ok(report)
    }

    // --- Diagnostic checks ---

    fn check_database(&self, report: &mut RecoveryReport) -> Result<()> {
        let db_path = self.ms_root.join("ms.db");
        if !db_path.exists() {
            report.issues.push(
                RecoveryIssue::new(FailureMode::Database, 2, "Database not found")
                    .with_fix("Run 'ms init' to create the database")
                    .not_auto_recoverable(),
            );
            return Ok(());
        }

        if let Some(db) = &self.db {
            match db.integrity_check() {
                Ok(true) => {}
                Ok(false) => {
                    report.issues.push(
                        RecoveryIssue::new(FailureMode::Database, 1, "Database integrity check failed")
                            .with_fix("Run 'ms doctor --fix' or restore from backup"),
                    );
                }
                Err(e) => {
                    report.issues.push(
                        RecoveryIssue::new(
                            FailureMode::Database,
                            1,
                            format!("Database integrity check error: {}", e),
                        )
                        .with_fix("Check database permissions and disk space"),
                    );
                }
            }
        }

        Ok(())
    }

    fn check_git_archive(&self, report: &mut RecoveryReport) -> Result<()> {
        let archive_path = self.ms_root.join("archive");
        if !archive_path.exists() {
            report.issues.push(
                RecoveryIssue::new(FailureMode::GitArchive, 2, "Git archive not found")
                    .with_fix("Run 'ms init' to create the archive")
                    .not_auto_recoverable(),
            );
            return Ok(());
        }

        let git_dir = archive_path.join(".git");
        if !git_dir.exists() {
            report.issues.push(
                RecoveryIssue::new(FailureMode::GitArchive, 1, "Archive is not a Git repository")
                    .with_fix("Run 'git init' in the archive directory"),
            );
            return Ok(());
        }

        // Check for index lock
        let index_lock = git_dir.join("index.lock");
        if index_lock.exists() {
            report.issues.push(
                RecoveryIssue::new(FailureMode::GitArchive, 2, "Git index lock present")
                    .with_fix("Remove .git/index.lock if no git operations are running"),
            );
        }

        Ok(())
    }

    fn check_locks(&self, report: &mut RecoveryReport) -> Result<()> {
        use crate::storage::tx::GlobalLock;

        if let Some(holder) = GlobalLock::status(&self.ms_root)? {
            // Check if process is still alive
            #[cfg(target_os = "linux")]
            {
                let proc_path = format!("/proc/{}", holder.pid);
                if !Path::new(&proc_path).exists() {
                    report.issues.push(RecoveryIssue::new(
                        FailureMode::Lock,
                        2,
                        format!(
                            "Stale lock held by dead process {} (acquired {})",
                            holder.pid, holder.acquired_at
                        ),
                    ));
                }
            }

            #[cfg(not(target_os = "linux"))]
            {
                // On non-Linux, we can't easily check if process is alive
                // Report as informational
                report.issues.push(
                    RecoveryIssue::new(
                        FailureMode::Lock,
                        3,
                        format!("Lock held by PID {} on {}", holder.pid, holder.hostname),
                    )
                    .not_auto_recoverable(),
                );
            }
        }

        Ok(())
    }

    fn check_transactions(&self, report: &mut RecoveryReport) -> Result<()> {
        if let Some(db) = &self.db {
            let incomplete = db.list_incomplete_transactions()?;
            for tx in incomplete {
                report.issues.push(RecoveryIssue::new(
                    FailureMode::Transaction,
                    2,
                    format!(
                        "Incomplete transaction {} ({}, phase: {})",
                        tx.id, tx.entity_type, tx.phase
                    ),
                ));
            }
        }
        Ok(())
    }

    fn check_search_index(&self, report: &mut RecoveryReport) -> Result<()> {
        let index_path = self.ms_root.join("search_index");
        if !index_path.exists() {
            // Not an error - index might not be created yet
            return Ok(());
        }

        // Check for obvious corruption indicators
        let meta_file = index_path.join("meta.json");
        if index_path.is_dir() && !meta_file.exists() {
            report.issues.push(RecoveryIssue::new(
                FailureMode::SearchIndex,
                2,
                "Search index appears corrupted (missing meta.json)",
            ));
        }

        Ok(())
    }

    fn check_cache(&self, report: &mut RecoveryReport) -> Result<()> {
        let cache_path = self.ms_root.join("cache");
        if !cache_path.exists() {
            return Ok(());
        }

        // Check for oversized cache
        let mut total_size = 0u64;
        let mut file_count = 0usize;
        if let Ok(entries) = std::fs::read_dir(&cache_path) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    total_size += meta.len();
                    file_count += 1;
                }
            }
        }

        // Warn if cache is very large (> 100MB)
        if total_size > 100 * 1024 * 1024 {
            report.issues.push(
                RecoveryIssue::new(
                    FailureMode::Cache,
                    3,
                    format!(
                        "Cache is large: {} files, {} MB",
                        file_count,
                        total_size / (1024 * 1024)
                    ),
                )
                .with_fix("Run 'ms cache clear' to free space"),
            );
        }

        Ok(())
    }

    // --- Recovery actions ---

    fn recover_transactions(&self, report: &mut RecoveryReport) -> Result<()> {
        if let (Some(db), Some(git)) = (&self.db, &self.git) {
            let tx_mgr =
                crate::storage::TxManager::new(db.clone(), git.clone(), self.ms_root.clone())?;
            let tx_report = tx_mgr.recover()?;

            report.rolled_back += tx_report.rolled_back;
            report.completed += tx_report.completed;
            report.orphaned_files += tx_report.orphaned_files;

            if tx_report.had_work() {
                report.fixed += tx_report.rolled_back + tx_report.completed;
            }
        }
        Ok(())
    }

    fn recover_locks(&self, report: &mut RecoveryReport) -> Result<()> {
        use crate::storage::tx::GlobalLock;

        if let Some(holder) = GlobalLock::status(&self.ms_root)? {
            // Only break lock if process is confirmed dead
            #[cfg(target_os = "linux")]
            {
                let proc_path = format!("/proc/{}", holder.pid);
                if !Path::new(&proc_path).exists() {
                    if GlobalLock::break_lock(&self.ms_root)? {
                        report.fixed += 1;
                    }
                }
            }
        }
        Ok(())
    }

    fn recover_search_index(&self, report: &mut RecoveryReport) -> Result<()> {
        let index_path = self.ms_root.join("search_index");
        if !index_path.exists() {
            return Ok(());
        }

        // Check if index is corrupted
        let meta_file = index_path.join("meta.json");
        if index_path.is_dir() && !meta_file.exists() {
            // Index needs rebuild - mark as needing attention
            // Actual rebuild would need the SearchIndex component
            report.unfixed += 1;
        }

        Ok(())
    }

    fn recover_cache(&self, report: &mut RecoveryReport) -> Result<()> {
        let cache_path = self.ms_root.join("cache");
        if !cache_path.exists() {
            return Ok(());
        }

        // Clean up obviously stale cache entries (e.g., temp files)
        if let Ok(entries) = std::fs::read_dir(&cache_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    // Clean up temp files
                    if name.starts_with(".tmp") || name.ends_with(".tmp") {
                        if tombstone_file(&self.ms_root, &path, "cache").is_ok() {
                            report.cache_invalidated += 1;
                        }
                    }
                }
            }
        }

        if report.cache_invalidated > 0 {
            report.fixed += 1;
        }

        Ok(())
    }
}

/// Execute a fallible operation with retry logic.
pub fn with_retry<T, E, F>(config: &RetryConfig, mut operation: F) -> std::result::Result<T, E>
where
    F: FnMut() -> std::result::Result<T, E>,
{
    let mut last_error = None;
    let attempts = config.max_attempts.max(1);

    for attempt in 0..attempts {
        match operation() {
            Ok(value) => return Ok(value),
            Err(e) => {
                last_error = Some(e);
                if attempt + 1 < attempts {
                    let delay = config.delay_for_attempt(attempt);
                    std::thread::sleep(delay);
                }
            }
        }
    }

    Err(last_error.expect("at least one attempt should have been made"))
}

/// Execute a fallible operation with retry logic, with a condition for retrying.
pub fn with_retry_if<T, E, F, C>(
    config: &RetryConfig,
    mut operation: F,
    should_retry: C,
) -> std::result::Result<T, E>
where
    F: FnMut() -> std::result::Result<T, E>,
    C: Fn(&E) -> bool,
{
    let mut last_error = None;
    let attempts = config.max_attempts.max(1);

    for attempt in 0..attempts {
        match operation() {
            Ok(value) => return Ok(value),
            Err(e) => {
                if !should_retry(&e) || attempt + 1 >= attempts {
                    return Err(e);
                }
                last_error = Some(e);
                let delay = config.delay_for_attempt(attempt);
                std::thread::sleep(delay);
            }
        }
    }

    Err(last_error.expect("at least one attempt should have been made"))
}

/// Checkpoint for resumable long-running operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Unique identifier for the operation.
    pub operation_id: String,
    /// Operation type (e.g., "build", "index", "sync").
    pub operation_type: String,
    /// Current phase/stage of the operation.
    pub phase: String,
    /// Progress within current phase (0.0-1.0).
    pub progress: f64,
    /// Arbitrary state data for resumption.
    pub state: HashMap<String, String>,
    /// When checkpoint was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When checkpoint was last updated.
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl Checkpoint {
    /// Create a new checkpoint.
    pub fn new(operation_id: impl Into<String>, operation_type: impl Into<String>) -> Self {
        let now = chrono::Utc::now();
        Self {
            operation_id: operation_id.into(),
            operation_type: operation_type.into(),
            phase: String::new(),
            progress: 0.0,
            state: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Update the phase and progress.
    pub fn update_progress(&mut self, phase: impl Into<String>, progress: f64) {
        self.phase = phase.into();
        self.progress = progress.clamp(0.0, 1.0);
        self.updated_at = chrono::Utc::now();
    }

    /// Store a state value.
    pub fn set_state(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.state.insert(key.into(), value.into());
        self.updated_at = chrono::Utc::now();
    }

    /// Retrieve a state value.
    pub fn get_state(&self, key: &str) -> Option<&str> {
        self.state.get(key).map(|s| s.as_str())
    }

    /// Save checkpoint to disk.
    pub fn save(&self, ms_root: &Path) -> Result<()> {
        let checkpoints_dir = ms_root.join("checkpoints");
        std::fs::create_dir_all(&checkpoints_dir).map_err(|e| {
            MsError::Config(format!("Failed to create checkpoints dir: {}", e))
        })?;

        let path = checkpoints_dir.join(format!("{}.json", self.operation_id));
        let json = serde_json::to_string_pretty(self).map_err(|e| {
            MsError::Config(format!("Failed to serialize checkpoint: {}", e))
        })?;

        std::fs::write(&path, json).map_err(|e| {
            MsError::Config(format!("Failed to write checkpoint: {}", e))
        })?;

        Ok(())
    }

    /// Load checkpoint from disk.
    pub fn load(ms_root: &Path, operation_id: &str) -> Result<Option<Self>> {
        let path = ms_root.join("checkpoints").join(format!("{}.json", operation_id));
        if !path.exists() {
            return Ok(None);
        }

        let json = std::fs::read_to_string(&path).map_err(|e| {
            MsError::Config(format!("Failed to read checkpoint: {}", e))
        })?;

        let checkpoint: Self = serde_json::from_str(&json).map_err(|e| {
            MsError::Config(format!("Failed to parse checkpoint: {}", e))
        })?;

        Ok(Some(checkpoint))
    }

    /// Remove checkpoint from disk.
    pub fn remove(ms_root: &Path, operation_id: &str) -> Result<bool> {
        let path = ms_root.join("checkpoints").join(format!("{}.json", operation_id));
        if !path.exists() {
            return Ok(false);
        }

        tombstone_file(ms_root, &path, "checkpoints").map_err(|e| {
            MsError::Config(format!("Failed to tombstone checkpoint: {}", e))
        })?;

        Ok(true)
    }
}

fn tombstone_file(ms_root: &Path, path: &Path, bucket: &str) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let tombstones = ms_root.join(".ms").join("tombstones").join(bucket);
    std::fs::create_dir_all(&tombstones).map_err(|e| {
        MsError::Config(format!("Failed to create tombstones dir: {}", e))
    })?;
    let name = path
        .file_name()
        .ok_or_else(|| MsError::Config("Invalid tombstone file name".to_string()))?;
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S");
    let dest = tombstones.join(format!("{}_{}", name.to_string_lossy(), stamp));
    std::fs::rename(path, &dest).map_err(|e| {
        MsError::Config(format!("Failed to tombstone file {}: {}", path.display(), e))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_config_delay_calculation() {
        let config = RetryConfig {
            max_attempts: 5,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 2.0,
            jitter_factor: 0.0, // No jitter for deterministic test
        };

        // Attempt 0: 100ms
        let d0 = config.delay_for_attempt(0);
        assert_eq!(d0, Duration::from_millis(100));

        // Attempt 1: 200ms
        let d1 = config.delay_for_attempt(1);
        assert_eq!(d1, Duration::from_millis(200));

        // Attempt 2: 400ms
        let d2 = config.delay_for_attempt(2);
        assert_eq!(d2, Duration::from_millis(400));

        // Should cap at max_delay
        let d10 = config.delay_for_attempt(10);
        assert_eq!(d10, Duration::from_secs(10));
    }

    #[test]
    fn retry_config_with_jitter() {
        let config = RetryConfig {
            max_attempts: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            backoff_multiplier: 2.0,
            jitter_factor: 0.2,
        };

        // With jitter, delays should be within +/- 20% of base
        let d0 = config.delay_for_attempt(0);
        let base = Duration::from_millis(100);
        let jitter_range = base.as_millis() as f64 * 0.2;
        assert!(
            d0.as_millis() as f64 >= base.as_millis() as f64 - jitter_range
                && d0.as_millis() as f64 <= base.as_millis() as f64 + jitter_range
        );
    }

    #[test]
    fn with_retry_succeeds_first_try() {
        let config = RetryConfig::default();
        let mut attempts = 0;

        let result: std::result::Result<i32, &str> = with_retry(&config, || {
            attempts += 1;
            Ok(42)
        });

        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts, 1);
    }

    #[test]
    fn with_retry_succeeds_after_failures() {
        let config = RetryConfig {
            max_attempts: 3,
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
            backoff_multiplier: 2.0,
            jitter_factor: 0.0,
        };
        let mut attempts = 0;

        let result: std::result::Result<i32, &str> = with_retry(&config, || {
            attempts += 1;
            if attempts < 3 {
                Err("not yet")
            } else {
                Ok(42)
            }
        });

        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts, 3);
    }

    #[test]
    fn with_retry_exhausts_attempts() {
        let config = RetryConfig {
            max_attempts: 3,
            initial_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(10),
            backoff_multiplier: 2.0,
            jitter_factor: 0.0,
        };
        let mut attempts = 0;

        let result: std::result::Result<i32, &str> = with_retry(&config, || {
            attempts += 1;
            Err("always fails")
        });

        assert!(result.is_err());
        assert_eq!(attempts, 3);
    }

    #[test]
    fn failure_mode_properties() {
        assert!(FailureMode::Database.is_recoverable());
        assert!(FailureMode::Cache.is_recoverable());
        assert!(!FailureMode::Config.is_recoverable());

        assert!(!FailureMode::Database.description().is_empty());
    }

    #[test]
    fn recovery_report_summary() {
        let mut report = RecoveryReport::default();
        assert_eq!(report.summary(), "no issues found");
        assert!(!report.had_work());

        report.fixed = 2;
        report.rolled_back = 1;
        assert!(report.summary().contains("fixed 2"));
        assert!(report.summary().contains("rolled back 1"));
        assert!(report.had_work());
    }

    #[test]
    fn checkpoint_state_management() {
        let mut cp = Checkpoint::new("test-op-123", "build");
        assert_eq!(cp.operation_id, "test-op-123");
        assert_eq!(cp.progress, 0.0);

        cp.update_progress("compiling", 0.5);
        assert_eq!(cp.phase, "compiling");
        assert_eq!(cp.progress, 0.5);

        cp.set_state("current_file", "main.rs");
        assert_eq!(cp.get_state("current_file"), Some("main.rs"));
        assert_eq!(cp.get_state("nonexistent"), None);
    }
}
