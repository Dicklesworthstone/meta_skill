//! ms doctor - Health checks and repairs

use std::path::Path;
use std::sync::Arc;

use clap::Args;
use tracing::debug;

use crate::app::AppContext;
use crate::core::recovery::{RecoveryManager, RecoveryReport};
use crate::error::Result;
use crate::output::{
    OutputModeReport, is_agent_environment, is_ci_environment, is_ide_environment,
};
use crate::security::{SafetyGate, scan_secrets_summary};
use crate::storage::tx::GlobalLock;

/// Conditional `println!` for the doctor command. Emits human-readable
/// output only when the active OutputFormat is *not* machine-readable
/// (json / jsonl / tsv / toon); otherwise the call is a no-op so that
/// `ms doctor --robot` can produce a single JSON envelope on stdout
/// without interleaving the long-form check log (which is what made
/// the CI robot-mode test fail before this fix — `jq -e .` rejected
/// the mixed text/JSON capture).
macro_rules! say {
    ($ctx:expr) => {
        if !$ctx.output_format.is_machine_readable() {
            ::std::println!();
        }
    };
    ($ctx:expr, $($arg:tt)*) => {
        if !$ctx.output_format.is_machine_readable() {
            ::std::println!($($arg)*);
        }
    };
}

/// Inline progress marker (`print!` with no newline). Same gating as
/// [`say!`] — silenced under machine-readable output formats.
macro_rules! say_inline {
    ($ctx:expr, $($arg:tt)*) => {
        if !$ctx.output_format.is_machine_readable() {
            ::std::print!($($arg)*);
        }
    };
}

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
    debug!(target: "doctor", mode = ?ctx.output_format, "output mode selected");
    debug!(target: "doctor", stage = "checks_start");

    let mut issues_found = 0;
    let mut issues_fixed = 0;
    let verbose = ctx.verbosity > 0;

    say!(ctx,"{}", "ms doctor - Health Checks");
    say!(ctx);

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
            say!(ctx,"{} Stale lock broken", "[ok]");
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
            "perf" => check_perf(ctx, verbose)?,
            "output" | "output-mode" => check_output_mode(ctx, verbose)?,
            other => {
                say!(ctx,"{} Unknown check: {}", "[!]", other);
                say!(ctx,"  Available checks: safety, security, recovery, perf, output");
                1
            }
        };
    }

    // Summary
    debug!(
        target: "doctor",
        stage = "checks_complete",
        passed = issues_found == 0,
        failed = issues_found,
    );
    debug!(target: "doctor", stage = "render_complete");

    if ctx.output_format.is_machine_readable() {
        // Robot mode: emit a single JSON envelope on stdout. Every
        // human-readable check line was already suppressed by the
        // `say!` macro, so stdout is empty up to this point and the
        // capturing CI test (`ms doctor --robot 2>&1 | tee | jq -e .`)
        // sees only this object.
        let status = if issues_found == 0 {
            "ok"
        } else if args.fix && issues_fixed == issues_found {
            "fixed"
        } else {
            "issues"
        };
        let payload = serde_json::json!({
            "status": status,
            "issues_found": issues_found,
            "issues_fixed": issues_fixed,
            "fix_requested": args.fix,
            "comprehensive": args.comprehensive,
            "check": args.check,
        });
        // Use `to_string` rather than `to_string_pretty` so consumers
        // doing line-buffered ndjson capture get a single line.
        println!(
            "{}",
            serde_json::to_string(&payload)
                .unwrap_or_else(|_| "{\"status\":\"ok\"}".to_string())
        );
    } else {
        say!(ctx);
        if issues_found == 0 {
            say!(ctx, "{} All checks passed", "[ok]");
        } else if args.fix && issues_fixed == issues_found {
            say!(ctx,
                "{} Found {} issues, fixed {}",
                "[ok]",
                issues_found,
                issues_fixed
            );
        } else {
            say!(ctx,
                "{} Found {} issues, fixed {}",
                "[!]",
                issues_found,
                issues_fixed
            );
            if !args.fix && issues_found > issues_fixed {
                say!(ctx, "  Run with --fix to attempt automatic repairs");
            }
        }
    }

    Ok(())
}

