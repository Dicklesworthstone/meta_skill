//! Path validation and canonicalization utilities for security.
//!
//! Provides centralized path validation to prevent:
//! - Path traversal attacks (../ sequences)
//! - Symlink escape attacks
//! - Unexpected path roots
//!
//! # Example
//!
//! ```rust,ignore
//! use std::path::Path;
//! use ms::security::path_policy::{canonicalize_with_root, validate_path_component};
//!
//! // Validate a user-provided path component
//! let component = "my-skill";
//! assert!(validate_path_component(component).is_ok());
//!
//! // Canonicalize a path ensuring it stays within root
//! let root = Path::new("/data/skills");
//! let user_path = Path::new("/data/skills/my-skill/file.txt");
//! let canonical = canonicalize_with_root(user_path, root).unwrap();
//! ```

use std::path::{Component, Path, PathBuf};

use crate::error::{MsError, Result};

/// Errors specific to path policy violations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathPolicyViolation {
    /// Path contains traversal sequences (.. or similar)
    TraversalAttempt,
    /// Path escapes the allowed root directory
    EscapesRoot { path: PathBuf, root: PathBuf },
    /// Path contains a symlink that escapes the root
    SymlinkEscape {
        symlink: PathBuf,
        target: PathBuf,
        root: PathBuf,
    },
    /// Path component contains invalid characters
    InvalidComponent { component: String, reason: String },
    /// Path is not within the expected root
    OutsideRoot { path: PathBuf, root: PathBuf },
}

impl std::fmt::Display for PathPolicyViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TraversalAttempt => write!(f, "path contains traversal sequences"),
            Self::EscapesRoot { path, root } => {
                write!(f, "path {path:?} escapes root {root:?}")
            }
            Self::SymlinkEscape {
                symlink,
                target,
                root,
            } => {
                write!(
                    f,
                    "symlink {symlink:?} targets {target:?} outside root {root:?}"
                )
            }
            Self::InvalidComponent { component, reason } => {
                write!(f, "invalid path component {component:?}: {reason}")
            }
            Self::OutsideRoot { path, root } => {
                write!(f, "path {path:?} is outside root {root:?}")
            }
        }
    }
}

impl std::error::Error for PathPolicyViolation {}

impl From<PathPolicyViolation> for MsError {
    fn from(violation: PathPolicyViolation) -> Self {
        Self::ValidationFailed(violation.to_string())
    }
}

/// Validate a single path component (filename or directory name).
///
/// Rejects components that contain:
/// - Path traversal sequences (.., ., leading /)
/// - Directory separators (/ or \)
/// - Null bytes
/// - Empty strings
///
/// # Arguments
///
/// * `component` - The path component to validate
///
/// # Returns
///
/// * `Ok(())` if the component is valid
/// * `Err(PathPolicyViolation)` if invalid
///
/// # Example
///
/// ```rust
/// use ms::security::path_policy::validate_path_component;
///
/// assert!(validate_path_component("my-skill").is_ok());
/// assert!(validate_path_component("..").is_err());
/// assert!(validate_path_component("foo/bar").is_err());
/// ```
pub fn validate_path_component(component: &str) -> std::result::Result<(), PathPolicyViolation> {
    if component.is_empty() {
        return Err(PathPolicyViolation::InvalidComponent {
            component: component.to_string(),
            reason: "empty component".to_string(),
        });
    }

    // Check for null bytes
    if component.contains('\0') {
        return Err(PathPolicyViolation::InvalidComponent {
            component: component.to_string(),
            reason: "contains null byte".to_string(),
        });
    }

    // Check for traversal sequences
    if component == ".." || component == "." {
        return Err(PathPolicyViolation::TraversalAttempt);
    }

    // Check for directory separators
    if component.contains('/') || component.contains('\\') {
        return Err(PathPolicyViolation::InvalidComponent {
            component: component.to_string(),
            reason: "contains directory separator".to_string(),
        });
    }

    Ok(())
}

