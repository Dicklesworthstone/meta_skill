//! GitHub publishing + downloads for bundles.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::bundler::package::BundlePackage;
use crate::error::{MsError, Result};

const GH_API: &str = "https://api.github.com";
const GH_UPLOADS: &str = "https://uploads.github.com";
const USER_AGENT: &str = "ms-cli";

/// Maximum download size for bundles (100 MB) to prevent memory exhaustion
const MAX_DOWNLOAD_SIZE: u64 = 100 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct GitHubConfig {
    pub repo: String,
    pub token: Option<String>,
    pub tag: Option<String>,
    pub asset_name: Option<String>,
    pub draft: bool,
    pub prerelease: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublishResult {
    pub repo: String,
    pub release_url: String,
    pub asset_name: String,
    pub tag: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadResult {
    pub repo: String,
    pub asset_name: String,
    pub tag: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
struct RepoRef {
    owner: String,
    repo: String,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)] // Fields from GitHub API response
struct Release {
    id: u64,
    tag_name: String,
    html_url: String,
    upload_url: String,
    assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)] // Fields from GitHub API response
struct ReleaseAsset {
    id: u64,
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Serialize)]
struct CreateReleaseRequest<'a> {
    tag_name: &'a str,
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<&'a str>,
    draft: bool,
    prerelease: bool,
}

