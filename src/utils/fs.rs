//! Filesystem utilities.
//!
//! Helper functions for file operations.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
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

/// Read the last `count` lines from a file efficiently.
pub fn read_tail(path: impl AsRef<Path>, count: usize) -> Result<Vec<String>> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(Vec::new());
    }

    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    if len == 0 {
        return Ok(Vec::new());
    }

    // Heuristic: Read last 8KB (enough for ~100 typical command lines)
    let chunk_size = 8 * 1024;
    let seek_pos = if len > chunk_size {
        len - chunk_size
    } else {
        0
    };

    file.seek(SeekFrom::Start(seek_pos))?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    let content = String::from_utf8_lossy(&buffer);
    let mut lines: Vec<String> = content.lines().map(String::from).collect();

    // If we sought into the middle of a line, discard the first partial line
    // unless we read the whole file.
    if seek_pos > 0 && !lines.is_empty() {
        lines.remove(0);
    }

    let start = lines.len().saturating_sub(count);
    Ok(lines[start..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // =========================================================================
    // read_tail tests
    // =========================================================================

    #[test]
    fn read_tail_short_file() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("short.txt");
        std::fs::write(&file, "line1\nline2\nline3").unwrap();

        let lines = read_tail(&file, 2).unwrap();
        assert_eq!(lines, vec!["line2", "line3"]);
    }

    #[test]
    fn read_tail_exact() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("exact.txt");
        std::fs::write(&file, "line1\nline2").unwrap();

        let lines = read_tail(&file, 2).unwrap();
        assert_eq!(lines, vec!["line1", "line2"]);
    }

    #[test]
    fn read_tail_more_than_exists() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("small.txt");
        std::fs::write(&file, "line1").unwrap();

        let lines = read_tail(&file, 5).unwrap();
        assert_eq!(lines, vec!["line1"]);
    }

    #[test]
    fn read_tail_long_file() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("long.txt");
        // Create 1000 lines
        let content: String = (0..1000).map(|i| format!("line{i}\n")).collect();
        std::fs::write(&file, content).unwrap();

        let lines = read_tail(&file, 5).unwrap();
        assert_eq!(lines.len(), 5);
        assert_eq!(lines[0], "line995");
        assert_eq!(lines[4], "line999");
    }

    // =========================================================================
    // ensure_dir tests
    // =========================================================================

    #[test]
    fn ensure_dir_creates_new_directory() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join("new_dir");

        assert!(!dir.exists());
        ensure_dir(&dir).unwrap();
        assert!(dir.exists());
        assert!(dir.is_dir());
    }

    #[test]
    fn ensure_dir_creates_nested_directories() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join("a").join("b").join("c");

        assert!(!dir.exists());
        ensure_dir(&dir).unwrap();
        assert!(dir.exists());
        assert!(dir.is_dir());
    }

    #[test]
    fn ensure_dir_noop_if_exists() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join("existing");
        std::fs::create_dir(&dir).unwrap();

        // Should not fail if directory exists
        ensure_dir(&dir).unwrap();
        assert!(dir.exists());
    }

    #[test]
    fn ensure_dir_idempotent() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join("idem");

        // Call multiple times
        ensure_dir(&dir).unwrap();
        ensure_dir(&dir).unwrap();
        ensure_dir(&dir).unwrap();
        assert!(dir.exists());
    }

    // =========================================================================
    // read_optional tests
    // =========================================================================

    #[test]
    fn read_optional_existing_file() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("test.txt");
        std::fs::write(&file, "hello world").unwrap();

        let result = read_optional(&file).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "hello world");
    }

    #[test]
    fn read_optional_nonexistent_file() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("nonexistent.txt");

        let result = read_optional(&file).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn read_optional_empty_file() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("empty.txt");
        std::fs::write(&file, "").unwrap();

        let result = read_optional(&file).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "");
    }

    #[test]
    fn read_optional_with_unicode() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("unicode.txt");
        std::fs::write(&file, "日本語🚀").unwrap();

        let result = read_optional(&file).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "日本語🚀");
    }

    #[test]
    fn read_optional_multiline() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("multiline.txt");
        std::fs::write(&file, "line1\nline2\nline3").unwrap();

        let result = read_optional(&file).unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains('\n'));
    }
}
