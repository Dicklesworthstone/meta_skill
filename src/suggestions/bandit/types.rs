use std::collections::HashMap;

use rand::distr::Distribution;
use rand::rngs::ThreadRng;
use rand_distr::Beta;
use serde::{Deserialize, Serialize};

/// Type of signal used for suggestion scoring.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalType {
    Bm25,
    Embedding,
    Trigger,
    Freshness,
    ProjectMatch,
    FileTypeMatch,
    CommandPattern,
    UserHistory,
}

impl SignalType {
    pub fn all() -> &'static [SignalType] {
        &[
            SignalType::Bm25,
            SignalType::Embedding,
            SignalType::Trigger,
            SignalType::Freshness,
            SignalType::ProjectMatch,
            SignalType::FileTypeMatch,
            SignalType::CommandPattern,
            SignalType::UserHistory,
        ]
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Reward {
    Success,
    Failure,
}

/// Beta distribution parameters for Thompson sampling.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BetaDistribution {
    pub alpha: f64,
    pub beta: f64,
}

impl Default for BetaDistribution {
    fn default() -> Self {
        Self {
            alpha: 1.0,
            beta: 1.0,
        }
    }
}

impl BetaDistribution {
    pub fn sample(&self, rng: &mut ThreadRng) -> f64 {
        // Ensure finite, positive values for Beta distribution parameters.
        // Fall back to uniform prior (1.0) if values are NaN, infinite, or non-positive.
        let alpha = if self.alpha.is_finite() && self.alpha > 0.0 {
            self.alpha
        } else {
            1.0
        };
        let beta_param = if self.beta.is_finite() && self.beta > 0.0 {
            self.beta
        } else {
            1.0
        };

        // Beta::new only fails for non-positive, NaN, or infinite params (now impossible)
        let beta = Beta::new(alpha, beta_param).expect("beta distribution with validated params");
        beta.sample(rng)
    }
}

/// A single arm in the multi-armed bandit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BanditArm {
    pub signal_type: SignalType,
    pub successes: f64,
    pub failures: f64,
    pub estimated_prob: f64,
    pub ucb: f64,
    pub last_selected: Option<chrono::DateTime<chrono::Utc>>,
    pub decay_factor: f64,
}

impl BanditArm {
    pub fn new(signal_type: SignalType, decay_factor: f64) -> Self {
        Self {
            signal_type,
            successes: 0.0,
            failures: 0.0,
            estimated_prob: 0.5,
            ucb: 0.0,
            last_selected: None,
            decay_factor,
        }
    }

    pub fn observe(&mut self, reward: Reward, prior: BetaDistribution) {
        let decay = self.decay_factor.clamp(0.0, 1.0);
        self.successes *= decay;
        self.failures *= decay;

        match reward {
            Reward::Success => self.successes += 1.0,
            Reward::Failure => self.failures += 1.0,
        }

        let total = self.successes + self.failures;
        if total > 0.0 {
            self.estimated_prob =
                (prior.alpha + self.successes) / (prior.alpha + prior.beta + total);
        }
    }

    pub fn observations(&self) -> f64 {
        self.successes + self.failures
    }
}

/// Selected weights for each signal type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalWeights {
    pub weights: HashMap<SignalType, f64>,
}

impl SignalWeights {
    pub fn get(&self, signal: SignalType) -> f64 {
        *self.weights.get(&signal).unwrap_or(&0.0)
    }

    pub fn normalize(&mut self) {
        let sum: f64 = self.weights.values().sum();
        if sum <= 0.0 {
            let uniform = 1.0 / (SignalType::all().len() as f64);
            for signal in SignalType::all() {
                self.weights.insert(*signal, uniform);
            }
            return;
        }
        for value in self.weights.values_mut() {
            *value /= sum;
        }
    }
}
