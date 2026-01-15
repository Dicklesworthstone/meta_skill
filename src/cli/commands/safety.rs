//! Safety command - DCG safety gate status, logs, and command checking.

use clap::{Args, Subcommand};
use colored::Colorize;
use serde_json::json;

use crate::app::AppContext;
use crate::core::safety::SafetyTier;
use crate::error::Result;
use crate::security::SafetyGate;

#[derive(Args, Debug)]
pub struct SafetyArgs {
    #[command(subcommand)]
    pub command: SafetyCommand,
}

#[derive(Subcommand, Debug)]
pub enum SafetyCommand {
    /// Show DCG safety gate status
    Status,

    /// Show recent safety events log
    Log(LogArgs),

    /// Check a command through the safety gate
    Check(CheckArgs),
}

#[derive(Args, Debug)]
pub struct LogArgs {
    /// Maximum number of events to show
    #[arg(short, long, default_value = "20")]
    pub limit: usize,

    /// Filter by session ID
    #[arg(long)]
    pub session: Option<String>,

    /// Show only blocked events
    #[arg(long)]
    pub blocked_only: bool,
}

#[derive(Args, Debug)]
pub struct CheckArgs {
    /// The command to check
    pub command: String,

    /// Session ID for audit logging
    #[arg(long)]
    pub session_id: Option<String>,

    /// Dry run - don't log the event
    #[arg(long)]
    pub dry_run: bool,
}

pub fn run(ctx: &AppContext, args: &SafetyArgs) -> Result<()> {
    match &args.command {
        SafetyCommand::Status => run_status(ctx),
        SafetyCommand::Log(log_args) => run_log(ctx, log_args),
        SafetyCommand::Check(check_args) => run_check(ctx, check_args),
    }
}

