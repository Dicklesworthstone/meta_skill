//! Tombstone-based deletion for ms-managed directories.
//!
//! Instead of immediately deleting files, this module moves them to a
//! tombstone directory with metadata. Files can later be permanently
//! removed via `ms prune --approve`.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{MsError, Result};

/// Tombstone record for a deleted item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TombstoneRecord {
    /// Unique tombstone ID (UUID).
    pub id: String,
    /// Original path relative to ms_root.
    pub original_path: String,
    /// Reason for deletion.
    pub reason: Option<String>,
    /// Timestamp of tombstone creation.
    pub tombstoned_at: DateTime<Utc>,
    /// Size in bytes of the tombstoned item.
    pub size_bytes: u64,
    /// Whether this is a directory.
    pub is_directory: bool,
    /// Session or agent that requested deletion.
    pub deleted_by: Option<String>,
}

/// Manager for tombstone operations.
#[derive(Debug)]
pub struct TombstoneManager {
    ms_root: PathBuf,
    tombstone_dir: PathBuf,
}

impl TombstoneManager {
    /// Create a new tombstone manager.
    pub fn new(ms_root: &Path) -> Self {
        let tombstone_dir = ms_root.join("tombstones");
        Self {
            ms_root: ms_root.to_path_buf(),
            tombstone_dir,
        }
    }

    /// Ensure the tombstone directory exists.
    pub fn ensure_tombstone_dir(&self) -> Result<()> {
        if !self.tombstone_dir.exists() {
            fs::create_dir_all(&self.tombstone_dir)?;
        }
        Ok(())
    }

