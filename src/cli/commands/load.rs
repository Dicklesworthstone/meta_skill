//! ms load - Load a skill with progressive disclosure

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use clap::{Args, ValueEnum};
use colored::Colorize;
use serde::Deserialize;

use crate::app::AppContext;
use crate::core::dependencies::{
    DependencyGraph, DependencyLoadMode, DependencyResolver, DisclosureLevel as DepDisclosure,
};
use crate::core::disclosure::{
    DisclosedContent, DisclosureLevel, DisclosurePlan, PackMode, TokenBudget, disclose,
};
use crate::core::pack_contracts::{PackContractPreset, custom_contracts_path, find_custom_contract};
use crate::core::skill::{PackContract, SkillAssets, SkillMetadata};
use crate::core::spec_lens::parse_markdown;
use crate::error::{MsError, Result};
use crate::meta_skills::{ConditionContext, MetaSkillManager, MetaSkillRegistry};
use crate::storage::sqlite::SkillRecord;

/// Dependency loading strategy
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum DepsMode {
    /// Dependencies at overview level
    #[default]
    Auto,
    /// No dependency loading
    Off,
    /// Dependencies at full disclosure
    Full,
}

impl From<DepsMode> for DependencyLoadMode {
    fn from(mode: DepsMode) -> Self {
        match mode {
            DepsMode::Auto => DependencyLoadMode::Auto,
            DepsMode::Off => DependencyLoadMode::Off,
            DepsMode::Full => DependencyLoadMode::Full,
        }
    }
}

/// Pack mode for token budget optimization
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum CliPackMode {
    /// Even distribution across slice types
    #[default]
    Balanced,
    /// Prioritize highest-utility slices
    UtilityFirst,
    /// Prioritize coverage (rules, commands first)
    CoverageFirst,
    /// Boost pitfalls and warnings
    PitfallSafe,
}

/// Pack contract presets for packing strategies.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CliPackContract {
    Complete,
    Debug,
    Refactor,
    Learn,
    Quickref,
    Codegen,
}

impl CliPackContract {
    fn preset(self) -> PackContractPreset {
        match self {
            CliPackContract::Complete => PackContractPreset::Complete,
            CliPackContract::Debug => PackContractPreset::Debug,
            CliPackContract::Refactor => PackContractPreset::Refactor,
            CliPackContract::Learn => PackContractPreset::Learn,
            CliPackContract::Quickref => PackContractPreset::QuickRef,
            CliPackContract::Codegen => PackContractPreset::CodeGen,
        }
    }
}

impl From<CliPackMode> for PackMode {
    fn from(mode: CliPackMode) -> Self {
        match mode {
            CliPackMode::Balanced => PackMode::Balanced,
            CliPackMode::UtilityFirst => PackMode::UtilityFirst,
            CliPackMode::CoverageFirst => PackMode::CoverageFirst,
            CliPackMode::PitfallSafe => PackMode::PitfallSafe,
        }
    }
}

#[derive(Args, Debug)]
pub struct LoadArgs {
    /// Skill ID or name to load
    pub skill: String,

    /// Disclosure level (0=minimal, 1=overview, 2=standard, 3=full, 4=complete)
    #[arg(long, short = 'l')]
    pub level: Option<String>,

    /// Token budget for packing (overrides --level)
    #[arg(long)]
    pub pack: Option<usize>,

    /// Pack mode when using --pack
    #[arg(long, value_enum, default_value = "balanced")]
    pub mode: CliPackMode,

    /// Pack contract preset (requires --pack). Values: complete|debug|refactor|learn|quickref|codegen
    #[arg(long, value_enum)]
    pub contract: Option<CliPackContract>,

    /// Custom pack contract id (requires --pack)
    #[arg(long)]
    pub contract_id: Option<String>,

    /// Max slices per coverage group
    #[arg(long, default_value = "2")]
    pub max_per_group: usize,

    /// Alias for --level full
    #[arg(long)]
    pub full: bool,

    /// Alias for --level complete (includes scripts + references)
    #[arg(long)]
    pub complete: bool,

    /// Dependency loading strategy
    #[arg(long, value_enum, default_value = "auto")]
    pub deps: DepsMode,

    /// Experiment id to attribute this load
    #[arg(long)]
    pub experiment_id: Option<String>,

    /// Variant id for experiment attribution
    #[arg(long)]
    pub variant_id: Option<String>,
}

/// Result of loading a skill
#[derive(Debug, Clone)]
pub struct LoadResult {
    pub skill_id: String,
    pub name: String,
    pub disclosed: DisclosedContent,
    pub dependencies_loaded: Vec<String>,
    pub slices_included: Option<usize>,
}

pub fn run(ctx: &AppContext, args: &LoadArgs) -> Result<()> {
    // First try to load as meta-skill
    if let Some(meta_result) = try_load_meta_skill(ctx, args)? {
        return if ctx.robot_mode {
            output_robot_meta(ctx, &meta_result, args)
        } else {
            output_human_meta(ctx, &meta_result, args)
        };
    }

    // Fall back to regular skill loading
    let result = load_skill(ctx, args)?;

    if ctx.robot_mode {
        output_robot(ctx, &result, args)
    } else {
        output_human(ctx, &result, args)
    }
}

