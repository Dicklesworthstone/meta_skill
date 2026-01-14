//! Bundle packaging

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::bundler::blob::BlobStore;
use crate::bundler::manifest::{BundleManifest, SignatureVerifier};
use crate::error::{MsError, Result};

/// A skill bundle definition with a source root.
#[derive(Debug, Clone)]
pub struct Bundle {
    pub manifest: BundleManifest,
    pub root: PathBuf,
}

impl Bundle {
    pub fn new(manifest: BundleManifest, root: impl AsRef<Path>) -> Self {
        Self {
            manifest,
            root: root.as_ref().to_path_buf(),
        }
    }

    /// Package the bundle for distribution.
    pub fn package(&self) -> Result<BundlePackage> {
        BundlePackage::build(self.manifest.clone(), &self.root)
    }
}

/// Packaged bundle with blobs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundlePackage {
    pub manifest: BundleManifest,
    pub blobs: Vec<BundleBlob>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleBlob {
    pub hash: String,
    pub bytes: Vec<u8>,
}

impl BundlePackage {
    pub fn build(mut manifest: BundleManifest, root: &Path) -> Result<Self> {
        let mut blobs = Vec::new();
        for skill in manifest.skills.iter_mut() {
            let skill_path = root.join(&skill.path);
            let bytes = build_blob_bytes(&skill_path)?;
            let hash = hash_bytes(&bytes);

            if let Some(existing) = skill.hash.as_ref() {
                if existing != &hash {
                    return Err(MsError::ValidationFailed(format!(
                        "skill hash mismatch for {}",
                        skill.name
                    )));
                }
            } else {
                skill.hash = Some(hash.clone());
            }

            blobs.push(BundleBlob { hash, bytes });
        }

        let checksum = bundle_checksum(&manifest, &blobs)?;
        manifest.checksum = Some(checksum);

        Ok(Self { manifest, blobs })
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let manifest_toml = self.manifest.to_toml_string()?;
        let mut blobs = self.blobs.clone();
        blobs.sort_by(|a, b| a.hash.cmp(&b.hash));

        let mut out = Vec::new();
        out.extend_from_slice(b"MSBUNDLE1");
        out.push(0);
        write_u64(&mut out, manifest_toml.len() as u64);
        out.extend_from_slice(manifest_toml.as_bytes());
        write_u64(&mut out, blobs.len() as u64);
        for blob in blobs {
            write_u64(&mut out, blob.hash.len() as u64);
            out.extend_from_slice(blob.hash.as_bytes());
            write_u64(&mut out, blob.bytes.len() as u64);
            out.extend_from_slice(&blob.bytes);
        }
        Ok(out)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut cursor = 0;
        let header = b"MSBUNDLE1\0";
        if bytes.len() < header.len() || &bytes[..header.len()] != header {
            return Err(MsError::ValidationFailed(
                "invalid bundle header".to_string(),
            ));
        }
        cursor += header.len();

        let manifest_len = read_u64(bytes, &mut cursor)? as usize;
        let manifest_bytes = read_slice(bytes, &mut cursor, manifest_len)?;
        let manifest_str = std::str::from_utf8(manifest_bytes).map_err(|_| {
            MsError::ValidationFailed("manifest is not valid UTF-8".to_string())
        })?;
        let manifest = BundleManifest::from_toml_str(manifest_str)?;

        let blob_count = read_u64(bytes, &mut cursor)? as usize;
        let mut blobs = Vec::with_capacity(blob_count);
        for _ in 0..blob_count {
            let hash_len = read_u64(bytes, &mut cursor)? as usize;
            let hash_bytes = read_slice(bytes, &mut cursor, hash_len)?;
            let hash = std::str::from_utf8(hash_bytes)
                .map_err(|_| MsError::ValidationFailed("invalid blob hash".to_string()))?
                .to_string();
            let blob_len = read_u64(bytes, &mut cursor)? as usize;
            let blob_bytes = read_slice(bytes, &mut cursor, blob_len)?.to_vec();
            blobs.push(BundleBlob { hash, bytes: blob_bytes });
        }

        Ok(Self { manifest, blobs })
    }

