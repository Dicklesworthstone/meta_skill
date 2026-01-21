//! Skill generator for transforming classified content blocks into SkillSpec.
//!
//! The generator takes parsed and classified content blocks and produces
//! a well-formed SkillSpec with appropriate sections and metadata.

use super::formatting::{
    self, extract_code_blocks, extract_description, extract_example_title, extract_skill_id,
    format_pitfall, format_rule, infer_domain, infer_tags, parse_checklist,
};
use super::types::{ContentBlock, ContentBlockType};
use crate::core::skill::{BlockType, SkillBlock, SkillSection, SkillSpec};
use std::collections::HashSet;

// =============================================================================
// CONFIGURATION
// =============================================================================

/// Configuration for skill generation.
#[derive(Debug, Clone)]
pub struct GeneratorConfig {
    /// Minimum confidence threshold for including a block.
    /// Blocks below this threshold generate warnings.
    pub min_confidence: f32,

    /// How to handle blocks classified as Unknown.
    pub unknown_handling: UnknownHandling,

    /// Whether to infer metadata from content.
    pub infer_metadata: bool,

    /// Whether to deduplicate similar content.
    pub deduplicate: bool,
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        Self {
            min_confidence: 0.3,
            unknown_handling: UnknownHandling::AddToContext,
            infer_metadata: true,
            deduplicate: true,
        }
    }
}

/// How to handle blocks with Unknown classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnknownHandling {
    /// Add to context section
    AddToContext,
    /// Track for user review
    TrackForReview,
    /// Discard entirely
    Discard,
}

/// Hints provided by the caller to assist generation.
#[derive(Debug, Clone, Default)]
pub struct ImportHints {
    /// Suggested skill ID
    pub suggested_id: Option<String>,
    /// Suggested skill name
    pub suggested_name: Option<String>,
    /// Source filename (for ID inference)
    pub source_filename: Option<String>,
    /// Known domain
    pub domain: Option<String>,
    /// Known tags
    pub tags: Vec<String>,
}

// =============================================================================
// GENERATOR OUTPUT
// =============================================================================

/// Result of skill generation.
#[derive(Debug, Clone)]
pub struct GeneratedSkill {
    /// The generated skill specification.
    pub skill: SkillSpec,
    /// Warnings encountered during generation.
    pub warnings: Vec<Warning>,
    /// Suggestions for user review.
    pub suggestions: Vec<Suggestion>,
    /// Statistics about the generation.
    pub stats: ImportStats,
}

/// Warning generated during skill creation.
#[derive(Debug, Clone)]
pub enum Warning {
    /// Block had low confidence
    LowConfidence {
        content_preview: String,
        confidence: f32,
    },
    /// Block was discarded
    Discarded {
        content_preview: String,
        reason: String,
    },
    /// Duplicate content detected
    Duplicate { content_preview: String },
    /// Missing expected content
    MissingContent { expected: String },
}

impl Warning {
    fn preview(content: &str) -> String {
        let preview: String = content.chars().take(50).collect();
        if content.len() > 50 {
            format!("{}...", preview)
        } else {
            preview
        }
    }
}

/// Suggestion for user review.
#[derive(Debug, Clone)]
pub enum Suggestion {
    /// Block needs manual classification
    ClassifyBlock {
        content_preview: String,
        likely_types: Vec<ContentBlockType>,
    },
    /// Metadata should be reviewed
    ReviewMetadata {
        field: String,
        inferred_value: String,
    },
}

/// Statistics about the import process.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ImportStats {
    /// Total blocks processed
    pub total_blocks: usize,
    /// Rules generated
    pub rules_count: usize,
    /// Examples generated
    pub examples_count: usize,
    /// Pitfalls generated
    pub pitfalls_count: usize,
    /// Checklist items generated
    pub checklist_count: usize,
    /// Context blocks generated
    pub context_count: usize,
    /// Unknown blocks
    pub unknown_count: usize,
    /// Average confidence of processed blocks
    pub avg_confidence: f32,
    /// Blocks skipped due to low confidence
    pub low_confidence_skipped: usize,
    /// Blocks skipped as duplicates
    pub duplicates_skipped: usize,
}

// =============================================================================
// SKILL GENERATOR
// =============================================================================

/// Generator for transforming classified content blocks into SkillSpec.
pub struct SkillGenerator {
    config: GeneratorConfig,
}

