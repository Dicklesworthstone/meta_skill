//! Meta-skill CLI commands.

use std::collections::HashSet;
use std::path::PathBuf;

use clap::{Args, Subcommand};
use colored::Colorize;

use crate::app::AppContext;
use crate::cli::output::{OutputFormat, emit_json};
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
    /// Self-mine CASS sessions and usage logs to update the 'ms' meta-skill
    Update(UpdateArgs),
    /// Validate the 'ms' skill and all meta-skill definitions
    Validate(ValidateArgs),
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

#[derive(Args, Debug)]
pub struct UpdateArgs {
    /// Maximum number of CASS sessions to mine
    #[arg(long, default_value = "10")]
    pub max_sessions: usize,

    /// Preview updates without writing
    #[arg(long)]
    pub dry_run: bool,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct ValidateArgs {
    /// Optional meta-skill ID to validate (defaults to 'ms' and all registered meta-skills)
    pub id: Option<String>,

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
        MetaCommand::Update(update_args) => run_update(ctx, update_args),
        MetaCommand::Validate(validate_args) => run_validate(ctx, validate_args),
    }
}

fn emit_meta_output<T: serde::Serialize>(
    ctx: &AppContext,
    json_flag: bool,
    payload: &T,
) -> Result<()> {
    if ctx.output_format != OutputFormat::Human {
        emit_json(payload)
    } else if json_flag {
        println!("{}", serde_json::to_string_pretty(payload)?);
        Ok(())
    } else {
        Ok(())
    }
}

