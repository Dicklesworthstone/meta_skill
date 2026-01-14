use crate::error::{MsError, Result};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleManifest {
    pub bundle: BundleInfo,
    #[serde(default)]
    pub skills: Vec<BundledSkill>,
    #[serde(default)]
    pub dependencies: Vec<BundleDependency>,
    #[serde(default)]
    pub checksum: Option<String>,
    #[serde(default)]
    pub signatures: Vec<BundleSignature>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub ms_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundledSkill {
    pub name: String,
    pub path: PathBuf,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub hash: Option<String>,
    #[serde(default)]
    pub optional: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleDependency {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub optional: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleSignature {
    pub signer: String,
    pub key_id: String,
    pub signature: String,
}

impl BundleManifest {
    pub fn from_toml_str(input: &str) -> Result<Self> {
        toml::from_str(input).map_err(|err| {
            MsError::ValidationFailed(format!("Bundle manifest TOML parse error: {err}"))
        })
    }

    pub fn to_toml_string(&self) -> Result<String> {
        toml::to_string_pretty(self).map_err(|err| {
            MsError::ValidationFailed(format!("Bundle manifest TOML serialize error: {err}"))
        })
    }

    pub fn from_yaml_str(input: &str) -> Result<Self> {
        serde_yaml::from_str(input).map_err(|err| {
            MsError::ValidationFailed(format!("Bundle manifest YAML parse error: {err}"))
        })
    }

    pub fn to_yaml_string(&self) -> Result<String> {
        serde_yaml::to_string(self).map_err(|err| {
            MsError::ValidationFailed(format!("Bundle manifest YAML serialize error: {err}"))
        })
    }

    pub fn validate(&self) -> Result<()> {
        validate_required("bundle.id", &self.bundle.id)?;
        validate_required("bundle.name", &self.bundle.name)?;
        validate_required("bundle.version", &self.bundle.version)?;
        validate_semver("bundle.version", &self.bundle.version)?;
        if let Some(ms_version) = self.bundle.ms_version.as_ref() {
            validate_semver_req("bundle.ms_version", ms_version)?;
        }

        if self.skills.is_empty() {
            return Err(MsError::ValidationFailed(
                "skills must include at least one entry".to_string(),
            ));
        }

        let mut seen_skill_names = HashSet::new();
        for skill in &self.skills {
            validate_required("skills.name", &skill.name)?;
            if !seen_skill_names.insert(skill.name.clone()) {
                return Err(MsError::ValidationFailed(format!(
                    "duplicate skill name: {}",
                    skill.name
                )));
            }
            if skill.path.as_os_str().is_empty() {
                return Err(MsError::ValidationFailed(format!(
                    "skill path is required for {}",
                    skill.name
                )));
            }
            if let Some(version) = skill.version.as_ref() {
                validate_semver("skills.version", version)?;
            }
            if let Some(hash) = skill.hash.as_ref() {
                if hash.trim().is_empty() {
                    return Err(MsError::ValidationFailed(format!(
                        "skill hash is required for {}",
                        skill.name
                    )));
                }
            }
        }

        let mut seen_deps = HashSet::new();
        for dep in &self.dependencies {
            validate_required("dependencies.id", &dep.id)?;
            if !seen_deps.insert(dep.id.clone()) {
                return Err(MsError::ValidationFailed(format!(
                    "duplicate dependency id: {}",
                    dep.id
                )));
            }
            validate_semver_req("dependencies.version", &dep.version)?;
        }

        if let Some(checksum) = self.checksum.as_ref() {
            if checksum.trim().is_empty() {
                return Err(MsError::ValidationFailed(
                    "checksum cannot be empty".to_string(),
                ));
            }
        }

        Ok(())
    }
}

pub trait SignatureVerifier {
    fn verify(&self, payload: &[u8], signature: &BundleSignature) -> Result<()>;
}

pub struct NoopSignatureVerifier;

impl SignatureVerifier for NoopSignatureVerifier {
    fn verify(&self, _payload: &[u8], signature: &BundleSignature) -> Result<()> {
        Err(MsError::ValidationFailed(format!(
            "signature verification not configured for signer {}",
            signature.signer
        )))
    }
}

impl BundleManifest {
    pub fn verify_signatures(
        &self,
        payload: &[u8],
        verifier: &impl SignatureVerifier,
    ) -> Result<()> {
        for sig in &self.signatures {
            verifier.verify(payload, sig)?;
        }
        Ok(())
    }
}

fn validate_required(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(MsError::ValidationFailed(format!(
            "{field} must be non-empty"
        )));
    }
    Ok(())
}

fn validate_semver(field: &str, value: &str) -> Result<()> {
    Version::parse(value).map_err(|err| {
        MsError::ValidationFailed(format!("{field} must be valid semver: {err}"))
    })?;
    Ok(())
}

fn validate_semver_req(field: &str, value: &str) -> Result<()> {
    VersionReq::parse(value).map_err(|err| {
        MsError::ValidationFailed(format!("{field} must be valid semver range: {err}"))
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_TOML: &str = r#"
[bundle]
id = "rust-patterns"
name = "Rust Coding Patterns"
version = "1.0.0"
description = "Common patterns for Rust development"
authors = ["Example <example@example.com>"]
license = "MIT"
repository = "https://example.com/rust-patterns"
keywords = ["rust", "patterns"]
ms_version = ">=0.1.0"

[[skills]]
name = "error-handling"
path = "skills/error-handling"
version = "1.2.0"
hash = "sha256:deadbeef"

[[skills]]
name = "async-patterns"
path = "skills/async-patterns"
version = "0.5.0"
hash = "sha256:cafebabe"
optional = true

[[dependencies]]
id = "core-utils"
version = "^1.0"
optional = true

checksum = "sha256:abc123"
"#;

    #[test]
    fn toml_roundtrip_parsing() {
        let manifest = BundleManifest::from_toml_str(SAMPLE_TOML).unwrap();
        manifest.validate().unwrap();
        let serialized = manifest.to_toml_string().unwrap();
        let reparsed = BundleManifest::from_toml_str(&serialized).unwrap();
        assert_eq!(manifest, reparsed);
    }

    #[test]
    fn yaml_roundtrip_parsing() {
        let manifest = BundleManifest::from_toml_str(SAMPLE_TOML).unwrap();
        let yaml = manifest.to_yaml_string().unwrap();
        let reparsed = BundleManifest::from_yaml_str(&yaml).unwrap();
        assert_eq!(manifest, reparsed);
    }

    #[test]
    fn validate_rejects_duplicate_skills() {
        let mut manifest = BundleManifest::from_toml_str(SAMPLE_TOML).unwrap();
        manifest.skills.push(BundledSkill {
            name: "error-handling".to_string(),
            path: PathBuf::from("skills/dup"),
            version: Some("1.2.0".to_string()),
            hash: Some("sha256:abc123".to_string()),
            optional: false,
        });
        let err = manifest.validate().unwrap_err();
        let message = err.to_string();
        assert!(message.contains("duplicate skill name"));
    }

    #[test]
    fn validate_rejects_invalid_versions() {
        let mut manifest = BundleManifest::from_toml_str(SAMPLE_TOML).unwrap();
        manifest.bundle.version = "not-a-version".to_string();
        let err = manifest.validate().unwrap_err();
        assert!(err.to_string().contains("bundle.version"));

        manifest.bundle.version = "1.2.3".to_string();
        manifest.dependencies.push(BundleDependency {
            id: "bad-dep".to_string(),
            version: "nope".to_string(),
            optional: false,
        });
        let err = manifest.validate().unwrap_err();
        assert!(err.to_string().contains("dependencies.version"));
    }
}