/// Publish a bundle file to GitHub releases.
pub fn publish_bundle(path: &Path, config: &GitHubConfig) -> Result<PublishResult> {
    let bytes = std::fs::read(path)
        .map_err(|err| MsError::Config(format!("read bundle {}: {err}", path.display())))?;
    let package = BundlePackage::from_bytes(&bytes)?;
    package.verify()?;

    let repo = parse_repo(&config.repo)?;
    let (owner, repo_name) = (repo.owner, repo.repo);
    let token = config.token.clone().or_else(token_from_env);
    if token.is_none() {
        return Err(MsError::ValidationFailed(
            "GitHub token required for bundle publish".to_string(),
        ));
    }

    let version = package.manifest.bundle.version.clone();
    let tag = config.tag.clone().unwrap_or_else(|| format!("v{version}"));
    let release_name = format!(
        "{} v{}",
        package.manifest.bundle.name, package.manifest.bundle.version
    );
    let asset_name = config
        .asset_name
        .clone()
        .or_else(|| {
            path.file_name()
                .and_then(|s| s.to_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| format!("{}.msb", package.manifest.bundle.id));

    let client = GitHubClient::new(token);
    let release = match client.get_release_by_tag(&owner, &repo_name, &tag)? {
        Some(release) => release,
        None => client.create_release(
            &owner,
            &repo_name,
            &CreateReleaseRequest {
                tag_name: &tag,
                name: &release_name,
                body: None,
                draft: config.draft,
                prerelease: config.prerelease,
            },
        )?,
    };

    if let Some(existing) = release.assets.iter().find(|asset| asset.name == asset_name) {
        client.delete_release_asset(&owner, &repo_name, existing.id)?;
    }

    let upload_url = normalize_upload_url(&release.upload_url);
    client.upload_release_asset(&upload_url, &asset_name, &bytes)?;

    Ok(PublishResult {
        repo: format!("{owner}/{repo_name}"),
        release_url: release.html_url,
        asset_name,
        tag,
    })
}

/// Download bundle bytes from a GitHub release.
pub fn download_bundle(
    repo: &str,
    tag: Option<&str>,
    asset_name: Option<&str>,
    token: Option<String>,
) -> Result<DownloadResult> {
    let repo = parse_repo(repo)?;
    let (owner, repo_name) = (repo.owner, repo.repo);
    let token = token.or_else(token_from_env);

    let client = GitHubClient::new(token);
    let release = match tag {
        Some(tag) => client
            .get_release_by_tag(&owner, &repo_name, tag)?
            .ok_or_else(|| MsError::ValidationFailed(format!("release tag not found: {tag}")))?,
        None => client.get_latest_release(&owner, &repo_name)?,
    };

    let asset = select_asset(&release.assets, asset_name)?;
    let bytes = client.download_release_asset(&owner, &repo_name, asset.id)?;

    Ok(DownloadResult {
        repo: format!("{owner}/{repo_name}"),
        asset_name: asset.name,
        tag: release.tag_name,
        bytes,
    })
}

/// Download bytes from a direct URL.
///
/// Only sends authentication token if the URL is a GitHub URL to prevent
/// token leakage to arbitrary servers.
pub fn download_url(url: &str, token: Option<String>) -> Result<Vec<u8>> {
    // Security: only send token to GitHub domains to prevent token leakage
    let safe_token = if is_github_url(url) {
        token.or_else(token_from_env)
    } else {
        None
    };
    let client = GitHubClient::new(safe_token);
    client.download_url(url)
}

/// Check if a URL is a GitHub URL where we should send auth tokens.
fn is_github_url(url: &str) -> bool {
    let url_lower = url.to_lowercase();
    url_lower.starts_with("https://github.com/")
        || url_lower.starts_with("https://api.github.com/")
        || url_lower.starts_with("https://raw.githubusercontent.com/")
        || url_lower.starts_with("https://objects.githubusercontent.com/")
}

fn select_asset(assets: &[ReleaseAsset], requested: Option<&str>) -> Result<ReleaseAsset> {
    if let Some(name) = requested {
        return assets
            .iter()
            .find(|asset| asset.name == name)
            .cloned()
            .ok_or_else(|| MsError::ValidationFailed(format!("bundle asset not found: {name}")));
    }

    let mut candidates = assets
        .iter()
        .filter(|asset| asset.name.ends_with(".msb"))
        .cloned()
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Err(MsError::ValidationFailed(
            "no .msb assets found in release".to_string(),
        ));
    }
    if candidates.len() > 1 {
        return Err(MsError::ValidationFailed(
            "multiple .msb assets found; specify --asset-name".to_string(),
        ));
    }
    Ok(candidates.remove(0))
}

fn parse_repo(input: &str) -> Result<RepoRef> {
    if let Some(stripped) = input.strip_prefix("https://github.com/") {
        return parse_repo(stripped);
    }
    if let Some(stripped) = input.strip_prefix("http://github.com/") {
        return parse_repo(stripped);
    }
    if let Some(stripped) = input.strip_prefix("github.com/") {
        return parse_repo(stripped);
    }
    let mut parts = input.split('/');
    let owner = parts.next().unwrap_or("").trim();
    let repo = parts.next().unwrap_or("").trim();
    if owner.is_empty() || repo.is_empty() {
        return Err(MsError::ValidationFailed(format!(
            "invalid repo reference: {input}"
        )));
    }
    for part in parts {
        if !part.trim().is_empty() {
            return Err(MsError::ValidationFailed(format!(
                "invalid repo reference: {input}"
            )));
        }
    }
    Ok(RepoRef {
        owner: owner.to_string(),
        repo: repo.trim_end_matches(".git").to_string(),
    })
}

fn normalize_upload_url(raw: &str) -> String {
    if let Some(idx) = raw.find("{?") {
        raw[..idx].to_string()
    } else if raw.starts_with(GH_UPLOADS) {
        raw.to_string()
    } else {
        raw.replace(GH_API, GH_UPLOADS)
    }
}

fn token_from_env() -> Option<String> {
    std::env::var("MS_GITHUB_TOKEN")
        .ok()
        .or_else(|| std::env::var("GITHUB_TOKEN").ok())
        .or_else(|| std::env::var("GH_TOKEN").ok())
}

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

    fn get_release_by_tag(&self, owner: &str, repo: &str, tag: &str) -> Result<Option<Release>> {
        let url = format!("{GH_API}/repos/{owner}/{repo}/releases/tags/{tag}");
        let response = self.get(&url)?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(MsError::ValidationFailed(format!(
                "get release failed: HTTP {}",
                response.status()
            )));
        }
        let release = response
            .json::<Release>()
            .map_err(|err| MsError::Config(format!("parse release response: {err}")))?;
        Ok(Some(release))
    }

    fn get_latest_release(&self, owner: &str, repo: &str) -> Result<Release> {
        let url = format!("{GH_API}/repos/{owner}/{repo}/releases/latest");
        let response = self.get(&url)?;
        parse_json_response(response, "latest release")
    }

    fn create_release(
        &self,
        owner: &str,
        repo: &str,
        request: &CreateReleaseRequest<'_>,
    ) -> Result<Release> {
        let url = format!("{GH_API}/repos/{owner}/{repo}/releases");
        let response = self.post_json(&url, request)?;
        parse_json_response(response, "create release")
    }

    fn delete_release_asset(&self, owner: &str, repo: &str, asset_id: u64) -> Result<()> {
        let url = format!("{GH_API}/repos/{owner}/{repo}/releases/assets/{asset_id}");
        let response = self.delete(&url)?;
        if !response.status().is_success() {
            return Err(MsError::ValidationFailed(format!(
                "failed to delete asset: HTTP {}",
                response.status()
            )));
        }
        Ok(())
    }

    fn upload_release_asset(&self, upload_url: &str, name: &str, bytes: &[u8]) -> Result<()> {
        let url = format!("{upload_url}?name={}", urlencoding::encode(name));
        let mut request = self.client.post(url).header("User-Agent", USER_AGENT);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request
            .header("Content-Type", "application/octet-stream")
            .body(bytes.to_vec())
            .send()
            .map_err(|err| MsError::Config(format!("upload asset failed: {err}")))?;

        if !response.status().is_success() {
            return Err(MsError::ValidationFailed(format!(
                "upload asset failed: HTTP {}",
                response.status()
            )));
        }
        Ok(())
    }

    fn download_url(&self, url: &str) -> Result<Vec<u8>> {
        use std::io::Read;

        let mut request = self.client.get(url).header("User-Agent", USER_AGENT);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request
            .send()
            .map_err(|err| MsError::Config(format!("download failed: {err}")))?;
        if !response.status().is_success() {
            return Err(MsError::ValidationFailed(format!(
                "download failed: HTTP {}",
                response.status()
            )));
        }

        // Check Content-Length if available to reject oversized downloads early
        if let Some(content_length) = response.content_length() {
            if content_length > MAX_DOWNLOAD_SIZE {
                return Err(MsError::ValidationFailed(format!(
                    "download too large: {} bytes (max {} MB)",
                    content_length,
                    MAX_DOWNLOAD_SIZE / (1024 * 1024)
                )));
            }
        }

        // Read with size limit to handle streaming responses without Content-Length
        let mut bytes = Vec::new();
        response
            .take(MAX_DOWNLOAD_SIZE + 1)
            .read_to_end(&mut bytes)
            .map_err(|err| MsError::Config(format!("download read failed: {err}")))?;

        if bytes.len() as u64 > MAX_DOWNLOAD_SIZE {
            return Err(MsError::ValidationFailed(format!(
                "download exceeded size limit ({} MB)",
                MAX_DOWNLOAD_SIZE / (1024 * 1024)
            )));
        }

        Ok(bytes)
    }

    fn download_release_asset(&self, owner: &str, repo: &str, asset_id: u64) -> Result<Vec<u8>> {
        use std::io::Read;

        let url = format!("{GH_API}/repos/{owner}/{repo}/releases/assets/{asset_id}");
        let mut request = self
            .client
            .get(url)
            .header("Accept", "application/octet-stream")
            .header("User-Agent", USER_AGENT);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request
            .send()
            .map_err(|err| MsError::Config(format!("download asset failed: {err}")))?;
        if !response.status().is_success() {
            return Err(MsError::ValidationFailed(format!(
                "download asset failed: HTTP {}",
                response.status()
            )));
        }

        // Check Content-Length if available to reject oversized downloads early
        if let Some(content_length) = response.content_length() {
            if content_length > MAX_DOWNLOAD_SIZE {
                return Err(MsError::ValidationFailed(format!(
                    "asset too large: {} bytes (max {} MB)",
                    content_length,
                    MAX_DOWNLOAD_SIZE / (1024 * 1024)
                )));
            }
        }

        // Read with size limit to handle streaming responses without Content-Length
        let mut bytes = Vec::new();
        response
            .take(MAX_DOWNLOAD_SIZE + 1)
            .read_to_end(&mut bytes)
            .map_err(|err| MsError::Config(format!("download asset read failed: {err}")))?;

        if bytes.len() as u64 > MAX_DOWNLOAD_SIZE {
            return Err(MsError::ValidationFailed(format!(
                "asset exceeded size limit ({} MB)",
                MAX_DOWNLOAD_SIZE / (1024 * 1024)
            )));
        }

        Ok(bytes)
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
            .map_err(|err| MsError::Config(format!("github request failed: {err}")))
    }

    fn post_json<T: Serialize + ?Sized>(
        &self,
        url: &str,
        payload: &T,
    ) -> Result<reqwest::blocking::Response> {
        let mut request = self
            .client
            .post(url)
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", USER_AGENT)
            .json(payload);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        request
            .send()
            .map_err(|err| MsError::Config(format!("github request failed: {err}")))
    }

    fn delete(&self, url: &str) -> Result<reqwest::blocking::Response> {
        let mut request = self
            .client
            .delete(url)
            .header("Accept", "application/vnd.github+json")
            .header("User-Agent", USER_AGENT);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        request
            .send()
            .map_err(|err| MsError::Config(format!("github request failed: {err}")))
    }
}

