//! Reward computation for skill feedback signals.
//!
//! Converts various feedback signals into normalized reward values (0.0-1.0)
//! for the contextual bandit to learn from.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Feedback signal for a skill recommendation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SkillFeedback {
    /// Skill was explicitly marked as helpful.
    ExplicitHelpful,

    /// Skill was loaded and used for a significant duration.
    UsedDuration {
        /// Duration the skill was actively used.
        minutes: u32,
    },

    /// Skill was loaded but not interacted with much.
    LoadedOnly,

    /// Skill was suggested but not loaded (ignored in suggestions).
    Ignored,

    /// Skill was explicitly marked as not helpful.
    ExplicitNotHelpful {
        /// Optional reason for why it wasn't helpful.
        reason: Option<String>,
    },

    /// Skill was loaded but quickly unloaded (negative signal).
    UnloadedQuickly,

    /// Numeric rating (1-5 stars).
    Rating {
        /// Rating value (1-5).
        stars: u8,
    },

    /// Task completion signal.
    TaskCompleted {
        /// Whether the task was successful.
        success: bool,
        /// Time to completion.
        duration: Duration,
    },
}

impl SkillFeedback {
    /// Create an explicit helpful feedback.
    #[must_use]
    pub const fn helpful() -> Self {
        Self::ExplicitHelpful
    }

    /// Create a used duration feedback.
    #[must_use]
    pub const fn used(minutes: u32) -> Self {
        Self::UsedDuration { minutes }
    }

    /// Create an ignored feedback.
    #[must_use]
    pub const fn ignored() -> Self {
        Self::Ignored
    }

    /// Create an explicit not helpful feedback.
    #[must_use]
    pub fn not_helpful(reason: Option<String>) -> Self {
        Self::ExplicitNotHelpful { reason }
    }

    /// Create a rating feedback.
    #[must_use]
    pub const fn rating(stars: u8) -> Self {
        Self::Rating { stars }
    }
}

/// Compute reward value (0.0-1.0) from feedback signal.
///
/// # Reward Scale
/// - 1.0: Explicit positive feedback
/// - 0.8: Extended use (>5 minutes)
/// - 0.4-0.8: Usage duration scaled
/// - 0.3: Loaded but minimal interaction
/// - 0.1: Ignored in suggestions
/// - 0.0: Explicit negative or quick unload
#[must_use]
pub fn compute_reward(feedback: &SkillFeedback) -> f32 {
    match feedback {
        // Explicit positive signals
        SkillFeedback::ExplicitHelpful => 1.0,

        // Duration-based signals
        SkillFeedback::UsedDuration { minutes } if *minutes > 5 => 0.8,
        SkillFeedback::UsedDuration { minutes } => {
            // Scale from 0.4 to 0.8 based on minutes (0-5)
            0.4 + (*minutes as f32 / 5.0).min(1.0) * 0.4
        }

        // Minimal engagement
        SkillFeedback::LoadedOnly => 0.3,

        // Ignored (weak negative signal, not 0 since there could be valid reasons)
        SkillFeedback::Ignored => 0.1,

        // Explicit negative signals
        SkillFeedback::ExplicitNotHelpful { .. } => 0.0,
        SkillFeedback::UnloadedQuickly => 0.0,

        // Rating-based signals (1-5 stars -> 0.0-1.0)
        SkillFeedback::Rating { stars } => (*stars as f32 - 1.0) / 4.0,

        // Task completion signals
        SkillFeedback::TaskCompleted { success, duration } => {
            if *success {
                // Faster completion = higher reward (up to 0.9)
                let time_bonus = (1.0 - (duration.as_secs_f32() / 600.0).min(1.0)) * 0.2;
                0.7 + time_bonus
            } else {
                0.2 // Failure, but at least they tried
            }
        }
    }
}

/// Compute reward with context-specific adjustments.
///
/// # Arguments
/// * `feedback` - The feedback signal
/// * `context_match` - How well the skill matched the context (0.0-1.0)
/// * `was_top_suggestion` - Whether this was the top-ranked suggestion
#[must_use]
pub fn compute_contextual_reward(
    feedback: &SkillFeedback,
    context_match: f32,
    was_top_suggestion: bool,
) -> f32 {
    let base_reward = compute_reward(feedback);

    // Context match bonus/penalty
    let context_factor = if context_match > 0.7 {
        1.1 // Slight boost for high context match
    } else if context_match < 0.3 {
        0.9 // Slight penalty for low context match
    } else {
        1.0
    };

    // Top suggestion bonus (we want to reward good top suggestions more)
    let position_factor = if was_top_suggestion && base_reward > 0.5 {
        1.1
    } else {
        1.0
    };

    (base_reward * context_factor * position_factor).min(1.0)
}

