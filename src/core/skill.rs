//! Skill data structure

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

fn default_format_version() -> String {
    SkillSpec::FORMAT_VERSION.to_string()
}

/// A skill definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// Unique skill ID
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Short description
    pub description: String,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Version string
    pub version: String,
}

impl Skill {
    /// Create a new skill with the given ID and name
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            tags: vec![],
            version: "0.1.0".to_string(),
        }
    }
}

/// Structured skill specification for deterministic compilation
/// The source-of-truth for a skill - SKILL.md is a compiled view of this.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSpec {
    /// Spec format version (for migrations)
    #[serde(default = "default_format_version")]
    pub format_version: String,
    /// Skill metadata
    pub metadata: SkillMetadata,
    /// Sections in the skill
    pub sections: Vec<SkillSection>,

    // === INHERITANCE FIELDS ===
    /// Parent skill to inherit from (single inheritance).
    /// When set, this skill inherits all sections from the parent
    /// unless explicitly overridden.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,

    /// If true, replace parent's rules instead of appending.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub replace_rules: bool,

    /// If true, replace parent's examples instead of appending.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub replace_examples: bool,

    /// If true, replace parent's pitfalls instead of appending.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub replace_pitfalls: bool,

    /// If true, replace parent's checklist instead of appending.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub replace_checklist: bool,

    // === COMPOSITION FIELDS ===
    /// Other skills to include/compose into this skill.
    /// Includes are applied after inheritance resolution.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub includes: Vec<SkillInclude>,
}

// =============================================================================
// SKILL COMPOSITION (INCLUDES)
// =============================================================================

/// Specification for including content from another skill.
///
/// Unlike inheritance (`extends`), includes allow composing content from
/// multiple skills by merging specific sections or block types into target
/// sections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInclude {
    /// ID of the skill to include content from.
    pub skill: String,

    /// Target section/block type to merge included content into.
    pub into: IncludeTarget,

    /// Optional prefix to add to included items for clarity.
    /// E.g., "Error: " to prefix all rules from an error handling skill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,

    /// Specific sections to include from the source skill.
    /// If empty, includes all matching sections.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sections: Option<Vec<String>>,

    /// Position to insert included content: prepend or append (default).
    #[serde(default)]
    pub position: IncludePosition,
}

/// Target for included content - which section/block type to merge into.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum IncludeTarget {
    /// Merge into rules section/blocks
    Rules,
    /// Merge into examples section/blocks
    Examples,
    /// Merge into pitfalls section/blocks
    Pitfalls,
    /// Merge into checklist section/blocks
    Checklist,
    /// Merge into context/overview section
    Context,
}

impl IncludeTarget {
    /// Get the corresponding BlockType for this target.
    #[must_use]
    pub const fn to_block_type(&self) -> BlockType {
        match self {
            Self::Rules => BlockType::Rule,
            Self::Examples => BlockType::Code,
            Self::Pitfalls => BlockType::Pitfall,
            Self::Checklist => BlockType::Checklist,
            Self::Context => BlockType::Text,
        }
    }
}

/// Position for inserting included content.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IncludePosition {
    /// Insert before existing content
    Prepend,
    /// Insert after existing content (default)
    #[default]
    Append,
}

impl Default for SkillSpec {
    fn default() -> Self {
        Self {
            format_version: Self::FORMAT_VERSION.to_string(),
            metadata: SkillMetadata::default(),
            sections: Vec::new(),
            extends: None,
            replace_rules: false,
            replace_examples: false,
            replace_pitfalls: false,
            replace_checklist: false,
            includes: Vec::new(),
        }
    }
}

/// Skill metadata
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillMetadata {
    /// Unique identifier
    #[serde(default)]
    pub id: String,
    /// Human-readable name
    #[serde(default)]
    pub name: String,
    /// Version
    #[serde(default)]
    pub version: String,
    /// Description
    #[serde(default)]
    pub description: String,
    /// Tags
    #[serde(default)]
    pub tags: Vec<String>,
    /// Required capabilities
    #[serde(default)]
    pub requires: Vec<String>,
    /// Provided capabilities
    #[serde(default)]
    pub provides: Vec<String>,
    /// Supported platforms
    #[serde(default)]
    pub platforms: Vec<String>,
    /// Author
    #[serde(default)]
    pub author: Option<String>,
    /// License
    #[serde(default)]
    pub license: Option<String>,
    /// Context tags for auto-loading relevance matching.
    #[serde(default, skip_serializing_if = "ContextTags::is_empty")]
    pub context: ContextTags,
}

