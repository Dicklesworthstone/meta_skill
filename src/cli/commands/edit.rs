//! ms edit - Edit a skill (structured round-trip)

use clap::Args;

use std::path::PathBuf;
use std::process::Command;

use crate::app::AppContext;
use crate::cli::commands::resolve_skill_markdown;
use crate::core::SkillMetadata;
use crate::core::spec_lens::{compile_markdown, parse_markdown};
use crate::error::Result;
use crate::security::SafetyGate;

#[derive(Args, Debug)]
pub struct EditArgs {
    /// Skill ID or name to edit
    pub skill: String,

    /// Editor to use (default: $EDITOR)
    #[arg(long)]
    pub editor: Option<String>,

    /// Edit metadata only
    #[arg(long)]
    pub meta: bool,
}

pub fn run(ctx: &AppContext, args: &EditArgs) -> Result<()> {
    let skill_md = resolve_skill_markdown(ctx, &args.skill)?;
    let skill_dir = skill_md
        .parent()
        .ok_or_else(|| crate::error::MsError::Config("invalid skill path".to_string()))?;
    let edit_path = edit_spec_path(skill_dir);

    let raw = std::fs::read_to_string(&skill_md).map_err(|err| {
        crate::error::MsError::Config(format!("read {}: {err}", skill_md.display()))
    })?;
    let spec = parse_markdown(&raw)?;
    let yaml = if args.meta {
        serde_yaml::to_string(&spec.metadata)
            .map_err(|err| crate::error::MsError::Config(format!("serialize metadata: {err}")))?
    } else {
        serde_yaml::to_string(&spec)
            .map_err(|err| crate::error::MsError::Config(format!("serialize spec: {err}")))?
    };

    if let Some(parent) = edit_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| crate::error::MsError::Config(format!("create edit dir: {err}")))?;
    }
    std::fs::write(&edit_path, yaml).map_err(|err| {
        crate::error::MsError::Config(format!("write {}: {err}", edit_path.display()))
    })?;

    let editor = args
        .editor
        .clone()
        .or_else(|| std::env::var("EDITOR").ok())
        .ok_or_else(|| crate::error::MsError::Config("EDITOR not set".to_string()))?;
    let gate = SafetyGate::from_context(ctx);
    run_editor(&gate, &editor, &edit_path)?;

    let updated_yaml = std::fs::read_to_string(&edit_path).map_err(|err| {
        crate::error::MsError::Config(format!("read {}: {err}", edit_path.display()))
    })?;
    let updated_spec = if args.meta {
        let updated_meta: SkillMetadata = serde_yaml::from_str(&updated_yaml).map_err(|err| {
            crate::error::MsError::ValidationFailed(format!("metadata parse: {err}"))
        })?;
        let mut updated = spec.clone();
        updated.metadata = updated_meta;
        updated
    } else {
        serde_yaml::from_str(&updated_yaml)
            .map_err(|err| crate::error::MsError::ValidationFailed(format!("spec parse: {err}")))?
    };
    let formatted = compile_markdown(&updated_spec);
    std::fs::write(&skill_md, formatted).map_err(|err| {
        crate::error::MsError::Config(format!("write {}: {err}", skill_md.display()))
    })?;
    record_field_history(skill_dir, &spec, &updated_spec)?;
    Ok(())
}

fn edit_spec_path(skill_dir: &std::path::Path) -> PathBuf {
    skill_dir.join(".ms").join("spec_edit.yaml")
}

fn run_editor(gate: &SafetyGate, editor: &str, path: &PathBuf) -> Result<()> {
    // Parse editor string into command and arguments
    // Handle quoted paths for editors with spaces in path
    let (cmd, args) = parse_editor_string(editor)?;

    // Validate the editor executable exists
    let cmd_path = std::path::Path::new(&cmd);
    if !cmd_path.is_absolute() {
        // For relative paths, use which to find the executable
        if which::which(&cmd).is_err() {
            return Err(crate::error::MsError::Config(format!(
                "editor not found: {cmd}"
            )));
        }
    } else if !cmd_path.exists() {
        return Err(crate::error::MsError::Config(format!(
            "editor not found: {cmd}"
        )));
    }

    // Build the command that will actually be executed for safety check
    // This ensures the safety gate sees exactly what we're going to run
    let mut actual_args = args.clone();
    actual_args.push(path.display().to_string());
    let command_str = std::iter::once(cmd.clone())
        .chain(actual_args.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ");
    gate.enforce(&command_str, None)?;

    // Execute the editor
    let mut command = Command::new(&cmd);
    for arg in &args {
        command.arg(arg);
    }
    let status = command
        .arg(path)
        .status()
        .map_err(|err| crate::error::MsError::Config(format!("launch editor: {err}")))?;
    if !status.success() {
        return Err(crate::error::MsError::Config(
            "editor exited with error".to_string(),
        ));
    }
    Ok(())
}

/// Parse an editor string into command and arguments.
/// Handles simple quoting for paths with spaces.
fn parse_editor_string(editor: &str) -> Result<(String, Vec<String>)> {
    let editor = editor.trim();
    if editor.is_empty() {
        return Err(crate::error::MsError::Config(
            "empty editor string".to_string(),
        ));
    }

    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut quote_char = '"';

    for ch in editor.chars() {
        match ch {
            '"' | '\'' if !in_quote => {
                in_quote = true;
                quote_char = ch;
            }
            c if c == quote_char && in_quote => {
                in_quote = false;
            }
            ' ' | '\t' if !in_quote => {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                }
            }
            _ => {
                current.push(ch);
            }
        }
    }

    if !current.is_empty() {
        parts.push(current);
    }

    if in_quote {
        return Err(crate::error::MsError::Config(
            "unclosed quote in editor string".to_string(),
        ));
    }

    if parts.is_empty() {
        return Err(crate::error::MsError::Config(
            "empty editor string".to_string(),
        ));
    }

    let cmd = parts.remove(0);
    Ok((cmd, parts))
}

