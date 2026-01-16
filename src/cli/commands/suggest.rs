use std::collections::HashMap;
use std::path::PathBuf;

use clap::Args;
use colored::Colorize;

use crate::app::AppContext;
use crate::cli::output::{HumanLayout, emit_json};
use crate::context::collector::{CollectedContext, ContextCollector, ContextCollectorConfig};
use crate::context::{ContextCapture, ContextFingerprint};
use crate::error::Result;
use crate::storage::sqlite::SkillRecord;
use crate::suggestions::bandit::contextual::ContextualBandit;
use crate::suggestions::bandit::features::{DefaultFeatureExtractor, FeatureExtractor, UserHistory, FEATURE_DIM};
use crate::suggestions::tracking::SuggestionTracker;
use crate::suggestions::SuggestionCooldownCache;

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
            if !ctx.robot_mode {
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
            if !ctx.robot_mode {
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
    let context_features = feature_extractor.extract_from_collected(&collected_context, &user_history);

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
    let skill_map: HashMap<String, &SkillRecord> = all_skills.iter().map(|s| (s.id.clone(), s)).collect();
    let mut suggestions: Vec<Suggestion> = recommendations
        .iter()
        .filter_map(|rec| {
            let skill = skill_map.get(&rec.skill_id)?;
            let tags = parse_tags_from_metadata(&skill.metadata_json);

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
                tags,
            })
        })
        .collect();

    // 9. Apply domain filter if specified
    if let Some(ref domain) = args.domain {
        let domain_lower = domain.to_lowercase();
        suggestions.retain(|s| {
            s.tags.iter().any(|t| t.to_lowercase().contains(&domain_lower))
                || s.name.to_lowercase().contains(&domain_lower)
                || s.description.to_lowercase().contains(&domain_lower)
        });
    }

    // 10. Apply personal mode (boost historical preferences)
    if args.personal {
        for suggestion in &mut suggestions {
            let frequency = user_history.skill_frequency(&suggestion.skill_id);
            let recency = user_history.skill_recency(&suggestion.skill_id);
            let personal_boost = frequency * 0.3 + recency * 0.2;
            suggestion.breakdown.personal_boost = personal_boost;
            suggestion.score = (suggestion.score + personal_boost).clamp(0.0, 1.0);
        }
        // Re-sort after personal boost
        suggestions.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    }

    // 11. Apply cooldown filter (unless ignored)
    let fp = fingerprint.as_u64();
    if !args.ignore_cooldowns {
        use crate::suggestions::CooldownStatus;
        suggestions.retain(|s| {
            !matches!(cache.status(fp, &s.skill_id), CooldownStatus::Active { .. })
        });
    }

    // 12. Truncate to limit
    suggestions.truncate(args.limit);

    // 13. Build discovery suggestions if requested
    let mut discovery_suggestions: Vec<Suggestion> = Vec::new();
    if args.discover {
        // Find skills not in main suggestions that are under-explored
        let suggested_ids: std::collections::HashSet<_> = suggestions.iter().map(|s| &s.skill_id).collect();
        let mut discovery_candidates: Vec<Suggestion> = all_skills
            .iter()
            .filter(|s| !suggested_ids.contains(&s.id))
            .filter_map(|skill| {
                let rec = recommendations.iter().find(|r| r.skill_id == skill.id);
                let components = rec.map(|r| &r.components);
                let pull_count = components.map(|c| c.pull_count).unwrap_or(0);

                // Only include under-explored skills
                if pull_count >= 10 {
                    return None;
                }

                let tags = parse_tags_from_metadata(&skill.metadata_json);
                let base_score = rec.map(|r| r.score).unwrap_or(0.3);

                Some(Suggestion {
                    skill_id: skill.id.clone(),
                    name: skill.name.clone(),
                    description: skill.description.clone(),
                    score: base_score,
                    breakdown: ScoreBreakdown {
                        contextual_score: components.map(|c| c.contextual_score).unwrap_or(0.0),
                        thompson_score: components.map(|c| c.thompson_score).unwrap_or(0.5),
                        exploration_bonus: components.map(|c| c.exploration_bonus).unwrap_or(0.1),
                        personal_boost: 0.0,
                        pull_count,
                        avg_reward: components.map(|c| c.avg_reward).unwrap_or(0.5),
                    },
                    is_discovery: true,
                    tags,
                })
            })
            .collect();

        // Sort by exploration potential
        discovery_candidates.sort_by(|a, b| {
            let a_potential = a.breakdown.exploration_bonus + (1.0 - a.breakdown.pull_count as f32 / 10.0).max(0.0) * 0.2;
            let b_potential = b.breakdown.exploration_bonus + (1.0 - b.breakdown.pull_count as f32 / 10.0).max(0.0) * 0.2;
            b_potential.partial_cmp(&a_potential).unwrap_or(std::cmp::Ordering::Equal)
        });

        discovery_suggestions = discovery_candidates.into_iter().take(3).collect();
    }

    // 14. Record suggestions for learning
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
    if ctx.robot_mode {
        output_robot(ctx, args, &fingerprint, &suggestions, &discovery_suggestions, &collected_context, &contextual_bandit)
    } else {
        output_human(ctx, args, &fingerprint, &suggestions, &discovery_suggestions, &collected_context)
    }
}