    pub fn verify(&self) -> Result<()> {
        self.manifest.validate()?;
        let blob_hashes = self
            .blobs
            .iter()
            .map(|blob| blob.hash.as_str())
            .collect::<HashSet<_>>();
        for blob in &self.blobs {
            let hash = hash_bytes(&blob.bytes);
            if hash != blob.hash {
                return Err(MsError::ValidationFailed(format!(
                    "blob hash mismatch: {}",
                    blob.hash
                )));
            }
        }
        for skill in &self.manifest.skills {
            if let Some(hash) = skill.hash.as_ref() {
                if !blob_hashes.contains(hash.as_str()) {
                    return Err(MsError::ValidationFailed(format!(
                        "missing blob for skill {}",
                        skill.name
                    )));
                }
            }
        }

        if let Some(expected) = self.manifest.checksum.as_ref() {
            let actual = bundle_checksum(&self.manifest, &self.blobs)?;
            if expected != &actual {
                return Err(MsError::ValidationFailed(
                    "bundle checksum mismatch".to_string(),
                ));
            }
        }

        Ok(())
    }

    pub fn verify_signatures(&self, verifier: &impl SignatureVerifier) -> Result<()> {
        if self.manifest.signatures.is_empty() {
            return Ok(());
        }
        let payload = self.to_bytes()?;
        self.manifest.verify_signatures(&payload, verifier)
    }

    pub fn write_missing_blobs(&self, store: &BlobStore) -> Result<usize> {
        let mut written = 0;
        for blob in &self.blobs {
            if !store.has_blob(&blob.hash) {
                store.write_blob(&blob.bytes)?;
                written += 1;
            }
        }
        Ok(written)
    }
}

pub fn missing_blobs(manifest: &BundleManifest, store: &BlobStore) -> Vec<String> {
    manifest
        .skills
        .iter()
        .filter_map(|skill| skill.hash.as_ref())
        .filter(|hash| !store.has_blob(hash))
        .cloned()
        .collect()
}

fn build_blob_bytes(path: &Path) -> Result<Vec<u8>> {
    if path.is_file() {
        return std::fs::read(path).map_err(|err| {
            MsError::Config(format!("read {}: {err}", path.display()))
        });
    }

    if !path.is_dir() {
        return Err(MsError::ValidationFailed(format!(
            "bundle path missing: {}",
            path.display()
        )));
    }

    let mut entries = Vec::new();
    super::blob::collect_files_for_bundle(path, path, &mut entries)?;
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out = Vec::new();
    for (rel, abs) in entries {
        let rel_str = rel.to_string_lossy();
        write_u64(&mut out, rel_str.len() as u64);
        out.extend_from_slice(rel_str.as_bytes());
        let data = std::fs::read(&abs).map_err(|err| {
            MsError::Config(format!("read {}: {err}", abs.display()))
        })?;
        write_u64(&mut out, data.len() as u64);
        out.extend_from_slice(&data);
    }
    Ok(out)
}

fn bundle_checksum(manifest: &BundleManifest, blobs: &[BundleBlob]) -> Result<String> {
    let mut manifest = manifest.clone();
    manifest.checksum = None;
    let toml = manifest.to_toml_string()?;

    let mut hasher = Sha256::new();
    hasher.update(toml.as_bytes());

    let mut blob_hashes = blobs.iter().map(|b| b.hash.as_str()).collect::<Vec<_>>();
    blob_hashes.sort();
    for hash in blob_hashes {
        hasher.update(hash.as_bytes());
    }

    let digest = hasher.finalize();
    Ok(format!("sha256:{}", hex::encode(digest)))
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    format!("sha256:{}", hex::encode(digest))
}

fn write_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn read_u64(input: &[u8], cursor: &mut usize) -> Result<u64> {
    let end = cursor.checked_add(8).ok_or_else(|| {
        MsError::ValidationFailed("bundle parse overflow".to_string())
    })?;
    if end > input.len() {
        return Err(MsError::ValidationFailed(
            "bundle parse truncated".to_string(),
        ));
    }
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&input[*cursor..end]);
    *cursor = end;
    Ok(u64::from_be_bytes(buf))
}

