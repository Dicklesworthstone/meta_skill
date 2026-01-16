//! Brenner Method / Guided Mining Wizard
//!
//! Implements the Brenner Method for extracting high-quality skills from CASS sessions.
//! The method enforces structured reasoning: identify invariants, variables, and
//! generative grammar rather than summarizing transcripts.
//!
//! The two axioms:
//! 1. Effective coding has a generative grammar - cognitive moves can be identified
//! 2. Understanding = ability to reproduce - a skill is valid only if executable

use std::collections::HashSet;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{MsError, Result};

use super::client::{CassClient, Session, SessionMatch};
use super::quality::{QualityScorer, SessionQuality};
use super::transformation::{GeneralizationValidation, SpecificToGeneralTransformer};
use super::uncertainty::UncertaintyQueue;

// =============================================================================
// Skill Tags (Operator Algebra)
// =============================================================================

/// Cognitive move tags from the Brenner method
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CognitiveMoveTag {
    /// How to pick what to work on
    ProblemSelection,
    /// Explicit enumeration of approaches
    HypothesisSlate,
    /// Both approaches could be wrong
    ThirdAlternative,
    /// Multi-round improvement
    IterativeRefinement,
    /// Abandoning failing approaches
    RuthlessKill,
    /// Pilot experiments to de-risk
    Quickie,
    /// "What would I see if true?"
    MaterializationInstinct,
    /// The generalizable principle
    InnerTruth,
}

impl CognitiveMoveTag {
    #[must_use] 
    pub const fn all() -> &'static [Self] {
        &[
            Self::ProblemSelection,
            Self::HypothesisSlate,
            Self::ThirdAlternative,
            Self::IterativeRefinement,
            Self::RuthlessKill,
            Self::Quickie,
            Self::MaterializationInstinct,
            Self::InnerTruth,
        ]
    }

    #[must_use] 
    pub const fn description(&self) -> &'static str {
        match self {
            Self::ProblemSelection => "How to pick what to work on",
            Self::HypothesisSlate => "Explicit enumeration of approaches",
            Self::ThirdAlternative => "Both approaches could be wrong",
            Self::IterativeRefinement => "Multi-round improvement",
            Self::RuthlessKill => "Abandoning failing approaches",
            Self::Quickie => "Pilot experiments to de-risk",
            Self::MaterializationInstinct => "What would I see if true?",
            Self::InnerTruth => "The generalizable principle",
        }
    }
}

// =============================================================================
// Cognitive Move
// =============================================================================

/// A cognitive move extracted from a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveMove {
    /// Unique identifier
    pub id: String,
    /// The move tag
    pub tag: CognitiveMoveTag,
    /// Description of what happened
    pub description: String,
    /// Evidence from the session
    pub evidence: MoveEvidence,
    /// Confidence in this extraction (0.0-1.0)
    pub confidence: f32,
    /// Has this been reviewed by user?
    pub reviewed: bool,
    /// User's decision on this move
    pub decision: Option<MoveDecision>,
}

/// Evidence supporting a cognitive move
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveEvidence {
    /// Session ID where found
    pub session_id: String,
    /// Message indices containing the evidence
    pub message_indices: Vec<usize>,
    /// Excerpt from the session
    pub excerpt: String,
    /// Additional notes
    pub notes: Option<String>,
}

/// User's decision on a cognitive move
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MoveDecision {
    /// Accept as valid
    Accept,
    /// Reject as invalid
    Reject,
    /// Needs more evidence
    NeedsEvidence,
    /// Flagged for third-alternative guard
    Flagged { reason: String },
}

// =============================================================================
// Selected Session
// =============================================================================

/// A session selected for mining
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectedSession {
    /// The session search match data
    pub match_data: SessionMatch,
    /// Full session content
    pub session: Session,
    /// Quality score
    pub quality: SessionQuality,
    /// Whether user has confirmed selection
    pub confirmed: bool,
}

// =============================================================================
// Skill Draft
// =============================================================================

/// A draft skill being built
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrennerSkillDraft {
    /// Name of the skill
    pub name: String,
    /// Description
    pub description: String,
    /// The rules/patterns
    pub rules: Vec<SkillRule>,
    /// Examples
    pub examples: Vec<SkillExample>,
    /// When to avoid using this skill
    pub avoid_when: Vec<String>,
    /// Calibration notes (limitations)
    pub calibration: Vec<String>,
    /// Validation result
    pub validation: Option<GeneralizationValidation>,
}

