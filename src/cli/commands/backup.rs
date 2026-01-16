//! ms backup - snapshot and restore ms state.

use std::path::{Path, PathBuf};

use chrono::Utc;
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

use crate::app::AppContext;
use crate::error::{MsError, Result};
use crate::storage::tx::GlobalLock;

#[derive(Args, Debug)]
pub struct BackupArgs {
    #[command(subcommand)]
    pub command: BackupCommand,
}

#[derive(Subcommand, Debug)]
pub enum BackupCommand {
    /// Create a new backup snapshot
    Create(BackupCreateArgs),
    /// List available backups
    List(BackupListArgs),
    /// Restore from a backup snapshot
    Restore(BackupRestoreArgs),
}

#[derive(Args, Debug)]
pub struct BackupCreateArgs {
    /// Backup ID (default: timestamp)
    #[arg(long)]
    pub id: Option<String>,
}

#[derive(Args, Debug)]
pub struct BackupListArgs {
    /// Maximum number of backups to show
    #[arg(long, default_value = "20")]
    pub limit: usize,
}

#[derive(Args, Debug)]
pub struct BackupRestoreArgs {
    /// Backup ID to restore
    pub id: Option<String>,

    /// Restore the most recent backup
    #[arg(long)]
    pub latest: bool,

    /// Apply restore (required)
    #[arg(long)]
    pub approve: bool,
}

#[derive(Serialize, Deserialize)]
struct BackupManifest {
    id: String,
    created_at: String,
    ms_root: String,
    config_path: Option<String>,
    entries: Vec<BackupEntry>,
    total_bytes: u64,
}

#[derive(Serialize, Deserialize)]
struct BackupEntry {
    name: String,
    source_path: String,
    backup_path: String,
    bytes: u64,
    is_dir: bool,
}

pub fn run(ctx: &AppContext, args: &BackupArgs) -> Result<()> {
    match &args.command {
        BackupCommand::Create(create) => run_create(ctx, create),
        BackupCommand::List(list) => run_list(ctx, list),
        BackupCommand::Restore(restore) => run_restore(ctx, restore),
    }
}

fn run_create(ctx: &AppContext, args: &BackupCreateArgs) -> Result<()> {
    let _lock = GlobalLock::acquire(&ctx.ms_root)?;
    let backup_root = backup_root(ctx);
    let backup_id = args.id.clone().unwrap_or_else(timestamp_id);
    validate_backup_id(&backup_id)?;

    let backup_dir = backup_root.join(&backup_id);
    if backup_dir.exists() {
        return Err(MsError::ValidationFailed(format!(
            "backup {backup_id} already exists"
        )));
    }
    std::fs::create_dir_all(&backup_dir)
        .map_err(|err| MsError::Config(format!("create {}: {err}", backup_dir.display())))?;

    let mut entries = Vec::new();
    let mut total_bytes = 0u64;

    let ms_db = ctx.ms_root.join("ms.db");
    push_file_entry(
        &mut entries,
        &mut total_bytes,
        "ms.db",
        &ms_db,
        &backup_dir.join("ms.db"),
    )?;
    let wal = ctx.ms_root.join("ms.db-wal");
    push_file_entry(
        &mut entries,
        &mut total_bytes,
        "ms.db-wal",
        &wal,
        &backup_dir.join("ms.db-wal"),
    )?;
    let shm = ctx.ms_root.join("ms.db-shm");
    push_file_entry(
        &mut entries,
        &mut total_bytes,
        "ms.db-shm",
        &shm,
        &backup_dir.join("ms.db-shm"),
    )?;

    let archive = ctx.ms_root.join("archive");
    if archive.exists() {
        let backup_path = backup_dir.join("archive");
        let bytes = copy_dir_recursive(&archive, &backup_path)?;
        total_bytes += bytes;
        entries.push(BackupEntry {
            name: "archive".to_string(),
            source_path: archive.display().to_string(),
            backup_path: backup_path.display().to_string(),
            bytes,
            is_dir: true,
        });
    }

    let index = ctx.ms_root.join("index");
    if index.exists() {
        let backup_path = backup_dir.join("index");
        let bytes = copy_dir_recursive(&index, &backup_path)?;
        total_bytes += bytes;
        entries.push(BackupEntry {
            name: "index".to_string(),
            source_path: index.display().to_string(),
            backup_path: backup_path.display().to_string(),
            bytes,
            is_dir: true,
        });
    }

    let config_path = if ctx.config_path.exists() {
        let backup_path = backup_dir.join("config.toml");
        let bytes = copy_file(&ctx.config_path, &backup_path)?;
        total_bytes += bytes;
        entries.push(BackupEntry {
            name: "config".to_string(),
            source_path: ctx.config_path.display().to_string(),
            backup_path: backup_path.display().to_string(),
            bytes,
            is_dir: false,
        });
        Some(ctx.config_path.display().to_string())
    } else {
        None
    };

    let manifest = BackupManifest {
        id: backup_id.clone(),
        created_at: Utc::now().to_rfc3339(),
        ms_root: ctx.ms_root.display().to_string(),
        config_path,
        entries,
        total_bytes,
    };
    let manifest_path = backup_dir.join("manifest.json");
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|err| MsError::Config(format!("serialize manifest: {err}")))?;
    std::fs::write(&manifest_path, manifest_json)
        .map_err(|err| MsError::Config(format!("write {}: {err}", manifest_path.display())))?;

    if ctx.robot_mode {
        return crate::cli::output::emit_json(&manifest);
    }

    println!("Backup created: {backup_id}");
    println!("Path: {}", backup_dir.display());
    println!("Entries: {}", manifest.entries.len());
    println!("Size: {} bytes", manifest.total_bytes);
    Ok(())
}

