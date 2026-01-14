//! Skill bundler for packaging and distribution

pub mod package;
pub mod github;
pub mod install;
pub mod manifest;
pub mod blob;

pub use package::Bundle;
pub use manifest::{BundleManifest, BundleInfo, BundledSkill, BundleDependency, BundleSignature};
pub use blob::BlobStore;
