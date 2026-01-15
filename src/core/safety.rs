//! Safety invariants and DCG integration

use std::path::PathBuf;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::{MsError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafetyTier {
    Safe,
    Caution,
    Danger,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DcgDecision {
    pub allowed: bool,
    pub tier: SafetyTier,
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pack: Option<String>,
    #[serde(default)]
    pub approved: bool,
}

impl DcgDecision {
    fn allowed(reason: String) -> Self {
        Self {
            allowed: true,
            tier: SafetyTier::Safe,
            reason,
            remediation: None,
            rule_id: None,
            pack: None,
            approved: false,
        }
    }

    /// Create a decision for when DCG is unavailable.
    ///
    /// # Security
    /// This returns `allowed: false` (fail-closed) because when the safety
    /// system cannot evaluate a command, we must assume it could be dangerous.
    /// This is a fundamental security principle: fail-closed, not fail-open.
    pub fn unavailable(reason: String) -> Self {
        Self {
            allowed: false,
            tier: SafetyTier::Critical,
            reason,
            remediation: Some("Install or configure DCG (Destructive Command Guard) to enable command safety evaluation".to_string()),
            rule_id: None,
            pack: None,
            approved: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DcgGuard {
    pub dcg_bin: PathBuf,
    pub packs: Vec<String>,
    pub explain_format: String,
}

impl DcgGuard {
    pub fn new(dcg_bin: PathBuf, packs: Vec<String>, explain_format: String) -> Self {
        Self {
            dcg_bin,
            packs,
            explain_format,
        }
    }

    pub fn version(&self) -> Option<String> {
        let output = Command::new(&self.dcg_bin).arg("--version").output().ok()?;
        if !output.status.success() {
            return None;
        }
        let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if raw.is_empty() {
            None
        } else {
            Some(raw)
        }
    }

    pub fn evaluate_command(&self, command: &str) -> Result<DcgDecision> {
        if command.trim().is_empty() {
            return Ok(DcgDecision::allowed("empty command".to_string()));
        }

        let mut cmd = Command::new(&self.dcg_bin);
        cmd.arg("explain")
            .arg("--format")
            .arg(&self.explain_format)
            .arg(command);

        if !self.packs.is_empty() {
            cmd.env("DCG_PACKS", self.packs.join(","));
        }

        let output = cmd
            .output()
            .map_err(|err| MsError::Config(format!("dcg explain failed: {err}")))?;
        if !output.status.success() {
            return Err(MsError::Config(format!(
                "dcg explain failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        let payload: ExplainOutput = serde_json::from_slice(&output.stdout)
            .map_err(|err| MsError::Config(format!("parse dcg explain: {err}")))?;

        let allowed = payload.decision == "allow";
        let (tier, reason, rule_id, pack) = if let Some(info) = payload.match_info.as_ref() {
            (
                map_severity(info.severity.as_deref()),
                info.reason.clone(),
                info.rule_id.clone(),
                info.pack_id.clone(),
            )
        } else {
            (SafetyTier::Safe, "allowed".to_string(), None, None)
        };

        let remediation = payload
            .suggestions
            .as_ref()
            .and_then(|items| items.first())
            .map(|item| item.text.clone());

        Ok(DcgDecision {
            allowed,
            tier,
            reason,
            remediation,
            rule_id,
            pack,
            approved: false,
        })
    }
}

fn map_severity(value: Option<&str>) -> SafetyTier {
    match value.unwrap_or("high") {
        "critical" => SafetyTier::Critical,
        "high" => SafetyTier::Danger,
        "medium" => SafetyTier::Caution,
        "low" => SafetyTier::Caution,
        _ => SafetyTier::Danger,
    }
}

#[derive(Debug, Deserialize)]
struct ExplainOutput {
    pub decision: String,
    #[serde(rename = "match")]
    pub match_info: Option<MatchInfo>,
    pub suggestions: Option<Vec<Suggestion>>,
}

#[derive(Debug, Deserialize)]
struct MatchInfo {
    pub rule_id: Option<String>,
    pub pack_id: Option<String>,
    pub severity: Option<String>,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
struct Suggestion {
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // SafetyTier tests
    // =========================================================================

    #[test]
    fn safety_tier_ordering() {
        // Verify tiers are ordered: Safe < Caution < Danger < Critical
        assert!(SafetyTier::Safe < SafetyTier::Caution);
        assert!(SafetyTier::Caution < SafetyTier::Danger);
        assert!(SafetyTier::Danger < SafetyTier::Critical);
    }

    #[test]
    fn safety_tier_equality() {
        assert_eq!(SafetyTier::Safe, SafetyTier::Safe);
        assert_ne!(SafetyTier::Safe, SafetyTier::Danger);
    }

    #[test]
    fn safety_tier_serialization() {
        let tier = SafetyTier::Critical;
        let json = serde_json::to_string(&tier).unwrap();
        assert_eq!(json, "\"critical\"");

        let tier = SafetyTier::Caution;
        let json = serde_json::to_string(&tier).unwrap();
        assert_eq!(json, "\"caution\"");
    }

    #[test]
    fn safety_tier_deserialization() {
        let tier: SafetyTier = serde_json::from_str("\"safe\"").unwrap();
        assert_eq!(tier, SafetyTier::Safe);

        let tier: SafetyTier = serde_json::from_str("\"danger\"").unwrap();
        assert_eq!(tier, SafetyTier::Danger);
    }

    // =========================================================================
    // DcgDecision tests
    // =========================================================================

    #[test]
    fn dcg_decision_allowed() {
        let decision = DcgDecision::allowed("safe command".to_string());
        assert!(decision.allowed);
        assert_eq!(decision.tier, SafetyTier::Safe);
        assert_eq!(decision.reason, "safe command");
        assert!(decision.remediation.is_none());
        assert!(!decision.approved);
    }

    #[test]
    fn dcg_decision_unavailable() {
        let decision = DcgDecision::unavailable("dcg not found".to_string());
        assert!(!decision.allowed);
        assert_eq!(decision.tier, SafetyTier::Critical);
        assert_eq!(decision.reason, "dcg not found");
        assert!(decision.remediation.is_some());
        assert!(decision
            .remediation
            .unwrap()
            .contains("Install or configure DCG"));
    }

    #[test]
    fn dcg_decision_serialization() {
        let decision = DcgDecision {
            allowed: false,
            tier: SafetyTier::Danger,
            reason: "destructive operation".to_string(),
            remediation: Some("use a safer alternative".to_string()),
            rule_id: Some("RULE-001".to_string()),
            pack: Some("default".to_string()),
            approved: false,
        };

        let json = serde_json::to_string(&decision).unwrap();
        assert!(json.contains("\"allowed\":false"));
        assert!(json.contains("\"tier\":\"danger\""));
        assert!(json.contains("\"reason\":\"destructive operation\""));
        assert!(json.contains("\"remediation\":\"use a safer alternative\""));
        assert!(json.contains("\"rule_id\":\"RULE-001\""));
    }

    #[test]
    fn dcg_decision_serialization_skips_none() {
        let decision = DcgDecision::allowed("ok".to_string());
        let json = serde_json::to_string(&decision).unwrap();

        // Should not contain these optional fields when None
        assert!(!json.contains("remediation"));
        assert!(!json.contains("rule_id"));
        assert!(!json.contains("pack"));
    }

    // =========================================================================
    // map_severity tests
    // =========================================================================

    #[test]
    fn map_severity_critical() {
        assert_eq!(map_severity(Some("critical")), SafetyTier::Critical);
    }

    #[test]
    fn map_severity_high() {
        assert_eq!(map_severity(Some("high")), SafetyTier::Danger);
    }

    #[test]
    fn map_severity_medium() {
        assert_eq!(map_severity(Some("medium")), SafetyTier::Caution);
    }

    #[test]
    fn map_severity_low() {
        assert_eq!(map_severity(Some("low")), SafetyTier::Caution);
    }

    #[test]
    fn map_severity_unknown() {
        assert_eq!(map_severity(Some("unknown")), SafetyTier::Danger);
    }

    #[test]
    fn map_severity_none_defaults_to_danger() {
        // When no severity is provided, defaults to "high" which maps to Danger
        assert_eq!(map_severity(None), SafetyTier::Danger);
    }

    // =========================================================================
    // DcgGuard tests
    // =========================================================================

    #[test]
    fn dcg_guard_new() {
        let guard = DcgGuard::new(
            PathBuf::from("/usr/bin/dcg"),
            vec!["default".to_string(), "extra".to_string()],
            "json".to_string(),
        );

        assert_eq!(guard.dcg_bin, PathBuf::from("/usr/bin/dcg"));
        assert_eq!(guard.packs.len(), 2);
        assert_eq!(guard.explain_format, "json");
    }

    #[test]
    fn dcg_guard_empty_command_allowed() {
        let guard = DcgGuard::new(
            PathBuf::from("/nonexistent"),
            vec![],
            "json".to_string(),
        );

        // Empty commands are always allowed
        let decision = guard.evaluate_command("").unwrap();
        assert!(decision.allowed);
        assert_eq!(decision.reason, "empty command");
    }

    #[test]
    fn dcg_guard_whitespace_only_allowed() {
        let guard = DcgGuard::new(
            PathBuf::from("/nonexistent"),
            vec![],
            "json".to_string(),
        );

        let decision = guard.evaluate_command("   \t  \n  ").unwrap();
        assert!(decision.allowed);
        assert_eq!(decision.reason, "empty command");
    }
}
