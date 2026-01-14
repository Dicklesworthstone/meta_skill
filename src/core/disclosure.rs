//! Progressive disclosure levels for skill loading
//!
//! Disclosure reveals skill content incrementally based on need, preventing
//! context bloat while ensuring agents get the guidance they require.

use serde::{Deserialize, Serialize};

use super::packing::{ConstrainedPacker, MandatoryPredicate, MandatorySlice, PackConstraints};
use super::skill::{
    ReferenceFile, ScriptFile, SkillAssets, SkillMetadata, SkillSection, SkillSpec,
};
use super::slicing::SkillSlicer;

// =============================================================================
// DISCLOSURE LEVELS
// =============================================================================

/// Disclosure level for skill loading
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DisclosureLevel {
    /// Level 0: Just name and one-line description (~50-100 tokens)
    Minimal,
    /// Level 1: Name, description, key section headers (~200-500 tokens)
    Overview,
    /// Level 2: Overview + main content, truncated examples (~500-1500 tokens)
    Standard,
    /// Level 3: Full SKILL.md content (variable, typically 1000-5000 tokens)
    Full,
    /// Level 4: Full content + scripts + references (5000+ tokens)
    Complete,
    /// Auto-select based on context
    Auto,
}

impl Default for DisclosureLevel {
    fn default() -> Self {
        Self::Auto
    }
}

impl DisclosureLevel {
    /// Target token budget for this disclosure level
    pub fn token_budget(&self) -> Option<usize> {
        match self {
            DisclosureLevel::Minimal => Some(100),
            DisclosureLevel::Overview => Some(500),
            DisclosureLevel::Standard => Some(1500),
            DisclosureLevel::Full => None,
            DisclosureLevel::Complete => None,
            DisclosureLevel::Auto => None,
        }
    }

    /// Parse from string (CLI argument)
    pub fn from_str_or_level(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "minimal" | "0" => Some(DisclosureLevel::Minimal),
            "overview" | "1" => Some(DisclosureLevel::Overview),
            "standard" | "moderate" | "2" => Some(DisclosureLevel::Standard),
            "full" | "3" => Some(DisclosureLevel::Full),
            "complete" | "4" => Some(DisclosureLevel::Complete),
            "auto" => Some(DisclosureLevel::Auto),
            _ => None,
        }
    }

    /// Get human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            DisclosureLevel::Minimal => "minimal",
            DisclosureLevel::Overview => "overview",
            DisclosureLevel::Standard => "standard",
            DisclosureLevel::Full => "full",
            DisclosureLevel::Complete => "complete",
            DisclosureLevel::Auto => "auto",
        }
    }

    /// Numeric level for comparison
    pub fn level_num(&self) -> u8 {
        match self {
            DisclosureLevel::Minimal => 0,
            DisclosureLevel::Overview => 1,
            DisclosureLevel::Standard => 2,
            DisclosureLevel::Full => 3,
            DisclosureLevel::Complete => 4,
            DisclosureLevel::Auto => 2, // Default to standard for comparison
        }
    }
}

// =============================================================================
// DISCLOSURE PLAN
// =============================================================================

/// Plan for disclosing skill content
#[derive(Debug, Clone)]
pub enum DisclosurePlan {
    /// Use a fixed disclosure level
    Level(DisclosureLevel),
    /// Use token packer with a budget
    Pack(TokenBudget),
}

impl Default for DisclosurePlan {
    fn default() -> Self {
        DisclosurePlan::Level(DisclosureLevel::Standard)
    }
}

/// Token budget for packing mode
#[derive(Debug, Clone, Copy)]
pub struct TokenBudget {
    /// Maximum tokens to emit
    pub tokens: usize,
    /// Packing mode
    pub mode: PackMode,
    /// Max slices per coverage group
    pub max_per_group: usize,
}

impl TokenBudget {
    /// Create a new token budget with defaults
    pub fn new(tokens: usize) -> Self {
        Self {
            tokens,
            mode: PackMode::Balanced,
            max_per_group: 2,
        }
    }

    /// Create with a specific mode
    pub fn with_mode(tokens: usize, mode: PackMode) -> Self {
        Self {
            tokens,
            mode,
            max_per_group: 2,
        }
    }
}

/// Packing mode for token budget optimization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackMode {
    /// Even distribution across slice types
    Balanced,
    /// Prioritize highest-utility slices
    UtilityFirst,
    /// Prioritize coverage (rules, commands first)
    CoverageFirst,
    /// Boost pitfalls and warnings
    PitfallSafe,
}

impl Default for PackMode {
    fn default() -> Self {
        PackMode::Balanced
    }
}