impl Default for SkillGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillGenerator {
    /// Create a new generator with default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: GeneratorConfig::default(),
        }
    }

    /// Create a generator with custom configuration.
    #[must_use]
    pub fn with_config(config: GeneratorConfig) -> Self {
        Self { config }
    }

    /// Generate a SkillSpec from parsed content blocks.
    #[must_use]
    pub fn generate(&self, blocks: Vec<ContentBlock>, hints: &ImportHints) -> GeneratedSkill {
        let mut skill = SkillSpec::default();
        let mut warnings = Vec::new();
        let mut suggestions = Vec::new();
        let mut stats = ImportStats {
            total_blocks: blocks.len(),
            ..Default::default()
        };

        // Track seen content for deduplication
        let mut seen_content: HashSet<String> = HashSet::new();

        // Collect confidence for average calculation
        let mut total_confidence = 0.0;
        let mut confidence_count = 0;

        // Section builders
        let mut rules_blocks: Vec<SkillBlock> = Vec::new();
        let mut examples_blocks: Vec<SkillBlock> = Vec::new();
        let mut pitfalls_blocks: Vec<SkillBlock> = Vec::new();
        let mut checklist_blocks: Vec<SkillBlock> = Vec::new();
        let mut context_blocks: Vec<SkillBlock> = Vec::new();

        // Block ID counters
        let mut rule_counter = 0;
        let mut example_counter = 0;
        let mut pitfall_counter = 0;
        let mut checklist_counter = 0;
        let mut context_counter = 0;

        // Phase 1: Extract metadata from metadata blocks
        if self.config.infer_metadata {
            self.infer_metadata(&mut skill, &blocks, hints, &mut suggestions);
        }

        // Phase 2: Process each block
        for block in &blocks {
            // Check confidence threshold
            if block.confidence < self.config.min_confidence {
                stats.low_confidence_skipped += 1;
                warnings.push(Warning::LowConfidence {
                    content_preview: Warning::preview(&block.content),
                    confidence: block.confidence,
                });
                continue;
            }

            // Check for duplicates
            if self.config.deduplicate {
                let normalized = normalize_for_dedup(&block.content);
                if seen_content.contains(&normalized) {
                    stats.duplicates_skipped += 1;
                    warnings.push(Warning::Duplicate {
                        content_preview: Warning::preview(&block.content),
                    });
                    continue;
                }
                seen_content.insert(normalized);
            }

            // Track confidence
            total_confidence += block.confidence;
            confidence_count += 1;

            // Map to appropriate section
            match block.block_type {
                ContentBlockType::Rule => {
                    rule_counter += 1;
                    let formatted = format_rule(&block.content);
                    rules_blocks.push(SkillBlock {
                        id: format!("rule-{}", rule_counter),
                        block_type: BlockType::Rule,
                        content: formatted,
                    });
                    stats.rules_count += 1;
                }

                ContentBlockType::Example => {
                    example_counter += 1;
                    let formatted = self.format_example_block(&block.content);
                    examples_blocks.push(SkillBlock {
                        id: format!("example-{}", example_counter),
                        block_type: BlockType::Code,
                        content: formatted,
                    });
                    stats.examples_count += 1;
                }

                ContentBlockType::Pitfall => {
                    pitfall_counter += 1;
                    let formatted = format_pitfall(&block.content);
                    pitfalls_blocks.push(SkillBlock {
                        id: format!("pitfall-{}", pitfall_counter),
                        block_type: BlockType::Pitfall,
                        content: formatted,
                    });
                    stats.pitfalls_count += 1;
                }

                ContentBlockType::Checklist => {
                    let items = parse_checklist(&block.content);
                    for item in items {
                        checklist_counter += 1;
                        checklist_blocks.push(SkillBlock {
                            id: format!("checklist-{}", checklist_counter),
                            block_type: BlockType::Checklist,
                            content: item.text,
                        });
                        stats.checklist_count += 1;
                    }
                }

                ContentBlockType::Context => {
                    context_counter += 1;
                    context_blocks.push(SkillBlock {
                        id: format!("context-{}", context_counter),
                        block_type: BlockType::Text,
                        content: block.content.trim().to_string(),
                    });
                    stats.context_count += 1;
                }

                ContentBlockType::Metadata => {
                    // Already processed in phase 1
                }

                ContentBlockType::Unknown => {
                    stats.unknown_count += 1;
                    match self.config.unknown_handling {
                        UnknownHandling::AddToContext => {
                            context_counter += 1;
                            context_blocks.push(SkillBlock {
                                id: format!("context-{}", context_counter),
                                block_type: BlockType::Text,
                                content: block.content.trim().to_string(),
                            });
                            stats.context_count += 1;
                        }
                        UnknownHandling::TrackForReview => {
                            suggestions.push(Suggestion::ClassifyBlock {
                                content_preview: Warning::preview(&block.content),
                                likely_types: vec![
                                    ContentBlockType::Rule,
                                    ContentBlockType::Context,
                                ],
                            });
                        }
                        UnknownHandling::Discard => {
                            warnings.push(Warning::Discarded {
                                content_preview: Warning::preview(&block.content),
                                reason: "Unknown block type".to_string(),
                            });
                        }
                    }
                }
            }
        }

        // Phase 3: Build sections
        if !context_blocks.is_empty() {
            skill.sections.push(SkillSection {
                id: "context".to_string(),
                title: "Context".to_string(),
                blocks: context_blocks,
            });
        }

        if !rules_blocks.is_empty() {
            skill.sections.push(SkillSection {
                id: "rules".to_string(),
                title: "Rules".to_string(),
                blocks: rules_blocks,
            });
        }

        if !examples_blocks.is_empty() {
            skill.sections.push(SkillSection {
                id: "examples".to_string(),
                title: "Examples".to_string(),
                blocks: examples_blocks,
            });
        }

        if !pitfalls_blocks.is_empty() {
            skill.sections.push(SkillSection {
                id: "pitfalls".to_string(),
                title: "Pitfalls".to_string(),
                blocks: pitfalls_blocks,
            });
        }

        if !checklist_blocks.is_empty() {
            skill.sections.push(SkillSection {
                id: "checklist".to_string(),
                title: "Checklist".to_string(),
                blocks: checklist_blocks,
            });
        }

        // Calculate average confidence
        if confidence_count > 0 {
            stats.avg_confidence = total_confidence / confidence_count as f32;
        }

        // Generate warnings for missing content
        if stats.rules_count == 0 {
            warnings.push(Warning::MissingContent {
                expected: "No rules found - consider adding imperative guidelines".to_string(),
            });
        }

        GeneratedSkill {
            skill,
            warnings,
            suggestions,
            stats,
        }
    }

    /// Infer and populate metadata from content blocks and hints.
    fn infer_metadata(
        &self,
        skill: &mut SkillSpec,
        blocks: &[ContentBlock],
        hints: &ImportHints,
        suggestions: &mut Vec<Suggestion>,
    ) {
        // Gather all content for analysis
        let all_content: String = blocks
            .iter()
            .map(|b| b.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        // ID: from hints, metadata blocks, or content
        skill.metadata.id = hints
            .suggested_id
            .clone()
            .or_else(|| {
                blocks
                    .iter()
                    .filter(|b| b.block_type == ContentBlockType::Metadata)
                    .find_map(|b| extract_skill_id(&b.content))
            })
            .or_else(|| {
                hints
                    .source_filename
                    .as_ref()
                    .map(|f| formatting::slugify(f))
            })
            .unwrap_or_else(|| "imported-skill".to_string());

        // Name: from hints or metadata blocks
        skill.metadata.name = hints
            .suggested_name
            .clone()
            .or_else(|| {
                blocks
                    .iter()
                    .filter(|b| b.block_type == ContentBlockType::Metadata)
                    .find_map(|b| extract_name_from_content(&b.content))
            })
            .unwrap_or_else(|| formatting::capitalize_first(&skill.metadata.id.replace('-', " ")));

        // Version: default
        if skill.metadata.version.is_empty() {
            skill.metadata.version = "1.0.0".to_string();
        }

        // Description: from first context block or metadata
        if skill.metadata.description.is_empty() {
            skill.metadata.description = blocks
                .iter()
                .filter(|b| b.block_type == ContentBlockType::Context)
                .next()
                .map(|b| {
                    let preview: String = b.content.chars().take(200).collect();
                    if b.content.len() > 200 {
                        format!("{}...", preview)
                    } else {
                        preview
                    }
                })
                .unwrap_or_default();
        }

        // Domain: from hints or inference
        let inferred_domain = hints.domain.clone().or_else(|| infer_domain(&all_content));
        if let Some(domain) = &inferred_domain {
            suggestions.push(Suggestion::ReviewMetadata {
                field: "domain".to_string(),
                inferred_value: domain.clone(),
            });
        }

        // Tags: merge hints with inferred
        let mut tags: HashSet<String> = hints.tags.iter().cloned().collect();
        for tag in infer_tags(&all_content) {
            tags.insert(tag);
        }
        skill.metadata.tags = tags.into_iter().collect();
        skill.metadata.tags.sort();
    }

    /// Format an example block, preserving code and extracting title/description.
    fn format_example_block(&self, content: &str) -> String {
        let code_blocks = extract_code_blocks(content);
        let description = extract_description(content);
        let title = extract_example_title(content);

        let mut result = String::new();

        // Add title if found
        if let Some(t) = title {
            result.push_str(&format!("## {}\n\n", t));
        }

        // Add description if found
        if !description.is_empty() {
            result.push_str(&description);
            result.push_str("\n\n");
        }

        // Add code blocks
        for (i, code_block) in code_blocks.iter().enumerate() {
            if i > 0 {
                result.push_str("\n\n");
            }
            let lang = code_block.language.as_deref().unwrap_or("");
            result.push_str(&format!("```{}\n{}\n```", lang, code_block.code));
        }

        // If no code blocks found, just return the content
        if code_blocks.is_empty() {
            return content.trim().to_string();
        }

        result.trim().to_string()
    }
}

