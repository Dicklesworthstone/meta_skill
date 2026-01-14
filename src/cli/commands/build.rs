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
    let query = args.from_cass.clone().ok_or_else(|| {
        MsError::Config("--from-cass is required for --auto builds".into())
    })?;

    if ctx.robot_mode {
        let output = json!({
            "status": "auto_build_started",
            "query": query,
            "sessions": args.sessions,
            "min_confidence": args.min_confidence,
            "cm_available": cm_context.is_some(),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", "Starting automatic build...".bold());
        println!("  Query: {}", query);
        println!("  Sessions: {}", args.sessions);
        println!("  Min confidence: {:.0}%", args.min_confidence * 100.0);
        if let Some(cm_ctx) = cm_context {
            println!("  CM rules: {}", cm_ctx.seed_rules.len());
        }
    }

    // TODO: Implement automatic build pipeline
    // 1. Search CASS for sessions
    // 2. Score and filter by quality
    // 3. Extract patterns
    // 4. Transform specific→general
    // 5. Filter by confidence
    // 6. Synthesize skill

    if !ctx.robot_mode {
        println!("\n{} Auto build not yet fully implemented", "Warning:".yellow());
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
fn run_resume(ctx: &AppContext, _args: &BuildArgs, session_id: &str) -> Result<()> {
    // TODO: Implement checkpoint loading
    if ctx.robot_mode {
        let output = json!({
            "error": true,
            "code": "not_implemented",
            "message": format!("Resume not yet implemented for session: {}", session_id),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!(
            "{} Resume not yet implemented for session: {}",
            "Warning:".yellow(),
            session_id
        );
    }
    Ok(())
}

/// Resolve pending uncertainties
fn run_resolve_uncertainties(ctx: &AppContext, _args: &BuildArgs) -> Result<()> {
    // TODO: Implement uncertainty resolution flow
    if ctx.robot_mode {
        let output = json!({
            "error": true,
            "code": "not_implemented",
            "message": "Uncertainty resolution not yet implemented",
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!(
            "{} Uncertainty resolution not yet implemented",
            "Warning:".yellow()
        );
        println!("  Use: ms uncertainties list");
        println!("  And: ms uncertainties resolve <id>");
    }
    Ok(())
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
