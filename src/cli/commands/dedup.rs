//! ms dedup - Find and manage duplicate skills
//!
//! Scans skills for near-duplicates using semantic and structural similarity.

use clap::{Args, Subcommand};
use colored::Colorize;

use crate::app::AppContext;
use crate::cli::output::OutputFormat;
use crate::dedup::{DedupConfig, DeduplicationAction, DeduplicationEngine, DuplicatePair};
use crate::error::Result;
use crate::search::embeddings::build_embedder;
use crate::storage::Database;

#[derive(Args, Debug)]
pub struct DedupArgs {
    #[command(subcommand)]
    pub command: DedupCommand,
}

#[derive(Subcommand, Debug)]
pub enum DedupCommand {
    /// Scan all skills for duplicates
    Scan(ScanArgs),
    /// Review a specific duplicate pair
    Review(ReviewArgs),
    /// Merge two skills
    Merge(MergeArgs),
    /// Create an alias for a skill
    Alias(AliasArgs),
}

#[derive(Args, Debug)]
pub struct ScanArgs {
    /// Similarity threshold (0.0-1.0, default: 0.85)
    #[arg(long, short)]
    pub threshold: Option<f32>,

    /// Show only recommendations of specific type
    #[arg(long)]
    pub filter: Option<String>,

    /// Maximum number of results
    #[arg(long, default_value = "50")]
    pub limit: usize,
}

#[derive(Args, Debug)]
pub struct ReviewArgs {
    /// First skill ID
    pub skill_a: String,
    /// Second skill ID
    pub skill_b: String,
}

#[derive(Args, Debug)]
pub struct MergeArgs {
    /// Primary skill (will be kept)
    pub primary: String,
    /// Secondary skill (will be merged into primary)
    pub secondary: String,
    /// Reason for merge
    #[arg(long)]
    pub reason: Option<String>,
}

#[derive(Args, Debug)]
pub struct AliasArgs {
    /// Canonical skill ID
    pub canonical: String,
    /// Alias to create
    pub alias: String,
}

pub fn run(ctx: &AppContext, args: &DedupArgs) -> Result<()> {
    match &args.command {
        DedupCommand::Scan(scan_args) => run_scan(ctx, scan_args),
        DedupCommand::Review(review_args) => run_review(ctx, review_args),
        DedupCommand::Merge(merge_args) => run_merge(ctx, merge_args),
        DedupCommand::Alias(alias_args) => run_alias(ctx, alias_args),
    }
}

fn run_scan(ctx: &AppContext, args: &ScanArgs) -> Result<()> {
    let db = ctx.db.as_ref();
    let embedder = build_embedder(&ctx.config.search)?;

    let mut config = DedupConfig::default();
    if let Some(threshold) = args.threshold {
        config.similarity_threshold = threshold;
    }

    let engine = DeduplicationEngine::new(config, embedder.as_ref());

    if ctx.output_format != OutputFormat::Human {
        run_scan_robot(ctx, args, db, &engine)
    } else {
        run_scan_human(ctx, args, db, &engine)
    }
}

