use crate::error::{MsError, Result};
use ring::signature::KeyPair;
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

/// Ed25519 signer for creating bundle signatures.
///
/// Loads private keys from OpenSSH format (the standard format used by ssh-keygen).
pub struct Ed25519Signer {
    keypair: ring::signature::Ed25519KeyPair,
    key_id: String,
}

impl std::fmt::Debug for Ed25519Signer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ed25519Signer")
            .field("key_id", &self.key_id)
            .field("keypair", &"[Ed25519KeyPair]")
            .finish()
    }
}

impl Ed25519Signer {
    /// Load a signer from an OpenSSH private key file.
    ///
    /// Supports unencrypted Ed25519 keys in the standard OpenSSH format
    /// (e.g., `~/.ssh/id_ed25519`).
    pub fn from_openssh_file(path: &std::path::Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path).map_err(|err| {
            MsError::Config(format!("failed to read key file {}: {}", path.display(), err))
        })?;
        Self::from_openssh_str(&contents)
    }

    /// Load a signer from an OpenSSH private key string.
    pub fn from_openssh_str(pem: &str) -> Result<Self> {
        let (seed, public_key_from_file) = parse_openssh_ed25519_key(pem)?;

        let keypair =
            ring::signature::Ed25519KeyPair::from_seed_unchecked(&seed).map_err(|_| {
                MsError::ValidationFailed("invalid Ed25519 seed in SSH key".to_string())
            })?;

        // Verify the public key from the file matches the one derived from the seed.
        // This catches corrupted key files where the public key doesn't match the private key.
        let derived_public_key = keypair.public_key().as_ref();
        if derived_public_key != public_key_from_file {
            return Err(MsError::ValidationFailed(
                "SSH key public key doesn't match private key (corrupted key file?)".to_string(),
            ));
        }

        // Generate key_id from public key (hex-encoded first 8 bytes)
        let key_id = format!("ed25519:{}", hex::encode(&public_key_from_file[..8]));

        Ok(Self { keypair, key_id })
    }

    /// Sign data and return a BundleSignature.
    pub fn sign(&self, payload: &[u8], signer_name: &str) -> BundleSignature {
        let signature = self.keypair.sign(payload);
        BundleSignature {
            signer: signer_name.to_string(),
            key_id: self.key_id.clone(),
            signature: hex::encode(signature.as_ref()),
        }
    }

    /// Get the public key bytes (32 bytes).
    pub fn public_key(&self) -> &[u8] {
        self.keypair.public_key().as_ref()
    }

    /// Get the key ID.
    pub fn key_id(&self) -> &str {
        &self.key_id
    }
}

