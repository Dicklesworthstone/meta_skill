use std::collections::HashMap;

use clap::Args;
use colored::Colorize;

use crate::app::AppContext;
use crate::cli::output::{HumanLayout, emit_human, emit_json};
use crate::error::Result;
use crate::sync::{MachineIdentity, SyncConfig, SyncEngine, SyncOptions, SyncState};

use super::index::{IndexArgs, run as run_index};

#[derive(Args, Debug, Default)]
pub struct SyncArgs {
    /// Specific remote name (default: all enabled)
    #[arg(value_name = "REMOTE")]
    pub remote: Option<String>,

    /// Show sync status without syncing
    #[arg(long)]
    pub status: bool,

    /// Only push local changes
    #[arg(long, conflicts_with = "pull_only")]
    pub push_only: bool,

    /// Only pull remote changes
    #[arg(long, conflicts_with = "push_only")]
    pub pull_only: bool,

    /// Preview sync operations without writing
    #[arg(long)]
    pub dry_run: bool,

    /// Force conflict resolution using configured strategies
    #[arg(long)]
    pub force: bool,
}

pub fn run(ctx: &AppContext, args: &SyncArgs) -> Result<()> {
    if args.status {
        return status(ctx, args);
    }

    let config = SyncConfig::load()?;
    let machine = MachineIdentity::load_or_generate_with_name(
        config.machine.name.clone(),
        config.machine.description.clone(),
    )?;
    let state = SyncState::load(&ctx.ms_root)?;
    let mut engine = SyncEngine::new(
        config,
        machine,
        state,
        ctx.git.clone(),
        ctx.db.clone(),
        ctx.ms_root.clone(),
        ctx.config.ru.clone(),
    );

    let options = SyncOptions {
        push_only: args.push_only,
        pull_only: args.pull_only,
        dry_run: args.dry_run,
        force: args.force,
    };

    let reports = if let Some(remote) = args.remote.as_deref() {
        vec![engine.sync_remote(remote, &options)?]
    } else {
        engine.sync_all(&options)?
    };

    // Check if we should auto-reindex after sync
    let should_reindex = !args.dry_run
        && ctx.config.ru.auto_index
        && reports.iter().any(|r| !r.cloned.is_empty() || !r.pulled.is_empty());

    if ctx.robot_mode {
        let mut payload = serde_json::json!({
            "status": "ok",
            "reports": reports,
        });

        // Auto-reindex if configured and there were changes
        if should_reindex {
            let reindex_result = auto_reindex(ctx);
            if let Some(obj) = payload.as_object_mut() {
                obj.insert(
                    "auto_reindex".to_string(),
                    serde_json::json!({
                        "triggered": true,
                        "success": reindex_result.is_ok(),
                        "error": reindex_result.err().map(|e| e.to_string()),
                    }),
                );
            }
        }

        emit_json(&payload)
    } else {
        let mut layout = HumanLayout::new();
        layout.title("Sync Report");
        for report in &reports {
            layout
                .section(&report.remote)
                .kv("Cloned", &report.cloned.len().to_string())
                .kv("Pulled", &report.pulled.len().to_string())
                .kv("Pushed", &report.pushed.len().to_string())
                .kv("Resolved", &report.resolved.len().to_string())
                .kv("Conflicts", &report.conflicts.len().to_string())
                .kv("Forked", &report.forked.len().to_string())
                .kv("Skipped", &report.skipped.len().to_string())
                .kv("Duration (ms)", &report.duration_ms.to_string())
                .blank();
        }
        emit_human(layout);

        // Auto-reindex if configured and there were changes
        if should_reindex {
            println!();
            println!("{}", "Auto-reindexing skills from synced repos...".cyan());
            match auto_reindex(ctx) {
                Ok(()) => {
                    println!("{} Skills reindexed successfully", "✓".green().bold());
                }
                Err(e) => {
                    println!("{} Failed to reindex skills: {}", "✗".red(), e);
                }
            }
        }

        Ok(())
    }
}

/// Trigger auto-reindex of skills from ru-managed repositories
fn auto_reindex(ctx: &AppContext) -> Result<()> {
    let index_args = IndexArgs {
        paths: Vec::new(),
        watch: false,
        force: false,
        all: false,
        from_ru: true,
    };
    run_index(ctx, &index_args)
}

fn status(ctx: &AppContext, _args: &SyncArgs) -> Result<()> {
    let config = SyncConfig::load()?;
    let machine = MachineIdentity::load_or_generate_with_name(
        config.machine.name.clone(),
        config.machine.description.clone(),
    )?;
    let state = SyncState::load(&ctx.ms_root)?;

    let mut status_counts = HashMap::new();
    for entry in state.skill_states.values() {
        *status_counts
            .entry(format!("{:?}", entry.status))
            .or_insert(0usize) += 1;
    }

    if ctx.robot_mode {
        let payload = serde_json::json!({
            "status": "ok",
            "machine": {
                "id": machine.machine_id,
                "name": machine.machine_name,
                "last_syncs": machine.sync_timestamps,
            },
            "remotes": config.remotes,
            "last_full_sync": state.last_full_sync,
            "status_counts": status_counts,
        });
        emit_json(&payload)
    } else {
        let mut layout = HumanLayout::new();
        layout
            .title("Sync Status")
            .section("Machine")
            .kv("Name", &machine.machine_name)
            .kv("ID", &machine.machine_id);

        if !machine.sync_timestamps.is_empty() {
            layout.blank().section("Last Syncs");
            let mut syncs: Vec<_> = machine.sync_timestamps.iter().collect();
            syncs.sort_by(|a, b| a.0.cmp(b.0));
            for (remote, ts) in syncs {
                layout.kv(remote, &ts.to_rfc3339());
            }
        }

        layout.blank().section("Remotes");
        for remote in &config.remotes {
            layout.kv(
                &remote.name,
                &format!("{} ({:?})", remote.url, remote.remote_type),
            );
        }

        if !state.last_full_sync.is_empty() {
            layout.blank().section("Last Full Sync");
            let mut last_syncs: Vec<_> = state.last_full_sync.iter().collect();
            last_syncs.sort_by(|a, b| a.0.cmp(b.0));
            for (remote, ts) in last_syncs {
                layout.kv(remote, &ts.to_rfc3339());
            }
        }

        if !status_counts.is_empty() {
            layout.blank().section("Status Counts");
            let mut counts: Vec<_> = status_counts.iter().collect();
            counts.sort_by(|a, b| a.0.cmp(b.0));
            for (status, count) in counts {
                layout.kv(status, &count.to_string());
            }
        }

        emit_human(layout);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    #[test]
    fn parse_sync_args_status() {
        let args = crate::cli::Cli::parse_from(["ms", "sync", "--status"]);
        if let crate::cli::Commands::Sync(sync) = args.command {
            assert!(sync.status);
        } else {
            panic!("expected sync command");
        }
    }

    #[test]
    fn parse_sync_args_remote() {
        let args = crate::cli::Cli::parse_from(["ms", "sync", "origin", "--dry-run"]);
        if let crate::cli::Commands::Sync(sync) = args.command {
            assert_eq!(sync.remote, Some("origin".to_string()));
            assert!(sync.dry_run);
        } else {
            panic!("expected sync command");
        }
    }
}