fn run_list(ctx: &AppContext, args: &BackupListArgs) -> Result<()> {
    let backup_root = backup_root(ctx);
    if !backup_root.exists() {
        if ctx.robot_mode {
            return crate::cli::output::emit_json(&serde_json::json!({
                "status": "ok",
                "count": 0,
                "backups": []
            }));
        }
        println!("No backups found.");
        return Ok(());
    }

    let mut backups = Vec::new();
    let mut dirs = std::fs::read_dir(&backup_root)
        .map_err(|err| MsError::Config(format!("read {}: {err}", backup_root.display())))?
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.path().is_dir())
        .collect::<Vec<_>>();
    dirs.sort_by_key(std::fs::DirEntry::file_name);
    dirs.reverse();

    for entry in dirs.into_iter().take(args.limit) {
        let path = entry.path();
        let manifest_path = path.join("manifest.json");
        if let Ok(content) = std::fs::read_to_string(&manifest_path) {
            if let Ok(manifest) = serde_json::from_str::<BackupManifest>(&content) {
                backups.push(manifest);
                continue;
            }
        }
        backups.push(BackupManifest {
            id: entry.file_name().to_string_lossy().to_string(),
            created_at: String::new(),
            ms_root: ctx.ms_root.display().to_string(),
            config_path: None,
            entries: Vec::new(),
            total_bytes: 0,
        });
    }

    if ctx.robot_mode {
        return crate::cli::output::emit_json(&serde_json::json!({
            "status": "ok",
            "count": backups.len(),
            "backups": backups,
        }));
    }

    if backups.is_empty() {
        println!("No backups found.");
        return Ok(());
    }

    println!("Backups:");
    for backup in backups {
        let created = if backup.created_at.is_empty() {
            "unknown".to_string()
        } else {
            backup.created_at.clone()
        };
        println!("  {}  {}  {} bytes", backup.id, created, backup.total_bytes);
    }
    Ok(())
}

