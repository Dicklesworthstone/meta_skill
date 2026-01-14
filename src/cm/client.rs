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
    pub relevant_bullets: Vec<PlaybookRule>,
    #[serde(rename = "antiPatterns", default)]
    pub anti_patterns: Vec<AntiPattern>,
    #[serde(rename = "historySnippets", default)]
    pub history_snippets: Vec<HistorySnippet>,
    #[serde(rename = "suggestedCassQueries", default)]
    pub suggested_cass_queries: Vec<String>,
}

/// A playbook rule from CM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookRule {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub maturity: String,
    #[serde(rename = "helpfulCount", default)]
    pub helpful_count: u32,
    #[serde(rename = "harmfulCount", default)]
    pub harmful_count: u32,
    #[serde(default)]
    pub scope: Option<String>,
}

/// An anti-pattern from CM context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AntiPattern {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub severity: String,
}

/// A history snippet from CM context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistorySnippet {
    #[serde(rename = "sessionId", default)]
    pub session_id: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub relevance: f32,
}

/// Similar rule match result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarMatch {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub similarity: f32,
    #[serde(default)]
    pub category: String,
}

/// Result from `cm playbook list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaybookListResult {
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub rules: Vec<PlaybookRule>,
    #[serde(default)]
    pub count: usize,
}

/// Result from `cm similar`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarResult {
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub matches: Vec<SimilarMatch>,
    #[serde(default)]
    pub query: String,
}

