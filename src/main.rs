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

    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            let active_fmt = ms::cli::output::active_output_format();
            if active_fmt != ms::cli::OutputFormat::Human {
                // Structured error output to stdout for non-human modes
                let (code, message) = match &e {
                    ms::MsError::ApprovalRequired(msg) => ("approval_required", msg.clone()),
                    ms::MsError::DestructiveBlocked(msg) => ("destructive_blocked", msg.clone()),
                    _ => ("error", e.to_string()),
                };
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

fn init_tracing(cli: &Cli) {
    if cli.quiet {
        return;
    }

    let filter = match cli.verbose {
        0 => "warn,ms=info,fsqlite_core=error,fsqlite_pager=error",
        1 => "info,ms=debug,fsqlite_core=error,fsqlite_pager=error",
        2 => "debug,ms=trace,fsqlite_core=warn,fsqlite_pager=warn",
        _ => "trace",
    };

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
