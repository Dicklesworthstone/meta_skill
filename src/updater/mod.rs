//! Auto-update system for ms.
//!
//! Provides self-update mechanism following xf pattern: check for new versions,
//! download, verify checksums, extract the executable from the release archive,
//! and replace binaries safely.
//!
//! # Safety gates
//!
//! Replacing the running executable is irreversible from the user's point of
//! view, so every candidate passes three independent checks before it is
//! allowed anywhere near `argv[0]` (meta_skill#159):
//!
//! 1. **Extraction** — release assets are `.tar.gz`/`.zip` bundles containing
//!    `ms` plus `README.md`/`LICENSE`. The `ms` member is unpacked; the archive
//!    itself is never installed.
//! 2. **Format gate** — [`verify_executable_format`] rejects anything whose
//!    leading bytes are a compressed-archive magic (gzip, zip, xz, bzip2, zstd)
//!    or that is not an ELF/Mach-O (Unix) / PE (Windows) image, and requires the
//!    executable bit on Unix.
//! 3. **Liveness gate** — [`verify_binary_runs`] actually executes the staged
//!    candidate with `--version` and requires a successful exit.
//!
//! Installation is atomic: the verified candidate is copied to a temporary path
//! *inside the destination directory*, re-verified there, and only then renamed
//! over the live binary. A post-swap liveness check rolls back from the backup
//! if anything went wrong, so a failed update never leaves the user without a
//! working `ms`.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{MsError, Result};

/// Update channel for release filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum UpdateChannel {
    #[default]
    Stable,
    Beta,
    Nightly,
}

impl std::str::FromStr for UpdateChannel {
    type Err = MsError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "stable" => Ok(Self::Stable),
            "beta" => Ok(Self::Beta),
            "nightly" => Ok(Self::Nightly),
            _ => Err(MsError::ValidationFailed(format!(
                "invalid update channel: {s} (expected stable, beta, or nightly)"
            ))),
        }
    }
}

impl std::fmt::Display for UpdateChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stable => write!(f, "stable"),
            Self::Beta => write!(f, "beta"),
            Self::Nightly => write!(f, "nightly"),
        }
    }
}

/// Information about an available release.
#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    pub version: Version,
    pub tag: String,
    pub prerelease: bool,
    pub assets: Vec<ReleaseAsset>,
    pub changelog: String,
    pub published_at: DateTime<Utc>,
    pub html_url: String,
}

/// Asset attached to a release.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseAsset {
    pub id: u64,
    pub name: String,
    pub download_url: String,
    pub size: u64,
}

/// Checker for available updates.
pub struct UpdateChecker {
    current_version: Version,
    channel: UpdateChannel,
    repo: String,
    token: Option<String>,
}

impl UpdateChecker {
    /// Create a new update checker.
    #[must_use]
    pub fn new(current_version: Version, channel: UpdateChannel, repo: String) -> Self {
        Self {
            current_version,
            channel,
            repo,
            token: token_from_env(),
        }
    }

    /// Set the GitHub token for authenticated requests.
    #[must_use]
    pub fn with_token(mut self, token: Option<String>) -> Self {
        self.token = token;
        self
    }

    /// Check if an update is available.
    pub fn check(&self) -> Result<Option<ReleaseInfo>> {
        let client = GitHubClient::new(self.token.clone());
        let (owner, repo) = parse_repo(&self.repo)?;

        let releases = client.list_releases(&owner, &repo)?;

        let latest = releases
            .into_iter()
            .filter(|r| self.matches_channel(r))
            .filter(|r| r.version > self.current_version)
            .max_by(|a, b| a.version.cmp(&b.version));

        Ok(latest)
    }

    /// Get the latest release matching the channel (regardless of current version).
    pub fn get_latest(&self) -> Result<Option<ReleaseInfo>> {
        let client = GitHubClient::new(self.token.clone());
        let (owner, repo) = parse_repo(&self.repo)?;

        let releases = client.list_releases(&owner, &repo)?;

        let latest = releases
            .into_iter()
            .filter(|r| self.matches_channel(r))
            .max_by(|a, b| a.version.cmp(&b.version));

        Ok(latest)
    }

    /// Get a specific release by version (ignores channel filtering).
    pub fn get_version(&self, target: &Version) -> Result<Option<ReleaseInfo>> {
        let client = GitHubClient::new(self.token.clone());
        let (owner, repo) = parse_repo(&self.repo)?;

        let releases = client.list_releases(&owner, &repo)?;
        Ok(releases.into_iter().find(|r| &r.version == target))
    }

    /// Get the current version being checked against.
    #[must_use]
    pub const fn current_version(&self) -> &Version {
        &self.current_version
    }

    /// Get the update channel.
    #[must_use]
    pub const fn channel(&self) -> UpdateChannel {
        self.channel
    }

    fn matches_channel(&self, release: &ReleaseInfo) -> bool {
        match self.channel {
            UpdateChannel::Stable => !release.prerelease,
            UpdateChannel::Beta => {
                release.tag.contains("beta") || release.tag.contains("rc") || !release.prerelease
            }
            UpdateChannel::Nightly => true,
        }
    }
}

/// Downloader for release assets with verification.
pub struct UpdateDownloader {
    temp_dir: PathBuf,
    token: Option<String>,
}

impl UpdateDownloader {
    /// Create a new downloader using the system temp directory.
    pub fn new() -> Result<Self> {
        let temp_dir = std::env::temp_dir().join("ms-update");
        std::fs::create_dir_all(&temp_dir)?;
        Ok(Self {
            temp_dir,
            token: token_from_env(),
        })
    }

    /// Create a downloader with a specific temp directory.
    pub fn with_temp_dir(temp_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&temp_dir)?;
        Ok(Self {
            temp_dir,
            token: token_from_env(),
        })
    }

    /// Set the GitHub token for authenticated downloads.
    #[must_use]
    pub fn with_token(mut self, token: Option<String>) -> Self {
        self.token = token;
        self
    }

    /// Download a release asset, verify its checksum, and extract the `ms`
    /// executable from it.
    ///
    /// The returned path is always an **executable**, never the downloaded
    /// archive: release assets are `.tar.gz`/`.zip` bundles that also carry
    /// `README.md` and `LICENSE`, so the `ms` member has to be selected out of
    /// them. Installing the archive verbatim is exactly the bricking bug
    /// reported in meta_skill#159, so the extracted candidate is additionally
    /// run through [`verify_executable_format`] here — before the installer is
    /// ever handed a path.
    pub fn download_and_verify(&self, release: &ReleaseInfo) -> Result<PathBuf> {
        let binary_asset = self.find_binary_asset(release)?;
        let checksum_asset = self.find_checksum_asset(release);

        // Download the asset (usually an archive).
        let download_path = self.temp_dir.join(&binary_asset.name);
        self.download_asset(binary_asset, &download_path)?;

        // Verify checksum if available. This proves the *download* is intact;
        // it says nothing about what the file contains.
        if let Some(checksum_asset) = checksum_asset {
            let checksums = self.download_checksums(checksum_asset)?;
            if let Some(expected_hash) = checksums.get(&binary_asset.name) {
                let actual_hash = compute_sha256(&download_path)?;
                if !actual_hash.eq_ignore_ascii_case(expected_hash) {
                    // Clean up failed download
                    let _ = std::fs::remove_file(&download_path);
                    return Err(MsError::ValidationFailed(format!(
                        "checksum mismatch: expected {expected_hash}, got {actual_hash}"
                    )));
                }
            } else {
                tracing::warn!(
                    asset = %binary_asset.name,
                    "release checksum manifest has no entry for this asset; skipping checksum verification"
                );
            }
        }

        // Unpack the executable out of the archive.
        let extract_dir = self.temp_dir.join("extracted");
        std::fs::create_dir_all(&extract_dir)?;
        let binary_path = extract_binary(&download_path, &extract_dir)?;

        // Hard gate: whatever we hand to the installer must look like an
        // executable for this platform, never an archive (#159).
        verify_executable_format(&binary_path)?;

        Ok(binary_path)
    }

    fn find_binary_asset<'a>(&self, release: &'a ReleaseInfo) -> Result<&'a ReleaseAsset> {
        let patterns = current_target_patterns();
        select_binary_asset(&release.assets, &patterns).ok_or_else(|| {
            let available = release
                .assets
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            MsError::ValidationFailed(format!(
                "no binary found for target {} in release {} (tried: {}; available assets: {})",
                current_target(),
                release.tag,
                patterns.join(", "),
                if available.is_empty() {
                    "<none>"
                } else {
                    available.as_str()
                }
            ))
        })
    }

    fn find_checksum_asset<'a>(&self, release: &'a ReleaseInfo) -> Option<&'a ReleaseAsset> {
        release.assets.iter().find(|a| {
            let name = a.name.to_lowercase();
            name.contains("checksum") || name.contains("sha256") || name.ends_with(".sha256")
        })
    }

    fn download_asset(&self, asset: &ReleaseAsset, dest: &Path) -> Result<()> {
        let client = GitHubClient::new(self.token.clone());
        let bytes = client.download_url(&asset.download_url)?;
        std::fs::write(dest, bytes)?;
        Ok(())
    }

    fn download_checksums(
        &self,
        asset: &ReleaseAsset,
    ) -> Result<std::collections::HashMap<String, String>> {
        let client = GitHubClient::new(self.token.clone());
        let bytes = client.download_url(&asset.download_url)?;
        let content = String::from_utf8(bytes)
            .map_err(|e| MsError::ValidationFailed(format!("invalid checksum file: {e}")))?;

        let mut checksums = std::collections::HashMap::new();
        for line in content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                // Format: "hash  filename" or "hash filename"
                let hash = parts[0].to_string();
                let filename = parts[parts.len() - 1].trim_start_matches('*').to_string();
                checksums.insert(filename, hash);
            }
        }

        Ok(checksums)
    }

    /// Clean up temporary files.
    pub fn cleanup(&self) -> Result<()> {
        if self.temp_dir.exists() {
            std::fs::remove_dir_all(&self.temp_dir)?;
        }
        Ok(())
    }
}

