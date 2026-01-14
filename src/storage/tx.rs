//! Two-Phase Commit (2PC) for dual persistence to SQLite and Git.
//!
//! All writes that touch both stores are wrapped in a lightweight transaction
//! protocol to prevent split-brain states where one store is updated but the
//! other fails.

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::core::{spec_lens::compile_markdown, SkillLayer, SkillSpec};
use crate::error::{MsError, Result};

use super::git::GitArchive;
use super::sqlite::Database;

// =============================================================================
// FSYNC HELPER
// =============================================================================

/// Write content to a file and fsync to ensure data is persisted to disk.
/// This is critical for crash safety in 2PC - without fsync, data could be
/// lost if the system crashes before the OS flushes buffers.
fn write_and_sync(path: &Path, content: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = File::create(path)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

// =============================================================================
// TRANSACTION RECORD
// =============================================================================

/// Transaction phase in the 2PC protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TxPhase {
    /// Intent recorded, changes not yet staged
    Prepare,
    /// SQLite write pending, Git not yet committed
    Pending,
    /// Git committed, SQLite not yet marked complete
    Committed,
    /// Transaction completed successfully
    Complete,
}

impl std::fmt::Display for TxPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TxPhase::Prepare => write!(f, "prepare"),
            TxPhase::Pending => write!(f, "pending"),
            TxPhase::Committed => write!(f, "committed"),
            TxPhase::Complete => write!(f, "complete"),
        }
    }
}

/// Record of an in-flight transaction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxRecord {
    /// Unique transaction ID
    pub id: String,
    /// Entity type (e.g., "skill")
    pub entity_type: String,
    /// Entity ID being modified
    pub entity_id: String,
    /// Current phase
    pub phase: TxPhase,
    /// JSON-serialized payload
    pub payload_json: String,
    /// When transaction was created
    pub created_at: DateTime<Utc>,
}

impl TxRecord {
    /// Create a new transaction record in prepare phase
    pub fn prepare<T: Serialize>(entity_type: &str, entity_id: &str, payload: &T) -> Result<Self> {
        let payload_json = serde_json::to_string(payload)
            .map_err(|e| MsError::TransactionFailed(format!("serialize payload: {e}")))?;

        Ok(Self {
            id: Uuid::new_v4().to_string(),
            entity_type: entity_type.to_string(),
            entity_id: entity_id.to_string(),
            phase: TxPhase::Prepare,
            payload_json,
            created_at: Utc::now(),
        })
    }
}

// =============================================================================
// GLOBAL FILE LOCK
// =============================================================================

/// Advisory file lock for coordinating dual-persistence writes
pub struct GlobalLock {
    #[allow(dead_code)]
    lock_file: File,
    #[allow(dead_code)]
    lock_path: PathBuf,
}

impl GlobalLock {
    const LOCK_FILENAME: &'static str = "ms.lock";

    /// Acquire exclusive lock (blocking)
    pub fn acquire(ms_root: &Path) -> Result<Self> {
        let lock_path = ms_root.join(Self::LOCK_FILENAME);
        fs::create_dir_all(ms_root)?;

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|e| MsError::TransactionFailed(format!("open lock file: {e}")))?;

        // Use fs2's cross-platform exclusive lock (blocking)
        lock_file
            .lock_exclusive()
            .map_err(|e| MsError::TransactionFailed(format!("acquire exclusive lock: {e}")))?;

        // Write lock holder info through the locked file handle
        Self::write_holder_info(&lock_file)?;

