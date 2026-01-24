//! ms load - Load a skill with progressive disclosure

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{self, Write};
use std::path::PathBuf;

use clap::{Args, ValueEnum};
use colored::Colorize;
use serde::Deserialize;

use crate::app::AppContext;
use crate::cli::output::OutputFormat;
use crate::context::collector::{CollectedContext, ContextCollector, ContextCollectorConfig};
use crate::context::scoring::{RankedSkill, RelevanceScorer, WorkingContext};
use crate::core::dependencies::{
    DependencyGraph, DependencyLoadMode, DependencyResolver, DisclosureLevel as DepDisclosure,
};
use crate::core::disclosure::{
    DisclosedContent, DisclosureLevel, DisclosurePlan, PackMode, TokenBudget, disclose,
};
use crate::core::pack_contracts::{
    PackContractPreset, custom_contracts_path, find_custom_contract,
};
use crate::core::resolution::{DbSkillRepository, resolve_full};
use crate::core::skill::{PackContract, SkillAssets, SkillMetadata};
use crate::core::spec_lens::parse_markdown;
use crate::error::{MsError, Result};
use crate::meta_skills::{ConditionContext, MetaSkillManager, MetaSkillRegistry};
use crate::storage::sqlite::SkillRecord;
use crate::suggestions::bandit::{
    ContextualBandit, DefaultFeatureExtractor, FeatureExtractor, SkillFeedback, UserHistory,
};

/// Dependency loading strategy
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum DepsMode {
    /// Dependencies at overview level
    #[default]
    Auto,
    /// No dependency loading
    Off,
    /// Dependencies at full disclosure
    Full,
}

impl From<DepsMode> for DependencyLoadMode {
    fn from(mode: DepsMode) -> Self {
        match mode {
            DepsMode::Auto => Self::Auto,
            DepsMode::Off => Self::Off,
            DepsMode::Full => Self::Full,
        }
    }
}

/// Pack mode for token budget optimization
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum CliPackMode {
    /// Even distribution across slice types
    #[default]
    Balanced,
    /// Prioritize highest-utility slices
    UtilityFirst,
    /// Prioritize coverage (rules, commands first)
    CoverageFirst,
    /// Boost pitfalls and warnings
    PitfallSafe,
}

/// Pack contract presets for packing strategies.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CliPackContract {
    Complete,
    Debug,
    Refactor,
    Learn,
    Quickref,
    Codegen,
}

impl CliPackContract {
    const fn preset(self) -> PackContractPreset {
        match self {
            Self::Complete => PackContractPreset::Complete,
            Self::Debug => PackContractPreset::Debug,
            Self::Refactor => PackContractPreset::Refactor,
            Self::Learn => PackContractPreset::Learn,
            Self::Quickref => PackContractPreset::QuickRef,
            Self::Codegen => PackContractPreset::CodeGen,
        }
    }
}

impl From<CliPackMode> for PackMode {
    fn from(mode: CliPackMode) -> Self {
        match mode {
            CliPackMode::Balanced => Self::Balanced,
            CliPackMode::UtilityFirst => Self::UtilityFirst,
            CliPackMode::CoverageFirst => Self::CoverageFirst,
            CliPackMode::PitfallSafe => Self::PitfallSafe,
        }
    }
}

#[derive(Args, Debug)]
pub struct LoadArgs {
    /// Skill ID or name to load (optional when using --auto)
    #[arg(required_unless_present = "auto")]
    pub skill: Option<String>,

    /// Automatically detect and load relevant skills based on context
    #[arg(long)]
    pub auto: bool,

    /// Minimum relevance score for auto-loading (0.0-1.0)
    #[arg(long, default_value = "0.3")]
    pub threshold: f32,

    /// Confirm before loading each skill in auto mode
    #[arg(long)]
    pub confirm: bool,

    /// Show what would be loaded without actually loading (dry-run)
    #[arg(long)]
    pub dry_run: bool,

    /// Disclosure level (0=minimal, 1=overview, 2=standard, 3=full, 4=complete)
    #[arg(long, short = 'l')]
    pub level: Option<String>,

    /// Token budget for packing (overrides --level)
    #[arg(long)]
    pub pack: Option<usize>,

    /// Pack mode when using --pack
    #[arg(long, value_enum, default_value = "balanced")]
    pub mode: CliPackMode,

    /// Pack contract preset (requires --pack). Values: complete|debug|refactor|learn|quickref|codegen
    #[arg(long, value_enum)]
    pub contract: Option<CliPackContract>,

    /// Custom pack contract id (requires --pack)
    #[arg(long)]
    pub contract_id: Option<String>,

    /// Max slices per coverage group
    #[arg(long, default_value = "2")]
    pub max_per_group: usize,

    /// Alias for --level full
    #[arg(long)]
    pub full: bool,

    /// Alias for --level complete (includes scripts + references)
    #[arg(long)]
    pub complete: bool,

    /// Dependency loading strategy
    #[arg(long, value_enum, default_value = "auto")]
    pub deps: DepsMode,

    /// Experiment id to attribute this load
    #[arg(long)]
    pub experiment_id: Option<String>,

    /// Variant id for experiment attribution
    #[arg(long)]
    pub variant_id: Option<String>,
}

