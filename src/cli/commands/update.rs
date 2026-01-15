//! ms update - Check for and apply updates

use clap::Args;
use semver::Version;

use crate::app::AppContext;
use crate::error::Result;
use crate::updater::{
    UpdateChannel, UpdateCheckResponse, UpdateChecker, UpdateDownloader, UpdateInstallResponse,
    UpdateInstaller,
};
use serde_json;

/// Default repository for updates.
const DEFAULT_REPO: &str = "Dicklesworthstone/meta_skill";

#[derive(Args, Debug)]
#[command(disable_version_flag = true)]
pub struct UpdateArgs {
    /// Check for updates without applying
    #[arg(long)]
    pub check: bool,

    /// Force update even if up to date
    #[arg(long)]
    pub force: bool,

    /// Update to specific version
    #[arg(long)]
    pub target_version: Option<String>,

    /// Update channel (stable, beta, nightly)
    #[arg(long)]
    pub channel: Option<String>,
}

pub fn run(ctx: &AppContext, args: &UpdateArgs) -> Result<()> {
    let current_version =
        Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or_else(|_| Version::new(0, 1, 0));

    // Determine channel from args or config
    let channel_str = args.channel.as_ref().unwrap_or(&ctx.config.update.channel);
    let channel: UpdateChannel = channel_str.parse()?;

    let checker = UpdateChecker::new(current_version.clone(), channel, DEFAULT_REPO.to_string());

    if ctx.robot_mode {
        run_robot(args, &checker)?;
    } else {
        run_interactive(args, &checker)?;
    }

    Ok(())
}

fn run_robot(args: &UpdateArgs, checker: &UpdateChecker) -> Result<()> {
    if args.check {
        // Check only
        let update = checker.check()?;
        let response = UpdateCheckResponse {
            current_version: checker.current_version().to_string(),
            channel: checker.channel().to_string(),
            update_available: update.is_some(),
            latest_version: update.as_ref().map(|u| u.version.to_string()),
            changelog: update.as_ref().map(|u| u.changelog.clone()),
            download_size: update
                .as_ref()
                .and_then(|u| u.assets.first().map(|a| a.size)),
            html_url: update.as_ref().map(|u| u.html_url.clone()),
        };
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        // Perform update
        let update = if args.force {
            checker.get_latest()?
        } else {
            checker.check()?
        };

        if let Some(release) = update {
            let downloader = UpdateDownloader::new()?;
            let binary_path = downloader.download_and_verify(&release)?;
            let installer = UpdateInstaller::new()?;
            let result = installer.install(&binary_path)?;
            downloader.cleanup()?;

            let response = UpdateInstallResponse {
                success: true,
                old_version: checker.current_version().to_string(),
                new_version: release.version.to_string(),
                changelog: release.changelog,
                restart_required: result.restart_required,
            };
            println!("{}", serde_json::to_string_pretty(&response)?);
        } else {
            let response = UpdateCheckResponse {
                current_version: checker.current_version().to_string(),
                channel: checker.channel().to_string(),
                update_available: false,
                latest_version: None,
                changelog: None,
                download_size: None,
                html_url: None,
            };
            println!("{}", serde_json::to_string_pretty(&response)?);
        }
    }

    Ok(())
}

fn run_interactive(args: &UpdateArgs, checker: &UpdateChecker) -> Result<()> {
    println!(
        "Current version: {} ({})",
        checker.current_version(),
        checker.channel()
    );

    if args.check {
        // Check only mode
        let update = checker.check()?;
        match update {
            Some(release) => {
                println!("\n✓ Update available: v{}", release.version);
                println!("  Released: {}", release.published_at.format("%Y-%m-%d"));
                if !release.changelog.is_empty() {
                    println!("\nChangelog:");
                    // Print first few lines
                    for line in release.changelog.lines().take(10) {
                        println!("  {}", line);
                    }
                    if release.changelog.lines().count() > 10 {
                        println!("  ...");
                    }
                }
                println!("\nRun `ms update` to install.");
            }
            None => {
                println!("\n✓ You are up to date!");
            }
        }
        return Ok(());
    }

    // Perform update
    println!("Checking for updates...");
    let update = if args.force {
        checker.get_latest()?
    } else {
        checker.check()?
    };

    let release = match update {
        Some(r) => r,
        None => {
            println!("✓ You are already running the latest version.");
            return Ok(());
        }
    };

    println!("\nUpdate available: v{}", release.version);

    // Download
    print!("Downloading...");
    let downloader = UpdateDownloader::new()?;
    let binary_path = downloader.download_and_verify(&release)?;
    println!(" done");

    // Install
    print!("Installing...");
    let installer = UpdateInstaller::new()?;
    let result = installer.install(&binary_path)?;
    println!(" done");

    // Cleanup
    downloader.cleanup()?;

    println!("\n✓ Successfully updated to v{}", release.version);
    if result.restart_required {
        println!("\nPlease restart ms for changes to take effect.");
    }

    if !release.changelog.is_empty() {
        println!("\nChangelog:");
        for line in release.changelog.lines().take(15) {
            println!("  {}", line);
        }
    }

    Ok(())
}