pub(crate) fn load_skill(ctx: &AppContext, args: &LoadArgs) -> Result<LoadResult> {
    // Resolve skill by ID or alias
    let skill = resolve_skill(ctx, &args.skill)?;

    if args.contract.is_some() && args.contract_id.is_some() {
        return Err(MsError::Config(
            "use either --contract or --contract-id".to_string(),
        ));
    }
    if (args.contract.is_some() || args.contract_id.is_some()) && args.pack.is_none() {
        return Err(MsError::Config(
            "--contract requires --pack".to_string(),
        ));
    }

    let contract = resolve_contract(ctx, args)?;

    let (experiment_id, variant_id) = validate_experiment_usage(
        ctx,
        &skill.id,
        args.experiment_id.as_deref(),
        args.variant_id.as_deref(),
    )?;

    // Determine disclosure plan
    let disclosure_plan = determine_disclosure_plan(args, contract);

    // Parse skill body into SkillSpec
    let spec = parse_markdown(&skill.body)
        .map_err(|e| MsError::ValidationFailed(format!("failed to parse skill body: {}", e)))?;

    // Merge metadata from database into spec metadata
    let metadata = merge_metadata(&skill, &spec.metadata);
    let mut spec = spec;
    spec.metadata = metadata;

    // Load assets from database
    let assets: SkillAssets = serde_json::from_str(&skill.assets_json)
        .unwrap_or_default();

    // Apply disclosure
    let disclosed = disclose(&spec, &assets, &disclosure_plan);
    let slices_included = disclosed.slices_included;

    // Handle dependencies if enabled
    let dependencies_loaded = if !matches!(args.deps, DepsMode::Off) {
        load_dependencies(ctx, &skill, args)?
    } else {
        vec![]
    };

    let result = LoadResult {
        skill_id: skill.id.clone(),
        name: skill.name.clone(),
        disclosed,
        dependencies_loaded,
        slices_included,
    };

    record_usage(
        ctx,
        &skill.id,
        &disclosure_plan,
        experiment_id.as_deref(),
        variant_id.as_deref(),
    );

    Ok(result)
}

fn resolve_skill(ctx: &AppContext, skill_ref: &str) -> Result<SkillRecord> {
    // Try direct ID lookup
    if let Some(skill) = ctx.db.get_skill(skill_ref)? {
        return Ok(skill);
    }

    // Try alias resolution
    if let Some(alias_result) = ctx.db.resolve_alias(skill_ref)? {
        if let Some(skill) = ctx.db.get_skill(&alias_result.canonical_id)? {
            return Ok(skill);
        }
    }

    Err(MsError::SkillNotFound(format!(
        "skill not found: {}",
        skill_ref
    )))
}

// ==================== Meta-Skill Integration ====================

/// Result of loading a meta-skill
#[derive(Debug)]
pub struct MetaSkillLoadResultWrapper {
    pub meta_skill_id: String,
    pub meta_skill_name: String,
    pub tokens_used: usize,
    pub slices_loaded: usize,
    pub slices_skipped: usize,
    pub packed_content: String,
}

/// Try to load as a meta-skill. Returns None if not found as a meta-skill.
fn try_load_meta_skill(ctx: &AppContext, args: &LoadArgs) -> Result<Option<MetaSkillLoadResultWrapper>> {
    let mut registry = MetaSkillRegistry::new();
    let meta_skill_paths = get_meta_skill_paths();

    // Load registry, but don't fail if no meta-skills exist
    if registry.load_from_paths(&meta_skill_paths).unwrap_or(0) == 0 {
        return Ok(None);
    }

    // Try to find meta-skill by ID
    let meta_skill = match registry.get(&args.skill) {
        Some(ms) => ms.clone(),
        None => return Ok(None),
    };

    // Found a meta-skill, load it
    let manager = MetaSkillManager::new(ctx);
    let working_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let tech_stacks = detect_tech_stacks(&working_dir);

    let condition_ctx = ConditionContext {
        working_dir: &working_dir,
        tech_stacks: &tech_stacks,
        loaded_slices: &HashSet::new(),
    };

    // Use pack budget if specified, otherwise use meta-skill's recommended tokens
    let budget = args.pack.unwrap_or(meta_skill.recommended_context_tokens);

    let result = manager.load(&meta_skill, budget, &condition_ctx)?;

    Ok(Some(MetaSkillLoadResultWrapper {
        meta_skill_id: result.meta_skill_id,
        meta_skill_name: meta_skill.name.clone(),
        tokens_used: result.tokens_used,
        slices_loaded: result.slices.len(),
        slices_skipped: result.skipped.len(),
        packed_content: result.packed_content,
    }))
}

/// Get meta-skill directories
fn get_meta_skill_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // Project meta-skills directory
    let working_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let project_meta = working_dir.join(".ms").join("meta-skills");
    if project_meta.exists() {
        paths.push(project_meta);
    }

    // Global meta-skills directory
    if let Some(home) = dirs::home_dir() {
        let global_meta = home.join(".ms").join("meta-skills");
        if global_meta.exists() {
            paths.push(global_meta);
        }
    }

    paths
}

