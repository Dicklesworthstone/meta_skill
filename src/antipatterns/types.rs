//! Anti-pattern types and data structures
//!
//! Anti-patterns are negative patterns extracted from failure signals,
//! rollbacks, corrections, and explicitly marked sessions. They represent
//! what NOT to do and link to positive patterns they constrain.

use serde::{Deserialize, Serialize};

/// Unique identifier for an anti-pattern
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AntiPatternId(pub String);

impl AntiPatternId {
    /// Create a new anti-pattern ID
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Generate a new unique ID
    #[must_use]
    pub fn generate() -> Self {
        Self(format!("ap-{}", uuid::Uuid::new_v4().simple()))
    }
}

impl std::fmt::Display for AntiPatternId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a positive pattern (from mining module)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PatternId(pub String);

impl PatternId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

/// Unique identifier for a session
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub String);

impl SessionId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

/// Unique identifier for an uncertainty queue item
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UncertaintyId(pub String);

// =============================================================================
// Anti-Pattern Core Types
// =============================================================================

/// A negative pattern extracted from failure evidence.
///
/// Anti-patterns represent actions that lead to failures, rollbacks, or
/// explicit corrections. Each anti-pattern MUST link to the positive
/// pattern it constrains.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AntiPattern {
    /// Unique identifier for this anti-pattern
    pub id: AntiPatternId,

    /// The positive pattern this anti-pattern constrains
    pub constrains: Option<PatternId>,

    /// Evidence from sessions supporting this anti-pattern
    pub evidence: Vec<AntiPatternEvidence>,

    /// Synthesized "do not" rule
    pub rule: NegativeRule,

    /// Conditions when this anti-pattern applies
    pub trigger_conditions: Vec<Condition>,

    /// What goes wrong when violated
    pub failure_modes: Vec<FailureMode>,

    /// Confidence based on evidence strength (0.0 to 1.0)
    pub confidence: f32,

    /// Tags for categorization
    pub tags: Vec<String>,

    /// When this anti-pattern was first detected
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// When this anti-pattern was last updated
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl AntiPattern {
    /// Create a new anti-pattern with the given rule
    #[must_use]
    pub fn new(rule: NegativeRule) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: AntiPatternId::generate(),
            constrains: None,
            evidence: Vec::new(),
            rule,
            trigger_conditions: Vec::new(),
            failure_modes: Vec::new(),
            confidence: 0.0,
            tags: Vec::new(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Check if this anti-pattern is orphaned (no positive counterpart)
    #[must_use]
    pub const fn is_orphaned(&self) -> bool {
        self.constrains.is_none()
    }

    /// Add evidence to this anti-pattern
    pub fn add_evidence(&mut self, evidence: AntiPatternEvidence) {
        self.evidence.push(evidence);
        self.updated_at = chrono::Utc::now();
        self.recalculate_confidence();
    }

    /// Recalculate confidence based on evidence strength
    fn recalculate_confidence(&mut self) {
        if self.evidence.is_empty() {
            self.confidence = 0.0;
            return;
        }

        // Base confidence from evidence count
        let evidence_count = self.evidence.len() as f32;
        let base_confidence = (evidence_count / (evidence_count + 2.0)).min(0.9);

        // Boost for diverse evidence sources
        let source_diversity = self.count_unique_sources() as f32;
        let diversity_bonus = (source_diversity / 5.0).min(0.1);

        self.confidence = (base_confidence + diversity_bonus).min(1.0);
    }

    /// Count unique evidence source types
    fn count_unique_sources(&self) -> usize {
        let mut sources = std::collections::HashSet::new();
        for e in &self.evidence {
            sources.insert(std::mem::discriminant(&e.source));
        }
        sources.len()
    }
}

/// Evidence supporting an anti-pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AntiPatternEvidence {
    /// Source of this evidence
    pub source: AntiPatternSource,

    /// Session where the anti-pattern was observed
    pub session_id: SessionId,

    /// The specific failure or correction incident
    pub incident: FailureIncident,

    /// User-provided context if marked explicitly
    pub user_annotation: Option<String>,

    /// When this evidence was collected
    pub collected_at: chrono::DateTime<chrono::Utc>,
}

