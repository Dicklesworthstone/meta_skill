//! ms list - List all indexed skills

use clap::Args;
use colored::Colorize;
use serde::Serialize;

use crate::app::AppContext;
use crate::cli::output::OutputFormat;
use crate::error::Result;
use crate::storage::sqlite::SkillRecord;

#[derive(Args, Debug)]
pub struct ListArgs {
    /// Filter by tags
    #[arg(long, short)]
    pub tags: Vec<String>,

    /// Filter by layer: base, org, project, user
    #[arg(long)]
    pub layer: Option<String>,

    /// Include deprecated skills
    #[arg(long)]
    pub include_deprecated: bool,

    /// Sort by: name, updated, relevance
    #[arg(long, default_value = "name")]
    pub sort: String,

    /// Maximum number of skills to show
    #[arg(long, short = 'n', default_value = "50")]
    pub limit: usize,

    /// Offset for pagination
    #[arg(long, default_value = "0")]
    pub offset: usize,
}

pub fn run(ctx: &AppContext, args: &ListArgs) -> Result<()> {
    // Fetch skills from database
    let skills = ctx.db.list_skills(args.limit, args.offset)?;

    // Filter by layer if specified
    let skills: Vec<_> = if let Some(ref layer) = args.layer {
        let normalized = normalize_layer(layer);
        skills
            .into_iter()
            .filter(|s| normalize_layer(&s.source_layer) == normalized)
            .collect()
    } else {
        skills
    };

    // Filter by tags if specified
    let skills: Vec<_> = if args.tags.is_empty() {
        skills
    } else {
        skills
            .into_iter()
            .filter(|s| {
                // Parse metadata_json to check tags
                if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&s.metadata_json) {
                    if let Some(tags) = meta.get("tags").and_then(|t| t.as_array()) {
                        let skill_tags: Vec<String> = tags
                            .iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect();
                        return args.tags.iter().any(|t| skill_tags.contains(t));
                    }
                }
                false
            })
            .collect()
    };

    // Filter deprecated unless explicitly included
    let skills: Vec<_> = if args.include_deprecated {
        skills
    } else {
        skills.into_iter().filter(|s| !s.is_deprecated).collect()
    };

    // Sort
    let mut skills = skills;
    match args.sort.as_str() {
        "name" => skills.sort_by(|a, b| a.name.cmp(&b.name)),
        "updated" => skills.sort_by(|a, b| b.modified_at.cmp(&a.modified_at)),
        _ => {}
    }

    display_list(ctx, &skills, args)
}

/// Serializable skill entry for JSON/JSONL output
#[derive(Debug, Clone, Serialize)]
struct SkillEntry {
    id: String,
    name: String,
    version: Option<String>,
    description: String,
    author: Option<String>,
    layer: String,
    source_path: String,
    modified_at: String,
    is_deprecated: bool,
    deprecation_reason: Option<String>,
    quality_score: f64,
}

impl From<&SkillRecord> for SkillEntry {
    fn from(s: &SkillRecord) -> Self {
        Self {
            id: s.id.clone(),
            name: s.name.clone(),
            version: s.version.clone(),
            description: s.description.clone(),
            author: s.author.clone(),
            layer: s.source_layer.clone(),
            source_path: s.source_path.clone(),
            modified_at: s.modified_at.clone(),
            is_deprecated: s.is_deprecated,
            deprecation_reason: s.deprecation_reason.clone(),
            quality_score: s.quality_score,
        }
    }
}

fn display_list(ctx: &AppContext, skills: &[SkillRecord], args: &ListArgs) -> Result<()> {
    match ctx.output_format {
        OutputFormat::Human => display_list_human(skills, args),
        OutputFormat::Json => {
            let entries: Vec<SkillEntry> = skills.iter().map(SkillEntry::from).collect();
            let output = serde_json::json!({
                "status": "ok",
                "count": entries.len(),
                "skills": entries
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&output).unwrap_or_default()
            );
            Ok(())
        }
        OutputFormat::Jsonl => {
            for skill in skills {
                let entry = SkillEntry::from(skill);
                println!("{}", serde_json::to_string(&entry).unwrap_or_default());
            }
            Ok(())
        }
        OutputFormat::Plain => {
            for skill in skills {
                println!("{}", skill.id);
            }
            Ok(())
        }
        OutputFormat::Tsv => {
            println!("id\tname\tversion\tlayer\tquality\tmodified_at\tis_deprecated");
            for skill in skills {
                println!(
                    "{}\t{}\t{}\t{}\t{:.2}\t{}\t{}",
                    skill.id,
                    skill.name,
                    skill.version.as_deref().unwrap_or("-"),
                    skill.source_layer,
                    skill.quality_score,
                    skill
                        .modified_at
                        .split('T')
                        .next()
                        .unwrap_or(&skill.modified_at),
                    skill.is_deprecated
                );
            }
            Ok(())
        }
    }
}

fn display_list_human(skills: &[SkillRecord], args: &ListArgs) -> Result<()> {
    if skills.is_empty() {
        println!("{}", "No skills found".dimmed());
        println!();
        println!("Index skills with: ms index");
        return Ok(());
    }

    // Print header
    println!(
        "{:40} {:12} {:8} {:20}",
        "ID".bold(),
        "VERSION".bold(),
        "LAYER".bold(),
        "UPDATED".bold()
    );
    println!("{}", "─".repeat(84).dimmed());

    for skill in skills {
        let layer = normalize_layer(&skill.source_layer);
        let layer_colored = match layer.as_str() {
            "base" => layer.blue(),
            "org" => layer.green(),
            "project" => layer.yellow(),
            "user" => layer.magenta(),
            _ => layer.normal(),
        };

        let deprecated_marker = if skill.is_deprecated {
            " [deprecated]".red().to_string()
        } else {
            String::new()
        };

        // Truncate ID if too long (use char count for UTF-8 safety)
        let id_display = if skill.id.chars().count() > 38 {
            format!("{}…", skill.id.chars().take(37).collect::<String>())
        } else {
            skill.id.clone()
        };

        // Format date - just date part
        let updated = skill
            .modified_at
            .split('T')
            .next()
            .unwrap_or(&skill.modified_at);

        println!(
            "{:40} {:12} {:8} {:20}{}",
            id_display,
            skill.version.as_deref().unwrap_or("-"),
            layer_colored,
            updated,
            deprecated_marker
        );
    }

    println!();
    println!(
        "{} {} skills (limit: {}, offset: {})",
        "Total:".dimmed(),
        skills.len(),
        args.limit,
        args.offset
    );

    Ok(())
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