/// A section in a skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSection {
    /// Section ID for block-level overlays
    pub id: String,
    /// Section title
    pub title: String,
    /// Section content blocks
    pub blocks: Vec<SkillBlock>,
}

/// A content block in a section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillBlock {
    /// Block ID
    pub id: String,
    /// Block type
    pub block_type: BlockType,
    /// Block content
    pub content: String,
}

/// Block type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "lowercase")]
pub enum BlockType {
    /// Plain text
    #[default]
    Text,
    /// Code example
    Code,
    /// Rule or invariant
    Rule,
    /// Warning or pitfall
    Pitfall,
    /// Command recipe
    Command,
    /// Checklist item
    Checklist,
}

impl SkillSpec {
    /// Current format version
    pub const FORMAT_VERSION: &'static str = "1.0";

    /// Create a new empty spec
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            format_version: Self::FORMAT_VERSION.to_string(),
            metadata: SkillMetadata {
                id: id.into(),
                name: name.into(),
                ..Default::default()
            },
            sections: vec![],
            extends: None,
            replace_rules: false,
            replace_examples: false,
            replace_pitfalls: false,
            replace_checklist: false,
            includes: Vec::new(),
        }
    }

    /// Check if this skill includes other skills
    #[must_use]
    pub fn has_includes(&self) -> bool {
        !self.includes.is_empty()
    }

    /// Check if this skill extends another skill
    #[must_use]
    pub const fn has_parent(&self) -> bool {
        self.extends.is_some()
    }

    /// Get the parent skill ID if this skill extends another
    #[must_use]
    pub fn parent_id(&self) -> Option<&str> {
        self.extends.as_deref()
    }
}

// =============================================================================
// SKILL SOURCE AND LAYERS
// =============================================================================

/// Source information for a skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSource {
    /// Path to skill directory or file
    pub path: std::path::PathBuf,
    /// Which layer this skill comes from
    pub layer: SkillLayer,
    /// Git remote URL if applicable
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_remote: Option<String>,
    /// Git commit hash if applicable
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,
    /// Content hash for change detection
    pub content_hash: String,
}

/// Skill layer for priority resolution
#[derive(Debug, Clone, Copy, Hash, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum SkillLayer {
    /// Base/system skills
    Base,
    /// Organization-wide skills
    Org,
    /// Project-specific skills
    Project,
    /// User-specific skills (highest priority)
    User,
}

impl SkillLayer {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::Org => "org",
            Self::Project => "project",
            Self::User => "user",
        }
    }
}

impl std::fmt::Display for SkillLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// =============================================================================
// SKILL ASSETS
// =============================================================================

/// Associated files for a skill
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillAssets {
    /// Scripts in scripts/ directory
    #[serde(default)]
    pub scripts: Vec<ScriptFile>,
    /// Reference files in references/ directory
    #[serde(default)]
    pub references: Vec<ReferenceFile>,
    /// Test files in tests/ directory
    #[serde(default)]
    pub tests: Vec<TestFile>,
}

/// A script file associated with a skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptFile {
    /// Relative path from skill directory
    pub path: std::path::PathBuf,
    /// Script language (bash, python, etc.)
    pub language: String,
    /// Brief description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// File content (populated during indexing)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// A reference file (examples, templates)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceFile {
    /// Relative path from skill directory
    pub path: std::path::PathBuf,
    /// File type
    pub file_type: String,
    /// File content (populated during indexing)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// A test file for the skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestFile {
    /// Relative path from skill directory
    pub path: std::path::PathBuf,
    /// Test framework (if applicable)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framework: Option<String>,
}

// =============================================================================
// TRIGGERS AND REQUIREMENTS
// =============================================================================

/// Trigger condition for skill suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillTrigger {
    /// Trigger type: "command", "`file_pattern`", "keyword", "context"
    pub trigger_type: String,
    /// Pattern to match
    pub pattern: String,
    /// Priority boost when triggered (0.0 - 1.0)
    #[serde(default)]
    pub boost: f32,
}

