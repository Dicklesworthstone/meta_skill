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

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::app::AppContext;
use crate::beads::{BeadsClient, IssueStatus, UpdateIssueRequest};
use crate::cass::{
    CassClient, QualityScorer,
    brenner::{BrennerConfig, BrennerWizard, WizardOutput, generate_skill_md, run_interactive},
};
use crate::cm::CmClient;
use crate::core::recovery::Checkpoint;
use crate::error::{MsError, Result};
use crate::tui::build_tui::run_build_tui;

// =============================================================================
// BuildSession State Machine
// =============================================================================

/// Phases of the autonomous build process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuildPhase {
    /// Searching CASS for matching sessions.
    SearchSessions,
    /// Filtering sessions by quality score.
    QualityFilter,
    /// Extracting patterns from qualified sessions.
    ExtractPatterns,
    /// Filtering patterns by confidence and taint.
    FilterPatterns,
    /// Synthesizing final skill specification.
    Synthesize,
    /// Build completed successfully.
    Complete,
    /// Build failed with an error.
    Failed,
}

impl BuildPhase {
    /// Get the next phase in the pipeline.
    pub fn next(&self) -> Option<BuildPhase> {
        match self {
            BuildPhase::SearchSessions => Some(BuildPhase::QualityFilter),
            BuildPhase::QualityFilter => Some(BuildPhase::ExtractPatterns),
            BuildPhase::ExtractPatterns => Some(BuildPhase::FilterPatterns),
            BuildPhase::FilterPatterns => Some(BuildPhase::Synthesize),
            BuildPhase::Synthesize => Some(BuildPhase::Complete),
            BuildPhase::Complete | BuildPhase::Failed => None,
        }
    }

    /// Get phase weight for overall progress calculation.
    fn weight(&self) -> f64 {
        match self {
            BuildPhase::SearchSessions => 0.15,
            BuildPhase::QualityFilter => 0.15,
            BuildPhase::ExtractPatterns => 0.30,
            BuildPhase::FilterPatterns => 0.15,
            BuildPhase::Synthesize => 0.25,
            BuildPhase::Complete | BuildPhase::Failed => 0.0,
        }
    }

    /// Get cumulative weight of all phases before this one.
    fn cumulative_weight(&self) -> f64 {
        match self {
            BuildPhase::SearchSessions => 0.0,
            BuildPhase::QualityFilter => 0.15,
            BuildPhase::ExtractPatterns => 0.30,
            BuildPhase::FilterPatterns => 0.60,
            BuildPhase::Synthesize => 0.75,
            BuildPhase::Complete => 1.0,
            BuildPhase::Failed => 0.0,
        }
    }
}

impl std::fmt::Display for BuildPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            BuildPhase::SearchSessions => "search_sessions",
            BuildPhase::QualityFilter => "quality_filter",
            BuildPhase::ExtractPatterns => "extract_patterns",
            BuildPhase::FilterPatterns => "filter_patterns",
            BuildPhase::Synthesize => "synthesize",
            BuildPhase::Complete => "complete",
            BuildPhase::Failed => "failed",
        };
        write!(f, "{}", name)
    }
}

/// Quality gates for build validation.
#[derive(Debug, Clone)]
pub struct QualityGates {
    /// Minimum quality score for sessions (0.0-1.0).
    pub min_session_quality: f32,
    /// Minimum confidence for patterns (0.0-1.0).
    pub min_pattern_confidence: f32,
    /// Minimum number of sessions required.
    pub min_sessions: usize,
    /// Minimum number of patterns required.
    pub min_patterns: usize,
}

impl Default for QualityGates {
    fn default() -> Self {
        Self {
            min_session_quality: 0.6,
            min_pattern_confidence: 0.8,
            min_sessions: 3,
            min_patterns: 5,
        }
    }
}

/// Persistent state for resumable builds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildState {
    /// Session IDs that passed quality filter.
    pub qualified_session_ids: Vec<String>,
    /// Number of patterns extracted so far.
    pub patterns_extracted: usize,
    /// Number of patterns after filtering.
    pub patterns_filtered: usize,
}

impl Default for BuildState {
    fn default() -> Self {
        Self {
            qualified_session_ids: Vec::new(),
            patterns_extracted: 0,
            patterns_filtered: 0,
        }
    }
}

/// State machine for autonomous build execution.
pub struct BuildSession {
    /// Unique session identifier.
    pub session_id: String,
    /// The CASS query for this build (stored for resume).
    pub query: String,
    /// Current build phase.
    pub phase: BuildPhase,
    /// Progress within current phase (0.0-1.0).
    pub phase_progress: f64,
    /// Quality gates for validation.
    pub gates: QualityGates,
    /// Persistent state for resumption.
    pub state: BuildState,
    /// When the session started.
    pub started_at: Instant,
    /// Maximum duration for the build (if set).
    pub max_duration: Option<Duration>,
    /// Checkpoint interval for persistence.
    pub checkpoint_interval: Option<Duration>,
    /// Last checkpoint time.
    pub last_checkpoint: Instant,
    /// Checkpoint for persistence.
    checkpoint: Checkpoint,
}

impl BuildSession {
    /// Create a new build session.
    pub fn new(query: &str, gates: QualityGates) -> Self {
        let session_id = format!("build-{}", chrono::Utc::now().format("%Y%m%d-%H%M%S"));
        let now = Instant::now();
        let checkpoint = Checkpoint::new(&session_id, "build");

        Self {
            session_id,
            query: query.to_string(),
            phase: BuildPhase::SearchSessions,
            phase_progress: 0.0,
            gates,
            state: BuildState::default(),
            started_at: now,
            max_duration: None,
            checkpoint_interval: None,
            last_checkpoint: now,
            checkpoint,
        }
    }

    /// Set maximum duration for the build.
    pub fn with_max_duration(mut self, duration: Duration) -> Self {
        self.max_duration = Some(duration);
        self
    }

    /// Set checkpoint interval.
    pub fn with_checkpoint_interval(mut self, interval: Duration) -> Self {
        self.checkpoint_interval = Some(interval);
        self
    }

    /// Calculate overall progress (0.0-1.0).
    pub fn overall_progress(&self) -> f64 {
        let base = self.phase.cumulative_weight();
        let phase_contribution = self.phase.weight() * self.phase_progress;
        (base + phase_contribution).min(1.0)
    }

    /// Advance to the next phase.
    pub fn advance_phase(&mut self) {
        if let Some(next) = self.phase.next() {
            self.phase = next;
            self.phase_progress = 0.0;
        }
    }

    /// Mark the build as failed.
    pub fn fail(&mut self) {
        self.phase = BuildPhase::Failed;
    }

    /// Check if the build has timed out.
    pub fn is_timed_out(&self) -> bool {
        if let Some(max_duration) = self.max_duration {
            self.started_at.elapsed() >= max_duration
        } else {
            false
        }
    }

    /// Check if a checkpoint should be saved.
    pub fn should_checkpoint(&self) -> bool {
        if let Some(interval) = self.checkpoint_interval {
            self.last_checkpoint.elapsed() >= interval
        } else {
            false
        }
    }

