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
    #[must_use]
    pub fn new() -> Self {
        Self {
            cm_bin: PathBuf::from("cm"),
            default_flags: Vec::new(),
            safety: None,
        }
    }

    /// Create a CM client from config.
    #[must_use]
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
    #[must_use]
    pub fn with_default_flags(mut self, flags: Vec<String>) -> Self {
        self.default_flags = flags;
        self
    }

    /// Attach a safety gate for command execution.
    #[must_use]
    pub fn with_safety(mut self, safety: SafetyGate) -> Self {
        self.safety = Some(safety);
        self
    }

    /// Check if CM is available and responsive.
    #[must_use]
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
        Ok(result
            .get("valid")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false))
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
        let output = cmd
            .output()
            .map_err(|e| MsError::CmUnavailable(format!("Failed to execute cm: {e}")))?;
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

    #[test]
    fn test_from_config_no_path() {
        use crate::config::CmConfig;

        let config = CmConfig {
            enabled: true,
            cm_path: None,
            default_flags: vec!["--verbose".to_string()],
        };

        let client = CmClient::from_config(&config);
        assert_eq!(client.cm_bin, PathBuf::from("cm")); // defaults to "cm"
        assert_eq!(client.default_flags, vec!["--verbose"]);
    }

    #[test]
    fn test_cm_client_default_trait() {
        let client = CmClient::default();
        assert_eq!(client.cm_bin, PathBuf::from("cm"));
        assert!(client.default_flags.is_empty());
        assert!(client.safety.is_none());
    }

    #[test]
    fn test_cm_client_builder_chaining() {
        let client = CmClient::with_binary("/opt/cm")
            .with_default_flags(vec!["--json".to_string(), "-v".to_string()]);
        assert_eq!(client.cm_bin, PathBuf::from("/opt/cm"));
        assert_eq!(client.default_flags, vec!["--json", "-v"]);
    }

    #[test]
    fn test_history_snippet_deserialization() {
        let json = r#"{
            "sessionId": "sess-12345",
            "summary": "User worked on debugging issue",
            "relevance": 0.78
        }"#;

        let snippet: HistorySnippet = serde_json::from_str(json).unwrap();
        assert_eq!(snippet.session_id, "sess-12345");
        assert_eq!(snippet.summary, "User worked on debugging issue");
        assert_eq!(snippet.relevance, 0.78);
    }

    #[test]
    fn test_history_snippet_defaults() {
        let json = r#"{}"#;
        let snippet: HistorySnippet = serde_json::from_str(json).unwrap();
        assert_eq!(snippet.session_id, "");
        assert_eq!(snippet.summary, "");
        assert_eq!(snippet.relevance, 0.0);
    }

    #[test]
    fn test_similar_result_deserialization() {
        let json = r#"{
            "success": true,
            "matches": [
                {"id": "m1", "content": "Match one", "similarity": 0.9, "category": "general"}
            ],
            "query": "test query"
        }"#;

        let result: SimilarResult = serde_json::from_str(json).unwrap();
        assert!(result.success);
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.query, "test query");
    }

    #[test]
    fn test_similar_result_defaults() {
        let json = r#"{}"#;
        let result: SimilarResult = serde_json::from_str(json).unwrap();
        assert!(!result.success);
        assert!(result.matches.is_empty());
        assert_eq!(result.query, "");
    }

    #[test]
    fn test_add_rule_result_deserialization() {
        let json = r#"{
            "success": true,
            "id": "new-rule-001",
            "content": "New rule added"
        }"#;

        let result: AddRuleResult = serde_json::from_str(json).unwrap();
        assert!(result.success);
        assert_eq!(result.id, "new-rule-001");
        assert_eq!(result.content, "New rule added");
    }

    #[test]
    fn test_add_rule_result_defaults() {
        let json = r#"{}"#;
        let result: AddRuleResult = serde_json::from_str(json).unwrap();
        assert!(!result.success);
        assert_eq!(result.id, "");
        assert_eq!(result.content, "");
    }

    #[test]
    fn test_playbook_rule_defaults() {
        let json = r#"{}"#;
        let rule: PlaybookRule = serde_json::from_str(json).unwrap();
        assert_eq!(rule.id, "");
        assert_eq!(rule.content, "");
        assert_eq!(rule.category, "");
        assert_eq!(rule.confidence, 0.0);
        assert_eq!(rule.maturity, "");
        assert_eq!(rule.helpful_count, 0);
        assert_eq!(rule.harmful_count, 0);
        assert!(rule.scope.is_none());
    }

    #[test]
    fn test_playbook_rule_with_scope() {
        let json = r#"{
            "id": "rule-scope",
            "content": "Scoped rule",
            "category": "project",
            "confidence": 0.75,
            "maturity": "new",
            "helpfulCount": 5,
            "harmfulCount": 0,
            "scope": "project:my-project"
        }"#;

        let rule: PlaybookRule = serde_json::from_str(json).unwrap();
        assert_eq!(rule.scope, Some("project:my-project".to_string()));
    }

    #[test]
    fn test_anti_pattern_defaults() {
        let json = r#"{}"#;
        let ap: AntiPattern = serde_json::from_str(json).unwrap();
        assert_eq!(ap.id, "");
        assert_eq!(ap.content, "");
        assert_eq!(ap.reason, "");
        assert_eq!(ap.severity, "");
    }

    #[test]
    fn test_similar_match_defaults() {
        let json = r#"{}"#;
        let m: SimilarMatch = serde_json::from_str(json).unwrap();
        assert_eq!(m.id, "");
        assert_eq!(m.content, "");
        assert_eq!(m.similarity, 0.0);
        assert_eq!(m.category, "");
    }

    #[test]
    fn test_cm_context_defaults() {
        let json = r#"{}"#;
        let ctx: CmContext = serde_json::from_str(json).unwrap();
        assert!(!ctx.success);
        assert_eq!(ctx.task, "");
        assert!(ctx.relevant_bullets.is_empty());
        assert!(ctx.anti_patterns.is_empty());
        assert!(ctx.history_snippets.is_empty());
        assert!(ctx.suggested_cass_queries.is_empty());
    }

    #[test]
    fn test_cm_context_with_populated_arrays() {
        let json = r#"{
            "success": true,
            "task": "complex task",
            "relevantBullets": [
                {"id": "r1", "content": "Rule 1", "category": "general", "confidence": 0.9}
            ],
            "antiPatterns": [
                {"id": "ap1", "content": "Bad pattern", "reason": "Slow", "severity": "medium"}
            ],
            "historySnippets": [
                {"sessionId": "s1", "summary": "Previous session", "relevance": 0.8}
            ],
            "suggestedCassQueries": ["query1", "query2", "query3"]
        }"#;

        let ctx: CmContext = serde_json::from_str(json).unwrap();
        assert!(ctx.success);
        assert_eq!(ctx.task, "complex task");
        assert_eq!(ctx.relevant_bullets.len(), 1);
        assert_eq!(ctx.relevant_bullets[0].id, "r1");
        assert_eq!(ctx.anti_patterns.len(), 1);
        assert_eq!(ctx.anti_patterns[0].severity, "medium");
        assert_eq!(ctx.history_snippets.len(), 1);
        assert_eq!(ctx.history_snippets[0].session_id, "s1");
        assert_eq!(ctx.suggested_cass_queries.len(), 3);
    }

    #[test]
    fn test_playbook_list_result_defaults() {
        let json = r#"{}"#;
        let result: PlaybookListResult = serde_json::from_str(json).unwrap();
        assert!(!result.success);
        assert!(result.rules.is_empty());
        assert_eq!(result.count, 0);
    }

    #[test]
    fn test_playbook_rule_clone() {
        let rule = PlaybookRule {
            id: "clone-test".to_string(),
            content: "Cloneable rule".to_string(),
            category: "test".to_string(),
            confidence: 0.88,
            maturity: "established".to_string(),
            helpful_count: 15,
            harmful_count: 3,
            scope: Some("global".to_string()),
        };

        let cloned = rule.clone();
        assert_eq!(cloned.id, rule.id);
        assert_eq!(cloned.content, rule.content);
        assert_eq!(cloned.confidence, rule.confidence);
        assert_eq!(cloned.scope, rule.scope);
    }

    #[test]
    fn test_cm_context_clone() {
        let ctx = CmContext {
            success: true,
            task: "clone task".to_string(),
            relevant_bullets: vec![],
            anti_patterns: vec![],
            history_snippets: vec![],
            suggested_cass_queries: vec!["q1".to_string()],
        };

        let cloned = ctx.clone();
        assert_eq!(cloned.success, ctx.success);
        assert_eq!(cloned.task, ctx.task);
        assert_eq!(cloned.suggested_cass_queries, ctx.suggested_cass_queries);
    }

    #[test]
    fn test_anti_pattern_clone() {
        let ap = AntiPattern {
            id: "ap-clone".to_string(),
            content: "Anti pattern".to_string(),
            reason: "Problematic".to_string(),
            severity: "high".to_string(),
        };

        let cloned = ap.clone();
        assert_eq!(cloned.id, ap.id);
        assert_eq!(cloned.severity, ap.severity);
    }

    #[test]
    fn test_history_snippet_clone() {
        let snippet = HistorySnippet {
            session_id: "session-clone".to_string(),
            summary: "Test summary".to_string(),
            relevance: 0.65,
        };

        let cloned = snippet.clone();
        assert_eq!(cloned.session_id, snippet.session_id);
        assert_eq!(cloned.relevance, snippet.relevance);
    }

    #[test]
    fn test_similar_match_clone() {
        let m = SimilarMatch {
            id: "match-clone".to_string(),
            content: "Match content".to_string(),
            similarity: 0.95,
            category: "testing".to_string(),
        };

        let cloned = m.clone();
        assert_eq!(cloned.id, m.id);
        assert_eq!(cloned.similarity, m.similarity);
    }

    #[test]
    fn test_similar_result_clone() {
        let result = SimilarResult {
            success: true,
            matches: vec![],
            query: "test query".to_string(),
        };

        let cloned = result.clone();
        assert_eq!(cloned.success, result.success);
        assert_eq!(cloned.query, result.query);
    }

    #[test]
    fn test_add_rule_result_clone() {
        let result = AddRuleResult {
            success: true,
            id: "new-id".to_string(),
            content: "new content".to_string(),
        };

        let cloned = result.clone();
        assert_eq!(cloned.success, result.success);
        assert_eq!(cloned.id, result.id);
    }

    #[test]
    fn test_playbook_list_result_clone() {
        let result = PlaybookListResult {
            success: true,
            rules: vec![],
            count: 5,
        };

        let cloned = result.clone();
        assert_eq!(cloned.success, result.success);
        assert_eq!(cloned.count, result.count);
    }

    #[test]
    fn test_command_string_simple() {
        let mut cmd = Command::new("cm");
        cmd.arg("context");
        cmd.arg("test query");
        let s = command_string(&cmd);
        assert_eq!(s, "cm context test query");
    }

    #[test]
    fn test_command_string_with_flags() {
        let mut cmd = Command::new("/usr/bin/cm");
        cmd.arg("--json");
        cmd.arg("--verbose");
        cmd.arg("playbook");
        cmd.arg("list");
        let s = command_string(&cmd);
        assert_eq!(s, "/usr/bin/cm --json --verbose playbook list");
    }

    #[test]
    fn test_command_string_empty_args() {
        let cmd = Command::new("cm");
        let s = command_string(&cmd);
        assert_eq!(s, "cm");
    }

    #[test]
    fn test_playbook_rule_serde_roundtrip() {
        let rule = PlaybookRule {
            id: "roundtrip".to_string(),
            content: "Test content".to_string(),
            category: "general".to_string(),
            confidence: 0.85,
            maturity: "established".to_string(),
            helpful_count: 10,
            harmful_count: 2,
            scope: Some("project".to_string()),
        };

        let json = serde_json::to_string(&rule).unwrap();
        let deserialized: PlaybookRule = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, rule.id);
        assert_eq!(deserialized.content, rule.content);
        assert_eq!(deserialized.confidence, rule.confidence);
        assert_eq!(deserialized.scope, rule.scope);
    }

    #[test]
    fn test_cm_context_serde_roundtrip() {
        let ctx = CmContext {
            success: true,
            task: "roundtrip task".to_string(),
            relevant_bullets: vec![PlaybookRule {
                id: "r1".to_string(),
                content: "Rule".to_string(),
                category: "cat".to_string(),
                confidence: 0.9,
                maturity: "new".to_string(),
                helpful_count: 0,
                harmful_count: 0,
                scope: None,
            }],
            anti_patterns: vec![],
            history_snippets: vec![],
            suggested_cass_queries: vec!["q1".to_string()],
        };

        let json = serde_json::to_string(&ctx).unwrap();
        let deserialized: CmContext = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.success, ctx.success);
        assert_eq!(deserialized.task, ctx.task);
        assert_eq!(deserialized.relevant_bullets.len(), 1);
        assert_eq!(deserialized.suggested_cass_queries.len(), 1);
    }

    #[test]
    fn test_similar_result_serde_roundtrip() {
        let result = SimilarResult {
            success: true,
            matches: vec![SimilarMatch {
                id: "m1".to_string(),
                content: "content".to_string(),
                similarity: 0.92,
                category: "cat".to_string(),
            }],
            query: "search query".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: SimilarResult = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.success, result.success);
        assert_eq!(deserialized.query, result.query);
        assert_eq!(deserialized.matches.len(), 1);
        assert_eq!(deserialized.matches[0].similarity, 0.92);
    }

    #[test]
    fn test_add_rule_result_serde_roundtrip() {
        let result = AddRuleResult {
            success: true,
            id: "added-rule".to_string(),
            content: "Added content".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: AddRuleResult = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.success, result.success);
        assert_eq!(deserialized.id, result.id);
        assert_eq!(deserialized.content, result.content);
    }

    #[test]
    fn test_cm_client_with_string_binary() {
        let client = CmClient::with_binary(String::from("/path/to/cm"));
        assert_eq!(client.cm_bin, PathBuf::from("/path/to/cm"));
    }

    #[test]
    fn test_cm_client_with_pathbuf_binary() {
        let path = PathBuf::from("/another/path/cm");
        let client = CmClient::with_binary(path.clone());
        assert_eq!(client.cm_bin, path);
    }

    #[test]
    fn test_playbook_rule_debug() {
        let rule = PlaybookRule {
            id: "debug-test".to_string(),
            content: "Debug content".to_string(),
            category: "general".to_string(),
            confidence: 0.5,
            maturity: "new".to_string(),
            helpful_count: 0,
            harmful_count: 0,
            scope: None,
        };

        let debug_str = format!("{:?}", rule);
        assert!(debug_str.contains("debug-test"));
        assert!(debug_str.contains("Debug content"));
    }

    #[test]
    fn test_anti_pattern_debug() {
        let ap = AntiPattern {
            id: "ap-debug".to_string(),
            content: "Bad practice".to_string(),
            reason: "Performance".to_string(),
            severity: "high".to_string(),
        };

        let debug_str = format!("{:?}", ap);
        assert!(debug_str.contains("ap-debug"));
        assert!(debug_str.contains("high"));
    }

    #[test]
    fn test_history_snippet_debug() {
        let snippet = HistorySnippet {
            session_id: "debug-session".to_string(),
            summary: "Debug summary".to_string(),
            relevance: 0.7,
        };

        let debug_str = format!("{:?}", snippet);
        assert!(debug_str.contains("debug-session"));
    }

    #[test]
    fn test_cm_context_debug() {
        let ctx = CmContext {
            success: true,
            task: "debug task".to_string(),
            relevant_bullets: vec![],
            anti_patterns: vec![],
            history_snippets: vec![],
            suggested_cass_queries: vec![],
        };

        let debug_str = format!("{:?}", ctx);
        assert!(debug_str.contains("debug task"));
        assert!(debug_str.contains("true"));
    }

    #[test]
    fn test_similar_match_debug() {
        let m = SimilarMatch {
            id: "match-debug".to_string(),
            content: "Match content".to_string(),
            similarity: 0.88,
            category: "cat".to_string(),
        };

        let debug_str = format!("{:?}", m);
        assert!(debug_str.contains("match-debug"));
        assert!(debug_str.contains("0.88"));
    }

    #[test]
    fn test_playbook_list_result_multiple_rules() {
        let json = r#"{
            "success": true,
            "rules": [
                {"id": "r1", "content": "Rule 1"},
                {"id": "r2", "content": "Rule 2"},
                {"id": "r3", "content": "Rule 3"}
            ],
            "count": 3
        }"#;

        let result: PlaybookListResult = serde_json::from_str(json).unwrap();
        assert!(result.success);
        assert_eq!(result.rules.len(), 3);
        assert_eq!(result.count, 3);
        assert_eq!(result.rules[0].id, "r1");
        assert_eq!(result.rules[2].id, "r3");
    }

    #[test]
    fn test_similar_result_multiple_matches() {
        let json = r#"{
            "success": true,
            "matches": [
                {"id": "m1", "similarity": 0.95},
                {"id": "m2", "similarity": 0.88},
                {"id": "m3", "similarity": 0.75}
            ],
            "query": "search"
        }"#;

        let result: SimilarResult = serde_json::from_str(json).unwrap();
        assert!(result.success);
        assert_eq!(result.matches.len(), 3);
        assert_eq!(result.matches[0].similarity, 0.95);
        assert_eq!(result.matches[2].similarity, 0.75);
    }

    #[test]
    fn test_playbook_rule_float_confidence_precision() {
        let json = r#"{
            "id": "precision",
            "confidence": 0.123456789
        }"#;

        let rule: PlaybookRule = serde_json::from_str(json).unwrap();
        // f32 has limited precision
        assert!(rule.confidence > 0.12 && rule.confidence < 0.13);
    }

    #[test]
    fn test_from_config_empty_flags() {
        use crate::config::CmConfig;

        let config = CmConfig {
            enabled: false,
            cm_path: Some("/cm".to_string()),
            default_flags: vec![],
        };

        let client = CmClient::from_config(&config);
        assert!(client.default_flags.is_empty());
    }
}