/// Detect tech stacks from common config files
fn detect_tech_stacks(working_dir: &std::path::Path) -> Vec<String> {
    let mut stacks = Vec::new();

    let indicators = [
        ("Cargo.toml", "rust"),
        ("package.json", "javascript"),
        ("tsconfig.json", "typescript"),
        ("go.mod", "go"),
        ("requirements.txt", "python"),
        ("pyproject.toml", "python"),
        ("Gemfile", "ruby"),
        ("pom.xml", "java"),
        ("build.gradle", "java"),
        ("composer.json", "php"),
    ];

    for (file, stack) in indicators {
        if working_dir.join(file).exists() {
            stacks.push(stack.to_string());
        }
    }

    stacks
}

fn output_human_meta(_ctx: &AppContext, result: &MetaSkillLoadResultWrapper, _args: &LoadArgs) -> Result<()> {
    println!(
        "{} (meta-skill: {})",
        format!("# {}", result.meta_skill_name).bold(),
        result.meta_skill_id.cyan()
    );
    println!();

    // Stats
    println!(
        "{} {} tokens | {} slices loaded | {} skipped",
        "─".repeat(40).dimmed(),
        result.tokens_used,
        result.slices_loaded,
        result.slices_skipped
    );
    println!();

    // Content
    println!("{}", result.packed_content);

    Ok(())
}