/// Result of loading a skill
#[derive(Debug, Clone)]
pub struct LoadResult {
    pub skill_id: String,
    pub name: String,
    pub disclosed: DisclosedContent,
    pub dependencies_loaded: Vec<String>,
    pub slices_included: Option<usize>,
    pub inheritance_chain: Vec<String>,
    pub included_from: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn run(ctx: &AppContext, args: &LoadArgs) -> Result<()> {
    // Handle auto mode
    if args.auto {
        return run_auto_load(ctx, args);
    }

    // Get skill reference (unwrap is safe because clap requires it unless --auto is present)
    let skill_ref = args.skill.as_ref().ok_or_else(|| {
        MsError::ValidationFailed("skill argument required when not using --auto".to_string())
    })?;

    // First try to load as meta-skill
    if let Some(meta_result) = try_load_meta_skill(ctx, args, skill_ref)? {
        return match ctx.output_format {
            OutputFormat::Json | OutputFormat::Jsonl | OutputFormat::Toon => {
                output_robot_meta(ctx, &meta_result, args)
            }
            _ => output_human_meta(ctx, &meta_result, args),
        };
    }

    // Fall back to regular skill loading
    let result = load_skill(ctx, args, skill_ref)?;

    match ctx.output_format {
        OutputFormat::Json | OutputFormat::Jsonl | OutputFormat::Toon => {
            output_robot(ctx, &result, args)
        }
        OutputFormat::Plain => output_plain(&result),
        OutputFormat::Tsv => output_tsv(&result),
        OutputFormat::Human => output_human(ctx, &result, args),
    }
}

// =============================================================================
// AUTO-LOAD IMPLEMENTATION
// =============================================================================

/// Result of auto-load operation
#[derive(Debug)]
pub struct AutoLoadResult {
    pub context_summary: ContextSummary,
    pub candidates: Vec<RankedSkill>,
    pub loaded: Vec<LoadResult>,
    pub skipped: Vec<String>,
    pub total_tokens: usize,
}

/// Summary of detected context
#[derive(Debug, Clone)]
pub struct ContextSummary {
    pub project_types: Vec<(String, f32)>,
    pub recent_files_count: usize,
    pub tools: Vec<String>,
}

/// Run auto-load: detect context, score skills, load relevant ones
fn run_auto_load(ctx: &AppContext, args: &LoadArgs) -> Result<()> {
    // Collect current working context
    let working_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let collector = ContextCollector::new(ContextCollectorConfig::default());
    let collected = collector.collect(&working_dir)?;

    // Convert to scoring context
    let scoring_context = convert_to_scoring_context(&collected);
    let context_summary = summarize_context(&collected);

    // Get all indexed skills
    let skills = get_all_skill_metadata(ctx)?;

    if skills.is_empty() {
        match ctx.output_format {
            OutputFormat::Json | OutputFormat::Jsonl | OutputFormat::Toon => {
                let output = serde_json::json!({
                    "status": "ok",
                    "data": {
                        "context": context_summary_to_json(&context_summary),
                        "candidates": [],
                        "loaded": [],
                        "message": "No skills indexed"
                    }
                });
                match ctx.output_format {
                    OutputFormat::Toon => {
                        println!("{}", toon_rust::encode(output, None));
                    }
                    OutputFormat::Jsonl => {
                        println!("{}", serde_json::to_string(&output)?);
                    }
                    _ => {
                        println!("{}", serde_json::to_string_pretty(&output)?);
                    }
                }
            }
            _ => {
                println!("{}", "No skills indexed. Run 'ms index' first.".yellow());
            }
        }
        return Ok(());
    }

    // Score and rank skills
    let scorer = RelevanceScorer::default();
    let mut candidates = scorer.above_threshold(&skills, &scoring_context, args.threshold);

    // Blend in bandit scores from historical learning
    let skill_ids: Vec<String> = candidates.iter().map(|c| c.skill_id.clone()).collect();
    let bandit_scores = get_bandit_scores(&collected, &skill_ids);

    // Apply bandit score boost (blend_factor from config, default 0.3)
    let blend_factor = ctx.config.auto_load.bandit_blend;
    for candidate in &mut candidates {
        if let Some(&bandit_score) = bandit_scores.get(&candidate.skill_id) {
            // Blend: final = (1 - blend) * relevance + blend * bandit
            candidate.score = (1.0 - blend_factor) * candidate.score + blend_factor * bandit_score;
        }
    }

    // Re-sort after blending bandit scores
    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    if candidates.is_empty() {
        match ctx.output_format {
            OutputFormat::Json | OutputFormat::Jsonl | OutputFormat::Toon => {
                let output = serde_json::json!({
                    "status": "ok",
                    "data": {
                        "context": context_summary_to_json(&context_summary),
                        "candidates": [],
                        "loaded": [],
                        "message": format!("No skills match threshold {}", args.threshold)
                    }
                });
                match ctx.output_format {
                    OutputFormat::Toon => {
                        println!("{}", toon_rust::encode(output, None));
                    }
                    OutputFormat::Jsonl => {
                        println!("{}", serde_json::to_string(&output)?);
                    }
                    _ => {
                        println!("{}", serde_json::to_string_pretty(&output)?);
                    }
                }
            }
            _ => {
                println!(
                    "{}",
                    format!("No skills match threshold {}.", args.threshold).yellow()
                );
                println!("Try lowering the threshold with --threshold 0.1");
            }
        }
        return Ok(());
    }

    // Dry-run mode: just show what would be loaded
    if args.dry_run {
        return output_dry_run(ctx, &context_summary, &candidates, args);
    }

    // Load skills (with optional confirmation)
    let mut loaded_results = Vec::new();
    let mut skipped = Vec::new();
    let mut total_tokens = 0usize;

    for candidate in &candidates {
        // Confirm mode: ask user before each load (only for human-readable output)
        if args.confirm && ctx.output_format == OutputFormat::Human {
            print!(
                "Load {} (score: {:.2})? [Y/n] ",
                candidate.skill_id.cyan(),
                candidate.score
            );
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            let input = input.trim().to_lowercase();

            if input == "n" || input == "no" {
                skipped.push(candidate.skill_id.clone());
                continue;
            }
        }

        // Load the skill
        match load_skill(ctx, args, &candidate.skill_id) {
            Ok(result) => {
                total_tokens += result.disclosed.token_estimate;
                loaded_results.push(result);
            }
            Err(e) => {
                if ctx.verbosity > 0 {
                    eprintln!("warning: failed to load {}: {}", candidate.skill_id, e);
                }
                skipped.push(candidate.skill_id.clone());
            }
        }
    }

    // Record auto-load events to the contextual bandit for learning
    if ctx.config.auto_load.learning_enabled {
        let scores: Vec<(String, f32)> = candidates
            .iter()
            .map(|c| (c.skill_id.clone(), c.score))
            .collect();
        record_auto_load_events(
            &collected,
            &loaded_results,
            &scores,
            &ctx.config.auto_load,
            ctx.verbosity.into(),
        );
    }

    let auto_result = AutoLoadResult {
        context_summary,
        candidates,
        loaded: loaded_results,
        skipped,
        total_tokens,
    };

    match ctx.output_format {
        OutputFormat::Json | OutputFormat::Jsonl | OutputFormat::Toon => {
            output_auto_robot(ctx, &auto_result, args)
        }
        _ => output_auto_human(ctx, &auto_result, args),
    }
}

/// Convert `CollectedContext` to `WorkingContext` for scoring
fn convert_to_scoring_context(collected: &CollectedContext) -> WorkingContext {
    WorkingContext::new()
        .with_projects(collected.detected_projects.clone())
        .with_recent_files(
            collected
                .recent_files
                .iter()
                .map(|f| f.path.to_string_lossy().to_string())
                .collect(),
        )
        .with_tools(collected.detected_tools.iter().cloned())
}

// =============================================================================
// CONTEXTUAL BANDIT INTEGRATION FOR AUTO-LOAD
// =============================================================================

/// Default path for the contextual bandit state file.
fn default_bandit_path() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("ms").join("contextual_bandit.json")
}

