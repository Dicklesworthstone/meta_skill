//! Agent detection service that orchestrates all detectors.
//!
//! The service provides a unified interface to detect all installed AI coding
//! agents and aggregate their detection results.

use std::path::PathBuf;

use tracing::{debug, info};

use super::detectors::{
    AiderDetector, ClaudeCodeDetector, ClineDetector, CodexDetector, ContinueDetector,
    CursorDetector, GeminiCliDetector, OpenCodeDetector, WindsurfDetector,
};
use super::{AgentDetector, AgentType, DetectedAgent};

/// Service that orchestrates detection of all supported AI coding agents.
pub struct AgentDetectionService {
    detectors: Vec<Box<dyn AgentDetector>>,
    /// Custom home directory override (for testing).
    home_dir: Option<PathBuf>,
}

impl Default for AgentDetectionService {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentDetectionService {
    /// Create a new detection service with all default detectors.
    #[must_use]
    pub fn new() -> Self {
        Self {
            detectors: vec![
                Box::new(ClaudeCodeDetector::new()),
                Box::new(CodexDetector::new()),
                Box::new(GeminiCliDetector::new()),
                Box::new(CursorDetector::new()),
                Box::new(ClineDetector::new()),
                Box::new(OpenCodeDetector::new()),
                Box::new(AiderDetector::new()),
                Box::new(WindsurfDetector::new()),
                Box::new(ContinueDetector::new()),
            ],
            home_dir: None,
        }
    }

    /// Create a detection service with a custom home directory (for testing).
    #[must_use]
    pub fn with_home(home: impl Into<PathBuf>) -> Self {
        let home = home.into();
        Self {
            detectors: vec![
                Box::new(ClaudeCodeDetector::with_home(&home)),
                Box::new(CodexDetector::with_home(&home)),
                Box::new(GeminiCliDetector::with_home(&home)),
                Box::new(CursorDetector::with_home(&home)),
                Box::new(ClineDetector::with_home(&home)),
                Box::new(OpenCodeDetector::with_home(&home)),
                Box::new(AiderDetector::with_home(&home)),
                Box::new(WindsurfDetector::with_home(&home)),
                Box::new(ContinueDetector::with_home(&home)),
            ],
            home_dir: Some(home),
        }
    }

    /// Get the home directory override, if set.
    #[must_use]
    pub fn home_dir(&self) -> Option<&PathBuf> {
        self.home_dir.as_ref()
    }

    /// Detect all installed agents.
    ///
    /// Returns a list of all agents that were found on the system.
    #[must_use]
    pub fn detect_all(&self) -> Vec<DetectedAgent> {
        info!("Starting agent detection scan");

        let mut results = Vec::new();

        for detector in &self.detectors {
            let agent_type = detector.agent_type();
            debug!(agent = ?agent_type, "Checking for agent");

            match detector.detect() {
                Some(agent) => {
                    info!(
                        agent = ?agent_type,
                        version = ?agent.version,
                        status = ?agent.integration_status,
                        method = ?agent.detected_via,
                        "Agent detected"
                    );
                    results.push(agent);
                }
                None => {
                    debug!(agent = ?agent_type, "Agent not found");
                }
            }
        }

        info!(count = results.len(), "Agent detection complete");
        results
    }

    /// Detect a specific agent by type.
    ///
    /// Returns the detection result for the specified agent type, or None if not found.
    #[must_use]
    pub fn detect_by_type(&self, agent_type: AgentType) -> Option<DetectedAgent> {
        debug!(agent = ?agent_type, "Detecting specific agent");

        self.detectors
            .iter()
            .find(|d| d.agent_type() == agent_type)?
            .detect()
    }

    /// Get all agent types that are supported.
    #[must_use]
    pub fn supported_agents(&self) -> Vec<AgentType> {
        self.detectors.iter().map(|d| d.agent_type()).collect()
    }

    /// Get the detector for a specific agent type.
    pub fn get_detector(&self, agent_type: AgentType) -> Option<&dyn AgentDetector> {
        self.detectors
            .iter()
            .find(|d| d.agent_type() == agent_type)
            .map(AsRef::as_ref)
    }

