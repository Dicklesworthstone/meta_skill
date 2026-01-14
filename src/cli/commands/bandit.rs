use std::path::PathBuf;

use clap::{Args, Subcommand};

use crate::app::AppContext;
use crate::cli::output::{emit_json, HumanLayout};
use crate::error::Result;
use crate::suggestions::bandit::{SignalBandit, SuggestionContext};

#[derive(Args, Debug)]
pub struct BanditArgs {
    #[command(subcommand)]
    pub command: BanditCommand,
}

#[derive(Subcommand, Debug)]
pub enum BanditCommand {
    /// Show bandit stats and weights
    Stats(StatsArgs),

    /// Reset bandit state
    Reset(ResetArgs),
}

#[derive(Args, Debug, Default)]
pub struct StatsArgs {
    /// Optional bandit state path
    #[arg(long)]
    pub path: Option<PathBuf>,
}

#[derive(Args, Debug, Default)]
pub struct ResetArgs {
    /// Optional bandit state path
    #[arg(long)]
    pub path: Option<PathBuf>,
}

pub fn run(ctx: &AppContext, args: &BanditArgs) -> Result<()> {
    match &args.command {
        BanditCommand::Stats(args) => stats(ctx, args),
        BanditCommand::Reset(args) => reset(ctx, args),
    }
}

fn stats(ctx: &AppContext, args: &StatsArgs) -> Result<()> {
    let path = args
        .path
        .clone()
        .unwrap_or_else(default_bandit_path);
    let bandit = SignalBandit::load(&path)?;
    let weights = bandit.estimated_weights(&SuggestionContext::default());

    if ctx.robot_mode {
        let arms: Vec<_> = bandit
            .arms
            .values()
            .map(|arm| {
                serde_json::json!({
                    "signal": format!("{:?}", arm.signal_type),
                    "successes": arm.successes,
                    "failures": arm.failures,
                    "estimated_prob": arm.estimated_prob,
                    "ucb": arm.ucb,
                    "last_selected": arm.last_selected,
                })
            })
            .collect();

        let payload = serde_json::json!({
            "status": "ok",
            "path": path.display().to_string(),
            "total_selections": bandit.total_selections,
            "config": {
                "exploration_factor": bandit.config.exploration_factor,
                "observation_decay": bandit.config.observation_decay,
                "min_observations": bandit.config.min_observations,
                "use_context": bandit.config.use_context,
                "persist_frequency": bandit.config.persist_frequency,
                "persistence_path": bandit.config.persistence_path,
            },
            "weights": weights.weights,
            "arms": arms,
        });
        emit_json(&payload)
    } else {
        let mut layout = HumanLayout::new();
        layout
            .title("Bandit Stats")
            .section("State")
            .kv("Path", &path.display().to_string())
            .kv("Total selections", &bandit.total_selections.to_string())
            .blank()
            .section("Config")
            .kv("Exploration", &format!("{:.3}", bandit.config.exploration_factor))
            .kv("Observation decay", &format!("{:.3}", bandit.config.observation_decay))
            .kv("Min observations", &bandit.config.min_observations.to_string())
            .kv("Use context", &bandit.config.use_context.to_string())
            .kv("Persist frequency", &bandit.config.persist_frequency.to_string())
            .kv(
                "Persistence path",
                &bandit
                    .config
                    .persistence_path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "(default)".to_string()),
            )
            .blank()
            .section("Weights");

        let mut weights: Vec<(String, String)> = weights
            .weights
            .iter()
            .map(|(signal, weight)| (format!("{signal:?}"), format!("{weight:.3}")))
            .collect();
        weights.sort_by(|a, b| a.0.cmp(&b.0));
        for (signal, weight) in weights {
            layout.kv(&signal, &weight);
        }

        crate::cli::output::emit_human(layout);
        Ok(())
    }
}

fn reset(ctx: &AppContext, args: &ResetArgs) -> Result<()> {
    let path = args
        .path
        .clone()
        .unwrap_or_else(default_bandit_path);
    let bandit = SignalBandit::new();
    bandit.save(&path)?;

    if ctx.robot_mode {
        let payload = serde_json::json!({
            "status": "ok",
            "reset": true,
            "path": path.display().to_string(),
        });
        emit_json(&payload)
    } else {
        let mut layout = HumanLayout::new();
        layout
            .title("Bandit Reset")
            .kv("Path", &path.display().to_string())
            .kv("Reset", "true");
        crate::cli::output::emit_human(layout);
        Ok(())
    }
}

fn default_bandit_path() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("ms").join("bandit.json")
}
