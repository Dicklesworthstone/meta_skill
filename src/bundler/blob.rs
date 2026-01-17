//! Content-addressed blob storage for bundles.

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{MsError, Result};

pub struct BlobStore {
    root: PathBuf,
}

impl BlobStore {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let blobs = root.join("blobs");
        fs::create_dir_all(&blobs).map_err(|err| {
            MsError::Config(format!("create blob store {}: {err}", blobs.display()))
        })?;
        Ok(Self { root })
    }

    pub fn write_blob(&self, bytes: &[u8]) -> Result<String> {
        let hash = hash_bytes(bytes);
        let path = self.blob_path(&hash)?;
        if !path.exists() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|err| {
                    MsError::Config(format!("create blob dir {}: {err}", parent.display()))
                })?;
            }
            fs::write(&path, bytes)
                .map_err(|err| MsError::Config(format!("write blob {}: {err}", path.display())))?;
        }
        Ok(hash)
    }

    pub fn read_blob(&self, hash: &str) -> Result<Vec<u8>> {
        let path = self.blob_path(hash)?;
        fs::read(&path)
            .map_err(|err| MsError::Config(format!("read blob {}: {err}", path.display())))
    }

    #[must_use] 
    pub fn has_blob(&self, hash: &str) -> bool {
        self.blob_path(hash).map(|p| p.exists()).unwrap_or(false)
    }

    pub fn verify_blob(&self, hash: &str) -> Result<bool> {
        let data = self.read_blob(hash)?;
        Ok(hash == hash_bytes(&data))
    }

    pub fn hash_path(path: &Path) -> Result<String> {
        if path.is_file() {
            let data = fs::read(path)
                .map_err(|err| MsError::Config(format!("read {}: {err}", path.display())))?;
            return Ok(hash_bytes(&data));
        }

        if !path.is_dir() {
            return Err(MsError::ValidationFailed(format!(
                "path is not file or directory: {}",
                path.display()
            )));
        }

        let mut entries = Vec::new();
        collect_files_for_bundle(path, path, &mut entries)?;
        entries.sort_by(|a, b| a.0.cmp(&b.0));

        let mut hasher = Sha256::new();
        for (rel, abs) in entries {
            let rel_str = rel.to_string_lossy();
            hasher.update(rel_str.as_bytes());
            hasher.update([0u8]);
            let data = fs::read(&abs)
                .map_err(|err| MsError::Config(format!("read {}: {err}", abs.display())))?;
            hasher.update(data);
        }

        let digest = hasher.finalize();
        Ok(format!("sha256:{}", hex::encode(digest)))
    }

    /// Get the path for a blob hash, validating against path traversal attacks.
    ///
    /// # Security
    /// This function validates that the hash doesn't contain path traversal
    /// sequences (../, ..\, /, \) or null bytes that could be used to
    /// read/write files outside the blob store.
    fn blob_path(&self, hash: &str) -> Result<PathBuf> {
        // Validate hash doesn't contain path traversal sequences
        if hash.contains('/') || hash.contains('\\') || hash.contains('\0') {
            return Err(MsError::ValidationFailed(
                "invalid blob hash: contains path separator or null byte".to_string(),
            ));
        }
        if hash.contains("..") {
            return Err(MsError::ValidationFailed(
                "invalid blob hash: contains path traversal sequence".to_string(),
            ));
        }
        if hash.is_empty() {
            return Err(MsError::ValidationFailed(
                "invalid blob hash: empty".to_string(),
            ));
        }
        // Additional validation: hash should match expected format (sha256:hex)
        if !hash.starts_with("sha256:") {
            return Err(MsError::ValidationFailed(format!(
                "invalid blob hash format: expected sha256:hex, got {}",
                hash.chars().take(20).collect::<String>()
            )));
        }
        let hex_part = &hash[7..];
        if hex_part.len() != 64 || !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(MsError::ValidationFailed("invalid blob hash: malformed hex component".to_string()));
        }

        // Check for legacy flat structure first
        let legacy_path = self.root.join("blobs").join(hash);
        if legacy_path.exists() {
            return Ok(legacy_path);
        }

        // Use nested structure for new blobs: blobs/sha256/hex
        Ok(self.root.join("blobs").join("sha256").join(hex_part))
    }
}

