//! Two-Phase Commit (2PC) for dual persistence
//!
//! All writes that touch both SQLite and Git are wrapped in a lightweight
//! transaction protocol to prevent split-brain states.
//!
//! ## Protocol Phases
//! 1. **Prepare**: Write intent to tx_log, stage changes
//! 2. **Pending**: Write to SQLite with pending marker
//! 3. **Commit**: Finalize Git commit
//! 4. **Complete**: Update SQLite, clean up tx_log

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{MsError, Result};
use crate::storage::git::GitArchive;

// =============================================================================
// TRANSACTION RECORD
// =============================================================================

/// A transaction record for 2PC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxRecord {
    /// Unique transaction ID
    pub id: String,
    /// Type of entity being written (e.g., "skill")
    pub entity_type: String,
    /// ID of the entity being written
    pub entity_id: String,
    /// Current phase: prepare, pending, committed, complete
    pub phase: TxPhase,
    /// Serialized payload
    pub payload_json: String,
    /// When the transaction was created
    pub created_at: DateTime<Utc>,
}

/// Transaction phases
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TxPhase {
    Prepare,
    Pending,
    Committed,
    Complete,
}

impl TxRecord {
    /// Create a new transaction record in prepare phase
    pub fn prepare<T: Serialize>(entity_type: &str, entity_id: &str, payload: &T) -> Result<Self> {
        Ok(Self {
            id: Uuid::new_v4().to_string(),
            entity_type: entity_type.to_string(),
            entity_id: entity_id.to_string(),
            phase: TxPhase::Prepare,
            payload_json: serde_json::to_string(payload)
                .map_err(|e| MsError::Serialization(e.to_string()))?,
            created_at: Utc::now(),
        })
    }
}

// =============================================================================
// GLOBAL FILE LOCKING
// =============================================================================

/// Advisory file lock for coordinating dual-persistence writes
pub struct GlobalLock {
    _lock_file: File,
    lock_path: PathBuf,
}

impl GlobalLock {
    const LOCK_FILENAME: &'static str = "ms.lock";

    /// Acquire exclusive lock (blocking)
    pub fn acquire(ms_root: &Path) -> io::Result<Self> {
        let lock_path = ms_root.join(Self::LOCK_FILENAME);

        // Ensure directory exists
        if let Some(parent) = lock_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;

        // Write lock holder info
        Self::write_lock_info(&lock_file)?;

        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = lock_file.as_raw_fd();
            // LOCK_EX = exclusive, blocks until acquired
            let result = unsafe { libc::flock(fd, libc::LOCK_EX) };
            if result != 0 {
                return Err(io::Error::last_os_error());
            }
        }

        #[cfg(not(unix))]
        {
            // Fallback: no-op on non-Unix (Windows would use LockFileEx)
        }

        Ok(Self {
            _lock_file: lock_file,
            lock_path,
        })
    }

    /// Try to acquire lock without blocking
    pub fn try_acquire(ms_root: &Path) -> io::Result<Option<Self>> {
        let lock_path = ms_root.join(Self::LOCK_FILENAME);

        if let Some(parent) = lock_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;

        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = lock_file.as_raw_fd();
            // LOCK_NB = non-blocking
            let result = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
            if result != 0 {
                let err = io::Error::last_os_error();
                if err.raw_os_error() == Some(libc::EWOULDBLOCK) {
                    return Ok(None); // Lock held by another process
                }
                return Err(err);
            }
        }

        Self::write_lock_info(&lock_file)?;

        Ok(Some(Self {
            _lock_file: lock_file,
            lock_path,
        }))
    }

    /// Acquire with timeout (polling)
    pub fn acquire_timeout(ms_root: &Path, timeout: Duration) -> io::Result<Option<Self>> {
        let start = std::time::Instant::now();
        let poll_interval = Duration::from_millis(50);

        while start.elapsed() < timeout {
            if let Some(lock) = Self::try_acquire(ms_root)? {
                return Ok(Some(lock));
            }
            std::thread::sleep(poll_interval);
        }

        Ok(None)
    }

    /// Write lock holder info to the file
    fn write_lock_info(file: &File) -> io::Result<()> {
        use std::io::Write;

        let info = LockInfo {
            pid: std::process::id(),
            acquired_at: Utc::now(),
            hostname: hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_else(|| "unknown".to_string()),
        };

        let mut file = file;
        file.set_len(0)?;
        let json = serde_json::to_string_pretty(&info).unwrap_or_default();
        file.write_all(json.as_bytes())?;
        file.sync_all()?;

        Ok(())
    }

    /// Read lock holder info
    pub fn read_lock_info(ms_root: &Path) -> io::Result<Option<LockInfo>> {
        let lock_path = ms_root.join(Self::LOCK_FILENAME);

        if !lock_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&lock_path)?;
        if content.is_empty() {
            return Ok(None);
        }

        match serde_json::from_str(&content) {
            Ok(info) => Ok(Some(info)),
            Err(_) => Ok(None),
        }
    }

    /// Check if the lock holder process is still running
    pub fn is_holder_alive(ms_root: &Path) -> bool {
        if let Ok(Some(info)) = Self::read_lock_info(ms_root) {
            #[cfg(unix)]
            {
                // Check if process exists
                unsafe {
                    libc::kill(info.pid as i32, 0) == 0
                }
            }
            #[cfg(not(unix))]
            {
                true // Assume alive on non-Unix
            }
        } else {
            false
        }
    }

    /// Force break a stale lock (use with caution)
    pub fn break_lock(ms_root: &Path) -> io::Result<bool> {
        let lock_path = ms_root.join(Self::LOCK_FILENAME);

        if !lock_path.exists() {
            return Ok(false);
        }

        // Only break if holder is dead
        if Self::is_holder_alive(ms_root) {
            return Ok(false);
        }

        fs::remove_file(&lock_path)?;
        Ok(true)
    }

    /// Get the lock file path
    pub fn path(&self) -> &Path {
        &self.lock_path
    }
}

