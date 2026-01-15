//! Skill validation

use super::skill::SkillSpec;
use crate::error::{MsError, Result};
use crate::quality::ubs::{UbsClient, UbsResult};

/// Validate a skill specification
pub fn validate(spec: &SkillSpec) -> Result<Vec<ValidationWarning>> {
    let mut warnings = vec![];

    if spec.metadata.id.is_empty() {
        return Err(MsError::ValidationFailed("skill ID is required".into()));
    }

    if spec.metadata.name.is_empty() {
        return Err(MsError::ValidationFailed("skill name is required".into()));
    }

    if spec.metadata.description.is_empty() {
        warnings.push(ValidationWarning {
            field: "description".to_string(),
            message: "skill should have a description".to_string(),
        });
    }

    if spec.metadata.tags.is_empty() {
        warnings.push(ValidationWarning {
            field: "tags".to_string(),
            message: "skill should have at least one tag".to_string(),
        });
    }

    Ok(warnings)
}

/// Validate skill code blocks with UBS.
pub fn validate_with_ubs(spec: &SkillSpec, ubs: &UbsClient) -> Result<UbsResult> {
    let code_blocks = extract_code_blocks(spec);
    if code_blocks.is_empty() {
        return Ok(UbsResult {
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
            findings: Vec::new(),
        });
    }

    let temp_root = std::env::temp_dir().join(format!("ms-ubs-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_root).map_err(|err| {
        MsError::Config(format!(
            "create ubs temp dir {}: {err}",
            temp_root.display()
        ))
    })?;

    let mut paths = Vec::new();
    for (idx, block) in code_blocks.into_iter().enumerate() {
        let ext = extension_for_language(block.language.as_deref());
        let filename = format!("block_{idx}.{ext}");
        let path = temp_root.join(filename);
        std::fs::write(&path, block.content).map_err(|err| {
            MsError::Config(format!("write ubs temp file {}: {err}", path.display()))
        })?;
        paths.push(path);
    }

    let result = ubs.check_files(&paths);
    let _ = std::fs::remove_dir_all(&temp_root);
    result
}

/// A validation warning (not an error)
#[derive(Debug)]
pub struct ValidationWarning {
    pub field: String,
    pub message: String,
}

struct CodeBlock {
    language: Option<String>,
    content: String,
}

fn extract_code_blocks(spec: &SkillSpec) -> Vec<CodeBlock> {
    let mut blocks = Vec::new();
    for section in &spec.sections {
        for block in &section.blocks {
            if block.block_type != crate::core::skill::BlockType::Code {
                continue;
            }
            let (lang, content) = parse_code_block(&block.content);
            if !content.trim().is_empty() {
                blocks.push(CodeBlock {
                    language: lang,
                    content,
                });
            }
        }
    }
    blocks
}

fn parse_code_block(content: &str) -> (Option<String>, String) {
    let mut lines = content.lines();
    let first = lines.next().unwrap_or("");
    if first.trim_start().starts_with("```") {
        let lang = first.trim_start().trim_start_matches("```").trim();
        let mut body: Vec<&str> = lines.collect();
        if let Some(last) = body.last() {
            if last.trim() == "```" {
                body.pop();
            }
        }
        let text = body.join("\n");
        let language = if lang.is_empty() {
            None
        } else {
            Some(lang.to_string())
        };
        return (language, text);
    }

    (None, content.to_string())
}

fn extension_for_language(language: Option<&str>) -> &'static str {
    match language.unwrap_or("").to_lowercase().as_str() {
        "rust" | "rs" => "rs",
        "go" => "go",
        "python" | "py" => "py",
        "javascript" | "js" => "js",
        "typescript" | "ts" => "ts",
        "bash" | "sh" | "shell" => "sh",
        "json" => "json",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        _ => "txt",
    }
}
