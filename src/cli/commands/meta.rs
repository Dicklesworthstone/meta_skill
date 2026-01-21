//! Meta-skill CLI commands.

use std::collections::HashSet;
use std::path::PathBuf;

use clap::{Args, Subcommand};
use colored::Colorize;

use crate::app::AppContext;
use crate::error::Result;
use crate::meta_skills::{ConditionContext, MetaSkillManager, MetaSkillQuery, MetaSkillRegistry};
use crate::utils::format::truncate_string;

#[derive(Args, Debug)]
pub struct MetaArgs {
    #[command(subcommand)]
    pub command: MetaCommand,
}

#[derive(Subcommand, Debug)]
pub enum MetaCommand {
    /// List available meta-skills
    List(ListArgs),
    /// Show details of a meta-skill
    Show(ShowArgs),
    /// Load a meta-skill (resolve and pack slices)
    Load(LoadArgs),
    /// Search for meta-skills
    Search(SearchArgs),
    /// Bootstrap the self-referential 'ms' skill
    Bootstrap,
}

#[derive(Args, Debug)]
pub struct ListArgs {
    /// Filter by tag
    #[arg(long)]
    pub tag: Option<String>,

    /// Filter by tech stack
    #[arg(long)]
    pub tech_stack: Option<String>,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct ShowArgs {
    /// Meta-skill ID to show
    pub id: String,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct LoadArgs {
    /// Meta-skill ID to load
    pub id: String,

    /// Token budget for packing
    #[arg(long, default_value = "4000")]
    pub budget: usize,

    /// Tech stacks to consider for conditions
    #[arg(long, value_delimiter = ',')]
    pub tech_stacks: Vec<String>,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct SearchArgs {
    /// Search query
    pub query: String,

    /// Filter by tag
    #[arg(long)]
    pub tag: Option<String>,

    /// Filter by tech stack
    #[arg(long)]
    pub tech_stack: Option<String>,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

pub fn run(ctx: &AppContext, args: &MetaArgs) -> Result<()> {
    match &args.command {
        MetaCommand::List(list_args) => run_list(ctx, list_args),
        MetaCommand::Show(show_args) => run_show(ctx, show_args),
        MetaCommand::Load(load_args) => run_load(ctx, load_args),
        MetaCommand::Search(search_args) => run_search(ctx, search_args),
        MetaCommand::Bootstrap => run_bootstrap(ctx),
    }
}

fn run_list(ctx: &AppContext, args: &ListArgs) -> Result<()> {
    let mut registry = MetaSkillRegistry::new();

    // Load meta-skills from configured paths
    let meta_skill_paths = get_meta_skill_paths(ctx);
    let count = registry.load_from_paths(&meta_skill_paths)?;

    if count == 0 {
        if args.json {
            println!("[]");
        } else {
            println!("No meta-skills found.");
            println!("\nMeta-skills are stored as .toml files in:");
            for path in &meta_skill_paths {
                println!("  - {}", path.display());
            }
        }
        return Ok(());
    }

    // Build query from filters
    let query = MetaSkillQuery {
        text: None,
        tags: args.tag.iter().cloned().collect(),
        tech_stack: args.tech_stack.clone(),
    };

    let results = registry.search(&query);

    if args.json {
        let json_results: Vec<_> = results
            .iter()
            .map(|ms| {
                serde_json::json!({
                    "id": ms.id,
                    "name": ms.name,
                    "description": ms.description,
                    "slice_count": ms.slices.len(),
                    "tags": ms.metadata.tags,
                    "tech_stacks": ms.metadata.tech_stacks,
                    "version": ms.metadata.version,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json_results)?);
    } else {
        println!(
            "{} meta-skill(s) found:\n",
            results.len().to_string().bold()
        );
        for ms in results {
            println!(
                "  {} {}",
                ms.id.cyan().bold(),
                format!("({})", ms.name).dimmed()
            );
            println!("    {}", truncate(&ms.description, 60).dimmed());
            println!(
                "    Slices: {} | Tags: {}",
                ms.slices.len(),
                ms.metadata.tags.join(", ")
            );
            println!();
        }
    }

    Ok(())
}

fn run_show(ctx: &AppContext, args: &ShowArgs) -> Result<()> {
    let mut registry = MetaSkillRegistry::new();
    let meta_skill_paths = get_meta_skill_paths(ctx);
    registry.load_from_paths(&meta_skill_paths)?;

    let meta_skill = registry
        .get(&args.id)
        .ok_or_else(|| crate::error::MsError::NotFound(format!("meta-skill: {}", args.id)))?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(meta_skill)?);
    } else {
        println!("{}", meta_skill.name.bold().cyan());
        println!("{}\n", "=".repeat(meta_skill.name.len()));
        println!("{}\n", meta_skill.description);

        println!("{}", "Metadata".bold());
        println!("  ID: {}", meta_skill.id);
        println!("  Version: {}", meta_skill.metadata.version);
        if let Some(author) = &meta_skill.metadata.author {
            println!("  Author: {author}");
        }
        println!("  Tags: {}", meta_skill.metadata.tags.join(", "));
        println!(
            "  Tech Stacks: {}",
            meta_skill.metadata.tech_stacks.join(", ")
        );
        println!();

        println!("{}", "Token Requirements".bold());
        println!("  Minimum: {} tokens", meta_skill.min_context_tokens);
        println!(
            "  Recommended: {} tokens",
            meta_skill.recommended_context_tokens
        );
        println!();

        println!("{} ({} total)", "Slices".bold(), meta_skill.slices.len());
        for (i, slice_ref) in meta_skill.slices.iter().enumerate() {
            let required_badge = if slice_ref.required {
                " [REQUIRED]".red().to_string()
            } else {
                String::new()
            };
            println!(
                "  {}. {} (priority: {}){}",
                i + 1,
                slice_ref.skill_id.cyan(),
                slice_ref.priority,
                required_badge
            );
            if !slice_ref.slice_ids.is_empty() {
                println!("     Slices: {}", slice_ref.slice_ids.join(", "));
            }
            if !slice_ref.conditions.is_empty() {
                println!("     Conditions: {} rule(s)", slice_ref.conditions.len());
            }
        }
    }

    Ok(())
}

fn run_load(ctx: &AppContext, args: &LoadArgs) -> Result<()> {
    let mut registry = MetaSkillRegistry::new();
    let meta_skill_paths = get_meta_skill_paths(ctx);
    registry.load_from_paths(&meta_skill_paths)?;

    let meta_skill = registry
        .get(&args.id)
        .ok_or_else(|| crate::error::MsError::NotFound(format!("meta-skill: {}", args.id)))?
        .clone();

    let manager = MetaSkillManager::new(ctx);

    let working_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    // Detect tech stacks if not provided
    let tech_stacks = if args.tech_stacks.is_empty() {
        detect_tech_stacks(&working_dir)
    } else {
        args.tech_stacks.clone()
    };

    let condition_ctx = ConditionContext {
        working_dir: &working_dir,
        tech_stacks: &tech_stacks,
        loaded_slices: &HashSet::new(),
    };

    let result = manager.load(&meta_skill, args.budget, &condition_ctx)?;

    if args.json {
        let json_output = serde_json::json!({
            "meta_skill_id": result.meta_skill_id,
            "tokens_used": result.tokens_used,
            "budget": args.budget,
            "slices_loaded": result.slices.len(),
            "slices_skipped": result.skipped.len(),
            "content": result.packed_content,
        });
        println!("{}", serde_json::to_string_pretty(&json_output)?);
    } else {
        println!(
            "{} Loaded meta-skill: {}\n",
            "SUCCESS".green().bold(),
            result.meta_skill_id.cyan()
        );
        println!(
            "Tokens: {} / {} ({:.1}% of budget)",
            result.tokens_used,
            args.budget,
            (result.tokens_used as f64 / args.budget as f64) * 100.0
        );
        println!("Slices loaded: {}", result.slices.len());

        if !result.skipped.is_empty() {
            println!(
                "\n{} ({}):",
                "Skipped slices".yellow(),
                result.skipped.len()
            );
            for skip in &result.skipped {
                let slice_info = skip
                    .slice_id
                    .as_ref()
                    .map(|s| format!(":{s}"))
                    .unwrap_or_default();
                println!("  - {}{}: {:?}", skip.skill_id, slice_info, skip.reason);
            }
        }

        println!("\n{}", "---".dimmed());
        println!("{}", result.packed_content);
    }

    Ok(())
}

fn run_search(ctx: &AppContext, args: &SearchArgs) -> Result<()> {
    let mut registry = MetaSkillRegistry::new();
    let meta_skill_paths = get_meta_skill_paths(ctx);
    registry.load_from_paths(&meta_skill_paths)?;

    let query = MetaSkillQuery {
        text: Some(args.query.clone()),
        tags: args.tag.iter().cloned().collect(),
        tech_stack: args.tech_stack.clone(),
    };

    let results = registry.search(&query);

    if args.json {
        let json_results: Vec<_> = results
            .iter()
            .map(|ms| {
                serde_json::json!({
                    "id": ms.id,
                    "name": ms.name,
                    "description": ms.description,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&json_results)?);
    } else if results.is_empty() {
        println!("No meta-skills found matching '{}'", args.query);
    } else {
        println!(
            "{} result(s) for '{}':\n",
            results.len().to_string().bold(),
            args.query
        );
        for ms in results {
            println!("  {} - {}", ms.id.cyan(), ms.name);
            println!("    {}", truncate(&ms.description, 60).dimmed());
            println!();
        }
    }

    Ok(())
}

fn run_bootstrap(ctx: &AppContext) -> Result<()> {
    if ctx.git.skill_exists("ms") {
        println!("Meta skill 'ms' already exists.");
        return Ok(());
    }

    let content = r#"---
id: ms
name: Using meta_skill (ms)
version: 1.0.0
description: How to effectively use the ms CLI to search, load, suggest, and build skills.
tags: [meta, tool, cli]
requires: []
provides: [ms-cli]
platforms: [any]
author: ms-bootstrap
license: MIT
---

# Skill: Using meta_skill (ms)

## Summary
How to effectively use the ms CLI to search, load, suggest, and build skills.

## Triggers
- command: "ms"
- keyword: "skill", "skills"
- context: ".ms/", "SKILL.md"

## Instructions

### Searching for Skills
Use `ms search` to find relevant skills:
```bash
ms search "error handling rust"
ms search --tag rust --layer project
```

### Loading Skills

Use `ms load` to get skill content with appropriate disclosure:

```bash
ms load rust-error-handling
ms load rust-error-handling --level full
ms load rust-error-handling --budget 500
```

### Getting Suggestions

Use `ms suggest` for context-aware recommendations:

```bash
ms suggest  # Based on current directory
ms suggest --file src/main.rs
ms suggest --pack 800
```

### Building New Skills

Use `ms build` to mine CASS sessions for patterns:

```bash
ms build --from-sessions ./sessions/
ms build --guided
```

## Examples

### Example 1: Quick Skill Lookup

User wants to know how to handle async errors in Rust.

```bash
ms search "async error handling rust"
ms load rust-async-errors
```

### Example 2: Context-Aware Development

Working in a new Rust project, want relevant skills.

```bash
cd /data/projects/my-rust-app
ms suggest --pack 800
```

## Pitfalls

- Don't use `ms load --level full` for simple tasks (wastes tokens)
- Remember to run `ms index` after adding new skills
- Check `ms doctor` if search results seem stale

## Related Skills

- skill-authoring
- cass-mining-patterns
- token-budget-optimization

## Self-Mining Loop

```rust
pub struct MetaSkillMiner {
    cass_client: CassClient,
    skill_builder: SkillBuilder,
}

impl MetaSkillMiner {
    /// Mine ms usage sessions to improve the meta skill
    pub async fn update_meta_skill(&self) -> Result<Skill> {
        // Query for sessions involving ms CLI
        let sessions = self.cass_client.query(
            "ms AND (search OR load OR suggest OR build)"
        ).await?;

        // Extract successful usage patterns
        let patterns = self.extract_patterns(&sessions)?;

        // Update meta skill with new patterns
        self.skill_builder.enhance_skill("meta-ms", &patterns).await
    }
}
```

## Bootstrap Process

1. Initial creation: Hand-write basic meta skill from design docs
2. Dog-fooding: Use ms to develop ms
3. Pattern extraction: Mine development sessions
4. Refinement: Update meta skill with extracted patterns
5. Validation: Test updated skill produces good results
6. Repeat: Continuous improvement loop

## CLI Commands

```bash
# View the meta skill
ms show ms

# Update meta skill from recent sessions
ms meta update

# Check meta skill quality
ms meta validate

# Bootstrap meta skill (first time)
ms meta bootstrap
```
"#;

    let spec = crate::core::spec_lens::parse_markdown(content).map_err(|e| {
        crate::error::MsError::ValidationFailed(format!("Failed to parse meta skill: {e}"))
    })?;

    // Use 2PC to ensure consistency
    let tx_mgr =
        crate::storage::TxManager::new(ctx.db.clone(), ctx.git.clone(), ctx.ms_root.clone())?;

    tx_mgr.write_skill_with_layer(&spec, crate::core::SkillLayer::Base)?;

    println!("Bootstrapped meta skill 'ms'.");
    Ok(())
}

fn get_meta_skill_paths(_ctx: &AppContext) -> Vec<PathBuf> {
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

fn detect_tech_stacks(working_dir: &std::path::Path) -> Vec<String> {
    let mut stacks = Vec::new();

    // Check for common config files
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

fn truncate(s: &str, max_len: usize) -> String {
    truncate_string(s, max_len)
}
