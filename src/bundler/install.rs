//! Bundle installation

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::path::{Component, Path, PathBuf};

use crate::bundler::blob::BlobStore;
use crate::bundler::manifest::SignatureVerifier;
use crate::bundler::package::BundlePackage;
use crate::error::{MsError, Result};

#[derive(Debug, Clone, serde::Serialize)]
pub struct InstallReport {
    pub bundle_id: String,
    pub installed: Vec<String>,
    pub skipped: Vec<String>,
    pub blobs_written: usize,
    pub signature_verified: bool,
}

/// Options for bundle installation.
pub struct InstallOptions<
    'a,
    V: SignatureVerifier = crate::bundler::manifest::NoopSignatureVerifier,
> {
    /// Skip signature verification entirely. When true, both unsigned bundles
    /// and signed bundles (without verifying signatures) are allowed.
    /// Default: false (signatures required and verified).
    pub allow_unsigned: bool,
    /// Signature verifier for signed bundles. Only used when `allow_unsigned` is false.
    pub verifier: Option<&'a V>,
}

impl<V: SignatureVerifier> Default for InstallOptions<'_, V> {
    fn default() -> Self {
        Self {
            allow_unsigned: false,
            verifier: None,
        }
    }
}

impl<'a, V: SignatureVerifier> InstallOptions<'a, V> {
    /// Create options that skip signature verification (for development/testing).
    /// Both unsigned bundles and signed bundles will be accepted without verification.
    #[must_use]
    pub const fn allow_unsigned() -> Self {
        Self {
            allow_unsigned: true,
            verifier: None,
        }
    }

    /// Create options with a signature verifier.
    pub const fn with_verifier(verifier: &'a V) -> Self {
        Self {
            allow_unsigned: false,
            verifier: Some(verifier),
        }
    }
}

/// Install a bundle into the git archive root with signature enforcement.
///
/// By default, bundles must be signed and signatures must be verified.
/// Use `InstallOptions::allow_unsigned()` for development/testing scenarios
/// where signature verification should be skipped (works for both signed
/// and unsigned bundles).
pub fn install_with_options<V: SignatureVerifier>(
    package: &BundlePackage,
    archive_root: &Path,
    only_skills: &[String],
    options: &InstallOptions<'_, V>,
) -> Result<InstallReport> {
    package.verify()?;

    // Signature verification (enforced by default)
    let signature_verified = if package.manifest.signatures.is_empty() {
        if !options.allow_unsigned {
            return Err(MsError::ValidationFailed(
                "bundle is unsigned; use --no-verify to install unsigned bundles".to_string(),
            ));
        }
        false
    } else if options.allow_unsigned {
        // allow_unsigned also skips signature verification for signed bundles
        // (used when --no-verify flag is specified at CLI level)
        false
    } else {
        let verifier = options.verifier.ok_or_else(|| {
            MsError::ValidationFailed(
                "bundle is signed but no signature verifier configured".to_string(),
            )
        })?;
        package.verify_signatures(verifier)?;
        true
    };

    let store = BlobStore::open(archive_root.join("bundles"))?;
    let blobs_written = package.write_missing_blobs(&store)?;

    let mut installed = Vec::new();
    let mut skipped = Vec::new();

    // Optimization: Pre-map blobs for O(1) lookup
    let blob_map: HashMap<&String, &crate::bundler::package::BundleBlob> =
        package.blobs.iter().map(|b| (&b.hash, b)).collect();

    // Rollback tracking
    let mut installed_paths = Vec::new();

    for skill in &package.manifest.skills {
        if !only_skills.is_empty() && !only_skills.contains(&skill.name) {
            skipped.push(skill.name.clone());
            continue;
        }
        let hash = skill.hash.as_ref().ok_or_else(|| {
            MsError::ValidationFailed(format!("missing blob hash for {}", skill.name))
        })?;

        let blob = blob_map.get(hash).ok_or_else(|| {
            MsError::ValidationFailed(format!("bundle missing blob {} for {}", hash, skill.name))
        })?;

        let target = resolve_target_path(archive_root, &skill.path, &skill.name)?;

        // Atomic-ish check: if directory exists, we fail.
        if target.exists() {
            // Rollback any previously installed skills in this transaction
            rollback_install(&installed_paths);
            return Err(MsError::ValidationFailed(format!(
                "skill already exists at {}",
                target.display()
            )));
        }

        // Try install
        if let Err(e) = perform_install(&target, &blob.bytes) {
            // perform_install is atomic; if it fails, target was not created by us.
            // We only rollback previous successes.
            rollback_install(&installed_paths);
            return Err(e);
        }

        installed_paths.push(target);
        installed.push(skill.name.clone());
    }

    Ok(InstallReport {
        bundle_id: package.manifest.bundle.id.clone(),
        installed,
        skipped,
        blobs_written,
        signature_verified,
    })
}

