//! Skill bundler for packaging and distribution

pub mod package;
pub mod github;
pub mod install;
pub mod manifest;
pub mod blob;

pub use package::{Bundle, BundlePackage, BundleBlob, missing_blobs};
pub use manifest::{BundleManifest, BundleInfo, BundledSkill, BundleDependency, BundleSignature, SignatureVerifier, Ed25519Verifier};
pub use blob::BlobStore;