impl PackMode {
    /// Parse from string (CLI argument)
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().replace('-', "_").as_str() {
            "balanced" => Some(PackMode::Balanced),
            "utility_first" | "utility" => Some(PackMode::UtilityFirst),
            "coverage_first" | "coverage" => Some(PackMode::CoverageFirst),
            "pitfall_safe" | "pitfall" => Some(PackMode::PitfallSafe),
            _ => None,
        }
    }
}

// =============================================================================
// DISCLOSED CONTENT
// =============================================================================

/// Content disclosed at a particular level or budget
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisclosedContent {
    /// Frontmatter/metadata (always included)
    pub frontmatter: DisclosedFrontmatter,
    /// Body content (may be truncated or absent)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// Scripts (only at Complete level)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<ScriptFile>,
    /// References (only at Complete level)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<ReferenceFile>,
    /// Actual token count of this disclosure
    pub token_estimate: usize,
    /// The disclosure level used
    pub level: DisclosureLevel,
}

/// Minimal frontmatter for disclosure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisclosedFrontmatter {
    /// Skill ID
    pub id: String,
    /// Skill name
    pub name: String,
    /// Version
    pub version: String,
    /// Description
    pub description: String,
    /// Tags (may be truncated at minimal level)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Dependencies
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
}

impl From<&SkillMetadata> for DisclosedFrontmatter {
    fn from(meta: &SkillMetadata) -> Self {
        Self {
            id: meta.id.clone(),
            name: meta.name.clone(),
            version: meta.version.clone(),
            description: meta.description.clone(),
            tags: meta.tags.clone(),
            requires: meta.requires.clone(),
        }
    }
}

// =============================================================================
// DISCLOSURE CONTEXT
// =============================================================================

/// Context for determining optimal disclosure level
#[derive(Debug, Clone, Default)]
pub struct DisclosureContext {
    /// Explicitly requested level (overrides all else)
    pub explicit_level: Option<DisclosureLevel>,
    /// Token budget for packing
    pub pack_budget: Option<usize>,
    /// Packing mode
    pub pack_mode: Option<PackMode>,
    /// Max slices per coverage group
    pub max_per_group: Option<usize>,
    /// Remaining tokens in agent context
    pub remaining_tokens: usize,
    /// Usage history for this skill
    pub usage_history: UsageHistory,
    /// Type of request
    pub request_type: RequestType,
}

/// Usage history for a skill
#[derive(Debug, Clone, Default)]
pub struct UsageHistory {
    /// Number of times used successfully
    pub successful_uses: u32,
    /// Number of times used unsuccessfully
    pub failed_uses: u32,
    /// Last used timestamp (Unix epoch seconds)
    pub last_used: Option<u64>,
}

/// Type of skill request
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RequestType {
    /// Direct request for specific skill
    Direct,
    /// Suggestion based on context
    #[default]
    Suggestion,
    /// Dependency of another skill
    Dependency,
}

// =============================================================================
// DISCLOSURE LOGIC
// =============================================================================

/// Generate content at a specified disclosure plan
pub fn disclose(spec: &SkillSpec, assets: &SkillAssets, plan: &DisclosurePlan) -> DisclosedContent {
    match plan {
        DisclosurePlan::Level(level) => disclose_level(spec, assets, *level),
        DisclosurePlan::Pack(budget) => disclose_packed(spec, assets, budget),
    }
}

