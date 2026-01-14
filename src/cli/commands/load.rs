//! ms load - Load a skill with progressive disclosure

use clap::{Args, ValueEnum};
use colored::Colorize;

use crate::app::AppContext;
use crate::core::dependencies::{
    DependencyGraph, DependencyLoadMode, DependencyResolver,
    DisclosureLevel as DepDisclosure,
};
use crate::core::disclosure::{
    disclose, DisclosedContent, DisclosureLevel, DisclosurePlan, PackMode, TokenBudget,
};
use crate::core::skill::{SkillAssets, SkillMetadata};
use crate::core::spec_lens::parse_markdown;
use crate::error::{MsError, Result};
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
    // Resolve skill by ID or alias
    let skill = resolve_skill(ctx, &args.skill)?;

    // Determine disclosure plan
    let disclosure_plan = determine_disclosure_plan(args);

    // Parse skill body into SkillSpec
    let spec = parse_markdown(&skill.body).map_err(|e| {
        MsError::ValidationFailed(format!("failed to parse skill body: {}", e))
    })?;

    // Merge metadata from database into spec metadata
    let metadata = merge_metadata(&skill, &spec.metadata);
    let mut spec = spec;
    spec.metadata = metadata;

    // Build empty assets for now (TODO: load from assets_json)
    let assets = SkillAssets::default();

    // Apply disclosure
    let disclosed = disclose(&spec, &assets, &disclosure_plan);

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
        slices_included: match &disclosure_plan {
            DisclosurePlan::Pack(_) => Some(0), // TODO: get actual slice count from packer
            DisclosurePlan::Level(_) => None,
        },
    };

    if ctx.robot_mode {
        output_robot(ctx, &result, args)
    } else {
        output_human(ctx, &result, args)
    }
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

fn determine_disclosure_plan(args: &LoadArgs) -> DisclosurePlan {
    // Token budget takes precedence
    if let Some(tokens) = args.pack {
        return DisclosurePlan::Pack(TokenBudget {
            tokens,
            mode: args.mode.into(),
            max_per_group: args.max_per_group,
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

fn merge_metadata(skill: &SkillRecord, parsed_meta: &SkillMetadata) -> SkillMetadata {
    // Parse metadata_json from database
    let db_meta: serde_json::Value =
        serde_json::from_str(&skill.metadata_json).unwrap_or_default();

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
    let meta: serde_json::Value =
        serde_json::from_str(&skill.metadata_json).unwrap_or_default();

    let requires: Vec<String> = meta
        .get("requires")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    if requires.is_empty() {
        return Ok(vec![]);
    }

    // Build dependency graph with available skills
    let mut graph = DependencyGraph::new();

    // Add root skill
    let provides: Vec<String> = meta
        .get("provides")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    graph.add_skill(skill.id.clone(), requires.clone(), provides);

    // Try to resolve each required capability
    let mut loaded_deps = Vec::new();

    for cap in &requires {
        // Try to find a skill that provides this capability
        // For now, try direct ID match (capability might be skill ID)
        if let Some(dep_skill) = ctx.db.get_skill(cap).ok().flatten() {
            let dep_meta: serde_json::Value =
                serde_json::from_str(&dep_skill.metadata_json).unwrap_or_default();
            let dep_provides: Vec<String> = dep_meta
                .get("provides")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_else(|| vec![cap.clone()]);
            let dep_requires: Vec<String> = dep_meta
                .get("requires")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            graph.add_skill(dep_skill.id.clone(), dep_requires, dep_provides);
            loaded_deps.push(dep_skill.id.clone());
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

    // Return just the dependency IDs (not the root)
    Ok(plan
        .ordered
        .iter()
        .filter(|p| p.skill_id != skill.id)
        .map(|p| p.skill_id.clone())
        .collect())
}

fn output_human(_ctx: &AppContext, result: &LoadResult, _args: &LoadArgs) -> Result<()> {
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

fn output_robot(_ctx: &AppContext, result: &LoadResult, _args: &LoadArgs) -> Result<()> {
    let disclosed = &result.disclosed;

    let output = serde_json::json!({
        "status": "ok",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "version": env!("CARGO_PKG_VERSION"),
        "data": {
            "skill_id": result.skill_id,
            "name": result.name,
            "disclosure_level": disclosed.level.name(),
            "token_count": disclosed.token_estimate,
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
    });

    println!("{}", serde_json::to_string_pretty(&output)?);

    Ok(())
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
            max_per_group: 2,
            full: false,
            complete: false,
            deps: DepsMode::Auto,
        };

        match determine_disclosure_plan(&args) {
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
            max_per_group: 2,
            full: true,
            complete: false,
            deps: DepsMode::Auto,
        };

        match determine_disclosure_plan(&args) {
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
            max_per_group: 2,
            full: false,
            complete: true,
            deps: DepsMode::Auto,
        };

        match determine_disclosure_plan(&args) {
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
            max_per_group: 3,
            full: true, // Should be ignored when pack is set
            complete: false,
            deps: DepsMode::Auto,
        };

        match determine_disclosure_plan(&args) {
            DisclosurePlan::Pack(budget) => {
                assert_eq!(budget.tokens, 800);
                assert_eq!(budget.mode, PackMode::UtilityFirst);
                assert_eq!(budget.max_per_group, 3);
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
            max_per_group: 2,
            full: false,
            complete: false,
            deps: DepsMode::Auto,
        };

        match determine_disclosure_plan(&args) {
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
            max_per_group: 2,
            full: false,
            complete: false,
            deps: DepsMode::Auto,
        };

        match determine_disclosure_plan(&args) {
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
}
