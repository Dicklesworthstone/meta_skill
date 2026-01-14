//! CASS CLI client
//!
//! Wraps the CASS CLI for programmatic access using robot mode.

use std::process::Command;

use crate::error::{MsError, Result};

/// Client for interacting with CASS
pub struct CassClient {
    /// Path to CASS binary (default: "cass")
    binary: String,
}

impl CassClient {
    /// Create a new CASS client
    pub fn new() -> Self {
        Self {
            binary: "cass".into(),
        }
    }

    /// Check if CASS is available
    pub fn is_available(&self) -> bool {
        Command::new(&self.binary)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Search CASS sessions
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SessionMatch>> {
        if !self.is_available() {
            return Err(MsError::CassUnavailable("CASS binary not found".into()));
        }

        let output = Command::new(&self.binary)
            .args(["search", query, "--robot", "--limit", &limit.to_string()])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MsError::CassUnavailable(format!("CASS search failed: {stderr}")));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let results: Vec<SessionMatch> = serde_json::from_str(&stdout)
            .map_err(|e| MsError::CassUnavailable(format!("Failed to parse CASS output: {e}")))?;

        Ok(results)
    }
}

impl Default for CassClient {
    fn default() -> Self {
        Self::new()
    }
}

/// A match from CASS search
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SessionMatch {
    pub path: String,
    pub score: f32,
    pub snippet: Option<String>,
}
