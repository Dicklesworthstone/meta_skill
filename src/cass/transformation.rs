//! Specific-to-General Transformation Pipeline
//!
//! Transforms extracted specific instances into reusable, generalized patterns.
//! This is the core intellectual innovation: extracting universal patterns
//! ("inner truths") from specific instances while avoiding over-generalization.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::{MsError, Result};
use crate::search::embeddings::HashEmbedder;

use super::client::{CassClient, SessionMatch};
use super::mining::{ExtractedPattern, PatternType};

// =============================================================================
// Core Types
// =============================================================================

/// A specific instance extracted from a session that can be generalized
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecificInstance {
    /// Unique identifier
    pub id: String,
    /// The actual content/pattern observed
    pub content: String,
    /// Context in which this instance appeared
    pub context: InstanceContext,
    /// Source session and location
    pub source: InstanceSource,
    /// Whether this is a positive example or counter-example
    pub is_counter_example: bool,
}

/// Context surrounding a specific instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceContext {
    /// File type (e.g., "rust", "typescript", "python")
    pub file_type: Option<String>,
    /// Project type or framework
    pub project_type: Option<String>,
    /// Tags describing the context
    pub tags: Vec<String>,
    /// Free-form context description
    pub description: Option<String>,
}

/// Source information for tracing back to original session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceSource {
    /// Session ID where found
    pub session_id: String,
    /// Message indices relevant to this instance
    pub message_indices: Vec<usize>,
    /// Timestamp of observation
    pub observed_at: Option<String>,
}

/// Structural pattern extracted from an instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuralPattern {
    /// Detected file type
    pub file_type: String,
    /// Code pattern signature
    pub code_pattern: CodePatternSignature,
    /// Problem class being addressed
    pub problem_class: String,
    /// Solution approach taken
    pub solution_approach: SolutionApproach,
}

/// Signature of a code pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodePatternSignature {
    /// Pattern category (e.g., "error_handling", "initialization", "cleanup")
    pub category: String,
    /// Key tokens/identifiers involved
    pub key_tokens: Vec<String>,
    /// Structural features (e.g., "uses_match", "async_await")
    pub features: Vec<String>,
}

impl CodePatternSignature {
    /// Generate a searchable signature string
    pub fn signature(&self) -> String {
        format!(
            "{} {} {}",
            self.category,
            self.key_tokens.join(" "),
            self.features.join(" ")
        )
    }
}

/// Approach taken to solve a problem
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolutionApproach {
    /// High-level strategy
    pub strategy: String,
    /// Keywords describing the approach
    pub keywords: Vec<String>,
    /// Tools or patterns used
    pub tools_used: Vec<String>,
}

impl SolutionApproach {
    /// Get keywords for search
    pub fn keywords(&self) -> &[String] {
        &self.keywords
    }
}

/// A cluster of similar instances grouped by context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceCluster {
    /// Cluster identifier
    pub id: String,
    /// Instances in this cluster
    pub instances: Vec<ClusteredInstance>,
    /// Common context conditions
    pub context_conditions: Vec<String>,
    /// Centroid embedding (for similarity)
    pub centroid: Option<Vec<f32>>,
    /// Cluster coherence score
    pub coherence: f32,
}

/// An instance with its cluster membership info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusteredInstance {
    /// The original instance
    pub instance: SpecificInstance,
    /// Distance to cluster centroid
    pub distance_to_centroid: f32,
    /// Embedding vector
    pub embedding: Vec<f32>,
}

impl ClusteredInstance {
    /// Convert to example format for general pattern
    pub fn to_example(&self) -> String {
        self.instance.content.chars().take(200).collect()
    }
}

/// Common elements extracted from a cluster (the "inner truth")
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommonElements {
    /// Abstracted description of the pattern
    pub abstracted_description: String,
    /// Invariants that hold across all instances
    pub invariants: Vec<String>,
    /// Context conditions when this applies
    pub context_conditions: Vec<String>,
    /// Placeholders identified (things that vary)
    pub placeholders: Vec<Placeholder>,
    /// Confidence in the extraction
    pub extraction_confidence: f32,
}

/// A placeholder representing variable parts of a pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Placeholder {
    /// Name of the placeholder
    pub name: String,
    /// Description of what it represents
    pub description: String,
    /// Observed values
    pub observed_values: Vec<String>,
    /// Constraints on valid values
    pub constraints: Vec<String>,
}

// =============================================================================
// Generalization Output
// =============================================================================

/// A generalized pattern derived from specific instances
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralPattern {
    /// The core principle or rule
    pub principle: String,
    /// Example usages
    pub examples: Vec<String>,
    /// When this pattern applies
    pub applicability: Vec<String>,
    /// Confidence in the generalization
    pub confidence: f32,
    /// Number of source instances
    pub source_instances: usize,
    /// Counter-examples and when NOT to apply
    pub avoid_when: Vec<String>,
    /// Evidence supporting this pattern
    pub evidence: PatternEvidence,
}

/// Evidence supporting a general pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternEvidence {
    /// Sessions where pattern was observed
    pub session_ids: Vec<String>,
    /// Validation metrics
    pub validation: GeneralizationValidation,
    /// Critique results if LLM refinement was used
    pub critique: Option<RefinementCritique>,
}

