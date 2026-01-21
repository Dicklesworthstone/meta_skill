//! ms recommend - View and tune skill recommendation engine
//!
//! Provides commands to inspect recommendation statistics, view history,
//! and tune the contextual bandit parameters.

use std::path::PathBuf;

use clap::{Args, Subcommand};
use colored::Colorize;

use crate::app::AppContext;
use crate::cli::output::OutputFormat;
use crate::cli::output::{HumanLayout, emit_json};
use crate::error::Result;
use crate::suggestions::bandit::contextual::ContextualBandit;
use crate::suggestions::bandit::features::{FEATURE_DIM, UserHistory};

#[derive(Args, Debug)]
pub struct RecommendArgs {
    #[command(subcommand)]
    pub command: RecommendCommand,
}

#[derive(Subcommand, Debug)]
pub enum RecommendCommand {
    /// Show recommendation engine statistics
    Stats(StatsArgs),

    /// View recommendation history
    History(HistoryArgs),

    /// Tune recommendation engine parameters
    Tune(TuneArgs),
}

#[derive(Args, Debug, Default)]
pub struct StatsArgs {
    /// Show detailed per-skill statistics
    #[arg(long, short)]
    pub detailed: bool,

    /// Limit number of skills to show in detailed view
    #[arg(long, short = 'n', default_value = "10")]
    pub limit: usize,
}

#[derive(Args, Debug, Default)]
pub struct HistoryArgs {
    /// Filter by skill ID
    #[arg(long)]
    pub skill: Option<String>,

    /// Limit number of entries
    #[arg(long, short = 'n', default_value = "20")]
    pub limit: usize,
}

#[derive(Args, Debug, Default)]
pub struct TuneArgs {
    /// Set exploration rate (0.0-1.0, higher = more exploration)
    #[arg(long)]
    pub exploration: Option<f64>,

    /// Set learning rate (0.0-1.0, higher = faster adaptation)
    #[arg(long)]
    pub learning_rate: Option<f64>,

    /// Reset bandit to fresh state
    #[arg(long)]
    pub reset: bool,

    /// Show current parameters without changing them
    #[arg(long)]
    pub show: bool,
}

pub fn run(ctx: &AppContext, args: &RecommendArgs) -> Result<()> {
    match &args.command {
        RecommendCommand::Stats(args) => stats(ctx, args),
        RecommendCommand::History(args) => history(ctx, args),
        RecommendCommand::Tune(args) => tune(ctx, args),
    }
}

fn stats(ctx: &AppContext, args: &StatsArgs) -> Result<()> {
    let bandit_path = default_bandit_path();
    let history_path = UserHistory::default_path();

    let bandit = ContextualBandit::load(&bandit_path)
        .unwrap_or_else(|_| ContextualBandit::with_feature_dim(FEATURE_DIM));
    let user_history = UserHistory::load(&history_path);

    if ctx.output_format != OutputFormat::Human {
        let mut skills_data: Vec<serde_json::Value> = Vec::new();

        if args.detailed {
            let skill_stats = bandit.get_all_skill_stats();
            for (skill_id, stats) in skill_stats.iter().take(args.limit) {
                skills_data.push(serde_json::json!({
                    "skill_id": skill_id,
                    "pulls": stats.pulls,
                    "avg_reward": stats.avg_reward,
                    "ucb_score": stats.ucb_score,
                }));
            }
        }

        let payload = serde_json::json!({
            "status": "ok",
            "bandit": {
                "path": bandit_path.display().to_string(),
                "total_recommendations": bandit.total_recommendations(),
                "total_updates": bandit.total_updates(),
                "registered_skills": bandit.skill_count(),
                "feature_dim": FEATURE_DIM,
                "config": bandit.config_summary(),
            },
            "user_history": {
                "path": history_path.display().to_string(),
                "total_loads": user_history.total_skill_loads,
                "unique_skills": user_history.skill_load_counts.len(),
                "days_since_last_use": user_history.days_since_last_use,
            },
            "skills": skills_data,
        });
        return emit_json(&payload);
    }

    let mut layout = HumanLayout::new();
    layout
        .title("Recommendation Engine Stats")
        .section("Contextual Bandit")
        .kv("Path", &bandit_path.display().to_string())
        .kv(
            "Total recommendations",
            &bandit.total_recommendations().to_string(),
        )
        .kv("Total updates", &bandit.total_updates().to_string())
        .kv("Registered skills", &bandit.skill_count().to_string())
        .kv("Feature dimensions", &FEATURE_DIM.to_string())
        .blank()
        .section("User History")
        .kv("Path", &history_path.display().to_string())
        .kv(
            "Total skill loads",
            &user_history.total_skill_loads.to_string(),
        )
        .kv(
            "Unique skills used",
            &user_history.skill_load_counts.len().to_string(),
        )
        .kv(
            "Days since last use",
            &user_history
                .days_since_last_use
                .map_or_else(|| "never".to_string(), |d| d.to_string()),
        );

    if args.detailed && bandit.skill_count() > 0 {
        layout.blank().section("Top Skills by Performance");

        let skill_stats = bandit.get_all_skill_stats();
        for (skill_id, stats) in skill_stats.iter().take(args.limit) {
            let info = format!(
                "{} pulls, {:.2} avg reward, {:.3} UCB",
                stats.pulls, stats.avg_reward, stats.ucb_score
            );
            layout.kv(skill_id, &info);
        }
    }

    crate::cli::output::emit_human(layout);
    Ok(())
}

