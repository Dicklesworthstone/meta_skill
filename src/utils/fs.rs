//! Filesystem utilities.
//!
//! Helper functions for file operations.

use std::path::Path;

use crate::error::Result;

/// Ensure a directory exists, creating it if necessary.
pub fn ensure_dir(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}

/// Read a file to string, returning None if it doesn't exist.
pub fn read_optional(path: impl AsRef<Path>) -> Result<Option<String>> {
    let path = path.as_ref();
    if path.exists() {
        Ok(Some(std::fs::read_to_string(path)?))
    } else {
        Ok(None)
    }
}