fn perform_install(target: &Path, bytes: &[u8]) -> Result<()> {
    // Ensure parent directory exists
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)
        .map_err(|err| MsError::Config(format!("create parent {}: {err}", parent.display())))?;

    // Create temporary directory on the same filesystem (sibling to target)
    let temp_name = format!(".tmp_install_{}", uuid::Uuid::new_v4());
    let temp_path = parent.join(&temp_name);

    std::fs::create_dir(&temp_path)
        .map_err(|err| MsError::Config(format!("create temp {}: {err}", temp_path.display())))?;

    // Unpack into temp directory
    if let Err(e) = unpack_blob(&temp_path, bytes) {
        let _ = std::fs::remove_dir_all(&temp_path);
        return Err(e);
    }

    // Atomic rename to final target
    // Note: rename fails if target exists (on most POSIX) or if non-empty (Windows)
    if let Err(e) = std::fs::rename(&temp_path, target) {
        let _ = std::fs::remove_dir_all(&temp_path);

        // Check for "already exists" errors
        if e.kind() == std::io::ErrorKind::AlreadyExists
            || (cfg!(windows) && e.kind() == std::io::ErrorKind::PermissionDenied)
        {
            return Err(MsError::ValidationFailed(format!(
                "skill already exists at {}",
                target.display()
            )));
        }

        return Err(MsError::Config(format!(
            "install finalization failed (rename {}: {}): {}",
            temp_path.display(),
            target.display(),
            e
        )));
    }

    Ok(())
}

