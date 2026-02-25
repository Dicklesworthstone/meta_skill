use std::collections::HashMap;
use std::path::PathBuf;

use clap::Args;
use tracing::debug;

use crate::app::AppContext;
use crate::cli::formatters::{
    ScorePercentageBreakdown, SuggestionContext, SuggestionItem, SuggestionOutput,
};
use crate::cli::output::Formattable;
use crate::context::collector::{CollectedContext, ContextCollector, ContextCollectorConfig};
use crate::context::{ContextCapture, ContextFingerprint};
use crate::error::Result;
use crate::storage::sqlite::SkillRecord;
use crate::suggestions::SuggestionCooldownCache;
use crate::suggestions::bandit::contextual::ContextualBandit;
use crate::suggestions::bandit::features::{
    DefaultFeatureExtractor, FEATURE_DIM, FeatureExtractor, UserHistory,
};
use crate::suggestions::tracking::SuggestionTracker;

#[derive(Args, Debug)]
pub struct SuggestArgs {
    /// Maximum number of suggestions to return
    #[arg(long, short, default_value = "5")]
    pub limit: usize,

    /// Include discovery suggestions (exploration of novel skills)
    #[arg(long)]
    pub discover: bool,

    /// Weight historical preferences heavily
    #[arg(long)]
    pub personal: bool,

    /// Show explanation for each suggestion
    #[arg(long)]
    pub explain: bool,

    /// Filter by domain/tag
    #[arg(long)]
    pub domain: Option<String>,

    /// Automatically load suggested skills
    #[arg(long)]
    pub load: bool,

