use clap::{Args, Subcommand};

use crate::app::AppContext;
use crate::cli::output::{HumanLayout, emit_human, emit_json};
use crate::error::{MsError, Result};
use crate::sync::{
    ConflictStrategy, MachineIdentity, SkillSyncStatus, SyncConfig, SyncEngine, SyncOptions,
    SyncState,
};

#[derive(Args, Debug)]
pub struct ConflictsArgs {
    #[command(subcommand)]
    pub command: ConflictsCommand,
}

#[derive(Subcommand, Debug)]
pub enum ConflictsCommand {
    /// List unresolved conflicts
    List,
    /// Resolve a conflict by choosing a strategy
    Resolve(ConflictsResolveArgs),
}

#[derive(Args, Debug)]
pub struct ConflictsResolveArgs {
    pub skill: String,

    /// Strategy: prefer-local | prefer-remote | prefer-newest | keep-both
    #[arg(long)]
    pub strategy: String,

    /// Apply immediately by syncing with --force
    #[arg(long)]
    pub apply: bool,

    /// Sync a specific remote when applying
    #[arg(long)]
    pub remote: Option<String>,
}

pub fn run(ctx: &AppContext, args: &ConflictsArgs) -> Result<()> {
    match &args.command {
        ConflictsCommand::List => list(ctx),
        ConflictsCommand::Resolve(args) => resolve(ctx, args),
    }
}

fn list(ctx: &AppContext) -> Result<()> {
    let state = SyncState::load(&ctx.ms_root)?;
    let mut conflicts: Vec<_> = state
        .skill_states
        .values()
        .filter(|entry| {
            matches!(
                entry.status,
                SkillSyncStatus::Conflict | SkillSyncStatus::Diverged
            )
        })
        .map(|entry| entry.skill_id.clone())
        .collect();
    conflicts.sort();

    if ctx.robot_mode {
        emit_json(&serde_json::json!({
            "status": "ok",
            "conflicts": conflicts,
        }))
    } else {
        let mut layout = HumanLayout::new();
        layout.title("Sync Conflicts");
        if conflicts.is_empty() {
            layout.bullet("No conflicts recorded.");
        } else {
            for skill in conflicts {
                layout.bullet(&skill);
            }
        }
        emit_human(layout);
        Ok(())
    }
}

fn resolve(ctx: &AppContext, args: &ConflictsResolveArgs) -> Result<()> {
    let strategy = parse_strategy(&args.strategy)?;
    let mut config = SyncConfig::load()?;
    config
        .conflict_strategies
        .insert(args.skill.clone(), strategy);
    config.save()?;

    if args.apply {
        let machine = MachineIdentity::load_or_generate_with_name(
            config.machine.name.clone(),
            config.machine.description.clone(),
        )?;
        let state = SyncState::load(&ctx.ms_root)?;
        let mut engine = SyncEngine::new(
            config.clone(),
            machine,
            state,
            ctx.git.clone(),
            ctx.db.clone(),
            ctx.ms_root.clone(),
            ctx.config.ru.clone(),
        );
        let options = SyncOptions {
            force: true,
            ..Default::default()
        };
        if let Some(remote) = args.remote.as_deref() {
            engine.sync_remote(remote, &options)?;
        } else {
            engine.sync_all(&options)?;
        }
    }

    if ctx.robot_mode {
        emit_json(&serde_json::json!({
            "status": "ok",
            "skill": args.skill,
            "strategy": args.strategy,
            "applied": args.apply,
        }))
    } else {
        let mut layout = HumanLayout::new();
        layout
            .title("Conflict Strategy Updated")
            .kv("Skill", &args.skill)
            .kv("Strategy", &args.strategy)
            .kv("Applied", &args.apply.to_string());
        emit_human(layout);
        Ok(())
    }
}

fn parse_strategy(raw: &str) -> Result<ConflictStrategy> {
    match raw {
        "prefer-local" | "local" => Ok(ConflictStrategy::PreferLocal),
        "prefer-remote" | "remote" => Ok(ConflictStrategy::PreferRemote),
        "prefer-newest" | "newest" => Ok(ConflictStrategy::PreferNewest),
        "keep-both" | "both" => Ok(ConflictStrategy::KeepBoth),
        _ => Err(MsError::Config(format!("unknown conflict strategy: {raw}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parse_conflicts_list() {
        let args = crate::cli::Cli::parse_from(["ms", "conflicts", "list"]);
        if let crate::cli::Commands::Conflicts(conflicts) = args.command {
            if !matches!(conflicts.command, ConflictsCommand::List) {
                panic!("expected list command");
            }
        } else {
            panic!("expected conflicts command");
        }
    }
}
