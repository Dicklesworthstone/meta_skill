//! Beads (bd) version parsing and compatibility checks.

use std::cmp::Ordering;
use std::fmt;

use once_cell::sync::Lazy;

use crate::error::{MsError, Result};

/// Semantic version for bd binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BeadsVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl BeadsVersion {
    /// Create a new version.
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self { major, minor, patch }
    }

    /// Parse version string like "bd version 1.2.3" or "1.2.3-abc123".
    pub fn parse(input: &str) -> Result<Self> {
        let fragment = extract_version_fragment(input)
            .ok_or_else(|| MsError::ValidationFailed(format!("invalid version: {input}")))?;

        let parts: Vec<&str> = fragment.split('.').collect();
        if parts.len() < 2 {
            return Err(MsError::ValidationFailed(format!(
                "invalid version (need major.minor): {input}"
            )));
        }

        let major = parts[0].parse().unwrap_or(0);
        let minor = parts[1].parse().unwrap_or(0);
        let patch = parts.get(2).and_then(|p| p.parse().ok()).unwrap_or(0);

        Ok(Self { major, minor, patch })
    }
}

impl fmt::Display for BeadsVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl Ord for BeadsVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.major.cmp(&other.major) {
            Ordering::Equal => match self.minor.cmp(&other.minor) {
                Ordering::Equal => self.patch.cmp(&other.patch),
                ord => ord,
            },
            ord => ord,
        }
    }
}

impl PartialOrd for BeadsVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Minimum bd version this client supports.
pub static MINIMUM_SUPPORTED_VERSION: Lazy<BeadsVersion> =
    Lazy::new(|| BeadsVersion::new(0, 9, 0));

/// Recommended bd version for full feature support.
pub static RECOMMENDED_VERSION: Lazy<BeadsVersion> =
    Lazy::new(|| BeadsVersion::new(1, 0, 0));

#[derive(Debug, Clone)]
pub enum VersionCompatibility {
    Full,
    Partial { warning: String },
    Unsupported { error: String },
}

fn extract_version_fragment(input: &str) -> Option<String> {
    let mut started = false;
    let mut out = String::new();

    for ch in input.chars() {
        if !started {
            if ch.is_ascii_digit() {
                started = true;
                out.push(ch);
            }
            continue;
        }

        if ch.is_ascii_digit() || ch == '.' {
            out.push(ch);
        } else {
            break;
        }
    }

    if started { Some(out) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_parsing() {
        let v = BeadsVersion::parse("bd version 1.2.3").unwrap();
        assert_eq!(v, BeadsVersion::new(1, 2, 3));

        let v = BeadsVersion::parse("0.9.15-abc123").unwrap();
        assert_eq!(v, BeadsVersion::new(0, 9, 15));

        let v = BeadsVersion::parse("2.0.0").unwrap();
        assert_eq!(v, BeadsVersion::new(2, 0, 0));

        let v = BeadsVersion::parse("v1.4").unwrap();
        assert_eq!(v, BeadsVersion::new(1, 4, 0));
    }

    #[test]
    fn test_version_display() {
        let v = BeadsVersion::new(1, 2, 3);
        assert_eq!(format!("{}", v), "1.2.3");
    }

    #[test]
    fn test_version_ordering() {
        let v1 = BeadsVersion::new(1, 0, 0);
        let v2 = BeadsVersion::new(0, 9, 15);
        let v3 = BeadsVersion::new(1, 0, 1);

        assert!(v1 > v2);
        assert!(v3 > v1);
        assert!(v2 < v1);
    }
}