fn run_list(ctx: &AppContext, args: &ListArgs) -> Result<()> {
    let mut registry = MetaSkillRegistry::new();
    let is_robot = args.json || ctx.output_format != OutputFormat::Human;

    // Load meta-skills from configured paths
    let meta_skill_paths = get_meta_skill_paths(ctx);
    let count = registry.load_from_paths(&meta_skill_paths)?;

    if count == 0 {
        if is_robot {
            let empty: Vec<serde_json::Value> = Vec::new();
            return emit_meta_output(ctx, args.json, &empty);
        }
        println!("No meta-skills found.");
        println!("\nMeta-skills are stored as .toml files in:");
        for path in &meta_skill_paths {
            println!("  - {}", path.display());
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

    if is_robot {
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
        return emit_meta_output(ctx, args.json, &json_results);
    }

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

    Ok(())
}

fn run_show(ctx: &AppContext, args: &ShowArgs) -> Result<()> {
    let mut registry = MetaSkillRegistry::new();
    let meta_skill_paths = get_meta_skill_paths(ctx);
    registry.load_from_paths(&meta_skill_paths)?;

    let meta_skill = registry
        .get(&args.id)
        .ok_or_else(|| crate::error::MsError::NotFound(format!("meta-skill: {}", args.id)))?;

    if args.json || ctx.output_format != OutputFormat::Human {
        return emit_meta_output(ctx, args.json, meta_skill);
    }

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

    if args.json || ctx.output_format != OutputFormat::Human {
        let json_output = serde_json::json!({
            "meta_skill_id": result.meta_skill_id,
            "tokens_used": result.tokens_used,
            "budget": args.budget,
            "slices_loaded": result.slices.len(),
            "slices_skipped": result.skipped.len(),
            "content": result.packed_content,
        });
        return emit_meta_output(ctx, args.json, &json_output);
    }

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

    if args.json || ctx.output_format != OutputFormat::Human {
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
        return emit_meta_output(ctx, args.json, &json_results);
    }
    if results.is_empty() {
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
    let is_robot = ctx.output_format != OutputFormat::Human;
    if ctx.git.skill_exists("ms") {
        if is_robot {
            return emit_json(&serde_json::json!({
                "status": "ok",
                "skill_id": "ms",
                "created": false,
                "message": "Meta skill 'ms' already exists."
            }));
        }
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

    let scorer = crate::quality::QualityScorer::with_defaults();
    let quality = scorer.score_spec(&spec, &crate::quality::QualityContext::default());
    let _ = ctx
        .db
        .update_skill_quality("ms", f64::from(quality.overall));
    if let Ok(Some(record)) = ctx.db.get_skill("ms") {
        let _ = ctx.search.index_skill(&record);
        let _ = ctx.search.commit();
    }

    if is_robot {
        return emit_json(&serde_json::json!({
            "status": "ok",
            "skill_id": "ms",
            "created": true,
            "quality_score": quality.overall,
            "message": "Bootstrapped meta skill 'ms'."
        }));
    }

    println!("Bootstrapped meta skill 'ms'.");
    Ok(())
}

fn run_update(ctx: &AppContext, args: &UpdateArgs) -> Result<()> {
    // Ensure the 'ms' skill exists first
    if !ctx.git.skill_exists("ms") {
        run_bootstrap(ctx)?;
    }

    let mut spec = ctx.git.read_skill("ms")?;
    let mut mined_patterns: Vec<String> = Vec::new();
    let mut sessions_examined = 0usize;

    // 1. Try mining CASS sessions for `ms` CLI usage patterns
    let client = match &ctx.config.cass.cass_path {
        Some(path) => crate::cass::CassClient::with_binary(std::path::PathBuf::from(path)),
        None => crate::cass::CassClient::new(),
    };
    if let Ok(matches) = client.search("ms search OR ms load OR ms suggest", args.max_sessions) {
        for m in matches.iter().take(args.max_sessions) {
            if let Ok(session) = client.get_session(&m.session_id) {
                sessions_examined += 1;
                if let Ok(patterns) = crate::cass::mining::extract_from_session(&session) {
                    for pat in &patterns {
                        if let crate::cass::mining::PatternType::CommandPattern {
                            commands, ..
                        } = &pat.pattern_type
                        {
                            for cmd in commands {
                                if cmd.contains("ms ") && mined_patterns.len() < 10 {
                                    let snippet =
                                        cmd.lines().next().unwrap_or("").trim().to_string();
                                    if !snippet.is_empty() && !mined_patterns.contains(&snippet) {
                                        mined_patterns.push(snippet);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Also incorporate top frequently loaded skills from local usage telemetry
    if let Ok((_total_loads, per_skill_counts, _last_loaded)) = ctx.db.get_skill_usage_stats() {
        let mut usage_pairs: Vec<(String, u64)> = per_skill_counts
            .into_iter()
            .filter(|(_, count)| *count > 0)
            .collect();
        usage_pairs.sort_by(|a, b| b.1.cmp(&a.1));
        for (skill_id, count) in usage_pairs.into_iter().take(3) {
            mined_patterns.push(format!("ms load {skill_id} # loaded {count} time(s)"));
        }
    }

    let mut updated = false;
    if !mined_patterns.is_empty() {
        let section_id = "mined-usage-patterns";
        let content_block = format!(
            "Observed high-signal `ms` workflow commands:\n```bash\n{}\n```",
            mined_patterns.join("\n")
        );
        if let Some(sec) = spec.sections.iter_mut().find(|s| s.id == section_id) {
            sec.blocks = vec![crate::core::SkillBlock {
                id: "mined-commands".to_string(),
                block_type: crate::core::BlockType::Command,
                content: content_block,
            }];
        } else {
            spec.sections.push(crate::core::SkillSection {
                id: section_id.to_string(),
                title: "Mined Usage Patterns".to_string(),
                blocks: vec![crate::core::SkillBlock {
                    id: "mined-commands".to_string(),
                    block_type: crate::core::BlockType::Command,
                    content: content_block,
                }],
            });
        }
        updated = true;
    }

    if updated && !args.dry_run {
        let tx_mgr =
            crate::storage::TxManager::new(ctx.db.clone(), ctx.git.clone(), ctx.ms_root.clone())?;
        tx_mgr.write_skill_with_layer(&spec, crate::core::SkillLayer::Base)?;
        if let Ok(Some(record)) = ctx.db.get_skill("ms") {
            let _ = ctx.search.index_skill(&record);
            let _ = ctx.search.commit();
        }
    }

    let payload = serde_json::json!({
        "status": "ok",
        "skill_id": "ms",
        "sessions_examined": sessions_examined,
        "patterns_added": mined_patterns.len(),
        "patterns": mined_patterns,
        "updated": updated && !args.dry_run,
        "dry_run": args.dry_run,
    });

    if args.json || ctx.output_format != OutputFormat::Human {
        return emit_meta_output(ctx, args.json, &payload);
    }

    println!(
        "{} Meta skill 'ms' update complete (sessions: {}, patterns: {}, dry_run: {})",
        "✓".green().bold(),
        sessions_examined,
        mined_patterns.len(),
        args.dry_run
    );
    Ok(())
}

fn run_validate(ctx: &AppContext, args: &ValidateArgs) -> Result<()> {
    let mut issues: Vec<String> = Vec::new();
    let mut ms_skill_valid = false;
    let mut ms_quality_score = 0.0_f32;
    let mut roundtrip_ok = false;

    // 1. Validate the self-referential 'ms' skill if present (or if specifically requested)
    if args.id.as_deref().unwrap_or("ms") == "ms" {
        if ctx.git.skill_exists("ms") {
            let spec = ctx.git.read_skill("ms")?;
            roundtrip_ok = crate::core::spec_lens::SpecLens
                .verify_roundtrip(&spec)
                .is_ok();
            if !roundtrip_ok {
                issues.push("Meta skill 'ms' failed markdown roundtrip verification".to_string());
            }
            let scorer = crate::quality::QualityScorer::with_defaults();
            let q = scorer.score_spec(&spec, &crate::quality::QualityContext::default());
            ms_quality_score = q.overall;
            ms_skill_valid = roundtrip_ok && !spec.sections.is_empty();
        } else if args.id.as_deref() == Some("ms") {
            return Err(crate::error::MsError::SkillNotFound(
                "Meta skill 'ms' is not bootstrapped yet (run `ms meta bootstrap`)".to_string(),
            ));
        }
    }

    // 2. Validate TOML meta-skills in MetaSkillRegistry
    let mut registry = MetaSkillRegistry::new();
    let meta_skill_paths = get_meta_skill_paths(ctx);
    let loaded_count = registry.load_from_paths(&meta_skill_paths)?;
    let mut validated_toml = Vec::new();

    let query = MetaSkillQuery {
        text: None,
        tags: Vec::new(),
        tech_stack: None,
    };
    for ms in registry.search(&query) {
        if let Some(ref target_id) = args.id {
            if target_id != "ms" && &ms.id != target_id {
                continue;
            }
        }
        let mut slice_errors = Vec::new();
        for slice_ref in &ms.slices {
            if slice_ref.required && ctx.db.get_skill(&slice_ref.skill_id)?.is_none() {
                let msg = format!(
                    "Meta-skill '{}' references missing required skill '{}'",
                    ms.id, slice_ref.skill_id
                );
                slice_errors.push(msg.clone());
                issues.push(msg);
            }
        }
        validated_toml.push(serde_json::json!({
            "id": ms.id,
            "slice_count": ms.slices.len(),
            "valid": slice_errors.is_empty(),
            "errors": slice_errors,
        }));
    }

    let ok = issues.is_empty();
    let payload = serde_json::json!({
        "status": if ok { "ok" } else { "invalid" },
        "ok": ok,
        "ms_skill_bootstrapped": ctx.git.skill_exists("ms"),
        "ms_skill_valid": ms_skill_valid,
        "ms_roundtrip_ok": roundtrip_ok,
        "ms_quality_score": ms_quality_score,
        "toml_meta_skills_loaded": loaded_count,
        "toml_meta_skills": validated_toml,
        "issues": issues,
    });

    if args.json || ctx.output_format != OutputFormat::Human {
        return emit_meta_output(ctx, args.json, &payload);
    }

    if ok {
        println!(
            "{} Meta-skill validation passed (ms_bootstrapped={}, toml_meta_skills={}, quality={:.2})",
            "✓".green().bold(),
            ctx.git.skill_exists("ms"),
            loaded_count,
            ms_quality_score
        );
    } else {
        println!(
            "{} Meta-skill validation found issues:",
            "!".yellow().bold()
        );
        for issue in &issues {
            println!("  - {issue}");
        }
    }
    Ok(())
}

fn get_meta_skill_paths(ctx: &AppContext) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // Active ms_root meta-skills directory
    let root_meta = ctx.ms_root.join("meta-skills");
    if root_meta.exists() {
        paths.push(root_meta);
    }

    // Project meta-skills directory
    let working_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let project_meta = working_dir.join(".ms").join("meta-skills");
    if project_meta.exists() && !paths.contains(&project_meta) {
        paths.push(project_meta);
    }

    // Global meta-skills directory
    if let Some(home) = dirs::home_dir() {
        let global_meta = home.join(".ms").join("meta-skills");
        if global_meta.exists() && !paths.contains(&global_meta) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: MetaCommand,
    }

    #[test]
    fn test_parse_meta_update_and_validate() {
        let u = TestCli::parse_from(["test", "update", "--max-sessions", "5", "--dry-run"]);
        assert!(matches!(
            u.cmd,
            MetaCommand::Update(UpdateArgs {
                max_sessions: 5,
                dry_run: true,
                ..
            })
        ));

        let v = TestCli::parse_from(["test", "validate", "ms", "--json"]);
        if let MetaCommand::Validate(args) = v.cmd {
            assert_eq!(args.id.as_deref(), Some("ms"));
            assert!(args.json);
        } else {
            panic!("Expected Validate subcommand");
        }
    }
}
