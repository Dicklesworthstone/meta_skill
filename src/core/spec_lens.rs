//! Round-trip spec <-> markdown mapping

use serde_json::Value as JsonValue;

use super::skill::{BlockType, SkillBlock, SkillMetadata, SkillSection, SkillSpec};
use crate::error::{MsError, Result};

/// Bidirectional mapping between SkillSpec and SKILL.md.
pub struct SpecLens;

impl SpecLens {
    /// Compile a SkillSpec to deterministic markdown.
    pub fn compile(&self, spec: &SkillSpec) -> String {
        compile_markdown(spec)
    }

    /// Parse markdown into a SkillSpec.
    pub fn parse(&self, md: &str) -> Result<SkillSpec> {
        parse_markdown(md)
    }

    /// Verify round-trip stability for a spec.
    pub fn verify_roundtrip(&self, spec: &SkillSpec) -> Result<()> {
        let md = self.compile(spec);
        let parsed = self.parse(&md)?;
        if !spec_equivalent(spec, &parsed)? {
            return Err(MsError::ValidationFailed(
                "round-trip spec mismatch".to_string(),
            ));
        }
        Ok(())
    }
}

/// Parse a SKILL.md file into a SkillSpec.
pub fn parse_markdown(content: &str) -> Result<SkillSpec> {
    let mut name = String::new();
    let mut description_lines = Vec::new();
    let mut sections: Vec<SkillSection> = Vec::new();
    let mut metadata = SkillMetadata::default();

    let mut current_section: Option<SkillSection> = None;
    let mut in_description = false;
    let mut in_code_block = false;
    let mut code_lines: Vec<String> = Vec::new();
    let mut paragraph_lines: Vec<String> = Vec::new();
    let mut in_frontmatter = false;
    let mut frontmatter_lines: Vec<String> = Vec::new();
    let mut lines_iter = content.lines().peekable();

    // Check for frontmatter start
    if let Some(first_line) = lines_iter.peek() {
        if first_line.trim() == "---" {
            in_frontmatter = true;
            lines_iter.next(); // Consume first ---
        }
    }

    let flush_paragraph = |section: &mut SkillSection, lines: &mut Vec<String>| {
        if lines.is_empty() {
            return;
        }
        let content = lines.join("\n").trim_end().to_string();
        lines.clear();
        if content.is_empty() {
            return;
        }
        section.blocks.push(SkillBlock {
            id: format!("{}-block-{}", section.id, section.blocks.len() + 1),
            block_type: BlockType::Text,
            content,
        });
    };

    for line in lines_iter {
        if in_frontmatter {
            if line.trim() == "---" {
                in_frontmatter = false;
                let yaml = frontmatter_lines.join("\n");
                match serde_yaml::from_str::<SkillMetadata>(&yaml) {
                    Ok(meta) => metadata = meta,
                    Err(e) => eprintln!("Failed to parse frontmatter: {}\nYAML:\n{}", e, yaml),
                }
                continue;
            }
            frontmatter_lines.push(line.to_string());
            continue;
        }

        if let Some(title) = line.strip_prefix("# ") {
            name = title.trim().to_string();
            in_description = true;
            continue;
        }

        if let Some(title) = line.strip_prefix("## ") {
            if let Some(section) = current_section.as_mut() {
                flush_paragraph(section, &mut paragraph_lines);
            }
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
                description_lines.push(line.trim_end().to_string());
            }
            continue;
        }

        let Some(section) = current_section.as_mut() else {
            continue;
        };

        if line.trim_start().starts_with("```") {
            if in_code_block {
                code_lines.push(line.to_string());
                let content = code_lines.join("\n");
                code_lines.clear();
                in_code_block = false;
                flush_paragraph(section, &mut paragraph_lines);
                section.blocks.push(SkillBlock {
                    id: format!("{}-block-{}", section.id, section.blocks.len() + 1),
                    block_type: BlockType::Code,
                    content,
                });
            } else {
                flush_paragraph(section, &mut paragraph_lines);
                in_code_block = true;
                code_lines.push(line.to_string());
            }
            continue;
        }

        if in_code_block {
            code_lines.push(line.to_string());
            continue;
        }

        if line.trim().is_empty() {
            flush_paragraph(section, &mut paragraph_lines);
        } else {
            paragraph_lines.push(line.trim_end().to_string());
        }
    }

    if let Some(section) = current_section.as_mut() {
        flush_paragraph(section, &mut paragraph_lines);
    }
    if in_code_block && !code_lines.is_empty() {
        if let Some(section) = current_section.as_mut() {
            section.blocks.push(SkillBlock {
                id: format!("{}-block-{}", section.id, section.blocks.len() + 1),
                block_type: BlockType::Code,
                content: code_lines.join("\n"),
            });
        }
    }

    if let Some(section) = current_section.take() {
        sections.push(section);
    }

    let id = if !metadata.id.is_empty() {
        metadata.id.clone()
    } else if name.is_empty() {
        "".to_string()
    } else {
        slugify(&name)
    };

    // If description wasn't in frontmatter, use extracted one
    if metadata.description.is_empty() {
        metadata.description = description_lines.join("\n").trim().to_string();
    }

    // If name wasn't in frontmatter, use extracted one
    if metadata.name.is_empty() {
        metadata.name = name;
    }

    if metadata.version.is_empty() {
        metadata.version = "0.1.0".to_string();
    }

    metadata.id = id;

    Ok(SkillSpec {
        format_version: SkillSpec::FORMAT_VERSION.to_string(),
        metadata,
        sections,
    })
}

