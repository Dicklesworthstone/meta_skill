//! ms prune - Prune tombstoned/outdated data
//!
//! Tombstones are created when files are "deleted" within ms-managed directories.
//! This command lists, purges, or restores tombstoned items.

use clap::{Args, Subcommand};
use colored::Colorize;
use serde_json::json;

use crate::app::AppContext;
use crate::error::Result;
use crate::security::SafetyGate;
use crate::storage::TombstoneManager;

#[derive(Args, Debug)]
pub struct PruneArgs {
    #[command(subcommand)]
    pub command: Option<PruneCommand>,

    /// Dry run - show what would be pruned (for list command)
    #[arg(long, global = true)]
    pub dry_run: bool,

    /// Older than N days (for list command)
    #[arg(long, global = true)]
    pub older_than: Option<u32>,
}

#[derive(Subcommand, Debug)]
pub enum PruneCommand {
    /// List tombstoned items (default if no subcommand)
    List,

    /// Purge (permanently delete) tombstoned items
    Purge(PurgeArgs),

    /// Restore a tombstoned item
    Restore(RestoreArgs),

    /// Show tombstone statistics
    Stats,
}

#[derive(Args, Debug)]
pub struct PurgeArgs {
    /// Tombstone ID to purge (or "all" for all tombstones)
    pub id: String,

    /// Approve the purge (required for destructive operation)
    #[arg(long)]
    pub approve: bool,

    /// Purge all tombstones older than N days
    #[arg(long)]
    pub older_than: Option<u32>,
}

#[derive(Args, Debug)]
pub struct RestoreArgs {
    /// Tombstone ID to restore
    pub id: String,
}

pub fn run(ctx: &AppContext, args: &PruneArgs) -> Result<()> {
    let command = args.command.as_ref().unwrap_or(&PruneCommand::List);

    match command {
        PruneCommand::List => run_list(ctx, args),
        PruneCommand::Purge(purge_args) => run_purge(ctx, purge_args),
        PruneCommand::Restore(restore_args) => run_restore(ctx, restore_args),
        PruneCommand::Stats => run_stats(ctx),
    }
}