fn run_restore(ctx: &AppContext, args: &BackupRestoreArgs) -> Result<()> {
    if !args.approve {
        return Err(MsError::ApprovalRequired(
            "backup restore requires --approve".to_string(),
        ));
    }
    let _lock = GlobalLock::acquire(&ctx.ms_root)?;

    let backup_root = backup_root(ctx);
    let backup_id = match (&args.id, args.latest) {
        (Some(id), false) => {
            validate_backup_id(id)?;
            id.clone()
        }
        (None, true) => latest_backup_id(&backup_root)?,
        (Some(_), true) => {
            return Err(MsError::ValidationFailed(
                "cannot use both id and --latest".to_string(),
            ))
        }
        (None, false) => {
            return Err(MsError::ValidationFailed(
                "restore requires backup id or --latest".to_string(),
            ))
        }
    };
    validate_backup_id(&backup_id)?;

    let backup_dir = backup_root.join(&backup_id);
    let manifest_path = backup_dir.join("manifest.json");
    if !manifest_path.exists() {
        return Err(MsError::NotFound(format!(
            "backup {backup_id} missing manifest"
        )));
    }
    let manifest_content = std::fs::read_to_string(&manifest_path)
        .map_err(|err| MsError::Config(format!("read {}: {err}", manifest_path.display())))?;
    let manifest: BackupManifest = serde_json::from_str(&manifest_content)?;

    let mut restored = 0usize;
    let mut skipped = Vec::new();
    for entry in &manifest.entries {
        let Some((source, dest, is_dir)) = restore_paths_for_entry(
            &ctx.ms_root,
            &ctx.config_path,
            &backup_dir,
            &entry.name,
        ) else {
            skipped.push(entry.name.clone());
            continue;
        };
        validate_restore_paths(
            source.to_string_lossy().as_ref(),
            dest.to_string_lossy().as_ref(),
            &backup_dir,
            &ctx.ms_root,
            ctx.config_path.as_path(),
        )?;
        if !source.exists() {
            skipped.push(entry.name.clone());
            continue;
        }
        if is_dir {
            let _ = copy_dir_recursive(&source, &dest)?;
        } else {
            let _ = copy_file(&source, &dest)?;
        }
        restored += 1;
    }

    if ctx.robot_mode {
        return crate::cli::output::emit_json(&serde_json::json!({
            "status": "ok",
            "restored": manifest.id,
            "entries": manifest.entries.len(),
            "restored_count": restored,
            "skipped": skipped,
        }));
    }

    println!("Restored backup: {}", manifest.id);
    println!("Entries restored: {restored}");
    if !skipped.is_empty() {
        println!("Entries skipped: {}", skipped.join(", "));
    }
    Ok(())
}

fn backup_root(ctx: &AppContext) -> PathBuf {
    ctx.ms_root.join("backups")
}

fn timestamp_id() -> String {
    Utc::now().format("%Y%m%d%H%M%S").to_string()
}

fn validate_backup_id(id: &str) -> Result<()> {
    if id.is_empty() {
        return Err(MsError::ValidationFailed("backup id is empty".to_string()));
    }
    if id.contains('/') || id.contains('\\') || id.contains("..") || id.contains('\0') {
        return Err(MsError::ValidationFailed(
            "backup id contains invalid path characters".to_string(),
        ));
    }
    Ok(())
}

/// Validate that restore paths are safe (no path traversal attacks).
///
/// - `backup_path` must be within `backup_dir`
/// - `dest_path` must be within `ms_root` or be the `config_path`
fn validate_restore_paths(
    backup_path: &str,
    dest_path: &str,
    backup_dir: &Path,
    ms_root: &Path,
    config_path: &Path,
) -> Result<()> {
    // Check for null bytes (path injection attack)
    if backup_path.contains('\0') || dest_path.contains('\0') {
        return Err(MsError::ValidationFailed(
            "restore path contains null byte".to_string(),
        ));
    }

    let backup_path = PathBuf::from(backup_path);
    let dest_path = PathBuf::from(dest_path);

    // Validate backup source is within backup_dir
    // Use lexical normalization first, then check prefix
    let normalized_backup = normalize_path(&backup_path);
    let normalized_backup_dir = normalize_path(backup_dir);
    if !normalized_backup.starts_with(&normalized_backup_dir) {
        return Err(MsError::ValidationFailed(format!(
            "backup path escapes backup directory: {}",
            backup_path.display()
        )));
    }

    // Validate destination is within ms_root or is config_path
    let normalized_dest = normalize_path(&dest_path);
    let normalized_ms_root = normalize_path(ms_root);
    let normalized_config = normalize_path(config_path);

    let dest_in_ms_root = normalized_dest.starts_with(&normalized_ms_root);
    let dest_is_config = normalized_dest == normalized_config
        || normalized_dest.starts_with(&normalized_config);

    if !dest_in_ms_root && !dest_is_config {
        return Err(MsError::ValidationFailed(format!(
            "restore destination outside allowed paths: {}",
            dest_path.display()
        )));
    }

    Ok(())
}