fn output_robot_meta(_ctx: &AppContext, result: &MetaSkillLoadResultWrapper, args: &LoadArgs) -> Result<()> {
    let output = serde_json::json!({
        "status": "ok",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "version": env!("CARGO_PKG_VERSION"),
        "type": "meta_skill",
        "data": {
            "meta_skill_id": result.meta_skill_id,
            "name": result.meta_skill_name,
            "tokens_used": result.tokens_used,
            "budget": args.pack,
            "slices_loaded": result.slices_loaded,
            "slices_skipped": result.slices_skipped,
            "content": result.packed_content,
        },
        "warnings": []
    });
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn determine_disclosure_plan(args: &LoadArgs, contract: Option<PackContract>) -> DisclosurePlan {
    // Token budget takes precedence
    if let Some(tokens) = args.pack {
        return DisclosurePlan::Pack(TokenBudget {
            tokens,
            mode: args.mode.into(),
            max_per_group: args.max_per_group,
            contract,
        });
    }

    // Handle --complete flag
    if args.complete {
        return DisclosurePlan::Level(DisclosureLevel::Complete);
    }

    // Handle --full flag
    if args.full {
        return DisclosurePlan::Level(DisclosureLevel::Full);
    }

    // Parse explicit level
    if let Some(ref level_str) = args.level {
        if let Some(level) = DisclosureLevel::from_str_or_level(level_str) {
            return DisclosurePlan::Level(level);
        }
    }

    // Default to Standard
    DisclosurePlan::Level(DisclosureLevel::Standard)
}

fn resolve_contract(ctx: &AppContext, args: &LoadArgs) -> Result<Option<PackContract>> {
    if let Some(contract) = args.contract {
        return Ok(Some(contract.preset().contract()));
    }
    if let Some(ref id) = args.contract_id {
        let path = custom_contracts_path(&ctx.ms_root);
        let Some(contract) = find_custom_contract(&path, id)? else {
            return Err(MsError::SkillNotFound(format!(
                "contract not found: {}",
                id
            )));
        };
        return Ok(Some(contract));
    }
    Ok(None)
}

#[derive(Deserialize)]
struct ExperimentVariantRef {
    id: String,
}

fn validate_experiment_usage(
    ctx: &AppContext,
    skill_id: &str,
    experiment_id: Option<&str>,
    variant_id: Option<&str>,
) -> Result<(Option<String>, Option<String>)> {
    match (experiment_id, variant_id) {
        (None, None) => return Ok((None, None)),
        (Some(_), None) => {
            return Err(MsError::ValidationFailed(
                "--variant-id is required when --experiment-id is set".to_string(),
            ))
        }
        (None, Some(_)) => {
            return Err(MsError::ValidationFailed(
                "--experiment-id is required when --variant-id is set".to_string(),
            ))
        }
        (Some(experiment_id), Some(variant_id)) => {
            if experiment_id.trim().is_empty() {
                return Err(MsError::ValidationFailed(
                    "experiment id cannot be empty".to_string(),
                ));
            }
            if variant_id.trim().is_empty() {
                return Err(MsError::ValidationFailed(
                    "variant id cannot be empty".to_string(),
                ));
            }
        }
    }

    let experiment_id = experiment_id.expect("checked above");
    let variant_id = variant_id.expect("checked above");

    let record = ctx
        .db
        .get_skill_experiment(experiment_id)?
        .ok_or_else(|| MsError::NotFound(format!("experiment not found: {experiment_id}")))?;

    if record.skill_id != skill_id {
        return Err(MsError::ValidationFailed(format!(
            "experiment {} belongs to skill {}, not {}",
            experiment_id, record.skill_id, skill_id
        )));
    }

    let variants: Vec<ExperimentVariantRef> =
        serde_json::from_str(&record.variants_json).map_err(|err| {
            MsError::Serialization(format!("experiment variants parse: {err}"))
        })?;

    if !variants.iter().any(|variant| variant.id == variant_id) {
        return Err(MsError::ValidationFailed(format!(
            "unknown variant id for experiment {}: {}",
            experiment_id, variant_id
        )));
    }

    Ok((Some(experiment_id.to_string()), Some(variant_id.to_string())))
}

fn record_usage(
    ctx: &AppContext,
    skill_id: &str,
    plan: &DisclosurePlan,
    experiment_id: Option<&str>,
    variant_id: Option<&str>,
) {
    let disclosure_level = match plan {
        DisclosurePlan::Level(level) => level.level_num(),
        DisclosurePlan::Pack(_) => DisclosureLevel::Standard.level_num(),
    };
    let project_path = std::env::current_dir()
        .ok()
        .map(|path| path.to_string_lossy().to_string());

    if let Err(err) = ctx.db.record_skill_usage(
        skill_id,
        project_path.as_deref(),
        disclosure_level,
        None,
        experiment_id,
        variant_id,
    ) {
        if ctx.verbosity > 0 {
            eprintln!("warning: failed to record skill usage: {err}");
        }
    }
}

fn merge_metadata(skill: &SkillRecord, parsed_meta: &SkillMetadata) -> SkillMetadata {
    // Parse metadata_json from database
    let db_meta: serde_json::Value = serde_json::from_str(&skill.metadata_json).unwrap_or_default();

    SkillMetadata {
        id: skill.id.clone(),
        name: skill.name.clone(),
        version: skill.version.clone().unwrap_or_else(|| "0.1.0".to_string()),
        description: skill.description.clone(),
        tags: db_meta
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_else(|| parsed_meta.tags.clone()),
        requires: db_meta
            .get("requires")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_else(|| parsed_meta.requires.clone()),
        provides: db_meta
            .get("provides")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_else(|| parsed_meta.provides.clone()),
        platforms: parsed_meta.platforms.clone(),
        author: skill.author.clone().or_else(|| parsed_meta.author.clone()),
        license: parsed_meta.license.clone(),
    }
}

fn load_dependencies(
    ctx: &AppContext,
    skill: &SkillRecord,
    args: &LoadArgs,
) -> Result<Vec<String>> {
    // Parse requires from metadata
    let meta: serde_json::Value = serde_json::from_str(&skill.metadata_json).unwrap_or_default();

    let requires = meta_list(&meta, "requires");

    if requires.is_empty() {
        return Ok(vec![]);
    }

    // Build dependency graph with available skills
    let mut graph = DependencyGraph::new();

    // Add root skill
    let provides = meta_list(&meta, "provides");

    graph.add_skill(skill.id.clone(), requires.clone(), provides);

    let (skill_index, provider_index, meta_cache) = build_dependency_indexes(ctx)?;

    // Resolve required capabilities using provides mappings (transitive).
    let mut loaded_deps = Vec::new();
    let mut loaded_set = HashSet::new();
    let mut seen_caps = HashSet::new();
    let mut queue: VecDeque<String> = requires.into_iter().collect();

    while let Some(cap) = queue.pop_front() {
        if !seen_caps.insert(cap.clone()) {
            continue;
        }

        // Direct skill-id match (capability equals skill id).
        if let Some(dep_skill) = skill_index.get(&cap) {
            if add_dependency_node(&mut graph, dep_skill, &meta_cache, &cap, &mut queue) {
                if loaded_set.insert(dep_skill.id.clone()) {
                    loaded_deps.push(dep_skill.id.clone());
                }
            }
            continue;
        }

        // Otherwise, resolve providers by capability.
        if let Some(provider_ids) = provider_index.get(&cap) {
            for provider_id in provider_ids {
                if let Some(dep_skill) = skill_index.get(provider_id) {
                    if add_dependency_node(&mut graph, dep_skill, &meta_cache, &cap, &mut queue) {
                        if loaded_set.insert(dep_skill.id.clone()) {
                            loaded_deps.push(dep_skill.id.clone());
                        }
                    }
                }
            }
        }
    }

    graph.build_edges();

    // Resolve and return dependency list
    let dep_disclosure = match args.deps {
        DepsMode::Auto => DepDisclosure::Overview,
        DepsMode::Full => DepDisclosure::Full,
        DepsMode::Off => return Ok(vec![]),
    };

    let resolver = DependencyResolver::new(&graph);
    let plan = resolver.resolve(&skill.id, dep_disclosure, args.deps.into())?;

    if ctx.verbosity > 0 {
        if !plan.missing.is_empty() {
            let missing = plan
                .missing
                .iter()
                .map(|m| format!("{} (required by {})", m.capability, m.required_by))
                .collect::<Vec<_>>()
                .join(", ");
            eprintln!("warning: missing dependency capabilities: {missing}");
        }
        if !plan.cycles.is_empty() {
            let cycles = plan
                .cycles
                .iter()
                .map(|cycle| cycle.join(" -> "))
                .collect::<Vec<_>>()
                .join("; ");
            eprintln!("warning: dependency cycles detected: {cycles}");
        }
    }

    // Return just the dependency IDs (not the root)
    Ok(plan
        .ordered
        .iter()
        .filter(|p| p.skill_id != skill.id)
        .map(|p| p.skill_id.clone())
        .collect())
}

fn meta_list(meta: &serde_json::Value, key: &str) -> Vec<String> {
    meta.get(key)
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Clone)]
struct CachedMeta {
    requires: Vec<String>,
    provides: Vec<String>,
}

