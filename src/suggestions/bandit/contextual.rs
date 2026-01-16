//! Contextual multi-armed bandit for skill recommendations.
//!
//! This module implements a contextual bandit that learns to recommend skills
//! based on context features (project type, time of day, activity patterns, etc.)
//! using Thompson sampling with linear contextual features.

use std::collections::HashMap;
use std::path::Path;

use rand::rng;
use rand_distr::{Beta, Distribution};
use serde::{Deserialize, Serialize};

use crate::error::{MsError, Result};

use super::features::ContextFeatures;
use super::rewards::SkillFeedback;

/// Configuration for the contextual bandit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextualBanditConfig {
    /// Base exploration rate for UCB bonus.
    pub exploration_rate: f32,

    /// Learning rate for gradient descent updates.
    pub learning_rate: f32,

    /// Minimum pulls before relying on learned weights.
    pub cold_start_threshold: u64,

    /// Regularization strength for weight updates.
    pub regularization: f32,

    /// Path for persistence.
    pub persistence_path: Option<std::path::PathBuf>,
}

impl Default for ContextualBanditConfig {
    fn default() -> Self {
        Self {
            exploration_rate: 0.1,
            learning_rate: 0.01,
            cold_start_threshold: 10,
            regularization: 0.001,
            persistence_path: None,
        }
    }
}

/// A contextual arm representing a skill with learned feature weights.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextualArm {
    /// Skill ID this arm represents.
    pub skill_id: String,

    /// Learned weights for context features.
    pub feature_weights: Vec<f32>,

    /// Beta distribution alpha parameter (successes).
    pub alpha: f32,

    /// Beta distribution beta parameter (failures).
    pub beta: f32,

    /// Total number of times this arm was pulled.
    pub pulls: u64,

    /// Total rewards received.
    pub total_reward: f64,

    /// Running average reward.
    pub avg_reward: f64,

    /// Last update timestamp.
    pub last_updated: Option<chrono::DateTime<chrono::Utc>>,
}

impl ContextualArm {
    /// Create a new arm for a skill with the given feature dimension.
    #[must_use]
    pub fn new(skill_id: &str, feature_dim: usize) -> Self {
        Self {
            skill_id: skill_id.to_string(),
            // Initialize weights to small random values
            feature_weights: vec![0.0; feature_dim],
            alpha: 1.0,
            beta: 1.0,
            pulls: 0,
            total_reward: 0.0,
            avg_reward: 0.5,
            last_updated: None,
        }
    }

    /// Sample from the Thompson posterior for exploration.
    fn thompson_sample(&self) -> f32 {
        let mut rng = rng();
        // Ensure valid Beta parameters
        let alpha = self.alpha.max(0.01);
        let beta_param = self.beta.max(0.01);

        match Beta::new(alpha, beta_param) {
            Ok(dist) => dist.sample(&mut rng),
            Err(_) => 0.5, // Fallback to neutral
        }
    }
}

/// A skill recommendation with score and explanation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    /// Recommended skill ID.
    pub skill_id: String,

    /// Predicted score (0.0-1.0).
    pub score: f32,

    /// Human-readable explanation of why this skill was recommended.
    pub reason: String,

    /// Component scores for transparency.
    pub components: RecommendationComponents,
}

/// Component scores for recommendation explanation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecommendationComponents {
    /// Score from contextual features.
    pub contextual_score: f32,

    /// Score from Thompson sampling.
    pub thompson_score: f32,

    /// Exploration bonus (UCB).
    pub exploration_bonus: f32,

    /// Number of times this skill has been recommended.
    pub pull_count: u64,

    /// Average reward from past interactions.
    pub avg_reward: f64,
}

/// Contextual multi-armed bandit for skill recommendations.
///
/// Uses Thompson sampling with linear contextual features to learn
/// which skills are most useful in different contexts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextualBandit {
    /// Per-skill arms with learned weights.
    arms: HashMap<String, ContextualArm>,

    /// Configuration.
    config: ContextualBanditConfig,

    /// Expected feature dimension.
    feature_dim: usize,

    /// Total recommendations made.
    total_recommendations: u64,

    /// Total updates received.
    total_updates: u64,
}