        debug!("Acquired global lock at {:?}", lock_path);
        Ok(Self {
            lock_file,
            lock_path,
        })
    }

    /// Try to acquire lock without blocking
    pub fn try_acquire(ms_root: &Path) -> Result<Option<Self>> {
        let lock_path = ms_root.join(Self::LOCK_FILENAME);
        fs::create_dir_all(ms_root)?;

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|e| MsError::TransactionFailed(format!("open lock file: {e}")))?;

        // Use fs2's cross-platform try_lock (non-blocking)
        match lock_file.try_lock_exclusive() {
            Ok(()) => {
                // Lock acquired
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                debug!("Lock held by another process");
                return Ok(None);
            }
            Err(e) => {
                return Err(MsError::TransactionFailed(format!(
                    "try acquire lock: {e}"
                )));
            }
        }

        // Write lock holder info through the locked file handle
        Self::write_holder_info(&lock_file)?;

        debug!("Acquired global lock (non-blocking) at {:?}", lock_path);
        Ok(Some(Self {
            lock_file,
            lock_path,
        }))
    }

    /// Acquire with timeout (polling)
    pub fn acquire_timeout(ms_root: &Path, timeout: Duration) -> Result<Option<Self>> {
        let start = std::time::Instant::now();
        let poll_interval = Duration::from_millis(50);

        while start.elapsed() < timeout {
            if let Some(lock) = Self::try_acquire(ms_root)? {
                return Ok(Some(lock));
            }
            std::thread::sleep(poll_interval);
        }

        warn!(
            "Timeout waiting for lock after {:?}",
            start.elapsed()
        );
        Ok(None)
    }

    /// Check lock status without acquiring.
    ///
    /// **Note**: This returns cached holder info from the lock file, which may
    /// be stale. The actual OS-level flock is authoritative - use `is_locked()`
    /// for a definitive check of whether the lock is currently held.
    pub fn status(ms_root: &Path) -> Result<Option<LockHolder>> {
        let lock_path = ms_root.join(Self::LOCK_FILENAME);
        if !lock_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&lock_path)?;
        if content.is_empty() {
            return Ok(None);
        }

        let holder: LockHolder = serde_json::from_str(&content)
            .map_err(|e| MsError::TransactionFailed(format!("parse lock holder: {e}")))?;

        // Check if process is still alive using /proc on Linux
        #[cfg(target_os = "linux")]
        {
            let proc_path = format!("/proc/{}", holder.pid);
            if !std::path::Path::new(&proc_path).exists() {
                // Process no longer exists - lock is stale
                return Ok(None);
            }
        }

        // On other platforms, we trust the lock file content
        // The lock itself is enforced by the OS-level flock

        Ok(Some(holder))
    }

    /// Check if the lock is currently held (authoritative check).
    ///
    /// Returns true if another process holds the lock, false if it's available.
    /// This performs an actual flock check rather than reading cached info.
    pub fn is_locked(ms_root: &Path) -> Result<bool> {
        let lock_path = ms_root.join(Self::LOCK_FILENAME);
        if !lock_path.exists() {
            return Ok(false);
        }

        let lock_file = match OpenOptions::new().read(true).write(true).open(&lock_path) {
            Ok(f) => f,
            Err(_) => return Ok(false),
        };

        // Try to acquire lock non-blocking
        match lock_file.try_lock_exclusive() {
            Ok(()) => {
                // We got the lock - it wasn't held. Release it.
                lock_file.unlock().ok();
                Ok(false)
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // Lock is held by another process
                Ok(true)
            }
            Err(_) => {
                // Some other error - assume not locked
                Ok(false)
            }
        }
    }

    /// Break a stale lock.
    ///
    /// # Safety
    ///
    /// **WARNING**: This is dangerous and should only be used as a last resort!
    ///
    /// Removing the lock file while another process holds an flock on it can
    /// lead to split-brain scenarios: the original holder still has a valid
    /// flock on the (now deleted) inode, and a new process can create a new
    /// file and get a lock on a different inode.
    ///
    /// Only use this when you are CERTAIN the holder process is dead and the
    /// lock is truly stale. Prefer letting the OS release the lock naturally
    /// when the holder process terminates.
    pub fn break_lock(ms_root: &Path) -> Result<bool> {
        let lock_path = ms_root.join(Self::LOCK_FILENAME);
        if !lock_path.exists() {
            return Ok(false);
        }

        // Safety check: verify the lock isn't currently held
        if Self::is_locked(ms_root)? {
            warn!(
                "Refusing to break lock - it is currently held by another process. \
                 Wait for the holder to release it or terminate the holder process."
            );
            return Ok(false);
        }

        // Get holder info for logging (may be stale but useful for audit)
        if let Some(holder) = Self::status(ms_root)? {
            warn!(
                "Breaking stale lock (holder PID {} since {} appears dead)",
                holder.pid, holder.acquired_at
            );
        }

        tombstone_file(ms_root, &lock_path, "locks")?;
        info!("Stale lock file removed");
        Ok(true)
    }

    /// Write lock holder info through the given file handle.
    fn write_holder_info(file: &File) -> Result<()> {
        use std::io::{Seek, SeekFrom, Write};

        let holder = LockHolder {
            pid: std::process::id(),
            acquired_at: Utc::now(),
            hostname: hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_else(|| "unknown".to_string()),
        };
        let holder_json = serde_json::to_string(&holder)
            .map_err(|e| MsError::TransactionFailed(format!("serialize holder: {e}")))?;

        // Write through the locked handle (truncate and write)
        let mut file_ref = file;
        file_ref
            .set_len(0)
            .map_err(|e| MsError::TransactionFailed(format!("truncate lock file: {e}")))?;
        file_ref
            .seek(SeekFrom::Start(0))
            .map_err(|e| MsError::TransactionFailed(format!("seek lock file: {e}")))?;
        file_ref
            .write_all(holder_json.as_bytes())
            .map_err(|e| MsError::TransactionFailed(format!("write lock holder: {e}")))?;
        file_ref
            .sync_all()
            .map_err(|e| MsError::TransactionFailed(format!("sync lock file: {e}")))?;

        Ok(())
    }
}

