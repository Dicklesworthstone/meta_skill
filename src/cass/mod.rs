//! CASS (Coding Agent Session Search) integration
//!
//! Mines CASS sessions to extract patterns and generate skills.

pub mod client;
pub mod mining;
pub mod synthesis;
pub mod refinement;

// Re-export main types
pub use client::{
    CassCapabilities, CassClient, CassHealth, FingerprintCache, Session, SessionExpanded,
    SessionMatch, SessionMessage, SessionMetadata, ToolCall, ToolResult,
};
pub use mining::{ExtractedPattern, Pattern, PatternIR, PatternType};
pub use synthesis::SkillDraft;
