//! ms doctor - Health checks and repairs

use std::path::Path;
use std::sync::Arc;

use clap::Args;
use colored::Colorize;

use crate::app::AppContext;
use crate::core::recovery::{RecoveryManager, RecoveryReport};
use crate::error::Result;
use crate::security::{SafetyGate, scan_secrets_summary};
use crate::storage::tx::GlobalLock;

#[derive(Args, Debug)]
pub struct DoctorArgs {
    /// Run a specific check only (e.g. safety, recovery)
    #[arg(long)]
    pub check: Option<String>,

    /// Attempt to fix issues automatically
    #[arg(long)]
    pub fix: bool,

    /// Check lock status
    #[arg(long)]
    pub check_lock: bool,

    /// Break a stale lock (use with caution)
    #[arg(long)]
    pub break_lock: bool,

    /// Run comprehensive recovery diagnostics
    #[arg(long)]
    pub comprehensive: bool,
}

pub fn run(ctx: &AppContext, args: &DoctorArgs) -> Result<()> {
    let mut issues_found = 0;
    let mut issues_fixed = 0;
    let verbose = ctx.verbosity > 0;

    println!("{}", "ms doctor - Health Checks".bold());
    println!();

    let run_only = args.check.as_deref();

    // Check lock status if requested or as part of general health check
    if run_only.is_none() && (args.check_lock || !args.break_lock) {
        issues_found += check_lock_status(ctx, verbose)?;
    }

    // Break lock if requested
    if run_only.is_none() && args.break_lock {
        let gate = SafetyGate::from_context(ctx);
        let lock_path = ctx.ms_root.join("ms.lock");
        let command_str = format!("rm -f {}", lock_path.display());
        gate.enforce(&command_str, None)?;
        if break_stale_lock(ctx)? {
            issues_fixed += 1;
            println!("{} Stale lock broken", "✓".green());
        }
    }

    // Check database integrity
    if run_only.is_none() {
        issues_found += check_database(ctx, verbose)?;
    }

    // Check Git archive integrity
    if run_only.is_none() {
        issues_found += check_git_archive(ctx, verbose)?;
    }

    // Check for incomplete transactions
    if run_only.is_none() {
        issues_found += check_transactions(ctx, args.fix, verbose, &mut issues_fixed)?;
    }

    // Run comprehensive recovery diagnostics if requested
    if run_only.is_none() && args.comprehensive {
        issues_found += run_comprehensive_check(ctx, args.fix, verbose, &mut issues_fixed)?;
    }

    // Run a specific check if requested
    if let Some(check) = run_only {
        issues_found += match check {
            "safety" => check_safety(ctx, verbose)?,
            "security" => check_security(ctx, verbose)?,
            "recovery" => run_comprehensive_check(ctx, args.fix, verbose, &mut issues_fixed)?,
            other => {
                println!("{} Unknown check: {}", "!".yellow(), other);
                println!("  Available checks: safety, security, recovery");
                1
            }
        };
    }

    // Summary
    println!();
    if issues_found == 0 {
        println!("{} All checks passed", "✓".green().bold());
    } else if args.fix && issues_fixed == issues_found {
        println!(
            "{} Found {} issues, fixed {}",
            "✓".green().bold(),
            issues_found,
            issues_fixed
        );
    } else {
        println!(
            "{} Found {} issues, fixed {}",
            "!".yellow().bold(),
            issues_found,
            issues_fixed
        );
        if !args.fix && issues_found > issues_fixed {
            println!("  Run with --fix to attempt automatic repairs");
        }
    }

    Ok(())
}

/// Check the global lock status
fn check_lock_status(ctx: &AppContext, verbose: bool) -> Result<usize> {
    print!("Checking lock status... ");

    let ms_root = &ctx.ms_root;

    match GlobalLock::status(ms_root)? {
        Some(holder) => {
            println!("{} Lock held", "!".yellow());
            println!("  PID: {}", holder.pid);
            println!("  Host: {}", holder.hostname);
            println!("  Since: {}", holder.acquired_at);

            // Check if process is still alive
            #[cfg(target_os = "linux")]
            {
                let proc_path = format!("/proc/{}", holder.pid);
                if !Path::new(&proc_path).exists() {
                    println!(
                        "  {} Process {} no longer exists - lock may be stale",
                        "!".yellow(),
                        holder.pid
                    );
                    println!("  Use --break-lock to remove stale lock");
                    return Ok(1);
                }
            }

            if verbose {
                println!("  Lock is held by an active process");
            }
            Ok(0) // Active lock is not an issue
        }
        None => {
            println!("{} No lock held", "✓".green());
            Ok(0)
        }
    }
}