fn parse_json_response<T: for<'de> Deserialize<'de>>(
    response: reqwest::blocking::Response,
    label: &str,
) -> Result<T> {
    if !response.status().is_success() {
        return Err(MsError::ValidationFailed(format!(
            "{label} failed: HTTP {}",
            response.status()
        )));
    }
    response
        .json::<T>()
        .map_err(|err| MsError::Config(format!("{label} parse failed: {err}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_repo_accepts_basic() {
        let repo = parse_repo("owner/repo").unwrap();
        assert_eq!(repo.owner, "owner");
        assert_eq!(repo.repo, "repo");
    }

    #[test]
    fn parse_repo_strips_prefixes() {
        let repo = parse_repo("https://github.com/owner/repo.git").unwrap();
        assert_eq!(repo.owner, "owner");
        assert_eq!(repo.repo, "repo");
    }

    #[test]
    fn parse_repo_rejects_invalid() -> Result<()> {
        let err = parse_repo("invalid").unwrap_err();
        match err {
            MsError::ValidationFailed(msg) => {
                assert!(msg.contains("invalid repo"));
                Ok(())
            }
            other => Err(other),
        }
    }

    #[test]
    fn parse_repo_rejects_extra_segments() -> Result<()> {
        let err = parse_repo("owner/repo/extra").unwrap_err();
        match err {
            MsError::ValidationFailed(msg) => {
                assert!(msg.contains("invalid repo"));
                Ok(())
            }
            other => Err(other),
        }
    }

    #[test]
    fn normalize_upload_url_strips_template() {
        let raw = "https://uploads.github.com/repos/owner/repo/releases/1/assets{?name,label}";
        let normalized = normalize_upload_url(raw);
        assert_eq!(
            normalized,
            "https://uploads.github.com/repos/owner/repo/releases/1/assets"
        );
    }

    #[test]
    fn select_asset_prefers_named() {
        let assets = vec![
            ReleaseAsset {
                id: 1,
                name: "a.msb".to_string(),
                browser_download_url: "https://example.com/a".to_string(),
            },
            ReleaseAsset {
                id: 2,
                name: "b.msb".to_string(),
                browser_download_url: "https://example.com/b".to_string(),
            },
        ];
        let asset = select_asset(&assets, Some("b.msb")).unwrap();
        assert_eq!(asset.id, 2);
    }

    #[test]
    fn select_asset_requires_single_msb() -> Result<()> {
        let assets = vec![
            ReleaseAsset {
                id: 1,
                name: "a.msb".to_string(),
                browser_download_url: "https://example.com/a".to_string(),
            },
            ReleaseAsset {
                id: 2,
                name: "b.msb".to_string(),
                browser_download_url: "https://example.com/b".to_string(),
            },
        ];
        let err = select_asset(&assets, None).unwrap_err();
        match err {
            MsError::ValidationFailed(msg) => {
                assert!(msg.contains("multiple .msb assets"));
                Ok(())
            }
            other => Err(other),
        }
    }

    #[test]
    fn select_asset_single_msb_ok() {
        let assets = vec![
            ReleaseAsset {
                id: 1,
                name: "bundle.msb".to_string(),
                browser_download_url: "https://example.com/bundle".to_string(),
            },
            ReleaseAsset {
                id: 2,
                name: "notes.txt".to_string(),
                browser_download_url: "https://example.com/notes".to_string(),
            },
        ];
        let asset = select_asset(&assets, None).unwrap();
        assert_eq!(asset.id, 1);
    }

    #[test]
    fn is_github_url_accepts_github_domains() {
        assert!(is_github_url("https://github.com/owner/repo"));
        assert!(is_github_url("https://api.github.com/repos/owner/repo"));
        assert!(is_github_url(
            "https://raw.githubusercontent.com/owner/repo/main/file"
        ));
        assert!(is_github_url(
            "https://objects.githubusercontent.com/some/path"
        ));
        // Case insensitive
        assert!(is_github_url("HTTPS://GITHUB.COM/owner/repo"));
    }

    #[test]
    fn is_github_url_rejects_non_github() {
        assert!(!is_github_url("https://example.com/bundle.msb"));
        assert!(!is_github_url("https://evil.com/github.com/fake"));
        assert!(!is_github_url("http://github.com.evil.com/"));
        assert!(!is_github_url("https://notgithub.com/"));
    }
}
