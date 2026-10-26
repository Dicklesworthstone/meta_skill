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
    FilterResult, HybridResult, RrfConfig, SearchContext, SearchFilters, SearchLayer, VectorIndex,
    admit_semantic_candidates, build_embedder, fuse_results, matches_skill_record,
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

    /// Show detailed ranking explanation (BM25 rank/score, semantic rank/score, RRF contribution)
    #[arg(long)]
    pub explain: bool,
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

/// Ranked lexical (BM25) candidates for the CLI search path.
///
/// Prefers the Tantivy BM25 index (`ctx.search`), which ranks by true BM25
/// relevance across name/description/body/tags/aliases — the same engine the
/// MCP server uses. Falls back to the SQLite substring scan only when the
/// index is unavailable: never built / empty (e.g. a state dir produced by an
/// older binary) or erroring (corrupt segment, unparsable query syntax). The
/// fallback assigns descending pseudo-scores so downstream RRF fusion still
/// sees a rank ordering (issue #144).
fn bm25_ranked(
    ctx: &AppContext,
    query: &str,
    layer_filter: Option<&str>,
    fetch_limit: usize,
) -> Result<Vec<(String, f32)>> {
    if ctx.search.is_empty() {
        debug!(
            target: "search",
            "bm25: tantivy index empty; falling back to substring scan"
        );
    } else {
        let search_res = if let Some(layer) = layer_filter {
            ctx.search.search_with_layer(query, layer, fetch_limit)
        } else {
            ctx.search.search(query, fetch_limit)
        };
        match search_res {
            Ok(hits) => {
                debug!(target: "search", backend = "tantivy", hits = hits.len(), "bm25 candidates");
                return Ok(hits.into_iter().map(|r| (r.skill_id, r.score)).collect());
            }
            Err(err) => {
                debug!(
                    target: "search",
                    error = %err,
                    "bm25: tantivy search failed; falling back to substring scan"
                );
            }
        }
    }

    let candidates = ctx.db.search_fts(query, fetch_limit)?;
    debug!(target: "search", backend = "substring", hits = candidates.len(), "bm25 candidates");
    Ok(candidates
        .into_iter()
        .enumerate()
        .map(|(i, c)| (c.id, 1.0 / (i + 1) as f32)) // Convert rank to pseudo-score
        .collect())
}

fn search_hybrid(ctx: &AppContext, args: &SearchArgs, filters: &SearchFilters) -> Result<()> {
    // Fetch enough results from both systems for fusion
    // Increase limit to allow for filtering
    let fetch_limit = args.limit * 50;

    let config = RrfConfig::with_weights(
        ctx.config.search.bm25_weight,
        ctx.config.search.semantic_weight,
    );

    let fused: Vec<HybridResult> =
        if let Some(cached) = ctx.cache.get_query(&args.query, fetch_limit) {
            cached
        } else {
            // BM25 search (Tantivy, with substring-scan fallback)
            let bm25_results = bm25_ranked(ctx, &args.query, args.layer.as_deref(), fetch_limit)?;

            // Build semantic search using embeddings (with cache lookup)
            let embedder = build_embedder(&ctx.config.search)?;
            let query_cache_key = format!("__query__:{}", embedder.name());
            let query_embedding = ctx
                .cache
                .get_embedding(&query_cache_key, &args.query)
                .unwrap_or_else(|| {
                    let emb = embedder.embed(&args.query);
                    ctx.cache
                        .put_embedding(&query_cache_key, &args.query, emb.clone());
                    emb
                });

            // Load the embeddings that live in the query vector's space
            let mut vector_index = VectorIndex::new(embedder.dims());
            let all_embeddings = ctx
                .db
                .get_embeddings_for(embedder.name(), embedder.dims())?;

            for (id, embedding) in all_embeddings {
                let _ = vector_index.insert(id, embedding);
            }

            // Semantic search. Only candidates with relevance evidence reach RRF; the
            // feature-hash embedder is lexical, so it re-ranks but never adds hits.
            let semantic_results = admit_semantic_candidates(
                &bm25_results,
                vector_index.search(&query_embedding, fetch_limit),
                embedder.name() == "hash",
            );

            let fused = fuse_results(&bm25_results, &semantic_results, &config);
            ctx.cache.put_query(&args.query, fetch_limit, fused.clone());
            fused
        };

    let search_ctx = build_search_context(ctx);
    let mut filter_audit = FilterResult::new(fused.len(), filters);

    // Fetch full skill records and apply filters + SearchContext tie-breaking boost
    let mut results = Vec::new();
    let mut explanations = Vec::new();
    for mut hit in fused {
        // Check lightweight metadata first; only load the full skill if it
        // passes the filters.
        if let Some(candidate) = ctx.db.get_skill_candidate(&hit.skill_id)? {
            let skill_tags = parse_tags_from_metadata(&candidate.metadata_json);

            // Apply filters on metadata and record filter audit counts
            if filter_audit.evaluate_candidate(
                &skill_tags,
                &candidate.source_layer,
                candidate.quality_score as f32,
                candidate.is_deprecated,
            ) {
                if let Some(skill) = ctx.db.get_skill(&hit.skill_id)? {
                    if matches_skill_record(filters, &skill) {
                        hit.score += search_ctx.boost_for(&skill.id, &skill_tags);
                        results.push((skill, hit.score));
                        explanations.push(hit);
                    }
                }
            }
        }

        if results.len() >= args.limit {
            break;
        }
    }

    if args.explain {
        return display_explained_results(
            ctx,
            &results,
            &explanations,
            &config,
            &filter_audit,
            &search_ctx,
            args,
            "hybrid",
        );
    }

    display_results(ctx, &results, args, "hybrid")
}