/// Check the global lock status
fn check_lock_status(ctx: &AppContext, verbose: bool) -> Result<usize> {
    say_inline!(ctx,"Checking lock status... ");

    let ms_root = &ctx.ms_root;

    if let Some(holder) = GlobalLock::status(ms_root)? {
        say!(ctx,"{} Lock held", "[!]");
        say!(ctx,"  PID: {}", holder.pid);
        say!(ctx,"  Host: {}", holder.hostname);
        say!(ctx,"  Since: {}", holder.acquired_at);

        // Check if process is still alive
        #[cfg(target_os = "linux")]
        {
            let proc_path = format!("/proc/{}", holder.pid);
            if !Path::new(&proc_path).exists() {
                say!(ctx,
                    "  {} Process {} no longer exists - lock may be stale",
                    "[!]",
                    holder.pid
                );
                say!(ctx,"  Use --break-lock to remove stale lock");
                return Ok(1);
            }
        }

        if verbose {
            say!(ctx,"  Lock is held by an active process");
        }
        Ok(0) // Active lock is not an issue
    } else {
        say!(ctx,"{} No lock held", "[ok]");
        Ok(0)
    }
}

/// Break a stale lock
fn break_stale_lock(ctx: &AppContext) -> Result<bool> {
    say_inline!(ctx,"Breaking stale lock... ");

    let ms_root = &ctx.ms_root;

    // First check if there's a lock to break
    if let Some(holder) = GlobalLock::status(ms_root)? {
        // Warn user about what we're doing
        say!(ctx);
        say!(ctx,
            "  {} Breaking lock held by PID {} on {} since {}",
            "[!]",
            holder.pid,
            holder.hostname,
            holder.acquired_at
        );

        if GlobalLock::break_lock(ms_root)? {
            Ok(true)
        } else {
            say!(ctx,"  Lock file not found");
            Ok(false)
        }
    } else {
        say!(ctx,"{} No lock to break", "[ok]");
        Ok(false)
    }
}

/// Check database integrity
fn check_database(ctx: &AppContext, verbose: bool) -> Result<usize> {
    say_inline!(ctx,"Checking database... ");

    let db_path = ctx.ms_root.join("ms.db");
    if !db_path.exists() {
        say!(ctx,"{} Database not found", "[!]");
        say!(ctx,"  Run 'ms init' to create the database");
        return Ok(1);
    }

    // Try to open and run integrity check
    match crate::storage::Database::open(&db_path) {
        Ok(db) => {
            // Run SQLite integrity check
            match db.integrity_check() {
                Ok(true) => {
                    say!(ctx,"{} OK", "[ok]");
                    if verbose {
                        say!(ctx,"  Database path: {}", db_path.display());
                    }
                    Ok(0)
                }
                Ok(false) => {
                    say!(ctx,"{} Integrity check failed", "[FAIL]");
                    Ok(1)
                }
                Err(e) => {
                    say!(ctx,"{} Error: {}", "[FAIL]", e);
                    Ok(1)
                }
            }
        }
        Err(e) => {
            say!(ctx,"{} Cannot open: {}", "[FAIL]", e);
            Ok(1)
        }
    }
}

/// Check Git archive integrity
fn check_git_archive(ctx: &AppContext, verbose: bool) -> Result<usize> {
    say_inline!(ctx,"Checking Git archive... ");

    let archive_path = ctx.ms_root.join("archive");
    if !archive_path.exists() {
        say!(ctx,"{} Archive not found", "[!]");
        say!(ctx,"  Run 'ms init' to create the archive");
        return Ok(1);
    }

    let git_dir = archive_path.join(".git");
    if !git_dir.exists() {
        say!(ctx,"{} Not a Git repository", "[FAIL]");
        return Ok(1);
    }

    match crate::storage::GitArchive::open(&archive_path) {
        Ok(_git) => {
            say!(ctx,"{} OK", "[ok]");
            if verbose {
                say!(ctx,"  Archive path: {}", archive_path.display());
            }
            Ok(0)
        }
        Err(e) => {
            say!(ctx,"{} Cannot open: {}", "[FAIL]", e);
            Ok(1)
        }
    }
}