/// Parse an OpenSSH Ed25519 private key and return (seed, public_key).
///
/// The OpenSSH format is documented at:
/// <https://github.com/openssh/openssh-portable/blob/master/PROTOCOL.key>
fn parse_openssh_ed25519_key(pem: &str) -> Result<([u8; 32], [u8; 32])> {
    use base64::Engine;

    const BEGIN_MARKER: &str = "-----BEGIN OPENSSH PRIVATE KEY-----";
    const END_MARKER: &str = "-----END OPENSSH PRIVATE KEY-----";
    const AUTH_MAGIC: &[u8] = b"openssh-key-v1\0";

    // Extract base64 content
    let start = pem.find(BEGIN_MARKER).ok_or_else(|| {
        MsError::ValidationFailed("not an OpenSSH private key (missing BEGIN marker)".to_string())
    })? + BEGIN_MARKER.len();

    let end = pem.find(END_MARKER).ok_or_else(|| {
        MsError::ValidationFailed("not an OpenSSH private key (missing END marker)".to_string())
    })?;

    let b64_content: String = pem[start..end]
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    let data = base64::engine::general_purpose::STANDARD
        .decode(&b64_content)
        .map_err(|err| MsError::ValidationFailed(format!("invalid base64 in SSH key: {}", err)))?;

    // Verify magic bytes
    if !data.starts_with(AUTH_MAGIC) {
        return Err(MsError::ValidationFailed(
            "invalid OpenSSH key (bad magic)".to_string(),
        ));
    }

    let mut cursor = AUTH_MAGIC.len();

    // Read cipher name
    let cipher = read_openssh_string(&data, &mut cursor)?;
    if cipher != "none" {
        return Err(MsError::ValidationFailed(format!(
            "encrypted SSH keys not supported (cipher: {})",
            cipher
        )));
    }

    // Read KDF name
    let kdf = read_openssh_string(&data, &mut cursor)?;
    if kdf != "none" {
        return Err(MsError::ValidationFailed(format!(
            "encrypted SSH keys not supported (kdf: {})",
            kdf
        )));
    }

    // Read KDF options (should be empty for "none")
    let _kdf_options = read_openssh_bytes(&data, &mut cursor)?;

    // Read number of keys
    let num_keys = read_openssh_u32(&data, &mut cursor)?;
    if num_keys != 1 {
        return Err(MsError::ValidationFailed(format!(
            "multi-key SSH files not supported (found {} keys)",
            num_keys
        )));
    }

    // Skip public key blob
    let _public_blob = read_openssh_bytes(&data, &mut cursor)?;

    // Read private section length
    let private_len = read_openssh_u32(&data, &mut cursor)? as usize;
    if cursor + private_len > data.len() {
        return Err(MsError::ValidationFailed(
            "truncated SSH key (private section)".to_string(),
        ));
    }

    // Read check integers (must match for verification)
    let check1 = read_openssh_u32(&data, &mut cursor)?;
    let check2 = read_openssh_u32(&data, &mut cursor)?;
    if check1 != check2 {
        return Err(MsError::ValidationFailed(
            "SSH key check integers don't match (corrupted key?)".to_string(),
        ));
    }

    // Read key type
    let key_type = read_openssh_string(&data, &mut cursor)?;
    if key_type != "ssh-ed25519" {
        return Err(MsError::ValidationFailed(format!(
            "expected ssh-ed25519 key, found: {}",
            key_type
        )));
    }

    // Read public key (32 bytes)
    let public_key_bytes = read_openssh_bytes(&data, &mut cursor)?;
    if public_key_bytes.len() != 32 {
        return Err(MsError::ValidationFailed(format!(
            "invalid Ed25519 public key length: {} (expected 32)",
            public_key_bytes.len()
        )));
    }

    // Read private key (64 bytes: 32-byte seed + 32-byte public key)
    let private_key_bytes = read_openssh_bytes(&data, &mut cursor)?;
    if private_key_bytes.len() != 64 {
        return Err(MsError::ValidationFailed(format!(
            "invalid Ed25519 private key length: {} (expected 64)",
            private_key_bytes.len()
        )));
    }

    let mut seed = [0u8; 32];
    let mut public_key = [0u8; 32];
    seed.copy_from_slice(&private_key_bytes[..32]);
    public_key.copy_from_slice(&public_key_bytes);

    Ok((seed, public_key))
}

fn read_openssh_u32(data: &[u8], cursor: &mut usize) -> Result<u32> {
    if *cursor + 4 > data.len() {
        return Err(MsError::ValidationFailed(
            "truncated SSH key data".to_string(),
        ));
    }
    let value = u32::from_be_bytes([
        data[*cursor],
        data[*cursor + 1],
        data[*cursor + 2],
        data[*cursor + 3],
    ]);
    *cursor += 4;
    Ok(value)
}

fn read_openssh_bytes<'a>(data: &'a [u8], cursor: &mut usize) -> Result<&'a [u8]> {
    let len = read_openssh_u32(data, cursor)? as usize;
    if *cursor + len > data.len() {
        return Err(MsError::ValidationFailed(
            "truncated SSH key data".to_string(),
        ));
    }
    let bytes = &data[*cursor..*cursor + len];
    *cursor += len;
    Ok(bytes)
}