/// Result from `cm playbook add`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddRuleResult {
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub content: String,
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

    /// Get playbook rules, optionally filtered by category.
    pub fn get_rules(&self, category: Option<&str>) -> Result<Vec<PlaybookRule>> {
        let mut args = vec!["playbook", "list", "--json"];
        let cat_arg;
        if let Some(cat) = category {
            args.push("--category");
            cat_arg = cat.to_string();
            args.push(&cat_arg);
        }
        let output = self.run_command(&args)?;
        let result: PlaybookListResult = serde_json::from_slice(&output)
            .map_err(|e| MsError::CmUnavailable(format!("Failed to parse playbook list: {e}")))?;
        Ok(result.rules)
    }

    /// Find similar rules in the playbook.
    pub fn similar(&self, query: &str, threshold: Option<f32>) -> Result<Vec<SimilarMatch>> {
        let mut args = vec!["similar", query, "--json"];
        let threshold_arg;
        if let Some(t) = threshold {
            args.push("--threshold");
            threshold_arg = t.to_string();
            args.push(&threshold_arg);
        }
        let output = self.run_command(&args)?;
        let result: SimilarResult = serde_json::from_slice(&output)
            .map_err(|e| MsError::CmUnavailable(format!("Failed to parse similar result: {e}")))?;
        Ok(result.matches)
    }

    /// Check if a rule with similar content already exists.
    /// Returns the matching rule if found with similarity >= threshold.
    pub fn rule_exists(&self, content: &str, threshold: f32) -> Result<Option<PlaybookRule>> {
        let matches = self.similar(content, Some(threshold))?;
        if let Some(m) = matches.first() {
            // Fetch full rule details
            let rules = self.get_rules(None)?;
            let rule = rules.into_iter().find(|r| r.id == m.id);
            Ok(rule)
        } else {
            Ok(None)
        }
    }

    /// Add a new rule to the playbook.
    pub fn add_rule(&self, content: &str, category: Option<&str>) -> Result<AddRuleResult> {
        let mut args = vec!["playbook", "add", content, "--json"];
        let cat_arg;
        if let Some(cat) = category {
            args.push("--category");
            cat_arg = cat.to_string();
            args.push(&cat_arg);
        }
        let output = self.run_command(&args)?;
        serde_json::from_slice(&output)
            .map_err(|e| MsError::CmUnavailable(format!("Failed to parse add rule result: {e}")))
    }

    /// Validate a proposed rule against CASS history.
    pub fn validate_rule(&self, rule: &str) -> Result<bool> {
        let output = self.run_command(&["validate", rule, "--json"])?;
        // cm validate returns success field
        let result: serde_json::Value = serde_json::from_slice(&output)
            .map_err(|e| MsError::CmUnavailable(format!("Failed to parse validate result: {e}")))?;
        Ok(result.get("valid").and_then(|v| v.as_bool()).unwrap_or(false))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cm_client_default() {
        let client = CmClient::new();
        assert_eq!(client.cm_bin, PathBuf::from("cm"));
        assert!(client.default_flags.is_empty());
        assert!(client.safety.is_none());
    }

    #[test]
    fn test_cm_client_with_binary() {
        let client = CmClient::with_binary("/usr/local/bin/cm");
        assert_eq!(client.cm_bin, PathBuf::from("/usr/local/bin/cm"));
    }

    #[test]
    fn test_cm_client_with_flags() {
        let client = CmClient::new().with_default_flags(vec!["--verbose".to_string()]);
        assert_eq!(client.default_flags, vec!["--verbose"]);
    }

    #[test]
    fn test_playbook_rule_deserialization() {
        let json = r#"{
            "id": "rule-001",
            "content": "Test rule content",
            "category": "general",
            "confidence": 0.85,
            "maturity": "established",
            "helpfulCount": 10,
            "harmfulCount": 2
        }"#;

        let rule: PlaybookRule = serde_json::from_str(json).unwrap();
        assert_eq!(rule.id, "rule-001");
        assert_eq!(rule.content, "Test rule content");
        assert_eq!(rule.category, "general");
        assert_eq!(rule.confidence, 0.85);
        assert_eq!(rule.helpful_count, 10);
        assert_eq!(rule.harmful_count, 2);
    }

    #[test]
    fn test_cm_context_deserialization() {
        let json = r#"{
            "success": true,
            "task": "test task",
            "relevantBullets": [],
            "antiPatterns": [],
            "historySnippets": [],
            "suggestedCassQueries": ["query1", "query2"]
        }"#;

        let ctx: CmContext = serde_json::from_str(json).unwrap();
        assert!(ctx.success);
        assert_eq!(ctx.task, "test task");
        assert!(ctx.relevant_bullets.is_empty());
        assert_eq!(ctx.suggested_cass_queries, vec!["query1", "query2"]);
    }

    #[test]
    fn test_similar_match_deserialization() {
        let json = r#"{
            "id": "match-001",
            "content": "Similar content",
            "similarity": 0.92,
            "category": "debugging"
        }"#;

        let m: SimilarMatch = serde_json::from_str(json).unwrap();
        assert_eq!(m.id, "match-001");
        assert_eq!(m.content, "Similar content");
        assert_eq!(m.similarity, 0.92);
        assert_eq!(m.category, "debugging");
    }

    #[test]
    fn test_playbook_list_result_deserialization() {
        let json = r#"{
            "success": true,
            "rules": [
                {
                    "id": "rule-1",
                    "content": "Rule one",
                    "category": "general",
                    "confidence": 0.9,
                    "maturity": "proven"
                }
            ],
            "count": 1
        }"#;

        let result: PlaybookListResult = serde_json::from_str(json).unwrap();
        assert!(result.success);
        assert_eq!(result.rules.len(), 1);
        assert_eq!(result.count, 1);
    }

    #[test]
    fn test_anti_pattern_deserialization() {
        let json = r#"{
            "id": "ap-001",
            "content": "Don't do this",
            "reason": "Causes issues",
            "severity": "high"
        }"#;

        let ap: AntiPattern = serde_json::from_str(json).unwrap();
        assert_eq!(ap.id, "ap-001");
        assert_eq!(ap.content, "Don't do this");
        assert_eq!(ap.severity, "high");
    }

    #[test]
    fn test_from_config() {
        use crate::config::CmConfig;

        let config = CmConfig {
            enabled: true,
            cm_path: Some("/custom/cm".to_string()),
            default_flags: vec!["--json".to_string()],
        };

        let client = CmClient::from_config(&config);
        assert_eq!(client.cm_bin, PathBuf::from("/custom/cm"));
        assert_eq!(client.default_flags, vec!["--json"]);
    }
}