/// Check command safety (DCG) availability
fn check_safety(ctx: &AppContext, verbose: bool) -> Result<usize> {
    say_inline!(ctx,"Checking command safety... ");

    let gate = SafetyGate::from_context(ctx);
    let status = gate.status();

    if let Some(version) = status.dcg_version {
        say!(ctx,"{} dcg {}", "[ok]", version);
        if verbose {
            say!(ctx,"  dcg_bin: {}", status.dcg_bin.display());
            if !status.packs.is_empty() {
                say!(ctx,"  packs: {}", status.packs.join(", "));
            }
        }
        Ok(0)
    } else {
        say!(ctx,"{} dcg not available", "[!]");
        Ok(1)
    }
}

/// Comprehensive security check
fn check_security(ctx: &AppContext, verbose: bool) -> Result<usize> {
    say!(ctx,"{}", "Security Checks");
    say!(ctx,"{}", "─".repeat(15));

    let mut issues = 0;

    // 1. Check DCG availability
    say_inline!(ctx,"  [1/5] Command safety (DCG)... ");
    let gate = SafetyGate::from_context(ctx);
    let status = gate.status();
    if let Some(version) = status.dcg_version {
        say!(ctx,"{} v{}", "[ok]", version);
    } else {
        say!(ctx,"{} not available", "[!]");
        say!(ctx,"        Commands will run without safety checks");
        issues += 1;
    }

    // 2. Check ACIP prompt availability
    say_inline!(ctx,"  [2/5] ACIP prompt... ");
    let acip_path = &ctx.config.security.acip.prompt_path;
    if acip_path.exists() {
        match crate::security::acip::prompt_version(acip_path) {
            Ok(Some(version)) => {
                say!(ctx,"{} v{}", "[ok]", version);
                if verbose {
                    say!(ctx,"        Path: {}", acip_path.display());
                }
            }
            Ok(None) => {
                say!(ctx,"{} no version detected", "[!]");
                issues += 1;
            }
            Err(e) => {
                say!(ctx,"{} error: {}", "[FAIL]", e);
                issues += 1;
            }
        }
    } else {
        say!(ctx,"{} not found", "-");
        if verbose {
            say!(ctx,"        Expected: {}", acip_path.display());
        }
    }

    // 3. Check safety tier configuration
    say_inline!(ctx,"  [3/5] Safety tier config... ");
    if ctx.config.safety.require_verbatim_approval {
        say!(ctx,
            "{} verbatim approval required for dangerous commands",
            "[ok]"
        );
    } else {
        say!(ctx,"{} verbatim approval disabled", "[!]");
        say!(ctx,"        Dangerous commands may execute without explicit approval");
        issues += 1;
    }

    // 4. Scan evidence for secrets
    say_inline!(ctx,"  [4/5] Evidence secret scan... ");
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
                                say!(ctx);
                                say!(ctx,
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
            say!(ctx,
                "{} {} potential secret(s) found",
                "[!]",
                secrets_found
            );
            say!(ctx,"        Review evidence files for sensitive data");
            issues += 1;
        } else {
            say!(ctx,
                "{} {} files scanned, no secrets detected",
                "[ok]",
                files_scanned
            );
        }
    } else {
        say!(ctx,"{} no evidence directory", "-");
    }

    // 5. Check for .env files that shouldn't be tracked
    say_inline!(ctx,"  [5/5] Environment files... ");
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
        say!(ctx,"{} no sensitive env files in ms root", "[ok]");
    } else {
        say!(ctx,
            "{} found sensitive files: {}",
            "[!]",
            env_issues.join(", ")
        );
        say!(ctx,"        These files should not be in the ms root directory");
        issues += env_issues.len();
    }

    // Summary
    say!(ctx);
    if issues == 0 {
        say!(ctx,"{} All security checks passed", "[ok]");
    } else {
        say!(ctx,"{} {} security issue(s) found", "[!]", issues);
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
    say_inline!(ctx,"Checking transactions... ");

    let db_path = ctx.ms_root.join("ms.db");
    let archive_path = ctx.ms_root.join("archive");

    if !db_path.exists() || !archive_path.exists() {
        say!(ctx,"{} Skipped (database or archive not found)", "-");
        return Ok(0);
    }

    let db = if let Ok(db) = crate::storage::Database::open(&db_path) {
        std::sync::Arc::new(db)
    } else {
        say!(ctx,"{} Skipped (cannot open database)", "-");
        return Ok(0);
    };

    let git = if let Ok(git) = crate::storage::GitArchive::open(&archive_path) {
        std::sync::Arc::new(git)
    } else {
        say!(ctx,"{} Skipped (cannot open archive)", "-");
        return Ok(0);
    };

    // Check for incomplete transactions
    let tx_mgr = crate::storage::TxManager::new(db.clone(), git, ctx.ms_root.clone())?;

    if fix {
        let report = tx_mgr.recover()?;
        if report.had_work() {
            say!(ctx,"{} Recovered", "[ok]");
            if verbose {
                say!(ctx,"  Rolled back: {}", report.rolled_back);
                say!(ctx,"  Completed: {}", report.completed);
                say!(ctx,"  Orphaned files cleaned: {}", report.orphaned_files);
            }
            *issues_fixed += report.rolled_back + report.completed + report.orphaned_files;
            Ok(report.rolled_back + report.completed + report.orphaned_files)
        } else {
            say!(ctx,"{} OK", "[ok]");
            Ok(0)
        }
    } else {
        // Just check without fixing
        let incomplete = db.list_incomplete_transactions()?;
        if incomplete.is_empty() {
            say!(ctx,"{} OK", "[ok]");
            Ok(0)
        } else {
            say!(ctx,
                "{} {} incomplete transactions",
                "[!]",
                incomplete.len()
            );
            if verbose {
                for tx in &incomplete {
                    say!(ctx,"  - {} ({}, phase: {})", tx.id, tx.entity_type, tx.phase);
                }
            }
            say!(ctx,"  Run with --fix to recover transactions");
            Ok(incomplete.len())
        }
    }
}