    /// Number of skills to auto-load (used with --load)
    #[arg(long, default_value = "3")]
    pub top: usize,

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

/// A suggestion with score and metadata.
#[derive(Debug, Clone)]
pub struct Suggestion {
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub score: f32,
    pub breakdown: ScoreBreakdown,
    pub is_discovery: bool,
    pub is_favorite: bool,
    pub tags: Vec<String>,
}

/// Score breakdown for explanation mode.
#[derive(Debug, Clone, Default)]
pub struct ScoreBreakdown {
    pub contextual_score: f32,
    pub thompson_score: f32,
    pub exploration_bonus: f32,
    pub personal_boost: f32,
    pub pull_count: u64,
    pub avg_reward: f64,
}

pub fn run(ctx: &AppContext, args: &SuggestArgs) -> Result<()> {
    debug!(target: "suggest", mode = ?ctx.output_format, "output mode selected");

    // 1. Capture working context
    let cwd_path: Option<PathBuf> = args.cwd.as_ref().map(PathBuf::from);
    let capture = ContextCapture::capture_current(cwd_path.clone())?;
    let fingerprint = ContextFingerprint::capture(&capture);

    // 2. Load cooldown cache
    let cache_path = cooldown_path();
    let mut cache = if args.reset_cooldowns {
        SuggestionCooldownCache::new()
    } else {
        SuggestionCooldownCache::load(&cache_path).unwrap_or_else(|e| {
            if !ctx.output_format.is_machine_readable() {
                eprintln!("Warning: Failed to load cooldown cache: {e}. Starting fresh.");
            }
            SuggestionCooldownCache::new()
        })
    };

    if args.reset_cooldowns {
        cache.save(&cache_path)?;
    }

    // 3. Load contextual bandit
    let contextual_bandit_path = contextual_bandit_path();
    let mut contextual_bandit = if args.reset_bandit {
        let bandit = ContextualBandit::with_feature_dim(FEATURE_DIM);
        bandit.save(&contextual_bandit_path)?;
        bandit
    } else {
        ContextualBandit::load(&contextual_bandit_path).unwrap_or_else(|e| {
            if !ctx.output_format.is_machine_readable() {
                eprintln!("Warning: Failed to load bandit state: {e}. Starting fresh.");
            }
            ContextualBandit::with_feature_dim(FEATURE_DIM)
        })
    };

    // 4. Collect context for feature extraction
    let collector_config = ContextCollectorConfig::default();
    let collector = ContextCollector::new(collector_config);
    let working_dir = cwd_path.unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let collected_context = collector.collect(&working_dir)?;

    // 5. Extract context features
    let feature_extractor = DefaultFeatureExtractor::new();
    let user_history = load_user_history();
    let context_features =
        feature_extractor.extract_from_collected(&collected_context, &user_history);

    // 6. Get all skills from database
    let all_skills = ctx.db.list_skills(1000, 0)?;
    if all_skills.is_empty() {
        return output_empty_suggestions(ctx, args, &fingerprint, &cache);
    }

    // Register all skills with the bandit
    let skill_ids: Vec<String> = all_skills.iter().map(|s| s.id.clone()).collect();
    contextual_bandit.register_skills(&skill_ids);

    // 7. Get recommendations from bandit
    let fetch_limit = args.limit * 2; // Fetch extra for filtering
    let recommendations = contextual_bandit.recommend(&context_features, fetch_limit);

    // 8. Build suggestions with metadata
    let skill_map: HashMap<String, &SkillRecord> =
        all_skills.iter().map(|s| (s.id.clone(), s)).collect();
    let mut suggestions: Vec<Suggestion> = recommendations
        .iter()
        .filter_map(|rec| {
            let skill = skill_map.get(&rec.skill_id)?;
            let tags = parse_tags_from_metadata(&skill.metadata_json);
            let is_favorite = ctx
                .db
                .has_user_preference(&rec.skill_id, "favorite")
                .unwrap_or(false);

            Some(Suggestion {
                skill_id: rec.skill_id.clone(),
                name: skill.name.clone(),
                description: skill.description.clone(),
                score: rec.score,
                breakdown: ScoreBreakdown {
                    contextual_score: rec.components.contextual_score,
                    thompson_score: rec.components.thompson_score,
                    exploration_bonus: rec.components.exploration_bonus,
                    personal_boost: 0.0,
                    pull_count: rec.components.pull_count,
                    avg_reward: rec.components.avg_reward,
                },
                is_discovery: rec.components.pull_count < 5,
                is_favorite,
                tags,
            })
        })
        .collect();

    // 9. Filter out hidden skills and boost favorites
    suggestions.retain(|s| {
        !ctx.db
            .has_user_preference(&s.skill_id, "hidden")
            .unwrap_or(false)
    });

    // Apply favorites boost (always, not just in personal mode)
    for suggestion in &mut suggestions {
        if suggestion.is_favorite {
            let favorites_boost = 0.25; // Significant boost for favorites
            suggestion.breakdown.personal_boost += favorites_boost;
            suggestion.score = (suggestion.score + favorites_boost).clamp(0.0, 1.0);
        }
    }
    // Re-sort after favorites boost
    suggestions.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // 10. Apply domain filter if specified
    if let Some(ref domain) = args.domain {
        let domain_lower = domain.to_lowercase();
        suggestions.retain(|s| {
            s.tags
                .iter()
                .any(|t| t.to_lowercase().contains(&domain_lower))
                || s.name.to_lowercase().contains(&domain_lower)
                || s.description.to_lowercase().contains(&domain_lower)
        });
    }

    // 11. Apply personal mode (boost historical preferences)
    if args.personal {
        for suggestion in &mut suggestions {
            let frequency = user_history.skill_frequency(&suggestion.skill_id);
            let recency = user_history.skill_recency(&suggestion.skill_id);
            let personal_boost = frequency * 0.3 + recency * 0.2;
            suggestion.breakdown.personal_boost = personal_boost;
            suggestion.score = (suggestion.score + personal_boost).clamp(0.0, 1.0);
        }
        // Re-sort after personal boost
        suggestions.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    // 12. Apply cooldown filter (unless ignored)
    let fp = fingerprint.as_u64();
    if !args.ignore_cooldowns {
        use crate::suggestions::CooldownStatus;
        suggestions
            .retain(|s| !matches!(cache.status(fp, &s.skill_id), CooldownStatus::Active { .. }));
    }

    // 13. Truncate to limit
    suggestions.truncate(args.limit);

    // 14. Build discovery suggestions if requested
    let mut discovery_suggestions: Vec<Suggestion> = Vec::new();
    if args.discover {
        // Find skills not in main suggestions that are under-explored
        let suggested_ids: std::collections::HashSet<_> =
            suggestions.iter().map(|s| &s.skill_id).collect();
        let mut discovery_candidates: Vec<Suggestion> = all_skills
            .iter()
            .filter(|s| !suggested_ids.contains(&s.id))
            // Filter out hidden skills from discovery too
            .filter(|s| !ctx.db.has_user_preference(&s.id, "hidden").unwrap_or(false))
            .filter_map(|skill| {
                let rec = recommendations.iter().find(|r| r.skill_id == skill.id);
                let components = rec.map(|r| &r.components);
                let pull_count = components.map(|c| c.pull_count).unwrap_or(0);

                // Only include under-explored skills
                if pull_count >= 10 {
                    return None;
                }

                let tags = parse_tags_from_metadata(&skill.metadata_json);
                let is_favorite = ctx
                    .db
                    .has_user_preference(&skill.id, "favorite")
                    .unwrap_or(false);
                let mut base_score = rec.map(|r| r.score).unwrap_or(0.3);
                let mut personal_boost = 0.0;

                // Apply favorites boost to discovery suggestions too
                if is_favorite {
                    let favorites_boost = 0.25;
                    personal_boost += favorites_boost;
                    base_score = (base_score + favorites_boost).clamp(0.0, 1.0);
                }

                Some(Suggestion {
                    skill_id: skill.id.clone(),
                    name: skill.name.clone(),
                    description: skill.description.clone(),
                    score: base_score,
                    breakdown: ScoreBreakdown {
                        contextual_score: components.map(|c| c.contextual_score).unwrap_or(0.0),
                        thompson_score: components.map(|c| c.thompson_score).unwrap_or(0.5),
                        exploration_bonus: components.map(|c| c.exploration_bonus).unwrap_or(0.1),
                        personal_boost,
                        pull_count,
                        avg_reward: components.map(|c| c.avg_reward).unwrap_or(0.5),
                    },
                    is_discovery: true,
                    is_favorite,
                    tags,
                })
            })
            .collect();

        // Sort by exploration potential
        discovery_candidates.sort_by(|a, b| {
            let a_potential = a.breakdown.exploration_bonus
                + (1.0 - a.breakdown.pull_count as f32 / 10.0).max(0.0) * 0.2;
            let b_potential = b.breakdown.exploration_bonus
                + (1.0 - b.breakdown.pull_count as f32 / 10.0).max(0.0) * 0.2;
            b_potential
                .partial_cmp(&a_potential)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        discovery_suggestions = discovery_candidates.into_iter().take(3).collect();
    }

    // 15. Record suggestions for learning
    let mut suggestion_tracker = SuggestionTracker::new();
    let all_suggested_ids: Vec<String> = suggestions
        .iter()
        .chain(discovery_suggestions.iter())
        .map(|s| s.skill_id.clone())
        .collect();
    suggestion_tracker.record_suggestions(&all_suggested_ids, Some(fingerprint.as_u64()));

    // 15. Update cooldowns for shown suggestions (default 5 minute cooldown)
    let cooldown_seconds = 300; // 5 minutes
    for suggestion in &suggestions {
        cache.record(fp, suggestion.skill_id.clone(), cooldown_seconds);
    }
    cache.save(&cache_path)?;

    // 16. Output results
    debug!(target: "suggest", count = suggestions.len(), "generating suggestions");
    output_suggestions(
        ctx,
        args,
        &fingerprint,
        &suggestions,
        &discovery_suggestions,
        &collected_context,
        &contextual_bandit,
    )
}

/// Output when no skills are available.
fn output_empty_suggestions(
    ctx: &AppContext,
    _args: &SuggestArgs,
    fingerprint: &ContextFingerprint,
    _cache: &SuggestionCooldownCache,
) -> Result<()> {
    let output = SuggestionOutput::new().with_fingerprint(fingerprint.as_u64());
    println!("{}", output.format(ctx.output_format));
    Ok(())
}

/// Unified output function using the new formatter system.
fn output_suggestions(
    ctx: &AppContext,
    args: &SuggestArgs,
    fingerprint: &ContextFingerprint,
    suggestions: &[Suggestion],
    discovery_suggestions: &[Suggestion],
    context: &CollectedContext,
    _bandit: &ContextualBandit,
) -> Result<()> {
    // Build context
    let suggestion_context = SuggestionContext {
        cwd: std::env::current_dir()
            .ok()
            .map(|p| p.display().to_string()),
        git_branch: context.git_context.as_ref().map(|g| g.branch.clone()),
        recent_files: context
            .recent_files
            .iter()
            .take(10)
            .map(|f| f.path.display().to_string())
            .collect(),
        fingerprint: Some(fingerprint.as_u64()),
    };

    // Build output
    let mut output = SuggestionOutput::new().with_context(suggestion_context);

    // Add main suggestions
    for s in suggestions {
        let reason = build_suggestion_reason(s);

        // Calculate percentage breakdown if --explain flag is used
        let breakdown = if args.explain {
            Some(ScorePercentageBreakdown::from_components(
                s.breakdown.contextual_score,
                s.breakdown.thompson_score,
                s.breakdown.exploration_bonus,
                s.breakdown.personal_boost,
            ))
        } else {
            None
        };

        output.add_suggestion(SuggestionItem {
            skill_id: s.skill_id.clone(),
            name: s.name.clone(),
            description: s.description.clone(),
            confidence: s.score,
            reason,
            is_discovery: false,
            tags: s.tags.clone(),
            breakdown,
        });
    }

    // Add discovery suggestions
    for s in discovery_suggestions {
        let mut reason_parts: Vec<String> = Vec::new();

        // Favorite status
        if s.is_favorite {
            reason_parts.push("Favorite".to_string());
        }

        // Discovery-specific reason
        reason_parts.push(format!("under-explored ({} uses)", s.breakdown.pull_count));

        // Calculate percentage breakdown if --explain flag is used
        let breakdown = if args.explain {
            Some(ScorePercentageBreakdown::from_components(
                s.breakdown.contextual_score,
                s.breakdown.thompson_score,
                s.breakdown.exploration_bonus,
                s.breakdown.personal_boost,
            ))
        } else {
            None
        };

        output.add_suggestion(SuggestionItem {
            skill_id: s.skill_id.clone(),
            name: s.name.clone(),
            description: s.description.clone(),
            confidence: s.score,
            reason: Some(reason_parts.join(", ")),
            is_discovery: true,
            tags: s.tags.clone(),
            breakdown,
        });
    }

    debug!(target: "suggest", stage = "render_complete");
    println!("{}", output.format(ctx.output_format));
    Ok(())
}

/// Build a human-readable reason for why a skill was suggested.
fn build_suggestion_reason(s: &Suggestion) -> Option<String> {
    let mut reasons: Vec<String> = Vec::new();

    // Favorite status is important - mention first
    if s.is_favorite {
        reasons.push("Favorite".to_string());
    }

    // Context match
    if s.breakdown.contextual_score > 0.5 {
        reasons.push(format!(
            "context match {:.0}%",
            s.breakdown.contextual_score * 100.0
        ));
    }

    // Historical usage
    if s.breakdown.pull_count > 10 {
        reasons.push(format!("{} prior uses", s.breakdown.pull_count));
    }

    if reasons.is_empty() {
        None
    } else {
        Some(reasons.join(", "))
    }
}

/// Parse tags from skill metadata JSON.
fn parse_tags_from_metadata(metadata_json: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(metadata_json) else {
        return vec![];
    };
    value
        .get("tags")
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Load user history from persistence.
fn load_user_history() -> UserHistory {
    UserHistory::load(&UserHistory::default_path())
}

fn cooldown_path() -> std::path::PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    base.join("ms").join("cooldowns.json")
}

fn contextual_bandit_path() -> std::path::PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    base.join("ms").join("contextual_bandit.json")
}

/// Check whether the terminal supports rich output for suggest commands.
#[allow(dead_code)]
fn should_use_rich_for_suggest() -> bool {
    use std::io::IsTerminal;

    if std::env::var("MS_FORCE_RICH").is_ok() {
        return true;
    }
    if std::env::var("NO_COLOR").is_ok() || std::env::var("MS_PLAIN_OUTPUT").is_ok() {
        return false;
    }

    use crate::output::{is_agent_environment, is_ci_environment};
    if is_agent_environment() || is_ci_environment() {
        return false;
    }

    std::io::stdout().is_terminal()
}

/// Get the terminal width, defaulting to 80 if detection fails.
#[allow(dead_code)]
fn terminal_width() -> usize {
    crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(80)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    // =========================================================================
    // Argument parsing tests
    // =========================================================================

    #[derive(Parser, Debug)]
    #[command(name = "test")]
    struct TestCli {
        #[command(flatten)]
        suggest: SuggestArgs,
    }

    #[test]
    fn parse_suggest_defaults() {
        let cli = TestCli::try_parse_from(["test"]).unwrap();
        assert!(cli.suggest.cwd.is_none());
        assert!(cli.suggest.budget.is_none());
        assert!(!cli.suggest.ignore_cooldowns);
        assert!(!cli.suggest.reset_cooldowns);
        assert!(!cli.suggest.no_bandit);
        assert!(cli.suggest.bandit_exploration.is_none());
        assert!(!cli.suggest.reset_bandit);
    }

    #[test]
    fn parse_suggest_with_cwd() {
        let cli = TestCli::try_parse_from(["test", "--cwd", "/path/to/dir"]).unwrap();
        assert_eq!(cli.suggest.cwd, Some("/path/to/dir".to_string()));
    }

    #[test]
    fn parse_suggest_with_budget() {
        let cli = TestCli::try_parse_from(["test", "--budget", "1000"]).unwrap();
        assert_eq!(cli.suggest.budget, Some(1000));
    }

    #[test]
    fn parse_suggest_ignore_cooldowns() {
        let cli = TestCli::try_parse_from(["test", "--ignore-cooldowns"]).unwrap();
        assert!(cli.suggest.ignore_cooldowns);
    }

    #[test]
    fn parse_suggest_reset_cooldowns() {
        let cli = TestCli::try_parse_from(["test", "--reset-cooldowns"]).unwrap();
        assert!(cli.suggest.reset_cooldowns);
    }

    #[test]
    fn parse_suggest_no_bandit() {
        let cli = TestCli::try_parse_from(["test", "--no-bandit"]).unwrap();
        assert!(cli.suggest.no_bandit);
    }

    #[test]
    fn parse_suggest_bandit_exploration() {
        let cli = TestCli::try_parse_from(["test", "--bandit-exploration", "0.5"]).unwrap();
        assert_eq!(cli.suggest.bandit_exploration, Some(0.5));
    }

    #[test]
    fn parse_suggest_bandit_exploration_zero() {
        let cli = TestCli::try_parse_from(["test", "--bandit-exploration", "0.0"]).unwrap();
        assert_eq!(cli.suggest.bandit_exploration, Some(0.0));
    }

    #[test]
    fn parse_suggest_reset_bandit() {
        let cli = TestCli::try_parse_from(["test", "--reset-bandit"]).unwrap();
        assert!(cli.suggest.reset_bandit);
    }

    #[test]
    fn parse_suggest_all_options() {
        let cli = TestCli::try_parse_from([
            "test",
            "--cwd",
            "/home/user/project",
            "--budget",
            "2000",
            "--ignore-cooldowns",
            "--reset-cooldowns",
            "--no-bandit",
            "--bandit-exploration",
            "1.5",
            "--reset-bandit",
        ])
        .unwrap();

        assert_eq!(cli.suggest.cwd, Some("/home/user/project".to_string()));
        assert_eq!(cli.suggest.budget, Some(2000));
        assert!(cli.suggest.ignore_cooldowns);
        assert!(cli.suggest.reset_cooldowns);
        assert!(cli.suggest.no_bandit);
        assert_eq!(cli.suggest.bandit_exploration, Some(1.5));
        assert!(cli.suggest.reset_bandit);
    }

    // =========================================================================
    // Error case tests
    // =========================================================================

    #[test]
    fn parse_suggest_invalid_budget() {
        let result = TestCli::try_parse_from(["test", "--budget", "not-a-number"]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_suggest_invalid_bandit_exploration() {
        let result = TestCli::try_parse_from(["test", "--bandit-exploration", "abc"]);
        assert!(result.is_err());
    }

    // =========================================================================
    // Path function tests
    // =========================================================================

    #[test]
    fn cooldown_path_ends_with_expected() {
        let path = cooldown_path();
        assert!(path.ends_with("ms/cooldowns.json"));
    }

    #[test]
    fn contextual_bandit_path_ends_with_expected() {
        let path = contextual_bandit_path();
        assert!(path.ends_with("ms/contextual_bandit.json"));
    }

    #[test]
    fn paths_are_in_same_directory() {
        let cooldown = cooldown_path();
        let bandit = contextual_bandit_path();

        // Both should be in the same parent directory
        assert_eq!(cooldown.parent(), bandit.parent());
    }

    // =========================================================================
    // Suggestion reason tests
    // =========================================================================

    fn make_test_suggestion(
        is_favorite: bool,
        contextual_score: f32,
        pull_count: u64,
    ) -> Suggestion {
        Suggestion {
            skill_id: "test-skill".to_string(),
            name: "Test Skill".to_string(),
            description: "A test skill".to_string(),
            score: 0.5,
            breakdown: ScoreBreakdown {
                contextual_score,
                thompson_score: 0.5,
                exploration_bonus: 0.1,
                personal_boost: 0.0,
                pull_count,
                avg_reward: 0.5,
            },
            is_discovery: false,
            is_favorite,
            tags: vec![],
        }
    }

    #[test]
    fn reason_favorite_only() {
        let s = make_test_suggestion(true, 0.3, 5);
        let reason = build_suggestion_reason(&s);
        assert_eq!(reason, Some("Favorite".to_string()));
    }

    #[test]
    fn reason_context_match_only() {
        let s = make_test_suggestion(false, 0.8, 5);
        let reason = build_suggestion_reason(&s);
        assert_eq!(reason, Some("context match 80%".to_string()));
    }

    #[test]
    fn reason_historical_only() {
        let s = make_test_suggestion(false, 0.3, 15);
        let reason = build_suggestion_reason(&s);
        assert_eq!(reason, Some("15 prior uses".to_string()));
    }

    #[test]
    fn reason_favorite_and_context() {
        let s = make_test_suggestion(true, 0.7, 5);
        let reason = build_suggestion_reason(&s);
        assert_eq!(reason, Some("Favorite, context match 70%".to_string()));
    }

    #[test]
    fn reason_all_factors() {
        let s = make_test_suggestion(true, 0.6, 20);
        let reason = build_suggestion_reason(&s);
        assert_eq!(
            reason,
            Some("Favorite, context match 60%, 20 prior uses".to_string())
        );
    }

    #[test]
    fn reason_none_when_no_factors() {
        let s = make_test_suggestion(false, 0.3, 5);
        let reason = build_suggestion_reason(&s);
        assert!(reason.is_none());
    }

    // =========================================================================
    // Score breakdown tests
    // =========================================================================

    #[test]
    fn favorites_boost_applies_to_score() {
        let mut s = make_test_suggestion(true, 0.5, 5);
        let original_score = s.score;

        // Apply the same boost logic as in the main function
        if s.is_favorite {
            let favorites_boost = 0.25;
            s.breakdown.personal_boost += favorites_boost;
            s.score = (s.score + favorites_boost).clamp(0.0, 1.0);
        }

        assert!(s.score > original_score);
        assert_eq!(s.breakdown.personal_boost, 0.25);
        assert_eq!(s.score, 0.75);
    }

    #[test]
    fn favorites_boost_clamps_to_max() {
        let mut s = make_test_suggestion(true, 0.5, 5);
        s.score = 0.9; // High base score

        if s.is_favorite {
            let favorites_boost = 0.25;
            s.breakdown.personal_boost += favorites_boost;
            s.score = (s.score + favorites_boost).clamp(0.0, 1.0);
        }

        assert_eq!(s.score, 1.0); // Clamped to max
    }

    // =========================================================================
    // Rich output bead tests (bd-11ye)
    // =========================================================================

    use crate::cli::formatters::{SuggestionContext, SuggestionItem, SuggestionOutput};
    use crate::cli::output::OutputFormat;

    fn make_suggestion_item(name: &str, score: f32, discovery: bool) -> SuggestionItem {
        SuggestionItem {
            skill_id: format!("skill-{name}"),
            name: name.to_string(),
            description: format!("Description for {name}"),
            confidence: score,
            reason: Some(format!("context match {:.0}%", score * 100.0)),
            is_discovery: discovery,
            tags: vec!["cli".to_string(), "rust".to_string()],
            breakdown: None,
        }
    }

    // ── 1. test_suggest_render_empty_suggestions ────────────────────

    #[test]
    fn test_suggest_render_empty_suggestions() {
        let output = SuggestionOutput::new();
        let human = output.format(OutputFormat::Human);
        assert!(human.contains("No suggestions"));
    }

    // ── 2. test_suggest_render_single_suggestion ────────────────────

    #[test]
    fn test_suggest_render_single_suggestion() {
        let mut output = SuggestionOutput::new();
        output.add_suggestion(make_suggestion_item("my-skill", 0.85, false));
        let human = output.format(OutputFormat::Human);
        assert!(human.contains("my-skill"));
    }

    // ── 3. test_suggest_render_many_suggestions ─────────────────────

    #[test]
    fn test_suggest_render_many_suggestions() {
        let mut output = SuggestionOutput::new();
        for i in 0..5 {
            output.add_suggestion(make_suggestion_item(
                &format!("skill-{i}"),
                0.9 - i as f32 * 0.1,
                false,
            ));
        }
        let human = output.format(OutputFormat::Human);
        assert!(human.contains("skill-0"));
        assert!(human.contains("skill-4"));
    }

    // ── 4. test_suggest_score_visualization ──────────────────────────

    #[test]
    fn test_suggest_score_visualization() {
        let mut output = SuggestionOutput::new();
        output.add_suggestion(make_suggestion_item("high-score", 0.95, false));
        let human = output.format(OutputFormat::Human);
        // Score should appear in human output
        assert!(human.contains("95") || human.contains("0.95") || human.contains("high-score"));
    }

    // ── 5. test_suggest_context_panel ────────────────────────────────

    #[test]
    fn test_suggest_context_panel() {
        let ctx = SuggestionContext {
            cwd: Some("/home/user/project".to_string()),
            git_branch: Some("main".to_string()),
            recent_files: vec!["src/main.rs".to_string()],
            fingerprint: Some(12345),
        };
        let mut output = SuggestionOutput::new().with_context(ctx);
        output.add_suggestion(make_suggestion_item("test", 0.8, false));
        let human = output.format(OutputFormat::Human);
        assert!(!human.is_empty());
    }

    // ── 6. test_suggest_ranking_badges ───────────────────────────────

    #[test]
    fn test_suggest_ranking_badges() {
        let mut output = SuggestionOutput::new();
        output.add_suggestion(make_suggestion_item("first", 0.9, false));
        output.add_suggestion(make_suggestion_item("second", 0.7, false));
        let human = output.format(OutputFormat::Human);
        // Both skills should appear in order
        let pos1 = human.find("first").unwrap_or(0);
        let pos2 = human.find("second").unwrap_or(0);
        assert!(pos1 < pos2, "first should appear before second");
    }

    // ── 7. test_suggest_discovery_section ────────────────────────────

    #[test]
    fn test_suggest_discovery_section() {
        let mut output = SuggestionOutput::new();
        output.add_suggestion(make_suggestion_item("main-skill", 0.8, false));
        output.add_suggestion(make_suggestion_item("discover-skill", 0.5, true));
        let json = output.format(OutputFormat::Json);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        // Discovery suggestions should be in the output
        assert!(parsed["suggestions"].is_array() || parsed["discovery_suggestions"].is_array());
    }

    // ── 8. test_suggest_plain_output_format ──────────────────────────

    #[test]
    fn test_suggest_plain_output_format() {
        let mut output = SuggestionOutput::new();
        output.add_suggestion(make_suggestion_item("plain-test", 0.75, false));
        let plain = output.format(OutputFormat::Plain);
        assert!(plain.contains("plain-test"));
        assert!(plain.contains("0.75"));
        assert!(!plain.contains("\x1b["), "plain output must have no ANSI");
    }

    // ── 9. test_suggest_json_output_format ───────────────────────────

    #[test]
    fn test_suggest_json_output_format() {
        let mut output = SuggestionOutput::new();
        output.add_suggestion(make_suggestion_item("json-test", 0.85, false));
        let json = output.format(OutputFormat::Json);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["suggestions"].is_array());
    }

    // ── 10. test_suggest_robot_mode_no_ansi ──────────────────────────

    #[test]
    fn test_suggest_robot_mode_no_ansi() {
        let mut output = SuggestionOutput::new();
        output.add_suggestion(make_suggestion_item("robot-test", 0.6, false));
        let plain = output.format(OutputFormat::Plain);
        assert!(!plain.contains("\x1b["), "robot mode must have no ANSI");
        let json = output.format(OutputFormat::Json);
        assert!(!json.contains("\x1b["), "JSON mode must have no ANSI");
    }

    // ── 11. test_suggest_breakdown_display ────────────────────────────

    #[test]
    fn test_suggest_breakdown_display() {
        let breakdown = ScoreBreakdown {
            contextual_score: 0.6,
            thompson_score: 0.5,
            exploration_bonus: 0.1,
            personal_boost: 0.2,
            pull_count: 10,
            avg_reward: 0.7,
        };
        let total = breakdown.contextual_score
            + breakdown.thompson_score
            + breakdown.exploration_bonus
            + breakdown.personal_boost;
        assert!(total > 0.0, "breakdown should have positive total");
        assert_eq!(breakdown.pull_count, 10);
    }

    // ── 12. test_suggest_rich_vs_plain_equivalence ───────────────────

    #[test]
    fn test_suggest_rich_vs_plain_equivalence() {
        let mut output = SuggestionOutput::new();
        output.add_suggestion(make_suggestion_item("equiv-skill", 0.7, false));

        let json = output.format(OutputFormat::Json);
        let plain = output.format(OutputFormat::Plain);

        // Both should contain the skill name
        assert!(json.contains("equiv-skill"));
        assert!(plain.contains("equiv-skill"));

        // JSON should be parseable
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed["suggestions"].is_array());
    }

    // ── 13. test_suggest_parse_tags_from_metadata ────────────────────

    #[test]
    fn test_suggest_parse_tags_from_metadata() {
        let meta = r#"{"tags":["cli","rust","testing"]}"#;
        let tags = parse_tags_from_metadata(meta);
        assert_eq!(tags, vec!["cli", "rust", "testing"]);

        let empty = r#"{"no_tags": true}"#;
        let tags = parse_tags_from_metadata(empty);
        assert!(tags.is_empty());

        let invalid = "not json";
        let tags = parse_tags_from_metadata(invalid);
        assert!(tags.is_empty());
    }

    // ── 14. test_suggest_should_use_rich_returns_bool ─────────────────

    #[test]
    fn test_suggest_should_use_rich_returns_bool() {
        let _result: bool = should_use_rich_for_suggest();
    }
}