impl Drop for GlobalLock {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = self._lock_file.as_raw_fd();
            unsafe { libc::flock(fd, libc::LOCK_UN) };
        }
        // Lock file is automatically unlocked when file handle is dropped
    }
}

/// Information about the lock holder
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockInfo {
    /// Process ID of the lock holder
    pub pid: u32,
    /// When the lock was acquired
    pub acquired_at: DateTime<Utc>,
    /// Hostname of the machine
    pub hostname: String,
}

// =============================================================================
// TRANSACTION MANAGER
// =============================================================================

/// Manager for two-phase commit transactions
pub struct TxManager {
    tx_dir: PathBuf,
    ms_root: PathBuf,
}

impl TxManager {
    /// Create a new transaction manager
    pub fn new(ms_root: PathBuf) -> Result<Self> {
        let tx_dir = ms_root.join("tx");
        fs::create_dir_all(&tx_dir)?;

        Ok(Self { tx_dir, ms_root })
    }

    /// Get the path to the ms root
    pub fn ms_root(&self) -> &Path {
        &self.ms_root
    }

    /// Write a transaction record to both tx_log and filesystem
    pub fn write_tx_record(&self, conn: &Connection, tx: &TxRecord) -> Result<()> {
        // Write to SQLite tx_log
        conn.execute(
            "INSERT INTO tx_log (id, entity_type, entity_id, phase, payload_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
            params![
                tx.id,
                tx.entity_type,
                tx.entity_id,
                format!("{:?}", tx.phase).to_lowercase(),
                tx.payload_json,
                tx.created_at.to_rfc3339(),
            ],
        )?;

        // Write to filesystem for crash recovery
        let tx_path = self.tx_dir.join(format!("{}.json", tx.id));
        let tx_json = serde_json::to_string_pretty(tx)
            .map_err(|e| MsError::Serialization(e.to_string()))?;
        fs::write(&tx_path, tx_json)?;

        // Sync to disk
        if let Ok(file) = File::open(&tx_path) {
            file.sync_all().ok();
        }

        Ok(())
    }

    /// Update transaction phase in tx_log
    pub fn update_tx_phase(&self, conn: &Connection, tx_id: &str, phase: TxPhase) -> Result<()> {
        conn.execute(
            "UPDATE tx_log SET phase = ? WHERE id = ?",
            params![format!("{:?}", phase).to_lowercase(), tx_id],
        )?;

        // Update filesystem record too
        let tx_path = self.tx_dir.join(format!("{}.json", tx_id));
        if tx_path.exists() {
            let content = fs::read_to_string(&tx_path)?;
            if let Ok(mut tx) = serde_json::from_str::<TxRecord>(&content) {
                tx.phase = phase;
                let tx_json = serde_json::to_string_pretty(&tx)
                    .map_err(|e| MsError::Serialization(e.to_string()))?;
                fs::write(&tx_path, tx_json)?;
            }
        }

        Ok(())
    }