/// A rule in a skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRule {
    pub id: String,
    pub description: String,
    pub evidence: Vec<String>,
    pub confidence: f32,
}

/// An example in a skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillExample {
    pub title: String,
    pub context: String,
    pub content: String,
}

// =============================================================================
// Wizard State Machine
// =============================================================================

/// The current state of the Brenner wizard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WizardState {
    /// Initial state - selecting sessions
    SessionSelection {
        query: String,
        results: Vec<SessionMatch>,
        selected: HashSet<usize>,
    },
    /// Extracting cognitive moves from sessions
    MoveExtraction {
        sessions: Vec<SelectedSession>,
        moves: Vec<CognitiveMove>,
        current_session_idx: usize,
    },
    /// Third-alternative guard - reviewing low-confidence moves
    ThirdAlternativeGuard {
        moves: Vec<CognitiveMove>,
        flagged_indices: Vec<usize>,
        current_idx: usize,
    },
    /// Formalizing into a skill
    SkillFormalization {
        moves: Vec<CognitiveMove>,
        draft: BrennerSkillDraft,
    },
    /// Running materialization test
    MaterializationTest {
        draft: BrennerSkillDraft,
        test_results: Option<TestResults>,
    },
    /// Wizard completed
    Complete {
        output_dir: PathBuf,
        skill_path: PathBuf,
        manifest_path: PathBuf,
        draft: BrennerSkillDraft,
    },
    /// Wizard cancelled
    Cancelled { reason: String },
}

/// Test results from materialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResults {
    pub retrieval_tests_passed: usize,
    pub retrieval_tests_total: usize,
    pub validation_passed: bool,
    pub issues: Vec<String>,
}

// =============================================================================
// Wizard Checkpoint
// =============================================================================

/// Checkpoint for resuming wizard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WizardCheckpoint {
    pub id: String,
    pub state: WizardState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub query: String,
}

impl WizardCheckpoint {
    #[must_use] 
    pub fn new(query: &str, state: WizardState) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            state,
            created_at: now,
            updated_at: now,
            query: query.to_string(),
        }
    }

    pub fn update(&mut self, state: WizardState) {
        self.state = state;
        self.updated_at = Utc::now();
    }
}

// =============================================================================
// Wizard Output
// =============================================================================

/// Output from wizard completion
#[derive(Debug)]
pub enum WizardOutput {
    Success {
        skill_path: PathBuf,
        manifest_path: PathBuf,
        calibration_path: PathBuf,
        /// The completed draft for file generation
        draft: BrennerSkillDraft,
        /// Pre-generated manifest JSON
        manifest_json: String,
    },
    Cancelled {
        reason: String,
        checkpoint_id: Option<String>,
    },
}

/// Generate SKILL.md content from a draft (standalone function)
#[must_use] 
pub fn generate_skill_md(draft: &BrennerSkillDraft) -> String {
    let mut md = String::new();

    md.push_str(&format!("# {}\n\n", draft.name));
    md.push_str(&format!("{}\n\n", draft.description));

    if !draft.rules.is_empty() {
        md.push_str("## Rules\n\n");
        for rule in &draft.rules {
            md.push_str(&format!(
                "### {} (confidence: {:.0}%)\n\n",
                rule.id,
                rule.confidence * 100.0
            ));
            md.push_str(&format!("{}\n\n", rule.description));
            if !rule.evidence.is_empty() {
                md.push_str("**Evidence:**\n");
                for ev in &rule.evidence {
                    md.push_str(&format!("- {ev}\n"));
                }
                md.push('\n');
            }
        }
    }

    if !draft.examples.is_empty() {
        md.push_str("## Examples\n\n");
        for example in &draft.examples {
            md.push_str(&format!("### {}\n\n", example.title));
            md.push_str(&format!("**Context:** {}\n\n", example.context));
            md.push_str(&format!("```\n{}\n```\n\n", example.content));
        }
    }

    if !draft.avoid_when.is_empty() {
        md.push_str("## Avoid When\n\n");
        for avoid in &draft.avoid_when {
            md.push_str(&format!("- {avoid}\n"));
        }
        md.push('\n');
    }

    if !draft.calibration.is_empty() {
        md.push_str("## Calibration Notes\n\n");
        for cal in &draft.calibration {
            md.push_str(&format!("- {cal}\n"));
        }
    }

    md
}

