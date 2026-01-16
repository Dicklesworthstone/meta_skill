//! Suggestion cooldown cache and helpers.

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::Result;

use super::cooldown_storage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestionResponse {
    pub fingerprint: u64,
    pub suggestions: Vec<String>,
    pub suppressed: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CooldownStatus {
    Active { remaining_seconds: u64 },
    Expired,
    NotFound,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CooldownEntry {
    pub fingerprint: u64,
    pub skill_id: String,
    pub suggested_at: DateTime<Utc>,
    pub cooldown_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CooldownStats {
    pub total_entries: usize,
    pub active_cooldowns: usize,
    pub expired_pending_cleanup: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SuggestionCooldownCache {
    entries: Vec<CooldownEntry>,
}

impl SuggestionCooldownCache {
    #[must_use] 
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        cooldown_storage::load_cache(path)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        cooldown_storage::save_cache(path, self)
    }

    #[must_use] 
    pub fn stats(&self) -> CooldownStats {
        let now = Utc::now();
        let mut active = 0usize;
        let mut expired = 0usize;
        for entry in &self.entries {
            if cooldown_remaining_seconds(entry, now) > 0 {
                active += 1;
            } else {
                expired += 1;
            }
        }
        CooldownStats {
            total_entries: self.entries.len(),
            active_cooldowns: active,
            expired_pending_cleanup: expired,
        }
    }

    #[must_use] 
    pub fn status(&self, fingerprint: u64, skill_id: &str) -> CooldownStatus {
        let now = Utc::now();
        let Some(entry) = self
            .entries
            .iter()
            .find(|entry| entry.fingerprint == fingerprint && entry.skill_id == skill_id)
        else {
            return CooldownStatus::NotFound;
        };
        let remaining = cooldown_remaining_seconds(entry, now);
        if remaining == 0 {
            CooldownStatus::Expired
        } else {
            CooldownStatus::Active {
                remaining_seconds: remaining,
            }
        }
    }

    pub fn record(&mut self, fingerprint: u64, skill_id: String, cooldown_seconds: u64) {
        let now = Utc::now();
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.fingerprint == fingerprint && entry.skill_id == skill_id)
        {
            entry.suggested_at = now;
            entry.cooldown_seconds = cooldown_seconds;
            return;
        }
        self.entries.push(CooldownEntry {
            fingerprint,
            skill_id,
            suggested_at: now,
            cooldown_seconds,
        });
    }

    pub fn purge_expired(&mut self) -> usize {
        let now = Utc::now();
        let before = self.entries.len();
        self.entries
            .retain(|entry| cooldown_remaining_seconds(entry, now) > 0);
        before.saturating_sub(self.entries.len())
    }
}

fn cooldown_remaining_seconds(entry: &CooldownEntry, now: DateTime<Utc>) -> u64 {
    let elapsed = now
        .signed_duration_since(entry.suggested_at)
        .num_seconds()
        .max(0);
    let cooldown = entry.cooldown_seconds as i64;
    if elapsed >= cooldown {
        0
    } else {
        (cooldown - elapsed) as u64
    }
}
