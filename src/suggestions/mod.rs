//! Suggestion utilities (cooldowns, fingerprints, tracking, bandits).

pub mod bandit;
pub mod cooldown;
pub mod cooldown_storage;
pub mod tracking;

pub use bandit::{BanditConfig, SignalBandit};
pub use cooldown::{CooldownStats, CooldownStatus, SuggestionCooldownCache, SuggestionResponse};
pub use tracking::{
    FeedbackCollector, InteractionType, SessionEvent, SessionStats, SessionTracker,
    SkillInteraction, SkillSession, SuggestionOutcome, SuggestionRecord, SuggestionTracker,
};
