//! ms index - Index skills from configured paths

use std::collections::BTreeMap;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::Args;
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use walkdir::WalkDir;

use crate::app::AppContext;
use crate::cli::output::OutputFormat;
use crate::core::{
    spec_lens::parse_markdown, GitSkillRepository, ResolutionCache, SkillLayer,
    SkillPackageManifest, SkillPackageSummary, SkillResourceEntry, SkillResourceType,
};
use crate::error::{MsError, Result};
use crate::storage::tx::GlobalLock;
use crate::storage::tx::{SkillWritePackagePayload, SkillWriteResourcePayload};
use crate::storage::{SkillRecord, TxManager};
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
    package_root: PathBuf,
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

        match index_skill_file(
            ctx,
            &tx_mgr,
            &resolution_cache,
            &repository,
            skill,
            args.force,
        ) {
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
        match index_skill_file(
            ctx,
            &tx_mgr,
            &resolution_cache,
            &repository,
            skill,
            args.force,
        ) {
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
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| {
                !entry.file_type().is_dir() || !should_skip_discovery_dir(entry.path())
            })
            .filter_map(std::result::Result::ok)
        {
            if entry.file_type().is_file() && entry.file_name() == "SKILL.md" {
                let Some(package_root) = entry.path().parent() else {
                    continue;
                };
                skill_files.push(DiscoveredSkill {
                    path: entry.path().to_path_buf(),
                    package_root: package_root.to_path_buf(),
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
    let package_manifest = discover_package_manifest(&skill.path, &skill.package_root)?;

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
    let needs_resolution = spec.extends.is_some() || !spec.includes.is_empty();
    let manifest_json = serde_json::to_string(&package_manifest)
        .map_err(|e| MsError::InvalidSkill(format!("serialize package manifest: {e}")))?;
    let existing = ctx.db.get_skill(&spec.metadata.id).ok().flatten();
    if !force {
        if let Some(existing) = existing.as_ref() {
            // Check content hash to skip unchanged skills
            let same_layer = existing.source_layer == skill.layer.as_str();
            let same_bundle =
                existing.bundle_hash.as_deref() == Some(&package_manifest.bundle_hash);
            let baseline_complete = existing.manifest_json == manifest_json
                && ctx.db.count_skill_resources(&spec.metadata.id).ok()
                    == Some(package_manifest.resources.len());
            if !needs_resolution
                && existing.content_hash == new_hash
                && same_layer
                && same_bundle
                && baseline_complete
            {
                return Ok(()); // Skip unchanged
            }
        }
    }
    let content_changed = existing
        .as_ref()
        .map(|existing| {
            existing.content_hash != new_hash || existing.source_layer != skill.layer.as_str()
        })
        .unwrap_or(true);
    let baseline_changed = existing
        .as_ref()
        .map(|existing| {
            existing.bundle_hash.as_deref() != Some(&package_manifest.bundle_hash)
                || existing.manifest_json != manifest_json
                || ctx.db.count_skill_resources(&spec.metadata.id).ok()
                    != Some(package_manifest.resources.len())
        })
        .unwrap_or(true);

    let requires_tx_write = force || content_changed || baseline_changed;

    if requires_tx_write {
        let package_payload = SkillWritePackagePayload {
            bundle_hash: Some(package_manifest.bundle_hash.clone()),
            manifest: Some(package_manifest.clone()),
            resources: package_manifest
                .resources
                .iter()
                .map(|resource| SkillWriteResourcePayload {
                    relative_path: resource.relative_path.clone(),
                    source_path: skill.package_root.join(&resource.relative_path),
                    resource_type: resource.resource_type.as_str().to_string(),
                    content_hash: resource.content_hash.clone(),
                    size_bytes: resource.size_bytes,
                })
                .collect(),
        };

        // Write using 2PC transaction manager (stores raw spec)
        tx_mgr.write_skill_with_package(&spec, skill.layer, Some(package_payload))?;
    }

    // Compute and persist quality score
    let scorer = crate::quality::QualityScorer::with_defaults();
    let quality = scorer.score_spec(&spec, &crate::quality::QualityContext::default());
    ctx.db
        .update_skill_quality(&spec.metadata.id, f64::from(quality.overall))?;
    let refreshed_record = ctx.db.get_skill(&spec.metadata.id)?;

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
        let resolved_record = build_skill_record_from_resolved(
            &resolved.spec,
            skill,
            &new_hash,
            &package_manifest,
            refreshed_record.as_ref(),
        );
        ctx.search.index_skill(&resolved_record)?;
    } else {
        // No resolution needed - index the raw spec directly
        if let Some(skill_record) = refreshed_record {
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
    package_manifest: &SkillPackageManifest,
    existing: Option<&SkillRecord>,
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
        git_remote: existing.and_then(|record| record.git_remote.clone()),
        git_commit: existing.and_then(|record| record.git_commit.clone()),
        content_hash: content_hash.to_string(),
        bundle_hash: Some(package_manifest.bundle_hash.clone()),
        body,
        manifest_json: serde_json::to_string(package_manifest).unwrap_or_else(|_| "{}".to_string()),
        metadata_json,
        assets_json: "[]".to_string(), // No assets in current SkillSpec
        token_count: existing.map_or(0, |record| record.token_count),
        quality_score: existing.map_or(0.0, |record| record.quality_score),
        indexed_at: existing
            .map(|record| record.indexed_at.clone())
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
        modified_at: existing
            .map(|record| record.modified_at.clone())
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
        is_deprecated: existing.is_some_and(|record| record.is_deprecated),
        deprecation_reason: existing.and_then(|record| record.deprecation_reason.clone()),
    }
}

fn discover_package_manifest(
    skill_path: &Path,
    package_root: &Path,
) -> Result<SkillPackageManifest> {
    let canonical_root = std::fs::canonicalize(package_root).map_err(|e| {
        MsError::InvalidSkill(format!(
            "{}: cannot canonicalize package root {}: {e}",
            skill_path.display(),
            package_root.display()
        ))
    })?;

    let mut resources = Vec::new();
    scan_package_resources(&canonical_root, &canonical_root, skill_path, &mut resources)?;
    resources.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));

    let mut resource_type_counts = BTreeMap::<String, usize>::new();
    let mut total_bytes: u64 = 0;
    for resource in &resources {
        *resource_type_counts
            .entry(resource.resource_type.as_str().to_string())
            .or_insert(0) += 1;
        total_bytes = total_bytes.saturating_add(resource.size_bytes);
    }

    let bundle_hash = compute_bundle_hash(&resources)?;
    Ok(SkillPackageManifest {
        bundle_hash,
        summary: SkillPackageSummary {
            package_root: PathBuf::from("."),
            resource_count: resources.len(),
            total_bytes,
            resource_type_counts,
        },
        resources,
    })
}

fn should_skip_package_dir(package_root: &Path, entry_path: &Path) -> bool {
    let Some(name) = entry_path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    if matches!(name, ".git" | ".hg" | ".svn" | "target" | "node_modules") {
        return true;
    }

    entry_path != package_root && entry_path.join("SKILL.md").is_file()
}

fn should_skip_discovery_dir(entry_path: &Path) -> bool {
    let Some(name) = entry_path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    matches!(name, ".git" | ".hg" | ".svn" | "target" | "node_modules")
}

fn scan_package_resources(
    package_root: &Path,
    dir: &Path,
    skill_path: &Path,
    resources: &mut Vec<SkillResourceEntry>,
) -> Result<()> {
    let read_dir = std::fs::read_dir(dir).map_err(|e| {
        MsError::InvalidSkill(format!(
            "{}: cannot read package directory {}: {e}",
            skill_path.display(),
            dir.display()
        ))
    })?;

    for entry in read_dir {
        let entry = entry.map_err(|e| {
            MsError::InvalidSkill(format!(
                "{}: cannot read package entry in {}: {e}",
                skill_path.display(),
                dir.display()
            ))
        })?;
        let entry_path = entry.path();
        let file_type = entry.file_type().map_err(|e| {
            MsError::InvalidSkill(format!(
                "{}: cannot inspect package entry {}: {e}",
                skill_path.display(),
                entry_path.display()
            ))
        })?;

        if file_type.is_symlink() {
            continue;
        }

        if file_type.is_dir() {
            if should_skip_package_dir(package_root, &entry_path) {
                continue;
            }
            scan_package_resources(package_root, &entry_path, skill_path, resources)?;
            continue;
        }

        if !file_type.is_file() {
            continue;
        }

        let canonical_file = std::fs::canonicalize(&entry_path).map_err(|e| {
            MsError::InvalidSkill(format!(
                "{}: cannot canonicalize package file {}: {e}",
                skill_path.display(),
                entry_path.display()
            ))
        })?;

        if !canonical_file.starts_with(package_root) {
            return Err(MsError::InvalidSkill(format!(
                "{}: resource path escapes package root: {}",
                skill_path.display(),
                entry_path.display()
            )));
        }

        let relative_path = canonical_file
            .strip_prefix(package_root)
            .map_err(|e| {
                MsError::InvalidSkill(format!(
                    "{}: invalid relative package path {}: {e}",
                    skill_path.display(),
                    canonical_file.display()
                ))
            })?
            .to_path_buf();

        let metadata = std::fs::metadata(&canonical_file).map_err(|e| {
            MsError::InvalidSkill(format!(
                "{}: cannot read metadata for {}: {e}",
                skill_path.display(),
                canonical_file.display()
            ))
        })?;
        let content_hash = compute_file_hash(&canonical_file)?;
        let resource_type = classify_resource_type(&relative_path);

        resources.push(SkillResourceEntry {
            relative_path,
            resource_type,
            size_bytes: metadata.len(),
            content_hash,
        });
    }

    Ok(())
}

fn classify_resource_type(relative_path: &Path) -> SkillResourceType {
    if relative_path == Path::new("SKILL.md") {
        return SkillResourceType::SkillSpec;
    }

    if relative_path.starts_with("scripts") {
        return SkillResourceType::Script;
    }
    if relative_path.starts_with("references") {
        return SkillResourceType::Reference;
    }
    if relative_path.starts_with("tests") {
        return SkillResourceType::Test;
    }

    SkillResourceType::Other
}

fn compute_file_hash(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let file = std::fs::File::open(path)
        .map_err(|e| MsError::InvalidSkill(format!("read resource {}: {e}", path.display())))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .map_err(|e| MsError::InvalidSkill(format!("hash resource {}: {e}", path.display())))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn normalize_resource_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn compute_bundle_hash(resources: &[SkillResourceEntry]) -> Result<String> {
    use sha2::{Digest, Sha256};
    let normalized = resources
        .iter()
        .map(|entry| {
            serde_json::json!({
                "path": normalize_resource_path(&entry.relative_path),
                "type": entry.resource_type.as_str(),
                "size": entry.size_bytes,
                "hash": entry.content_hash,
            })
        })
        .collect::<Vec<_>>();

    let json = serde_json::to_string(&normalized)
        .map_err(|e| MsError::InvalidSkill(format!("serialize bundle payload for hash: {e}")))?;
    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());
    Ok(hex::encode(hasher.finalize()))
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
    #[cfg(unix)]
    use std::os::unix::fs::symlink;
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
            package_root: PathBuf::from("/test/skill"),
            layer: SkillLayer::Base,
        };

        assert_eq!(skill.path, PathBuf::from("/test/skill/SKILL.md"));
        assert_eq!(skill.package_root, PathBuf::from("/test/skill"));
        assert_eq!(skill.layer, SkillLayer::Base);
    }

    #[test]
    fn test_discover_package_manifest_skill_md_only() {
        let temp = TempDir::new().unwrap();
        let skill_dir = temp.path().join("legacy-skill");
        fs::create_dir(&skill_dir).unwrap();
        let skill_md = skill_dir.join("SKILL.md");
        fs::write(&skill_md, "# Legacy Skill").unwrap();

        let manifest = discover_package_manifest(&skill_md, &skill_dir).unwrap();
        assert_eq!(manifest.summary.resource_count, 1);
        assert_eq!(manifest.resources.len(), 1);
        assert_eq!(
            manifest.resources[0].relative_path,
            PathBuf::from("SKILL.md")
        );
        assert_eq!(
            manifest.resources[0].resource_type,
            SkillResourceType::SkillSpec
        );
        assert_eq!(
            manifest.summary.resource_type_counts.get("skill_spec"),
            Some(&1)
        );
        assert_eq!(manifest.bundle_hash.len(), 64);
        assert_eq!(manifest.summary.package_root, PathBuf::from("."));
    }

    #[test]
    fn test_discover_package_manifest_skips_git_dir_and_nested_skill_package() {
        let temp = TempDir::new().unwrap();
        let root_skill_dir = temp.path().join("root-skill");
        fs::create_dir(&root_skill_dir).unwrap();
        let skill_md = root_skill_dir.join("SKILL.md");
        fs::write(&skill_md, "# Root Skill").unwrap();

        let git_dir = root_skill_dir.join(".git");
        fs::create_dir(&git_dir).unwrap();
        fs::write(git_dir.join("config"), "[core]").unwrap();

        let docs_dir = root_skill_dir.join("references");
        fs::create_dir(&docs_dir).unwrap();
        fs::write(docs_dir.join("guide.md"), "guide").unwrap();

        let nested_skill = root_skill_dir.join("nested-skill");
        fs::create_dir(&nested_skill).unwrap();
        fs::write(nested_skill.join("SKILL.md"), "# Nested").unwrap();
        fs::write(nested_skill.join("notes.md"), "nested notes").unwrap();

        let manifest = discover_package_manifest(&skill_md, &root_skill_dir).unwrap();
        let paths: Vec<_> = manifest
            .resources
            .iter()
            .map(|r| r.relative_path.to_string_lossy().to_string())
            .collect();

        assert!(paths.contains(&"SKILL.md".to_string()));
        assert!(paths.contains(&"references/guide.md".to_string()));
        assert!(!paths.iter().any(|path| path.starts_with(".git/")));
        assert!(!paths.iter().any(|path| path.starts_with("nested-skill/")));
    }

    #[test]
    fn test_discover_skill_files_prunes_junk_dirs() {
        let temp = TempDir::new().unwrap();
        let real_skill = temp.path().join("real-skill");
        fs::create_dir(&real_skill).unwrap();
        fs::write(real_skill.join("SKILL.md"), "# Real").unwrap();

        let git_dir = temp.path().join(".git");
        fs::create_dir(&git_dir).unwrap();
        fs::write(git_dir.join("SKILL.md"), "# Should not be discovered").unwrap();

        let roots = vec![SkillRoot {
            path: temp.path().to_path_buf(),
            layer: SkillLayer::Project,
        }];

        let result = discover_skill_files(&roots);
        assert_eq!(result.len(), 1);
        assert!(result[0].path.ends_with("real-skill/SKILL.md"));
    }

    #[cfg(unix)]
    #[test]
    fn test_discover_skill_files_does_not_follow_symlink_dirs() {
        let temp = TempDir::new().unwrap();
        let real_skill = temp.path().join("real");
        fs::create_dir(&real_skill).unwrap();
        fs::write(real_skill.join("SKILL.md"), "# Real").unwrap();

        let external = TempDir::new().unwrap();
        let external_skill = external.path().join("external");
        fs::create_dir(&external_skill).unwrap();
        fs::write(external_skill.join("SKILL.md"), "# External").unwrap();
        let link_path = temp.path().join("linked-dir");
        symlink(&external_skill, &link_path).unwrap();

        let roots = vec![SkillRoot {
            path: temp.path().to_path_buf(),
            layer: SkillLayer::Project,
        }];

        let result = discover_skill_files(&roots);
        assert_eq!(result.len(), 1);
        assert!(result[0].path.ends_with("real/SKILL.md"));
    }

    #[cfg(unix)]
    #[test]
    fn test_discover_package_manifest_skips_symlinked_file() {
        let temp = TempDir::new().unwrap();
        let skill_dir = temp.path().join("skill");
        fs::create_dir(&skill_dir).unwrap();
        let skill_md = skill_dir.join("SKILL.md");
        fs::write(&skill_md, "# Skill").unwrap();

        let outside = temp.path().join("outside.txt");
        fs::write(&outside, "secret").unwrap();
        symlink(&outside, skill_dir.join("scripts_link.sh")).unwrap();

        let manifest = discover_package_manifest(&skill_md, &skill_dir).unwrap();
        let paths: Vec<_> = manifest
            .resources
            .iter()
            .map(|r| r.relative_path.to_string_lossy().to_string())
            .collect();

        assert_eq!(paths, vec!["SKILL.md".to_string()]);
    }
}
