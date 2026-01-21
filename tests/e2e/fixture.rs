//! E2E test fixture with comprehensive logging and checkpointing.
//!
//! This module provides a rich test fixture for end-to-end testing with:
//! - Structured JSON logging for CI parsing
//! - Checkpoint diffing between states
//! - Database state snapshots at each checkpoint
//! - File system tree snapshots with content hashes
//! - Timing breakdown per operation
//! - Environment logging (env vars, tool versions)
//! - Failure diagnostics with context
//! - HTML report generation option

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as FmtWrite;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant, SystemTime};

use ms::error::{MsError, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

// ============================================================================
// Structured JSON Logging Types
// ============================================================================

/// Log level for structured events.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Warn => write!(f, "WARN"),
            LogLevel::Error => write!(f, "ERROR"),
        }
    }
}

/// Structured log event for CI parsing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEvent {
    /// Timestamp in ISO 8601 format
    pub timestamp: String,
    /// Log level
    pub level: LogLevel,
    /// Event category (step, checkpoint, command, assert, error)
    pub category: String,
    /// Event message
    pub message: String,
    /// Scenario name
    pub scenario: String,
    /// Step number when event occurred
    pub step: usize,
    /// Elapsed time from test start
    pub elapsed_ms: u64,
    /// Additional structured data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl LogEvent {
    /// Create a new log event.
    pub fn new(
        level: LogLevel,
        category: &str,
        message: &str,
        scenario: &str,
        step: usize,
        elapsed: Duration,
    ) -> Self {
        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            level,
            category: category.to_string(),
            message: message.to_string(),
            scenario: scenario.to_string(),
            step,
            elapsed_ms: elapsed.as_millis() as u64,
            data: None,
        }
    }

    /// Add structured data to the event.
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }
}

// ============================================================================
// Database Snapshot Types
// ============================================================================

/// Detailed database state snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbSnapshot {
    /// Snapshot name
    pub name: String,
    /// When the snapshot was taken
    pub timestamp: String,
    /// Table statistics
    pub tables: BTreeMap<String, TableSnapshot>,
    /// Total database size in bytes
    pub size_bytes: u64,
    /// Schema version if available
    pub schema_version: Option<i64>,
}

/// Snapshot of a single database table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSnapshot {
    /// Table name
    pub name: String,
    /// Row count
    pub row_count: i64,
    /// Sample rows (first 3)
    pub sample_rows: Vec<serde_json::Value>,
    /// Column names
    pub columns: Vec<String>,
}

impl DbSnapshot {
    /// Compare with another snapshot to produce a diff.
    pub fn diff(&self, other: &DbSnapshot) -> DbSnapshotDiff {
        let mut added_tables = Vec::new();
        let mut removed_tables = Vec::new();
        let mut row_count_changes = BTreeMap::new();

        // Find added/changed tables
        for (name, table) in &other.tables {
            if let Some(old_table) = self.tables.get(name) {
                if table.row_count != old_table.row_count {
                    row_count_changes.insert(
                        name.clone(),
                        RowCountChange {
                            before: old_table.row_count,
                            after: table.row_count,
                            delta: table.row_count - old_table.row_count,
                        },
                    );
                }
            } else {
                added_tables.push(name.clone());
            }
        }

        // Find removed tables
        for name in self.tables.keys() {
            if !other.tables.contains_key(name) {
                removed_tables.push(name.clone());
            }
        }

        DbSnapshotDiff {
            from: self.name.clone(),
            to: other.name.clone(),
            added_tables,
            removed_tables,
            row_count_changes,
            size_delta: other.size_bytes as i64 - self.size_bytes as i64,
        }
    }
}

/// Difference between two database snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbSnapshotDiff {
    pub from: String,
    pub to: String,
    pub added_tables: Vec<String>,
    pub removed_tables: Vec<String>,
    pub row_count_changes: BTreeMap<String, RowCountChange>,
    pub size_delta: i64,
}

/// Row count change for a table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RowCountChange {
    pub before: i64,
    pub after: i64,
    pub delta: i64,
}

// ============================================================================
// File System Snapshot Types
// ============================================================================

/// File system tree snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsSnapshot {
    /// Snapshot name
    pub name: String,
    /// When the snapshot was taken
    pub timestamp: String,
    /// Root path of the snapshot
    pub root: PathBuf,
    /// All files with metadata
    pub files: BTreeMap<PathBuf, FileInfo>,
    /// Directory structure (dirs only)
    pub directories: Vec<PathBuf>,
    /// Total size of all files
    pub total_size: u64,
    /// Total file count
    pub file_count: usize,
}

/// Information about a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    /// Relative path from root
    pub path: PathBuf,
    /// File size in bytes
    pub size: u64,
    /// SHA-256 hash of contents (hex)
    pub hash: String,
    /// Last modified time (Unix timestamp)
    pub modified: u64,
    /// File permissions (Unix mode)
    #[cfg(unix)]
    pub mode: u32,
    /// Is this a symlink?
    pub is_symlink: bool,
}

impl FsSnapshot {
    /// Compare with another snapshot to produce a diff.
    pub fn diff(&self, other: &FsSnapshot) -> FsSnapshotDiff {
        let mut added_files = Vec::new();
        let mut removed_files = Vec::new();
        let mut modified_files = Vec::new();

        // Find added/modified files
        for (path, info) in &other.files {
            if let Some(old_info) = self.files.get(path) {
                if info.hash != old_info.hash {
                    modified_files.push(FileChange {
                        path: path.clone(),
                        old_size: old_info.size,
                        new_size: info.size,
                        old_hash: old_info.hash.clone(),
                        new_hash: info.hash.clone(),
                    });
                }
            } else {
                added_files.push(path.clone());
            }
        }

        // Find removed files
        for path in self.files.keys() {
            if !other.files.contains_key(path) {
                removed_files.push(path.clone());
            }
        }

        FsSnapshotDiff {
            from: self.name.clone(),
            to: other.name.clone(),
            added_files,
            removed_files,
            modified_files,
            size_delta: other.total_size as i64 - self.total_size as i64,
            file_count_delta: other.file_count as i64 - self.file_count as i64,
        }
    }
}

/// Difference between two file system snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsSnapshotDiff {
    pub from: String,
    pub to: String,
    pub added_files: Vec<PathBuf>,
    pub removed_files: Vec<PathBuf>,
    pub modified_files: Vec<FileChange>,
    pub size_delta: i64,
    pub file_count_delta: i64,
}

/// Information about a modified file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: PathBuf,
    pub old_size: u64,
    pub new_size: u64,
    pub old_hash: String,
    pub new_hash: String,
}

// ============================================================================
// Timing Report Types
// ============================================================================

/// Timing breakdown report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimingReport {
    /// Total elapsed time
    pub total_elapsed: Duration,
    /// Per-operation timings
    pub operations: Vec<OperationTiming>,
    /// Timing by category (command, checkpoint, etc.)
    pub by_category: BTreeMap<String, CategoryTiming>,
    /// Slowest operations (top 5)
    pub slowest: Vec<OperationTiming>,
}