    /// Clean up a completed transaction
    pub fn cleanup_tx(&self, conn: &Connection, tx_id: &str) -> Result<()> {
        // Remove from tx_log table
        conn.execute("DELETE FROM tx_log WHERE id = ?", [tx_id])?;

        // Remove tx file
        let tx_path = self.tx_dir.join(format!("{}.json", tx_id));
        fs::remove_file(&tx_path).ok();

        Ok(())
    }

    /// Rollback a transaction
    pub fn rollback_tx(&self, conn: &Connection, tx: &TxRecord) -> Result<()> {
        // Remove any pending data from skills table
        conn.execute(
            "DELETE FROM skills WHERE id = ? AND source_path = 'pending'",
            [&tx.entity_id],
        )?;

        // Remove from tx_log
        conn.execute("DELETE FROM tx_log WHERE id = ?", [&tx.id])?;

        // Remove tx file
        let tx_path = self.tx_dir.join(format!("{}.json", tx.id));
        fs::remove_file(&tx_path).ok();

        Ok(())
    }

    /// Find incomplete transactions
    pub fn find_incomplete_txs(&self, conn: &Connection) -> Result<Vec<TxRecord>> {
        let mut stmt = conn.prepare(
            "SELECT id, entity_type, entity_id, phase, payload_json, created_at
             FROM tx_log WHERE phase != 'complete'",
        )?;

        let txs = stmt
            .query_map([], |row| {
                let phase_str: String = row.get(3)?;
                let phase = match phase_str.as_str() {
                    "prepare" => TxPhase::Prepare,
                    "pending" => TxPhase::Pending,
                    "committed" => TxPhase::Committed,
                    "complete" => TxPhase::Complete,
                    _ => TxPhase::Prepare,
                };

                let created_str: String = row.get(5)?;
                let created_at = DateTime::parse_from_rfc3339(&created_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());

                Ok(TxRecord {
                    id: row.get(0)?,
                    entity_type: row.get(1)?,
                    entity_id: row.get(2)?,
                    phase,
                    payload_json: row.get(4)?,
                    created_at,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(txs)
    }

    /// Recover from incomplete transactions on startup
    pub fn recover(&self, conn: &Connection, git: &GitArchive) -> Result<RecoveryReport> {
        let mut report = RecoveryReport::default();

        // Find incomplete transactions in tx_log
        let txs = self.find_incomplete_txs(conn)?;

        for tx in txs {
            match tx.phase {
                TxPhase::Prepare => {
                    // Transaction never started - roll back
                    tracing::info!("Rolling back prepare-only tx: {}", tx.id);
                    self.rollback_tx(conn, &tx)?;
                    report.rolled_back += 1;
                }
                TxPhase::Pending => {
                    // SQLite written but Git not committed - roll back
                    tracing::info!("Rolling back pending tx: {}", tx.id);
                    self.rollback_tx(conn, &tx)?;
                    report.rolled_back += 1;
                }
                TxPhase::Committed => {
                    // Git committed but not marked complete - complete it
                    tracing::info!("Completing committed tx: {}", tx.id);
                    self.complete_committed_tx(conn, git, &tx)?;
                    report.completed += 1;
                }
                TxPhase::Complete => {
                    // Should not be in results, but clean up just in case
                    self.cleanup_tx(conn, &tx.id)?;
                }
            }
        }

        // Check tx_dir for orphaned tx files
        report.orphaned_files = self.cleanup_orphaned_tx_files(conn)?;

        Ok(report)
    }

    /// Complete a committed transaction
    fn complete_committed_tx(
        &self,
        conn: &Connection,
        git: &GitArchive,
        tx: &TxRecord,
    ) -> Result<()> {
        // Update skill with final values
        let skill_path = git.skill_path(&tx.entity_id);
        let content_hash = if skill_path.exists() {
            compute_file_hash(&skill_path)?
        } else {
            "unknown".to_string()
        };

        conn.execute(
            "UPDATE skills SET
             source_path = ?,
             content_hash = ?
             WHERE id = ?",
            params![
                skill_path.to_string_lossy(),
                content_hash,
                tx.entity_id,
            ],
        )?;

        // Mark as complete and clean up
        self.update_tx_phase(conn, &tx.id, TxPhase::Complete)?;
        self.cleanup_tx(conn, &tx.id)?;

        Ok(())
    }

    /// Clean up orphaned transaction files
    fn cleanup_orphaned_tx_files(&self, conn: &Connection) -> Result<usize> {
        let mut count = 0;

        if !self.tx_dir.exists() {
            return Ok(0);
        }

        for entry in fs::read_dir(&self.tx_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().map_or(false, |e| e == "json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(tx) = serde_json::from_str::<TxRecord>(&content) {
                        // Check if in database
                        let in_db: bool = conn.query_row(
                            "SELECT EXISTS(SELECT 1 FROM tx_log WHERE id = ?)",
                            [&tx.id],
                            |row| row.get(0),
                        )?;

                        if !in_db {
                            tracing::warn!("Removing orphaned tx file: {}", tx.id);
                            fs::remove_file(&path).ok();
                            count += 1;
                        }
                    } else {
                        // Invalid JSON, remove it
                        fs::remove_file(&path).ok();
                        count += 1;
                    }
                }
            }
        }

        Ok(count)
    }

    /// Acquire global lock with timeout
    pub fn acquire_lock(&self, timeout: Duration) -> Result<GlobalLock> {
        GlobalLock::acquire_timeout(&self.ms_root, timeout)?
            .ok_or_else(|| MsError::LockTimeout("Timeout waiting for global lock".to_string()))
    }

    /// Try to acquire global lock without blocking
    pub fn try_acquire_lock(&self) -> Result<Option<GlobalLock>> {
        Ok(GlobalLock::try_acquire(&self.ms_root)?)
    }
}

/// Report of recovery operations
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RecoveryReport {
    /// Number of transactions rolled back
    pub rolled_back: usize,
    /// Number of transactions completed
    pub completed: usize,
    /// Number of orphaned files cleaned up
    pub orphaned_files: usize,
}

impl RecoveryReport {
    /// Check if any recovery actions were taken
    pub fn had_actions(&self) -> bool {
        self.rolled_back > 0 || self.completed > 0 || self.orphaned_files > 0
    }
}

/// Compute SHA256 hash of a file
fn compute_file_hash(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};

    let content = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&content);
    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_tx_record_prepare() {
        #[derive(Serialize)]
        struct TestPayload {
            value: i32,
        }

        let payload = TestPayload { value: 42 };
        let tx = TxRecord::prepare("test", "entity-1", &payload).unwrap();

        assert!(!tx.id.is_empty());
        assert_eq!(tx.entity_type, "test");
        assert_eq!(tx.entity_id, "entity-1");
        assert_eq!(tx.phase, TxPhase::Prepare);
        assert!(tx.payload_json.contains("42"));
    }

