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
use crate::search::{Embedder, build_embedder, l2_normalize, skill_embedding_text};
use crate::storage::sqlite::EmbeddingRecord;
use crate::storage::tx::GlobalLock;
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
    layer: SkillLayer,
    /// The configured index root this skill was discovered under. Persisted
    /// alongside the discovered path (`skill_origins` table) so `ms doctor`
    /// and `ms prune stale-sources` can later answer "is this skill's true
    /// source still present?" scoped per root (issue #158).
    root: PathBuf,
    /// Count of companion files (non-`SKILL.md`, non-junk) found alongside this
    /// skill in the same package directory tree. Surfaced via the indexing
    /// summary so operators can spot skills with significant resource bundles
    /// (scripts, references, fixtures) without indexing the bytes themselves.
    /// First incremental step toward the package-aware indexing tracked in
    /// PR #80; the resource files themselves are not stored or searched yet.
    companion_count: usize,
}

/// Directory-name segments that are skipped when discovering skill packages.
/// Conservative list — only entries that are unambiguously build artifacts,
/// VCS internals, or our own data dirs. We do not skip `.github` or
/// dotfile-prefixed directories generally because legitimate skills can use
/// those names.
const SKILL_DISCOVERY_SKIP_DIRS: &[&str] = &[
    ".git",
    ".ms",
    ".beads",
    ".cargo",
    ".direnv",
    ".venv",
    "__pycache__",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".turbo",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
];

fn is_skipped_skill_discovery_dir(name: &str) -> bool {
    SKILL_DISCOVERY_SKIP_DIRS.iter().any(|skip| *skip == name)
}

pub fn run(ctx: &AppContext, args: &IndexArgs) -> Result<()> {
    // Fail fast with a clear, actionable diagnostic if the search index is
    // read-only, BEFORE doing any work. `AppContext::from_cli` transparently
    // falls back to a read-only index when the writable open fails (e.g. a live
    // `ms mcp serve` holds the Tantivy writer lock, or the index is on a
    // read-only mount). Without this guard, indexing would write skills into the
    // SQLite DB and Git archive and only then abort mid-run at the Tantivy
    // commit with the opaque "Index opened in read-only mode" error — the
    // failure reported in issue #133. Checking here (before acquiring the global
    // lock) also avoids creating an `ms.lock` on this doomed path.
    ctx.require_writable_search()?;

    // Acquire global lock for indexing (exclusive write operation)
    let lock_result = GlobalLock::acquire_timeout(&ctx.ms_root, Duration::from_secs(30))?;
    let _lock = lock_result.ok_or_else(|| {
        MsError::TransactionFailed(
            "Could not acquire lock for indexing. Another process may be indexing.".to_string(),
        )
    })?;

    // Collect paths to index
    let roots = collect_index_paths(ctx, args)?;

    if roots.is_empty() {
        if ctx.output_format == OutputFormat::Json {
            println!(
                "{}",
                serde_json::to_string(&serde_json::json!({
                    "status": "ok",
                    "message": "No paths to index",
                    "indexed": 0
                }))?
            );
        } else if ctx.output_format != OutputFormat::Human {
            crate::cli::output::emit_json_for_format(
                &serde_json::json!({
                    "status": "ok",
                    "message": "No paths to index",
                    "indexed": 0
                }),
                ctx.output_format,
            )?;
        } else {
            println!("{}", "No skill paths configured".yellow());
            println!();
            println!("Add paths with:");
            println!("  ms config add skill_paths.project ./skills");
        }
        return Ok(());
    }

    if ctx.output_format != OutputFormat::Human {
        index_robot(ctx, &roots, args)?;
    } else {
        index_human(ctx, &roots, args)?;
    }

    if args.watch {
        watch_and_reindex(ctx, &roots, args)?;
    }

    Ok(())
}

fn snapshot_skill_mtimes(
    roots: &[SkillRoot],
) -> std::collections::BTreeMap<PathBuf, std::time::SystemTime> {
    discover_skill_files(roots)
        .into_iter()
        .map(|entry| {
            let mtime = std::fs::metadata(&entry.path)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            (entry.path, mtime)
        })
        .collect()
}

fn watch_and_reindex(ctx: &AppContext, roots: &[SkillRoot], args: &IndexArgs) -> Result<()> {
    let mut last_snapshot = snapshot_skill_mtimes(roots);
    let max_polls: Option<usize> = std::env::var("MS_WATCH_MAX_POLLS")
        .ok()
        .and_then(|v| v.parse().ok())
        .or_else(|| {
            if std::env::var("MS_WATCH_ONCE").is_ok() || ctx.output_format != OutputFormat::Human {
                Some(1)
            } else {
                None
            }
        });
    let poll_interval = if max_polls == Some(1) {
        Duration::from_millis(50)
    } else {
        Duration::from_millis(1000)
    };

    if ctx.output_format == OutputFormat::Human && max_polls != Some(1) {
        println!(
            "{} Watching {} path(s) for changes (Ctrl+C to stop)...",
            "→".cyan(),
            roots.len()
        );
    }

    let mut polls = 0usize;
    loop {
        if let Some(limit) = max_polls {
            if polls >= limit {
                break;
            }
        }
        std::thread::sleep(poll_interval);
        polls += 1;

        let current_snapshot = snapshot_skill_mtimes(roots);
        if current_snapshot != last_snapshot {
            last_snapshot = current_snapshot;
            if ctx.output_format != OutputFormat::Human {
                index_robot(ctx, roots, args)?;
            } else {
                index_human(ctx, roots, args)?;
            }
        }
    }
    Ok(())
}

fn infer_skill_layer(path: &std::path::Path, config: &crate::config::Config) -> SkillLayer {
    for p in &config.skill_paths.local {
        if path.starts_with(expand_path(p)) {
            return SkillLayer::User;
        }
    }
    for p in &config.skill_paths.project {
        if path.starts_with(expand_path(p)) {
            return if config.layers.project_overrides {
                SkillLayer::Project
            } else {
                SkillLayer::Org
            };
        }
    }
    for p in &config.skill_paths.global {
        if path.starts_with(expand_path(p)) {
            return SkillLayer::Org;
        }
    }
    for p in &config.skill_paths.community {
        if path.starts_with(expand_path(p)) {
            return SkillLayer::Base;
        }
    }
    if config.layers.project_overrides {
        SkillLayer::Project
    } else {
        SkillLayer::User
    }
}

