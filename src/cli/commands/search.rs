//! ms search - Search for skills
//!
//! Provides hybrid search combining BM25 full-text and semantic vector
//! similarity via RRF fusion.

use clap::Args;
use tracing::debug;

use crate::app::AppContext;
use crate::cli::formatters::SearchResults;
use crate::cli::output::{Formattable, OutputFormat};
use crate::error::{MsError, Result};
use crate::search::{
    RrfConfig, SearchFilters, SearchLayer, VectorIndex, build_embedder, fuse_simple,
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

    /// Filter by layer: base, org, project, user (aliases: system, global, local)
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
            let error_msg = format!(
                "Invalid layer '{}'. Valid: base, org, project, user",
                layer_str
            );
            match ctx.output_format {
                OutputFormat::Json | OutputFormat::Jsonl => {
                    println!(
                        "{}",
                        serde_json::json!({
                            "status": "error",
                            "message": error_msg
                        })
                    );
                }
                _ => {
                    println!("! {error_msg}");
                }
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
        "semantic" => {
            if !ctx.config.search.use_embeddings {
                return Err(MsError::Config(
                    "semantic search disabled (search.use_embeddings=false)".to_string(),
                ));
            }
            search_semantic(ctx, args, &filters)
        }
        "hybrid" | _ => {
            if !ctx.config.search.use_embeddings {
                return search_bm25(ctx, args, &filters);
            }
            search_hybrid(ctx, args, &filters)
        }
    }
}

fn search_hybrid(ctx: &AppContext, args: &SearchArgs, filters: &SearchFilters) -> Result<()> {
    // Fetch enough results from both systems for fusion
    // Increase limit to allow for filtering
    let fetch_limit = args.limit * 50;

    // BM25 search using SQLite FTS
    let bm25_candidates = ctx.db.search_fts(&args.query, fetch_limit)?;

    // Build semantic search using embeddings
    let embedder = build_embedder(&ctx.config.search)?;
    let query_embedding = embedder.embed(&args.query);

    // Load embeddings from database
    let mut vector_index = VectorIndex::new(embedder.dims());
    let all_embeddings = ctx.db.get_all_embeddings()?;

    for (id, embedding) in all_embeddings {
        let _ = vector_index.insert(id, embedding);
    }

    // Semantic search
    let semantic_results = vector_index.search(&query_embedding, fetch_limit);

    // Convert to (id, score) format
    let bm25_results: Vec<(String, f32)> = bm25_candidates
        .iter()
        .enumerate()
        .map(|(i, c)| (c.id.clone(), 1.0 / (i + 1) as f32)) // Convert rank to pseudo-score
        .collect();

    // RRF fusion
    let config = RrfConfig::with_weights(
        ctx.config.search.bm25_weight,
        ctx.config.search.semantic_weight,
    );
    let fused = fuse_simple(&bm25_results, &semantic_results, &config);

    // Fetch full skill records and apply filters
    let mut results = Vec::new();
    for (skill_id, score) in fused {
        // Optimization: Check metadata first
        let candidate = if let Some(c) = bm25_candidates.iter().find(|c| c.id == skill_id) {
            Some(c.clone())
        } else {
            ctx.db.get_skill_candidate(&skill_id)?
        };

        if let Some(candidate) = candidate {
            let skill_tags = parse_tags_from_metadata(&candidate.metadata_json);

            // Apply filters on metadata
            if filters.matches(
                &skill_tags,
                &candidate.source_layer,
                candidate.quality_score as f32,
                candidate.is_deprecated,
            ) {
                // Only load full skill if it passes filters
                if let Some(skill) = ctx.db.get_skill(&skill_id)? {
                    results.push((skill, score));
                }
            }
        }

        if results.len() >= args.limit {
            break;
        }
    }

    display_results(ctx, &results, args, "hybrid")
}

