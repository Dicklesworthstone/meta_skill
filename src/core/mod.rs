//! Core skill types and logic

pub mod dependencies;
pub mod disclosure;
pub mod layering;
pub mod overlay;
pub mod pack_contracts;
pub mod packing;
pub mod recovery;
pub mod requirements;
pub mod resolution;
pub mod resolution_cache;
pub mod safety;
pub mod skill;
pub mod slicing;
pub mod spec_lens;
pub mod spec_migration;
pub mod validation;

pub use dependencies::{
    DependencyGraph, DependencyLoadMode, DependencyResolver, DisclosureLevel,
    ResolvedDependencyPlan, SkillLoadPlan,
};
pub use layering::{
    BlockDiff, ConflictDetail, ConflictResolution, ConflictStrategy, LayeredRegistry,
    MergeStrategy, ResolutionOptions, ResolvedSkill, SectionDiff, SkillCandidate,
};
pub use pack_contracts::{PackContractPreset, contract_from_name};
pub use packing::{
    ConstrainedPacker, CoverageQuota, MandatoryPredicate, MandatorySlice, PackConstraints,
    PackError, PackResult,
};
pub use recovery::{
    Checkpoint, FailureMode, RecoveryIssue, RecoveryManager, RecoveryReport, RetryConfig,
    with_retry, with_retry_if,
};
pub use resolution::{
    CycleDetectionResult, GitSkillRepository, MAX_INHERITANCE_DEPTH, ResolutionWarning,
    ResolvedSkillSpec, SkillRepository, detect_inheritance_cycle, get_inheritance_chain,
    resolve_extends, resolve_full,
};
pub use resolution_cache::{
    CacheKey, CacheStats, CachedResolvedSkill, DependencyGraph as ResolutionDependencyGraph,
    ResolutionCache,
};
pub use skill::{
    BlockType, EvidenceCoverage, EvidenceLevel, EvidenceRef, Skill, SkillBlock, SkillEvidenceIndex,
    SkillLayer, SkillMetadata, SkillSection, SkillSpec,
};
pub use slicing::{SkillSliceIndex, SkillSlicer};
pub use spec_migration::migrate_spec;