// =============================================================================
// Validation
// =============================================================================

/// Validation metrics for a generalization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralizationValidation {
    /// How many instances fit the generalization (0.0-1.0)
    pub coverage: f32,
    /// How well it predicts outcomes given applicability (0.0-1.0)
    pub predictive_power: f32,
    /// Semantic coherence score (0.0-1.0)
    pub coherence: f32,
    /// Inverse of overbreadth - prevents platitudes (0.0-1.0)
    pub specificity: f32,
    /// Combined confidence score (0.0-1.0)
    pub confidence: f32,
    /// Identified counter-examples
    pub counterexamples: Vec<CounterExample>,
}

impl GeneralizationValidation {
    /// Compute validation metrics for a pattern against instances
    pub fn compute(
        common: &CommonElements,
        instances: &[ClusteredInstance],
        all_instances: &[ClusteredInstance],
    ) -> Self {
        if instances.is_empty() {
            return Self::empty();
        }

        // Coverage: what fraction of all instances does this generalization apply to?
        let coverage = instances.len() as f32 / all_instances.len().max(1) as f32;

        // Predictive power: how consistent are the outcomes within the cluster?
        // For now, use cluster coherence as proxy
        let predictive_power = Self::compute_predictive_power(instances);

        // Coherence: semantic coherence of the abstracted description
        let coherence = common.extraction_confidence;

        // Specificity: penalize overly broad patterns
        let specificity = Self::compute_specificity(coverage, coherence, &common.invariants);

        // Combined confidence with weights from spec
        let confidence =
            0.35 * coverage + 0.35 * predictive_power + 0.20 * coherence + 0.10 * specificity;

        // Identify counter-examples (instances that don't fit well)
        let counterexamples = Self::identify_counterexamples(instances, all_instances);

        Self {
            coverage,
            predictive_power,
            coherence,
            specificity,
            confidence,
            counterexamples,
        }
    }

    fn empty() -> Self {
        Self {
            coverage: 0.0,
            predictive_power: 0.0,
            coherence: 0.0,
            specificity: 0.0,
            confidence: 0.0,
            counterexamples: vec![],
        }
    }

    fn compute_predictive_power(instances: &[ClusteredInstance]) -> f32 {
        if instances.is_empty() {
            return 0.0;
        }

        // Use average distance to centroid as inverse proxy for consistency
        let avg_distance: f32 = instances
            .iter()
            .map(|i| i.distance_to_centroid)
            .sum::<f32>()
            / instances.len() as f32;

        // Convert distance to similarity-like score (closer = better)
        // Assuming distances are normalized, 1.0 - avg_distance gives consistency
        (1.0 - avg_distance).clamp(0.0, 1.0)
    }

    fn compute_specificity(coverage: f32, coherence: f32, invariants: &[String]) -> f32 {
        // High coverage + low coherence = probably a platitude
        if coverage > 0.95 && coherence < 0.5 {
            return 0.3; // Penalty for overbreadth
        }

        // More invariants = more specific
        let invariant_bonus = (invariants.len() as f32 * 0.05).min(0.2);

        // Slight preference for more specific patterns
        let base_specificity = 1.0 - (coverage * 0.2);

        (base_specificity + invariant_bonus).clamp(0.0, 1.0)
    }

    fn identify_counterexamples(
        cluster_instances: &[ClusteredInstance],
        all_instances: &[ClusteredInstance],
    ) -> Vec<CounterExample> {
        let mut counterexamples = Vec::new();
        let cluster_ids: std::collections::HashSet<_> =
            cluster_instances.iter().map(|i| &i.instance.id).collect();

        for instance in all_instances {
            if cluster_ids.contains(&instance.instance.id) {
                continue;
            }

            // Instance not in cluster - check if it's a relevant counterexample
            if instance.distance_to_centroid < 0.5 {
                // Close but not included - interesting counterexample
                counterexamples.push(CounterExample {
                    instance_id: instance.instance.id.clone(),
                    failure_reason: CounterExampleReason::PatternNotApplicable,
                    missing_precondition: Some("Context conditions not met".to_string()),
                    suggests_refinement: None,
                });
            }
        }

        // Limit to most relevant counterexamples
        counterexamples.truncate(5);
        counterexamples
    }
}

/// A counterexample captures why a pattern didn't apply or failed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterExample {
    /// ID of the instance
    pub instance_id: String,
    /// Why the pattern didn't apply
    pub failure_reason: CounterExampleReason,
    /// What precondition was missing
    pub missing_precondition: Option<String>,
    /// Suggested refinement to pattern
    pub suggests_refinement: Option<String>,
}

/// Reason why a counterexample doesn't fit the pattern
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CounterExampleReason {
    /// Preconditions not met
    PatternNotApplicable,
    /// Applied but wrong outcome
    OutcomeMismatch,
    /// Similar surface but different underlying situation
    DifferentContext,
}

// =============================================================================
// LLM Refinement (Pluggable)
// =============================================================================

/// Trait for LLM-assisted refinement of generalizations
pub trait GeneralizationRefiner: Send + Sync {
    /// Critique a candidate generalization for overgeneralization
    fn critique(
        &self,
        common: &CommonElements,
        cluster: &InstanceCluster,
    ) -> Result<RefinementCritique>;
}