// NOTE: Intentionally not implementing Default for UpdateDownloader.
// Creating a temp directory can fail, so callers must use UpdateDownloader::new()
// which properly returns a Result for error handling.

// --- Archive extraction (#159) ---

/// Executable member names accepted inside a release archive.
///
/// Unix archives contain `ms`; the Windows zip contains `ms.exe`.
const BINARY_MEMBER_NAMES: &[&str] = &["ms", "ms.exe"];

/// Smallest plausible size for a real `ms` build. The shipped binaries are
/// >10 MB; anything under a kilobyte is a truncated download or an error page.
const MIN_BINARY_BYTES: u64 = 1024;

const GZIP_MAGIC: &[u8] = &[0x1f, 0x8b];
const ZIP_MAGIC: &[u8] = b"PK\x03\x04";

/// Compressed-container magics that must never reach the installer, with a
/// human-readable name for the error message.
const ARCHIVE_MAGICS: &[(&[u8], &str)] = &[
    (GZIP_MAGIC, "gzip"),
    (ZIP_MAGIC, "zip"),
    (b"PK\x05\x06", "zip (empty archive)"),
    (b"BZh", "bzip2"),
    (&[0xfd, b'7', b'z', b'X', b'Z', 0x00], "xz"),
    (&[0x28, 0xb5, 0x2f, 0xfd], "zstd"),
    (&[0x37, 0x7a, 0xbc, 0xaf, 0x27, 0x1c], "7z"),
];

/// Shape of a downloaded release asset, decided by content magic rather than
/// by file extension so a mislabelled asset cannot slip past.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
    TarGz,
    Zip,
    /// Not a container — the asset is the executable itself.
    Raw,
}

fn read_magic(path: &Path, len: usize) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut buf = vec![0u8; len];
    let mut filled = 0;
    while filled < len {
        let n = file.read(&mut buf[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    buf.truncate(filled);
    Ok(buf)
}

fn detect_archive_kind(path: &Path) -> Result<ArchiveKind> {
    let magic = read_magic(path, 4)?;
    if magic.starts_with(GZIP_MAGIC) {
        Ok(ArchiveKind::TarGz)
    } else if magic.starts_with(ZIP_MAGIC) {
        Ok(ArchiveKind::Zip)
    } else {
        Ok(ArchiveKind::Raw)
    }
}

fn is_binary_member(name: &str) -> bool {
    let base = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .to_ascii_lowercase();
    BINARY_MEMBER_NAMES.contains(&base.as_str())
}

/// Copy a stream into `dest`, flush it to disk, and mark it executable.
fn write_executable<R: std::io::Read>(reader: &mut R, dest: &Path) -> Result<()> {
    let mut out = std::fs::File::create(dest)?;
    std::io::copy(reader, &mut out)?;
    out.sync_all()?;
    drop(out);
    set_executable_bit(dest)
}

fn set_executable_bit(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(path, perms)?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

/// Extract the `ms` executable from a downloaded release asset.
///
/// Entries are streamed into a fixed destination path chosen by us, so a
/// hostile archive cannot write outside `dest_dir` (no `unpack`/`extract`
/// call is used). Returns the path of the extracted executable.
fn extract_binary(archive: &Path, dest_dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(dest_dir)?;
    let dest = dest_dir.join(if cfg!(windows) { "ms.exe" } else { "ms" });

    match detect_archive_kind(archive)? {
        ArchiveKind::TarGz => extract_from_tar_gz(archive, &dest)?,
        ArchiveKind::Zip => extract_from_zip(archive, &dest)?,
        ArchiveKind::Raw => {
            // The asset is the executable itself (older/bare release layouts).
            std::fs::copy(archive, &dest)?;
            set_executable_bit(&dest)?;
        }
    }

    Ok(dest)
}

fn extract_from_tar_gz(archive: &Path, dest: &Path) -> Result<()> {
    let file = std::fs::File::open(archive)?;
    let decoder = flate2::read::GzDecoder::new(std::io::BufReader::new(file));
    let mut tar = tar::Archive::new(decoder);

    for entry in tar.entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let name = entry
            .path()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        if is_binary_member(&name) {
            write_executable(&mut entry, dest)?;
            return Ok(());
        }
    }

    Err(MsError::ValidationFailed(format!(
        "release archive {} contains no `ms` executable member",
        archive.display()
    )))
}

fn extract_from_zip(archive: &Path, dest: &Path) -> Result<()> {
    let file = std::fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file))
        .map_err(|e| MsError::ValidationFailed(format!("invalid zip release archive: {e}")))?;

    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|e| MsError::ValidationFailed(format!("unreadable zip entry: {e}")))?;
        if !entry.is_file() {
            continue;
        }
        let name = entry.name().to_string();
        if is_binary_member(&name) {
            write_executable(&mut entry, dest)?;
            return Ok(());
        }
    }

    Err(MsError::ValidationFailed(format!(
        "release archive {} contains no `ms` executable member",
        archive.display()
    )))
}

// --- Pre-install verification gates (#159) ---

/// Reject anything that is not a native executable image for this platform.
///
/// This is the gate that turns the meta_skill#159 class of bug (installing a
/// `.tar.gz` verbatim) into a loud failure instead of a silent brick. It
/// checks, in order:
///
/// * the path is a regular, non-trivially-sized file;
/// * the leading bytes are not a compressed-archive magic (gzip/zip/xz/...);
/// * the leading bytes are a native executable magic — ELF or Mach-O on Unix,
///   `MZ` on Windows;
/// * on Unix, the executable bit is set.
pub fn verify_executable_format(path: &Path) -> Result<()> {
    let metadata = std::fs::metadata(path).map_err(|e| {
        MsError::ValidationFailed(format!(
            "update candidate {} is not readable: {e}",
            path.display()
        ))
    })?;

    if !metadata.is_file() {
        return Err(MsError::ValidationFailed(format!(
            "update candidate {} is not a regular file",
            path.display()
        )));
    }

    if metadata.len() < MIN_BINARY_BYTES {
        return Err(MsError::ValidationFailed(format!(
            "update candidate {} is only {} bytes; refusing to install a truncated binary",
            path.display(),
            metadata.len()
        )));
    }

    let magic = read_magic(path, 8)?;

    for (bytes, label) in ARCHIVE_MAGICS {
        if magic.starts_with(bytes) {
            return Err(MsError::ValidationFailed(format!(
                "update candidate {} is a {label} archive, not an executable; \
                 refusing to install it (the `ms` member must be extracted first)",
                path.display()
            )));
        }
    }

    if !has_native_executable_magic(&magic) {
        return Err(MsError::ValidationFailed(format!(
            "update candidate {} is not a native executable (leading bytes: {}); \
             refusing to replace the running binary",
            path.display(),
            hex::encode(&magic)
        )));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(MsError::ValidationFailed(format!(
                "update candidate {} is not marked executable",
                path.display()
            )));
        }
    }

    Ok(())
}

/// Whether the leading bytes are a native executable image for this platform.
///
/// Both ELF and Mach-O are accepted on Unix: the format check is a cheap
/// "definitely not an archive" filter, and [`verify_binary_runs`] is the
/// authoritative check that the image is loadable on *this* machine.
fn has_native_executable_magic(magic: &[u8]) -> bool {
    // PE/COFF (Windows).
    if cfg!(windows) {
        return magic.starts_with(b"MZ");
    }

    // ELF (Linux, BSD).
    if magic.starts_with(b"\x7fELF") {
        return true;
    }

    // Mach-O thin (32/64-bit, both endiannesses) and universal/fat.
    const MACHO_MAGICS: &[[u8; 4]] = &[
        [0xfe, 0xed, 0xfa, 0xce],
        [0xfe, 0xed, 0xfa, 0xcf],
        [0xce, 0xfa, 0xed, 0xfe],
        [0xcf, 0xfa, 0xed, 0xfe],
        [0xca, 0xfe, 0xba, 0xbe],
        [0xbe, 0xba, 0xfe, 0xca],
        [0xca, 0xfe, 0xba, 0xbf],
        [0xbf, 0xba, 0xfe, 0xca],
    ];
    MACHO_MAGICS.iter().any(|m| magic.starts_with(m))
}

/// How long to wait for a candidate binary to answer `--version`.
const RUN_CHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Execute a candidate binary with `--version` and require a successful exit.
///
/// A gzip file with the executable bit set cannot be `exec`'d at all, and a
/// wrong-architecture or truncated image fails to load, so this catches every
/// mismatch the static format check might miss.
pub fn verify_binary_runs(path: &Path) -> Result<()> {
    use std::process::{Command, Stdio};

    let mut child = Command::new(path)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            MsError::ValidationFailed(format!(
                "update candidate {} could not be executed: {e}",
                path.display()
            ))
        })?;

    let deadline = std::time::Instant::now() + RUN_CHECK_TIMEOUT;
    let status = loop {
        match child.try_wait()? {
            Some(status) => break status,
            None => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(MsError::ValidationFailed(format!(
                        "update candidate {} did not answer `--version` within {}s",
                        path.display(),
                        RUN_CHECK_TIMEOUT.as_secs()
                    )));
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
        }
    };

    if !status.success() {
        return Err(MsError::ValidationFailed(format!(
            "update candidate {} exited with {status} when run with `--version`",
            path.display()
        )));
    }

    Ok(())
}

