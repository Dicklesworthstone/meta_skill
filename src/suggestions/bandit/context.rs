use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::types::SignalType;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextKey {
    TechStack(String),
    TimeOfDay(TimeOfDay),
    ProjectSize(ProjectSize),
    ActivityPattern(String),
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeOfDay {
    Morning,
    Afternoon,
    Evening,
    Night,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectSize {
    Small,
    Medium,
    Large,
    Massive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextModifier {
    pub probability_bonus: HashMap<SignalType, f64>,
    pub weight_multiplier: HashMap<SignalType, f64>,
    pub observation_count: u64,
}

impl ContextModifier {
    #[must_use]
    pub fn apply(&self, signal: SignalType, base: f64) -> f64 {
        let mut adjusted = base;
        if let Some(multiplier) = self.weight_multiplier.get(&signal) {
            adjusted *= *multiplier;
        }
        if let Some(bonus) = self.probability_bonus.get(&signal) {
            adjusted += *bonus;
        }
        adjusted.max(0.0)
    }

    pub fn update(&mut self, signal: SignalType, reward: crate::suggestions::bandit::Reward) {
        // Learning rate for context updates
        const LEARNING_RATE: f64 = 0.05;

        let bonus = self.probability_bonus.entry(signal).or_insert(0.0);
        
        match reward {
            crate::suggestions::bandit::Reward::Success => {
                *bonus += LEARNING_RATE;
            }
            crate::suggestions::bandit::Reward::Failure => {
                *bonus -= LEARNING_RATE;
            }
        }

        // Clamp bonus to prevent extreme skewing (-0.5 to 0.5)
        *bonus = bonus.clamp(-0.5, 0.5);
    }
}

/// Context used by the bandit to adjust weights.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SuggestionContext {
    pub tech_stack: Option<String>,
    pub time_of_day: Option<TimeOfDay>,
    pub project_size: Option<ProjectSize>,
    pub activity_pattern: Option<String>,
}

impl SuggestionContext {
    #[must_use]
    pub fn keys(&self) -> Vec<ContextKey> {
        let mut keys = Vec::new();
        if let Some(stack) = &self.tech_stack {
            keys.push(ContextKey::TechStack(stack.clone()));
        }
        if let Some(time) = self.time_of_day {
            keys.push(ContextKey::TimeOfDay(time));
        }
        if let Some(size) = self.project_size {
            keys.push(ContextKey::ProjectSize(size));
        }
        if let Some(pattern) = &self.activity_pattern {
            keys.push(ContextKey::ActivityPattern(pattern.clone()));
        }
        keys
    }
}