fn search_bm25(ctx: &AppContext, args: &SearchArgs, filters: &SearchFilters) -> Result<()> {
    // Increase limit to allow for filtering
    let candidates = ctx.db.search_fts(&args.query, args.limit * 50)?;

    let mut results = Vec::new();
    for (i, candidate) in candidates.iter().enumerate() {
        let skill_tags = parse_tags_from_metadata(&candidate.metadata_json);

        if filters.matches(
            &skill_tags,
            &candidate.source_layer,
            candidate.quality_score as f32,
            candidate.is_deprecated,
        ) {
            if let Some(skill) = ctx.db.get_skill(&candidate.id)? {
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
    let embedder = build_embedder(&ctx.config.search)?;
    let query_embedding = embedder.embed(&args.query);

    // Load embeddings
    let mut vector_index = VectorIndex::new(embedder.dims());
    let all_embeddings = ctx.db.get_all_embeddings()?;

    for (id, embedding) in all_embeddings {
        let _ = vector_index.insert(id, embedding);
    }

    // Search more to allow filtering
    let search_results = vector_index.search(&query_embedding, args.limit * 50);

    let mut results = Vec::new();
    for (skill_id, score) in search_results {
        // Fetch metadata first
        if let Some(candidate) = ctx.db.get_skill_candidate(&skill_id)? {
            let skill_tags = parse_tags_from_metadata(&candidate.metadata_json);

            if filters.matches(
                &skill_tags,
                &candidate.source_layer,
                candidate.quality_score as f32,
                candidate.is_deprecated,
            ) {
                if let Some(skill) = ctx.db.get_skill(&skill_id)? {
                    results.push((skill, score));
                }
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
    debug!(target: "search", stage = "render_start");
    debug!(target: "search", results = results.len(), "rendering results");
    debug!(target: "search", mode = ?ctx.output_format, "output mode selected");

    let start = std::time::Instant::now();

    // Build SearchResults using the new formatter
    let mut search_results = SearchResults::from_tuples(&args.query, search_type, results);

    // Add snippets if requested
    if args.snippets {
        for (i, (skill, _)) in results.iter().enumerate() {
            if !skill.body.is_empty() {
                if let Some(snippet) = find_snippet(&skill.body, &args.query) {
                    if i < search_results.results.len() {
                        search_results.results[i].snippet = Some(snippet);
                    }
                }
            }
        }
    }

    // Use the new output format
    println!("{}", search_results.format(ctx.output_format));

    let elapsed = start.elapsed();
    debug!(
        target: "search",
        stage = "render_complete",
        duration_ms = elapsed.as_millis() as u64,
    );

    Ok(())
}

fn parse_tags_from_metadata(metadata_json: &str) -> Vec<String> {
    if let Ok(meta) = serde_json::from_str::<serde_json::Value>(metadata_json) {
        if let Some(tags) = meta.get("tags").and_then(|t| t.as_array()) {
            return tags
                .iter()
                .filter_map(|v| v.as_str().map(str::to_lowercase))
                .collect();
        }
    }
    Vec::new()
}

fn find_snippet(body: &str, query: &str) -> Option<String> {
    let query_lower = query.to_lowercase();
    let body_chars: Vec<char> = body.chars().collect();
    let total_chars = body_chars.len();

    for word in query_lower.split_whitespace() {
        for (char_idx, (byte_idx, _)) in body.char_indices().enumerate() {
            if is_match_at(body, byte_idx, word) {
                let source_len = count_source_chars_consumed(body, byte_idx, word);

                let start_char = char_idx.saturating_sub(30);
                let end_char = (char_idx + source_len + 50).min(total_chars);

                // Find word boundaries (scan for whitespace)
                let start_char = body_chars[..start_char]
                    .iter()
                    .rposition(|c| c.is_whitespace())
                    .map_or(start_char, |p| p + 1);
                let end_char = body_chars[end_char..]
                    .iter()
                    .position(|c| c.is_whitespace())
                    .map_or(end_char, |p| end_char + p);

                let snippet: String = body_chars[start_char..end_char].iter().collect();
                let snippet = snippet.trim();
                if !snippet.is_empty() {
                    let prefix = if start_char > 0 { "..." } else { "" };
                    let suffix = if end_char < total_chars { "..." } else { "" };
                    return Some(format!("{prefix}{snippet}{suffix}"));
                }
            }
        }
    }
    None
}

fn is_match_at(body: &str, start_byte: usize, word_lower: &str) -> bool {
    let slice = &body[start_byte..];
    let mut slice_chars = slice.chars().flat_map(char::to_lowercase);
    let mut word_chars = word_lower.chars();

    loop {
        match (slice_chars.next(), word_chars.next()) {
            (Some(sc), Some(wc)) => {
                if sc != wc {
                    return false;
                }
            }
            (None, Some(_)) => return false, // slice ended before word
            (_, None) => return true,        // word ended, match!
        }
    }
}

fn count_source_chars_consumed(body: &str, start_byte: usize, word_lower: &str) -> usize {
    let slice = &body[start_byte..];
    let mut slice_chars = slice.chars();
    let mut consumed_count = 0;
    let mut matched_lower_count = 0;
    let target_count = word_lower.chars().count();

    while matched_lower_count < target_count {
        if let Some(c) = slice_chars.next() {
            consumed_count += 1;
            matched_lower_count += c.to_lowercase().count();
        } else {
            break;
        }
    }
    consumed_count
}

/// Truncate a string to a maximum number of characters (not bytes), safe for UTF-8
#[cfg(test)]
fn truncate_str(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== truncate_str Tests ====================

    #[test]
    fn test_truncate_str_short() {
        assert_eq!(truncate_str("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_str_exact() {
        assert_eq!(truncate_str("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_str_truncated() {
        assert_eq!(truncate_str("hello world", 5), "hello");
    }

    #[test]
    fn test_truncate_str_empty() {
        assert_eq!(truncate_str("", 10), "");
    }

    #[test]
    fn test_truncate_str_unicode() {
        let emoji_str = "🦀🐍🚀";
        assert_eq!(truncate_str(emoji_str, 2), "🦀🐍");
    }

    // ==================== parse_tags_from_metadata Tests ====================

    #[test]
    fn test_parse_tags_valid_json() {
        let metadata = r#"{"tags": ["rust", "cli", "testing"]}"#;
        let tags = parse_tags_from_metadata(metadata);
        assert_eq!(tags, vec!["rust", "cli", "testing"]);
    }

    #[test]
    fn test_parse_tags_empty_array() {
        let metadata = r#"{"tags": []}"#;
        let tags = parse_tags_from_metadata(metadata);
        assert!(tags.is_empty());
    }

    #[test]
    fn test_parse_tags_no_tags_field() {
        let metadata = r#"{"name": "test"}"#;
        let tags = parse_tags_from_metadata(metadata);
        assert!(tags.is_empty());
    }

    #[test]
    fn test_parse_tags_invalid_json() {
        let tags = parse_tags_from_metadata("not valid json");
        assert!(tags.is_empty());
    }

    // ==================== find_snippet Tests ====================

    #[test]
    fn test_find_snippet_simple_match() {
        let body = "This is a test of the search functionality.";
        let snippet = find_snippet(body, "search");
        assert!(snippet.is_some());
        assert!(snippet.unwrap().contains("search"));
    }

    #[test]
    fn test_find_snippet_no_match() {
        let body = "This is a test.";
        let snippet = find_snippet(body, "notfound");
        assert!(snippet.is_none());
    }

    #[test]
    fn test_find_snippet_case_insensitive() {
        let body = "This is a TEST of Search functionality.";
        let snippet = find_snippet(body, "search");
        assert!(snippet.is_some());
    }

    // ==================== Argument Parsing Tests ====================

    #[test]
    fn test_search_args_defaults() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            args: SearchArgs,
        }

        let parsed = TestCli::parse_from(["test", "rust error handling"]);
        assert_eq!(parsed.args.query, "rust error handling");
        assert_eq!(parsed.args.limit, 20);
        assert_eq!(parsed.args.search_type, "hybrid");
    }

    #[test]
    fn test_search_args_with_options() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            args: SearchArgs,
        }

        let parsed = TestCli::parse_from([
            "test",
            "query",
            "--limit",
            "10",
            "--tags",
            "rust",
            "--layer",
            "base",
            "--min-quality",
            "0.5",
            "--include-deprecated",
            "--snippets",
        ]);

        assert_eq!(parsed.args.limit, 10);
        assert_eq!(parsed.args.tags, Some("rust".to_string()));
        assert_eq!(parsed.args.layer, Some("base".to_string()));
        assert_eq!(parsed.args.min_quality, Some(0.5));
        assert!(parsed.args.include_deprecated);
        assert!(parsed.args.snippets);
    }

    #[test]
    fn test_search_args_search_types() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            args: SearchArgs,
        }

        let bm25 = TestCli::parse_from(["test", "query", "--search-type", "bm25"]);
        assert_eq!(bm25.args.search_type, "bm25");

        let semantic = TestCli::parse_from(["test", "query", "--search-type", "semantic"]);
        assert_eq!(semantic.args.search_type, "semantic");
    }

    #[test]
    fn test_search_args_short_flags() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(flatten)]
            args: SearchArgs,
        }

        let parsed = TestCli::parse_from(["test", "query", "-l", "5", "-t", "testing"]);
        assert_eq!(parsed.args.limit, 5);
        assert_eq!(parsed.args.tags, Some("testing".to_string()));
    }

    #[test]
    fn test_find_snippet_unicode_expansion_bug() {
        // "İ" (U+0130) lowercases to "i\u{307}" (U+0069 U+0307)
        // Original: 1 char. Lower: 2 chars.

        // Create a string with enough expanding characters to offset the index
        // beyond the length of the original string.
        let mut body = String::new();
        for _ in 0..50 {
            body.push('İ');
        }
        body.push_str(" final");

        // body len: 50 + 6 = 56 chars.
        // body_lower len: 100 + 6 = 106 chars.

        // "final" found at char index 101 in lower.
        // But body only has 56 chars.
        // This should panic if the bug exists.

        let snippet = find_snippet(&body, "final");
        assert!(snippet.is_some());
        let s = snippet.unwrap();
        assert!(s.contains("final"), "Should contain 'final', found {:?}", s);
    }
}