/// Record auto-load events to the contextual bandit for learning.
///
/// This function records that skills were auto-loaded based on context,
/// enabling the bandit to learn which skills are relevant in different contexts.
fn record_auto_load_events(
    collected: &CollectedContext,
    loaded_skills: &[LoadResult],
    _scores: &[(String, f32)], // Reserved for future: weighted feedback based on relevance
    config: &crate::config::AutoLoadConfig,
    verbosity: i32,
) {
    use crate::suggestions::bandit::{contextual::ContextualBanditConfig, features::FEATURE_DIM};

    // Skip if persistence is disabled
    if !config.persist_state {
        return;
    }

    // Extract context features from the collected context
    let extractor = DefaultFeatureExtractor::new();
    let history_path = UserHistory::default_path();
    let mut history = UserHistory::load(&history_path);
    let features = extractor.extract_from_collected(&collected, &history);

    // Record each skill load to user history
    for result in loaded_skills {
        history.record_skill_load(&result.skill_id);
    }

    // Save updated user history
    if let Err(e) = history.save(&history_path) {
        if verbosity > 0 {
            eprintln!("warning: failed to save user history: {e}");
        }
    }

    // Load the bandit or create new with configured parameters
    let bandit_path = default_bandit_path();
    let mut bandit = match ContextualBandit::load(&bandit_path) {
        Ok(b) => b,
        Err(e) => {
            if verbosity > 1 {
                eprintln!("note: creating new bandit (load failed: {e})");
            }
            // Create new bandit with config from AutoLoadConfig
            let bandit_config = ContextualBanditConfig {
                exploration_rate: config.exploration_rate,
                learning_rate: config.learning_rate,
                cold_start_threshold: config.cold_start_threshold,
                ..Default::default()
            };
            ContextualBandit::new(bandit_config, FEATURE_DIM)
        }
    };

    // Register and update each loaded skill
    for result in loaded_skills {
        // Register the skill if not already known
        bandit.register_skill(&result.skill_id);

        // Record as LoadedOnly initially - user feedback will upgrade this
        bandit.update(&result.skill_id, &features, &SkillFeedback::LoadedOnly);
    }

    // Save the updated bandit
    if let Err(e) = bandit.save(&bandit_path) {
        if verbosity > 0 {
            eprintln!("warning: failed to save bandit: {e}");
        }
    }
}

/// Get bandit-boosted scores for skills based on historical learning.
///
/// This function samples from the contextual bandit to boost scores for
/// skills that have historically performed well in similar contexts.
fn get_bandit_scores(collected: &CollectedContext, skill_ids: &[String]) -> HashMap<String, f32> {
    let extractor = DefaultFeatureExtractor::new();
    let history = UserHistory::load(&UserHistory::default_path());
    let features = extractor.extract_from_collected(collected, &history);

    let bandit_path = default_bandit_path();
    let mut bandit = match ContextualBandit::load(&bandit_path) {
        Ok(b) => b,
        Err(_) => return HashMap::new(),
    };

    let mut scores = HashMap::new();
    for skill_id in skill_ids {
        if bandit.has_skill(skill_id) {
            let sample = bandit.sample(skill_id, &features);
            scores.insert(skill_id.clone(), sample);
        }
    }

    scores
}

/// Create a context summary for output
fn summarize_context(collected: &CollectedContext) -> ContextSummary {
    ContextSummary {
        project_types: collected
            .detected_projects
            .iter()
            .map(|p| (p.project_type.id().to_string(), p.confidence))
            .collect(),
        recent_files_count: collected.recent_files.len(),
        tools: collected.detected_tools.iter().cloned().collect(),
    }
}

/// Get all skill metadata from the database
fn get_all_skill_metadata(ctx: &AppContext) -> Result<Vec<SkillMetadata>> {
    let mut all_skills = Vec::new();
    let mut offset = 0usize;
    let limit = 200usize;

    loop {
        let batch = ctx.db.list_skills(limit, offset)?;
        if batch.is_empty() {
            break;
        }
        offset += batch.len();

        for skill in batch {
            all_skills.push(skill_record_to_metadata(&skill));
        }
    }

    Ok(all_skills)
}

/// Convert `SkillRecord` to `SkillMetadata`
fn skill_record_to_metadata(skill: &SkillRecord) -> SkillMetadata {
    let meta_json: serde_json::Value =
        serde_json::from_str(&skill.metadata_json).unwrap_or_default();

    SkillMetadata {
        id: skill.id.clone(),
        name: skill.name.clone(),
        version: skill.version.clone().unwrap_or_else(|| "0.1.0".to_string()),
        description: skill.description.clone(),
        tags: meta_json
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        requires: meta_json
            .get("requires")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        provides: meta_json
            .get("provides")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        context: serde_json::from_value(meta_json.get("context").cloned().unwrap_or_default())
            .unwrap_or_default(),
        ..Default::default()
    }
}