fn run_scan_human(
    _ctx: &AppContext,
    args: &ScanArgs,
    db: &Database,
    engine: &DeduplicationEngine,
) -> Result<()> {
    println!("{}", "Scanning for duplicate skills...".bold());
    println!();

    let pairs = engine.scan_all(db)?;

    if pairs.is_empty() {
        println!("{}", "No duplicates found.".green());
        return Ok(());
    }

    // Filter by recommendation type if requested
    let filtered: Vec<&DuplicatePair> = if let Some(ref filter) = args.filter {
        let filter_action = match filter.to_lowercase().as_str() {
            "merge" => Some(DeduplicationAction::Merge),
            "alias" => Some(DeduplicationAction::Alias),
            "review" => Some(DeduplicationAction::Review),
            "keep" | "keep_both" => Some(DeduplicationAction::KeepBoth),
            _ => None,
        };

        if let Some(action) = filter_action {
            pairs.iter().filter(|p| p.recommendation == action).collect()
        } else {
            pairs.iter().collect()
        }
    } else {
        pairs.iter().collect()
    };

    let display_pairs: Vec<_> = filtered.into_iter().take(args.limit).collect();

    println!(
        "Found {} potential duplicate pairs (showing {}):",
        pairs.len(),
        display_pairs.len()
    );
    println!();

    for (i, pair) in display_pairs.iter().enumerate() {
        let sim_color = if pair.similarity >= 0.95 {
            "red"
        } else if pair.similarity >= 0.90 {
            "yellow"
        } else {
            "cyan"
        };

        let action_str = match pair.recommendation {
            DeduplicationAction::Merge => "MERGE".red().bold(),
            DeduplicationAction::Alias => "ALIAS".yellow(),
            DeduplicationAction::Review => "REVIEW".cyan(),
            DeduplicationAction::Deprecate => "DEPRECATE".magenta(),
            DeduplicationAction::KeepBoth => "KEEP".green(),
        };

        println!(
            "{}. {} <-> {}",
            (i + 1).to_string().dimmed(),
            pair.skill_a_name.bold(),
            pair.skill_b_name.bold()
        );
        println!(
            "   Similarity: {} (semantic: {:.2}, structural: {:.2})",
            format!("{:.1}%", pair.similarity * 100.0).color(sim_color),
            pair.semantic_score,
            pair.structural_score
        );
        println!("   Recommendation: {action_str}");

        if pair.structural_details.tag_overlap > 0 {
            println!(
                "   Tag overlap: {}/{} tags in common",
                pair.structural_details.tag_overlap,
                pair.structural_details.primary_tags.max(pair.structural_details.candidate_tags)
            );
        }

        println!();
    }

    println!("{}", "Commands:".bold());
    println!("  ms dedup review <skill_a> <skill_b>  - Review pair details");
    println!("  ms dedup merge <primary> <secondary> - Merge skills");
    println!("  ms dedup alias <canonical> <alias>   - Create alias");

    Ok(())
}