fn rollback_install(paths: &[PathBuf]) {
    for path in paths {
        if path.exists() {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

/// Install a bundle into the git archive root (allows unsigned bundles).
///
/// This is a convenience wrapper for development/testing. For production use,
/// prefer `install_with_options` with proper signature verification.
pub fn install(
    package: &BundlePackage,
    archive_root: &Path,
    only_skills: &[String],
) -> Result<InstallReport> {
    install_with_options(
        package,
        archive_root,
        only_skills,
        &InstallOptions::<crate::bundler::manifest::NoopSignatureVerifier>::allow_unsigned(),
    )
}

fn resolve_target_path(root: &Path, path: &Path, fallback_id: &str) -> Result<PathBuf> {
    if !path.as_os_str().is_empty() {
        ensure_relative(path)?;
        return Ok(root.join(path));
    }
    // Validate fallback_id to prevent path traversal
    ensure_safe_id(fallback_id)?;
    Ok(root.join("skills").join("by-id").join(fallback_id))
}

fn ensure_safe_id(id: &str) -> Result<()> {
    if id.is_empty() {
        return Err(MsError::ValidationFailed(
            "skill id must not be empty".to_string(),
        ));
    }
    if id == "." || id == ".." {
        return Err(MsError::ValidationFailed(format!(
            "skill id cannot be '.' or '..': {id}"
        )));
    }
    // Only allow alphanumeric, hyphen, underscore, and dot
    // This prevents path separators (/, \), control characters, and other weirdness
    if !id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(MsError::ValidationFailed(format!(
            "skill id contains invalid characters (allowed: a-z, A-Z, 0-9, -, _, .): {id}"
        )));
    }

    // Block Windows reserved names (CON, PRN, AUX, NUL, COM1-9, LPT1-9)
    // to ensure portability and safety on shared filesystems.
    let upper = id.to_ascii_uppercase();
    let stem = upper.split('.').next().unwrap_or("");
    match stem {
        "CON" | "PRN" | "AUX" | "NUL" => {
            return Err(MsError::ValidationFailed(format!(
                "skill id '{id}' is a reserved filename on Windows"
            )));
        }
        _ => {
            if (stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.len() == 4
                && stem
                    .chars()
                    .nth(3)
                    .is_some_and(|c| c.is_ascii_digit() && c != '0')
            {
                return Err(MsError::ValidationFailed(format!(
                    "skill id '{id}' is a reserved filename on Windows"
                )));
            }
        }
    }

    Ok(())
}

fn ensure_relative(path: &Path) -> Result<()> {
    if path.is_absolute() {
        return Err(MsError::ValidationFailed(format!(
            "bundle path must be relative: {}",
            path.display()
        )));
    }
    // Block paths that are just "." or empty to prevent writing to the target directory root
    if path.as_os_str().is_empty() || path.as_os_str() == "." {
        return Err(MsError::ValidationFailed(format!(
            "bundle path cannot be empty or '.': {}",
            path.display()
        )));
    }
    for comp in path.components() {
        match comp {
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(MsError::ValidationFailed(format!(
                    "bundle path contains invalid component: {}",
                    path.display()
                )));
            }
            _ => {}
        }
    }
    Ok(())
}

fn unpack_blob(target: &Path, bytes: &[u8]) -> Result<()> {
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        let name_len = read_u64(bytes, &mut cursor)?;
        if name_len > bytes.len() as u64 {
            return Err(MsError::ValidationFailed(format!(
                "bundle entry path length {name_len} exceeds blob size",
            )));
        }
        let name_len = name_len as usize;
        if name_len == 0 {
            return Err(MsError::ValidationFailed(
                "bundle entry has empty path".to_string(),
            ));
        }
        let name_bytes = read_slice(bytes, &mut cursor, name_len)?;
        let name = std::str::from_utf8(name_bytes).map_err(|_| {
            MsError::ValidationFailed("bundle entry path is invalid UTF-8".to_string())
        })?;
        let file_len = read_u64(bytes, &mut cursor)?;
        if file_len > bytes.len() as u64 {
            return Err(MsError::ValidationFailed(format!(
                "bundle entry file length {file_len} exceeds blob size",
            )));
        }
        let file_len = file_len as usize;
        let file_bytes = read_slice(bytes, &mut cursor, file_len)?;

        let rel = Path::new(name);
        ensure_relative(rel)?;
        let path = target.join(rel);

        // Ensure parent directories exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| MsError::Config(format!("create {}: {err}", parent.display())))?;
        }

        // Use create_new(true) to prevent overwriting existing files and ensure atomicity
        use std::io::Write;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|err| {
                if err.kind() == std::io::ErrorKind::AlreadyExists {
                    MsError::ValidationFailed(format!("bundle file collision: {}", path.display()))
                } else {
                    MsError::Config(format!("write {}: {err}", path.display()))
                }
            })?;

        file.write_all(file_bytes)
            .map_err(|err| MsError::Config(format!("write content {}: {err}", path.display())))?;
    }
    Ok(())
}