/// Aggregate multiple feedback signals into a single reward.
///
/// Uses weighted average favoring explicit signals.
#[must_use]
pub fn aggregate_rewards(feedbacks: &[SkillFeedback]) -> f32 {
    if feedbacks.is_empty() {
        return 0.5; // Prior
    }

    let (weighted_sum, total_weight) =
        feedbacks
            .iter()
            .fold((0.0f32, 0.0f32), |(sum, weight), feedback| {
                let reward = compute_reward(feedback);
                let signal_weight = feedback_weight(feedback);
                (sum + reward * signal_weight, weight + signal_weight)
            });

    if total_weight > 0.0 {
        weighted_sum / total_weight
    } else {
        0.5
    }
}

/// Get the weight for a feedback signal (explicit signals weighted more).
fn feedback_weight(feedback: &SkillFeedback) -> f32 {
    match feedback {
        SkillFeedback::ExplicitHelpful | SkillFeedback::ExplicitNotHelpful { .. } => 2.0,
        SkillFeedback::Rating { .. } => 1.5,
        SkillFeedback::TaskCompleted { .. } => 1.5,
        SkillFeedback::UsedDuration { .. } => 1.0,
        SkillFeedback::LoadedOnly | SkillFeedback::UnloadedQuickly => 0.8,
        SkillFeedback::Ignored => 0.5,
    }
}

/// Convert the old Reward enum to a numeric value for compatibility.
pub fn legacy_reward_to_f32(reward: &super::types::Reward) -> f32 {
    match reward {
        super::types::Reward::Success => 1.0,
        super::types::Reward::Failure => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_explicit_helpful_reward() {
        let reward = compute_reward(&SkillFeedback::ExplicitHelpful);
        assert!((reward - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_explicit_not_helpful_reward() {
        let reward = compute_reward(&SkillFeedback::ExplicitNotHelpful { reason: None });
        assert!(reward < 0.001);
    }

    #[test]
    fn test_duration_reward_scaling() {
        let short = compute_reward(&SkillFeedback::UsedDuration { minutes: 1 });
        let medium = compute_reward(&SkillFeedback::UsedDuration { minutes: 3 });
        let long = compute_reward(&SkillFeedback::UsedDuration { minutes: 10 });

        assert!(short < medium);
        assert!(medium < long);
        assert!(long >= 0.8);
    }

    #[test]
    fn test_rating_reward() {
        let one_star = compute_reward(&SkillFeedback::Rating { stars: 1 });
        let three_star = compute_reward(&SkillFeedback::Rating { stars: 3 });
        let five_star = compute_reward(&SkillFeedback::Rating { stars: 5 });

        assert!(one_star < 0.01);
        assert!((three_star - 0.5).abs() < 0.01);
        assert!((five_star - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_task_completion_reward() {
        let success = compute_reward(&SkillFeedback::TaskCompleted {
            success: true,
            duration: Duration::from_secs(60),
        });
        let failure = compute_reward(&SkillFeedback::TaskCompleted {
            success: false,
            duration: Duration::from_secs(60),
        });

        assert!(success > 0.7);
        assert!(failure < 0.3);
    }

    #[test]
    fn test_contextual_reward_boost() {
        let _base = compute_reward(&SkillFeedback::ExplicitHelpful);
        let boosted = compute_contextual_reward(&SkillFeedback::ExplicitHelpful, 0.9, true);

        // Should be capped at 1.0
        assert!((boosted - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_aggregate_rewards() {
        let feedbacks = vec![
            SkillFeedback::ExplicitHelpful,
            SkillFeedback::UsedDuration { minutes: 3 },
        ];

        let agg = aggregate_rewards(&feedbacks);
        // Should be weighted toward the explicit helpful signal
        assert!(agg > 0.8);
    }

    #[test]
    fn test_aggregate_empty() {
        let agg = aggregate_rewards(&[]);
        assert!((agg - 0.5).abs() < 0.01); // Prior
    }

    #[test]
    fn test_ignored_reward() {
        let reward = compute_reward(&SkillFeedback::Ignored);
        assert!((reward - 0.1).abs() < 0.01);
    }

    #[test]
    fn test_loaded_only_reward() {
        let reward = compute_reward(&SkillFeedback::LoadedOnly);
        assert!((reward - 0.3).abs() < 0.01);
    }
}