impl Drop for GlobalLock {
    fn drop(&mut self) {
        // fs2's unlock is safe and cross-platform
        if let Err(e) = self.lock_file.unlock() {
            // Use debug level in drop - can't use error! without triggering additional allocations
            debug!("Failed to release lock: {}", e);
        }
        debug!("Released global lock");
    }
}

/// Information about the current lock holder
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockHolder {
    /// Process ID holding the lock
    pub pid: u32,
    /// When the lock was acquired
    pub acquired_at: DateTime<Utc>,
    /// Hostname of the lock holder
    pub hostname: String,
}

// =============================================================================
// TRANSACTION MANAGER
// =============================================================================

/// Two-Phase Commit transaction manager for dual persistence
pub struct TxManager {
    db: Arc<Database>,
    git: Arc<GitArchive>,
    tx_dir: PathBuf,
    ms_root: PathBuf,
}

impl TxManager {
    /// Create a new transaction manager
    pub fn new(db: Arc<Database>, git: Arc<GitArchive>, ms_root: PathBuf) -> Result<Self> {
        let tx_dir = ms_root.join("tx");
        fs::create_dir_all(&tx_dir)?;

        Ok(Self {
            db,
            git,
            tx_dir,
            ms_root,
        })
    }

    /// Write a skill with 2PC guarantees (without global lock)
    pub fn write_skill(&self, skill: &SkillSpec) -> Result<()> {
        self.write_skill_with_layer(skill, SkillLayer::Project)
    }

    /// Write a skill with 2PC guarantees and an explicit layer
    pub fn write_skill_with_layer(&self, skill: &SkillSpec, layer: SkillLayer) -> Result<()> {
        let tx = TxRecord::prepare("skill", &skill.metadata.id, skill)?;
        debug!("Starting 2PC transaction {} for skill {}", tx.id, skill.metadata.id);

        // Phase 1: Prepare - write intent
        self.write_tx_record(&tx)?;

        // Phase 2: Pending - write to SQLite
        let tx = self.db_write_pending(&tx, layer)?;

        // Phase 3: Commit - write to Git
        let tx = self.git_commit(&tx)?;

        // Phase 4: Complete - finalize SQLite
        let tx = self.db_mark_committed(&tx)?;

        // Cleanup
        self.cleanup_tx(&tx)?;

        info!("2PC transaction {} completed for skill {}", tx.id, skill.metadata.id);
        Ok(())
    }

    /// Write a skill with global lock coordination
    pub fn write_skill_locked(&self, skill: &SkillSpec) -> Result<()> {
        let _lock = GlobalLock::acquire_timeout(&self.ms_root, Duration::from_secs(30))?
            .ok_or_else(|| {
                MsError::TransactionFailed("timeout waiting for global lock".to_string())
            })?;

        self.write_skill_with_layer(skill, SkillLayer::Project)
    }

    /// Batch write skills with a single lock acquisition
    pub fn write_skills_batch(&self, skills: &[SkillSpec]) -> Result<()> {
        if skills.is_empty() {
            return Ok(());
        }

        let _lock = GlobalLock::acquire(&self.ms_root)?;

        for skill in skills {
            self.write_skill_with_layer(skill, SkillLayer::Project)?;
        }

        Ok(())
    }