fn search_bm25(ctx: &AppContext, args: &SearchArgs, filters: &SearchFilters) -> Result<()> {
    // Increase limit to allow for filtering
    let ranked = bm25_ranked(ctx, &args.query, args.layer.as_deref(), args.limit * 50)?;
    let search_ctx = build_search_context(ctx);
    let mut filter_audit = FilterResult::new(ranked.len(), filters);

    let mut results = Vec::new();
    let mut explanations = Vec::new();
    for (idx, (skill_id, score)) in ranked.into_iter().enumerate() {
        if let Some(candidate) = ctx.db.get_skill_candidate(&skill_id)? {
            let skill_tags = parse_tags_from_metadata(&candidate.metadata_json);

            if filter_audit.evaluate_candidate(
                &skill_tags,
                &candidate.source_layer,
                candidate.quality_score as f32,
                candidate.is_deprecated,
            ) {
                if let Some(skill) = ctx.db.get_skill(&skill_id)? {
                    if matches_skill_record(filters, &skill) {
                        results.push((skill, score));
                        explanations.push(HybridResult {
                            skill_id,
                            score,
                            bm25_rank: Some(idx + 1),
                            semantic_rank: None,
                            bm25_score: Some(score),
                            semantic_score: None,
                        });
                    }
                }
            }
        }

        if results.len() >= args.limit {
            break;
        }
    }

    if args.explain {
        let config = RrfConfig::with_weights(ctx.config.search.bm25_weight, 0.0);
        return display_explained_results(
            ctx,
            &results,
            &explanations,
            &config,
            &filter_audit,
            &search_ctx,
            args,
            "bm25",
        );
    }

    display_results(ctx, &results, args, "bm25")
}