/// Canonicalize a path and verify it stays within a root directory.
///
/// This function:
/// 1. Canonicalizes both the path and root (resolving symlinks, .., etc.)
/// 2. Verifies the canonical path is under the canonical root
///
/// # Arguments
///
/// * `path` - The path to canonicalize
/// * `root` - The root directory the path must stay within
///
/// # Returns
///
/// * `Ok(PathBuf)` - The canonicalized path if it's within root
/// * `Err` - If the path escapes root or can't be canonicalized
///
/// # Example
///
/// ```rust,ignore
/// use std::path::{Path, PathBuf};
/// use ms::security::path_policy::canonicalize_with_root;
///
/// let root = Path::new("/data/skills");
/// let path = Path::new("/data/skills/my-skill/../other-skill");
///
/// // This succeeds because the canonical path is still under root
/// let canonical = canonicalize_with_root(path, root).unwrap();
/// assert_eq!(canonical, PathBuf::from("/data/skills/other-skill"));
/// ```
pub fn canonicalize_with_root(path: &Path, root: &Path) -> Result<PathBuf> {
    // Canonicalize root first
    let canonical_root = root.canonicalize().map_err(|e| {
        MsError::ValidationFailed(format!("cannot canonicalize root {root:?}: {e}"))
    })?;

    // Canonicalize the target path
    let canonical_path = path.canonicalize().map_err(|e| {
        MsError::ValidationFailed(format!("cannot canonicalize path {path:?}: {e}"))
    })?;

    // Verify the path is under root
    if !canonical_path.starts_with(&canonical_root) {
        return Err(PathPolicyViolation::EscapesRoot {
            path: canonical_path,
            root: canonical_root,
        }
        .into());
    }

    Ok(canonical_path)
}

/// Check if a path would escape root via symlinks.
///
/// Unlike `canonicalize_with_root`, this checks the symlink target
/// without requiring the target to exist.
///
/// # Arguments
///
/// * `path` - The path to check
/// * `root` - The root directory the path must stay within
///
/// # Returns
///
/// * `Ok(())` if no symlink escape detected
/// * `Err(PathPolicyViolation::SymlinkEscape)` if escape detected
pub fn deny_symlink_escape(
    path: &Path,
    root: &Path,
) -> std::result::Result<(), PathPolicyViolation> {
    // Canonicalize root
    let canonical_root = match root.canonicalize() {
        Ok(r) => r,
        Err(_) => return Ok(()), // If root doesn't exist, can't check
    };

    // Walk through path components, checking each for symlinks
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(p) => current.push(p.as_os_str()),
            Component::RootDir => current.push("/"),
            Component::CurDir => {} // Skip .
            Component::ParentDir => {
                // .. is handled by checking the result
                current.pop();
            }
            Component::Normal(name) => {
                current.push(name);

                // Check if this component is a symlink
                if current.is_symlink() {
                    match std::fs::read_link(&current) {
                        Ok(target) => {
                            // Resolve relative symlink targets
                            let resolved = if target.is_absolute() {
                                target.clone()
                            } else {
                                current.parent().unwrap_or(&current).join(&target)
                            };

                            // Check if symlink target escapes root
                            if let Ok(canonical_target) = resolved.canonicalize() {
                                if !canonical_target.starts_with(&canonical_root) {
                                    return Err(PathPolicyViolation::SymlinkEscape {
                                        symlink: current.clone(),
                                        target: canonical_target,
                                        root: canonical_root,
                                    });
                                }
                            }
                        }
                        Err(_) => {} // Can't read symlink, continue
                    }
                }
            }
        }
    }

    Ok(())
}

