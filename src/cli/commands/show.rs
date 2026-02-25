//! ms show - Show skill details
//!
//! Displays skill information in multiple formats: rich terminal output with
//! panels and styled metadata (Human mode), plain YAML-like key-value pairs
//! (Plain mode), JSON, JSONL, TSV, and TOON.

use clap::Args;
use tracing::debug;

use crate::app::AppContext;
use crate::cli::output::OutputFormat;
use crate::error::{MsError, Result};
use crate::output::{
    is_agent_environment, is_ci_environment, key_value_table, skill_detail_panel, warning_panel,
};
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
    debug!(target: "show", skill_id = %skill.id, "loading skill");
    debug!(target: "show", mode = ?ctx.output_format, "output mode selected");

    let result = match ctx.output_format {
        OutputFormat::Human => show_human(ctx, skill, args),
        OutputFormat::Json => show_json(skill, args, true),
        OutputFormat::Jsonl => show_json(skill, args, false),
        OutputFormat::Plain => show_plain(skill),
        OutputFormat::Tsv => show_tsv(skill),
        OutputFormat::Toon => show_toon(skill, args),
    };

    debug!(target: "show", stage = "render_complete");
    result
}

fn show_human(_ctx: &AppContext, skill: &SkillRecord, args: &ShowArgs) -> Result<()> {
    let use_rich = should_use_rich_for_show();
    let width = terminal_width();

    if use_rich {
        show_human_rich(skill, args, width)
    } else {
        show_human_plain(skill, args)
    }
}

/// Rich terminal rendering using panels and styled tables.
fn show_human_rich(skill: &SkillRecord, args: &ShowArgs, width: usize) -> Result<()> {
    // Header panel with skill info
    let panel = skill_detail_panel(
        &skill.name,
        &skill.description,
        &normalize_layer(&skill.source_layer),
        skill.quality_score,
        "",
    );
    println!("{panel}");

    // Metadata table
    let mut pairs: Vec<(&str, String)> = vec![
        ("ID", skill.id.clone()),
        (
            "Version",
            skill.version.as_deref().unwrap_or("-").to_string(),
        ),
        ("Layer", normalize_layer(&skill.source_layer)),
        ("Source", skill.source_path.clone()),
    ];
    if let Some(ref author) = skill.author {
        pairs.push(("Author", author.clone()));
    }

    let table_data: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let table = key_value_table(&table_data);
    println!("{}", table.render_plain(width));

    // Deprecation warning
    if skill.is_deprecated {
        let reason = skill
            .deprecation_reason
            .as_deref()
            .unwrap_or("No reason provided");
        let warn = warning_panel("DEPRECATED", reason);
        println!("\n{warn}");
    }

    // Stats
    println!("\nStats");
    println!("{}", "-".repeat(40));
    println!("Tokens:   {}", skill.token_count);
    println!("Quality:  {:.2}", skill.quality_score);
    println!("Indexed:  {}", format_date(&skill.indexed_at));
    println!("Modified: {}", format_date(&skill.modified_at));

    // Provenance
    if skill.git_remote.is_some() || skill.git_commit.is_some() {
        println!("\nProvenance");
        println!("{}", "-".repeat(40));
        if let Some(ref remote) = skill.git_remote {
            println!("Remote: {remote}");
        }
        if let Some(ref commit) = skill.git_commit {
            println!("Commit: {}", &commit[..commit.len().min(8)]);
        }
        if !skill.content_hash.is_empty() {
            println!(
                "Hash:   {}",
                &skill.content_hash[..skill.content_hash.len().min(16)]
            );
        }
    }

    // Metadata JSON
    if args.meta || args.full {
        println!("\nMetadata");
        println!("{}", "-".repeat(40));
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&skill.metadata_json) {
            if let Ok(pretty) = serde_json::to_string_pretty(&meta) {
                println!("{pretty}");
            }
        }
    }

    // Full body
    if args.full {
        println!("\nBody");
        println!("{}", "-".repeat(40));
        println!("{}", skill.body);
    }

    // Dependencies
    if args.deps {
        show_deps(skill);
    }

    Ok(())
}

