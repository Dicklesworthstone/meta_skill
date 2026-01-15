//! Skill data structure

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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
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
        }
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
    pub fn as_str(&self) -> &'static str {
        match self {
            SkillLayer::Base => "base",
            SkillLayer::Org => "org",
            SkillLayer::Project => "project",
            SkillLayer::User => "user",
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
}

/// A reference file (examples, templates)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceFile {
    /// Relative path from skill directory
    pub path: std::path::PathBuf,
    /// File type
    pub file_type: String,
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
    /// Trigger type: "command", "file_pattern", "keyword", "context"
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
/// Uses BTreeMap for deterministic serialization (consistent JSON output).
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
    /// Contract ID (e.g., "DebugContract")
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
}

// =============================================================================
// SPEC LENS (MARKDOWN MAPPING)
// =============================================================================

/// Mapping from compiled markdown back to spec blocks.
/// Note: Named SpecLensMapping to avoid collision with the SpecLens
/// converter type in spec_lens.rs.
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
}