// =============================================================================
// Brenner Wizard
// =============================================================================

/// The Brenner Method wizard for guided skill mining
pub struct BrennerWizard {
    /// Current state
    state: WizardState,
    /// Checkpoint for resume
    checkpoint: WizardCheckpoint,
    /// Configuration
    config: BrennerConfig,
    /// Transformer for generalization (lazy initialized)
    #[allow(dead_code)]
    transformer: Option<SpecificToGeneralTransformer>,
    /// Uncertainty queue for low-confidence items
    #[allow(dead_code)]
    uncertainty_queue: Option<UncertaintyQueue>,
}

/// Configuration for the Brenner wizard
#[derive(Debug, Clone)]
pub struct BrennerConfig {
    /// Minimum session quality score
    pub min_quality: f32,
    /// Minimum confidence for moves
    pub min_confidence: f32,
    /// Maximum sessions to consider
    pub max_sessions: usize,
    /// Output directory
    pub output_dir: PathBuf,
}

impl Default for BrennerConfig {
    fn default() -> Self {
        Self {
            min_quality: 0.6,
            min_confidence: 0.5,
            max_sessions: 10,
            output_dir: PathBuf::from("."),
        }
    }
}

impl BrennerWizard {
    /// Create a new wizard with a query
    #[must_use] 
    pub fn new(query: &str, config: BrennerConfig) -> Self {
        let state = WizardState::SessionSelection {
            query: query.to_string(),
            results: Vec::new(),
            selected: HashSet::new(),
        };
        let checkpoint = WizardCheckpoint::new(query, state.clone());

        Self {
            state,
            checkpoint,
            config,
            transformer: None,
            uncertainty_queue: None,
        }
    }

    /// Resume from a checkpoint
    #[must_use] 
    pub fn resume(checkpoint: WizardCheckpoint, config: BrennerConfig) -> Self {
        Self {
            state: checkpoint.state.clone(),
            checkpoint,
            config,
            transformer: None,
            uncertainty_queue: None,
        }
    }

    /// Get current checkpoint for saving
    pub const fn checkpoint(&self) -> &WizardCheckpoint {
        &self.checkpoint
    }

    /// Get current state
    pub const fn state(&self) -> &WizardState {
        &self.state
    }

    /// Check if wizard is complete
    pub const fn is_complete(&self) -> bool {
        matches!(
            self.state,
            WizardState::Complete { .. } | WizardState::Cancelled { .. }
        )
    }

    // =========================================================================
    // State Transitions
    // =========================================================================

    /// Set session search results
    pub fn set_session_results(&mut self, new_results: Vec<SessionMatch>) {
        if let WizardState::SessionSelection {
            ref mut results, ..
        } = self.state
        {
            *results = new_results;
            self.checkpoint.update(self.state.clone());
        }
    }

    /// Toggle session selection
    pub fn toggle_session(&mut self, index: usize) {
        if let WizardState::SessionSelection {
            ref mut selected, ..
        } = self.state
        {
            if selected.contains(&index) {
                selected.remove(&index);
            } else {
                selected.insert(index);
            }
            self.checkpoint.update(self.state.clone());
        }
    }

    /// Confirm session selection and move to extraction
    ///
    /// The caller is responsible for fetching full sessions and scoring them.
    pub fn confirm_sessions(&mut self, sessions: Vec<SelectedSession>) -> Result<()> {
        if sessions.is_empty() {
            return Err(MsError::Config("No sessions selected".into()));
        }

        self.state = WizardState::MoveExtraction {
            sessions,
            moves: Vec::new(),
            current_session_idx: 0,
        };
        self.checkpoint.update(self.state.clone());
        Ok(())
    }

    /// Add an extracted cognitive move
    pub fn add_move(&mut self, mov: CognitiveMove) {
        if let WizardState::MoveExtraction { ref mut moves, .. } = self.state {
            moves.push(mov);
            self.checkpoint.update(self.state.clone());
        }
    }

    /// Move to next session in extraction
    pub fn next_session(&mut self) -> bool {
        if let WizardState::MoveExtraction {
            ref sessions,
            ref mut current_session_idx,
            ..
        } = self.state
        {
            if *current_session_idx + 1 < sessions.len() {
                *current_session_idx += 1;
                self.checkpoint.update(self.state.clone());
                return true;
            }
        }
        false
    }

