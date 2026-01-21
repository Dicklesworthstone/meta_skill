use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{MsError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineIdentity {
    pub machine_id: String,
    pub machine_name: String,
    #[serde(default)]
    pub sync_timestamps: HashMap<String, DateTime<Utc>>,
    pub metadata: MachineMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineMetadata {
    pub os: String,
    pub hostname: String,
    pub registered_at: DateTime<Utc>,
    #[serde(default)]
    pub description: Option<String>,
}

impl MachineIdentity {
    #[must_use]
    pub fn generate(machine_name: String, description: Option<String>) -> Self {
        let hostname = hostname::get().map_or_else(
            |_| "unknown".to_string(),
            |h| h.to_string_lossy().to_string(),
        );
        Self {
            machine_id: Uuid::new_v4().to_string(),
            machine_name,
            sync_timestamps: HashMap::new(),
            metadata: MachineMetadata {
                os: std::env::consts::OS.to_string(),
                hostname,
                registered_at: Utc::now(),
                description,
            },
        }
    }

    pub fn load_or_generate_with_name(
        name_override: Option<String>,
        description: Option<String>,
    ) -> Result<Self> {
        let path = Self::identity_path()?;
        if path.exists() {
            let contents = std::fs::read_to_string(&path).map_err(|err| {
                MsError::Config(format!("read machine identity {}: {err}", path.display()))
            })?;
            let mut identity: Self = serde_json::from_str(&contents)?;
            let mut changed = false;
            if let Some(name) = name_override {
                if identity.machine_name != name {
                    identity.machine_name = name;
                    changed = true;
                }
            }
            if let Some(desc) = description {
                if identity.metadata.description.as_deref() != Some(desc.as_str()) {
                    identity.metadata.description = Some(desc);
                    changed = true;
                }
            }
            if changed {
                identity.save()?;
            }
            Ok(identity)
        } else {
            let name = name_override.unwrap_or_else(Self::default_machine_name);
            let identity = Self::generate(name, description);
            identity.save()?;
            Ok(identity)
        }
    }

    pub fn record_sync(&mut self, remote_name: &str) {
        self.sync_timestamps
            .insert(remote_name.to_string(), Utc::now());
    }

    pub fn rename(&mut self, new_name: String) {
        self.machine_name = new_name;
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::identity_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| MsError::Config(format!("create identity dir: {err}")))?;
        }
        let contents = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, contents)
            .map_err(|err| MsError::Config(format!("write identity {}: {err}", path.display())))?;
        Ok(())
    }

    pub fn identity_path() -> Result<PathBuf> {
        let base = dirs::config_dir()
            .ok_or_else(|| MsError::MissingConfig("config directory not found".to_string()))?;
        Ok(base.join("ms").join("machine_identity.json"))
    }

    fn default_machine_name() -> String {
        hostname::get().map_or_else(
            |_| "default-machine".to_string(),
            |h| h.to_string_lossy().to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_sets_fields() {
        let identity = MachineIdentity::generate("test-machine".to_string(), None);
        assert!(!identity.machine_id.is_empty());
        assert_eq!(identity.machine_name, "test-machine");
        assert!(!identity.metadata.hostname.is_empty());
    }
}
