//! Agent-specific detection implementations.
//!
//! Each agent has its own detector that knows where to look for configuration
//! files, binaries, and how to determine version information.

use std::path::PathBuf;
use std::process::Command;

use tracing::{debug, trace};

use super::{AgentDetector, AgentType, DetectedAgent, DetectionMethod, IntegrationStatus};

/// Helper to check if ms integration is configured.
fn check_ms_integration(config_path: &PathBuf) -> IntegrationStatus {
    if let Ok(content) = std::fs::read_to_string(config_path) {
        if content.contains("meta_skill") || content.contains("ms ") || content.contains("\"ms\"") {
            IntegrationStatus::FullyConfigured
        } else {
            IntegrationStatus::NotConfigured
        }
    } else {
        IntegrationStatus::NotConfigured
    }
}

/// Helper to extract version from command output.
fn get_version_from_command(binary: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(binary).args(args).output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);

    // Try to extract version number from output
    // Common patterns: "vX.Y.Z", "version X.Y.Z", "X.Y.Z"
    for line in combined.lines() {
        let line = line.trim();
        // Skip empty lines
        if line.is_empty() {
            continue;
        }

        // Try to find version pattern
        if let Some(version) = extract_version(line) {
            return Some(version);
        }
    }

    None
}

/// Extract version number from a string.
fn extract_version(s: &str) -> Option<String> {
    // Match patterns like "v1.2.3", "1.2.3", "version 1.2.3"
    let s = s.to_lowercase();
    let s = s
        .trim_start_matches("version")
        .trim_start_matches("v")
        .trim();

    // Find the start of a version number (digit)
    let start = s.find(|c: char| c.is_ascii_digit())?;
    let rest = &s[start..];

    // Take characters that are digits or dots
    let version: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();

    // Validate it looks like a version (has at least one dot)
    if version.contains('.') && !version.is_empty() {
        Some(version)
    } else {
        None
    }
}

// ============================================================================
// Claude Code Detector
// ============================================================================

/// Detector for Claude Code by Anthropic.
#[derive(Debug, Default)]
pub struct ClaudeCodeDetector {
    /// Optional override for home directory (for testing).
    home_override: Option<PathBuf>,
}

impl ClaudeCodeDetector {
    /// Create a new detector.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a detector with a custom home directory (for testing).
    #[must_use]
    pub fn with_home(home: impl Into<PathBuf>) -> Self {
        Self {
            home_override: Some(home.into()),
        }
    }

    fn home_dir(&self) -> Option<PathBuf> {
        self.home_override.clone().or_else(dirs::home_dir)
    }

    fn parse_version_from_config(&self, config_path: &PathBuf) -> Option<String> {
        let content = std::fs::read_to_string(config_path).ok()?;
        let json: serde_json::Value = serde_json::from_str(&content).ok()?;
        json.get("version")
            .and_then(|v| v.as_str())
            .map(String::from)
    }
}

impl AgentDetector for ClaudeCodeDetector {
    fn agent_type(&self) -> AgentType {
        AgentType::ClaudeCode
    }

    fn detect(&self) -> Option<DetectedAgent> {
        debug!("Checking for Claude Code");

        // 1. Check config file
        let config_path = self.get_config_path()?;
        trace!(path = ?config_path, "Checking Claude Code config");

        if config_path.exists() {
            let version = self.parse_version_from_config(&config_path);
            let binary_path = which::which("claude").ok();
            let integration_status = check_ms_integration(&config_path);

            return Some(
                DetectedAgent::new(AgentType::ClaudeCode, DetectionMethod::ConfigFile)
                    .with_config_path(config_path)
                    .with_integration_status(integration_status)
                    .with_version(version.unwrap_or_default())
                    .with_binary_path(binary_path.unwrap_or_default()),
            );
        }

        // 2. Check for binary in PATH
        if let Ok(binary_path) = which::which("claude") {
            let version = get_version_from_command("claude", &["--version"]);

            return Some(
                DetectedAgent::new(AgentType::ClaudeCode, DetectionMethod::Binary)
                    .with_binary_path(binary_path)
                    .with_version(version.unwrap_or_default()),
            );
        }

        None
    }

    fn get_config_path(&self) -> Option<PathBuf> {
        self.home_dir().map(|h| h.join(".claude/config.json"))
    }

    fn get_integration_paths(&self) -> Vec<PathBuf> {
        vec![PathBuf::from("SKILL.md"), PathBuf::from(".claude/SKILL.md")]
    }
}

