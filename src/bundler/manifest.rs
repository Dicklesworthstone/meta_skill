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

/// Ed25519 signature verifier using the ring crate.
pub struct Ed25519Verifier {
    /// Map of key_id -> public key bytes (32 bytes)
    trusted_keys: std::collections::HashMap<String, Vec<u8>>,
}

impl Ed25519Verifier {
    /// Create a new verifier with no trusted keys.
    pub fn new() -> Self {
        Self {
            trusted_keys: std::collections::HashMap::new(),
        }
    }

    /// Add a trusted public key.
    pub fn add_key(&mut self, key_id: impl Into<String>, public_key: Vec<u8>) {
        self.trusted_keys.insert(key_id.into(), public_key);
    }

    /// Create a verifier from an iterator of (key_id, public_key) pairs.
    pub fn from_keys<I, K>(keys: I) -> Self
    where
        I: IntoIterator<Item = (K, Vec<u8>)>,
        K: Into<String>,
    {
        let mut verifier = Self::new();
        for (key_id, public_key) in keys {
            verifier.add_key(key_id, public_key);
        }
        verifier
    }
}

impl Default for Ed25519Verifier {
    fn default() -> Self {
        Self::new()
    }
}

impl SignatureVerifier for Ed25519Verifier {
    fn verify(&self, payload: &[u8], signature: &BundleSignature) -> Result<()> {
        let public_key_bytes = self.trusted_keys.get(&signature.key_id).ok_or_else(|| {
            MsError::ValidationFailed(format!(
                "unknown signing key: {} (signer: {})",
                signature.key_id, signature.signer
            ))
        })?;

        let signature_bytes = hex::decode(&signature.signature).map_err(|err| {
            MsError::ValidationFailed(format!("invalid signature encoding: {err}"))
        })?;

        let public_key =
            ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, public_key_bytes);

        public_key.verify(payload, &signature_bytes).map_err(|_| {
            MsError::ValidationFailed(format!(
                "signature verification failed for signer {}",
                signature.signer
            ))
        })
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

    // --- Ed25519 signature verification tests ---

    /// Generate a test Ed25519 keypair and return (public_key_bytes, sign_fn)
    fn generate_test_keypair() -> (Vec<u8>, impl Fn(&[u8]) -> Vec<u8>) {
        use ring::rand::SystemRandom;
        use ring::signature::{Ed25519KeyPair, KeyPair};

        let rng = SystemRandom::new();
        let pkcs8_bytes = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        let keypair = Ed25519KeyPair::from_pkcs8(pkcs8_bytes.as_ref()).unwrap();
        let public_key = keypair.public_key().as_ref().to_vec();

        // Return a closure that can sign data
        // We need to recreate the keypair in the closure since Ed25519KeyPair isn't Clone
        let pkcs8_owned = pkcs8_bytes.as_ref().to_vec();
        let sign_fn = move |data: &[u8]| -> Vec<u8> {
            let kp = Ed25519KeyPair::from_pkcs8(&pkcs8_owned).unwrap();
            kp.sign(data).as_ref().to_vec()
        };

        (public_key, sign_fn)
    }

    #[test]
    fn ed25519_verifier_accepts_valid_signature() {
        let (public_key, sign) = generate_test_keypair();
        let payload = b"test payload for bundle verification";
        let signature_bytes = sign(payload);

        let mut verifier = Ed25519Verifier::new();
        verifier.add_key("test-key-1", public_key);

        let bundle_sig = BundleSignature {
            signer: "Test Signer".to_string(),
            key_id: "test-key-1".to_string(),
            algorithm: "ed25519".to_string(),
            signature: hex::encode(&signature_bytes),
            timestamp: None,
        };

        // Should succeed
        let result = verifier.verify(payload, &bundle_sig);
        assert!(result.is_ok(), "Expected valid signature to verify: {:?}", result);
    }