/// Break a stale lock
fn break_stale_lock(ctx: &AppContext) -> Result<bool> {
    print!("Breaking stale lock... ");

    let ms_root = &ctx.ms_root;

    // First check if there's a lock to break
    match GlobalLock::status(ms_root)? {
        Some(holder) => {
            // Warn user about what we're doing
            println!();
            println!(
                "  {} Breaking lock held by PID {} on {} since {}",
                "!".yellow(),
                holder.pid,
                holder.hostname,
                holder.acquired_at
            );

            if GlobalLock::break_lock(ms_root)? {
                Ok(true)
            } else {
                println!("  Lock file not found");
                Ok(false)
            }
        }
        None => {
            println!("{} No lock to break", "✓".green());
            Ok(false)
        }
    }
}

/// Check database integrity
fn check_database(ctx: &AppContext, verbose: bool) -> Result<usize> {
    print!("Checking database... ");

    let db_path = ctx.ms_root.join("ms.db");
    if !db_path.exists() {
        println!("{} Database not found", "!".yellow());
        println!("  Run 'ms init' to create the database");
        return Ok(1);
    }

    // Try to open and run integrity check
    match crate::storage::Database::open(&db_path) {
        Ok(db) => {
            // Run SQLite integrity check
            match db.integrity_check() {
                Ok(true) => {
                    println!("{} OK", "✓".green());
                    if verbose {
                        println!("  Database path: {}", db_path.display());
                    }
                    Ok(0)
                }
                Ok(false) => {
                    println!("{} Integrity check failed", "✗".red());
                    Ok(1)
                }
                Err(e) => {
                    println!("{} Error: {}", "✗".red(), e);
                    Ok(1)
                }
            }
        }
        Err(e) => {
            println!("{} Cannot open: {}", "✗".red(), e);
            Ok(1)
        }
    }
}

/// Check Git archive integrity
fn check_git_archive(ctx: &AppContext, verbose: bool) -> Result<usize> {
    print!("Checking Git archive... ");

    let archive_path = ctx.ms_root.join("archive");
    if !archive_path.exists() {
        println!("{} Archive not found", "!".yellow());
        println!("  Run 'ms init' to create the archive");
        return Ok(1);
    }

    let git_dir = archive_path.join(".git");
    if !git_dir.exists() {
        println!("{} Not a Git repository", "✗".red());
        return Ok(1);
    }

    match crate::storage::GitArchive::open(&archive_path) {
        Ok(_git) => {
            println!("{} OK", "✓".green());
            if verbose {
                println!("  Archive path: {}", archive_path.display());
            }
            Ok(0)
        }
        Err(e) => {
            println!("{} Cannot open: {}", "✗".red(), e);
            Ok(1)
        }
    }
}

/// Check command safety (DCG) availability
fn check_safety(ctx: &AppContext, verbose: bool) -> Result<usize> {
    print!("Checking command safety... ");

    let gate = SafetyGate::from_context(ctx);
    let status = gate.status();

    match status.dcg_version {
        Some(version) => {
            println!("{} dcg {}", "✓".green(), version);
            if verbose {
                println!("  dcg_bin: {}", status.dcg_bin.display());
                if !status.packs.is_empty() {
                    println!("  packs: {}", status.packs.join(", "));
                }
            }
            Ok(0)
        }
        None => {
            println!("{} dcg not available", "!".yellow());
            Ok(1)
        }
    }
}