// ============================================================================
// Codex Detector
// ============================================================================

/// Detector for Codex CLI by OpenAI.
#[derive(Debug, Default)]
pub struct CodexDetector {
    home_override: Option<PathBuf>,
}

impl CodexDetector {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_home(home: impl Into<PathBuf>) -> Self {
        Self {
            home_override: Some(home.into()),
        }
    }

    fn home_dir(&self) -> Option<PathBuf> {
        self.home_override.clone().or_else(dirs::home_dir)
    }
}

impl AgentDetector for CodexDetector {
    fn agent_type(&self) -> AgentType {
        AgentType::Codex
    }

    fn detect(&self) -> Option<DetectedAgent> {
        debug!("Checking for Codex");

        // 1. Check config file
        if let Some(config_path) = self.get_config_path() {
            if config_path.exists() {
                let binary_path = which::which("codex").ok();
                let version = binary_path
                    .as_ref()
                    .and_then(|_| get_version_from_command("codex", &["--version"]));
                let integration_status = check_ms_integration(&config_path);

                return Some(
                    DetectedAgent::new(AgentType::Codex, DetectionMethod::ConfigFile)
                        .with_config_path(config_path)
                        .with_integration_status(integration_status)
                        .with_version(version.unwrap_or_default())
                        .with_binary_path(binary_path.unwrap_or_default()),
                );
            }
        }

        // 2. Check for binary in PATH
        if let Ok(binary_path) = which::which("codex") {
            let version = get_version_from_command("codex", &["--version"]);

            return Some(
                DetectedAgent::new(AgentType::Codex, DetectionMethod::Binary)
                    .with_binary_path(binary_path)
                    .with_version(version.unwrap_or_default()),
            );
        }

        None
    }

    fn get_config_path(&self) -> Option<PathBuf> {
        self.home_dir().map(|h| h.join(".codex/config.json"))
    }

    fn get_integration_paths(&self) -> Vec<PathBuf> {
        vec![
            PathBuf::from("AGENTS.md"),
            PathBuf::from(".codex/AGENTS.md"),
        ]
    }
}

// ============================================================================
// Gemini CLI Detector
// ============================================================================

/// Detector for Gemini CLI by Google.
#[derive(Debug, Default)]
pub struct GeminiCliDetector {
    home_override: Option<PathBuf>,
}

impl GeminiCliDetector {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_home(home: impl Into<PathBuf>) -> Self {
        Self {
            home_override: Some(home.into()),
        }
    }

    fn home_dir(&self) -> Option<PathBuf> {
        self.home_override.clone().or_else(dirs::home_dir)
    }
}

impl AgentDetector for GeminiCliDetector {
    fn agent_type(&self) -> AgentType {
        AgentType::GeminiCli
    }

    fn detect(&self) -> Option<DetectedAgent> {
        debug!("Checking for Gemini CLI");

        // 1. Check config file
        if let Some(config_path) = self.get_config_path() {
            if config_path.exists() {
                let binary_path = which::which("gemini").ok();
                let version = binary_path
                    .as_ref()
                    .and_then(|_| get_version_from_command("gemini", &["--version"]));
                let integration_status = check_ms_integration(&config_path);

                return Some(
                    DetectedAgent::new(AgentType::GeminiCli, DetectionMethod::ConfigFile)
                        .with_config_path(config_path)
                        .with_integration_status(integration_status)
                        .with_version(version.unwrap_or_default())
                        .with_binary_path(binary_path.unwrap_or_default()),
                );
            }
        }

        // 2. Check for binary in PATH
        if let Ok(binary_path) = which::which("gemini") {
            let version = get_version_from_command("gemini", &["--version"]);

            return Some(
                DetectedAgent::new(AgentType::GeminiCli, DetectionMethod::Binary)
                    .with_binary_path(binary_path)
                    .with_version(version.unwrap_or_default()),
            );
        }

        None
    }

    fn get_config_path(&self) -> Option<PathBuf> {
        self.home_dir().map(|h| h.join(".gemini/config.json"))
    }

    fn get_integration_paths(&self) -> Vec<PathBuf> {
        vec![
            PathBuf::from("AGENTS.md"),
            PathBuf::from(".gemini/AGENTS.md"),
        ]
    }
}

// ============================================================================
// Cursor Detector
// ============================================================================

