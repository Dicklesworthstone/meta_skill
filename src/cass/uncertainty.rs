//! Uncertainty Queue and Active Learning
//!
//! When generalization confidence is too low, queue candidates for targeted
//! evidence gathering. Generates 3-7 targeted CASS queries per uncertainty
//! (positive, negative, boundary cases) to close the feedback loop between
//! pattern mining and evidence collection.

use std::collections::VecDeque;
use std::ops::RangeInclusive;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;

use super::mining::ExtractedPattern;
use super::transformation::{
    GeneralizationValidation, InstanceCluster, RefinementCritique, SpecificInstance,
    UncertaintyQueueSink,
};

// =============================================================================
// UNCERTAINTY TYPES
// =============================================================================

/// Unique identifier for an uncertainty item
pub type UncertaintyId = String;

/// Session identifier type
pub type SessionId = String;

/// An item in the uncertainty queue awaiting resolution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UncertaintyItem {
    /// Unique identifier for this uncertainty
    pub id: UncertaintyId,

    /// The pattern candidate that triggered uncertainty
    pub pattern_candidate: ExtractedPattern,

    /// Why confidence is too low
    pub reason: UncertaintyReason,

    /// Current confidence score (0.0-1.0)
    pub confidence: f32,

    /// Minimum confidence threshold for acceptance
    pub threshold: f32,

    /// Generated queries to gather evidence
    pub suggested_queries: Vec<SuggestedQuery>,

    /// Current resolution status
    pub status: UncertaintyStatus,

    /// When this item was created
    pub created_at: DateTime<Utc>,

    /// When this was last updated
    pub updated_at: DateTime<Utc>,

    /// Resolution attempts history
    pub attempts: Vec<ResolutionAttempt>,

    /// Optional refinement critique from LLM
    #[serde(skip_serializing_if = "Option::is_none")]
    pub critique: Option<RefinementCritique>,

    /// Source instance that triggered this uncertainty
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_instance: Option<SourceInstanceInfo>,

    /// Cluster info if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_info: Option<ClusterSummary>,
}

/// Minimal info about source instance (to avoid storing full instance)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInstanceInfo {
    pub session_id: String,
    pub description: String,
    pub tool_signatures: Vec<String>,
}

/// Summary of the cluster that produced this uncertainty
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterSummary {
    pub cluster_id: String,
    pub instance_count: usize,
    pub common_tools: Vec<String>,
    pub common_file_types: Vec<String>,
}

/// Why confidence is insufficient - reasons for uncertainty
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UncertaintyReason {
    /// Not enough examples to generalize
    InsufficientInstances { have: u32, need: u32, variance: f32 },

    /// Examples show high variation
    HighVariance {
        variance_score: f32,
        conflicting_aspects: Vec<String>,
    },

    /// Found examples that contradict the pattern
    CounterExampleFound {
        counter_example: SessionId,
        contradiction: String,
    },

    /// Scope/applicability unclear
    AmbiguousScope {
        possible_scopes: Vec<ScopeCandidate>,
    },

    /// Preconditions unclear
    UnclearPreconditions { candidates: Vec<String> },

    /// Effect boundaries unknown
    UnknownBoundaries {
        dimension: String,
        observed_range: (f32, f32),
    },

    /// LLM critique flagged overgeneralization
    OvergeneralizationFlagged { critique_summary: String },

    /// Multiple conflicting patterns detected
    ConflictingPatterns {
        pattern_ids: Vec<String>,
        conflict_description: String,
    },
}

/// A possible scope for a pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeCandidate {
    pub scope: String,
    pub confidence: f32,
    pub evidence_count: usize,
}

/// Status of uncertainty resolution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum UncertaintyStatus {
    /// Waiting in queue
    Pending,

    /// Currently gathering evidence
    InProgress {
        started_at: DateTime<Utc>,
        queries_completed: u32,
    },

    /// Resolved - pattern accepted
    Resolved {
        new_confidence: f32,
        resolution: Resolution,
        resolved_at: DateTime<Utc>,
    },

    /// Resolved - pattern rejected
    Rejected {
        reason: String,
        rejected_at: DateTime<Utc>,
    },

    /// Stalled - needs human input
    NeedsHuman {
        reason: String,
        escalated_at: DateTime<Utc>,
    },

    /// Expired - aged out
    Expired { expired_at: DateTime<Utc> },
}

/// How an uncertainty was resolved
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Resolution {
    /// Gathered enough evidence to accept
    EvidenceGathered { new_sessions: Vec<SessionId> },

    /// Refined pattern to be more specific
    PatternRefined { new_pattern_id: String },

    /// Split into multiple patterns
    PatternSplit { pattern_ids: Vec<String> },

    /// Human provided clarification
    HumanClarified { annotation: String },
}

/// Record of a resolution attempt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolutionAttempt {
    pub attempted_at: DateTime<Utc>,
    pub queries_executed: Vec<String>,
    pub new_sessions_found: usize,
    pub new_confidence: f32,
    pub outcome: AttemptOutcome,
}

/// Outcome of a resolution attempt
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptOutcome {
    Improved,
    NoChange,
    Degraded,
    Resolved,
    Escalated,
}

// =============================================================================
// SUGGESTED QUERIES
// =============================================================================

