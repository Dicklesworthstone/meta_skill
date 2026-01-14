//! JSON storage for suggestion cooldown cache.

use std::fs;
use std::path::Path;

use crate::error::{MsError, Result};

use super::cooldown::SuggestionCooldownCache;

pub fn load_cache(path: &Path) -> Result<SuggestionCooldownCache> {
    if !path.exists() {
        return Ok(SuggestionCooldownCache::new());
    }
    let raw = fs::read_to_string(path)?;
    if raw.trim().is_empty() {
        return Ok(SuggestionCooldownCache::new());
    }
    let cache: SuggestionCooldownCache = serde_json::from_str(&raw)
        .map_err(|err| MsError::Serialization(format!("cooldown cache parse: {err}")))?;
    Ok(cache)
}

pub fn save_cache(path: &Path, cache: &SuggestionCooldownCache) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_string_pretty(cache)
        .map_err(|err| MsError::Serialization(format!("cooldown cache serialize: {err}")))?;
    fs::write(path, payload)?;
    Ok(())
}