/// Detector for Cursor AI-powered editor.
#[derive(Debug, Default)]
pub struct CursorDetector {
    home_override: Option<PathBuf>,
}

impl CursorDetector {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_home(home: impl Into<PathBuf>) -> Self {
        Self {
            home_override: Some(home.into()),
        }
    }

    fn home_dir(&self) -> Option<PathBuf> {
        self.home_override.clone().or_else(dirs::home_dir)
    }
}

impl AgentDetector for CursorDetector {
    fn agent_type(&self) -> AgentType {
        AgentType::Cursor
    }

    fn detect(&self) -> Option<DetectedAgent> {
        debug!("Checking for Cursor");

        // 1. Check config directory
        if let Some(config_path) = self.get_config_path() {
            if config_path.exists() {
                let binary_path = which::which("cursor").ok();
                let version = binary_path
                    .as_ref()
                    .and_then(|_| get_version_from_command("cursor", &["--version"]));
                let integration_status = check_ms_integration(&config_path);

                return Some(
                    DetectedAgent::new(AgentType::Cursor, DetectionMethod::ConfigFile)
                        .with_config_path(config_path)
                        .with_integration_status(integration_status)
                        .with_version(version.unwrap_or_default())
                        .with_binary_path(binary_path.unwrap_or_default()),
                );
            }
        }

        // 2. Check for binary in PATH
        if let Ok(binary_path) = which::which("cursor") {
            let version = get_version_from_command("cursor", &["--version"]);

            return Some(
                DetectedAgent::new(AgentType::Cursor, DetectionMethod::Binary)
                    .with_binary_path(binary_path)
                    .with_version(version.unwrap_or_default()),
            );
        }

        None
    }

    fn get_config_path(&self) -> Option<PathBuf> {
        self.home_dir().map(|h| h.join(".cursor/settings.json"))
    }

    fn get_integration_paths(&self) -> Vec<PathBuf> {
        vec![
            PathBuf::from(".cursor/rules"),
            PathBuf::from(".cursorrules"),
        ]
    }
}

// ============================================================================
// Cline Detector
// ============================================================================

/// Detector for Cline VSCode extension.
#[derive(Debug, Default)]
pub struct ClineDetector {
    home_override: Option<PathBuf>,
}

impl ClineDetector {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_home(home: impl Into<PathBuf>) -> Self {
        Self {
            home_override: Some(home.into()),
        }
    }

    fn home_dir(&self) -> Option<PathBuf> {
        self.home_override.clone().or_else(dirs::home_dir)
    }

    fn vscode_extensions_dir(&self) -> Option<PathBuf> {
        self.home_dir().map(|h| h.join(".vscode/extensions"))
    }
}

impl AgentDetector for ClineDetector {
    fn agent_type(&self) -> AgentType {
        AgentType::Cline
    }

    fn detect(&self) -> Option<DetectedAgent> {
        debug!("Checking for Cline");

        // Check VSCode extensions directory for Cline
        let extensions_dir = self.vscode_extensions_dir()?;

        if extensions_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&extensions_dir) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_lowercase();
                    if name.contains("cline") || name.contains("saoudrizwan.claude-dev") {
                        // Found Cline extension
                        let version = extract_version(&name);

                        return Some(
                            DetectedAgent::new(AgentType::Cline, DetectionMethod::VscodeExtension)
                                .with_config_path(entry.path())
                                .with_version(version.unwrap_or_default()),
                        );
                    }
                }
            }
        }

        // Also check for cline binary
        if let Ok(binary_path) = which::which("cline") {
            let version = get_version_from_command("cline", &["--version"]);

            return Some(
                DetectedAgent::new(AgentType::Cline, DetectionMethod::Binary)
                    .with_binary_path(binary_path)
                    .with_version(version.unwrap_or_default()),
            );
        }

        None
    }

    fn get_config_path(&self) -> Option<PathBuf> {
        // Cline uses VSCode settings
        self.home_dir().map(|h| h.join(".vscode/settings.json"))
    }

    fn get_integration_paths(&self) -> Vec<PathBuf> {
        vec![PathBuf::from(".clinerules"), PathBuf::from("AGENTS.md")]
    }
}

// ============================================================================
// OpenCode Detector
// ============================================================================

/// Detector for OpenCode CLI.
#[derive(Debug, Default)]
pub struct OpenCodeDetector {
    home_override: Option<PathBuf>,
}

impl OpenCodeDetector {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_home(home: impl Into<PathBuf>) -> Self {
        Self {
            home_override: Some(home.into()),
        }
    }