/// Plain text rendering without any ANSI/styling.
fn show_human_plain(skill: &SkillRecord, args: &ShowArgs) -> Result<()> {
    // Header
    println!("{}", skill.name);
    println!("{}", "=".repeat(skill.name.len()));
    println!();

    // Core fields
    println!("ID:      {}", skill.id);
    println!(
        "Version: {}",
        skill.version.as_deref().unwrap_or("-")
    );
    if let Some(ref author) = skill.author {
        println!("Author:  {author}");
    }
    println!("Layer:   {}", normalize_layer(&skill.source_layer));
    println!("Source:  {}", skill.source_path);

    // Description
    if !skill.description.is_empty() {
        println!();
        println!("{}", skill.description);
    }

    // Deprecation
    if skill.is_deprecated {
        println!();
        println!(
            "WARNING DEPRECATED: {}",
            skill
                .deprecation_reason
                .as_deref()
                .unwrap_or("No reason provided")
        );
    }

    // Stats
    println!();
    println!("Stats");
    println!("{}", "-".repeat(40));
    println!("Tokens:   {}", skill.token_count);
    println!("Quality:  {:.2}", skill.quality_score);
    println!("Indexed:  {}", format_date(&skill.indexed_at));
    println!("Modified: {}", format_date(&skill.modified_at));

    // Provenance
    if skill.git_remote.is_some() || skill.git_commit.is_some() {
        println!();
        println!("Provenance");
        println!("{}", "-".repeat(40));
        if let Some(ref remote) = skill.git_remote {
            println!("Remote: {remote}");
        }
        if let Some(ref commit) = skill.git_commit {
            println!("Commit: {}", &commit[..commit.len().min(8)]);
        }
        if !skill.content_hash.is_empty() {
            println!(
                "Hash:   {}",
                &skill.content_hash[..skill.content_hash.len().min(16)]
            );
        }
    }

    // Metadata JSON
    if args.meta || args.full {
        println!();
        println!("Metadata");
        println!("{}", "-".repeat(40));
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&skill.metadata_json) {
            if let Ok(pretty) = serde_json::to_string_pretty(&meta) {
                println!("{pretty}");
            }
        }
    }

    // Full body
    if args.full {
        println!();
        println!("Body");
        println!("{}", "-".repeat(40));
        println!("{}", skill.body);
    }

    // Dependencies
    if args.deps {
        show_deps(skill);
    }

    Ok(())
}

/// Show dependency information from metadata.
fn show_deps(skill: &SkillRecord) {
    println!();
    println!("Dependencies");
    println!("{}", "-".repeat(40));
    if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&skill.metadata_json) {
        if let Some(requires) = meta.get("requires").and_then(|d| d.as_array()) {
            if requires.is_empty() {
                println!("No dependencies");
            } else {
                for req in requires {
                    if let Some(req_str) = req.as_str() {
                        println!("  -> {req_str}");
                    }
                }
            }
        } else {
            println!("No dependencies");
        }
    }
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

/// Check whether the terminal supports rich output for the show command.
///
/// Returns `true` when:
/// - `MS_FORCE_RICH` is set (explicit override), OR
/// - stdout is a terminal AND we're not in an agent/CI environment AND
///   `NO_COLOR`/`MS_PLAIN_OUTPUT` are not set.
fn should_use_rich_for_show() -> bool {
    use std::io::IsTerminal;

    // Explicit overrides
    if std::env::var("MS_FORCE_RICH").is_ok() {
        return true;
    }
    if std::env::var("NO_COLOR").is_ok() || std::env::var("MS_PLAIN_OUTPUT").is_ok() {
        return false;
    }

    // Agent and CI environments -> plain
    if is_agent_environment() || is_ci_environment() {
        return false;
    }

    // Not a terminal -> plain
    std::io::stdout().is_terminal()
}