impl AntiPatternEvidence {
    /// Create new evidence from a source and incident
    #[must_use]
    pub fn new(
        source: AntiPatternSource,
        session_id: SessionId,
        incident: FailureIncident,
    ) -> Self {
        Self {
            source,
            session_id,
            incident,
            user_annotation: None,
            collected_at: chrono::Utc::now(),
        }
    }
}

/// Source of anti-pattern evidence
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AntiPatternSource {
    /// Session explicitly marked as anti-pattern example
    MarkedAntiPattern {
        /// The marker used (e.g., "anti-pattern", "wrong approach")
        marker: String,
    },

    /// Detected from rollback or undo sequence
    RollbackDetected {
        /// Type of rollback observed
        rollback_type: RollbackType,
    },

    /// Explicit "wrong" fix with correction
    WrongFix {
        /// The original (wrong) action
        original: String,
        /// The correction applied
        correction: String,
    },

    /// Failure signal in session
    FailureSignal {
        /// Type of failure signal
        signal_type: FailureSignalType,
    },

    /// Counter-example surfaced during uncertainty resolution
    CounterExample {
        /// Reference to the uncertainty queue item
        uncertainty_id: UncertaintyId,
    },
}

/// Types of rollback/undo operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RollbackType {
    /// `git reset` command
    GitReset,
    /// `git revert` command
    GitRevert,
    /// File was restored from backup or previous version
    FileRestore,
    /// Manual undo (ctrl+z, edit back)
    ManualUndo,
    /// Explicit correction mentioned in conversation
    ExplicitCorrection,
}

impl RollbackType {
    /// Get a human-readable description
    #[must_use]
    pub const fn description(&self) -> &'static str {
        match self {
            Self::GitReset => "git reset (discarded changes)",
            Self::GitRevert => "git revert (undid commit)",
            Self::FileRestore => "file restored from previous version",
            Self::ManualUndo => "manual undo operation",
            Self::ExplicitCorrection => "explicit correction made",
        }
    }
}

/// Types of failure signals
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureSignalType {
    /// Test failure (unit, integration, e2e)
    TestFailure,
    /// Build/compilation error
    BuildError,
    /// Runtime exception or crash
    RuntimeException,
    /// User explicitly rejected the approach
    UserRejection,
    /// Explicit "no" or negative response
    ExplicitNo,
    /// Frustration or negative sentiment
    Frustration,
    /// Timeout or hang
    Timeout,
    /// Permission or access error
    PermissionError,
}

impl FailureSignalType {
    /// Get a human-readable description
    #[must_use]
    pub const fn description(&self) -> &'static str {
        match self {
            Self::TestFailure => "test failure",
            Self::BuildError => "build/compilation error",
            Self::RuntimeException => "runtime exception",
            Self::UserRejection => "user rejected approach",
            Self::ExplicitNo => "explicit negative response",
            Self::Frustration => "frustration signal",
            Self::Timeout => "timeout or hang",
            Self::PermissionError => "permission/access error",
        }
    }
}

// =============================================================================
// Failure Incident Types
// =============================================================================

/// A specific failure incident that generated anti-pattern evidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureIncident {
    /// Description of what failed
    pub description: String,

    /// The action that caused the failure
    pub failed_action: String,

    /// Message indices in the session where this occurred
    pub message_indices: Vec<usize>,

    /// Context around the failure
    pub context: IncidentContext,

    /// The correction applied (if any)
    pub correction: Option<String>,
}

/// Context around a failure incident
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentContext {
    /// Messages before the failure
    pub before: Vec<String>,
    /// Messages after the failure (showing response/correction)
    pub after: Vec<String>,
    /// Working directory or file context
    pub location: Option<String>,
    /// Tool that was used when failure occurred
    pub tool: Option<String>,
}

// =============================================================================
// Negative Rule Types
// =============================================================================

/// A synthesized negative rule (what NOT to do)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NegativeRule {
    /// The rule statement (e.g., "NEVER do X when Y")
    pub statement: String,

    /// Formal predicate for rule matching (optional)
    pub predicate: Option<Predicate>,

    /// Severity if violated
    pub severity: AntiPatternSeverity,

    /// Rationale for why this is wrong
    pub rationale: Option<String>,

    /// What to do instead (reference to positive pattern)
    pub instead: Option<String>,
}

