//! CLI module - Command-line interface definitions and handlers
//!
//! Uses clap v4 with derive macros for argument parsing.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

pub use output::OutputFormat;

pub mod colors;
pub mod commands;
pub mod formatters;
pub mod output;
pub mod progress;

/// Meta Skill - Mine CASS sessions to generate Claude Code skills
#[derive(Parser, Debug)]
#[command(name = "ms")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    /// Enable robot mode (JSON output to stdout, logs to stderr)
    /// Deprecated: use --output-format=json instead
    #[arg(long, global = true)]
    pub robot: bool,

    /// Output format (human, json, jsonl, plain, tsv)
    #[arg(long, short = 'O', global = true, value_enum)]
    pub output_format: Option<OutputFormat>,

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

impl Cli {
    /// Get the effective output format, considering --robot flag for backward compatibility
    #[must_use]
    pub fn output_format(&self) -> OutputFormat {
        OutputFormat::from_args(self.robot, self.output_format)
    }
}

/// Available commands
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Mine and manage anti-patterns from CASS sessions
    Antipatterns(commands::antipatterns::AntiPatternsArgs),

    /// Initialize ms in current directory or globally
    Init(commands::init::InitArgs),

    /// Import skills from unstructured text documents
    Import(commands::import::ImportArgs),

    /// Index skills from configured paths
    Index(commands::index::IndexArgs),

    /// Search for skills
    Search(commands::search::SearchArgs),

    /// Load a skill with progressive disclosure
    Load(commands::load::LoadArgs),

    /// Install a bundle from URL or path (alias for bundle install)
    Install(commands::install::InstallArgs),

    /// Get context-aware skill suggestions
    Suggest(commands::suggest::SuggestArgs),

    /// Show skill details
    Show(commands::show::ShowArgs),

    /// List all indexed skills
    List(commands::list::ListArgs),

    /// Interactively browse and search skills (TUI)
    Browse(commands::browse::BrowseArgs),

    /// Check Agent Mail inbox
    Inbox(commands::inbox::InboxArgs),

    /// Lint skill specifications for issues
    Lint(commands::lint::LintArgs),

    /// Edit a skill (structured round-trip)
    Edit(commands::edit::EditArgs),

    /// Format skill files
    Fmt(commands::fmt::FmtArgs),

    /// Semantic diff between skills
    Diff(commands::diff::DiffArgs),

    /// Find and manage duplicate skills
    Dedup(commands::dedup::DedupArgs),

    /// Manage skill aliases
    Alias(commands::alias::AliasArgs),

    /// Check environment requirements
    Requirements(commands::requirements::RequirementsArgs),

    /// Record and inspect skill feedback
    Feedback(commands::feedback::FeedbackArgs),

    /// Manage favorite skills
    Favorite(commands::favorite::FavoriteArgs),

    /// Hide skills from suggestions
    Hide(commands::hide::HideArgs),

    /// Record implicit success/failure outcomes
    Outcome(commands::outcome::OutcomeArgs),

    /// Personalize skills to user coding style
    Personalize(commands::personalize::PersonalizeArgs),

    /// Manage skill preferences (favorites/hidden)
    Preferences(commands::preferences::PreferencesArgs),

    /// Manage skill experiments
    Experiment(commands::experiment::ExperimentArgs),

    /// Build skills from CASS sessions
    Build(commands::build::BuildArgs),

    /// Manage skill bundles
    Bundle(commands::bundle::BundleArgs),

    /// Synchronize skills across machines
    Sync(commands::sync::SyncArgs),

    /// Manage sync remotes
    Remote(commands::remote::RemoteArgs),

    /// Show or update machine identity
    Machine(commands::machine::MachineArgs),

    /// Manage meta-skills (composed slice bundles)
    Meta(commands::meta::MetaArgs),

    /// Skill graph analysis (bv integration)
    Graph(commands::graph::GraphArgs),

    /// Cross-project learning and coverage analysis
    CrossProject(commands::cross_project::CrossProjectArgs),

    /// Manage sync conflicts
    Conflicts(commands::conflicts::ConflictsArgs),

    /// Manage pack contracts
    Contract(commands::contract::ContractArgs),

    /// Migrate skills to latest spec format
    Migrate(commands::migrate::MigrateArgs),

    /// Check for and apply updates
    Update(commands::update::UpdateArgs),

    /// CM (cass-memory) integration
    Cm(commands::cm::CmArgs),

    /// Suggestion bandit controls
    Bandit(commands::bandit::BanditArgs),

    /// Backup and restore ms state
    Backup(commands::backup::BackupArgs),

    /// Health checks and repairs
    Doctor(commands::doctor::DoctorArgs),

    /// Pre-commit hook: run UBS on staged files
    PreCommit(commands::pre_commit::PreCommitArgs),

    /// Prune tombstoned/outdated data
    Prune(commands::prune::PruneArgs),

    /// Manage configuration
    Config(commands::config::ConfigArgs),

    /// Security and prompt-injection defenses
    Security(commands::security::SecurityArgs),

    /// Setup ms integration for AI coding agents
    Setup(commands::setup::SetupArgs),

    /// Shell integration hooks
    Shell(commands::shell::ShellArgs),

    /// Command safety (DCG) logs and status
    Safety(commands::safety::SafetyArgs),

    /// Validate skill specs
    Validate(commands::validate::ValidateArgs),

    /// Run skill tests
    Test(commands::test::TestArgs),

    /// Simulate a skill in a sandbox
    Simulate(commands::simulate::SimulateArgs),

    /// Compute skill quality scores
    Quality(commands::quality::QualityArgs),

    /// View and manage skill provenance evidence
    Evidence(commands::evidence::EvidenceArgs),

    /// Use curated skill templates
    Template(commands::template::TemplateArgs),

    /// Unhide a previously hidden skill
    Unhide(commands::unhide::UnhideArgs),

    /// Run as MCP (Model Context Protocol) server
    Mcp(commands::mcp::McpArgs),

    /// Test embedding backends
    Embed(commands::embed::EmbedArgs),
}