pub(crate) fn collect_files_for_bundle(
    root: &Path,
    current: &Path,
    out: &mut Vec<(PathBuf, PathBuf)>,
) -> Result<()> {
    let mut queue = VecDeque::new();
    queue.push_back(current.to_path_buf());

    while let Some(dir) = queue.pop_front() {
        for entry in fs::read_dir(&dir)
            .map_err(|err| MsError::Config(format!("read dir {}: {err}", dir.display())))?
        {
            let entry = entry.map_err(|err| {
                MsError::Config(format!("read dir entry {}: {err}", dir.display()))
            })?;
            let path = entry.path();
            let file_name = entry.file_name();
            let name_str = file_name.to_string_lossy();

            // Skip hidden VCS directories/files
            if name_str == ".git" || name_str == ".svn" || name_str == ".hg" || name_str == ".DS_Store" {
                continue;
            }

            let file_type = entry.file_type().map_err(|err| {
                MsError::Config(format!("get file type {}: {err}", path.display()))
            })?;

            if file_type.is_symlink() {
                continue;
            }

            if file_type.is_dir() {
                queue.push_back(path);
            } else if file_type.is_file() {
                let rel = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
                out.push((rel, path));
            }
        }
    }

    Ok(())
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    format!("sha256:{}", hex::encode(digest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn hash_bytes_is_deterministic() {
        let first = hash_bytes(b"hello");
        let second = hash_bytes(b"hello");
        assert_eq!(first, second);
    }

    #[test]
    fn write_and_verify_blob() {
        let dir = tempdir().unwrap();
        let store = BlobStore::open(dir.path()).unwrap();
        let hash = store.write_blob(b"bundle-data").unwrap();
        assert!(store.has_blob(&hash));
        assert!(store.verify_blob(&hash).unwrap());
    }

    #[test]
    fn blob_path_rejects_path_traversal() {
        let dir = tempdir().unwrap();
        let store = BlobStore::open(dir.path()).unwrap();

        // Path traversal with ..
        assert!(store.blob_path("../../../etc/passwd").is_err());
        assert!(store.blob_path("sha256:../abc").is_err());

        // Path separators
        assert!(store.blob_path("sha256:abc/def").is_err());
        assert!(store.blob_path("sha256:abc\\def").is_err());

        // Null byte injection
        assert!(store.blob_path("sha256:abc\0def").is_err());

        // Empty hash
        assert!(store.blob_path("").is_err());

        // Invalid format (not sha256:)
        assert!(store.blob_path("md5:abcdef").is_err());
        assert!(store.blob_path("plain-hash").is_err());

        // Invalid hex (wrong length or non-hex chars)
        assert!(store.blob_path("sha256:abc").is_err());
        assert!(
            store
                .blob_path(
                    "sha256:gggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg"
                )
                .is_err()
        );
    }

    #[test]
    fn blob_path_uses_nested_structure() {
        let dir = tempdir().unwrap();
        let store = BlobStore::open(dir.path()).unwrap();

        // Valid sha256 hash (64 hex chars)
        let valid_hash = "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        let path = store.blob_path(valid_hash).unwrap();
        let expected = dir
            .path()
            .join("blobs")
            .join("sha256")
            .join("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
        assert_eq!(path, expected);
    }

    #[test]
    fn blob_path_prefers_legacy_when_present() {
        let dir = tempdir().unwrap();
        let store = BlobStore::open(dir.path()).unwrap();

        let valid_hash = "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        let legacy_path = dir.path().join("blobs").join(valid_hash);
        fs::write(&legacy_path, b"legacy").unwrap();

        let path = store.blob_path(valid_hash).unwrap();
        assert_eq!(path, legacy_path);
    }
}