fn build_dependency_indexes(
    ctx: &AppContext,
) -> Result<(
    HashMap<String, SkillRecord>,
    HashMap<String, Vec<String>>,
    HashMap<String, CachedMeta>,
)> {
    let mut all_skills = Vec::new();
    let mut offset = 0usize;
    let limit = 200usize;
    loop {
        let batch = ctx.db.list_skills(limit, offset)?;
        if batch.is_empty() {
            break;
        }
        offset += batch.len();
        all_skills.extend(batch);
    }

    let mut skill_index = HashMap::new();
    let mut provider_index: HashMap<String, Vec<String>> = HashMap::new();
    let mut meta_cache = HashMap::new();

    for skill in all_skills {
        let meta_json: serde_json::Value =
            serde_json::from_str(&skill.metadata_json).unwrap_or_default();
        let provides = meta_list(&meta_json, "provides");
        let requires = meta_list(&meta_json, "requires");

        for cap in &provides {
            provider_index
                .entry(cap.clone())
                .or_default()
                .push(skill.id.clone());
        }
        meta_cache.insert(
            skill.id.clone(),
            CachedMeta {
                requires,
                provides,
            },
        );
        skill_index.insert(skill.id.clone(), skill);
    }

    Ok((skill_index, provider_index, meta_cache))
}

fn add_dependency_node(
    graph: &mut DependencyGraph,
    skill: &SkillRecord,
    meta_cache: &HashMap<String, CachedMeta>,
    fallback_capability: &str,
    queue: &mut VecDeque<String>,
) -> bool {
    if graph.get_node(&skill.id).is_some() {
        return false;
    }

    let meta = meta_cache.get(&skill.id);
    let mut provides = meta
        .map(|m| m.provides.clone())
        .unwrap_or_default();
    if provides.is_empty() {
        provides.push(fallback_capability.to_string());
    }

    let requires = meta
        .map(|m| m.requires.clone())
        .unwrap_or_default();

    for required in &requires {
        queue.push_back(required.clone());
    }

    graph.add_skill(skill.id.clone(), requires, provides);
    true
}

pub(crate) fn output_human(
    _ctx: &AppContext,
    result: &LoadResult,
    _args: &LoadArgs,
) -> Result<()> {
    let disclosed = &result.disclosed;

    // Header with skill name
    println!("{}", format!("# {}", disclosed.frontmatter.name).bold());
    println!();

    // Description
    if !disclosed.frontmatter.description.is_empty() {
        println!("{}", disclosed.frontmatter.description);
        println!();
    }

    // Dependencies loaded info
    if !result.dependencies_loaded.is_empty() {
        println!(
            "{} {}",
            "Dependencies loaded:".dimmed(),
            result.dependencies_loaded.join(", ")
        );
        println!();
    }

    // Main body content
    if let Some(ref body) = disclosed.body {
        println!("{}", body);
    }

    // Scripts (at Complete level)
    if !disclosed.scripts.is_empty() {
        println!();
        println!("{}", "## Scripts".bold());
        for script in &disclosed.scripts {
            println!("- {} ({})", script.path.display(), script.language);
        }
    }

    // References (at Complete level)
    if !disclosed.references.is_empty() {
        println!();
        println!("{}", "## References".bold());
        for reference in &disclosed.references {
            println!("- {} ({})", reference.path.display(), reference.file_type);
        }
    }

    // Footer with stats
    println!();
    println!(
        "{} {} tokens | {} level",
        "─".repeat(40).dimmed(),
        disclosed.token_estimate,
        disclosed.level.name()
    );

    Ok(())
}

