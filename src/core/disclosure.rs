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
#[derive(Default)]
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
    #[default]
    Auto,
}

impl DisclosureLevel {
    /// Target token budget for this disclosure level
    #[must_use]
    pub const fn token_budget(&self) -> Option<usize> {
        match self {
            Self::Minimal => Some(100),
            Self::Overview => Some(500),
            Self::Standard => Some(1500),
            Self::Full => None,
            Self::Complete => None,
            Self::Auto => None,
        }
    }

    /// Parse from string (CLI argument)
    #[must_use]
    pub fn from_str_or_level(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "minimal" | "0" => Some(Self::Minimal),
            "overview" | "1" => Some(Self::Overview),
            "standard" | "moderate" | "2" => Some(Self::Standard),
            "full" | "3" => Some(Self::Full),
            "complete" | "4" => Some(Self::Complete),
            "auto" => Some(Self::Auto),
            _ => None,
        }
    }

    /// Get human-readable name
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Overview => "overview",
            Self::Standard => "standard",
            Self::Full => "full",
            Self::Complete => "complete",
            Self::Auto => "auto",
        }
    }

    /// Numeric level for comparison
    #[must_use]
    pub const fn level_num(&self) -> u8 {
        match self {
            Self::Minimal => 0,
            Self::Overview => 1,
            Self::Standard => 2,
            Self::Full => 3,
            Self::Complete => 4,
            Self::Auto => 2, // Default to standard for comparison
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
        Self::Level(DisclosureLevel::Standard)
    }
}

/// Token budget for packing mode
#[derive(Debug, Clone)]
pub struct TokenBudget {
    /// Maximum tokens to emit
    pub tokens: usize,
    /// Packing mode
    pub mode: PackMode,
    /// Max slices per coverage group
    pub max_per_group: usize,
    /// Optional pack contract
    pub contract: Option<crate::core::skill::PackContract>,
}

impl TokenBudget {
    /// Create a new token budget with defaults
    #[must_use]
    pub const fn new(tokens: usize) -> Self {
        Self {
            tokens,
            mode: PackMode::Balanced,
            max_per_group: 2,
            contract: None,
        }
    }

    /// Create with a specific mode
    #[must_use]
    pub const fn with_mode(tokens: usize, mode: PackMode) -> Self {
        Self {
            tokens,
            mode,
            max_per_group: 2,
            contract: None,
        }
    }
}

/// Packing mode for token budget optimization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum PackMode {
    /// Even distribution across slice types
    #[default]
    Balanced,
    /// Prioritize highest-utility slices
    UtilityFirst,
    /// Prioritize coverage (rules, commands first)
    CoverageFirst,
    /// Boost pitfalls and warnings
    PitfallSafe,
}

impl PackMode {
    /// Parse from string (CLI argument)
    #[must_use]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().replace('-', "_").as_str() {
            "balanced" => Some(Self::Balanced),
            "utility_first" | "utility" => Some(Self::UtilityFirst),
            "coverage_first" | "coverage" => Some(Self::CoverageFirst),
            "pitfall_safe" | "pitfall" => Some(Self::PitfallSafe),
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
    /// Number of slices included (only for Pack mode)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slices_included: Option<usize>,
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
#[must_use]
pub fn disclose(spec: &SkillSpec, assets: &SkillAssets, plan: &DisclosurePlan) -> DisclosedContent {
    match plan {
        DisclosurePlan::Level(level) => disclose_level(spec, assets, *level),
        DisclosurePlan::Pack(budget) => disclose_packed(spec, assets, budget),
    }
}

/// Generate content at a specified disclosure level
#[must_use]
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
                slices_included: None,
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
                slices_included: None,
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
                slices_included: None,
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
                slices_included: None,
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
                slices_included: None,
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
            slices_included: Some(0),
        };
    }

    let slice_index = SkillSlicer::slice(spec);
    let mut constraints = PackConstraints::new(slice_budget, budget.max_per_group);
    constraints.contract = budget.contract.clone();
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
                slices_included: Some(0),
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
        Some(render_packed_body(&packed.slices))
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

    let slice_count = packed.slices.len();
    DisclosedContent {
        frontmatter,
        body,
        scripts: vec![],
        references: vec![],
        token_estimate: frontmatter_tokens + body_tokens,
        level,
        slices_included: Some(slice_count),
    }
}

