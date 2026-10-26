//! ms update - Check for and apply updates

use clap::Args;
use semver::Version;

use crate::app::AppContext;
use crate::cli::output::OutputFormat;
use crate::error::{MsError, Result};
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdateCheckState {
    pub last_checked_at: chrono::DateTime<chrono::Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_version: Option<String>,
}

/// Check whether an automatic update check is due based on `UpdateConfig` (`auto_check`, `check_interval_hours`).
#[must_use]
pub fn is_update_check_due(
    ms_root: &std::path::Path,
    update_cfg: &crate::config::UpdateConfig,
) -> bool {
    if !update_cfg.auto_check {
        return false;
    }
    let state_path = ms_root.join("last_update_check.json");
    let Ok(raw) = std::fs::read_to_string(&state_path) else {
        return true;
    };
    let Ok(state) = serde_json::from_str::<UpdateCheckState>(&raw) else {
        return true;
    };
    let interval = chrono::Duration::hours(i64::from(update_cfg.check_interval_hours.max(1)));
    chrono::Utc::now().signed_duration_since(state.last_checked_at) >= interval
}

/// Record that an update check was performed in `<ms_root>/last_update_check.json`.
pub fn record_update_check(ms_root: &std::path::Path, latest_version: Option<&str>) {
    let state = UpdateCheckState {
        last_checked_at: chrono::Utc::now(),
        latest_version: latest_version.map(std::string::ToString::to_string),
    };
    if let Ok(serialized) = serde_json::to_string_pretty(&state) {
        let _ = std::fs::write(ms_root.join("last_update_check.json"), serialized);
    }
}

pub fn run(ctx: &AppContext, args: &UpdateArgs) -> Result<()> {
    let current_version =
        Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or_else(|_| Version::new(0, 1, 0));

    // Determine channel from args or config
    let channel_str = args.channel.as_ref().unwrap_or(&ctx.config.update.channel);
    let channel: UpdateChannel = channel_str.parse()?;
    let target_version = args
        .target_version
        .as_ref()
        .map(|v| Version::parse(v))
        .transpose()
        .map_err(|err| MsError::ValidationFailed(format!("invalid target version: {err}")))?;

    let checker = UpdateChecker::new(current_version, channel, DEFAULT_REPO.to_string());

    if ctx.output_format != OutputFormat::Human {
        run_robot(args, &checker, target_version.as_ref())?;
    } else {
        run_interactive(args, &checker, target_version.as_ref())?;
    }

    record_update_check(&ctx.ms_root, None);
    Ok(())
}

fn run_robot(
    args: &UpdateArgs,
    checker: &UpdateChecker,
    target_version: Option<&Version>,
) -> Result<()> {
    if args.check {
        // Check only
        let update = if let Some(target) = target_version {
            checker.get_version(target)?
        } else {
            checker.check()?
        };
        if update.is_none() && target_version.is_some() {
            return Err(MsError::NotFound(format!(
                "target version not found: {}",
                target_version.unwrap()
            )));
        }
        let update_available = update
            .as_ref()
            .is_some_and(|u| u.version != *checker.current_version());
        let response = UpdateCheckResponse {
            current_version: checker.current_version().to_string(),
            channel: checker.channel().to_string(),
            update_available,
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
        let update = if let Some(target) = target_version {
            checker.get_version(target)?
        } else if args.force {
            checker.get_latest()?
        } else {
            checker.check()?
        };

        if let Some(release) = update {
            if release.version == *checker.current_version()
                && target_version.is_some()
                && !args.force
            {
                let response = UpdateCheckResponse {
                    current_version: checker.current_version().to_string(),
                    channel: checker.channel().to_string(),
                    update_available: false,
                    latest_version: Some(release.version.to_string()),
                    changelog: None,
                    download_size: None,
                    html_url: None,
                };
                println!("{}", serde_json::to_string_pretty(&response)?);
                return Ok(());
            }
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
            if let Some(target) = target_version {
                return Err(MsError::NotFound(format!(
                    "target version not found: {target}"
                )));
            }
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

fn run_interactive(
    args: &UpdateArgs,
    checker: &UpdateChecker,
    target_version: Option<&Version>,
) -> Result<()> {
    println!(
        "Current version: {} ({})",
        checker.current_version(),
        checker.channel()
    );
    if let Some(target) = target_version {
        println!("Target version: {target}");
    }

    if args.check {
        // Check only mode
        let update = if let Some(target) = target_version {
            checker.get_version(target)?
        } else {
            checker.check()?
        };
        if let Some(release) = update {
            if release.version == *checker.current_version() && !args.force {
                println!("\n✓ You are already running v{}", release.version);
                return Ok(());
            }
            println!("\n✓ Update available: v{}", release.version);
            println!("  Released: {}", release.published_at.format("%Y-%m-%d"));
            if !release.changelog.is_empty() {
                println!("\nChangelog:");
                // Print first few lines
                for line in release.changelog.lines().take(10) {
                    println!("  {line}");
                }
                if release.changelog.lines().count() > 10 {
                    println!("  ...");
                }
            }
            println!("\nRun `ms update` to install.");
        } else {
            if let Some(target) = target_version {
                println!("\n✗ Target version not found: v{target}");
                return Err(MsError::NotFound(format!(
                    "target version not found: {target}"
                )));
            }
            println!("\n✓ You are up to date!");
        }
        return Ok(());
    }

    // Perform update
    println!("Checking for updates...");
    let update = if let Some(target) = target_version {
        checker.get_version(target)?
    } else if args.force {
        checker.get_latest()?
    } else {
        checker.check()?
    };

    let release = if let Some(r) = update {
        r
    } else {
        if let Some(target) = target_version {
            println!("✗ Target version not found: v{target}");
            return Err(MsError::NotFound(format!(
                "target version not found: {target}"
            )));
        }
        println!("✓ You are already running the latest version.");
        return Ok(());
    };

    if release.version == *checker.current_version() && target_version.is_some() && !args.force {
        println!("✓ You are already running v{}", release.version);
        return Ok(());
    }

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
    if let Some(backup) = &result.backup_path {
        if args.force {
            let _ = installer.cleanup_backup();
        } else {
            println!("  Previous binary backed up to: {}", backup.display());
        }
    }
    if result.restart_required {
        println!("\nPlease restart ms for changes to take effect.");
    }

    if !release.changelog.is_empty() {
        println!("\nChangelog:");
        for line in release.changelog.lines().take(15) {
            println!("  {line}");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_update_check_due_respects_auto_check_and_interval() {
        let tmp = tempfile::tempdir().unwrap();
        let mut cfg = crate::config::UpdateConfig {
            auto_check: false,
            check_interval_hours: 24,
            channel: "stable".to_string(),
        };
        assert!(!is_update_check_due(tmp.path(), &cfg));

        cfg.auto_check = true;
        // Missing state file -> due
        assert!(is_update_check_due(tmp.path(), &cfg));

        // Record check -> no longer due
        record_update_check(tmp.path(), Some("0.2.3"));
        assert!(!is_update_check_due(tmp.path(), &cfg));
    }
}