impl Default for ContextualBandit {
    fn default() -> Self {
        Self::new(ContextualBanditConfig::default(), 28) // Default feature dim
    }
}

impl ContextualBandit {
    /// Create a new contextual bandit.
    #[must_use]
    pub fn new(config: ContextualBanditConfig, feature_dim: usize) -> Self {
        Self {
            arms: HashMap::new(),
            config,
            feature_dim,
            total_recommendations: 0,
            total_updates: 0,
        }
    }

    /// Create a new bandit with default config and given feature dimension.
    #[must_use]
    pub fn with_feature_dim(feature_dim: usize) -> Self {
        Self::new(ContextualBanditConfig::default(), feature_dim)
    }

    /// Get or create an arm for a skill.
    fn get_or_create_arm(&mut self, skill_id: &str) -> &mut ContextualArm {
        if !self.arms.contains_key(skill_id) {
            self.arms.insert(
                skill_id.to_string(),
                ContextualArm::new(skill_id, self.feature_dim),
            );
        }
        self.arms.get_mut(skill_id).unwrap()
    }

    /// Sample a score for a skill given context features.
    ///
    /// Combines contextual prediction with Thompson sampling and exploration bonus.
    pub fn sample(&mut self, skill_id: &str, features: &ContextFeatures) -> f32 {
        // Copy config values to avoid borrow conflicts
        let cold_start_threshold = self.config.cold_start_threshold;
        let exploration_rate = self.config.exploration_rate;
        let total_recommendations = self.total_recommendations;

        let arm = self.get_or_create_arm(skill_id);

        // Contextual score from feature weights
        let contextual_score = sigmoid(features.dot(&arm.feature_weights));

        // Thompson sample for exploration
        let thompson_sample = arm.thompson_sample();

        // Combine: weighted average of contextual and thompson
        let combined = contextual_score * 0.7 + thompson_sample * 0.3;

        // Exploration bonus for under-explored arms (UCB-style)
        let exploration_bonus = if arm.pulls < cold_start_threshold {
            let exploration_factor = 1.0 - (arm.pulls as f32 / cold_start_threshold as f32);
            exploration_rate * exploration_factor
        } else {
            // UCB bonus decreases as we learn more
            let t = total_recommendations.max(1) as f32;
            let n = arm.pulls.max(1) as f32;
            exploration_rate * (2.0 * t.ln() / n).sqrt()
        };

        (combined + exploration_bonus).clamp(0.0, 1.0)
    }

    /// Get top-k skill recommendations for the given context.
    pub fn recommend(&mut self, features: &ContextFeatures, k: usize) -> Vec<Recommendation> {
        // Get all known skill IDs
        let skill_ids: Vec<String> = self.arms.keys().cloned().collect();

        if skill_ids.is_empty() {
            return vec![];
        }

        let mut recommendations: Vec<Recommendation> = skill_ids
            .iter()
            .map(|skill_id| {
                let score = self.sample(skill_id, features);
                let components = self.get_components(skill_id, features);
                let reason = self.explain_score(skill_id, &components);

                Recommendation {
                    skill_id: skill_id.clone(),
                    score,
                    reason,
                    components,
                }
            })
            .collect();

        // Sort by score descending
        recommendations.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        recommendations.truncate(k);
        self.total_recommendations += 1;

        recommendations
    }

    /// Register a skill so it can be recommended.
    pub fn register_skill(&mut self, skill_id: &str) {
        self.get_or_create_arm(skill_id);
    }

    /// Register multiple skills.
    pub fn register_skills(&mut self, skill_ids: &[String]) {
        for skill_id in skill_ids {
            self.register_skill(skill_id);
        }
    }

    /// Update the bandit with feedback for a skill.
    pub fn update(
        &mut self,
        skill_id: &str,
        features: &ContextFeatures,
        feedback: &SkillFeedback,
    ) {
        let reward = super::rewards::compute_reward(feedback);
        self.update_with_reward(skill_id, features, reward);
    }

