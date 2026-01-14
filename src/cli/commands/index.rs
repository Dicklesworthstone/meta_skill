//! ms index - Index skills from configured paths

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Args;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use walkdir::WalkDir;

use crate::app::AppContext;
use crate::core::{spec_lens::parse_markdown, SkillLayer};
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

struct SkillRoot {
    path: PathBuf,
    layer: SkillLayer,
}

struct DiscoveredSkill {
    path: PathBuf,
    layer: SkillLayer,
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
    let roots = collect_index_paths(ctx, args)?;

    if roots.is_empty() {
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
        index_robot(ctx, &roots, args)
    } else {
        index_human(ctx, &roots, args)
    }
}

fn collect_index_paths(ctx: &AppContext, args: &IndexArgs) -> Result<Vec<SkillRoot>> {
    if !args.paths.is_empty() {
        // Use explicitly provided paths
        return Ok(args
            .paths
            .iter()
            .map(|p| SkillRoot {
                path: expand_path(p),
                layer: SkillLayer::Project,
            })
            .collect());
    }

    // Use configured paths
    let mut roots: Vec<SkillRoot> = Vec::new();

    // Map configured path buckets to canonical layers.
    for p in &ctx.config.skill_paths.global {
        roots.push(SkillRoot {
            path: expand_path(p),
            layer: SkillLayer::Org,
        });
    }
    for p in &ctx.config.skill_paths.project {
        roots.push(SkillRoot {
            path: expand_path(p),
            layer: SkillLayer::Project,
        });
    }
    for p in &ctx.config.skill_paths.community {
        roots.push(SkillRoot {
            path: expand_path(p),
            layer: SkillLayer::Base,
        });
    }
    for p in &ctx.config.skill_paths.local {
        roots.push(SkillRoot {
            path: expand_path(p),
            layer: SkillLayer::User,
        });
    }

    roots.sort_by_key(|root| root.layer);
    Ok(roots)
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

fn index_human(ctx: &AppContext, roots: &[SkillRoot], args: &IndexArgs) -> Result<()> {
    println!("{}", "Indexing skills...".bold());
    println!();

    let start = Instant::now();
    let mut indexed = 0;
    let mut errors = 0;

    // First pass: discover all SKILL.md files
    let skill_files = discover_skill_files(roots);

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

    for skill in &skill_files {
        pb.set_message(format!(
            "{}",
            skill
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        ));

        match index_skill_file(ctx, &tx_mgr, skill, args.force) {
            Ok(_) => indexed += 1,
            Err(e) => {
                errors += 1;
                pb.println(format!(
                    "{} {} - {}",
                    "✗".red(),
                    skill.path.display(),
                    e
                ));
            }
        }

        pb.inc(1);
    }

    pb.finish_and_clear();

    // Commit Tantivy index
    ctx.search.commit()?;

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

fn index_robot(ctx: &AppContext, roots: &[SkillRoot], args: &IndexArgs) -> Result<()> {
    let start = Instant::now();
    let mut indexed = 0;
    let mut errors: Vec<serde_json::Value> = Vec::new();

    // Discover skill files
    let skill_files = discover_skill_files(roots);

    // Create transaction manager
    let tx_mgr = TxManager::new(
        Arc::clone(&ctx.db),
        Arc::clone(&ctx.git),
        ctx.ms_root.clone(),
    )?;

    for skill in &skill_files {
        match index_skill_file(ctx, &tx_mgr, skill, args.force) {
            Ok(_) => indexed += 1,
            Err(e) => {
                errors.push(serde_json::json!({
                    "path": skill.path.display().to_string(),
                    "error": e.to_string()
                }));
            }
        }
    }

    // Commit Tantivy index
    ctx.search.commit()?;

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

fn discover_skill_files(roots: &[SkillRoot]) -> Vec<DiscoveredSkill> {
    let mut skill_files = Vec::new();

    for root in roots {
        if !root.path.exists() {
            continue;
        }

        for entry in WalkDir::new(&root.path)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() && entry.file_name() == "SKILL.md" {
                skill_files.push(DiscoveredSkill {
                    path: entry.path().to_path_buf(),
                    layer: root.layer,
                });
            }
        }
    }

    skill_files
}

fn index_skill_file(
    ctx: &AppContext,
    tx_mgr: &TxManager,
    skill: &DiscoveredSkill,
    force: bool,
) -> Result<()> {
    // Read the file
    let content = std::fs::read_to_string(&skill.path)?;

    // Parse the skill spec
    let spec = parse_markdown(&content)
        .map_err(|e| MsError::InvalidSkill(format!("{}: {}", skill.path.display(), e)))?;

    if spec.metadata.id.trim().is_empty() {
        return Err(MsError::InvalidSkill(format!(
            "{}: missing skill id",
            skill.path.display()
        )));
    }

    // Check if already indexed (unless force)
    if !force {
        if let Ok(Some(existing)) = ctx.db.get_skill(&spec.metadata.id) {
            // Check content hash to skip unchanged skills
            let new_hash = compute_spec_hash(&spec)?;
            let same_layer = existing.source_layer == skill.layer.as_str();
            if existing.content_hash == new_hash && same_layer {
                return Ok(()); // Skip unchanged
            }
        }
    }

    // Write using 2PC transaction manager
    tx_mgr.write_skill_with_layer(&spec, skill.layer)?;

    // Compute and persist quality score
    let scorer = crate::quality::QualityScorer::with_defaults();
    let quality = scorer.score_spec(&spec, &crate::quality::QualityContext::default());
    ctx.db
        .update_skill_quality(&spec.metadata.id, quality.overall as f64)?;

    // Also update Tantivy search index
    if let Ok(Some(skill_record)) = ctx.db.get_skill(&spec.metadata.id) {
        ctx.search.index_skill(&skill_record)?;
    }

    Ok(())
}

fn compute_spec_hash(spec: &crate::core::SkillSpec) -> Result<String> {
    use sha2::{Digest, Sha256};

    let json = serde_json::to_string(spec)
        .map_err(|e| MsError::InvalidSkill(format!("serialize spec for hash: {e}")))?;
    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());
    let result = hasher.finalize();
    Ok(hex::encode(result))
}