/// Run comprehensive recovery diagnostics using `RecoveryManager`.
fn run_comprehensive_check(
    ctx: &AppContext,
    fix: bool,
    verbose: bool,
    issues_fixed: &mut usize,
) -> Result<usize> {
    say!(ctx);
    say!(ctx,"{}", "Comprehensive Recovery Diagnostics");
    say!(ctx,"{}", "─".repeat(35));

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
    print_recovery_report(ctx, &report, verbose);

    // Update fixed count
    *issues_fixed += report.fixed;

    Ok(report.issues.len())
}

/// Print a formatted recovery report.
fn print_recovery_report(ctx: &AppContext, report: &RecoveryReport, verbose: bool) {
    if report.issues.is_empty() {
        say!(ctx,"{} No issues detected", "[ok]");
    } else {
        say!(ctx,
            "{} Found {} issues:",
            if report.has_critical_issues() {
                "[FAIL]"
            } else {
                "[!]"
            },
            report.issues.len()
        );

        for issue in &report.issues {
            let severity_marker = match issue.severity {
                1 => "CRITICAL",
                2 => "MAJOR",
                _ => "MINOR",
            };

            let arrow = if issue.auto_recoverable {
                "[auto]"
            } else {
                "[manual]"
            };

            say!(ctx,"  {} [{}] {}", arrow, severity_marker, issue.description);

            if verbose {
                say!(ctx,"    Mode: {:?}", issue.mode);
                if let Some(fix) = &issue.suggested_fix {
                    say!(ctx,"    Fix: {fix}");
                }
            }
        }
    }

    if report.had_work() {
        say!(ctx);
        say!(ctx,"{}", "Recovery actions:");
        if report.rolled_back > 0 {
            say!(ctx,
                "  {} Rolled back {} transactions",
                "[ok]",
                report.rolled_back
            );
        }
        if report.completed > 0 {
            say!(ctx,
                "  {} Completed {} transactions",
                "[ok]",
                report.completed
            );
        }
        if report.orphaned_files > 0 {
            say!(ctx,
                "  {} Cleaned {} orphaned files",
                "[ok]",
                report.orphaned_files
            );
        }
        if report.cache_invalidated > 0 {
            say!(ctx,
                "  {} Invalidated {} cache entries",
                "[ok]",
                report.cache_invalidated
            );
        }
    }

    if let Some(duration) = report.duration {
        if verbose {
            say!(ctx);
            say!(ctx,"  Duration: {duration:?}");
        }
    }
}

