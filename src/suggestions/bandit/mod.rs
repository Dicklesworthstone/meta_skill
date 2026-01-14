//! Suggestion signal bandit (Thompson sampling).

pub mod bandit;
pub mod context;
pub mod types;

pub use bandit::{BanditConfig, SignalBandit};
pub use context::{ContextKey, ContextModifier, ProjectSize, SuggestionContext, TimeOfDay};
pub use types::{BanditArm, BetaDistribution, Reward, SignalType, SignalWeights};
