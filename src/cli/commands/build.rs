//! ms build - Build skills from CASS sessions
//!
//! This command orchestrates the skill mining pipeline:
//! - Fetch sessions from CASS
//! - Apply redaction and injection filters
//! - Extract patterns and generalize
//! - Synthesize SkillSpec and compile SKILL.md
//!
//! When `--guided` is passed, uses the Brenner Method wizard for
//! structured reasoning and high-quality skill extraction.

use std::fs;
use std::io::{self, Write as IoWrite};
use std::path::PathBuf;

use clap::Args;
use colored::Colorize;
use serde_json::json;

use crate::app::AppContext;
use crate::cass::{
    brenner::{run_interactive, BrennerConfig, BrennerWizard, WizardOutput},
    CassClient, QualityScorer,
};
use crate::cm::CmClient;
use crate::error::{MsError, Result};

#[derive(Args, Debug)]
pub struct BuildArgs {
    /// Build from CASS sessions matching this query
    #[arg(long)]
    pub from_cass: Option<String>,

    /// Interactive guided build using Brenner Method
    #[arg(long)]
    pub guided: bool,

    /// Skill name (required for non-interactive builds)
    #[arg(long)]
    pub name: Option<String>,

    /// Number of sessions to use
    #[arg(long, default_value = "5")]
    pub sessions: usize,

    /// Autonomous build duration (e.g., "4h")
    #[arg(long)]
    pub duration: Option<String>,

    /// Checkpoint interval for long builds
    #[arg(long)]
    pub checkpoint_interval: Option<String>,

    /// Resume a previous build session
    #[arg(long)]
    pub resume: Option<String>,

    /// Seed build with CM (cass-memory) context and rules
    #[arg(long)]
    pub with_cm: bool,

    /// Minimum session quality score (0.0-1.0)
    #[arg(long, default_value = "0.6")]
    pub min_session_quality: f32,

    /// Emit redaction report without building
    #[arg(long)]
    pub redaction_report: bool,

    /// Skip redaction (explicit risk acceptance)
    #[arg(long)]
    pub no_redact: bool,

    /// Skip antipattern/counterexample extraction
    #[arg(long)]
    pub no_antipatterns: bool,

    /// Skip injection filter (explicit risk acceptance)
    #[arg(long)]
    pub no_injection_filter: bool,

    /// Generalization method: "heuristic" or "llm"
    #[arg(long, default_value = "heuristic")]
    pub generalize: String,

    /// Use LLM critique for overgeneralization detection
    #[arg(long)]
    pub llm_critique: bool,

    /// Output directory for generated skill
    #[arg(long, short)]
    pub output: Option<PathBuf>,

    /// Output spec JSON file path
    #[arg(long)]
    pub output_spec: Option<PathBuf>,

    /// Minimum confidence for automatic acceptance
    #[arg(long, default_value = "0.8")]
    pub min_confidence: f32,

    /// Fully automatic build (no prompts)
    #[arg(long)]
    pub auto: bool,

    /// Resolve pending uncertainties
    #[arg(long)]
    pub resolve_uncertainties: bool,
}

/// CM integration context for build process.
pub struct CmBuildContext {
    /// Rules to seed pattern extraction
    pub seed_rules: Vec<crate::cm::PlaybookRule>,
    /// Anti-patterns for pitfalls section
    pub anti_patterns: Vec<crate::cm::AntiPattern>,
    /// Suggested CASS queries from CM
    pub suggested_queries: Vec<String>,
}

impl CmBuildContext {
    /// Fetch CM context for a topic.
    pub fn fetch(client: &CmClient, topic: &str) -> Result<Option<Self>> {
        if !client.is_available() {
            return Ok(None);
        }

        let context = client.context(topic)?;
        Ok(Some(Self {
            seed_rules: context.relevant_bullets,
            anti_patterns: context.anti_patterns,
            suggested_queries: context.suggested_cass_queries,
        }))
    }
}

