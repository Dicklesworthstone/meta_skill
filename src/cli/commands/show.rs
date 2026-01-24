//! ms show - Show skill details

use clap::Args;

use crate::app::AppContext;
use crate::cli::output::OutputFormat;
use crate::error::{MsError, Result};
use crate::storage::sqlite::SkillRecord;

#[derive(Args, Debug)]
pub struct ShowArgs {
    /// Skill ID or name to show
    pub skill: String,

    /// Show full spec (not just summary)
    #[arg(long)]
    pub full: bool,

    /// Show metadata only
    #[arg(long)]
    pub meta: bool,

    /// Show dependency graph
    #[arg(long)]
    pub deps: bool,
}

pub fn run(ctx: &AppContext, args: &ShowArgs) -> Result<()> {
    // Try to find skill by ID or name
    let skill = ctx
        .db
        .get_skill(&args.skill)?
        .or_else(|| {
            // Try alias resolution
            ctx.db
                .resolve_alias(&args.skill)
                .ok()
                .flatten()
                .and_then(|res| ctx.db.get_skill(&res.canonical_id).ok().flatten())
        })
        .ok_or_else(|| MsError::SkillNotFound(format!("skill not found: {}", args.skill)))?;

    display_skill(ctx, &skill, args)
}

fn display_skill(ctx: &AppContext, skill: &SkillRecord, args: &ShowArgs) -> Result<()> {
    match ctx.output_format {
        OutputFormat::Human => show_human(ctx, skill, args),
        OutputFormat::Json => show_json(skill, args, true),
        OutputFormat::Jsonl => show_json(skill, args, false),
        OutputFormat::Plain => show_plain(skill),
        OutputFormat::Tsv => show_tsv(skill),
        OutputFormat::Toon => show_toon(skill, args),
    }
}

fn show_human(_ctx: &AppContext, skill: &SkillRecord, args: &ShowArgs) -> Result<()> {
    use colored::Colorize;
    // Header
    println!("{}", skill.name.bold());
    println!("{}", "═".repeat(skill.name.len()));
    println!();

    // ID and version
    println!("{}: {}", "ID".dimmed(), skill.id);
    println!(
        "{}: {}",
        "Version".dimmed(),
        skill.version.as_deref().unwrap_or("-")
    );

    // Author
    if let Some(ref author) = skill.author {
        println!("{}: {}", "Author".dimmed(), author);
    }

    // Layer
    let layer = normalize_layer(&skill.source_layer);
    let layer_colored = match layer.as_str() {
        "base" => layer.blue(),
        "org" => layer.green(),
        "project" => layer.yellow(),
        "user" => layer.magenta(),
        _ => layer.normal(),
    };
    println!("{}: {}", "Layer".dimmed(), layer_colored);

    // Source path
    println!("{}: {}", "Source".dimmed(), skill.source_path);

    // Description
    println!();
    if !skill.description.is_empty() {
        println!("{}", skill.description);
    }

    // Deprecation warning
    if skill.is_deprecated {
        println!();
        println!(
            "{} {}",
            "⚠ DEPRECATED:".red().bold(),
            skill
                .deprecation_reason
                .as_deref()
                .unwrap_or("No reason provided")
        );
    }

    // Stats
    println!();
    println!("{}", "Stats".bold());
    println!("{}", "─".repeat(40).dimmed());
    println!("{}: {}", "Tokens".dimmed(), skill.token_count);
    println!("{}: {:.2}", "Quality".dimmed(), skill.quality_score);
    println!("{}: {}", "Indexed".dimmed(), format_date(&skill.indexed_at));
    println!(
        "{}: {}",
        "Modified".dimmed(),
        format_date(&skill.modified_at)
    );

    // Git info
    if skill.git_remote.is_some() || skill.git_commit.is_some() {
        println!();
        println!("{}", "Provenance".bold());
        println!("{}", "─".repeat(40).dimmed());
        if let Some(ref remote) = skill.git_remote {
            println!("{}: {}", "Remote".dimmed(), remote);
        }
        if let Some(ref commit) = skill.git_commit {
            println!("{}: {}", "Commit".dimmed(), &commit[..commit.len().min(8)]);
        }
        let hash = &skill.content_hash;
        if !hash.is_empty() {
            println!("{}: {}", "Hash".dimmed(), &hash[..hash.len().min(16)]);
        }
    }

    // Metadata JSON (if requested)
    if args.meta || args.full {
        println!();
        println!("{}", "Metadata".bold());
        println!("{}", "─".repeat(40).dimmed());
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&skill.metadata_json) {
            if let Ok(pretty) = serde_json::to_string_pretty(&meta) {
                println!("{pretty}");
            }
        }
    }

    // Full body (if requested)
    if args.full {
        println!();
        println!("{}", "Body".bold());
        println!("{}", "─".repeat(40).dimmed());
        println!("{}", skill.body);
    }

    // Dependencies (if requested)
    if args.deps {
        println!();
        println!("{}", "Dependencies".bold());
        println!("{}", "─".repeat(40).dimmed());
        // Parse metadata for requires list
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&skill.metadata_json) {
            if let Some(requires) = meta.get("requires").and_then(|d| d.as_array()) {
                if requires.is_empty() {
                    println!("{}", "No dependencies".dimmed());
                } else {
                    for req in requires {
                        if let Some(req_str) = req.as_str() {
                            println!("  → {req_str}");
                        }
                    }
                }
            } else {
                println!("{}", "No dependencies".dimmed());
            }
        }
    }

    Ok(())
}