fn history(ctx: &AppContext, args: &HistoryArgs) -> Result<()> {
    let history_path = UserHistory::default_path();
    let user_history = UserHistory::load(&history_path);

    if ctx.output_format != OutputFormat::Human {
        let mut records: Vec<serde_json::Value> = Vec::new();

        let mut entries: Vec<_> = user_history.skill_load_counts.iter().collect();
        // Sort by count descending
        entries.sort_by(|a, b| b.1.cmp(a.1));

        for (skill_id, count) in entries.iter().take(args.limit) {
            if let Some(ref filter) = args.skill {
                if !skill_id.contains(filter) {
                    continue;
                }
            }

            let last_load = user_history
                .skill_last_load
                .get(*skill_id)
                .map(|dt| dt.to_rfc3339());

            records.push(serde_json::json!({
                "skill_id": skill_id,
                "load_count": count,
                "last_load": last_load,
                "frequency": user_history.skill_frequency(skill_id),
                "recency": user_history.skill_recency(skill_id),
            }));
        }

        let payload = serde_json::json!({
            "status": "ok",
            "total_loads": user_history.total_skill_loads,
            "records": records,
        });
        return emit_json(&payload);
    }

    if user_history.total_skill_loads == 0 {
        println!("{}", "No recommendation history yet.".dimmed());
        println!();
        println!(
            "Use {} to get skill suggestions and build history.",
            "ms suggest".cyan()
        );
        return Ok(());
    }

    let mut layout = HumanLayout::new();
    layout.title("Recommendation History");
    layout.kv("Total loads", &user_history.total_skill_loads.to_string());
    layout.blank();

    let mut entries: Vec<_> = user_history.skill_load_counts.iter().collect();
    entries.sort_by(|a, b| b.1.cmp(a.1));

    println!(
        "{:40} {:>8} {:>10} {:>8}",
        "SKILL".bold(),
        "LOADS".bold(),
        "FREQUENCY".bold(),
        "RECENCY".bold()
    );
    println!("{}", "─".repeat(70).dimmed());

    let mut shown = 0;
    for (skill_id, count) in entries {
        if let Some(ref filter) = args.skill {
            if !skill_id.contains(filter) {
                continue;
            }
        }

        if shown >= args.limit {
            break;
        }

        let frequency = user_history.skill_frequency(skill_id);
        let recency = user_history.skill_recency(skill_id);

        println!(
            "{:40} {:>8} {:>9.1}% {:>7.2}",
            skill_id.cyan(),
            count,
            frequency * 100.0,
            recency
        );
        shown += 1;
    }

    Ok(())
}

