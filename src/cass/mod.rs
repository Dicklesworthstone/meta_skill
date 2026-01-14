//! CASS (Coding Agent Session Search) integration
//!
//! Mines CASS sessions to extract patterns and generate skills.

pub mod client;
pub mod mining;
pub mod synthesis;
pub mod refinement;

pub use client::CassClient;
pub use synthesis::SkillDraft;
