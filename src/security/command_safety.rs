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

#[derive(Debug, Clone)]
pub struct SafetyGate {
    guard: DcgGuard,
    dcg_version: Option<String>,
    require_verbatim_approval: bool,
    db: Option<Arc<Database>>,
}

impl SafetyGate {
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
        let db = Database::open(ms_root.join("ms.db")).ok().map(Arc::new);
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

    pub fn enforce(&self, command: &str, session_id: Option<&str>) -> Result<()> {
        let mut decision = match self.guard.evaluate_command(command) {
            Ok(decision) => decision,
            Err(err) => {
                warn!("dcg unavailable: {err}");
                DcgDecision::unavailable(format!("dcg unavailable: {err}"))
            }
        };

        let mut final_allowed = decision.allowed;
        if !decision.allowed {
            if self.require_verbatim_approval && decision.tier >= SafetyTier::Danger {
                if approval_matches(command) {
                    decision.approved = true;
                    decision.allowed = true;
                    final_allowed = true;
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
        if !final_allowed {
            return Err(MsError::DestructiveBlocked(format!(
                "blocked by dcg: {}",
                decision.reason
            )));
        }

        if decision.approved {
            info!("command approved by verbatim match: {command}");
        }

        Ok(())
    }

    fn log_event(&self, command: &str, decision: &DcgDecision, session_id: Option<&str>) -> Result<()> {
        let Some(db) = self.db.as_ref() else {
            return Ok(());
        };
        let event = CommandSafetyEvent {
            session_id: session_id.map(|s| s.to_string()),
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

    candidates
        .iter()
        .flatten()
        .any(|value| value == command)
}

fn approval_hint(command: &str) -> String {
    format!(
        "approval required: set MS_APPROVE_COMMAND to exact command: {command}"
    )
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
