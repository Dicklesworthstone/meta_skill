use std::path::PathBuf;

use clap::{Args, Subcommand};

use crate::app::AppContext;
use crate::cli::output::{HumanLayout, emit_human, emit_json};
use crate::error::{MsError, Result};
use crate::sync::{RemoteAuth, RemoteConfig, RemoteType, SyncConfig, SyncDirection, validate_remote_name};

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

    /// Remote type (filesystem|git|ru)
    #[arg(long, default_value = "filesystem")]
    pub remote_type: String,

    /// Branch name (git remotes only)
    #[arg(long)]
    pub branch: Option<String>,

    /// Auth method for git remotes (token|ssh)
    #[arg(long)]
    pub auth: Option<String>,

    /// Token env var for auth=token
    #[arg(long)]
    pub token_env: Option<String>,

    /// Username for auth=token (default: x-access-token)
    #[arg(long)]
    pub username: Option<String>,

    /// SSH key path for auth=ssh
    #[arg(long)]
    pub ssh_key: Option<PathBuf>,

    /// SSH public key path for auth=ssh (optional)
    #[arg(long)]
    pub ssh_pubkey: Option<PathBuf>,

    /// SSH passphrase env var for auth=ssh (optional)
    #[arg(long)]
    pub ssh_passphrase_env: Option<String>,

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

    /// Branch name (git remotes only)
    #[arg(long)]
    pub branch: Option<String>,

    /// Clear stored branch override
    #[arg(long)]
    pub clear_branch: bool,

    /// Auth method for git remotes (token|ssh)
    #[arg(long)]
    pub auth: Option<String>,

    /// Token env var for auth=token
    #[arg(long)]
    pub token_env: Option<String>,

    /// Username for auth=token (default: x-access-token)
    #[arg(long)]
    pub username: Option<String>,

    /// SSH key path for auth=ssh
    #[arg(long)]
    pub ssh_key: Option<PathBuf>,

    /// SSH public key path for auth=ssh (optional)
    #[arg(long)]
    pub ssh_pubkey: Option<PathBuf>,

    /// SSH passphrase env var for auth=ssh (optional)
    #[arg(long)]
    pub ssh_passphrase_env: Option<String>,

    /// Clear stored auth configuration
    #[arg(long)]
    pub clear_auth: bool,
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
    validate_remote_name(&args.name)?;
    let mut config = SyncConfig::load()?;
    let remote_type = RemoteType::from_str(&args.remote_type)?;
    validate_remote_flags(
        &remote_type,
        args.branch.is_some(),
        has_auth_flags(
            args.auth.as_ref(),
            args.token_env.as_ref(),
            args.username.as_ref(),
            args.ssh_key.as_ref(),
            args.ssh_pubkey.as_ref(),
            args.ssh_passphrase_env.as_ref(),
        ),
        false,
        false,
    )?;
    let direction = parse_direction(args)?;
    let auth = parse_auth_from(
        args.auth.as_deref(),
        args.token_env.as_ref(),
        args.username.as_ref(),
        args.ssh_key.as_ref(),
        args.ssh_pubkey.as_ref(),
        args.ssh_passphrase_env.as_ref(),
    )?;
    let remote = RemoteConfig {
        name: args.name.clone(),
        remote_type,
        url: args.url.clone(),
        branch: args.branch.clone(),
        auth,
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
            .kv("Type", &format!("{remote_type:?}"))
            .kv("Direction", &format!("{direction:?}"))
            .kv("Enabled", &(!args.disabled).to_string());
        if let Some(branch) = &args.branch {
            layout.kv("Branch", branch);
        }
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
                .kv(
                    "Branch",
                    &remote
                        .branch
                        .clone()
                        .unwrap_or_else(|| "(default)".to_string()),
                )
                .kv(
                    "Auth",
                    &remote
                        .auth
                        .as_ref().map_or_else(|| "none".to_string(), |a| format!("{a:?}")),
                )
                .blank();
        }
        emit_human(layout);
        Ok(())
    }
}

