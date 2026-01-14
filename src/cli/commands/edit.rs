//! ms edit - Edit a skill (structured round-trip)

use clap::Args;

use std::path::PathBuf;
use std::process::Command;

use crate::app::AppContext;
use crate::cli::commands::resolve_skill_markdown;
use crate::core::spec_lens::{compile_markdown, parse_markdown};
use crate::error::Result;

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

pub fn run(_ctx: &AppContext, _args: &EditArgs) -> Result<()> {
    let ctx = _ctx;
    let args = _args;

    let skill_md = resolve_skill_markdown(ctx, &args.skill)?;
    let skill_dir = skill_md
        .parent()
        .ok_or_else(|| crate::error::MsError::Config("invalid skill path".to_string()))?;
    let edit_path = edit_spec_path(skill_dir);

    let raw = std::fs::read_to_string(&skill_md).map_err(|err| {
        crate::error::MsError::Config(format!("read {}: {err}", skill_md.display()))
    })?;
    let spec = parse_markdown(&raw)?;
    let yaml = serde_yaml::to_string(&spec)
        .map_err(|err| crate::error::MsError::Config(format!("serialize spec: {err}")))?;

    if let Some(parent) = edit_path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            crate::error::MsError::Config(format!("create edit dir: {err}"))
        })?;
    }
    std::fs::write(&edit_path, yaml).map_err(|err| {
        crate::error::MsError::Config(format!("write {}: {err}", edit_path.display()))
    })?;

    let editor = args
        .editor
        .clone()
        .or_else(|| std::env::var("EDITOR").ok())
        .ok_or_else(|| crate::error::MsError::Config("EDITOR not set".to_string()))?;
    run_editor(&editor, &edit_path)?;

    let updated_yaml = std::fs::read_to_string(&edit_path).map_err(|err| {
        crate::error::MsError::Config(format!("read {}: {err}", edit_path.display()))
    })?;
    let updated_spec: crate::core::SkillSpec = serde_yaml::from_str(&updated_yaml)
        .map_err(|err| crate::error::MsError::ValidationFailed(format!("spec parse: {err}")))?;
    let formatted = compile_markdown(&updated_spec);
    std::fs::write(&skill_md, formatted).map_err(|err| {
        crate::error::MsError::Config(format!("write {}: {err}", skill_md.display()))
    })?;
    Ok(())
}

fn edit_spec_path(skill_dir: &std::path::Path) -> PathBuf {
    skill_dir.join(".ms").join("spec_edit.yaml")
}

fn run_editor(editor: &str, path: &PathBuf) -> Result<()> {
    let mut parts = editor.split_whitespace();
    let cmd = parts
        .next()
        .ok_or_else(|| crate::error::MsError::Config("invalid editor".to_string()))?;
    let mut command = Command::new(cmd);
    for part in parts {
        command.arg(part);
    }
    let status = command.arg(path).status().map_err(|err| {
        crate::error::MsError::Config(format!("launch editor: {err}"))
    })?;
    if !status.success() {
        return Err(crate::error::MsError::Config(
            "editor exited with error".to_string(),
        ));
    }
    Ok(())
}
