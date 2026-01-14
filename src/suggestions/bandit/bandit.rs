use std::collections::HashMap;
use std::path::Path;

use rand::thread_rng;
use serde::{Deserialize, Serialize};

use crate::error::{MsError, Result};

use super::context::{ContextKey, ContextModifier, SuggestionContext};
use super::types::{BanditArm, BetaDistribution, Reward, SignalType, SignalWeights};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BanditConfig {
    pub exploration_factor: f64,
    pub observation_decay: f64,
    pub min_observations: u64,
    pub use_context: bool,
    pub persist_frequency: u64,
    pub persistence_path: Option<std::path::PathBuf>,
}

impl Default for BanditConfig {
    fn default() -> Self {
        Self {
            exploration_factor: 0.1,
            observation_decay: 0.99,
            min_observations: 10,
            use_context: true,
            persist_frequency: 10,
            persistence_path: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalBandit {
    pub arms: HashMap<SignalType, BanditArm>,
    pub prior: BetaDistribution,
    pub context_modifiers: HashMap<ContextKey, ContextModifier>,
    pub total_selections: u64,
    pub config: BanditConfig,
}

impl SignalBandit {
    pub fn new() -> Self {
        let config = BanditConfig::default();
        let mut arms = HashMap::new();
        for signal in SignalType::all() {
            arms.insert(*signal, BanditArm::new(*signal, config.observation_decay));
        }
        Self {
            arms,
            prior: BetaDistribution::default(),
            context_modifiers: HashMap::new(),
            total_selections: 0,
            config,
        }
    }

    pub fn select_weights(&mut self, context: &SuggestionContext) -> SignalWeights {
        let mut rng = thread_rng();
        let mut weights = HashMap::new();

        for signal in SignalType::all() {
            let mut sample = self
                .prior
                .sample(&mut rng)
                .max(0.0);
            if let Some(arm) = self.arms.get(signal) {
                let prior = BetaDistribution {
                    alpha: self.prior.alpha + arm.successes,
                    beta: self.prior.beta + arm.failures,
                };
                sample = prior.sample(&mut rng).max(0.0);
            }

            if self.config.use_context {
                for key in context.keys() {
                    if let Some(modifier) = self.context_modifiers.get(&key) {
                        sample = modifier.apply(*signal, sample);
                    }
                }
            }

            weights.insert(*signal, sample);
        }

        let mut weights = SignalWeights { weights };
        weights.normalize();
        self.total_selections += 1;
        weights
    }

    pub fn estimated_weights(&self, context: &SuggestionContext) -> SignalWeights {
        let mut weights = HashMap::new();
        for signal in SignalType::all() {
            let mut value = self
                .arms
                .get(signal)
                .map(|arm| arm.estimated_prob)
                .unwrap_or(0.5);
            if self.config.use_context {
                for key in context.keys() {
                    if let Some(modifier) = self.context_modifiers.get(&key) {
                        value = modifier.apply(*signal, value);
                    }
                }
            }
            weights.insert(*signal, value.max(0.0));
        }

        let mut weights = SignalWeights { weights };
        weights.normalize();
        weights
    }

    pub fn update(&mut self, signal: SignalType, reward: Reward, context: &SuggestionContext) {
        let Some(arm) = self.arms.get_mut(&signal) else {
            return;
        };
        arm.observe(reward, self.prior);
        arm.last_selected = Some(chrono::Utc::now());

        let observations = arm.observations().max(1.0);
        let total = self.total_selections.max(1) as f64;
        let bonus = (total.ln() / observations).sqrt() * self.config.exploration_factor;
        arm.ucb = (arm.estimated_prob + bonus).clamp(0.0, 1.0);

        if self.config.use_context {
            for key in context.keys() {
                let entry = self
                    .context_modifiers
                    .entry(key)
                    .or_insert_with(|| ContextModifier {
                        probability_bonus: HashMap::new(),
                        weight_multiplier: HashMap::new(),
                        observation_count: 0,
                    });
                entry.observation_count += 1;
            }
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(MsError::Io)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        let temp_path = path.with_extension("tmp");
        std::fs::write(&temp_path, json).map_err(MsError::Io)?;
        std::fs::rename(&temp_path, path).map_err(MsError::Io)?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let contents = std::fs::read_to_string(path).map_err(MsError::Io)?;
        let bandit: Self = serde_json::from_str(&contents)?;
        Ok(bandit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::suggestions::bandit::context::SuggestionContext;
    use crate::suggestions::bandit::types::Reward;

    #[test]
    fn weights_sum_to_one() {
        let mut bandit = SignalBandit::new();
        let context = SuggestionContext::default();
        let weights = bandit.select_weights(&context);
        let sum: f64 = weights.weights.values().sum();
        assert!((sum - 1.0).abs() < 1e-6);
    }

    #[test]
    fn update_increases_estimated_prob() {
        let mut bandit = SignalBandit::new();
        let context = SuggestionContext::default();
        for _ in 0..50 {
            bandit.update(SignalType::Bm25, Reward::Success, &context);
        }
        let arm = bandit.arms.get(&SignalType::Bm25).unwrap();
        assert!(arm.estimated_prob > 0.5);
    }

    #[test]
    fn estimated_weights_are_deterministic() {
        let bandit = SignalBandit::new();
        let context = SuggestionContext::default();
        let weights = bandit.estimated_weights(&context);
        let sum: f64 = weights.weights.values().sum();
        assert!((sum - 1.0).abs() < 1e-6);
    }
}
