//! Core skill types and logic

pub mod dependencies;
pub mod disclosure;
pub mod layering;
pub mod registry;
pub mod requirements;
pub mod safety;
pub mod skill;
pub mod slicing;
pub mod spec_lens;
pub mod validation;

pub use dependencies::{
    DependencyGraph, DependencyLoadMode, DependencyResolver, DisclosureLevel,
    ResolvedDependencyPlan, SkillLoadPlan,
};
pub use layering::{
    BlockDiff, ConflictDetail, ConflictResolution, ConflictStrategy, LayeredRegistry,
    MergeStrategy, ResolvedSkill, ResolutionOptions, SectionDiff, SkillCandidate,
};
pub use skill::{BlockType, Skill, SkillBlock, SkillMetadata, SkillSection, SkillSpec};
pub use slicing::{SkillSliceIndex, SkillSlicer};
