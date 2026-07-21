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
    init_tracing(&cli);

    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            if cli.robot {
                // Robot mode: JSON error output to stdout
                let (code, message) = match &e {
                    ms::MsError::ApprovalRequired(msg) => ("approval_required", msg.clone()),
                    ms::MsError::DestructiveBlocked(msg) => ("destructive_blocked", msg.clone()),
                    _ => ("error", e.to_string()),
                };
                let error_json = serde_json::json!({
                    "error": true,
                    "code": code,
                    "message": message,
                });
                println!("{}", serde_json::to_string(&error_json).unwrap_or_default());
            } else {
                eprintln!("Error: {e}");
            }
            ExitCode::FAILURE
        }
    }
}

fn run(cli: &Cli) -> Result<()> {
    if let Commands::Init(args) = &cli.command {
        return ms::cli::commands::init::run_without_context(cli.robot, args);
    }
    let ctx = AppContext::from_cli(cli)?;
    ms::cli::commands::run(&ctx, &cli.command)
}

fn init_tracing(cli: &Cli) {
    if cli.quiet {
        return;
    }

    let filter = match cli.verbose {
        0 => "warn,ms=info",
        1 => "info,ms=debug",
        2 => "debug,ms=trace",
        _ => "trace",
    };

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter));

    if cli.robot {
        // JSON logging for robot mode
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