    /// Get integration paths for all detected agents.
    ///
    /// Returns a map of agent types to their integration paths.
    #[must_use]
    pub fn get_all_integration_paths(&self) -> Vec<(AgentType, Vec<PathBuf>)> {
        self.detectors
            .iter()
            .map(|d| (d.agent_type(), d.get_integration_paths()))
            .collect()
    }

    /// Check if a specific agent is installed.
    #[must_use]
    pub fn is_installed(&self, agent_type: AgentType) -> bool {
        self.detect_by_type(agent_type).is_some()
    }

    /// Get a summary of detection results.
    #[must_use]
    pub fn summary(&self) -> DetectionSummary {
        let agents = self.detect_all();
        DetectionSummary::from_agents(&agents)
    }
}

/// Summary of agent detection results.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DetectionSummary {
    /// Total number of agents detected.
    pub total_detected: usize,
    /// Number of agents with full ms integration.
    pub fully_configured: usize,
    /// Number of agents needing configuration.
    pub needs_configuration: usize,
    /// List of detected agent types.
    pub detected_agents: Vec<AgentType>,
    /// List of agents needing configuration.
    pub agents_needing_setup: Vec<AgentType>,
}

impl DetectionSummary {
    /// Create a summary from a list of detected agents.
    #[must_use]
    pub fn from_agents(agents: &[DetectedAgent]) -> Self {
        use super::IntegrationStatus;

        let fully_configured = agents
            .iter()
            .filter(|a| a.integration_status == IntegrationStatus::FullyConfigured)
            .count();

        let needs_configuration = agents
            .iter()
            .filter(|a| a.integration_status.needs_configuration())
            .count();

        let detected_agents: Vec<AgentType> = agents.iter().map(|a| a.agent_type).collect();

        let agents_needing_setup: Vec<AgentType> = agents
            .iter()
            .filter(|a| a.integration_status.needs_configuration())
            .map(|a| a.agent_type)
            .collect();

        Self {
            total_detected: agents.len(),
            fully_configured,
            needs_configuration,
            detected_agents,
            agents_needing_setup,
        }
    }

    /// Check if all detected agents are fully configured.
    #[must_use]
    pub fn all_configured(&self) -> bool {
        self.needs_configuration == 0
    }