    /// Delete a skill with 2PC guarantees
    pub fn delete_skill_locked(&self, skill_id: &str) -> Result<()> {
        let _lock = GlobalLock::acquire_timeout(&self.ms_root, Duration::from_secs(30))?
            .ok_or_else(|| {
                MsError::TransactionFailed("timeout waiting for global lock".to_string())
            })?;

        // Create delete transaction record
        let tx = TxRecord {
            id: Uuid::new_v4().to_string(),
            entity_type: "delete_skill".to_string(),
            entity_id: skill_id.to_string(),
            phase: TxPhase::Prepare,
            payload_json: "{}".to_string(), // No payload needed for delete
            created_at: Utc::now(),
        };
        debug!(
            "Starting 2PC delete transaction {} for skill {}",
            tx.id, skill_id
        );

        // Phase 1: Prepare - write intent
        self.write_tx_record(&tx)?;

        // Phase 2: Commit - delete from Git (creates commit)
        self.git.delete_skill(skill_id)?;
        let mut tx = tx;
        tx.phase = TxPhase::Committed;
        self.db.update_tx_phase(&tx.id, TxPhase::Committed)?;
        let tx_path = self.tx_dir.join(format!("{}.json", tx.id));
        let tx_json = serde_json::to_string_pretty(&tx)
            .map_err(|e| MsError::TransactionFailed(format!("serialize tx: {e}")))?;
        write_and_sync(&tx_path, &tx_json)?;

        // Phase 3: Complete - delete from SQLite
        self.db.delete_skill(skill_id)?;
        tx.phase = TxPhase::Complete;
        self.db.update_tx_phase(&tx.id, TxPhase::Complete)?;

        // Cleanup
        self.cleanup_tx(&tx)?;

        info!(
            "2PC delete transaction {} completed for skill {}",
            tx.id, skill_id
        );
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Internal transaction phases
    // -------------------------------------------------------------------------

    /// Write transaction record to tx_log and filesystem
    fn write_tx_record(&self, tx: &TxRecord) -> Result<()> {
        debug!("Phase: prepare (tx={})", tx.id);

        // Write to SQLite tx_log
        self.db.insert_tx_record(tx)?;

        // Write to filesystem for crash recovery (with fsync for durability)
        let tx_path = self.tx_dir.join(format!("{}.json", tx.id));
        let tx_json = serde_json::to_string_pretty(tx)
            .map_err(|e| MsError::TransactionFailed(format!("serialize tx: {e}")))?;
        write_and_sync(&tx_path, &tx_json)?;

        Ok(())
    }

    /// Write to SQLite in pending state
    fn db_write_pending(&self, tx: &TxRecord, layer: SkillLayer) -> Result<TxRecord> {
        debug!("Phase: pending (tx={})", tx.id);

        let skill: SkillSpec = serde_json::from_str(&tx.payload_json)
            .map_err(|e| MsError::TransactionFailed(format!("deserialize skill: {e}")))?;

        // Upsert skill with pending marker
        self.db.upsert_skill_pending(&skill, layer)?;

        // Update phase
        let mut tx = tx.clone();
        tx.phase = TxPhase::Pending;
        self.db.update_tx_phase(&tx.id, TxPhase::Pending)?;

        // Update filesystem tx record (with fsync for durability)
        let tx_path = self.tx_dir.join(format!("{}.json", tx.id));
        let tx_json = serde_json::to_string_pretty(&tx)
            .map_err(|e| MsError::TransactionFailed(format!("serialize tx: {e}")))?;
        write_and_sync(&tx_path, &tx_json)?;

        Ok(tx)
    }

    /// Commit to Git archive
    fn git_commit(&self, tx: &TxRecord) -> Result<TxRecord> {
        debug!("Phase: committed (tx={})", tx.id);

        let skill: SkillSpec = serde_json::from_str(&tx.payload_json)
            .map_err(|e| MsError::TransactionFailed(format!("deserialize skill: {e}")))?;

        // Write to Git
        self.git.write_skill(&skill)?;

        // Update phase
        let mut tx = tx.clone();
        tx.phase = TxPhase::Committed;
        self.db.update_tx_phase(&tx.id, TxPhase::Committed)?;

        // Update filesystem tx record (with fsync for durability)
        let tx_path = self.tx_dir.join(format!("{}.json", tx.id));
        let tx_json = serde_json::to_string_pretty(&tx)
            .map_err(|e| MsError::TransactionFailed(format!("serialize tx: {e}")))?;
        write_and_sync(&tx_path, &tx_json)?;

        Ok(tx)
    }

    /// Mark SQLite record as committed with final values
    fn db_mark_committed(&self, tx: &TxRecord) -> Result<TxRecord> {
        debug!("Phase: complete (tx={})", tx.id);

        let skill: SkillSpec = serde_json::from_str(&tx.payload_json)
            .map_err(|e| MsError::TransactionFailed(format!("deserialize skill: {e}")))?;

        // Update skill with final values
        let git_path = self
            .git
            .root()
            .join("skills/by-id")
            .join(&skill.metadata.id);
        let git_path_str = git_path.to_string_lossy();
        let content_hash = compute_content_hash(&skill)?;

        // Compile skill to markdown for FTS-searchable body
        let body = compile_markdown(&skill);

        self.db.finalize_skill_commit(&skill.metadata.id, &git_path_str, &content_hash, &body)?;

        // Update phase
        let mut tx = tx.clone();
        tx.phase = TxPhase::Complete;
        self.db.update_tx_phase(&tx.id, TxPhase::Complete)?;

        Ok(tx)
    }

    /// Clean up completed transaction
    fn cleanup_tx(&self, tx: &TxRecord) -> Result<()> {
        debug!("Cleanup tx={}", tx.id);

        // Remove from tx_log table
        self.db.delete_tx_record(&tx.id)?;

        // Tombstone tx file instead of deleting
        let tx_path = self.tx_dir.join(format!("{}.json", tx.id));
        let _ = tombstone_file(&self.ms_root, &tx_path, "tx");

        Ok(())
    }

    // -------------------------------------------------------------------------
    // Recovery
    // -------------------------------------------------------------------------

    /// Recover from incomplete transactions on startup
    pub fn recover(&self) -> Result<RecoveryReport> {
        info!("Starting transaction recovery");
        let mut report = RecoveryReport::default();

        // Find incomplete transactions in tx_log
        let txs = self.db.list_incomplete_transactions()?;

        for tx in txs {
            // Handle delete_skill transactions differently from write transactions
            if tx.entity_type == "delete_skill" {
                self.recover_delete_tx(&tx, &mut report)?;
            } else {
                self.recover_write_tx(&tx, &mut report)?;
            }
        }

        // Also check tx_dir for orphaned tx files
        if self.tx_dir.exists() {
            for entry in fs::read_dir(&self.tx_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("json") {
                    let tx_json = fs::read_to_string(&path)?;
                    let tx: TxRecord = match serde_json::from_str(&tx_json) {
                        Ok(tx) => tx,
                        Err(e) => {
                            warn!("Invalid tx file {:?}: {}", path, e);
                            tombstone_file(&self.ms_root, &path, "tx")?;
                            report.orphaned_files += 1;
                            continue;
                        }
                    };

                    // Check if in database
                    if !self.db.tx_exists(&tx.id)? {
                        warn!("Orphaned tx file: {}", tx.id);
                        tombstone_file(&self.ms_root, &path, "tx")?;
                        report.orphaned_files += 1;
                    }
                }
            }
        }

        if report.rolled_back > 0 || report.completed > 0 || report.orphaned_files > 0 {
            info!(
                "Recovery complete: {} rolled back, {} completed, {} orphaned files cleaned",
                report.rolled_back, report.completed, report.orphaned_files
            );
        } else {
            debug!("Recovery complete: no incomplete transactions found");
        }

        Ok(report)
    }

    /// Recover a write transaction (entity_type = "skill")
    fn recover_write_tx(&self, tx: &TxRecord, report: &mut RecoveryReport) -> Result<()> {
        match tx.phase {
            TxPhase::Prepare => {
                // Transaction never started - roll back
                info!("Rolling back prepare-only tx: {}", tx.id);
                self.rollback_tx(tx)?;
                report.rolled_back += 1;
            }
            TxPhase::Pending => {
                // SQLite written but phase still pending - need to check Git state.
                // If Git already has the skill (from a partial git_commit where
                // write_skill succeeded but update_tx_phase failed), complete the tx.
                // Otherwise, Git doesn't have it and we can safely roll back.
                if self.git.skill_exists(&tx.entity_id) {
                    info!("Completing pending tx with existing Git data: {}", tx.id);
                    let tx = self.db_mark_committed(tx)?;
                    self.cleanup_tx(&tx)?;
                    report.completed += 1;
                } else {
                    info!("Rolling back pending tx: {}", tx.id);
                    self.rollback_tx(tx)?;
                    report.rolled_back += 1;
                }
            }
            TxPhase::Committed => {
                // Git committed but not marked complete - complete it
                info!("Completing committed tx: {}", tx.id);
                let tx = self.db_mark_committed(tx)?;
                self.cleanup_tx(&tx)?;
                report.completed += 1;
            }
            TxPhase::Complete => {
                // Should not be in incomplete list, but cleanup if found
                warn!("Found complete tx in incomplete list: {}", tx.id);
                self.cleanup_tx(tx)?;
            }
        }
        Ok(())
    }

    /// Recover a delete transaction (entity_type = "delete_skill")
    fn recover_delete_tx(&self, tx: &TxRecord, report: &mut RecoveryReport) -> Result<()> {
        match tx.phase {
            TxPhase::Prepare => {
                // Check if Git delete actually happened (git.delete_skill succeeded but
                // db.update_tx_phase to Committed failed, leaving us at Prepare phase).
                // If skill is gone from Git, we need to complete the delete in SQLite.
                // If skill still exists in Git, delete truly never started.
                if self.git.skill_exists(&tx.entity_id) {
                    // Skill exists in Git - delete never started, clean up tx record
                    info!("Rolling back prepare-only delete tx: {}", tx.id);
                    self.db.delete_tx_record(&tx.id)?;
                    let tx_path = self.tx_dir.join(format!("{}.json", tx.id));
                    let _ = tombstone_file(&self.ms_root, &tx_path, "tx");
                    report.rolled_back += 1;
                } else {
                    // Skill gone from Git - delete happened, complete SQLite delete
                    info!("Completing delete tx (Git already deleted): {}", tx.id);
                    if let Err(e) = self.db.delete_skill(&tx.entity_id) {
                        debug!(
                            "SQLite delete during recovery (may already be gone): {}",
                            e
                        );
                    }
                    self.db.delete_tx_record(&tx.id)?;
                    let tx_path = self.tx_dir.join(format!("{}.json", tx.id));
                    let _ = tombstone_file(&self.ms_root, &tx_path, "tx");
                    report.completed += 1;
                }
            }
            TxPhase::Pending => {
                // Delete transactions skip Pending phase, but handle just in case.
                // Apply same logic as Prepare: check Git state to determine action.
                warn!("Unexpected Pending phase for delete tx: {}", tx.id);
                if self.git.skill_exists(&tx.entity_id) {
                    // Skill exists in Git - clean up tx record
                    self.db.delete_tx_record(&tx.id)?;
                    let tx_path = self.tx_dir.join(format!("{}.json", tx.id));
                    let _ = tombstone_file(&self.ms_root, &tx_path, "tx");
                    report.rolled_back += 1;
                } else {
                    // Skill gone from Git - complete SQLite delete
                    if let Err(e) = self.db.delete_skill(&tx.entity_id) {
                        debug!(
                            "SQLite delete during recovery (may already be gone): {}",
                            e
                        );
                    }
                    self.db.delete_tx_record(&tx.id)?;
                    let tx_path = self.tx_dir.join(format!("{}.json", tx.id));
                    let _ = tombstone_file(&self.ms_root, &tx_path, "tx");
                    report.completed += 1;
                }
            }
            TxPhase::Committed => {
                // Git deleted, SQLite not yet - complete the SQLite delete
                info!("Completing committed delete tx: {}", tx.id);
                // Try to delete from SQLite (may already be gone, that's ok)
                if let Err(e) = self.db.delete_skill(&tx.entity_id) {
                    debug!(
                        "SQLite delete during recovery (may already be gone): {}",
                        e
                    );
                }
                self.db.delete_tx_record(&tx.id)?;
                let tx_path = self.tx_dir.join(format!("{}.json", tx.id));
                let _ = tombstone_file(&self.ms_root, &tx_path, "tx");
                report.completed += 1;
            }
            TxPhase::Complete => {
                // Should not be in incomplete list, but cleanup if found
                warn!("Found complete delete tx in incomplete list: {}", tx.id);
                self.db.delete_tx_record(&tx.id)?;
                let tx_path = self.tx_dir.join(format!("{}.json", tx.id));
                let _ = tombstone_file(&self.ms_root, &tx_path, "tx");
            }
        }
        Ok(())
    }

    /// Roll back a transaction
    fn rollback_tx(&self, tx: &TxRecord) -> Result<()> {
        debug!("Rolling back tx={}", tx.id);

        // Remove from skills table if it was written with pending marker
        if tx.phase == TxPhase::Pending {
            self.db.delete_pending_skill(&tx.entity_id)?;
        }

        // Remove from tx_log
        self.db.delete_tx_record(&tx.id)?;

        // Tombstone tx file instead of deleting
        let tx_path = self.tx_dir.join(format!("{}.json", tx.id));
        let _ = tombstone_file(&self.ms_root, &tx_path, "tx");

        Ok(())
    }
}

/// Report of recovery actions taken
#[derive(Debug, Default)]
pub struct RecoveryReport {
    /// Number of transactions rolled back
    pub rolled_back: usize,
    /// Number of transactions completed
    pub completed: usize,
    /// Number of orphaned tx files cleaned
    pub orphaned_files: usize,
}

impl RecoveryReport {
    /// Check if any recovery actions were needed
    pub fn had_work(&self) -> bool {
        self.rolled_back > 0 || self.completed > 0 || self.orphaned_files > 0
    }
}

// =============================================================================
// HELPERS
// =============================================================================

/// Compute content hash for a skill spec
fn compute_content_hash(skill: &SkillSpec) -> Result<String> {
    use sha2::{Digest, Sha256};

    let json = serde_json::to_string(skill)
        .map_err(|e| MsError::TransactionFailed(format!("serialize skill for hash: {e}")))?;

    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());
    let result = hasher.finalize();

