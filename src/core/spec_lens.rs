//! Round-trip spec <-> markdown mapping

use crate::error::Result;
use super::skill::{BlockType, SkillBlock, SkillMetadata, SkillSection, SkillSpec};

/// Parse a SKILL.md file into a SkillSpec
pub fn parse_markdown(content: &str) -> Result<SkillSpec> {
    let mut name = String::new();
    let mut description_lines = Vec::new();
    let mut sections: Vec<SkillSection> = Vec::new();

    let mut current_section: Option<SkillSection> = None;
    let mut in_description = false;

    for line in content.lines() {
        if let Some(title) = line.strip_prefix("# ") {
            name = title.trim().to_string();
            in_description = true;
            continue;
        }

        if let Some(title) = line.strip_prefix("## ") {
            if let Some(section) = current_section.take() {
                sections.push(section);
            }
            current_section = Some(SkillSection {
                id: slugify(title),
                title: title.trim().to_string(),
                blocks: Vec::new(),
            });
            in_description = false;
            continue;
        }

        if in_description {
            if line.trim().is_empty() {
                if !description_lines.is_empty() {
                    in_description = false;
                }
            } else {
                description_lines.push(line.trim().to_string());
            }
            continue;
        }

        if let Some(section) = current_section.as_mut() {
            if !line.trim().is_empty() {
                section.blocks.push(SkillBlock {
                    id: format!("{}-block-{}", section.id, section.blocks.len() + 1),
                    block_type: BlockType::Text,
                    content: line.to_string(),
                });
            }
        }
    }

    if let Some(section) = current_section.take() {
        sections.push(section);
    }

    let id = if name.is_empty() { "".to_string() } else { slugify(&name) };
    let description = description_lines.join(" ");

    Ok(SkillSpec {
        metadata: SkillMetadata {
            id,
            name,
            description,
            version: "0.1.0".to_string(),
            ..Default::default()
        },
        sections,
    })
}

/// Compile a SkillSpec back to markdown
pub fn compile_markdown(spec: &SkillSpec) -> String {
    let mut output = String::new();

    output.push_str(&format!("# {}\n\n", spec.metadata.name));

    if !spec.metadata.description.is_empty() {
        output.push_str(&format!("{}\n\n", spec.metadata.description));
    }

    for section in &spec.sections {
        output.push_str(&format!("## {}\n\n", section.title));
        for block in &section.blocks {
            output.push_str(&block.content);
            output.push_str("\n\n");
        }
    }

    output
}

fn slugify(input: &str) -> String {
    let lowered = input.trim().to_lowercase();
    let mut out = String::with_capacity(lowered.len());
    let mut last_was_dash = false;

    for ch in lowered.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_was_dash = false;
        } else if !last_was_dash {
            out.push('-');
            last_was_dash = true;
        }
    }

    out.trim_matches('-').to_string()
}
