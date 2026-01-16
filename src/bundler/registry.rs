//! Installed bundle registry for tracking bundle installations.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{MsError, Result};

/// Information about an installed bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledBundle {
    pub id: String,
    pub version: String,
    pub source: InstallSource,
    pub installed_at: DateTime<Utc>,
    pub skills: Vec<String>,
    pub checksum: Option<String>,
}

/// Source from which a bundle was installed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InstallSource {
    GitHub {
        repo: String,
        tag: Option<String>,
        asset: Option<String>,
    },
    File {
        path: String,
    },
    Url {
        url: String,
    },
}

impl std::fmt::Display for InstallSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GitHub { repo, tag, .. } => {
                write!(f, "github:{repo}")?;
                if let Some(t) = tag {
                    write!(f, "@{t}")?;
                }
                Ok(())
            }
            Self::File { path } => write!(f, "file:{path}"),
            Self::Url { url } => write!(f, "{url}"),
        }
    }
}

/// Parsed install source from user input.
#[derive(Debug, Clone)]
pub struct ParsedSource {
    pub source: InstallSource,
    pub asset_name: Option<String>,
}

impl ParsedSource {
    /// Parse a source string into a structured source.
    ///
    /// Supported formats:
    /// - `github:owner/repo` - GitHub repo, latest release
    /// - `github:owner/repo@tag` - GitHub repo, specific tag
    /// - `github:owner/repo/asset` - GitHub repo, specific asset name
    /// - `github:owner/repo/asset@tag` - GitHub repo, specific asset and tag
    /// - `owner/repo` - GitHub shorthand
    /// - `owner/repo@tag` - GitHub shorthand with tag
    /// - `http://...` or `https://...` - Direct URL
    /// - `./path` or `../path` or `/path` or `~/path` - Local file
    pub fn parse(input: &str) -> Result<Self> {
        // GitHub explicit prefix
        if let Some(rest) = input.strip_prefix("github:") {
            return Self::parse_github(rest);
        }

        // URL
        if input.starts_with("http://") || input.starts_with("https://") {
            return Ok(Self {
                source: InstallSource::Url {
                    url: input.to_string(),
                },
                asset_name: None,
            });
        }

        // Local path
        if Self::looks_like_path(input) {
            let expanded = Self::expand_path(input);
            return Ok(Self {
                source: InstallSource::File {
                    path: expanded.display().to_string(),
                },
                asset_name: None,
            });
        }

        // GitHub shorthand (owner/repo or owner/repo@tag)
        if input.contains('/') {
            return Self::parse_github(input);
        }

        // Unknown format - treat as local path
        Ok(Self {
            source: InstallSource::File {
                path: input.to_string(),
            },
            asset_name: None,
        })
    }

    fn parse_github(input: &str) -> Result<Self> {
        // Split on @ for tag
        let (path_part, tag) = if let Some((p, t)) = input.split_once('@') {
            (p, Some(t.to_string()))
        } else {
            (input, None)
        };

        // Split path into components: owner/repo[/asset]
        let parts: Vec<&str> = path_part.split('/').collect();

        if parts.len() < 2 {
            return Err(MsError::ValidationFailed(format!(
                "invalid GitHub source: {input} (expected owner/repo)"
            )));
        }

        let owner = parts[0];
        let repo = parts[1];
        let asset = if parts.len() > 2 {
            Some(parts[2..].join("/"))
        } else {
            None
        };

        Ok(Self {
            source: InstallSource::GitHub {
                repo: format!("{owner}/{repo}"),
                tag,
                asset: asset.clone(),
            },
            asset_name: asset,
        })
    }

    fn looks_like_path(input: &str) -> bool {
        input == "~"
            || input.starts_with("~/")
            || input.starts_with("./")
            || input.starts_with("../")
            || input.starts_with('/')
    }

    fn expand_path(input: &str) -> PathBuf {
        if input == "~" {
            if let Some(home) = dirs::home_dir() {
                return home;
            }
        }
        if let Some(stripped) = input.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(stripped);
            }
        }
        PathBuf::from(input)
    }
}

/// Registry for tracking installed bundles.
pub struct BundleRegistry {
    path: PathBuf,
    bundles: HashMap<String, InstalledBundle>,
}

impl BundleRegistry {
    const REGISTRY_FILE: &'static str = "installed_bundles.json";

    /// Open or create the bundle registry at the given root.
    pub fn open(root: &Path) -> Result<Self> {
        let bundles_dir = root.join("bundles");
        std::fs::create_dir_all(&bundles_dir)?;

        let path = bundles_dir.join(Self::REGISTRY_FILE);
        let bundles = if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            serde_json::from_str(&content)
                .map_err(|e| MsError::ValidationFailed(format!("invalid bundle registry: {e}")))?
        } else {
            HashMap::new()
        };

