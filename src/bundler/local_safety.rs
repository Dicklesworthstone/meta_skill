//! Local Modification Safety
//!
//! Protects local user modifications when installing or updating bundles.
//! Ensures merges are safe, reversible, and explicit.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{MsError, Result};

/// Format a `SystemTime` as RFC3339 string
fn format_time(time: SystemTime) -> String {
    let datetime: DateTime<Utc> = time.into();
    datetime.to_rfc3339()
}

// =============================================================================
// MODIFICATION STATUS
// =============================================================================

/// Status of a local file relative to its tracked state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModificationStatus {
    /// File matches tracked hash - no local changes
    Clean,
    /// File has been modified since last sync
    Modified,
    /// File is new (not in bundle manifest)
    New,
    /// File was in bundle but is now missing
    Deleted,
    /// File is in conflict with incoming bundle
    Conflict,
}

/// Strategy for resolving conflicts during bundle operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictStrategy {
    /// Fail if any conflicts detected (safest, default)
    #[default]
    Abort,
    /// Keep local modifications, skip conflicting bundle files
    PreferLocal,
    /// Overwrite local modifications with bundle content
    PreferBundle,
    /// Create backup of local files before overwriting
    BackupAndReplace,
    /// Require interactive resolution for each conflict
    Interactive,
}

// =============================================================================
// FILE STATUS
// =============================================================================

/// Status of a single file in a skill directory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStatus {
    /// Relative path within the skill directory
    pub path: PathBuf,
    /// Modification status
    pub status: ModificationStatus,
    /// Current file hash (if file exists)
    pub current_hash: Option<String>,
    /// Expected hash from bundle/manifest
    pub expected_hash: Option<String>,
    /// File size in bytes (if exists)
    pub size: Option<u64>,
    /// Last modification time (if exists)
    pub modified_at: Option<String>,
}

impl FileStatus {
    /// Check if this file requires user attention
    #[must_use] 
    pub const fn needs_attention(&self) -> bool {
        matches!(
            self.status,
            ModificationStatus::Modified
                | ModificationStatus::Conflict
                | ModificationStatus::Deleted
        )
    }
}

// =============================================================================
// SKILL MODIFICATION REPORT
// =============================================================================

/// Report of modifications for a single skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillModificationReport {
    /// Skill identifier
    pub skill_id: String,
    /// Path to the skill directory
    pub skill_path: PathBuf,
    /// Overall status of the skill
    pub status: ModificationStatus,
    /// Per-file status
    pub files: Vec<FileStatus>,
    /// Summary counts
    pub summary: ModificationSummary,
}

/// Summary of modification counts
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModificationSummary {
    pub clean: usize,
    pub modified: usize,
    pub new: usize,
    pub deleted: usize,
    pub conflict: usize,
}

impl ModificationSummary {
    /// Total number of files
    #[must_use] 
    pub const fn total(&self) -> usize {
        self.clean + self.modified + self.new + self.deleted + self.conflict
    }

    /// Check if any files need attention
    #[must_use] 
    pub const fn needs_attention(&self) -> bool {
        self.modified > 0 || self.deleted > 0 || self.conflict > 0
    }
}

impl SkillModificationReport {
    /// Check if any files in this skill need attention
    #[must_use] 
    pub const fn needs_attention(&self) -> bool {
        self.summary.needs_attention()
    }

    /// Get list of modified files
    #[must_use] 
    pub fn modified_files(&self) -> Vec<&FileStatus> {
        self.files
            .iter()
            .filter(|f| f.status == ModificationStatus::Modified)
            .collect()
    }

    /// Get list of files in conflict
    #[must_use] 
    pub fn conflicting_files(&self) -> Vec<&FileStatus> {
        self.files
            .iter()
            .filter(|f| f.status == ModificationStatus::Conflict)
            .collect()
    }
}

// =============================================================================
// CONFLICT DETAIL
// =============================================================================

/// Details about a specific conflict
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictDetail {
    /// Skill identifier
    pub skill_id: String,
    /// File path relative to skill directory
    pub file_path: PathBuf,
    /// Hash of local file
    pub local_hash: String,
    /// Hash in incoming bundle
    pub bundle_hash: String,
    /// Whether local file is newer than bundle
    pub local_is_newer: bool,
    /// Size of local file
    pub local_size: u64,
    /// Size of bundle file
    pub bundle_size: u64,
}

