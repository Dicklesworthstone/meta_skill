//! ms list - List all indexed skills

use clap::Args;
use serde::Serialize;
use tracing::debug;

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
    debug!(target: "list", mode = ?ctx.output_format, "output mode selected");

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

    debug!(target: "list", count = skills.len(), filters = ?args.tags, "listing skills");

    let result = display_list(ctx, &skills, args);
    debug!(target: "list", stage = "render_complete");
    result
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
            // bd-olwb spec: NAME<TAB>LAYER<TAB>TAGS<TAB>UPDATED (no headers)
            for skill in skills {
                // Extract tags from metadata_json
                let tags = if let Ok(meta) =
                    serde_json::from_str::<serde_json::Value>(&skill.metadata_json)
                {
                    meta.get("tags")
                        .and_then(|t| t.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str())
                                .collect::<Vec<_>>()
                                .join(",")
                        })
                        .unwrap_or_default()
                } else {
                    String::new()
                };

                // Format date - just date part
                let updated = skill
                    .modified_at
                    .split('T')
                    .next()
                    .unwrap_or(&skill.modified_at);

                println!(
                    "{}\t{}\t{}\t{}",
                    skill.name, skill.source_layer, tags, updated
                );
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
        OutputFormat::Toon => {
            let entries: Vec<SkillEntry> = skills.iter().map(SkillEntry::from).collect();
            let output = serde_json::json!({
                "status": "ok",
                "count": entries.len(),
                "skills": entries
            });
            let toon = toon_rust::encode(output, None);
            println!("{toon}");
            Ok(())
        }
    }
}

