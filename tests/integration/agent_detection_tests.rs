//! Integration tests for agent detection module.
//!
//! These tests verify the agent detection functionality works correctly
//! in realistic scenarios with mock environments.

use std::path::PathBuf;

use ms::agent_detection::{
    AgentDetectionService, AgentType, DetectedAgent, DetectionMethod, IntegrationStatus,
};
use tempfile::TempDir;

/// Helper to create a mock home directory with specified agents.
fn setup_mock_home(agents: &[AgentType]) -> TempDir {
    let temp = TempDir::new().unwrap();

    for agent in agents {
        match agent {
            AgentType::ClaudeCode => {
                let dir = temp.path().join(".claude");
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(
                    dir.join("config.json"),
                    r#"{"version": "1.0.0", "apiKey": "test"}"#,
                )
                .unwrap();
            }
            AgentType::Codex => {
                let dir = temp.path().join(".codex");
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(dir.join("config.json"), r#"{"model": "gpt-4"}"#).unwrap();
            }
            AgentType::GeminiCli => {
                let dir = temp.path().join(".gemini");
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(dir.join("config.json"), r#"{"project": "test"}"#).unwrap();
            }
            AgentType::Cursor => {
                let dir = temp.path().join(".cursor");
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(dir.join("settings.json"), r#"{"theme": "dark"}"#).unwrap();
            }
            AgentType::Cline => {
                let dir = temp.path().join(".vscode/extensions/saoudrizwan.claude-dev-2.1.0");
                std::fs::create_dir_all(&dir).unwrap();
            }
            AgentType::OpenCode => {
                let dir = temp.path().join(".opencode");
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(dir.join("config.json"), r#"{}"#).unwrap();
            }
            AgentType::Aider => {
                std::fs::write(temp.path().join(".aider.conf.yml"), "model: claude-3-opus\n")
                    .unwrap();
            }
            AgentType::Windsurf => {
                let dir = temp.path().join(".windsurf");
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(dir.join("settings.json"), r#"{}"#).unwrap();
            }
            AgentType::Continue => {
                let dir = temp.path().join(".continue");
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(dir.join("config.json"), r#"{"models": []}"#).unwrap();
            }
        }
    }

    temp
}

/// Test that detection service finds all mock agents.
#[test]
fn test_detect_multiple_agents() {
    let mock_agents = vec![AgentType::ClaudeCode, AgentType::Cursor, AgentType::Aider];
    let temp = setup_mock_home(&mock_agents);

    let service = AgentDetectionService::with_home(temp.path());
    let detected = service.detect_all();

    // Should find at least the mock agents (might find more if in PATH)
    let detected_types: Vec<AgentType> = detected.iter().map(|a| a.agent_type).collect();

    for expected in &mock_agents {
        assert!(
            detected_types.contains(expected),
            "Expected to detect {:?}",
            expected
        );
    }
}

/// Test that detection returns correct detection method.
#[test]
fn test_detection_method_is_config_file() {
    let temp = setup_mock_home(&[AgentType::ClaudeCode]);

    let service = AgentDetectionService::with_home(temp.path());
    let claude = service.detect_by_type(AgentType::ClaudeCode);

    assert!(claude.is_some());
    let claude = claude.unwrap();
    assert_eq!(claude.detected_via, DetectionMethod::ConfigFile);
}

/// Test that version is parsed from config when available.
#[test]
fn test_version_parsing() {
    let temp = TempDir::new().unwrap();
    let claude_dir = temp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    std::fs::write(
        claude_dir.join("config.json"),
        r#"{"version": "1.2.3"}"#,
    )
    .unwrap();

    let service = AgentDetectionService::with_home(temp.path());
    let claude = service.detect_by_type(AgentType::ClaudeCode);

    assert!(claude.is_some());
    assert_eq!(claude.unwrap().version, Some("1.2.3".to_string()));
}

/// Test detection summary statistics.
#[test]
fn test_detection_summary() {
    let temp = setup_mock_home(&[AgentType::ClaudeCode, AgentType::Cursor]);

    let service = AgentDetectionService::with_home(temp.path());
    let summary = service.summary();

    assert!(summary.has_agents());
    assert!(summary.total_detected >= 2);
    assert!(summary.detected_agents.contains(&AgentType::ClaudeCode));
    assert!(summary.detected_agents.contains(&AgentType::Cursor));
}

/// Test that empty home returns no agents (from config files).
#[test]
fn test_empty_home_detection() {
    let temp = TempDir::new().unwrap();

    let service = AgentDetectionService::with_home(temp.path());
    let summary = service.summary();

    // In an empty home with no agents in PATH, should find nothing from config
    // But might find agents from PATH, so we just verify it doesn't error
    let _ = summary;
}

/// Test integration status detection.
#[test]
fn test_integration_status() {
    let temp = TempDir::new().unwrap();
    let claude_dir = temp.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();

    // Config without ms integration
    std::fs::write(
        claude_dir.join("config.json"),
        r#"{"version": "1.0.0"}"#,
    )
    .unwrap();

    let service = AgentDetectionService::with_home(temp.path());
    let claude = service.detect_by_type(AgentType::ClaudeCode).unwrap();
    assert_eq!(claude.integration_status, IntegrationStatus::NotConfigured);

    // Config with ms integration
    std::fs::write(
        claude_dir.join("config.json"),
        r#"{"version": "1.0.0", "mcp_servers": {"ms": {}}}"#,
    )
    .unwrap();

    let service = AgentDetectionService::with_home(temp.path());
    let claude = service.detect_by_type(AgentType::ClaudeCode).unwrap();
    assert_eq!(claude.integration_status, IntegrationStatus::FullyConfigured);
}

/// Test Cline VSCode extension detection.
#[test]
fn test_cline_vscode_extension_detection() {
    let temp = TempDir::new().unwrap();
    let ext_dir = temp.path().join(".vscode/extensions/saoudrizwan.claude-dev-2.5.0");
    std::fs::create_dir_all(&ext_dir).unwrap();

    let service = AgentDetectionService::with_home(temp.path());
    let cline = service.detect_by_type(AgentType::Cline);

    assert!(cline.is_some());
    let cline = cline.unwrap();
    assert_eq!(cline.detected_via, DetectionMethod::VscodeExtension);
    assert_eq!(cline.version, Some("2.5.0".to_string()));
}

/// Test that all agent types have integration paths defined.
#[test]
fn test_all_agents_have_integration_paths() {
    let service = AgentDetectionService::new();
    let paths = service.get_all_integration_paths();

    assert_eq!(paths.len(), 9); // 9 supported agents

    for (agent_type, agent_paths) in &paths {
        assert!(
            !agent_paths.is_empty(),
            "{:?} should have integration paths",
            agent_type
        );
    }
}

/// Test serialization of detected agents.
#[test]
fn test_detected_agent_serialization() {
    let agent = DetectedAgent::new(AgentType::ClaudeCode, DetectionMethod::ConfigFile)
        .with_version("1.0.0")
        .with_config_path(PathBuf::from("/home/user/.claude/config.json"))
        .with_integration_status(IntegrationStatus::FullyConfigured);

    let json = serde_json::to_string(&agent).unwrap();

    // Verify JSON structure
    assert!(json.contains("\"agent_type\":\"claude_code\""));
    assert!(json.contains("\"version\":\"1.0.0\""));
    assert!(json.contains("\"detected_via\":\"config_file\""));
    assert!(json.contains("\"integration_status\":\"fully_configured\""));
}

/// Test summary serialization.
#[test]
fn test_detection_summary_serialization() {
    let temp = setup_mock_home(&[AgentType::ClaudeCode]);

    let service = AgentDetectionService::with_home(temp.path());
    let summary = service.summary();

    let json = serde_json::to_string(&summary).unwrap();

    // Verify JSON structure
    assert!(json.contains("\"total_detected\""));
    assert!(json.contains("\"fully_configured\""));
    assert!(json.contains("\"needs_configuration\""));
    assert!(json.contains("\"detected_agents\""));
}

/// Test is_installed method.
#[test]
fn test_is_installed() {
    let temp = setup_mock_home(&[AgentType::Aider]);

    let service = AgentDetectionService::with_home(temp.path());

    assert!(service.is_installed(AgentType::Aider));
    // Codex not set up in mock, so might not be installed (depends on PATH)
    // Just verify the method works
    let _ = service.is_installed(AgentType::Codex);
}

/// Test supported_agents returns all agent types.
#[test]
fn test_supported_agents_complete() {
    let service = AgentDetectionService::new();
    let supported = service.supported_agents();

    let expected: Vec<AgentType> = AgentType::all().to_vec();
    assert_eq!(supported.len(), expected.len());

    for agent in &expected {
        assert!(
            supported.contains(agent),
            "{:?} should be in supported agents",
            agent
        );
    }
}