/// Timing for a single operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationTiming {
    /// Operation name
    pub name: String,
    /// Category (command, checkpoint, etc.)
    pub category: String,
    /// Duration
    pub duration: Duration,
    /// Start time from test start
    pub start_offset: Duration,
    /// Step number
    pub step: usize,
}

/// Aggregate timing for a category.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryTiming {
    /// Category name
    pub category: String,
    /// Total time spent
    pub total: Duration,
    /// Count of operations
    pub count: usize,
    /// Average time per operation
    pub average: Duration,
    /// Minimum time
    pub min: Duration,
    /// Maximum time
    pub max: Duration,
}

// ============================================================================
// Environment Information
// ============================================================================

/// Environment information snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentInfo {
    /// Timestamp when captured
    pub timestamp: String,
    /// Operating system
    pub os: String,
    /// OS version
    pub os_version: String,
    /// Architecture
    pub arch: String,
    /// Rust version
    pub rust_version: Option<String>,
    /// ms CLI version
    pub ms_version: Option<String>,
    /// Current working directory
    pub cwd: PathBuf,
    /// Relevant environment variables
    pub env_vars: BTreeMap<String, String>,
    /// Tool versions
    pub tool_versions: BTreeMap<String, String>,
    /// System resources
    pub resources: SystemResources,
}

/// System resource information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemResources {
    /// Number of CPUs
    pub cpu_count: usize,
    /// Total memory in bytes (if available)
    pub memory_total: Option<u64>,
    /// Available memory in bytes (if available)
    pub memory_available: Option<u64>,
}

// ============================================================================
// Failure Diagnostics
// ============================================================================

/// Comprehensive failure diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureDiagnostics {
    /// Error message
    pub error: String,
    /// Error type/category
    pub error_type: String,
    /// Step where failure occurred
    pub step: usize,
    /// Step name
    pub step_name: String,
    /// Elapsed time at failure
    pub elapsed: Duration,
    /// Last successful step
    pub last_success: Option<String>,
    /// Recent log events before failure
    pub recent_events: Vec<LogEvent>,
    /// Current database state
    pub db_state: Option<DbSnapshot>,
    /// Current file system state
    pub fs_state: Option<FsSnapshot>,
    /// Environment info
    pub environment: EnvironmentInfo,
    /// Suggested remediation steps
    pub suggestions: Vec<String>,
}

// ============================================================================
// Checkpoint and Step Types (Enhanced)
// ============================================================================

/// Checkpoint snapshot for test debugging.
#[derive(Debug, Clone)]
pub struct Checkpoint {
    pub name: String,
    pub timestamp: Duration,
    pub step_count: usize,
    /// Legacy simple DB state string
    pub db_state: Option<String>,
    /// Rich database snapshot
    pub db_snapshot: Option<DbSnapshot>,
    /// Rich file system snapshot
    pub fs_snapshot: Option<FsSnapshot>,
    pub files_created: Vec<PathBuf>,
}

/// Step result for report generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub name: String,
    pub success: bool,
    pub duration: Duration,
    pub output_summary: String,
    /// Category of the step (command, checkpoint, assert, etc.)
    pub category: String,
    /// Exit code if this was a command
    pub exit_code: Option<i32>,
}

// ============================================================================
// E2E Fixture Configuration
// ============================================================================

/// Configuration options for E2E fixture.
#[derive(Debug, Clone)]
pub struct E2EConfig {
    /// Enable JSON logging to file
    pub json_logging: bool,
    /// Path for JSON log file (defaults to temp_dir/e2e_log.jsonl)
    pub json_log_path: Option<PathBuf>,
    /// Enable rich database snapshots (slower but more detailed)
    pub rich_db_snapshots: bool,
    /// Enable file system snapshots with hashes (slower but more detailed)
    pub rich_fs_snapshots: bool,
    /// Generate HTML report at end
    pub html_report: bool,
    /// Path for HTML report (defaults to temp_dir/report.html)
    pub html_report_path: Option<PathBuf>,
    /// Capture environment info at start
    pub capture_environment: bool,
    /// Maximum sample rows per table in DB snapshots
    pub db_sample_rows: usize,
}

impl Default for E2EConfig {
    fn default() -> Self {
        Self {
            json_logging: true,
            json_log_path: None,
            rich_db_snapshots: true,
            rich_fs_snapshots: true,
            html_report: false,
            html_report_path: None,
            capture_environment: true,
            db_sample_rows: 3,
        }
    }
}

/// E2E test fixture providing isolated environment with comprehensive logging.
pub struct E2EFixture {
    /// Test scenario name
    pub scenario_name: String,
    /// Root temp directory
    pub temp_dir: TempDir,
    /// Project root (temp_dir path)
    pub root: PathBuf,
    /// ms root directory (./.ms)
    pub ms_root: PathBuf,
    /// Config file path
    pub config_path: PathBuf,
    /// Skills directories for different layers
    pub skills_dirs: HashMap<String, PathBuf>,
    /// Database connection for state verification
    pub db: Option<Connection>,
    /// Test start time
    start_time: Instant,
    /// Current step number
    step_count: usize,
    /// Checkpoints captured
    checkpoints: Vec<Checkpoint>,
    /// Step results for report
    step_results: Vec<StepResult>,
    /// Configuration options
    config: E2EConfig,
    /// Structured log events
    log_events: Vec<LogEvent>,
    /// JSON log file handle
    json_log_file: Option<std::fs::File>,
    /// Operation timings for timing report
    operation_timings: Vec<OperationTiming>,
    /// Environment info captured at start
    environment: Option<EnvironmentInfo>,
    /// Current step name (for failure diagnostics)
    current_step_name: String,
    /// Last successful step name
    last_success_step: Option<String>,
}

impl E2EFixture {
    /// Create a fresh E2E test fixture with default configuration.
    pub fn new(scenario_name: &str) -> Self {
        Self::with_config(scenario_name, E2EConfig::default())
    }

    /// Create a fresh E2E test fixture with custom configuration.
    pub fn with_config(scenario_name: &str, config: E2EConfig) -> Self {
        let start_time = Instant::now();
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let root = temp_dir.path().to_path_buf();
        let ms_root = root.join(".ms");
        let config_path = ms_root.join("config.toml");

        // Create skills directories for different layers
        let mut skills_dirs = HashMap::new();
        let project_skills = root.join("skills");
        let global_skills = root.join("global_skills");
        let local_skills = root.join("local_skills");

        std::fs::create_dir_all(&project_skills).expect("Failed to create project skills dir");
        std::fs::create_dir_all(&global_skills).expect("Failed to create global skills dir");
        std::fs::create_dir_all(&local_skills).expect("Failed to create local skills dir");

        skills_dirs.insert("project".to_string(), project_skills);
        skills_dirs.insert("global".to_string(), global_skills);
        skills_dirs.insert("local".to_string(), local_skills);

        // Setup JSON log file if enabled
        let json_log_file = if config.json_logging {
            let log_path = config
                .json_log_path
                .clone()
                .unwrap_or_else(|| root.join("e2e_log.jsonl"));
            std::fs::File::create(&log_path).ok()
        } else {
            None
        };

        println!();
        println!("{}", "█".repeat(70));
        println!("█ E2E SCENARIO: {}", scenario_name);
        println!("{}", "█".repeat(70));
        println!();
        println!("[E2E] Root: {:?}", root);
        println!("[E2E] MS Root: {:?}", ms_root);
        println!("[E2E] Config: {:?}", config_path);
        println!(
            "[E2E] Skills Dirs: {:?}",
            skills_dirs.keys().collect::<Vec<_>>()
        );
        println!();

        let mut fixture = Self {
            scenario_name: scenario_name.to_string(),
            temp_dir,
            root,
            ms_root,
            config_path,
            skills_dirs,
            db: None,
            start_time,
            step_count: 0,
            checkpoints: Vec::new(),
            step_results: Vec::new(),
            config,
            log_events: Vec::new(),
            json_log_file,
            operation_timings: Vec::new(),
            environment: None,
            current_step_name: String::new(),
            last_success_step: None,
        };

        // Capture environment if configured
        if fixture.config.capture_environment {
            fixture.environment = Some(fixture.capture_environment());
            fixture.log_json(
                &LogEvent::new(
                    LogLevel::Info,
                    "environment",
                    "Environment captured",
                    &fixture.scenario_name,
                    0,
                    Duration::ZERO,
                )
                .with_data(serde_json::to_value(&fixture.environment).unwrap_or_default()),
            );
        }

        fixture
    }

