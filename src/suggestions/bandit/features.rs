//! Feature extraction for contextual bandit recommendations.
//!
//! Extracts features from working context for use in contextual multi-armed bandits.
//! Features include project type encodings, time-based features, activity patterns,
//! and historical usage patterns.

use std::collections::HashMap;

use chrono::{Datelike, Local, Timelike};
use serde::{Deserialize, Serialize};

use crate::context::collector::CollectedContext;
use crate::context::detector::ProjectType;
use crate::context::scoring::WorkingContext;

/// Feature vector dimension (project_types + time + activity + history).
/// Project types: 18, Time: 4, Activity: 3, History: 3 = 28 dimensions
pub const FEATURE_DIM: usize = 28;

/// Context features for contextual bandit recommendations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextFeatures {
    /// Project type one-hot encoding (18 dimensions).
    pub project_type: Vec<f32>,

    /// Time features: [hour_sin, hour_cos, day_sin, day_cos] (4 dimensions).
    pub time_features: Vec<f32>,

    /// Activity features: [files_ratio, tools_present, has_git] (3 dimensions).
    pub activity_features: Vec<f32>,

    /// Historical features: [skill_frequency, recency, session_depth] (3 dimensions).
    pub history_features: Vec<f32>,
}

impl Default for ContextFeatures {
    fn default() -> Self {
        Self {
            project_type: vec![0.0; 18],
            time_features: vec![0.0; 4],
            activity_features: vec![0.0; 3],
            history_features: vec![0.5; 3], // Prior for unknown history
        }
    }
}

impl ContextFeatures {
    /// Create a new feature vector with the given components.
    #[must_use]
    pub fn new(
        project_type: Vec<f32>,
        time_features: Vec<f32>,
        activity_features: Vec<f32>,
        history_features: Vec<f32>,
    ) -> Self {
        Self {
            project_type,
            time_features,
            activity_features,
            history_features,
        }
    }

    /// Get the total dimension of the feature vector.
    #[must_use]
    pub fn dim(&self) -> usize {
        self.project_type.len()
            + self.time_features.len()
            + self.activity_features.len()
            + self.history_features.len()
    }

    /// Convert to a flat feature vector.
    #[must_use]
    pub fn as_vec(&self) -> Vec<f32> {
        let mut vec = Vec::with_capacity(self.dim());
        vec.extend(&self.project_type);
        vec.extend(&self.time_features);
        vec.extend(&self.activity_features);
        vec.extend(&self.history_features);
        vec
    }

    /// Compute dot product with weights.
    #[must_use]
    pub fn dot(&self, weights: &[f32]) -> f32 {
        let features = self.as_vec();
        features
            .iter()
            .zip(weights.iter())
            .map(|(f, w)| f * w)
            .sum()
    }
}

/// User history for personalization.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UserHistory {
    /// Total number of skill loads in history.
    pub total_skill_loads: u64,
    /// Days since last skill load.
    pub days_since_last_use: Option<u32>,
    /// Average session duration in minutes.
    pub avg_session_minutes: f32,
    /// Per-skill load counts.
    pub skill_load_counts: HashMap<String, u64>,
    /// Per-skill last load timestamps.
    pub skill_last_load: HashMap<String, chrono::DateTime<chrono::Utc>>,
}

impl UserHistory {
    /// Get the frequency score for a skill (0.0-1.0).
    #[must_use]
    pub fn skill_frequency(&self, skill_id: &str) -> f32 {
        if self.total_skill_loads == 0 {
            return 0.0;
        }
        let count = self.skill_load_counts.get(skill_id).copied().unwrap_or(0);
        (count as f32 / self.total_skill_loads as f32).min(1.0)
    }

    /// Get the recency score for a skill (0.0-1.0, higher = more recent).
    #[must_use]
    pub fn skill_recency(&self, skill_id: &str) -> f32 {
        let Some(last_load) = self.skill_last_load.get(skill_id) else {
            return 0.0;
        };
        let days_ago = chrono::Utc::now()
            .signed_duration_since(*last_load)
            .num_days() as f32;
        // Exponential decay with half-life of 7 days
        (-days_ago / 7.0).exp()
    }
}

