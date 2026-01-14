//! Context capture utilities for suggestion fingerprinting.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{MsError, Result};

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("command failed: {0}")]
    Command(String),
}

/// Raw context data for fingerprint computation.
#[derive(Debug, Clone)]
pub struct ContextCapture {
    pub repo_root: PathBuf,
    pub git_head: Option<String>,
    pub diff_content: Option<String>,
    pub open_files: Vec<PathBuf>,
    pub recent_commands: Vec<String>,
}

impl ContextCapture {
    /// Capture context from the current environment.
    pub fn capture_current(cwd: Option<PathBuf>) -> Result<Self> {
        let repo_root = Self::find_repo_root(cwd.as_deref())?;
        let git_head = Self::get_git_head(&repo_root);
        let diff_content = Self::get_git_diff(&repo_root);
        let open_files = Self::get_open_files(&repo_root);
        let recent_commands = Self::get_recent_commands()?;

        Ok(Self {
            repo_root,
            git_head,
            diff_content,
            open_files,
            recent_commands,
        })
    }

    fn find_repo_root(cwd: Option<&Path>) -> Result<PathBuf> {
        let working = cwd
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
        let output = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .current_dir(&working)
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !root.is_empty() {
                    return Ok(PathBuf::from(root));
                }
            }
        }
        Ok(working)
    }

    fn get_git_head(repo_root: &Path) -> Option<String> {
        let output = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(repo_root)
            .output()
            .ok()?;
        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            None
        }
    }

    fn get_git_diff(repo_root: &Path) -> Option<String> {
        let output = Command::new("git")
            .args(["diff", "HEAD"])
            .current_dir(repo_root)
            .output()
            .ok()?;
        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            None
        }
    }

    fn get_open_files(repo_root: &Path) -> Vec<PathBuf> {
        if let Ok(raw) = std::env::var("MS_OPEN_FILES") {
            let mut files = Vec::new();
            for item in raw.split(',') {
                let trimmed = item.trim();
                if trimmed.is_empty() {
                    continue;
                }
                files.push(PathBuf::from(trimmed));
            }
            if !files.is_empty() {
                return files;
            }
        }

        Self::get_recently_modified_files(repo_root).unwrap_or_default()
    }

    fn get_recently_modified_files(repo_root: &Path) -> Option<Vec<PathBuf>> {
        let output = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(repo_root)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut files = Vec::new();
        for line in stdout.lines() {
            if line.len() < 4 {
                continue;
            }
            let path = line[3..].trim();
            if !path.is_empty() {
                files.push(repo_root.join(path));
            }
        }
        Some(files)
    }

    fn get_recent_commands() -> Result<Vec<String>> {
        let history_path = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ms")
            .join("command_history");
        if !history_path.exists() {
            return Ok(Vec::new());
        }
        let content = std::fs::read_to_string(&history_path)
            .map_err(|err| MsError::Config(format!("read history {}: {err}", history_path.display())))?;
        let commands = content
            .lines()
            .rev()
            .take(20)
            .map(|line| line.to_string())
            .collect();
        Ok(commands)
    }

    pub fn compute_diff_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        if let Some(diff) = &self.diff_content {
            diff.hash(&mut hasher);
        }
        hasher.finish()
    }

    pub fn compute_open_files_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        let mut sorted = self.open_files.clone();
        sorted.sort();
        for path in sorted {
            path.hash(&mut hasher);
        }
        hasher.finish()
    }

    pub fn compute_commands_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        for cmd in &self.recent_commands {
            cmd.hash(&mut hasher);
        }
        hasher.finish()
    }
}