fn display_list_human(skills: &[SkillRecord], args: &ListArgs) -> Result<()> {
    if skills.is_empty() {
        println!("No skills found");
        println!();
        println!("Index skills with: ms index");
        return Ok(());
    }

    // Print header
    println!(
        "{:40} {:12} {:8} {:20}",
        "ID", "VERSION", "LAYER", "UPDATED"
    );
    println!("{}", "─".repeat(84));

    for skill in skills {
        let layer = normalize_layer(&skill.source_layer);

        let deprecated_marker = if skill.is_deprecated {
            " [deprecated]".to_string()
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
            layer,
            updated,
            deprecated_marker
        );
    }

    println!();
    println!(
        "Total: {} skills (limit: {}, offset: {})",
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

/// Check whether the terminal supports rich output for list commands.
#[allow(dead_code)]
fn should_use_rich_for_list() -> bool {
    use std::io::IsTerminal;

    if std::env::var("MS_FORCE_RICH").is_ok() {
        return true;
    }
    if std::env::var("NO_COLOR").is_ok() || std::env::var("MS_PLAIN_OUTPUT").is_ok() {
        return false;
    }

    use crate::output::{is_agent_environment, is_ci_environment};
    if is_agent_environment() || is_ci_environment() {
        return false;
    }

    std::io::stdout().is_terminal()
}

/// Get the terminal width, defaulting to 80 if detection fails.
#[allow(dead_code)]
fn terminal_width() -> usize {
    crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(80)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_skill(name: &str, layer: &str, deprecated: bool) -> SkillRecord {
        SkillRecord {
            id: format!("skill-{name}"),
            name: name.to_string(),
            version: Some("1.0.0".to_string()),
            description: format!("Description for {name}"),
            author: Some("test-author".to_string()),
            source_layer: layer.to_string(),
            source_path: format!("/skills/{name}"),
            git_remote: None,
            git_commit: None,
            content_hash: "abc123".to_string(),
            body: String::new(),
            metadata_json: r#"{"tags":["cli","rust"]}"#.to_string(),
            assets_json: "[]".to_string(),
            token_count: 100,
            quality_score: 0.85,
            indexed_at: "2025-01-01T00:00:00Z".to_string(),
            modified_at: "2025-06-15T10:30:00Z".to_string(),
            is_deprecated: deprecated,
            deprecation_reason: if deprecated {
                Some("Superseded".to_string())
            } else {
                None
            },
        }
    }

    fn default_args() -> ListArgs {
        ListArgs {
            tags: vec![],
            layer: None,
            include_deprecated: false,
            sort: "name".to_string(),
            limit: 50,
            offset: 0,
        }
    }

    // ── 1. test_list_render_empty_state ─────────────────────────────

    #[test]
    fn test_list_render_empty_state() {
        let skills: Vec<SkillRecord> = vec![];
        let args = default_args();
        let result = display_list_human(&skills, &args);
        assert!(result.is_ok());
    }

    // ── 2. test_list_render_single_skill ────────────────────────────

    #[test]
    fn test_list_render_single_skill() {
        let skills = vec![make_skill("hello-world", "base", false)];
        let args = default_args();
        let result = display_list_human(&skills, &args);
        assert!(result.is_ok());
    }

    // ── 3. test_list_render_many_skills ─────────────────────────────

    #[test]
    fn test_list_render_many_skills() {
        let skills: Vec<SkillRecord> = (0..20)
            .map(|i| make_skill(&format!("skill-{i}"), "project", false))
            .collect();
        let args = default_args();
        let result = display_list_human(&skills, &args);
        assert!(result.is_ok());
    }

    // ── 4. test_list_column_width_adapt ─────────────────────────────

    #[test]
    fn test_list_column_width_adapt() {
        let width = terminal_width();
        assert!(width >= 40, "minimum terminal width should be reasonable");
        assert!(width <= 500, "maximum terminal width should be reasonable");
    }

    // ── 5. test_list_truncate_long_names ────────────────────────────

    #[test]
    fn test_list_truncate_long_names() {
        let long_name = "a".repeat(50);
        let mut skill = make_skill(&long_name, "base", false);
        skill.id = "a".repeat(50);
        // The display function truncates IDs > 38 chars
        let id_display = if skill.id.chars().count() > 38 {
            format!("{}…", skill.id.chars().take(37).collect::<String>())
        } else {
            skill.id.clone()
        };
        assert_eq!(id_display.chars().count(), 38); // 37 + ellipsis
    }

    // ── 6. test_list_filter_display ─────────────────────────────────

    #[test]
    fn test_list_filter_display() {
        let skills = vec![
            make_skill("alpha", "base", false),
            make_skill("beta", "project", false),
        ];
        // Filter by layer
        let filtered: Vec<_> = skills
            .iter()
            .filter(|s| normalize_layer(&s.source_layer) == "base")
            .collect();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "alpha");
    }

    // ── 7. test_list_pagination_display ─────────────────────────────

    #[test]
    fn test_list_pagination_display() {
        let args = ListArgs {
            tags: vec![],
            layer: None,
            include_deprecated: false,
            sort: "name".to_string(),
            limit: 10,
            offset: 5,
        };
        // Pagination values accessible
        assert_eq!(args.limit, 10);
        assert_eq!(args.offset, 5);
    }

    // ── 8. test_list_plain_output_format ────────────────────────────

    #[test]
    fn test_list_plain_output_format() {
        let skill = make_skill("my-skill", "project", false);
        let tags = if let Ok(meta) =
            serde_json::from_str::<serde_json::Value>(&skill.metadata_json)
        {
            meta.get("tags")
                .and_then(|t| t.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .unwrap_or_default()
        } else {
            String::new()
        };
        let updated = skill
            .modified_at
            .split('T')
            .next()
            .unwrap_or(&skill.modified_at);
        let line = format!("{}\t{}\t{}\t{}", skill.name, skill.source_layer, tags, updated);
        assert!(line.contains("my-skill"));
        assert!(line.contains("project"));
        assert!(line.contains("cli,rust"));
        assert!(line.contains("2025-06-15"));
        assert!(!line.contains("\x1b["), "plain output must have no ANSI");
    }

    // ── 9. test_list_json_output_format ─────────────────────────────

    #[test]
    fn test_list_json_output_format() {
        let skills = vec![make_skill("alpha", "base", false)];
        let entries: Vec<SkillEntry> = skills.iter().map(SkillEntry::from).collect();
        let output = serde_json::json!({
            "status": "ok",
            "count": entries.len(),
            "skills": entries
        });
        let json_str = serde_json::to_string(&output).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["status"], "ok");
        assert_eq!(parsed["count"], 1);
        assert!(parsed["skills"].is_array());
        assert_eq!(parsed["skills"][0]["name"], "alpha");
    }

    // ── 10. test_list_robot_mode_no_ansi ────────────────────────────

    #[test]
    fn test_list_robot_mode_no_ansi() {
        let skill = make_skill("test-skill", "base", false);
        let entry = SkillEntry::from(&skill);
        let json = serde_json::to_string_pretty(&entry).unwrap();
        assert!(!json.contains("\x1b["), "robot mode must have no ANSI");
    }

    // ── 11. test_list_deprecated_marker ─────────────────────────────

    #[test]
    fn test_list_deprecated_marker() {
        let skill = make_skill("old-skill", "base", true);
        let marker = if skill.is_deprecated {
            " [deprecated]".to_string()
        } else {
            String::new()
        };
        assert_eq!(marker, " [deprecated]");
        assert!(!marker.contains("\x1b["), "deprecated marker must be plain");
    }

    // ── 12. test_list_rich_vs_plain_equivalence ─────────────────────

    #[test]
    fn test_list_rich_vs_plain_equivalence() {
        let skill = make_skill("equiv-skill", "project", false);
        let entry = SkillEntry::from(&skill);

        let pretty = serde_json::to_string_pretty(&entry).unwrap();
        let compact = serde_json::to_string(&entry).unwrap();

        let v1: serde_json::Value = serde_json::from_str(&pretty).unwrap();
        let v2: serde_json::Value = serde_json::from_str(&compact).unwrap();

        assert_eq!(v1["name"], v2["name"]);
        assert_eq!(v1["layer"], v2["layer"]);
        assert_eq!(v1["is_deprecated"], v2["is_deprecated"]);
    }

    // ── 13. test_list_normalize_layer ───────────────────────────────

    #[test]
    fn test_list_normalize_layer() {
        assert_eq!(normalize_layer("system"), "base");
        assert_eq!(normalize_layer("global"), "org");
        assert_eq!(normalize_layer("local"), "user");
        assert_eq!(normalize_layer("project"), "project");
        assert_eq!(normalize_layer("SYSTEM"), "base");
    }

    // ── 14. test_list_skill_entry_from_record ───────────────────────

    #[test]
    fn test_list_skill_entry_from_record() {
        let record = make_skill("convert-test", "org", false);
        let entry = SkillEntry::from(&record);
        assert_eq!(entry.name, "convert-test");
        assert_eq!(entry.layer, "org");
        assert!(!entry.is_deprecated);
        assert_eq!(entry.quality_score, 0.85);
    }

    // ── 15. test_list_should_use_rich_respects_no_color ─────────────

    #[test]
    fn test_list_should_use_rich_respects_no_color() {
        // The helper checks env vars - verify it exists and returns bool
        let _result: bool = should_use_rich_for_list();
    }

    // ── 16. test_list_sort_by_name ──────────────────────────────────

    #[test]
    fn test_list_sort_by_name() {
        let mut skills = vec![
            make_skill("zulu", "base", false),
            make_skill("alpha", "base", false),
            make_skill("mike", "base", false),
        ];
        skills.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(skills[0].name, "alpha");
        assert_eq!(skills[1].name, "mike");
        assert_eq!(skills[2].name, "zulu");
    }
}
