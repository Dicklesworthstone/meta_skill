//! Agent detection module for identifying installed AI coding agents.
//!
//! This module provides functionality to detect which AI coding agents are installed
//! on the user's system, enabling automatic configuration and integration.
//!
//! # Supported Agents
//!
//! - Claude Code (Anthropic)
//! - Codex (OpenAI)
//! - Gemini CLI (Google)
//! - Cursor
//! - Cline (VSCode extension)
//! - OpenCode
//! - Aider
//! - Windsurf
//! - Continue
//!
//! # Example
//!
//! ```rust,no_run
//! use meta_skill::agent_detection::{AgentDetectionService, AgentType};
//!
//! let service = AgentDetectionService::new();
//! let agents = service.detect_all();
//!
//! for agent in agents {
//!     println!("Found: {:?} v{:?}", agent.agent_type, agent.version);
//! }
//! ```

mod detectors;
mod service;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub use detectors::{
    AiderDetector, ClaudeCodeDetector, ClineDetector, CodexDetector, ContinueDetector,
    CursorDetector, GeminiCliDetector, OpenCodeDetector, WindsurfDetector,
};
pub use service::{AgentDetectionService, DetectionSummary};

/// Supported AI coding agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentType {
    /// Claude Code by Anthropic
    ClaudeCode,
    /// Codex CLI by OpenAI
    Codex,
    /// Gemini CLI by Google
    GeminiCli,
    /// Cursor AI-powered editor
    Cursor,
    /// Cline VSCode extension
    Cline,
    /// OpenCode CLI
    OpenCode,
    /// Aider - AI pair programming
    Aider,
    /// Windsurf IDE
    Windsurf,
    /// Continue - open-source AI code assistant
    Continue,
}

impl AgentType {
    /// Get the display name for this agent type.
    #[must_use]
    pub const fn display_name(&self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
            Self::GeminiCli => "Gemini CLI",
            Self::Cursor => "Cursor",
            Self::Cline => "Cline",
            Self::OpenCode => "OpenCode",
            Self::Aider => "Aider",
            Self::Windsurf => "Windsurf",
            Self::Continue => "Continue",
        }
    }

    /// Get the typical binary name for this agent.
    #[must_use]
    pub const fn binary_name(&self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude",
            Self::Codex => "codex",
            Self::GeminiCli => "gemini",
            Self::Cursor => "cursor",
            Self::Cline => "cline",
            Self::OpenCode => "opencode",
            Self::Aider => "aider",
            Self::Windsurf => "windsurf",
            Self::Continue => "continue",
        }
    }

    /// Get all supported agent types.
    #[must_use]
    pub const fn all() -> &'static [AgentType] {
        &[
            Self::ClaudeCode,
            Self::Codex,
            Self::GeminiCli,
            Self::Cursor,
            Self::Cline,
            Self::OpenCode,
            Self::Aider,
            Self::Windsurf,
            Self::Continue,
        ]
    }
}

impl std::fmt::Display for AgentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Detection result for a single agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedAgent {
    /// The type of agent detected.
    pub agent_type: AgentType,
    /// Version if it could be determined.
    pub version: Option<String>,
    /// Path to the agent's configuration file.
    pub config_path: Option<PathBuf>,
    /// Path to the agent's binary.
    pub binary_path: Option<PathBuf>,
    /// Current integration status with ms.
    pub integration_status: IntegrationStatus,
    /// How the agent was detected.
    pub detected_via: DetectionMethod,
}

impl DetectedAgent {
    /// Create a new detected agent.
    #[must_use]
    pub fn new(agent_type: AgentType, detected_via: DetectionMethod) -> Self {
        Self {
            agent_type,
            version: None,
            config_path: None,
            binary_path: None,
            integration_status: IntegrationStatus::NotConfigured,
            detected_via,
        }
    }

    /// Set the version.
    #[must_use]
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Set the config path.
    #[must_use]
    pub fn with_config_path(mut self, path: PathBuf) -> Self {
        self.config_path = Some(path);
        self
    }

    /// Set the binary path.
    #[must_use]
    pub fn with_binary_path(mut self, path: PathBuf) -> Self {
        self.binary_path = Some(path);
        self
    }

    /// Set the integration status.
    #[must_use]
    pub fn with_integration_status(mut self, status: IntegrationStatus) -> Self {
        self.integration_status = status;
        self
    }
}

