use clap::Args;

use crate::app::AppContext;
use crate::cli::output::{emit_json, HumanLayout};
use crate::context::{ContextCapture, ContextFingerprint};
use crate::error::Result;
use crate::suggestions::SuggestionCooldownCache;

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

    if ctx.robot_mode {
        let payload = serde_json::json!({
            "status": "ok",
            "fingerprint": fingerprint.as_u64(),
            "cooldown": {
                "path": cache_path.display().to_string(),
                "stats": stats,
            },
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
            .kv("Total", &stats.total_entries.to_string())
            .kv("Active", &stats.active_cooldowns.to_string())
            .kv("Expired", &stats.expired_pending_cleanup.to_string())
            .blank()
            .bullet("Suggestion engine integration pending; cooldown cache ready.");
        crate::cli::output::emit_human(layout);
        Ok(())
    }
}

fn cooldown_path() -> std::path::PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    base.join("ms").join("cooldowns.json")
}