/// Environment requirements for a skill
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillRequirements {
    /// Supported platforms (empty = any)
    #[serde(default)]
    pub platforms: Vec<Platform>,
    /// Required external tools (git, docker, gh, etc.)
    #[serde(default)]
    pub tools: Vec<ToolRequirement>,
    /// Required environment variables (presence only)
    #[serde(default)]
    pub env: Vec<String>,
    /// Network requirement (offline/online)
    #[serde(default)]
    pub network: NetworkRequirement,
}

/// Platform constraint
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Any,
    Linux,
    Macos,
    Windows,
}

/// Tool requirement specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRequirement {
    /// Tool name (e.g., "git", "docker")
    pub name: String,
    /// Minimum version
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_version: Option<String>,
    /// Whether the tool is required (vs. optional)
    #[serde(default)]
    pub required: bool,
}

/// Network access requirement
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NetworkRequirement {
    #[default]
    OfflineOk,
    Required,
    PreferOffline,
}

// =============================================================================
// CONTEXT TAGS (AUTO-LOADING)
// =============================================================================

/// Context metadata for skill auto-loading and relevance matching.
///
/// Skills can declare what contexts they're relevant for, enabling
/// automatic suggestion when the user is working in a matching context.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContextTags {
    /// Project types this skill is relevant for (e.g., "rust", "node", "python").
    /// Should match `ProjectType` identifiers from the detector module.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub project_types: Vec<String>,

    /// File glob patterns that indicate relevance (e.g., "*.rs", "Cargo.toml").
    /// When files matching these patterns are open or modified, boost the skill.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub file_patterns: Vec<String>,

    /// Tool/binary names that indicate relevance (e.g., "cargo", "rustc", "npm").
    /// When these tools are detected in the environment, boost the skill.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,

    /// Advanced signal patterns for fine-grained relevance matching.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signals: Vec<ContextSignal>,
}

impl ContextTags {
    /// Check if this context has any relevance criteria defined.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.project_types.is_empty()
            && self.file_patterns.is_empty()
            && self.tools.is_empty()
            && self.signals.is_empty()
    }

    /// Check if a project type matches this context.
    #[must_use]
    pub fn matches_project_type(&self, project_type: &str) -> bool {
        if self.project_types.is_empty() {
            return false;
        }
        let pt_lower = project_type.to_lowercase();
        self.project_types
            .iter()
            .any(|t| t.to_lowercase() == pt_lower)
    }

    /// Check if a filename matches any file pattern.
    #[must_use]
    pub fn matches_file(&self, filename: &str) -> bool {
        for pattern in &self.file_patterns {
            if pattern_matches(pattern, filename) {
                return true;
            }
        }
        false
    }

    /// Check if a tool name matches.
    #[must_use]
    pub fn matches_tool(&self, tool: &str) -> bool {
        if self.tools.is_empty() {
            return false;
        }
        let tool_lower = tool.to_lowercase();
        self.tools.iter().any(|t| t.to_lowercase() == tool_lower)
    }
}

/// A contextual signal pattern for relevance matching.
///
/// Signals are advanced patterns that look for specific code constructs
/// or content patterns to determine skill relevance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSignal {
    /// Human-readable name for this signal.
    pub name: String,

    /// Regex pattern to match against file content.
    pub pattern: String,

    /// Weight/contribution of this signal (0.0-1.0).
    /// Higher weights mean stronger relevance indication.
    #[serde(default = "default_signal_weight")]
    pub weight: f32,
}

const fn default_signal_weight() -> f32 {
    0.5
}

impl ContextSignal {
    /// Create a new context signal.
    pub fn new(name: impl Into<String>, pattern: impl Into<String>, weight: f32) -> Self {
        Self {
            name: name.into(),
            pattern: pattern.into(),
            weight: weight.clamp(0.0, 1.0),
        }
    }

    /// Compile the pattern to a regex (returns None if invalid).
    #[must_use]
    pub fn compile_pattern(&self) -> Option<regex::Regex> {
        regex::Regex::new(&self.pattern).ok()
    }
}