/// A suggested query to gather evidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedQuery {
    /// Unique query ID
    pub id: String,

    /// Query type
    pub query_type: QueryType,

    /// Natural language query
    pub query: String,

    /// CASS-formatted query if applicable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cass_query: Option<String>,

    /// What evidence this would provide
    pub expected_evidence: String,

    /// Priority (higher = more valuable)
    pub priority: u32,

    /// Whether this query has been executed
    pub executed: bool,

    /// Results if executed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results: Option<QueryResults>,
}

/// Type of query for evidence gathering
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryType {
    /// Looking for positive examples
    Positive,

    /// Looking for negative examples / counter-examples
    Negative,

    /// Looking for boundary cases
    Boundary,

    /// Looking for scope clarification
    ScopeClarification,

    /// Looking for precondition evidence
    PreconditionEvidence,
}

impl QueryType {
    /// Get a human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Self::Positive => "Find positive examples supporting the pattern",
            Self::Negative => "Find counter-examples or failures",
            Self::Boundary => "Find edge cases at pattern boundaries",
            Self::ScopeClarification => "Clarify where the pattern applies",
            Self::PreconditionEvidence => "Find evidence for preconditions",
        }
    }
}

/// Results from executing a query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResults {
    pub executed_at: DateTime<Utc>,
    pub sessions_found: usize,
    pub session_ids: Vec<SessionId>,
    pub relevance_scores: Vec<f32>,
    pub execution_time_ms: u64,
}

// =============================================================================
// QUERY GENERATION
// =============================================================================

/// Trait for generating targeted queries for uncertainty resolution
pub trait QueryGenerator: Send + Sync {
    /// Generate targeted queries for an uncertainty
    fn generate_queries(
        &self,
        uncertainty: &UncertaintyItem,
        max_queries: usize,
    ) -> Vec<SuggestedQuery>;

    /// Generate positive example queries
    fn generate_positive_queries(
        &self,
        pattern: &ExtractedPattern,
        count: usize,
    ) -> Vec<SuggestedQuery>;

    /// Generate negative/counter-example queries
    fn generate_negative_queries(
        &self,
        pattern: &ExtractedPattern,
        count: usize,
    ) -> Vec<SuggestedQuery>;

    /// Generate boundary case queries
    fn generate_boundary_queries(
        &self,
        pattern: &ExtractedPattern,
        count: usize,
    ) -> Vec<SuggestedQuery>;
}

/// Default query generator using pattern analysis
#[allow(dead_code)] // Field used for configuration, implementation pending
pub struct DefaultQueryGenerator {
    queries_per_type: usize,
}

impl DefaultQueryGenerator {
    pub fn new(queries_per_type: usize) -> Self {
        Self { queries_per_type }
    }
}

impl Default for DefaultQueryGenerator {
    fn default() -> Self {
        Self::new(2)
    }
}

impl QueryGenerator for DefaultQueryGenerator {
    fn generate_queries(
        &self,
        uncertainty: &UncertaintyItem,
        max_queries: usize,
    ) -> Vec<SuggestedQuery> {
        let mut queries = Vec::new();
        let pattern = &uncertainty.pattern_candidate;

        // Distribute queries based on uncertainty reason
        let (positive, negative, boundary) = match &uncertainty.reason {
            UncertaintyReason::InsufficientInstances { .. } => (3, 1, 1),
            UncertaintyReason::HighVariance { .. } => (1, 2, 2),
            UncertaintyReason::CounterExampleFound { .. } => (2, 2, 1),
            UncertaintyReason::AmbiguousScope { .. } => (1, 1, 3),
            UncertaintyReason::UnclearPreconditions { .. } => (2, 1, 2),
            UncertaintyReason::UnknownBoundaries { .. } => (1, 1, 3),
            UncertaintyReason::OvergeneralizationFlagged { .. } => (1, 3, 1),
            UncertaintyReason::ConflictingPatterns { .. } => (2, 2, 1),
        };

        queries.extend(self.generate_positive_queries(pattern, positive.min(max_queries)));
        queries.extend(
            self.generate_negative_queries(pattern, negative.min(max_queries - queries.len())),
        );
        queries.extend(
            self.generate_boundary_queries(pattern, boundary.min(max_queries - queries.len())),
        );

        // Limit to max and assign priorities
        queries.truncate(max_queries);
        for (i, query) in queries.iter_mut().enumerate() {
            query.priority = (max_queries - i) as u32;
        }

        queries
    }

    fn generate_positive_queries(
        &self,
        pattern: &ExtractedPattern,
        count: usize,
    ) -> Vec<SuggestedQuery> {
        let description = pattern
            .description
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("pattern");

        let tags_str = pattern.tags.join(" ");
        let mut queries = Vec::new();

        // Query 1: Direct pattern search
        queries.push(SuggestedQuery {
            id: Uuid::new_v4().to_string(),
            query_type: QueryType::Positive,
            query: format!(
                "Show sessions where I successfully applied {} techniques",
                description
            ),
            cass_query: Some(format!("topic:{} AND outcome:success", tags_str)),
            expected_evidence: "More positive examples supporting the pattern".into(),
            priority: 3,
            executed: false,
            results: None,
        });

        // Query 2: Context-based search
        if queries.len() < count {
            queries.push(SuggestedQuery {
                id: Uuid::new_v4().to_string(),
                query_type: QueryType::Positive,
                query: format!(
                    "Find sessions with similar context where {} was helpful",
                    description
                ),
                cass_query: Some(format!("context:{} AND helpful:true", tags_str)),
                expected_evidence: "Context-similar positive examples".into(),
                priority: 2,
                executed: false,
                results: None,
            });
        }

        queries.truncate(count);
        queries
    }

