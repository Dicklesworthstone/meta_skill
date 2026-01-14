//! Suggestion utilities (cooldowns, fingerprints).

pub mod cooldown;
pub mod cooldown_storage;
pub mod bandit;

pub use cooldown::{
    CooldownStatus, SuggestionCooldownCache, SuggestionResponse, CooldownStats,
};
pub use bandit::{BanditConfig, SignalBandit};
