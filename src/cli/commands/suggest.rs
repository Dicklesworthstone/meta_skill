use clap::Args;

use crate::app::AppContext;
use crate::cli::output::{emit_json, HumanLayout};
use crate::context::{ContextCapture, ContextFingerprint};
use crate::error::{MsError, Result};
use crate::suggestions::SuggestionCooldownCache;
use crate::suggestions::bandit::{SignalBandit, SuggestionContext};

#[derive(Args, Debug)]
pub struct SuggestArgs {
    /// Working directory context
    #[arg(long)]
    pub cwd: Option<String>,

    /// Budget for packed output
    #[arg(long)]
    pub budget: Option<usize>,

    /// Ignore suggestion cooldowns
    #[arg(long)]
    pub ignore_cooldowns: bool,

    /// Clear cooldown cache before suggesting
    #[arg(long)]
    pub reset_cooldowns: bool,

    /// Disable bandit-based weighting
    #[arg(long)]
    pub no_bandit: bool,

    /// Override bandit exploration factor
    #[arg(long)]
    pub bandit_exploration: Option<f64>,

    /// Reset bandit state before suggesting
    #[arg(long)]
    pub reset_bandit: bool,
}

pub fn run(ctx: &AppContext, args: &SuggestArgs) -> Result<()> {
    let cwd = args.cwd.as_ref().map(|v| v.into());
    let capture = ContextCapture::capture_current(cwd)?;
    let fingerprint = ContextFingerprint::capture(&capture);

    let cache_path = cooldown_path();
    let mut cache = SuggestionCooldownCache::load(&cache_path)?;
    if args.reset_cooldowns {
        cache = SuggestionCooldownCache::new();
        cache.save(&cache_path)?;
    }

    let stats = cache.stats();
    let cooldown_ignored = args.ignore_cooldowns;

    let bandit_path = bandit_path();
    let mut bandit_weights = None;
    let mut bandit_exploration = args.bandit_exploration;
    let bandit_enabled = !args.no_bandit;

    if let Some(value) = bandit_exploration {
        if value < 0.0 {
            return Err(MsError::ValidationFailed(
                "bandit_exploration must be >= 0".to_string(),
            ));
        }
    }

    if args.reset_bandit {
        let mut bandit = SignalBandit::new();
        if let Some(value) = bandit_exploration {
            bandit.config.exploration_factor = value;
        }
        bandit.save(&bandit_path)?;
        if bandit_enabled {
            let weights = bandit.select_weights(&SuggestionContext::default());
            bandit_weights = Some(weights);
        }
    } else if bandit_enabled {
        let mut bandit = SignalBandit::load(&bandit_path)?;
        if let Some(value) = bandit_exploration {
            bandit.config.exploration_factor = value;
        } else {
            bandit_exploration = Some(bandit.config.exploration_factor);
        }
        let weights = bandit.select_weights(&SuggestionContext::default());
        bandit_weights = Some(weights);
    }

    if ctx.robot_mode {
        let bandit_payload = if bandit_enabled {
            let weights_json = bandit_weights
                .as_ref()
                .and_then(|weights| serde_json::to_value(&weights.weights).ok())
                .unwrap_or_else(|| serde_json::json!({}));
            serde_json::json!({
                "enabled": true,
                "path": bandit_path.display().to_string(),
                "exploration_factor": bandit_exploration,
                "weights": weights_json,
            })
        } else {
            serde_json::json!({
                "enabled": false,
                "path": bandit_path.display().to_string(),
            })
        };
        let payload = serde_json::json!({
            "status": "ok",
            "fingerprint": fingerprint.as_u64(),
            "cooldown": {
                "path": cache_path.display().to_string(),
                "ignored": cooldown_ignored,
                "stats": stats,
            },
            "bandit": bandit_payload,
            "suggestions": [],
        });
        emit_json(&payload)
    } else {
        let mut layout = HumanLayout::new();
        layout
            .title("Suggestions")
            .section("Context Fingerprint")
            .kv("Fingerprint", &format!("{}", fingerprint.as_u64()))
            .blank()
            .section("Cooldown Cache")
            .kv("Path", &cache_path.display().to_string())
            .kv("Ignored", &cooldown_ignored.to_string())
            .kv("Total", &stats.total_entries.to_string())
            .kv("Active", &stats.active_cooldowns.to_string())
            .kv("Expired", &stats.expired_pending_cleanup.to_string())
            .blank()
            .section("Bandit Weights")
            .kv("Enabled", &bandit_enabled.to_string())
            .kv("Path", &bandit_path.display().to_string());
        if let Some(value) = bandit_exploration {
            layout.kv("Exploration", &format!("{value:.3}"));
        }
        if let Some(weights) = bandit_weights {
            let mut rows: Vec<(String, String)> = weights
                .weights
                .iter()
                .map(|(signal, weight)| (format!("{signal:?}"), format!("{weight:.3}")))
                .collect();
            rows.sort_by(|a, b| a.0.cmp(&b.0));
            for (signal, weight) in rows {
                layout.kv(&signal, &weight);
            }
        } else {
            layout.kv("Weights", "disabled");
        }
        layout
            .bullet("Suggestion engine integration pending; cooldown cache ready.");
        crate::cli::output::emit_human(layout);
        Ok(())
    }
}

fn cooldown_path() -> std::path::PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    base.join("ms").join("cooldowns.json")
}

fn bandit_path() -> std::path::PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    base.join("ms").join("bandit.json")
}
