//! CASS (Coding Agent Session Search) integration
//!
//! Mines CASS sessions to extract patterns and generate skills.

pub mod client;
pub mod mining;
pub mod quality;
pub mod refinement;
pub mod synthesis;
pub mod transformation;

// Re-export main types
pub use client::{
    CassCapabilities, CassClient, CassHealth, FingerprintCache, Session, SessionExpanded,
    SessionMatch, SessionMessage, SessionMetadata, ToolCall, ToolResult,
};
pub use mining::{
    segment_session, Pattern, PatternType, SegmentedSession, SessionPhase, SessionSegment,
};
pub use quality::{MissingSignal, QualityConfig, QualityScorer, SessionQuality};
pub use synthesis::SkillDraft;
pub use transformation::{
    GeneralPattern, GeneralizationRefiner, GeneralizationValidation, InstanceCluster,
    RefinementCritique, SpecificInstance, SpecificToGeneralTransformer, TransformerConfig,
    UncertaintyQueueSink,
};