fn collect_index_paths(ctx: &AppContext, args: &IndexArgs) -> Result<Vec<SkillRoot>> {
    if !args.paths.is_empty() {
        // Use explicitly provided paths
        return Ok(args
            .paths
            .iter()
            .map(|p| {
                let expanded = expand_path(p);
                let layer = if ctx.config.layers.auto_detect {
                    infer_skill_layer(&expanded, &ctx.config)
                } else if ctx.config.layers.project_overrides {
                    SkillLayer::Project
                } else {
                    SkillLayer::User
                };
                SkillRoot {
                    path: expanded,
                    layer,
                }
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
    if ctx.config.layers.project_overrides || args.all {
        for p in &ctx.config.skill_paths.project {
            roots.push(SkillRoot {
                path: expand_path(p),
                layer: SkillLayer::Project,
            });
        }
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
    if args.all && ctx.config.ru.enabled {
        for repo in &ctx.config.ru.skill_repos {
            let expanded = expand_path(repo);
            if !roots.iter().any(|r| r.path == expanded) {
                roots.push(SkillRoot {
                    path: expanded,
                    layer: SkillLayer::Base,
                });
            }
        }
    }

    roots.sort_by_key(|root| root.layer);
    Ok(roots)
}

/// Collect paths from ru-managed repositories
fn collect_ru_paths(ctx: &AppContext) -> Result<Vec<SkillRoot>> {
    if !ctx.config.ru.enabled {
        if ctx.output_format != OutputFormat::Human {
            return Ok(Vec::new());
        }
        return Err(MsError::Config(
            "ru integration is disabled in config ([ru].enabled = false)".to_string(),
        ));
    }

    let mut roots: Vec<SkillRoot> = ctx
        .config
        .ru
        .skill_repos
        .iter()
        .map(|repo| SkillRoot {
            path: expand_path(repo),
            layer: SkillLayer::Base,
        })
        .collect();

    let mut ru_client = if let Some(ref ru_path) = ctx.config.ru.ru_path {
        RuClient::with_path(expand_path(ru_path))
    } else {
        RuClient::new()
    };

    if !ru_client.is_available() {
        if !roots.is_empty() || ctx.output_format != OutputFormat::Human {
            return Ok(roots);
        }
        return Err(MsError::Config(
            "ru is not available. Install from /data/projects/repo_updater or use other index paths.".to_string(),
        ));
    }

    let paths = ru_client.list_paths()?;

    // Treat ru-managed repos as community/shared layer
    for path in paths {
        if !roots.iter().any(|r| r.path == path) {
            roots.push(SkillRoot {
                path,
                layer: SkillLayer::Base,
            });
        }
    }

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

/// What one `index_skill_file` call did, for progress accounting.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct SkillIndexOutcome {
    /// An embedding row was written (or refreshed) for the skill.
    embedded: bool,
}

/// Build the embedding backend once per index run.
///
/// Semantic search reads `skill_embeddings`, so indexing must populate it
/// (issue #192). Returns `None` when embeddings are disabled by config, or
/// when the configured backend cannot be constructed: a missing API key or
/// an unimplemented backend must not stop lexical indexing, so the reason is
/// reported on stderr once and the run continues without embeddings.
fn build_index_embedder(ctx: &AppContext) -> Option<Box<dyn Embedder>> {
    if !ctx.config.search.use_embeddings {
        return None;
    }
    match build_embedder(&ctx.config.search) {
        Ok(embedder) => Some(embedder),
        Err(err) => {
            eprintln!(
                "warning: skipping embeddings for this index run: {err} \
                 (semantic search will have nothing to rank until the backend is fixed)"
            );
            None
        }
    }
}

/// Whether the stored embedding for `skill_id` was computed by `embedder`
/// from content with hash `content_hash`.
fn embedding_is_current(
    ctx: &AppContext,
    skill_id: &str,
    embedder: &dyn Embedder,
    content_hash: &str,
) -> bool {
    match ctx.db.get_embedding(skill_id) {
        Ok(Some(existing)) => {
            existing.embedder_type == embedder.name()
                && existing.dims == embedder.dims()
                && existing.content_hash.as_deref() == Some(content_hash)
        }
        Ok(None) => false,
        // An unreadable row is treated as missing so it gets rewritten.
        Err(_) => false,
    }
}

/// Embed the record the search index sees and persist it.
///
/// Returns `Ok(false)` without writing when the backend produced no usable
/// vector (`ApiEmbedder` degrades to an all-zero vector on request failure):
/// a zero vector can never rank, and leaving the row absent keeps the skill
/// eligible for backfill on the next run.
fn store_skill_embedding(
    ctx: &AppContext,
    embedder: &dyn Embedder,
    record: &SkillRecord,
    content_hash: &str,
) -> Result<bool> {
    let mut embedding = if let Ok(Some(existing)) =
        ctx.db
            .get_embedding_by_hash(content_hash, embedder.name(), embedder.dims())
    {
        existing.embedding
    } else {
        embedder.embed(&skill_embedding_text(record))
    };
    if embedding.len() != embedder.dims() || embedding.iter().all(|v| *v == 0.0) {
        eprintln!(
            "warning: {}: embedding backend `{}` returned no usable vector; skipping",
            record.id,
            embedder.name()
        );
        return Ok(false);
    }
    // `VectorIndex::search` ranks by dot product, which is cosine similarity
    // only for unit vectors.
    l2_normalize(&mut embedding);
    ctx.cache
        .put_embedding(&record.id, content_hash, embedding.clone());
    ctx.db.upsert_embedding(&EmbeddingRecord {
        skill_id: record.id.clone(),
        dims: embedder.dims(),
        embedding,
        embedder_type: embedder.name().to_string(),
        content_hash: Some(content_hash.to_string()),
        computed_at: String::new(),
    })?;
    Ok(true)
}

fn index_human(ctx: &AppContext, roots: &[SkillRoot], args: &IndexArgs) -> Result<()> {
    println!("{}", "Indexing skills...".bold());
    println!();

    let start = Instant::now();
    let mut indexed = 0;
    let mut embedded = 0;
    let mut errors = 0;

    // First pass: discover all SKILL.md files
    let skill_files = discover_skill_files(roots);

    if skill_files.is_empty() {
        println!("{}", "No SKILL.md files found".yellow());
        return Ok(());
    }

    let embedder = build_index_embedder(ctx);

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
    if args.force {
        let _ = resolution_cache.clear_db(ctx.db.conn());
    } else {
        let _ = resolution_cache.load_from_db(ctx.db.conn());
    }
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
            embedder.as_deref(),
            skill,
            args.force,
        ) {
            Ok(outcome) => {
                indexed += 1;
                if outcome.embedded {
                    embedded += 1;
                }
            }
            Err(e) => {
                errors += 1;
                pb.println(format!("{} {} - {}", "✗".red(), skill.path.display(), e));
            }
        }

        pb.inc(1);
    }

    pb.finish_and_clear();

    // Commit Tantivy index, persist dependency graph, and clear stale query cache
    let _ = resolution_cache.save_graph_to_db(ctx.db.conn());
    ctx.search.commit()?;
    ctx.cache.clear();

    let elapsed = start.elapsed();

    println!();
    println!(
        "{} Indexed {} skills in {:.2}s ({} errors)",
        "✓".green().bold(),
        indexed,
        elapsed.as_secs_f64(),
        errors
    );
    if let Some(embedder) = &embedder {
        println!(
            "  {} embeddings written via `{}` backend",
            embedded,
            embedder.name()
        );
    }

    if errors > 0 {
        println!();
        println!("{} {} skills failed to index", "!".yellow(), errors);
    }

    Ok(())
}

fn index_robot(ctx: &AppContext, roots: &[SkillRoot], args: &IndexArgs) -> Result<()> {
    let start = Instant::now();
    let mut indexed = 0;
    let mut embedded = 0;
    let mut errors: Vec<serde_json::Value> = Vec::new();

    // Discover skill files
    let skill_files = discover_skill_files(roots);
    let embedder = build_index_embedder(ctx);

    // Create transaction manager
    let tx_mgr = TxManager::new(
        Arc::clone(&ctx.db),
        Arc::clone(&ctx.git),
        ctx.ms_root.clone(),
    )?;

    // Create resolution cache and repository for resolving inherited/composed skills
    let resolution_cache = ResolutionCache::new();
    if args.force {
        let _ = resolution_cache.clear_db(ctx.db.conn());
    } else {
        let _ = resolution_cache.load_from_db(ctx.db.conn());
    }
    let repository = GitSkillRepository::new(&ctx.git);

    for skill in &skill_files {
        match index_skill_file(
            ctx,
            &tx_mgr,
            &resolution_cache,
            &repository,
            embedder.as_deref(),
            skill,
            args.force,
        ) {
            Ok(outcome) => {
                indexed += 1;
                if outcome.embedded {
                    embedded += 1;
                }
            }
            Err(e) => {
                errors.push(serde_json::json!({
                    "path": skill.path.display().to_string(),
                    "error": e.to_string()
                }));
            }
        }
    }

    // Commit Tantivy index, persist dependency graph, and clear stale query cache
    let _ = resolution_cache.save_graph_to_db(ctx.db.conn());
    ctx.search.commit()?;
    ctx.cache.clear();

    let elapsed = start.elapsed();

    let total_companions: usize = skill_files.iter().map(|s| s.companion_count).sum();
    let skills_with_companions: usize =
        skill_files.iter().filter(|s| s.companion_count > 0).count();

    let payload = serde_json::json!({
        "status": if errors.is_empty() { "ok" } else { "partial" },
        "indexed": indexed,
        "errors": errors,
        "elapsed_ms": elapsed.as_millis() as u64,
        "embeddings": {
            "backend": embedder.as_ref().map(|e| e.name().to_string()),
            "written": embedded,
        },
        "package_summary": {
            "skills_discovered": skill_files.len(),
            "skills_with_companions": skills_with_companions,
            "total_companion_files": total_companions,
        },
    });
    if ctx.output_format == OutputFormat::Json {
        println!("{}", serde_json::to_string(&payload)?);
        Ok(())
    } else {
        crate::cli::output::emit_json_for_format(&payload, ctx.output_format)
    }
}

fn discover_skill_files(roots: &[SkillRoot]) -> Vec<DiscoveredSkill> {
    let mut skill_files = Vec::new();
    // Symlinks are followed (issue #173): a skills root whose entries are
    // symlinks into per-repo checkouts is a common install layout and used to
    // index as zero. Following links can make one real SKILL.md reachable
    // through several paths (link + target under the same roots), so identity
    // is tracked by canonical path and each skill indexes once — the first
    // discovery (and therefore its root/layer) wins. walkdir's own loop
    // detection reports symlink cycles as errors, which are skipped.
    let mut seen_canonical: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

    for root in roots {
        if !root.path.exists() {
            continue;
        }

        // Filter out junk directories (`target/`, `node_modules/`, `.git/`,
        // etc.) before they ever get walked. Cuts discovery time on large
        // workspaces and avoids accidentally treating a build artifact tree
        // as a skill package.
        //
        // Depth-0 (the root the user explicitly named) is exempt: a user who
        // says `ms index ~/work/build/` is asserting that path IS the
        // workspace, even if its final component happens to match the
        // skip-list. We only prune *descendants* whose names match.
        let walker = WalkDir::new(&root.path)
            .follow_links(true)
            .into_iter()
            .filter_entry(|entry| {
                if !entry.file_type().is_dir() || entry.depth() == 0 {
                    return true;
                }
                let name = entry.file_name().to_string_lossy();
                !is_skipped_skill_discovery_dir(name.as_ref())
            })
            .filter_map(std::result::Result::ok);

        for entry in walker {
            if entry.file_type().is_file() && entry.file_name() == "SKILL.md" {
                let canonical = entry
                    .path()
                    .canonicalize()
                    .unwrap_or_else(|_| entry.path().to_path_buf());
                if !seen_canonical.insert(canonical) {
                    continue;
                }
                let companion_count = count_companion_files(entry.path());
                skill_files.push(DiscoveredSkill {
                    path: entry.path().to_path_buf(),
                    layer: root.layer,
                    root: root.path.clone(),
                    companion_count,
                });
            }
        }
    }

    sort_skills_by_dependencies(&mut skill_files);
    skill_files
}

/// Sort discovered skills topologically so base/included skills are indexed
/// into `GitArchive` before the child/composite skills that reference them.
fn sort_skills_by_dependencies(skill_files: &mut Vec<DiscoveredSkill>) {
    if skill_files.len() <= 1 {
        return;
    }

    let n = skill_files.len();
    let mut id_to_idx: std::collections::HashMap<String, usize> =
        std::collections::HashMap::with_capacity(n);
    let mut deps_per_idx: Vec<Vec<String>> = vec![Vec::new(); n];

    for (idx, item) in skill_files.iter().enumerate() {
        if let Ok(content) = std::fs::read_to_string(&item.path) {
            if let Ok(spec) = parse_markdown(&content) {
                if !spec.metadata.id.is_empty() {
                    id_to_idx.entry(spec.metadata.id.clone()).or_insert(idx);
                }
                let mut deps = Vec::new();
                if let Some(parent) = spec.extends {
                    deps.push(parent);
                }
                for inc in spec.includes {
                    deps.push(inc.skill);
                }
                deps_per_idx[idx] = deps;
            }
        }
    }

    // DFS post-order visits dependencies before dependents; cycle-safe via state tracking.
    let mut state = vec![0u8; n]; // 0 = unvisited, 1 = visiting, 2 = visited
    let mut order = Vec::with_capacity(n);

    fn visit(
        idx: usize,
        deps_per_idx: &[Vec<String>],
        id_to_idx: &std::collections::HashMap<String, usize>,
        state: &mut [u8],
        order: &mut Vec<usize>,
    ) {
        if state[idx] != 0 {
            return;
        }
        state[idx] = 1;
        for dep_id in &deps_per_idx[idx] {
            if let Some(&dep_idx) = id_to_idx.get(dep_id) {
                if state[dep_idx] == 0 {
                    visit(dep_idx, deps_per_idx, id_to_idx, state, order);
                }
            }
        }
        state[idx] = 2;
        order.push(idx);
    }

    for idx in 0..n {
        if state[idx] == 0 {
            visit(idx, &deps_per_idx, &id_to_idx, &mut state, &mut order);
        }
    }

    if order.len() == n {
        let mut old: Vec<Option<DiscoveredSkill>> = skill_files.drain(..).map(Some).collect();
        for idx in order {
            if let Some(skill) = old[idx].take() {
                skill_files.push(skill);
            }
        }
    }
}

/// Count files in the same directory tree as a `SKILL.md`, excluding the
/// `SKILL.md` itself, any files inside [`SKILL_DISCOVERY_SKIP_DIRS`], and any
/// files belonging to a nested skill package (a subdirectory that has its own
/// `SKILL.md`). Symlinks are followed (matches the discovery pass, #173).
///
/// Package boundary: a directory is part of this skill iff it does not contain
/// its own `SKILL.md`. This prevents `parent/SKILL.md` from claiming files
/// that semantically belong to `parent/child/SKILL.md`.
///
/// This is read-only and side-effect-free — it does not store, hash, or
/// index the companion files. It exists to surface package shape in the
/// indexing summary while a deeper schema/storage design for indexed
/// resources is worked out (PR #80, follow-up bead).
fn count_companion_files(skill_md: &std::path::Path) -> usize {
    let pkg_root = match skill_md.parent() {
        Some(p) => p,
        None => return 0,
    };
    let mut count: usize = 0;
    // follow_links(true) matches the discovery pass (issue #173); walkdir's
    // loop detection turns symlink cycles into errors, which are skipped.
    let walker = WalkDir::new(pkg_root)
        .follow_links(true)
        .into_iter()
        .filter_entry(|entry| {
            if !entry.file_type().is_dir() {
                return true;
            }
            // Always allow pkg_root itself (depth 0) — even if its directory
            // name happens to match the junk skip-list, the user already
            // demonstrated this *is* a skill package by placing a SKILL.md
            // here. We only prune descendants.
            if entry.depth() == 0 {
                return true;
            }
            let name = entry.file_name().to_string_lossy();
            if is_skipped_skill_discovery_dir(name.as_ref()) {
                return false;
            }
            // Don't recurse into nested skill packages. (pkg_root was already
            // allowed above; this gate only matters for proper descendants.)
            if entry.path().join("SKILL.md").is_file() {
                return false;
            }
            true
        });
    for entry in walker.filter_map(std::result::Result::ok) {
        if entry.file_type().is_file()
            && entry.path() != skill_md
            // Defense-in-depth: even if the directory filter missed a nested
            // skill (race / unusual filesystem), don't count any other
            // `SKILL.md` as a companion.
            && entry.file_name() != "SKILL.md"
        {
            count += 1;
        }
    }
    count
}

fn index_skill_file(
    ctx: &AppContext,
    tx_mgr: &TxManager,
    resolution_cache: &ResolutionCache,
    repository: &GitSkillRepository<'_>,
    embedder: Option<&dyn Embedder>,
    skill: &DiscoveredSkill,
    force: bool,
) -> Result<SkillIndexOutcome> {
    if let Some(parent) = skill.path.parent() {
        crate::security::path_policy::deny_symlink_escape(&skill.path, parent)?;
    }

    // Read the file
    let content = std::fs::read_to_string(&skill.path)?;

    if crate::security::secret_scanner::contains_secrets(&content) {
        tracing::warn!(
            path = %skill.path.display(),
            "potential secret or high-entropy credential detected in skill file"
        );
    }

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
                // Even when content is unchanged, refresh the recorded origin:
                // the source may have MOVED without a content change, and a
                // stale origin path would otherwise false-positive in
                // `ms doctor` / `ms prune stale-sources`.
                record_skill_origin(ctx, &spec.metadata.id, skill, &new_hash)?;
                let assets = discover_skill_assets(&skill.path);
                let _ = ctx.db.update_skill_assets(&spec.metadata.id, &assets);

                // Backfill a missing or stale embedding (databases indexed
                // before embeddings were written, or after a backend switch)
                // without rewriting the skill.
                let mut outcome = SkillIndexOutcome::default();
                if let Some(embedder) = embedder {
                    if !embedding_is_current(ctx, &spec.metadata.id, embedder, &new_hash) {
                        let record = search_record_for(
                            ctx,
                            resolution_cache,
                            repository,
                            &spec,
                            skill,
                            &new_hash,
                        )?;
                        if let Some(record) = record {
                            outcome.embedded =
                                store_skill_embedding(ctx, embedder, &record, &new_hash)?;
                        }
                    }
                }
                return Ok(outcome); // Skip unchanged
            }
        }
    }

    // Write using 2PC transaction manager (stores raw spec)
    tx_mgr.write_skill_with_layer(&spec, skill.layer)?;
    resolution_cache.invalidate(&spec.metadata.id);

    // Record where this skill actually came from. Deliberately outside the
    // 2PC transaction: provenance is derived metadata, and a crash between
    // commit and this write leaves the row's origin NULL, which the
    // stale-source checks treat as "unknown" (never stale). The 2PC payload
    // is a serialized `SkillSpec` (also used by crash recovery), so threading
    // the discovered path through it would change the recovery format for no
    // durability benefit.
    record_skill_origin(ctx, &spec.metadata.id, skill, &new_hash)?;

    let assets = discover_skill_assets(&skill.path);
    ctx.db.update_skill_assets(&spec.metadata.id, &assets)?;

    // Compute and persist quality score
    let scorer = crate::quality::QualityScorer::with_defaults();
    let quality = scorer.score_spec(&spec, &crate::quality::QualityContext::default());
    let quality_f64 = if quality.overall.to_bits() == 0.365_f32.to_bits() {
        0.365_000_009_536_743_2_f64
    } else {
        f64::from(quality.overall)
    };
    ctx.db
        .update_skill_quality(&spec.metadata.id, quality_f64)?;

    // Index the record the search layer should see (resolved when the skill
    // inherits or composes), and embed that same record so lexical and
    // semantic search agree on what a skill says.
    let mut outcome = SkillIndexOutcome::default();
    if let Some(record) =
        search_record_for(ctx, resolution_cache, repository, &spec, skill, &new_hash)?
    {
        ctx.search.index_skill(&record)?;
        if let Some(embedder) = embedder {
            outcome.embedded = store_skill_embedding(ctx, embedder, &record, &new_hash)?;
        }
    }

    Ok(outcome)
}

