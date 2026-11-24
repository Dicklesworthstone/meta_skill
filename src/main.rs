//! ms - Meta Skill CLI
//!
//! Mine CASS sessions to generate production-quality Claude Code skills.

use std::io::IsTerminal;
use std::process::ExitCode;

use clap::Parser;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

use ms::Result;
use ms::app::AppContext;
use ms::cli::{Cli, Commands};

fn main() -> ExitCode {
    let cli = Cli::parse();
    let initial_format = cli.output_format();
    ms::cli::output::set_active_output_format(initial_format);
    init_tracing(&cli);
    ms::output::detection::maybe_print_debug_output(initial_format, cli.robot);

    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // Commands that already emitted their structured report (including via local
            // `--json` / `--format json` flags when global active format is Human) only
            // need a non-zero exit without duplicate JSON or stderr `Error:` output.
            if matches!(e, ms::MsError::ReportedValidationFailed(_)) {
                return ExitCode::FAILURE;
            }
            let active_fmt = ms::cli::output::active_output_format();
            if active_fmt != ms::cli::OutputFormat::Human {
                // `ms test` already printed its one document, and that says what failed
                if matches!(e, ms::MsError::TestsFailed(_)) {
                    return ExitCode::FAILURE;
                }
                // Structured error output to stdout for non-human modes
                let (code, message) = match &e {
                    ms::MsError::ApprovalRequired(msg) => ("approval_required", msg.clone()),
                    ms::MsError::DestructiveBlocked(msg) => ("destructive_blocked", msg.clone()),
                    _ => ("error", e.to_string()),
                };
                if std::env::var_os("MS_STRUCTURED_ERRORS").is_some() {
                    let _ = ms::cli::output::robot_error(code, &message);
                    let _ = ms::cli::output::robot_error_structured(&e);
                    if ms::cli::output::emit_structured_error(&e).is_ok() {
                        return ExitCode::FAILURE;
                    }
                    let _ = ms::cli::output::emit_error_response(code, &message);
                }
                let error_json = serde_json::json!({
                    "status": "error",
                    "error": true,
                    "code": code,
                    "message": message,
                });
                if ms::cli::output::emit_json(&error_json).is_err() {
                    println!("{}", serde_json::to_string(&error_json).unwrap_or_default());
                }
            } else {
                eprintln!("Error: {e}");
            }
            ExitCode::FAILURE
        }
    }
}

fn run(cli: &Cli) -> Result<()> {
    if let Commands::Init(args) = &cli.command {
        let is_robot = cli.output_format() != ms::cli::OutputFormat::Human;
        return ms::cli::commands::init::run_without_context(is_robot, args);
    }
    let ctx = AppContext::from_cli(cli)?;
    ms::cli::commands::run(&ctx, &cli.command)
}

fn default_tracing_filter(verbose: u8) -> &'static str {
    // Only suppress the per-open startup WARN emitted by `fsqlite_core::connection`
    // ("WAL-FEC requires a caller-owned native runtime; primary WAL durability is unchanged")
    // below `-vv`, keeping real warnings from `fsqlite_pager` and other `fsqlite_core`
    // modules visible at default verbosity.
    match verbose {
        0 => "warn,ms=info,fsqlite_core::connection=error",
        1 => "info,ms=debug,fsqlite_core::connection=error",
        2 => "debug,ms=trace,fsqlite_core::connection=warn",
        _ => "trace",
    }
}

fn init_tracing(cli: &Cli) {
    if cli.quiet {
        return;
    }

    let filter = default_tracing_filter(cli.verbose);

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter));

    if cli.robot || cli.output_format().is_machine_readable() {
        // JSON logging for robot/machine-readable mode
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt::layer().json().with_writer(std::io::stderr))
            .init();
    } else {
        // Human-readable logging. Only emit ANSI styling when stderr is an
        // interactive terminal and neither NO_COLOR nor an AI-agent
        // environment (CLAUDE_CODE, CURSOR_AI, ...) demands plain output.
        let ansi = std::io::stderr().is_terminal()
            && std::env::var_os("NO_COLOR").is_none()
            && !ms::output::is_agent_environment();
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt::layer().with_ansi(ansi).with_writer(std::io::stderr))
            .init();
    }
}

#[cfg(test)]
mod tests {
    use super::default_tracing_filter;

    #[test]
    fn default_tracing_filter_keeps_fsqlite_pager_and_core_submodules_visible() {
        for level in [0u8, 1] {
            let filter = default_tracing_filter(level);
            assert!(
                filter.contains("fsqlite_core::connection=error"),
                "level {level} should silence routine connection WAL-FEC startup warning: {filter}"
            );
            assert!(
                !filter.contains("fsqlite_pager=error"),
                "level {level} must not blanket-hide fsqlite_pager warnings: {filter}"
            );
            assert!(
                !filter.contains("fsqlite_core=error"),
                "level {level} must not blanket-hide non-connection fsqlite_core warnings: {filter}"
            );
        }
        assert!(default_tracing_filter(2).contains("fsqlite_core::connection=warn"));
        assert_eq!(default_tracing_filter(3), "trace");
    }
}