/// Comprehensive security check
fn check_security(ctx: &AppContext, verbose: bool) -> Result<usize> {
    println!("{}", "Security Checks".bold());
    println!("{}", "─".repeat(15));

    let mut issues = 0;

    // 1. Check DCG availability
    print!("  [1/5] Command safety (DCG)... ");
    let gate = SafetyGate::from_context(ctx);
    let status = gate.status();
    match status.dcg_version {
        Some(version) => {
            println!("{} v{}", "✓".green(), version);
        }
        None => {
            println!("{} not available", "!".yellow());
            println!("        Commands will run without safety checks");
            issues += 1;
        }
    }

    // 2. Check ACIP prompt availability
    print!("  [2/5] ACIP prompt... ");
    let acip_path = &ctx.config.security.acip.prompt_path;
    if acip_path.exists() {
        match crate::security::acip::prompt_version(acip_path) {
            Ok(Some(version)) => {
                println!("{} v{}", "✓".green(), version);
                if verbose {
                    println!("        Path: {}", acip_path.display());
                }
            }
            Ok(None) => {
                println!("{} no version detected", "!".yellow());
                issues += 1;
            }
            Err(e) => {
                println!("{} error: {}", "✗".red(), e);
                issues += 1;
            }
        }
    } else {
        println!("{} not found", "-".dimmed());
        if verbose {
            println!("        Expected: {}", acip_path.display());
        }
    }

    // 3. Check safety tier configuration
    print!("  [3/5] Safety tier config... ");
    if ctx.config.safety.require_verbatim_approval {
        println!(
            "{} verbatim approval required for dangerous commands",
            "✓".green()
        );
    } else {
        println!("{} verbatim approval disabled", "!".yellow());
        println!("        Dangerous commands may execute without explicit approval");
        issues += 1;
    }

    // 4. Scan evidence for secrets
    print!("  [4/5] Evidence secret scan... ");
    let evidence_dir = ctx.ms_root.join("archive").join("skills");
    if evidence_dir.exists() {
        let mut secrets_found = 0;
        let mut files_scanned = 0;

        // Scan a sample of evidence files
        if let Ok(entries) = std::fs::read_dir(&evidence_dir) {
            for entry in entries.take(50).flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|e| e == "json" || e == "md") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        files_scanned += 1;
                        let summary = scan_secrets_summary(&content);
                        if summary.total_count > 0 {
                            secrets_found += summary.total_count;
                            if verbose {
                                println!();
                                println!(
                                    "        {} potential secret(s) in {}",
                                    summary.total_count,
                                    path.display()
                                );
                            }
                        }
                    }
                }
            }
        }

        if secrets_found > 0 {
            println!(
                "{} {} potential secret(s) found",
                "!".yellow(),
                secrets_found
            );
            println!("        Review evidence files for sensitive data");
            issues += 1;
        } else {
            println!(
                "{} {} files scanned, no secrets detected",
                "✓".green(),
                files_scanned
            );
        }
    } else {
        println!("{} no evidence directory", "-".dimmed());
    }

    // 5. Check for .env files that shouldn't be tracked
    print!("  [5/5] Environment files... ");
    let mut env_issues = Vec::new();

    for env_file in &[
        ".env",
        ".env.local",
        ".env.production",
        "credentials.json",
        "secrets.yaml",
    ] {
        let path = ctx.ms_root.join(env_file);
        if path.exists() {
            env_issues.push(env_file.to_string());
        }
    }

    if env_issues.is_empty() {
        println!("{} no sensitive env files in ms root", "✓".green());
    } else {
        println!(
            "{} found sensitive files: {}",
            "!".yellow(),
            env_issues.join(", ")
        );
        println!("        These files should not be in the ms root directory");
        issues += env_issues.len();
    }

    // Summary
    println!();
    if issues == 0 {
        println!("{} All security checks passed", "✓".green().bold());
    } else {
        println!("{} {} security issue(s) found", "!".yellow().bold(), issues);
    }

    Ok(issues)
}