    /// Update the checkpoint with current state.
    fn update_checkpoint(&mut self) {
        self.checkpoint.phase = self.phase.to_string();
        self.checkpoint.progress = self.overall_progress();
        self.checkpoint.updated_at = chrono::Utc::now();

        // Store state - including query for resume
        self.checkpoint
            .state
            .insert("query".to_string(), self.query.clone());
        self.checkpoint.state.insert(
            "qualified_sessions".to_string(),
            self.state.qualified_session_ids.join(","),
        );
        self.checkpoint.state.insert(
            "patterns_extracted".to_string(),
            self.state.patterns_extracted.to_string(),
        );
        self.checkpoint.state.insert(
            "patterns_filtered".to_string(),
            self.state.patterns_filtered.to_string(),
        );
    }

    /// Save checkpoint to disk.
    pub fn save_checkpoint(&mut self, ms_root: &std::path::Path) -> Result<()> {
        self.update_checkpoint();
        self.checkpoint.save(ms_root)?;
        self.last_checkpoint = Instant::now();
        Ok(())
    }

    /// Check if quality gates pass.
    pub fn check_quality_gates(&self) -> std::result::Result<(), String> {
        if self.state.qualified_session_ids.len() < self.gates.min_sessions {
            return Err(format!(
                "Insufficient sessions: {} < {} required",
                self.state.qualified_session_ids.len(),
                self.gates.min_sessions
            ));
        }
        if self.state.patterns_filtered < self.gates.min_patterns {
            return Err(format!(
                "Insufficient patterns: {} < {} required",
                self.state.patterns_filtered, self.gates.min_patterns
            ));
        }
        Ok(())
    }

    /// Get remaining time if duration is set.
    pub fn remaining_time(&self) -> Option<Duration> {
        self.max_duration.map(|max| {
            let elapsed = self.started_at.elapsed();
            if elapsed >= max {
                Duration::ZERO
            } else {
                max - elapsed
            }
        })
    }
}

/// Parse a duration string like "4h", "30m", "2h30m", "1h15m30s".
pub fn parse_duration(s: &str) -> Result<Duration> {
    let s = s.trim().to_lowercase();
    if s.is_empty() {
        return Err(MsError::Config("Empty duration string".into()));
    }

    let mut total_secs: u64 = 0;
    let mut current_num = String::new();

    for c in s.chars() {
        if c.is_ascii_digit() {
            current_num.push(c);
        } else {
            if current_num.is_empty() {
                return Err(MsError::Config(format!(
                    "Missing number before '{}' in duration",
                    c
                )));
            }
            let num: u64 = current_num.parse().map_err(|_| {
                MsError::Config(format!("Invalid number in duration: {}", current_num))
            })?;
            current_num.clear();

            match c {
                'h' => total_secs += num * 3600,
                'm' => total_secs += num * 60,
                's' => total_secs += num,
                _ => {
                    return Err(MsError::Config(format!(
                        "Invalid duration unit '{}'. Use h, m, or s.",
                        c
                    )));
                }
            }
        }
    }

    // Handle trailing number (e.g., "30" defaults to minutes)
    if !current_num.is_empty() {
        let num: u64 = current_num
            .parse()
            .map_err(|_| MsError::Config(format!("Invalid number in duration: {}", current_num)))?;
        // If no unit specified, assume minutes for backwards compatibility
        total_secs += num * 60;
    }

    if total_secs == 0 {
        return Err(MsError::Config("Duration must be greater than zero".into()));
    }

    Ok(Duration::from_secs(total_secs))
}

#[derive(Args, Debug)]
pub struct BuildArgs {
    /// Build from CASS sessions matching this query
    #[arg(long)]
    pub from_cass: Option<String>,

    /// Track build progress against a beads issue
    #[arg(long)]
    pub bead_id: Option<String>,

    /// Close bead automatically on successful build (default: true)
    #[arg(long, default_value = "true")]
    pub close_bead_on_success: bool,

    /// Interactive guided build using Brenner Method
    #[arg(long)]
    pub guided: bool,

    /// Use rich TUI interface for guided mode
    #[arg(long)]
    pub tui: bool,

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

    /// Minimum number of sessions required (quality gate)
    #[arg(long)]
    pub min_sessions: Option<usize>,

    /// Minimum number of patterns required (quality gate)
    #[arg(long)]
    pub min_patterns: Option<usize>,

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

// =============================================================================
// Beads Build Integration
// =============================================================================

/// Build completion data for beads updates.
#[derive(Debug, Clone, Serialize)]
pub struct BuildCompletion {
    pub duration_secs: f64,
    pub success: bool,
    pub tests_passed: Option<u32>,
    pub tests_failed: Option<u32>,
    pub tests_skipped: Option<u32>,
    pub coverage_percent: Option<f64>,
    pub error_summary: Option<String>,
}

impl BuildCompletion {
    pub fn success(duration_secs: f64) -> Self {
        Self {
            duration_secs,
            success: true,
            tests_passed: None,
            tests_failed: None,
            tests_skipped: None,
            coverage_percent: None,
            error_summary: None,
        }
    }

    pub fn failure(duration_secs: f64, error: impl Into<String>) -> Self {
        Self {
            duration_secs,
            success: false,
            tests_passed: None,
            tests_failed: None,
            tests_skipped: None,
            coverage_percent: None,
            error_summary: Some(error.into()),
        }
    }

    /// Format as markdown for bead notes.
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        if self.success {
            md.push_str("## Build Succeeded\n\n");
        } else {
            md.push_str("## Build Failed\n\n");
        }

        md.push_str(&format!("**Duration:** {:.1}s\n", self.duration_secs));

        if let (Some(p), Some(f), Some(s)) =
            (self.tests_passed, self.tests_failed, self.tests_skipped)
        {
            md.push_str(&format!(
                "**Tests:** {} passed, {} failed, {} skipped\n",
                p, f, s
            ));
        }

        if let Some(cov) = self.coverage_percent {
            md.push_str(&format!("**Coverage:** {:.1}%\n", cov));
        }

        if let Some(err) = &self.error_summary {
            md.push_str("\n### Error Summary\n```\n");
            md.push_str(err);
            if !err.ends_with('\n') {
                md.push('\n');
            }
            md.push_str("```\n");
        }

        md
    }
}

/// Tracks build progress in a beads issue.
///
/// When a bead_id is provided, this tracker:
/// - Sets the issue to in_progress at build start
/// - Closes the issue on successful build
/// - Adds failure notes on build failure (keeps in_progress)
pub struct BeadsTracker {
    client: BeadsClient,
    bead_id: String,
    started_at: Instant,
    close_on_success: bool,
}

impl BeadsTracker {
    /// Create a new tracker for the given bead ID.
    ///
    /// Returns None if beads is not available.
    pub fn new(bead_id: String, close_on_success: bool) -> Option<Self> {
        let client = BeadsClient::new();
        if !client.is_available() {
            eprintln!(
                "{} beads (bd) not available, skipping bead tracking",
                "Warning:".yellow()
            );
            return None;
        }
        Some(Self {
            client,
            bead_id,
            started_at: Instant::now(),
            close_on_success,
        })
    }

