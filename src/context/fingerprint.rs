//! Context fingerprinting for suggestion cooldowns.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use super::ContextCapture;

/// A fingerprint capturing the current working context.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContextFingerprint {
    /// Absolute path to repo root (or project root if not git).
    pub repo_root: PathBuf,
    /// Current git HEAD commit hash.
    pub git_head: Option<String>,
    /// Hash of current git diff (staged + unstaged).
    pub diff_hash: u64,
    /// Hash of currently open files.
    pub open_files_hash: u64,
    /// Hash of recent command history.
    pub recent_commands_hash: u64,
}

impl ContextFingerprint {
    /// Create a fingerprint from captured context.
    #[must_use] 
    pub fn capture(ctx: &ContextCapture) -> Self {
        Self {
            repo_root: ctx.repo_root.clone(),
            git_head: ctx.git_head.clone(),
            diff_hash: ctx.compute_diff_hash(),
            open_files_hash: ctx.compute_open_files_hash(),
            recent_commands_hash: ctx.compute_commands_hash(),
        }
    }

    /// Compute a single u64 hash for storage.
    #[must_use] 
    pub fn as_u64(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }

    /// Compare two fingerprints for change significance.
    #[must_use] 
    pub fn compare(&self, other: &Self) -> ChangeSignificance {
        if self.repo_root != other.repo_root {
            return ChangeSignificance::Major;
        }
        if self.git_head != other.git_head {
            return ChangeSignificance::Major;
        }

        let mut minor_changes = 0;
        if self.diff_hash != other.diff_hash {
            minor_changes += 1;
        }
        if self.open_files_hash != other.open_files_hash {
            minor_changes += 1;
        }
        if self.recent_commands_hash != other.recent_commands_hash {
            minor_changes += 1;
        }

        match minor_changes {
            0 => ChangeSignificance::None,
            1 => ChangeSignificance::Minor,
            _ => ChangeSignificance::Moderate,
        }
    }
}

/// How significantly has the context changed?
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ChangeSignificance {
    None,
    Minor,
    Moderate,
    Major,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_major_on_repo_change() {
        let a = ContextFingerprint {
            repo_root: PathBuf::from("/a"),
            git_head: Some("abc".to_string()),
            diff_hash: 1,
            open_files_hash: 2,
            recent_commands_hash: 3,
        };
        let b = ContextFingerprint {
            repo_root: PathBuf::from("/b"),
            ..a.clone()
        };
        assert_eq!(a.compare(&b), ChangeSignificance::Major);
    }
}
