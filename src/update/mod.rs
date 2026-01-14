//! Auto-update system for ms.
//!
//! Provides self-update mechanism following xf pattern: check for new versions,
//! download, verify checksums, and replace binaries safely.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{MsError, Result};

/// Update channel for release filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateChannel {
    Stable,
    Beta,
    Nightly,
}

impl Default for UpdateChannel {
    fn default() -> Self {
        Self::Stable
    }
}

impl std::str::FromStr for UpdateChannel {
    type Err = MsError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "stable" => Ok(Self::Stable),
            "beta" => Ok(Self::Beta),
            "nightly" => Ok(Self::Nightly),
            _ => Err(MsError::ValidationFailed(format!(
                "invalid update channel: {} (expected stable, beta, or nightly)",
                s
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub fn new(current_version: Version, channel: UpdateChannel, repo: String) -> Self {
        Self {
            current_version,
            channel,
            repo,
            token: token_from_env(),
        }
    }

    /// Set the GitHub token for authenticated requests.
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
    pub fn with_token(mut self, token: Option<String>) -> Self {
        self.token = token;
        self
    }

    /// Download and verify a release binary.
    pub fn download_and_verify(&self, release: &ReleaseInfo) -> Result<PathBuf> {
        let binary_asset = self.find_binary_asset(release)?;
        let checksum_asset = self.find_checksum_asset(release);

        // Download binary
        let binary_path = self.temp_dir.join(&binary_asset.name);
        self.download_asset(&binary_asset, &binary_path)?;

        // Verify checksum if available
        if let Some(checksum_asset) = checksum_asset {
            let checksums = self.download_checksums(&checksum_asset)?;
            if let Some(expected_hash) = checksums.get(&binary_asset.name) {
                let actual_hash = compute_sha256(&binary_path)?;
                if actual_hash != *expected_hash {
                    // Clean up failed download
                    let _ = std::fs::remove_file(&binary_path);
                    return Err(MsError::ValidationFailed(format!(
                        "checksum mismatch: expected {}, got {}",
                        expected_hash, actual_hash
                    )));
                }
            }
        }

        Ok(binary_path)
    }

    fn find_binary_asset(&self, release: &ReleaseInfo) -> Result<&ReleaseAsset> {
        let target = current_target();

        // Try to find a matching binary
        let candidates: Vec<_> = release
            .assets
            .iter()
            .filter(|a| {
                let name = a.name.to_lowercase();
                name.contains("ms") && (name.contains(&target) || is_generic_binary(&name))
            })
            .collect();

        if candidates.is_empty() {
            return Err(MsError::ValidationFailed(format!(
                "no binary found for target {} in release {}",
                target, release.tag
            )));
        }

        // Prefer target-specific binary
        candidates
            .iter()
            .find(|a| a.name.to_lowercase().contains(&target))
            .or(candidates.first())
            .copied()
            .ok_or_else(|| {
                MsError::ValidationFailed(format!("no suitable binary found for {}", target))
            })
    }

    fn find_checksum_asset(&self, release: &ReleaseInfo) -> Option<&ReleaseAsset> {
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
            .map_err(|e| MsError::ValidationFailed(format!("invalid checksum file: {}", e)))?;

        let mut checksums = std::collections::HashMap::new();
        for line in content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                // Format: "hash  filename" or "hash filename"
                let hash = parts[0].to_string();
                let filename = parts[parts.len() - 1]
                    .trim_start_matches('*')
                    .to_string();
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

impl Default for UpdateDownloader {
    fn default() -> Self {
        Self::new().expect("failed to create temp directory")
    }
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
    pub fn with_paths(current_binary: PathBuf, backup_dir: PathBuf) -> Self {
        Self {
            current_binary,
            backup_dir,
        }
    }

    /// Install a new binary atomically.
    pub fn install(&self, new_binary: &Path) -> Result<InstallResult> {
        // Ensure backup directory exists
        std::fs::create_dir_all(&self.backup_dir)?;

        // Make new binary executable (Unix)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(new_binary)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(new_binary, perms)?;
        }

        // Create backup
        let backup_path = self.backup_dir.join("ms.backup");
        if self.current_binary.exists() {
            std::fs::copy(&self.current_binary, &backup_path)?;
        }

        // Perform atomic swap
        #[cfg(unix)]
        {
            std::fs::rename(new_binary, &self.current_binary)?;
        }

        #[cfg(windows)]
        {
            // Windows cannot rename over a running binary
            let temp_current = self.current_binary.with_extension("old");
            if self.current_binary.exists() {
                std::fs::rename(&self.current_binary, &temp_current)?;
            }
            std::fs::rename(new_binary, &self.current_binary)?;
            // Old binary will be deleted on next startup
        }

        Ok(InstallResult {
            backup_path: Some(backup_path),
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

// --- Internal GitHub client (reusing bundler patterns) ---

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

        let raw_releases: Vec<GitHubRelease> = response.json().map_err(|e| {
            MsError::ValidationFailed(format!("failed to parse releases: {}", e))
        })?;

        Ok(raw_releases
            .into_iter()
            .filter_map(|r| r.into_release_info())
            .collect())
    }

    fn download_url(&self, url: &str) -> Result<Vec<u8>> {
        let mut request = self.client.get(url).header("User-Agent", USER_AGENT);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }

        let response = request
            .send()
            .map_err(|e| MsError::Config(format!("download failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(MsError::ValidationFailed(format!(
                "download failed: HTTP {}",
                response.status()
            )));
        }

        response
            .bytes()
            .map(|b| b.to_vec())
            .map_err(|e| MsError::Config(format!("download read failed: {}", e)))
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
            .map_err(|e| MsError::Config(format!("github request failed: {}", e)))
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
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

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
            "invalid repo reference: {}",
            input
        )));
    }

    Ok((
        parts[0].to_string(),
        parts[1].trim_end_matches(".git").to_string(),
    ))
}

fn compute_sha256(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let hash = hasher.finalize();
    Ok(hex::encode(hash))
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

    format!("{}-{}", os, arch)
}

fn is_generic_binary(name: &str) -> bool {
    // Check if it's a generic binary without target in name
    (name.ends_with(".exe") || !name.contains('.'))
        && !name.contains("linux")
        && !name.contains("macos")
        && !name.contains("windows")
        && !name.contains("darwin")
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn current_target_format() {
        let target = current_target();
        assert!(target.contains('-'));
        let parts: Vec<&str> = target.split('-').collect();
        assert_eq!(parts.len(), 2);
    }

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
}