    // ========================================================================
    // Structured JSON Logging
    // ========================================================================

    /// Log a structured event to JSON log file.
    pub fn log_json(&mut self, event: &LogEvent) {
        self.log_events.push(event.clone());

        if let Some(ref mut file) = self.json_log_file {
            if let Ok(json) = serde_json::to_string(event) {
                let _ = writeln!(file, "{}", json);
            }
        }
    }

    /// Create and log an event.
    fn emit_event(
        &mut self,
        level: LogLevel,
        category: &str,
        message: &str,
        data: Option<serde_json::Value>,
    ) {
        let event = LogEvent::new(
            level,
            category,
            message,
            &self.scenario_name,
            self.step_count,
            self.start_time.elapsed(),
        );
        let event = if let Some(d) = data {
            event.with_data(d)
        } else {
            event
        };
        self.log_json(&event);
    }

    // ========================================================================
    // Environment Capture
    // ========================================================================

    /// Capture environment information.
    pub fn capture_environment(&self) -> EnvironmentInfo {
        let mut env_vars = BTreeMap::new();
        let relevant_vars = [
            "HOME",
            "USER",
            "PATH",
            "RUST_BACKTRACE",
            "RUST_LOG",
            "MS_ROOT",
            "MS_CONFIG",
            "CARGO_PKG_VERSION",
        ];
        for var in relevant_vars {
            if let Ok(val) = std::env::var(var) {
                // Truncate PATH to avoid noise
                let val = if var == "PATH" && val.len() > 100 {
                    format!("{}...", &val[..100])
                } else {
                    val
                };
                env_vars.insert(var.to_string(), val);
            }
        }

        let mut tool_versions = BTreeMap::new();

        // Get Rust version
        if let Ok(output) = Command::new("rustc").arg("--version").output() {
            if output.status.success() {
                tool_versions.insert(
                    "rustc".to_string(),
                    String::from_utf8_lossy(&output.stdout).trim().to_string(),
                );
            }
        }

        // Get ms version
        if let Ok(output) = Command::new(env!("CARGO_BIN_EXE_ms"))
            .arg("--version")
            .output()
        {
            if output.status.success() {
                tool_versions.insert(
                    "ms".to_string(),
                    String::from_utf8_lossy(&output.stdout).trim().to_string(),
                );
            }
        }

        // Get git version
        if let Ok(output) = Command::new("git").arg("--version").output() {
            if output.status.success() {
                tool_versions.insert(
                    "git".to_string(),
                    String::from_utf8_lossy(&output.stdout).trim().to_string(),
                );
            }
        }

        EnvironmentInfo {
            timestamp: chrono::Utc::now().to_rfc3339(),
            os: std::env::consts::OS.to_string(),
            os_version: get_os_version(),
            arch: std::env::consts::ARCH.to_string(),
            rust_version: tool_versions.get("rustc").cloned(),
            ms_version: tool_versions.get("ms").cloned(),
            cwd: std::env::current_dir().unwrap_or_default(),
            env_vars,
            tool_versions,
            resources: SystemResources {
                cpu_count: std::thread::available_parallelism()
                    .map(|p| p.get())
                    .unwrap_or(1),
                memory_total: None,
                memory_available: None,
            },
        }
    }

    /// Log environment information to console and JSON.
    pub fn log_environment(&mut self) {
        let env = self
            .environment
            .clone()
            .unwrap_or_else(|| self.capture_environment());

        println!();
        println!("┌{}", "─".repeat(68));
        println!("│ ENVIRONMENT");
        println!("├{}", "─".repeat(68));
        println!("│ OS: {} ({}) on {}", env.os, env.os_version, env.arch);
        println!("│ CPUs: {}", env.resources.cpu_count);
        if let Some(ref rust_ver) = env.rust_version {
            println!("│ Rust: {}", rust_ver);
        }
        if let Some(ref ms_ver) = env.ms_version {
            println!("│ ms: {}", ms_ver);
        }
        println!("│ CWD: {:?}", env.cwd);
        println!(
            "│ Tools: {:?}",
            env.tool_versions.keys().collect::<Vec<_>>()
        );
        println!("└{}", "─".repeat(68));

        self.emit_event(
            LogLevel::Info,
            "environment",
            "Environment logged",
            Some(serde_json::to_value(&env).unwrap_or_default()),
        );
    }

    // ========================================================================
    // Database Snapshots
    // ========================================================================

    /// Capture a detailed database snapshot.
    pub fn snapshot_db(&self) -> Option<DbSnapshot> {
        let db = self.db.as_ref()?;
        let db_path = self.ms_root.join("ms.db");

        let size_bytes = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);

        let schema_version: Option<i64> =
            db.query_row("PRAGMA user_version", [], |r| r.get(0)).ok();

        let mut tables = BTreeMap::new();

        // Get list of tables
        let table_names: Vec<String> = {
            let mut stmt = db
                .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")
                .ok()?;
            stmt.query_map([], |r| r.get(0))
                .ok()?
                .filter_map(|r| r.ok())
                .collect()
        };

        for table_name in table_names {
            if let Some(snapshot) = self.snapshot_table(db, &table_name) {
                tables.insert(table_name, snapshot);
            }
        }