    fn generate_negative_queries(
        &self,
        pattern: &ExtractedPattern,
        count: usize,
    ) -> Vec<SuggestedQuery> {
        let description = pattern
            .description
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("pattern");

        let tags_str = pattern.tags.join(" ");
        let mut queries = Vec::new();

        // Query 1: Failure cases
        queries.push(SuggestedQuery {
            id: Uuid::new_v4().to_string(),
            query_type: QueryType::Negative,
            query: format!(
                "Show sessions where {} failed or I had to retry",
                description
            ),
            cass_query: Some(format!(
                "topic:{} AND (outcome:failure OR action:retry)",
                tags_str
            )),
            expected_evidence: "Counter-examples or boundary failures".into(),
            priority: 3,
            executed: false,
            results: None,
        });

        // Query 2: Abandoned attempts
        if queries.len() < count {
            queries.push(SuggestedQuery {
                id: Uuid::new_v4().to_string(),
                query_type: QueryType::Negative,
                query: format!("Find sessions where {} approach was abandoned", description),
                cass_query: Some(format!("topic:{} AND abandoned:true", tags_str)),
                expected_evidence: "Cases where pattern didn't work".into(),
                priority: 2,
                executed: false,
                results: None,
            });
        }

        queries.truncate(count);
        queries
    }

    fn generate_boundary_queries(
        &self,
        pattern: &ExtractedPattern,
        count: usize,
    ) -> Vec<SuggestedQuery> {
        let description = pattern
            .description
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("pattern");

        let tags_str = pattern.tags.join(" ");
        let mut queries = Vec::new();

        // Query 1: Edge cases
        queries.push(SuggestedQuery {
            id: Uuid::new_v4().to_string(),
            query_type: QueryType::Boundary,
            query: format!(
                "Show sessions with unusual or edge case {} scenarios",
                description
            ),
            cass_query: Some(format!(
                "topic:{} AND (edge_case:true OR unusual:true)",
                tags_str
            )),
            expected_evidence: "Evidence for where pattern boundaries lie".into(),
            priority: 2,
            executed: false,
            results: None,
        });

        // Query 2: Mixed outcomes
        if queries.len() < count {
            queries.push(SuggestedQuery {
                id: Uuid::new_v4().to_string(),
                query_type: QueryType::Boundary,
                query: format!("Find sessions where {} had partial success", description),
                cass_query: Some(format!("topic:{} AND outcome:partial", tags_str)),
                expected_evidence: "Cases at the boundary of applicability".into(),
                priority: 1,
                executed: false,
                results: None,
            });
        }

        queries.truncate(count);
        queries
    }
}

// =============================================================================
// RESOLUTION
// =============================================================================

/// Result of attempting to resolve an uncertainty
#[derive(Debug, Clone)]
pub enum ResolutionResult {
    /// Resolved successfully
    Resolved(Resolution),

    /// Need more evidence
    NeedsMoreEvidence {
        remaining_queries: Vec<SuggestedQuery>,
    },

    /// Pattern should be rejected
    Reject { reason: String },

    /// Needs human intervention
    Escalate { reason: String },
}

/// Trait for resolving uncertainties with new evidence
pub trait UncertaintyResolver: Send + Sync {
    /// Attempt to resolve an uncertainty with new evidence
    fn attempt_resolution(
        &self,
        uncertainty: &mut UncertaintyItem,
        new_sessions: &[SessionId],
    ) -> ResolutionResult;

    /// Check if uncertainty can be auto-resolved
    fn can_auto_resolve(&self, uncertainty: &UncertaintyItem) -> bool;

    /// Escalate to human if needed
    fn escalate_to_human(&self, uncertainty: &mut UncertaintyItem, reason: &str);
}

/// Default resolver implementation
pub struct DefaultResolver {
    confidence_threshold: f32,
    max_attempts: usize,
}

impl DefaultResolver {
    pub fn new(confidence_threshold: f32, max_attempts: usize) -> Self {
        Self {
            confidence_threshold,
            max_attempts,
        }
    }
}

impl Default for DefaultResolver {
    fn default() -> Self {
        Self::new(0.7, 5)
    }
}