/// Trait for extracting features from context.
pub trait FeatureExtractor: Send + Sync {
    /// Extract features from the given context and user history.
    fn extract(&self, context: &WorkingContext, history: &UserHistory) -> ContextFeatures;

    /// Extract features from collected context.
    fn extract_from_collected(
        &self,
        context: &CollectedContext,
        history: &UserHistory,
    ) -> ContextFeatures;

    /// Get the expected feature dimension.
    fn dim(&self) -> usize;
}

/// Default feature extractor implementation.
#[derive(Debug, Clone, Default)]
pub struct DefaultFeatureExtractor {
    /// Ordered list of project types for one-hot encoding.
    project_types: Vec<ProjectType>,
}

impl DefaultFeatureExtractor {
    /// Create a new default feature extractor.
    #[must_use]
    pub fn new() -> Self {
        Self {
            project_types: vec![
                ProjectType::Rust,
                ProjectType::Node,
                ProjectType::Python,
                ProjectType::Go,
                ProjectType::Java,
                ProjectType::CSharp,
                ProjectType::Ruby,
                ProjectType::Elixir,
                ProjectType::Php,
                ProjectType::Swift,
                ProjectType::Kotlin,
                ProjectType::Scala,
                ProjectType::Haskell,
                ProjectType::Clojure,
                ProjectType::Cpp,
                ProjectType::C,
                ProjectType::Zig,
                ProjectType::Nim,
            ],
        }
    }

    /// Encode project type as one-hot vector.
    fn encode_project_type(&self, primary: Option<ProjectType>) -> Vec<f32> {
        let mut encoding = vec![0.0; self.project_types.len()];
        if let Some(pt) = primary {
            if let Some(idx) = self.project_types.iter().position(|t| *t == pt) {
                encoding[idx] = 1.0;
            }
        }
        encoding
    }

    /// Extract time-based features using cyclical encoding.
    fn extract_time_features(&self) -> Vec<f32> {
        let now = Local::now();
        let hour = now.hour() as f32;
        let day = now.weekday().num_days_from_monday() as f32;

        vec![
            // Hour cyclical encoding
            (hour * std::f32::consts::TAU / 24.0).sin(),
            (hour * std::f32::consts::TAU / 24.0).cos(),
            // Day of week cyclical encoding
            (day * std::f32::consts::TAU / 7.0).sin(),
            (day * std::f32::consts::TAU / 7.0).cos(),
        ]
    }

    /// Extract activity features from working context.
    fn extract_activity_features(&self, context: &WorkingContext) -> Vec<f32> {
        vec![
            // Files ratio (normalized)
            (context.recent_files.len() as f32 / 20.0).min(1.0),
            // Tools present (normalized)
            (context.detected_tools.len() as f32 / 10.0).min(1.0),
            // Has detected projects
            if context.detected_projects.is_empty() {
                0.0
            } else {
                1.0
            },
        ]
    }

    /// Extract activity features from collected context.
    fn extract_activity_features_collected(&self, context: &CollectedContext) -> Vec<f32> {
        vec![
            // Files ratio (normalized)
            (context.recent_files.len() as f32 / 20.0).min(1.0),
            // Tools present (normalized)
            (context.detected_tools.len() as f32 / 10.0).min(1.0),
            // Has git context
            if context.git_context.is_some() {
                1.0
            } else {
                0.0
            },
        ]
    }

    /// Extract historical features from user history.
    fn extract_history_features(&self, history: &UserHistory) -> Vec<f32> {
        vec![
            // Overall activity level (normalized)
            (history.total_skill_loads as f32 / 100.0).min(1.0),
            // Days since last use (inverse, normalized)
            history
                .days_since_last_use
                .map_or(0.5, |d| 1.0 / (1.0 + d as f32)),
            // Session depth (normalized)
            (history.avg_session_minutes / 60.0).min(1.0),
        ]
    }
}