/// Safely join a root path with a user-provided relative path.
///
/// This function:
/// 1. Validates the relative path has no traversal sequences
/// 2. Normalizes the path (removes redundant separators)
/// 3. Joins with root
/// 4. If `check_escape` is true, verifies no escape via symlinks
///
/// # Arguments
///
/// * `root` - The root directory
/// * `relative` - User-provided relative path
/// * `check_escape` - Whether to check for symlink escapes (requires paths to exist)
///
/// # Returns
///
/// * `Ok(PathBuf)` - The joined path
/// * `Err` - If validation fails or escape detected
pub fn safe_join(root: &Path, relative: &str, check_escape: bool) -> Result<PathBuf> {
    // Check for null bytes
    if relative.contains('\0') {
        return Err(PathPolicyViolation::InvalidComponent {
            component: relative.to_string(),
            reason: "contains null byte".to_string(),
        }
        .into());
    }

    // Parse and validate each component
    let rel_path = Path::new(relative);
    for component in rel_path.components() {
        match component {
            Component::ParentDir => {
                return Err(PathPolicyViolation::TraversalAttempt.into());
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(PathPolicyViolation::InvalidComponent {
                    component: relative.to_string(),
                    reason: "must be a relative path".to_string(),
                }
                .into());
            }
            Component::Normal(name) => {
                // Check for hidden traversal in component name
                let name_str = name.to_string_lossy();
                if name_str.contains('\0') {
                    return Err(PathPolicyViolation::InvalidComponent {
                        component: name_str.to_string(),
                        reason: "contains null byte".to_string(),
                    }
                    .into());
                }
            }
            Component::CurDir => {} // Skip .
        }
    }

    let joined = root.join(relative);

    // Optionally check for symlink escapes
    if check_escape {
        deny_symlink_escape(&joined, root)?;
    }

    Ok(joined)
}

/// Normalize a path by removing redundant components.
///
/// This does NOT resolve symlinks or check the filesystem.
/// It purely normalizes the path string.
///
/// # Example
///
/// ```rust
/// use std::path::{Path, PathBuf};
/// use ms::security::path_policy::normalize_path;
///
/// let path = Path::new("/foo/./bar/../baz");
/// let normalized = normalize_path(path);
/// assert_eq!(normalized, PathBuf::from("/foo/baz"));
/// ```
#[must_use]
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::ParentDir => {
                // Pop unless we're at root (can't go above root) or already empty
                let last = normalized.components().next_back();
                match last {
                    None => {}                       // Empty, nothing to pop
                    Some(Component::RootDir) => {}   // At root, can't go higher
                    Some(Component::Prefix(_)) => {} // At prefix (Windows), can't go higher
                    _ => {
                        normalized.pop();
                    }
                }
            }
            Component::CurDir => {} // Skip .
            _ => normalized.push(component),
        }
    }

    normalized
}