fn tune(ctx: &AppContext, args: &TuneArgs) -> Result<()> {
    let bandit_path = default_bandit_path();

    if args.reset {
        let bandit = ContextualBandit::with_feature_dim(FEATURE_DIM);
        bandit.save(&bandit_path)?;

        if ctx.output_format != OutputFormat::Human {
            let payload = serde_json::json!({
                "status": "ok",
                "action": "reset",
                "message": "Contextual bandit reset to fresh state",
            });
            return emit_json(&payload);
        }

        println!(
            "{} Contextual bandit reset to fresh state",
            "✓".green().bold()
        );
        println!();
        println!(
            "{}",
            "The recommendation engine will now re-learn from your usage patterns.".dimmed()
        );
        return Ok(());
    }

    let mut bandit = ContextualBandit::load(&bandit_path)
        .unwrap_or_else(|_| ContextualBandit::with_feature_dim(FEATURE_DIM));

    let mut changed = false;

    if let Some(exploration) = args.exploration {
        let clamped = exploration.clamp(0.0, 1.0);
        bandit.set_exploration_rate(clamped);
        changed = true;
    }

    if let Some(learning_rate) = args.learning_rate {
        let clamped = learning_rate.clamp(0.0, 1.0);
        bandit.set_learning_rate(clamped);
        changed = true;
    }

    if changed {
        bandit.save(&bandit_path)?;
    }

    // Show current settings
    let config = bandit.config_summary();

    if ctx.output_format != OutputFormat::Human {
        let payload = serde_json::json!({
            "status": "ok",
            "changed": changed,
            "config": config,
        });
        return emit_json(&payload);
    }

    let mut layout = HumanLayout::new();
    layout.title("Recommendation Engine Parameters");

    if changed {
        layout.section("Updated");
        if args.exploration.is_some() {
            layout.kv(
                "Exploration rate",
                &format!(
                    "{:.3}",
                    config
                        .get("exploration_rate")
                        .unwrap_or(&serde_json::json!(0.1))
                ),
            );
        }
        if args.learning_rate.is_some() {
            layout.kv(
                "Learning rate",
                &format!(
                    "{:.3}",
                    config
                        .get("learning_rate")
                        .unwrap_or(&serde_json::json!(0.01))
                ),
            );
        }
        layout.blank();
    }

    layout.section("Current Settings");
    for (key, value) in config.as_object().unwrap_or(&serde_json::Map::new()) {
        layout.kv(key, &format!("{}", value));
    }

    crate::cli::output::emit_human(layout);
    Ok(())
}

fn default_bandit_path() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("ms").join("contextual_bandit.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Parser, Subcommand};

    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: TestCommand,
    }

    #[derive(Subcommand)]
    enum TestCommand {
        Recommend(RecommendArgs),
    }

    #[test]
    fn parse_recommend_stats() {
        let parsed = TestCli::parse_from(["test", "recommend", "stats"]);
        let TestCommand::Recommend(args) = parsed.cmd;
        match args.command {
            RecommendCommand::Stats(stats) => {
                assert!(!stats.detailed);
                assert_eq!(stats.limit, 10);
            }
            _ => panic!("expected stats command"),
        }
    }

    #[test]
    fn parse_recommend_stats_detailed() {
        let parsed = TestCli::parse_from(["test", "recommend", "stats", "--detailed", "-n", "20"]);
        let TestCommand::Recommend(args) = parsed.cmd;
        match args.command {
            RecommendCommand::Stats(stats) => {
                assert!(stats.detailed);
                assert_eq!(stats.limit, 20);
            }
            _ => panic!("expected stats command"),
        }
    }

    #[test]
    fn parse_recommend_history() {
        let parsed = TestCli::parse_from(["test", "recommend", "history"]);
        let TestCommand::Recommend(args) = parsed.cmd;
        match args.command {
            RecommendCommand::History(history) => {
                assert!(history.skill.is_none());
                assert_eq!(history.limit, 20);
            }
            _ => panic!("expected history command"),
        }
    }

    #[test]
    fn parse_recommend_history_with_filter() {
        let parsed =
            TestCli::parse_from(["test", "recommend", "history", "--skill", "rust", "-n", "5"]);
        let TestCommand::Recommend(args) = parsed.cmd;
        match args.command {
            RecommendCommand::History(history) => {
                assert_eq!(history.skill.as_deref(), Some("rust"));
                assert_eq!(history.limit, 5);
            }
            _ => panic!("expected history command"),
        }
    }

    #[test]
    fn parse_recommend_tune() {
        let parsed = TestCli::parse_from(["test", "recommend", "tune", "--show"]);
        let TestCommand::Recommend(args) = parsed.cmd;
        match args.command {
            RecommendCommand::Tune(tune) => {
                assert!(tune.show);
                assert!(!tune.reset);
                assert!(tune.exploration.is_none());
            }
            _ => panic!("expected tune command"),
        }
    }

    #[test]
    fn parse_recommend_tune_with_params() {
        let parsed = TestCli::parse_from([
            "test",
            "recommend",
            "tune",
            "--exploration",
            "0.2",
            "--learning-rate",
            "0.05",
        ]);
        let TestCommand::Recommend(args) = parsed.cmd;
        match args.command {
            RecommendCommand::Tune(tune) => {
                assert_eq!(tune.exploration, Some(0.2));
                assert_eq!(tune.learning_rate, Some(0.05));
            }
            _ => panic!("expected tune command"),
        }
    }

    #[test]
    fn parse_recommend_tune_reset() {
        let parsed = TestCli::parse_from(["test", "recommend", "tune", "--reset"]);
        let TestCommand::Recommend(args) = parsed.cmd;
        match args.command {
            RecommendCommand::Tune(tune) => {
                assert!(tune.reset);
            }
            _ => panic!("expected tune command"),
        }
    }
}