impl FeatureExtractor for DefaultFeatureExtractor {
    fn extract(&self, context: &WorkingContext, history: &UserHistory) -> ContextFeatures {
        let primary_project = context.primary_project_type();

        ContextFeatures::new(
            self.encode_project_type(primary_project),
            self.extract_time_features(),
            self.extract_activity_features(context),
            self.extract_history_features(history),
        )
    }

    fn extract_from_collected(
        &self,
        context: &CollectedContext,
        history: &UserHistory,
    ) -> ContextFeatures {
        let primary_project = context
            .detected_projects
            .iter()
            .max_by(|a, b| {
                a.confidence
                    .partial_cmp(&b.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|d| d.project_type);

        ContextFeatures::new(
            self.encode_project_type(primary_project),
            self.extract_time_features(),
            self.extract_activity_features_collected(context),
            self.extract_history_features(history),
        )
    }

    fn dim(&self) -> usize {
        FEATURE_DIM
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::detector::DetectedProject;
    use std::path::PathBuf;

    fn sample_rust_context() -> WorkingContext {
        WorkingContext::new()
            .with_projects(vec![DetectedProject {
                project_type: ProjectType::Rust,
                confidence: 1.0,
                marker_path: PathBuf::from("Cargo.toml"),
                marker_pattern: "Cargo.toml".to_string(),
            }])
            .with_recent_files(vec!["src/main.rs".to_string(), "src/lib.rs".to_string()])
            .with_tools(["cargo", "rustc"].map(String::from))
    }

    #[test]
    fn test_feature_extraction() {
        let extractor = DefaultFeatureExtractor::new();
        let context = sample_rust_context();
        let history = UserHistory::default();

        let features = extractor.extract(&context, &history);

        assert_eq!(features.dim(), FEATURE_DIM);
        // Rust should be encoded in position 0
        assert!((features.project_type[0] - 1.0).abs() < 0.001);
        // Other project types should be 0
        assert!((features.project_type[1]).abs() < 0.001);
    }

    #[test]
    fn test_feature_vector_conversion() {
        let features = ContextFeatures::default();
        let vec = features.as_vec();

        assert_eq!(vec.len(), FEATURE_DIM);
    }

    #[test]
    fn test_dot_product() {
        let features = ContextFeatures {
            project_type: vec![1.0, 0.0],
            time_features: vec![0.5, 0.5],
            activity_features: vec![1.0],
            history_features: vec![0.5],
        };
        let weights = vec![2.0, 1.0, 1.0, 1.0, 1.0, 1.0];

        let dot = features.dot(&weights);
        // 1.0*2.0 + 0.0*1.0 + 0.5*1.0 + 0.5*1.0 + 1.0*1.0 + 0.5*1.0 = 4.5
        assert!((dot - 4.5).abs() < 0.001);
    }

    #[test]
    fn test_user_history_skill_frequency() {
        let mut history = UserHistory::default();
        history.total_skill_loads = 100;
        history
            .skill_load_counts
            .insert("rust-errors".to_string(), 10);

        assert!((history.skill_frequency("rust-errors") - 0.1).abs() < 0.001);
        assert!((history.skill_frequency("unknown")).abs() < 0.001);
    }

    #[test]
    fn test_time_features_cyclical() {
        let extractor = DefaultFeatureExtractor::new();
        let time_features = extractor.extract_time_features();

        assert_eq!(time_features.len(), 4);
        // Sin and cos should be in [-1, 1]
        for f in &time_features {
            assert!(*f >= -1.0 && *f <= 1.0);
        }
    }

    #[test]
    fn test_empty_context() {
        let extractor = DefaultFeatureExtractor::new();
        let context = WorkingContext::default();
        let history = UserHistory::default();

        let features = extractor.extract(&context, &history);

        // All project type features should be 0
        assert!(features.project_type.iter().all(|f| *f < 0.001));
    }
}