/// Extract name from metadata content.
fn extract_name_from_content(content: &str) -> Option<String> {
    for kv in formatting::extract_metadata_kv(content) {
        if kv.key.to_lowercase() == "name" || kv.key.to_lowercase() == "title" {
            return Some(kv.value);
        }
    }

    // Look for markdown header
    for line in content.lines() {
        if let Some(title) = line.trim().strip_prefix('#') {
            let title = title.trim_start_matches('#').trim();
            if !title.is_empty() {
                return Some(title.to_string());
            }
        }
    }

    None
}

/// Normalize content for deduplication comparison.
fn normalize_for_dedup(content: &str) -> String {
    content
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::types::SourceSpan;

    fn make_block(block_type: ContentBlockType, content: &str, confidence: f32) -> ContentBlock {
        ContentBlock::new(
            block_type,
            content.to_string(),
            confidence,
            SourceSpan::new(0, content.len(), 1, 1),
            vec![],
        )
    }

    #[test]
    fn test_generator_basic() {
        let generator = SkillGenerator::new();
        let blocks = vec![
            make_block(ContentBlockType::Rule, "Always handle errors", 0.8),
            make_block(ContentBlockType::Rule, "Never use eval", 0.9),
            make_block(ContentBlockType::Context, "This is background info", 0.7),
        ];

        let result = generator.generate(blocks, &ImportHints::default());

        assert_eq!(result.stats.rules_count, 2);
        assert_eq!(result.stats.context_count, 1);
        assert!(result.skill.sections.iter().any(|s| s.id == "rules"));
        assert!(result.skill.sections.iter().any(|s| s.id == "context"));
    }

    #[test]
    fn test_generator_with_examples() {
        let generator = SkillGenerator::new();
        let blocks = vec![make_block(
            ContentBlockType::Example,
            "## Good Example\n```rust\nfn main() {}\n```",
            0.9,
        )];

        let result = generator.generate(blocks, &ImportHints::default());

        assert_eq!(result.stats.examples_count, 1);
        let examples_section = result.skill.sections.iter().find(|s| s.id == "examples");
        assert!(examples_section.is_some());
        let block = &examples_section.unwrap().blocks[0];
        assert!(block.content.contains("## Good Example"));
        assert!(block.content.contains("fn main()"));
    }

    #[test]
    fn test_generator_with_checklist() {
        let generator = SkillGenerator::new();
        let blocks = vec![make_block(
            ContentBlockType::Checklist,
            "- [ ] Check one\n- [x] Check two\n- [ ] Check three",
            0.8,
        )];

        let result = generator.generate(blocks, &ImportHints::default());

        assert_eq!(result.stats.checklist_count, 3);
    }

    #[test]
    fn test_generator_low_confidence_filtering() {
        let mut config = GeneratorConfig::default();
        config.min_confidence = 0.5;
        let generator = SkillGenerator::with_config(config);

        let blocks = vec![
            make_block(ContentBlockType::Rule, "High confidence rule", 0.8),
            make_block(ContentBlockType::Rule, "Low confidence rule", 0.2),
        ];

        let result = generator.generate(blocks, &ImportHints::default());

        assert_eq!(result.stats.rules_count, 1);
        assert_eq!(result.stats.low_confidence_skipped, 1);
        assert!(
            result
                .warnings
                .iter()
                .any(|w| matches!(w, Warning::LowConfidence { .. }))
        );
    }

    #[test]
    fn test_generator_deduplication() {
        let generator = SkillGenerator::new();
        let blocks = vec![
            make_block(ContentBlockType::Rule, "Always handle errors", 0.8),
            make_block(ContentBlockType::Rule, "ALWAYS HANDLE ERRORS", 0.7), // Duplicate
        ];

        let result = generator.generate(blocks, &ImportHints::default());

        assert_eq!(result.stats.rules_count, 1);
        assert_eq!(result.stats.duplicates_skipped, 1);
    }

    #[test]
    fn test_generator_unknown_handling_context() {
        let mut config = GeneratorConfig::default();
        config.unknown_handling = UnknownHandling::AddToContext;
        let generator = SkillGenerator::with_config(config);

        let blocks = vec![make_block(
            ContentBlockType::Unknown,
            "Unknown content",
            0.5,
        )];

        let result = generator.generate(blocks, &ImportHints::default());

        assert_eq!(result.stats.context_count, 1);
        assert_eq!(result.stats.unknown_count, 1);
    }

    #[test]
    fn test_generator_unknown_handling_discard() {
        let mut config = GeneratorConfig::default();
        config.unknown_handling = UnknownHandling::Discard;
        let generator = SkillGenerator::with_config(config);

        let blocks = vec![make_block(
            ContentBlockType::Unknown,
            "Unknown content",
            0.5,
        )];

        let result = generator.generate(blocks, &ImportHints::default());

        assert_eq!(result.stats.context_count, 0);
        assert!(
            result
                .warnings
                .iter()
                .any(|w| matches!(w, Warning::Discarded { .. }))
        );
    }

    #[test]
    fn test_generator_metadata_inference() {
        let generator = SkillGenerator::new();
        let blocks = vec![
            make_block(
                ContentBlockType::Metadata,
                "---\nid: my-skill\nname: My Great Skill\n---",
                0.9,
            ),
            make_block(
                ContentBlockType::Context,
                "This skill helps with testing.",
                0.7,
            ),
            make_block(ContentBlockType::Rule, "Always test your code", 0.8),
        ];

        let hints = ImportHints::default();
        let result = generator.generate(blocks, &hints);

        assert_eq!(result.skill.metadata.id, "my-skill");
        assert_eq!(result.skill.metadata.name, "My Great Skill");
        assert!(result.skill.metadata.tags.contains(&"testing".to_string()));
    }

    #[test]
    fn test_generator_hints_override_inference() {
        let generator = SkillGenerator::new();
        let blocks = vec![make_block(
            ContentBlockType::Metadata,
            "id: inferred-id\nname: Inferred Name",
            0.9,
        )];

        let hints = ImportHints {
            suggested_id: Some("hint-id".to_string()),
            suggested_name: Some("Hint Name".to_string()),
            ..Default::default()
        };
        let result = generator.generate(blocks, &hints);

        assert_eq!(result.skill.metadata.id, "hint-id");
        assert_eq!(result.skill.metadata.name, "Hint Name");
    }

    #[test]
    fn test_generator_pitfalls() {
        let generator = SkillGenerator::new();
        let blocks = vec![make_block(
            ContentBlockType::Pitfall,
            "⚠️ Don't use eval in production",
            0.85,
        )];

        let result = generator.generate(blocks, &ImportHints::default());

        assert_eq!(result.stats.pitfalls_count, 1);
        let pitfalls_section = result.skill.sections.iter().find(|s| s.id == "pitfalls");
        assert!(pitfalls_section.is_some());
        // Emoji should be stripped
        assert!(!pitfalls_section.unwrap().blocks[0].content.contains("⚠️"));
    }

    #[test]
    fn test_generator_missing_rules_warning() {
        let generator = SkillGenerator::new();
        let blocks = vec![make_block(
            ContentBlockType::Context,
            "Just some context",
            0.7,
        )];

        let result = generator.generate(blocks, &ImportHints::default());

        assert!(
            result
                .warnings
                .iter()
                .any(|w| matches!(w, Warning::MissingContent { .. }))
        );
    }

    #[test]
    fn test_normalize_for_dedup() {
        assert_eq!(
            normalize_for_dedup("Always  handle   errors!"),
            "always handle errors"
        );
        assert_eq!(
            normalize_for_dedup("ALWAYS HANDLE ERRORS"),
            "always handle errors"
        );
    }

    #[test]
    fn test_generator_stats() {
        let generator = SkillGenerator::new();
        let blocks = vec![
            make_block(ContentBlockType::Rule, "Rule 1", 0.8),
            make_block(ContentBlockType::Rule, "Rule 2", 0.6),
            make_block(ContentBlockType::Example, "```\ncode\n```", 0.9),
        ];

        let result = generator.generate(blocks, &ImportHints::default());

        assert_eq!(result.stats.total_blocks, 3);
        assert_eq!(result.stats.rules_count, 2);
        assert_eq!(result.stats.examples_count, 1);
        assert!(result.stats.avg_confidence > 0.7);
    }
}