    fn home_dir(&self) -> Option<PathBuf> {
        self.home_override.clone().or_else(dirs::home_dir)
    }
}

impl AgentDetector for OpenCodeDetector {
    fn agent_type(&self) -> AgentType {
        AgentType::OpenCode
    }

    fn detect(&self) -> Option<DetectedAgent> {
        debug!("Checking for OpenCode");

        // 1. Check config file
        if let Some(config_path) = self.get_config_path() {
            if config_path.exists() {
                let binary_path = which::which("opencode").ok();
                let version = binary_path
                    .as_ref()
                    .and_then(|_| get_version_from_command("opencode", &["--version"]));
                let integration_status = check_ms_integration(&config_path);

                return Some(
                    DetectedAgent::new(AgentType::OpenCode, DetectionMethod::ConfigFile)
                        .with_config_path(config_path)
                        .with_integration_status(integration_status)
                        .with_version(version.unwrap_or_default())
                        .with_binary_path(binary_path.unwrap_or_default()),
                );
            }
        }

        // 2. Check for binary in PATH
        if let Ok(binary_path) = which::which("opencode") {
            let version = get_version_from_command("opencode", &["--version"]);

            return Some(
                DetectedAgent::new(AgentType::OpenCode, DetectionMethod::Binary)
                    .with_binary_path(binary_path)
                    .with_version(version.unwrap_or_default()),
            );
        }

        None
    }

    fn get_config_path(&self) -> Option<PathBuf> {
        self.home_dir().map(|h| h.join(".opencode/config.json"))
    }

    fn get_integration_paths(&self) -> Vec<PathBuf> {
        vec![
            PathBuf::from("AGENTS.md"),
            PathBuf::from(".opencode/AGENTS.md"),
        ]
    }
}

// ============================================================================
// Aider Detector
// ============================================================================

/// Detector for Aider AI pair programming tool.
#[derive(Debug, Default)]
pub struct AiderDetector {
    home_override: Option<PathBuf>,
}

impl AiderDetector {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_home(home: impl Into<PathBuf>) -> Self {
        Self {
            home_override: Some(home.into()),
        }
    }

    fn home_dir(&self) -> Option<PathBuf> {
        self.home_override.clone().or_else(dirs::home_dir)
    }
}

impl AgentDetector for AiderDetector {
    fn agent_type(&self) -> AgentType {
        AgentType::Aider
    }

    fn detect(&self) -> Option<DetectedAgent> {
        debug!("Checking for Aider");

        // 1. Check config file
        if let Some(config_path) = self.get_config_path() {
            if config_path.exists() {
                let binary_path = which::which("aider").ok();
                let version = binary_path
                    .as_ref()
                    .and_then(|_| get_version_from_command("aider", &["--version"]));
                let integration_status = check_ms_integration(&config_path);

                return Some(
                    DetectedAgent::new(AgentType::Aider, DetectionMethod::ConfigFile)
                        .with_config_path(config_path)
                        .with_integration_status(integration_status)
                        .with_version(version.unwrap_or_default())
                        .with_binary_path(binary_path.unwrap_or_default()),
                );
            }
        }

        // 2. Check for binary in PATH
        if let Ok(binary_path) = which::which("aider") {
            let version = get_version_from_command("aider", &["--version"]);

            return Some(
                DetectedAgent::new(AgentType::Aider, DetectionMethod::Binary)
                    .with_binary_path(binary_path)
                    .with_version(version.unwrap_or_default()),
            );
        }

        None
    }

    fn get_config_path(&self) -> Option<PathBuf> {
        self.home_dir().map(|h| h.join(".aider.conf.yml"))
    }

    fn get_integration_paths(&self) -> Vec<PathBuf> {
        vec![
            PathBuf::from(".aider.conf.yml"),
            PathBuf::from("CONVENTIONS.md"),
        ]
    }
}

// ============================================================================
// Windsurf Detector
// ============================================================================

/// Detector for Windsurf IDE.
#[derive(Debug, Default)]
pub struct WindsurfDetector {
    home_override: Option<PathBuf>,
}

impl WindsurfDetector {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_home(home: impl Into<PathBuf>) -> Self {
        Self {
            home_override: Some(home.into()),
        }
    }

    fn home_dir(&self) -> Option<PathBuf> {
        self.home_override.clone().or_else(dirs::home_dir)
    }
}