/// Result of LLM critique
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefinementCritique {
    /// Summary of the critique
    pub summary: String,
    /// Whether overgeneralization was detected
    pub flags_overgeneralization: bool,
    /// Suggested refinements
    pub suggested_refinements: Vec<String>,
    /// Confidence in the critique
    pub critique_confidence: f32,
}

// =============================================================================
// Uncertainty Queue Integration
// =============================================================================

/// Trait for uncertainty queue integration (implemented separately)
pub trait UncertaintyQueueSink: Send + Sync {
    /// Queue an uncertain generalization for human review
    fn queue_uncertain(
        &self,
        instance: &SpecificInstance,
        validation: &GeneralizationValidation,
        cluster: &InstanceCluster,
        critique: Option<&RefinementCritique>,
    ) -> Result<String>;
}

/// Item to be queued for uncertainty review
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UncertaintyItem {
    /// Unique identifier
    pub id: String,
    /// The original instance that couldn't be generalized
    pub instance: SpecificInstance,
    /// Validation that showed low confidence
    pub validation: GeneralizationValidation,
    /// Best cluster found
    pub best_cluster: Option<InstanceCluster>,
    /// Critique if available
    pub critique: Option<RefinementCritique>,
    /// Reason for uncertainty
    pub reason: UncertaintyReason,
    /// Timestamp
    pub created_at: String,
}

/// Why an item was queued for uncertainty
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UncertaintyReason {
    /// Confidence below threshold
    LowConfidence { confidence: f32, threshold: f32 },
    /// Not enough similar instances found
    InsufficientInstances { found: usize, required: usize },
    /// LLM critique flagged overgeneralization
    OvergeneralizationDetected { summary: String },
}

/// Null implementation of uncertainty queue for testing
pub struct NullUncertaintyQueue;

impl UncertaintyQueueSink for NullUncertaintyQueue {
    fn queue_uncertain(
        &self,
        _instance: &SpecificInstance,
        _validation: &GeneralizationValidation,
        _cluster: &InstanceCluster,
        _critique: Option<&RefinementCritique>,
    ) -> Result<String> {
        Ok("null-queue-item".to_string())
    }
}

// =============================================================================
// Main Transformer
// =============================================================================

/// Configuration for the transformer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformerConfig {
    /// Minimum instances required to generalize
    pub min_instances: usize,
    /// Minimum confidence threshold for generalization
    pub confidence_threshold: f32,
    /// Maximum instances to search for similarity
    pub max_search_results: usize,
    /// Embedding dimension for similarity
    pub embedding_dim: usize,
    /// Clustering distance threshold
    pub cluster_threshold: f32,
}

impl Default for TransformerConfig {
    fn default() -> Self {
        Self {
            min_instances: 3,
            confidence_threshold: 0.7,
            max_search_results: 100,
            embedding_dim: 384,
            cluster_threshold: 0.5,
        }
    }
}

/// The main transformer that converts specific instances to general patterns
pub struct SpecificToGeneralTransformer {
    cass: CassClient,
    embedder: HashEmbedder,
    uncertainty_queue: Box<dyn UncertaintyQueueSink>,
    refiner: Option<Box<dyn GeneralizationRefiner>>,
    config: TransformerConfig,
}

impl SpecificToGeneralTransformer {
    /// Create a new transformer with default settings
    pub fn new(cass: CassClient) -> Self {
        let config = TransformerConfig::default();
        Self {
            cass,
            embedder: HashEmbedder::new(config.embedding_dim),
            uncertainty_queue: Box::new(NullUncertaintyQueue),
            refiner: None,
            config,
        }
    }

    /// Create with custom configuration
    pub fn with_config(cass: CassClient, config: TransformerConfig) -> Self {
        Self {
            cass,
            embedder: HashEmbedder::new(config.embedding_dim),
            uncertainty_queue: Box::new(NullUncertaintyQueue),
            refiner: None,
            config,
        }
    }

    /// Set the uncertainty queue sink
    pub fn with_uncertainty_queue(mut self, queue: Box<dyn UncertaintyQueueSink>) -> Self {
        self.uncertainty_queue = queue;
        self
    }

    /// Set the LLM refiner
    pub fn with_refiner(mut self, refiner: Box<dyn GeneralizationRefiner>) -> Self {
        self.refiner = Some(refiner);
        self
    }