/// Generate content at a specified disclosure level
pub fn disclose_level(
    spec: &SkillSpec,
    assets: &SkillAssets,
    level: DisclosureLevel,
) -> DisclosedContent {
    match level {
        DisclosureLevel::Minimal => {
            let frontmatter = minimal_frontmatter(&spec.metadata);
            DisclosedContent {
                frontmatter,
                body: None,
                scripts: vec![],
                references: vec![],
                token_estimate: estimate_tokens_frontmatter(&spec.metadata, true),
                level,
            }
        }
        DisclosureLevel::Overview => {
            let frontmatter = DisclosedFrontmatter::from(&spec.metadata);
            let body = Some(extract_headings(&spec.sections));
            let token_estimate = estimate_tokens_frontmatter(&spec.metadata, false)
                + estimate_tokens_body(body.as_deref());
            DisclosedContent {
                frontmatter,
                body,
                scripts: vec![],
                references: vec![],
                token_estimate,
                level,
            }
        }
        DisclosureLevel::Standard => {
            let frontmatter = DisclosedFrontmatter::from(&spec.metadata);
            let full_body = render_sections(&spec.sections);
            let body = Some(truncate_examples(&full_body, 1500));
            let token_estimate = estimate_tokens_frontmatter(&spec.metadata, false)
                + estimate_tokens_body(body.as_deref());
            DisclosedContent {
                frontmatter,
                body,
                scripts: vec![],
                references: vec![],
                token_estimate,
                level,
            }
        }
        DisclosureLevel::Full => {
            let frontmatter = DisclosedFrontmatter::from(&spec.metadata);
            let body = Some(render_sections(&spec.sections));
            let token_estimate = estimate_tokens_frontmatter(&spec.metadata, false)
                + estimate_tokens_body(body.as_deref());
            DisclosedContent {
                frontmatter,
                body,
                scripts: vec![],
                references: vec![],
                token_estimate,
                level,
            }
        }
        DisclosureLevel::Complete => {
            let frontmatter = DisclosedFrontmatter::from(&spec.metadata);
            let body = Some(render_sections(&spec.sections));
            let token_estimate = estimate_tokens_frontmatter(&spec.metadata, false)
                + estimate_tokens_body(body.as_deref())
                + estimate_tokens_assets(assets);
            DisclosedContent {
                frontmatter,
                body,
                scripts: assets.scripts.clone(),
                references: assets.references.clone(),
                token_estimate,
                level,
            }
        }
        DisclosureLevel::Auto => {
            // Default to Standard for Auto
            disclose_level(spec, assets, DisclosureLevel::Standard)
        }
    }
}

/// Pack content within a token budget
fn disclose_packed(
    spec: &SkillSpec,
    _assets: &SkillAssets,
    budget: &TokenBudget,
) -> DisclosedContent {
    // Start with frontmatter (always included)
    let frontmatter = DisclosedFrontmatter::from(&spec.metadata);
    let frontmatter_tokens = estimate_tokens_frontmatter(&spec.metadata, false);

    let slice_budget = budget.tokens.saturating_sub(frontmatter_tokens);
    if slice_budget < 50 {
        // Not enough for body, return minimal
        return DisclosedContent {
            frontmatter,
            body: None,
            scripts: vec![],
            references: vec![],
            token_estimate: frontmatter_tokens,
            level: DisclosureLevel::Minimal,
        };
    }

    let slice_index = SkillSlicer::slice(spec);
    let mut constraints = PackConstraints::new(slice_budget, budget.max_per_group);
    constraints
        .mandatory_slices
        .push(MandatorySlice::ByPredicate(MandatoryPredicate::Always));
    let packer = ConstrainedPacker;
    let packed = match packer.pack(&slice_index.slices, &constraints, budget.mode) {
        Ok(result) => result,
        Err(_) => {
            return DisclosedContent {
                frontmatter,
                body: None,
                scripts: vec![],
                references: vec![],
                token_estimate: frontmatter_tokens,
                level: DisclosureLevel::Minimal,
            };
        }
    };

    let body_tokens = packed
        .slices
        .iter()
        .map(|slice| slice.token_estimate)
        .sum::<usize>();
    let body = if packed.slices.is_empty() {
        None
    } else {
        Some(
            packed
                .slices
                .iter()
                .map(|slice| slice.content.trim_end())
                .collect::<Vec<_>>()
                .join("\n\n"),
        )
    };

    // Determine effective level based on content included
    let level = if body_tokens < 100 {
        DisclosureLevel::Minimal
    } else if body_tokens < 500 {
        DisclosureLevel::Overview
    } else if body_tokens < 1500 {
        DisclosureLevel::Standard
    } else {
        DisclosureLevel::Full
    };

    DisclosedContent {
        frontmatter,
        body,
        scripts: vec![],
        references: vec![],
        token_estimate: frontmatter_tokens + body_tokens,
        level,
    }
}