fn run_list(ctx: &AppContext, args: &PruneArgs) -> Result<()> {
    let manager = TombstoneManager::new(&ctx.ms_root);

    let records = if let Some(days) = args.older_than {
        manager.list_older_than(days)?
    } else {
        manager.list()?
    };

    if ctx.robot_mode {
        let output = json!({
            "tombstones": records,
            "count": records.len(),
            "total_size_bytes": records.iter().map(|r| r.size_bytes).sum::<u64>(),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        if records.is_empty() {
            println!("No tombstones found.");
            return Ok(());
        }

        println!("{}", "Tombstoned Items".bold());
        println!("{}", "─".repeat(60));
        println!();

        for record in &records {
            let type_icon = if record.is_directory { "📁" } else { "📄" };
            let size = format_size(record.size_bytes);

            println!(
                "{} {} ({}) - {}",
                type_icon,
                record.original_path.cyan(),
                size.dimmed(),
                record
                    .tombstoned_at
                    .format("%Y-%m-%d %H:%M")
                    .to_string()
                    .dimmed()
            );
            println!("    ID: {}", record.id.dimmed());
            if let Some(reason) = &record.reason {
                println!("    Reason: {}", reason);
            }
            println!();
        }

        let total_size = records.iter().map(|r| r.size_bytes).sum::<u64>();
        println!(
            "Total: {} items, {}",
            records.len().to_string().cyan(),
            format_size(total_size).yellow()
        );

        if args.dry_run {
            println!();
            println!("  (dry run - no changes made)");
        }
    }

    Ok(())
}

fn run_purge(ctx: &AppContext, args: &PurgeArgs) -> Result<()> {
    let manager = TombstoneManager::new(&ctx.ms_root);
    let gate = SafetyGate::from_context(ctx);

    // Get the list of tombstones to purge
    let to_purge: Vec<_> = if args.id == "all" {
        if let Some(days) = args.older_than {
            manager.list_older_than(days)?
        } else {
            manager.list()?
        }
    } else {
        let all = manager.list()?;
        all.into_iter()
            .filter(|r| r.id == args.id || r.id.starts_with(&args.id))
            .collect()
    };

    if to_purge.is_empty() {
        if ctx.robot_mode {
            let output = json!({
                "error": true,
                "code": "not_found",
                "message": "No matching tombstones found",
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!("No matching tombstones found.");
        }
        return Ok(());
    }

    // Require approval for purge
    if !args.approve {
        if ctx.robot_mode {
            let output = json!({
                "error": true,
                "code": "approval_required",
                "message": "Purge requires --approve flag",
                "items_to_purge": to_purge.len(),
                "total_bytes": to_purge.iter().map(|r| r.size_bytes).sum::<u64>(),
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!("{}", "Approval Required".yellow().bold());
            println!();
            println!(
                "The following {} items will be permanently deleted:",
                to_purge.len()
            );
            for record in &to_purge {
                println!(
                    "  - {} ({})",
                    record.original_path,
                    format_size(record.size_bytes)
                );
            }
            println!();
            println!(
                "Total: {}",
                format_size(to_purge.iter().map(|r| r.size_bytes).sum::<u64>()).yellow()
            );
            println!();
            println!("Run with {} to confirm deletion.", "--approve".cyan());
        }
        return Ok(());
    }

    // Check with safety gate
    for record in &to_purge {
        let command = format!("ms prune purge {}", record.id);
        gate.enforce(&command, None)?;
    }

    // Perform the purge
    let mut results = Vec::new();
    let mut total_freed = 0u64;

    for record in &to_purge {
        match manager.purge(&record.id) {
            Ok(result) => {
                total_freed += result.bytes_freed;
                results.push(result);
            }
            Err(e) => {
                if !ctx.robot_mode {
                    println!("{} Failed to purge {}: {}", "✗".red(), record.id, e);
                }
            }
        }
    }

    if ctx.robot_mode {
        let output = json!({
            "purged": results,
            "count": results.len(),
            "bytes_freed": total_freed,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{} Purged {} items", "✓".green(), results.len());
        println!("  Freed: {}", format_size(total_freed).yellow());
    }

    Ok(())
}

fn run_restore(ctx: &AppContext, args: &RestoreArgs) -> Result<()> {
    let manager = TombstoneManager::new(&ctx.ms_root);

    // Find matching tombstone
    let all = manager.list()?;
    let matching: Vec<_> = all
        .iter()
        .filter(|r| r.id == args.id || r.id.starts_with(&args.id))
        .collect();

    if matching.is_empty() {
        if ctx.robot_mode {
            let output = json!({
                "error": true,
                "code": "not_found",
                "message": format!("No tombstone found matching: {}", args.id),
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!("No tombstone found matching: {}", args.id);
        }
        return Ok(());
    }

    if matching.len() > 1 {
        if ctx.robot_mode {
            let output = json!({
                "error": true,
                "code": "ambiguous",
                "message": "Multiple tombstones match. Please use a more specific ID.",
                "matches": matching.iter().map(|r| &r.id).collect::<Vec<_>>(),
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!("Multiple tombstones match. Please use a more specific ID:");
            for record in matching {
                println!("  - {} ({})", record.id, record.original_path);
            }
        }
        return Ok(());
    }

    let record = matching[0];
    let result = manager.restore(&record.id)?;

    if ctx.robot_mode {
        let output = json!({
            "restored": result,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!(
            "{} Restored {} to {}",
            "✓".green(),
            record.id.dimmed(),
            result.restored_path.cyan()
        );
    }

    Ok(())
}

fn run_stats(ctx: &AppContext) -> Result<()> {
    let manager = TombstoneManager::new(&ctx.ms_root);
    let records = manager.list()?;

    let total_size = records.iter().map(|r| r.size_bytes).sum::<u64>();
    let file_count = records.iter().filter(|r| !r.is_directory).count();
    let dir_count = records.iter().filter(|r| r.is_directory).count();

    // Age statistics
    let now = chrono::Utc::now();
    let older_than_7d = records
        .iter()
        .filter(|r| (now - r.tombstoned_at).num_days() > 7)
        .count();
    let older_than_30d = records
        .iter()
        .filter(|r| (now - r.tombstoned_at).num_days() > 30)
        .count();

    if ctx.robot_mode {
        let output = json!({
            "count": records.len(),
            "files": file_count,
            "directories": dir_count,
            "total_size_bytes": total_size,
            "older_than_7_days": older_than_7d,
            "older_than_30_days": older_than_30d,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", "Tombstone Statistics".bold());
        println!("{}", "─".repeat(30));
        println!();
        println!("  Total items:     {}", records.len().to_string().cyan());
        println!("  Files:           {}", file_count);
        println!("  Directories:     {}", dir_count);
        println!("  Total size:      {}", format_size(total_size).yellow());
        println!();
        println!("  Older than 7d:   {}", older_than_7d);
        println!("  Older than 30d:  {}", older_than_30d);

        if older_than_30d > 0 {
            println!();
            println!(
                "  {} Consider running: {} {}",
                "!".yellow(),
                "ms prune purge all --older-than 30 --approve".cyan(),
                ""
            );
        }
    }

    Ok(())
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    // =========================================================================
    // Argument parsing tests
    // =========================================================================

    #[derive(Parser, Debug)]
    #[command(name = "test")]
    struct TestCli {
        #[command(flatten)]
        prune: PruneArgs,
    }

    #[test]
    fn parse_prune_defaults() {
        let cli = TestCli::try_parse_from(["test"]).unwrap();
        assert!(cli.prune.command.is_none());
        assert!(!cli.prune.dry_run);
        assert!(cli.prune.older_than.is_none());
    }

    #[test]
    fn parse_prune_dry_run() {
        let cli = TestCli::try_parse_from(["test", "--dry-run"]).unwrap();
        assert!(cli.prune.dry_run);
    }

    #[test]
    fn parse_prune_older_than() {
        let cli = TestCli::try_parse_from(["test", "--older-than", "30"]).unwrap();
        assert_eq!(cli.prune.older_than, Some(30));
    }

    #[test]
    fn parse_prune_list_subcommand() {
        let cli = TestCli::try_parse_from(["test", "list"]).unwrap();
        assert!(matches!(cli.prune.command, Some(PruneCommand::List)));
    }

    #[test]
    fn parse_prune_list_with_flags() {
        let cli =
            TestCli::try_parse_from(["test", "list", "--dry-run", "--older-than", "7"]).unwrap();
        assert!(matches!(cli.prune.command, Some(PruneCommand::List)));
        assert!(cli.prune.dry_run);
        assert_eq!(cli.prune.older_than, Some(7));
    }

    #[test]
    fn parse_prune_stats_subcommand() {
        let cli = TestCli::try_parse_from(["test", "stats"]).unwrap();
        assert!(matches!(cli.prune.command, Some(PruneCommand::Stats)));
    }

    #[test]
    fn parse_prune_purge_with_id() {
        let cli = TestCli::try_parse_from(["test", "purge", "abc123"]).unwrap();
        match cli.prune.command {
            Some(PruneCommand::Purge(args)) => {
                assert_eq!(args.id, "abc123");
                assert!(!args.approve);
                assert!(args.older_than.is_none());
            }
            _ => panic!("Expected Purge subcommand"),
        }
    }

    #[test]
    fn parse_prune_purge_with_approve() {
        let cli = TestCli::try_parse_from(["test", "purge", "abc123", "--approve"]).unwrap();
        match cli.prune.command {
            Some(PruneCommand::Purge(args)) => {
                assert_eq!(args.id, "abc123");
                assert!(args.approve);
            }
            _ => panic!("Expected Purge subcommand"),
        }
    }

    #[test]
    fn parse_prune_purge_all_with_older_than() {
        let cli =
            TestCli::try_parse_from(["test", "purge", "all", "--older-than", "30", "--approve"])
                .unwrap();
        match cli.prune.command {
            Some(PruneCommand::Purge(args)) => {
                assert_eq!(args.id, "all");
                assert!(args.approve);
                assert_eq!(args.older_than, Some(30));
            }
            _ => panic!("Expected Purge subcommand"),
        }
    }

    #[test]
    fn parse_prune_restore_with_id() {
        let cli = TestCli::try_parse_from(["test", "restore", "xyz789"]).unwrap();
        match cli.prune.command {
            Some(PruneCommand::Restore(args)) => {
                assert_eq!(args.id, "xyz789");
            }
            _ => panic!("Expected Restore subcommand"),
        }
    }

    #[test]
    fn parse_prune_purge_requires_id() {
        let result = TestCli::try_parse_from(["test", "purge"]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_prune_restore_requires_id() {
        let result = TestCli::try_parse_from(["test", "restore"]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_prune_invalid_older_than() {
        let result = TestCli::try_parse_from(["test", "--older-than", "not-a-number"]);
        assert!(result.is_err());
    }

    // =========================================================================
    // format_size tests
    // =========================================================================

    #[test]
    fn format_size_zero_bytes() {
        assert_eq!(format_size(0), "0 B");
    }

    #[test]
    fn format_size_small_bytes() {
        assert_eq!(format_size(512), "512 B");
    }

    #[test]
    fn format_size_one_byte() {
        assert_eq!(format_size(1), "1 B");
    }

    #[test]
    fn format_size_max_bytes() {
        // Just under 1 KB
        assert_eq!(format_size(1023), "1023 B");
    }

    #[test]
    fn format_size_exactly_one_kb() {
        assert_eq!(format_size(1024), "1.00 KB");
    }

    #[test]
    fn format_size_kilobytes() {
        assert_eq!(format_size(2048), "2.00 KB");
    }

    #[test]
    fn format_size_kilobytes_fractional() {
        assert_eq!(format_size(1536), "1.50 KB");
    }

    #[test]
    fn format_size_max_kilobytes() {
        // Just under 1 MB (1024 * 1024 - 1)
        let result = format_size(1024 * 1024 - 1);
        assert!(result.ends_with(" KB"));
    }

    #[test]
    fn format_size_exactly_one_mb() {
        assert_eq!(format_size(1024 * 1024), "1.00 MB");
    }

    #[test]
    fn format_size_megabytes() {
        assert_eq!(format_size(5 * 1024 * 1024), "5.00 MB");
    }

    #[test]
    fn format_size_megabytes_fractional() {
        // 1.5 MB
        assert_eq!(format_size(1024 * 1024 + 512 * 1024), "1.50 MB");
    }

    #[test]
    fn format_size_max_megabytes() {
        // Just under 1 GB
        let result = format_size(1024 * 1024 * 1024 - 1);
        assert!(result.ends_with(" MB"));
    }

    #[test]
    fn format_size_exactly_one_gb() {
        assert_eq!(format_size(1024 * 1024 * 1024), "1.00 GB");
    }

    #[test]
    fn format_size_gigabytes() {
        assert_eq!(format_size(2 * 1024 * 1024 * 1024), "2.00 GB");
    }

    #[test]
    fn format_size_gigabytes_fractional() {
        // 1.5 GB
        let gb: u64 = 1024 * 1024 * 1024;
        assert_eq!(format_size(gb + gb / 2), "1.50 GB");
    }

    #[test]
    fn format_size_large_gigabytes() {
        // 100 GB
        let gb: u64 = 1024 * 1024 * 1024;
        assert_eq!(format_size(100 * gb), "100.00 GB");
    }
}