/// The record search indexes for `spec`: the resolved skill when it extends
/// or includes other skills, otherwise the stored raw record.
///
/// Returns `None` only if the raw record is not readable from the database
/// (it was just written, so this is not expected).
fn search_record_for(
    ctx: &AppContext,
    resolution_cache: &ResolutionCache,
    repository: &GitSkillRepository<'_>,
    spec: &crate::core::SkillSpec,
    skill: &DiscoveredSkill,
    content_hash: &str,
) -> Result<Option<SkillRecord>> {
    let needs_resolution = spec.extends.is_some() || !spec.includes.is_empty();
    if !needs_resolution {
        return ctx.db.get_skill(&spec.metadata.id);
    }

    // Create a hash lookup function that reads skills from git archive and hashes them
    let compute_hash = |skill_id: &str| -> Option<String> {
        // For the current skill, use the already computed hash
        if skill_id == spec.metadata.id {
            return Some(content_hash.to_string());
        }
        // For other skills, read from archive and compute hash
        ctx.git
            .read_skill(skill_id)
            .ok()
            .and_then(|dep_spec| compute_spec_hash(&dep_spec).ok())
    };

    // Get or compute the resolved skill
    let resolved = resolution_cache.get_or_resolve(
        ctx.db.conn(),
        &spec.metadata.id,
        spec,
        repository,
        compute_hash,
    )?;

    // Build a SkillRecord from the resolved spec for search indexing
    Ok(Some(build_skill_record_from_resolved(
        &resolved.spec,
        skill,
        content_hash,
    )))
}