fn search_semantic(ctx: &AppContext, args: &SearchArgs, filters: &SearchFilters) -> Result<()> {
    let embedder = build_embedder(&ctx.config.search)?;
    let query_cache_key = format!("__query__:{}", embedder.name());
    let query_embedding = if let Some(q) = ctx
        .cache
        .get_embedding_quantized(&query_cache_key, &args.query)
    {
        crate::search::embeddings::dequantize_f16(&q)
    } else {
        let emb = embedder.embed(&args.query);
        ctx.cache.put_embedding_quantized(
            &query_cache_key,
            &args.query,
            crate::search::embeddings::quantize_f16(&emb),
        );
        emb
    };

    // Load the embeddings that live in the query vector's space
    let mut vector_index = VectorIndex::new(embedder.dims());
    let all_embeddings = ctx
        .db
        .get_embeddings_for(embedder.name(), embedder.dims())?;

    for (id, embedding) in all_embeddings {
        if let Some(q) = ctx.cache.get_embedding_quantized(&id, embedder.name()) {
            let _ = vector_index.insert_quantized(id, &q);
        } else {
            let _ = vector_index.insert(id.clone(), embedding);
            if let Some(q) = vector_index.get_quantized(&id) {
                ctx.cache.put_embedding_quantized(&id, embedder.name(), q);
            }
        }
    }

    // Search more to allow filtering
    let search_results = vector_index.search(&query_embedding, args.limit * 50);
    let search_ctx = build_search_context(ctx);
    let mut filter_audit = FilterResult::new(search_results.len(), filters);

    let mut results = Vec::new();
    let mut explanations = Vec::new();
    for (idx, (skill_id, score)) in search_results.into_iter().enumerate() {
        // Fetch metadata first
        if let Some(candidate) = ctx.db.get_skill_candidate(&skill_id)? {
            let skill_tags = parse_tags_from_metadata(&candidate.metadata_json);

            if filter_audit.evaluate_candidate(
                &skill_tags,
                &candidate.source_layer,
                candidate.quality_score as f32,
                candidate.is_deprecated,
            ) {
                if let Some(skill) = ctx.db.get_skill(&skill_id)? {
                    if matches_skill_record(filters, &skill) {
                        results.push((skill, score));
                        explanations.push(HybridResult {
                            skill_id,
                            score,
                            bm25_rank: None,
                            semantic_rank: Some(idx + 1),
                            bm25_score: None,
                            semantic_score: Some(score),
                        });
                    }
                }
            }
        }

        if results.len() >= args.limit {
            break;
        }
    }

    if args.explain {
        let config = RrfConfig::with_weights(0.0, ctx.config.search.semantic_weight);
        return display_explained_results(
            ctx,
            &results,
            &explanations,
            &config,
            &filter_audit,
            &search_ctx,
            args,
            "semantic",
        );
    }

    display_results(ctx, &results, args, "semantic")
}