    /// Check if any agents were detected.
    #[must_use]
    pub fn has_agents(&self) -> bool {
        self.total_detected > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_service_creation() {
        let service = AgentDetectionService::new();
        assert!(!service.detectors.is_empty());
        assert_eq!(service.detectors.len(), 9); // All 9 agents
    }

    #[test]
    fn test_service_with_custom_home() {
        let temp = TempDir::new().unwrap();
        let service = AgentDetectionService::with_home(temp.path());
        assert!(!service.detectors.is_empty());
        assert!(service.home_dir().is_some());
    }

    #[test]
    fn test_supported_agents() {
        let service = AgentDetectionService::new();
        let supported = service.supported_agents();

        assert_eq!(supported.len(), 9);
        assert!(supported.contains(&AgentType::ClaudeCode));
        assert!(supported.contains(&AgentType::Codex));
        assert!(supported.contains(&AgentType::GeminiCli));
        assert!(supported.contains(&AgentType::Cursor));
        assert!(supported.contains(&AgentType::Cline));
        assert!(supported.contains(&AgentType::OpenCode));
        assert!(supported.contains(&AgentType::Aider));
        assert!(supported.contains(&AgentType::Windsurf));
        assert!(supported.contains(&AgentType::Continue));
    }

    #[test]
    fn test_detect_all_empty_home() {
        let temp = TempDir::new().unwrap();
        let service = AgentDetectionService::with_home(temp.path());

        // In an empty home, should find no agents (unless they're in PATH)
        let agents = service.detect_all();
        // This might find agents if they're in PATH, so just verify it doesn't panic
        let _ = agents;
    }

    #[test]
    fn test_detect_all_with_mock_agents() {
        let temp = TempDir::new().unwrap();

        // Set up mock Claude Code
        let claude_dir = temp.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        std::fs::write(
            claude_dir.join("config.json"),
            r#"{"version": "1.0.0"}"#,
        )
        .unwrap();

        // Set up mock Cursor
        let cursor_dir = temp.path().join(".cursor");
        std::fs::create_dir_all(&cursor_dir).unwrap();
        std::fs::write(cursor_dir.join("settings.json"), r#"{}"#).unwrap();

        let service = AgentDetectionService::with_home(temp.path());
        let agents = service.detect_all();

        // Should find at least Claude Code and Cursor from config files
        let agent_types: Vec<AgentType> = agents.iter().map(|a| a.agent_type).collect();
        assert!(agent_types.contains(&AgentType::ClaudeCode));
        assert!(agent_types.contains(&AgentType::Cursor));
    }

    #[test]
    fn test_detect_by_type() {
        let temp = TempDir::new().unwrap();

        // Set up mock Claude Code
        let claude_dir = temp.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        std::fs::write(
            claude_dir.join("config.json"),
            r#"{"version": "2.0.0"}"#,
        )
        .unwrap();

        let service = AgentDetectionService::with_home(temp.path());

        // Should find Claude Code
        let claude = service.detect_by_type(AgentType::ClaudeCode);
        assert!(claude.is_some());
        assert_eq!(claude.unwrap().version, Some("2.0.0".to_string()));

        // Should not find Codex (not configured)
        let codex = service.detect_by_type(AgentType::Codex);
        // Might be in PATH, so just verify it doesn't panic
        let _ = codex;
    }

    #[test]
    fn test_is_installed() {
        let temp = TempDir::new().unwrap();

        // Set up mock Aider
        std::fs::write(
            temp.path().join(".aider.conf.yml"),
            "model: claude-3-opus\n",
        )
        .unwrap();

        let service = AgentDetectionService::with_home(temp.path());

        assert!(service.is_installed(AgentType::Aider));
    }

    #[test]
    fn test_get_all_integration_paths() {
        let service = AgentDetectionService::new();
        let paths = service.get_all_integration_paths();

        assert_eq!(paths.len(), 9); // One entry per detector

        // Verify Claude Code has expected paths
        let claude_paths = paths
            .iter()
            .find(|(t, _)| *t == AgentType::ClaudeCode)
            .map(|(_, p)| p);
        assert!(claude_paths.is_some());
        let claude_paths = claude_paths.unwrap();
        assert!(claude_paths.iter().any(|p| p.to_string_lossy().contains("SKILL.md")));
    }

    #[test]
    fn test_detection_summary() {
        let temp = TempDir::new().unwrap();

        // Set up two mock agents
        let claude_dir = temp.path().join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        std::fs::write(
            claude_dir.join("config.json"),
            r#"{"version": "1.0.0", "tools": ["ms"]}"#, // Has ms integration
        )
        .unwrap();

        let cursor_dir = temp.path().join(".cursor");
        std::fs::create_dir_all(&cursor_dir).unwrap();
        std::fs::write(cursor_dir.join("settings.json"), r#"{}"#).unwrap(); // No ms

        let service = AgentDetectionService::with_home(temp.path());
        let summary = service.summary();

        assert!(summary.has_agents());
        assert!(summary.total_detected >= 2);
        assert!(summary.detected_agents.contains(&AgentType::ClaudeCode));
        assert!(summary.detected_agents.contains(&AgentType::Cursor));
    }

    #[test]
    fn test_summary_all_configured() {
        let summary = DetectionSummary {
            total_detected: 2,
            fully_configured: 2,
            needs_configuration: 0,
            detected_agents: vec![AgentType::ClaudeCode, AgentType::Cursor],
            agents_needing_setup: vec![],
        };

        assert!(summary.all_configured());
        assert!(summary.has_agents());
    }

    #[test]
    fn test_summary_needs_configuration() {
        let summary = DetectionSummary {
            total_detected: 2,
            fully_configured: 1,
            needs_configuration: 1,
            detected_agents: vec![AgentType::ClaudeCode, AgentType::Cursor],
            agents_needing_setup: vec![AgentType::Cursor],
        };

        assert!(!summary.all_configured());
        assert!(summary.has_agents());
    }

    #[test]
    fn test_get_detector() {
        let service = AgentDetectionService::new();

        let detector = service.get_detector(AgentType::ClaudeCode);
        assert!(detector.is_some());
        assert_eq!(detector.unwrap().agent_type(), AgentType::ClaudeCode);

        // All supported agents should have detectors
        for agent_type in AgentType::all() {
            assert!(
                service.get_detector(*agent_type).is_some(),
                "Missing detector for {:?}",
                agent_type
            );
        }
    }
}
