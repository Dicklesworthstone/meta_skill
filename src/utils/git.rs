//! Git utilities

use crate::error::Result;

/// Get current branch name
pub fn current_branch() -> Result<Option<String>> {
    // TODO: Implement
    Ok(None)
}

/// Check if directory is a git repository
pub fn is_repo(path: impl AsRef<std::path::Path>) -> bool {
    path.as_ref().join(".git").exists()
}