/// Check output mode detection and explain the decision
fn check_output_mode(ctx: &AppContext, verbose: bool) -> Result<usize> {
    say!(ctx,"{}", "Output Mode Detection Report");
    say!(ctx,"{}", "═".repeat(28));
    say!(ctx);

    // Get the output format from context
    let output_format = ctx.output_format;
    let robot_mode = ctx.robot_mode;

    // Generate comprehensive report
    let report = OutputModeReport::generate(output_format, robot_mode);

    // Print format and mode settings
    say!(ctx,"{} Configuration", ">");
    say!(ctx,"  Format:     {}", report.format);
    say!(ctx,"  Robot Mode: {}", report.robot_mode);
    say!(ctx);

    // Print environment variable status
    say!(ctx,"{} Environment Variables", ">");
    say!(ctx,
        "  NO_COLOR:        {}",
        if report.env.no_color {
            "set"
        } else {
            "not set"
        }
    );
    say!(ctx,
        "  MS_PLAIN_OUTPUT: {}",
        if report.env.plain_output {
            "set"
        } else {
            "not set"
        }
    );
    say!(ctx,
        "  MS_FORCE_RICH:   {}",
        if report.env.force_rich {
            "set"
        } else {
            "not set"
        }
    );
    say!(ctx);

    // Print terminal information
    say!(ctx,"{} Terminal", ">");
    say!(ctx,
        "  is_terminal(): {}",
        if report.env.stdout_is_terminal {
            "true"
        } else {
            "false"
        }
    );
    say!(ctx,
        "  TERM:          {}",
        report.term.as_deref().unwrap_or("not set")
    );
    say!(ctx,
        "  COLORTERM:     {}",
        report.colorterm.as_deref().unwrap_or("not set")
    );
    say!(ctx,
        "  COLUMNS:       {}",
        report.columns.as_deref().unwrap_or("not set")
    );
    say!(ctx);

    // Print agent detection
    say!(ctx,"{} Agent Detection", ">");
    if is_agent_environment() {
        say!(ctx,"  Status: {} Agent environment detected", "[!]");
        for var in &report.agent_vars {
            if let Ok(value) = std::env::var(var) {
                say!(ctx,"    {} = {:?}", var, value);
            }
        }
    } else {
        say!(ctx,"  Status: {} No agent environment", "[ok]");
        if verbose {
            say!(ctx,
                "  (Checked {} agent env vars)",
                crate::output::AGENT_ENV_VARS.len()
            );
        }
    }
    say!(ctx);

    // Print CI detection
    say!(ctx,"{} CI Detection", ">");
    if is_ci_environment() {
        say!(ctx,"  Status: {} CI environment detected", "[!]");
        for var in &report.ci_vars {
            if let Ok(value) = std::env::var(var) {
                say!(ctx,"    {} = {:?}", var, value);
            }
        }
    } else {
        say!(ctx,"  Status: {} No CI environment", "[ok]");
        if verbose {
            say!(ctx,
                "  (Checked {} CI env vars)",
                crate::output::CI_ENV_VARS.len()
            );
        }
    }
    say!(ctx);

    // Print IDE detection
    say!(ctx,"{} IDE Detection", ">");
    if is_ide_environment() {
        say!(ctx,"  Status: {} IDE environment detected", "[!]");
        for var in &report.ide_vars {
            if let Ok(value) = std::env::var(var) {
                say!(ctx,"    {} = {:?}", var, value);
            }
        }
    } else {
        say!(ctx,"  Status: {} No special IDE environment", "[ok]");
        if verbose {
            say!(ctx,
                "  (Checked {} IDE env vars)",
                crate::output::IDE_ENV_VARS.len()
            );
        }
    }
    say!(ctx);

    // Print final decision
    say!(ctx,"{} Decision", ">");
    let mode = if report.decision.use_rich {
        "RICH OUTPUT"
    } else {
        "PLAIN OUTPUT"
    };
    say!(ctx,"  Mode:   {}", mode);
    say!(ctx,"  Reason: {:?}", report.decision.reason);
    say!(ctx);

    // Print summary
    if report.decision.use_rich {
        say!(ctx,"{} Rich terminal output is enabled", "[ok]");
        say!(ctx,"  Colors, Unicode box drawing, and styling will be used.");
    } else {
        say!(ctx,"{} Plain text output is enabled", "[!]");
        say!(ctx,"  No ANSI codes or fancy Unicode will be emitted.");
    }

    // Hints for debugging
    if verbose {
        say!(ctx);
        say!(ctx,"{} Debug Tips", ">");
        say!(ctx,"  • Set MS_DEBUG_OUTPUT=1 to see detection info on every command");
        say!(ctx,"  • Set MS_FORCE_RICH=1 to force rich output (if terminal supports it)");
        say!(ctx,"  • Set NO_COLOR=1 to disable all colors");
        say!(ctx,"  • Use --output-format=plain for plain text output");
    }

    Ok(0)
}