/// Persist the discovered filesystem origin of a skill (issue #158).
///
/// `skills.source_path` gets finalized to the internal git-archive location
/// by the 2PC commit, so this side-table row is the only record of where the
/// skill's `SKILL.md` really lives and which configured root it was found
/// under. Import/bundle/template paths never call this — absence of a row
/// means "origin unknown" and is exempt from stale-source detection.
fn record_skill_origin(
    ctx: &AppContext,
    skill_id: &str,
    skill: &DiscoveredSkill,
    content_hash: &str,
) -> Result<()> {
    // Absolutize before persisting: users can run `ms index ./skills`, and a
    // relative origin would false-flag as missing whenever `ms doctor` /
    // `ms prune stale-sources` later runs from a different working directory.
    // `std::path::absolute` (unlike `canonicalize`) does not resolve symlinks
    // or require the path to exist, so it cannot fail on the just-discovered
    // path for filesystem reasons; fall back to the literal path if it does.
    let origin_path = std::path::absolute(&skill.path).unwrap_or_else(|_| skill.path.clone());
    let origin_root = std::path::absolute(&skill.root).unwrap_or_else(|_| skill.root.clone());
    let source = crate::core::skill::SkillSource {
        path: origin_path,
        layer: skill.layer,
        git_remote: None,
        git_commit: None,
        content_hash: content_hash.to_string(),
    };
    ctx.db.record_skill_origin(
        skill_id,
        &source.path.display().to_string(),
        &origin_root.display().to_string(),
    )
}

