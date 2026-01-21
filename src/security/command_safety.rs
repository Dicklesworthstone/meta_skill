//! Command safety gate backed by DCG (Destructive Command Guard).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tracing::{info, warn};

use crate::app::AppContext;
use crate::config::Config;
use crate::core::safety::{DcgDecision, DcgGuard, SafetyTier};
use crate::error::{MsError, Result};
use crate::storage::Database;

#[derive(Debug, Clone, serde::Serialize)]
pub struct CommandSafetyEvent {
    pub session_id: Option<String>,
    pub command: String,
    pub dcg_version: Option<String>,
    pub dcg_pack: Option<String>,
    pub decision: DcgDecision,
    pub created_at: String,
}

/// Status of the safety gate and DCG availability.
#[derive(Debug, Clone)]
pub struct SafetyStatus {
    /// DCG version if available.
    pub dcg_version: Option<String>,
    /// Path to dcg binary.
    pub dcg_bin: PathBuf,
    /// Loaded packs.
    pub packs: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SafetyGate {
    guard: DcgGuard,
    dcg_version: Option<String>,
    require_verbatim_approval: bool,
    db: Option<Arc<Database>>,
}

impl SafetyGate {
    #[must_use]
    pub fn from_context(ctx: &AppContext) -> Self {
        let guard = DcgGuard::new(
            ctx.config.safety.dcg_bin.clone(),
            ctx.config.safety.dcg_packs.clone(),
            ctx.config.safety.dcg_explain_format.clone(),
        );
        let dcg_version = guard.version();
        Self {
            guard,
            dcg_version,
            require_verbatim_approval: ctx.config.safety.require_verbatim_approval,
            db: Some(ctx.db.clone()),
        }
    }

    pub fn from_env() -> Result<Self> {
        let ms_root = find_ms_root()?;
        let config = Config::load(None, &ms_root)?;
        let db_path = ms_root.join("ms.db");
        let db = match Database::open(&db_path) {
            Ok(db) => Some(Arc::new(db)),
            Err(err) => {
                warn!(
                    "safety gate: could not open database at {}: {err}",
                    db_path.display()
                );
                None
            }
        };
        let guard = DcgGuard::new(
            config.safety.dcg_bin.clone(),
            config.safety.dcg_packs.clone(),
            config.safety.dcg_explain_format.clone(),
        );
        let dcg_version = guard.version();
        Ok(Self {
            guard,
            dcg_version,
            require_verbatim_approval: config.safety.require_verbatim_approval,
            db,
        })
    }

    /// Get the current status of the safety gate.
    #[must_use]
    pub fn status(&self) -> SafetyStatus {
        SafetyStatus {
            dcg_version: self.dcg_version.clone(),
            dcg_bin: self.guard.dcg_bin.clone(),
            packs: self.guard.packs.clone(),
        }
    }

    pub fn enforce(&self, command: &str, session_id: Option<&str>) -> Result<()> {
        let (mut decision, dcg_unavailable) = match self.guard.evaluate_command(command) {
            Ok(decision) => (decision, false),
            Err(err) => {
                warn!("dcg unavailable: {err}");
                (
                    DcgDecision::unavailable(format!("dcg unavailable: {err}")),
                    true,
                )
            }
        };

        if !decision.allowed {
            // If DCG is unavailable, provide a specific error explaining the situation
            if dcg_unavailable {
                self.log_event(command, &decision, session_id)?;
                return Err(MsError::DestructiveBlocked(format!(
                    "command blocked (safety system unavailable): {}. {}",
                    decision.reason,
                    decision
                        .remediation
                        .as_deref()
                        .unwrap_or("Install DCG to enable command evaluation")
                )));
            }

            if self.require_verbatim_approval && decision.tier >= SafetyTier::Danger {
                if approval_matches(command) {
                    decision.approved = true;
                    decision.allowed = true;
                } else {
                    self.log_event(command, &decision, session_id)?;
                    return Err(MsError::ApprovalRequired(approval_hint(command)));
                }
            } else {
                self.log_event(command, &decision, session_id)?;
                return Err(MsError::DestructiveBlocked(format!(
                    "blocked by dcg: {}",
                    decision.reason
                )));
            }
        }

        self.log_event(command, &decision, session_id)?;

        if decision.approved {
            info!("command approved by verbatim match: {command}");
        }

        Ok(())
    }