impl UncertaintyResolver for DefaultResolver {
    fn attempt_resolution(
        &self,
        uncertainty: &mut UncertaintyItem,
        new_sessions: &[SessionId],
    ) -> ResolutionResult {
        // Record the attempt
        let new_confidence = self.estimate_new_confidence(uncertainty, new_sessions.len());

        let attempt = ResolutionAttempt {
            attempted_at: Utc::now(),
            queries_executed: uncertainty
                .suggested_queries
                .iter()
                .filter(|q| q.executed)
                .map(|q| q.id.clone())
                .collect(),
            new_sessions_found: new_sessions.len(),
            new_confidence,
            outcome: if new_confidence >= self.confidence_threshold {
                AttemptOutcome::Resolved
            } else if new_confidence > uncertainty.confidence {
                AttemptOutcome::Improved
            } else if new_confidence < uncertainty.confidence {
                AttemptOutcome::Degraded
            } else {
                AttemptOutcome::NoChange
            },
        };
        uncertainty.attempts.push(attempt);
        uncertainty.updated_at = Utc::now();

        // Check if threshold met
        if new_confidence >= self.confidence_threshold {
            return ResolutionResult::Resolved(Resolution::EvidenceGathered {
                new_sessions: new_sessions.to_vec(),
            });
        }

        // Check if max attempts reached
        if uncertainty.attempts.len() >= self.max_attempts {
            return ResolutionResult::Escalate {
                reason: format!(
                    "Max attempts ({}) reached without achieving threshold",
                    self.max_attempts
                ),
            };
        }

        // Get remaining queries
        let remaining: Vec<_> = uncertainty
            .suggested_queries
            .iter()
            .filter(|q| !q.executed)
            .cloned()
            .collect();

        if remaining.is_empty() {
            ResolutionResult::Escalate {
                reason: "All queries exhausted, still below threshold".into(),
            }
        } else {
            ResolutionResult::NeedsMoreEvidence {
                remaining_queries: remaining,
            }
        }
    }

    fn can_auto_resolve(&self, uncertainty: &UncertaintyItem) -> bool {
        // Can auto-resolve if we have unexecuted queries and haven't hit max attempts
        let has_queries = uncertainty.suggested_queries.iter().any(|q| !q.executed);
        let under_max = uncertainty.attempts.len() < self.max_attempts;

        has_queries && under_max
    }

    fn escalate_to_human(&self, uncertainty: &mut UncertaintyItem, reason: &str) {
        uncertainty.status = UncertaintyStatus::NeedsHuman {
            reason: reason.to_string(),
            escalated_at: Utc::now(),
        };
        uncertainty.updated_at = Utc::now();
    }
}

impl DefaultResolver {
    fn estimate_new_confidence(&self, uncertainty: &UncertaintyItem, new_sessions: usize) -> f32 {
        // Simple heuristic: each new session adds confidence
        let session_boost = (new_sessions as f32) * 0.05;
        let base = uncertainty.confidence;

        // Cap at threshold + small margin
        (base + session_boost).min(self.confidence_threshold + 0.1)
    }
}

// =============================================================================
// UNCERTAINTY QUEUE
// =============================================================================

/// Configuration for uncertainty queue behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UncertaintyConfig {
    /// Minimum confidence to skip queue
    pub min_confidence: f32,

    /// Maximum items to hold before forcing resolution
    pub max_queue_size: usize,

    /// Number of queries to generate per uncertainty (min, max)
    pub queries_per_uncertainty: (u32, u32),

    /// How long before expiry (seconds)
    pub expiry_seconds: u64,

    /// Auto-resolve if possible
    pub auto_resolve: bool,
}

impl Default for UncertaintyConfig {
    fn default() -> Self {
        Self {
            min_confidence: 0.7,
            max_queue_size: 100,
            queries_per_uncertainty: (3, 7),
            expiry_seconds: 30 * 24 * 60 * 60, // 30 days
            auto_resolve: true,
        }
    }
}

impl UncertaintyConfig {
    pub fn queries_range(&self) -> RangeInclusive<u32> {
        self.queries_per_uncertainty.0..=self.queries_per_uncertainty.1
    }

    pub fn expiry_duration(&self) -> Duration {
        Duration::from_secs(self.expiry_seconds)
    }
}

/// Statistics about the uncertainty queue
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueueStats {
    pub total_queued: u64,
    pub total_resolved: u64,
    pub total_rejected: u64,
    pub total_expired: u64,
    pub average_resolution_time_secs: f64,
    pub average_queries_needed: f32,
}

/// The uncertainty queue for managing low-confidence patterns
pub struct UncertaintyQueue {
    /// All items in the queue
    items: Arc<RwLock<VecDeque<UncertaintyItem>>>,

    /// Configuration
    config: UncertaintyConfig,

    /// Statistics
    stats: Arc<RwLock<QueueStats>>,

    /// Query generator
    query_generator: Box<dyn QueryGenerator>,
}

impl UncertaintyQueue {
    /// Create a new uncertainty queue with the given configuration
    pub fn new(config: UncertaintyConfig) -> Self {
        Self {
            items: Arc::new(RwLock::new(VecDeque::new())),
            config,
            stats: Arc::new(RwLock::new(QueueStats::default())),
            query_generator: Box::new(DefaultQueryGenerator::default()),
        }
    }

    /// Create with default configuration
    pub fn with_defaults() -> Self {
        Self::new(UncertaintyConfig::default())
    }

    /// Set a custom query generator
    pub fn with_query_generator(mut self, generator: Box<dyn QueryGenerator>) -> Self {
        self.query_generator = generator;
        self
    }

    /// Get the current configuration
    pub fn config(&self) -> &UncertaintyConfig {
        &self.config
    }

    /// Get the current queue length
    pub fn len(&self) -> usize {
        self.items.read().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.items
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty()
    }