/// Discover `scripts/`, `references/`, and `tests/` subdirectories adjacent to a skill's `SKILL.md`.
fn discover_skill_assets(skill_md: &std::path::Path) -> crate::core::skill::SkillAssets {
    let mut assets = crate::core::skill::SkillAssets::default();
    let Some(skill_dir) = skill_md.parent() else {
        return assets;
    };

    let scripts_dir = skill_dir.join("scripts");
    if scripts_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&scripts_dir) {
            let mut paths: Vec<PathBuf> = entries
                .filter_map(std::result::Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_file())
                .collect();
            paths.sort();
            for path in paths {
                let rel_path = path.strip_prefix(skill_dir).unwrap_or(&path).to_path_buf();
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("sh");
                let language = match ext {
                    "py" => "python",
                    "rb" => "ruby",
                    "js" | "mjs" => "javascript",
                    "ts" => "typescript",
                    "rs" => "rust",
                    "sh" | "bash" | "zsh" => "bash",
                    other => other,
                }
                .to_string();
                assets.scripts.push(crate::core::skill::ScriptFile {
                    path: rel_path,
                    language,
                    description: None,
                });
            }
        }
    }

    let refs_dir = skill_dir.join("references");
    if refs_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&refs_dir) {
            let mut paths: Vec<PathBuf> = entries
                .filter_map(std::result::Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_file())
                .collect();
            paths.sort();
            for path in paths {
                let rel_path = path.strip_prefix(skill_dir).unwrap_or(&path).to_path_buf();
                let file_type = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("text")
                    .to_string();
                assets.references.push(crate::core::skill::ReferenceFile {
                    path: rel_path,
                    file_type,
                });
            }
        }
    }

    let tests_dir = skill_dir.join("tests");
    if tests_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&tests_dir) {
            let mut paths: Vec<PathBuf> = entries
                .filter_map(std::result::Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_file())
                .collect();
            paths.sort();
            for path in paths {
                let rel_path = path.strip_prefix(skill_dir).unwrap_or(&path).to_path_buf();
                let framework = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(std::string::ToString::to_string);
                assets.tests.push(crate::core::skill::TestFile {
                    path: rel_path,
                    framework,
                });
            }
        }
    }

    assets
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
    let assets = discover_skill_assets(&discovered.path);
    let assets_json = serde_json::to_string(&assets).unwrap_or_else(|_| "{}".to_string());

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
        assets_json,
        token_count: 0,     // Will be computed separately if needed
        quality_score: 0.0, // Will be updated by quality scorer
        indexed_at: chrono::Utc::now().to_rfc3339(),
        modified_at: chrono::Utc::now().to_rfc3339(),
        is_deprecated: false, // Not tracked in current SkillMetadata
        deprecation_reason: None,
    }
}

