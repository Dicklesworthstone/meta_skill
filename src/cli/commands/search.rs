//! ms search - Search for skills
//!
//! Provides hybrid search combining BM25 full-text and semantic vector
//! similarity via RRF fusion.

use clap::Args;
use colored::Colorize;

use crate::app::AppContext;
use crate::error::Result;
use crate::search::{
    fuse_simple, HashEmbedder, RrfConfig, SearchFilters, SearchLayer, VectorIndex,
};

#[derive(Args, Debug)]
pub struct SearchArgs {
    /// Search query
    pub query: String,

    /// Maximum number of results
    #[arg(long, short, default_value = "20")]
    pub limit: usize,

    /// Filter by tags (comma-separated)
    #[arg(long, short)]
    pub tags: Option<String>,

    /// Filter by layer: system, global, project, local
    #[arg(long)]
    pub layer: Option<String>,

    /// Minimum quality score (0.0-1.0)
    #[arg(long)]
    pub min_quality: Option<f32>,

    /// Include deprecated skills
    #[arg(long)]
    pub include_deprecated: bool,

    /// Search type: hybrid (default), bm25, semantic
    #[arg(long, default_value = "hybrid")]
    pub search_type: String,

    /// Show snippets of matching content
    #[arg(long)]
    pub snippets: bool,
}

pub fn run(ctx: &AppContext, args: &SearchArgs) -> Result<()> {
    // Build search filters
    let mut filters = SearchFilters::new();

    if let Some(ref tags_str) = args.tags {
        filters = filters.tags(SearchFilters::parse_tags(tags_str));
    }

    if let Some(ref layer_str) = args.layer {
        if let Some(layer) = SearchLayer::from_str(layer_str) {
            filters = filters.layer(layer);
        } else {
            if ctx.robot_mode {
                println!(
                    "{}",
                    serde_json::json!({
                        "status": "error",
                        "message": format!("Invalid layer: {}. Valid: system, global, project, local", layer_str)
                    })
                );
            } else {
                println!(
                    "{} Invalid layer '{}'. Valid: system, global, project, local",
                    "!".yellow(),
                    layer_str
                );
            }
            return Ok(());
        }
    }

    if let Some(min_q) = args.min_quality {
        filters = filters.min_quality(min_q);
    }

    filters = filters.include_deprecated(args.include_deprecated);

    // Execute search
    match args.search_type.as_str() {
        "bm25" => search_bm25(ctx, args, &filters),
        "semantic" => search_semantic(ctx, args, &filters),
        "hybrid" | _ => search_hybrid(ctx, args, &filters),
    }
}

fn search_hybrid(ctx: &AppContext, args: &SearchArgs, filters: &SearchFilters) -> Result<()> {
    // Fetch enough results from both systems for fusion
    let fetch_limit = args.limit * 2;

    // BM25 search using SQLite FTS
    let bm25_ids = ctx.db.search_fts(&args.query, fetch_limit)?;

    // Build semantic search using embeddings
    let embedder = HashEmbedder::default();
    let query_embedding = embedder.embed(&args.query);

    // Load embeddings from database
    let mut vector_index = VectorIndex::new(embedder.dims());
    let all_skills = ctx.db.list_skills(1000, 0)?; // Get all skills for embedding lookup

    for skill in &all_skills {
        if let Ok(Some(emb)) = ctx.db.get_embedding(&skill.id) {
            vector_index.insert(&skill.id, emb.embedding);
        }
    }

    // Semantic search
    let semantic_results = vector_index.search(&query_embedding, fetch_limit);

    // Convert to (id, score) format
    let bm25_results: Vec<(String, f32)> = bm25_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.clone(), 1.0 / (i + 1) as f32)) // Convert rank to pseudo-score
        .collect();

    // RRF fusion
    let config = RrfConfig::default();
    let fused = fuse_simple(&bm25_results, &semantic_results, &config);

    // Fetch full skill records and apply filters
    let mut results = Vec::new();
    for (skill_id, score) in fused {
        if let Some(skill) = ctx.db.get_skill(&skill_id)? {
            // Parse tags from metadata
            let skill_tags = parse_tags_from_metadata(&skill.metadata_json);

            // Apply filters
            if filters.matches(
                &skill_tags,
                &skill.source_layer,
                skill.quality_score as f32,
                skill.is_deprecated,
            ) {
                results.push((skill, score));
            }
        }

        if results.len() >= args.limit {
            break;
        }
    }

    display_results(ctx, &results, args, "hybrid")
}

fn search_bm25(ctx: &AppContext, args: &SearchArgs, filters: &SearchFilters) -> Result<()> {
    let ids = ctx.db.search_fts(&args.query, args.limit * 2)?;

    let mut results = Vec::new();
    for (i, id) in ids.iter().enumerate() {
        if let Some(skill) = ctx.db.get_skill(id)? {
            let skill_tags = parse_tags_from_metadata(&skill.metadata_json);

            if filters.matches(
                &skill_tags,
                &skill.source_layer,
                skill.quality_score as f32,
                skill.is_deprecated,
            ) {
                let score = 1.0 / (i + 1) as f32;
                results.push((skill, score));
            }
        }

        if results.len() >= args.limit {
            break;
        }
    }

    display_results(ctx, &results, args, "bm25")
}