    #[test]
    fn test_global_lock_acquire_release() {
        let dir = tempdir().unwrap();

        // First lock should succeed
        let lock1 = GlobalLock::try_acquire(dir.path()).unwrap();
        assert!(lock1.is_some());

        // Drop lock1
        drop(lock1);

        // Now another lock should succeed
        let lock2 = GlobalLock::try_acquire(dir.path()).unwrap();
        assert!(lock2.is_some());
    }

    #[test]
    fn test_global_lock_exclusive() {
        let dir = tempdir().unwrap();

        // First lock
        let _lock1 = GlobalLock::try_acquire(dir.path()).unwrap().unwrap();

        // Second lock should fail (non-blocking)
        let lock2 = GlobalLock::try_acquire(dir.path()).unwrap();
        assert!(lock2.is_none());
    }

    #[test]
    fn test_lock_info() {
        let dir = tempdir().unwrap();

        let _lock = GlobalLock::acquire(dir.path()).unwrap();

        let info = GlobalLock::read_lock_info(dir.path()).unwrap();
        assert!(info.is_some());

        let info = info.unwrap();
        assert_eq!(info.pid, std::process::id());
    }

    #[test]
    fn test_tx_manager_creation() {
        let dir = tempdir().unwrap();
        let manager = TxManager::new(dir.path().to_path_buf()).unwrap();

        assert!(dir.path().join("tx").exists());
        assert_eq!(manager.ms_root(), dir.path());
    }

    #[test]
    fn test_recovery_report() {
        let report = RecoveryReport::default();
        assert!(!report.had_actions());

        let report = RecoveryReport {
            rolled_back: 1,
            completed: 0,
            orphaned_files: 0,
        };
        assert!(report.had_actions());
    }
}