/// Compile a SkillSpec back to markdown.
pub fn compile_markdown(spec: &SkillSpec) -> String {
    let mut output = String::new();

    // Serialize metadata to YAML frontmatter
    if let Ok(yaml) = serde_yaml::to_string(&spec.metadata) {
        output.push_str("---\n");
        output.push_str(yaml.trim());
        output.push_str("\n---\n\n");
    }

    output.push_str(&format!("# {}\n\n", spec.metadata.name));

    if !spec.metadata.description.is_empty() {
        output.push_str(spec.metadata.description.trim_end());
        output.push_str("\n\n");
    }

    for section in &spec.sections {
        output.push_str(&format!("## {}\n\n", section.title));
        for block in &section.blocks {
            match block.block_type {
                BlockType::Code => {
                    let content = block.content.trim_end();
                    if content.starts_with("```") {
                        output.push_str(content);
                        output.push_str("\n\n");
                    } else {
                        output.push_str("```\n");
                        output.push_str(content);
                        output.push_str("\n```\n\n");
                    }
                }
                _ => {
                    output.push_str(block.content.trim_end());
                    output.push_str("\n\n");
                }
            }
        }
    }

    output.trim_end().to_string() + "\n"
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

fn spec_equivalent(left: &SkillSpec, right: &SkillSpec) -> Result<bool> {
    let left_json = serde_json::to_value(left)
        .map_err(|err| MsError::ValidationFailed(format!("serialize spec: {err}")))?;
    let right_json = serde_json::to_value(right)
        .map_err(|err| MsError::ValidationFailed(format!("serialize spec: {err}")))?;
    Ok(json_equivalent(&left_json, &right_json))
}

fn json_equivalent(left: &JsonValue, right: &JsonValue) -> bool {
    match (left, right) {
        (JsonValue::Array(a), JsonValue::Array(b)) => a == b,
        (JsonValue::Object(a), JsonValue::Object(b)) => a == b,
        _ => left == right,
    }
}

#[cfg(test)]
mod tests {
    use super::{compile_markdown, parse_markdown};

    #[test]
    fn roundtrip_simple_markdown() {
        let md = "# Sample Skill\n\nA short description.\n\n## Usage\n\nDo the thing.\n\n```bash\nls -la\n```\n";
        let parsed = parse_markdown(md).expect("parse");
        let compiled = compile_markdown(&parsed);

        let expected = "---\nid: sample-skill\nname: Sample Skill\nversion: 0.1.0\ndescription: A short description.\ntags: []\nrequires: []\nprovides: []\nplatforms: []\nauthor: null\nlicense: null\n---\n\n# Sample Skill\n\nA short description.\n\n## Usage\n\nDo the thing.\n\n```bash\nls -la\n```\n";

        assert_eq!(compiled, expected);
    }
    #[test]
    fn parse_frontmatter_tags() {
        let md = "---\nid: tagged-skill\nname: Tagged Skill\nversion: 0.1.0\ndescription: A test skill\ntags: [rust, backend]\nrequires: []\nprovides: []\nplatforms: []\n---\n\n# Tagged Skill\n\nDescription.\n";
        let parsed = parse_markdown(md).expect("parse");
        assert_eq!(parsed.metadata.name, "Tagged Skill");
        assert_eq!(parsed.metadata.tags, vec!["rust", "backend"]);
    }
}