/// Full pre-install gate: native-executable format plus a live `--version` run.
pub fn verify_installable_binary(path: &Path) -> Result<()> {
    verify_executable_format(path)?;
    verify_binary_runs(path)
}

/// Installer for atomic binary replacement.
pub struct UpdateInstaller {
    current_binary: PathBuf,
    backup_dir: PathBuf,
}

impl UpdateInstaller {
    /// Create a new installer for the current binary.
    pub fn new() -> Result<Self> {
        let current_binary = std::env::current_exe()?;
        let backup_dir = current_binary
            .parent()
            .unwrap_or(Path::new("."))
            .join(".ms-backup");
        Ok(Self {
            current_binary,
            backup_dir,
        })
    }

    /// Create an installer with explicit paths.
    #[must_use]
    pub const fn with_paths(current_binary: PathBuf, backup_dir: PathBuf) -> Self {
        Self {
            current_binary,
            backup_dir,
        }
    }

    /// Install a new binary atomically, refusing to proceed unless it is a
    /// working executable.
    ///
    /// Sequence (meta_skill#159):
    ///
    /// 1. Verify the candidate's format (rejects archives outright).
    /// 2. Stage it as a hidden temp file **inside the destination directory**
    ///    so the final step is a same-filesystem `rename` (truly atomic).
    /// 3. Re-verify the staged copy and actually run it (`--version`).
    /// 4. Back up the live binary, then rename the staged copy over it.
    /// 5. Verify the installed binary still runs; roll back from the backup and
    ///    fail loudly if it does not.
    ///
    /// Any failure before step 4 leaves the existing binary untouched; a
    /// failure after it restores the backup. The user is never left without a
    /// working `ms`.
    pub fn install(&self, new_binary: &Path) -> Result<InstallResult> {
        // 1. The candidate must be a native executable, not an archive.
        verify_executable_format(new_binary)?;

        std::fs::create_dir_all(&self.backup_dir)?;
        let install_dir = self
            .current_binary
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        std::fs::create_dir_all(&install_dir)?;

        // 2. Stage inside the destination directory (same filesystem).
        let staged = install_dir.join(format!(
            ".ms-update-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        if let Err(err) = std::fs::copy(new_binary, &staged) {
            let _ = std::fs::remove_file(&staged);
            return Err(err.into());
        }

        // 3. Re-verify the staged copy in its final home, and prove it runs.
        if let Err(err) =
            set_executable_bit(&staged).and_then(|()| verify_installable_binary(&staged))
        {
            let _ = std::fs::remove_file(&staged);
            return Err(err);
        }

        // 4. Back up the live binary, then swap.
        let backup_path = self.backup_dir.join("ms.backup");
        let had_previous = self.current_binary.exists();
        if had_previous && let Err(err) = std::fs::copy(&self.current_binary, &backup_path) {
            let _ = std::fs::remove_file(&staged);
            return Err(err.into());
        }

        #[cfg(windows)]
        {
            // Windows cannot rename over a running binary: move it aside first.
            let temp_current = self.current_binary.with_extension("old");
            if self.current_binary.exists() {
                let _ = std::fs::remove_file(&temp_current);
                if let Err(err) = std::fs::rename(&self.current_binary, &temp_current) {
                    let _ = std::fs::remove_file(&staged);
                    return Err(err.into());
                }
            }
        }

        if let Err(err) = std::fs::rename(&staged, &self.current_binary) {
            let _ = std::fs::remove_file(&staged);
            #[cfg(windows)]
            {
                // Put the displaced binary back rather than leaving a hole.
                let displaced = self.current_binary.with_extension("old");
                if displaced.exists() && !self.current_binary.exists() {
                    let _ = std::fs::rename(&displaced, &self.current_binary);
                }
            }
            return Err(err.into());
        }

        // 5. Post-swap liveness check; roll back if the installed binary is bad.
        if let Err(err) = verify_binary_runs(&self.current_binary) {
            if had_previous && backup_path.exists() {
                let _ = std::fs::copy(&backup_path, &self.current_binary);
                let _ = set_executable_bit(&self.current_binary);
                return Err(MsError::ValidationFailed(format!(
                    "installed binary failed its post-install check ({err}); \
                     rolled back to the previous version from {}",
                    backup_path.display()
                )));
            }
            return Err(MsError::ValidationFailed(format!(
                "installed binary failed its post-install check ({err}) and no backup was available"
            )));
        }

        Ok(InstallResult {
            backup_path: if had_previous {
                Some(backup_path)
            } else {
                None
            },
            restart_required: true,
        })
    }

    /// Rollback to the backed-up binary.
    pub fn rollback(&self) -> Result<()> {
        let backup_path = self.backup_dir.join("ms.backup");
        if backup_path.exists() {
            std::fs::copy(&backup_path, &self.current_binary)?;
            std::fs::remove_file(&backup_path)?;
        }
        Ok(())
    }

    /// Clean up backup files.
    pub fn cleanup_backup(&self) -> Result<()> {
        let backup_path = self.backup_dir.join("ms.backup");
        if backup_path.exists() {
            std::fs::remove_file(&backup_path)?;
        }
        // Also clean up Windows .old files
        let old_path = self.current_binary.with_extension("old");
        if old_path.exists() {
            let _ = std::fs::remove_file(&old_path);
        }
        Ok(())
    }
}

/// Result of an installation.
#[derive(Debug, Clone, Serialize)]
pub struct InstallResult {
    pub backup_path: Option<PathBuf>,
    pub restart_required: bool,
}

/// Response for update check (robot mode).
#[derive(Debug, Clone, Serialize)]
pub struct UpdateCheckResponse {
    pub current_version: String,
    pub channel: String,
    pub update_available: bool,
    pub latest_version: Option<String>,
    pub changelog: Option<String>,
    pub download_size: Option<u64>,
    pub html_url: Option<String>,
}

/// Response for update install (robot mode).
#[derive(Debug, Clone, Serialize)]
pub struct UpdateInstallResponse {
    pub success: bool,
    pub old_version: String,
    pub new_version: String,
    pub changelog: String,
    pub restart_required: bool,
}

// --- Internal GitHub client ---

const GH_API: &str = "https://api.github.com";
const USER_AGENT: &str = "ms-cli";

struct GitHubClient {
    client: reqwest::blocking::Client,
    token: Option<String>,
}

impl GitHubClient {
    fn new(token: Option<String>) -> Self {
        Self {
            client: reqwest::blocking::Client::new(),
            token,
        }
    }

    fn list_releases(&self, owner: &str, repo: &str) -> Result<Vec<ReleaseInfo>> {
        let url = format!("{GH_API}/repos/{owner}/{repo}/releases?per_page=30");
        let response = self.get(&url)?;

        if !response.status().is_success() {
            return Err(MsError::ValidationFailed(format!(
                "failed to list releases: HTTP {}",
                response.status()
            )));
        }

        let raw_releases: Vec<GitHubRelease> = response
            .json()
            .map_err(|e| MsError::ValidationFailed(format!("failed to parse releases: {e}")))?;

        Ok(raw_releases
            .into_iter()
            .filter_map(GitHubRelease::into_release_info)
            .collect())
    }

    fn download_url(&self, url: &str) -> Result<Vec<u8>> {
        let mut request = self.client.get(url).header("User-Agent", USER_AGENT);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }

        let response = request
            .send()
            .map_err(|e| MsError::Config(format!("download failed: {e}")))?;

        if !response.status().is_success() {
            return Err(MsError::ValidationFailed(format!(
                "download failed: HTTP {}",
                response.status()
            )));
        }

        response
            .bytes()
            .map(|b| b.to_vec())
            .map_err(|e| MsError::Config(format!("download read failed: {e}")))
    }

    fn get(&self, url: &str) -> Result<reqwest::blocking::Response> {
        let mut request = self
            .client
            .get(url)
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", USER_AGENT);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        request
            .send()
            .map_err(|e| MsError::Config(format!("github request failed: {e}")))
    }
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    prerelease: bool,
    body: Option<String>,
    published_at: Option<String>,
    html_url: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Deserialize)]
struct GitHubAsset {
    id: u64,
    name: String,
    browser_download_url: String,
    size: u64,
}

impl GitHubRelease {
    fn into_release_info(self) -> Option<ReleaseInfo> {
        // Parse version from tag (strip 'v' prefix)
        let version_str = self.tag_name.strip_prefix('v').unwrap_or(&self.tag_name);
        let version = Version::parse(version_str).ok()?;

        let published_at = self
            .published_at
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map_or_else(Utc::now, |dt| dt.with_timezone(&Utc));

        Some(ReleaseInfo {
            version,
            tag: self.tag_name,
            prerelease: self.prerelease,
            changelog: self.body.unwrap_or_default(),
            published_at,
            html_url: self.html_url,
            assets: self
                .assets
                .into_iter()
                .map(|a| ReleaseAsset {
                    id: a.id,
                    name: a.name,
                    download_url: a.browser_download_url,
                    size: a.size,
                })
                .collect(),
        })
    }
}

// --- Helper functions ---

fn token_from_env() -> Option<String> {
    std::env::var("MS_GITHUB_TOKEN")
        .ok()
        .or_else(|| std::env::var("GITHUB_TOKEN").ok())
        .or_else(|| std::env::var("GH_TOKEN").ok())
}