fn context_summary_to_json(summary: &ContextSummary) -> serde_json::Value {
    serde_json::json!({
        "project_types": summary.project_types.iter().map(|(t, c)| {
            serde_json::json!({"type": t, "confidence": c})
        }).collect::<Vec<_>>(),
        "recent_files": summary.recent_files_count,
        "tools": summary.tools
    })
}

/// Output dry-run results
fn output_dry_run(
    ctx: &AppContext,
    context_summary: &ContextSummary,
    candidates: &[RankedSkill],
    args: &LoadArgs,
) -> Result<()> {
    match ctx.output_format {
        OutputFormat::Json | OutputFormat::Jsonl | OutputFormat::Toon => {
            let output = serde_json::json!({
                "status": "ok",
                "dry_run": true,
                "data": {
                    "context": context_summary_to_json(context_summary),
                    "would_load": candidates.iter().map(|c| {
                        serde_json::json!({
                            "skill_id": c.skill_id,
                            "name": c.skill_name,
                            "score": c.score,
                            "breakdown": {
                                "project_type": c.breakdown.project_type,
                                "file_patterns": c.breakdown.file_patterns,
                                "tools": c.breakdown.tools,
                                "signals": c.breakdown.signals
                            }
                        })
                    }).collect::<Vec<_>>(),
                    "threshold": args.threshold
                }
            });
            match ctx.output_format {
                OutputFormat::Toon => {
                    println!("{}", toon_rust::encode(output, None));
                }
                OutputFormat::Jsonl => {
                    println!("{}", serde_json::to_string(&output)?);
                }
                _ => {
                    println!("{}", serde_json::to_string_pretty(&output)?);
                }
            }
        }
        OutputFormat::Plain => {
            for candidate in candidates {
                println!("{}", candidate.skill_id);
            }
        }
        OutputFormat::Tsv => {
            println!("skill_id\tname\tscore");
            for candidate in candidates {
                println!(
                    "{}\t{}\t{:.2}",
                    candidate.skill_id, candidate.skill_name, candidate.score
                );
            }
        }
        OutputFormat::Human => {
            println!("{}", "Would load skills:".bold());
            println!();

            for (i, candidate) in candidates.iter().enumerate() {
                println!(
                    "  {}. {} (score: {:.2})",
                    i + 1,
                    candidate.skill_id.cyan(),
                    candidate.score
                );
                if ctx.verbosity > 0 {
                    println!(
                        "     {} project: {:.2}, files: {:.2}, tools: {:.2}",
                        "├".dimmed(),
                        candidate.breakdown.project_type,
                        candidate.breakdown.file_patterns,
                        candidate.breakdown.tools
                    );
                }
            }

            println!();
            println!(
                "{} {} skills would be loaded (threshold: {})",
                "Total:".dimmed(),
                candidates.len(),
                args.threshold
            );
        }
    }

    Ok(())
}

/// Output auto-load results in human-readable format
fn output_auto_human(ctx: &AppContext, result: &AutoLoadResult, _args: &LoadArgs) -> Result<()> {
    // Context summary
    println!("{}", "Detecting context...".dimmed());
    if !result.context_summary.project_types.is_empty() {
        for (ptype, confidence) in &result.context_summary.project_types {
            println!(
                "  Project type: {} (confidence: {:.2})",
                ptype.cyan(),
                confidence
            );
        }
    }
    println!(
        "  Recent files: {}",
        result.context_summary.recent_files_count
    );
    if ctx.verbosity > 0 && !result.context_summary.tools.is_empty() {
        println!(
            "  Tools detected: {}",
            result.context_summary.tools.join(", ")
        );
    }
    println!();

    // Candidates found
    if !result.candidates.is_empty() {
        println!("{}", "Relevant skills found:".bold());
        for candidate in &result.candidates {
            let status = if result
                .loaded
                .iter()
                .any(|l| l.skill_id == candidate.skill_id)
            {
                "✓".green().to_string()
            } else {
                "⊘".dimmed().to_string()
            };
            println!(
                "  {} [{:.2}] {} - {}",
                status,
                candidate.score,
                candidate.skill_id.cyan(),
                candidate.skill_name
            );
        }
        println!();
    }

    // Loaded skills content
    if !result.loaded.is_empty() {
        println!("{} {} skills", "Loading".bold(), result.loaded.len());
        println!();

        for load_result in &result.loaded {
            println!("{}", format!("# {}", load_result.name).bold());
            if let Some(ref body) = load_result.disclosed.body {
                println!("{body}");
            }
            println!();
        }
    }

    // Summary footer
    println!("{} {} tokens", "─".repeat(40).dimmed(), result.total_tokens);

    if !result.skipped.is_empty() {
        println!("{} {}", "Skipped:".dimmed(), result.skipped.join(", "));
    }

    Ok(())
}

