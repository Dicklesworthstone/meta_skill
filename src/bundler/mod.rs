//! Skill bundler for packaging and distribution

pub mod blob;
pub mod github;
pub mod install;
pub mod local_safety;
pub mod manifest;
pub mod package;
pub mod registry;

pub use blob::BlobStore;
pub use install::{InstallOptions, InstallReport, install, install_with_options};
pub use local_safety::{
    ConflictDetail, ConflictStrategy, FileStatus, ModificationStatus, ModificationSummary,
    ResolutionResult, SkillModificationReport, detect_conflicts, detect_modifications,
    hash_directory, hash_file,
};
pub use manifest::{
    BundleDependency, BundleInfo, BundleManifest, BundleSignature, BundledSkill, Ed25519Signer,
    Ed25519Verifier, SignatureVerifier,
};
pub use package::{Bundle, BundleBlob, BundlePackage, missing_blobs};
pub use registry::{BundleRegistry, InstallSource, InstalledBundle, ParsedSource};