/// Output when no skills are available.
fn output_empty_suggestions(
    ctx: &AppContext,
    _args: &SuggestArgs,
    fingerprint: &ContextFingerprint,
    _cache: &SuggestionCooldownCache,
) -> Result<()> {
    if ctx.robot_mode {
        let payload = serde_json::json!({
            "status": "ok",
            "fingerprint": fingerprint.as_u64(),
            "suggestions": [],
            "discovery_suggestions": [],
            "message": "No skills indexed. Run 'ms index' to index skills."
        });
        emit_json(&payload)
    } else {
        let mut layout = HumanLayout::new();
        layout
            .title("Suggestions")
            .section("Status")
            .bullet("No skills indexed. Run 'ms index' to index skills first.");
        crate::cli::output::emit_human(layout);
        Ok(())
    }
}

/// Output in robot (JSON) mode.
fn output_robot(
    _ctx: &AppContext,
    args: &SuggestArgs,
    fingerprint: &ContextFingerprint,
    suggestions: &[Suggestion],
    discovery_suggestions: &[Suggestion],
    context: &CollectedContext,
    bandit: &ContextualBandit,
) -> Result<()> {
    let context_json = serde_json::json!({
        "project_types": context.detected_projects.iter().map(|p| serde_json::json!({
            "type": format!("{:?}", p.project_type),
            "confidence": p.confidence
        })).collect::<Vec<_>>(),
        "recent_files": context.recent_files.len(),
        "tools": context.detected_tools.iter().collect::<Vec<_>>()
    });

    let suggestions_json: Vec<serde_json::Value> = suggestions
        .iter()
        .map(|s| {
            let mut obj = serde_json::json!({
                "skill_id": s.skill_id,
                "name": s.name,
                "description": s.description,
                "score": s.score,
                "tags": s.tags,
                "discovery": s.is_discovery
            });
            if args.explain {
                obj["breakdown"] = serde_json::json!({
                    "contextual_score": s.breakdown.contextual_score,
                    "thompson_score": s.breakdown.thompson_score,
                    "exploration_bonus": s.breakdown.exploration_bonus,
                    "personal_boost": s.breakdown.personal_boost,
                    "pull_count": s.breakdown.pull_count,
                    "avg_reward": s.breakdown.avg_reward
                });
            }
            obj
        })
        .collect();

    let discovery_json: Vec<serde_json::Value> = discovery_suggestions
        .iter()
        .map(|s| serde_json::json!({
            "skill_id": s.skill_id,
            "name": s.name,
            "description": s.description,
            "score": s.score,
            "reason": format!("under-explored ({} uses), high exploration potential", s.breakdown.pull_count)
        }))
        .collect();

    let bandit_stats = bandit.stats();
    let payload = serde_json::json!({
        "status": "ok",
        "fingerprint": fingerprint.as_u64(),
        "context": context_json,
        "suggestions": suggestions_json,
        "discovery_suggestions": discovery_json,
        "bandit_stats": {
            "num_skills": bandit_stats.num_skills,
            "total_recommendations": bandit_stats.total_recommendations,
            "total_updates": bandit_stats.total_updates,
            "avg_reward": bandit_stats.avg_reward,
            "cold_start_skills": bandit_stats.cold_start_skills
        }
    });

    emit_json(&payload)
}

