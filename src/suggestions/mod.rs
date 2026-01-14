//! Suggestion utilities (cooldowns, fingerprints).

pub mod cooldown;
pub mod cooldown_storage;

pub use cooldown::{
    CooldownStatus, SuggestionCooldownCache, SuggestionResponse, CooldownStats,
};