    /// Mark the bead as in_progress at build start.
    pub fn on_start(&self) -> Result<()> {
        match self
            .client
            .update_status(&self.bead_id, IssueStatus::InProgress)
        {
            Ok(_) => {
                eprintln!("{} {} set to in_progress", "Bead:".cyan(), self.bead_id);
                Ok(())
            }
            Err(e) => {
                // Non-blocking: log warning but don't fail the build
                eprintln!(
                    "{} failed to update bead {}: {}",
                    "Warning:".yellow(),
                    self.bead_id,
                    e
                );
                Ok(())
            }
        }
    }

    /// Close the bead on successful build.
    pub fn on_success(&self, skill_name: &str) -> Result<()> {
        let completion = BuildCompletion::success(self.duration_secs());
        if let Err(e) = self.append_completion_note(&completion) {
            eprintln!(
                "{} failed to append completion note for {}: {}",
                "Warning:".yellow(),
                self.bead_id,
                e
            );
        }

        if self.close_on_success {
            match self.client.close(&self.bead_id, None) {
                Ok(_) => {
                    eprintln!(
                        "{} {} closed (build successful: {})",
                        "Bead:".green(),
                        self.bead_id,
                        skill_name
                    );
                    Ok(())
                }
                Err(e) => {
                    eprintln!(
                        "{} failed to close bead {}: {}",
                        "Warning:".yellow(),
                        self.bead_id,
                        e
                    );
                    Ok(())
                }
            }
        } else {
            eprintln!(
                "{} {} build completed (close disabled: {})",
                "Bead:".green(),
                self.bead_id,
                skill_name
            );
            Ok(())
        }
    }

    /// Add failure note on build failure (keeps in_progress).
    pub fn on_failure(&self, error: &str) -> Result<()> {
        let completion = BuildCompletion::failure(self.duration_secs(), error);
        match self.append_completion_note(&completion) {
            Ok(_) => {
                eprintln!(
                    "{} {} updated with failure note",
                    "Bead:".yellow(),
                    self.bead_id
                );
                Ok(())
            }
            Err(e) => {
                eprintln!(
                    "{} failed to update bead {}: {}",
                    "Warning:".yellow(),
                    self.bead_id,
                    e
                );
                Ok(())
            }
        }
    }

    fn duration_secs(&self) -> f64 {
        self.started_at.elapsed().as_secs_f64()
    }