fn read_openssh_string(data: &[u8], cursor: &mut usize) -> Result<String> {
    let bytes = read_openssh_bytes(data, cursor)?;
    String::from_utf8(bytes.to_vec())
        .map_err(|_| MsError::ValidationFailed("invalid UTF-8 in SSH key".to_string()))
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
            signature: hex::encode(&signature_bytes),
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
            signature: hex::encode(&signature_bytes),
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
            signature: hex::encode(&corrupted_sig),
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
            signature: hex::encode(&signature_bytes),
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
            signature: "not-valid-hex!@#$".to_string(),
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
            signature: hex::encode(&sig),
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

    // --- Ed25519Signer tests ---

    /// Generate a test OpenSSH Ed25519 private key string.
    /// This creates a valid unencrypted key for testing.
    fn generate_test_openssh_key() -> (String, Vec<u8>) {
        use ring::rand::SystemRandom;
        use ring::signature::{Ed25519KeyPair, KeyPair};

        let rng = SystemRandom::new();
        let pkcs8_bytes = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        let keypair = Ed25519KeyPair::from_pkcs8(pkcs8_bytes.as_ref()).unwrap();
        let public_key = keypair.public_key().as_ref().to_vec();

        // Extract the seed from PKCS8 (the last 32 bytes after the fixed prefix)
        // PKCS8 for Ed25519 is: version(3) + alg_id(5) + priv_key_wrapper(2+32+2+32)
        // Simplified: seed is at offset 16, public is at offset 51
        let seed = &pkcs8_bytes.as_ref()[16..48];

        // Build OpenSSH format key
        let mut ssh_data = Vec::new();

        // Magic
        ssh_data.extend_from_slice(b"openssh-key-v1\0");

        // Cipher: "none"
        ssh_data.extend_from_slice(&4u32.to_be_bytes());
        ssh_data.extend_from_slice(b"none");

        // KDF: "none"
        ssh_data.extend_from_slice(&4u32.to_be_bytes());
        ssh_data.extend_from_slice(b"none");

        // KDF options: empty
        ssh_data.extend_from_slice(&0u32.to_be_bytes());

        // Number of keys: 1
        ssh_data.extend_from_slice(&1u32.to_be_bytes());

        // Public key blob
        let mut pub_blob = Vec::new();
        pub_blob.extend_from_slice(&11u32.to_be_bytes()); // "ssh-ed25519" length
        pub_blob.extend_from_slice(b"ssh-ed25519");
        pub_blob.extend_from_slice(&32u32.to_be_bytes());
        pub_blob.extend_from_slice(&public_key);
        ssh_data.extend_from_slice(&(pub_blob.len() as u32).to_be_bytes());
        ssh_data.extend_from_slice(&pub_blob);

        // Private section
        let check = 0x12345678u32;
        let mut priv_section = Vec::new();
        priv_section.extend_from_slice(&check.to_be_bytes());
        priv_section.extend_from_slice(&check.to_be_bytes());
        priv_section.extend_from_slice(&11u32.to_be_bytes()); // "ssh-ed25519"
        priv_section.extend_from_slice(b"ssh-ed25519");
        priv_section.extend_from_slice(&32u32.to_be_bytes()); // public key
        priv_section.extend_from_slice(&public_key);
        priv_section.extend_from_slice(&64u32.to_be_bytes()); // private key (seed + pub)
        priv_section.extend_from_slice(seed);
        priv_section.extend_from_slice(&public_key);
        priv_section.extend_from_slice(&0u32.to_be_bytes()); // empty comment

        // Padding to multiple of 8
        let pad_needed = (8 - (priv_section.len() % 8)) % 8;
        for i in 1..=pad_needed {
            priv_section.push(i as u8);
        }

        ssh_data.extend_from_slice(&(priv_section.len() as u32).to_be_bytes());
        ssh_data.extend_from_slice(&priv_section);

        // Base64 encode and format as PEM
        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&ssh_data);
        let mut pem = String::from("-----BEGIN OPENSSH PRIVATE KEY-----\n");
        for chunk in b64.as_bytes().chunks(70) {
            pem.push_str(std::str::from_utf8(chunk).unwrap());
            pem.push('\n');
        }
        pem.push_str("-----END OPENSSH PRIVATE KEY-----\n");

        (pem, public_key)
    }

    #[test]
    fn ed25519_signer_parses_openssh_key() {
        let (pem, _public_key) = generate_test_openssh_key();
        let signer = Ed25519Signer::from_openssh_str(&pem);
        assert!(signer.is_ok(), "Failed to parse OpenSSH key: {:?}", signer.err());

        let signer = signer.unwrap();
        assert!(signer.key_id().starts_with("ed25519:"));
        assert_eq!(signer.public_key().len(), 32);
    }

    #[test]
    fn ed25519_signer_signs_data() {
        let (pem, _public_key) = generate_test_openssh_key();
        let signer = Ed25519Signer::from_openssh_str(&pem).unwrap();

        let payload = b"test payload for signing";
        let signature = signer.sign(payload, "Test Signer");

        assert_eq!(signature.signer, "Test Signer");
        assert_eq!(signature.key_id, signer.key_id());
        assert!(!signature.signature.is_empty());
        // Ed25519 signatures are 64 bytes = 128 hex chars
        assert_eq!(signature.signature.len(), 128);
    }

    #[test]
    fn ed25519_signer_signature_verifies() {
        let (pem, public_key) = generate_test_openssh_key();
        let signer = Ed25519Signer::from_openssh_str(&pem).unwrap();

        let payload = b"test payload for verification";
        let signature = signer.sign(payload, "Test Signer");

        // Verify with Ed25519Verifier
        let mut verifier = Ed25519Verifier::new();
        verifier.add_key(signer.key_id(), public_key);

        let result = verifier.verify(payload, &signature);
        assert!(result.is_ok(), "Signature verification failed: {:?}", result.err());
    }

    #[test]
    fn ed25519_signer_rejects_invalid_pem() {
        let result = Ed25519Signer::from_openssh_str("not a valid key");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing BEGIN marker"));
    }

    #[test]
    fn ed25519_signer_rejects_encrypted_key() {
        // A key with cipher != "none" should be rejected
        let encrypted_marker = "-----BEGIN OPENSSH PRIVATE KEY-----\n\
            b3BlbnNzaC1rZXktdjEAAAAACmFlczI1Ni1jdHIAAAAGYmNyeXB0AAAAGAAAABDs\n\
            -----END OPENSSH PRIVATE KEY-----\n";
        let result = Ed25519Signer::from_openssh_str(encrypted_marker);
        assert!(result.is_err());
        // The exact error depends on how far parsing gets, but it should fail
    }

    #[test]
    fn ed25519_signer_rejects_mismatched_public_key() {
        // Generate a valid key and then corrupt the public key in the file
        use ring::rand::SystemRandom;
        use ring::signature::{Ed25519KeyPair, KeyPair};

        let rng = SystemRandom::new();
        let pkcs8_bytes = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        let keypair = Ed25519KeyPair::from_pkcs8(pkcs8_bytes.as_ref()).unwrap();
        let real_public_key = keypair.public_key().as_ref().to_vec();

        // Use a different (wrong) public key
        let mut wrong_public_key = real_public_key.clone();
        wrong_public_key[0] ^= 0xFF; // Corrupt first byte

        let seed = &pkcs8_bytes.as_ref()[16..48];

        // Build OpenSSH format key with mismatched public key
        let mut ssh_data = Vec::new();
        ssh_data.extend_from_slice(b"openssh-key-v1\0");
        ssh_data.extend_from_slice(&4u32.to_be_bytes());
        ssh_data.extend_from_slice(b"none");
        ssh_data.extend_from_slice(&4u32.to_be_bytes());
        ssh_data.extend_from_slice(b"none");
        ssh_data.extend_from_slice(&0u32.to_be_bytes());
        ssh_data.extend_from_slice(&1u32.to_be_bytes());

        // Public key blob (using WRONG public key)
        let mut pub_blob = Vec::new();
        pub_blob.extend_from_slice(&11u32.to_be_bytes());
        pub_blob.extend_from_slice(b"ssh-ed25519");
        pub_blob.extend_from_slice(&32u32.to_be_bytes());
        pub_blob.extend_from_slice(&wrong_public_key);
        ssh_data.extend_from_slice(&(pub_blob.len() as u32).to_be_bytes());
        ssh_data.extend_from_slice(&pub_blob);

        // Private section (also using WRONG public key to be consistent in the file)
        let check = 0x12345678u32;
        let mut priv_section = Vec::new();
        priv_section.extend_from_slice(&check.to_be_bytes());
        priv_section.extend_from_slice(&check.to_be_bytes());
        priv_section.extend_from_slice(&11u32.to_be_bytes());
        priv_section.extend_from_slice(b"ssh-ed25519");
        priv_section.extend_from_slice(&32u32.to_be_bytes());
        priv_section.extend_from_slice(&wrong_public_key); // Wrong public key
        priv_section.extend_from_slice(&64u32.to_be_bytes());
        priv_section.extend_from_slice(seed);
        priv_section.extend_from_slice(&real_public_key); // Real public key in private section
        priv_section.extend_from_slice(&0u32.to_be_bytes());

        let pad_needed = (8 - (priv_section.len() % 8)) % 8;
        for i in 1..=pad_needed {
            priv_section.push(i as u8);
        }

        ssh_data.extend_from_slice(&(priv_section.len() as u32).to_be_bytes());
        ssh_data.extend_from_slice(&priv_section);

        use base64::Engine;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&ssh_data);
        let mut pem = String::from("-----BEGIN OPENSSH PRIVATE KEY-----\n");
        for chunk in b64.as_bytes().chunks(70) {
            pem.push_str(std::str::from_utf8(chunk).unwrap());
            pem.push('\n');
        }
        pem.push_str("-----END OPENSSH PRIVATE KEY-----\n");

        let result = Ed25519Signer::from_openssh_str(&pem);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("public key doesn't match"),
            "Expected public key mismatch error"
        );
    }
}