/// Check for incomplete transactions
fn check_transactions(
    ctx: &AppContext,
    fix: bool,
    verbose: bool,
    issues_fixed: &mut usize,
) -> Result<usize> {
    print!("Checking transactions... ");

    let db_path = ctx.ms_root.join("ms.db");
    let archive_path = ctx.ms_root.join("archive");

    if !db_path.exists() || !archive_path.exists() {
        println!("{} Skipped (database or archive not found)", "-".dimmed());
        return Ok(0);
    }

    let db = match crate::storage::Database::open(&db_path) {
        Ok(db) => std::sync::Arc::new(db),
        Err(_) => {
            println!("{} Skipped (cannot open database)", "-".dimmed());
            return Ok(0);
        }
    };

    let git = match crate::storage::GitArchive::open(&archive_path) {
        Ok(git) => std::sync::Arc::new(git),
        Err(_) => {
            println!("{} Skipped (cannot open archive)", "-".dimmed());
            return Ok(0);
        }
    };

    // Check for incomplete transactions
    let tx_mgr = crate::storage::TxManager::new(db.clone(), git.clone(), ctx.ms_root.clone())?;

    if fix {
        let report = tx_mgr.recover()?;
        if report.had_work() {
            println!("{} Recovered", "✓".green());
            if verbose {
                println!("  Rolled back: {}", report.rolled_back);
                println!("  Completed: {}", report.completed);
                println!("  Orphaned files cleaned: {}", report.orphaned_files);
            }
            *issues_fixed += report.rolled_back + report.completed + report.orphaned_files;
            Ok(report.rolled_back + report.completed + report.orphaned_files)
        } else {
            println!("{} OK", "✓".green());
            Ok(0)
        }
    } else {
        // Just check without fixing
        let incomplete = db.list_incomplete_transactions()?;
        if incomplete.is_empty() {
            println!("{} OK", "✓".green());
            Ok(0)
        } else {
            println!(
                "{} {} incomplete transactions",
                "!".yellow(),
                incomplete.len()
            );
            if verbose {
                for tx in &incomplete {
                    println!("  - {} ({}, phase: {})", tx.id, tx.entity_type, tx.phase);
                }
            }
            println!("  Run with --fix to recover transactions");
            Ok(incomplete.len())
        }
    }
}

/// Run comprehensive recovery diagnostics using RecoveryManager.
fn run_comprehensive_check(
    ctx: &AppContext,
    fix: bool,
    verbose: bool,
    issues_fixed: &mut usize,
) -> Result<usize> {
    println!();
    println!("{}", "Comprehensive Recovery Diagnostics".bold());
    println!("{}", "─".repeat(35));

    let db_path = ctx.ms_root.join("ms.db");
    let archive_path = ctx.ms_root.join("archive");

    // Build RecoveryManager with available resources
    let mut manager = RecoveryManager::new(&ctx.ms_root);

    if let Ok(db) = crate::storage::Database::open(&db_path) {
        manager = manager.with_db(Arc::new(db));
    }

    if let Ok(git) = crate::storage::GitArchive::open(&archive_path) {
        manager = manager.with_git(Arc::new(git));
    }

    // Run diagnosis or recovery
    let report = manager.recover(fix)?;
    print_recovery_report(&report, verbose);

    // Update fixed count
    *issues_fixed += report.fixed;

    Ok(report.issues.len())
}