    /// Transform a specific instance into a general pattern
    pub fn transform(&self, instance: &SpecificInstance) -> Result<GeneralPattern> {
        // Step 1: Extract structural features
        let structure = self.extract_structure(instance)?;

        // Step 2: Find similar instances in CASS
        let similar = self.find_similar_instances(&structure)?;

        if similar.len() < self.config.min_instances {
            let validation = GeneralizationValidation::empty();
            let cluster = InstanceCluster {
                id: "insufficient".to_string(),
                instances: vec![],
                context_conditions: vec![],
                centroid: None,
                coherence: 0.0,
            };
            self.queue_uncertainty(instance, &validation, &cluster, None)?;
            return Err(MsError::MiningFailed(format!(
                "Insufficient instances for generalization: found {}, need {}",
                similar.len(),
                self.config.min_instances
            )));
        }

        // Step 3: Cluster by context
        let clusters = self.cluster_by_context(&similar)?;
        let primary_cluster = clusters
            .into_iter()
            .max_by(|a, b| {
                a.instances.len().cmp(&b.instances.len()).then_with(|| {
                    a.coherence
                        .partial_cmp(&b.coherence)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
            })
            .ok_or_else(|| MsError::MiningFailed("No valid clusters found".to_string()))?;

        // Step 4: Extract common elements (the "inner truth")
        let common = self.extract_common_elements(&primary_cluster)?;

        // Step 5: Validate generalization
        let validation =
            GeneralizationValidation::compute(&common, &primary_cluster.instances, &similar);

        if validation.confidence < self.config.confidence_threshold {
            self.queue_uncertainty(instance, &validation, &primary_cluster, None)?;
            return Err(MsError::MiningFailed(format!(
                "Generalization confidence too low: {:.2} < {:.2}",
                validation.confidence, self.config.confidence_threshold
            )));
        }

        // Step 6: Optional refinement/critique (LLM-assisted if configured)
        let critique = if let Some(ref refiner) = self.refiner {
            let critique = refiner.critique(&common, &primary_cluster)?;
            if critique.flags_overgeneralization {
                self.queue_uncertainty(instance, &validation, &primary_cluster, Some(&critique))?;
                return Err(MsError::MiningFailed(format!(
                    "Generalization critique failed: {}",
                    critique.summary
                )));
            }
            Some(critique)
        } else {
            None
        };

        // Step 7: Generate general pattern
        Ok(GeneralPattern {
            principle: common.abstracted_description,
            examples: primary_cluster
                .instances
                .iter()
                .take(3)
                .map(|i| i.to_example())
                .collect(),
            applicability: common.context_conditions,
            confidence: validation.confidence,
            source_instances: similar.len(),
            avoid_when: validation
                .counterexamples
                .iter()
                .filter_map(|c| c.missing_precondition.clone())
                .collect(),
            evidence: PatternEvidence {
                session_ids: similar
                    .iter()
                    .map(|i| i.instance.source.session_id.clone())
                    .collect(),
                validation,
                critique,
            },
        })
    }

    /// Extract structural features from an instance
    fn extract_structure(&self, instance: &SpecificInstance) -> Result<StructuralPattern> {
        let file_type = self.detect_file_type(&instance.context);
        let code_pattern = self.extract_code_pattern(&instance.content);
        let problem_class = self.classify_problem(&instance.content);
        let solution_approach = self.extract_solution(&instance.content);

        Ok(StructuralPattern {
            file_type,
            code_pattern,
            problem_class,
            solution_approach,
        })
    }

    /// Detect file type from context
    fn detect_file_type(&self, context: &InstanceContext) -> String {
        context
            .file_type
            .clone()
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// Extract code pattern signature
    fn extract_code_pattern(&self, content: &str) -> CodePatternSignature {
        let mut category = "general".to_string();
        let mut key_tokens = Vec::new();
        let mut features = Vec::new();

        let content_lower = content.to_lowercase();

        // Detect category from content patterns
        if content_lower.contains("error") || content_lower.contains("err") {
            category = "error_handling".to_string();
            features.push("handles_errors".to_string());
        } else if content_lower.contains("async") || content_lower.contains("await") {
            category = "async_operation".to_string();
            features.push("async_await".to_string());
        } else if content_lower.contains("test") {
            category = "testing".to_string();
            features.push("test_code".to_string());
        } else if content_lower.contains("init") || content_lower.contains("new") {
            category = "initialization".to_string();
            features.push("constructor".to_string());
        }

        // Extract key tokens (simple tokenization)
        for word in content.split_whitespace() {
            let clean = word.trim_matches(|c: char| !c.is_alphanumeric());
            if clean.len() > 3 && clean.chars().all(|c| c.is_alphanumeric() || c == '_') {
                if !key_tokens.contains(&clean.to_string()) && key_tokens.len() < 10 {
                    key_tokens.push(clean.to_string());
                }
            }
        }

        // Detect Rust-specific features
        if content.contains("match") {
            features.push("uses_match".to_string());
        }
        if content.contains("Result<") || content.contains("-> Result") {
            features.push("returns_result".to_string());
        }
        if content.contains("Option<") {
            features.push("uses_option".to_string());
        }
        if content.contains("impl ") {
            features.push("impl_block".to_string());
        }
        if content.contains("trait ") {
            features.push("trait_definition".to_string());
        }

        CodePatternSignature {
            category,
            key_tokens,
            features,
        }
    }

    /// Classify the problem being addressed
    fn classify_problem(&self, content: &str) -> String {
        let content_lower = content.to_lowercase();

        if content_lower.contains("fix") || content_lower.contains("bug") {
            "bug_fix".to_string()
        } else if content_lower.contains("add") || content_lower.contains("implement") {
            "feature_implementation".to_string()
        } else if content_lower.contains("refactor") || content_lower.contains("clean") {
            "refactoring".to_string()
        } else if content_lower.contains("optimize") || content_lower.contains("performance") {
            "optimization".to_string()
        } else if content_lower.contains("test") {
            "testing".to_string()
        } else if content_lower.contains("document") || content_lower.contains("comment") {
            "documentation".to_string()
        } else {
            "general".to_string()
        }
    }

    /// Extract solution approach from content
    fn extract_solution(&self, content: &str) -> SolutionApproach {
        let content_lower = content.to_lowercase();
        let mut keywords = Vec::new();
        let mut tools_used = Vec::new();

        // Extract strategy
        let strategy = if content_lower.contains("replace") {
            keywords.push("replacement".to_string());
            "replacement".to_string()
        } else if content_lower.contains("wrap") {
            keywords.push("wrapper".to_string());
            "wrapping".to_string()
        } else if content_lower.contains("extend") {
            keywords.push("extension".to_string());
            "extension".to_string()
        } else if content_lower.contains("extract") {
            keywords.push("extraction".to_string());
            "extraction".to_string()
        } else {
            "modification".to_string()
        };

        // Detect tools/patterns used
        if content_lower.contains("regex") {
            tools_used.push("regex".to_string());
        }
        if content_lower.contains("iterator") || content_lower.contains("iter()") {
            tools_used.push("iterators".to_string());
        }
        if content_lower.contains("closure") || content.contains("|") {
            tools_used.push("closures".to_string());
        }
        if content_lower.contains("macro") {
            tools_used.push("macros".to_string());
        }

        // Add general keywords from content
        for word in ["pattern", "struct", "enum", "function", "method", "module"] {
            if content_lower.contains(word) {
                keywords.push(word.to_string());
            }
        }

        SolutionApproach {
            strategy,
            keywords,
            tools_used,
        }
    }

    /// Find similar instances in CASS
    fn find_similar_instances(
        &self,
        pattern: &StructuralPattern,
    ) -> Result<Vec<ClusteredInstance>> {
        let query = format!(
            "{} {} {} {}",
            pattern.file_type,
            pattern.code_pattern.signature(),
            pattern.problem_class,
            pattern.solution_approach.keywords().join(" ")
        );

        let matches = self.cass.search(&query, self.config.max_search_results)?;

        // Convert matches to clustered instances
        let instances: Vec<ClusteredInstance> = matches
            .into_iter()
            .filter_map(|m| self.session_match_to_instance(m, pattern))
            .collect();

        Ok(instances)
    }

    /// Convert a CASS session match to a clustered instance
    fn session_match_to_instance(
        &self,
        m: SessionMatch,
        pattern: &StructuralPattern,
    ) -> Option<ClusteredInstance> {
        let content = m.snippet.unwrap_or_default();
        if content.is_empty() {
            return None;
        }

        let embedding = self.embedder.embed(&content);
        let instance = SpecificInstance {
            id: m.session_id.clone(),
            content,
            context: InstanceContext {
                file_type: Some(pattern.file_type.clone()),
                project_type: m.project,
                tags: vec![],
                description: None,
            },
            source: InstanceSource {
                session_id: m.session_id,
                message_indices: vec![],
                observed_at: m.timestamp,
            },
            is_counter_example: false,
        };

        Some(ClusteredInstance {
            instance,
            distance_to_centroid: 0.0, // Will be computed during clustering
            embedding,
        })
    }

    /// Cluster instances by context similarity
    fn cluster_by_context(&self, instances: &[ClusteredInstance]) -> Result<Vec<InstanceCluster>> {
        if instances.is_empty() {
            return Ok(vec![]);
        }

        // Simple single-linkage clustering based on embedding similarity
        let mut clusters: Vec<InstanceCluster> = Vec::new();
        let mut assigned: Vec<bool> = vec![false; instances.len()];

        for (i, instance) in instances.iter().enumerate() {
            if assigned[i] {
                continue;
            }

            // Start a new cluster with this instance
            let mut cluster_instances = vec![instance.clone()];
            assigned[i] = true;

            // Find all instances similar enough to join this cluster
            for (j, other) in instances.iter().enumerate() {
                if assigned[j] {
                    continue;
                }

                let similarity = self
                    .embedder
                    .similarity(&instance.embedding, &other.embedding);
                if similarity > self.config.cluster_threshold {
                    cluster_instances.push(other.clone());
                    assigned[j] = true;
                }
            }

            // Compute cluster centroid
            let centroid = self.compute_centroid(&cluster_instances);

            // Update distances to centroid
            let cluster_instances: Vec<ClusteredInstance> = cluster_instances
                .into_iter()
                .map(|mut ci| {
                    ci.distance_to_centroid =
                        1.0 - self.embedder.similarity(&ci.embedding, &centroid);
                    ci
                })
                .collect();

            // Compute coherence
            let coherence = self.compute_cluster_coherence(&cluster_instances);

            // Extract context conditions
            let context_conditions = self.extract_context_conditions(&cluster_instances);

            clusters.push(InstanceCluster {
                id: format!("cluster-{}", clusters.len()),
                instances: cluster_instances,
                context_conditions,
                centroid: Some(centroid),
                coherence,
            });
        }

        Ok(clusters)
    }

    /// Compute centroid embedding for a cluster
    fn compute_centroid(&self, instances: &[ClusteredInstance]) -> Vec<f32> {
        if instances.is_empty() {
            return vec![0.0; self.config.embedding_dim];
        }

        let dim = instances[0].embedding.len();
        let mut centroid = vec![0.0; dim];

        for instance in instances {
            for (i, &v) in instance.embedding.iter().enumerate() {
                centroid[i] += v;
            }
        }

        let n = instances.len() as f32;
        for v in &mut centroid {
            *v /= n;
        }

        // Normalize
        let norm: f32 = centroid.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut centroid {
                *v /= norm;
            }
        }

        centroid
    }

    /// Compute coherence score for a cluster
    fn compute_cluster_coherence(&self, instances: &[ClusteredInstance]) -> f32 {
        if instances.len() < 2 {
            return 1.0; // Single instance is perfectly coherent with itself
        }

        // Compute average pairwise similarity
        let mut total_sim = 0.0;
        let mut count = 0;

        for i in 0..instances.len() {
            for j in (i + 1)..instances.len() {
                let sim = self
                    .embedder
                    .similarity(&instances[i].embedding, &instances[j].embedding);
                total_sim += sim;
                count += 1;
            }
        }

        if count > 0 {
            total_sim / count as f32
        } else {
            0.0
        }
    }

    /// Extract common context conditions from clustered instances
    fn extract_context_conditions(&self, instances: &[ClusteredInstance]) -> Vec<String> {
        let mut conditions = Vec::new();

        // Find common file types
        let file_types: HashMap<String, usize> = instances
            .iter()
            .filter_map(|i| i.instance.context.file_type.clone())
            .fold(HashMap::new(), |mut acc, ft| {
                *acc.entry(ft).or_insert(0) += 1;
                acc
            });

        for (ft, count) in file_types {
            if count > instances.len() / 2 {
                conditions.push(format!("file_type={}", ft));
            }
        }

        // Find common tags
        let mut tag_counts: HashMap<String, usize> = HashMap::new();
        for instance in instances {
            for tag in &instance.instance.context.tags {
                *tag_counts.entry(tag.clone()).or_insert(0) += 1;
            }
        }

        for (tag, count) in tag_counts {
            if count > instances.len() / 2 {
                conditions.push(format!("tag={}", tag));
            }
        }

        conditions
    }

    /// Extract common elements from a cluster
    fn extract_common_elements(&self, cluster: &InstanceCluster) -> Result<CommonElements> {
        if cluster.instances.is_empty() {
            return Err(MsError::MiningFailed(
                "Cannot extract from empty cluster".to_string(),
            ));
        }

        // Build abstracted description from common patterns
        let abstracted_description = self.abstract_description(&cluster.instances);

        // Find invariants (things that hold across all instances)
        let invariants = self.find_invariants(&cluster.instances);

        // Context conditions come from the cluster
        let context_conditions = cluster.context_conditions.clone();

        // Identify placeholders (variable parts)
        let placeholders = self.identify_placeholders(&cluster.instances);

        // Extraction confidence based on cluster coherence and size
        let size_factor = (cluster.instances.len() as f32 / 10.0).min(1.0);
        let extraction_confidence = cluster.coherence * 0.7 + size_factor * 0.3;

        Ok(CommonElements {
            abstracted_description,
            invariants,
            context_conditions,
            placeholders,
            extraction_confidence,
        })
    }

    /// Generate abstracted description from instances
    fn abstract_description(&self, instances: &[ClusteredInstance]) -> String {
        if instances.is_empty() {
            return "Empty pattern".to_string();
        }

        // Find common words across instance contents
        let mut word_counts: HashMap<String, usize> = HashMap::new();
        for instance in instances {
            let words: std::collections::HashSet<_> = instance
                .instance
                .content
                .split_whitespace()
                .map(|w| w.to_lowercase())
                .filter(|w| w.len() > 3)
                .collect();

            for word in words {
                *word_counts.entry(word).or_insert(0) += 1;
            }
        }

        // Get words that appear in majority of instances
        let threshold = instances.len() / 2;
        let mut common_words: Vec<_> = word_counts
            .into_iter()
            .filter(|(_, count)| *count > threshold)
            .collect();
        common_words.sort_by(|a, b| b.1.cmp(&a.1));

        let keywords: Vec<_> = common_words.into_iter().take(5).map(|(w, _)| w).collect();

        if keywords.is_empty() {
            "Pattern extracted from multiple instances".to_string()
        } else {
            format!("Pattern involving: {}", keywords.join(", "))
        }
    }

    /// Find invariants across instances
    fn find_invariants(&self, instances: &[ClusteredInstance]) -> Vec<String> {
        let mut invariants = Vec::new();

        // Check for structural invariants
        let all_have_error_handling = instances.iter().all(|i| {
            let content = &i.instance.content.to_lowercase();
            content.contains("error") || content.contains("result") || content.contains("?")
        });
        if all_have_error_handling {
            invariants.push("Uses error handling pattern".to_string());
        }

        let all_are_async = instances.iter().all(|i| {
            let content = &i.instance.content.to_lowercase();
            content.contains("async") || content.contains("await")
        });
        if all_are_async {
            invariants.push("Asynchronous operation".to_string());
        }

        let all_have_tests = instances.iter().all(|i| {
            let content = &i.instance.content.to_lowercase();
            content.contains("test") || content.contains("#[test]")
        });
        if all_have_tests {
            invariants.push("Includes test coverage".to_string());
        }

        invariants
    }

    /// Identify placeholders (variable parts) in patterns
    fn identify_placeholders(&self, instances: &[ClusteredInstance]) -> Vec<Placeholder> {
        let mut placeholders = Vec::new();

        // Look for common structural placeholders
        let mut file_types: Vec<String> = instances
            .iter()
            .filter_map(|i| i.instance.context.file_type.clone())
            .collect();
        file_types.sort();
        file_types.dedup();

        if file_types.len() > 1 {
            placeholders.push(Placeholder {
                name: "FILE_TYPE".to_string(),
                description: "File type/language".to_string(),
                observed_values: file_types,
                constraints: vec![],
            });
        }

        placeholders
    }

    /// Queue an uncertain generalization for human review
    fn queue_uncertainty(
        &self,
        instance: &SpecificInstance,
        validation: &GeneralizationValidation,
        cluster: &InstanceCluster,
        critique: Option<&RefinementCritique>,
    ) -> Result<()> {
        self.uncertainty_queue
            .queue_uncertain(instance, validation, cluster, critique)?;
        Ok(())
    }
}

// =============================================================================
// Converting Between Pattern Types
// =============================================================================

impl From<GeneralPattern> for ExtractedPattern {
    fn from(gp: GeneralPattern) -> Self {
        ExtractedPattern {
            id: format!("gen_{}", uuid::Uuid::new_v4().to_string()[..8].to_string()),
            pattern_type: PatternType::WorkflowPattern {
                steps: vec![],
                triggers: gp.applicability.clone(),
                outcomes: gp.examples.clone(),
            },
            evidence: gp
                .evidence
                .session_ids
                .iter()
                .map(|sid| super::mining::EvidenceRef {
                    session_id: sid.clone(),
                    message_indices: vec![],
                    relevance: gp.confidence,
                    snippet: Some(gp.principle.clone()),
                })
                .collect(),
            confidence: gp.confidence,
            frequency: gp.source_instances,
            tags: vec!["generalized".to_string()],
            description: Some(gp.principle),
            taint_label: None,
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_instance(id: &str, content: &str) -> SpecificInstance {
        SpecificInstance {
            id: id.to_string(),
            content: content.to_string(),
            context: InstanceContext {
                file_type: Some("rust".to_string()),
                project_type: None,
                tags: vec![],
                description: None,
            },
            source: InstanceSource {
                session_id: format!("session-{}", id),
                message_indices: vec![0],
                observed_at: None,
            },
            is_counter_example: false,
        }
    }

    fn make_clustered_instance(
        instance: SpecificInstance,
        embedder: &HashEmbedder,
    ) -> ClusteredInstance {
        let embedding = embedder.embed(&instance.content);
        ClusteredInstance {
            instance,
            distance_to_centroid: 0.0,
            embedding,
        }
    }

    #[test]
    fn test_code_pattern_signature() {
        let sig = CodePatternSignature {
            category: "error_handling".to_string(),
            key_tokens: vec!["Result".to_string(), "Error".to_string()],
            features: vec!["uses_match".to_string()],
        };

        let signature = sig.signature();
        assert!(signature.contains("error_handling"));
        assert!(signature.contains("Result"));
        assert!(signature.contains("uses_match"));
    }

    #[test]
    fn test_generalization_validation_empty() {
        let validation = GeneralizationValidation::empty();
        assert_eq!(validation.coverage, 0.0);
        assert_eq!(validation.confidence, 0.0);
    }

    #[test]
    fn test_generalization_validation_compute() {
        let embedder = HashEmbedder::new(64);

        let instances: Vec<ClusteredInstance> = vec![
            make_clustered_instance(
                make_test_instance("1", "error handling with Result"),
                &embedder,
            ),
            make_clustered_instance(
                make_test_instance("2", "error handling with match"),
                &embedder,
            ),
            make_clustered_instance(make_test_instance("3", "error handling pattern"), &embedder),
        ];

        let common = CommonElements {
            abstracted_description: "Error handling pattern".to_string(),
            invariants: vec!["Uses Result type".to_string()],
            context_conditions: vec!["file_type=rust".to_string()],
            placeholders: vec![],
            extraction_confidence: 0.8,
        };

        let validation = GeneralizationValidation::compute(&common, &instances, &instances);

        assert!(validation.coverage > 0.0);
        assert!(validation.coherence > 0.0);
        assert!(validation.confidence > 0.0);
    }

    #[test]
    fn test_specificity_penalty_for_overbreadth() {
        // High coverage + low coherence should get penalty
        let specificity = GeneralizationValidation::compute_specificity(0.98, 0.3, &[]);
        assert!(specificity < 0.5);

        // Normal coverage should not get penalty
        let specificity = GeneralizationValidation::compute_specificity(0.7, 0.8, &[]);
        assert!(specificity > 0.5);
    }

    #[test]
    fn test_cluster_coherence() {
        let cass = CassClient::new();
        let transformer = SpecificToGeneralTransformer::new(cass);

        let embedder = HashEmbedder::new(64);
        let instances = vec![
            make_clustered_instance(make_test_instance("1", "rust error handling"), &embedder),
            make_clustered_instance(make_test_instance("2", "rust error result"), &embedder),
        ];

        let coherence = transformer.compute_cluster_coherence(&instances);
        assert!(coherence > 0.0);
    }

    #[test]
    fn test_extract_code_pattern() {
        let cass = CassClient::new();
        let transformer = SpecificToGeneralTransformer::new(cass);

        let content = "async fn handle_error() -> Result<(), Error> { match result { Ok(v) => v, Err(e) => return Err(e) } }";
        let pattern = transformer.extract_code_pattern(content);

        assert!(
            pattern.features.contains(&"async_await".to_string())
                || pattern.features.contains(&"handles_errors".to_string())
        );
        assert!(pattern.features.contains(&"uses_match".to_string()));
        assert!(pattern.features.contains(&"returns_result".to_string()));
    }

    #[test]
    fn test_classify_problem() {
        let cass = CassClient::new();
        let transformer = SpecificToGeneralTransformer::new(cass);

        assert_eq!(
            transformer.classify_problem("fix the bug in parser"),
            "bug_fix"
        );
        assert_eq!(
            transformer.classify_problem("add new feature"),
            "feature_implementation"
        );
        assert_eq!(
            transformer.classify_problem("refactor the code"),
            "refactoring"
        );
        assert_eq!(
            transformer.classify_problem("optimize performance"),
            "optimization"
        );
        assert_eq!(transformer.classify_problem("write tests"), "testing");
    }

    #[test]
    fn test_extract_solution() {
        let cass = CassClient::new();
        let transformer = SpecificToGeneralTransformer::new(cass);

        let solution = transformer
            .extract_solution("replace the old iterator with a new pattern using closures");
        assert_eq!(solution.strategy, "replacement");
        assert!(solution.keywords.contains(&"replacement".to_string()));
        assert!(
            solution.tools_used.contains(&"iterators".to_string())
                || solution.tools_used.contains(&"closures".to_string())
        );
    }

    #[test]
    fn test_abstract_description() {
        let cass = CassClient::new();
        let transformer = SpecificToGeneralTransformer::new(cass);
        let embedder = HashEmbedder::new(64);

        let instances = vec![
            make_clustered_instance(
                make_test_instance("1", "error handling with Result type"),
                &embedder,
            ),
            make_clustered_instance(
                make_test_instance("2", "error handling with Result match"),
                &embedder,
            ),
            make_clustered_instance(
                make_test_instance("3", "error handling Result pattern"),
                &embedder,
            ),
        ];

        let description = transformer.abstract_description(&instances);
        // Common words should appear in description
        assert!(
            description.contains("Pattern")
                || description.contains("error")
                || description.contains("result")
        );
    }

    #[test]
    fn test_find_invariants() {
        let cass = CassClient::new();
        let transformer = SpecificToGeneralTransformer::new(cass);
        let embedder = HashEmbedder::new(64);

        let instances = vec![
            make_clustered_instance(
                make_test_instance("1", "fn handle() -> Result<T, Error> { }"),
                &embedder,
            ),
            make_clustered_instance(
                make_test_instance("2", "let result: Result<_, _> = foo()?;"),
                &embedder,
            ),
            make_clustered_instance(
                make_test_instance("3", "match result { Ok(v) => v, Err(e) => return Err(e) }"),
                &embedder,
            ),
        ];

        let invariants = transformer.find_invariants(&instances);
        assert!(
            invariants
                .iter()
                .any(|i| i.contains("error") || i.contains("Error"))
        );
    }

    #[test]
    fn test_null_uncertainty_queue() {
        let queue = NullUncertaintyQueue;
        let instance = make_test_instance("test", "content");
        let validation = GeneralizationValidation::empty();
        let cluster = InstanceCluster {
            id: "test".to_string(),
            instances: vec![],
            context_conditions: vec![],
            centroid: None,
            coherence: 0.0,
        };

        let result = queue.queue_uncertain(&instance, &validation, &cluster, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_transformer_config_default() {
        let config = TransformerConfig::default();
        assert_eq!(config.min_instances, 3);
        assert_eq!(config.confidence_threshold, 0.7);
        assert_eq!(config.max_search_results, 100);
    }

    #[test]
    fn test_general_pattern_to_extracted_pattern() {
        let gp = GeneralPattern {
            principle: "Test principle".to_string(),
            examples: vec!["example1".to_string()],
            applicability: vec!["when testing".to_string()],
            confidence: 0.85,
            source_instances: 5,
            avoid_when: vec![],
            evidence: PatternEvidence {
                session_ids: vec!["s1".to_string()],
                validation: GeneralizationValidation::empty(),
                critique: None,
            },
        };

        let extracted: ExtractedPattern = gp.into();
        assert!(extracted.id.starts_with("gen_"));
        assert_eq!(extracted.confidence, 0.85);
        assert_eq!(extracted.frequency, 5);
        assert!(extracted.tags.contains(&"generalized".to_string()));
    }

    #[test]
    fn test_counterexample_reason_serialization() {
        let reason = CounterExampleReason::PatternNotApplicable;
        let json = serde_json::to_string(&reason).unwrap();
        assert_eq!(json, "\"pattern_not_applicable\"");

        let reason = CounterExampleReason::OutcomeMismatch;
        let json = serde_json::to_string(&reason).unwrap();
        assert_eq!(json, "\"outcome_mismatch\"");
    }

    #[test]
    fn test_uncertainty_reason_serialization() {
        let reason = UncertaintyReason::LowConfidence {
            confidence: 0.5,
            threshold: 0.7,
        };
        let json = serde_json::to_string(&reason).unwrap();
        assert!(json.contains("low_confidence"));
        assert!(json.contains("0.5"));
    }
}