impl NegativeRule {
    /// Create a new negative rule with the given statement
    pub fn new(statement: impl Into<String>, severity: AntiPatternSeverity) -> Self {
        Self {
            statement: statement.into(),
            predicate: None,
            severity,
            rationale: None,
            instead: None,
        }
    }

    /// Builder method to add rationale
    pub fn with_rationale(mut self, rationale: impl Into<String>) -> Self {
        self.rationale = Some(rationale.into());
        self
    }

    /// Builder method to add "instead" suggestion
    pub fn with_instead(mut self, instead: impl Into<String>) -> Self {
        self.instead = Some(instead.into());
        self
    }
}

/// Predicate for matching when a rule applies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Predicate {
    /// The type of predicate
    pub kind: PredicateKind,
    /// Parameters for the predicate
    pub params: Vec<String>,
}

/// Types of predicates
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PredicateKind {
    /// Match on file pattern (glob)
    FilePattern,
    /// Match on command pattern
    CommandPattern,
    /// Match on context (working directory, project type)
    ContextMatch,
    /// Custom regex pattern
    Regex,
}

/// Severity levels for anti-patterns
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AntiPatternSeverity {
    /// Suggestion to avoid - informational
    Advisory,
    /// Strong recommendation against
    Warning,
    /// Must not do - blocks action
    Blocking,
}

impl AntiPatternSeverity {
    /// Get a human-readable description
    #[must_use]
    pub const fn description(&self) -> &'static str {
        match self {
            Self::Advisory => "advisory (suggestion to avoid)",
            Self::Warning => "warning (strong recommendation against)",
            Self::Blocking => "blocking (must not do)",
        }
    }

    /// Minimum evidence count required for this severity
    #[must_use]
    pub const fn min_evidence_count(&self) -> usize {
        match self {
            Self::Advisory => 1,
            Self::Warning => 2,
            Self::Blocking => 3, // Plus explicit user confirmation
        }
    }
}

// =============================================================================
// Condition and Failure Mode Types
// =============================================================================

/// A condition when an anti-pattern applies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    /// Description of the condition
    pub description: String,
    /// Whether this condition must be true (vs. should be true)
    pub required: bool,
}

impl Condition {
    /// Create a required condition
    pub fn required(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            required: true,
        }
    }

    /// Create an optional/suggested condition
    pub fn suggested(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            required: false,
        }
    }
}

/// A failure mode - what goes wrong when anti-pattern is violated
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureMode {
    /// Description of the failure
    pub description: String,
    /// How many times this failure was observed
    pub observed_count: u32,
    /// Example session showing this failure
    pub example_session: Option<SessionId>,
}

impl FailureMode {
    /// Create a new failure mode
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            observed_count: 1,
            example_session: None,
        }
    }

    /// Increment the observed count
    pub const fn increment(&mut self) {
        self.observed_count = self.observed_count.saturating_add(1);
    }
}

// =============================================================================
// Output Types
// =============================================================================

/// Section for anti-patterns in skill output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AntiPatternSection {
    /// Header text (e.g., "## Avoid / When NOT to use")
    pub header: String,
    /// Formatted anti-patterns
    pub patterns: Vec<FormattedAntiPattern>,
}

impl Default for AntiPatternSection {
    fn default() -> Self {
        Self {
            header: "## Avoid / When NOT to use".to_string(),
            patterns: Vec::new(),
        }
    }
}

impl AntiPatternSection {
    /// Check if section is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }

    /// Render to markdown
    #[must_use]
    pub fn to_markdown(&self) -> String {
        if self.patterns.is_empty() {
            return String::new();
        }

        let mut output = format!("{}\n\n", self.header);
        for pattern in &self.patterns {
            output.push_str(&pattern.to_markdown());
            output.push('\n');
        }
        output
    }
}

/// A formatted anti-pattern for output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedAntiPattern {
    /// The rule statement (e.g., "NEVER X when Y")
    pub rule: String,
    /// Severity level
    pub severity: AntiPatternSeverity,
    /// Conditions when this applies
    pub conditions: Vec<String>,
    /// Why this is wrong
    pub rationale: String,
    /// What to do instead
    pub instead: String,
    /// Evidence summary
    pub evidence_summary: String,
    /// Example from evidence (optional)
    pub example: Option<String>,
}