        Ok(Self { path, bundles })
    }

    /// Register an installed bundle.
    pub fn register(&mut self, bundle: InstalledBundle) -> Result<()> {
        self.bundles.insert(bundle.id.clone(), bundle);
        self.save()
    }

    /// Remove a bundle from the registry.
    pub fn unregister(&mut self, id: &str) -> Result<Option<InstalledBundle>> {
        let removed = self.bundles.remove(id);
        if removed.is_some() {
            self.save()?;
        }
        Ok(removed)
    }

    /// Get an installed bundle by ID.
    #[must_use] 
    pub fn get(&self, id: &str) -> Option<&InstalledBundle> {
        self.bundles.get(id)
    }

    /// List all installed bundles.
    pub fn list(&self) -> impl Iterator<Item = &InstalledBundle> {
        self.bundles.values()
    }

    /// Check if a bundle is installed.
    #[must_use] 
    pub fn is_installed(&self, id: &str) -> bool {
        self.bundles.contains_key(id)
    }

    fn save(&self) -> Result<()> {
        use std::io::Write;

        let content = serde_json::to_string_pretty(&self.bundles)
            .map_err(|e| MsError::Config(format!("serialize bundle registry: {e}")))?;

        // Atomic write: write to temp file, sync, then rename
        let temp_path = self.path.with_extension("json.tmp");
        let mut file = std::fs::File::create(&temp_path)
            .map_err(|e| MsError::Config(format!("create temp registry file: {e}")))?;
        file.write_all(content.as_bytes())
            .map_err(|e| MsError::Config(format!("write temp registry file: {e}")))?;
        file.sync_all()
            .map_err(|e| MsError::Config(format!("sync temp registry file: {e}")))?;
        drop(file);

        match std::fs::rename(&temp_path, &self.path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                // Windows does not allow renaming over an existing file.
                std::fs::remove_file(&self.path).map_err(|e| {
                    MsError::Config(format!("remove existing registry file: {e}"))
                })?;
                if let Err(err) = std::fs::rename(&temp_path, &self.path) {
                    let _ = std::fs::remove_file(&temp_path);
                    return Err(MsError::Config(format!("rename registry file: {err}")));
                }
            }
            Err(err) => {
                // Clean up temp file on rename failure
                let _ = std::fs::remove_file(&temp_path);
                return Err(MsError::Config(format!(
                    "rename registry file: {err}"
                )));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_github_prefix() {
        let parsed = ParsedSource::parse("github:owner/repo").unwrap();
        match parsed.source {
            InstallSource::GitHub { repo, tag, .. } => {
                assert_eq!(repo, "owner/repo");
                assert!(tag.is_none());
            }
            _ => panic!("expected GitHub source"),
        }
    }

    #[test]
    fn parse_github_with_tag() {
        let parsed = ParsedSource::parse("github:owner/repo@v1.0.0").unwrap();
        match parsed.source {
            InstallSource::GitHub { repo, tag, .. } => {
                assert_eq!(repo, "owner/repo");
                assert_eq!(tag, Some("v1.0.0".to_string()));
            }
            _ => panic!("expected GitHub source"),
        }
    }

    #[test]
    fn parse_github_with_asset() {
        let parsed = ParsedSource::parse("github:owner/repo/my-bundle").unwrap();
        match &parsed.source {
            InstallSource::GitHub { repo, asset, .. } => {
                assert_eq!(repo, "owner/repo");
                assert_eq!(asset, &Some("my-bundle".to_string()));
            }
            _ => panic!("expected GitHub source"),
        }
        assert_eq!(parsed.asset_name, Some("my-bundle".to_string()));
    }

    #[test]
    fn parse_github_shorthand() {
        let parsed = ParsedSource::parse("owner/repo").unwrap();
        match parsed.source {
            InstallSource::GitHub { repo, .. } => {
                assert_eq!(repo, "owner/repo");
            }
            _ => panic!("expected GitHub source"),
        }
    }

    #[test]
    fn parse_url() {
        let parsed = ParsedSource::parse("https://example.com/bundle.msb").unwrap();
        match parsed.source {
            InstallSource::Url { url } => {
                assert_eq!(url, "https://example.com/bundle.msb");
            }
            _ => panic!("expected URL source"),
        }
    }

    #[test]
    fn parse_local_path() {
        let parsed = ParsedSource::parse("./local/bundle.msb").unwrap();
        match parsed.source {
            InstallSource::File { path } => {
                assert_eq!(path, "./local/bundle.msb");
            }
            _ => panic!("expected File source"),
        }
    }

    #[test]
    fn test_ambiguous_local_path_parses_as_github() {
        // "skills/bundle.msb" looks like "owner/repo"
        let parsed = ParsedSource::parse("skills/bundle.msb").unwrap();
        match parsed.source {
            InstallSource::GitHub { repo, .. } => {
                assert_eq!(repo, "skills/bundle.msb");
            }
            _ => panic!("expected GitHub source for ambiguous path"),
        }
    }
}