fn parse_repo(input: &str) -> Result<(String, String)> {
    let cleaned = input
        .strip_prefix("https://github.com/")
        .or_else(|| input.strip_prefix("http://github.com/"))
        .or_else(|| input.strip_prefix("github.com/"))
        .unwrap_or(input);

    let parts: Vec<&str> = cleaned.split('/').collect();
    if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(MsError::ValidationFailed(format!(
            "invalid repo reference: {input}"
        )));
    }

    Ok((
        parts[0].to_string(),
        parts[1].trim_end_matches(".git").to_string(),
    ))
}

fn compute_sha256(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn current_target() -> String {
    let os = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "unknown"
    };

    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unknown"
    };

    format!("{os}-{arch}")
}

/// Alternative spellings of an architecture, canonical name first.
///
/// Release tooling in the wild is inconsistent: the Rust triple says
/// `aarch64`/`x86_64`, while hand-rolled packaging scripts frequently emit
/// `arm64`/`amd64`/`x64`. `ms update` has to match all of them (#152).
fn arch_aliases(arch: &str) -> Vec<&str> {
    match arch {
        "aarch64" => vec!["aarch64", "arm64"],
        "arm64" => vec!["arm64", "aarch64"],
        "x86_64" => vec!["x86_64", "amd64", "x64"],
        "amd64" => vec!["amd64", "x86_64", "x64"],
        "x64" => vec!["x64", "x86_64", "amd64"],
        other => vec![other],
    }
}

/// Alternative spellings of an operating system, canonical name first.
fn os_aliases(os: &str) -> Vec<&str> {
    match os {
        "macos" => vec!["macos", "darwin", "apple", "osx"],
        "linux" => vec!["linux"],
        "windows" => vec!["windows", "win"],
        other => vec![other],
    }
}

/// Asset-name substrings identifying a binary for the given platform, in
/// priority order.
///
/// The release workflow names assets with the **Rust target triple**
/// (`ms-0.1.5-aarch64-apple-darwin.tar.gz`), while other tooling uses a
/// legacy `os-arch` scheme (`macos-aarch64`), an `arch-os` scheme
/// (`arm64-darwin`), or vendor spellings (`arm64`, `amd64`, `x64`). The
/// updater must match every convention we could plausibly ship (#152), so the
/// triples come first and the looser spellings follow as fallbacks.
fn target_patterns(os: &str, arch: &str) -> Vec<String> {
    let arches = arch_aliases(arch);
    let oses = os_aliases(os);
    let mut patterns: Vec<String> = Vec::new();

    // 1. Rust target triples — what the release workflow actually emits.
    for a in &arches {
        match os {
            "linux" => {
                patterns.push(format!("{a}-unknown-linux-gnu"));
                // musl builds are statically linked and run on glibc hosts too.
                patterns.push(format!("{a}-unknown-linux-musl"));
            }
            "macos" => patterns.push(format!("{a}-apple-darwin")),
            "windows" => {
                patterns.push(format!("{a}-pc-windows-msvc"));
                patterns.push(format!("{a}-pc-windows-gnu"));
            }
            _ => {}
        }
    }

    // 2. Legacy `os-arch` spellings (`macos-aarch64`, `linux-amd64`, ...).
    for o in &oses {
        for a in &arches {
            patterns.push(format!("{o}-{a}"));
        }
    }

    // 3. Reversed `arch-os` spellings (`arm64-darwin`, `x86_64-linux`, ...).
    for a in &arches {
        for o in &oses {
            patterns.push(format!("{a}-{o}"));
        }
    }

    // 4. macOS universal (fat) binaries run on every Mac architecture.
    if os == "macos" {
        patterns.push("universal2-apple-darwin".to_string());
        patterns.push("universal-apple-darwin".to_string());
        patterns.push("macos-universal".to_string());
        patterns.push("universal2".to_string());
    }

    if patterns.is_empty() {
        patterns.push(format!("{os}-{arch}"));
    }

    let mut seen = std::collections::HashSet::new();
    patterns.retain(|p| seen.insert(p.clone()));
    patterns
}

/// Target patterns for the platform this binary was compiled for.
fn current_target_patterns() -> Vec<String> {
    let target = current_target();
    let (os, arch) = target.split_once('-').unwrap_or(("unknown", "unknown"));
    target_patterns(os, arch)
}

/// Suffixes that mark an asset as release *metadata* rather than a payload.
const METADATA_SUFFIXES: &[&str] = &[
    ".txt",
    ".md",
    ".sha256",
    ".sha512",
    ".md5",
    ".sig",
    ".asc",
    ".pem",
    ".crt",
    ".json",
    ".jsonl",
    ".sbom",
    ".spdx",
    ".cdx",
    ".pub",
    ".sigstore",
];

/// Whether an asset name belongs to the `ms` binary family.
///
/// The old check was a bare `name.contains("ms")`, which also matches
/// `SHA256SUMS.txt` ("su**ms**"). Require the name to *start* with `ms`
/// followed by a separator (or nothing) instead.
fn is_ms_asset(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let base = lower.rsplit(['/', '\\']).next().unwrap_or(lower.as_str());
    base.strip_prefix("ms")
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(['-', '_', '.']))
}

/// Whether an asset is a checksum manifest, signature, SBOM, or similar.
fn is_metadata_asset(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    METADATA_SUFFIXES.iter().any(|s| lower.ends_with(s))
        || lower.contains("sha256sums")
        || lower.contains("checksums")
}

/// Pick the best-matching binary asset for the given target patterns.
///
/// Only assets belonging to the `ms` family and carrying no metadata suffix are
/// considered, so a checksum manifest or signature can never be mistaken for a
/// binary. Patterns are tried in priority order; a generic binary (bare `ms` /
/// `ms.exe` with no platform in its name) is the last resort.
fn select_binary_asset<'a>(
    assets: &'a [ReleaseAsset],
    patterns: &[String],
) -> Option<&'a ReleaseAsset> {
    let candidates: Vec<&'a ReleaseAsset> = assets
        .iter()
        .filter(|a| is_ms_asset(&a.name) && !is_metadata_asset(&a.name))
        .collect();

    for pattern in patterns {
        if let Some(asset) = candidates
            .iter()
            .copied()
            .find(|a| a.name.to_ascii_lowercase().contains(pattern.as_str()))
        {
            return Some(asset);
        }
    }

    candidates.into_iter().find(|a| is_generic_binary(&a.name))
}