fn render_packed_body(slices: &[crate::core::skill::SkillSlice]) -> String {
    let mut out = String::new();
    let mut last_section = None;

    for slice in slices {
        if let Some(title) = &slice.section_title {
            if last_section.as_ref() == Some(title) {
                // Same section, just add spacer
                out.push_str("\n\n");
            } else {
                if !out.is_empty() {
                    out.push_str("\n\n");
                }
                out.push_str("## ");
                out.push_str(title);
                out.push_str("\n\n");
                last_section = Some(title.clone());
            }
        } else {
            // No section title (e.g. Overview maybe?), just spacer
            if !out.is_empty() {
                out.push_str("\n\n");
            }
        }
        out.push_str(slice.content.trim_end());
    }
    out
}

/// Determine optimal disclosure level based on context
#[must_use]
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
            contract: None,
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

/// Estimate tokens for frontmatter
fn estimate_tokens_frontmatter(meta: &SkillMetadata, minimal: bool) -> usize {
    // Rough estimate: id + name + version + description
    let base = meta.id.len() + meta.name.len() + meta.version.len() + meta.description.len();
    let extras = if minimal {
        0
    } else {
        meta.tags
            .iter()
            .map(std::string::String::len)
            .sum::<usize>()
            + meta
                .requires
                .iter()
                .map(std::string::String::len)
                .sum::<usize>()
    };
    // Rough: 4 chars per token
    (base + extras) / 4 + 20 // +20 for formatting overhead
}

/// Estimate tokens for body content
fn estimate_tokens_body(body: Option<&str>) -> usize {
    body.map_or(0, |b| b.len() / 4)
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
    fn test_optimal_disclosure_explicit() -> Result<(), String> {
        let ctx = DisclosureContext {
            explicit_level: Some(DisclosureLevel::Full),
            ..Default::default()
        };
        match optimal_disclosure(&ctx) {
            DisclosurePlan::Level(level) => {
                assert_eq!(level, DisclosureLevel::Full);
                Ok(())
            }
            other => Err(format!("Expected Level plan, got {:?}", other)),
        }
    }

    #[test]
    fn test_optimal_disclosure_pack_budget() -> Result<(), String> {
        let ctx = DisclosureContext {
            pack_budget: Some(800),
            pack_mode: Some(PackMode::UtilityFirst),
            ..Default::default()
        };
        match optimal_disclosure(&ctx) {
            DisclosurePlan::Pack(budget) => {
                assert_eq!(budget.tokens, 800);
                assert_eq!(budget.mode, PackMode::UtilityFirst);
                Ok(())
            }
            other => Err(format!("Expected Pack plan, got {:?}", other)),
        }
    }

    #[test]
    fn test_optimal_disclosure_low_tokens() -> Result<(), String> {
        let ctx = DisclosureContext {
            remaining_tokens: 500,
            ..Default::default()
        };
        match optimal_disclosure(&ctx) {
            DisclosurePlan::Level(level) => {
                assert_eq!(level, DisclosureLevel::Minimal);
                Ok(())
            }
            other => Err(format!("Expected Level plan, got {:?}", other)),
        }
    }

    #[test]
    fn test_optimal_disclosure_direct_request() -> Result<(), String> {
        let ctx = DisclosureContext {
            request_type: RequestType::Direct,
            remaining_tokens: 10000,
            ..Default::default()
        };
        match optimal_disclosure(&ctx) {
            DisclosurePlan::Level(level) => {
                assert_eq!(level, DisclosureLevel::Full);
                Ok(())
            }
            other => Err(format!("Expected Level plan, got {:?}", other)),
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