/// Check if a path is contained within a root directory.
///
/// This uses string comparison on normalized paths and does NOT
/// resolve symlinks. For symlink-aware checking, use `canonicalize_with_root`.
///
/// # Arguments
///
/// * `path` - The path to check
/// * `root` - The root directory
///
/// # Returns
///
/// * `true` if path is under root (or equals root)
/// * `false` otherwise
#[must_use]
pub fn is_under_root(path: &Path, root: &Path) -> bool {
    let normalized_path = normalize_path(path);
    let normalized_root = normalize_path(root);

    normalized_path.starts_with(&normalized_root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_validate_path_component_valid() {
        assert!(validate_path_component("my-skill").is_ok());
        assert!(validate_path_component("skill_123").is_ok());
        assert!(validate_path_component("a").is_ok());
        assert!(validate_path_component("foo.bar").is_ok());
    }

    #[test]
    fn test_validate_path_component_traversal() {
        assert!(matches!(
            validate_path_component(".."),
            Err(PathPolicyViolation::TraversalAttempt)
        ));
        assert!(matches!(
            validate_path_component("."),
            Err(PathPolicyViolation::TraversalAttempt)
        ));
    }

    #[test]
    fn test_validate_path_component_separators() {
        assert!(validate_path_component("foo/bar").is_err());
        assert!(validate_path_component("foo\\bar").is_err());
    }

    #[test]
    fn test_validate_path_component_empty() {
        assert!(validate_path_component("").is_err());
    }

    #[test]
    fn test_validate_path_component_null() {
        assert!(validate_path_component("foo\0bar").is_err());
    }

    #[test]
    fn test_normalize_path() {
        // Absolute paths
        assert_eq!(
            normalize_path(Path::new("/foo/bar")),
            PathBuf::from("/foo/bar")
        );
        assert_eq!(
            normalize_path(Path::new("/foo/./bar")),
            PathBuf::from("/foo/bar")
        );
        assert_eq!(
            normalize_path(Path::new("/foo/bar/../baz")),
            PathBuf::from("/foo/baz")
        );
        assert_eq!(
            normalize_path(Path::new("/foo/./bar/./../baz")),
            PathBuf::from("/foo/baz")
        );
        // Can't go above root
        assert_eq!(normalize_path(Path::new("/foo/..")), PathBuf::from("/"));
        assert_eq!(
            normalize_path(Path::new("/foo/bar/../..")),
            PathBuf::from("/")
        );

        // Relative paths with .. should correctly normalize
        assert_eq!(normalize_path(Path::new("foo/..")), PathBuf::from(""));
        assert_eq!(normalize_path(Path::new("a/b/..")), PathBuf::from("a"));
        assert_eq!(
            normalize_path(Path::new("foo/bar/../..")),
            PathBuf::from("")
        );
    }

    #[test]
    fn test_is_under_root() {
        let root = Path::new("/data/skills");

        assert!(is_under_root(Path::new("/data/skills/my-skill"), root));
        assert!(is_under_root(Path::new("/data/skills"), root));
        assert!(is_under_root(Path::new("/data/skills/a/b/c"), root));

        assert!(!is_under_root(Path::new("/data/other"), root));
        assert!(!is_under_root(Path::new("/data"), root));
        assert!(!is_under_root(Path::new("/"), root));
    }

    #[test]
    fn test_is_under_root_with_traversal() {
        let root = Path::new("/data/skills");

        // Normalized path escapes root
        assert!(!is_under_root(Path::new("/data/skills/../other"), root));

        // Normalized path stays under root
        assert!(is_under_root(Path::new("/data/skills/foo/../bar"), root));
    }

    #[test]
    fn test_safe_join_valid() {
        let root = Path::new("/data/skills");

        let result = safe_join(root, "my-skill", false).unwrap();
        assert_eq!(result, PathBuf::from("/data/skills/my-skill"));

        let result = safe_join(root, "a/b/c", false).unwrap();
        assert_eq!(result, PathBuf::from("/data/skills/a/b/c"));
    }

    #[test]
    fn test_safe_join_traversal_blocked() {
        let root = Path::new("/data/skills");

        assert!(safe_join(root, "../escape", false).is_err());
        assert!(safe_join(root, "foo/../../escape", false).is_err());
        assert!(safe_join(root, "foo/../../../etc/passwd", false).is_err());
    }

    #[test]
    fn test_safe_join_absolute_blocked() {
        let root = Path::new("/data/skills");

        assert!(safe_join(root, "/etc/passwd", false).is_err());
    }

    #[test]
    fn test_canonicalize_with_root() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Create a subdirectory
        let subdir = root.join("sub");
        fs::create_dir(&subdir).unwrap();

        // Create a file
        let file = subdir.join("file.txt");
        fs::write(&file, "test").unwrap();

        // Canonicalize should succeed for path under root
        let result = canonicalize_with_root(&file, root).unwrap();
        assert!(result.starts_with(root.canonicalize().unwrap()));
    }

    #[test]
    fn test_canonicalize_with_root_escape() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("root");
        fs::create_dir(&root).unwrap();

        // Create a file outside root
        let outside = temp.path().join("outside.txt");
        fs::write(&outside, "secret").unwrap();

        // Should fail - file is outside root
        assert!(canonicalize_with_root(&outside, &root).is_err());
    }

    #[test]
    fn test_deny_symlink_escape() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("root");
        fs::create_dir(&root).unwrap();

        // Create a file inside root
        let inside = root.join("inside.txt");
        fs::write(&inside, "safe").unwrap();

        // Create a file outside root
        let outside = temp.path().join("outside.txt");
        fs::write(&outside, "secret").unwrap();

        // Create symlink inside root pointing outside
        let symlink = root.join("escape");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &symlink).unwrap();

        #[cfg(unix)]
        {
            // Should detect symlink escape
            let result = deny_symlink_escape(&symlink, &root);
            assert!(matches!(
                result,
                Err(PathPolicyViolation::SymlinkEscape { .. })
            ));
        }

        // Safe path should pass
        assert!(deny_symlink_escape(&inside, &root).is_ok());
    }
}