fn restore_paths_for_entry(
    ms_root: &Path,
    config_path: &Path,
    backup_dir: &Path,
    entry_name: &str,
) -> Option<(PathBuf, PathBuf, bool)> {
    match entry_name {
        "ms.db" => Some((
            backup_dir.join("ms.db"),
            ms_root.join("ms.db"),
            false,
        )),
        "ms.db-wal" => Some((
            backup_dir.join("ms.db-wal"),
            ms_root.join("ms.db-wal"),
            false,
        )),
        "ms.db-shm" => Some((
            backup_dir.join("ms.db-shm"),
            ms_root.join("ms.db-shm"),
            false,
        )),
        "archive" => Some((
            backup_dir.join("archive"),
            ms_root.join("archive"),
            true,
        )),
        "index" => Some((backup_dir.join("index"), ms_root.join("index"), true)),
        "config" => Some((
            backup_dir.join("config.toml"),
            config_path.to_path_buf(),
            false,
        )),
        _ => None,
    }
}

/// Normalize a path by resolving `.` and `..` components lexically.
/// This doesn't access the filesystem, so it works on non-existent paths.
fn normalize_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                result.pop();
            }
            std::path::Component::CurDir => {}
            other => {
                result.push(other);
            }
        }
    }
    result
}

fn latest_backup_id(root: &Path) -> Result<String> {
    if !root.exists() {
        return Err(MsError::NotFound("no backups found".to_string()));
    }
    let mut dirs = std::fs::read_dir(root)
        .map_err(|err| MsError::Config(format!("read {}: {err}", root.display())))?
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.path().is_dir())
        .collect::<Vec<_>>();
    dirs.sort_by_key(std::fs::DirEntry::file_name);
    let latest = dirs.pop().ok_or_else(|| MsError::NotFound("no backups found".to_string()))?;
    Ok(latest.file_name().to_string_lossy().to_string())
}

fn push_file_entry(
    entries: &mut Vec<BackupEntry>,
    total_bytes: &mut u64,
    name: &str,
    src: &Path,
    dst: &Path,
) -> Result<()> {
    if !src.exists() {
        return Ok(());
    }
    let bytes = copy_file(src, dst)?;
    *total_bytes += bytes;
    entries.push(BackupEntry {
        name: name.to_string(),
        source_path: src.display().to_string(),
        backup_path: dst.display().to_string(),
        bytes,
        is_dir: false,
    });
    Ok(())
}