// =============================================================================
// DETECTION
// =============================================================================

/// Compute SHA256 hash of file contents
pub fn hash_file(path: &Path) -> Result<String> {
    let content = std::fs::read(path)
        .map_err(|e| MsError::Config(format!("failed to read {}: {e}", path.display())))?;
    Ok(hash_bytes(&content))
}

/// Compute SHA256 hash of bytes
#[must_use] 
pub fn hash_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    hex::encode(result)
}

/// Compute hash of an entire directory
pub fn hash_directory(path: &Path) -> Result<HashMap<PathBuf, String>> {
    let mut hashes = HashMap::new();

    if !path.exists() || !path.is_dir() {
        return Ok(hashes);
    }

    for entry in walkdir::WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(std::result::Result::ok)
    {
        if entry.file_type().is_file() {
            let rel_path = entry
                .path()
                .strip_prefix(path)
                .map_err(|e| MsError::Config(format!("strip prefix: {e}")))?
                .to_path_buf();
            let hash = hash_file(entry.path())?;
            hashes.insert(rel_path, hash);
        }
    }

    Ok(hashes)
}

/// Detect local modifications in a skill directory
pub fn detect_modifications(
    skill_path: &Path,
    skill_id: &str,
    expected_hashes: &HashMap<PathBuf, String>,
) -> Result<SkillModificationReport> {
    let current_hashes = hash_directory(skill_path)?;
    let mut files = Vec::new();
    let mut summary = ModificationSummary::default();

    // Check files that should exist (from manifest)
    for (rel_path, expected_hash) in expected_hashes {
        let file_path = skill_path.join(rel_path);

        if let Some(current_hash) = current_hashes.get(rel_path) {
            let status = if current_hash == expected_hash {
                summary.clean += 1;
                ModificationStatus::Clean
            } else {
                summary.modified += 1;
                ModificationStatus::Modified
            };

            let metadata = std::fs::metadata(&file_path).ok();
            files.push(FileStatus {
                path: rel_path.clone(),
                status,
                current_hash: Some(current_hash.clone()),
                expected_hash: Some(expected_hash.clone()),
                size: metadata.as_ref().map(std::fs::Metadata::len),
                modified_at: metadata.and_then(|m| m.modified().ok().map(format_time)),
            });
        } else {
            summary.deleted += 1;
            files.push(FileStatus {
                path: rel_path.clone(),
                status: ModificationStatus::Deleted,
                current_hash: None,
                expected_hash: Some(expected_hash.clone()),
                size: None,
                modified_at: None,
            });
        }
    }

    // Check for new files (not in manifest)
    for (rel_path, current_hash) in &current_hashes {
        if !expected_hashes.contains_key(rel_path) {
            summary.new += 1;
            let file_path = skill_path.join(rel_path);
            let metadata = std::fs::metadata(&file_path).ok();

            files.push(FileStatus {
                path: rel_path.clone(),
                status: ModificationStatus::New,
                current_hash: Some(current_hash.clone()),
                expected_hash: None,
                size: metadata.as_ref().map(std::fs::Metadata::len),
                modified_at: metadata.and_then(|m| m.modified().ok().map(format_time)),
            });
        }
    }

    // Determine overall status
    let status = if summary.conflict > 0 {
        ModificationStatus::Conflict
    } else if summary.modified > 0 || summary.deleted > 0 {
        ModificationStatus::Modified
    } else if summary.new > 0 {
        ModificationStatus::New
    } else {
        ModificationStatus::Clean
    };

    // Sort files by path for consistent output
    files.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(SkillModificationReport {
        skill_id: skill_id.to_string(),
        skill_path: skill_path.to_path_buf(),
        status,
        files,
        summary,
    })
}

