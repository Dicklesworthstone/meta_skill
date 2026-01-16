//! Context capture and fingerprinting for suggestions.

pub mod capture;
pub mod collector;
pub mod detector;
pub mod fingerprint;
pub mod scoring;

pub use capture::{CaptureError, ContextCapture};
pub use collector::{
    CollectorFingerprint, ContextCollector, ContextCollectorConfig, GitContext, RecentFile,
    WorkingContext,
};
pub use detector::{DefaultDetector, DetectedProject, ProjectDetector, ProjectMarker, ProjectType};
pub use fingerprint::{ChangeSignificance, ContextFingerprint};
pub use scoring::{
    RankedSkill, RelevanceScorer, ScoreBreakdown, ScoringWeights, WorkingContext as ScoringContext,
};