        Some(DbSnapshot {
            name: format!("db_snapshot_{}", self.step_count),
            timestamp: chrono::Utc::now().to_rfc3339(),
            tables,
            size_bytes,
            schema_version,
        })
    }

    /// Snapshot a single database table.
    fn snapshot_table(&self, db: &Connection, table_name: &str) -> Option<TableSnapshot> {
        // Get row count
        let count_query = format!("SELECT COUNT(*) FROM \"{}\"", table_name);
        let row_count: i64 = db.query_row(&count_query, [], |r| r.get(0)).ok()?;

        // Get column names
        let pragma_query = format!("PRAGMA table_info(\"{}\")", table_name);
        let columns: Vec<String> = {
            let mut stmt = db.prepare(&pragma_query).ok()?;
            stmt.query_map([], |r| r.get::<_, String>(1))
                .ok()?
                .filter_map(|r| r.ok())
                .collect()
        };

        // Get sample rows
        let sample_limit = self.config.db_sample_rows;
        let sample_query = format!("SELECT * FROM \"{}\" LIMIT {}", table_name, sample_limit);
        let sample_rows: Vec<serde_json::Value> = {
            let mut stmt = match db.prepare(&sample_query) {
                Ok(s) => s,
                Err(_) => {
                    return Some(TableSnapshot {
                        name: table_name.to_string(),
                        row_count,
                        sample_rows: Vec::new(),
                        columns,
                    });
                }
            };

            let col_count = stmt.column_count();
            let mut rows = Vec::new();

            if let Ok(mut query_rows) = stmt.query([]) {
                while let Ok(Some(row)) = query_rows.next() {
                    let mut row_map = serde_json::Map::new();
                    for i in 0..col_count {
                        let col_name = columns
                            .get(i)
                            .cloned()
                            .unwrap_or_else(|| format!("col{}", i));
                        let value: serde_json::Value = match row.get_ref(i) {
                            Ok(rusqlite::types::ValueRef::Null) => serde_json::Value::Null,
                            Ok(rusqlite::types::ValueRef::Integer(i)) => serde_json::json!(i),
                            Ok(rusqlite::types::ValueRef::Real(f)) => serde_json::json!(f),
                            Ok(rusqlite::types::ValueRef::Text(t)) => {
                                let s = String::from_utf8_lossy(t);
                                // Truncate long text values
                                if s.len() > 100 {
                                    serde_json::json!(format!("{}...", &s[..100]))
                                } else {
                                    serde_json::json!(s)
                                }
                            }
                            Ok(rusqlite::types::ValueRef::Blob(b)) => {
                                serde_json::json!(format!("<blob:{} bytes>", b.len()))
                            }
                            Err(_) => serde_json::Value::Null,
                        };
                        row_map.insert(col_name, value);
                    }
                    rows.push(serde_json::Value::Object(row_map));
                }
            }
            rows
        };

        Some(TableSnapshot {
            name: table_name.to_string(),
            row_count,
            sample_rows,
            columns,
        })
    }

    // ========================================================================
    // File System Snapshots
    // ========================================================================

    /// Capture a file system tree snapshot.
    pub fn snapshot_fs(&self) -> FsSnapshot {
        let mut files = BTreeMap::new();
        let mut directories = Vec::new();
        let mut total_size = 0u64;

        for entry in walkdir::WalkDir::new(&self.root)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            let relative = path.strip_prefix(&self.root).unwrap_or(path).to_path_buf();

            if entry.file_type().is_dir() {
                directories.push(relative.clone());
            } else if entry.file_type().is_file() {
                let metadata = entry.metadata().ok();
                let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
                total_size += size;

                let modified = metadata
                    .as_ref()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                #[cfg(unix)]
                let mode = {
                    use std::os::unix::fs::PermissionsExt;
                    metadata
                        .as_ref()
                        .map(|m| m.permissions().mode())
                        .unwrap_or(0)
                };

                let hash = if self.config.rich_fs_snapshots && size < 1_000_000 {
                    // Only hash files < 1MB for performance
                    compute_file_hash(path)
                } else {
                    String::new()
                };

                let is_symlink = entry.path_is_symlink();

                let file_info = FileInfo {
                    path: relative.clone(),
                    size,
                    hash,
                    modified,
                    #[cfg(unix)]
                    mode,
                    is_symlink,
                };

                files.insert(relative, file_info);
            }
        }

        FsSnapshot {
            name: format!("fs_snapshot_{}", self.step_count),
            timestamp: chrono::Utc::now().to_rfc3339(),
            root: self.root.clone(),
            file_count: files.len(),
            files,
            directories,
            total_size,
        }
    }

    // ========================================================================
    // Checkpoint Diffing
    // ========================================================================

    /// Get the diff between two checkpoints.
    pub fn checkpoint_diff(&self, from_name: &str, to_name: &str) -> Option<CheckpointDiff> {
        let from = self.checkpoints.iter().find(|c| c.name == from_name)?;
        let to = self.checkpoints.iter().find(|c| c.name == to_name)?;

        let db_diff = match (&from.db_snapshot, &to.db_snapshot) {
            (Some(from_db), Some(to_db)) => Some(from_db.diff(to_db)),
            _ => None,
        };

        let fs_diff = match (&from.fs_snapshot, &to.fs_snapshot) {
            (Some(from_fs), Some(to_fs)) => Some(from_fs.diff(to_fs)),
            _ => None,
        };

        Some(CheckpointDiff {
            from: from_name.to_string(),
            to: to_name.to_string(),
            time_delta: to.timestamp.saturating_sub(from.timestamp),
            step_delta: to.step_count.saturating_sub(from.step_count),
            db_diff,
            fs_diff,
        })
    }

    /// Get the diff between the last two checkpoints.
    pub fn last_checkpoint_diff(&self) -> Option<CheckpointDiff> {
        if self.checkpoints.len() < 2 {
            return None;
        }
        let len = self.checkpoints.len();
        self.checkpoint_diff(
            &self.checkpoints[len - 2].name,
            &self.checkpoints[len - 1].name,
        )
    }

    // ========================================================================
    // Timing Reports
    // ========================================================================

    /// Generate a timing breakdown report.
    pub fn timing_report(&self) -> TimingReport {
        let total_elapsed = self.start_time.elapsed();

        // Group by category
        let mut by_category: BTreeMap<String, Vec<&OperationTiming>> = BTreeMap::new();
        for timing in &self.operation_timings {
            by_category
                .entry(timing.category.clone())
                .or_default()
                .push(timing);
        }

        let category_timings: BTreeMap<String, CategoryTiming> = by_category
            .into_iter()
            .map(|(cat, ops)| {
                let total: Duration = ops.iter().map(|o| o.duration).sum();
                let count = ops.len();
                let average = if count > 0 {
                    total / count as u32
                } else {
                    Duration::ZERO
                };
                let min = ops
                    .iter()
                    .map(|o| o.duration)
                    .min()
                    .unwrap_or(Duration::ZERO);
                let max = ops
                    .iter()
                    .map(|o| o.duration)
                    .max()
                    .unwrap_or(Duration::ZERO);

                (
                    cat.clone(),
                    CategoryTiming {
                        category: cat,
                        total,
                        count,
                        average,
                        min,
                        max,
                    },
                )
            })
            .collect();

        // Find slowest operations
        let mut sorted_ops = self.operation_timings.clone();
        sorted_ops.sort_by(|a, b| b.duration.cmp(&a.duration));
        let slowest: Vec<OperationTiming> = sorted_ops.into_iter().take(5).collect();

        TimingReport {
            total_elapsed,
            operations: self.operation_timings.clone(),
            by_category: category_timings,
            slowest,
        }
    }

    // ========================================================================
    // Failure Diagnostics
    // ========================================================================

    /// Generate comprehensive failure diagnostics.
    pub fn failure_diagnostics(&mut self, error: &str, error_type: &str) -> FailureDiagnostics {
        // Get recent events (last 10)
        let recent_events: Vec<LogEvent> = self
            .log_events
            .iter()
            .rev()
            .take(10)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();

        // Generate suggestions based on error type
        let suggestions = generate_error_suggestions(error, error_type);

        FailureDiagnostics {
            error: error.to_string(),
            error_type: error_type.to_string(),
            step: self.step_count,
            step_name: self.current_step_name.clone(),
            elapsed: self.start_time.elapsed(),
            last_success: self.last_success_step.clone(),
            recent_events,
            db_state: self.snapshot_db(),
            fs_state: Some(self.snapshot_fs()),
            environment: self
                .environment
                .clone()
                .unwrap_or_else(|| self.capture_environment()),
            suggestions,
        }
    }

    // ========================================================================
    // Step and Checkpoint Methods (Enhanced)
    // ========================================================================

    /// Log a step in the E2E workflow.
    pub fn log_step(&mut self, description: &str) {
        self.step_count += 1;
        let elapsed = self.start_time.elapsed();
        self.current_step_name = description.to_string();

        println!();
        println!("┌{}", "─".repeat(68));
        println!("│ STEP {}: {}", self.step_count, description);
        println!("│ Time: {:?}", elapsed);
        println!("└{}", "─".repeat(68));

        // Emit JSON event
        self.emit_event(
            LogLevel::Info,
            "step",
            description,
            Some(serde_json::json!({
                "step_number": self.step_count,
                "elapsed_ms": elapsed.as_millis(),
            })),
        );
    }

    /// Capture a checkpoint for debugging.
    pub fn checkpoint(&mut self, name: &str) {
        let start = Instant::now();
        let timestamp = self.start_time.elapsed();

        // Collect files in root
        let files_created: Vec<PathBuf> = walkdir::WalkDir::new(&self.root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| e.path().to_path_buf())
            .collect();

        // Get simple db state if available (for backwards compatibility)
        let db_state = self.db.as_ref().map(|db| self.dump_db_state(db));

        // Get rich snapshots if configured
        let db_snapshot = if self.config.rich_db_snapshots {
            self.snapshot_db()
        } else {
            None
        };

        let fs_snapshot = if self.config.rich_fs_snapshots {
            Some(self.snapshot_fs())
        } else {
            None
        };

        let checkpoint = Checkpoint {
            name: name.to_string(),
            timestamp,
            step_count: self.step_count,
            db_state,
            db_snapshot: db_snapshot.clone(),
            fs_snapshot: fs_snapshot.clone(),
            files_created: files_created.clone(),
        };

        println!();
        println!("[CHECKPOINT] {}", name);
        println!("[CHECKPOINT] Files: {}", files_created.len());
        if let Some(ref state) = checkpoint.db_state {
            println!("[CHECKPOINT] DB: {}", state);
        }
        if let Some(ref db_snap) = db_snapshot {
            println!("[CHECKPOINT] DB Tables: {}", db_snap.tables.len());
        }
        if let Some(ref fs_snap) = fs_snapshot {
            println!(
                "[CHECKPOINT] FS: {} files, {} bytes",
                fs_snap.file_count, fs_snap.total_size
            );
        }

        // Record timing
        let checkpoint_duration = start.elapsed();
        self.operation_timings.push(OperationTiming {
            name: format!("checkpoint:{}", name),
            category: "checkpoint".to_string(),
            duration: checkpoint_duration,
            start_offset: timestamp,
            step: self.step_count,
        });

        // Emit JSON event
        self.emit_event(
            LogLevel::Info,
            "checkpoint",
            name,
            Some(serde_json::json!({
                "file_count": files_created.len(),
                "db_tables": db_snapshot.as_ref().map(|s| s.tables.len()),
                "fs_total_size": fs_snapshot.as_ref().map(|s| s.total_size),
                "duration_ms": checkpoint_duration.as_millis(),
            })),
        );

        self.checkpoints.push(checkpoint);
    }

    /// Run ms CLI command and capture output.
    pub fn run_ms(&mut self, args: &[&str]) -> CommandOutput {
        let step_name = format!("ms {}", args.join(" "));
        let start_offset = self.start_time.elapsed();
        let start = Instant::now();

        println!();
        println!("[CMD] {}", step_name);

        let output = Command::new(env!("CARGO_BIN_EXE_ms"))
            .args(args)
            .env("HOME", &self.root)
            .env("MS_ROOT", &self.ms_root)
            .env("MS_CONFIG", &self.config_path)
            .current_dir(&self.root)
            .output()
            .expect("Failed to execute ms command");

        let elapsed = start.elapsed();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);

        let result = CommandOutput {
            success: output.status.success(),
            exit_code,
            stdout: stdout.clone(),
            stderr: stderr.clone(),
            elapsed,
        };

        println!("[CMD] Exit: {} ({:?})", result.exit_code, elapsed);
        if !stdout.is_empty() {
            let preview = if stdout.len() > 500 {
                format!("{}...", &stdout[..500])
            } else {
                stdout.clone()
            };
            println!("[STDOUT] {}", preview);
        }
        if !stderr.is_empty() {
            println!("[STDERR] {}", stderr);
        }

        // Record timing
        self.operation_timings.push(OperationTiming {
            name: step_name.clone(),
            category: "command".to_string(),
            duration: elapsed,
            start_offset,
            step: self.step_count,
        });

        // Record step result
        let summary = if result.success {
            format!("OK ({})", truncate(&stdout, 50))
        } else {
            format!("FAIL: {}", truncate(&stderr, 100))
        };

        self.step_results.push(StepResult {
            name: step_name.clone(),
            success: result.success,
            duration: elapsed,
            output_summary: summary.clone(),
            category: "command".to_string(),
            exit_code: Some(exit_code),
        });

        // Track success/failure for diagnostics
        if result.success {
            self.last_success_step = Some(step_name.clone());
        }

        // Emit JSON event
        let level = if result.success {
            LogLevel::Info
        } else {
            LogLevel::Error
        };
        self.emit_event(
            level,
            "command",
            &step_name,
            Some(serde_json::json!({
                "exit_code": exit_code,
                "success": result.success,
                "duration_ms": elapsed.as_millis(),
                "stdout_len": stdout.len(),
                "stderr_len": stderr.len(),
                "args": args,
            })),
        );

        result
    }

    /// Initialize ms in the test environment.
    pub fn init(&mut self) -> CommandOutput {
        self.run_ms(&["--robot", "init"])
    }

    /// Create a skill in the specified layer.
    pub fn create_skill(&self, name: &str, content: &str) -> Result<()> {
        self.create_skill_in_layer(name, content, "project")
    }

    /// Create a skill in a specific layer.
    pub fn create_skill_in_layer(&self, name: &str, content: &str, layer: &str) -> Result<()> {
        let skills_dir = self
            .skills_dirs
            .get(layer)
            .ok_or_else(|| MsError::ValidationFailed(format!("Unknown layer: {}", layer)))?;

        let skill_dir = skills_dir.join(name);
        std::fs::create_dir_all(&skill_dir)?;

        let skill_file = skill_dir.join("SKILL.md");
        std::fs::write(&skill_file, content)?;

        println!(
            "[SKILL] Created '{}' in layer '{}' ({} bytes)",
            name,
            layer,
            content.len()
        );
        Ok(())
    }

    /// Open database connection for verification.
    pub fn open_db(&mut self) {
        let db_path = self.ms_root.join("ms.db");
        if db_path.exists() {
            self.db = Some(Connection::open(&db_path).expect("Failed to open db"));
            println!("[E2E] Database opened: {:?}", db_path);
        } else {
            println!("[E2E] Database not found: {:?}", db_path);
        }
    }

    /// Assert command succeeded.
    pub fn assert_success(&self, output: &CommandOutput, operation: &str) {
        assert!(
            output.success,
            "[E2E] {} failed with exit code {}: {}",
            operation, output.exit_code, output.stderr
        );
        println!("[ASSERT] {} - SUCCESS", operation);
    }

    /// Assert output contains expected text.
    pub fn assert_output_contains(&self, output: &CommandOutput, expected: &str) {
        let found = output.stdout.contains(expected) || output.stderr.contains(expected);
        assert!(
            found,
            "[E2E] Output does not contain '{}'\nStdout: {}\nStderr: {}",
            expected,
            truncate(&output.stdout, 500),
            truncate(&output.stderr, 500)
        );
        println!("[ASSERT] Output contains '{}' - PASSED", expected);
    }

    /// Assert output does not contain text.
    pub fn assert_output_not_contains(&self, output: &CommandOutput, unexpected: &str) {
        let found = output.stdout.contains(unexpected) || output.stderr.contains(unexpected);
        assert!(
            !found,
            "[E2E] Output unexpectedly contains '{}'\nStdout: {}\nStderr: {}",
            unexpected,
            truncate(&output.stdout, 500),
            truncate(&output.stderr, 500)
        );
        println!("[ASSERT] Output does not contain '{}' - PASSED", unexpected);
    }

    /// Verify database state with custom check.
    pub fn verify_db_state(&self, check: impl FnOnce(&Connection) -> bool, description: &str) {
        if let Some(ref db) = self.db {
            let state = self.dump_db_state(db);
            println!("[DB STATE] {}", state);

            let result = check(db);
            assert!(result, "[E2E] Database check failed: {}", description);

            println!("[ASSERT] DB: {} - PASSED", description);
        } else {
            println!("[ASSERT] DB: {} - SKIPPED (no connection)", description);
        }
    }

    // ========================================================================
    // Report Generation (Enhanced)
    // ========================================================================

    /// Generate final test report.
    pub fn generate_report(&mut self) {
        let total_time = self.start_time.elapsed();
        let timing_report = self.timing_report();

        println!();
        println!("{}", "█".repeat(70));
        println!("█ E2E REPORT: {}", self.scenario_name);
        println!("{}", "█".repeat(70));
        println!();

        println!("SUMMARY");
        println!("───────────────────────────────────────────────────");
        println!("Total Steps: {}", self.step_count);
        println!("Checkpoints: {}", self.checkpoints.len());
        println!("Total Time:  {:?}", total_time);
        println!("Log Events:  {}", self.log_events.len());
        println!();

        println!("STEP RESULTS");
        println!("───────────────────────────────────────────────────");
        for (i, step) in self.step_results.iter().enumerate() {
            let status = if step.success { "✓" } else { "✗" };
            println!(
                "{:2}. {} {} ({:?})",
                i + 1,
                status,
                step.name,
                step.duration
            );
            if !step.success {
                println!("     └─ {}", step.output_summary);
            }
        }
        println!();

        println!("CHECKPOINTS");
        println!("───────────────────────────────────────────────────");
        for checkpoint in &self.checkpoints {
            println!(
                "  [{:?}] {} (step {}, {} files)",
                checkpoint.timestamp,
                checkpoint.name,
                checkpoint.step_count,
                checkpoint.files_created.len()
            );
        }
        println!();

        // Timing breakdown
        println!("TIMING BREAKDOWN BY CATEGORY");
        println!("───────────────────────────────────────────────────");
        for (cat, timing) in &timing_report.by_category {
            println!(
                "  {}: {} ops, total {:?}, avg {:?}",
                cat, timing.count, timing.total, timing.average
            );
        }
        println!();

        // Slowest operations
        if !timing_report.slowest.is_empty() {
            println!("SLOWEST OPERATIONS");
            println!("───────────────────────────────────────────────────");
            for op in timing_report.slowest.iter().take(5) {
                println!("  {:?} - {}", op.duration, op.name);
            }
            println!();
        }

        // Overall result
        let all_passed = self.step_results.iter().all(|s| s.success);
        if all_passed {
            println!("RESULT: ✓ ALL STEPS PASSED");
        } else {
            let failed_count = self.step_results.iter().filter(|s| !s.success).count();
            println!("RESULT: ✗ {} STEPS FAILED", failed_count);
        }

        println!();
        println!("{}", "█".repeat(70));

        // Emit final JSON report
        self.emit_event(
            if all_passed {
                LogLevel::Info
            } else {
                LogLevel::Error
            },
            "report",
            &format!(
                "Test {} completed",
                if all_passed { "PASSED" } else { "FAILED" }
            ),
            Some(serde_json::json!({
                "scenario": self.scenario_name,
                "total_steps": self.step_count,
                "checkpoints": self.checkpoints.len(),
                "total_time_ms": total_time.as_millis(),
                "all_passed": all_passed,
                "failed_count": self.step_results.iter().filter(|s| !s.success).count(),
                "timing_by_category": timing_report.by_category.keys().collect::<Vec<_>>(),
            })),
        );

        // Generate HTML report if configured
        if self.config.html_report {
            let html = self.generate_html_report();
            let html_path = self
                .config
                .html_report_path
                .clone()
                .unwrap_or_else(|| self.root.join("report.html"));
            if let Err(e) = std::fs::write(&html_path, &html) {
                eprintln!("[E2E] Failed to write HTML report: {}", e);
            } else {
                println!("[E2E] HTML report: {:?}", html_path);
            }
        }
    }

    /// Generate HTML report.
    pub fn generate_html_report(&self) -> String {
        let total_time = self.start_time.elapsed();
        let timing_report = self.timing_report();
        let all_passed = self.step_results.iter().all(|s| s.success);
        let failed_count = self.step_results.iter().filter(|s| !s.success).count();

        let mut html = String::new();

        // HTML header
        let _ = write!(
            html,
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>E2E Report: {}</title>
    <style>
        :root {{
            --success: #22c55e;
            --error: #ef4444;
            --warning: #f59e0b;
            --bg: #0f172a;
            --bg-secondary: #1e293b;
            --text: #f8fafc;
            --text-secondary: #94a3b8;
            --border: #334155;
        }}
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: var(--bg);
            color: var(--text);
            margin: 0;
            padding: 2rem;
            line-height: 1.6;
        }}
        .container {{ max-width: 1200px; margin: 0 auto; }}
        h1 {{ color: var(--text); border-bottom: 2px solid var(--border); padding-bottom: 0.5rem; }}
        h2 {{ color: var(--text-secondary); margin-top: 2rem; }}
        .summary {{
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
            gap: 1rem;
            margin: 1rem 0;
        }}
        .stat {{
            background: var(--bg-secondary);
            padding: 1rem;
            border-radius: 8px;
            border: 1px solid var(--border);
        }}
        .stat-value {{ font-size: 2rem; font-weight: bold; }}
        .stat-label {{ color: var(--text-secondary); font-size: 0.875rem; }}
        .success {{ color: var(--success); }}
        .error {{ color: var(--error); }}
        table {{
            width: 100%;
            border-collapse: collapse;
            margin: 1rem 0;
            background: var(--bg-secondary);
            border-radius: 8px;
            overflow: hidden;
        }}
        th, td {{
            padding: 0.75rem 1rem;
            text-align: left;
            border-bottom: 1px solid var(--border);
        }}
        th {{ background: var(--bg); color: var(--text-secondary); }}
        tr:hover {{ background: rgba(255,255,255,0.05); }}
        .status {{ width: 30px; text-align: center; }}
        .checkpoint {{ background: var(--bg-secondary); padding: 1rem; margin: 0.5rem 0; border-radius: 8px; }}
        .timing-bar {{
            height: 8px;
            background: var(--border);
            border-radius: 4px;
            overflow: hidden;
        }}
        .timing-fill {{
            height: 100%;
            background: var(--success);
            transition: width 0.3s;
        }}
        pre {{
            background: var(--bg);
            padding: 1rem;
            border-radius: 8px;
            overflow-x: auto;
            font-size: 0.875rem;
        }}
        .badge {{
            display: inline-block;
            padding: 0.25rem 0.5rem;
            border-radius: 4px;
            font-size: 0.75rem;
            font-weight: 600;
        }}
        .badge-success {{ background: rgba(34, 197, 94, 0.2); color: var(--success); }}
        .badge-error {{ background: rgba(239, 68, 68, 0.2); color: var(--error); }}
    </style>