/// Integration status of an agent with ms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationStatus {
    /// Agent is not configured to use ms.
    #[default]
    NotConfigured,
    /// Agent has partial ms integration (e.g., missing some config).
    PartiallyConfigured,
    /// Agent is fully configured to use ms.
    FullyConfigured,
    /// Agent's ms integration is outdated.
    Outdated,
}

impl IntegrationStatus {
    /// Check if the agent needs configuration.
    #[must_use]
    pub const fn needs_configuration(&self) -> bool {
        matches!(
            self,
            Self::NotConfigured | Self::PartiallyConfigured | Self::Outdated
        )
    }
}

/// How an agent was detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionMethod {
    /// Detected via configuration file.
    ConfigFile,
    /// Detected via binary in PATH.
    Binary,
    /// Detected via running process.
    ProcessRunning,
    /// Detected via environment variable.
    EnvironmentVariable,
    /// Detected via VSCode extension.
    VscodeExtension,
}

impl std::fmt::Display for DetectionMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConfigFile => write!(f, "config file"),
            Self::Binary => write!(f, "binary"),
            Self::ProcessRunning => write!(f, "running process"),
            Self::EnvironmentVariable => write!(f, "environment variable"),
            Self::VscodeExtension => write!(f, "VSCode extension"),
        }
    }
}

/// Trait for agent-specific detection logic.
pub trait AgentDetector: Send + Sync {
    /// Get the agent type this detector handles.
    fn agent_type(&self) -> AgentType;

    /// Detect if the agent is installed.
    fn detect(&self) -> Option<DetectedAgent>;

    /// Get the expected config path for this agent.
    fn get_config_path(&self) -> Option<PathBuf>;

    /// Get paths where this agent looks for SKILL.md or similar.
    fn get_integration_paths(&self) -> Vec<PathBuf>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_type_display_names() {
        assert_eq!(AgentType::ClaudeCode.display_name(), "Claude Code");
        assert_eq!(AgentType::GeminiCli.display_name(), "Gemini CLI");
        assert_eq!(AgentType::Aider.display_name(), "Aider");
    }

    #[test]
    fn test_agent_type_binary_names() {
        assert_eq!(AgentType::ClaudeCode.binary_name(), "claude");
        assert_eq!(AgentType::Codex.binary_name(), "codex");
        assert_eq!(AgentType::Aider.binary_name(), "aider");
    }

    #[test]
    fn test_agent_type_all() {
        let all = AgentType::all();
        assert_eq!(all.len(), 9);
        assert!(all.contains(&AgentType::ClaudeCode));
        assert!(all.contains(&AgentType::Continue));
    }

    #[test]
    fn test_detected_agent_builder() {
        let agent = DetectedAgent::new(AgentType::ClaudeCode, DetectionMethod::ConfigFile)
            .with_version("1.2.3")
            .with_config_path(PathBuf::from("/home/user/.claude/config.json"))
            .with_integration_status(IntegrationStatus::FullyConfigured);

        assert_eq!(agent.agent_type, AgentType::ClaudeCode);
        assert_eq!(agent.version, Some("1.2.3".to_string()));
        assert!(agent.config_path.is_some());
        assert_eq!(agent.integration_status, IntegrationStatus::FullyConfigured);
    }

    #[test]
    fn test_integration_status_needs_configuration() {
        assert!(IntegrationStatus::NotConfigured.needs_configuration());
        assert!(IntegrationStatus::PartiallyConfigured.needs_configuration());
        assert!(IntegrationStatus::Outdated.needs_configuration());
        assert!(!IntegrationStatus::FullyConfigured.needs_configuration());
    }

    #[test]
    fn test_agent_type_serialization() {
        let agent = AgentType::ClaudeCode;
        let json = serde_json::to_string(&agent).unwrap();
        assert_eq!(json, "\"claude_code\"");

        let parsed: AgentType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, AgentType::ClaudeCode);
    }

    #[test]
    fn test_detected_agent_serialization() {
        let agent = DetectedAgent::new(AgentType::Cursor, DetectionMethod::Binary)
            .with_version("0.50.0");

        let json = serde_json::to_string(&agent).unwrap();
        assert!(json.contains("\"cursor\""));
        assert!(json.contains("\"binary\""));
        assert!(json.contains("\"0.50.0\""));
    }
}