/// Output auto-load results in robot mode (JSON/TOON)
fn output_auto_robot(ctx: &AppContext, result: &AutoLoadResult, args: &LoadArgs) -> Result<()> {
    let output = serde_json::json!({
        "status": "ok",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "version": env!("CARGO_PKG_VERSION"),
        "data": {
            "context": context_summary_to_json(&result.context_summary),
            "candidates": result.candidates.iter().map(|c| {
                serde_json::json!({
                    "skill_id": c.skill_id,
                    "name": c.skill_name,
                    "score": c.score
                })
            }).collect::<Vec<_>>(),
            "loaded": result.loaded.iter().map(|l| {
                serde_json::json!({
                    "skill_id": l.skill_id,
                    "name": l.name,
                    "token_count": l.disclosed.token_estimate,
                    "content": l.disclosed.body
                })
            }).collect::<Vec<_>>(),
            "skipped": result.skipped,
            "total_tokens": result.total_tokens,
            "threshold": args.threshold
        },
        "warnings": []
    });
    match ctx.output_format {
        OutputFormat::Toon => {
            println!("{}", toon_rust::encode(output, None));
        }
        OutputFormat::Jsonl => {
            println!("{}", serde_json::to_string(&output)?);
        }
        _ => {
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
    }
    Ok(())
}

pub(crate) fn load_skill(ctx: &AppContext, args: &LoadArgs, skill_ref: &str) -> Result<LoadResult> {
    // Resolve skill by ID or alias
    let skill = resolve_skill(ctx, skill_ref)?;

    if args.contract.is_some() && args.contract_id.is_some() {
        return Err(MsError::Config(
            "use either --contract or --contract-id".to_string(),
        ));
    }
    if (args.contract.is_some() || args.contract_id.is_some()) && args.pack.is_none() {
        return Err(MsError::Config("--contract requires --pack".to_string()));
    }

    let contract = resolve_contract(ctx, args)?;

    let (experiment_id, variant_id) = validate_experiment_usage(
        ctx,
        &skill.id,
        args.experiment_id.as_deref(),
        args.variant_id.as_deref(),
    )?;

    // Determine disclosure plan
    let disclosure_plan = determine_disclosure_plan(args, contract);

    // Parse skill body into SkillSpec
    let spec = parse_markdown(&skill.body)
        .map_err(|e| MsError::ValidationFailed(format!("failed to parse skill body: {e}")))?;

    // Merge metadata from database into spec metadata
    let metadata = merge_metadata(&skill, &spec.metadata);
    let mut spec = spec;
    spec.metadata = metadata;

    // Resolve inheritance and composition
    let repo = DbSkillRepository::new(&ctx.db);
    let resolved = resolve_full(&spec, &repo)?;
    let spec = resolved.spec;

    // Load assets from database
    let assets: SkillAssets = serde_json::from_str(&skill.assets_json).unwrap_or_default();

    // Apply disclosure
    let disclosed = disclose(&spec, &assets, &disclosure_plan);
    let slices_included = disclosed.slices_included;

    // Handle dependencies if enabled
    let dependencies_loaded = if matches!(args.deps, DepsMode::Off) {
        vec![]
    } else {
        load_dependencies(ctx, &skill, args)?
    };

    let result = LoadResult {
        skill_id: skill.id.clone(),
        name: skill.name.clone(),
        disclosed,
        dependencies_loaded,
        slices_included,
        inheritance_chain: resolved.inheritance_chain,
        included_from: resolved.included_from,
        warnings: resolved
            .warnings
            .iter()
            .map(|w| format!("{:?}", w))
            .collect(),
    };

    record_usage(
        ctx,
        &skill.id,
        &disclosure_plan,
        experiment_id.as_deref(),
        variant_id.as_deref(),
    );

    Ok(result)
}

fn resolve_skill(ctx: &AppContext, skill_ref: &str) -> Result<SkillRecord> {
    // Try direct ID lookup
    if let Some(skill) = ctx.db.get_skill(skill_ref)? {
        return Ok(skill);
    }

    // Try alias resolution
    if let Some(alias_result) = ctx.db.resolve_alias(skill_ref)? {
        if let Some(skill) = ctx.db.get_skill(&alias_result.canonical_id)? {
            return Ok(skill);
        }
    }

    Err(MsError::SkillNotFound(format!(
        "skill not found: {skill_ref}"
    )))
}

// ==================== Meta-Skill Integration ====================

/// Result of loading a meta-skill
#[derive(Debug)]
pub struct MetaSkillLoadResultWrapper {
    pub meta_skill_id: String,
    pub meta_skill_name: String,
    pub tokens_used: usize,
    pub slices_loaded: usize,
    pub slices_skipped: usize,
    pub packed_content: String,
}

/// Try to load as a meta-skill. Returns None if not found as a meta-skill.
fn try_load_meta_skill(
    ctx: &AppContext,
    args: &LoadArgs,
    skill_ref: &str,
) -> Result<Option<MetaSkillLoadResultWrapper>> {
    let mut registry = MetaSkillRegistry::new();
    let meta_skill_paths = get_meta_skill_paths();

    // Load registry, but don't fail if no meta-skills exist
    if registry.load_from_paths(&meta_skill_paths).unwrap_or(0) == 0 {
        return Ok(None);
    }

    // Try to find meta-skill by ID
    let meta_skill = match registry.get(skill_ref) {
        Some(ms) => ms.clone(),
        None => return Ok(None),
    };

    // Found a meta-skill, load it
    let manager = MetaSkillManager::new(ctx);
    let working_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let tech_stacks = detect_tech_stacks(&working_dir);

    let condition_ctx = ConditionContext {
        working_dir: &working_dir,
        tech_stacks: &tech_stacks,
        loaded_slices: &HashSet::new(),
    };

    // Use pack budget if specified, otherwise use meta-skill's recommended tokens
    let budget = args.pack.unwrap_or(meta_skill.recommended_context_tokens);

    let result = manager.load(&meta_skill, budget, &condition_ctx)?;

    Ok(Some(MetaSkillLoadResultWrapper {
        meta_skill_id: result.meta_skill_id,
        meta_skill_name: meta_skill.name,
        tokens_used: result.tokens_used,
        slices_loaded: result.slices.len(),
        slices_skipped: result.skipped.len(),
        packed_content: result.packed_content,
    }))
}

/// Get meta-skill directories
fn get_meta_skill_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // Project meta-skills directory
    let working_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let project_meta = working_dir.join(".ms").join("meta-skills");
    if project_meta.exists() {
        paths.push(project_meta);
    }

    // Global meta-skills directory
    if let Some(home) = dirs::home_dir() {
        let global_meta = home.join(".ms").join("meta-skills");
        if global_meta.exists() {
            paths.push(global_meta);
        }
    }

    paths
}

/// Detect tech stacks from common config files
fn detect_tech_stacks(working_dir: &std::path::Path) -> Vec<String> {
    let mut stacks = Vec::new();

    let indicators = [
        ("Cargo.toml", "rust"),
        ("package.json", "javascript"),
        ("tsconfig.json", "typescript"),
        ("go.mod", "go"),
        ("requirements.txt", "python"),
        ("pyproject.toml", "python"),
        ("Gemfile", "ruby"),
        ("pom.xml", "java"),
        ("build.gradle", "java"),
        ("composer.json", "php"),
    ];

    for (file, stack) in indicators {
        if working_dir.join(file).exists() {
            stacks.push(stack.to_string());
        }
    }

    stacks
}

