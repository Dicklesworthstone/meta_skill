//! Round-trip spec <-> markdown mapping

use serde_json::Value as JsonValue;

use super::skill::{BlockType, SkillBlock, SkillMetadata, SkillSection, SkillSpec};
use crate::error::{MsError, Result};

const PREAMBLE_SECTION_ID: &str = "__preamble";

/// Bidirectional mapping between `SkillSpec` and SKILL.md.
pub struct SpecLens;

impl SpecLens {
    /// Compile a `SkillSpec` to deterministic markdown.
    #[must_use]
    pub fn compile(&self, spec: &SkillSpec) -> String {
        compile_markdown(spec)
    }

    /// Parse markdown into a `SkillSpec`.
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

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct FrontmatterSpec {
    #[serde(flatten)]
    metadata: SkillMetadata,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    extends: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    replace_rules: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    replace_examples: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    replace_pitfalls: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    replace_checklist: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    includes: Vec<super::skill::SkillInclude>,
}

fn classify_markdown_block(
    section_id: &str,
    section_title: &str,
    content: &str,
    is_fenced_code: bool,
) -> BlockType {
    let sec_id = section_id.to_ascii_lowercase();
    let sec_title = section_title.to_ascii_lowercase();
    let is_command_section = matches!(
        sec_id.as_str(),
        "commands" | "command" | "cli" | "cli-commands" | "quick-commands" | "command-reference"
    ) || sec_title == "commands"
        || sec_title == "cli";

    if is_fenced_code {
        if is_command_section {
            return BlockType::Command;
        }
        return BlockType::Code;
    }

    // Check task list syntax first (`- [ ] `, `- [x] `, `* [ ] `, etc.)
    let has_checkbox = content.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("- [ ] ")
            || trimmed.starts_with("- [x] ")
            || trimmed.starts_with("- [X] ")
            || trimmed.starts_with("* [ ] ")
            || trimmed.starts_with("* [x] ")
            || trimmed.starts_with("* [X] ")
    });
    if has_checkbox
        || sec_id.contains("checklist")
        || sec_title.contains("checklist")
        || matches!(sec_id.as_str(), "verification" | "pre-flight" | "preflight")
    {
        return BlockType::Checklist;
    }

    if sec_id.contains("pitfall")
        || sec_id.contains("antipattern")
        || sec_id.contains("anti-pattern")
        || sec_id.contains("mistake")
        || sec_id.contains("gotcha")
        || sec_id.contains("caveat")
        || sec_title.contains("pitfall")
        || sec_title.contains("anti-pattern")
        || sec_title.contains("mistake")
        || sec_title.contains("gotcha")
    {
        return BlockType::Pitfall;
    }

    if sec_id.contains("rule")
        || sec_id.contains("guideline")
        || sec_id.contains("principle")
        || sec_id.contains("invariant")
        || sec_id.contains("policy")
        || sec_id.contains("policies")
        || sec_id.contains("best-practice")
        || sec_id.contains("constraint")
        || sec_title.contains("rule")
        || sec_title.contains("guideline")
        || sec_title.contains("principle")
        || sec_title.contains("invariant")
        || sec_title.contains("best practice")
    {
        return BlockType::Rule;
    }

    if is_command_section
        || content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .all(|l| l.trim_start().starts_with("$ "))
            && !content.trim().is_empty()
    {
        return BlockType::Command;
    }

    BlockType::Text
}

