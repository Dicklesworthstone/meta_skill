use clap::{Args, Subcommand};

use crate::app::AppContext;
use crate::cli::output::{HumanLayout, emit_human, emit_json};
use crate::error::{MsError, Result};
use crate::sync::{RemoteConfig, RemoteType, SyncConfig, SyncDirection};

#[derive(Args, Debug)]
pub struct RemoteArgs {
    #[command(subcommand)]
    pub command: RemoteCommand,
}

#[derive(Subcommand, Debug)]
pub enum RemoteCommand {
    /// Add a sync remote
    Add(RemoteAddArgs),
    /// List configured remotes
    List(RemoteListArgs),
    /// Remove a remote
    Remove(RemoteRemoveArgs),
    /// Enable a remote
    Enable(RemoteToggleArgs),
    /// Disable a remote
    Disable(RemoteToggleArgs),
    /// Update a remote URL
    SetUrl(RemoteSetUrlArgs),
}

#[derive(Args, Debug)]
pub struct RemoteAddArgs {
    pub name: String,
    pub url: String,

    /// Remote type (filesystem|git)
    #[arg(long, default_value = "filesystem")]
    pub remote_type: String,

    /// Sync direction (pull-only|push-only|bidirectional)
    #[arg(long)]
    pub direction: Option<String>,

    /// Enable auto-sync for this remote
    #[arg(long)]
    pub auto_sync: bool,

    /// Disable this remote
    #[arg(long)]
    pub disabled: bool,

    /// Only pull from remote
    #[arg(long, conflicts_with_all = ["push_only", "bidirectional"])]
    pub pull_only: bool,

    /// Only push to remote
    #[arg(long, conflicts_with_all = ["pull_only", "bidirectional"])]
    pub push_only: bool,

    /// Bidirectional sync
    #[arg(long, conflicts_with_all = ["pull_only", "push_only"])]
    pub bidirectional: bool,
}

#[derive(Args, Debug, Default)]
pub struct RemoteListArgs {}

#[derive(Args, Debug)]
pub struct RemoteRemoveArgs {
    pub name: String,
}

#[derive(Args, Debug)]
pub struct RemoteToggleArgs {
    pub name: String,
}

#[derive(Args, Debug)]
pub struct RemoteSetUrlArgs {
    pub name: String,
    pub url: String,
}

pub fn run(ctx: &AppContext, args: &RemoteArgs) -> Result<()> {
    match &args.command {
        RemoteCommand::Add(args) => add(ctx, args),
        RemoteCommand::List(args) => list(ctx, args),
        RemoteCommand::Remove(args) => remove(ctx, args),
        RemoteCommand::Enable(args) => toggle(ctx, args, true),
        RemoteCommand::Disable(args) => toggle(ctx, args, false),
        RemoteCommand::SetUrl(args) => set_url(ctx, args),
    }
}

fn add(ctx: &AppContext, args: &RemoteAddArgs) -> Result<()> {
    let mut config = SyncConfig::load()?;
    let remote_type = RemoteType::from_str(&args.remote_type)?;
    let direction = parse_direction(args)?;
    let remote = RemoteConfig {
        name: args.name.clone(),
        remote_type,
        url: args.url.clone(),
        enabled: !args.disabled,
        direction,
        auto_sync: args.auto_sync,
        exclude_patterns: Vec::new(),
        include_patterns: Vec::new(),
    };
    config.upsert_remote(remote.clone());
    config.save()?;

    if ctx.robot_mode {
        let payload = serde_json::json!({
            "status": "ok",
            "remote": remote,
        });
        emit_json(&payload)
    } else {
        let mut layout = HumanLayout::new();
        layout
            .title("Remote Added")
            .kv("Name", &args.name)
            .kv("URL", &args.url)
            .kv("Type", &format!("{:?}", remote_type))
            .kv("Direction", &format!("{:?}", direction))
            .kv("Enabled", &(!args.disabled).to_string());
        emit_human(layout);
        Ok(())
    }
}

