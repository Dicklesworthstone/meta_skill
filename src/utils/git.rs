//! Git utilities

use crate::error::Result;

/// Get current branch name from the current working directory
pub fn current_branch() -> Result<Option<String>> {
    current_branch_in(std::env::current_dir().ok())
}

/// Get current branch name from a specific directory
pub fn current_branch_in(path: Option<impl AsRef<std::path::Path>>) -> Result<Option<String>> {
    let mut cmd = std::process::Command::new("git");
    cmd.args(["rev-parse", "--abbrev-ref", "HEAD"]);

    if let Some(p) = path {
        cmd.current_dir(p);
    }

    let output = match cmd.output() {
        Ok(out) => out,
        Err(_) => return Ok(None), // git not available
    };

    if !output.status.success() {
        return Ok(None); // not in a git repo
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if branch.is_empty() || branch == "HEAD" {
        Ok(None) // detached HEAD state
    } else {
        Ok(Some(branch))
    }
}

/// Check if directory is a git repository
pub fn is_repo(path: impl AsRef<std::path::Path>) -> bool {
    path.as_ref().join(".git").exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // =========================================================================
    // current_branch tests
    // =========================================================================

    #[test]
    fn current_branch_in_git_repo_returns_some() {
        // Running tests from within the meta_skill repo, should have a branch
        let result = current_branch().unwrap();
        // In a git repo, we expect Some(branch_name)
        assert!(result.is_some(), "Should detect branch in git repo");
    }

    #[test]
    fn current_branch_in_non_git_returns_none() {
        let temp = TempDir::new().unwrap();
        let result = current_branch_in(Some(temp.path())).unwrap();
        assert!(result.is_none(), "Non-git directory should return None");
    }

    // =========================================================================
    // is_repo tests
    // =========================================================================

    #[test]
    fn is_repo_with_git_directory() {
        let temp = TempDir::new().unwrap();
        let git_dir = temp.path().join(".git");
        std::fs::create_dir(&git_dir).unwrap();

        assert!(is_repo(temp.path()));
    }

    #[test]
    fn is_repo_without_git_directory() {
        let temp = TempDir::new().unwrap();

        assert!(!is_repo(temp.path()));
    }

    #[test]
    fn is_repo_with_git_file_not_dir() {
        let temp = TempDir::new().unwrap();
        let git_file = temp.path().join(".git");
        // Create a file named .git instead of a directory
        std::fs::write(&git_file, "gitdir: ../other").unwrap();

        // A .git file (worktree) should also count as a repo
        assert!(is_repo(temp.path()));
    }

    #[test]
    fn is_repo_nonexistent_path() {
        let temp = TempDir::new().unwrap();
        let nonexistent = temp.path().join("nonexistent");

        assert!(!is_repo(&nonexistent));
    }
}