/// Whether an asset name is a bare, platform-less binary (`ms` or `ms.exe`).
fn is_generic_binary(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let stem = lower.strip_suffix(".exe").unwrap_or(lower.as_str());
    stem == "ms"
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // =========================================================================
    // UpdateChannel tests
    // =========================================================================

    #[test]
    fn parse_update_channel() {
        assert_eq!(
            "stable".parse::<UpdateChannel>().unwrap(),
            UpdateChannel::Stable
        );
        assert_eq!(
            "BETA".parse::<UpdateChannel>().unwrap(),
            UpdateChannel::Beta
        );
        assert_eq!(
            "Nightly".parse::<UpdateChannel>().unwrap(),
            UpdateChannel::Nightly
        );
        assert!("invalid".parse::<UpdateChannel>().is_err());
    }

    #[test]
    fn update_channel_default() {
        assert_eq!(UpdateChannel::default(), UpdateChannel::Stable);
    }

    #[test]
    fn update_channel_display() {
        assert_eq!(UpdateChannel::Stable.to_string(), "stable");
        assert_eq!(UpdateChannel::Beta.to_string(), "beta");
        assert_eq!(UpdateChannel::Nightly.to_string(), "nightly");
    }

    #[test]
    fn update_channel_serialization() {
        let channel = UpdateChannel::Beta;
        let json = serde_json::to_string(&channel).unwrap();
        assert_eq!(json, "\"beta\"");

        let deserialized: UpdateChannel = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, UpdateChannel::Beta);
    }

    // =========================================================================
    // parse_repo tests
    // =========================================================================

    #[test]
    fn parse_repo_basic() {
        let (owner, repo) = parse_repo("owner/repo").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn parse_repo_with_url() {
        let (owner, repo) = parse_repo("https://github.com/owner/repo.git").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn parse_repo_http_url() {
        let (owner, repo) = parse_repo("http://github.com/foo/bar").unwrap();
        assert_eq!(owner, "foo");
        assert_eq!(repo, "bar");
    }

    #[test]
    fn parse_repo_github_com_prefix() {
        let (owner, repo) = parse_repo("github.com/test/project").unwrap();
        assert_eq!(owner, "test");
        assert_eq!(repo, "project");
    }

    #[test]
    fn parse_repo_invalid_empty() {
        assert!(parse_repo("").is_err());
    }

    #[test]
    fn parse_repo_invalid_no_slash() {
        assert!(parse_repo("justrepo").is_err());
    }

    #[test]
    fn parse_repo_invalid_empty_owner() {
        assert!(parse_repo("/repo").is_err());
    }

    #[test]
    fn parse_repo_invalid_empty_repo() {
        assert!(parse_repo("owner/").is_err());
    }

    // =========================================================================
    // current_target tests
    // =========================================================================

    #[test]
    fn current_target_format() {
        let target = current_target();
        assert!(target.contains('-'));
        let parts: Vec<&str> = target.split('-').collect();
        assert_eq!(parts.len(), 2);
    }

    #[test]
    fn current_target_known_os() {
        let target = current_target();
        let os_known = target.contains("linux")
            || target.contains("macos")
            || target.contains("windows")
            || target.contains("unknown");
        assert!(os_known);
    }

    #[test]
    fn current_target_known_arch() {
        let target = current_target();
        let arch_known =
            target.contains("x86_64") || target.contains("aarch64") || target.contains("unknown");
        assert!(arch_known);
    }

    // =========================================================================
    // is_generic_binary tests
    // =========================================================================

    #[test]
    fn is_generic_binary_windows_exe() {
        assert!(is_generic_binary("ms.exe"));
    }

    #[test]
    fn is_generic_binary_no_extension() {
        assert!(is_generic_binary("ms"));
    }

    #[test]
    fn is_generic_binary_with_linux_target() {
        assert!(!is_generic_binary("ms-linux-x86_64"));
    }

    #[test]
    fn is_generic_binary_with_macos_target() {
        assert!(!is_generic_binary("ms-macos-aarch64"));
    }

    #[test]
    fn is_generic_binary_with_darwin_target() {
        assert!(!is_generic_binary("ms-darwin-x86_64"));
    }

    #[test]
    fn is_generic_binary_with_windows_target() {
        assert!(!is_generic_binary("ms-windows-x86_64.exe"));
    }

    // =========================================================================
    // target pattern / asset selection tests (#152)
    // =========================================================================

    fn asset(name: &str) -> ReleaseAsset {
        ReleaseAsset {
            id: 1,
            name: name.to_string(),
            download_url: format!("https://example.invalid/{name}"),
            size: 1024,
        }
    }

    /// The exact asset list shipped in release v0.1.5.
    fn v015_assets() -> Vec<ReleaseAsset> {
        vec![
            asset("ms-0.1.5-aarch64-apple-darwin.tar.gz"),
            asset("ms-0.1.5-aarch64-unknown-linux-gnu.tar.gz"),
            asset("ms-0.1.5-x86_64-pc-windows-msvc.zip"),
            asset("ms-0.1.5-x86_64-unknown-linux-gnu.tar.gz"),
            asset("SHA256SUMS.txt"),
        ]
    }

    /// Every shipped platform must resolve its own asset from the real
    /// v0.1.5 asset names (regression for #152: `macos-aarch64` matched
    /// nothing because assets are named with Rust triples).
    #[test]
    fn select_binary_asset_matches_all_shipped_assets() {
        let assets = v015_assets();
        let cases = [
            ("macos", "aarch64", "ms-0.1.5-aarch64-apple-darwin.tar.gz"),
            (
                "linux",
                "aarch64",
                "ms-0.1.5-aarch64-unknown-linux-gnu.tar.gz",
            ),
            ("windows", "x86_64", "ms-0.1.5-x86_64-pc-windows-msvc.zip"),
            (
                "linux",
                "x86_64",
                "ms-0.1.5-x86_64-unknown-linux-gnu.tar.gz",
            ),
        ];
        for (os, arch, expected) in cases {
            let patterns = target_patterns(os, arch);
            let selected = select_binary_asset(&assets, &patterns)
                .unwrap_or_else(|| panic!("no asset selected for {os}-{arch}"));
            assert_eq!(
                selected.name, expected,
                "wrong asset for {os}-{arch}: got {}",
                selected.name
            );
        }
    }

    /// A platform with no shipped asset (e.g. macOS x86_64 in v0.1.5) must
    /// yield None rather than a wrong-platform binary.
    #[test]
    fn select_binary_asset_missing_platform_returns_none() {
        let assets = v015_assets();
        let patterns = target_patterns("macos", "x86_64");
        assert!(select_binary_asset(&assets, &patterns).is_none());
    }

    /// The checksum manifest must never be selected as a binary.
    #[test]
    fn select_binary_asset_never_picks_checksums() {
        let assets = vec![asset("SHA256SUMS.txt")];
        for (os, arch) in [
            ("linux", "x86_64"),
            ("linux", "aarch64"),
            ("macos", "aarch64"),
            ("windows", "x86_64"),
        ] {
            assert!(
                select_binary_asset(&assets, &target_patterns(os, arch)).is_none(),
                "checksum manifest selected for {os}-{arch}"
            );
        }
    }

    /// Legacy `os-arch` asset names must still match as a fallback.
    #[test]
    fn select_binary_asset_legacy_naming_fallback() {
        let assets = vec![
            asset("ms-macos-aarch64.tar.gz"),
            asset("ms-linux-x86_64.tar.gz"),
            asset("ms-windows-x86_64.zip"),
        ];
        let cases = [
            ("macos", "aarch64", "ms-macos-aarch64.tar.gz"),
            ("linux", "x86_64", "ms-linux-x86_64.tar.gz"),
            ("windows", "x86_64", "ms-windows-x86_64.zip"),
        ];
        for (os, arch, expected) in cases {
            let selected = select_binary_asset(&assets, &target_patterns(os, arch)).unwrap();
            assert_eq!(
                selected.name, expected,
                "wrong legacy asset for {os}-{arch}"
            );
        }
    }

    /// The Rust-triple asset wins over a legacy-named or generic asset.
    #[test]
    fn select_binary_asset_prefers_triple_over_legacy_and_generic() {
        let assets = vec![
            asset("ms"),
            asset("ms-macos-aarch64.tar.gz"),
            asset("ms-0.1.6-aarch64-apple-darwin.tar.gz"),
        ];
        let selected = select_binary_asset(&assets, &target_patterns("macos", "aarch64")).unwrap();
        assert_eq!(selected.name, "ms-0.1.6-aarch64-apple-darwin.tar.gz");
    }

    /// A generic bare binary is used as the last resort.
    #[test]
    fn select_binary_asset_generic_fallback() {
        let assets = vec![asset("ms"), asset("SHA256SUMS.txt")];
        let selected = select_binary_asset(&assets, &target_patterns("linux", "x86_64")).unwrap();
        assert_eq!(selected.name, "ms");
    }

    /// `current_target_patterns` puts the compile-target's Rust triple first.
    #[test]
    fn current_target_patterns_lead_with_triple() {
        let patterns = current_target_patterns();
        assert!(!patterns.is_empty());
        if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            assert_eq!(patterns[0], "x86_64-unknown-linux-gnu");
        } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            assert_eq!(patterns[0], "aarch64-apple-darwin");
        } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
            assert_eq!(patterns[0], "x86_64-pc-windows-msvc");
        } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
            assert_eq!(patterns[0], "aarch64-unknown-linux-gnu");
        }
    }

    // =========================================================================
    // compute_sha256 tests
    // =========================================================================

    #[test]
    fn compute_sha256_known_content() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("test.txt");
        std::fs::write(&file, "hello world").unwrap();

        let hash = compute_sha256(&file).unwrap();
        // SHA256 of "hello world" is b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn compute_sha256_empty_file() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("empty.txt");
        std::fs::write(&file, "").unwrap();

        let hash = compute_sha256(&file).unwrap();
        // SHA256 of empty string
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn compute_sha256_nonexistent_file() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("nonexistent.txt");

        let result = compute_sha256(&file);
        assert!(result.is_err());
    }

    // =========================================================================
    // channel_matches tests
    // =========================================================================

    #[test]
    fn channel_matches() {
        let checker = UpdateChecker::new(
            Version::new(0, 1, 0),
            UpdateChannel::Stable,
            "owner/repo".to_string(),
        );

        let stable_release = ReleaseInfo {
            version: Version::new(1, 0, 0),
            tag: "v1.0.0".to_string(),
            prerelease: false,
            assets: vec![],
            changelog: String::new(),
            published_at: Utc::now(),
            html_url: String::new(),
        };

        let beta_release = ReleaseInfo {
            version: Version::new(1, 1, 0),
            tag: "v1.1.0-beta.1".to_string(),
            prerelease: true,
            assets: vec![],
            changelog: String::new(),
            published_at: Utc::now(),
            html_url: String::new(),
        };

        assert!(checker.matches_channel(&stable_release));
        assert!(!checker.matches_channel(&beta_release));
    }

    #[test]
    fn channel_matches_beta_accepts_stable() {
        let checker = UpdateChecker::new(
            Version::new(0, 1, 0),
            UpdateChannel::Beta,
            "owner/repo".to_string(),
        );

        let stable_release = ReleaseInfo {
            version: Version::new(1, 0, 0),
            tag: "v1.0.0".to_string(),
            prerelease: false,
            assets: vec![],
            changelog: String::new(),
            published_at: Utc::now(),
            html_url: String::new(),
        };

        // Beta channel accepts stable releases
        assert!(checker.matches_channel(&stable_release));
    }

    #[test]
    fn channel_matches_nightly_accepts_all() {
        let checker = UpdateChecker::new(
            Version::new(0, 1, 0),
            UpdateChannel::Nightly,
            "owner/repo".to_string(),
        );

        let stable_release = ReleaseInfo {
            version: Version::new(1, 0, 0),
            tag: "v1.0.0".to_string(),
            prerelease: false,
            assets: vec![],
            changelog: String::new(),
            published_at: Utc::now(),
            html_url: String::new(),
        };

        let prerelease = ReleaseInfo {
            version: Version::new(1, 1, 0),
            tag: "v1.1.0-alpha.1".to_string(),
            prerelease: true,
            assets: vec![],
            changelog: String::new(),
            published_at: Utc::now(),
            html_url: String::new(),
        };

        // Nightly channel accepts everything
        assert!(checker.matches_channel(&stable_release));
        assert!(checker.matches_channel(&prerelease));
    }

    // =========================================================================
    // UpdateChecker tests
    // =========================================================================

    #[test]
    fn update_checker_current_version() {
        let checker = UpdateChecker::new(
            Version::new(1, 2, 3),
            UpdateChannel::Stable,
            "owner/repo".to_string(),
        );

        assert_eq!(*checker.current_version(), Version::new(1, 2, 3));
    }

    #[test]
    fn update_checker_channel() {
        let checker = UpdateChecker::new(
            Version::new(1, 0, 0),
            UpdateChannel::Beta,
            "owner/repo".to_string(),
        );

        assert_eq!(checker.channel(), UpdateChannel::Beta);
    }

    #[test]
    fn update_checker_with_token() {
        let checker = UpdateChecker::new(
            Version::new(1, 0, 0),
            UpdateChannel::Stable,
            "owner/repo".to_string(),
        )
        .with_token(Some("test_token".to_string()));

        // Can't directly access token, but ensure it doesn't panic
        assert_eq!(checker.channel(), UpdateChannel::Stable);
    }

    // =========================================================================
    // ReleaseAsset tests
    // =========================================================================

    #[test]
    fn release_asset_serialization() {
        let asset = ReleaseAsset {
            id: 12345,
            name: "ms-linux-x86_64".to_string(),
            download_url: "https://example.com/download".to_string(),
            size: 1024 * 1024,
        };

        let json = serde_json::to_string(&asset).unwrap();
        assert!(json.contains("12345"));
        assert!(json.contains("ms-linux-x86_64"));

        let deserialized: ReleaseAsset = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, 12345);
        assert_eq!(deserialized.name, "ms-linux-x86_64");
    }

    // =========================================================================
    // InstallResult tests
    // =========================================================================

    #[test]
    fn install_result_serialization() {
        let result = InstallResult {
            backup_path: Some(PathBuf::from("/tmp/backup")),
            restart_required: true,
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("restart_required"));
        assert!(json.contains("true"));
    }

    // =========================================================================
    // UpdateDownloader tests
    // =========================================================================

    #[test]
    fn update_downloader_with_temp_dir() {
        let temp = TempDir::new().unwrap();
        let downloader = UpdateDownloader::with_temp_dir(temp.path().to_path_buf()).unwrap();

        // Cleanup should not fail
        downloader.cleanup().unwrap();
    }

    #[test]
    fn update_downloader_cleanup_nonexistent() {
        let temp = TempDir::new().unwrap();
        let temp_path = temp.path().join("nonexistent");
        // Create then drop
        {
            let _ = UpdateDownloader::with_temp_dir(temp_path.clone());
        }
        // Manual cleanup after temp_dir is gone should not fail
        if temp_path.exists() {
            std::fs::remove_dir_all(&temp_path).unwrap();
        }
    }

    // =========================================================================
    // UpdateInstaller tests
    // =========================================================================

    #[test]
    fn update_installer_with_paths() {
        let temp = TempDir::new().unwrap();
        let binary = temp.path().join("ms");
        let backup = temp.path().join("backup");

        let installer = UpdateInstaller::with_paths(binary.clone(), backup.clone());

        // Create a fake binary
        std::fs::write(&binary, "fake binary").unwrap();

        // Cleanup should work
        let _ = installer.cleanup_backup();
    }

    #[test]
    fn update_installer_rollback_no_backup() {
        let temp = TempDir::new().unwrap();
        let binary = temp.path().join("ms");
        let backup = temp.path().join("backup");

        let installer = UpdateInstaller::with_paths(binary, backup);

        // Rollback with no backup should succeed (no-op)
        installer.rollback().unwrap();
    }

    // =========================================================================
    // Asset-name hygiene (#152)
    // =========================================================================

    #[test]
    fn is_ms_asset_accepts_the_ms_family() {
        for name in [
            "ms",
            "ms.exe",
            "ms-0.1.5-aarch64-apple-darwin.tar.gz",
            "ms_0.1.5_linux_amd64.tar.gz",
            "MS-0.1.5-x86_64-unknown-linux-gnu.tar.gz",
        ] {
            assert!(is_ms_asset(name), "{name} should be an ms asset");
        }
    }

    /// `SHA256SUMS.txt` contains the substring "ms" ("su**ms**"); the old
    /// `name.contains("ms")` test accepted it as a binary candidate.
    #[test]
    fn is_ms_asset_rejects_lookalikes() {
        for name in [
            "SHA256SUMS.txt",
            "checksums.txt",
            "forms.tar.gz",
            "meta_skill-0.1.5.tar.gz",
            "",
        ] {
            assert!(!is_ms_asset(name), "{name} should not be an ms asset");
        }
    }

    #[test]
    fn is_metadata_asset_flags_manifests_and_signatures() {
        for name in [
            "SHA256SUMS.txt",
            "ms-0.1.5-x86_64-unknown-linux-gnu.tar.gz.sha256",
            "ms-0.1.5-x86_64-unknown-linux-gnu.tar.gz.sig",
            "ms-0.1.5.sbom.json",
            "README.md",
        ] {
            assert!(is_metadata_asset(name), "{name} should be metadata");
        }
        assert!(!is_metadata_asset(
            "ms-0.1.5-x86_64-unknown-linux-gnu.tar.gz"
        ));
    }

    /// A realistic, messy release listing: Rust triples, vendor spellings
    /// (`arm64`/`amd64`), checksum manifests, per-asset sidecars and an SBOM.
    /// Every shipped platform must resolve to its own payload (#152).
    #[test]
    fn select_binary_asset_over_realistic_release_listing() {
        let assets = vec![
            asset("ms-0.1.6-aarch64-apple-darwin.tar.gz"),
            asset("ms-0.1.6-aarch64-apple-darwin.tar.gz.sha256"),
            asset("ms-0.1.6-x86_64-apple-darwin.tar.gz"),
            asset("ms-0.1.6-aarch64-unknown-linux-gnu.tar.gz"),
            asset("ms-0.1.6-x86_64-unknown-linux-gnu.tar.gz"),
            asset("ms-0.1.6-x86_64-unknown-linux-musl.tar.gz"),
            asset("ms-0.1.6-x86_64-pc-windows-msvc.zip"),
            asset("ms-0.1.6.sbom.json"),
            asset("SHA256SUMS.txt"),
        ];
        let cases = [
            ("macos", "aarch64", "ms-0.1.6-aarch64-apple-darwin.tar.gz"),
            ("macos", "x86_64", "ms-0.1.6-x86_64-apple-darwin.tar.gz"),
            (
                "linux",
                "aarch64",
                "ms-0.1.6-aarch64-unknown-linux-gnu.tar.gz",
            ),
            (
                "linux",
                "x86_64",
                "ms-0.1.6-x86_64-unknown-linux-gnu.tar.gz",
            ),
            ("windows", "x86_64", "ms-0.1.6-x86_64-pc-windows-msvc.zip"),
        ];
        for (os, arch, expected) in cases {
            let selected = select_binary_asset(&assets, &target_patterns(os, arch))
                .unwrap_or_else(|| panic!("no asset selected for {os}-{arch}"));
            assert_eq!(selected.name, expected, "wrong asset for {os}-{arch}");
        }
    }

    /// `arm64`/`amd64`/`x64` spellings must resolve for the equivalent Rust
    /// architecture, in both `os-arch` and `arch-os` order (#152).
    #[test]
    fn select_binary_asset_matches_vendor_arch_spellings() {
        let cases = [
            (
                "macos",
                "aarch64",
                vec!["ms-0.1.6-darwin-arm64.tar.gz"],
                "ms-0.1.6-darwin-arm64.tar.gz",
            ),
            (
                "macos",
                "aarch64",
                vec!["ms-0.1.6-macos-arm64.tar.gz"],
                "ms-0.1.6-macos-arm64.tar.gz",
            ),
            (
                "macos",
                "aarch64",
                vec!["ms-0.1.6-arm64-darwin.tar.gz"],
                "ms-0.1.6-arm64-darwin.tar.gz",
            ),
            (
                "linux",
                "x86_64",
                vec!["ms-0.1.6-linux-amd64.tar.gz"],
                "ms-0.1.6-linux-amd64.tar.gz",
            ),
            (
                "linux",
                "aarch64",
                vec!["ms-0.1.6-linux-arm64.tar.gz"],
                "ms-0.1.6-linux-arm64.tar.gz",
            ),
            (
                "windows",
                "x86_64",
                vec!["ms-0.1.6-windows-x64.zip"],
                "ms-0.1.6-windows-x64.zip",
            ),
        ];
        for (os, arch, names, expected) in cases {
            let assets: Vec<ReleaseAsset> = names.iter().map(|n| asset(n)).collect();
            let selected = select_binary_asset(&assets, &target_patterns(os, arch))
                .unwrap_or_else(|| panic!("no asset selected for {os}-{arch} from {names:?}"));
            assert_eq!(selected.name, expected);
        }
    }

    /// macOS universal binaries are an acceptable fallback on either Mac arch.
    #[test]
    fn select_binary_asset_falls_back_to_macos_universal() {
        let assets = vec![asset("ms-0.1.6-universal2-apple-darwin.tar.gz")];
        for arch in ["aarch64", "x86_64"] {
            let selected = select_binary_asset(&assets, &target_patterns("macos", arch)).unwrap();
            assert_eq!(selected.name, "ms-0.1.6-universal2-apple-darwin.tar.gz");
        }
        // ... but never on Linux/Windows.
        for (os, arch) in [("linux", "x86_64"), ("windows", "x86_64")] {
            assert!(select_binary_asset(&assets, &target_patterns(os, arch)).is_none());
        }
    }

    /// An architecture must never resolve to a different architecture's asset.
    #[test]
    fn select_binary_asset_never_crosses_architectures() {
        let assets = vec![
            asset("ms-0.1.6-aarch64-unknown-linux-gnu.tar.gz"),
            asset("ms-0.1.6-x86_64-unknown-linux-gnu.tar.gz"),
        ];
        let arm = select_binary_asset(&assets, &target_patterns("linux", "aarch64")).unwrap();
        assert_eq!(arm.name, "ms-0.1.6-aarch64-unknown-linux-gnu.tar.gz");
        let intel = select_binary_asset(&assets, &target_patterns("linux", "x86_64")).unwrap();
        assert_eq!(intel.name, "ms-0.1.6-x86_64-unknown-linux-gnu.tar.gz");
    }

    /// Checksum sidecars must never be selected, even when they carry the
    /// platform pattern in their name.
    #[test]
    fn select_binary_asset_ignores_per_asset_sidecars() {
        let assets = vec![
            asset("ms-0.1.6-x86_64-unknown-linux-gnu.tar.gz.sha256"),
            asset("ms-0.1.6-x86_64-unknown-linux-gnu.tar.gz.sig"),
        ];
        assert!(select_binary_asset(&assets, &target_patterns("linux", "x86_64")).is_none());
    }

    #[test]
    fn arch_aliases_are_symmetric() {
        assert!(arch_aliases("aarch64").contains(&"arm64"));
        assert!(arch_aliases("arm64").contains(&"aarch64"));
        assert!(arch_aliases("x86_64").contains(&"amd64"));
        assert!(arch_aliases("x86_64").contains(&"x64"));
        assert_eq!(arch_aliases("riscv64"), vec!["riscv64"]);
    }

    #[test]
    fn target_patterns_lead_with_the_rust_triple() {
        assert_eq!(
            target_patterns("macos", "aarch64")[0],
            "aarch64-apple-darwin"
        );
        assert_eq!(
            target_patterns("linux", "x86_64")[0],
            "x86_64-unknown-linux-gnu"
        );
        assert_eq!(
            target_patterns("windows", "x86_64")[0],
            "x86_64-pc-windows-msvc"
        );
    }

    // =========================================================================
    // Archive extraction and install-time verification (#159)
    // =========================================================================

    /// Bytes that pass the native-executable magic check on Unix hosts.
    fn fake_executable_bytes() -> Vec<u8> {
        let mut bytes = if cfg!(windows) {
            b"MZ\x90\x00".to_vec()
        } else {
            b"\x7fELF".to_vec()
        };
        bytes.extend(std::iter::repeat_n(0x42u8, 4096));
        bytes
    }

    /// Build a release-shaped `.tar.gz`: `LICENSE`, `ms`, `README.md`.
    fn build_release_tar_gz(path: &Path, binary_bytes: &[u8], include_binary: bool) {
        use std::io::Write;
        let file = std::fs::File::create(path).unwrap();
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
        let mut builder = tar::Builder::new(encoder);

        let mut add = |name: &str, data: &[u8], mode: u32| {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(mode);
            header.set_cksum();
            builder.append_data(&mut header, name, data).unwrap();
        };

        add("LICENSE", b"MIT", 0o644);
        if include_binary {
            add("ms", binary_bytes, 0o755);
        }
        add("README.md", b"# ms", 0o644);

        let encoder = builder.into_inner().unwrap();
        let mut file = encoder.finish().unwrap();
        file.flush().unwrap();
    }

    /// Build a release-shaped `.zip`: `README.md`, `ms.exe`.
    fn build_release_zip(path: &Path, binary_bytes: &[u8]) {
        use std::io::Write;
        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        writer.start_file("README.md", options).unwrap();
        writer.write_all(b"# ms").unwrap();
        writer.start_file("ms.exe", options).unwrap();
        writer.write_all(binary_bytes).unwrap();
        writer.finish().unwrap();
    }

    /// The core meta_skill#159 regression: extraction must yield the `ms`
    /// member, never the archive.
    #[test]
    fn extract_binary_unpacks_the_ms_member_from_tar_gz() {
        let temp = TempDir::new().unwrap();
        let archive = temp.path().join("ms-0.1.5-x86_64-unknown-linux-gnu.tar.gz");
        let expected = fake_executable_bytes();
        build_release_tar_gz(&archive, &expected, true);

        // Sanity: the archive really is gzip.
        assert_eq!(&std::fs::read(&archive).unwrap()[..2], GZIP_MAGIC);

        let extracted = extract_binary(&archive, &temp.path().join("out")).unwrap();
        let bytes = std::fs::read(&extracted).unwrap();
        assert_eq!(bytes, expected, "extracted payload is not the `ms` member");
        assert_ne!(
            bytes,
            std::fs::read(&archive).unwrap(),
            "the archive itself was installed (#159)"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&extracted).unwrap().permissions().mode();
            assert!(mode & 0o111 != 0, "extracted binary is not executable");
        }
    }

    #[test]
    fn extract_binary_unpacks_the_ms_member_from_zip() {
        let temp = TempDir::new().unwrap();
        let archive = temp.path().join("ms-0.1.5-x86_64-pc-windows-msvc.zip");
        let expected = fake_executable_bytes();
        build_release_zip(&archive, &expected);

        let extracted = extract_binary(&archive, &temp.path().join("out")).unwrap();
        assert_eq!(std::fs::read(&extracted).unwrap(), expected);
    }

    #[test]
    fn extract_binary_passes_through_a_raw_binary_asset() {
        let temp = TempDir::new().unwrap();
        let raw = temp.path().join("ms");
        let expected = fake_executable_bytes();
        std::fs::write(&raw, &expected).unwrap();

        let extracted = extract_binary(&raw, &temp.path().join("out")).unwrap();
        assert_eq!(std::fs::read(&extracted).unwrap(), expected);
    }

    #[test]
    fn extract_binary_errors_when_archive_has_no_ms_member() {
        let temp = TempDir::new().unwrap();
        let archive = temp.path().join("ms-0.1.5-x86_64-unknown-linux-gnu.tar.gz");
        build_release_tar_gz(&archive, &[], false);

        let err = extract_binary(&archive, &temp.path().join("out")).unwrap_err();
        assert!(
            err.to_string().contains("no `ms` executable member"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn detect_archive_kind_uses_content_not_extension() {
        let temp = TempDir::new().unwrap();

        let targz = temp.path().join("mislabelled.bin");
        build_release_tar_gz(&targz, &fake_executable_bytes(), true);
        assert_eq!(detect_archive_kind(&targz).unwrap(), ArchiveKind::TarGz);

        let zipped = temp.path().join("mislabelled.tar.gz");
        build_release_zip(&zipped, &fake_executable_bytes());
        assert_eq!(detect_archive_kind(&zipped).unwrap(), ArchiveKind::Zip);

        let raw = temp.path().join("ms");
        std::fs::write(&raw, fake_executable_bytes()).unwrap();
        assert_eq!(detect_archive_kind(&raw).unwrap(), ArchiveKind::Raw);
    }

    /// The hard gate from meta_skill#159: a gzip file must never be accepted
    /// as an executable, no matter how it got there.
    #[test]
    fn verify_executable_format_rejects_gzip_magic() {
        let temp = TempDir::new().unwrap();
        let archive = temp.path().join("ms");
        build_release_tar_gz(&archive, &fake_executable_bytes(), true);
        set_executable_bit(&archive).unwrap();

        let err = verify_executable_format(&archive).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("gzip"), "unexpected error: {msg}");
        assert!(
            msg.contains("refusing to install"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn verify_executable_format_rejects_every_container_magic() {
        let temp = TempDir::new().unwrap();
        let cases: &[(&str, &[u8])] = &[
            ("zip", b"PK\x03\x04"),
            ("bzip2", b"BZh"),
            ("xz", &[0xfd, b'7', b'z', b'X', b'Z', 0x00]),
            ("zstd", &[0x28, 0xb5, 0x2f, 0xfd]),
            ("7z", &[0x37, 0x7a, 0xbc, 0xaf, 0x27, 0x1c]),
        ];
        for (label, magic) in cases {
            let path = temp.path().join(format!("candidate-{label}"));
            let mut bytes = (*magic).to_vec();
            bytes.extend(std::iter::repeat_n(0u8, 4096));
            std::fs::write(&path, &bytes).unwrap();
            set_executable_bit(&path).unwrap();
            let err = verify_executable_format(&path)
                .unwrap_err()
                .to_string()
                .to_lowercase();
            assert!(err.contains(*label), "{label} not rejected clearly: {err}");
        }
    }

    #[test]
    fn verify_executable_format_rejects_truncated_downloads() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("ms");
        std::fs::write(&path, b"\x7fELF short").unwrap();
        set_executable_bit(&path).unwrap();

        let err = verify_executable_format(&path).unwrap_err().to_string();
        assert!(err.contains("truncated"), "unexpected error: {err}");
    }

    #[test]
    fn verify_executable_format_rejects_text_payloads() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("ms");
        std::fs::write(&path, "x".repeat(4096)).unwrap();
        set_executable_bit(&path).unwrap();

        let err = verify_executable_format(&path).unwrap_err().to_string();
        assert!(err.contains("not a native executable"), "unexpected: {err}");
    }

    #[test]
    fn verify_executable_format_accepts_a_native_image() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("ms");
        std::fs::write(&path, fake_executable_bytes()).unwrap();
        set_executable_bit(&path).unwrap();

        verify_executable_format(&path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn verify_executable_format_requires_the_exec_bit() {
        use std::os::unix::fs::PermissionsExt;
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("ms");
        std::fs::write(&path, fake_executable_bytes()).unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o644);
        std::fs::set_permissions(&path, perms).unwrap();

        let err = verify_executable_format(&path).unwrap_err().to_string();
        assert!(err.contains("not marked executable"), "unexpected: {err}");
    }

    #[test]
    fn has_native_executable_magic_recognises_platform_images() {
        #[cfg(not(windows))]
        {
            assert!(has_native_executable_magic(b"\x7fELF\x02\x01\x01\x00"));
            assert!(has_native_executable_magic(&[
                0xcf, 0xfa, 0xed, 0xfe, 0x0c, 0x00, 0x00, 0x01
            ]));
            assert!(has_native_executable_magic(&[
                0xca, 0xfe, 0xba, 0xbe, 0x00, 0x00, 0x00, 0x02
            ]));
        }
        #[cfg(windows)]
        assert!(has_native_executable_magic(b"MZ\x90\x00"));

        assert!(!has_native_executable_magic(&[0x1f, 0x8b, 0x08, 0x00]));
        assert!(!has_native_executable_magic(b"hello wo"));
        assert!(!has_native_executable_magic(&[]));
    }

    /// A real, working executable for the run-check tests.
    #[cfg(unix)]
    fn system_true_binary() -> Option<PathBuf> {
        ["/usr/bin/true", "/bin/true"]
            .into_iter()
            .map(PathBuf::from)
            .find(|p| p.exists())
    }

    #[cfg(unix)]
    #[test]
    fn verify_binary_runs_accepts_a_real_executable() {
        let Some(bin) = system_true_binary() else {
            return;
        };
        verify_binary_runs(&bin).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn verify_binary_runs_rejects_something_that_cannot_exec() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("ms");
        // ELF magic, but not a loadable image.
        std::fs::write(&path, fake_executable_bytes()).unwrap();
        set_executable_bit(&path).unwrap();

        // Passes the static gate ...
        verify_executable_format(&path).unwrap();
        // ... but cannot actually run.
        let err = verify_binary_runs(&path).unwrap_err().to_string();
        assert!(
            err.contains("could not be executed") || err.contains("exited with"),
            "unexpected error: {err}"
        );
    }

    /// End-to-end meta_skill#159 regression: handing the installer a release
    /// archive must fail loudly and leave the live binary untouched.
    #[cfg(unix)]
    #[test]
    fn install_refuses_an_archive_and_preserves_the_live_binary() {
        let Some(system_bin) = system_true_binary() else {
            return;
        };
        let temp = TempDir::new().unwrap();
        let current = temp.path().join("ms");
        let backup_dir = temp.path().join(".ms-backup");
        std::fs::copy(&system_bin, &current).unwrap();
        set_executable_bit(&current).unwrap();
        let before = std::fs::read(&current).unwrap();

        let archive = temp.path().join("ms-0.1.5-aarch64-apple-darwin.tar.gz");
        build_release_tar_gz(&archive, &fake_executable_bytes(), true);

        let installer = UpdateInstaller::with_paths(current.clone(), backup_dir);
        let err = installer.install(&archive).unwrap_err().to_string();
        assert!(err.contains("gzip"), "unexpected error: {err}");

        assert_eq!(
            std::fs::read(&current).unwrap(),
            before,
            "live binary was modified despite the failed update (#159)"
        );
        verify_binary_runs(&current).expect("live binary must still work");
    }

    /// A candidate that passes the format gate but cannot execute must not be
    /// installed either.
    #[cfg(unix)]
    #[test]
    fn install_refuses_a_candidate_that_cannot_run() {
        let Some(system_bin) = system_true_binary() else {
            return;
        };
        let temp = TempDir::new().unwrap();
        let current = temp.path().join("ms");
        std::fs::copy(&system_bin, &current).unwrap();
        set_executable_bit(&current).unwrap();
        let before = std::fs::read(&current).unwrap();

        let candidate = temp.path().join("candidate");
        std::fs::write(&candidate, fake_executable_bytes()).unwrap();
        set_executable_bit(&candidate).unwrap();

        let installer =
            UpdateInstaller::with_paths(current.clone(), temp.path().join(".ms-backup"));
        assert!(installer.install(&candidate).is_err());
        assert_eq!(std::fs::read(&current).unwrap(), before);

        // No staging debris left behind.
        let leftovers: Vec<_> = std::fs::read_dir(temp.path())
            .unwrap()
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with(".ms-update-"))
            .collect();
        assert!(leftovers.is_empty(), "staging files left behind");
    }

    /// The happy path: a genuine executable is installed atomically, the old
    /// binary is backed up, and the installed file actually runs.
    #[cfg(unix)]
    #[test]
    fn install_replaces_the_binary_with_a_working_executable() {
        let Some(system_bin) = system_true_binary() else {
            return;
        };
        let temp = TempDir::new().unwrap();
        let current = temp.path().join("ms");
        let backup_dir = temp.path().join(".ms-backup");
        std::fs::write(&current, fake_executable_bytes()).unwrap();
        set_executable_bit(&current).unwrap();

        let installer = UpdateInstaller::with_paths(current.clone(), backup_dir.clone());
        let result = installer.install(&system_bin).unwrap();

        assert!(result.restart_required);
        let backup = result
            .backup_path
            .expect("previous binary must be backed up");
        assert!(backup.exists(), "backup missing at {}", backup.display());

        verify_installable_binary(&current).expect("installed binary must run");
        assert_eq!(
            std::fs::read(&current).unwrap(),
            std::fs::read(&system_bin).unwrap()
        );
    }

    /// The whole download path, end to end, against a mocked release server:
    /// tar.gz asset + SHA256SUMS manifest in, extracted executable out.
    #[cfg(unix)]
    #[test]
    fn download_and_verify_returns_an_extracted_executable() {
        use httpmock::MockServer;

        let temp = TempDir::new().unwrap();
        let archive_path = temp.path().join("release.tar.gz");
        let payload = fake_executable_bytes();
        build_release_tar_gz(&archive_path, &payload, true);
        let archive_bytes = std::fs::read(&archive_path).unwrap();
        let archive_hash = compute_sha256(&archive_path).unwrap();

        let asset_name = format!("ms-9.9.9-{}.tar.gz", current_target_patterns()[0]);
        let server = MockServer::start();
        let archive_mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path(format!("/{asset_name}"));
            then.status(200).body(archive_bytes.clone());
        });
        let sums_mock = server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/SHA256SUMS.txt");
            then.status(200)
                .body(format!("{archive_hash}  {asset_name}\n"));
        });

        let release = ReleaseInfo {
            version: Version::new(9, 9, 9),
            tag: "v9.9.9".to_string(),
            prerelease: false,
            assets: vec![
                ReleaseAsset {
                    id: 1,
                    name: asset_name.clone(),
                    download_url: server.url(format!("/{asset_name}")),
                    size: archive_bytes.len() as u64,
                },
                ReleaseAsset {
                    id: 2,
                    name: "SHA256SUMS.txt".to_string(),
                    download_url: server.url("/SHA256SUMS.txt"),
                    size: 64,
                },
            ],
            changelog: String::new(),
            published_at: Utc::now(),
            html_url: String::new(),
        };

        let downloader = UpdateDownloader::with_temp_dir(temp.path().join("dl"))
            .unwrap()
            .with_token(None);
        let installed = downloader.download_and_verify(&release).unwrap();

        archive_mock.assert();
        sums_mock.assert();

        let bytes = std::fs::read(&installed).unwrap();
        assert_eq!(
            bytes, payload,
            "download_and_verify returned the wrong file"
        );
        assert_ne!(
            bytes, archive_bytes,
            "download_and_verify returned the archive itself (#159)"
        );
        verify_executable_format(&installed).unwrap();
    }

    /// A corrupted download must be rejected by the checksum gate before any
    /// extraction is attempted.
    #[cfg(unix)]
    #[test]
    fn download_and_verify_rejects_a_checksum_mismatch() {
        use httpmock::MockServer;

        let temp = TempDir::new().unwrap();
        let archive_path = temp.path().join("release.tar.gz");
        build_release_tar_gz(&archive_path, &fake_executable_bytes(), true);
        let archive_bytes = std::fs::read(&archive_path).unwrap();

        let asset_name = format!("ms-9.9.9-{}.tar.gz", current_target_patterns()[0]);
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path(format!("/{asset_name}"));
            then.status(200).body(archive_bytes.clone());
        });
        server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/SHA256SUMS.txt");
            then.status(200)
                .body(format!("{}  {asset_name}\n", "0".repeat(64)));
        });

        let release = ReleaseInfo {
            version: Version::new(9, 9, 9),
            tag: "v9.9.9".to_string(),
            prerelease: false,
            assets: vec![
                ReleaseAsset {
                    id: 1,
                    name: asset_name.clone(),
                    download_url: server.url(format!("/{asset_name}")),
                    size: archive_bytes.len() as u64,
                },
                ReleaseAsset {
                    id: 2,
                    name: "SHA256SUMS.txt".to_string(),
                    download_url: server.url("/SHA256SUMS.txt"),
                    size: 64,
                },
            ],
            changelog: String::new(),
            published_at: Utc::now(),
            html_url: String::new(),
        };

        let downloader = UpdateDownloader::with_temp_dir(temp.path().join("dl"))
            .unwrap()
            .with_token(None);
        let err = downloader.download_and_verify(&release).unwrap_err();
        assert!(
            err.to_string().contains("checksum mismatch"),
            "unexpected error: {err}"
        );
    }
}
