//! CM CLI client.
//!
//! Wraps the CM (cass-memory) CLI for programmatic access using JSON output.

use std::path::PathBuf;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::config::CmConfig;
use crate::error::{MsError, Result};
use crate::security::SafetyGate;

/// Parsed response from `cm context`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CmContext {
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub task: String,
    #[serde(rename = "relevantBullets", default)]
    pub relevant_bullets: Vec<serde_json::Value>,
    #[serde(rename = "antiPatterns", default)]
    pub anti_patterns: Vec<serde_json::Value>,
    #[serde(rename = "historySnippets", default)]
    pub history_snippets: Vec<serde_json::Value>,
    #[serde(rename = "suggestedCassQueries", default)]
    pub suggested_cass_queries: Vec<String>,
}

/// Client for interacting with CM (cass-memory).
pub struct CmClient {
    /// Path to cm binary (default: "cm")
    cm_bin: PathBuf,

    /// Default flags for cm invocations
    default_flags: Vec<String>,

    /// Optional safety gate for command execution
    safety: Option<SafetyGate>,
}

impl CmClient {
    /// Create a new CM client with default settings.
    pub fn new() -> Self {
        Self {
            cm_bin: PathBuf::from("cm"),
            default_flags: Vec::new(),
            safety: None,
        }
    }

    /// Create a CM client from config.
    pub fn from_config(config: &CmConfig) -> Self {
        let mut client = Self::new();
        if let Some(path) = config.cm_path.as_ref() {
            client.cm_bin = PathBuf::from(path);
        }
        client.default_flags = config.default_flags.clone();
        client
    }

    /// Create a CM client with a custom binary path.
    pub fn with_binary(binary: impl Into<PathBuf>) -> Self {
        Self {
            cm_bin: binary.into(),
            default_flags: Vec::new(),
            safety: None,
        }
    }

    /// Set default flags for cm invocations.
    pub fn with_default_flags(mut self, flags: Vec<String>) -> Self {
        self.default_flags = flags;
        self
    }

    /// Attach a safety gate for command execution.
    pub fn with_safety(mut self, safety: SafetyGate) -> Self {
        self.safety = Some(safety);
        self
    }

    /// Check if CM is available and responsive.
    pub fn is_available(&self) -> bool {
        let mut cmd = Command::new(&self.cm_bin);
        cmd.arg("onboard").arg("status").arg("--json");
        if let Some(gate) = self.safety.as_ref() {
            let command_str = command_string(&cmd);
            if gate.enforce(&command_str, None).is_err() {
                return false;
            }
        }
        cmd.output().map(|o| o.status.success()).unwrap_or(false)
    }

    /// Fetch CM context for a task query.
    pub fn context(&self, task: &str) -> Result<CmContext> {
        let output = self.run_command(&["context", task, "--json"])?;
        serde_json::from_slice(&output)
            .map_err(|e| MsError::CmUnavailable(format!("Failed to parse cm context: {e}")))
    }

    fn run_command(&self, args: &[&str]) -> Result<Vec<u8>> {
        let mut cmd = Command::new(&self.cm_bin);
        for flag in &self.default_flags {
            cmd.arg(flag);
        }
        for arg in args {
            cmd.arg(arg);
        }
        if let Some(gate) = self.safety.as_ref() {
            let command_str = command_string(&cmd);
            gate.enforce(&command_str, None)?;
        }
        let output = cmd.output().map_err(|e| {
            MsError::CmUnavailable(format!("Failed to execute cm: {e}"))
        })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(MsError::CmUnavailable(format!(
                "cm command failed: {}",
                stderr.trim()
            )));
        }
        Ok(output.stdout)
    }
}

impl Default for CmClient {
    fn default() -> Self {
        Self::new()
    }
}

fn command_string(cmd: &Command) -> String {
    let mut parts = Vec::new();
    parts.push(cmd.get_program().to_string_lossy().to_string());
    for arg in cmd.get_args() {
        parts.push(arg.to_string_lossy().to_string());
    }
    parts.join(" ")
}