    #[test]
    fn ed25519_verifier_rejects_unknown_key() {
        let (public_key, sign) = generate_test_keypair();
        let payload = b"test payload";
        let signature_bytes = sign(payload);

        let mut verifier = Ed25519Verifier::new();
        verifier.add_key("known-key", public_key);

        let bundle_sig = BundleSignature {
            signer: "Test Signer".to_string(),
            key_id: "unknown-key".to_string(), // Not in verifier's trusted keys
            algorithm: "ed25519".to_string(),
            signature: hex::encode(&signature_bytes),
            timestamp: None,
        };

        let result = verifier.verify(payload, &bundle_sig);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("unknown signing key"));
        assert!(err_msg.contains("unknown-key"));
    }

    #[test]
    fn ed25519_verifier_rejects_invalid_signature() {
        let (public_key, sign) = generate_test_keypair();
        let payload = b"test payload";
        let signature_bytes = sign(payload);

        let mut verifier = Ed25519Verifier::new();
        verifier.add_key("test-key", public_key);

        // Corrupt the signature
        let mut corrupted_sig = signature_bytes.clone();
        corrupted_sig[0] ^= 0xff;

        let bundle_sig = BundleSignature {
            signer: "Test Signer".to_string(),
            key_id: "test-key".to_string(),
            algorithm: "ed25519".to_string(),
            signature: hex::encode(&corrupted_sig),
            timestamp: None,
        };

        let result = verifier.verify(payload, &bundle_sig);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("verification failed"));
    }

    #[test]
    fn ed25519_verifier_rejects_wrong_payload() {
        let (public_key, sign) = generate_test_keypair();
        let original_payload = b"original payload";
        let signature_bytes = sign(original_payload);

        let mut verifier = Ed25519Verifier::new();
        verifier.add_key("test-key", public_key);

        let bundle_sig = BundleSignature {
            signer: "Test Signer".to_string(),
            key_id: "test-key".to_string(),
            algorithm: "ed25519".to_string(),
            signature: hex::encode(&signature_bytes),
            timestamp: None,
        };

        // Verify against different payload
        let tampered_payload = b"tampered payload";
        let result = verifier.verify(tampered_payload, &bundle_sig);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("verification failed"));
    }

    #[test]
    fn ed25519_verifier_rejects_invalid_hex_encoding() {
        let (public_key, _sign) = generate_test_keypair();
        let payload = b"test payload";

        let mut verifier = Ed25519Verifier::new();
        verifier.add_key("test-key", public_key);

        let bundle_sig = BundleSignature {
            signer: "Test Signer".to_string(),
            key_id: "test-key".to_string(),
            algorithm: "ed25519".to_string(),
            signature: "not-valid-hex!@#$".to_string(),
            timestamp: None,
        };

        let result = verifier.verify(payload, &bundle_sig);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid signature encoding"));
    }

    #[test]
    fn ed25519_verifier_from_keys_constructor() {
        let (public_key1, _) = generate_test_keypair();
        let (public_key2, sign2) = generate_test_keypair();

        let verifier = Ed25519Verifier::from_keys([
            ("key1", public_key1),
            ("key2", public_key2.clone()),
        ]);

        // Verify that key2 works
        let payload = b"test";
        let sig = sign2(payload);
        let bundle_sig = BundleSignature {
            signer: "Signer 2".to_string(),
            key_id: "key2".to_string(),
            algorithm: "ed25519".to_string(),
            signature: hex::encode(&sig),
            timestamp: None,
        };

        assert!(verifier.verify(payload, &bundle_sig).is_ok());
    }

    #[test]
    fn manifest_verify_signatures_all_valid() {
        let (public_key, sign) = generate_test_keypair();
        let payload = b"manifest content";
        let sig = sign(payload);

        let mut manifest = BundleManifest::from_toml_str(SAMPLE_TOML).unwrap();
        manifest.signatures.push(BundleSignature {
            signer: "Publisher".to_string(),
            key_id: "publisher-key".to_string(),
            signature: hex::encode(&sig),
        });

        let mut verifier = Ed25519Verifier::new();
        verifier.add_key("publisher-key", public_key);

        assert!(manifest.verify_signatures(payload, &verifier).is_ok());
    }

    #[test]
    fn manifest_verify_signatures_fails_if_any_invalid() {
        let (public_key, sign) = generate_test_keypair();
        let payload = b"manifest content";
        let valid_sig = sign(payload);

        let mut manifest = BundleManifest::from_toml_str(SAMPLE_TOML).unwrap();

        // First signature is valid
        manifest.signatures.push(BundleSignature {
            signer: "Publisher".to_string(),
            key_id: "publisher-key".to_string(),
            signature: hex::encode(&valid_sig),
        });

        // Second signature has unknown key
        manifest.signatures.push(BundleSignature {
            signer: "Unknown".to_string(),
            key_id: "unknown-key".to_string(),
            signature: hex::encode(&valid_sig),
        });

        let mut verifier = Ed25519Verifier::new();
        verifier.add_key("publisher-key", public_key);

        let result = manifest.verify_signatures(payload, &verifier);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unknown signing key"));
    }
}