/// Detect conflicts between local files and incoming bundle
#[must_use] 
pub fn detect_conflicts(
    skill_path: &Path,
    skill_id: &str,
    bundle_hashes: &HashMap<PathBuf, String>,
    local_hashes: &HashMap<PathBuf, String>,
) -> Vec<ConflictDetail> {
    let mut conflicts = Vec::new();

    for (rel_path, bundle_hash) in bundle_hashes {
        if let Some(local_hash) = local_hashes.get(rel_path) {
            if local_hash != bundle_hash {
                let local_path = skill_path.join(rel_path);
                let local_meta = std::fs::metadata(&local_path);

                conflicts.push(ConflictDetail {
                    skill_id: skill_id.to_string(),
                    file_path: rel_path.clone(),
                    local_hash: local_hash.clone(),
                    bundle_hash: bundle_hash.clone(),
                    local_is_newer: true, // Conservative default
                    local_size: local_meta.as_ref().map(std::fs::Metadata::len).unwrap_or(0),
                    bundle_size: 0, // Would need bundle content to determine
                });
            }
        }
    }

    conflicts
}

// =============================================================================
// BACKUP
// =============================================================================

/// Backup destination for modified files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    /// Original path
    pub original_path: PathBuf,
    /// Backup path
    pub backup_path: PathBuf,
    /// Content hash
    pub content_hash: String,
    /// Timestamp
    pub created_at: String,
}

/// Create backup of a file before overwriting
pub fn backup_file(original_path: &Path, backup_root: &Path) -> Result<BackupInfo> {
    if !original_path.exists() {
        return Err(MsError::Config(format!(
            "cannot backup non-existent file: {}",
            original_path.display()
        )));
    }

    let content_hash = hash_file(original_path)?;
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();

    let backup_name = format!(
        "{}.{}.backup",
        original_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown"),
        timestamp
    );

    let backup_path = backup_root.join(&backup_name);

    std::fs::create_dir_all(backup_root)
        .map_err(|e| MsError::Config(format!("create backup dir: {e}")))?;

    std::fs::copy(original_path, &backup_path).map_err(|e| {
        MsError::Config(format!(
            "backup {} to {}: {e}",
            original_path.display(),
            backup_path.display()
        ))
    })?;

    Ok(BackupInfo {
        original_path: original_path.to_path_buf(),
        backup_path,
        content_hash,
        created_at: chrono::Utc::now().to_rfc3339(),
    })
}

/// Restore a file from backup
pub fn restore_from_backup(backup: &BackupInfo) -> Result<()> {
    if !backup.backup_path.exists() {
        return Err(MsError::Config(format!(
            "backup file not found: {}",
            backup.backup_path.display()
        )));
    }

    // Verify backup integrity
    let current_hash = hash_file(&backup.backup_path)?;
    if current_hash != backup.content_hash {
        return Err(MsError::Config(format!(
            "backup file corrupted: expected hash {}, got {}",
            backup.content_hash, current_hash
        )));
    }

    // Create parent directories if needed
    if let Some(parent) = backup.original_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| MsError::Config(format!("create restore dir: {e}")))?;
    }

    std::fs::copy(&backup.backup_path, &backup.original_path).map_err(|e| {
        MsError::Config(format!(
            "restore {} from {}: {e}",
            backup.original_path.display(),
            backup.backup_path.display()
        ))
    })?;

    Ok(())
}

// =============================================================================
// RESOLUTION
// =============================================================================

/// Result of conflict resolution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionResult {
    /// Files that were kept (local version preserved)
    pub kept_local: Vec<PathBuf>,
    /// Files that were replaced (bundle version used)
    pub replaced: Vec<PathBuf>,
    /// Files that were backed up before replacement
    pub backed_up: Vec<BackupInfo>,
    /// Files that still need resolution
    pub unresolved: Vec<PathBuf>,
}

impl ResolutionResult {
    #[must_use] 
    pub const fn new() -> Self {
        Self {
            kept_local: Vec::new(),
            replaced: Vec::new(),
            backed_up: Vec::new(),
            unresolved: Vec::new(),
        }
    }

    /// Check if all conflicts were resolved
    #[must_use] 
    pub fn is_complete(&self) -> bool {
        self.unresolved.is_empty()
    }
}

impl Default for ResolutionResult {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_hash_bytes() {
        let hash = hash_bytes(b"hello world");
        assert_eq!(hash.len(), 64); // SHA256 produces 64 hex chars
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_hash_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "hello world").unwrap();