fn run_scan_robot(
    _ctx: &AppContext,
    args: &ScanArgs,
    db: &Database,
    engine: &DeduplicationEngine,
) -> Result<()> {
    let pairs = engine.scan_all(db)?;

    // Filter by recommendation type if requested
    let filtered: Vec<&DuplicatePair> = if let Some(ref filter) = args.filter {
        let filter_action = match filter.to_lowercase().as_str() {
            "merge" => Some(DeduplicationAction::Merge),
            "alias" => Some(DeduplicationAction::Alias),
            "review" => Some(DeduplicationAction::Review),
            "keep" | "keep_both" => Some(DeduplicationAction::KeepBoth),
            _ => None,
        };

        if let Some(action) = filter_action {
            pairs.iter().filter(|p| p.recommendation == action).collect()
        } else {
            pairs.iter().collect()
        }
    } else {
        pairs.iter().collect()
    };

    let display_pairs: Vec<_> = filtered.into_iter().take(args.limit).collect();

    let output = serde_json::json!({
        "status": "ok",
        "total_pairs": pairs.len(),
        "displayed_pairs": display_pairs.len(),
        "pairs": display_pairs,
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn run_review(ctx: &AppContext, args: &ReviewArgs) -> Result<()> {
    let db = ctx.db.as_ref();

    let skill_a = db.get_skill(&args.skill_a)?;
    let skill_b = db.get_skill(&args.skill_b)?;

    let skill_a = skill_a.ok_or_else(|| {
        crate::error::MsError::SkillNotFound(format!("skill not found: {}", args.skill_a))
    })?;
    let skill_b = skill_b.ok_or_else(|| {
        crate::error::MsError::SkillNotFound(format!("skill not found: {}", args.skill_b))
    })?;

    if ctx.output_format != OutputFormat::Human {
        let output = serde_json::json!({
            "status": "ok",
            "skill_a": {
                "id": skill_a.id,
                "name": skill_a.name,
                "description": skill_a.description,
                "version": skill_a.version,
                "source_path": skill_a.source_path,
            },
            "skill_b": {
                "id": skill_b.id,
                "name": skill_b.name,
                "description": skill_b.description,
                "version": skill_b.version,
                "source_path": skill_b.source_path,
            },
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", "Skill Comparison".bold());
        println!();

        println!("{}", "Skill A:".bold().cyan());
        println!("  ID: {}", skill_a.id);
        println!("  Name: {}", skill_a.name);
        println!("  Description: {}", skill_a.description);
        println!("  Version: {}", skill_a.version.as_deref().unwrap_or("N/A"));
        println!("  Path: {}", skill_a.source_path);
        println!();

        println!("{}", "Skill B:".bold().cyan());
        println!("  ID: {}", skill_b.id);
        println!("  Name: {}", skill_b.name);
        println!("  Description: {}", skill_b.description);
        println!("  Version: {}", skill_b.version.as_deref().unwrap_or("N/A"));
        println!("  Path: {}", skill_b.source_path);
        println!();

        println!("{}", "Actions:".bold());
        println!(
            "  ms dedup merge {} {} - Merge B into A",
            skill_a.id, skill_b.id
        );
        println!(
            "  ms dedup alias {} {} - Make B an alias of A",
            skill_a.id, skill_b.id
        );
    }

    Ok(())
}

fn run_merge(ctx: &AppContext, args: &MergeArgs) -> Result<()> {
    let db = ctx.db.as_ref();

    // Verify both skills exist
    let primary = db.get_skill(&args.primary)?;
    let secondary = db.get_skill(&args.secondary)?;

    let primary = primary.ok_or_else(|| {
        crate::error::MsError::SkillNotFound(format!("primary skill not found: {}", args.primary))
    })?;
    let secondary = secondary.ok_or_else(|| {
        crate::error::MsError::SkillNotFound(format!(
            "secondary skill not found: {}",
            args.secondary
        ))
    })?;

    // Deprecate the secondary skill
    let reason = args.reason.clone().unwrap_or_else(|| {
        format!("Merged into {} ({})", primary.name, primary.id)
    });

    db.update_skill_deprecation(&secondary.id, true, Some(&reason))?;

    let created_at = chrono::Utc::now().to_rfc3339();
    db.upsert_alias(&secondary.id, &primary.id, "deprecated", &created_at)?;

    if let Some(record) = db.get_skill(&secondary.id)? {
        ctx.search.index_skill(&record)?;
        ctx.search.commit()?;
    }

    if ctx.output_format != OutputFormat::Human {
        let output = serde_json::json!({
            "status": "ok",
            "action": "merge",
            "primary": {
                "id": primary.id,
                "name": primary.name,
            },
            "secondary": {
                "id": secondary.id,
                "name": secondary.name,
                "deprecated": true,
                "reason": reason,
            },
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", "Merge completed:".bold().green());
        println!("  Primary: {} ({})", primary.name, primary.id);
        println!(
            "  Secondary: {} ({}) - {}",
            secondary.name,
            secondary.id,
            "deprecated".red()
        );
        println!("  Alias created: {} -> {}", secondary.id, primary.id);
    }

    Ok(())
}

fn run_alias(ctx: &AppContext, args: &AliasArgs) -> Result<()> {
    let db = ctx.db.as_ref();

    // Verify canonical skill exists
    let canonical = db.get_skill(&args.canonical)?;
    let canonical = canonical.ok_or_else(|| {
        crate::error::MsError::SkillNotFound(format!(
            "canonical skill not found: {}",
            args.canonical
        ))
    })?;

    // Create alias
    let created_at = chrono::Utc::now().to_rfc3339();
    db.upsert_alias(&args.alias, &canonical.id, "dedup_alias", &created_at)?;

    if ctx.output_format != OutputFormat::Human {
        let output = serde_json::json!({
            "status": "ok",
            "action": "alias",
            "canonical": {
                "id": canonical.id,
                "name": canonical.name,
            },
            "alias": args.alias,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", "Alias created:".bold().green());
        println!("  {} -> {} ({})", args.alias, canonical.name, canonical.id);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        command: DedupCommand,
    }

    #[test]
    fn test_scan_args_parse() {
        let cli = TestCli::parse_from(["test", "scan"]);
        assert!(matches!(cli.command, DedupCommand::Scan(_)));
    }

    #[test]
    fn test_scan_with_threshold() {
        let cli = TestCli::parse_from(["test", "scan", "--threshold", "0.9"]);
        if let DedupCommand::Scan(args) = cli.command {
            assert_eq!(args.threshold, Some(0.9));
        } else {
            panic!("Expected Scan command");
        }
    }

    #[test]
    fn test_review_args_parse() {
        let cli = TestCli::parse_from(["test", "review", "skill-a", "skill-b"]);
        if let DedupCommand::Review(args) = cli.command {
            assert_eq!(args.skill_a, "skill-a");
            assert_eq!(args.skill_b, "skill-b");
        } else {
            panic!("Expected Review command");
        }
    }

    #[test]
    fn test_merge_args_parse() {
        let cli = TestCli::parse_from(["test", "merge", "primary", "secondary"]);
        if let DedupCommand::Merge(args) = cli.command {
            assert_eq!(args.primary, "primary");
            assert_eq!(args.secondary, "secondary");
        } else {
            panic!("Expected Merge command");
        }
    }
}
