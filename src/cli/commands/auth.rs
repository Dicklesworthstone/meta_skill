//! JFP Cloud Authentication Commands
//!
//! Manage authentication with JeffreysPrompts Premium Cloud:
//!
//! - `ms auth login`  - Authenticate via device code flow
//! - `ms auth status` - Show current authentication status
//! - `ms auth logout` - Clear local credentials
//! - `ms auth revoke` - Revoke token on server and clear locally

use clap::{Args, Subcommand};
use std::io::{self, Write};
use std::time::{Duration, Instant};

use crate::app::AppContext;
use crate::auth::{JfpAuthClient, JfpAuthConfig, device_code, token_storage};
use crate::cli::output::{HumanLayout, OutputFormat, emit_human, emit_json};
use crate::error::{MsError, Result};

#[derive(Args, Debug)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub command: AuthCommand,
}

#[derive(Subcommand, Debug)]
pub enum AuthCommand {
    /// Authenticate with JFP Cloud (device code flow)
    Login(LoginArgs),
    /// Show authentication status
    Status,
    /// Clear local credentials (does not revoke on server)
    Logout,
    /// Revoke token on server and clear locally
    Revoke,
}

#[derive(Args, Debug)]
pub struct LoginArgs {
    /// Custom API base URL (for staging/testing)
    #[arg(long, env = "JFP_API_URL")]
    pub api_url: Option<String>,

    /// Don't auto-open browser
    #[arg(long)]
    pub no_browser: bool,
}

pub fn run(ctx: &AppContext, args: &AuthArgs) -> Result<()> {
    match &args.command {
        AuthCommand::Login(login_args) => login(ctx, login_args),
        AuthCommand::Status => status(ctx),
        AuthCommand::Logout => logout(ctx),
        AuthCommand::Revoke => revoke(ctx),
    }
}

fn login(ctx: &AppContext, args: &LoginArgs) -> Result<()> {
    // Build config
    let config = if let Some(url) = &args.api_url {
        JfpAuthConfig::with_base_url(url)
    } else {
        JfpAuthConfig::default()
    };

    let client = JfpAuthClient::with_config(config.clone())?;

    // Check if already logged in
    if let Ok(status) = client.status() {
        if status.authenticated {
            if ctx.output_format != OutputFormat::Human {
                return emit_json(&serde_json::json!({
                    "status": "already_authenticated",
                    "email": status.email,
                    "tier": status.tier,
                }));
            }
            println!(
                "Already authenticated as {} ({}). Use 'ms auth logout' first to re-authenticate.",
                status.email.unwrap_or_default(),
                status.tier.unwrap_or_default()
            );
            return Ok(());
        }
    }

    // Request device code
    let device_code = client.start_device_code_flow()?;

    // Display instructions
    if ctx.output_format != OutputFormat::Human {
        // For machine output, emit the device code info and exit
        // The caller is responsible for polling
        return emit_json(&serde_json::json!({
            "status": "pending",
            "user_code": device_code.user_code,
            "verification_url": device_code.verification_url,
            "verification_url_complete": device_code.verification_url_complete,
            "device_code": device_code.device_code,
            "expires_in": device_code.expires_in,
            "interval": device_code.interval,
        }));
    }

    // Human-friendly output
    println!();
    println!("  ╭─────────────────────────────────────────────────────╮");
    println!("  │                                                     │");
    println!("  │   To authenticate, visit:                           │");
    println!("  │                                                     │");
    println!(
        "  │   {}{}│",
        device_code.verification_url,
        " ".repeat(37_usize.saturating_sub(device_code.verification_url.len()))
    );
    println!("  │                                                     │");
    println!("  │   And enter the code:                               │");
    println!("  │                                                     │");
    println!("  │        ┌─────────────────────┐                      │");
    println!(
        "  │        │     {}     │                      │",
        device_code.user_code
    );
    println!("  │        └─────────────────────┘                      │");
    println!("  │                                                     │");
    println!("  ╰─────────────────────────────────────────────────────╯");
    println!();

    // Try to open browser
    if !args.no_browser && device_code::is_tty() {
        if device_code::open_browser(&device_code.verification_url_complete) {
            println!("  Browser opened automatically.");
        } else {
            println!("  (Could not open browser automatically)");
        }
    }

    println!();
    println!("  Waiting for authentication...");
    println!();

    // Poll for verification
    let expires_at = Instant::now() + Duration::from_secs(device_code.expires_in);
    let mut poll_interval = Duration::from_secs(device_code.interval);

    loop {
        if Instant::now() >= expires_at {
            return Err(MsError::AuthError(
                "Device code expired. Please run 'ms auth login' again.".to_string(),
            ));
        }

        std::thread::sleep(poll_interval);

        // Show a spinner dot
        print!(".");
        io::stdout().flush().ok();

        match client.poll_device_code(&device_code.device_code)? {
            device_code::PollResult::Success(creds) => {
                // Save credentials
                client.complete_login(creds.clone())?;

                println!();
                println!();
                println!("  ✓ Successfully authenticated as {}", creds.email);
                println!("    Subscription tier: {}", creds.tier);
                println!();
                println!(
                    "    Credentials stored in: {}",
                    token_storage::current_storage_method()
                );
                println!();

                return Ok(());
            }
            device_code::PollResult::Pending => {
                // Continue polling
            }
            device_code::PollResult::SlowDown(new_interval) => {
                poll_interval = Duration::from_secs(new_interval);
            }
            device_code::PollResult::Expired => {
                println!();
                return Err(MsError::AuthError(
                    "Device code expired. Please run 'ms auth login' again.".to_string(),
                ));
            }
            device_code::PollResult::AccessDenied => {
                println!();
                return Err(MsError::AuthError(
                    "Access denied. The authorization request was rejected.".to_string(),
                ));
            }
        }
    }
}