</head>
<body>
    <div class="container">
        <h1>E2E Report: {}</h1>
        <div class="summary">
            <div class="stat">
                <div class="stat-value {}">{}</div>
                <div class="stat-label">Result</div>
            </div>
            <div class="stat">
                <div class="stat-value">{}</div>
                <div class="stat-label">Total Steps</div>
            </div>
            <div class="stat">
                <div class="stat-value">{}</div>
                <div class="stat-label">Checkpoints</div>
            </div>
            <div class="stat">
                <div class="stat-value">{:.2}s</div>
                <div class="stat-label">Total Time</div>
            </div>
            <div class="stat">
                <div class="stat-value {}">{}</div>
                <div class="stat-label">Failed</div>
            </div>
        </div>
"#,
            self.scenario_name,
            self.scenario_name,
            if all_passed { "success" } else { "error" },
            if all_passed { "PASSED" } else { "FAILED" },
            self.step_count,
            self.checkpoints.len(),
            total_time.as_secs_f64(),
            if failed_count > 0 { "error" } else { "success" },
            failed_count
        );

        // Step Results Table
        let _ = write!(
            html,
            r#"
        <h2>Step Results</h2>
        <table>
            <thead>
                <tr>
                    <th class="status">#</th>
                    <th>Step</th>
                    <th>Category</th>
                    <th>Duration</th>
                    <th>Status</th>
                </tr>
            </thead>
            <tbody>
"#
        );

        for (i, step) in self.step_results.iter().enumerate() {
            let status_class = if step.success { "success" } else { "error" };
            let status_icon = if step.success { "✓" } else { "✗" };
            let _ = write!(
                html,
                r#"
                <tr>
                    <td class="status">{}</td>
                    <td>{}</td>
                    <td>{}</td>
                    <td>{:?}</td>
                    <td><span class="badge badge-{}">{}</span></td>
                </tr>
"#,
                i + 1,
                escape_html(&step.name),
                &step.category,
                step.duration,
                status_class,
                status_icon
            );
        }

        let _ = write!(
            html,
            r#"
            </tbody>
        </table>
"#
        );

        // Timing Breakdown
        let _ = write!(
            html,
            r#"
        <h2>Timing Breakdown</h2>
        <table>
            <thead>
                <tr>
                    <th>Category</th>
                    <th>Count</th>
                    <th>Total</th>
                    <th>Average</th>
                    <th>Min</th>
                    <th>Max</th>
                </tr>
            </thead>
            <tbody>
"#
        );

        for (cat, timing) in &timing_report.by_category {
            let _ = write!(
                html,
                r#"
                <tr>
                    <td>{}</td>
                    <td>{}</td>
                    <td>{:?}</td>
                    <td>{:?}</td>
                    <td>{:?}</td>
                    <td>{:?}</td>
                </tr>
"#,
                cat, timing.count, timing.total, timing.average, timing.min, timing.max
            );
        }

        let _ = write!(
            html,
            r#"
            </tbody>
        </table>
"#
        );

        // Checkpoints
        let _ = write!(
            html,
            r#"
        <h2>Checkpoints</h2>
"#
        );

        for checkpoint in &self.checkpoints {
            let _ = write!(
                html,
                r#"
        <div class="checkpoint">
            <strong>{}</strong> (step {}, {:?})
            <div>Files: {}</div>
"#,
                escape_html(&checkpoint.name),
                checkpoint.step_count,
                checkpoint.timestamp,
                checkpoint.files_created.len()
            );

            if let Some(ref db_snap) = checkpoint.db_snapshot {
                let _ = write!(
                    html,
                    r#"
            <div>DB Tables: {} (size: {} bytes)</div>
"#,
                    db_snap.tables.len(),
                    db_snap.size_bytes
                );
            }

            if let Some(ref fs_snap) = checkpoint.fs_snapshot {
                let _ = write!(
                    html,
                    r#"
            <div>FS: {} files, {} bytes total</div>
"#,
                    fs_snap.file_count, fs_snap.total_size
                );
            }

            let _ = write!(
                html,
                r#"
        </div>
"#
            );
        }

        // Environment info
        if let Some(ref env) = self.environment {
            let _ = write!(
                html,
                r#"
        <h2>Environment</h2>
        <pre>{}</pre>
"#,
                serde_json::to_string_pretty(env).unwrap_or_default()
            );
        }

        // Footer
        let _ = write!(
            html,
            r#"
        <footer style="margin-top: 2rem; color: var(--text-secondary); font-size: 0.875rem;">
            Generated at {} | ms E2E Test Framework
        </footer>
    </div>
</body>
</html>
"#,
            chrono::Utc::now().to_rfc3339()
        );

        html
    }

    /// Dump database state for logging (legacy format).
    fn dump_db_state(&self, db: &Connection) -> String {
        let mut state = String::new();

        if let Ok(count) =
            db.query_row::<i64, _, _>("SELECT COUNT(*) FROM skills", [], |r| r.get(0))
        {
            state.push_str(&format!("skills={} ", count));
        }
        if let Ok(count) =
            db.query_row::<i64, _, _>("SELECT COUNT(*) FROM skills_fts", [], |r| r.get(0))
        {
            state.push_str(&format!("fts={} ", count));
        }
        if let Ok(count) =
            db.query_row::<i64, _, _>("SELECT COUNT(*) FROM skill_aliases", [], |r| r.get(0))
        {
            state.push_str(&format!("aliases={} ", count));
        }

        state
    }

    /// Export all log events to a JSON file.
    pub fn export_logs(&self, path: &std::path::Path) -> std::io::Result<()> {
        let file = std::fs::File::create(path)?;
        serde_json::to_writer_pretty(file, &self.log_events)?;
        Ok(())
    }

    /// Get all checkpoints.
    pub fn checkpoints(&self) -> &[Checkpoint] {
        &self.checkpoints
    }

    /// Get all log events.
    pub fn log_events(&self) -> &[LogEvent] {
        &self.log_events
    }
}

