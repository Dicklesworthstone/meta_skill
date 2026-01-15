//! Micro-slicing engine for SkillSpec content.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::skill::{BlockType, SkillBlock, SkillSection, SkillSlice, SkillSpec, SliceType};

/// Index of slices generated for a skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSliceIndex {
    pub slices: Vec<SkillSlice>,
    pub generated_at: DateTime<Utc>,
}

/// Slice a SkillSpec into atomic slices for packing.
pub struct SkillSlicer;

impl SkillSlicer {
    pub fn slice(spec: &SkillSpec) -> SkillSliceIndex {
        let mut slices = Vec::new();
        let mut counters: HashMap<&'static str, usize> = HashMap::new();

        for section in &spec.sections {
            slice_section(spec, section, &mut slices, &mut counters);
        }

        SkillSliceIndex {
            slices,
            generated_at: Utc::now(),
        }
    }

    pub fn estimate_total_tokens(spec: &SkillSpec) -> usize {
        let mut total = 0;
        for section in &spec.sections {
            for block in &section.blocks {
                total += estimate_tokens(&block.content);
            }
        }
        total
    }
}

fn slice_section(
    spec: &SkillSpec,
    section: &SkillSection,
    slices: &mut Vec<SkillSlice>,
    counters: &mut HashMap<&'static str, usize>,
) {
    let mut first = true;
    for block in &section.blocks {
        if block.content.trim().is_empty() {
            continue;
        }

        let slice_type = classify_block(block);
        let content = slice_content(section, block, first);
        let id = slice_id(block, slice_type, counters);
        let token_estimate = estimate_tokens(&content);
        let utility_score = utility_score(slice_type);
        let coverage_group = coverage_group(slice_type);
        let mut tags = spec.metadata.tags.clone();
        tags.push(slice_type_tag(slice_type).to_string());

        slices.push(SkillSlice {
            id,
            slice_type,
            token_estimate,
            utility_score,
            coverage_group,
            tags,
            requires: Vec::new(),
            condition: None,
            content,
        });

        first = false;
    }
}

fn classify_block(block: &SkillBlock) -> SliceType {
    match block.block_type {
        BlockType::Rule => {
            if is_policy_block(block) {
                SliceType::Policy
            } else {
                SliceType::Rule
            }
        }
        BlockType::Command => SliceType::Command,
        BlockType::Code => SliceType::Example,
        BlockType::Checklist => SliceType::Checklist,
        BlockType::Pitfall => SliceType::Pitfall,
        BlockType::Text => SliceType::Overview,
    }
}

fn is_policy_block(block: &SkillBlock) -> bool {
    let id = block.id.to_lowercase();
    id.starts_with("policy") || id.starts_with("invariant")
}

fn slice_content(section: &SkillSection, block: &SkillBlock, include_heading: bool) -> String {
    let body = block.content.trim_end().to_string();
    if include_heading && !section.title.trim().is_empty() {
        format!("## {}\n\n{}", section.title.trim(), body)
    } else {
        body
    }
}

fn slice_id(
    block: &SkillBlock,
    slice_type: SliceType,
    counters: &mut HashMap<&'static str, usize>,
) -> String {
    if !block.id.trim().is_empty() {
        return block.id.clone();
    }
    let prefix = slice_type_tag(slice_type);
    let counter = counters.entry(prefix).or_insert(0);
    *counter += 1;
    format!("{prefix}-{counter}")
}

fn slice_type_tag(slice_type: SliceType) -> &'static str {
    match slice_type {
        SliceType::Rule => "rule",
        SliceType::Command => "command",
        SliceType::Example => "example",
        SliceType::Checklist => "checklist",
        SliceType::Pitfall => "pitfall",
        SliceType::Overview => "overview",
        SliceType::Reference => "reference",
        SliceType::Policy => "policy",
    }
}

fn coverage_group(slice_type: SliceType) -> Option<String> {
    Some(match slice_type {
        SliceType::Rule => "rules",
        SliceType::Command => "commands",
        SliceType::Example => "examples",
        SliceType::Checklist => "checklists",
        SliceType::Pitfall => "pitfalls",
        SliceType::Overview => "overview",
        SliceType::Reference => "reference",
        SliceType::Policy => "policy",
    }
    .to_string())
}

fn utility_score(slice_type: SliceType) -> f32 {
    match slice_type {
        SliceType::Policy => 0.95,
        SliceType::Rule => 0.9,
        SliceType::Pitfall => 0.85,
        SliceType::Checklist => 0.75,
        SliceType::Command => 0.7,
        SliceType::Example => 0.65,
        SliceType::Overview => 0.55,
        SliceType::Reference => 0.4,
    }
}

fn estimate_tokens(content: &str) -> usize {
    let chars = content.chars().count();
    let estimate = (chars + 3) / 4;
    estimate.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::skill::{SkillMetadata, SkillSection};

    #[test]
    fn test_slice_attaches_section_title() {
        let spec = SkillSpec {
            format_version: SkillSpec::FORMAT_VERSION.to_string(),
            metadata: SkillMetadata {
                id: "test".to_string(),
                name: "Test".to_string(),
                version: "0.1.0".to_string(),
                ..Default::default()
            },
            sections: vec![SkillSection {
                id: "s1".to_string(),
                title: "Intro".to_string(),
                blocks: vec![SkillBlock {
                    id: "rule-1".to_string(),
                    block_type: BlockType::Rule,
                    content: "Always sanitize input.".to_string(),
                }],
            }],
        };

        let index = SkillSlicer::slice(&spec);
        assert_eq!(index.slices.len(), 1);
        assert!(index.slices[0].content.starts_with("## Intro"));
    }

    #[test]
    fn test_policy_detection() {
        let spec = SkillSpec {
            format_version: SkillSpec::FORMAT_VERSION.to_string(),
            metadata: SkillMetadata {
                id: "test".to_string(),
                name: "Test".to_string(),
                version: "0.1.0".to_string(),
                ..Default::default()
            },
            sections: vec![SkillSection {
                id: "s1".to_string(),
                title: "Safety".to_string(),
                blocks: vec![SkillBlock {
                    id: "policy-1".to_string(),
                    block_type: BlockType::Rule,
                    content: "Never run destructive commands.".to_string(),
                }],
            }],
        };

        let index = SkillSlicer::slice(&spec);
        assert_eq!(index.slices[0].slice_type, SliceType::Policy);
    }

    #[test]
    fn test_token_estimate_nonzero() {
        let estimate = estimate_tokens("abcd");
        assert_eq!(estimate, 1);
        let estimate = estimate_tokens("abcdefgh");
        assert_eq!(estimate, 2);
    }
}