    /// Update with a raw reward value.
    pub fn update_with_reward(
        &mut self,
        skill_id: &str,
        features: &ContextFeatures,
        reward: f32,
    ) {
        // Copy config values to avoid borrow conflicts
        let learning_rate = self.config.learning_rate;
        let regularization = self.config.regularization;

        let arm = self.get_or_create_arm(skill_id);

        // Update Beta distribution for Thompson sampling
        arm.alpha += reward;
        arm.beta += 1.0 - reward;

        // Update feature weights via gradient descent
        let feature_vec = features.as_vec();
        let predicted = sigmoid(arm.feature_weights.iter()
            .zip(feature_vec.iter())
            .map(|(w, f)| w * f)
            .sum::<f32>());

        let error = reward - predicted;

        // Gradient descent with L2 regularization
        for (w, f) in arm.feature_weights.iter_mut().zip(feature_vec.iter()) {
            let gradient = error * f - regularization * *w;
            *w += learning_rate * gradient;
        }

        // Update statistics
        arm.pulls += 1;
        arm.total_reward += reward as f64;
        arm.avg_reward = arm.total_reward / arm.pulls as f64;
        arm.last_updated = Some(chrono::Utc::now());

        self.total_updates += 1;
    }

    /// Get component scores for a skill.
    fn get_components(&self, skill_id: &str, features: &ContextFeatures) -> RecommendationComponents {
        let arm = match self.arms.get(skill_id) {
            Some(a) => a,
            None => return RecommendationComponents::default(),
        };

        let contextual_score = sigmoid(features.dot(&arm.feature_weights));
        let thompson_score = arm.thompson_sample();

        let exploration_bonus = if arm.pulls < self.config.cold_start_threshold {
            let factor = 1.0 - (arm.pulls as f32 / self.config.cold_start_threshold as f32);
            self.config.exploration_rate * factor
        } else {
            let t = self.total_recommendations.max(1) as f32;
            let n = arm.pulls.max(1) as f32;
            self.config.exploration_rate * (2.0 * t.ln() / n).sqrt()
        };

        RecommendationComponents {
            contextual_score,
            thompson_score,
            exploration_bonus,
            pull_count: arm.pulls,
            avg_reward: arm.avg_reward,
        }
    }

    /// Generate explanation for a recommendation.
    fn explain_score(&self, skill_id: &str, components: &RecommendationComponents) -> String {
        let arm = match self.arms.get(skill_id) {
            Some(a) => a,
            None => return "Unknown skill".to_string(),
        };

        let mut reasons = Vec::new();

        // Cold start explanation
        if arm.pulls < self.config.cold_start_threshold {
            reasons.push(format!(
                "exploring (only {} past uses)",
                arm.pulls
            ));
        }

        // Context match
        if components.contextual_score > 0.7 {
            reasons.push("strong context match".to_string());
        } else if components.contextual_score > 0.5 {
            reasons.push("good context match".to_string());
        }

        // Historical performance
        if arm.pulls >= self.config.cold_start_threshold {
            if components.avg_reward > 0.7 {
                reasons.push("historically helpful".to_string());
            } else if components.avg_reward < 0.3 {
                reasons.push("learning from feedback".to_string());
            }
        }

        // Thompson sampling
        if components.thompson_score > 0.8 {
            reasons.push("high exploration potential".to_string());
        }

        if reasons.is_empty() {
            "general recommendation".to_string()
        } else {
            reasons.join(", ")
        }
    }