fn search_semantic(ctx: &AppContext, args: &SearchArgs, filters: &SearchFilters) -> Result<()> {
    let embedder = HashEmbedder::default();
    let query_embedding = embedder.embed(&args.query);

    // Load embeddings
    let mut vector_index = VectorIndex::new(embedder.dims());
    let all_skills = ctx.db.list_skills(1000, 0)?;

    for skill in &all_skills {
        if let Ok(Some(emb)) = ctx.db.get_embedding(&skill.id) {
            vector_index.insert(&skill.id, emb.embedding);
        }
    }

    let search_results = vector_index.search(&query_embedding, args.limit * 2);

    let mut results = Vec::new();
    for (skill_id, score) in search_results {
        if let Some(skill) = ctx.db.get_skill(&skill_id)? {
            let skill_tags = parse_tags_from_metadata(&skill.metadata_json);

            if filters.matches(
                &skill_tags,
                &skill.source_layer,
                skill.quality_score as f32,
                skill.is_deprecated,
            ) {
                results.push((skill, score));
            }
        }

        if results.len() >= args.limit {
            break;
        }
    }

    display_results(ctx, &results, args, "semantic")
}

fn display_results(
    ctx: &AppContext,
    results: &[(crate::storage::sqlite::SkillRecord, f32)],
    args: &SearchArgs,
    search_type: &str,
) -> Result<()> {
    if ctx.robot_mode {
        let output: Vec<serde_json::Value> = results
            .iter()
            .map(|(skill, score)| {
                serde_json::json!({
                    "id": skill.id,
                    "name": skill.name,
                    "description": skill.description,
                    "layer": skill.source_layer,
                    "score": score,
                    "quality": skill.quality_score,
                    "is_deprecated": skill.is_deprecated,
                })
            })
            .collect();

        println!(
            "{}",
            serde_json::json!({
                "status": "ok",
                "query": args.query,
                "search_type": search_type,
                "count": results.len(),
                "limit": args.limit,
                "results": output
            })
        );
    } else if results.is_empty() {
        println!(
            "{} No skills found for '{}'",
            "!".yellow(),
            args.query.cyan()
        );
        println!();
        println!("Try:");
        println!("  - Using different keywords");
        println!("  - Removing filters (--tags, --layer, --min-quality)");
        println!("  - Including deprecated skills: --include-deprecated");
    } else {
        println!(
            "{} results for '{}' ({} search):",
            results.len().to_string().bold(),
            args.query.cyan(),
            search_type
        );
        println!();

        for (i, (skill, score)) in results.iter().enumerate() {
            let rank = format!("{}.", i + 1);
            let layer = skill.source_layer.as_str();
            let layer_colored = match layer {
                "system" => layer.blue(),
                "global" => layer.green(),
                "project" => layer.yellow(),
                "local" => layer.magenta(),
                _ => layer.normal(),
            };

            let deprecated_marker = if skill.is_deprecated {
                " [deprecated]".red().to_string()
            } else {
                String::new()
            };

            println!(
                "{:4} {} {}{}",
                rank.dimmed(),
                skill.name.bold(),
                layer_colored,
                deprecated_marker
            );
            println!(
                "     {} (score: {:.3}, quality: {:.2})",
                skill.id.dimmed(),
                score,
                skill.quality_score
            );

            // Show description (truncated)
            if !skill.description.is_empty() {
                let desc = if skill.description.len() > 80 {
                    format!("{}...", &skill.description[..77])
                } else {
                    skill.description.clone()
                };
                println!("     {}", desc.dimmed());
            }

            if args.snippets && !skill.body.is_empty() {
                // Find relevant snippet
                if let Some(snippet) = find_snippet(&skill.body, &args.query) {
                    println!("     \"{}\"", snippet.italic());
                }
            }

            println!();
        }
    }

    Ok(())
}

fn parse_tags_from_metadata(metadata_json: &str) -> Vec<String> {
    if let Ok(meta) = serde_json::from_str::<serde_json::Value>(metadata_json) {
        if let Some(tags) = meta.get("tags").and_then(|t| t.as_array()) {
            return tags
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
        }
    }
    Vec::new()
}

fn find_snippet(body: &str, query: &str) -> Option<String> {
    let query_lower = query.to_lowercase();
    let body_lower = body.to_lowercase();

    // Find first occurrence of any query word
    for word in query_lower.split_whitespace() {
        if let Some(pos) = body_lower.find(word) {
            // Extract context around the match
            let start = pos.saturating_sub(30);
            let end = (pos + word.len() + 50).min(body.len());

            // Find word boundaries
            let start = body[..start]
                .rfind(|c: char| c.is_whitespace())
                .map(|p| p + 1)
                .unwrap_or(start);
            let end = body[end..]
                .find(|c: char| c.is_whitespace())
                .map(|p| end + p)
                .unwrap_or(end);

            let snippet = body[start..end].trim();
            if !snippet.is_empty() {
                let prefix = if start > 0 { "..." } else { "" };
                let suffix = if end < body.len() { "..." } else { "" };
                return Some(format!("{}{}{}", prefix, snippet, suffix));
            }
        }
    }
    None
}