/// Parse a SKILL.md file into a `SkillSpec`.
pub fn parse_markdown(content: &str) -> Result<SkillSpec> {
    let mut name = String::new();
    let mut description_lines = Vec::new();
    let mut sections: Vec<SkillSection> = Vec::new();
    let mut frontmatter = FrontmatterSpec::default();

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
        let block_type = classify_markdown_block(&section.id, &section.title, &content, false);
        section.blocks.push(SkillBlock {
            id: format!("{}-block-{}", section.id, section.blocks.len() + 1),
            block_type,
            content,
        });
    };

    for line in lines_iter {
        if in_frontmatter {
            if line.trim() == "---" {
                in_frontmatter = false;
                let yaml = frontmatter_lines.join("\n");
                match serde_yaml::from_str::<FrontmatterSpec>(&yaml) {
                    Ok(fm) => frontmatter = fm,
                    Err(e) => eprintln!("Failed to parse frontmatter: {e}\nYAML:\n{yaml}"),
                }
                continue;
            }
            frontmatter_lines.push(line.to_string());
            continue;
        }

        // Consult the code-fence state BEFORE heading detection. A start-of-line
        // `# ` / `## ` that appears inside a fenced code block is code (e.g. a
        // shell comment), not a markdown heading. Without the `!in_code_block`
        // guard these lines hit the heading branches and `continue`, so they
        // never reach the code accumulator below — truncating/emptying the
        // fenced block and mis-splitting sections in the normalized SKILL.md
        // that `ms load --full` and the MCP `load` server serve (issue #134).
        if let Some(title) = line.strip_prefix("# ").filter(|_| !in_code_block) {
            name = title.trim().to_string();
            // The first paragraph may be either the canonical description
            // repeated by `compile_markdown`, or distinct body prose written
            // below the H1. Collect it first and disambiguate after parsing so
            // canonical output remains stable while real preamble content is
            // retained (issue #150).
            in_description = true;
            continue;
        }

        if let Some(title) = line.strip_prefix("## ").filter(|_| !in_code_block) {
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

        if current_section.is_none() && !line.trim().is_empty() {
            current_section = Some(SkillSection {
                id: PREAMBLE_SECTION_ID.to_string(),
                title: String::new(),
                blocks: Vec::new(),
            });
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
                let block_type =
                    classify_markdown_block(&section.id, &section.title, &content, true);
                section.blocks.push(SkillBlock {
                    id: format!("{}-block-{}", section.id, section.blocks.len() + 1),
                    block_type,
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
            let content = code_lines.join("\n");
            let block_type = classify_markdown_block(&section.id, &section.title, &content, true);
            section.blocks.push(SkillBlock {
                id: format!("{}-block-{}", section.id, section.blocks.len() + 1),
                block_type,
                content,
            });
        }
    }

    if let Some(section) = current_section.take() {
        sections.push(section);
    }

    let mut metadata = frontmatter.metadata;

    // Derive the skill id honoring the Anthropic SKILL.md contract where `name`
    // is the required identity field. Precedence: an explicit frontmatter `id:`,
    // then an H1 `# ` title, then the frontmatter `name:`. A name-only SKILL.md
    // (no `id:`, no H1, body starting with prose or `## `) must still yield a
    // stable slugified id rather than an empty one that `ms index` rejects
    // (issue #141).
    let id = if !metadata.id.is_empty() {
        metadata.id.clone()
    } else if !name.is_empty() {
        slugify(&name)
    } else if !metadata.name.is_empty() {
        slugify(&metadata.name)
    } else {
        String::new()
    };

    let extracted_description = description_lines.join("\n").trim().to_string();

    // Without frontmatter, the first post-H1 paragraph is the description. If
    // frontmatter already supplied a description, an exact repeated paragraph
    // is compiler-generated canonical text; any distinct paragraph is body
    // content and belongs at the front of the heading-less preamble section.
    if metadata.description.is_empty() {
        metadata.description = extracted_description;
    } else if !extracted_description.is_empty()
        && extracted_description.trim() != metadata.description.trim()
    {
        let preamble = if sections
            .first()
            .is_some_and(|section| section.id == PREAMBLE_SECTION_ID)
        {
            &mut sections[0]
        } else {
            sections.insert(
                0,
                SkillSection {
                    id: PREAMBLE_SECTION_ID.to_string(),
                    title: String::new(),
                    blocks: Vec::new(),
                },
            );
            &mut sections[0]
        };
        preamble.blocks.insert(
            0,
            SkillBlock {
                id: String::new(),
                block_type: BlockType::Text,
                content: extracted_description,
            },
        );
        for (index, block) in preamble.blocks.iter_mut().enumerate() {
            block.id = format!("{PREAMBLE_SECTION_ID}-block-{}", index + 1);
        }
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
        extends: frontmatter.extends,
        replace_rules: frontmatter.replace_rules,
        replace_examples: frontmatter.replace_examples,
        replace_pitfalls: frontmatter.replace_pitfalls,
        replace_checklist: frontmatter.replace_checklist,
        includes: frontmatter.includes,
    })
}

/// Compile a `SkillSpec` back to markdown.
#[must_use]
pub fn compile_markdown(spec: &SkillSpec) -> String {
    let mut output = String::new();

    // Serialize metadata and composition fields to YAML frontmatter
    let frontmatter = FrontmatterSpec {
        metadata: spec.metadata.clone(),
        extends: spec.extends.clone(),
        replace_rules: spec.replace_rules,
        replace_examples: spec.replace_examples,
        replace_pitfalls: spec.replace_pitfalls,
        replace_checklist: spec.replace_checklist,
        includes: spec.includes.clone(),
    };
    if let Ok(yaml) = serde_yaml::to_string(&frontmatter) {
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
        if section.id != PREAMBLE_SECTION_ID || !section.title.is_empty() {
            output.push_str(&format!("## {}\n\n", section.title));
        }
        for block in &section.blocks {
            if matches!(block.block_type, BlockType::Code | BlockType::Command) {
                let content = block.content.trim_end();
                if content.starts_with("```") || content.starts_with("$ ") {
                    output.push_str(content);
                    output.push_str("\n\n");
                } else {
                    output.push_str("```\n");
                    output.push_str(content);
                    output.push_str("\n```\n\n");
                }
            } else {
                output.push_str(block.content.trim_end());
                output.push_str("\n\n");
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
    use super::{BlockType, PREAMBLE_SECTION_ID, compile_markdown, parse_markdown};

    #[test]
    fn fenced_hash_comments_are_not_parsed_as_headings() {
        // A `## Demo` section whose bash fence contains start-of-line `#`/`##`
        // comment lines must round-trip with the code block INTACT: the comment
        // lines must be preserved inside a single code block, NOT swallowed as
        // headings that truncate the block or mis-split the document (issue #134).
        let md = "# Real Skill\n\nA description.\n\n## Demo\n\n```bash\n# not a heading\n## also not a heading\necho hi\n```\n";
        let parsed = parse_markdown(md).expect("parse");

        // The H1-derived name must survive (not overwritten by `# not a heading`).
        assert_eq!(parsed.metadata.name, "Real Skill");

        // Exactly one section, titled from the real `## Demo` heading — the
        // `## also not a heading` fence line must NOT open a second section.
        assert_eq!(
            parsed.sections.len(),
            1,
            "expected exactly one section, got: {:?}",
            parsed
                .sections
                .iter()
                .map(|s| s.title.clone())
                .collect::<Vec<_>>()
        );
        let section = &parsed.sections[0];
        assert_eq!(section.title, "Demo");

        // The section must contain a single code block that preserves all three
        // fenced lines (both `#` comments and the command), fences included.
        let code_blocks: Vec<&_> = section
            .blocks
            .iter()
            .filter(|b| b.block_type == BlockType::Code)
            .collect();
        assert_eq!(
            code_blocks.len(),
            1,
            "expected exactly one code block, got {}",
            code_blocks.len()
        );
        let code = &code_blocks[0].content;
        assert!(
            code.contains("# not a heading"),
            "code block lost the `#` comment line: {code:?}"
        );
        assert!(
            code.contains("## also not a heading"),
            "code block lost the `##` comment line: {code:?}"
        );
        assert!(
            code.contains("echo hi"),
            "code block lost the command line: {code:?}"
        );

        // End-to-end: recompiling must still contain the comment lines (the
        // normalized copy served by `ms load --full` is not garbled).
        let compiled = compile_markdown(&parsed);
        assert!(compiled.contains("# not a heading"), "compiled: {compiled}");
        assert!(
            compiled.contains("## also not a heading"),
            "compiled: {compiled}"
        );
    }

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

    #[test]
    fn frontmatter_description_does_not_discard_distinct_h1_preamble() {
        let md = "---\nname: Preamble Skill\ndescription: Short catalog summary\n---\n\n# Preamble Skill\n\nThis intro carries unique token preamblecanary.\n\nA second intro paragraph must survive too.\n\n## Notes\n\nSection content.\n";
        let parsed = parse_markdown(md).expect("parse");

        assert_eq!(parsed.metadata.description, "Short catalog summary");
        let preamble = parsed
            .sections
            .first()
            .expect("heading-less preamble section");
        assert_eq!(preamble.id, PREAMBLE_SECTION_ID);
        assert!(preamble.title.is_empty());
        assert_eq!(preamble.blocks.len(), 2);
        assert!(preamble.blocks[0].content.contains("preamblecanary"));
        assert!(
            preamble.blocks[1]
                .content
                .contains("second intro paragraph")
        );

        let compiled = compile_markdown(&parsed);
        assert!(compiled.contains("preamblecanary"));
        assert!(compiled.contains("A second intro paragraph must survive too."));
        assert!(!compiled.contains("## \n"));

        let reparsed = parse_markdown(&compiled).expect("reparse canonical markdown");
        assert_eq!(
            reparsed.sections[0].blocks.len(),
            2,
            "the canonical metadata description must not reappear as a body block"
        );
        assert!(
            reparsed.sections[0].blocks[0]
                .content
                .contains("preamblecanary")
        );
    }

    #[test]
    fn name_only_frontmatter_derives_id_from_name() {
        // A SKILL.md that follows the Anthropic contract — a `name:` in the
        // frontmatter but no explicit `id:` and no `# ` H1, with a body that
        // begins with prose and a `## ` section — must derive a stable
        // slugified id from `name` rather than yielding an empty id that
        // `ms index` rejects (issue #141).
        let md = "---\nname: My Cool Skill\ndescription: Does cool things\n---\n\nThis skill does cool things without an H1.\n\n## Usage\n\nRun it.\n";
        let parsed = parse_markdown(md).expect("parse");
        assert_eq!(
            parsed.metadata.id, "my-cool-skill",
            "id should be slugified from the frontmatter name"
        );
        assert_eq!(parsed.metadata.name, "My Cool Skill");
        // The description from frontmatter is preserved.
        assert_eq!(parsed.metadata.description, "Does cool things");
        // The `## Usage` section is still parsed (body prose did not swallow it).
        assert!(parsed.sections.iter().any(|s| s.title == "Usage"));
        assert!(
            parsed.sections[0].blocks[0]
                .content
                .contains("without an H1"),
            "body prose before the first H2 must not be discarded"
        );
    }

    #[test]
    fn explicit_id_wins_over_name_derivation() {
        // When an explicit `id:` is present it must take precedence over any
        // name-derived slug.
        let md = "---\nid: custom-id\nname: A Different Name\n---\n\nBody text.\n\n## Section\n\nContent.\n";
        let parsed = parse_markdown(md).expect("parse");
        assert_eq!(parsed.metadata.id, "custom-id");
    }

    #[test]
    fn classifies_semantic_block_types_and_roundtrips_composition_frontmatter() {
        let md = r#"---
id: child-skill
name: Child Skill
version: 0.2.0
description: Extends base and includes helpers
extends: base-skill
replace_rules: true
includes:
  - skill: helper-skill
    into: rules
    prefix: "[Helper] "
---

# Child Skill

Extends base and includes helpers

## Overview

General context prose.

## Rules

- Always validate inputs
- Never ignore errors

## Pitfalls

- Swallowing errors silently

## Checklist

- [ ] Run unit tests
- [x] Verify clippy

## Commands

```bash
cargo test
```

## Examples

```rust
let x = 42;
```
"#;
        let parsed = parse_markdown(md).expect("parse");
        assert_eq!(parsed.extends.as_deref(), Some("base-skill"));
        assert!(parsed.replace_rules);
        assert!(!parsed.replace_examples);
        assert_eq!(parsed.includes.len(), 1);
        assert_eq!(parsed.includes[0].skill, "helper-skill");

        let by_id = |id: &str| {
            parsed
                .sections
                .iter()
                .find(|s| s.id == id)
                .expect("section")
                .blocks[0]
                .block_type
                .clone()
        };
        assert_eq!(by_id("overview"), BlockType::Text);
        assert_eq!(by_id("rules"), BlockType::Rule);
        assert_eq!(by_id("pitfalls"), BlockType::Pitfall);
        assert_eq!(by_id("checklist"), BlockType::Checklist);
        assert_eq!(by_id("commands"), BlockType::Command);
        assert_eq!(by_id("examples"), BlockType::Code);

        let compiled = compile_markdown(&parsed);
        let reparsed = parse_markdown(&compiled).expect("reparse");
        assert_eq!(reparsed.extends.as_deref(), Some("base-skill"));
        assert!(reparsed.replace_rules);
        assert_eq!(reparsed.includes.len(), 1);
        assert_eq!(reparsed.includes[0].skill, "helper-skill");
    }
}