fn output_human_meta(
    _ctx: &AppContext,
    result: &MetaSkillLoadResultWrapper,
    _args: &LoadArgs,
) -> Result<()> {
    println!(
        "{} (meta-skill: {})",
        format!("# {}", result.meta_skill_name).bold(),
        result.meta_skill_id.cyan()
    );
    println!();

    // Stats
    println!(
        "{} {} tokens | {} slices loaded | {} skipped",
        "─".repeat(40).dimmed(),
        result.tokens_used,
        result.slices_loaded,
        result.slices_skipped
    );
    println!();

    // Content
    println!("{}", result.packed_content);

    Ok(())
}

fn output_robot_meta(
    ctx: &AppContext,
    result: &MetaSkillLoadResultWrapper,
    args: &LoadArgs,
) -> Result<()> {
    let output = serde_json::json!({
        "status": "ok",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "version": env!("CARGO_PKG_VERSION"),
        "type": "meta_skill",
        "data": {
            "meta_skill_id": result.meta_skill_id,
            "name": result.meta_skill_name,
            "tokens_used": result.tokens_used,
            "budget": args.pack,
            "slices_loaded": result.slices_loaded,
            "slices_skipped": result.slices_skipped,
            "content": result.packed_content,
        },
        "warnings": []
    });
    match ctx.output_format {
        OutputFormat::Toon => {
            println!("{}", toon_rust::encode(output, None));
        }
        OutputFormat::Jsonl => {
            println!("{}", serde_json::to_string(&output)?);
        }
        _ => {
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
    }
    Ok(())
}

fn determine_disclosure_plan(args: &LoadArgs, contract: Option<PackContract>) -> DisclosurePlan {
    // Token budget takes precedence
    if let Some(tokens) = args.pack {
        return DisclosurePlan::Pack(TokenBudget {
            tokens,
            mode: args.mode.into(),
            max_per_group: args.max_per_group,
            contract,
        });
    }

    // Handle --complete flag
    if args.complete {
        return DisclosurePlan::Level(DisclosureLevel::Complete);
    }

    // Handle --full flag
    if args.full {
        return DisclosurePlan::Level(DisclosureLevel::Full);
    }

    // Parse explicit level
    if let Some(ref level_str) = args.level {
        if let Some(level) = DisclosureLevel::from_str_or_level(level_str) {
            return DisclosurePlan::Level(level);
        }
    }

    // Default to Standard
    DisclosurePlan::Level(DisclosureLevel::Standard)
}

fn resolve_contract(ctx: &AppContext, args: &LoadArgs) -> Result<Option<PackContract>> {
    if let Some(contract) = args.contract {
        return Ok(Some(contract.preset().contract()));
    }
    if let Some(ref id) = args.contract_id {
        let path = custom_contracts_path(&ctx.ms_root);
        let Some(contract) = find_custom_contract(&path, id)? else {
            return Err(MsError::SkillNotFound(format!("contract not found: {id}")));
        };
        return Ok(Some(contract));
    }
    Ok(None)
}

#[derive(Deserialize)]
struct ExperimentVariantRef {
    id: String,
}

fn validate_experiment_usage(
    ctx: &AppContext,
    skill_id: &str,
    experiment_id: Option<&str>,
    variant_id: Option<&str>,
) -> Result<(Option<String>, Option<String>)> {
    match (experiment_id, variant_id) {
        (None, None) => return Ok((None, None)),
        (Some(_), None) => {
            return Err(MsError::ValidationFailed(
                "--variant-id is required when --experiment-id is set".to_string(),
            ));
        }
        (None, Some(_)) => {
            return Err(MsError::ValidationFailed(
                "--experiment-id is required when --variant-id is set".to_string(),
            ));
        }
        (Some(experiment_id), Some(variant_id)) => {
            if experiment_id.trim().is_empty() {
                return Err(MsError::ValidationFailed(
                    "experiment id cannot be empty".to_string(),
                ));
            }
            if variant_id.trim().is_empty() {
                return Err(MsError::ValidationFailed(
                    "variant id cannot be empty".to_string(),
                ));
            }
        }
    }

    let experiment_id = experiment_id.expect("checked above");
    let variant_id = variant_id.expect("checked above");

    let record = ctx
        .db
        .get_skill_experiment(experiment_id)?
        .ok_or_else(|| MsError::NotFound(format!("experiment not found: {experiment_id}")))?;

    if record.skill_id != skill_id {
        return Err(MsError::ValidationFailed(format!(
            "experiment {} belongs to skill {}, not {}",
            experiment_id, record.skill_id, skill_id
        )));
    }

    let variants: Vec<ExperimentVariantRef> = serde_json::from_str(&record.variants_json)
        .map_err(|err| MsError::Serialization(format!("experiment variants parse: {err}")))?;

    if !variants.iter().any(|variant| variant.id == variant_id) {
        return Err(MsError::ValidationFailed(format!(
            "unknown variant id for experiment {experiment_id}: {variant_id}"
        )));
    }

    Ok((
        Some(experiment_id.to_string()),
        Some(variant_id.to_string()),
    ))
}

fn record_usage(
    ctx: &AppContext,
    skill_id: &str,
    plan: &DisclosurePlan,
    experiment_id: Option<&str>,
    variant_id: Option<&str>,
) {
    let disclosure_level = match plan {
        DisclosurePlan::Level(level) => level.level_num(),
        DisclosurePlan::Pack(_) => DisclosureLevel::Standard.level_num(),
    };
    let project_path = std::env::current_dir()
        .ok()
        .map(|path| path.to_string_lossy().to_string());

    if let Err(err) = ctx.db.record_skill_usage(
        skill_id,
        project_path.as_deref(),
        disclosure_level,
        None,
        experiment_id,
        variant_id,
    ) {
        if ctx.verbosity > 0 {
            eprintln!("warning: failed to record skill usage: {err}");
        }
    }
}

fn merge_metadata(skill: &SkillRecord, parsed_meta: &SkillMetadata) -> SkillMetadata {
    // Parse metadata_json from database
    let db_meta: serde_json::Value = serde_json::from_str(&skill.metadata_json).unwrap_or_default();

    SkillMetadata {
        id: skill.id.clone(),
        name: skill.name.clone(),
        version: skill.version.clone().unwrap_or_else(|| "0.1.0".to_string()),
        description: skill.description.clone(),
        tags: db_meta
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_else(|| parsed_meta.tags.clone()),
        requires: db_meta
            .get("requires")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_else(|| parsed_meta.requires.clone()),
        provides: db_meta
            .get("provides")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_else(|| parsed_meta.provides.clone()),
        platforms: parsed_meta.platforms.clone(),
        author: skill.author.clone().or_else(|| parsed_meta.author.clone()),
        license: parsed_meta.license.clone(),
        context: parsed_meta.context.clone(),
    }
}

fn load_dependencies(
    ctx: &AppContext,
    skill: &SkillRecord,
    args: &LoadArgs,
) -> Result<Vec<String>> {
    // Parse requires from metadata
    let meta: serde_json::Value = serde_json::from_str(&skill.metadata_json).unwrap_or_default();

    let requires = meta_list(&meta, "requires");

    if requires.is_empty() {
        return Ok(vec![]);
    }

    // Build dependency graph with available skills
    let mut graph = DependencyGraph::new();

    // Add root skill
    let provides = meta_list(&meta, "provides");

    graph.add_skill(skill.id.clone(), requires.clone(), provides);

    let (skill_index, provider_index, meta_cache) = build_dependency_indexes(ctx)?;

    // Resolve required capabilities using provides mappings (transitive).
    let mut loaded_deps = Vec::new();
    let mut loaded_set = HashSet::new();
    let mut seen_caps = HashSet::new();
    let mut queue: VecDeque<String> = requires.into_iter().collect();

    while let Some(cap) = queue.pop_front() {
        if !seen_caps.insert(cap.clone()) {
            continue;
        }

        // Direct skill-id match (capability equals skill id).
        if let Some(dep_skill) = skill_index.get(&cap) {
            if add_dependency_node(&mut graph, dep_skill, &meta_cache, &cap, &mut queue)
                && loaded_set.insert(dep_skill.id.clone())
            {
                loaded_deps.push(dep_skill.id.clone());
            }
            continue;
        }

        // Otherwise, resolve providers by capability.
        if let Some(provider_ids) = provider_index.get(&cap) {
            for provider_id in provider_ids {
                if let Some(dep_skill) = skill_index.get(provider_id) {
                    if add_dependency_node(&mut graph, dep_skill, &meta_cache, &cap, &mut queue)
                        && loaded_set.insert(dep_skill.id.clone())
                    {
                        loaded_deps.push(dep_skill.id.clone());
                    }
                }
            }
        }
    }

    graph.build_edges();

    // Resolve and return dependency list
    let dep_disclosure = match args.deps {
        DepsMode::Auto => DepDisclosure::Overview,
        DepsMode::Full => DepDisclosure::Full,
        DepsMode::Off => return Ok(vec![]),
    };

    let resolver = DependencyResolver::new(&graph);
    let plan = resolver.resolve(&skill.id, dep_disclosure, args.deps.into())?;

    if ctx.verbosity > 0 {
        if !plan.missing.is_empty() {
            let missing = plan
                .missing
                .iter()
                .map(|m| format!("{} (required by {})", m.capability, m.required_by))
                .collect::<Vec<_>>()
                .join(", ");
            eprintln!("warning: missing dependency capabilities: {missing}");
        }
        if !plan.cycles.is_empty() {
            let cycles = plan
                .cycles
                .iter()
                .map(|cycle| cycle.join(" -> "))
                .collect::<Vec<_>>()
                .join("; ");
            eprintln!("warning: dependency cycles detected: {cycles}");
        }
    }

    // Return just the dependency IDs (not the root)
    Ok(plan
        .ordered
        .iter()
        .filter(|p| p.skill_id != skill.id)
        .map(|p| p.skill_id.clone())
        .collect())
}

fn meta_list(meta: &serde_json::Value, key: &str) -> Vec<String> {
    meta.get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Clone)]