/// Simple glob pattern matching for file patterns.
fn pattern_matches(pattern: &str, filename: &str) -> bool {
    // Handle exact matches
    if pattern == filename {
        return true;
    }

    // Handle ** glob (match any directory structure) - check this FIRST
    // because ** patterns also contain single *
    if pattern.contains("**") {
        let parts: Vec<&str> = pattern.split("**").collect();
        if parts.len() == 2 {
            let prefix = parts[0];
            let suffix = parts[1];

            // Check prefix (empty prefix means match from start)
            let prefix_ok = prefix.is_empty() || filename.starts_with(prefix);
            if !prefix_ok {
                return false;
            }

            // Handle suffix patterns like "/*.rs" or "*.rs"
            let suffix = suffix.strip_prefix('/').unwrap_or(suffix);
            if suffix.is_empty() {
                return true;
            }

            // If suffix starts with *, check extension match
            if let Some(ext) = suffix.strip_prefix('*') {
                return filename.ends_with(ext);
            }

            // Otherwise check exact suffix match
            return filename.ends_with(suffix);
        }
    }

    // Handle simple wildcards at start (e.g., "*.rs")
    if let Some(suffix) = pattern.strip_prefix('*') {
        // Skip leading / if present (e.g., "/*.rs" should match ".rs")
        let suffix = suffix.strip_prefix('/').unwrap_or(suffix);
        return filename.ends_with(suffix);
    }

    // Handle simple wildcards at end (e.g., "Cargo*")
    if let Some(prefix) = pattern.strip_suffix('*') {
        return filename.starts_with(prefix);
    }

    false
}

// =============================================================================
// SKILL SLICES (TOKEN PACKING)
// =============================================================================

/// A sliceable unit of a skill for token-aware packing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSlice {
    /// Stable slice id (rule-1, example-2)
    pub id: String,
    /// Type of slice
    pub slice_type: SliceType,
    /// Estimated token count
    pub token_estimate: usize,
    /// Utility score (0.0 - 1.0)
    pub utility_score: f32,
    /// Coverage group for optimization
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage_group: Option<String>,
    /// Tags for filtering
    #[serde(default)]
    pub tags: Vec<String>,
    /// Dependencies on other slices
    #[serde(default)]
    pub requires: Vec<String>,
    /// Conditional inclusion predicate
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<SlicePredicate>,
    /// Section title for context
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_title: Option<String>,
    /// Markdown content
    pub content: String,
}

/// Type of skill slice
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SliceType {
    Rule,
    Command,
    Example,
    Checklist,
    Pitfall,
    Overview,
    Reference,
    /// Policy slices cannot be removed even under tight budgets
    Policy,
}

/// Predicate for conditional slice inclusion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlicePredicate {
    /// Expression string (e.g., "package:next >= 16.0.0")
    pub expr: String,
    /// Parsed predicate type
    pub predicate_type: PredicateType,
}

/// Types of predicates for conditional inclusion
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PredicateType {
    PackageVersion {
        package: String,
        op: VersionOp,
        version: String,
    },
    EnvVar {
        var: String,
    },
    FileExists {
        pattern: String,
    },
    RustEdition {
        op: VersionOp,
        edition: String,
    },
    ToolVersion {
        tool: String,
        op: VersionOp,
        version: String,
    },
}

/// Version comparison operators
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum VersionOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

// =============================================================================
// EVIDENCE AND PROVENANCE
// =============================================================================

/// Rule-level evidence index for provenance and auditing.
/// Uses `BTreeMap` for deterministic serialization (consistent JSON output).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillEvidenceIndex {
    /// Map of rule ID to evidence references
    pub rules: std::collections::BTreeMap<String, Vec<EvidenceRef>>,
    /// Coverage statistics
    pub coverage: EvidenceCoverage,
}

/// Reference to evidence supporting a rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRef {
    /// CASS session ID
    pub session_id: String,
    /// Message range within session
    pub message_range: (u32, u32),
    /// Hash of the snippet
    pub snippet_hash: String,
    /// Optional excerpt (may be redacted)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
    /// Detail level of evidence
    pub level: EvidenceLevel,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
}

/// Level of detail for evidence
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EvidenceLevel {
    /// Hash + message range only
    Pointer,
    /// Minimal redacted excerpt
    Excerpt,
    /// Full context available via CASS
    Expanded,
}

/// Evidence coverage statistics
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EvidenceCoverage {
    /// Total number of rules
    pub total_rules: usize,
    /// Rules with at least one evidence reference
    pub rules_with_evidence: usize,
    /// Average confidence across all evidence
    pub avg_confidence: f32,
}

