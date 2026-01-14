//! ms init - Initialize ms in current directory or globally

use std::fs;
use std::path::{Path, PathBuf};

use clap::Args;
use colored::Colorize;

use crate::error::{MsError, Result};
use crate::search::SearchIndex;
use crate::storage::{Database, GitArchive};

#[derive(Args, Debug)]
pub struct InitArgs {
    /// Initialize globally (~/.local/share/ms) instead of locally (.ms/)
    #[arg(long)]
    pub global: bool,

    /// Force initialization even if already initialized
    #[arg(long, short)]
    pub force: bool,
}

pub fn run(ctx: &crate::app::AppContext, args: &InitArgs) -> Result<()> {
    run_with_robot(ctx.robot_mode, args)
}

pub fn run_without_context(robot_mode: bool, args: &InitArgs) -> Result<()> {
    run_with_robot(robot_mode, args)
}

fn run_with_robot(robot_mode: bool, args: &InitArgs) -> Result<()> {
    let target = if args.global {
        global_ms_root()?
    } else {
        local_ms_root()?
    };
    let config_path = config_path_for(&target, args.global)?;

    // Check if already initialized
    if args.global {
        if config_path.exists() && !args.force {
            if robot_mode {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "error",
                        "message": "Already initialized",
                        "config": config_path.display().to_string()
                    })
                );
            } else {
                println!(
                    "{} Already initialized at {}",
                    "!".yellow(),
                    config_path.display()
                );
                println!("  Use --force to reinitialize");
            }
            return Ok(());
        }
    } else if target.exists() && !args.force {
        if robot_mode {
            println!(
                "{}",
                serde_json::json!({
                    "status": "error",
                    "message": "Already initialized",
                    "path": target.display().to_string()
                })
            );
        } else {
            println!(
                "{} Already initialized at {}",
                "!".yellow(),
                target.display()
            );
            println!("  Use --force to reinitialize");
        }
        return Ok(());
    }

    if args.global {
        return if robot_mode {
            init_robot_global(&config_path, args)
        } else {
            init_human_global(&config_path, args)
        };
    }

    if robot_mode {
        return init_robot(&target, args);
    }

    init_human(&target, args)
}

fn init_human_global(config_path: &Path, args: &InitArgs) -> Result<()> {
    println!("{}", "Initializing global ms configuration...".bold());
    println!();

    print!("Creating default configuration... ");
    create_default_config(config_path, true, args.force)?;
    println!("{}", "OK".green());

    println!();
    println!("{} Initialized at {}", "✓".green().bold(), config_path.display());
    println!();
    println!("Add skill paths with:");
    println!("  ms config add skill_paths.global ~/my-skills");

    Ok(())
}

fn init_robot_global(config_path: &Path, args: &InitArgs) -> Result<()> {
    create_default_config(config_path, true, args.force)?;
    println!(
        "{}",
        serde_json::json!({
            "status": "ok",
            "config": config_path.display().to_string(),
        })
    );
    Ok(())
}

fn init_human(target: &Path, args: &InitArgs) -> Result<()> {
    println!("{}", "Initializing ms...".bold());
    println!();

    // Create directory structure
    print!("Creating directory structure... ");
    create_directories(target)?;
    println!("{}", "OK".green());

    // Create SQLite database
    print!("Initializing database... ");
    let db_path = target.join("ms.db");
    Database::open(&db_path)?;
    println!("{}", "OK".green());

    // Create Git archive
    print!("Initializing Git archive... ");
    let archive_path = target.join("archive");
    GitArchive::open(&archive_path)?;
    println!("{}", "OK".green());

    // Create search index
    print!("Initializing search index... ");
    let index_path = target.join("index");
    SearchIndex::open(&index_path)?;
    println!("{}", "OK".green());

    // Create default config
    print!("Creating default configuration... ");
    let config_path = config_path_for(target, args.global)?;
    create_default_config(&config_path, args.global, args.force)?;
    println!("{}", "OK".green());

    println!();
    println!("{} Initialized at {}", "✓".green().bold(), target.display());

    println!();
    println!("Add skill paths with:");
    println!("  ms config add skill_paths.project ./skills");

    Ok(())
}

fn init_robot(target: &Path, args: &InitArgs) -> Result<()> {
    // Create everything silently
    create_directories(target)?;

    let db_path = target.join("ms.db");
    Database::open(&db_path)?;

    let archive_path = target.join("archive");
    GitArchive::open(&archive_path)?;

    let index_path = target.join("index");
    SearchIndex::open(&index_path)?;

    let config_path = config_path_for(target, args.global)?;
    create_default_config(&config_path, args.global, args.force)?;

    println!(
        "{}",
        serde_json::json!({
            "status": "ok",
            "path": target.display().to_string(),
            "db": db_path.display().to_string(),
            "archive": archive_path.display().to_string(),
            "index": index_path.display().to_string(),
            "config": config_path.display().to_string(),
        })
    );

    Ok(())
}

fn create_directories(target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    fs::create_dir_all(target.join("tx"))?;
    Ok(())
}

fn create_default_config(config_path: &Path, global: bool, force: bool) -> Result<()> {
    // Create parent directory if needed
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Don't overwrite existing config
    if config_path.exists() && !force {
        return Ok(());
    }

    let default_config = if global {
        r#"# ms configuration

[skill_paths]
# Global skill repositories
global = []

# Community skill repositories
community = []

[search]
# Embedding backend configuration
use_embeddings = true
embedding_backend = "hash"
embedding_dims = 384
bm25_weight = 0.5
semantic_weight = 0.5

[robot]
# Default robot mode format
format = "json"
include_metadata = true

[safety]
# Destructive Command Guard configuration
dcg_bin = "dcg"
dcg_packs = []
dcg_explain_format = "json"
require_verbatim_approval = true
"#
    } else {
        r#"# ms configuration (project-local)

[skill_paths]
# Project-local skill paths
project = ["./skills"]

# Local overrides
local = []

[search]
# Embedding backend configuration
use_embeddings = true
embedding_backend = "hash"
embedding_dims = 384
bm25_weight = 0.5
semantic_weight = 0.5

[safety]
# Destructive Command Guard configuration
dcg_bin = "dcg"
dcg_packs = []
dcg_explain_format = "json"
require_verbatim_approval = true
"#
    };

    fs::write(&config_path, default_config)?;
    Ok(())
}

fn config_path_for(target: &Path, global: bool) -> Result<PathBuf> {
    if global {
        return dirs::config_dir()
            .ok_or_else(|| MsError::MissingConfig("config directory not found".to_string()))
            .map(|dir| dir.join("ms/config.toml"));
    }
    Ok(target.join("config.toml"))
}

fn global_ms_root() -> Result<PathBuf> {
    let data_dir = dirs::data_dir()
        .ok_or_else(|| MsError::MissingConfig("data directory not found".to_string()))?;
    Ok(data_dir.join("ms"))
}

fn local_ms_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    Ok(cwd.join(".ms"))
}