impl AgentDetector for WindsurfDetector {
    fn agent_type(&self) -> AgentType {
        AgentType::Windsurf
    }

    fn detect(&self) -> Option<DetectedAgent> {
        debug!("Checking for Windsurf");

        // 1. Check config directory
        if let Some(config_path) = self.get_config_path() {
            if config_path.exists() {
                let binary_path = which::which("windsurf").ok();
                let version = binary_path
                    .as_ref()
                    .and_then(|_| get_version_from_command("windsurf", &["--version"]));
                let integration_status = check_ms_integration(&config_path);

                return Some(
                    DetectedAgent::new(AgentType::Windsurf, DetectionMethod::ConfigFile)
                        .with_config_path(config_path)
                        .with_integration_status(integration_status)
                        .with_version(version.unwrap_or_default())
                        .with_binary_path(binary_path.unwrap_or_default()),
                );
            }
        }

        // 2. Check for binary in PATH
        if let Ok(binary_path) = which::which("windsurf") {
            let version = get_version_from_command("windsurf", &["--version"]);

            return Some(
                DetectedAgent::new(AgentType::Windsurf, DetectionMethod::Binary)
                    .with_binary_path(binary_path)
                    .with_version(version.unwrap_or_default()),
            );
        }

        None
    }

    fn get_config_path(&self) -> Option<PathBuf> {
        self.home_dir().map(|h| h.join(".windsurf/settings.json"))
    }

    fn get_integration_paths(&self) -> Vec<PathBuf> {
        vec![PathBuf::from(".windsurfrules"), PathBuf::from("AGENTS.md")]
    }
}

// ============================================================================
// Continue Detector
// ============================================================================

/// Detector for Continue - open-source AI code assistant.
#[derive(Debug, Default)]
pub struct ContinueDetector {
    home_override: Option<PathBuf>,
}

impl ContinueDetector {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_home(home: impl Into<PathBuf>) -> Self {
        Self {
            home_override: Some(home.into()),
        }
    }

    fn home_dir(&self) -> Option<PathBuf> {
        self.home_override.clone().or_else(dirs::home_dir)
    }
}

impl AgentDetector for ContinueDetector {
    fn agent_type(&self) -> AgentType {
        AgentType::Continue
    }

    fn detect(&self) -> Option<DetectedAgent> {
        debug!("Checking for Continue");

        // 1. Check config file
        if let Some(config_path) = self.get_config_path() {
            if config_path.exists() {
                let integration_status = check_ms_integration(&config_path);

                return Some(
                    DetectedAgent::new(AgentType::Continue, DetectionMethod::ConfigFile)
                        .with_config_path(config_path)
                        .with_integration_status(integration_status),
                );
            }
        }

        // 2. Check for continue binary
        if let Ok(binary_path) = which::which("continue") {
            let version = get_version_from_command("continue", &["--version"]);

            return Some(
                DetectedAgent::new(AgentType::Continue, DetectionMethod::Binary)
                    .with_binary_path(binary_path)
                    .with_version(version.unwrap_or_default()),
            );
        }

        None
    }

    fn get_config_path(&self) -> Option<PathBuf> {
        self.home_dir().map(|h| h.join(".continue/config.json"))
    }