    fn append_completion_note(&self, completion: &BuildCompletion) -> Result<()> {
        let note = completion.to_markdown();
        let combined = match self.client.show(&self.bead_id) {
            Ok(issue) => {
                if let Some(existing) = issue.notes {
                    if existing.trim().is_empty() {
                        note
                    } else {
                        format!("{existing}\n\n{note}")
                    }
                } else {
                    note
                }
            }
            Err(_) => note,
        };

        let req = UpdateIssueRequest::new().with_notes(combined);
        self.client.update(&self.bead_id, &req)?;
        Ok(())
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
                    eprintln!(
                        "{} CM not available, proceeding without CM context",
                        "Warning:".yellow()
                    );
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

    // Initialize beads tracker if bead_id is provided
    let bead_tracker = args
        .bead_id
        .as_ref()
        .and_then(|id| BeadsTracker::new(id.clone(), args.close_bead_on_success));

    // Mark bead as in_progress at build start
    if let Some(ref tracker) = bead_tracker {
        tracker.on_start()?;
    }

    // Handle resume
    if let Some(ref session_id) = args.resume {
        return run_resume(ctx, args, session_id, bead_tracker);
    }

    // Handle resolve uncertainties
    if args.resolve_uncertainties {
        return run_resolve_uncertainties(ctx, args);
    }

    // Guided mode uses Brenner wizard
    if args.guided {
        return run_guided(ctx, args, cm_context.as_ref(), bead_tracker);
    }

    // Auto mode
    if args.auto {
        return run_auto(ctx, args, cm_context.as_ref(), bead_tracker, None);
    }

    // Default: interactive but not guided
    run_interactive_build(ctx, args, cm_context.as_ref(), bead_tracker)
}

/// Run guided build using Brenner Method wizard
fn run_guided(
    ctx: &AppContext,
    args: &BuildArgs,
    cm_context: Option<&CmBuildContext>,
    tracker: Option<BeadsTracker>,
) -> Result<()> {
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

    let mut wizard = BrennerWizard::new(&query, config.clone());

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

    // Run interactive wizard - TUI or text mode
    let result = if args.tui {
        run_build_tui(&query, config, &client, &quality_scorer)?
    } else {
        run_interactive(&mut wizard, &client, &quality_scorer)?
    };

    match result {
        WizardOutput::Success {
            skill_path,
            manifest_path,
            calibration_path,
            draft,
            manifest_json,
        } => {
            // Write outputs using draft from WizardOutput
            let skill_md = generate_skill_md(&draft);
            fs::write(&skill_path, &skill_md)?;

            // Use the pre-generated manifest_json from WizardOutput
            fs::write(&manifest_path, &manifest_json)?;

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

            if let Some(t) = tracker {
                t.on_success(&draft.name)?;
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
            if let Some(t) = tracker {
                t.on_failure(&format!("Cancelled: {}", reason))?;
            }
        }
    }

    Ok(())
}

/// Run automatic build (no user interaction)
fn run_auto(
    ctx: &AppContext,
    args: &BuildArgs,
    cm_context: Option<&CmBuildContext>,
    tracker: Option<BeadsTracker>,
    query_override: Option<&str>,
) -> Result<()> {
    use crate::cass::QualityConfig;
    use crate::cass::mining::{ExtractedPattern, extract_from_session};

    // Use query_override (from checkpoint resume) or fall back to args.from_cass
    let query = query_override
        .map(|s| s.to_string())
        .or_else(|| args.from_cass.clone())
        .ok_or_else(|| MsError::Config("--from-cass is required for --auto builds".into()))?;

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

    // Initialize BuildSession with quality gates
    let gates = QualityGates {
        min_session_quality: args.min_session_quality,
        min_pattern_confidence: args.min_confidence,
        min_sessions: args.min_sessions.unwrap_or(3),
        min_patterns: args.min_patterns.unwrap_or(5),
    };

    let mut session = BuildSession::new(&query, gates);

    // Configure duration limit if specified
    if let Some(ref duration_str) = args.duration {
        let duration = parse_duration(duration_str)?;
        session = session.with_max_duration(duration);
    }

    // Configure checkpoint interval if specified
    if let Some(ref interval_str) = args.checkpoint_interval {
        let interval = parse_duration(interval_str)?;
        session = session.with_checkpoint_interval(interval);
    }

    if ctx.robot_mode {
        let output = json!({
            "status": "auto_build_started",
            "session_id": session.session_id,
            "query": query,
            "sessions": args.sessions,
            "min_confidence": args.min_confidence,
            "min_sessions": session.gates.min_sessions,
            "min_patterns": session.gates.min_patterns,
            "duration": args.duration,
            "output_dir": output_dir.display().to_string(),
            "cm_available": cm_context.is_some(),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", "Starting automatic build...".bold());
        println!("  Session: {}", session.session_id);
        println!("  Query: {}", query);
        println!("  Sessions: {}", args.sessions);
        println!("  Min confidence: {:.0}%", args.min_confidence * 100.0);
        println!("  Min sessions: {}", session.gates.min_sessions);
        println!("  Min patterns: {}", session.gates.min_patterns);
        if let Some(ref d) = args.duration {
            println!("  Duration limit: {}", d);
        }
        println!("  Output: {}", output_dir.display());
        if let Some(cm_ctx) = cm_context {
            println!("  CM rules: {}", cm_ctx.seed_rules.len());
        }
    }

    // Create CASS client and quality scorer
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

    // =========================================================================
    // Phase 1: Search CASS for sessions
    // =========================================================================
    if !ctx.robot_mode {
        println!("\n{} Searching CASS...", "Phase 1:".cyan());
    }

    // Check for timeout before starting phase
    if session.is_timed_out() {
        if let Some(t) = &tracker {
            t.on_failure("Build timed out during session search")?;
        }
        return output_timeout(ctx, &mut session, &output_dir);
    }

    let search_limit = args.sessions * 3;
    let session_matches = cass_client.search(&query, search_limit)?;

    session.phase_progress = 1.0;
    session.advance_phase(); // -> QualityFilter

    if session_matches.is_empty() {
        if let Some(t) = &tracker {
            t.on_failure("No matching sessions found")?;
        }
        return output_no_sessions(ctx, &session, &query);
    }

    if !ctx.robot_mode {
        println!("  Found {} potential sessions", session_matches.len());
    }

    // Save checkpoint if interval elapsed
    if session.should_checkpoint() {
        session.save_checkpoint(&ctx.ms_root)?;
        if !ctx.robot_mode {
            println!("  {} Checkpoint saved", "📌".cyan());
        }
    }

    // =========================================================================
    // Phase 2: Quality filtering
    // =========================================================================
    if !ctx.robot_mode {
        println!("\n{} Quality filtering...", "Phase 2:".cyan());
    }

    if session.is_timed_out() {
        if let Some(t) = &tracker {
            t.on_failure("Build timed out during quality filtering")?;
        }
        return output_timeout(ctx, &mut session, &output_dir);
    }

    let mut quality_sessions = Vec::new();
    let mut skipped_sessions = Vec::new();
    let total_to_process = session_matches.len().min(search_limit);

    for (i, session_match) in session_matches.into_iter().take(search_limit).enumerate() {
        // Update phase progress
        session.phase_progress = (i + 1) as f64 / total_to_process as f64;

        match cass_client.get_session(&session_match.session_id) {
            Ok(cass_session) => {
                let quality = quality_scorer.score(&cass_session);
                if quality.passes_threshold(&quality_config) {
                    quality_sessions.push((cass_session, quality));
                    session
                        .state
                        .qualified_session_ids
                        .push(session_match.session_id.clone());
                    if quality_sessions.len() >= args.sessions {
                        break;
                    }
                } else {
                    skipped_sessions.push((session_match.session_id, quality.score));
                }
            }
            Err(e) => {
                if !ctx.robot_mode {
                    eprintln!(
                        "  Warning: Failed to fetch session {}: {}",
                        session_match.session_id, e
                    );
                }
            }
        }

        // Check timeout during processing
        if session.is_timed_out() {
            if let Some(t) = &tracker {
                t.on_failure("Build timed out during quality filtering loop")?;
            }
            return output_timeout(ctx, &mut session, &output_dir);
        }
    }

    session.phase_progress = 1.0;
    session.advance_phase(); // -> ExtractPatterns

    if quality_sessions.is_empty() {
        if let Some(t) = &tracker {
            t.on_failure("No sessions passed quality threshold")?;
        }
        return output_no_quality(
            ctx,
            &session,
            &query,
            &skipped_sessions,
            args.min_session_quality,
        );
    }

    if !ctx.robot_mode {
        println!(
            "  {} sessions passed quality threshold (min: {:.0}%)",
            quality_sessions.len(),
            args.min_session_quality * 100.0
        );
        for (s, q) in &quality_sessions {
            println!("    - {} ({:.0}%)", s.id, q.score * 100.0);
        }
    }

    // Save checkpoint if interval elapsed
    if session.should_checkpoint() {
        session.save_checkpoint(&ctx.ms_root)?;
        if !ctx.robot_mode {
            println!("  {} Checkpoint saved", "📌".cyan());
        }
    }

    // =========================================================================
    // Phase 3: Extract patterns
    // =========================================================================
    if !ctx.robot_mode {
        println!("\n{} Extracting patterns...", "Phase 3:".cyan());
    }

    if session.is_timed_out() {
        if let Some(t) = &tracker {
            t.on_failure("Build timed out during pattern extraction")?;
        }
        return output_timeout(ctx, &mut session, &output_dir);
    }

    let mut all_patterns: Vec<ExtractedPattern> = Vec::new();
    let total_sessions = quality_sessions.len();

    for (i, (cass_session, _quality)) in quality_sessions.iter().enumerate() {
        session.phase_progress = (i + 1) as f64 / total_sessions as f64;

        match extract_from_session(cass_session) {
            Ok(patterns) => {
                if !ctx.robot_mode && !patterns.is_empty() {
                    println!("  {} patterns from {}", patterns.len(), cass_session.id);
                }
                session.state.patterns_extracted += patterns.len();
                all_patterns.extend(patterns);
            }
            Err(e) => {
                if !ctx.robot_mode {
                    eprintln!(
                        "  Warning: Failed to extract from {}: {}",
                        cass_session.id, e
                    );
                }
            }
        }

        if session.is_timed_out() {
            if let Some(t) = &tracker {
                t.on_failure("Build timed out during pattern extraction loop")?;
            }
            return output_timeout(ctx, &mut session, &output_dir);
        }
    }

    session.phase_progress = 1.0;
    session.advance_phase(); // -> FilterPatterns

    if all_patterns.is_empty() {
        if let Some(t) = &tracker {
            t.on_failure("No patterns extracted from sessions")?;
        }
        return output_no_patterns(ctx, &session, &query, quality_sessions.len());
    }

    if !ctx.robot_mode {
        println!("  Total: {} patterns extracted", all_patterns.len());
    }

    // =========================================================================
    // Phase 4: Filter patterns
    // =========================================================================
    if !ctx.robot_mode {
        println!("\n{} Filtering by confidence...", "Phase 4:".cyan());
    }

    if session.is_timed_out() {
        if let Some(t) = &tracker {
            t.on_failure("Build timed out during pattern filtering")?;
        }
        return output_timeout(ctx, &mut session, &output_dir);
    }

    let high_confidence_patterns: Vec<_> = all_patterns
        .into_iter()
        .filter(|p| p.confidence >= args.min_confidence)
        .collect();

    session.phase_progress = 0.5;

    if !ctx.robot_mode {
        println!(
            "  {} patterns above confidence threshold ({:.0}%)",
            high_confidence_patterns.len(),
            args.min_confidence * 100.0
        );
    }

    // Filter out tainted patterns (unless --no-injection-filter)
    let pre_taint_count = high_confidence_patterns.len();
    let filtered_patterns: Vec<_> = if args.no_injection_filter {
        high_confidence_patterns
    } else {
        high_confidence_patterns
            .into_iter()
            .filter(|p| p.taint_label.is_none())
            .collect()
    };

    session.state.patterns_filtered = filtered_patterns.len();
    session.phase_progress = 1.0;
    session.advance_phase(); // -> Synthesize

    if !ctx.robot_mode && filtered_patterns.len() < pre_taint_count {
        println!(
            "  {} patterns after taint filtering",
            filtered_patterns.len()
        );
    }

    // Check quality gates before synthesis
    if let Err(gate_error) = session.check_quality_gates() {
        if let Some(t) = &tracker {
            t.on_failure(&format!("Quality gate failed: {}", gate_error))?;
        }
        return output_gate_fail(ctx, &session, &gate_error);
    }

    // =========================================================================
    // Phase 5: Synthesize (write outputs)
    // =========================================================================
    if !ctx.robot_mode {
        println!("\n{} Writing outputs...", "Phase 5:".cyan());
    }

    if session.is_timed_out() {
        if let Some(t) = &tracker {
            t.on_failure("Build timed out during synthesis")?;
        }
        return output_timeout(ctx, &mut session, &output_dir);
    }

    // Write patterns JSON
    let patterns_path = output_dir.join("patterns.json");
    let patterns_json = serde_json::to_string_pretty(&filtered_patterns)?;
    fs::write(&patterns_path, &patterns_json)?;

    session.phase_progress = 0.5;

    if !ctx.robot_mode {
        println!("  Patterns: {}", patterns_path.display());
    }

    // Write build manifest
    let manifest = json!({
        "version": "1.0.0",
        "session_id": session.session_id,
        "query": query,
        "build_type": "auto",
        "sessions_used": quality_sessions.iter().map(|(s, q)| json!({
            "id": s.id,
            "quality_score": q.score,
        })).collect::<Vec<_>>(),
        "patterns_extracted": filtered_patterns.len(),
        "quality_gates": {
            "min_confidence": args.min_confidence,
            "min_session_quality": args.min_session_quality,
            "min_sessions": session.gates.min_sessions,
            "min_patterns": session.gates.min_patterns,
        },
        "cm_context_used": cm_context.is_some(),
        "filters": {
            "redaction_enabled": !args.no_redact,
            "injection_filter_enabled": !args.no_injection_filter,
        },
        "created_at": chrono::Utc::now().to_rfc3339(),
    });
    let manifest_path = output_dir.join("build-manifest.json");
    fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;

    session.phase_progress = 1.0;
    session.advance_phase(); // -> Complete

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
            "session_id": session.session_id,
            "query": query,
            "sessions_used": quality_sessions.len(),
            "patterns_extracted": filtered_patterns.len(),
            "progress": session.overall_progress(),
            "elapsed_ms": session.started_at.elapsed().as_millis(),
            "output_dir": output_dir.display().to_string(),
            "patterns_path": patterns_path.display().to_string(),
            "manifest_path": manifest_path.display().to_string(),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("\n{} Auto build complete!", "Success:".green());
        println!("  Session: {}", session.session_id);
        println!("  Sessions processed: {}", quality_sessions.len());
        println!("  Patterns extracted: {}", filtered_patterns.len());
        println!("  Output directory: {}", output_dir.display());
    }

    if let Some(t) = tracker {
        t.on_success(&format!("Auto build: {}", query))?;
    }

    Ok(())
}

/// Output helper for timeout condition.
fn output_timeout(
    ctx: &AppContext,
    session: &mut BuildSession,
    _output_dir: &std::path::Path,
) -> Result<()> {
    // Save final checkpoint before exiting
    session.save_checkpoint(&ctx.ms_root)?;

    if ctx.robot_mode {
        let output = json!({
            "status": "timeout",
            "session_id": session.session_id,
            "phase": session.phase.to_string(),
            "progress": session.overall_progress(),
            "elapsed_ms": session.started_at.elapsed().as_millis(),
            "checkpoint_saved": true,
            "resume_command": format!("ms build --resume {}", session.session_id),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!(
            "\n{} Build timed out at phase: {}",
            "Timeout:".yellow(),
            session.phase
        );
        println!("  Progress: {:.0}%", session.overall_progress() * 100.0);
        println!("  Checkpoint saved. Resume with:");
        println!("    ms build --resume {}", session.session_id);
    }
    Ok(())
}

/// Output helper for no sessions found.
fn output_no_sessions(ctx: &AppContext, session: &BuildSession, query: &str) -> Result<()> {
    if ctx.robot_mode {
        let output = json!({
            "status": "no_sessions",
            "session_id": session.session_id,
            "query": query,
            "message": "No sessions found matching query"
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!(
            "{} No sessions found matching query: {}",
            "Error:".red(),
            query
        );
    }
    Ok(())
}

/// Output helper for no quality sessions.
fn output_no_quality(
    ctx: &AppContext,
    session: &BuildSession,
    query: &str,
    skipped: &[(String, f32)],
    min_quality: f32,
) -> Result<()> {
    if ctx.robot_mode {
        let output = json!({
            "status": "no_quality_sessions",
            "session_id": session.session_id,
            "query": query,
            "skipped": skipped.len(),
            "min_quality": min_quality,
            "message": "No sessions passed quality threshold"
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!(
            "{} No sessions passed quality threshold (min: {:.0}%)",
            "Error:".red(),
            min_quality * 100.0
        );
        if !skipped.is_empty() {
            println!("  {} sessions were below threshold:", skipped.len());
            for (id, score) in skipped.iter().take(5) {
                println!("    - {} ({:.0}%)", id, score * 100.0);
            }
        }
    }
    Ok(())
}

/// Output helper for no patterns extracted.
fn output_no_patterns(
    ctx: &AppContext,
    session: &BuildSession,
    query: &str,
    sessions_processed: usize,
) -> Result<()> {
    if ctx.robot_mode {
        let output = json!({
            "status": "no_patterns",
            "session_id": session.session_id,
            "query": query,
            "sessions_processed": sessions_processed,
            "message": "No patterns extracted from sessions"
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{} No patterns extracted from sessions", "Error:".red());
    }
    Ok(())
}

/// Output helper for quality gate failure.
fn output_gate_fail(ctx: &AppContext, session: &BuildSession, error: &str) -> Result<()> {
    if ctx.robot_mode {
        let output = json!({
            "status": "quality_gate_failed",
            "session_id": session.session_id,
            "error": error,
            "gates": {
                "min_sessions": session.gates.min_sessions,
                "min_patterns": session.gates.min_patterns,
                "actual_sessions": session.state.qualified_session_ids.len(),
                "actual_patterns": session.state.patterns_filtered,
            },
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{} Quality gate failed: {}", "Error:".red(), error);
        println!(
            "  Required: {} sessions, {} patterns",
            session.gates.min_sessions, session.gates.min_patterns
        );
        println!(
            "  Actual: {} sessions, {} patterns",
            session.state.qualified_session_ids.len(),
            session.state.patterns_filtered
        );
    }
    Ok(())
}

/// Run interactive build (not guided)
fn run_interactive_build(
    ctx: &AppContext,
    args: &BuildArgs,
    cm_context: Option<&CmBuildContext>,
    tracker: Option<BeadsTracker>,
) -> Result<()> {
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
    run_guided(ctx, args, cm_context, tracker)
}

/// Resume a previous build session
fn run_resume(
    ctx: &AppContext,
    args: &BuildArgs,
    session_id: &str,
    tracker: Option<BeadsTracker>,
) -> Result<()> {
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
                println!(
                    "\n{} Checkpoint indicates Brenner wizard session",
                    "Info:".cyan()
                );
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

                // Resume by restarting with same parameters, using query from checkpoint
                // (full incremental resume would require more state)
                return run_auto(ctx, args, cm_context.as_ref(), tracker, Some(&query));
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
    // In robot mode with --auto, we skip the initial status output since
    // we'll output a comprehensive result after resolution completes.
    if ctx.robot_mode && !args.auto {
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
    } else if !ctx.robot_mode {
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
        println!("  Resolved:    {} {}", counts.resolved, "✓".green());
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
        let mut rejected_count = 0;
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
                        let relevance_scores: Vec<_> = matches.iter().map(|m| m.score).collect();

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
                                query
                                    .results
                                    .as_ref()
                                    .map(|r| r.sessions_found)
                                    .unwrap_or(0)
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
                    rejected_count += 1;
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
            // Recount after updates
            let final_counts = count_uncertainties(&updated_items);
            let output = json!({
                "status": "resolution_complete",
                "resolved": resolved_count,
                "escalated": escalated_count,
                "rejected": rejected_count,
                "counts_before": {
                    "pending": counts.pending,
                    "in_progress": counts.in_progress,
                    "resolved": counts.resolved,
                    "rejected": counts.rejected,
                    "needs_human": counts.needs_human,
                    "expired": counts.expired,
                    "total": counts.total(),
                },
                "counts_after": {
                    "pending": final_counts.pending,
                    "in_progress": final_counts.in_progress,
                    "resolved": final_counts.resolved,
                    "rejected": final_counts.rejected,
                    "needs_human": final_counts.needs_human,
                    "expired": final_counts.expired,
                    "total": final_counts.total(),
                },
                "path": uncertainties_path.display().to_string(),
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!();
            println!("{} Resolution complete", "Done:".green());
            println!("  Resolved:  {}", resolved_count);
            println!("  Escalated: {}", escalated_count);
            println!("  Rejected:  {}", rejected_count);
        }
    } else if args.auto && pending_items.is_empty() {
        // Auto-resolution requested but no pending items (e.g., only in_progress)
        if ctx.robot_mode {
            let output = json!({
                "status": "no_pending_for_auto",
                "message": "Auto-resolution requested but no pending items to process",
                "counts": {
                    "pending": counts.pending,
                    "in_progress": counts.in_progress,
                    "resolved": counts.resolved,
                    "rejected": counts.rejected,
                    "needs_human": counts.needs_human,
                    "expired": counts.expired,
                    "total": counts.total(),
                },
                "path": uncertainties_path.display().to_string(),
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!("{} No pending items to auto-resolve", "Info:".cyan());
            println!("  {} items are in-progress", counts.in_progress);
        }
    } else if !ctx.robot_mode {
        // Interactive mode hint (non-auto, non-robot)
        println!("{}", "Options:".bold());
        println!("  Run with --auto to attempt automatic resolution");
        println!("  Use: ms uncertainties resolve <id> for manual resolution");
    }

    Ok(())
}

/// Load uncertainties from JSON file
fn load_uncertainties(
    path: &std::path::Path,
) -> Result<(
    crate::cass::UncertaintyQueue,
    Vec<crate::cass::UncertaintyItem>,
)> {
    use crate::cass::{UncertaintyConfig, UncertaintyItem, UncertaintyQueue};

    let queue = UncertaintyQueue::new(UncertaintyConfig::default());

    if path.exists() {
        let content = fs::read_to_string(path)?;
        let items: Vec<UncertaintyItem> = serde_json::from_str(&content)
            .map_err(|e| MsError::Config(format!("Failed to parse uncertainties file: {}", e)))?;
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
            format!(
                "Counter-example: {}",
                contradiction.chars().take(30).collect::<String>()
            )
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
            format!(
                "Overgeneralization: {}",
                critique_summary.chars().take(30).collect::<String>()
            )
        }
        UncertaintyReason::ConflictingPatterns { pattern_ids, .. } => {
            format!("Conflicting patterns ({})", pattern_ids.len())
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // BuildPhase Tests
    // =========================================================================

    #[test]
    fn test_build_phase_transitions() {
        // Test the phase transition chain
        assert_eq!(
            BuildPhase::SearchSessions.next(),
            Some(BuildPhase::QualityFilter)
        );
        assert_eq!(
            BuildPhase::QualityFilter.next(),
            Some(BuildPhase::ExtractPatterns)
        );
        assert_eq!(
            BuildPhase::ExtractPatterns.next(),
            Some(BuildPhase::FilterPatterns)
        );
        assert_eq!(
            BuildPhase::FilterPatterns.next(),
            Some(BuildPhase::Synthesize)
        );
        assert_eq!(BuildPhase::Synthesize.next(), Some(BuildPhase::Complete));

        // Terminal states have no next phase
        assert_eq!(BuildPhase::Complete.next(), None);
        assert_eq!(BuildPhase::Failed.next(), None);
    }

    #[test]
    fn test_build_phase_weights_sum_to_one() {
        let phases = [
            BuildPhase::SearchSessions,
            BuildPhase::QualityFilter,
            BuildPhase::ExtractPatterns,
            BuildPhase::FilterPatterns,
            BuildPhase::Synthesize,
        ];

        let total_weight: f64 = phases.iter().map(|p| p.weight()).sum();
        assert!(
            (total_weight - 1.0).abs() < 0.001,
            "Phase weights should sum to 1.0"
        );
    }

    #[test]
    fn test_build_phase_cumulative_weights() {
        // Verify cumulative weights are correct
        assert_eq!(BuildPhase::SearchSessions.cumulative_weight(), 0.0);
        assert!((BuildPhase::QualityFilter.cumulative_weight() - 0.15).abs() < 0.001);
        assert!((BuildPhase::ExtractPatterns.cumulative_weight() - 0.30).abs() < 0.001);
        assert!((BuildPhase::FilterPatterns.cumulative_weight() - 0.60).abs() < 0.001);
        assert!((BuildPhase::Synthesize.cumulative_weight() - 0.75).abs() < 0.001);
        assert!((BuildPhase::Complete.cumulative_weight() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_build_phase_display() {
        assert_eq!(format!("{}", BuildPhase::SearchSessions), "search_sessions");
        assert_eq!(format!("{}", BuildPhase::QualityFilter), "quality_filter");
        assert_eq!(
            format!("{}", BuildPhase::ExtractPatterns),
            "extract_patterns"
        );
        assert_eq!(format!("{}", BuildPhase::FilterPatterns), "filter_patterns");
        assert_eq!(format!("{}", BuildPhase::Synthesize), "synthesize");
        assert_eq!(format!("{}", BuildPhase::Complete), "complete");
        assert_eq!(format!("{}", BuildPhase::Failed), "failed");
    }

    // =========================================================================
    // QualityGates Tests
    // =========================================================================

    #[test]
    fn test_quality_gates_defaults() {
        let gates = QualityGates::default();
        assert!((gates.min_session_quality - 0.6).abs() < 0.001);
        assert!((gates.min_pattern_confidence - 0.8).abs() < 0.001);
        assert_eq!(gates.min_sessions, 3);
        assert_eq!(gates.min_patterns, 5);
    }

    // =========================================================================
    // BuildSession Tests
    // =========================================================================

    #[test]
    fn test_build_session_new() {
        let session = BuildSession::new("test query", QualityGates::default());

        assert!(session.session_id.starts_with("build-"));
        assert_eq!(session.phase, BuildPhase::SearchSessions);
        assert_eq!(session.phase_progress, 0.0);
        assert!(session.state.qualified_session_ids.is_empty());
        assert_eq!(session.state.patterns_extracted, 0);
        assert_eq!(session.state.patterns_filtered, 0);
    }

    #[test]
    fn test_build_session_overall_progress() {
        let mut session = BuildSession::new("test", QualityGates::default());

        // Initial progress is 0
        assert_eq!(session.overall_progress(), 0.0);

        // At 50% of first phase (SearchSessions, weight 0.15)
        session.phase_progress = 0.5;
        let expected = 0.0 + (0.15 * 0.5); // 0.075
        assert!((session.overall_progress() - expected).abs() < 0.001);

        // Advance to next phase
        session.advance_phase();
        assert_eq!(session.phase, BuildPhase::QualityFilter);
        assert_eq!(session.phase_progress, 0.0);

        // At start of QualityFilter, cumulative is 0.15
        assert!((session.overall_progress() - 0.15).abs() < 0.001);
    }

    #[test]
    fn test_build_session_advance_phase() {
        let mut session = BuildSession::new("test", QualityGates::default());
        session.phase_progress = 0.8; // Some progress in current phase

        session.advance_phase();

        assert_eq!(session.phase, BuildPhase::QualityFilter);
        assert_eq!(session.phase_progress, 0.0); // Reset on advance
    }

    #[test]
    fn test_build_session_fail() {
        let mut session = BuildSession::new("test", QualityGates::default());
        session.phase = BuildPhase::ExtractPatterns;

        session.fail();

        assert_eq!(session.phase, BuildPhase::Failed);
    }

    #[test]
    fn test_build_session_timeout() {
        let session = BuildSession::new("test", QualityGates::default());

        // No duration set, never times out
        assert!(!session.is_timed_out());

        // With duration set
        let session_with_timeout = session.with_max_duration(Duration::from_millis(1));
        // Sleep briefly to ensure timeout
        std::thread::sleep(Duration::from_millis(5));
        assert!(session_with_timeout.is_timed_out());
    }

    #[test]
    fn test_build_session_quality_gates() {
        let mut session = BuildSession::new("test", QualityGates::default());

        // Initially fails quality gates (not enough sessions or patterns)
        assert!(session.check_quality_gates().is_err());

        // Add enough sessions
        session.state.qualified_session_ids =
            vec!["s1".to_string(), "s2".to_string(), "s3".to_string()];
        // Still fails (not enough patterns)
        assert!(session.check_quality_gates().is_err());

        // Add enough patterns
        session.state.patterns_filtered = 5;
        assert!(session.check_quality_gates().is_ok());
    }

    #[test]
    fn test_build_session_remaining_time() {
        let session = BuildSession::new("test", QualityGates::default());

        // No duration set
        assert!(session.remaining_time().is_none());

        // With duration
        let session_with_duration = session.with_max_duration(Duration::from_secs(60));
        let remaining = session_with_duration.remaining_time().unwrap();
        assert!(remaining <= Duration::from_secs(60));
    }

    // =========================================================================
    // parse_duration Tests
    // =========================================================================

    #[test]
    fn test_parse_duration_hours() {
        let dur = parse_duration("4h").unwrap();
        assert_eq!(dur, Duration::from_secs(4 * 3600));
    }

    #[test]
    fn test_parse_duration_minutes() {
        let dur = parse_duration("30m").unwrap();
        assert_eq!(dur, Duration::from_secs(30 * 60));
    }

    #[test]
    fn test_parse_duration_seconds() {
        let dur = parse_duration("45s").unwrap();
        assert_eq!(dur, Duration::from_secs(45));
    }

    #[test]
    fn test_parse_duration_combined() {
        let dur = parse_duration("2h30m").unwrap();
        assert_eq!(dur, Duration::from_secs(2 * 3600 + 30 * 60));

        let dur = parse_duration("1h15m30s").unwrap();
        assert_eq!(dur, Duration::from_secs(3600 + 15 * 60 + 30));
    }

    #[test]
    fn test_parse_duration_bare_number_defaults_to_minutes() {
        let dur = parse_duration("30").unwrap();
        assert_eq!(dur, Duration::from_secs(30 * 60));
    }

    #[test]
    fn test_parse_duration_empty_fails() {
        assert!(parse_duration("").is_err());
    }

    #[test]
    fn test_parse_duration_zero_fails() {
        assert!(parse_duration("0h").is_err());
        assert!(parse_duration("0m").is_err());
    }

    #[test]
    fn test_parse_duration_invalid_unit_fails() {
        assert!(parse_duration("5d").is_err()); // Days not supported
        assert!(parse_duration("5x").is_err());
    }

    #[test]
    fn test_parse_duration_missing_number_fails() {
        // Unit without a preceding number
        let result = parse_duration("h");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Missing number"));

        let result = parse_duration("m");
        assert!(result.is_err());

        // Unit at start of combined duration
        let result = parse_duration("h30m");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_duration_case_insensitive() {
        let dur_lower = parse_duration("2h30m").unwrap();
        let dur_upper = parse_duration("2H30M").unwrap();
        assert_eq!(dur_lower, dur_upper);
    }

    // =========================================================================
    // Checkpoint Integration Tests (Crash + Resume Simulation)
    // =========================================================================

    #[test]
    fn test_build_session_checkpoint_save_and_load() {
        use crate::core::recovery::Checkpoint;

        let temp_dir = tempfile::tempdir().unwrap();
        let ms_root = temp_dir.path();

        // Create a build session and advance through phases
        let mut session = BuildSession::new("test-checkpoint", QualityGates::default());
        session.state.qualified_session_ids = vec!["sess-1".to_string(), "sess-2".to_string()];
        session.state.patterns_extracted = 42;
        session.state.patterns_filtered = 35;
        session.advance_phase(); // -> QualityFilter
        session.phase_progress = 0.75;

        // Save checkpoint
        session.save_checkpoint(ms_root).unwrap();

        // Verify checkpoint exists
        let checkpoint_path = ms_root
            .join("checkpoints")
            .join(format!("{}.json", session.session_id));
        assert!(
            checkpoint_path.exists(),
            "Checkpoint file should be created"
        );

        // Load checkpoint
        let loaded = Checkpoint::load(ms_root, &session.session_id)
            .unwrap()
            .expect("Checkpoint should exist");

        // Verify loaded checkpoint content
        assert_eq!(loaded.operation_type, "build");
        assert_eq!(loaded.phase, "quality_filter");
        assert!(loaded.progress > 0.0);

        // Verify state data including query (critical for resume)
        assert_eq!(loaded.get_state("query"), Some("test-checkpoint"));
        assert_eq!(
            loaded.get_state("qualified_sessions"),
            Some("sess-1,sess-2")
        );
        assert_eq!(loaded.get_state("patterns_extracted"), Some("42"));
        assert_eq!(loaded.get_state("patterns_filtered"), Some("35"));
    }

    #[test]
    fn test_build_session_simulate_crash_and_resume() {
        use crate::core::recovery::Checkpoint;

        let temp_dir = tempfile::tempdir().unwrap();
        let ms_root = temp_dir.path();

        // Simulate a build session that crashes mid-way
        let session_id = {
            let mut session = BuildSession::new("crash-test", QualityGates::default());
            session.state.qualified_session_ids = vec![
                "session-a".to_string(),
                "session-b".to_string(),
                "session-c".to_string(),
            ];
            session.state.patterns_extracted = 100;
            session.advance_phase(); // SearchSessions -> QualityFilter
            session.advance_phase(); // QualityFilter -> ExtractPatterns
            session.phase_progress = 0.5;

            // Save checkpoint before "crash"
            session.save_checkpoint(ms_root).unwrap();

            session.session_id.clone()
        };
        // Session is dropped here (simulating crash)

        // Resume: Load checkpoint and verify state can be recovered
        let loaded = Checkpoint::load(ms_root, &session_id)
            .unwrap()
            .expect("Checkpoint should exist after crash");

        // Verify we can recover the essential state
        assert_eq!(loaded.phase, "extract_patterns");
        assert!(loaded.progress >= 0.30, "Should be past first two phases");

        // Critical: query must be recoverable for resume
        assert_eq!(loaded.get_state("query"), Some("crash-test"));

        let qualified_sessions = loaded.get_state("qualified_sessions").unwrap();
        let session_ids: Vec<&str> = qualified_sessions.split(',').collect();
        assert_eq!(session_ids.len(), 3);
        assert!(session_ids.contains(&"session-a"));
        assert!(session_ids.contains(&"session-b"));
        assert!(session_ids.contains(&"session-c"));

        assert_eq!(loaded.get_state("patterns_extracted"), Some("100"));
    }

    #[test]
    fn test_build_session_checkpoint_interval_tracking() {
        let temp_dir = tempfile::tempdir().unwrap();
        let ms_root = temp_dir.path();

        let mut session = BuildSession::new("interval-test", QualityGates::default())
            .with_checkpoint_interval(Duration::from_millis(10));

        // Initially should not need checkpoint
        assert!(!session.should_checkpoint());

        // Wait for interval to elapse
        std::thread::sleep(Duration::from_millis(15));
        assert!(session.should_checkpoint());

        // Save checkpoint
        session.save_checkpoint(ms_root).unwrap();

        // After saving, should not need checkpoint again immediately
        assert!(!session.should_checkpoint());

        // Wait again
        std::thread::sleep(Duration::from_millis(15));
        assert!(session.should_checkpoint());
    }

    #[test]
    fn test_build_state_serialization() {
        let state = BuildState {
            qualified_session_ids: vec!["a".to_string(), "b".to_string(), "c".to_string()],
            patterns_extracted: 150,
            patterns_filtered: 120,
        };

        let json = serde_json::to_string(&state).unwrap();
        let restored: BuildState = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.qualified_session_ids, state.qualified_session_ids);
        assert_eq!(restored.patterns_extracted, state.patterns_extracted);
        assert_eq!(restored.patterns_filtered, state.patterns_filtered);
    }

    #[test]
    fn test_build_completion_markdown_success() {
        let completion = BuildCompletion {
            duration_secs: 12.3,
            success: true,
            tests_passed: Some(10),
            tests_failed: Some(1),
            tests_skipped: Some(2),
            coverage_percent: Some(87.5),
            error_summary: None,
        };

        let md = completion.to_markdown();
        assert!(md.contains("Build Succeeded"));
        assert!(md.contains("Duration"));
        assert!(md.contains("10 passed, 1 failed, 2 skipped"));
        assert!(md.contains("87.5%"));
        assert!(!md.contains("Error Summary"));
    }

    #[test]
    fn test_build_completion_markdown_failure_includes_error() {
        let completion = BuildCompletion::failure(4.2, "boom");
        let md = completion.to_markdown();
        assert!(md.contains("Build Failed"));
        assert!(md.contains("Duration"));
        assert!(md.contains("Error Summary"));
        assert!(md.contains("boom"));
    }

    #[test]
    fn test_build_phase_serialization() {
        // Test all phases serialize and deserialize correctly
        let phases = [
            BuildPhase::SearchSessions,
            BuildPhase::QualityFilter,
            BuildPhase::ExtractPatterns,
            BuildPhase::FilterPatterns,
            BuildPhase::Synthesize,
            BuildPhase::Complete,
            BuildPhase::Failed,
        ];

        for phase in phases {
            let json = serde_json::to_string(&phase).unwrap();
            let restored: BuildPhase = serde_json::from_str(&json).unwrap();
            assert_eq!(restored, phase, "Phase {:?} should roundtrip", phase);
        }
    }

    #[test]
    fn test_build_session_progress_at_each_phase() {
        let mut session = BuildSession::new("progress-test", QualityGates::default());

        // Test progress calculation through full pipeline
        let expected_cumulative: &[(BuildPhase, f64)] = &[
            (BuildPhase::SearchSessions, 0.0),
            (BuildPhase::QualityFilter, 0.15),
            (BuildPhase::ExtractPatterns, 0.30),
            (BuildPhase::FilterPatterns, 0.60),
            (BuildPhase::Synthesize, 0.75),
            (BuildPhase::Complete, 1.0),
        ];

        for (expected_phase, expected_progress) in expected_cumulative {
            assert_eq!(session.phase, *expected_phase);
            session.phase_progress = 0.0;
            let progress = session.overall_progress();
            assert!(
                (progress - expected_progress).abs() < 0.01,
                "At {:?}, expected progress {}, got {}",
                expected_phase,
                expected_progress,
                progress
            );

            if session.phase != BuildPhase::Complete {
                session.advance_phase();
            }
        }
    }

    #[test]
    fn test_build_session_does_not_advance_past_complete() {
        let mut session = BuildSession::new("test", QualityGates::default());

        // Advance to complete
        while session.phase != BuildPhase::Complete {
            session.advance_phase();
        }

        // Try to advance past complete
        session.advance_phase();
        assert_eq!(session.phase, BuildPhase::Complete);
        session.advance_phase();
        assert_eq!(session.phase, BuildPhase::Complete);
    }

    #[test]
    fn test_build_session_does_not_advance_past_failed() {
        let mut session = BuildSession::new("test", QualityGates::default());
        session.fail();

        // Try to advance past failed
        session.advance_phase();
        assert_eq!(session.phase, BuildPhase::Failed);
    }
}