// =============================================================================
// SKILL PACK (RUNTIME CACHE)
// =============================================================================

/// Precompiled runtime cache for low-latency load/suggest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPack {
    /// Skill ID
    pub skill_id: String,
    /// Path to pack file
    pub pack_path: std::path::PathBuf,
    /// Hash of the spec
    pub spec_hash: String,
    /// Hash of the slices
    pub slices_hash: String,
    /// Hash of the embeddings
    pub embedding_hash: String,
}

/// Pack contracts define minimal guidance guarantees for specific tasks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackContract {
    /// Contract ID (e.g., "`DebugContract`")
    pub id: String,
    /// Description of what this contract guarantees
    pub description: String,
    /// Required coverage groups
    pub required_groups: Vec<String>,
    /// Mandatory slice IDs
    pub mandatory_slices: Vec<String>,
    /// Optional max slices per coverage group
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_per_group: Option<usize>,
    /// Optional weighting by coverage group (lowercase keys)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_weights: Option<HashMap<String, f32>>,
    /// Optional weighting by tag (lowercase keys)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag_weights: Option<HashMap<String, f32>>,
}

// =============================================================================
// SPEC LENS (MARKDOWN MAPPING)
// =============================================================================

/// Mapping from compiled markdown back to spec blocks.
/// Note: Named `SpecLensMapping` to avoid collision with the `SpecLens`
/// converter type in `spec_lens.rs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecLensMapping {
    /// Format version
    pub format_version: String,
    /// Block mappings
    pub blocks: Vec<BlockLens>,
}