    /// Tombstone a file or directory instead of deleting it.
    ///
    /// Returns the tombstone record.
    pub fn tombstone(
        &self,
        path: &Path,
        reason: Option<&str>,
        deleted_by: Option<&str>,
    ) -> Result<TombstoneRecord> {
        // Verify path is within ms_root
        let canonical_path = path.canonicalize().map_err(|e| {
            MsError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("cannot tombstone non-existent path: {}", e),
            ))
        })?;
        let canonical_root = self.ms_root.canonicalize()?;

        if !canonical_path.starts_with(&canonical_root) {
            return Err(MsError::Config(format!(
                "cannot tombstone path outside ms_root: {}",
                path.display()
            )));
        }

        // Get relative path
        let relative_path = canonical_path
            .strip_prefix(&canonical_root)
            .map_err(|_| MsError::Config("path prefix error".to_string()))?
            .to_string_lossy()
            .to_string();

        // Get metadata
        let metadata = fs::metadata(&canonical_path)?;
        let is_directory = metadata.is_dir();
        let size_bytes = if is_directory {
            self.dir_size(&canonical_path)?
        } else {
            metadata.len()
        };

        // Generate tombstone ID
        let id = uuid::Uuid::new_v4().to_string();
        let tombstoned_at = Utc::now();

        // Create tombstone record
        let record = TombstoneRecord {
            id: id.clone(),
            original_path: relative_path,
            reason: reason.map(|s| s.to_string()),
            tombstoned_at,
            size_bytes,
            is_directory,
            deleted_by: deleted_by.map(|s| s.to_string()),
        };

        // Ensure tombstone directory exists
        self.ensure_tombstone_dir()?;

        // Move file/directory to tombstone location
        let tombstone_path = self.tombstone_dir.join(&id);
        if is_directory {
            fs::create_dir_all(&tombstone_path)?;
            self.copy_dir_recursive(&canonical_path, &tombstone_path)?;
            fs::remove_dir_all(&canonical_path)?;
        } else {
            fs::copy(&canonical_path, &tombstone_path)?;
            fs::remove_file(&canonical_path)?;
        }

        // Write metadata
        let meta_path = self.tombstone_dir.join(format!("{}.json", id));
        let meta_json = serde_json::to_string_pretty(&record)?;
        fs::write(&meta_path, meta_json)?;

        Ok(record)
    }

    /// List all tombstones.
    pub fn list(&self) -> Result<Vec<TombstoneRecord>> {
        if !self.tombstone_dir.exists() {
            return Ok(Vec::new());
        }

        let mut records = Vec::new();
        for entry in fs::read_dir(&self.tombstone_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                let content = fs::read_to_string(&path)?;
                if let Ok(record) = serde_json::from_str::<TombstoneRecord>(&content) {
                    records.push(record);
                }
            }
        }

        // Sort by tombstoned_at descending
        records.sort_by(|a, b| b.tombstoned_at.cmp(&a.tombstoned_at));
        Ok(records)
    }

    /// List tombstones older than a given duration.
    pub fn list_older_than(&self, days: u32) -> Result<Vec<TombstoneRecord>> {
        let cutoff = Utc::now() - chrono::Duration::days(i64::from(days));
        let all = self.list()?;
        Ok(all
            .into_iter()
            .filter(|r| r.tombstoned_at < cutoff)
            .collect())
    }

    /// Permanently delete a tombstone (requires approval).
    pub fn purge(&self, id: &str) -> Result<PurgeResult> {
        // Validate ID against path traversal attacks
        Self::validate_tombstone_id(id)?;

        let tombstone_path = self.tombstone_dir.join(id);
        let meta_path = self.tombstone_dir.join(format!("{}.json", id));

        if !meta_path.exists() {
            return Err(MsError::NotFound(format!("tombstone not found: {}", id)));
        }

        // Read the record first
        let content = fs::read_to_string(&meta_path)?;
        let record: TombstoneRecord = serde_json::from_str(&content)?;

        // Remove the tombstoned content - use record.is_directory for correctness
        if tombstone_path.exists() {
            if record.is_directory {
                fs::remove_dir_all(&tombstone_path)?;
            } else {
                fs::remove_file(&tombstone_path)?;
            }
        }

        // Remove the metadata
        fs::remove_file(&meta_path)?;

        Ok(PurgeResult {
            id: id.to_string(),
            original_path: record.original_path,
            bytes_freed: record.size_bytes,
        })
    }

    /// Restore a tombstoned item to its original location.
    pub fn restore(&self, id: &str) -> Result<RestoreResult> {
        // Validate ID against path traversal attacks
        Self::validate_tombstone_id(id)?;

        let tombstone_path = self.tombstone_dir.join(id);
        let meta_path = self.tombstone_dir.join(format!("{}.json", id));

        if !meta_path.exists() {
            return Err(MsError::NotFound(format!("tombstone not found: {}", id)));
        }

        // Read the record
        let content = fs::read_to_string(&meta_path)?;
        let record: TombstoneRecord = serde_json::from_str(&content)?;

        // Verify tombstone content exists
        if !tombstone_path.exists() {
            return Err(MsError::NotFound(format!(
                "tombstone content missing for {}: metadata exists but content at {} not found",
                id,
                tombstone_path.display()
            )));
        }

        // Compute original path
        let original_full_path = self.ms_root.join(&record.original_path);

        // Check if original location already exists
        if original_full_path.exists() {
            return Err(MsError::Config(format!(
                "cannot restore: path already exists: {}",
                original_full_path.display()
            )));
        }

        // Ensure parent directory exists
        if let Some(parent) = original_full_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Move back - use record.is_directory for correctness
        if record.is_directory {
            self.copy_dir_recursive(&tombstone_path, &original_full_path)?;
            fs::remove_dir_all(&tombstone_path)?;
        } else {
            fs::copy(&tombstone_path, &original_full_path)?;
            fs::remove_file(&tombstone_path)?;
        }

        // Remove metadata
        fs::remove_file(&meta_path)?;

        Ok(RestoreResult {
            id: id.to_string(),
            restored_path: record.original_path,
        })
    }

    /// Get total size of all tombstones.
    pub fn total_size(&self) -> Result<u64> {
        let records = self.list()?;
        Ok(records.iter().map(|r| r.size_bytes).sum())
    }

    fn dir_size(&self, path: &Path) -> Result<u64> {
        let mut total = 0;
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                total += self.dir_size(&entry.path())?;
            } else {
                total += metadata.len();
            }
        }
        Ok(total)
    }

    fn copy_dir_recursive(&self, src: &Path, dst: &Path) -> Result<()> {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            if src_path.is_dir() {
                self.copy_dir_recursive(&src_path, &dst_path)?;
            } else {
                fs::copy(&src_path, &dst_path)?;
            }
        }
        Ok(())
    }

    /// Validate a tombstone ID against path traversal attacks.
    ///
    /// Tombstone IDs should be UUIDs, so they should only contain
    /// hexadecimal characters and hyphens.
    fn validate_tombstone_id(id: &str) -> Result<()> {
        if id.is_empty() {
            return Err(MsError::ValidationFailed(
                "invalid tombstone ID: empty".to_string(),
            ));
        }
        if id.contains('/') || id.contains('\\') || id.contains('\0') {
            return Err(MsError::ValidationFailed(
                "invalid tombstone ID: contains path separator or null byte".to_string(),
            ));
        }
        if id.contains("..") {
            return Err(MsError::ValidationFailed(
                "invalid tombstone ID: contains path traversal sequence".to_string(),
            ));
        }
        // UUID format: 8-4-4-4-12 hex chars with hyphens
        // e.g., 550e8400-e29b-41d4-a716-446655440000
        if !id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
            return Err(MsError::ValidationFailed(format!(
                "invalid tombstone ID: contains invalid characters (expected UUID format)"
            )));
        }
        Ok(())
    }
}