    Ok(hex::encode(result))
}

fn tombstone_file(ms_root: &Path, path: &Path, bucket: &str) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let tombstones = ms_root.join("tombstones").join(bucket);
    fs::create_dir_all(&tombstones)?;
    let name = path
        .file_name()
        .ok_or_else(|| MsError::ValidationFailed("invalid file name".to_string()))?;
    let now = chrono::Utc::now();
    let stamp = format!(
        "{}{:09}",
        now.format("%Y%m%dT%H%M%S"),
        now.timestamp_subsec_nanos()
    );
    let dest = tombstones.join(format!("{}_{}", name.to_string_lossy(), stamp));
    fs::rename(path, &dest)?;
    Ok(())
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{SkillMetadata, SkillSection};
    use tempfile::tempdir;

    fn sample_skill(id: &str) -> SkillSpec {
        SkillSpec {
            metadata: SkillMetadata {
                id: id.to_string(),
                name: format!("Test Skill {}", id),
                version: "1.0.0".to_string(),
                description: "A test skill".to_string(),
                ..Default::default()
            },
            sections: vec![SkillSection {
                id: "intro".to_string(),
                title: "Introduction".to_string(),
                blocks: vec![],
            }],
        }
    }

    #[test]
    fn test_tx_record_prepare() {
        let skill = sample_skill("test-1");
        let tx = TxRecord::prepare("skill", &skill.metadata.id, &skill).unwrap();

        assert!(!tx.id.is_empty());
        assert_eq!(tx.entity_type, "skill");
        assert_eq!(tx.entity_id, "test-1");
        assert_eq!(tx.phase, TxPhase::Prepare);
        assert!(tx.payload_json.contains("test-1"));
    }

    #[test]
    fn test_lock_acquisition_and_release() {
        let dir = tempdir().unwrap();
        let ms_root = dir.path().to_path_buf();

        // First lock should succeed
        let lock1 = GlobalLock::acquire(&ms_root).unwrap();

        // Second lock with try_acquire should fail
        let lock2 = GlobalLock::try_acquire(&ms_root).unwrap();
        assert!(lock2.is_none(), "Should not acquire lock while held");

        // Release first lock
        drop(lock1);

        // Now should succeed
        let lock3 = GlobalLock::try_acquire(&ms_root).unwrap();
        assert!(lock3.is_some(), "Should acquire lock after release");
    }

    #[test]
    fn test_lock_timeout() {
        let dir = tempdir().unwrap();
        let ms_root = dir.path().to_path_buf();

        // Acquire lock
        let _lock = GlobalLock::acquire(&ms_root).unwrap();

        // Timeout should return None quickly
        let start = std::time::Instant::now();
        let result = GlobalLock::acquire_timeout(&ms_root, Duration::from_millis(100)).unwrap();
        let elapsed = start.elapsed();

        assert!(result.is_none());
        assert!(elapsed >= Duration::from_millis(100));
        assert!(elapsed < Duration::from_millis(500)); // Reasonable upper bound (allow slack for busy systems)
    }

    #[test]
    fn test_lock_status_and_break() {
        let dir = tempdir().unwrap();
        let ms_root = dir.path().to_path_buf();

        // No lock initially
        let status = GlobalLock::status(&ms_root).unwrap();
        assert!(status.is_none());

        // Acquire lock
        let lock = GlobalLock::acquire(&ms_root).unwrap();

        // Status should show current process
        let status = GlobalLock::status(&ms_root).unwrap();
        assert!(status.is_some());
        let holder = status.unwrap();
        assert_eq!(holder.pid, std::process::id());

        // Release lock
        drop(lock);
    }

    #[test]
    fn test_successful_2pc() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let archive_path = dir.path().join("archive");
        let ms_root = dir.path().to_path_buf();

        let db = Arc::new(Database::open(&db_path).unwrap());
        let git = Arc::new(GitArchive::open(&archive_path).unwrap());
        let tx_mgr = TxManager::new(db.clone(), git.clone(), ms_root).unwrap();

        let skill = sample_skill("2pc-test");
        tx_mgr.write_skill(&skill).unwrap();

        // Verify skill exists in Git
        let git_skill = git.read_skill("2pc-test").unwrap();
        assert_eq!(git_skill.metadata.id, "2pc-test");

        // Verify skill exists in SQLite
        let db_skill = db.get_skill("2pc-test").unwrap();
        assert!(db_skill.is_some());

        // Verify no incomplete transactions
        let incomplete = db.list_incomplete_transactions().unwrap();
        assert!(incomplete.is_empty());

        // Verify no tx files remain
        assert!(
            !dir.path().join("tx").exists()
                || fs::read_dir(dir.path().join("tx"))
                    .unwrap()
                    .count()
                    == 0
        );
    }

    #[test]
    fn test_recovery_empty() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let archive_path = dir.path().join("archive");
        let ms_root = dir.path().to_path_buf();

        let db = Arc::new(Database::open(&db_path).unwrap());
        let git = Arc::new(GitArchive::open(&archive_path).unwrap());
        let tx_mgr = TxManager::new(db, git, ms_root).unwrap();

        let report = tx_mgr.recover().unwrap();
        assert!(!report.had_work());
    }

    #[test]
    fn test_compute_content_hash() {
        let skill1 = sample_skill("hash-test-1");
        let skill2 = sample_skill("hash-test-2");
        let skill1_copy = sample_skill("hash-test-1");

        let hash1 = compute_content_hash(&skill1).unwrap();
        let hash2 = compute_content_hash(&skill2).unwrap();
        let hash1_copy = compute_content_hash(&skill1_copy).unwrap();

        // Different skills should have different hashes
        assert_ne!(hash1, hash2);

        // Same skill should have same hash
        assert_eq!(hash1, hash1_copy);

        // Hash should be hex string of SHA256 (64 chars)
        assert_eq!(hash1.len(), 64);
    }

    #[test]
    fn test_is_locked() {
        let dir = tempdir().unwrap();
        let ms_root = dir.path().to_path_buf();

        // No lock file - should return false
        assert!(!GlobalLock::is_locked(&ms_root).unwrap());

        // Acquire lock
        let lock = GlobalLock::acquire(&ms_root).unwrap();

        // Should now be locked
        assert!(GlobalLock::is_locked(&ms_root).unwrap());

        // Release lock
        drop(lock);

        // Should no longer be locked
        assert!(!GlobalLock::is_locked(&ms_root).unwrap());
    }

    #[test]
    fn test_break_lock_refuses_held_lock() {
        let dir = tempdir().unwrap();
        let ms_root = dir.path().to_path_buf();

        // Acquire lock
        let _lock = GlobalLock::acquire(&ms_root).unwrap();

        // break_lock should refuse to break a held lock
        let result = GlobalLock::break_lock(&ms_root).unwrap();
        assert!(!result, "break_lock should refuse when lock is held");

        // Lock file should still exist
        assert!(ms_root.join("ms.lock").exists());
    }

    #[test]
    fn test_break_lock_removes_stale_lock() {
        let dir = tempdir().unwrap();
        let ms_root = dir.path().to_path_buf();

        // Create a lock file manually (simulating a stale lock from a dead process)
        let lock_path = ms_root.join("ms.lock");
        fs::create_dir_all(&ms_root).unwrap();
        fs::write(&lock_path, r#"{"pid":999999,"acquired_at":"2020-01-01T00:00:00Z","hostname":"test"}"#).unwrap();

        // break_lock should remove it (no flock is held)
        let result = GlobalLock::break_lock(&ms_root).unwrap();
        assert!(result, "break_lock should remove stale lock");

        // Lock file should be gone
        assert!(!lock_path.exists());
    }
}