struct CachedMeta {
    requires: Vec<String>,
    provides: Vec<String>,
}

fn build_dependency_indexes(
    ctx: &AppContext,
) -> Result<(
    HashMap<String, SkillRecord>,
    HashMap<String, Vec<String>>,
    HashMap<String, CachedMeta>,
)> {
    let mut all_skills = Vec::new();
    let mut offset = 0usize;
    let limit = 200usize;
    loop {
        let batch = ctx.db.list_skills(limit, offset)?;
        if batch.is_empty() {
            break;
        }
        offset += batch.len();
        all_skills.extend(batch);
    }

    let mut skill_index = HashMap::new();
    let mut provider_index: HashMap<String, Vec<String>> = HashMap::new();
    let mut meta_cache = HashMap::new();

    for skill in all_skills {
        let meta_json: serde_json::Value =
            serde_json::from_str(&skill.metadata_json).unwrap_or_default();
        let provides = meta_list(&meta_json, "provides");
        let requires = meta_list(&meta_json, "requires");

        for cap in &provides {
            provider_index
                .entry(cap.clone())
                .or_default()
                .push(skill.id.clone());
        }
        meta_cache.insert(skill.id.clone(), CachedMeta { requires, provides });
        skill_index.insert(skill.id.clone(), skill);
    }

    Ok((skill_index, provider_index, meta_cache))
}

fn add_dependency_node(
    graph: &mut DependencyGraph,
    skill: &SkillRecord,
    meta_cache: &HashMap<String, CachedMeta>,
    fallback_capability: &str,
    queue: &mut VecDeque<String>,
) -> bool {
    if graph.get_node(&skill.id).is_some() {
        return false;
    }

    let meta = meta_cache.get(&skill.id);
    let mut provides = meta.map(|m| m.provides.clone()).unwrap_or_default();
    if provides.is_empty() {
        provides.push(fallback_capability.to_string());
    }

    let requires = meta.map(|m| m.requires.clone()).unwrap_or_default();

    for required in &requires {
        queue.push_back(required.clone());
    }

    graph.add_skill(skill.id.clone(), requires, provides);
    true
}