/// Result of a purge operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PurgeResult {
    pub id: String,
    pub original_path: String,
    pub bytes_freed: u64,
}

/// Result of a restore operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreResult {
    pub id: String,
    pub restored_path: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_tombstone_file() {
        let tmp = TempDir::new().unwrap();
        let ms_root = tmp.path();
        let manager = TombstoneManager::new(ms_root);

        // Create a test file
        let test_file = ms_root.join("test.txt");
        fs::write(&test_file, "hello world").unwrap();

        // Tombstone it
        let record = manager
            .tombstone(&test_file, Some("test deletion"), None)
            .unwrap();

        assert!(!test_file.exists());
        assert_eq!(record.original_path, "test.txt");
        assert_eq!(record.size_bytes, 11);
        assert!(!record.is_directory);

        // List should show the tombstone
        let list = manager.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, record.id);
    }

    #[test]
    fn test_tombstone_restore() {
        let tmp = TempDir::new().unwrap();
        let ms_root = tmp.path();
        let manager = TombstoneManager::new(ms_root);

        // Create a test file
        let test_file = ms_root.join("test.txt");
        fs::write(&test_file, "hello world").unwrap();

        // Tombstone it
        let record = manager.tombstone(&test_file, None, None).unwrap();
        assert!(!test_file.exists());

        // Restore it
        let result = manager.restore(&record.id).unwrap();
        assert_eq!(result.restored_path, "test.txt");
        assert!(test_file.exists());
        assert_eq!(fs::read_to_string(&test_file).unwrap(), "hello world");

        // List should be empty
        let list = manager.list().unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn test_tombstone_purge() {
        let tmp = TempDir::new().unwrap();
        let ms_root = tmp.path();
        let manager = TombstoneManager::new(ms_root);

        // Create a test file
        let test_file = ms_root.join("test.txt");
        fs::write(&test_file, "hello world").unwrap();

        // Tombstone it
        let record = manager.tombstone(&test_file, None, None).unwrap();

        // Purge it
        let result = manager.purge(&record.id).unwrap();
        assert_eq!(result.bytes_freed, 11);

        // List should be empty
        let list = manager.list().unwrap();
        assert!(list.is_empty());

        // File should still not exist at original location
        assert!(!test_file.exists());
    }
}
