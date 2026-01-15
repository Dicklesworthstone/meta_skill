//! CASS (Coding Agent Session Search) integration
//!
//! Mines CASS sessions to extract patterns and generate skills.

pub mod brenner;
pub mod client;
pub mod mining;
pub mod quality;
pub mod refinement;
pub mod synthesis;
pub mod transformation;
pub mod uncertainty;

// Re-export main types
pub use brenner::{
    BrennerConfig, BrennerSkillDraft, BrennerWizard, CognitiveMove, CognitiveMoveTag, MoveDecision,
    MoveEvidence, SelectedSession, SkillExample, SkillRule, TestResults, WizardCheckpoint,
    WizardOutput, WizardState, generate_skill_md,
};
pub use client::{
    CassCapabilities, CassClient, CassHealth, FingerprintCache, Session, SessionExpanded,
    SessionMatch, SessionMessage, SessionMetadata, ToolCall, ToolResult,
};
pub use mining::{
    Pattern, PatternType, SegmentedSession, SessionPhase, SessionSegment, segment_session,
};
pub use quality::{MissingSignal, QualityConfig, QualityScorer, SessionQuality};
pub use synthesis::SkillDraft;
pub use transformation::{
    GeneralPattern, GeneralizationRefiner, GeneralizationValidation, InstanceCluster,
    RefinementCritique, SpecificInstance, SpecificToGeneralTransformer, TransformerConfig,
    UncertaintyQueueSink,
};
pub use uncertainty::{
    DefaultQueryGenerator, DefaultResolver, QueryGenerator, QueryResults, QueryType, Resolution,
    ResolutionAttempt, ResolutionResult, SuggestedQuery, UncertaintyConfig, UncertaintyCounts,
    UncertaintyId, UncertaintyItem, UncertaintyQueue, UncertaintyReason, UncertaintyResolver,
    UncertaintyStatus,
};