fn show_json(skill: &SkillRecord, args: &ShowArgs, pretty: bool) -> Result<()> {
    let mut output = serde_json::json!({
        "status": "ok",
        "skill": {
            "id": skill.id,
            "name": skill.name,
            "version": skill.version,
            "description": skill.description,
            "author": skill.author,
            "layer": skill.source_layer,
            "source_path": skill.source_path,
            "git_remote": skill.git_remote,
            "git_commit": skill.git_commit,
            "content_hash": skill.content_hash,
            "token_count": skill.token_count,
            "quality_score": skill.quality_score,
            "indexed_at": skill.indexed_at,
            "modified_at": skill.modified_at,
            "is_deprecated": skill.is_deprecated,
            "deprecation_reason": skill.deprecation_reason,
        }
    });

    if args.meta || args.full {
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&skill.metadata_json) {
            output["skill"]["metadata"] = meta;
        }
    }

    if args.full {
        output["skill"]["body"] = serde_json::Value::String(skill.body.clone());
    }

    if args.deps {
        // Parse requires from metadata
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&skill.metadata_json) {
            output["skill"]["dependencies"] = meta
                .get("requires")
                .cloned()
                .unwrap_or(serde_json::Value::Array(vec![]));
        }
    }

    if pretty {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", serde_json::to_string(&output)?);
    }

    Ok(())
}

/// Format as plain YAML-like key-value (bd-olwb spec).
///
/// Output format:
/// ```text
/// name: my-skill
/// type: tool
/// version: 1.0.0
/// description: A helpful tool...
/// tags: cli, rust
/// layer: user
/// created: 2024-01-01
/// updated: 2024-01-15
/// ---
/// [content blocks follow]
/// ```
fn show_plain(skill: &SkillRecord) -> Result<()> {
    // Extract tags from metadata
    let tags = if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&skill.metadata_json) {
        meta.get("tags")
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default()
    } else {
        String::new()
    };

    // Extract skill type from metadata
    let skill_type =
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&skill.metadata_json) {
            meta.get("type")
                .and_then(|t| t.as_str())
                .unwrap_or("skill")
                .to_string()
        } else {
            "skill".to_string()
        };

    println!("name: {}", skill.name);
    println!("type: {}", skill_type);
    println!("version: {}", skill.version.as_deref().unwrap_or("-"));

    // Description - escape newlines for single-line output
    let desc = skill.description.replace('\n', " ").replace('\r', "");
    println!("description: {}", desc);

    println!("tags: {}", tags);
    println!("layer: {}", skill.source_layer);
    println!("updated: {}", format_date(&skill.modified_at));

    // Separator before content
    println!("---");

    // Body content
    println!("{}", skill.body);

    Ok(())
}

fn show_tsv(skill: &SkillRecord) -> Result<()> {
    println!(
        "{}\t{}\t{}\t{}\t{:.2}\t{}",
        skill.id,
        skill.name,
        skill.source_layer,
        skill.version.as_deref().unwrap_or("-"),
        skill.quality_score,
        skill.is_deprecated
    );
    Ok(())
}

fn show_toon(skill: &SkillRecord, args: &ShowArgs) -> Result<()> {
    let mut output = serde_json::json!({
        "status": "ok",
        "skill": {
            "id": skill.id,
            "name": skill.name,
            "version": skill.version,
            "description": skill.description,
            "author": skill.author,
            "layer": skill.source_layer,
            "source_path": skill.source_path,
            "git_remote": skill.git_remote,
            "git_commit": skill.git_commit,
            "content_hash": skill.content_hash,
            "token_count": skill.token_count,
            "quality_score": skill.quality_score,
            "indexed_at": skill.indexed_at,
            "modified_at": skill.modified_at,
            "is_deprecated": skill.is_deprecated,
            "deprecation_reason": skill.deprecation_reason,
        }
    });

    if args.meta || args.full {
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&skill.metadata_json) {
            output["skill"]["metadata"] = meta;
        }
    }

    if args.full {
        output["skill"]["body"] = serde_json::Value::String(skill.body.clone());
    }

    if args.deps {
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&skill.metadata_json) {
            output["skill"]["dependencies"] = meta
                .get("requires")
                .cloned()
                .unwrap_or(serde_json::Value::Array(vec![]));
        }
    }

    let toon = toon_rust::encode(output, None);
    println!("{toon}");
    Ok(())
}

fn format_date(datetime: &str) -> String {
    // Try to parse and format nicely
    datetime.split('T').next().unwrap_or(datetime).to_string()
}

fn normalize_layer(input: &str) -> String {
    match input.to_lowercase().as_str() {
        "system" => "base",
        "global" => "org",
        "local" => "user",
        other => other,
    }
    .to_string()
}
