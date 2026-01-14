//! ms init - Initialize ms in current directory or globally

use std::fs;
use std::path::{Path, PathBuf};

use clap::Args;
use colored::Colorize;

use crate::app::AppContext;
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

pub fn run(ctx: &AppContext, args: &InitArgs) -> Result<()> {
    let target = if args.global {
        global_ms_root()?
    } else {
        local_ms_root()?
    };

    // Check if already initialized
    if target.exists() && !args.force {
        if ctx.robot_mode {
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

    if ctx.robot_mode {
        return init_robot(ctx, &target, args);
    }

    init_human(ctx, &target, args)
}

fn init_human(_ctx: &AppContext, target: &Path, args: &InitArgs) -> Result<()> {
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
    create_default_config(target, args.global)?;
    println!("{}", "OK".green());

    println!();
    println!("{} Initialized at {}", "✓".green().bold(), target.display());

    if args.global {
        println!();
        println!("Add skill paths with:");
        println!("  ms config add skill_paths.global ~/my-skills");
    } else {
        println!();
        println!("Add skill paths with:");
        println!("  ms config add skill_paths.project ./skills");
    }

    Ok(())
}

fn init_robot(_ctx: &AppContext, target: &Path, args: &InitArgs) -> Result<()> {
    // Create everything silently
    create_directories(target)?;

    let db_path = target.join("ms.db");
    Database::open(&db_path)?;

    let archive_path = target.join("archive");
    GitArchive::open(&archive_path)?;

    let index_path = target.join("index");
    SearchIndex::open(&index_path)?;

    create_default_config(target, args.global)?;

    println!(
        "{}",
        serde_json::json!({
            "status": "ok",
            "path": target.display().to_string(),
            "db": db_path.display().to_string(),
            "archive": archive_path.display().to_string(),
            "index": index_path.display().to_string(),
        })
    );

    Ok(())
}

fn create_directories(target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    fs::create_dir_all(target.join("tx"))?;
    Ok(())
}

fn create_default_config(target: &Path, global: bool) -> Result<()> {
    let config_path = if global {
        dirs::config_dir()
            .ok_or_else(|| MsError::MissingConfig("config directory not found".to_string()))?
            .join("ms/config.toml")
    } else {
        target.join("config.toml")
    };

    // Create parent directory if needed
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Don't overwrite existing config
    if config_path.exists() {
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
# Maximum results to return
max_results = 20

[robot]
# Default robot mode format
format = "json"
"#
    } else {
        r#"# ms configuration (project-local)

[skill_paths]
# Project-local skill paths
project = ["./skills"]

# Local overrides
local = []

[search]
# Maximum results to return
max_results = 20
"#
    };

    fs::write(&config_path, default_config)?;
    Ok(())
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