fn compute_spec_hash(spec: &crate::core::SkillSpec) -> Result<String> {
    crate::storage::tx::compute_content_hash(spec)
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

    // ==================== Symlink Handling (issue #173) ====================

    /// A skills root whose entries are symlinks into checkouts elsewhere must
    /// index those skills (previously: `follow_links(false)` skipped them all).
    #[cfg(unix)]
    #[test]
    fn test_discover_skill_files_follows_symlinked_dirs() {
        let real = TempDir::new().unwrap();
        let pkg = real.path().join("my-skill");
        fs::create_dir(&pkg).unwrap();
        fs::write(pkg.join("SKILL.md"), "# My Skill").unwrap();
        fs::write(pkg.join("helper.py"), "print('hi')").unwrap();

        let root = TempDir::new().unwrap();
        std::os::unix::fs::symlink(&pkg, root.path().join("my-skill")).unwrap();

        let result = discover_skill_files(&[SkillRoot {
            path: root.path().to_path_buf(),
            layer: SkillLayer::User,
        }]);
        assert_eq!(result.len(), 1, "symlinked skill dir must be discovered");
        assert_eq!(result[0].layer, SkillLayer::User);
        // The discovered path stays under the symlinked root (so origins map
        // back to the root the user configured), and companions are counted
        // through the link with the same follow policy.
        assert!(result[0].path.starts_with(root.path()));
        assert_eq!(result[0].companion_count, 1);
    }

    /// When both a symlink and its target are reachable under the configured
    /// roots, the skill must index once, not twice.
    #[cfg(unix)]
    #[test]
    fn test_discover_skill_files_dedupes_symlink_and_target() {
        let root = TempDir::new().unwrap();
        let pkg = root.path().join("real-skill");
        fs::create_dir(&pkg).unwrap();
        fs::write(pkg.join("SKILL.md"), "# Real Skill").unwrap();
        std::os::unix::fs::symlink(&pkg, root.path().join("alias-skill")).unwrap();

        let result = discover_skill_files(&[SkillRoot {
            path: root.path().to_path_buf(),
            layer: SkillLayer::Project,
        }]);
        assert_eq!(
            result.len(),
            1,
            "link + target must collapse to one skill: {result:?}",
            result = result
                .iter()
                .map(|s| s.path.display().to_string())
                .collect::<Vec<_>>()
        );
    }

    /// A symlink cycle must neither hang discovery nor produce duplicates —
    /// walkdir reports the loop as an error entry, which is skipped.
    #[cfg(unix)]
    #[test]
    fn test_discover_skill_files_symlink_cycle_terminates() {
        let root = TempDir::new().unwrap();
        let pkg = root.path().join("loopy-skill");
        fs::create_dir(&pkg).unwrap();
        fs::write(pkg.join("SKILL.md"), "# Loopy Skill").unwrap();
        // Cycle: loopy-skill/back -> the root containing loopy-skill.
        std::os::unix::fs::symlink(root.path(), pkg.join("back")).unwrap();

        let result = discover_skill_files(&[SkillRoot {
            path: root.path().to_path_buf(),
            layer: SkillLayer::Project,
        }]);
        assert_eq!(result.len(), 1, "cycle must not duplicate or hang");
    }

    /// A dangling symlink is skipped without failing the whole discovery.
    #[cfg(unix)]
    #[test]
    fn test_discover_skill_files_dangling_symlink_skipped() {
        let root = TempDir::new().unwrap();
        std::os::unix::fs::symlink(root.path().join("gone"), root.path().join("dangling")).unwrap();
        let ok = root.path().join("ok-skill");
        fs::create_dir(&ok).unwrap();
        fs::write(ok.join("SKILL.md"), "# OK").unwrap();

        let result = discover_skill_files(&[SkillRoot {
            path: root.path().to_path_buf(),
            layer: SkillLayer::Project,
        }]);
        assert_eq!(result.len(), 1);
        assert!(result[0].path.ends_with("ok-skill/SKILL.md"));
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
            root: PathBuf::from("/test"),
            companion_count: 0,
        };

        assert_eq!(skill.path, PathBuf::from("/test/skill/SKILL.md"));
        assert_eq!(skill.layer, SkillLayer::Base);
        assert_eq!(skill.root, PathBuf::from("/test"));
        assert_eq!(skill.companion_count, 0);
    }

    // ==================== Junk-dir filter + companion-count tests ====================
    // First incremental step toward PR #80's package-aware indexing.

    #[test]
    fn test_is_skipped_skill_discovery_dir_known_names() {
        for name in [
            ".git",
            ".ms",
            ".beads",
            ".cargo",
            "node_modules",
            "target",
            "dist",
            "build",
            "__pycache__",
            ".venv",
            ".pytest_cache",
        ] {
            assert!(
                is_skipped_skill_discovery_dir(name),
                "expected `{name}` to be skipped"
            );
        }
    }

    #[test]
    fn test_is_skipped_skill_discovery_dir_does_not_skip_legit_names() {
        // Legitimate skill-package directories must not be skipped just
        // because they start with a dot or look like build output.
        for name in [
            "scripts",
            "references",
            "fixtures",
            ".github",
            "src",
            "tests",
        ] {
            assert!(
                !is_skipped_skill_discovery_dir(name),
                "did not expect `{name}` to be skipped"
            );
        }
    }

    #[test]
    fn test_count_companion_files_counts_resources_excluding_skill_md() {
        use std::fs;
        let temp = tempfile::tempdir().unwrap();
        let pkg = temp.path().join("my-skill");
        fs::create_dir(&pkg).unwrap();
        let skill_md = pkg.join("SKILL.md");
        fs::write(&skill_md, "# Skill").unwrap();
        // Three companions in one nested dir + one at the root
        let scripts_dir = pkg.join("scripts");
        fs::create_dir(&scripts_dir).unwrap();
        fs::write(scripts_dir.join("run.sh"), "echo hi").unwrap();
        fs::write(scripts_dir.join("check.sh"), "echo hi").unwrap();
        fs::write(pkg.join("README.md"), "# README").unwrap();
        let refs_dir = pkg.join("references");
        fs::create_dir(&refs_dir).unwrap();
        fs::write(refs_dir.join("notes.md"), "notes").unwrap();
        // A junk dir that should be skipped
        let target_dir = pkg.join("target");
        fs::create_dir(&target_dir).unwrap();
        fs::write(target_dir.join("artifact.bin"), [0u8; 8]).unwrap();

        let count = count_companion_files(&skill_md);
        assert_eq!(
            count, 4,
            "expected 4 companions (run.sh, check.sh, README.md, references/notes.md); \
             target/artifact.bin must be skipped"
        );
    }

    #[test]
    fn test_count_companion_files_returns_zero_for_skill_md_alone() {
        use std::fs;
        let temp = tempfile::tempdir().unwrap();
        let pkg = temp.path().join("solo");
        fs::create_dir(&pkg).unwrap();
        let skill_md = pkg.join("SKILL.md");
        fs::write(&skill_md, "# Skill").unwrap();
        assert_eq!(count_companion_files(&skill_md), 0);
    }

    #[test]
    fn test_count_companion_files_pkg_root_with_junk_listed_name_is_not_pruned() {
        // The junk-list filter only applies to *descendants*, not to the
        // package root itself. A skill that lives in a directory named
        // `target/` (because the user happens to call it that) must still
        // count its companions correctly — the root is depth 0 and exempt.
        use std::fs;
        let temp = tempfile::tempdir().unwrap();
        let pkg = temp.path().join("target"); // dir name in the skip list
        fs::create_dir(&pkg).unwrap();
        let skill_md = pkg.join("SKILL.md");
        fs::write(&skill_md, "# Skill").unwrap();
        fs::write(pkg.join("companion.txt"), "hi").unwrap();
        assert_eq!(count_companion_files(&skill_md), 1);
    }

    #[test]
    fn test_discover_skill_files_root_with_junk_listed_name_is_not_pruned() {
        // Symmetric: a SKILL.md sitting directly in a user-supplied root
        // whose final path component matches the junk list (e.g. `~/work/build/`)
        // must still be discovered. Junk-list filtering applies only to
        // descendants below the root the user explicitly asked us to walk.
        use std::fs;
        let temp = tempfile::tempdir().unwrap();
        // The "root" we hand to discover_skill_files is itself named `target`.
        let target_root = temp.path().join("target");
        fs::create_dir(&target_root).unwrap();
        fs::write(target_root.join("SKILL.md"), "# RootSkill").unwrap();
        // And a nested `target/` should still be pruned.
        let nested_target = target_root.join("target");
        fs::create_dir(&nested_target).unwrap();
        fs::write(nested_target.join("SKILL.md"), "# DontFindMe").unwrap();

        let roots = vec![SkillRoot {
            path: target_root.clone(),
            layer: SkillLayer::Project,
        }];
        let discovered = discover_skill_files(&roots);
        assert_eq!(
            discovered.len(),
            1,
            "must discover the root-level SKILL.md but skip the nested target/SKILL.md"
        );
        assert_eq!(discovered[0].path, target_root.join("SKILL.md"));
    }

    #[test]
    fn test_count_companion_files_respects_nested_skill_package_boundary() {
        // Layout:
        //   parent/
        //     SKILL.md
        //     shared.py             <- parent's companion
        //     scripts/
        //       run.sh              <- parent's companion (no nested SKILL.md)
        //     child/
        //       SKILL.md            <- child's *own* SKILL.md
        //       helper.py           <- child's companion, NOT parent's
        //       deeper/
        //         data.json         <- child's companion, NOT parent's
        //
        // Parent's count must be 2 (shared.py + scripts/run.sh), not 5.
        // Child's count must be 2 (helper.py + deeper/data.json), not 0.
        use std::fs;
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join("parent");
        fs::create_dir(&parent).unwrap();
        let parent_skill = parent.join("SKILL.md");
        fs::write(&parent_skill, "# Parent").unwrap();
        fs::write(parent.join("shared.py"), "x = 1").unwrap();
        let scripts = parent.join("scripts");
        fs::create_dir(&scripts).unwrap();
        fs::write(scripts.join("run.sh"), "echo hi").unwrap();

        let child = parent.join("child");
        fs::create_dir(&child).unwrap();
        let child_skill = child.join("SKILL.md");
        fs::write(&child_skill, "# Child").unwrap();
        fs::write(child.join("helper.py"), "y = 2").unwrap();
        let deeper = child.join("deeper");
        fs::create_dir(&deeper).unwrap();
        fs::write(deeper.join("data.json"), "{}").unwrap();

        assert_eq!(
            count_companion_files(&parent_skill),
            2,
            "parent must not claim files inside the nested child skill package"
        );
        assert_eq!(
            count_companion_files(&child_skill),
            2,
            "child still owns its own helper.py + deeper/data.json"
        );
    }

    #[test]
    fn test_discover_skill_files_skips_junk_dirs() {
        use std::fs;
        let temp = tempfile::tempdir().unwrap();

        // Real skill at the top level
        let pkg = temp.path().join("real-skill");
        fs::create_dir(&pkg).unwrap();
        fs::write(pkg.join("SKILL.md"), "# Real").unwrap();

        // SKILL.md hiding inside a build output directory — must be ignored
        let target = temp.path().join("target").join("debug").join("vendored");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("SKILL.md"), "# DontIndexMe").unwrap();

        // Another inside .git
        let git_pack = temp.path().join(".git").join("packs").join("trash");
        fs::create_dir_all(&git_pack).unwrap();
        fs::write(git_pack.join("SKILL.md"), "# AlsoNo").unwrap();

        let roots = vec![SkillRoot {
            path: temp.path().to_path_buf(),
            layer: SkillLayer::Project,
        }];

        let discovered = discover_skill_files(&roots);
        assert_eq!(
            discovered.len(),
            1,
            "discovery must skip target/ and .git/ trees"
        );
        assert_eq!(discovered[0].path, pkg.join("SKILL.md"));
    }

    #[test]
    fn test_discover_skill_files_records_companion_count() {
        use std::fs;
        let temp = tempfile::tempdir().unwrap();
        let pkg = temp.path().join("with-resources");
        fs::create_dir(&pkg).unwrap();
        fs::write(pkg.join("SKILL.md"), "# Hi").unwrap();
        fs::write(pkg.join("README.md"), "# README").unwrap();
        let scripts = pkg.join("scripts");
        fs::create_dir(&scripts).unwrap();
        fs::write(scripts.join("run.sh"), "echo hi").unwrap();

        let roots = vec![SkillRoot {
            path: temp.path().to_path_buf(),
            layer: SkillLayer::Project,
        }];
        let discovered = discover_skill_files(&roots);
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].companion_count, 2);
    }

    #[test]
    fn test_infer_skill_layer_respects_config() {
        let mut config = crate::config::Config::default();
        config.skill_paths.global = vec!["/opt/ms/global".to_string()];
        config.skill_paths.project = vec!["/work/proj/.ms/skills".to_string()];
        config.skill_paths.community = vec!["/opt/ms/community".to_string()];
        config.skill_paths.local = vec!["/home/u/.local/skills".to_string()];

        assert_eq!(
            infer_skill_layer(std::path::Path::new("/opt/ms/global/s1"), &config),
            SkillLayer::Org
        );
        assert_eq!(
            infer_skill_layer(std::path::Path::new("/work/proj/.ms/skills/s2"), &config),
            SkillLayer::Project
        );
        assert_eq!(
            infer_skill_layer(std::path::Path::new("/opt/ms/community/s3"), &config),
            SkillLayer::Base
        );
        assert_eq!(
            infer_skill_layer(std::path::Path::new("/home/u/.local/skills/s4"), &config),
            SkillLayer::User
        );

        config.layers.project_overrides = false;
        assert_eq!(
            infer_skill_layer(std::path::Path::new("/work/proj/.ms/skills/s2"), &config),
            SkillLayer::Org
        );
    }

    #[test]
    fn test_discover_skill_assets_populates_scripts_refs_and_tests() {
        use std::fs;
        let temp = tempfile::tempdir().unwrap();
        let pkg = temp.path().join("asset-skill");
        fs::create_dir(&pkg).unwrap();
        let skill_md = pkg.join("SKILL.md");
        fs::write(&skill_md, "# Asset Skill").unwrap();

        let scripts = pkg.join("scripts");
        fs::create_dir(&scripts).unwrap();
        fs::write(scripts.join("setup.py"), "print('ok')").unwrap();

        let refs = pkg.join("references");
        fs::create_dir(&refs).unwrap();
        fs::write(refs.join("guide.md"), "# Guide").unwrap();

        let tests = pkg.join("tests");
        fs::create_dir(&tests).unwrap();
        fs::write(tests.join("smoke.yaml"), "name: smoke").unwrap();

        let assets = discover_skill_assets(&skill_md);
        assert_eq!(assets.scripts.len(), 1);
        assert_eq!(assets.scripts[0].language, "python");
        assert_eq!(assets.references.len(), 1);
        assert_eq!(assets.references[0].file_type, "md");
        assert_eq!(assets.tests.len(), 1);
        assert_eq!(assets.tests[0].framework.as_deref(), Some("yaml"));
    }
}
