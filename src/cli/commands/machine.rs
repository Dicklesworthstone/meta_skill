use clap::{Args, Subcommand};

use crate::app::AppContext;
use crate::cli::output::{HumanLayout, emit_human, emit_json};
use crate::error::Result;
use crate::sync::{MachineIdentity, SyncConfig};

#[derive(Args, Debug)]
pub struct MachineArgs {
    #[command(subcommand)]
    pub command: MachineCommand,
}

#[derive(Subcommand, Debug)]
pub enum MachineCommand {
    /// Show machine identity
    Info,
    /// Rename this machine
    Rename(MachineRenameArgs),
}

#[derive(Args, Debug)]
pub struct MachineRenameArgs {
    pub name: String,
}

pub fn run(ctx: &AppContext, args: &MachineArgs) -> Result<()> {
    match &args.command {
        MachineCommand::Info => info(ctx),
        MachineCommand::Rename(args) => rename(ctx, args),
    }
}

fn info(ctx: &AppContext) -> Result<()> {
    let config = SyncConfig::load()?;
    let machine = MachineIdentity::load_or_generate_with_name(
        config.machine.name.clone(),
        config.machine.description,
    )?;

    if ctx.robot_mode {
        emit_json(&serde_json::json!({
            "status": "ok",
            "machine": machine,
        }))
    } else {
        let mut layout = HumanLayout::new();
        layout
            .title("Machine Identity")
            .kv("Name", &machine.machine_name)
            .kv("ID", &machine.machine_id)
            .kv("OS", &machine.metadata.os)
            .kv("Hostname", &machine.metadata.hostname)
            .kv("Registered", &machine.metadata.registered_at.to_rfc3339());

        if let Some(desc) = &machine.metadata.description {
            layout.kv("Description", desc);
        }

        if !machine.sync_timestamps.is_empty() {
            layout.blank().section("Last Syncs");
            let mut syncs: Vec<_> = machine.sync_timestamps.iter().collect();
            syncs.sort_by(|a, b| a.0.cmp(b.0));
            for (remote, ts) in syncs {
                layout.kv(remote, &ts.to_rfc3339());
            }
        }

        emit_human(layout);
        Ok(())
    }
}

fn rename(ctx: &AppContext, args: &MachineRenameArgs) -> Result<()> {
    let config = SyncConfig::load()?;
    let mut machine = MachineIdentity::load_or_generate_with_name(
        config.machine.name.clone(),
        config.machine.description,
    )?;
    machine.rename(args.name.clone());
    machine.save()?;

    if ctx.robot_mode {
        emit_json(&serde_json::json!({
            "status": "ok",
            "name": machine.machine_name,
            "id": machine.machine_id,
        }))
    } else {
        let mut layout = HumanLayout::new();
        layout
            .title("Machine Renamed")
            .kv("Name", &machine.machine_name)
            .kv("ID", &machine.machine_id);
        emit_human(layout);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parse_machine_info() {
        let args = crate::cli::Cli::parse_from(["ms", "machine", "info"]);
        if let crate::cli::Commands::Machine(machine) = args.command {
            if !matches!(machine.command, MachineCommand::Info) {
                panic!("expected info command");
            }
        } else {
            panic!("expected machine command");
        }
    }
}
