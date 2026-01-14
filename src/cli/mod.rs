//! CLI module - Command-line interface definitions and handlers
//!
//! Uses clap v4 with derive macros for argument parsing.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

pub mod commands;
pub mod output;

/// Meta Skill - Mine CASS sessions to generate Claude Code skills
#[derive(Parser, Debug)]
#[command(name = "ms")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    /// Enable robot mode (JSON output to stdout, logs to stderr)
    #[arg(long, global = true)]
    pub robot: bool,

    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Suppress all output except errors
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Config file path (default: ~/.config/ms/config.toml)
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

/// Available commands
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Initialize ms in current directory or globally
    Init(commands::init::InitArgs),

    /// Index skills from configured paths
    Index(commands::index::IndexArgs),

    /// Search for skills
    Search(commands::search::SearchArgs),

    /// Load a skill with progressive disclosure
    Load(commands::load::LoadArgs),

    /// Get context-aware skill suggestions
    Suggest(commands::suggest::SuggestArgs),

    /// Show skill details
    Show(commands::show::ShowArgs),

    /// List all indexed skills
    List(commands::list::ListArgs),

    /// Edit a skill (structured round-trip)
    Edit(commands::edit::EditArgs),

    /// Format skill files
    Fmt(commands::fmt::FmtArgs),

    /// Semantic diff between skills
    Diff(commands::diff::DiffArgs),

    /// Manage skill aliases
    Alias(commands::alias::AliasArgs),

    /// Check environment requirements
    Requirements(commands::requirements::RequirementsArgs),

    /// Build skills from CASS sessions
    Build(commands::build::BuildArgs),

    /// Manage skill bundles
    Bundle(commands::bundle::BundleArgs),

    /// Check for and apply updates
    Update(commands::update::UpdateArgs),

    /// Health checks and repairs
    Doctor(commands::doctor::DoctorArgs),

    /// Prune tombstoned/outdated data
    Prune(commands::prune::PruneArgs),

    /// Manage configuration
    Config(commands::config::ConfigArgs),

    /// Security and prompt-injection defenses
    Security(commands::security::SecurityArgs),

    /// Run skill tests
    Test(commands::test::TestArgs),
}