    /// Save the bandit state to a file.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(MsError::Io)?;
        }

        let json = serde_json::to_string_pretty(self)?;
        let temp_path = path.with_extension("tmp");
        std::fs::write(&temp_path, json).map_err(MsError::Io)?;

        // Atomic rename
        match std::fs::rename(&temp_path, path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                std::fs::remove_file(path).map_err(MsError::Io)?;
                std::fs::rename(&temp_path, path).map_err(MsError::Io)?;
                Ok(())
            }
            Err(err) => {
                let _ = std::fs::remove_file(&temp_path);
                Err(MsError::Io(err))
            }
        }
    }

    /// Load bandit state from a file.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let contents = std::fs::read_to_string(path).map_err(MsError::Io)?;
        let bandit: Self = serde_json::from_str(&contents)?;
        Ok(bandit)
    }

    /// Get statistics about the bandit.
    #[must_use]
    pub fn stats(&self) -> BanditStats {
        let mut total_pulls = 0u64;
        let mut total_reward = 0.0f64;
        let mut cold_start_count = 0usize;

        for arm in self.arms.values() {
            total_pulls += arm.pulls;
            total_reward += arm.total_reward;
            if arm.pulls < self.config.cold_start_threshold {
                cold_start_count += 1;
            }
        }

        BanditStats {
            num_skills: self.arms.len(),
            total_recommendations: self.total_recommendations,
            total_updates: self.total_updates,
            total_pulls,
            avg_reward: if total_pulls > 0 {
                total_reward / total_pulls as f64
            } else {
                0.5
            },
            cold_start_skills: cold_start_count,
        }
    }

    /// Get the number of registered skills.
    #[must_use]
    pub fn num_skills(&self) -> usize {
        self.arms.len()
    }

    /// Check if a skill is registered.
    #[must_use]
    pub fn has_skill(&self, skill_id: &str) -> bool {
        self.arms.contains_key(skill_id)
    }
}

/// Statistics about the bandit state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BanditStats {
    /// Number of registered skills.
    pub num_skills: usize,

    /// Total recommendations made.
    pub total_recommendations: u64,

    /// Total feedback updates received.
    pub total_updates: u64,

    /// Total arm pulls.
    pub total_pulls: u64,

    /// Average reward across all arms.
    pub avg_reward: f64,

    /// Number of skills still in cold start phase.
    pub cold_start_skills: usize,
}