/// Output in human-readable mode.
fn output_human(
    _ctx: &AppContext,
    args: &SuggestArgs,
    _fingerprint: &ContextFingerprint,
    suggestions: &[Suggestion],
    discovery_suggestions: &[Suggestion],
    context: &CollectedContext,
) -> Result<()> {
    // Context summary
    println!("{}", "Analyzing context...".dimmed());
    for project in &context.detected_projects {
        println!(
            "  {}: {:?} (confidence: {:.2})",
            "Project".cyan(),
            project.project_type,
            project.confidence
        );
    }
    if !context.recent_files.is_empty() {
        println!(
            "  {}: {} files modified recently",
            "Recent".cyan(),
            context.recent_files.len()
        );
    }
    if !context.detected_tools.is_empty() {
        let tools: Vec<&str> = context.detected_tools.iter().take(5).map(|s| s.as_str()).collect();
        println!("  {}: {}", "Tools".cyan(), tools.join(", "));
    }
    println!();

    // Main suggestions
    if suggestions.is_empty() {
        println!("{}", "No suggestions available for current context.".yellow());
        println!("Try running 'ms index' to index skills, or use --discover for exploration.");
    } else {
        println!("{}", "Suggested skills:".bold());
        println!();

        for (i, suggestion) in suggestions.iter().enumerate() {
            let stars = score_to_stars(suggestion.score);
            println!(
                "  {}. {}",
                format!("{}", i + 1).bold(),
                suggestion.name.green().bold()
            );
            if !suggestion.description.is_empty() {
                println!("     {}", suggestion.description.dimmed());
            }
            println!(
                "     Score: {:.2} {}",
                suggestion.score,
                stars.yellow()
            );

            // Explain mode: show score breakdown
            if args.explain {
                println!("     {}", "├─".dimmed());
                println!(
                    "     {} Context match: {:.2}",
                    "├─".dimmed(),
                    suggestion.breakdown.contextual_score
                );
                println!(
                    "     {} Thompson sample: {:.2}",
                    "├─".dimmed(),
                    suggestion.breakdown.thompson_score
                );
                println!(
                    "     {} Exploration bonus: {:.2}",
                    "├─".dimmed(),
                    suggestion.breakdown.exploration_bonus
                );
                if args.personal && suggestion.breakdown.personal_boost > 0.0 {
                    println!(
                        "     {} Personal boost: +{:.2}",
                        "├─".dimmed(),
                        suggestion.breakdown.personal_boost
                    );
                }
                println!(
                    "     {} Historical: {} uses, avg reward {:.2}",
                    "└─".dimmed(),
                    suggestion.breakdown.pull_count,
                    suggestion.breakdown.avg_reward
                );
            }

            if !suggestion.tags.is_empty() {
                let tags_str = suggestion.tags.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
                println!("     Tags: {}", tags_str.dimmed());
            }
            println!();
        }
    }

    // Discovery suggestions
    if !discovery_suggestions.is_empty() {
        println!("{}", "Discovery suggestions (things you might like):".bold());
        println!();

        for suggestion in discovery_suggestions {
            println!(
                "  {} {}",
                "🔍".to_string(),
                suggestion.name.blue()
            );
            if !suggestion.description.is_empty() {
                println!("     {}", suggestion.description.dimmed());
            }
            println!(
                "     {}",
                format!(
                    "\"You haven't tried this yet ({} uses), but it might be useful for your context\"",
                    suggestion.breakdown.pull_count
                ).italic().dimmed()
            );
            println!();
        }
    }

    // Auto-load prompt (if not in auto-load mode)
    if !suggestions.is_empty() && !args.load {
        println!(
            "{}",
            format!(
                "Tip: Use 'ms suggest --load' to automatically load top {} skills",
                args.top
            ).dimmed()
        );
    }

    // Handle auto-load
    if args.load && !suggestions.is_empty() {
        let to_load: Vec<_> = suggestions.iter().take(args.top).collect();
        println!();
        println!(
            "{}",
            format!("Loading top {} suggested skills...", to_load.len()).cyan()
        );
        for suggestion in &to_load {
            println!("  → Loading: {}", suggestion.name);
            // The actual loading would be done by invoking the load command
            // For now, we just indicate what would be loaded
        }
        println!();
        println!(
            "{}",
            "Use 'ms load <skill-name>' to load skills individually.".dimmed()
        );
    }

    Ok(())
}

/// Convert score (0-1) to star rating.
fn score_to_stars(score: f32) -> String {
    let filled = (score * 5.0).round() as usize;
    let empty = 5 - filled;
    format!("{}{}", "★".repeat(filled), "☆".repeat(empty))
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
    let path = user_history_path();
    if !path.exists() {
        return UserHistory::default();
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn user_history_path() -> std::path::PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    base.join("ms").join("user_history.json")
}

fn cooldown_path() -> std::path::PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    base.join("ms").join("cooldowns.json")
}

fn contextual_bandit_path() -> std::path::PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    base.join("ms").join("contextual_bandit.json")
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
}