pub fn run(ctx: &AppContext, args: &BuildArgs) -> Result<()> {
    // Validate incompatible options
    if args.guided && args.auto {
        return Err(MsError::Config(
            "--guided and --auto are mutually exclusive".into(),
        ));
    }

    // Warn about risky flags
    if (args.no_redact || args.no_injection_filter) && !args.auto && !args.guided {
        if !ctx.robot_mode {
            eprintln!(
                "{} Using --no-redact or --no-injection-filter bypasses safety filters.",
                "Warning:".yellow()
            );
            eprint!("Continue? [y/N] ");
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            if !input.trim().eq_ignore_ascii_case("y") {
                return Err(MsError::Config("Build cancelled".into()));
            }
        }
    }

    // Initialize CM client if --with-cm flag is set
    let cm_context = if args.with_cm {
        let cm_client = CmClient::from_config(&ctx.config.cm);

        let topic = args
            .from_cass
            .as_deref()
            .or(args.name.as_deref())
            .unwrap_or("general");

        match CmBuildContext::fetch(&cm_client, topic) {
            Ok(Some(cm_ctx)) => {
                if !ctx.robot_mode {
                    if !cm_ctx.seed_rules.is_empty() {
                        eprintln!(
                            "{} Loaded {} CM rules as seeds",
                            "Info:".cyan(),
                            cm_ctx.seed_rules.len()
                        );
                    }
                    if !cm_ctx.anti_patterns.is_empty() {
                        eprintln!(
                            "{} Loaded {} anti-patterns for pitfalls",
                            "Info:".cyan(),
                            cm_ctx.anti_patterns.len()
                        );
                    }
                }
                Some(cm_ctx)
            }
            Ok(None) => {
                if !ctx.robot_mode {
                    eprintln!("{} CM not available, proceeding without CM context", "Warning:".yellow());
                }
                None
            }
            Err(e) => {
                if !ctx.robot_mode {
                    eprintln!("{} Failed to fetch CM context: {e}", "Warning:".yellow());
                }
                None
            }
        }
    } else {
        None
    };

    // Handle resume
    if let Some(ref session_id) = args.resume {
        return run_resume(ctx, args, session_id);
    }

    // Handle resolve uncertainties
    if args.resolve_uncertainties {
        return run_resolve_uncertainties(ctx, args);
    }

    // Guided mode uses Brenner wizard
    if args.guided {
        return run_guided(ctx, args, cm_context.as_ref());
    }

    // Auto mode
    if args.auto {
        return run_auto(ctx, args, cm_context.as_ref());
    }

    // Default: interactive but not guided
    run_interactive_build(ctx, args, cm_context.as_ref())
}

