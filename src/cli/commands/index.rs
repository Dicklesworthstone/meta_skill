//! ms index - Index skills from configured paths

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Args;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use walkdir::WalkDir;

use crate::app::AppContext;
use crate::core::spec_lens::parse_markdown;
use crate::error::{MsError, Result};
use crate::storage::tx::GlobalLock;
use crate::storage::TxManager;

#[derive(Args, Debug)]
pub struct IndexArgs {
    /// Paths to index (overrides config)
    #[arg(value_name = "PATH")]
    pub paths: Vec<String>,

    /// Watch for changes and re-index automatically
    #[arg(long)]
    pub watch: bool,

    /// Force full re-index
    #[arg(long, short)]
    pub force: bool,

    /// Index all configured paths
    #[arg(long)]
    pub all: bool,
}

pub fn run(ctx: &AppContext, args: &IndexArgs) -> Result<()> {
    // Acquire global lock for indexing (exclusive write operation)
    let lock_result = GlobalLock::acquire_timeout(&ctx.ms_root, Duration::from_secs(30))?;
    let _lock = lock_result.ok_or_else(|| {
        MsError::TransactionFailed(
            "Could not acquire lock for indexing. Another process may be indexing.".to_string(),
        )
    })?;

    if args.watch {
        return Err(MsError::Config(
            "Watch mode not yet implemented. Use a file watcher with 'ms index' instead."
                .to_string(),
        ));
    }

    // Collect paths to index
    let paths = collect_index_paths(ctx, args)?;

    if paths.is_empty() {
        if ctx.robot_mode {
            println!(
                "{}",
                serde_json::json!({
                    "status": "ok",
                    "message": "No paths to index",
                    "indexed": 0
                })
            );
        } else {
            println!("{}", "No skill paths configured".yellow());
            println!();
            println!("Add paths with:");
            println!("  ms config add skill_paths.project ./skills");
        }
        return Ok(());
    }

    if ctx.robot_mode {
        index_robot(ctx, &paths, args)
    } else {
        index_human(ctx, &paths, args)
    }
}

fn collect_index_paths(ctx: &AppContext, args: &IndexArgs) -> Result<Vec<PathBuf>> {
    if !args.paths.is_empty() {
        // Use explicitly provided paths
        return Ok(args
            .paths
            .iter()
            .map(|p| expand_path(p))
            .collect());
    }

    // Use configured paths
    let mut paths = Vec::new();

    for p in &ctx.config.skill_paths.global {
        paths.push(expand_path(p));
    }
    for p in &ctx.config.skill_paths.project {
        paths.push(expand_path(p));
    }
    for p in &ctx.config.skill_paths.community {
        paths.push(expand_path(p));
    }
    for p in &ctx.config.skill_paths.local {
        paths.push(expand_path(p));
    }

    Ok(paths)
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

fn index_human(ctx: &AppContext, paths: &[PathBuf], args: &IndexArgs) -> Result<()> {
    println!("{}", "Indexing skills...".bold());
    println!();

    let start = Instant::now();
    let mut indexed = 0;
    let mut errors = 0;

    // First pass: discover all SKILL.md files
    let skill_files = discover_skill_files(paths);

    if skill_files.is_empty() {
        println!("{}", "No SKILL.md files found".yellow());
        return Ok(());
    }

    // Progress bar
    let pb = ProgressBar::new(skill_files.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("#>-"),
    );

    // Create transaction manager
    let tx_mgr = TxManager::new(
        Arc::clone(&ctx.db),
        Arc::clone(&ctx.git),
        ctx.ms_root.clone(),
    )?;

    for path in &skill_files {
        pb.set_message(format!(
            "{}",
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
        ));

        match index_skill_file(ctx, &tx_mgr, path, args.force) {
            Ok(_) => indexed += 1,
            Err(e) => {
                errors += 1;
                pb.println(format!(
                    "{} {} - {}",
                    "✗".red(),
                    path.display(),
                    e
                ));
            }
        }

        pb.inc(1);
    }

    pb.finish_and_clear();

    let elapsed = start.elapsed();

    println!();
    println!(
        "{} Indexed {} skills in {:.2}s ({} errors)",
        "✓".green().bold(),
        indexed,
        elapsed.as_secs_f64(),
        errors
    );

    if errors > 0 {
        println!();
        println!(
            "{} {} skills failed to index",
            "!".yellow(),
            errors
        );
    }

    Ok(())
}

fn index_robot(ctx: &AppContext, paths: &[PathBuf], args: &IndexArgs) -> Result<()> {
    let start = Instant::now();
    let mut indexed = 0;
    let mut errors: Vec<serde_json::Value> = Vec::new();

    // Discover skill files
    let skill_files = discover_skill_files(paths);

    // Create transaction manager
    let tx_mgr = TxManager::new(
        Arc::clone(&ctx.db),
        Arc::clone(&ctx.git),
        ctx.ms_root.clone(),
    )?;

    for path in &skill_files {
        match index_skill_file(ctx, &tx_mgr, path, args.force) {
            Ok(_) => indexed += 1,
            Err(e) => {
                errors.push(serde_json::json!({
                    "path": path.display().to_string(),
                    "error": e.to_string()
                }));
            }
        }
    }

    let elapsed = start.elapsed();

    println!(
        "{}",
        serde_json::json!({
            "status": if errors.is_empty() { "ok" } else { "partial" },
            "indexed": indexed,
            "errors": errors,
            "elapsed_ms": elapsed.as_millis() as u64,
        })
    );

    Ok(())
}

fn discover_skill_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut skill_files = Vec::new();

    for path in paths {
        if !path.exists() {
            continue;
        }

        for entry in WalkDir::new(path)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() && entry.file_name() == "SKILL.md" {
                skill_files.push(entry.path().to_path_buf());
            }
        }
    }

    skill_files
}

fn index_skill_file(
    ctx: &AppContext,
    tx_mgr: &TxManager,
    path: &PathBuf,
    force: bool,
) -> Result<()> {
    // Read the file
    let content = std::fs::read_to_string(path)?;

    // Parse the skill spec
    let spec = parse_markdown(&content)
        .map_err(|e| MsError::InvalidSkill(format!("{}: {}", path.display(), e)))?;

    // Check if already indexed (unless force)
    if !force {
        if let Ok(Some(existing)) = ctx.db.get_skill(&spec.metadata.id) {
            // Check content hash to skip unchanged skills
            let new_hash = compute_content_hash(&content);
            if existing.content_hash == new_hash {
                return Ok(()); // Skip unchanged
            }
        }
    }

    // Write using 2PC transaction manager
    tx_mgr.write_skill(&spec)?;

    Ok(())
}

fn compute_content_hash(content: &str) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}