impl FormattedAntiPattern {
    /// Render to markdown
    #[must_use]
    pub fn to_markdown(&self) -> String {
        let mut output = format!("### {}\n", self.rule);
        output.push_str(&format!(
            "**Severity**: {}\n",
            format!("{:?}", self.severity).to_lowercase()
        ));

        if !self.conditions.is_empty() {
            output.push_str(&format!("**Conditions**: {}\n", self.conditions.join(", ")));
        }

        output.push_str(&format!("**Failure mode**: {}\n", self.rationale));
        output.push_str(&format!("**Instead**: {}\n", self.instead));
        output.push_str(&format!("**Evidence**: {}\n", self.evidence_summary));

        if let Some(example) = &self.example {
            output.push_str(&format!("\n```\n{example}\n```\n"));
        }

        output
    }
}

// =============================================================================
// Clustering Types
// =============================================================================

/// A cluster of similar anti-patterns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AntiPatternCluster {
    /// Cluster identifier
    pub id: String,
    /// Representative pattern for this cluster (centroid)
    pub centroid: AntiPatternContext,
    /// All contexts in this cluster
    pub members: Vec<AntiPatternContext>,
    /// Synthesized conditions derived from cluster analysis
    pub synthesized_conditions: Vec<Condition>,
    /// Similarity score within cluster (0.0 to 1.0)
    pub cohesion: f32,
}

/// Context extracted from an anti-pattern occurrence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AntiPatternContext {
    /// The action that failed
    pub failed_action: String,
    /// Why it failed (if determinable)
    pub failure_reason: Option<String>,
    /// Conditions that made it wrong
    pub conditions: Vec<Condition>,
    /// The correction applied (if any)
    pub correction: Option<String>,
    /// Session where this was observed
    pub session_id: SessionId,
    /// Message index
    pub message_idx: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anti_pattern_id_generation() {
        let id1 = AntiPatternId::generate();
        let id2 = AntiPatternId::generate();
        assert_ne!(id1, id2);
        assert!(id1.0.starts_with("ap-"));
    }

    #[test]
    fn test_anti_pattern_orphan_detection() {
        let rule = NegativeRule::new("Test rule", AntiPatternSeverity::Advisory);
        let mut ap = AntiPattern::new(rule);
        assert!(ap.is_orphaned());

        ap.constrains = Some(PatternId::new("pattern-1"));
        assert!(!ap.is_orphaned());
    }

    #[test]
    fn test_confidence_calculation() {
        let rule = NegativeRule::new("Test rule", AntiPatternSeverity::Warning);
        let mut ap = AntiPattern::new(rule);

        // No evidence = 0 confidence
        assert_eq!(ap.confidence, 0.0);

        // Add evidence
        let evidence = AntiPatternEvidence::new(
            AntiPatternSource::FailureSignal {
                signal_type: FailureSignalType::TestFailure,
            },
            SessionId::new("session-1"),
            FailureIncident {
                description: "test".into(),
                failed_action: "action".into(),
                message_indices: vec![0],
                context: IncidentContext {
                    before: vec![],
                    after: vec![],
                    location: None,
                    tool: None,
                },
                correction: None,
            },
        );
        ap.add_evidence(evidence);
        assert!(ap.confidence > 0.0);
        assert!(ap.confidence < 1.0);
    }

    #[test]
    fn test_severity_min_evidence() {
        assert_eq!(AntiPatternSeverity::Advisory.min_evidence_count(), 1);
        assert_eq!(AntiPatternSeverity::Warning.min_evidence_count(), 2);
        assert_eq!(AntiPatternSeverity::Blocking.min_evidence_count(), 3);
    }

    #[test]
    fn test_formatted_anti_pattern_markdown() {
        let fap = FormattedAntiPattern {
            rule: "NEVER force-push to shared branches".to_string(),
            severity: AntiPatternSeverity::Blocking,
            conditions: vec!["branch has upstream".to_string()],
            rationale: "Overwrites others' work".to_string(),
            instead: "Use git push --force-with-lease".to_string(),
            evidence_summary: "3 sessions, 2 corrections".to_string(),
            example: None,
        };

        let md = fap.to_markdown();
        assert!(md.contains("### NEVER force-push"));
        assert!(md.contains("**Severity**: blocking"));
        assert!(md.contains("**Instead**: Use git push --force-with-lease"));
    }
}