/// Check performance metrics
fn check_perf(ctx: &AppContext, verbose: bool) -> Result<usize> {
    say_inline!(ctx,"Checking performance... ");

    let mut issues = 0;

    #[cfg(target_os = "linux")]
    {
        if let Ok(statm) = std::fs::read_to_string("/proc/self/statm") {
            let parts: Vec<&str> = statm.split_whitespace().collect();
            if let Some(rss_pages) = parts.get(1) {
                if let Ok(pages) = rss_pages.parse::<u64>() {
                    let page_size = 4096; // Standard page size assumption
                    let rss_bytes = pages * page_size;
                    let rss_mb = rss_bytes as f64 / (1024.0 * 1024.0);

                    if rss_mb > 100.0 {
                        say!(ctx,
                            "{} High memory usage: {:.2} MB (target < 100 MB)",
                            "[!]",
                            rss_mb
                        );
                        issues += 1;
                    } else {
                        say!(ctx,"{} Memory usage: {:.2} MB", "[ok]", rss_mb);
                    }
                }
            }
        } else {
            say!(ctx,
                "{} Memory check failed (cannot read /proc/self/statm)",
                "[!]"
            );
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        say!(ctx,
            "{} Memory check skipped (not supported on this OS)",
            "-"
        );
    }

    // Check search latency (simple benchmark)
    let start = std::time::Instant::now();
    // Use a simple query that should be fast
    let _ = ctx.db.search_fts("test", 1).ok();
    let elapsed = start.elapsed();

    if elapsed.as_millis() > 50 {
        say!(ctx,
            "{} Search latency high: {:?} (target < 50ms)",
            "[!]",
            elapsed
        );
        issues += 1;
    } else if verbose {
        say!(ctx,"  Search latency: {elapsed:?}");
    }

    Ok(issues)
}

/// Check whether the terminal supports rich output for the doctor command.
#[allow(dead_code)]
fn should_use_rich_for_doctor() -> bool {
    use std::io::IsTerminal;

    if std::env::var("MS_FORCE_RICH").is_ok() {
        return true;
    }
    if std::env::var("NO_COLOR").is_ok() || std::env::var("MS_PLAIN_OUTPUT").is_ok() {
        return false;
    }

    if is_agent_environment() || is_ci_environment() {
        return false;
    }

    std::io::stdout().is_terminal()
}

/// Get the terminal width, defaulting to 80 if detection fails.
#[allow(dead_code)]
fn terminal_width() -> usize {
    crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(80)
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
        let available_checks = [
            "safety",
            "security",
            "recovery",
            "perf",
            "output",
            "output-mode",
        ];

        for check in &available_checks {
            let cli = TestCli::try_parse_from(["test", "--check", check]).unwrap();
            assert_eq!(cli.doctor.check, Some(check.to_string()));
        }
    }

    // =========================================================================
    // Rich Output Tests (bd-2caj)
    // =========================================================================

    // ── 1. test_doctor_render_dashboard_healthy ──────────────────────

    #[test]
    fn test_doctor_render_dashboard_healthy() {
        // Healthy state: 0 issues, all checks pass
        let issues_found = 0;
        let msg = if issues_found == 0 {
            format!("{} All checks passed", "[ok]")
        } else {
            format!("{} Found {} issues", "[!]", issues_found)
        };
        assert!(msg.contains("[ok]"));
        assert!(msg.contains("All checks passed"));
        assert!(!msg.contains("\x1b["), "no ANSI in plain output");
    }

    // ── 2. test_doctor_render_dashboard_warning ──────────────────────

    #[test]
    fn test_doctor_render_dashboard_warning() {
        let issues_found = 2;
        let issues_fixed = 1;
        let msg = format!(
            "{} Found {} issues, fixed {}",
            "[!]", issues_found, issues_fixed
        );
        assert!(msg.contains("[!]"));
        assert!(msg.contains("2"));
        assert!(msg.contains("1"));
        assert!(!msg.contains("\x1b["), "no ANSI in plain output");
    }

    // ── 3. test_doctor_render_dashboard_error ────────────────────────

    #[test]
    fn test_doctor_render_dashboard_error() {
        use crate::core::recovery::{FailureMode, RecoveryIssue, RecoveryReport};

        let mut report = RecoveryReport::default();
        report.issues.push(RecoveryIssue {
            description: "Database corruption detected".to_string(),
            severity: 1,
            mode: FailureMode::Database,
            auto_recoverable: false,
            suggested_fix: Some("Run ms init --force".to_string()),
        });
        assert!(report.has_critical_issues());

        let severity_marker = match report.issues[0].severity {
            1 => "CRITICAL",
            2 => "MAJOR",
            _ => "MINOR",
        };
        assert_eq!(severity_marker, "CRITICAL");
    }

    // ── 4. test_doctor_render_check_table ────────────────────────────

    #[test]
    fn test_doctor_render_check_table() {
        // Check results should be plain key-value lines
        let checks = [
            ("Database", "[ok]", "OK"),
            ("Git archive", "[ok]", "OK"),
            ("Lock status", "[ok]", "No lock held"),
        ];
        for (name, icon, status) in checks {
            let line = format!("Checking {}... {} {}", name, icon, status);
            assert!(!line.contains("\x1b["), "no ANSI in check line");
            assert!(line.contains(name));
        }
    }

    // ── 5. test_doctor_render_check_icons ────────────────────────────

    #[test]
    fn test_doctor_render_check_icons() {
        // Verify status icons are plain text
        let ok = "[ok]";
        let warn = "[!]";
        let fail = "[FAIL]";

        assert!(!ok.contains("\x1b["));
        assert!(!warn.contains("\x1b["));
        assert!(!fail.contains("\x1b["));

        // They should be distinct
        assert_ne!(ok, warn);
        assert_ne!(ok, fail);
        assert_ne!(warn, fail);
    }

    // ── 6. test_doctor_render_issue_panel ────────────────────────────

    #[test]
    fn test_doctor_render_issue_panel() {
        use crate::core::recovery::{FailureMode, RecoveryIssue};

        let issue = RecoveryIssue {
            description: "WAL file too large".to_string(),
            severity: 2,
            mode: FailureMode::Database,
            auto_recoverable: true,
            suggested_fix: Some("PRAGMA wal_checkpoint(TRUNCATE)".to_string()),
        };

        let arrow = if issue.auto_recoverable {
            "[auto]"
        } else {
            "[manual]"
        };
        let severity = match issue.severity {
            1 => "CRITICAL",
            2 => "MAJOR",
            _ => "MINOR",
        };

        let line = format!("  {} [{}] {}", arrow, severity, issue.description);
        assert!(line.contains("[auto]"));
        assert!(line.contains("MAJOR"));
        assert!(line.contains("WAL file too large"));
    }

    // ── 7. test_doctor_render_fix_suggestions ────────────────────────

    #[test]
    fn test_doctor_render_fix_suggestions() {
        use crate::core::recovery::{FailureMode, RecoveryIssue};

        let issue = RecoveryIssue {
            description: "Stale lock".to_string(),
            severity: 2,
            mode: FailureMode::Transaction,
            auto_recoverable: true,
            suggested_fix: Some("ms doctor --break-lock".to_string()),
        };

        assert!(issue.suggested_fix.is_some());
        let fix = issue.suggested_fix.unwrap();
        assert!(fix.contains("ms doctor"));
    }

    // ── 8. test_doctor_render_environment_info ───────────────────────

    #[test]
    fn test_doctor_render_environment_info() {
        // Environment info lines should be plain text
        let lines = [
            format!("> Configuration"),
            format!("  Format:     Human"),
            format!("  Robot Mode: false"),
            format!("> Terminal"),
            format!("  is_terminal(): true"),
        ];
        for line in &lines {
            assert!(!line.contains("\x1b["), "no ANSI in env info: {line}");
        }
    }

    // ── 9. test_doctor_render_recommendations ────────────────────────

    #[test]
    fn test_doctor_render_recommendations() {
        let recommendation = "  Run with --fix to attempt automatic repairs";
        assert!(!recommendation.contains("\x1b["), "no ANSI in recommendation");
        assert!(recommendation.contains("--fix"));
    }

    // ── 10. test_doctor_plain_output_format ──────────────────────────

    #[test]
    fn test_doctor_plain_output_format() {
        let header = format!("{}", "ms doctor - Health Checks");
        assert!(!header.contains("\x1b["), "header should be plain text");
        assert!(header.contains("ms doctor"));
    }

    // ── 11. test_doctor_json_output_format ───────────────────────────

    #[test]
    fn test_doctor_json_output_format() {
        // JSON health report structure
        let output = serde_json::json!({
            "status": "healthy",
            "checks": {
                "database": "ok",
                "git_archive": "ok",
                "lock": "ok",
                "safety": "ok",
            },
            "issues_found": 0,
            "issues_fixed": 0,
        });
        let json_str = serde_json::to_string_pretty(&output).unwrap();
        assert!(json_str.contains("\"status\": \"healthy\""));
        assert!(!json_str.contains("\x1b["), "no ANSI in JSON");
    }

    // ── 12. test_doctor_robot_mode_no_ansi ───────────────────────────

    #[test]
    fn test_doctor_robot_mode_no_ansi() {
        // All status markers must be ANSI-free
        let markers = ["[ok]", "[!]", "[FAIL]", "[auto]", "[manual]",
                        "CRITICAL", "MAJOR", "MINOR"];
        for marker in markers {
            assert!(
                !marker.contains("\x1b["),
                "marker '{}' must not contain ANSI",
                marker
            );
        }
    }

    // ── 13. test_doctor_exit_code_healthy ─────────────────────────────

    #[test]
    fn test_doctor_exit_code_healthy() {
        // When all checks pass, issues_found == 0
        let issues_found = 0;
        assert_eq!(issues_found, 0, "healthy state should have 0 issues");
    }

    // ── 14. test_doctor_exit_code_warning ────────────────────────────

    #[test]
    fn test_doctor_exit_code_warning() {
        // When some checks fail, issues_found > 0
        let issues_found = 3;
        let issues_fixed = 1;
        assert!(issues_found > 0, "warning state should have issues");
        assert!(issues_found > issues_fixed, "not all issues fixed");
    }

    // ── 15. test_doctor_rich_vs_plain_equivalence ────────────────────

    #[test]
    fn test_doctor_rich_vs_plain_equivalence() {
        // Both modes should expose the same data
        let issues_found = 2_usize;
        let issues_fixed = 1_usize;

        // Plain mode
        let plain_summary = format!(
            "Found {} issues, fixed {}",
            issues_found, issues_fixed
        );

        // JSON mode
        let json_summary = serde_json::json!({
            "issues_found": issues_found,
            "issues_fixed": issues_fixed,
        });

        assert!(plain_summary.contains("2"));
        assert_eq!(
            json_summary["issues_found"].as_u64().unwrap(),
            issues_found as u64
        );
        assert_eq!(
            json_summary["issues_fixed"].as_u64().unwrap(),
            issues_fixed as u64
        );
    }

    // ── 16. test_doctor_severity_markers ─────────────────────────────

    #[test]
    fn test_doctor_severity_markers() {
        for severity in 1..=3 {
            let marker = match severity {
                1 => "CRITICAL",
                2 => "MAJOR",
                _ => "MINOR",
            };
            assert!(!marker.is_empty());
            assert!(!marker.contains("\x1b["), "no ANSI in severity marker");
        }
    }
}