fn copy_file(src: &Path, dst: &Path) -> Result<u64> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| MsError::Config(format!("create {}: {err}", parent.display())))?;
    }
    std::fs::copy(src, dst)
        .map_err(|err| MsError::Config(format!("copy {}: {err}", src.display())))
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<u64> {
    let mut total = 0u64;
    std::fs::create_dir_all(dst)
        .map_err(|err| MsError::Config(format!("create {}: {err}", dst.display())))?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            total += copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            total += copy_file(&src_path, &dst_path)?;
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_backup_id_rejects_paths() {
        assert!(validate_backup_id("ok-id").is_ok());
        assert!(validate_backup_id("").is_err());
        assert!(validate_backup_id("../bad").is_err());
        assert!(validate_backup_id("bad/seg").is_err());
        assert!(validate_backup_id("bad\\seg").is_err());
    }

    #[test]
    fn validate_backup_id_rejects_null_bytes() {
        assert!(validate_backup_id("bad\0id").is_err());
        assert!(validate_backup_id("\0").is_err());
    }

    #[test]
    fn latest_backup_id_picks_last_sorted() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("20240101010101")).unwrap();
        std::fs::create_dir_all(dir.path().join("20240202020202")).unwrap();

        let latest = latest_backup_id(dir.path()).unwrap();
        assert_eq!(latest, "20240202020202");
    }

    #[test]
    fn validate_restore_paths_accepts_valid_paths() {
        let backup_dir = Path::new("/backups/20240101");
        let ms_root = Path::new("/home/user/.ms");
        let config_path = Path::new("/home/user/.ms/config.toml");

        // Valid: backup path in backup_dir, dest in ms_root
        assert!(validate_restore_paths(
            "/backups/20240101/ms.db",
            "/home/user/.ms/ms.db",
            backup_dir,
            ms_root,
            config_path,
        )
        .is_ok());

        // Valid: dest is config_path
        assert!(validate_restore_paths(
            "/backups/20240101/config.toml",
            "/home/user/.ms/config.toml",
            backup_dir,
            ms_root,
            config_path,
        )
        .is_ok());
    }

    #[test]
    fn validate_restore_paths_rejects_path_traversal() {
        let backup_dir = Path::new("/backups/20240101");
        let ms_root = Path::new("/home/user/.ms");
        let config_path = Path::new("/home/user/.ms/config.toml");

        // Reject: backup path escapes backup_dir
        assert!(validate_restore_paths(
            "/backups/20240101/../20230101/ms.db",
            "/home/user/.ms/ms.db",
            backup_dir,
            ms_root,
            config_path,
        )
        .is_err());

        // Reject: dest escapes ms_root
        assert!(validate_restore_paths(
            "/backups/20240101/ms.db",
            "/home/user/.ms/../.bashrc",
            backup_dir,
            ms_root,
            config_path,
        )
        .is_err());

        // Reject: dest to arbitrary location
        assert!(validate_restore_paths(
            "/backups/20240101/evil",
            "/etc/passwd",
            backup_dir,
            ms_root,
            config_path,
        )
        .is_err());
    }

    #[test]
    fn validate_restore_paths_rejects_null_bytes() {
        let backup_dir = Path::new("/backups/20240101");
        let ms_root = Path::new("/home/user/.ms");
        let config_path = Path::new("/home/user/.ms/config.toml");

        // Reject: null byte in backup path
        assert!(validate_restore_paths(
            "/backups/20240101/ms\0.db",
            "/home/user/.ms/ms.db",
            backup_dir,
            ms_root,
            config_path,
        )
        .is_err());

        // Reject: null byte in dest path
        assert!(validate_restore_paths(
            "/backups/20240101/ms.db",
            "/home/user/.ms/ms\0.db",
            backup_dir,
            ms_root,
            config_path,
        )
        .is_err());
    }

    #[test]
    fn normalize_path_handles_parent_dir() {
        assert_eq!(
            normalize_path(Path::new("/a/b/../c")),
            PathBuf::from("/a/c")
        );
        assert_eq!(
            normalize_path(Path::new("/a/b/./c")),
            PathBuf::from("/a/b/c")
        );
        assert_eq!(
            normalize_path(Path::new("/a/b/../../c")),
            PathBuf::from("/c")
        );
    }

    #[test]
    fn restore_paths_for_entry_maps_known_paths() {
        let ms_root = Path::new("/tmp/ms-root");
        let config_path = Path::new("/tmp/ms-config/config.toml");
        let backup_dir = Path::new("/tmp/ms-backups/20240101010101");

        let (src, dst, is_dir) =
            restore_paths_for_entry(ms_root, config_path, backup_dir, "archive").unwrap();
        assert_eq!(src, backup_dir.join("archive"));
        assert_eq!(dst, ms_root.join("archive"));
        assert!(is_dir);

        let (src, dst, is_dir) =
            restore_paths_for_entry(ms_root, config_path, backup_dir, "config").unwrap();
        assert_eq!(src, backup_dir.join("config.toml"));
        assert_eq!(dst, config_path);
        assert!(!is_dir);
    }
}