        let hash = hash_file(&file_path).unwrap();
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_hash_directory() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "file a").unwrap();
        std::fs::write(dir.path().join("b.txt"), "file b").unwrap();
        std::fs::create_dir(dir.path().join("subdir")).unwrap();
        std::fs::write(dir.path().join("subdir/c.txt"), "file c").unwrap();

        let hashes = hash_directory(dir.path()).unwrap();
        assert_eq!(hashes.len(), 3);
        assert!(hashes.contains_key(&PathBuf::from("a.txt")));
        assert!(hashes.contains_key(&PathBuf::from("b.txt")));
        assert!(hashes.contains_key(&PathBuf::from("subdir/c.txt")));
    }

    #[test]
    fn test_detect_modifications_clean() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("SKILL.md"), "# Skill").unwrap();

        let mut expected = HashMap::new();
        expected.insert(PathBuf::from("SKILL.md"), hash_bytes(b"# Skill"));

        let report = detect_modifications(dir.path(), "test-skill", &expected).unwrap();
        assert_eq!(report.status, ModificationStatus::Clean);
        assert_eq!(report.summary.clean, 1);
        assert_eq!(report.summary.modified, 0);
    }

    #[test]
    fn test_detect_modifications_modified() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("SKILL.md"), "# Modified Skill").unwrap();

        let mut expected = HashMap::new();
        expected.insert(PathBuf::from("SKILL.md"), hash_bytes(b"# Original Skill"));

        let report = detect_modifications(dir.path(), "test-skill", &expected).unwrap();
        assert_eq!(report.status, ModificationStatus::Modified);
        assert_eq!(report.summary.modified, 1);
    }

    #[test]
    fn test_detect_modifications_new_file() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("SKILL.md"), "# Skill").unwrap();
        std::fs::write(dir.path().join("custom.txt"), "user content").unwrap();

        let mut expected = HashMap::new();
        expected.insert(PathBuf::from("SKILL.md"), hash_bytes(b"# Skill"));

        let report = detect_modifications(dir.path(), "test-skill", &expected).unwrap();
        assert_eq!(report.status, ModificationStatus::New);
        assert_eq!(report.summary.clean, 1);
        assert_eq!(report.summary.new, 1);
    }

    #[test]
    fn test_detect_modifications_deleted() {
        let dir = tempdir().unwrap();
        // Don't create the expected file

        let mut expected = HashMap::new();
        expected.insert(PathBuf::from("SKILL.md"), hash_bytes(b"# Skill"));

        let report = detect_modifications(dir.path(), "test-skill", &expected).unwrap();
        assert_eq!(report.status, ModificationStatus::Modified);
        assert_eq!(report.summary.deleted, 1);
    }

    #[test]
    fn test_backup_and_restore() {
        let dir = tempdir().unwrap();
        let original = dir.path().join("file.txt");
        let backup_root = dir.path().join("backups");

        std::fs::write(&original, "original content").unwrap();

        let backup_info = backup_file(&original, &backup_root).unwrap();
        assert!(backup_info.backup_path.exists());

        // Modify original
        std::fs::write(&original, "modified content").unwrap();

        // Restore
        restore_from_backup(&backup_info).unwrap();

        let restored = std::fs::read_to_string(&original).unwrap();
        assert_eq!(restored, "original content");
    }

    #[test]
    fn test_detect_conflicts() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("SKILL.md"), "local content").unwrap();

        let mut local_hashes = HashMap::new();
        local_hashes.insert(PathBuf::from("SKILL.md"), hash_bytes(b"local content"));

        let mut bundle_hashes = HashMap::new();
        bundle_hashes.insert(PathBuf::from("SKILL.md"), hash_bytes(b"bundle content"));

        let conflicts = detect_conflicts(dir.path(), "test-skill", &bundle_hashes, &local_hashes);

        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].file_path, PathBuf::from("SKILL.md"));
    }

    #[test]
    fn test_modification_summary() {
        let summary = ModificationSummary {
            clean: 5,
            modified: 2,
            new: 1,
            deleted: 0,
            conflict: 1,
        };

        assert_eq!(summary.total(), 9);
        assert!(summary.needs_attention());
    }
}