/// Run guided build using Brenner Method wizard
fn run_guided(ctx: &AppContext, args: &BuildArgs, cm_context: Option<&CmBuildContext>) -> Result<()> {
    let query = args
        .from_cass
        .clone()
        .unwrap_or_else(|| "skill patterns".to_string());

    let output_dir = args.output.clone().unwrap_or_else(|| {
        ctx.ms_root.join("builds").join(
            query
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                .collect::<String>(),
        )
    });

    // Ensure output directory exists
    fs::create_dir_all(&output_dir)?;

    let config = BrennerConfig {
        min_quality: args.min_session_quality,
        min_confidence: args.min_confidence,
        max_sessions: args.sessions,
        output_dir: output_dir.clone(),
    };

    let mut wizard = BrennerWizard::new(&query, config);

    // Show CM suggestions if available
    if let Some(cm_ctx) = cm_context {
        if !cm_ctx.suggested_queries.is_empty() && !ctx.robot_mode {
            eprintln!("\n{} CM suggested CASS queries:", "Tip:".cyan());
            for q in &cm_ctx.suggested_queries {
                eprintln!("   - {q}");
            }
            eprintln!();
        }
    }

    // Create CASS client and quality scorer
    let client = if let Some(ref cass_path) = ctx.config.cass.cass_path {
        CassClient::with_binary(cass_path)
    } else {
        CassClient::new()
    };
    let quality_scorer = QualityScorer::with_defaults();

    if ctx.robot_mode {
        // Robot mode: output checkpoint ID and wait for commands
        let output = json!({
            "status": "wizard_started",
            "checkpoint_id": wizard.checkpoint().id,
            "query": query,
            "output_dir": output_dir.display().to_string(),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    // Run interactive wizard
    match run_interactive(&mut wizard, &client, &quality_scorer)? {
        WizardOutput::Success {
            skill_path,
            manifest_path,
            calibration_path,
        } => {
            // Write outputs
            if let Some(draft) = get_draft_from_wizard(&wizard) {
                let skill_md = wizard.generate_skill_md(&draft);
                fs::write(&skill_path, &skill_md)?;

                let manifest = wizard.generate_manifest()?;
                fs::write(&manifest_path, &manifest)?;

                // Write calibration notes
                let calibration = if draft.calibration.is_empty() {
                    "# Calibration Notes\n\nNo calibration notes recorded.\n".to_string()
                } else {
                    let mut cal = "# Calibration Notes\n\n".to_string();
                    for note in &draft.calibration {
                        cal.push_str(&format!("- {}\n", note));
                    }
                    cal
                };
                fs::write(&calibration_path, calibration)?;

                println!("\n{} Build complete!", "Success:".green());
                println!("  Skill: {}", skill_path.display());
                println!("  Manifest: {}", manifest_path.display());
                println!("  Calibration: {}", calibration_path.display());
            }
        }
        WizardOutput::Cancelled {
            reason,
            checkpoint_id,
        } => {
            println!("\n{} Build cancelled: {}", "Info:".yellow(), reason);
            if let Some(id) = checkpoint_id {
                println!("  Resume with: ms build --resume {}", id);
            }
        }
    }

    Ok(())
}

/// Run automatic build (no user interaction)
fn run_auto(ctx: &AppContext, args: &BuildArgs, cm_context: Option<&CmBuildContext>) -> Result<()> {
    use crate::cass::mining::{extract_from_session, ExtractedPattern};
    use crate::cass::QualityConfig;

    let query = args.from_cass.clone().ok_or_else(|| {
        MsError::Config("--from-cass is required for --auto builds".into())
    })?;

    let output_dir = args.output.clone().unwrap_or_else(|| {
        ctx.ms_root.join("builds").join(
            query
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                .collect::<String>(),
        )
    });

    // Ensure output directory exists
    fs::create_dir_all(&output_dir)?;

    if ctx.robot_mode {
        let output = json!({
            "status": "auto_build_started",
            "query": query,
            "sessions": args.sessions,
            "min_confidence": args.min_confidence,
            "output_dir": output_dir.display().to_string(),
            "cm_available": cm_context.is_some(),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", "Starting automatic build...".bold());
        println!("  Query: {}", query);
        println!("  Sessions: {}", args.sessions);
        println!("  Min confidence: {:.0}%", args.min_confidence * 100.0);
        println!("  Output: {}", output_dir.display());
        if let Some(cm_ctx) = cm_context {
            println!("  CM rules: {}", cm_ctx.seed_rules.len());
        }
    }

    // Step 1: Create CASS client and quality scorer
    let cass_client = if let Some(ref cass_path) = ctx.config.cass.cass_path {
        CassClient::with_binary(cass_path)
    } else {
        CassClient::new()
    };

    let quality_config = QualityConfig {
        min_score: args.min_session_quality,
        ..Default::default()
    };
    let quality_scorer = QualityScorer::new(quality_config.clone());

    // Step 2: Search CASS for sessions
    if !ctx.robot_mode {
        println!("\n{} Searching CASS...", "Step 1:".cyan());
    }

    let search_limit = args.sessions * 3; // Search more to account for quality filtering
    let session_matches = cass_client.search(&query, search_limit)?;

    if session_matches.is_empty() {
        if ctx.robot_mode {
            let output = json!({
                "status": "no_sessions",
                "query": query,
                "message": "No sessions found matching query"
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!("{} No sessions found matching query: {}", "Error:".red(), query);
        }
        return Ok(());
    }

    if !ctx.robot_mode {
        println!("  Found {} potential sessions", session_matches.len());
    }

    // Step 3: Score and filter sessions by quality
    if !ctx.robot_mode {
        println!("\n{} Quality filtering...", "Step 2:".cyan());
    }

    let mut quality_sessions = Vec::new();
    let mut skipped_sessions = Vec::new();

    for session_match in session_matches.into_iter().take(search_limit) {
        match cass_client.get_session(&session_match.session_id) {
            Ok(session) => {
                let quality = quality_scorer.score(&session);
                if quality.passes_threshold(&quality_config) {
                    quality_sessions.push((session, quality));
                    if quality_sessions.len() >= args.sessions {
                        break;
                    }
                } else {
                    skipped_sessions.push((session_match.session_id, quality.score));
                }
            }
            Err(e) => {
                if !ctx.robot_mode {
                    eprintln!("  Warning: Failed to fetch session {}: {}", session_match.session_id, e);
                }
            }
        }
    }

    if quality_sessions.is_empty() {
        if ctx.robot_mode {
            let output = json!({
                "status": "no_quality_sessions",
                "query": query,
                "skipped": skipped_sessions.len(),
                "min_quality": args.min_session_quality,
                "message": "No sessions passed quality threshold"
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!(
                "{} No sessions passed quality threshold (min: {:.0}%)",
                "Error:".red(),
                args.min_session_quality * 100.0
            );
            if !skipped_sessions.is_empty() {
                println!("  {} sessions were below threshold:", skipped_sessions.len());
                for (id, score) in skipped_sessions.iter().take(5) {
                    println!("    - {} ({:.0}%)", id, score * 100.0);
                }
            }
        }
        return Ok(());
    }

    if !ctx.robot_mode {
        println!(
            "  {} sessions passed quality threshold (min: {:.0}%)",
            quality_sessions.len(),
            args.min_session_quality * 100.0
        );
        for (session, quality) in &quality_sessions {
            println!("    - {} ({:.0}%)", session.id, quality.score * 100.0);
        }
    }

    // Step 4: Extract patterns from sessions
    if !ctx.robot_mode {
        println!("\n{} Extracting patterns...", "Step 3:".cyan());
    }

    let mut all_patterns: Vec<ExtractedPattern> = Vec::new();

    for (session, _quality) in &quality_sessions {
        match extract_from_session(session) {
            Ok(patterns) => {
                if !ctx.robot_mode && !patterns.is_empty() {
                    println!("  {} patterns from {}", patterns.len(), session.id);
                }
                all_patterns.extend(patterns);
            }
            Err(e) => {
                if !ctx.robot_mode {
                    eprintln!("  Warning: Failed to extract from {}: {}", session.id, e);
                }
            }
        }
    }

    if all_patterns.is_empty() {
        if ctx.robot_mode {
            let output = json!({
                "status": "no_patterns",
                "query": query,
                "sessions_processed": quality_sessions.len(),
                "message": "No patterns extracted from sessions"
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!("{} No patterns extracted from sessions", "Error:".red());
        }
        return Ok(());
    }

    if !ctx.robot_mode {
        println!("  Total: {} patterns extracted", all_patterns.len());
    }

    // Step 5: Filter patterns by confidence
    if !ctx.robot_mode {
        println!("\n{} Filtering by confidence...", "Step 4:".cyan());
    }

    let high_confidence_patterns: Vec<_> = all_patterns
        .into_iter()
        .filter(|p| p.confidence >= args.min_confidence)
        .collect();

    if !ctx.robot_mode {
        println!(
            "  {} patterns above confidence threshold ({:.0}%)",
            high_confidence_patterns.len(),
            args.min_confidence * 100.0
        );
    }

    // Step 6: Filter out tainted patterns (unless --no-injection-filter)
    let pre_taint_count = high_confidence_patterns.len();
    let filtered_patterns: Vec<_> = if args.no_injection_filter {
        high_confidence_patterns
    } else {
        high_confidence_patterns
            .into_iter()
            .filter(|p| p.taint_label.is_none())
            .collect()
    };

    if !ctx.robot_mode && filtered_patterns.len() < pre_taint_count {
        println!(
            "  {} patterns after taint filtering",
            filtered_patterns.len()
        );
    }

    // Step 7: Output results
    if !ctx.robot_mode {
        println!("\n{} Writing outputs...", "Step 5:".cyan());
    }

    // Write patterns JSON
    let patterns_path = output_dir.join("patterns.json");
    let patterns_json = serde_json::to_string_pretty(&filtered_patterns)?;
    fs::write(&patterns_path, &patterns_json)?;

    if !ctx.robot_mode {
        println!("  Patterns: {}", patterns_path.display());
    }

    // Write build manifest
    let manifest = json!({
        "version": "1.0.0",
        "query": query,
        "build_type": "auto",
        "sessions_used": quality_sessions.iter().map(|(s, q)| json!({
            "id": s.id,
            "quality_score": q.score,
        })).collect::<Vec<_>>(),
        "patterns_extracted": filtered_patterns.len(),
        "min_confidence": args.min_confidence,
        "min_session_quality": args.min_session_quality,
        "cm_context_used": cm_context.is_some(),
        "filters": {
            "redaction_enabled": !args.no_redact,
            "injection_filter_enabled": !args.no_injection_filter,
        },
        "created_at": chrono::Utc::now().to_rfc3339(),
    });
    let manifest_path = output_dir.join("build-manifest.json");
    fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;

    if !ctx.robot_mode {
        println!("  Manifest: {}", manifest_path.display());
    }

    // Output spec JSON if requested
    if let Some(spec_path) = &args.output_spec {
        fs::write(spec_path, &patterns_json)?;
        if !ctx.robot_mode {
            println!("  Spec: {}", spec_path.display());
        }
    }

    // Final summary
    if ctx.robot_mode {
        let output = json!({
            "status": "complete",
            "query": query,
            "sessions_used": quality_sessions.len(),
            "patterns_extracted": filtered_patterns.len(),
            "output_dir": output_dir.display().to_string(),
            "patterns_path": patterns_path.display().to_string(),
            "manifest_path": manifest_path.display().to_string(),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("\n{} Auto build complete!", "Success:".green());
        println!("  Sessions processed: {}", quality_sessions.len());
        println!("  Patterns extracted: {}", filtered_patterns.len());
        println!("  Output directory: {}", output_dir.display());
    }

    Ok(())
}

/// Run interactive build (not guided)
fn run_interactive_build(ctx: &AppContext, args: &BuildArgs, cm_context: Option<&CmBuildContext>) -> Result<()> {
    if ctx.robot_mode {
        let output = json!({
            "error": true,
            "code": "interactive_required",
            "message": "Interactive build requires terminal. Use --auto or --guided with robot mode.",
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    println!("{}", "Interactive Build".bold());
    println!();

    if args.from_cass.is_none() {
        println!("Usage: ms build --from-cass <query> [options]");
        println!();
        println!("Options:");
        println!("  --guided              Use Brenner Method wizard");
        println!("  --auto                Fully automatic (no prompts)");
        println!("  --sessions N          Number of sessions to use");
        println!("  --min-confidence N    Minimum confidence threshold");
        println!("  --with-cm             Seed with CM context");
        println!();

        if let Some(cm_ctx) = cm_context {
            if !cm_ctx.suggested_queries.is_empty() {
                println!("{} CM suggested queries:", "Tip:".cyan());
                for q in &cm_ctx.suggested_queries {
                    println!("   ms build --guided --from-cass \"{q}\"");
                }
                println!();
            }
        }

        println!("For guided skill mining, use: ms build --guided --from-cass <query>");
        return Ok(());
    }

    // Default to guided for interactive use
    run_guided(ctx, args, cm_context)
}

/// Resume a previous build session
fn run_resume(ctx: &AppContext, args: &BuildArgs, session_id: &str) -> Result<()> {
    use crate::core::recovery::Checkpoint;

    // Try to load checkpoint
    let checkpoint = match Checkpoint::load(&ctx.ms_root, session_id)? {
        Some(cp) => cp,
        None => {
            if ctx.robot_mode {
                let output = json!({
                    "error": true,
                    "code": "checkpoint_not_found",
                    "session_id": session_id,
                    "message": format!("No checkpoint found for session: {}", session_id),
                });
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                println!(
                    "{} No checkpoint found for session: {}",
                    "Error:".red(),
                    session_id
                );
                println!("\nTo list available checkpoints:");
                println!("  ls {}/.ms/checkpoints/", ctx.ms_root.display());
            }
            return Ok(());
        }
    };

    // Validate checkpoint is for a build operation
    if checkpoint.operation_type != "build" && checkpoint.operation_type != "wizard" {
        if ctx.robot_mode {
            let output = json!({
                "error": true,
                "code": "invalid_checkpoint_type",
                "session_id": session_id,
                "operation_type": checkpoint.operation_type,
                "message": "Checkpoint is not from a build operation",
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!(
                "{} Checkpoint {} is not from a build operation (type: {})",
                "Error:".red(),
                session_id,
                checkpoint.operation_type
            );
        }
        return Ok(());
    }

    // Display checkpoint info
    if ctx.robot_mode {
        let output = json!({
            "status": "resuming",
            "session_id": session_id,
            "operation_type": checkpoint.operation_type,
            "phase": checkpoint.phase,
            "progress": checkpoint.progress,
            "created_at": checkpoint.created_at.to_rfc3339(),
            "updated_at": checkpoint.updated_at.to_rfc3339(),
            "state": checkpoint.state,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", "Resuming build from checkpoint...".bold());
        println!("  Session: {}", session_id);
        println!("  Phase: {}", checkpoint.phase);
        println!("  Progress: {:.0}%", checkpoint.progress * 100.0);
        println!("  Created: {}", checkpoint.created_at);

        if let Some(query) = checkpoint.get_state("query") {
            println!("  Query: {}", query);
        }
        if let Some(sessions) = checkpoint.get_state("sessions_processed") {
            println!("  Sessions processed: {}", sessions);
        }
    }

    // Resume based on checkpoint type/phase
    match checkpoint.phase.as_str() {
        "wizard_started" | "pattern_extraction" | "materialization_test" => {
            // These are Brenner wizard phases - need to recreate wizard state
            let query = checkpoint
                .get_state("query")
                .unwrap_or("skill patterns")
                .to_string();

            let output_dir = checkpoint
                .get_state("output_dir")
                .map(PathBuf::from)
                .or_else(|| args.output.clone())
                .unwrap_or_else(|| {
                    ctx.ms_root.join("builds").join(
                        query
                            .chars()
                            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                            .collect::<String>(),
                    )
                });

            if !ctx.robot_mode {
                println!("\n{} Checkpoint indicates Brenner wizard session", "Info:".cyan());
                println!("  Use --guided flag to continue wizard workflow:");
                println!(
                    "    ms build --guided --from-cass \"{}\" --output {:?}",
                    query,
                    output_dir.display()
                );
            }
        }
        "auto_build" | "pattern_filtering" | "synthesis" => {
            // Auto build phases - can resume from checkpoint state
            if let Some(query) = checkpoint.get_state("query") {
                if !ctx.robot_mode {
                    println!("\n{} Auto build checkpoint found", "Info:".cyan());
                    println!("  Restarting auto build from beginning...");
                }

                // Get CM context if available
                let cm_context = if args.with_cm {
                    let cm_client = CmClient::from_config(&ctx.config.cm);
                    CmBuildContext::fetch(&cm_client, query).ok().flatten()
                } else {
                    None
                };

                // Resume by restarting with same parameters
                // (full incremental resume would require more state)
                return run_auto(ctx, args, cm_context.as_ref());
            }
        }
        _ => {
            if !ctx.robot_mode {
                println!(
                    "\n{} Unknown checkpoint phase: {}",
                    "Warning:".yellow(),
                    checkpoint.phase
                );
                println!("  This checkpoint may be from an older version.");
            }
        }
    }

    Ok(())
}

/// Resolve pending uncertainties
fn run_resolve_uncertainties(ctx: &AppContext, args: &BuildArgs) -> Result<()> {
    use crate::cass::{DefaultResolver, UncertaintyResolver, UncertaintyStatus};

    // Load uncertainty queue from file or create new
    let uncertainties_path = ctx.ms_root.join(".ms").join("uncertainties.json");
    let (_queue, items) = load_uncertainties(&uncertainties_path)?;

    // Get counts
    let counts = count_uncertainties(&items);

    // If no pending items, we're done
    if counts.pending == 0 && counts.in_progress == 0 {
        if ctx.robot_mode {
            let output = json!({
                "status": "no_pending",
                "message": "No pending uncertainties to resolve",
                "counts": {
                    "pending": counts.pending,
                    "in_progress": counts.in_progress,
                    "resolved": counts.resolved,
                    "rejected": counts.rejected,
                    "needs_human": counts.needs_human,
                    "expired": counts.expired,
                    "total": counts.total(),
                    "active": counts.active(),
                },
                "path": uncertainties_path.display().to_string(),
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!("{}", "Uncertainty Queue Status".bold());
            println!();
            println!("  Pending:     0");
            println!("  In Progress: 0");
            println!("  Resolved:    {}", counts.resolved);
            println!("  Rejected:    {}", counts.rejected);
            println!("  Total:       {}", counts.total());
            println!();
            println!("{} No pending uncertainties to resolve", "Info:".cyan());
        }
        return Ok(());
    }

    // Display status (only when there are pending items)
    if ctx.robot_mode {
        let output = json!({
            "status": "uncertainty_queue",
            "counts": {
                "pending": counts.pending,
                "in_progress": counts.in_progress,
                "resolved": counts.resolved,
                "rejected": counts.rejected,
                "needs_human": counts.needs_human,
                "expired": counts.expired,
                "total": counts.total(),
                "active": counts.active(),
            },
            "path": uncertainties_path.display().to_string(),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", "Uncertainty Queue Status".bold());
        println!();
        println!(
            "  Pending:     {}{}",
            counts.pending,
            if counts.pending > 0 { " ⏳" } else { "" }
        );
        println!("  In Progress: {}", counts.in_progress);
        println!(
            "  Needs Human: {}{}",
            counts.needs_human,
            if counts.needs_human > 0 { " 👤" } else { "" }
        );
        println!(
            "  Resolved:    {} {}",
            counts.resolved,
            "✓".green()
        );
        println!("  Rejected:    {}", counts.rejected);
        println!("  Expired:     {}", counts.expired);
        println!("  ──────────────────");
        println!("  Total:       {}", counts.total());
        println!();
    }

    // Get pending items
    let pending_items: Vec<_> = items
        .iter()
        .filter(|i| matches!(i.status, UncertaintyStatus::Pending))
        .collect();

    if !ctx.robot_mode && !pending_items.is_empty() {
        println!("{}", "Pending Uncertainties:".bold());
        for (i, item) in pending_items.iter().take(10).enumerate() {
            let reason_str = format_uncertainty_reason(&item.reason);
            let description = item
                .pattern_candidate
                .description
                .as_deref()
                .unwrap_or("(no description)");
            println!(
                "  {}. {} ({:.0}% confidence)",
                i + 1,
                description.chars().take(50).collect::<String>(),
                item.confidence * 100.0
            );
            println!("     Reason: {}", reason_str);
            println!("     Queries: {}", item.suggested_queries.len());
            println!("     ID: {}", item.id);
            println!();
        }
        if pending_items.len() > 10 {
            println!("  ... and {} more", pending_items.len() - 10);
        }
    }

    // Auto-resolution flow
    if args.auto && !pending_items.is_empty() {
        if !ctx.robot_mode {
            println!("\n{} Running auto-resolution...", "Step:".cyan());
        }

        let resolver = DefaultResolver::new(args.min_confidence, 5);
        let cass_client = if let Some(ref cass_path) = ctx.config.cass.cass_path {
            CassClient::with_binary(cass_path)
        } else {
            CassClient::new()
        };

        let mut resolved_count = 0;
        let mut escalated_count = 0;
        let mut updated_items = items.clone();

        for item in pending_items.into_iter() {
            let mut item = item.clone();

            // Execute unexecuted queries
            let mut new_sessions = Vec::new();
            for query in item.suggested_queries.iter_mut() {
                if query.executed {
                    continue;
                }

                // Execute the CASS query
                let cass_query = query.cass_query.as_ref().unwrap_or(&query.query);
                match cass_client.search(cass_query, 5) {
                    Ok(matches) => {
                        let session_ids: Vec<_> =
                            matches.iter().map(|m| m.session_id.clone()).collect();
                        let relevance_scores: Vec<_> =
                            matches.iter().map(|m| m.score).collect();

                        query.executed = true;
                        query.results = Some(crate::cass::QueryResults {
                            executed_at: chrono::Utc::now(),
                            sessions_found: session_ids.len(),
                            session_ids: session_ids.clone(),
                            relevance_scores,
                            execution_time_ms: 0,
                        });

                        new_sessions.extend(session_ids);

                        if !ctx.robot_mode {
                            println!(
                                "    Query '{}': {} sessions found",
                                query.query.chars().take(40).collect::<String>(),
                                query.results.as_ref().map(|r| r.sessions_found).unwrap_or(0)
                            );
                        }
                    }
                    Err(e) => {
                        if !ctx.robot_mode {
                            eprintln!("    Query failed: {}", e);
                        }
                    }
                }
            }

            // Attempt resolution with gathered evidence
            let result = resolver.attempt_resolution(&mut item, &new_sessions);

            match result {
                crate::cass::ResolutionResult::Resolved(resolution) => {
                    item.status = UncertaintyStatus::Resolved {
                        new_confidence: item.confidence,
                        resolution,
                        resolved_at: chrono::Utc::now(),
                    };
                    resolved_count += 1;
                    if !ctx.robot_mode {
                        println!(
                            "  {} Resolved: {}",
                            "✓".green(),
                            item.pattern_candidate
                                .description
                                .as_deref()
                                .unwrap_or(&item.id)
                        );
                    }
                }
                crate::cass::ResolutionResult::NeedsMoreEvidence { .. } => {
                    // Keep in pending state for next round
                    if !ctx.robot_mode {
                        println!(
                            "  {} Needs more evidence: {}",
                            "…".yellow(),
                            item.pattern_candidate
                                .description
                                .as_deref()
                                .unwrap_or(&item.id)
                        );
                    }
                }
                crate::cass::ResolutionResult::Escalate { reason } => {
                    item.status = UncertaintyStatus::NeedsHuman {
                        reason: reason.clone(),
                        escalated_at: chrono::Utc::now(),
                    };
                    escalated_count += 1;
                    if !ctx.robot_mode {
                        println!(
                            "  {} Escalated: {} - {}",
                            "👤".yellow(),
                            item.pattern_candidate
                                .description
                                .as_deref()
                                .unwrap_or(&item.id),
                            reason
                        );
                    }
                }
                crate::cass::ResolutionResult::Reject { reason } => {
                    item.status = UncertaintyStatus::Rejected {
                        reason: reason.clone(),
                        rejected_at: chrono::Utc::now(),
                    };
                    if !ctx.robot_mode {
                        println!(
                            "  {} Rejected: {} - {}",
                            "✗".red(),
                            item.pattern_candidate
                                .description
                                .as_deref()
                                .unwrap_or(&item.id),
                            reason
                        );
                    }
                }
            }

            // Update in the items list
            if let Some(pos) = updated_items.iter().position(|i| i.id == item.id) {
                updated_items[pos] = item;
            }
        }

        // Save updated uncertainties
        save_uncertainties(&uncertainties_path, &updated_items)?;

        // Summary
        if ctx.robot_mode {
            let output = json!({
                "status": "resolution_complete",
                "resolved": resolved_count,
                "escalated": escalated_count,
                "path": uncertainties_path.display().to_string(),
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!();
            println!("{} Resolution complete", "Done:".green());
            println!("  Resolved:  {}", resolved_count);
            println!("  Escalated: {}", escalated_count);
        }
    } else if !args.auto && !ctx.robot_mode {
        // Interactive mode hint
        println!("{}", "Options:".bold());
        println!("  Run with --auto to attempt automatic resolution");
        println!("  Use: ms uncertainties resolve <id> for manual resolution");
    }

    Ok(())
}

/// Load uncertainties from JSON file
fn load_uncertainties(
    path: &std::path::Path,
) -> Result<(crate::cass::UncertaintyQueue, Vec<crate::cass::UncertaintyItem>)> {
    use crate::cass::{UncertaintyConfig, UncertaintyItem, UncertaintyQueue};

    let queue = UncertaintyQueue::new(UncertaintyConfig::default());

    if path.exists() {
        let content = fs::read_to_string(path)?;
        let items: Vec<UncertaintyItem> = serde_json::from_str(&content).map_err(|e| {
            MsError::Config(format!("Failed to parse uncertainties file: {}", e))
        })?;
        Ok((queue, items))
    } else {
        Ok((queue, Vec::new()))
    }
}

/// Save uncertainties to JSON file
fn save_uncertainties(
    path: &std::path::Path,
    items: &[crate::cass::UncertaintyItem],
) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(items)?;
    fs::write(path, content)?;
    Ok(())
}

/// Count uncertainties by status
fn count_uncertainties(items: &[crate::cass::UncertaintyItem]) -> crate::cass::UncertaintyCounts {
    use crate::cass::{UncertaintyCounts, UncertaintyStatus};

    let mut counts = UncertaintyCounts::default();

    for item in items {
        match &item.status {
            UncertaintyStatus::Pending => counts.pending += 1,
            UncertaintyStatus::InProgress { .. } => counts.in_progress += 1,
            UncertaintyStatus::Resolved { .. } => counts.resolved += 1,
            UncertaintyStatus::Rejected { .. } => counts.rejected += 1,
            UncertaintyStatus::NeedsHuman { .. } => counts.needs_human += 1,
            UncertaintyStatus::Expired { .. } => counts.expired += 1,
        }
    }

    counts
}

/// Format uncertainty reason for display
fn format_uncertainty_reason(reason: &crate::cass::UncertaintyReason) -> String {
    use crate::cass::UncertaintyReason;

    match reason {
        UncertaintyReason::InsufficientInstances { have, need, .. } => {
            format!("Insufficient instances ({}/{})", have, need)
        }
        UncertaintyReason::HighVariance { variance_score, .. } => {
            format!("High variance ({:.0}%)", variance_score * 100.0)
        }
        UncertaintyReason::CounterExampleFound { contradiction, .. } => {
            format!("Counter-example: {}", contradiction.chars().take(30).collect::<String>())
        }
        UncertaintyReason::AmbiguousScope { possible_scopes } => {
            format!("Ambiguous scope ({} candidates)", possible_scopes.len())
        }
        UncertaintyReason::UnclearPreconditions { candidates } => {
            format!("Unclear preconditions ({} candidates)", candidates.len())
        }
        UncertaintyReason::UnknownBoundaries { dimension, .. } => {
            format!("Unknown boundaries: {}", dimension)
        }
        UncertaintyReason::OvergeneralizationFlagged { critique_summary } => {
            format!("Overgeneralization: {}", critique_summary.chars().take(30).collect::<String>())
        }
        UncertaintyReason::ConflictingPatterns { pattern_ids, .. } => {
            format!("Conflicting patterns ({})", pattern_ids.len())
        }
    }
}

/// Extract draft from wizard state (helper)
fn get_draft_from_wizard(
    wizard: &BrennerWizard,
) -> Option<crate::cass::brenner::BrennerSkillDraft> {
    match wizard.state() {
        crate::cass::brenner::WizardState::Complete { .. } => {
            // Get from last formalization state through checkpoint
            None // Would need to track this differently
        }
        crate::cass::brenner::WizardState::SkillFormalization { draft, .. } => Some(draft.clone()),
        crate::cass::brenner::WizardState::MaterializationTest { draft, .. } => Some(draft.clone()),
        _ => None,
    }
}