fn list(ctx: &AppContext, _args: &RemoteListArgs) -> Result<()> {
    let config = SyncConfig::load()?;
    if ctx.robot_mode {
        let payload = serde_json::json!({
            "status": "ok",
            "remotes": config.remotes,
        });
        emit_json(&payload)
    } else {
        let mut layout = HumanLayout::new();
        layout.title("Sync Remotes");
        for remote in config.remotes {
            layout
                .section(&remote.name)
                .kv("URL", &remote.url)
                .kv("Type", &format!("{:?}", remote.remote_type))
                .kv("Direction", &format!("{:?}", remote.direction))
                .kv("Enabled", &remote.enabled.to_string())
                .kv("Auto sync", &remote.auto_sync.to_string())
                .blank();
        }
        emit_human(layout);
        Ok(())
    }
}

fn remove(ctx: &AppContext, args: &RemoteRemoveArgs) -> Result<()> {
    let mut config = SyncConfig::load()?;
    if !config.remove_remote(&args.name) {
        return Err(MsError::Config(format!("unknown remote: {}", args.name)));
    }
    config.save()?;

    if ctx.robot_mode {
        emit_json(&serde_json::json!({
            "status": "ok",
            "removed": args.name,
        }))
    } else {
        let mut layout = HumanLayout::new();
        layout.title("Remote Removed").kv("Name", &args.name);
        emit_human(layout);
        Ok(())
    }
}

fn toggle(ctx: &AppContext, args: &RemoteToggleArgs, enabled: bool) -> Result<()> {
    let mut config = SyncConfig::load()?;
    let Some(remote) = config.remotes.iter_mut().find(|r| r.name == args.name) else {
        return Err(MsError::Config(format!("unknown remote: {}", args.name)));
    };
    remote.enabled = enabled;
    config.save()?;

    if ctx.robot_mode {
        emit_json(&serde_json::json!({
            "status": "ok",
            "name": args.name,
            "enabled": enabled,
        }))
    } else {
        let mut layout = HumanLayout::new();
        layout
            .title("Remote Updated")
            .kv("Name", &args.name)
            .kv("Enabled", &enabled.to_string());
        emit_human(layout);
        Ok(())
    }
}

fn set_url(ctx: &AppContext, args: &RemoteSetUrlArgs) -> Result<()> {
    let mut config = SyncConfig::load()?;
    let Some(remote) = config.remotes.iter_mut().find(|r| r.name == args.name) else {
        return Err(MsError::Config(format!("unknown remote: {}", args.name)));
    };
    remote.url = args.url.clone();
    config.save()?;

    if ctx.robot_mode {
        emit_json(&serde_json::json!({
            "status": "ok",
            "name": args.name,
            "url": args.url,
        }))
    } else {
        let mut layout = HumanLayout::new();
        layout
            .title("Remote Updated")
            .kv("Name", &args.name)
            .kv("URL", &args.url);
        emit_human(layout);
        Ok(())
    }
}

fn parse_direction(args: &RemoteAddArgs) -> Result<SyncDirection> {
    if args.pull_only {
        return Ok(SyncDirection::PullOnly);
    }
    if args.push_only {
        return Ok(SyncDirection::PushOnly);
    }
    if args.bidirectional {
        return Ok(SyncDirection::Bidirectional);
    }
    if let Some(ref dir) = args.direction {
        return SyncDirection::from_str(dir);
    }
    Ok(SyncDirection::Bidirectional)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parse_remote_add() {
        let args = crate::cli::Cli::parse_from([
            "ms",
            "remote",
            "add",
            "origin",
            "/tmp/skills",
            "--pull-only",
        ]);
        if let crate::cli::Commands::Remote(remote) = args.command {
            if let RemoteCommand::Add(add) = remote.command {
                assert_eq!(add.name, "origin");
                assert!(add.pull_only);
            } else {
                panic!("expected add subcommand");
            }
        } else {
            panic!("expected remote command");
        }
    }
}