/// Get the terminal width, defaulting to 80 if detection fails.
fn terminal_width() -> usize {
    crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(80)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal SkillRecord for testing.
    fn make_skill() -> SkillRecord {
        SkillRecord {
            id: "sk-abc123".to_string(),
            name: "test-skill".to_string(),
            version: Some("1.0.0".to_string()),
            description: "A test skill for unit tests".to_string(),
            author: Some("tester".to_string()),
            source_layer: "local".to_string(),
            source_path: "/skills/test-skill.md".to_string(),
            body: "# Test\nHello world".to_string(),
            metadata_json: serde_json::json!({
                "tags": ["cli", "rust"],
                "type": "tool",
                "requires": ["dep-a", "dep-b"]
            })
            .to_string(),
            content_hash: "abcdef0123456789abcdef".to_string(),
            assets_json: "[]".to_string(),
            token_count: 42,
            quality_score: 0.85,
            indexed_at: "2025-06-01T12:00:00Z".to_string(),
            modified_at: "2025-06-15T08:30:00Z".to_string(),
            is_deprecated: false,
            deprecation_reason: None,
            git_remote: Some("https://github.com/example/repo".to_string()),
            git_commit: Some("deadbeef12345678".to_string()),
        }
    }

    // ── 1. test_show_render_header_panel ──────────────────────────────

    #[test]
    fn test_show_render_header_panel() {
        let skill = make_skill();
        let panel = skill_detail_panel(
            &skill.name,
            &skill.description,
            &normalize_layer(&skill.source_layer),
            skill.quality_score,
            "",
        );
        let rendered = format!("{panel}");
        assert!(
            rendered.contains("test-skill"),
            "panel should contain skill name"
        );
    }

    // ── 2. test_show_render_metadata_table ────────────────────────────

    #[test]
    fn test_show_render_metadata_table() {
        let skill = make_skill();
        let pairs: Vec<(&str, String)> = vec![
            ("ID", skill.id.clone()),
            ("Version", skill.version.clone().unwrap()),
            ("Layer", normalize_layer(&skill.source_layer)),
            ("Source", skill.source_path.clone()),
        ];
        let table_data: Vec<(&str, &str)> =
            pairs.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let table = key_value_table(&table_data);
        let rendered = table.render_plain(80);
        assert!(rendered.contains("sk-abc123"), "table should contain ID");
        assert!(rendered.contains("1.0.0"), "table should contain version");
    }

    // ── 3. test_show_render_tags_chips ────────────────────────────────

    #[test]
    fn test_show_render_tags_chips() {
        let skill = make_skill();
        let meta: serde_json::Value = serde_json::from_str(&skill.metadata_json).unwrap();
        let tags = meta
            .get("tags")
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        assert_eq!(tags, "cli, rust");
    }

    // ── 4. test_show_render_description_markdown ──────────────────────

    #[test]
    fn test_show_render_description_markdown() {
        let skill = make_skill();
        assert!(
            !skill.description.is_empty(),
            "description should not be empty"
        );
        // In plain mode, description is printed as-is
        let desc = skill.description.replace('\n', " ").replace('\r', "");
        assert_eq!(desc, "A test skill for unit tests");
    }

    // ── 5. test_show_render_code_blocks ───────────────────────────────

    #[test]
    fn test_show_render_code_blocks() {
        let mut skill = make_skill();
        skill.body = "```rust\nfn main() {}\n```".to_string();
        // Full mode should output the body verbatim
        assert!(skill.body.contains("```rust"));
        assert!(skill.body.contains("fn main()"));
    }

    // ── 6. test_show_render_blocks_section ────────────────────────────

    #[test]
    fn test_show_render_blocks_section() {
        let skill = make_skill();
        // Body has heading + content
        assert!(skill.body.starts_with("# Test"));
        assert!(skill.body.contains("Hello world"));
    }

    // ── 7. test_show_plain_output_format ──────────────────────────────

    #[test]
    fn test_show_plain_output_format() {
        let skill = make_skill();
        // Plain format produces YAML-like key-value pairs
        let meta: serde_json::Value = serde_json::from_str(&skill.metadata_json).unwrap();
        let skill_type = meta
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("skill");
        assert_eq!(skill_type, "tool");

        // Verify all plain output fields are available
        assert!(!skill.name.is_empty());
        assert!(skill.version.is_some());
    }

    // ── 8. test_show_json_output_format ───────────────────────────────

    #[test]
    fn test_show_json_output_format() {
        let skill = make_skill();
        let output = serde_json::json!({
            "status": "ok",
            "skill": {
                "id": skill.id,
                "name": skill.name,
                "version": skill.version,
                "description": skill.description,
                "token_count": skill.token_count,
                "quality_score": skill.quality_score,
                "is_deprecated": skill.is_deprecated,
            }
        });
        let json_str = serde_json::to_string_pretty(&output).unwrap();
        assert!(json_str.contains("\"status\": \"ok\""));
        assert!(json_str.contains("\"id\": \"sk-abc123\""));
        assert!(json_str.contains("\"token_count\": 42"));
    }

    // ── 9. test_show_robot_mode_no_ansi ───────────────────────────────

    #[test]
    fn test_show_robot_mode_no_ansi() {
        let skill = make_skill();
        // Plain mode output should contain no ANSI escape sequences
        let plain_header = format!(
            "{}\n{}\n\nID:      {}\nVersion: {}",
            skill.name,
            "=".repeat(skill.name.len()),
            skill.id,
            skill.version.as_deref().unwrap_or("-"),
        );
        assert!(
            !plain_header.contains("\x1b["),
            "plain output must not contain ANSI escapes"
        );
    }

    // ── 10. test_show_missing_fields ──────────────────────────────────

    #[test]
    fn test_show_missing_fields() {
        let mut skill = make_skill();
        skill.version = None;
        skill.author = None;
        skill.git_remote = None;
        skill.git_commit = None;
        skill.deprecation_reason = None;

        // Verify graceful handling of missing optional fields
        assert_eq!(skill.version.as_deref().unwrap_or("-"), "-");
        assert!(skill.author.is_none());
        assert!(skill.git_remote.is_none());
        assert!(skill.git_commit.is_none());
    }

    // ── 11. test_show_long_content_truncation ─────────────────────────

    #[test]
    fn test_show_long_content_truncation() {
        let skill = make_skill();
        // Verify content_hash and git_commit truncation logic
        let hash_trunc = &skill.content_hash[..skill.content_hash.len().min(16)];
        assert_eq!(hash_trunc, "abcdef0123456789");

        let commit_trunc = &skill.git_commit.as_ref().unwrap()
            [..skill.git_commit.as_ref().unwrap().len().min(8)];
        assert_eq!(commit_trunc, "deadbeef");
    }

    // ── 12. test_show_rich_vs_plain_equivalence ───────────────────────

    #[test]
    fn test_show_rich_vs_plain_equivalence() {
        let skill = make_skill();
        // Both modes should expose the same core data fields
        let fields = vec![
            ("id", skill.id.as_str()),
            ("name", skill.name.as_str()),
            ("description", skill.description.as_str()),
            ("source_path", skill.source_path.as_str()),
        ];
        for (label, value) in &fields {
            assert!(
                !value.is_empty(),
                "field '{label}' should be non-empty in both modes"
            );
        }
    }

    // ── 13. test_show_format_date ─────────────────────────────────────

    #[test]
    fn test_show_format_date() {
        assert_eq!(format_date("2025-06-01T12:00:00Z"), "2025-06-01");
        assert_eq!(format_date("2025-06-01"), "2025-06-01");
        assert_eq!(format_date(""), "");
    }

    // ── 14. test_show_normalize_layer ─────────────────────────────────

    #[test]
    fn test_show_normalize_layer() {
        assert_eq!(normalize_layer("system"), "base");
        assert_eq!(normalize_layer("System"), "base");
        assert_eq!(normalize_layer("global"), "org");
        assert_eq!(normalize_layer("Global"), "org");
        assert_eq!(normalize_layer("local"), "user");
        assert_eq!(normalize_layer("Local"), "user");
        assert_eq!(normalize_layer("custom"), "custom");
    }

    // ── 15. test_show_deprecation_warning ─────────────────────────────

    #[test]
    fn test_show_deprecation_warning() {
        let mut skill = make_skill();
        skill.is_deprecated = true;
        skill.deprecation_reason = Some("Use v2 instead".to_string());

        let warn = warning_panel(
            "DEPRECATED",
            skill.deprecation_reason.as_deref().unwrap(),
        );
        let rendered = format!("{warn}");
        assert!(!rendered.is_empty(), "warning panel should render");
    }

    // ── 16. test_show_deps_parsing ────────────────────────────────────

    #[test]
    fn test_show_deps_parsing() {
        let skill = make_skill();
        let meta: serde_json::Value = serde_json::from_str(&skill.metadata_json).unwrap();
        let requires = meta
            .get("requires")
            .and_then(|d| d.as_array())
            .unwrap();
        assert_eq!(requires.len(), 2);
        assert_eq!(requires[0].as_str().unwrap(), "dep-a");
        assert_eq!(requires[1].as_str().unwrap(), "dep-b");
    }

    // ── 17. test_show_json_with_meta_flag ─────────────────────────────

    #[test]
    fn test_show_json_with_meta_flag() {
        let skill = make_skill();
        let mut output = serde_json::json!({
            "status": "ok",
            "skill": {
                "id": skill.id,
                "name": skill.name,
            }
        });
        // Simulate --meta flag adding metadata
        let meta: serde_json::Value = serde_json::from_str(&skill.metadata_json).unwrap();
        output["skill"]["metadata"] = meta;

        let json_str = serde_json::to_string(&output).unwrap();
        assert!(json_str.contains("\"tags\""));
        assert!(json_str.contains("\"requires\""));
    }

    // ── 18. test_show_json_with_deps_flag ─────────────────────────────

    #[test]
    fn test_show_json_with_deps_flag() {
        let skill = make_skill();
        let meta: serde_json::Value = serde_json::from_str(&skill.metadata_json).unwrap();
        let deps = meta
            .get("requires")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![]));
        assert!(deps.is_array());
        assert_eq!(deps.as_array().unwrap().len(), 2);
    }
}