    /// Add new uncertainty to queue
    pub fn enqueue(&self, mut item: UncertaintyItem) -> UncertaintyId {
        let id = item.id.clone();

        // Generate queries if not already present
        if item.suggested_queries.is_empty() {
            let max_queries = *self.config.queries_range().end() as usize;
            item.suggested_queries = self.query_generator.generate_queries(&item, max_queries);
        }

        let mut items = self.items.write().unwrap_or_else(|e| e.into_inner());
        items.push_back(item);

        // Update stats
        let mut stats = self.stats.write().unwrap_or_else(|e| e.into_inner());
        stats.total_queued += 1;

        // Check if we need to force resolution
        if items.len() > self.config.max_queue_size {
            // Mark oldest items as needing human intervention
            if let Some(oldest) = items.front_mut() {
                if matches!(oldest.status, UncertaintyStatus::Pending) {
                    oldest.status = UncertaintyStatus::NeedsHuman {
                        reason: "Queue size limit reached".into(),
                        escalated_at: Utc::now(),
                    };
                }
            }
        }

        id
    }

    /// Get next item to process (FIFO)
    pub fn next(&self) -> Option<UncertaintyItem> {
        let items = self.items.read().unwrap_or_else(|e| e.into_inner());
        items
            .iter()
            .find(|item| matches!(item.status, UncertaintyStatus::Pending))
            .cloned()
    }

    /// Get item by ID
    pub fn get(&self, id: &str) -> Option<UncertaintyItem> {
        let items = self.items.read().unwrap_or_else(|e| e.into_inner());
        items.iter().find(|item| item.id == id).cloned()
    }

    /// Update an item in the queue
    pub fn update(&self, updated: UncertaintyItem) -> bool {
        let mut items = self.items.write().unwrap_or_else(|e| e.into_inner());
        if let Some(item) = items.iter_mut().find(|item| item.id == updated.id) {
            *item = updated;
            return true;
        }
        false
    }

    /// Mark item as resolved
    pub fn mark_resolved(&self, id: &str, resolution: Resolution, new_confidence: f32) {
        let mut items = self.items.write().unwrap_or_else(|e| e.into_inner());
        if let Some(item) = items.iter_mut().find(|item| item.id == id) {
            item.status = UncertaintyStatus::Resolved {
                new_confidence,
                resolution,
                resolved_at: Utc::now(),
            };
            item.updated_at = Utc::now();
            item.confidence = new_confidence;

            // Update stats
            let mut stats = self.stats.write().unwrap_or_else(|e| e.into_inner());
            stats.total_resolved += 1;
            let resolution_time = (item.updated_at - item.created_at).num_seconds() as f64;
            let total_resolved = stats.total_resolved as f64;
            stats.average_resolution_time_secs =
                stats.average_resolution_time_secs * (total_resolved - 1.0) / total_resolved
                    + resolution_time / total_resolved;
        }
    }

    /// Mark item as rejected
    pub fn mark_rejected(&self, id: &str, reason: &str) {
        let mut items = self.items.write().unwrap_or_else(|e| e.into_inner());
        if let Some(item) = items.iter_mut().find(|item| item.id == id) {
            item.status = UncertaintyStatus::Rejected {
                reason: reason.to_string(),
                rejected_at: Utc::now(),
            };
            item.updated_at = Utc::now();

            // Update stats
            let mut stats = self.stats.write().unwrap_or_else(|e| e.into_inner());
            stats.total_rejected += 1;
        }
    }

    /// Prune expired items
    pub fn prune_expired(&self) -> Vec<UncertaintyItem> {
        let expiry = self.config.expiry_duration();
        let now = Utc::now();
        let mut expired = Vec::new();

        let mut items = self.items.write().unwrap_or_else(|e| e.into_inner());

        // Find and mark expired items
        for item in items.iter_mut() {
            if matches!(
                item.status,
                UncertaintyStatus::Pending | UncertaintyStatus::InProgress { .. }
            ) {
                let age = now.signed_duration_since(item.created_at);
                if age.to_std().unwrap_or(Duration::ZERO) > expiry {
                    item.status = UncertaintyStatus::Expired { expired_at: now };
                    item.updated_at = now;
                    expired.push(item.clone());
                }
            }
        }

        // Update stats
        if !expired.is_empty() {
            let mut stats = self.stats.write().unwrap_or_else(|e| e.into_inner());
            stats.total_expired += expired.len() as u64;
        }

        expired
    }