fn output_robot(_ctx: &AppContext, result: &LoadResult, args: &LoadArgs) -> Result<()> {
    let output = build_robot_payload(result, args);
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

pub(crate) fn build_robot_payload(result: &LoadResult, args: &LoadArgs) -> serde_json::Value {
    let disclosed = &result.disclosed;

    let pack_info = if let Some(tokens) = args.pack {
        serde_json::json!({
            "budget": tokens,
            "mode": format!("{:?}", args.mode),
            "contract": args.contract.map(|c| format!("{:?}", c)),
            "contract_id": args.contract_id.clone(),
        })
    } else {
        serde_json::Value::Null
    };

    serde_json::json!({
        "status": "ok",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "version": env!("CARGO_PKG_VERSION"),
        "data": {
            "skill_id": result.skill_id,
            "name": result.name,
            "disclosure_level": disclosed.level.name(),
            "token_count": disclosed.token_estimate,
            "pack": pack_info,
            "content": disclosed.body,
            "frontmatter": {
                "id": disclosed.frontmatter.id,
                "name": disclosed.frontmatter.name,
                "version": disclosed.frontmatter.version,
                "description": disclosed.frontmatter.description,
                "tags": disclosed.frontmatter.tags,
                "requires": disclosed.frontmatter.requires,
            },
            "dependencies_loaded": result.dependencies_loaded,
            "slices_included": result.slices_included,
            "scripts": disclosed.scripts.iter().map(|s| {
                serde_json::json!({
                    "path": s.path.to_string_lossy(),
                    "language": s.language,
                })
            }).collect::<Vec<_>>(),
            "references": disclosed.references.iter().map(|r| {
                serde_json::json!({
                    "path": r.path.to_string_lossy(),
                    "file_type": r.file_type,
                })
            }).collect::<Vec<_>>(),
        },
        "warnings": []
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_determine_disclosure_plan_default() {
        let args = LoadArgs {
            skill: "test".to_string(),
            level: None,
            pack: None,
            mode: CliPackMode::Balanced,
            contract: None,
            contract_id: None,
            max_per_group: 2,
            full: false,
            complete: false,
            deps: DepsMode::Auto,
            experiment_id: None,
            variant_id: None,
        };

        match determine_disclosure_plan(&args, None) {
            DisclosurePlan::Level(level) => {
                assert_eq!(level, DisclosureLevel::Standard);
            }
            _ => panic!("Expected Level plan"),
        }
    }

    #[test]
    fn test_determine_disclosure_plan_full_flag() {
        let args = LoadArgs {
            skill: "test".to_string(),
            level: None,
            pack: None,
            mode: CliPackMode::Balanced,
            contract: None,
            contract_id: None,
            max_per_group: 2,
            full: true,
            complete: false,
            deps: DepsMode::Auto,
            experiment_id: None,
            variant_id: None,
        };

        match determine_disclosure_plan(&args, None) {
            DisclosurePlan::Level(level) => {
                assert_eq!(level, DisclosureLevel::Full);
            }
            _ => panic!("Expected Level plan"),
        }
    }

    #[test]
    fn test_determine_disclosure_plan_complete_flag() {
        let args = LoadArgs {
            skill: "test".to_string(),
            level: None,
            pack: None,
            mode: CliPackMode::Balanced,
            contract: None,
            contract_id: None,
            max_per_group: 2,
            full: false,
            complete: true,
            deps: DepsMode::Auto,
            experiment_id: None,
            variant_id: None,
        };

        match determine_disclosure_plan(&args, None) {
            DisclosurePlan::Level(level) => {
                assert_eq!(level, DisclosureLevel::Complete);
            }
            _ => panic!("Expected Level plan"),
        }
    }

    #[test]
    fn test_determine_disclosure_plan_pack_budget() {
        let args = LoadArgs {
            skill: "test".to_string(),
            level: None,
            pack: Some(800),
            mode: CliPackMode::UtilityFirst,
            contract: Some(CliPackContract::Debug),
            contract_id: None,
            max_per_group: 3,
            full: true, // Should be ignored when pack is set
            complete: false,
            deps: DepsMode::Auto,
            experiment_id: None,
            variant_id: None,
        };

        let contract = args.contract.map(|c| c.preset().contract());
        match determine_disclosure_plan(&args, contract) {
            DisclosurePlan::Pack(budget) => {
                assert_eq!(budget.tokens, 800);
                assert_eq!(budget.mode, PackMode::UtilityFirst);
                assert_eq!(budget.max_per_group, 3);
                assert_eq!(
                    budget.contract.as_ref().map(|c| c.id.as_str()),
                    Some("debug")
                );
            }
            _ => panic!("Expected Pack plan"),
        }
    }

    #[test]
    fn test_determine_disclosure_plan_explicit_level() {
        let args = LoadArgs {
            skill: "test".to_string(),
            level: Some("overview".to_string()),
            pack: None,
            mode: CliPackMode::Balanced,
            contract: None,
            contract_id: None,
            max_per_group: 2,
            full: false,
            complete: false,
            deps: DepsMode::Auto,
            experiment_id: None,
            variant_id: None,
        };

        match determine_disclosure_plan(&args, None) {
            DisclosurePlan::Level(level) => {
                assert_eq!(level, DisclosureLevel::Overview);
            }
            _ => panic!("Expected Level plan"),
        }
    }

    #[test]
    fn test_determine_disclosure_plan_numeric_level() {
        let args = LoadArgs {
            skill: "test".to_string(),
            level: Some("1".to_string()),
            pack: None,
            mode: CliPackMode::Balanced,
            contract: None,
            contract_id: None,
            max_per_group: 2,
            full: false,
            complete: false,
            deps: DepsMode::Auto,
            experiment_id: None,
            variant_id: None,
        };

        match determine_disclosure_plan(&args, None) {
            DisclosurePlan::Level(level) => {
                assert_eq!(level, DisclosureLevel::Overview);
            }
            _ => panic!("Expected Level plan"),
        }
    }

    #[test]
    fn test_deps_mode_conversion() {
        assert!(matches!(
            DependencyLoadMode::from(DepsMode::Auto),
            DependencyLoadMode::Auto
        ));
        assert!(matches!(
            DependencyLoadMode::from(DepsMode::Off),
            DependencyLoadMode::Off
        ));
        assert!(matches!(
            DependencyLoadMode::from(DepsMode::Full),
            DependencyLoadMode::Full
        ));
    }

    #[test]
    fn test_cli_pack_mode_conversion() {
        assert!(matches!(
            PackMode::from(CliPackMode::Balanced),
            PackMode::Balanced
        ));
        assert!(matches!(
            PackMode::from(CliPackMode::UtilityFirst),
            PackMode::UtilityFirst
        ));
        assert!(matches!(
            PackMode::from(CliPackMode::CoverageFirst),
            PackMode::CoverageFirst
        ));
        assert!(matches!(
            PackMode::from(CliPackMode::PitfallSafe),
            PackMode::PitfallSafe
        ));
    }

    // ==================== Argument Parsing Tests ====================

    #[test]
    fn test_load_args_minimal() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            args: LoadArgs,
        }

        let cli = TestCli::parse_from(["test", "my-skill"]);
        assert_eq!(cli.args.skill, "my-skill");
        assert!(cli.args.level.is_none());
        assert!(cli.args.pack.is_none());
        assert!(!cli.args.full);
        assert!(!cli.args.complete);
        assert!(cli.args.experiment_id.is_none());
        assert!(cli.args.variant_id.is_none());
    }

    #[test]
    fn test_load_args_with_level_short() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            args: LoadArgs,
        }

        let cli = TestCli::parse_from(["test", "skill", "-l", "full"]);
        assert_eq!(cli.args.level, Some("full".to_string()));
    }

    #[test]
    fn test_load_args_with_level_long() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            args: LoadArgs,
        }

        let cli = TestCli::parse_from(["test", "skill", "--level", "minimal"]);
        assert_eq!(cli.args.level, Some("minimal".to_string()));
    }

    #[test]
    fn test_load_args_with_pack() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            args: LoadArgs,
        }

        let cli = TestCli::parse_from(["test", "skill", "--pack", "500"]);
        assert_eq!(cli.args.pack, Some(500));
    }

    #[test]
    fn test_load_args_with_pack_mode() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            args: LoadArgs,
        }

        let cli =
            TestCli::parse_from(["test", "skill", "--pack", "1000", "--mode", "utility-first"]);
        assert_eq!(cli.args.pack, Some(1000));
        assert!(matches!(cli.args.mode, CliPackMode::UtilityFirst));
    }

    #[test]
    fn test_load_args_max_per_group_default() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            args: LoadArgs,
        }

        let cli = TestCli::parse_from(["test", "skill"]);
        assert_eq!(cli.args.max_per_group, 2);
    }

    #[test]
    fn test_load_args_max_per_group_custom() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            args: LoadArgs,
        }

        let cli = TestCli::parse_from(["test", "skill", "--max-per-group", "5"]);
        assert_eq!(cli.args.max_per_group, 5);
    }

    #[test]
    fn test_load_args_full_flag() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            args: LoadArgs,
        }

        let cli = TestCli::parse_from(["test", "skill", "--full"]);
        assert!(cli.args.full);
        assert!(!cli.args.complete);
    }

    #[test]
    fn test_load_args_complete_flag() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            args: LoadArgs,
        }

        let cli = TestCli::parse_from(["test", "skill", "--complete"]);
        assert!(cli.args.complete);
        assert!(!cli.args.full);
    }

    #[test]
    fn test_load_args_deps_modes() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            args: LoadArgs,
        }

        // Default is auto
        let cli = TestCli::parse_from(["test", "skill"]);
        assert!(matches!(cli.args.deps, DepsMode::Auto));

        // Off
        let cli = TestCli::parse_from(["test", "skill", "--deps", "off"]);
        assert!(matches!(cli.args.deps, DepsMode::Off));

        // Full
        let cli = TestCli::parse_from(["test", "skill", "--deps", "full"]);
        assert!(matches!(cli.args.deps, DepsMode::Full));
    }

    #[test]
    fn test_load_args_all_pack_modes() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            args: LoadArgs,
        }

        for (name, expected_mode) in [
            ("balanced", CliPackMode::Balanced),
            ("utility-first", CliPackMode::UtilityFirst),
            ("coverage-first", CliPackMode::CoverageFirst),
            ("pitfall-safe", CliPackMode::PitfallSafe),
        ] {
            let cli = TestCli::parse_from(["test", "skill", "--mode", name]);
            assert!(
                std::mem::discriminant(&cli.args.mode) == std::mem::discriminant(&expected_mode),
                "Expected mode {} to match",
                name
            );
        }
    }

    #[test]
    fn test_load_args_combined() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            args: LoadArgs,
        }

        let cli = TestCli::parse_from([
            "test",
            "my-skill",
            "--pack",
            "750",
            "--mode",
            "coverage-first",
            "--contract",
            "debug",
            "--max-per-group",
            "3",
            "--deps",
            "off",
        ]);

        assert_eq!(cli.args.skill, "my-skill");
        assert_eq!(cli.args.pack, Some(750));
        assert!(matches!(cli.args.mode, CliPackMode::CoverageFirst));
        assert!(matches!(cli.args.contract, Some(CliPackContract::Debug)));
        assert_eq!(cli.args.max_per_group, 3);
        assert!(matches!(cli.args.deps, DepsMode::Off));
    }

    // ==================== Disclosure Plan Edge Cases ====================

    #[test]
    fn test_determine_disclosure_plan_invalid_level_falls_back() {
        let args = LoadArgs {
            skill: "test".to_string(),
            level: Some("invalid".to_string()),
            pack: None,
            mode: CliPackMode::Balanced,
            contract: None,
            contract_id: None,
            max_per_group: 2,
            full: false,
            complete: false,
            deps: DepsMode::Auto,
            experiment_id: None,
            variant_id: None,
        };

        // Invalid level should fall through to Standard
        match determine_disclosure_plan(&args, None) {
            DisclosurePlan::Level(level) => {
                assert_eq!(level, DisclosureLevel::Standard);
            }
            _ => panic!("Expected Level plan"),
        }
    }

    #[test]
    fn test_determine_disclosure_plan_complete_overrides_level() {
        let args = LoadArgs {
            skill: "test".to_string(),
            level: Some("minimal".to_string()),
            pack: None,
            mode: CliPackMode::Balanced,
            contract: None,
            contract_id: None,
            max_per_group: 2,
            full: false,
            complete: true, // Should override level
            deps: DepsMode::Auto,
            experiment_id: None,
            variant_id: None,
        };

        match determine_disclosure_plan(&args, None) {
            DisclosurePlan::Level(level) => {
                assert_eq!(level, DisclosureLevel::Complete);
            }
            _ => panic!("Expected Level plan"),
        }
    }

    #[test]
    fn test_determine_disclosure_plan_pack_overrides_all() {
        let args = LoadArgs {
            skill: "test".to_string(),
            level: Some("full".to_string()),
            pack: Some(100),
            mode: CliPackMode::Balanced,
            contract: None,
            contract_id: None,
            max_per_group: 2,
            full: true,
            complete: true, // Pack should override all flags
            deps: DepsMode::Auto,
            experiment_id: None,
            variant_id: None,
        };

        match determine_disclosure_plan(&args, None) {
            DisclosurePlan::Pack(budget) => {
                assert_eq!(budget.tokens, 100);
            }
            DisclosurePlan::Level(_) => panic!("Expected Pack plan, not Level"),
        }
    }

    #[test]
    fn test_determine_disclosure_plan_level_0() {
        let args = LoadArgs {
            skill: "test".to_string(),
            level: Some("0".to_string()),
            pack: None,
            mode: CliPackMode::Balanced,
            contract: None,
            contract_id: None,
            max_per_group: 2,
            full: false,
            complete: false,
            deps: DepsMode::Auto,
            experiment_id: None,
            variant_id: None,
        };

        match determine_disclosure_plan(&args, None) {
            DisclosurePlan::Level(level) => {
                assert_eq!(level, DisclosureLevel::Minimal);
            }
            _ => panic!("Expected Level plan"),
        }
    }

    #[test]
    fn test_determine_disclosure_plan_level_4() {
        let args = LoadArgs {
            skill: "test".to_string(),
            level: Some("4".to_string()),
            pack: None,
            mode: CliPackMode::Balanced,
            contract: None,
            contract_id: None,
            max_per_group: 2,
            full: false,
            complete: false,
            deps: DepsMode::Auto,
            experiment_id: None,
            variant_id: None,
        };

        match determine_disclosure_plan(&args, None) {
            DisclosurePlan::Level(level) => {
                assert_eq!(level, DisclosureLevel::Complete);
            }
            _ => panic!("Expected Level plan"),
        }
    }

    // ==================== LoadResult Tests ====================

    #[test]
    fn test_load_result_struct() {
        use crate::core::disclosure::{DisclosedContent, DisclosedFrontmatter};

        let result = LoadResult {
            skill_id: "test-skill".to_string(),
            name: "Test Skill".to_string(),
            disclosed: DisclosedContent {
                level: DisclosureLevel::Standard,
                frontmatter: DisclosedFrontmatter {
                    id: "test-skill".to_string(),
                    name: "Test Skill".to_string(),
                    version: "1.0.0".to_string(),
                    description: "A test".to_string(),
                    tags: vec![],
                    requires: vec![],
                },
                body: Some("Body content".to_string()),
                scripts: vec![],
                references: vec![],
                token_estimate: 100,
                slices_included: None,
            },
            dependencies_loaded: vec!["dep1".to_string()],
            slices_included: None,
        };

        assert_eq!(result.skill_id, "test-skill");
        assert_eq!(result.name, "Test Skill");
        assert_eq!(result.dependencies_loaded.len(), 1);
        assert!(result.slices_included.is_none());
    }

    // ==================== DepsMode Tests ====================

    #[test]
    fn test_deps_mode_default() {
        let mode = DepsMode::default();
        assert!(matches!(mode, DepsMode::Auto));
    }

    // ==================== CliPackMode Tests ====================

    #[test]
    fn test_cli_pack_mode_default() {
        let mode = CliPackMode::default();
        assert!(matches!(mode, CliPackMode::Balanced));
    }
}
