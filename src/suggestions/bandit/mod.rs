//! Suggestion bandits: signal-based and contextual multi-armed bandits.
//!
//! This module provides two types of bandits:
//! - `SignalBandit`: Selects weights for different signal types (BM25, embedding, etc.)
//! - `ContextualBandit`: Recommends skills based on contextual features

pub mod bandit;
pub mod context;
pub mod contextual;
pub mod features;
pub mod rewards;
pub mod types;

pub use bandit::{BanditConfig, SignalBandit};
pub use context::{ContextKey, ContextModifier, ProjectSize, SuggestionContext, TimeOfDay};
pub use contextual::{ContextualArm, ContextualBandit, ContextualBanditConfig, Recommendation};
pub use features::{ContextFeatures, DefaultFeatureExtractor, FeatureExtractor, UserHistory};
pub use rewards::{SkillFeedback, compute_reward};
pub use types::{BanditArm, BetaDistribution, Reward, SignalType, SignalWeights};