    /// Get queue statistics
    pub fn stats(&self) -> QueueStats {
        self.stats.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// List all items with optional status filter
    pub fn list(&self, status_filter: Option<&str>) -> Vec<UncertaintyItem> {
        let items = self.items.read().unwrap_or_else(|e| e.into_inner());

        items
            .iter()
            .filter(|item| {
                if let Some(filter) = status_filter {
                    match (&item.status, filter) {
                        (UncertaintyStatus::Pending, "pending") => true,
                        (UncertaintyStatus::InProgress { .. }, "in_progress") => true,
                        (UncertaintyStatus::Resolved { .. }, "resolved") => true,
                        (UncertaintyStatus::Rejected { .. }, "rejected") => true,
                        (UncertaintyStatus::NeedsHuman { .. }, "needs_human") => true,
                        (UncertaintyStatus::Expired { .. }, "expired") => true,
                        _ => false,
                    }
                } else {
                    true
                }
            })
            .cloned()
            .collect()
    }

    /// Get counts by status
    pub fn counts(&self) -> UncertaintyCounts {
        let items = self.items.read().unwrap_or_else(|e| e.into_inner());

        let mut counts = UncertaintyCounts::default();

        for item in items.iter() {
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
}

/// Counts of uncertainties by status
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UncertaintyCounts {
    pub pending: usize,
    pub in_progress: usize,
    pub resolved: usize,
    pub rejected: usize,
    pub needs_human: usize,
    pub expired: usize,
}

impl UncertaintyCounts {
    pub fn total(&self) -> usize {
        self.pending
            + self.in_progress
            + self.resolved
            + self.rejected
            + self.needs_human
            + self.expired
    }

    pub fn active(&self) -> usize {
        self.pending + self.in_progress
    }
}

// =============================================================================
// INTEGRATION WITH TRANSFORMATION
// =============================================================================

impl UncertaintyQueueSink for UncertaintyQueue {
    fn queue_uncertain(
        &self,
        instance: &SpecificInstance,
        validation: &GeneralizationValidation,
        cluster: &InstanceCluster,
        critique: Option<&RefinementCritique>,
    ) -> Result<String> {
        // Determine reason for uncertainty
        let reason = self.determine_uncertainty_reason(validation, critique);

        // Create extracted pattern from cluster
        let pattern = self.create_pattern_from_cluster(cluster);

        // Create source instance info
        let source_info = SourceInstanceInfo {
            session_id: instance.source.session_id.clone(),
            description: instance.context.description.clone().unwrap_or_default(),
            // Use tags from context as a proxy for tool signatures
            tool_signatures: instance.context.tags.clone(),
        };

        // Derive common file types from cluster instances
        let common_file_types: Vec<String> = cluster
            .instances
            .iter()
            .filter_map(|ci| ci.instance.context.file_type.clone())
            .take(5)
            .collect();

        // Create cluster summary
        let cluster_summary = ClusterSummary {
            cluster_id: cluster.id.clone(),
            instance_count: cluster.instances.len(),
            // Use context_conditions as a proxy for common tools
            common_tools: cluster.context_conditions.iter().take(5).cloned().collect(),
            common_file_types,
        };

        // Build the uncertainty item
        let item_id = Uuid::new_v4().to_string();
        let item = UncertaintyItem {
            id: item_id.clone(),
            pattern_candidate: pattern,
            reason,
            confidence: validation.confidence,
            threshold: self.config.min_confidence,
            suggested_queries: Vec::new(), // Will be generated on enqueue
            status: UncertaintyStatus::Pending,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            attempts: Vec::new(),
            critique: critique.cloned(),
            source_instance: Some(source_info),
            cluster_info: Some(cluster_summary),
        };

        self.enqueue(item);
        Ok(item_id)
    }
}

impl UncertaintyQueue {
    fn determine_uncertainty_reason(
        &self,
        validation: &GeneralizationValidation,
        critique: Option<&RefinementCritique>,
    ) -> UncertaintyReason {
        // Check for LLM critique first
        if let Some(c) = critique {
            if c.flags_overgeneralization {
                return UncertaintyReason::OvergeneralizationFlagged {
                    critique_summary: c.summary.clone(),
                };
            }
        }

        // Check validation metrics
        if validation.coverage < 0.3 {
            return UncertaintyReason::InsufficientInstances {
                have: (validation.coverage * 10.0) as u32,
                need: 5,
                variance: 1.0 - validation.coherence,
            };
        }

        if validation.coherence < 0.4 {
            return UncertaintyReason::HighVariance {
                variance_score: 1.0 - validation.coherence,
                conflicting_aspects: vec![],
            };
        }

        if !validation.counterexamples.is_empty() {
            let ce = &validation.counterexamples[0];
            return UncertaintyReason::CounterExampleFound {
                counter_example: ce.instance_id.clone(),
                contradiction: ce
                    .suggests_refinement
                    .clone()
                    .unwrap_or_else(|| "Pattern contradiction found".into()),
            };
        }

        if validation.specificity < 0.3 {
            return UncertaintyReason::AmbiguousScope {
                possible_scopes: vec![],
            };
        }

        // Default to insufficient instances
        UncertaintyReason::InsufficientInstances {
            have: 2,
            need: 5,
            variance: 0.5,
        }
    }

    fn create_pattern_from_cluster(&self, cluster: &InstanceCluster) -> ExtractedPattern {
        use super::mining::{EvidenceRef, PatternType, WorkflowStep};

        // Extract workflow steps from context_conditions
        let steps: Vec<WorkflowStep> = cluster
            .context_conditions
            .iter()
            .enumerate()
            .map(|(i, condition)| WorkflowStep {
                order: i,
                action: condition.clone(),
                description: format!("Condition {} from cluster", i + 1),
                optional: false,
                conditions: vec![],
            })
            .take(10)
            .collect();

        // Create evidence refs from cluster instances
        let evidence: Vec<EvidenceRef> = cluster
            .instances
            .iter()
            .map(|inst| {
                // Convert distance_to_centroid to similarity (closer = more similar)
                let similarity = 1.0 / (1.0 + inst.distance_to_centroid);
                // Use content truncated as snippet
                let snippet = inst.instance.content.chars().take(100).collect::<String>();
                EvidenceRef {
                    session_id: inst.instance.source.session_id.clone(),
                    message_indices: vec![],
                    relevance: similarity,
                    snippet: Some(snippet),
                }
            })
            .collect();

        // Derive file types from instances
        let file_types: Vec<String> = cluster
            .instances
            .iter()
            .filter_map(|i| i.instance.context.file_type.clone())
            .collect();

        // Create description from first instance content
        let description = cluster.instances.first().map(|i| {
            let truncated: String = i.instance.content.chars().take(200).collect();
            format!(
                "Pattern from cluster {} instances: {}",
                cluster.instances.len(),
                truncated
            )
        });

        ExtractedPattern {
            id: format!("uncertain-{}", Uuid::new_v4()),
            pattern_type: PatternType::WorkflowPattern {
                steps,
                triggers: cluster.context_conditions.clone(),
                outcomes: vec![], // Derived outcomes not available
            },
            evidence,
            confidence: 0.0, // Will be set from validation
            frequency: cluster.instances.len(),
            tags: file_types,
            description,
            taint_label: None,
        }
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_pattern() -> ExtractedPattern {
        use super::super::mining::PatternType;

        ExtractedPattern {
            id: "test-pattern".to_string(),
            pattern_type: PatternType::CommandPattern {
                commands: vec!["git commit".to_string()],
                frequency: 5,
                contexts: vec!["version control".to_string()],
            },
            evidence: vec![],
            confidence: 0.5,
            frequency: 5,
            tags: vec!["git".to_string(), "workflow".to_string()],
            description: Some("Git commit pattern".to_string()),
            taint_label: None,
        }
    }

    fn make_test_item() -> UncertaintyItem {
        UncertaintyItem {
            id: Uuid::new_v4().to_string(),
            pattern_candidate: make_test_pattern(),
            reason: UncertaintyReason::InsufficientInstances {
                have: 2,
                need: 5,
                variance: 0.3,
            },
            confidence: 0.4,
            threshold: 0.7,
            suggested_queries: vec![],
            status: UncertaintyStatus::Pending,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            attempts: vec![],
            critique: None,
            source_instance: None,
            cluster_info: None,
        }
    }

    #[test]
    fn test_queue_enqueue_and_get() {
        let queue = UncertaintyQueue::with_defaults();
        let item = make_test_item();
        let id = item.id.clone();

        let returned_id = queue.enqueue(item);
        assert_eq!(returned_id, id);
        assert_eq!(queue.len(), 1);

        let retrieved = queue.get(&id).unwrap();
        assert_eq!(retrieved.id, id);
    }

    #[test]
    fn test_queue_next_returns_pending() {
        let queue = UncertaintyQueue::with_defaults();

        let item1 = make_test_item();
        let item2 = make_test_item();

        queue.enqueue(item1.clone());
        queue.enqueue(item2.clone());

        let next = queue.next().unwrap();
        assert_eq!(next.id, item1.id);
    }

    #[test]
    fn test_queue_mark_resolved() {
        let queue = UncertaintyQueue::with_defaults();
        let item = make_test_item();
        let id = item.id.clone();

        queue.enqueue(item);
        queue.mark_resolved(
            &id,
            Resolution::EvidenceGathered {
                new_sessions: vec!["session1".to_string()],
            },
            0.85,
        );

        let retrieved = queue.get(&id).unwrap();
        assert!(matches!(
            retrieved.status,
            UncertaintyStatus::Resolved { .. }
        ));
        assert_eq!(retrieved.confidence, 0.85);
    }

    #[test]
    fn test_queue_mark_rejected() {
        let queue = UncertaintyQueue::with_defaults();
        let item = make_test_item();
        let id = item.id.clone();

        queue.enqueue(item);
        queue.mark_rejected(&id, "Pattern too vague");

        let retrieved = queue.get(&id).unwrap();
        assert!(matches!(
            retrieved.status,
            UncertaintyStatus::Rejected { .. }
        ));
    }

    #[test]
    fn test_queue_counts() {
        let queue = UncertaintyQueue::with_defaults();

        queue.enqueue(make_test_item());
        queue.enqueue(make_test_item());

        let counts = queue.counts();
        assert_eq!(counts.pending, 2);
        assert_eq!(counts.active(), 2);
    }

    #[test]
    fn test_queue_list_with_filter() {
        let queue = UncertaintyQueue::with_defaults();

        let item1 = make_test_item();
        let id1 = item1.id.clone();
        queue.enqueue(item1);

        let item2 = make_test_item();
        queue.enqueue(item2);

        queue.mark_rejected(&id1, "test");

        let pending = queue.list(Some("pending"));
        assert_eq!(pending.len(), 1);

        let rejected = queue.list(Some("rejected"));
        assert_eq!(rejected.len(), 1);
    }

    #[test]
    fn test_query_generator_generates_queries() {
        let generator = DefaultQueryGenerator::default();
        let item = make_test_item();

        let queries = generator.generate_queries(&item, 5);
        assert!(!queries.is_empty());
        assert!(queries.len() <= 5);

        // Check query types are diverse
        let types: std::collections::HashSet<_> = queries.iter().map(|q| q.query_type).collect();
        assert!(types.len() >= 2);
    }

    #[test]
    fn test_query_generator_positive_queries() {
        let generator = DefaultQueryGenerator::default();
        let pattern = make_test_pattern();

        let queries = generator.generate_positive_queries(&pattern, 2);
        assert!(!queries.is_empty());

        for q in &queries {
            assert_eq!(q.query_type, QueryType::Positive);
            assert!(!q.executed);
        }
    }

    #[test]
    fn test_query_generator_negative_queries() {
        let generator = DefaultQueryGenerator::default();
        let pattern = make_test_pattern();

        let queries = generator.generate_negative_queries(&pattern, 2);
        assert!(!queries.is_empty());

        for q in &queries {
            assert_eq!(q.query_type, QueryType::Negative);
        }
    }

    #[test]
    fn test_query_generator_boundary_queries() {
        let generator = DefaultQueryGenerator::default();
        let pattern = make_test_pattern();

        let queries = generator.generate_boundary_queries(&pattern, 2);
        assert!(!queries.is_empty());

        for q in &queries {
            assert_eq!(q.query_type, QueryType::Boundary);
        }
    }

    #[test]
    fn test_resolver_can_auto_resolve() {
        let resolver = DefaultResolver::default();
        let mut item = make_test_item();

        // Add some queries
        item.suggested_queries = vec![SuggestedQuery {
            id: "q1".to_string(),
            query_type: QueryType::Positive,
            query: "test".to_string(),
            cass_query: None,
            expected_evidence: "test".to_string(),
            priority: 1,
            executed: false,
            results: None,
        }];

        assert!(resolver.can_auto_resolve(&item));

        // Mark query as executed
        item.suggested_queries[0].executed = true;
        assert!(!resolver.can_auto_resolve(&item));
    }

    #[test]
    fn test_resolver_attempt_resolution_success() {
        let resolver = DefaultResolver::new(0.7, 5);
        let mut item = make_test_item();
        item.confidence = 0.65;

        // Many new sessions should push it over threshold
        let new_sessions: Vec<SessionId> = (0..5).map(|i| format!("session-{}", i)).collect();

        let result = resolver.attempt_resolution(&mut item, &new_sessions);

        assert!(matches!(result, ResolutionResult::Resolved(_)));
        assert!(!item.attempts.is_empty());
    }

    #[test]
    fn test_resolver_escalate_on_max_attempts() {
        let resolver = DefaultResolver::new(0.9, 2);
        let mut item = make_test_item();
        item.confidence = 0.3;

        // First attempt
        let _ = resolver.attempt_resolution(&mut item, &[]);

        // Second attempt
        let _ = resolver.attempt_resolution(&mut item, &[]);

        // Third attempt should escalate
        let result = resolver.attempt_resolution(&mut item, &[]);

        assert!(matches!(result, ResolutionResult::Escalate { .. }));
    }

    #[test]
    fn test_uncertainty_reason_serialization() {
        let reason = UncertaintyReason::InsufficientInstances {
            have: 2,
            need: 5,
            variance: 0.3,
        };

        let json = serde_json::to_string(&reason).unwrap();
        let parsed: UncertaintyReason = serde_json::from_str(&json).unwrap();

        match parsed {
            UncertaintyReason::InsufficientInstances { have, need, .. } => {
                assert_eq!(have, 2);
                assert_eq!(need, 5);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_uncertainty_status_serialization() {
        let status = UncertaintyStatus::InProgress {
            started_at: Utc::now(),
            queries_completed: 3,
        };

        let json = serde_json::to_string(&status).unwrap();
        let parsed: UncertaintyStatus = serde_json::from_str(&json).unwrap();

        assert!(matches!(
            parsed,
            UncertaintyStatus::InProgress {
                queries_completed: 3,
                ..
            }
        ));
    }

    #[test]
    fn test_query_type_description() {
        assert_eq!(
            QueryType::Positive.description(),
            "Find positive examples supporting the pattern"
        );
        assert_eq!(
            QueryType::Negative.description(),
            "Find counter-examples or failures"
        );
        assert_eq!(
            QueryType::Boundary.description(),
            "Find edge cases at pattern boundaries"
        );
    }

    #[test]
    fn test_uncertainty_config_default() {
        let config = UncertaintyConfig::default();
        assert_eq!(config.min_confidence, 0.7);
        assert_eq!(config.max_queue_size, 100);
        assert_eq!(config.queries_per_uncertainty, (3, 7));
    }

    #[test]
    fn test_queue_stats() {
        let queue = UncertaintyQueue::with_defaults();

        let item = make_test_item();
        let id = item.id.clone();
        queue.enqueue(item);

        queue.mark_resolved(
            &id,
            Resolution::EvidenceGathered {
                new_sessions: vec![],
            },
            0.8,
        );

        let stats = queue.stats();
        assert_eq!(stats.total_queued, 1);
        assert_eq!(stats.total_resolved, 1);
    }

    #[test]
    fn test_resolution_serialization() {
        let resolution = Resolution::EvidenceGathered {
            new_sessions: vec!["s1".to_string(), "s2".to_string()],
        };

        let json = serde_json::to_string(&resolution).unwrap();
        let parsed: Resolution = serde_json::from_str(&json).unwrap();

        match parsed {
            Resolution::EvidenceGathered { new_sessions } => {
                assert_eq!(new_sessions.len(), 2);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_scope_candidate_serialization() {
        let candidate = ScopeCandidate {
            scope: "rust files".to_string(),
            confidence: 0.8,
            evidence_count: 5,
        };

        let json = serde_json::to_string(&candidate).unwrap();
        assert!(json.contains("rust files"));

        let parsed: ScopeCandidate = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.scope, "rust files");
    }
}
