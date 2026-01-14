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

    pub fn unavailable(reason: String) -> Self {
        Self {
            allowed: true,
            tier: SafetyTier::Caution,
            reason,
            remediation: None,
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