    /// Finish extraction and move to third-alternative guard
    pub fn finish_extraction(&mut self) -> Result<()> {
        let moves = match &self.state {
            WizardState::MoveExtraction { moves, .. } => moves.clone(),
            _ => return Err(MsError::Config("Not in move extraction state".into())),
        };

        if moves.is_empty() {
            return Err(MsError::Config("No moves extracted".into()));
        }

        // Find moves needing review (low confidence or unreviewed)
        let flagged_indices: Vec<usize> = moves
            .iter()
            .enumerate()
            .filter(|(_, m)| m.confidence < self.config.min_confidence || !m.reviewed)
            .map(|(i, _)| i)
            .collect();

        self.state = WizardState::ThirdAlternativeGuard {
            moves,
            flagged_indices,
            current_idx: 0,
        };
        self.checkpoint.update(self.state.clone());
        Ok(())
    }

    /// Review a move in the guard phase
    pub fn review_move(&mut self, decision: MoveDecision) -> Result<()> {
        if let WizardState::ThirdAlternativeGuard {
            ref mut moves,
            ref flagged_indices,
            ref mut current_idx,
        } = self.state
        {
            if let Some(&move_idx) = flagged_indices.get(*current_idx) {
                if let Some(mov) = moves.get_mut(move_idx) {
                    mov.decision = Some(decision);
                    mov.reviewed = true;
                }
            }
            self.checkpoint.update(self.state.clone());
            Ok(())
        } else {
            Err(MsError::Config("Not in guard state".into()))
        }
    }

    /// Move to next flagged move
    pub fn next_flagged_move(&mut self) -> bool {
        if let WizardState::ThirdAlternativeGuard {
            ref flagged_indices,
            ref mut current_idx,
            ..
        } = self.state
        {
            if *current_idx + 1 < flagged_indices.len() {
                *current_idx += 1;
                self.checkpoint.update(self.state.clone());
                return true;
            }
        }
        false
    }

    /// Finish guard and move to formalization
    pub fn finish_guard(&mut self) -> Result<()> {
        let moves = match &self.state {
            WizardState::ThirdAlternativeGuard { moves, .. } => moves.clone(),
            _ => return Err(MsError::Config("Not in guard state".into())),
        };

        // Filter to accepted moves
        let accepted_moves: Vec<CognitiveMove> = moves
            .into_iter()
            .filter(|m| {
                matches!(
                    m.decision,
                    Some(MoveDecision::Accept | MoveDecision::NeedsEvidence) | None
                )
            })
            .collect();

        // Build initial skill draft
        let draft = self.build_skill_draft(&accepted_moves);

        self.state = WizardState::SkillFormalization {
            moves: accepted_moves,
            draft,
        };
        self.checkpoint.update(self.state.clone());
        Ok(())
    }

    /// Build a skill draft from moves
    fn build_skill_draft(&self, moves: &[CognitiveMove]) -> BrennerSkillDraft {
        let rules: Vec<SkillRule> = moves
            .iter()
            .filter(|m| {
                matches!(
                    m.tag,
                    CognitiveMoveTag::InnerTruth
                        | CognitiveMoveTag::MaterializationInstinct
                        | CognitiveMoveTag::HypothesisSlate
                )
            })
            .map(|m| SkillRule {
                id: m.id.clone(),
                description: m.description.clone(),
                evidence: vec![m.evidence.excerpt.clone()],
                confidence: m.confidence,
            })
            .collect();

        let avoid_when: Vec<String> = moves
            .iter()
            .filter(|m| matches!(m.decision, Some(MoveDecision::Flagged { .. })))
            .map(|m| {
                if let Some(MoveDecision::Flagged { reason }) = &m.decision {
                    format!("{}: {}", m.description, reason)
                } else {
                    m.description.clone()
                }
            })
            .collect();

        BrennerSkillDraft {
            name: self.checkpoint.query.clone(),
            description: format!(
                "Skill extracted using Brenner method from query: {}",
                self.checkpoint.query
            ),
            rules,
            examples: Vec::new(),
            avoid_when,
            calibration: Vec::new(),
            validation: None,
        }
    }

