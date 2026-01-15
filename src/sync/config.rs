use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{MsError, Result};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ConflictStrategy {
    PreferLocal,
    PreferRemote,
    PreferNewest,
    KeepBoth,
}

impl Default for ConflictStrategy {
    fn default() -> Self {
        Self::PreferNewest
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteType {
    FileSystem,
    Git,
}

impl RemoteType {
    pub fn from_str(value: &str) -> Result<Self> {
        match value {
            "filesystem" | "fs" => Ok(Self::FileSystem),
            "git" => Ok(Self::Git),
            _ => Err(MsError::Config(format!(
                "unknown remote type: {value} (use filesystem|git)"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SyncDirection {
    PullOnly,
    PushOnly,
    Bidirectional,
}

impl Default for SyncDirection {
    fn default() -> Self {
        Self::Bidirectional
    }
}

impl SyncDirection {
    pub fn from_str(value: &str) -> Result<Self> {
        match value {
            "pull-only" | "pull" => Ok(Self::PullOnly),
            "push-only" | "push" => Ok(Self::PushOnly),
            "bidirectional" | "bi" | "both" => Ok(Self::Bidirectional),
            _ => Err(MsError::Config(format!(
                "unknown sync direction: {value} (use pull-only|push-only|bidirectional)"
            ))),
        }
    }

    pub fn allows_pull(self) -> bool {
        matches!(self, Self::PullOnly | Self::Bidirectional)
    }

    pub fn allows_push(self) -> bool {
        matches!(self, Self::PushOnly | Self::Bidirectional)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineConfig {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

impl Default for MachineConfig {
    fn default() -> Self {
        Self {
            name: None,
            description: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncSettings {
    #[serde(default)]
    pub default_conflict_strategy: ConflictStrategy,
    #[serde(default)]
    pub auto_sync_on_change: bool,
    #[serde(default)]
    pub auto_sync_interval_minutes: u32,
    #[serde(default)]
    pub sync_on_startup: bool,
    #[serde(default = "default_sync_skills")]
    pub sync_skills: bool,
    #[serde(default)]
    pub sync_bundles: bool,
}

fn default_sync_skills() -> bool {
    true
}

impl Default for SyncSettings {
    fn default() -> Self {
        Self {
            default_conflict_strategy: ConflictStrategy::default(),
            auto_sync_on_change: false,
            auto_sync_interval_minutes: 0,
            sync_on_startup: false,
            sync_skills: true,
            sync_bundles: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteConfig {
    pub name: String,
    #[serde(rename = "type")]
    pub remote_type: RemoteType,
    pub url: String,
    #[serde(default = "default_remote_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub direction: SyncDirection,
    #[serde(default)]
    pub auto_sync: bool,
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    #[serde(default)]
    pub include_patterns: Vec<String>,
}

fn default_remote_enabled() -> bool {
    true
}

impl RemoteConfig {
    pub fn new(name: impl Into<String>, remote_type: RemoteType, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            remote_type,
            url: url.into(),
            enabled: true,
            direction: SyncDirection::default(),
            auto_sync: false,
            exclude_patterns: Vec::new(),
            include_patterns: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    #[serde(default)]
    pub machine: MachineConfig,
    #[serde(default)]
    pub sync: SyncSettings,
    #[serde(default)]
    pub remotes: Vec<RemoteConfig>,
    #[serde(default)]
    pub conflict_strategies: HashMap<String, ConflictStrategy>,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            machine: MachineConfig::default(),
            sync: SyncSettings::default(),
            remotes: Vec::new(),
            conflict_strategies: HashMap::new(),
        }
    }
}

impl SyncConfig {
    pub fn path() -> Result<PathBuf> {
        let base = dirs::config_dir()
            .ok_or_else(|| MsError::MissingConfig("config directory not found".to_string()))?;
        Ok(base.join("ms").join("sync.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        Self::load_from(&path)
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(path).map_err(|err| {
            MsError::Config(format!("read sync config {}: {err}", path.display()))
        })?;
        let config: Self = toml::from_str(&contents).map_err(|err| {
            MsError::Config(format!("parse sync config {}: {err}", path.display()))
        })?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        self.save_to(&path)
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| MsError::Config(format!("create sync config dir: {err}")))?;
        }
        let rendered = toml::to_string_pretty(self)
            .map_err(|err| MsError::Config(format!("render sync config: {err}")))?;
        std::fs::write(path, rendered).map_err(|err| {
            MsError::Config(format!("write sync config {}: {err}", path.display()))
        })?;
        Ok(())
    }

    pub fn upsert_remote(&mut self, remote: RemoteConfig) {
        if let Some(existing) = self.remotes.iter_mut().find(|r| r.name == remote.name) {
            *existing = remote;
        } else {
            self.remotes.push(remote);
        }
    }

    pub fn remove_remote(&mut self, name: &str) -> bool {
        let before = self.remotes.len();
        self.remotes.retain(|r| r.name != name);
        before != self.remotes.len()
    }

    pub fn remote(&self, name: &str) -> Option<&RemoteConfig> {
        self.remotes.iter().find(|r| r.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_sync_config_has_sections() {
        let config = SyncConfig::default();
        assert!(config.remotes.is_empty());
        assert!(matches!(
            config.sync.default_conflict_strategy,
            ConflictStrategy::PreferNewest
        ));
        assert!(config.sync.sync_skills);
    }

    #[test]
    fn remote_roundtrip() {
        let mut config = SyncConfig::default();
        config.upsert_remote(RemoteConfig {
            name: "origin".to_string(),
            remote_type: RemoteType::FileSystem,
            url: "/tmp/skills".to_string(),
            enabled: true,
            direction: SyncDirection::PullOnly,
            auto_sync: false,
            exclude_patterns: vec!["draft-*".to_string()],
            include_patterns: vec![],
        });

        let rendered = toml::to_string_pretty(&config).unwrap();
        let parsed: SyncConfig = toml::from_str(&rendered).unwrap();
        assert_eq!(parsed.remotes.len(), 1);
        assert_eq!(parsed.remotes[0].name, "origin");
        assert_eq!(parsed.remotes[0].direction, SyncDirection::PullOnly);
    }
}