#[derive(serde::Serialize)]
struct FieldHistory {
    field_path: String,
    old_value: Option<serde_json::Value>,
    new_value: serde_json::Value,
    changed_at: String,
    changed_by: String,
}

fn record_field_history(
    skill_dir: &std::path::Path,
    before: &crate::core::SkillSpec,
    after: &crate::core::SkillSpec,
) -> Result<()> {
    let mut entries = Vec::new();
    let who = std::env::var("USER").unwrap_or_else(|_| "unknown".to_string());
    let now = chrono::Utc::now().to_rfc3339();

    diff_field(
        &mut entries,
        "metadata.name",
        &before.metadata.name,
        &after.metadata.name,
        &now,
        &who,
    );
    diff_field(
        &mut entries,
        "metadata.description",
        &before.metadata.description,
        &after.metadata.description,
        &now,
        &who,
    );
    diff_field(
        &mut entries,
        "metadata.version",
        &before.metadata.version,
        &after.metadata.version,
        &now,
        &who,
    );
    if before.metadata.tags != after.metadata.tags {
        entries.push(FieldHistory {
            field_path: "metadata.tags".to_string(),
            old_value: Some(serde_json::Value::from(before.metadata.tags.clone())),
            new_value: serde_json::Value::from(after.metadata.tags.clone()),
            changed_at: now.clone(),
            changed_by: who.clone(),
        });
    }

    let max_sections = before.sections.len().max(after.sections.len());
    for idx in 0..max_sections {
        let path_prefix = format!("sections[{idx}]");
        let before_section = before.sections.get(idx);
        let after_section = after.sections.get(idx);
        match (before_section, after_section) {
            (Some(left), Some(right)) => {
                diff_field(
                    &mut entries,
                    &format!("{path_prefix}.title"),
                    &left.title,
                    &right.title,
                    &now,
                    &who,
                );
                let max_blocks = left.blocks.len().max(right.blocks.len());
                for bidx in 0..max_blocks {
                    let block_path = format!("{path_prefix}.blocks[{bidx}]");
                    let left_block = left.blocks.get(bidx);
                    let right_block = right.blocks.get(bidx);
                    match (left_block, right_block) {
                        (Some(lb), Some(rb)) => {
                            if lb.block_type != rb.block_type {
                                entries.push(FieldHistory {
                                    field_path: format!("{block_path}.type"),
                                    old_value: Some(serde_json::Value::String(format!(
                                        "{:?}",
                                        lb.block_type
                                    ))),
                                    new_value: serde_json::Value::String(format!(
                                        "{:?}",
                                        rb.block_type
                                    )),
                                    changed_at: now.clone(),
                                    changed_by: who.clone(),
                                });
                            }
                            if lb.content != rb.content {
                                entries.push(FieldHistory {
                                    field_path: format!("{block_path}.content"),
                                    old_value: Some(serde_json::Value::String(lb.content.clone())),
                                    new_value: serde_json::Value::String(rb.content.clone()),
                                    changed_at: now.clone(),
                                    changed_by: who.clone(),
                                });
                            }
                        }
                        (Some(lb), None) => entries.push(FieldHistory {
                            field_path: block_path,
                            old_value: Some(serde_json::Value::String(lb.content.clone())),
                            new_value: serde_json::Value::Null,
                            changed_at: now.clone(),
                            changed_by: who.clone(),
                        }),
                        (None, Some(rb)) => entries.push(FieldHistory {
                            field_path: block_path,
                            old_value: None,
                            new_value: serde_json::Value::String(rb.content.clone()),
                            changed_at: now.clone(),
                            changed_by: who.clone(),
                        }),
                        (None, None) => {}
                    }
                }
            }
            (Some(left), None) => entries.push(FieldHistory {
                field_path: path_prefix,
                old_value: Some(serde_json::Value::String(left.title.clone())),
                new_value: serde_json::Value::Null,
                changed_at: now.clone(),
                changed_by: who.clone(),
            }),
            (None, Some(right)) => entries.push(FieldHistory {
                field_path: path_prefix,
                old_value: None,
                new_value: serde_json::Value::String(right.title.clone()),
                changed_at: now.clone(),
                changed_by: who.clone(),
            }),
            (None, None) => {}
        }
    }

    if entries.is_empty() {
        return Ok(());
    }

    let history_path = skill_dir.join(".ms").join("field_history.jsonl");
    if let Some(parent) = history_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| crate::error::MsError::Config(format!("create history dir: {err}")))?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&history_path)
        .map_err(|err| crate::error::MsError::Config(format!("open history log: {err}")))?;
    for entry in entries {
        let line = serde_json::to_string(&entry)
            .map_err(|err| crate::error::MsError::Config(format!("serialize history: {err}")))?;
        use std::io::Write;
        writeln!(file, "{line}")
            .map_err(|err| crate::error::MsError::Config(format!("write history: {err}")))?;
    }
    Ok(())
}

fn diff_field(
    entries: &mut Vec<FieldHistory>,
    field_path: &str,
    before: &str,
    after: &str,
    changed_at: &str,
    changed_by: &str,
) {
    if before == after {
        return;
    }
    entries.push(FieldHistory {
        field_path: field_path.to_string(),
        old_value: Some(serde_json::Value::String(before.to_string())),
        new_value: serde_json::Value::String(after.to_string()),
        changed_at: changed_at.to_string(),
        changed_by: changed_by.to_string(),
    });
}