/// Determine optimal disclosure level based on context
pub fn optimal_disclosure(context: &DisclosureContext) -> DisclosurePlan {
    // If explicitly requested, use that level
    if let Some(level) = context.explicit_level {
        return DisclosurePlan::Level(level);
    }

    // If a token budget is specified, use packing
    if let Some(tokens) = context.pack_budget {
        return DisclosurePlan::Pack(TokenBudget {
            tokens,
            mode: context.pack_mode.unwrap_or(PackMode::Balanced),
            max_per_group: context.max_per_group.unwrap_or(2),
        });
    }

    // If agent has used this skill before successfully, give standard
    if context.usage_history.successful_uses > 0 {
        return DisclosurePlan::Level(DisclosureLevel::Standard);
    }

    // If remaining context budget is low, give minimal
    if context.remaining_tokens < 1000 {
        return DisclosurePlan::Level(DisclosureLevel::Minimal);
    }

    // If this is a direct request for the skill, give full
    if context.request_type == RequestType::Direct {
        return DisclosurePlan::Level(DisclosureLevel::Full);
    }

    // Default to overview for suggestions
    DisclosurePlan::Level(DisclosureLevel::Overview)
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Create minimal frontmatter (id, name, one-line description)
fn minimal_frontmatter(meta: &SkillMetadata) -> DisclosedFrontmatter {
    // Truncate description to first sentence or 80 chars
    let description = meta
        .description
        .split('.')
        .next()
        .unwrap_or(&meta.description)
        .chars()
        .take(80)
        .collect::<String>();

    DisclosedFrontmatter {
        id: meta.id.clone(),
        name: meta.name.clone(),
        version: meta.version.clone(),
        description,
        tags: vec![],     // Omit tags at minimal level
        requires: vec![], // Omit requires at minimal level
    }
}

/// Extract just the headings from sections
fn extract_headings(sections: &[SkillSection]) -> String {
    let mut out = String::new();
    for section in sections {
        out.push_str("## ");
        out.push_str(&section.title);
        out.push('\n');
        // Add one-line summary if first block is text
        if let Some(first) = section.blocks.first() {
            let summary: String = first
                .content
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(100)
                .collect();
            if !summary.is_empty() {
                out.push_str(&summary);
                out.push_str("...\n");
            }
        }
        out.push('\n');
    }
    out
}

/// Render sections to markdown
fn render_sections(sections: &[SkillSection]) -> String {
    let mut out = String::new();
    for section in sections {
        out.push_str("## ");
        out.push_str(&section.title);
        out.push_str("\n\n");
        for block in &section.blocks {
            out.push_str(&block.content);
            out.push_str("\n\n");
        }
    }
    out
}

/// Truncate examples and code blocks to fit within token budget
fn truncate_examples(body: &str, max_tokens: usize) -> String {
    // Simple heuristic: 4 chars per token
    let max_chars = max_tokens * 4;
    if body.len() <= max_chars {
        return body.to_string();
    }

    // Try to truncate at a good boundary (end of section)
    let truncated: String = body.chars().take(max_chars).collect();
    if let Some(last_section) = truncated.rfind("\n## ") {
        truncated[..last_section].to_string() + "\n\n[... truncated ...]"
    } else if let Some(last_para) = truncated.rfind("\n\n") {
        truncated[..last_para].to_string() + "\n\n[... truncated ...]"
    } else {
        truncated + "\n\n[... truncated ...]"
    }
}

/// Truncate to fit within a token budget
fn truncate_to_tokens(body: &str, max_tokens: usize) -> String {
    truncate_examples(body, max_tokens)
}

/// Estimate tokens for frontmatter
fn estimate_tokens_frontmatter(meta: &SkillMetadata, minimal: bool) -> usize {
    // Rough estimate: id + name + version + description
    let base = meta.id.len() + meta.name.len() + meta.version.len() + meta.description.len();
    let extras = if minimal {
        0
    } else {
        meta.tags.iter().map(|t| t.len()).sum::<usize>()
            + meta.requires.iter().map(|r| r.len()).sum::<usize>()
    };
    // Rough: 4 chars per token
    (base + extras) / 4 + 20 // +20 for formatting overhead
}

/// Estimate tokens for body content
fn estimate_tokens_body(body: Option<&str>) -> usize {
    body.map(|b| b.len() / 4).unwrap_or(0)
}

/// Estimate tokens for assets
fn estimate_tokens_assets(assets: &SkillAssets) -> usize {
    // Scripts: file paths + language info
    let scripts = assets
        .scripts
        .iter()
        .map(|s| s.path.to_string_lossy().len() + s.language.len() + 20)
        .sum::<usize>();
    // References: file paths
    let refs = assets
        .references
        .iter()
        .map(|r| r.path.to_string_lossy().len() + r.file_type.len() + 10)
        .sum::<usize>();
    (scripts + refs) / 4
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disclosure_level_from_str() {
        assert_eq!(
            DisclosureLevel::from_str_or_level("minimal"),
            Some(DisclosureLevel::Minimal)
        );
        assert_eq!(
            DisclosureLevel::from_str_or_level("0"),
            Some(DisclosureLevel::Minimal)
        );
        assert_eq!(
            DisclosureLevel::from_str_or_level("overview"),
            Some(DisclosureLevel::Overview)
        );
        assert_eq!(
            DisclosureLevel::from_str_or_level("1"),
            Some(DisclosureLevel::Overview)
        );
        assert_eq!(
            DisclosureLevel::from_str_or_level("standard"),
            Some(DisclosureLevel::Standard)
        );
        assert_eq!(
            DisclosureLevel::from_str_or_level("2"),
            Some(DisclosureLevel::Standard)
        );
        assert_eq!(
            DisclosureLevel::from_str_or_level("full"),
            Some(DisclosureLevel::Full)
        );
        assert_eq!(
            DisclosureLevel::from_str_or_level("3"),
            Some(DisclosureLevel::Full)
        );
        assert_eq!(
            DisclosureLevel::from_str_or_level("complete"),
            Some(DisclosureLevel::Complete)
        );
        assert_eq!(
            DisclosureLevel::from_str_or_level("4"),
            Some(DisclosureLevel::Complete)
        );
        assert_eq!(DisclosureLevel::from_str_or_level("invalid"), None);
    }

    #[test]
    fn test_disclosure_level_token_budget() {
        assert_eq!(DisclosureLevel::Minimal.token_budget(), Some(100));
        assert_eq!(DisclosureLevel::Overview.token_budget(), Some(500));
        assert_eq!(DisclosureLevel::Standard.token_budget(), Some(1500));
        assert_eq!(DisclosureLevel::Full.token_budget(), None);
        assert_eq!(DisclosureLevel::Complete.token_budget(), None);
    }

    #[test]
    fn test_pack_mode_from_str() {
        assert_eq!(PackMode::from_str("balanced"), Some(PackMode::Balanced));
        assert_eq!(
            PackMode::from_str("utility_first"),
            Some(PackMode::UtilityFirst)
        );
        assert_eq!(
            PackMode::from_str("utility-first"),
            Some(PackMode::UtilityFirst)
        );
        assert_eq!(
            PackMode::from_str("coverage_first"),
            Some(PackMode::CoverageFirst)
        );
        assert_eq!(
            PackMode::from_str("pitfall_safe"),
            Some(PackMode::PitfallSafe)
        );
        assert_eq!(PackMode::from_str("invalid"), None);
    }

    #[test]
    fn test_optimal_disclosure_explicit() {
        let ctx = DisclosureContext {
            explicit_level: Some(DisclosureLevel::Full),
            ..Default::default()
        };
        match optimal_disclosure(&ctx) {
            DisclosurePlan::Level(level) => assert_eq!(level, DisclosureLevel::Full),
            _ => panic!("Expected Level plan"),
        }
    }

    #[test]
    fn test_optimal_disclosure_pack_budget() {
        let ctx = DisclosureContext {
            pack_budget: Some(800),
            pack_mode: Some(PackMode::UtilityFirst),
            ..Default::default()
        };
        match optimal_disclosure(&ctx) {
            DisclosurePlan::Pack(budget) => {
                assert_eq!(budget.tokens, 800);
                assert_eq!(budget.mode, PackMode::UtilityFirst);
            }
            _ => panic!("Expected Pack plan"),
        }
    }

    #[test]
    fn test_optimal_disclosure_low_tokens() {
        let ctx = DisclosureContext {
            remaining_tokens: 500,
            ..Default::default()
        };
        match optimal_disclosure(&ctx) {
            DisclosurePlan::Level(level) => assert_eq!(level, DisclosureLevel::Minimal),
            _ => panic!("Expected Level plan"),
        }
    }

    #[test]
    fn test_optimal_disclosure_direct_request() {
        let ctx = DisclosureContext {
            request_type: RequestType::Direct,
            remaining_tokens: 10000,
            ..Default::default()
        };
        match optimal_disclosure(&ctx) {
            DisclosurePlan::Level(level) => assert_eq!(level, DisclosureLevel::Full),
            _ => panic!("Expected Level plan"),
        }
    }

    #[test]
    fn test_truncate_examples() {
        let body = "## Section 1\n\nContent here.\n\n## Section 2\n\nMore content.";
        let truncated = truncate_examples(body, 10); // Very small budget
        assert!(truncated.contains("[... truncated ...]"));
    }

    #[test]
    fn test_minimal_frontmatter() {
        let meta = SkillMetadata {
            id: "test-skill".to_string(),
            name: "Test Skill".to_string(),
            version: "1.0.0".to_string(),
            description: "This is a test skill. It has multiple sentences.".to_string(),
            tags: vec!["tag1".to_string(), "tag2".to_string()],
            ..Default::default()
        };
        let fm = minimal_frontmatter(&meta);
        assert_eq!(fm.id, "test-skill");
        assert_eq!(fm.name, "Test Skill");
        assert!(fm.description.len() <= 80);
        assert!(fm.tags.is_empty()); // Tags omitted at minimal
    }
}