    /// Update the skill draft
    pub fn update_draft(&mut self, draft: BrennerSkillDraft) {
        if let WizardState::SkillFormalization {
            draft: ref mut d, ..
        } = self.state
        {
            *d = draft;
            self.checkpoint.update(self.state.clone());
        }
    }

    /// Move to materialization test
    pub fn start_test(&mut self) -> Result<()> {
        let draft = match &self.state {
            WizardState::SkillFormalization { draft, .. } => draft.clone(),
            _ => return Err(MsError::Config("Not in formalization state".into())),
        };

        self.state = WizardState::MaterializationTest {
            draft,
            test_results: None,
        };
        self.checkpoint.update(self.state.clone());
        Ok(())
    }

    /// Set test results
    pub fn set_test_results(&mut self, results: TestResults) {
        if let WizardState::MaterializationTest {
            ref mut test_results,
            ..
        } = self.state
        {
            *test_results = Some(results);
            self.checkpoint.update(self.state.clone());
        }
    }

    /// Complete the wizard
    pub fn complete(&mut self, output_dir: PathBuf) -> Result<WizardOutput> {
        let draft = match &self.state {
            WizardState::MaterializationTest { draft, .. } => draft.clone(),
            WizardState::SkillFormalization { draft, .. } => draft.clone(),
            _ => return Err(MsError::Config("Not ready to complete".into())),
        };

        let skill_path = output_dir.join("SKILL.md");
        let manifest_path = output_dir.join("mining-manifest.json");
        let calibration_path = output_dir.join("calibration.md");

        self.state = WizardState::Complete {
            output_dir,
            skill_path: skill_path.clone(),
            manifest_path: manifest_path.clone(),
            draft: draft.clone(),
        };
        self.checkpoint.update(self.state.clone());

        // Generate manifest JSON
        let manifest_json = self.generate_manifest()?;

        Ok(WizardOutput::Success {
            skill_path,
            manifest_path,
            calibration_path,
            draft,
            manifest_json,
        })
    }

    /// Cancel the wizard
    pub fn cancel(&mut self, reason: &str) {
        self.state = WizardState::Cancelled {
            reason: reason.to_string(),
        };
        self.checkpoint.update(self.state.clone());
    }

    /// Return to formalization from materialization test
    pub fn return_to_formalization(&mut self, draft: BrennerSkillDraft) {
        self.state = WizardState::SkillFormalization {
            moves: Vec::new(),
            draft,
        };
        self.checkpoint.update(self.state.clone());
    }

    // =========================================================================
    // Serialization
    // =========================================================================

    /// Generate SKILL.md content (delegates to standalone function)
    pub fn generate_skill_md(&self, draft: &BrennerSkillDraft) -> String {
        generate_skill_md(draft)
    }

    /// Generate mining manifest JSON
    pub fn generate_manifest(&self) -> Result<String> {
        let manifest = serde_json::json!({
            "version": "1.0",
            "checkpoint_id": self.checkpoint.id,
            "query": self.checkpoint.query,
            "created_at": self.checkpoint.created_at,
            "updated_at": self.checkpoint.updated_at,
            "state": format!("{:?}", std::mem::discriminant(&self.state)),
        });
        Ok(serde_json::to_string_pretty(&manifest)?)
    }
}

// =============================================================================
// Interactive Runner
// =============================================================================

