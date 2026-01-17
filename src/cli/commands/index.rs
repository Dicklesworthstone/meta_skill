//! ms index - Index skills from configured paths

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Args;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use walkdir::WalkDir;

use crate::app::AppContext;
use crate::cli::output::OutputFormat;
use crate::core::{GitSkillRepository, ResolutionCache, SkillLayer, spec_lens::parse_markdown};
use crate::error::{MsError, Result};
use crate::storage::{SkillRecord, TxManager};
use crate::storage::tx::GlobalLock;
use crate::sync::ru::RuClient;

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

    /// Index skills from ru-managed repositories
    #[arg(long)]
    pub from_ru: bool,
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
        if ctx.output_format != OutputFormat::Human {
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

    if ctx.output_format != OutputFormat::Human {
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

    // If --from-ru, use ru-managed repositories
    if args.from_ru {
        return collect_ru_paths(ctx);
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

/// Collect paths from ru-managed repositories
fn collect_ru_paths(ctx: &AppContext) -> Result<Vec<SkillRoot>> {
    let mut ru_client = RuClient::new();

    if !ru_client.is_available() {
        if ctx.output_format != OutputFormat::Human {
            // Return empty list with no error for robot mode
            return Ok(Vec::new());
        }
        return Err(MsError::Config(
            "ru is not available. Install from /data/projects/repo_updater or use other index paths.".to_string(),
        ));
    }

    let paths = ru_client.list_paths()?;

    // Treat ru-managed repos as community/shared layer
    let roots: Vec<SkillRoot> = paths
        .into_iter()
        .map(|path| SkillRoot {
            path,
            layer: SkillLayer::Base,
        })
        .collect();

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

    // Create resolution cache and repository for resolving inherited/composed skills
    let resolution_cache = ResolutionCache::new();
    let repository = GitSkillRepository::new(&ctx.git);

    for skill in &skill_files {
        pb.set_message(format!(
            "{}",
            skill.path.file_name().unwrap_or_default().to_string_lossy()
        ));

        match index_skill_file(ctx, &tx_mgr, &resolution_cache, &repository, skill, args.force) {
            Ok(()) => indexed += 1,
            Err(e) => {
                errors += 1;
                pb.println(format!("{} {} - {}", "✗".red(), skill.path.display(), e));
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
        println!("{} {} skills failed to index", "!".yellow(), errors);
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

    // Create resolution cache and repository for resolving inherited/composed skills
    let resolution_cache = ResolutionCache::new();
    let repository = GitSkillRepository::new(&ctx.git);

    for skill in &skill_files {
        match index_skill_file(ctx, &tx_mgr, &resolution_cache, &repository, skill, args.force) {
            Ok(()) => indexed += 1,
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
            .filter_map(std::result::Result::ok)
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
    resolution_cache: &ResolutionCache,
    repository: &GitSkillRepository<'_>,
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
    let new_hash = compute_spec_hash(&spec)?;
    if !force {
        if let Ok(Some(existing)) = ctx.db.get_skill(&spec.metadata.id) {
            // Check content hash to skip unchanged skills
            let same_layer = existing.source_layer == skill.layer.as_str();
            if existing.content_hash == new_hash && same_layer {
                return Ok(()); // Skip unchanged
            }
        }
    }

    // Write using 2PC transaction manager (stores raw spec)
    tx_mgr.write_skill_with_layer(&spec, skill.layer)?;

    // Compute and persist quality score
    let scorer = crate::quality::QualityScorer::with_defaults();
    let quality = scorer.score_spec(&spec, &crate::quality::QualityContext::default());
    ctx.db
        .update_skill_quality(&spec.metadata.id, f64::from(quality.overall))?;

    // Resolve the skill if it has inheritance or composition
    let needs_resolution = spec.extends.is_some() || !spec.includes.is_empty();

    if needs_resolution {
        // Create a hash lookup function that reads skills from git archive and hashes them
        let compute_hash = |skill_id: &str| -> Option<String> {
            // For the current skill, use the already computed hash
            if skill_id == spec.metadata.id {
                return Some(new_hash.clone());
            }
            // For other skills, read from archive and compute hash
            ctx.git
                .read_skill(skill_id)
                .ok()
                .and_then(|dep_spec| compute_spec_hash(&dep_spec).ok())
        };

        // Get or compute the resolved skill
        let db_conn = ctx.db.conn();
        let resolved = resolution_cache.get_or_resolve(
            db_conn,
            &spec.metadata.id,
            &spec,
            repository,
            compute_hash,
        )?;

        // Build a SkillRecord from the resolved spec for search indexing
        let resolved_record = build_skill_record_from_resolved(&resolved.spec, skill, &new_hash);
        ctx.search.index_skill(&resolved_record)?;
    } else {
        // No resolution needed - index the raw spec directly
        if let Ok(Some(skill_record)) = ctx.db.get_skill(&spec.metadata.id) {
            ctx.search.index_skill(&skill_record)?;
        }
    }

    Ok(())
}

/// Build a SkillRecord from a resolved SkillSpec for search indexing
fn build_skill_record_from_resolved(
    spec: &crate::core::SkillSpec,
    discovered: &DiscoveredSkill,
    content_hash: &str,
) -> SkillRecord {
    // Concatenate all section content for the body field
    let body = spec
        .sections
        .iter()
        .flat_map(|section| section.blocks.iter())
        .map(|block| block.content.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");

    // Serialize metadata for the JSON field
    let metadata_json = serde_json::to_string(&spec.metadata).unwrap_or_default();

    // Version may be empty string, convert to Option
    let version = if spec.metadata.version.is_empty() {
        None
    } else {
        Some(spec.metadata.version.clone())
    };

    SkillRecord {
        id: spec.metadata.id.clone(),
        name: spec.metadata.name.clone(),
        description: spec.metadata.description.clone(),
        version,
        author: spec.metadata.author.clone(),
        source_path: discovered.path.display().to_string(),
        source_layer: discovered.layer.as_str().to_string(),
        git_remote: None,
        git_commit: None,
        content_hash: content_hash.to_string(),
        body,
        metadata_json,
        assets_json: "[]".to_string(), // No assets in current SkillSpec
        token_count: 0,                // Will be computed separately if needed
        quality_score: 0.0,            // Will be updated by quality scorer
        indexed_at: chrono::Utc::now().to_rfc3339(),
        modified_at: chrono::Utc::now().to_rfc3339(),
        is_deprecated: false, // Not tracked in current SkillMetadata
        deprecation_reason: None,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ==================== Expand Path Tests ====================

    #[test]
    fn test_expand_path_relative() {
        let result = expand_path("./relative/path");
        assert_eq!(result, PathBuf::from("./relative/path"));
    }

    #[test]
    fn test_expand_path_absolute() {
        let result = expand_path("/absolute/path");
        assert_eq!(result, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn test_expand_path_tilde_only() {
        let result = expand_path("~");
        if let Some(home) = dirs::home_dir() {
            assert_eq!(result, home);
        } else {
            assert_eq!(result, PathBuf::from("~"));
        }
    }

    #[test]
    fn test_expand_path_tilde_subpath() {
        let result = expand_path("~/subpath/file");
        if let Some(home) = dirs::home_dir() {
            assert_eq!(result, home.join("subpath/file"));
        } else {
            assert_eq!(result, PathBuf::from("~/subpath/file"));
        }
    }

    #[test]
    fn test_expand_path_no_tilde_prefix() {
        // Paths like "~user/path" should not be expanded
        let result = expand_path("~user/path");
        assert_eq!(result, PathBuf::from("~user/path"));
    }

    #[test]
    fn test_expand_path_empty() {
        let result = expand_path("");
        assert_eq!(result, PathBuf::from(""));
    }

    // ==================== Argument Parsing Tests ====================

    #[test]
    fn test_index_args_defaults() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            args: IndexArgs,
        }

        let cli = TestCli::parse_from(["test"]);
        assert!(cli.args.paths.is_empty());
        assert!(!cli.args.watch);
        assert!(!cli.args.force);
        assert!(!cli.args.all);
    }

    #[test]
    fn test_index_args_with_paths() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            args: IndexArgs,
        }

        let cli = TestCli::parse_from(["test", "./skills", "./more-skills"]);
        assert_eq!(cli.args.paths, vec!["./skills", "./more-skills"]);
    }

    #[test]
    fn test_index_args_watch_flag() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            args: IndexArgs,
        }

        let cli = TestCli::parse_from(["test", "--watch"]);
        assert!(cli.args.watch);
    }

    #[test]
    fn test_index_args_force_long() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            args: IndexArgs,
        }

        let cli = TestCli::parse_from(["test", "--force"]);
        assert!(cli.args.force);
    }

    #[test]
    fn test_index_args_force_short() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            args: IndexArgs,
        }

        let cli = TestCli::parse_from(["test", "-f"]);
        assert!(cli.args.force);
    }

    #[test]
    fn test_index_args_all_flag() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            args: IndexArgs,
        }

        let cli = TestCli::parse_from(["test", "--all"]);
        assert!(cli.args.all);
    }

    #[test]
    fn test_index_args_combined() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            args: IndexArgs,
        }

        let cli = TestCli::parse_from(["test", "--force", "--all", "./path"]);
        assert!(cli.args.force);
        assert!(cli.args.all);
        assert_eq!(cli.args.paths, vec!["./path"]);
    }

    #[test]
    fn test_index_args_from_ru_flag() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            args: IndexArgs,
        }

        let cli = TestCli::parse_from(["test", "--from-ru"]);
        assert!(cli.args.from_ru);
        assert!(!cli.args.force);
        assert!(!cli.args.all);
    }

    #[test]
    fn test_index_args_from_ru_with_force() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            args: IndexArgs,
        }

        let cli = TestCli::parse_from(["test", "--from-ru", "--force"]);
        assert!(cli.args.from_ru);
        assert!(cli.args.force);
    }

    // ==================== Discover Skill Files Tests ====================

    #[test]
    fn test_discover_skill_files_empty_root() {
        let temp = TempDir::new().unwrap();
        let roots = vec![SkillRoot {
            path: temp.path().to_path_buf(),
            layer: SkillLayer::Project,
        }];

        let result = discover_skill_files(&roots);
        assert!(result.is_empty());
    }

    #[test]
    fn test_discover_skill_files_single_skill() {
        let temp = TempDir::new().unwrap();
        let skill_dir = temp.path().join("my-skill");
        fs::create_dir(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# My Skill").unwrap();

        let roots = vec![SkillRoot {
            path: temp.path().to_path_buf(),
            layer: SkillLayer::Project,
        }];

        let result = discover_skill_files(&roots);
        assert_eq!(result.len(), 1);
        assert!(result[0].path.ends_with("SKILL.md"));
        assert_eq!(result[0].layer, SkillLayer::Project);
    }

    #[test]
    fn test_discover_skill_files_multiple_skills() {
        let temp = TempDir::new().unwrap();

        for name in ["skill1", "skill2", "skill3"] {
            let skill_dir = temp.path().join(name);
            fs::create_dir(&skill_dir).unwrap();
            fs::write(skill_dir.join("SKILL.md"), format!("# {}", name)).unwrap();
        }

        let roots = vec![SkillRoot {
            path: temp.path().to_path_buf(),
            layer: SkillLayer::User,
        }];

        let result = discover_skill_files(&roots);
        assert_eq!(result.len(), 3);
        assert!(result.iter().all(|s| s.layer == SkillLayer::User));
    }

    #[test]
    fn test_discover_skill_files_nested_directory() {
        let temp = TempDir::new().unwrap();

        let nested_path = temp.path().join("nested").join("deep").join("skill");
        fs::create_dir_all(&nested_path).unwrap();
        fs::write(nested_path.join("SKILL.md"), "# Nested Skill").unwrap();

        let roots = vec![SkillRoot {
            path: temp.path().to_path_buf(),
            layer: SkillLayer::Base,
        }];

        let result = discover_skill_files(&roots);
        assert_eq!(result.len(), 1);
        assert!(result[0].path.to_string_lossy().contains("nested"));
    }

    #[test]
    fn test_discover_skill_files_ignores_non_skill() {
        let temp = TempDir::new().unwrap();

        // Create a skill directory
        let skill_dir = temp.path().join("real-skill");
        fs::create_dir(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# Real Skill").unwrap();

        // Create a non-skill directory with README.md instead
        let non_skill_dir = temp.path().join("not-a-skill");
        fs::create_dir(&non_skill_dir).unwrap();
        fs::write(non_skill_dir.join("README.md"), "# Not a skill").unwrap();

        // Create a file named SKILL.md at root (not in a subdirectory)
        fs::write(temp.path().join("SKILL.md"), "# Root Level").unwrap();

        let roots = vec![SkillRoot {
            path: temp.path().to_path_buf(),
            layer: SkillLayer::Project,
        }];

        let result = discover_skill_files(&roots);
        // Should find both the nested skill and the root-level SKILL.md
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_discover_skill_files_nonexistent_root() {
        let roots = vec![SkillRoot {
            path: PathBuf::from("/nonexistent/path/12345"),
            layer: SkillLayer::Project,
        }];

        let result = discover_skill_files(&roots);
        assert!(result.is_empty());
    }

    #[test]
    fn test_discover_skill_files_multiple_roots() {
        let temp1 = TempDir::new().unwrap();
        let temp2 = TempDir::new().unwrap();

        // Create skills in each root
        let skill1 = temp1.path().join("skill1");
        fs::create_dir(&skill1).unwrap();
        fs::write(skill1.join("SKILL.md"), "# Skill 1").unwrap();

        let skill2 = temp2.path().join("skill2");
        fs::create_dir(&skill2).unwrap();
        fs::write(skill2.join("SKILL.md"), "# Skill 2").unwrap();

        let roots = vec![
            SkillRoot {
                path: temp1.path().to_path_buf(),
                layer: SkillLayer::Project,
            },
            SkillRoot {
                path: temp2.path().to_path_buf(),
                layer: SkillLayer::User,
            },
        ];

        let result = discover_skill_files(&roots);
        assert_eq!(result.len(), 2);

        let project_skills: Vec<_> = result
            .iter()
            .filter(|s| s.layer == SkillLayer::Project)
            .collect();
        let user_skills: Vec<_> = result
            .iter()
            .filter(|s| s.layer == SkillLayer::User)
            .collect();

        assert_eq!(project_skills.len(), 1);
        assert_eq!(user_skills.len(), 1);
    }

    // ==================== Compute Spec Hash Tests ====================

    #[test]
    fn test_compute_spec_hash_deterministic() {
        use crate::core::SkillSpec;

        let spec = SkillSpec::new("test-skill", "Test Skill");

        let hash1 = compute_spec_hash(&spec).unwrap();
        let hash2 = compute_spec_hash(&spec).unwrap();

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_compute_spec_hash_different_for_different_specs() {
        use crate::core::SkillSpec;

        let spec1 = SkillSpec::new("spec1", "Spec One");
        let spec2 = SkillSpec::new("spec2", "Spec Two");

        let hash1 = compute_spec_hash(&spec1).unwrap();
        let hash2 = compute_spec_hash(&spec2).unwrap();

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_compute_spec_hash_is_sha256() {
        use crate::core::SkillSpec;

        let spec = SkillSpec::new("test-skill", "Test");
        let hash = compute_spec_hash(&spec).unwrap();

        // SHA256 produces 64 hex characters
        assert_eq!(hash.len(), 64);

        // Should only contain hex characters
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // ==================== SkillRoot Tests ====================

    #[test]
    fn test_skill_root_struct() {
        let root = SkillRoot {
            path: PathBuf::from("/test/path"),
            layer: SkillLayer::Org,
        };

        assert_eq!(root.path, PathBuf::from("/test/path"));
        assert_eq!(root.layer, SkillLayer::Org);
    }

    // ==================== DiscoveredSkill Tests ====================

    #[test]
    fn test_discovered_skill_struct() {
        let skill = DiscoveredSkill {
            path: PathBuf::from("/test/skill/SKILL.md"),
            layer: SkillLayer::Base,
        };

        assert_eq!(skill.path, PathBuf::from("/test/skill/SKILL.md"));
        assert_eq!(skill.layer, SkillLayer::Base);
    }
}
