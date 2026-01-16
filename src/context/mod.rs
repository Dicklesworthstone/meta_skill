//! Context capture and fingerprinting for suggestions.

pub mod capture;
pub mod detector;
pub mod fingerprint;

pub use capture::{CaptureError, ContextCapture};
pub use detector::{DefaultDetector, DetectedProject, ProjectDetector, ProjectMarker, ProjectType};
pub use fingerprint::{ChangeSignificance, ContextFingerprint};
