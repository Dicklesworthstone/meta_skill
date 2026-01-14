//! CLI command implementations
//!
//! Each subcommand has its own module with:
//! - Args struct for command-line arguments
//! - run() function to execute the command

use std::collections::HashSet;
use std::path::PathBuf;

use walkdir::WalkDir;

use crate::app::AppContext;
use crate::cli::Commands;
use crate::error::Result;

pub mod alias;
pub mod build;
pub mod bundle;
pub mod config;
pub mod diff;
pub mod doctor;
pub mod edit;
pub mod fmt;
pub mod index;
pub mod init;
pub mod list;
pub mod load;
pub mod pre_commit;
pub mod prune;
pub mod quality;
pub mod requirements;
pub mod search;
pub mod security;
pub mod show;
pub mod suggest;
pub mod test;
pub mod update;
pub mod validate;

/// Dispatch a command to its handler
pub fn run(ctx: &AppContext, command: &Commands) -> Result<()> {
    match command {
        Commands::Init(args) => init::run(ctx, args),
        Commands::Index(args) => index::run(ctx, args),
        Commands::Search(args) => search::run(ctx, args),
        Commands::Load(args) => load::run(ctx, args),
        Commands::Suggest(args) => suggest::run(ctx, args),
        Commands::Show(args) => show::run(ctx, args),
        Commands::List(args) => list::run(ctx, args),
        Commands::Edit(args) => edit::run(ctx, args),
        Commands::Fmt(args) => fmt::run(ctx, args),
        Commands::Diff(args) => diff::run(ctx, args),
        Commands::Alias(args) => alias::run(ctx, args),
        Commands::Requirements(args) => requirements::run(ctx, args),
        Commands::Build(args) => build::run(ctx, args),
        Commands::Bundle(args) => bundle::run(ctx, args),
        Commands::Update(args) => update::run(ctx, args),
        Commands::Doctor(args) => doctor::run(ctx, args),
        Commands::PreCommit(args) => pre_commit::run(ctx, args),
        Commands::Prune(args) => prune::run(ctx, args),
        Commands::Config(args) => config::run(ctx, args),
        Commands::Security(args) => security::run(ctx, args),
        Commands::Validate(args) => validate::run(ctx, args),
        Commands::Test(args) => test::run(ctx, args),
        Commands::Quality(args) => quality::run(ctx, args),
    }
}

pub(crate) fn discover_skill_markdowns(ctx: &AppContext) -> Result<Vec<PathBuf>> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for root in skill_roots(ctx) {
        if !root.exists() {
            continue;
        }
        for entry in WalkDir::new(root).follow_links(true) {
            let entry = entry.map_err(|err| {
                crate::error::MsError::Config(format!("walk skill paths: {err}"))
            })?;
            if !entry.file_type().is_file() {
                continue;
            }
            if entry.file_name() == "SKILL.md" {
                let path = entry.path().to_path_buf();
                if seen.insert(path.clone()) {
                    out.push(path);
                }
            }
        }
    }
    Ok(out)
}

pub(crate) fn resolve_skill_markdown(ctx: &AppContext, input: &str) -> Result<PathBuf> {
    let direct = expand_path(input);
    if direct.exists() {
        if direct.is_file() {
            return Ok(direct);
        }
        let candidate = direct.join("SKILL.md");
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    for root in skill_roots(ctx) {
        let candidate = root.join(input);
        if candidate.is_file() {
            return Ok(candidate);
        }
        let skill_md = candidate.join("SKILL.md");
        if skill_md.exists() {
            return Ok(skill_md);
        }
    }

    Err(crate::error::MsError::SkillNotFound(format!(
        "skill not found: {input}"
    )))
}

fn skill_roots(ctx: &AppContext) -> Vec<PathBuf> {
    let paths = ctx
        .config
        .skill_paths
        .global
        .iter()
        .chain(ctx.config.skill_paths.project.iter())
        .chain(ctx.config.skill_paths.community.iter())
        .chain(ctx.config.skill_paths.local.iter());
    paths.map(|path| expand_path(path)).collect()
}

fn expand_path(input: &str) -> PathBuf {
    if let Some(stripped) = input.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    if input == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    PathBuf::from(input)
}