/// Print a formatted recovery report.
fn print_recovery_report(report: &RecoveryReport, verbose: bool) {
    if report.issues.is_empty() {
        println!("{} No issues detected", "✓".green());
    } else {
        println!(
            "{} Found {} issues:",
            if report.has_critical_issues() {
                "✗".red()
            } else {
                "!".yellow()
            },
            report.issues.len()
        );

        for issue in &report.issues {
            let severity_marker = match issue.severity {
                1 => "CRITICAL".red().bold(),
                2 => "MAJOR".yellow(),
                _ => "MINOR".dimmed(),
            };

            println!(
                "  {} [{}] {}",
                if issue.auto_recoverable {
                    "→".green()
                } else {
                    "→".red()
                },
                severity_marker,
                issue.description
            );

            if verbose {
                println!("    Mode: {:?}", issue.mode);
                if let Some(fix) = &issue.suggested_fix {
                    println!("    Fix: {}", fix);
                }
            }
        }
    }

    if report.had_work() {
        println!();
        println!("{}", "Recovery actions:".bold());
        if report.rolled_back > 0 {
            println!(
                "  {} Rolled back {} transactions",
                "✓".green(),
                report.rolled_back
            );
        }
        if report.completed > 0 {
            println!(
                "  {} Completed {} transactions",
                "✓".green(),
                report.completed
            );
        }
        if report.orphaned_files > 0 {
            println!(
                "  {} Cleaned {} orphaned files",
                "✓".green(),
                report.orphaned_files
            );
        }
        if report.cache_invalidated > 0 {
            println!(
                "  {} Invalidated {} cache entries",
                "✓".green(),
                report.cache_invalidated
            );
        }
    }

    if let Some(duration) = report.duration {
        if verbose {
            println!();
            println!("  Duration: {:?}", duration);
        }
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
        doctor: DoctorArgs,
    }

    #[test]
    fn parse_doctor_defaults() {
        let cli = TestCli::try_parse_from(["test"]).unwrap();
        assert!(cli.doctor.check.is_none());
        assert!(!cli.doctor.fix);
        assert!(!cli.doctor.check_lock);
        assert!(!cli.doctor.break_lock);
        assert!(!cli.doctor.comprehensive);
    }

    #[test]
    fn parse_doctor_check_safety() {
        let cli = TestCli::try_parse_from(["test", "--check", "safety"]).unwrap();
        assert_eq!(cli.doctor.check, Some("safety".to_string()));
    }

    #[test]
    fn parse_doctor_check_security() {
        let cli = TestCli::try_parse_from(["test", "--check", "security"]).unwrap();
        assert_eq!(cli.doctor.check, Some("security".to_string()));
    }

    #[test]
    fn parse_doctor_check_recovery() {
        let cli = TestCli::try_parse_from(["test", "--check", "recovery"]).unwrap();
        assert_eq!(cli.doctor.check, Some("recovery".to_string()));
    }

    #[test]
    fn parse_doctor_fix() {
        let cli = TestCli::try_parse_from(["test", "--fix"]).unwrap();
        assert!(cli.doctor.fix);
    }

    #[test]
    fn parse_doctor_check_lock() {
        let cli = TestCli::try_parse_from(["test", "--check-lock"]).unwrap();
        assert!(cli.doctor.check_lock);
    }

    #[test]
    fn parse_doctor_break_lock() {
        let cli = TestCli::try_parse_from(["test", "--break-lock"]).unwrap();
        assert!(cli.doctor.break_lock);
    }

    #[test]
    fn parse_doctor_comprehensive() {
        let cli = TestCli::try_parse_from(["test", "--comprehensive"]).unwrap();
        assert!(cli.doctor.comprehensive);
    }

    #[test]
    fn parse_doctor_all_options() {
        let cli = TestCli::try_parse_from([
            "test",
            "--check",
            "safety",
            "--fix",
            "--check-lock",
            "--break-lock",
            "--comprehensive",
        ])
        .unwrap();

        assert_eq!(cli.doctor.check, Some("safety".to_string()));
        assert!(cli.doctor.fix);
        assert!(cli.doctor.check_lock);
        assert!(cli.doctor.break_lock);
        assert!(cli.doctor.comprehensive);
    }

    // =========================================================================
    // RecoveryReport tests
    // =========================================================================

    #[test]
    fn recovery_report_empty() {
        let report = RecoveryReport::default();
        assert!(report.issues.is_empty());
        assert!(!report.has_critical_issues());
        assert!(!report.had_work());
    }

    #[test]
    fn recovery_report_with_issues() {
        use crate::core::recovery::{FailureMode, RecoveryIssue};

        let mut report = RecoveryReport::default();
        report.issues.push(RecoveryIssue {
            description: "Test issue".to_string(),
            severity: 2,
            mode: FailureMode::Database,
            auto_recoverable: true,
            suggested_fix: Some("Fix this".to_string()),
        });

        assert_eq!(report.issues.len(), 1);
        assert!(!report.has_critical_issues()); // severity 2 is not critical
    }

    #[test]
    fn recovery_report_with_critical_issue() {
        use crate::core::recovery::{FailureMode, RecoveryIssue};

        let mut report = RecoveryReport::default();
        report.issues.push(RecoveryIssue {
            description: "Critical issue".to_string(),
            severity: 1, // Critical severity
            mode: FailureMode::Transaction,
            auto_recoverable: false,
            suggested_fix: None,
        });

        assert!(report.has_critical_issues());
    }

    #[test]
    fn recovery_report_had_work() {
        let mut report = RecoveryReport::default();
        report.rolled_back = 1;
        assert!(report.had_work());

        let mut report = RecoveryReport::default();
        report.completed = 1;
        assert!(report.had_work());

        let mut report = RecoveryReport::default();
        report.orphaned_files = 1;
        assert!(report.had_work());

        let mut report = RecoveryReport::default();
        report.cache_invalidated = 1;
        assert!(report.had_work());
    }

    // =========================================================================
    // Available checks tests
    // =========================================================================

    #[test]
    fn available_checks_are_documented() {
        // This test documents the available check types
        let available_checks = ["safety", "security", "recovery"];

        for check in &available_checks {
            let cli = TestCli::try_parse_from(["test", "--check", check]).unwrap();
            assert_eq!(cli.doctor.check, Some(check.to_string()));
        }
    }
}