impl Drop for E2EFixture {
    fn drop(&mut self) {
        let elapsed = self.start_time.elapsed();
        println!();
        println!("{}", "█".repeat(70));
        println!("█ E2E CLEANUP: {}", self.scenario_name);
        println!("█ Total time: {:?}", elapsed);
        println!("█ Temp dir: {:?}", self.temp_dir.path());
        println!("{}", "█".repeat(70));
    }
}

/// Command output structure.
pub struct CommandOutput {
    pub success: bool,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub elapsed: Duration,
}

impl CommandOutput {
    /// Parse stdout as JSON.
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.stdout).expect("stdout should be valid JSON")
    }
}

// ============================================================================
// Checkpoint Diff Types
// ============================================================================

/// Difference between two checkpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointDiff {
    /// Name of the "from" checkpoint
    pub from: String,
    /// Name of the "to" checkpoint
    pub to: String,
    /// Time elapsed between checkpoints
    pub time_delta: Duration,
    /// Steps between checkpoints
    pub step_delta: usize,
    /// Database state changes
    pub db_diff: Option<DbSnapshotDiff>,
    /// File system changes
    pub fs_diff: Option<FsSnapshotDiff>,
}

impl CheckpointDiff {
    /// Check if there were any changes between checkpoints.
    pub fn has_changes(&self) -> bool {
        if let Some(ref db) = self.db_diff {
            if !db.added_tables.is_empty()
                || !db.removed_tables.is_empty()
                || !db.row_count_changes.is_empty()
            {
                return true;
            }
        }
        if let Some(ref fs) = self.fs_diff {
            if !fs.added_files.is_empty()
                || !fs.removed_files.is_empty()
                || !fs.modified_files.is_empty()
            {
                return true;
            }
        }
        false
    }

