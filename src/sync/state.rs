use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{MsError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SkillSyncStatus {
    Synced,
    LocalAhead,
    RemoteAhead,
    Diverged,
    LocalOnly,
    RemoteOnly,
    Conflict,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSyncState {
    pub skill_id: String,
    #[serde(default)]
    pub local_hash: Option<String>,
    #[serde(default)]
    pub remote_hashes: HashMap<String, String>,
    #[serde(default)]
    pub local_modified: Option<DateTime<Utc>>,
    #[serde(default)]
    pub remote_modified: HashMap<String, DateTime<Utc>>,
    pub status: SkillSyncStatus,
    #[serde(default)]
    pub last_modified_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    #[serde(default)]
    pub skill_states: HashMap<String, SkillSyncState>,
    #[serde(default)]
    pub last_full_sync: HashMap<String, DateTime<Utc>>,
}

impl Default for SyncState {
    fn default() -> Self {
        Self {
            skill_states: HashMap::new(),
            last_full_sync: HashMap::new(),
        }
    }
}

impl SyncState {
    pub fn path(ms_root: &Path) -> PathBuf {
        ms_root.join("sync").join("state.json")
    }

    pub fn load(ms_root: &Path) -> Result<Self> {
        let path = Self::path(ms_root);
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = std::fs::read_to_string(&path)
            .map_err(|err| MsError::Config(format!("read sync state {}: {err}", path.display())))?;
        let state: Self = serde_json::from_str(&contents)?;
        Ok(state)
    }

    pub fn save(&self, ms_root: &Path) -> Result<()> {
        let path = Self::path(ms_root);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| MsError::Config(format!("create sync state dir: {err}")))?;
        }
        let rendered = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, rendered).map_err(|err| {
            MsError::Config(format!("write sync state {}: {err}", path.display()))
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_sync_state_empty() {
        let state = SyncState::default();
        assert!(state.skill_states.is_empty());
        assert!(state.last_full_sync.is_empty());
    }
}