pub(crate) fn output_human(_ctx: &AppContext, result: &LoadResult, _args: &LoadArgs) -> Result<()> {
    let disclosed = &result.disclosed;

    // Header with skill name
    println!("{}", format!("# {}", disclosed.frontmatter.name).bold());

    // Inheritance info
    if result.inheritance_chain.len() > 1 {
        let chain = result.inheritance_chain.join(" → ");
        println!("{}", format!("Inheritance: {}", chain).dimmed());
    }
    if !result.included_from.is_empty() {
        println!(
            "{}",
            format!("Includes: {}", result.included_from.join(", ")).dimmed()
        );
    }

    println!();

    // Warnings
    if !result.warnings.is_empty() {
        println!("{}", "Warnings during resolution:".yellow());
        for warning in &result.warnings {
            println!("  - {}", warning);
        }
        println!();
    }

    // Description
    if !disclosed.frontmatter.description.is_empty() {
        println!("{}", disclosed.frontmatter.description);
        println!();
    }

    // Dependencies loaded info
    if !result.dependencies_loaded.is_empty() {
        println!(
            "{} {}",
            "Dependencies loaded:".dimmed(),
            result.dependencies_loaded.join(", ")
        );
        println!();
    }

    // Main body content
    if let Some(ref body) = disclosed.body {
        println!("{body}");
    }

    // Scripts (at Complete level)
    if !disclosed.scripts.is_empty() {
        println!();
        println!("{}", "## Scripts".bold());
        for script in &disclosed.scripts {
            println!("- {} ({})", script.path.display(), script.language);
        }
    }

    // References (at Complete level)
    if !disclosed.references.is_empty() {
        println!();
        println!("{}", "## References".bold());
        for reference in &disclosed.references {
            println!("- {} ({})", reference.path.display(), reference.file_type);
        }
    }

    // Footer with stats
    println!();
    println!(
        "{} {} tokens | {} level",
        "─".repeat(40).dimmed(),
        disclosed.token_estimate,
        disclosed.level.name()
    );

    Ok(())
}

fn output_robot(ctx: &AppContext, result: &LoadResult, args: &LoadArgs) -> Result<()> {
    let output = build_robot_payload(result, args);
    match ctx.output_format {
        OutputFormat::Toon => {
            println!("{}", toon_rust::encode(output, None));
        }
        OutputFormat::Jsonl => {
            println!("{}", serde_json::to_string(&output)?);
        }
        _ => {
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
    }
    Ok(())
}

fn output_plain(result: &LoadResult) -> Result<()> {
    println!("{}", result.skill_id);
    Ok(())
}

fn output_tsv(result: &LoadResult) -> Result<()> {
    println!(
        "{}\t{}\t{}\t{}",
        result.skill_id,
        result.name,
        result.disclosed.level.name(),
        result.disclosed.token_estimate
    );
    Ok(())
}

pub(crate) fn build_robot_payload(result: &LoadResult, args: &LoadArgs) -> serde_json::Value {
    let disclosed = &result.disclosed;

    let pack_info = if let Some(tokens) = args.pack {
        serde_json::json!({
            "budget": tokens,
            "mode": format!("{:?}", args.mode),
            "contract": args.contract.map(|c| format!("{c:?}")),
            "contract_id": args.contract_id.clone(),
        })
    } else {
        serde_json::Value::Null
    };

    serde_json::json!({
        "status": "ok",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "version": env!("CARGO_PKG_VERSION"),
        "data": {
            "skill_id": result.skill_id,
            "name": result.name,
            "disclosure_level": disclosed.level.name(),
            "token_count": disclosed.token_estimate,
            "pack": pack_info,
            "content": disclosed.body,
            "frontmatter": {
                "id": disclosed.frontmatter.id,
                "name": disclosed.frontmatter.name,
                "version": disclosed.frontmatter.version,
                "description": disclosed.frontmatter.description,
                "tags": disclosed.frontmatter.tags,
                "requires": disclosed.frontmatter.requires,
            },
            "dependencies_loaded": result.dependencies_loaded,
            "slices_included": result.slices_included,
            "inheritance_chain": result.inheritance_chain,
            "included_from": result.included_from,
            "scripts": disclosed.scripts.iter().map(|s| {
                serde_json::json!({
                    "path": s.path.to_string_lossy(),
                    "language": s.language,
                })
            }).collect::<Vec<_>>(),
            "references": disclosed.references.iter().map(|r| {
                serde_json::json!({
                    "path": r.path.to_string_lossy(),
                    "file_type": r.file_type,
                })
            }).collect::<Vec<_>>(),
        },
        "warnings": result.warnings
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_result_struct() {
        use crate::core::disclosure::{DisclosedContent, DisclosedFrontmatter};

        let result = LoadResult {
            skill_id: "test-skill".to_string(),
            name: "Test Skill".to_string(),
            disclosed: DisclosedContent {
                level: DisclosureLevel::Standard,
                frontmatter: DisclosedFrontmatter {
                    id: "test-skill".to_string(),
                    name: "Test Skill".to_string(),
                    version: "1.0.0".to_string(),
                    description: "A test".to_string(),
                    tags: vec![],
                    requires: vec![],
                },
                body: Some("Body content".to_string()),
                scripts: vec![],
                references: vec![],
                token_estimate: 100,
                slices_included: None,
            },
            dependencies_loaded: vec!["dep1".to_string()],
            slices_included: None,
            inheritance_chain: vec!["test-skill".to_string()],
            included_from: vec![],
            warnings: vec![],
        };

        assert_eq!(result.skill_id, "test-skill");
        assert_eq!(result.name, "Test Skill");
        assert_eq!(result.dependencies_loaded.len(), 1);
        assert!(result.slices_included.is_none());
        assert_eq!(result.inheritance_chain.len(), 1);
    }

    // ==================== DepsMode Tests ====================

    #[test]
    fn test_deps_mode_default() {
        let mode = DepsMode::default();
        assert!(matches!(mode, DepsMode::Auto));
    }

    // ==================== CliPackMode Tests ====================

    #[test]
    fn test_cli_pack_mode_default() {
        let mode = CliPackMode::default();
        assert!(matches!(mode, CliPackMode::Balanced));
    }
}