fn read_slice<'a>(input: &'a [u8], cursor: &mut usize, len: usize) -> Result<&'a [u8]> {
    let end = cursor.checked_add(len).ok_or_else(|| {
        MsError::ValidationFailed("bundle parse overflow".to_string())
    })?;
    if end > input.len() {
        return Err(MsError::ValidationFailed(
            "bundle parse truncated".to_string(),
        ));
    }
    let slice = &input[*cursor..end];
    *cursor = end;
    Ok(slice)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundler::manifest::{BundleDependency, BundleInfo, BundledSkill};
    use crate::bundler::BlobStore;
    use tempfile::tempdir;

    #[test]
    fn package_bytes_are_deterministic() {
        let dir = tempdir().unwrap();
        let skill_dir = dir.path().join("skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "content").unwrap();

        let manifest = BundleManifest {
            bundle: BundleInfo {
                id: "bundle".to_string(),
                name: "Bundle".to_string(),
                version: "1.0.0".to_string(),
                description: None,
                authors: vec![],
                license: None,
                repository: None,
                keywords: vec![],
                ms_version: None,
            },
            skills: vec![BundledSkill {
                name: "skill".to_string(),
                path: PathBuf::from("skill"),
                version: Some("1.0.0".to_string()),
                hash: None,
                optional: false,
            }],
            dependencies: vec![BundleDependency {
                id: "dep".to_string(),
                version: "^1.0".to_string(),
                optional: true,
            }],
            checksum: None,
            signatures: vec![],
        };

        let bundle = Bundle::new(manifest, dir.path());
        let package = bundle.package().unwrap();
        let bytes1 = package.to_bytes().unwrap();
        let bytes2 = package.to_bytes().unwrap();
        assert_eq!(bytes1, bytes2);
    }

    #[test]
    fn write_missing_blobs_only_writes_new() {
        let dir = tempdir().unwrap();
        let skill_dir = dir.path().join("skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "content").unwrap();

        let manifest = BundleManifest {
            bundle: BundleInfo {
                id: "bundle".to_string(),
                name: "Bundle".to_string(),
                version: "1.0.0".to_string(),
                description: None,
                authors: vec![],
                license: None,
                repository: None,
                keywords: vec![],
                ms_version: None,
            },
            skills: vec![BundledSkill {
                name: "skill".to_string(),
                path: PathBuf::from("skill"),
                version: Some("1.0.0".to_string()),
                hash: None,
                optional: false,
            }],
            dependencies: vec![BundleDependency {
                id: "dep".to_string(),
                version: "^1.0".to_string(),
                optional: true,
            }],
            checksum: None,
            signatures: vec![],
        };

        let bundle = Bundle::new(manifest, dir.path());
        let package = bundle.package().unwrap();
        let store = BlobStore::open(dir.path().join("store")).unwrap();
        let count = package.write_missing_blobs(&store).unwrap();
        assert_eq!(count, 1);
        let count = package.write_missing_blobs(&store).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn roundtrip_bundle_bytes() {
        let dir = tempdir().unwrap();
        let skill_dir = dir.path().join("skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "content").unwrap();

        let manifest = BundleManifest {
            bundle: BundleInfo {
                id: "bundle".to_string(),
                name: "Bundle".to_string(),
                version: "1.0.0".to_string(),
                description: None,
                authors: vec![],
                license: None,
                repository: None,
                keywords: vec![],
                ms_version: None,
            },
            skills: vec![BundledSkill {
                name: "skill".to_string(),
                path: PathBuf::from("skill"),
                version: Some("1.0.0".to_string()),
                hash: None,
                optional: false,
            }],
            dependencies: vec![],
            checksum: None,
            signatures: vec![],
        };

        let bundle = Bundle::new(manifest, dir.path());
        let package = bundle.package().unwrap();
        let bytes = package.to_bytes().unwrap();
        let parsed = BundlePackage::from_bytes(&bytes).unwrap();
        parsed.verify().unwrap();
    }
}