fn read_u64(input: &[u8], cursor: &mut usize) -> Result<u64> {
    let end = cursor
        .checked_add(8)
        .ok_or_else(|| MsError::ValidationFailed("bundle parse overflow".to_string()))?;
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
    let end = cursor
        .checked_add(len)
        .ok_or_else(|| MsError::ValidationFailed("bundle parse overflow".to_string()))?;
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
    use crate::bundler::manifest::{BundleInfo, BundleManifest, BundledSkill};
    use crate::bundler::package::Bundle;
    use tempfile::tempdir;

    #[test]
    fn install_unpacks_skill_files() {
        let dir = tempdir().unwrap();
        let skill_dir = dir.path().join("skills/by-id/demo");
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
                name: "demo".to_string(),
                path: PathBuf::from("skills/by-id/demo"),
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

        let install_root = tempdir().unwrap();
        let report = install(&package, install_root.path(), &[]).unwrap();
        assert_eq!(report.installed, vec!["demo".to_string()]);
        let installed_path = install_root.path().join("skills/by-id/demo/SKILL.md");
        assert!(installed_path.exists());
    }

    #[test]
    fn ensure_safe_id_blocks_path_traversal() {
        // Path traversal with ..
        assert!(ensure_safe_id("../malicious").is_err());
        assert!(ensure_safe_id("foo/../bar").is_err());
        assert!(ensure_safe_id("..").is_err());

        // Forward slashes
        assert!(ensure_safe_id("foo/bar").is_err());
        assert!(ensure_safe_id("/etc/passwd").is_err());

        // Backslashes
        assert!(ensure_safe_id("foo\\bar").is_err());
        assert!(ensure_safe_id("..\\malicious").is_err());

        // Empty
        assert!(ensure_safe_id("").is_err());

        // Dot and DotDot strict checks
        assert!(ensure_safe_id(".").is_err());
        assert!(ensure_safe_id("..").is_err());

        // Invalid characters
        assert!(ensure_safe_id("foo bar").is_err()); // Space
        assert!(ensure_safe_id("foo*bar").is_err()); // Special char
        assert!(ensure_safe_id("foo\nbar").is_err()); // Control char

        // Valid IDs
        assert!(ensure_safe_id("my-skill").is_ok());
        assert!(ensure_safe_id("skill_123").is_ok());
        assert!(ensure_safe_id("skill.v1").is_ok());
    }

    #[test]
    fn resolve_target_path_blocks_malicious_skill_name() {
        let root = Path::new("/archive");

        // Empty path with malicious skill name should be rejected
        let result = resolve_target_path(root, Path::new(""), "../../../tmp/malicious");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("invalid characters")
        );

        // Valid skill name with empty path should work
        let result = resolve_target_path(root, Path::new(""), "valid-skill");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            PathBuf::from("/archive/skills/by-id/valid-skill")
        );
    }

    #[test]
    fn allow_unsigned_skips_verification_for_signed_bundles() {
        use crate::bundler::manifest::BundleSignature;

        let dir = tempdir().unwrap();
        let skill_dir = dir.path().join("skills/by-id/signed-demo");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "signed content").unwrap();

        // Create a manifest with a fake signature
        let manifest = BundleManifest {
            bundle: BundleInfo {
                id: "signed-bundle".to_string(),
                name: "Signed Bundle".to_string(),
                version: "1.0.0".to_string(),
                description: None,
                authors: vec![],
                license: None,
                repository: None,
                keywords: vec![],
                ms_version: None,
            },
            skills: vec![BundledSkill {
                name: "signed-demo".to_string(),
                path: PathBuf::from("skills/by-id/signed-demo"),
                version: Some("1.0.0".to_string()),
                hash: None,
                optional: false,
            }],
            dependencies: vec![],
            checksum: None,
            // Bundle has signatures - would normally require verification
            signatures: vec![BundleSignature {
                key_id: "test-key".to_string(),
                signer: "ed25519".to_string(),
                signature: "fake-signature-for-test".to_string(),
            }],
        };

        let bundle = Bundle::new(manifest, dir.path());
        let package = bundle.package().unwrap();

        // With allow_unsigned(), signed bundles should install without verification
        let install_root = tempdir().unwrap();
        let options =
            InstallOptions::<crate::bundler::manifest::NoopSignatureVerifier>::allow_unsigned();
        let report = install_with_options(&package, install_root.path(), &[], &options).unwrap();

        assert_eq!(report.installed, vec!["signed-demo".to_string()]);
        assert!(!report.signature_verified); // Signature was NOT verified
        let installed_path = install_root
            .path()
            .join("skills/by-id/signed-demo/SKILL.md");
        assert!(installed_path.exists());
    }

    #[test]
    fn signed_bundle_without_verifier_fails_by_default() {
        use crate::bundler::manifest::BundleSignature;

        let dir = tempdir().unwrap();
        let skill_dir = dir.path().join("skills/by-id/signed-demo2");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "signed content").unwrap();

        let manifest = BundleManifest {
            bundle: BundleInfo {
                id: "signed-bundle2".to_string(),
                name: "Signed Bundle 2".to_string(),
                version: "1.0.0".to_string(),
                description: None,
                authors: vec![],
                license: None,
                repository: None,
                keywords: vec![],
                ms_version: None,
            },
            skills: vec![BundledSkill {
                name: "signed-demo2".to_string(),
                path: PathBuf::from("skills/by-id/signed-demo2"),
                version: Some("1.0.0".to_string()),
                hash: None,
                optional: false,
            }],
            dependencies: vec![],
            checksum: None,
            signatures: vec![BundleSignature {
                key_id: "test-key".to_string(),
                signer: "ed25519".to_string(),
                signature: "fake-signature".to_string(),
            }],
        };

        let bundle = Bundle::new(manifest, dir.path());
        let package = bundle.package().unwrap();

        // Default options (allow_unsigned=false, no verifier) should fail for signed bundle
        let install_root = tempdir().unwrap();
        let options = InstallOptions::<crate::bundler::manifest::NoopSignatureVerifier>::default();
        let result = install_with_options(&package, install_root.path(), &[], &options);

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("no signature verifier configured"));
    }
}
