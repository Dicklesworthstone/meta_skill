//! Core skill types and logic

pub mod skill;
pub mod registry;
pub mod disclosure;
pub mod safety;
pub mod requirements;
pub mod spec_lens;
pub mod validation;

pub use skill::{Skill, SkillSpec, SkillMetadata, SkillSection, SkillBlock, BlockType};