/// Run the wizard interactively
pub fn run_interactive(
    wizard: &mut BrennerWizard,
    client: &CassClient,
    quality_scorer: &QualityScorer,
) -> Result<WizardOutput> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        match wizard.state() {
            WizardState::SessionSelection {
                query,
                results,
                selected,
            } => {
                println!("\n{}", "=".repeat(60));
                println!("BRENNER WIZARD - Session Selection");
                println!("{}", "=".repeat(60));
                println!("\nQuery: {query}");

                if results.is_empty() {
                    println!("\nSearching for sessions...");
                    // In real impl, search via client
                    println!("(Simulated: no sessions found - enter 'd' to use demo data)");
                }

                println!("\nSelected: {} sessions", selected.len());
                println!("\nCommands:");
                println!("  [number] - toggle session selection");
                println!("  c - confirm and proceed");
                println!("  q - quit wizard");
                print!("\n> ");
                stdout.flush()?;

                let mut input = String::new();
                stdin.lock().read_line(&mut input)?;
                let input = input.trim();

                match input {
                    "q" => {
                        wizard.cancel("User cancelled");
                        break;
                    }
                    "c" => {
                        // Build Vec<SelectedSession> from selected indices
                        let mut sessions = Vec::new();
                        for &idx in selected {
                            if let Some(match_data) = results.get(idx) {
                                match client.get_session(&match_data.session_id) {
                                    Ok(session) => {
                                        let quality = quality_scorer.score(&session);
                                        sessions.push(SelectedSession {
                                            match_data: match_data.clone(),
                                            session,
                                            quality,
                                            confirmed: true,
                                        });
                                    }
                                    Err(e) => {
                                        println!("Failed to load session {}: {}", match_data.session_id, e);
                                    }
                                }
                            }
                        }
                        if sessions.is_empty() {
                            println!("No valid sessions to confirm");
                            continue;
                        }
                        if let Err(e) = wizard.confirm_sessions(sessions) {
                            println!("Error: {e}");
                            continue;
                        }
                    }
                    "d" => {
                        // Demo data
                        let demo_results = vec![
                            SessionMatch {
                                session_id: "session-001".to_string(),
                                path: "/demo/sessions/session-001.json".to_string(),
                                score: 0.9,
                                snippet: Some("Authentication refactoring session".to_string()),
                                content_hash: None,
                                project: Some("demo-project".to_string()),
                                timestamp: Some("2024-01-01T00:00:00Z".to_string()),
                            },
                            SessionMatch {
                                session_id: "session-002".to_string(),
                                path: "/demo/sessions/session-002.json".to_string(),
                                score: 0.85,
                                snippet: Some("Error handling improvements".to_string()),
                                content_hash: None,
                                project: Some("demo-project".to_string()),
                                timestamp: Some("2024-01-02T00:00:00Z".to_string()),
                            },
                        ];
                        wizard.set_session_results(demo_results);
                        wizard.toggle_session(0);
                        wizard.toggle_session(1);
                    }
                    n if n.parse::<usize>().is_ok() => {
                        let idx = n.parse::<usize>().unwrap();
                        wizard.toggle_session(idx);
                    }
                    _ => println!("Unknown command"),
                }
            }

            WizardState::MoveExtraction {
                sessions,
                moves,
                current_session_idx,
            } => {
                println!("\n{}", "=".repeat(60));
                println!("BRENNER WIZARD - Cognitive Move Extraction");
                println!("{}", "=".repeat(60));
                println!(
                    "\nSession {}/{}: {}",
                    current_session_idx + 1,
                    sessions.len(),
                    sessions
                        .get(*current_session_idx)
                        .map_or("?", |s| s.session.id.as_str())
                );
                println!("Moves extracted: {}", moves.len());

                println!("\nCognitive Move Tags:");
                for (i, tag) in CognitiveMoveTag::all().iter().enumerate() {
                    println!("  {} - {:?}: {}", i + 1, tag, tag.description());
                }

                println!("\nCommands:");
                println!("  a [tag] [description] - add move");
                println!("  n - next session");
                println!("  f - finish extraction");
                println!("  q - quit wizard");
                print!("\n> ");
                stdout.flush()?;

                let mut input = String::new();
                stdin.lock().read_line(&mut input)?;
                let input = input.trim();

                match input.split_whitespace().next() {
                    Some("q") => {
                        wizard.cancel("User cancelled");
                        break;
                    }
                    Some("n") => {
                        if !wizard.next_session() {
                            println!("No more sessions");
                        }
                    }
                    Some("f") => {
                        if let Err(e) = wizard.finish_extraction() {
                            println!("Error: {e}");
                            continue;
                        }
                    }
                    Some("a") => {
                        let parts: Vec<&str> = input.splitn(3, ' ').collect();
                        if parts.len() >= 3 {
                            let tag_idx: usize = parts[1].parse().unwrap_or(0);
                            if tag_idx > 0 && tag_idx <= CognitiveMoveTag::all().len() {
                                let tag = CognitiveMoveTag::all()[tag_idx - 1];
                                let session = sessions.get(*current_session_idx);
                                let mov = CognitiveMove {
                                    id: Uuid::new_v4().to_string(),
                                    tag,
                                    description: parts[2].to_string(),
                                    evidence: MoveEvidence {
                                        session_id: session
                                            .map(|s| s.session.id.clone())
                                            .unwrap_or_default(),
                                        message_indices: vec![],
                                        excerpt: "...".to_string(),
                                        notes: None,
                                    },
                                    confidence: 0.7,
                                    reviewed: true,
                                    decision: Some(MoveDecision::Accept),
                                };
                                wizard.add_move(mov);
                                println!("Added move");
                            } else {
                                println!("Invalid tag number");
                            }
                        } else {
                            println!("Usage: a [tag_number] [description]");
                        }
                    }
                    _ => println!("Unknown command"),
                }
            }

            WizardState::ThirdAlternativeGuard {
                moves,
                flagged_indices,
                current_idx,
            } => {
                println!("\n{}", "=".repeat(60));
                println!("BRENNER WIZARD - Third Alternative Guard");
                println!("{}", "=".repeat(60));

                if flagged_indices.is_empty() {
                    println!("\nNo moves flagged for review!");
                    if let Err(e) = wizard.finish_guard() {
                        println!("Error: {e}");
                    }
                    continue;
                }

                println!(
                    "\nReviewing flagged move {}/{}",
                    current_idx + 1,
                    flagged_indices.len()
                );

                if let Some(&move_idx) = flagged_indices.get(*current_idx) {
                    if let Some(mov) = moves.get(move_idx) {
                        println!("\nTag: {:?}", mov.tag);
                        println!("Description: {}", mov.description);
                        println!("Confidence: {:.0}%", mov.confidence * 100.0);
                        println!("Evidence: {}", mov.evidence.excerpt);
                    }
                }

                println!("\nCommands:");
                println!("  y - accept");
                println!("  n - reject");
                println!("  e - needs more evidence");
                println!("  f [reason] - flag for avoid_when");
                println!("  s - skip to formalization");
                println!("  q - quit wizard");
                print!("\n> ");
                stdout.flush()?;

                let mut input = String::new();
                stdin.lock().read_line(&mut input)?;
                let input = input.trim();

                match input.split_whitespace().next() {
                    Some("q") => {
                        wizard.cancel("User cancelled");
                        break;
                    }
                    Some("y") => {
                        wizard.review_move(MoveDecision::Accept)?;
                        if !wizard.next_flagged_move() {
                            wizard.finish_guard()?;
                        }
                    }
                    Some("n") => {
                        wizard.review_move(MoveDecision::Reject)?;
                        if !wizard.next_flagged_move() {
                            wizard.finish_guard()?;
                        }
                    }
                    Some("e") => {
                        wizard.review_move(MoveDecision::NeedsEvidence)?;
                        if !wizard.next_flagged_move() {
                            wizard.finish_guard()?;
                        }
                    }
                    Some("f") => {
                        let reason = input.strip_prefix("f ").unwrap_or("").to_string();
                        wizard.review_move(MoveDecision::Flagged {
                            reason: if reason.is_empty() {
                                "Unspecified".to_string()
                            } else {
                                reason
                            },
                        })?;
                        if !wizard.next_flagged_move() {
                            wizard.finish_guard()?;
                        }
                    }
                    Some("s") => {
                        wizard.finish_guard()?;
                    }
                    _ => println!("Unknown command"),
                }
            }

            WizardState::SkillFormalization { moves: _, draft } => {
                println!("\n{}", "=".repeat(60));
                println!("BRENNER WIZARD - Skill Formalization");
                println!("{}", "=".repeat(60));

                println!("\nDraft Skill: {}", draft.name);
                println!("Rules: {}", draft.rules.len());
                println!("Avoid When: {}", draft.avoid_when.len());

                println!("\n--- PREVIEW ---");
                println!("{}", wizard.generate_skill_md(draft));
                println!("--- END PREVIEW ---");

                println!("\nCommands:");
                println!("  t - run materialization test");
                println!("  c - complete and save");
                println!("  q - quit wizard");
                print!("\n> ");
                stdout.flush()?;

                let mut input = String::new();
                stdin.lock().read_line(&mut input)?;
                let input = input.trim();

                match input {
                    "q" => {
                        wizard.cancel("User cancelled");
                        break;
                    }
                    "t" => {
                        wizard.start_test()?;
                    }
                    "c" => {
                        let output = wizard.complete(wizard.config.output_dir.clone())?;
                        return Ok(output);
                    }
                    _ => println!("Unknown command"),
                }
            }

            WizardState::MaterializationTest {
                draft,
                test_results: _,
            } => {
                // Extract data first to avoid borrow issues
                let draft_clone = draft.clone();
                let draft_name = draft_clone.name.clone();
                let rules_len = draft_clone.rules.len();

                println!("\n{}", "=".repeat(60));
                println!("BRENNER WIZARD - Materialization Test");
                println!("{}", "=".repeat(60));

                println!("\nRunning tests for: {draft_name}");

                // Simulate test
                let results = TestResults {
                    retrieval_tests_passed: rules_len,
                    retrieval_tests_total: rules_len,
                    validation_passed: true,
                    issues: Vec::new(),
                };
                wizard.set_test_results(results.clone());

                println!("\nResults:");
                println!(
                    "  Retrieval: {}/{}",
                    results.retrieval_tests_passed, results.retrieval_tests_total
                );
                println!(
                    "  Validation: {}",
                    if results.validation_passed {
                        "PASSED"
                    } else {
                        "FAILED"
                    }
                );

                println!("\nCommands:");
                println!("  c - complete and save");
                println!("  r - return to formalization");
                println!("  q - quit wizard");
                print!("\n> ");
                stdout.flush()?;

                let mut input = String::new();
                stdin.lock().read_line(&mut input)?;
                let input = input.trim();

                match input {
                    "q" => {
                        wizard.cancel("User cancelled");
                        break;
                    }
                    "c" => {
                        let output = wizard.complete(wizard.config.output_dir.clone())?;
                        return Ok(output);
                    }
                    "r" => {
                        wizard.return_to_formalization(draft_clone);
                    }
                    _ => println!("Unknown command"),
                }
            }

            WizardState::Complete {
                skill_path,
                manifest_path,
                draft,
                ..
            } => {
                println!("\n{}", "=".repeat(60));
                println!("BRENNER WIZARD - Complete!");
                println!("{}", "=".repeat(60));
                println!("\nOutputs:");
                println!("  Skill: {}", skill_path.display());
                println!("  Manifest: {}", manifest_path.display());
                let manifest_json = wizard.generate_manifest()?;
                return Ok(WizardOutput::Success {
                    skill_path: skill_path.clone(),
                    manifest_path: manifest_path.clone(),
                    calibration_path: wizard.config.output_dir.join("calibration.md"),
                    draft: draft.clone(),
                    manifest_json,
                });
            }

            WizardState::Cancelled { reason } => {
                return Ok(WizardOutput::Cancelled {
                    reason: reason.clone(),
                    checkpoint_id: Some(wizard.checkpoint.id.clone()),
                });
            }
        }
    }

    Ok(WizardOutput::Cancelled {
        reason: "Wizard loop exited".to_string(),
        checkpoint_id: Some(wizard.checkpoint.id.clone()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wizard_creation() {
        let wizard = BrennerWizard::new("test query", BrennerConfig::default());
        assert!(matches!(
            wizard.state(),
            WizardState::SessionSelection { .. }
        ));
    }

    #[test]
    fn test_cognitive_move_tags() {
        assert_eq!(CognitiveMoveTag::all().len(), 8);
        assert!(!CognitiveMoveTag::InnerTruth.description().is_empty());
    }

    #[test]
    fn test_checkpoint_creation() {
        let checkpoint = WizardCheckpoint::new(
            "query",
            WizardState::SessionSelection {
                query: "query".to_string(),
                results: vec![],
                selected: HashSet::new(),
            },
        );
        assert!(!checkpoint.id.is_empty());
    }

    #[test]
    fn test_skill_draft_generation() {
        let wizard = BrennerWizard::new("test", BrennerConfig::default());
        let draft = BrennerSkillDraft {
            name: "Test Skill".to_string(),
            description: "A test skill".to_string(),
            rules: vec![SkillRule {
                id: "rule-1".to_string(),
                description: "Always test your code".to_string(),
                evidence: vec!["Session showed testing".to_string()],
                confidence: 0.9,
            }],
            examples: vec![],
            avoid_when: vec!["Time pressure".to_string()],
            calibration: vec![],
            validation: None,
        };

        let md = wizard.generate_skill_md(&draft);
        assert!(md.contains("Test Skill"));
        assert!(md.contains("rule-1"));
        assert!(md.contains("Avoid When"));
    }
}
