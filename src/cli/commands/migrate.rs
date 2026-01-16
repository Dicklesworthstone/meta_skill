//! ms migrate - Upgrade `SkillSpec` format versions.

use clap::Args;

use crate::app::AppContext;
use crate::cli::output::{HumanLayout, emit_json};
use crate::core::{SkillLayer, migrate_spec};
use crate::error::{MsError, Result};
use crate::storage::tx::TxManager;

#[derive(Args, Debug)]
pub struct MigrateArgs {
    /// Skills to migrate (defaults to all in archive)
    pub skills: Vec<String>,

    /// Check for required migrations without writing
    #[arg(long)]
    pub check: bool,
}

#[derive(serde::Serialize)]
struct MigrationItem {
    skill_id: String,
    from_version: String,
    to_version: String,
    changed: bool,
}

pub fn run(ctx: &AppContext, args: &MigrateArgs) -> Result<()> {
    let skill_ids = resolve_targets(ctx, &args.skills)?;
    let tx_mgr = TxManager::new(ctx.db.clone(), ctx.git.clone(), ctx.ms_root.clone())?;

    let mut items = Vec::new();
    let mut changed_count = 0usize;

    for skill_id in skill_ids {
        let spec = ctx.git.read_skill(&skill_id).map_err(|err| {
            MsError::Config(format!("read skill {skill_id} from archive: {err}"))
        })?;
        let from_version = spec.format_version.clone();
        let (migrated, changed) = migrate_spec(spec)?;
        let to_version = migrated.format_version.clone();

        if changed {
            changed_count += 1;
            if !args.check {
                let layer = resolve_layer(ctx, &skill_id);
                tx_mgr.write_skill_with_layer(&migrated, layer)?;
            }
        }

        items.push(MigrationItem {
            skill_id,
            from_version,
            to_version,
            changed,
        });
    }

    if ctx.robot_mode {
        let payload = serde_json::json!({
            "status": "ok",
            "count": items.len(),
            "changed": changed_count,
            "items": items,
        });
        return emit_json(&payload);
    }

    let mut layout = HumanLayout::new();
    layout.title("Skill Migrations");
    layout.kv("Total", &items.len().to_string());
    layout.kv("Changed", &changed_count.to_string());
    layout.blank();

    for item in items {
        let status = if item.changed { "migrated" } else { "current" };
        layout
            .section(&item.skill_id)
            .kv("From", &item.from_version)
            .kv("To", &item.to_version)
            .kv("Status", status)
            .blank();
    }

    crate::cli::output::emit_human(layout);
    Ok(())
}

fn resolve_targets(ctx: &AppContext, inputs: &[String]) -> Result<Vec<String>> {
    if inputs.is_empty() {
        return ctx.git.list_skill_ids();
    }

    let mut out = Vec::new();
    for input in inputs {
        if let Some(skill) = ctx.db.get_skill(input)? {
            out.push(skill.id);
            continue;
        }
        if let Ok(Some(alias)) = ctx.db.resolve_alias(input) {
            out.push(alias.canonical_id);
            continue;
        }
        out.push(input.clone());
    }
    Ok(out)
}

fn resolve_layer(ctx: &AppContext, skill_id: &str) -> SkillLayer {
    if let Ok(Some(skill)) = ctx.db.get_skill(skill_id) {
        return match skill.source_layer.to_lowercase().as_str() {
            "base" | "system" => SkillLayer::Base,
            "org" | "global" => SkillLayer::Org,
            "user" | "local" => SkillLayer::User,
            _ => SkillLayer::Project,
        };
    }
    SkillLayer::Project
}