/// Mapping for a single block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockLens {
    /// Block ID in the spec
    pub block_id: String,
    /// Section name
    pub section: String,
    /// Block type
    pub block_type: String,
    /// Byte range in compiled markdown
    pub byte_range: (u32, u32),
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_new() {
        let skill = Skill::new("test-id", "Test Skill");
        assert_eq!(skill.id, "test-id");
        assert_eq!(skill.name, "Test Skill");
    }

    #[test]
    fn test_skill_spec_new() {
        let spec = SkillSpec::new("test-id", "Test Skill");
        assert_eq!(spec.metadata.id, "test-id");
        assert_eq!(spec.metadata.name, "Test Skill");
    }

    #[test]
    fn test_skill_layer_ordering() {
        assert!(SkillLayer::Base < SkillLayer::Org);
        assert!(SkillLayer::Org < SkillLayer::Project);
        assert!(SkillLayer::Project < SkillLayer::User);
    }

    #[test]
    fn test_slice_type_serialization() {
        let slice_type = SliceType::Policy;
        let json = serde_json::to_string(&slice_type).unwrap();
        assert_eq!(json, "\"policy\"");
    }

    #[test]
    fn test_version_op_variants() {
        let ops = [
            VersionOp::Eq,
            VersionOp::Ne,
            VersionOp::Lt,
            VersionOp::Le,
            VersionOp::Gt,
            VersionOp::Ge,
        ];
        for op in ops {
            let json = serde_json::to_string(&op).unwrap();
            let parsed: VersionOp = serde_json::from_str(&json).unwrap();
            assert_eq!(op, parsed);
        }
    }

    // Context tags tests

    #[test]
    fn test_context_tags_empty() {
        let ctx = ContextTags::default();
        assert!(ctx.is_empty());
    }

    #[test]
    fn test_context_tags_not_empty() {
        let ctx = ContextTags {
            project_types: vec!["rust".to_string()],
            ..Default::default()
        };
        assert!(!ctx.is_empty());
    }

    #[test]
    fn test_context_tags_matches_project_type() {
        let ctx = ContextTags {
            project_types: vec!["rust".to_string(), "python".to_string()],
            ..Default::default()
        };
        assert!(ctx.matches_project_type("rust"));
        assert!(ctx.matches_project_type("RUST")); // Case insensitive
        assert!(ctx.matches_project_type("Python"));
        assert!(!ctx.matches_project_type("node"));
    }

    #[test]
    fn test_context_tags_matches_file() {
        let ctx = ContextTags {
            file_patterns: vec!["*.rs".to_string(), "Cargo.toml".to_string()],
            ..Default::default()
        };
        assert!(ctx.matches_file("main.rs"));
        assert!(ctx.matches_file("lib.rs"));
        assert!(ctx.matches_file("Cargo.toml"));
        assert!(!ctx.matches_file("package.json"));
    }

    #[test]
    fn test_context_tags_matches_tool() {
        let ctx = ContextTags {
            tools: vec!["cargo".to_string(), "rustc".to_string()],
            ..Default::default()
        };
        assert!(ctx.matches_tool("cargo"));
        assert!(ctx.matches_tool("CARGO")); // Case insensitive
        assert!(ctx.matches_tool("rustc"));
        assert!(!ctx.matches_tool("npm"));
    }

    #[test]
    fn test_context_signal_new() {
        let signal = ContextSignal::new("error_handling", "use.*thiserror", 0.8);
        assert_eq!(signal.name, "error_handling");
        assert_eq!(signal.pattern, "use.*thiserror");
        assert!((signal.weight - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_context_signal_weight_clamped() {
        let signal = ContextSignal::new("test", "pattern", 1.5);
        assert!((signal.weight - 1.0).abs() < 0.001);

        let signal = ContextSignal::new("test", "pattern", -0.5);
        assert!(signal.weight.abs() < 0.001);
    }

    #[test]
    fn test_context_signal_compile_pattern() {
        let signal = ContextSignal::new("test", r"Result<.*,.*>", 0.5);
        let regex = signal.compile_pattern();
        assert!(regex.is_some());
        let regex = regex.unwrap();
        assert!(regex.is_match("fn foo() -> Result<i32, Error>"));
    }

    #[test]
    fn test_context_signal_invalid_pattern() {
        let signal = ContextSignal::new("test", "[invalid", 0.5);
        let regex = signal.compile_pattern();
        assert!(regex.is_none());
    }

    #[test]
    fn test_context_tags_serialization() {
        let ctx = ContextTags {
            project_types: vec!["rust".to_string()],
            file_patterns: vec!["*.rs".to_string()],
            tools: vec!["cargo".to_string()],
            signals: vec![ContextSignal::new("test", "pattern", 0.5)],
        };
        let json = serde_json::to_string(&ctx).unwrap();
        let parsed: ContextTags = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.project_types, ctx.project_types);
        assert_eq!(parsed.file_patterns, ctx.file_patterns);
        assert_eq!(parsed.tools, ctx.tools);
        assert_eq!(parsed.signals.len(), 1);
    }

    #[test]
    fn test_skill_metadata_with_context() {
        let yaml = r#"
id: rust-errors
name: Rust Error Handling
version: "1.0.0"
context:
  project_types:
    - rust
  file_patterns:
    - "*.rs"
  tools:
    - cargo
  signals:
    - name: thiserror_usage
      pattern: "use.*thiserror"
      weight: 0.8
"#;
        let metadata: SkillMetadata = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(metadata.id, "rust-errors");
        assert!(!metadata.context.is_empty());
        assert!(metadata.context.matches_project_type("rust"));
        assert!(metadata.context.matches_file("main.rs"));
        assert!(metadata.context.matches_tool("cargo"));
        assert_eq!(metadata.context.signals.len(), 1);
    }

    #[test]
    fn test_skill_metadata_empty_context_not_serialized() {
        let metadata = SkillMetadata {
            id: "test".to_string(),
            name: "Test".to_string(),
            ..Default::default()
        };
        let json = serde_json::to_string(&metadata).unwrap();
        // Empty context should not be in the JSON
        assert!(!json.contains("context"));
    }

    #[test]
    fn test_pattern_matches_glob() {
        // Prefix glob
        assert!(pattern_matches("*.rs", "main.rs"));
        assert!(pattern_matches("*.rs", "lib.rs"));
        assert!(!pattern_matches("*.rs", "package.json"));

        // Suffix glob
        assert!(pattern_matches("Cargo*", "Cargo.toml"));
        assert!(pattern_matches("Cargo*", "Cargo.lock"));
        assert!(!pattern_matches("Cargo*", "package.json"));

        // Double star glob
        assert!(pattern_matches("**/*.rs", "src/main.rs"));
        assert!(pattern_matches("src/**", "src/lib.rs"));

        // Exact match
        assert!(pattern_matches("Cargo.toml", "Cargo.toml"));
        assert!(!pattern_matches("Cargo.toml", "cargo.toml"));
    }
}
