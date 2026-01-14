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
        let path = self.blob_path(&hash);
        if !path.exists() {
            fs::write(&path, bytes).map_err(|err| {
                MsError::Config(format!("write blob {}: {err}", path.display()))
            })?;
        }
        Ok(hash)
    }

    pub fn read_blob(&self, hash: &str) -> Result<Vec<u8>> {
        let path = self.blob_path(hash);
        fs::read(&path).map_err(|err| {
            MsError::Config(format!("read blob {}: {err}", path.display()))
        })
    }

    pub fn has_blob(&self, hash: &str) -> bool {
        self.blob_path(hash).exists()
    }

    pub fn verify_blob(&self, hash: &str) -> Result<bool> {
        let data = self.read_blob(hash)?;
        Ok(hash == hash_bytes(&data))
    }

    pub fn hash_path(path: &Path) -> Result<String> {
        if path.is_file() {
            let data = fs::read(path).map_err(|err| {
                MsError::Config(format!("read {}: {err}", path.display()))
            })?;
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
            hasher.update(&[0u8]);
            let data = fs::read(&abs).map_err(|err| {
                MsError::Config(format!("read {}: {err}", abs.display()))
            })?;
            hasher.update(data);
        }

        let digest = hasher.finalize();
        Ok(format!("sha256:{}", hex::encode(digest)))
    }

    fn blob_path(&self, hash: &str) -> PathBuf {
        self.root.join("blobs").join(hash)
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
        for entry in fs::read_dir(&dir).map_err(|err| {
            MsError::Config(format!("read dir {}: {err}", dir.display()))
        })? {
            let entry = entry.map_err(|err| {
                MsError::Config(format!("read dir entry {}: {err}", dir.display()))
            })?;
            let path = entry.path();
            if path.is_dir() {
                queue.push_back(path);
            } else if path.is_file() {
                let rel = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_path_buf();
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
}