    /// Get a summary of changes.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();

        if let Some(ref db) = self.db_diff {
            let total_changes =
                db.added_tables.len() + db.removed_tables.len() + db.row_count_changes.len();
            if total_changes > 0 {
                parts.push(format!("DB: {} changes", total_changes));
            }
        }

        if let Some(ref fs) = self.fs_diff {
            let total_changes =
                fs.added_files.len() + fs.removed_files.len() + fs.modified_files.len();
            if total_changes > 0 {
                parts.push(format!(
                    "FS: +{} -{} ~{}",
                    fs.added_files.len(),
                    fs.removed_files.len(),
                    fs.modified_files.len()
                ));
            }
        }

        if parts.is_empty() {
            "No changes".to_string()
        } else {
            parts.join(", ")
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Truncate string for display.
fn truncate(s: &str, max_len: usize) -> String {
    if max_len == 0 {
        return String::new();
    }
    if s.chars().count() <= max_len {
        s.to_string()
    } else if max_len < 3 {
        ".".repeat(max_len)
    } else {
        let trimmed: String = s.chars().take(max_len - 3).collect();
        format!("{trimmed}...")
    }
}

/// Escape HTML special characters.
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Get OS version string.
fn get_os_version() -> String {
    #[cfg(target_os = "linux")]
    {
        if let Ok(output) = Command::new("uname").arg("-r").output() {
            if output.status.success() {
                return String::from_utf8_lossy(&output.stdout).trim().to_string();
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = Command::new("sw_vers").arg("-productVersion").output() {
            if output.status.success() {
                return String::from_utf8_lossy(&output.stdout).trim().to_string();
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = Command::new("cmd").args(["/C", "ver"]).output() {
            if output.status.success() {
                return String::from_utf8_lossy(&output.stdout).trim().to_string();
            }
        }
    }

    "unknown".to_string()
}

/// Compute SHA-256 hash of a file.
fn compute_file_hash(path: &std::path::Path) -> String {
    match std::fs::read(path) {
        Ok(contents) => {
            let mut hasher = Sha256::new();
            hasher.update(&contents);
            let result = hasher.finalize();
            hex::encode(result)
        }
        Err(_) => String::new(),
    }
}

/// Generate error suggestions based on error type and message.
fn generate_error_suggestions(error: &str, error_type: &str) -> Vec<String> {
    let mut suggestions = Vec::new();
    let error_lower = error.to_lowercase();

    // Database-related errors
    if error_lower.contains("database") || error_lower.contains("sqlite") {
        suggestions.push("Check if the database file exists and is not locked".to_string());
        suggestions.push("Verify database permissions".to_string());
        suggestions.push("Try running `ms doctor --fix` to repair database issues".to_string());
    }

    // File not found errors
    if error_lower.contains("not found") || error_lower.contains("no such file") {
        suggestions.push("Verify that the file path is correct".to_string());
        suggestions.push("Check if `ms init` was run before this operation".to_string());
        suggestions.push("Ensure skills are indexed with `ms index`".to_string());
    }

    // Permission errors
    if error_lower.contains("permission denied") || error_lower.contains("access denied") {
        suggestions.push("Check file and directory permissions".to_string());
        suggestions.push("Verify the test is running with appropriate privileges".to_string());
    }

    // JSON parsing errors
    if error_lower.contains("json") || error_lower.contains("parse") {
        suggestions.push("Verify the command output is valid JSON".to_string());
        suggestions.push("Check if `--robot` flag is being used for JSON output".to_string());
    }

    // Skill-related errors
    if error_lower.contains("skill") {
        suggestions.push("Verify skill file exists and is properly formatted".to_string());
        suggestions.push("Check skill YAML frontmatter syntax".to_string());
        suggestions.push("Run `ms validate <skill>` to check skill format".to_string());
    }

    // Index-related errors
    if error_lower.contains("index") || error_lower.contains("tantivy") {
        suggestions.push("Try rebuilding the index with `ms index`".to_string());
        suggestions.push("Check if the .ms/index directory is accessible".to_string());
    }

    // Generic suggestions based on error type
    match error_type {
        "command" => {
            suggestions.push("Review the command arguments".to_string());
            suggestions.push("Check command help with `ms <command> --help`".to_string());
        }
        "assertion" => {
            suggestions.push("Review the expected vs actual values".to_string());
            suggestions.push(
                "Check previous steps for issues that may have caused this failure".to_string(),
            );
        }
        _ => {}
    }

    // Always add these generic suggestions
    if suggestions.is_empty() {
        suggestions.push("Review the error message for details".to_string());
        suggestions.push("Check the log events for context".to_string());
        suggestions.push("Verify environment setup is correct".to_string());
    }

    suggestions
}