fn status(ctx: &AppContext) -> Result<()> {
    let client = JfpAuthClient::new()?;
    let status = client.status()?;

    if ctx.output_format != OutputFormat::Human {
        return emit_json(&serde_json::json!({
            "authenticated": status.authenticated,
            "email": status.email,
            "tier": status.tier,
            "expires_in": status.expires_in,
            "storage_method": status.storage_method,
        }));
    }

    let mut layout = HumanLayout::new();
    layout.title("JFP Cloud Authentication Status");

    if status.authenticated {
        layout
            .kv("Status", "Authenticated ✓")
            .kv("Email", status.email.as_deref().unwrap_or("-"))
            .kv("Tier", status.tier.as_deref().unwrap_or("-"));

        if let Some(expires_in) = status.expires_in {
            let hours = expires_in / 3600;
            let minutes = (expires_in % 3600) / 60;
            layout.kv("Token Expires In", &format!("{}h {}m", hours, minutes));
        }

        if let Some(method) = &status.storage_method {
            layout.kv("Storage", method);
        }
    } else {
        layout
            .kv("Status", "Not authenticated")
            .blank()
            .bullet("Run 'ms auth login' to authenticate with JFP Cloud.");
    }

    emit_human(layout);
    Ok(())
}

fn logout(ctx: &AppContext) -> Result<()> {
    let client = JfpAuthClient::new()?;

    // Check if logged in first
    let status = client.status()?;
    if !status.authenticated {
        if ctx.output_format != OutputFormat::Human {
            return emit_json(&serde_json::json!({
                "status": "not_authenticated",
                "message": "Not logged in",
            }));
        }
        println!("Not logged in.");
        return Ok(());
    }

    let email = status.email.clone().unwrap_or_default();
    client.logout()?;

    if ctx.output_format != OutputFormat::Human {
        return emit_json(&serde_json::json!({
            "status": "logged_out",
            "email": email,
        }));
    }

    println!("Logged out. Local credentials cleared.");
    println!();
    println!("Note: The token is still valid on the server until it expires.");
    println!("Use 'ms auth revoke' to immediately invalidate the token.");

    Ok(())
}

fn revoke(ctx: &AppContext) -> Result<()> {
    let client = JfpAuthClient::new()?;

    // Check if logged in first
    let status = client.status()?;
    if !status.authenticated {
        if ctx.output_format != OutputFormat::Human {
            return emit_json(&serde_json::json!({
                "status": "not_authenticated",
                "message": "Not logged in",
            }));
        }
        println!("Not logged in.");
        return Ok(());
    }

    let email = status.email.clone().unwrap_or_default();
    client.revoke_token()?;

    if ctx.output_format != OutputFormat::Human {
        return emit_json(&serde_json::json!({
            "status": "revoked",
            "email": email,
        }));
    }

    println!("Token revoked and local credentials cleared.");
    println!("You will need to run 'ms auth login' to authenticate again.");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parse_auth_login() {
        let args = crate::cli::Cli::parse_from(["ms", "auth", "login"]);
        if let crate::cli::Commands::Auth(auth) = args.command {
            assert!(matches!(auth.command, AuthCommand::Login(_)));
        } else {
            panic!("expected auth command");
        }
    }

    #[test]
    fn parse_auth_login_with_url() {
        let args = crate::cli::Cli::parse_from([
            "ms",
            "auth",
            "login",
            "--api-url",
            "http://localhost:3000",
        ]);
        if let crate::cli::Commands::Auth(auth) = args.command {
            if let AuthCommand::Login(login) = auth.command {
                assert_eq!(login.api_url, Some("http://localhost:3000".to_string()));
            } else {
                panic!("expected login command");
            }
        } else {
            panic!("expected auth command");
        }
    }

    #[test]
    fn parse_auth_status() {
        let args = crate::cli::Cli::parse_from(["ms", "auth", "status"]);
        if let crate::cli::Commands::Auth(auth) = args.command {
            assert!(matches!(auth.command, AuthCommand::Status));
        } else {
            panic!("expected auth command");
        }
    }

    #[test]
    fn parse_auth_logout() {
        let args = crate::cli::Cli::parse_from(["ms", "auth", "logout"]);
        if let crate::cli::Commands::Auth(auth) = args.command {
            assert!(matches!(auth.command, AuthCommand::Logout));
        } else {
            panic!("expected auth command");
        }
    }

    #[test]
    fn parse_auth_revoke() {
        let args = crate::cli::Cli::parse_from(["ms", "auth", "revoke"]);
        if let crate::cli::Commands::Auth(auth) = args.command {
            assert!(matches!(auth.command, AuthCommand::Revoke));
        } else {
            panic!("expected auth command");
        }
    }
}