fn remove(ctx: &AppContext, args: &RemoteRemoveArgs) -> Result<()> {
    validate_remote_name(&args.name)?;
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
    validate_remote_name(&args.name)?;
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
    validate_remote_name(&args.name)?;
    let mut config = SyncConfig::load()?;
    let Some(remote) = config.remotes.iter_mut().find(|r| r.name == args.name) else {
        return Err(MsError::Config(format!("unknown remote: {}", args.name)));
    };

    let has_auth = has_auth_flags(
        args.auth.as_ref(),
        args.token_env.as_ref(),
        args.username.as_ref(),
        args.ssh_key.as_ref(),
        args.ssh_pubkey.as_ref(),
        args.ssh_passphrase_env.as_ref(),
    );
    validate_remote_flags(
        &remote.remote_type,
        args.branch.is_some(),
        has_auth,
        args.clear_branch,
        args.clear_auth,
    )?;

    remote.url = args.url.clone();
    if args.clear_branch {
        remote.branch = None;
    } else if let Some(branch) = &args.branch {
        remote.branch = Some(branch.clone());
    }

    if args.clear_auth {
        remote.auth = None;
    } else if has_auth || args.auth.is_some() {
        remote.auth = parse_auth_from(
            args.auth.as_deref(),
            args.token_env.as_ref(),
            args.username.as_ref(),
            args.ssh_key.as_ref(),
            args.ssh_pubkey.as_ref(),
            args.ssh_passphrase_env.as_ref(),
        )?;
    }
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
        if args.clear_branch {
            layout.kv("Branch", "(cleared)");
        } else if let Some(branch) = &args.branch {
            layout.kv("Branch", branch);
        }
        if args.clear_auth {
            layout.kv("Auth", "(cleared)");
        } else if args.auth.is_some() {
            layout.kv("Auth", args.auth.as_deref().unwrap_or("none"));
        }
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

fn parse_auth_from(
    raw: Option<&str>,
    token_env: Option<&String>,
    username: Option<&String>,
    ssh_key: Option<&PathBuf>,
    ssh_pubkey: Option<&PathBuf>,
    ssh_passphrase_env: Option<&String>,
) -> Result<Option<RemoteAuth>> {
    let Some(raw) = raw else {
        if token_env.is_some()
            || username.is_some()
            || ssh_key.is_some()
            || ssh_pubkey.is_some()
            || ssh_passphrase_env.is_some()
        {
            return Err(MsError::Config(
                "auth flags provided without --auth".to_string(),
            ));
        }
        return Ok(None);
    };
    match raw {
        "token" => {
            let Some(token_env) = token_env else {
                return Err(MsError::Config(
                    "auth=token requires --token-env".to_string(),
                ));
            };
            Ok(Some(RemoteAuth::Token {
                token_env: token_env.clone(),
                username: username.cloned(),
            }))
        }
        "ssh" => {
            let Some(key_path) = ssh_key else {
                return Err(MsError::Config(
                    "auth=ssh requires --ssh-key".to_string(),
                ));
            };
            Ok(Some(RemoteAuth::SshKey {
                key_path: key_path.clone(),
                public_key: ssh_pubkey.cloned(),
                passphrase_env: ssh_passphrase_env.cloned(),
            }))
        }
        _ => Err(MsError::Config(format!(
            "unknown auth method: {raw} (use token|ssh)"
        ))),
    }
}

fn validate_remote_flags(
    remote_type: &RemoteType,
    has_branch: bool,
    has_auth: bool,
    clear_branch: bool,
    clear_auth: bool,
) -> Result<()> {
    if !matches!(remote_type, RemoteType::Git) {
        if has_branch || clear_branch {
            return Err(MsError::Config(
                "--branch is only valid for git remotes".to_string(),
            ));
        }
        if has_auth || clear_auth {
            return Err(MsError::Config(
                "auth flags are only valid for git remotes".to_string(),
            ));
        }
    }
    Ok(())
}

const fn has_auth_flags(
    auth: Option<&String>,
    token_env: Option<&String>,
    username: Option<&String>,
    ssh_key: Option<&PathBuf>,
    ssh_pubkey: Option<&PathBuf>,
    ssh_passphrase_env: Option<&String>,
) -> bool {
    auth.is_some()
        || token_env.is_some()
        || username.is_some()
        || ssh_key.is_some()
        || ssh_pubkey.is_some()
        || ssh_passphrase_env.is_some()
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

    #[test]
    fn parse_remote_add_git_auth() {
        let args = crate::cli::Cli::parse_from([
            "ms",
            "remote",
            "add",
            "origin",
            "https://example.com/skills.git",
            "--remote-type",
            "git",
            "--branch",
            "main",
            "--auth",
            "token",
            "--token-env",
            "GIT_TOKEN",
        ]);
        if let crate::cli::Commands::Remote(remote) = args.command {
            if let RemoteCommand::Add(add) = remote.command {
                assert_eq!(add.remote_type, "git");
                assert_eq!(add.branch.as_deref(), Some("main"));
                assert_eq!(add.auth.as_deref(), Some("token"));
                assert_eq!(add.token_env.as_deref(), Some("GIT_TOKEN"));
            } else {
                panic!("expected add subcommand");
            }
        } else {
            panic!("expected remote command");
        }
    }
}