/// Show DCG safety gate status.
fn run_status(ctx: &AppContext) -> Result<()> {
    let gate = SafetyGate::from_context(ctx);
    let status = gate.status();

    if ctx.robot_mode {
        let output = json!({
            "dcg_available": status.dcg_version.is_some(),
            "dcg_version": status.dcg_version,
            "dcg_bin": status.dcg_bin.display().to_string(),
            "packs": status.packs,
            "require_verbatim_approval": ctx.config.safety.require_verbatim_approval,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", "Safety Gate Status".bold());
        println!("{}", "─".repeat(30));
        println!();

        match &status.dcg_version {
            Some(version) => {
                println!("  {} DCG Available", "✓".green());
                println!("    Version: {}", version.cyan());
            }
            None => {
                println!("  {} DCG Not Available", "✗".red());
                println!("    Commands will be allowed with warnings");
            }
        }
        println!("    Binary: {}", status.dcg_bin.display());

        if !status.packs.is_empty() {
            println!("    Packs: {}", status.packs.join(", ").dimmed());
        }

        println!();
        println!(
            "  Verbatim Approval: {}",
            if ctx.config.safety.require_verbatim_approval {
                "Required".yellow()
            } else {
                "Disabled".dimmed()
            }
        );
    }

    Ok(())
}

/// Show recent safety events log.
fn run_log(ctx: &AppContext, args: &LogArgs) -> Result<()> {
    let events = ctx.db.list_command_safety_events(args.limit)?;

    // Filter events if needed
    let events: Vec<_> = events
        .into_iter()
        .filter(|e| {
            if let Some(session) = &args.session {
                e.session_id.as_deref() == Some(session.as_str())
            } else {
                true
            }
        })
        .filter(|e| {
            if args.blocked_only {
                !e.decision.allowed
            } else {
                true
            }
        })
        .collect();

    if ctx.robot_mode {
        let output = json!({
            "events": events,
            "count": events.len(),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        if events.is_empty() {
            println!("No safety events found.");
            return Ok(());
        }

        println!("{}", "Safety Event Log".bold());
        println!("{}", "─".repeat(60));
        println!();

        for event in &events {
            let status_icon = if event.decision.allowed {
                if event.decision.approved {
                    "✓".green()
                } else {
                    "○".green()
                }
            } else {
                "✗".red()
            };

            let tier_label = format_tier(&event.decision.tier);

            println!(
                "{} [{}] {} {}",
                status_icon,
                tier_label,
                event.created_at.dimmed(),
                event.session_id.as_deref().unwrap_or("-").dimmed()
            );
            println!("    {}", truncate_command(&event.command, 70));

            if !event.decision.allowed {
                println!("    Reason: {}", event.decision.reason.yellow());
                if let Some(remediation) = &event.decision.remediation {
                    println!("    Fix: {}", remediation.cyan());
                }
            }
            println!();
        }

        println!(
            "Showing {} of {} events",
            events.len().to_string().cyan(),
            args.limit.to_string().dimmed()
        );
    }

    Ok(())
}

/// Check a command through the safety gate.
fn run_check(ctx: &AppContext, args: &CheckArgs) -> Result<()> {
    let gate = SafetyGate::from_context(ctx);

    // Get DCG decision without logging
    let status = gate.status();
    if status.dcg_version.is_none() {
        if ctx.robot_mode {
            let output = json!({
                "error": true,
                "code": "dcg_unavailable",
                "message": "DCG is not available",
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!("{} DCG is not available", "!".yellow());
            println!("  Configure dcg_bin in your ms config");
        }
        return Ok(());
    }

    // Use the core safety module directly to avoid logging on dry run
    use crate::core::safety::DcgGuard;
    let guard = DcgGuard::new(
        ctx.config.safety.dcg_bin.clone(),
        ctx.config.safety.dcg_packs.clone(),
        ctx.config.safety.dcg_explain_format.clone(),
    );

    let decision = match guard.evaluate_command(&args.command) {
        Ok(d) => d,
        Err(e) => {
            if ctx.robot_mode {
                let output = json!({
                    "error": true,
                    "code": "dcg_error",
                    "message": e.to_string(),
                });
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                println!("{} DCG error: {}", "✗".red(), e);
            }
            return Ok(());
        }
    };

    // Determine if approval would be required
    let approval_required = !decision.allowed
        && ctx.config.safety.require_verbatim_approval
        && decision.tier >= SafetyTier::Danger;

    if ctx.robot_mode {
        let output = json!({
            "command": args.command,
            "allowed": decision.allowed,
            "tier": format!("{:?}", decision.tier).to_lowercase(),
            "reason": decision.reason,
            "remediation": decision.remediation,
            "rule_id": decision.rule_id,
            "pack": decision.pack,
            "approval_required": approval_required,
            "approval_hint": if approval_required {
                Some(format!("MS_APPROVE_COMMAND=\"{}\"", args.command))
            } else {
                None
            },
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", "Safety Check Result".bold());
        println!("{}", "─".repeat(40));
        println!();

        println!("  Command: {}", args.command.cyan());
        println!();

        let status_icon = if decision.allowed {
            "✓".green()
        } else {
            "✗".red()
        };
        let tier_label = format_tier(&decision.tier);

        println!("  {} {} - {}", status_icon, tier_label, decision.reason);

        if let Some(rule_id) = &decision.rule_id {
            println!("    Rule: {}", rule_id.dimmed());
        }
        if let Some(pack) = &decision.pack {
            println!("    Pack: {}", pack.dimmed());
        }

        if !decision.allowed {
            if let Some(remediation) = &decision.remediation {
                println!();
                println!("  {} {}", "Suggestion:".yellow(), remediation);
            }

            if approval_required {
                println!();
                println!(
                    "  {} Approval required. Set environment variable:",
                    "!".yellow()
                );
                println!(
                    "    {}=\"{}\"",
                    "MS_APPROVE_COMMAND".cyan(),
                    args.command
                );
            }
        }
    }

    // Log the event if not dry run
    if !args.dry_run {
        let _session_id = args.session_id.as_deref();
        // Note: In a full implementation, we would log the event here
        // using gate.enforce() or a similar mechanism
    }

    Ok(())
}

fn format_tier(tier: &SafetyTier) -> colored::ColoredString {
    match tier {
        SafetyTier::Safe => "SAFE".green(),
        SafetyTier::Caution => "CAUTION".yellow(),
        SafetyTier::Danger => "DANGER".red(),
        SafetyTier::Critical => "CRITICAL".red().bold(),
    }
}

fn truncate_command(cmd: &str, max_len: usize) -> String {
    if cmd.chars().count() <= max_len {
        cmd.to_string()
    } else {
        let truncated: String = cmd.chars().take(max_len.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    // =========================================================================
    // truncate_command tests
    // =========================================================================

    #[test]
    fn truncate_command_short_string_unchanged() {
        let cmd = "ls -la";
        assert_eq!(truncate_command(cmd, 10), "ls -la");
    }

    #[test]
    fn truncate_command_exact_length_unchanged() {
        let cmd = "exactly10!";
        assert_eq!(cmd.chars().count(), 10);
        assert_eq!(truncate_command(cmd, 10), "exactly10!");
    }

    #[test]
    fn truncate_command_long_string_truncated() {
        let cmd = "this is a very long command that should be truncated";
        let result = truncate_command(cmd, 20);
        assert_eq!(result, "this is a very lo...");
        assert_eq!(result.chars().count(), 20);
    }

    #[test]
    fn truncate_command_multibyte_utf8() {
        // "日本語" = 3 chars, 9 bytes
        let cmd = "日本語テスト";
        assert_eq!(cmd.chars().count(), 6);

        // Truncate to 5 chars: should get "日本..." (2 chars + "...")
        let result = truncate_command(cmd, 5);
        assert_eq!(result, "日本...");
        assert_eq!(result.chars().count(), 5);
    }

    #[test]
    fn truncate_command_empty_string() {
        assert_eq!(truncate_command("", 10), "");
    }

    #[test]
    fn truncate_command_max_len_zero() {
        // Edge case: max_len 0 means truncate to "..." but saturating_sub prevents underflow
        let result = truncate_command("hello", 0);
        assert_eq!(result, "...");
    }

    #[test]
    fn truncate_command_max_len_three() {
        // With max_len 3, we get 0 chars + "..." = "..."
        let result = truncate_command("hello", 3);
        assert_eq!(result, "...");
    }

    #[test]
    fn truncate_command_max_len_four() {
        // With max_len 4, we get 1 char + "..." = "h..."
        let result = truncate_command("hello", 4);
        assert_eq!(result, "h...");
    }

    #[test]
    fn truncate_command_emoji() {
        // Emoji are single chars but multi-byte
        let cmd = "🚀🎉🔥💯";
        assert_eq!(cmd.chars().count(), 4);

        let result = truncate_command(cmd, 3);
        // max_len 3 means 0 chars + "..." but we have emoji
        assert_eq!(result, "...");
    }

    #[test]
    fn truncate_command_mixed_content() {
        // "rm -rf /日本語/🚀" = 13 chars
        let cmd = "rm -rf /日本語/🚀";
        assert_eq!(cmd.chars().count(), 13);

        // Truncate to 10 chars: should get 7 chars + "..."
        let result = truncate_command(cmd, 10);
        assert!(result.ends_with("..."));
        assert_eq!(result.chars().count(), 10);
    }

    // =========================================================================
    // format_tier tests
    // =========================================================================

    #[test]
    fn format_tier_safe() {
        let result = format_tier(&SafetyTier::Safe);
        // ColoredString implements Display, check the underlying string
        assert!(result.to_string().contains("SAFE"));
    }

    #[test]
    fn format_tier_caution() {
        let result = format_tier(&SafetyTier::Caution);
        assert!(result.to_string().contains("CAUTION"));
    }

    #[test]
    fn format_tier_danger() {
        let result = format_tier(&SafetyTier::Danger);
        assert!(result.to_string().contains("DANGER"));
    }

    #[test]
    fn format_tier_critical() {
        let result = format_tier(&SafetyTier::Critical);
        assert!(result.to_string().contains("CRITICAL"));
    }

    // =========================================================================
    // Argument parsing tests
    // =========================================================================

    // CLI wrapper for testing
    #[derive(Parser, Debug)]
    #[command(name = "test")]
    struct TestCli {
        #[command(flatten)]
        safety: SafetyArgs,
    }

    #[test]
    fn parse_status_command() {
        let cli = TestCli::try_parse_from(["test", "status"]).unwrap();
        assert!(matches!(cli.safety.command, SafetyCommand::Status));
    }

    #[test]
    fn parse_log_command_defaults() {
        let cli = TestCli::try_parse_from(["test", "log"]).unwrap();
        match cli.safety.command {
            SafetyCommand::Log(args) => {
                assert_eq!(args.limit, 20);
                assert!(args.session.is_none());
                assert!(!args.blocked_only);
            }
            _ => panic!("Expected Log command"),
        }
    }

    #[test]
    fn parse_log_command_with_limit() {
        let cli = TestCli::try_parse_from(["test", "log", "--limit", "50"]).unwrap();
        match cli.safety.command {
            SafetyCommand::Log(args) => {
                assert_eq!(args.limit, 50);
            }
            _ => panic!("Expected Log command"),
        }
    }

    #[test]
    fn parse_log_command_with_session() {
        let cli = TestCli::try_parse_from(["test", "log", "--session", "abc123"]).unwrap();
        match cli.safety.command {
            SafetyCommand::Log(args) => {
                assert_eq!(args.session, Some("abc123".to_string()));
            }
            _ => panic!("Expected Log command"),
        }
    }

    #[test]
    fn parse_log_command_blocked_only() {
        let cli = TestCli::try_parse_from(["test", "log", "--blocked-only"]).unwrap();
        match cli.safety.command {
            SafetyCommand::Log(args) => {
                assert!(args.blocked_only);
            }
            _ => panic!("Expected Log command"),
        }
    }

    #[test]
    fn parse_log_command_all_options() {
        let cli = TestCli::try_parse_from([
            "test",
            "log",
            "--limit",
            "100",
            "--session",
            "session-xyz",
            "--blocked-only",
        ])
        .unwrap();
        match cli.safety.command {
            SafetyCommand::Log(args) => {
                assert_eq!(args.limit, 100);
                assert_eq!(args.session, Some("session-xyz".to_string()));
                assert!(args.blocked_only);
            }
            _ => panic!("Expected Log command"),
        }
    }

    #[test]
    fn parse_check_command() {
        let cli = TestCli::try_parse_from(["test", "check", "ls -la"]).unwrap();
        match cli.safety.command {
            SafetyCommand::Check(args) => {
                assert_eq!(args.command, "ls -la");
                assert!(args.session_id.is_none());
                assert!(!args.dry_run);
            }
            _ => panic!("Expected Check command"),
        }
    }

    #[test]
    fn parse_check_command_with_session() {
        let cli =
            TestCli::try_parse_from(["test", "check", "rm -rf /tmp", "--session-id", "sess-001"])
                .unwrap();
        match cli.safety.command {
            SafetyCommand::Check(args) => {
                assert_eq!(args.command, "rm -rf /tmp");
                assert_eq!(args.session_id, Some("sess-001".to_string()));
            }
            _ => panic!("Expected Check command"),
        }
    }

    #[test]
    fn parse_check_command_dry_run() {
        let cli = TestCli::try_parse_from(["test", "check", "echo hello", "--dry-run"]).unwrap();
        match cli.safety.command {
            SafetyCommand::Check(args) => {
                assert!(args.dry_run);
            }
            _ => panic!("Expected Check command"),
        }
    }

    #[test]
    fn parse_check_command_all_options() {
        let cli = TestCli::try_parse_from([
            "test",
            "check",
            "chmod 777 /",
            "--session-id",
            "danger-test",
            "--dry-run",
        ])
        .unwrap();
        match cli.safety.command {
            SafetyCommand::Check(args) => {
                assert_eq!(args.command, "chmod 777 /");
                assert_eq!(args.session_id, Some("danger-test".to_string()));
                assert!(args.dry_run);
            }
            _ => panic!("Expected Check command"),
        }
    }

    #[test]
    fn parse_short_limit_flag() {
        let cli = TestCli::try_parse_from(["test", "log", "-l", "5"]).unwrap();
        match cli.safety.command {
            SafetyCommand::Log(args) => {
                assert_eq!(args.limit, 5);
            }
            _ => panic!("Expected Log command"),
        }
    }

    // =========================================================================
    // Error case tests
    // =========================================================================

    #[test]
    fn parse_check_command_missing_argument() {
        let result = TestCli::try_parse_from(["test", "check"]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_invalid_subcommand() {
        let result = TestCli::try_parse_from(["test", "invalid"]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_log_invalid_limit() {
        let result = TestCli::try_parse_from(["test", "log", "--limit", "not-a-number"]);
        assert!(result.is_err());
    }
}
