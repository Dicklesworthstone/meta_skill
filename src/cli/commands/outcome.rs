//! ms outcome - Record implicit success/failure outcomes.

use clap::Args;

use crate::app::AppContext;
use crate::cli::output::OutputFormat;
use crate::cli::output::{HumanLayout, emit_json};
use crate::error::{MsError, Result};

#[derive(Args, Debug)]
pub struct OutcomeArgs {
    /// Skill ID or name
    pub skill: String,

    /// Mark latest usage as success
    #[arg(long)]
    pub success: bool,

    /// Mark latest usage as failure
    #[arg(long)]
    pub failure: bool,
}

pub fn run(ctx: &AppContext, args: &OutcomeArgs) -> Result<()> {
    if args.success == args.failure {
        return Err(MsError::ValidationFailed(
            "provide exactly one of --success or --failure".to_string(),
        ));
    }

    let skill_id = resolve_skill_id(ctx, &args.skill)?;
    let success = args.success;
    ctx.db.record_skill_outcome(&skill_id, success)?;

    let (contextual_reward, aggregated_reward) =
        match update_bandits_from_outcome(ctx, &skill_id, success) {
            Ok(rewards) => rewards,
            Err(e) => {
                eprintln!("Warning: Failed to update bandit from outcome: {e}");
                (
                    if success { 1.0 } else { 0.0 },
                    if success { 1.0 } else { 0.0 },
                )
            }
        };

    if ctx.output_format != OutputFormat::Human {
        let payload = serde_json::json!({
            "status": "ok",
            "skill_id": skill_id,
            "success": success,
            "contextual_reward": contextual_reward,
            "aggregated_reward": aggregated_reward,
        });
        return emit_json(&payload);
    }

    let mut layout = HumanLayout::new();
    layout
        .title("Outcome Recorded")
        .kv("Skill", &skill_id)
        .kv("Outcome", if success { "success" } else { "failure" })
        .kv("Reward", &format!("{aggregated_reward:.2}"));
    crate::cli::output::emit_human(layout);
    Ok(())
}

fn update_bandits_from_outcome(
    ctx: &AppContext,
    skill_id: &str,
    success: bool,
) -> Result<(f32, f32)> {
    use crate::suggestions::bandit::{
        ContextFeatures, ContextualBandit, Reward, SignalBandit, SignalType, SkillFeedback,
        SuggestionContext as BanditSuggestionContext, aggregate_rewards, compute_contextual_reward,
    };
    use crate::suggestions::tracking::FeedbackCollector;

    let contextual_path = ctx.ms_root.join("contextual_bandit.json");
    let mut contextual = ContextualBandit::load(&contextual_path)?;
    contextual.register_skill(skill_id);
    let features = ContextFeatures::default();

    // Flush any pending session/unload feedback for loaded skills and ignored suggestions
    let tracking_path = FeedbackCollector::default_path(&ctx.ms_root);
    let mut collector = FeedbackCollector::load(&tracking_path);

    let was_loaded = collector.session.is_skill_loaded(skill_id);
    let load_mins = collector
        .session
        .skill_load_duration_minutes(skill_id)
        .unwrap_or(0);
    let was_top_suggestion = collector
        .suggestions
        .all_suggestions()
        .iter()
        .any(|s| s.skill_id == skill_id && s.position == 0);

    let _ = collector.on_skill_unload_and_update_bandit(skill_id, &mut contextual, &features);
    let _ = collector.end_session_and_update_bandit(&mut contextual, &features);
    let _ = collector.save(&tracking_path);

    let feedback = if success {
        SkillFeedback::ExplicitHelpful
    } else {
        SkillFeedback::ExplicitNotHelpful { reason: None }
    };
    let mut feedback_signals = vec![feedback.clone()];
    if was_loaded && load_mins > 0 {
        feedback_signals.push(SkillFeedback::used(load_mins));
    }
    let aggregated_reward = aggregate_rewards(&feedback_signals);
    let contextual_reward = compute_contextual_reward(&feedback, 0.5, was_top_suggestion);

    contextual.update(skill_id, &features, &feedback);
    contextual.save(&contextual_path)?;

    let signal_path = ctx.ms_root.join("bandit.json");
    let mut signal_bandit = SignalBandit::load(&signal_path)?;
    let reward = if aggregated_reward >= 0.5 {
        Reward::Success
    } else {
        Reward::Failure
    };
    let bandit_ctx = BanditSuggestionContext::default();
    for signal in SignalType::all() {
        signal_bandit.update(*signal, reward, &bandit_ctx);
    }
    signal_bandit.save(&signal_path)?;
    Ok((contextual_reward, aggregated_reward))
}

fn resolve_skill_id(ctx: &AppContext, input: &str) -> Result<String> {
    if let Some(skill) = ctx.db.get_skill(input)? {
        return Ok(skill.id);
    }
    if let Ok(Some(alias)) = ctx.db.resolve_alias(input) {
        if let Some(skill) = ctx.db.get_skill(&alias.canonical_id)? {
            return Ok(skill.id);
        }
    }
    Err(MsError::SkillNotFound(format!("skill not found: {input}")))
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
        Outcome(OutcomeArgs),
    }

    #[test]
    fn parse_outcome_success() {
        let parsed = TestCli::parse_from(["test", "outcome", "skill-1", "--success"]);
        let TestCommand::Outcome(args) = parsed.cmd;
        assert_eq!(args.skill, "skill-1");
        assert!(args.success);
        assert!(!args.failure);
    }

    #[test]
    fn parse_outcome_failure() {
        let parsed = TestCli::parse_from(["test", "outcome", "skill-1", "--failure"]);
        let TestCommand::Outcome(args) = parsed.cmd;
        assert_eq!(args.skill, "skill-1");
        assert!(!args.success);
        assert!(args.failure);
    }
}