    fn log_event(
        &self,
        command: &str,
        decision: &DcgDecision,
        session_id: Option<&str>,
    ) -> Result<()> {
        let Some(db) = self.db.as_ref() else {
            return Ok(());
        };
        let event = CommandSafetyEvent {
            session_id: session_id.map(str::to_string),
            command: command.to_string(),
            dcg_version: self.dcg_version.clone(),
            dcg_pack: decision.pack.clone(),
            decision: decision.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        db.insert_command_safety_event(&event)
    }
}

fn approval_matches(command: &str) -> bool {
    let candidates = [
        std::env::var("MS_APPROVE_COMMAND").ok(),
        std::env::var("MS_APPROVE").ok(),
    ];

    let trimmed_command = command.trim();
    candidates
        .iter()
        .flatten()
        .any(|value| value.trim() == trimmed_command)
}

fn approval_hint(command: &str) -> String {
    format!("approval required: set MS_APPROVE_COMMAND to exact command: {command}")
}

fn find_ms_root() -> Result<PathBuf> {
    if let Ok(root) = std::env::var("MS_ROOT") {
        return Ok(PathBuf::from(root));
    }
    let cwd = std::env::current_dir()?;
    if let Some(found) = find_upwards(&cwd, ".ms")? {
        return Ok(found);
    }

    let data_dir = dirs::data_dir()
        .ok_or_else(|| MsError::MissingConfig("data directory not found".to_string()))?;
    Ok(data_dir.join("ms"))
}

fn find_upwards(start: &Path, name: &str) -> Result<Option<PathBuf>> {
    let mut current = Some(start);
    while let Some(dir) = current {
        let candidate = dir.join(name);
        if candidate.is_dir() {
            return Ok(Some(candidate));
        }
        current = dir.parent();
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // =========================================================================
    // approval_hint tests
    // =========================================================================

    #[test]
    fn approval_hint_format() {
        let hint = approval_hint("rm -rf /");
        assert!(hint.contains("MS_APPROVE_COMMAND"));
        assert!(hint.contains("rm -rf /"));
    }

    #[test]
    fn approval_hint_with_special_chars() {
        let hint = approval_hint("echo 'hello world' | grep 'hello'");
        assert!(hint.contains("echo 'hello world' | grep 'hello'"));
    }

    // =========================================================================
    // approval_matches tests
    // =========================================================================
    // Note: approval_matches tests require env var manipulation which is unsafe
    // in newer Rust. The approval logic is tested via E2E tests in the safety
    // workflow tests instead.

    // =========================================================================
    // find_upwards tests
    // =========================================================================

    #[test]
    fn find_upwards_finds_existing_dir() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("target_dir");
        std::fs::create_dir(&target).unwrap();

        let result = find_upwards(temp.path(), "target_dir").unwrap();
        assert_eq!(result, Some(target));
    }

    #[test]
    fn find_upwards_finds_in_parent() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join(".ms");
        std::fs::create_dir(&target).unwrap();

        let nested = temp.path().join("a").join("b").join("c");
        std::fs::create_dir_all(&nested).unwrap();

        let result = find_upwards(&nested, ".ms").unwrap();
        assert_eq!(result, Some(target));
    }

    #[test]
    fn find_upwards_not_found() {
        let temp = TempDir::new().unwrap();
        let result = find_upwards(temp.path(), "nonexistent").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn find_upwards_stops_at_root() {
        // Start from temp, look for something that definitely doesn't exist
        let temp = TempDir::new().unwrap();
        let result =
            find_upwards(temp.path(), "definitely_not_a_real_directory_name_xyz123").unwrap();
        assert_eq!(result, None);
    }

    // =========================================================================
    // SafetyStatus tests
    // =========================================================================

    #[test]
    fn safety_status_default_fields() {
        let status = SafetyStatus {
            dcg_version: Some("1.0.0".to_string()),
            dcg_bin: PathBuf::from("/usr/bin/dcg"),
            packs: vec!["default".to_string()],
        };

        assert_eq!(status.dcg_version, Some("1.0.0".to_string()));
        assert_eq!(status.dcg_bin, PathBuf::from("/usr/bin/dcg"));
        assert_eq!(status.packs.len(), 1);
    }

    #[test]
    fn safety_status_no_dcg() {
        let status = SafetyStatus {
            dcg_version: None,
            dcg_bin: PathBuf::from("/nonexistent/dcg"),
            packs: vec![],
        };

        assert!(status.dcg_version.is_none());
        assert!(status.packs.is_empty());
    }

    // =========================================================================
    // CommandSafetyEvent tests
    // =========================================================================

    #[test]
    fn command_safety_event_serialization() {
        use crate::core::safety::{DcgDecision, SafetyTier};

        let event = CommandSafetyEvent {
            session_id: Some("test-session".to_string()),
            command: "rm -rf /".to_string(),
            dcg_version: Some("1.0.0".to_string()),
            dcg_pack: Some("default".to_string()),
            decision: DcgDecision {
                allowed: false,
                tier: SafetyTier::Critical,
                reason: "destructive command".to_string(),
                remediation: Some("Use a safer alternative".to_string()),
                rule_id: Some("R001".to_string()),
                pack: Some("default".to_string()),
                approved: false,
            },
            created_at: "2024-01-01T00:00:00Z".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("rm -rf /"));
        assert!(json.contains("destructive command"));
        assert!(json.contains("test-session"));
    }
}
