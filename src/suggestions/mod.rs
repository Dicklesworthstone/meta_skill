//! Suggestion utilities (cooldowns, fingerprints).

pub mod bandit;
pub mod cooldown;
pub mod cooldown_storage;

pub use bandit::{BanditConfig, SignalBandit};
pub use cooldown::{CooldownStats, CooldownStatus, SuggestionCooldownCache, SuggestionResponse};