    fn get_integration_paths(&self) -> Vec<PathBuf> {
        vec![
            PathBuf::from(".continuerules"),
            PathBuf::from(".continue/rules"),
        ]
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_extract_version_patterns() {
        assert_eq!(extract_version("v1.2.3"), Some("1.2.3".to_string()));
        assert_eq!(extract_version("version 1.2.3"), Some("1.2.3".to_string()));
        assert_eq!(extract_version("1.2.3"), Some("1.2.3".to_string()));
        assert_eq!(
            extract_version("claude v1.0.15 (build 123)"),
            Some("1.0.15".to_string())
        );
        assert_eq!(extract_version("no version here"), None);
        assert_eq!(extract_version("123"), None); // No dot
    }

    #[test]
    fn test_claude_code_detection_from_config() {
        let temp = TempDir::new().unwrap();
        let config_dir = temp.path().join(".claude");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(config_dir.join("config.json"), r#"{"version": "1.2.3"}"#).unwrap();

        let detector = ClaudeCodeDetector::with_home(temp.path());
        let result = detector.detect();

        assert!(result.is_some());
        let agent = result.unwrap();
        assert_eq!(agent.agent_type, AgentType::ClaudeCode);
        assert_eq!(agent.version, Some("1.2.3".to_string()));
        assert_eq!(agent.detected_via, DetectionMethod::ConfigFile);
    }

    #[test]
    fn test_claude_code_not_found() {
        let temp = TempDir::new().unwrap();
        // No config or binary

        let detector = ClaudeCodeDetector::with_home(temp.path());
        let result = detector.detect();

        // Result depends on whether claude is in PATH on the system
        // For isolated tests, we just verify the detector doesn't panic
        let _ = result;
    }

    #[test]
    fn test_cursor_detection_from_config() {
        let temp = TempDir::new().unwrap();
        let config_dir = temp.path().join(".cursor");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(config_dir.join("settings.json"), r#"{"theme": "dark"}"#).unwrap();

        let detector = CursorDetector::with_home(temp.path());
        let result = detector.detect();

        assert!(result.is_some());
        let agent = result.unwrap();
        assert_eq!(agent.agent_type, AgentType::Cursor);
        assert_eq!(agent.detected_via, DetectionMethod::ConfigFile);
    }

    #[test]
    fn test_aider_detection_from_config() {
        let temp = TempDir::new().unwrap();
        std::fs::write(
            temp.path().join(".aider.conf.yml"),
            "model: gpt-4\nauto-commits: false\n",
        )
        .unwrap();

        let detector = AiderDetector::with_home(temp.path());
        let result = detector.detect();

        assert!(result.is_some());
        let agent = result.unwrap();
        assert_eq!(agent.agent_type, AgentType::Aider);
        assert_eq!(agent.detected_via, DetectionMethod::ConfigFile);
    }

    #[test]
    fn test_cline_detection_from_vscode_extension() {
        let temp = TempDir::new().unwrap();
        let extensions_dir = temp.path().join(".vscode/extensions");
        std::fs::create_dir_all(&extensions_dir).unwrap();
        std::fs::create_dir(extensions_dir.join("saoudrizwan.claude-dev-2.1.0")).unwrap();

        let detector = ClineDetector::with_home(temp.path());
        let result = detector.detect();

        assert!(result.is_some());
        let agent = result.unwrap();
        assert_eq!(agent.agent_type, AgentType::Cline);
        assert_eq!(agent.detected_via, DetectionMethod::VscodeExtension);
        assert_eq!(agent.version, Some("2.1.0".to_string()));
    }

    #[test]
    fn test_continue_detection_from_config() {
        let temp = TempDir::new().unwrap();
        let config_dir = temp.path().join(".continue");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(config_dir.join("config.json"), r#"{"models": []}"#).unwrap();

        let detector = ContinueDetector::with_home(temp.path());
        let result = detector.detect();

        assert!(result.is_some());
        let agent = result.unwrap();
        assert_eq!(agent.agent_type, AgentType::Continue);
        assert_eq!(agent.detected_via, DetectionMethod::ConfigFile);
    }

    #[test]
    fn test_integration_status_detection() {
        let temp = TempDir::new().unwrap();
        let config_dir = temp.path().join(".claude");
        std::fs::create_dir_all(&config_dir).unwrap();

        // Config without ms integration
        std::fs::write(config_dir.join("config.json"), r#"{"version": "1.0.0"}"#).unwrap();

        let status = check_ms_integration(&config_dir.join("config.json"));
        assert_eq!(status, IntegrationStatus::NotConfigured);

        // Config with ms integration
        std::fs::write(
            config_dir.join("config.json"),
            r#"{"version": "1.0.0", "tools": ["ms"]}"#,
        )
        .unwrap();

        let status = check_ms_integration(&config_dir.join("config.json"));
        assert_eq!(status, IntegrationStatus::FullyConfigured);
    }

    #[test]
    fn test_all_detectors_have_integration_paths() {
        let detectors: Vec<Box<dyn AgentDetector>> = vec![
            Box::new(ClaudeCodeDetector::new()),
            Box::new(CodexDetector::new()),
            Box::new(GeminiCliDetector::new()),
            Box::new(CursorDetector::new()),
            Box::new(ClineDetector::new()),
            Box::new(OpenCodeDetector::new()),
            Box::new(AiderDetector::new()),
            Box::new(WindsurfDetector::new()),
            Box::new(ContinueDetector::new()),
        ];

        for detector in detectors {
            let paths = detector.get_integration_paths();
            assert!(
                !paths.is_empty(),
                "Detector for {:?} should have integration paths",
                detector.agent_type()
            );
        }
    }
}