fn build_search_context(ctx: &AppContext) -> SearchContext {
    use crate::context::{DefaultDetector, ProjectDetector, ProjectMarker, ProjectType};
    let cwd_path = std::env::current_dir().ok();
    let tech_stack = cwd_path
        .as_deref()
        .map(|dir| {
            let mut detector = DefaultDetector::new();
            detector.add_marker(ProjectMarker::new(
                "rust-toolchain.toml",
                ProjectType::Rust,
                0.9,
            ));
            detector
                .detect(dir)
                .into_iter()
                .map(|d| d.project_type.name().to_lowercase())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let recent_skills = ctx
        .db
        .get_skill_usage_stats()
        .map(|(_, counts, _)| {
            let mut pairs: Vec<(String, u64)> =
                counts.into_iter().filter(|(_, c)| *c > 0).collect();
            pairs.sort_by(|a, b| b.1.cmp(&a.1));
            pairs.into_iter().take(5).map(|(id, _)| id).collect()
        })
        .unwrap_or_default();
    SearchContext {
        cwd: cwd_path.map(|p| p.display().to_string()),
        recent_skills,
        tech_stack,
    }
}

#[allow(clippy::too_many_arguments)]
fn display_explained_results(
    ctx: &AppContext,
    results: &[(crate::storage::sqlite::SkillRecord, f32)],
    explanations: &[HybridResult],
    config: &RrfConfig,
    filter_audit: &FilterResult,
    search_ctx: &SearchContext,
    args: &SearchArgs,
    search_type: &str,
) -> Result<()> {
    if ctx.output_format != OutputFormat::Human {
        let items: Vec<serde_json::Value> = results
            .iter()
            .zip(explanations.iter())
            .map(|((skill, score), exp)| {
                let bm25_rrf = exp
                    .bm25_rank
                    .map(|r| config.bm25_weight / (config.k + r as f32))
                    .unwrap_or(0.0);
                let semantic_rrf = exp
                    .semantic_rank
                    .map(|r| config.semantic_weight / (config.k + r as f32))
                    .unwrap_or(0.0);
                let skill_tags = parse_tags_from_metadata(&skill.metadata_json);
                let context_boost = search_ctx.boost_for(&skill.id, &skill_tags);
                serde_json::json!({
                    "id": skill.id,
                    "name": skill.name,
                    "description": skill.description,
                    "score": score,
                    "layer": skill.source_layer,
                    "quality_score": skill.quality_score,
                    "explanation": {
                        "bm25_rank": exp.bm25_rank,
                        "bm25_score": exp.bm25_score,
                        "bm25_rrf_contribution": bm25_rrf,
                        "semantic_rank": exp.semantic_rank,
                        "semantic_score": exp.semantic_score,
                        "semantic_rrf_contribution": semantic_rrf,
                        "context_boost": context_boost,
                        "rrf_k": config.k,
                        "bm25_weight": config.bm25_weight,
                        "semantic_weight": config.semantic_weight,
                    }
                })
            })
            .collect();
        let cache_stats = ctx.cache.stats();
        let (query_cache_size, embedding_cache_size, fingerprint_cache_size) = ctx.cache.sizes();
        let payload = serde_json::json!({
            "status": "ok",
            "query": args.query,
            "search_type": search_type,
            "count": items.len(),
            "filter_audit": filter_audit,
            "search_context": search_ctx,
            "cache": {
                "query_hit_rate": cache_stats.query_hit_rate(),
                "embedding_hit_rate": cache_stats.embedding_hit_rate(),
                "fingerprint_hit_rate": cache_stats.fingerprint_hit_rate(),
                "sizes": {
                    "query": query_cache_size,
                    "embedding": embedding_cache_size,
                    "fingerprint": fingerprint_cache_size,
                }
            },
            "results": items,
        });
        return crate::cli::output::emit_json(&payload);
    }

    let cache_stats = ctx.cache.stats();
    let (query_cache_size, embedding_cache_size, fingerprint_cache_size) = ctx.cache.sizes();
    println!(
        "Search Explain for '{}' (mode: {}, k={}, bm25_w={:.2}, sem_w={:.2}):",
        args.query, search_type, config.k, config.bm25_weight, config.semantic_weight
    );
    println!(
        "  Filter Audit: before={}, passed={}, filtered=(tags={}, layer={}, quality={}, deprecated={})",
        filter_audit.total_before,
        filter_audit.total_after,
        filter_audit.filtered_by_tags,
        filter_audit.filtered_by_layer,
        filter_audit.filtered_by_quality,
        filter_audit.filtered_by_deprecated
    );
    println!(
        "  Cache: entries=(q={}, emb={}, fp={}), hit_rates=(q={:.1}%, emb={:.1}%, fp={:.1}%)",
        query_cache_size,
        embedding_cache_size,
        fingerprint_cache_size,
        cache_stats.query_hit_rate() * 100.0,
        cache_stats.embedding_hit_rate() * 100.0,
        cache_stats.fingerprint_hit_rate() * 100.0
    );
    for ((skill, score), exp) in results.iter().zip(explanations.iter()) {
        let bm25_rrf = exp
            .bm25_rank
            .map(|r| config.bm25_weight / (config.k + r as f32))
            .unwrap_or(0.0);
        let semantic_rrf = exp
            .semantic_rank
            .map(|r| config.semantic_weight / (config.k + r as f32))
            .unwrap_or(0.0);
        println!("  - {} ({}) [score: {:.4}]", skill.id, skill.name, score);
        println!(
            "    BM25: rank={:?}, raw={:?}, rrf={:.4} | Semantic: rank={:?}, raw={:?}, rrf={:.4}",
            exp.bm25_rank,
            exp.bm25_score,
            bm25_rrf,
            exp.semantic_rank,
            exp.semantic_score,
            semantic_rrf
        );
    }
    Ok(())
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

    // Build SearchResults using the formatter
    let mut search_results = if args.snippets {
        let mut sr = SearchResults::new(&args.query, search_type);
        for (skill, score) in results {
            if !skill.body.is_empty()
                && let Some(snippet) = find_snippet(&skill.body, &args.query)
            {
                sr.add_result_with_snippet(skill.clone(), *score, snippet);
            } else {
                sr.add_result(skill.clone(), *score);
            }
        }
        sr
    } else {
        SearchResults::from_tuples(&args.query, search_type, results)
    };
    search_results = search_results.with_duration(start.elapsed().as_millis() as u64);

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