/// Sigmoid activation function.
fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_features() -> ContextFeatures {
        ContextFeatures {
            project_type: vec![1.0, 0.0, 0.0], // Rust
            time_features: vec![0.5, 0.5, 0.0, 1.0],
            activity_features: vec![0.3, 0.2, 1.0],
            history_features: vec![0.5, 0.8, 0.4],
        }
    }

    #[test]
    fn test_contextual_bandit_new() {
        let bandit = ContextualBandit::with_feature_dim(10);
        assert_eq!(bandit.num_skills(), 0);
        assert_eq!(bandit.feature_dim, 10);
    }

    #[test]
    fn test_register_skill() {
        let mut bandit = ContextualBandit::with_feature_dim(10);
        bandit.register_skill("rust-errors");

        assert!(bandit.has_skill("rust-errors"));
        assert_eq!(bandit.num_skills(), 1);
    }

    #[test]
    fn test_sample_cold_start() {
        let mut bandit = ContextualBandit::with_feature_dim(10);
        bandit.register_skill("rust-errors");

        let features = ContextFeatures {
            project_type: vec![1.0, 0.0],
            time_features: vec![0.5, 0.5],
            activity_features: vec![0.3, 0.2, 1.0],
            history_features: vec![0.5],
        };

        // Cold start should have exploration bonus
        let score = bandit.sample("rust-errors", &features);
        assert!(score >= 0.0 && score <= 1.0);
    }

    #[test]
    fn test_update_increases_pulls() {
        let mut bandit = ContextualBandit::with_feature_dim(10);
        bandit.register_skill("rust-errors");

        let features = sample_features();

        assert_eq!(bandit.arms.get("rust-errors").unwrap().pulls, 0);

        bandit.update("rust-errors", &features, &SkillFeedback::ExplicitHelpful);

        assert_eq!(bandit.arms.get("rust-errors").unwrap().pulls, 1);
    }

    #[test]
    fn test_positive_feedback_increases_alpha() {
        let mut bandit = ContextualBandit::with_feature_dim(10);
        bandit.register_skill("rust-errors");

        let features = sample_features();
        let initial_alpha = bandit.arms.get("rust-errors").unwrap().alpha;

        bandit.update("rust-errors", &features, &SkillFeedback::ExplicitHelpful);

        let updated_alpha = bandit.arms.get("rust-errors").unwrap().alpha;
        assert!(updated_alpha > initial_alpha);
    }

    #[test]
    fn test_negative_feedback_increases_beta() {
        let mut bandit = ContextualBandit::with_feature_dim(10);
        bandit.register_skill("rust-errors");

        let features = sample_features();
        let initial_beta = bandit.arms.get("rust-errors").unwrap().beta;

        bandit.update(
            "rust-errors",
            &features,
            &SkillFeedback::ExplicitNotHelpful { reason: None },
        );

        let updated_beta = bandit.arms.get("rust-errors").unwrap().beta;
        assert!(updated_beta > initial_beta);
    }

    #[test]
    fn test_recommend_empty_bandit() {
        let mut bandit = ContextualBandit::with_feature_dim(10);
        let features = sample_features();

        let recommendations = bandit.recommend(&features, 5);
        assert!(recommendations.is_empty());
    }

    #[test]
    fn test_recommend_returns_top_k() {
        let mut bandit = ContextualBandit::with_feature_dim(10);

        bandit.register_skill("skill-a");
        bandit.register_skill("skill-b");
        bandit.register_skill("skill-c");

        let features = sample_features();
        let recommendations = bandit.recommend(&features, 2);

        assert_eq!(recommendations.len(), 2);
        // Should be sorted by score descending
        assert!(recommendations[0].score >= recommendations[1].score);
    }

    #[test]
    fn test_learning_improves_predictions() {
        let mut bandit = ContextualBandit::with_feature_dim(10);
        bandit.register_skill("good-skill");
        bandit.register_skill("bad-skill");

        let features = sample_features();

        // Train: good-skill gets positive feedback, bad-skill gets negative
        for _ in 0..20 {
            bandit.update("good-skill", &features, &SkillFeedback::ExplicitHelpful);
            bandit.update(
                "bad-skill",
                &features,
                &SkillFeedback::ExplicitNotHelpful { reason: None },
            );
        }

        // After learning, good-skill should have higher avg_reward
        let good_reward = bandit.arms.get("good-skill").unwrap().avg_reward;
        let bad_reward = bandit.arms.get("bad-skill").unwrap().avg_reward;

        assert!(good_reward > bad_reward);
    }

    #[test]
    fn test_stats() {
        let mut bandit = ContextualBandit::with_feature_dim(10);
        bandit.register_skill("skill-a");
        bandit.register_skill("skill-b");

        let features = sample_features();
        bandit.update("skill-a", &features, &SkillFeedback::ExplicitHelpful);

        let stats = bandit.stats();
        assert_eq!(stats.num_skills, 2);
        assert_eq!(stats.total_updates, 1);
        assert_eq!(stats.cold_start_skills, 2); // Both still in cold start
    }

    #[test]
    fn test_explanation_cold_start() {
        let mut bandit = ContextualBandit::with_feature_dim(10);
        bandit.register_skill("new-skill");

        let features = sample_features();
        let recommendations = bandit.recommend(&features, 1);

        assert!(!recommendations.is_empty());
        assert!(recommendations[0].reason.contains("exploring"));
    }

    #[test]
    fn test_sigmoid() {
        assert!((sigmoid(0.0) - 0.5).abs() < 0.001);
        assert!(sigmoid(10.0) > 0.99);
        assert!(sigmoid(-10.0) < 0.01);
    }

    #[test]
    fn test_persistence_roundtrip() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("bandit.json");

        let mut bandit = ContextualBandit::with_feature_dim(10);
        bandit.register_skill("skill-a");
        bandit.update(
            "skill-a",
            &sample_features(),
            &SkillFeedback::ExplicitHelpful,
        );

        bandit.save(&path).unwrap();

        let loaded = ContextualBandit::load(&path).unwrap();
        assert_eq!(loaded.num_skills(), 1);
        assert!(loaded.has_skill("skill-a"));
        assert_eq!(loaded.arms.get("skill-a").unwrap().pulls, 1);
    }
}
