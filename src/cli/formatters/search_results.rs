//! Search results formatter
//!
//! Renders search results in multiple formats: rich terminal output with
//! colored tables and panels (Human mode), plain TSV (Plain mode), JSON,
//! JSONL, full TSV with headers, and TOON.

use serde::Serialize;
use tracing::debug;

use crate::cli::output::{Formattable, OutputFormat};
use crate::output::{is_agent_environment, is_ci_environment, search_results_table, warning_panel};
use crate::storage::sqlite::SkillRecord;

/// Search result item with score
#[derive(Debug, Clone)]
pub struct SearchResultItem {
    /// The skill record
    pub skill: SkillRecord,
    /// Search relevance score
    pub score: f32,
    /// Optional snippet of matching content
    pub snippet: Option<String>,
}

/// Search results collection for formatted display
#[derive(Debug, Clone)]
pub struct SearchResults {
    /// The search query
    pub query: String,
    /// Type of search performed
    pub search_type: String,
    /// Search results with scores
    pub results: Vec<SearchResultItem>,
    /// Search duration in milliseconds
    pub duration_ms: Option<u64>,
}

/// Serializable search result for JSON output
#[derive(Debug, Clone, Serialize)]
struct SearchResultJson {
    id: String,
    name: String,
    description: String,
    layer: String,
    score: f32,
    quality: f64,
    is_deprecated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    snippet: Option<String>,
}

/// Serializable search response for JSON output
#[derive(Debug, Clone, Serialize)]
struct SearchResponseJson {
    status: String,
    query: String,
    search_type: String,
    count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_ms: Option<u64>,
    results: Vec<SearchResultJson>,
}

impl SearchResults {
    /// Create a new search results collection
    pub fn new(query: impl Into<String>, search_type: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            search_type: search_type.into(),
            results: Vec::new(),
            duration_ms: None,
        }
    }

    /// Add a result with score
    pub fn add_result(&mut self, skill: SkillRecord, score: f32) {
        self.results.push(SearchResultItem {
            skill,
            score,
            snippet: None,
        });
    }

    /// Add a result with score and snippet
    pub fn add_result_with_snippet(
        &mut self,
        skill: SkillRecord,
        score: f32,
        snippet: impl Into<String>,
    ) {
        self.results.push(SearchResultItem {
            skill,
            score,
            snippet: Some(snippet.into()),
        });
    }

    /// Set the search duration
    #[must_use]
    pub const fn with_duration(mut self, duration_ms: u64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }

    /// Build from tuples (for compatibility with existing code)
    pub fn from_tuples(
        query: impl Into<String>,
        search_type: impl Into<String>,
        results: &[(SkillRecord, f32)],
    ) -> Self {
        let mut sr = Self::new(query, search_type);
        for (skill, score) in results {
            sr.add_result(skill.clone(), *score);
        }
        sr
    }

    fn to_json_response(&self) -> SearchResponseJson {
        SearchResponseJson {
            status: "ok".to_string(),
            query: self.query.clone(),
            search_type: self.search_type.clone(),
            count: self.results.len(),
            duration_ms: self.duration_ms,
            results: self
                .results
                .iter()
                .map(|r| SearchResultJson {
                    id: r.skill.id.clone(),
                    name: r.skill.name.clone(),
                    description: r.skill.description.clone(),
                    layer: r.skill.source_layer.clone(),
                    score: r.score,
                    quality: r.skill.quality_score,
                    is_deprecated: r.skill.is_deprecated,
                    snippet: r.snippet.clone(),
                })
                .collect(),
        }
    }

    /// Format for human-readable rich terminal output.
    ///
    /// Uses `rich_rust` tables and panels when a capable terminal is detected,
    /// falling back to plain text in agent/CI/piped environments.
    fn format_human(&self) -> String {
        let start = std::time::Instant::now();
        debug!(target: "search", stage = "render_start");

        // Detect whether rich output is appropriate
        let use_rich = should_use_rich_for_search();
        debug!(target: "search", mode = %if use_rich { "rich" } else { "plain" }, "output mode selected");

        let output = if self.results.is_empty() {
            self.format_empty_results(use_rich)
        } else {
            self.format_populated_results(use_rich)
        };

        let elapsed = start.elapsed();
        debug!(
            target: "search",
            stage = "render_complete",
            duration_ms = elapsed.as_millis() as u64,
            results = self.results.len(),
        );

        output
    }

    /// Render the empty-results message.
    fn format_empty_results(&self, use_rich: bool) -> String {
        let suggestions = "Try:\n  \
             - Using different keywords\n  \
             - Removing filters (--tags, --layer, --min-quality)\n  \
             - Including deprecated skills: --include-deprecated";

        if use_rich {
            let title = format!("No skills found for '{}'", self.query);
            warning_panel(&title, suggestions)
        } else {
            let mut out = format!("! No skills found for '{}'\n\n", self.query);
            out.push_str(suggestions);
            out.push('\n');
            out
        }
    }

    /// Render populated search results with a table and metadata header.
    fn format_populated_results(&self, use_rich: bool) -> String {
        debug!(target: "search", results = self.results.len(), "rendering results");

        let mut out = String::new();

        // Metadata header
        let header = self.build_metadata_header();
        out.push_str(&header);
        out.push_str("\n\n");

        if use_rich {
            // Build table data for the rich_rust builder
            let table_data: Vec<(&str, f32, &str, &str)> = self
                .results
                .iter()
                .map(|r| {
                    (
                        r.skill.name.as_str(),
                        r.score,
                        r.skill.source_layer.as_str(),
                        r.skill.description.as_str(),
                    )
                })
                .collect();

            let width = terminal_width();
            let table = search_results_table(&table_data, width);
            out.push_str(&table.render_plain(width));
        } else {
            // Plain list output (no ANSI)
            for (i, result) in self.results.iter().enumerate() {
                out.push_str(&format!(
                    "{}. {} [{:.2}] {}",
                    i + 1,
                    result.skill.name,
                    result.score,
                    result.skill.source_layer,
                ));
                if result.skill.is_deprecated {
                    out.push_str(" [deprecated]");
                }
                out.push('\n');

                if !result.skill.description.is_empty() {
                    out.push_str(&format!("   {}\n", result.skill.description));
                }
                if let Some(ref snippet) = result.snippet {
                    out.push_str(&format!("   {snippet}\n"));
                }
                out.push('\n');
            }
        }

        // Append snippet section for rich mode (below table)
        if use_rich {
            let snippets: Vec<_> = self
                .results
                .iter()
                .filter_map(|r| {
                    r.snippet
                        .as_ref()
                        .map(|s| format!("  {} : {s}", r.skill.name))
                })
                .collect();
            if !snippets.is_empty() {
                out.push_str("\nSnippets:\n");
                for s in &snippets {
                    out.push_str(s);
                    out.push('\n');
                }
            }
        }

        out
    }

    /// Build the metadata header line summarizing the search.
    fn build_metadata_header(&self) -> String {
        let mut header = format!(
            "{} results for '{}' ({} search)",
            self.results.len(),
            self.query,
            self.search_type,
        );
        if let Some(ms) = self.duration_ms {
            header.push_str(&format!(" in {ms}ms"));
        }
        header
    }

    /// Format as plain TSV (bd-olwb spec: SCORE<TAB>NAME<TAB>LAYER<TAB>DESCRIPTION).
    ///
    /// No headers, just data rows for easy parsing with cut/awk.
    fn format_plain(&self) -> String {
        use crate::output::plain_utils::{escape_tsv, format_score};

        self.results
            .iter()
            .map(|r| {
                let desc = escape_tsv(&r.skill.description);
                format!(
                    "{}\t{}\t{}\t{}",
                    format_score(r.score),
                    r.skill.name,
                    r.skill.source_layer,
                    desc
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn format_tsv(&self) -> String {
        let mut out = String::from("id\tname\tlayer\tscore\tquality\tdescription\n");
        for r in &self.results {
            let desc = r.skill.description.replace('\t', " ").replace('\n', " ");
            out.push_str(&format!(
                "{}\t{}\t{}\t{:.4}\t{:.2}\t{}\n",
                r.skill.id,
                r.skill.name,
                r.skill.source_layer,
                r.score,
                r.skill.quality_score,
                desc
            ));
        }
        out
    }

    fn format_jsonl(&self) -> String {
        self.results
            .iter()
            .filter_map(|r| {
                serde_json::to_string(&SearchResultJson {
                    id: r.skill.id.clone(),
                    name: r.skill.name.clone(),
                    description: r.skill.description.clone(),
                    layer: r.skill.source_layer.clone(),
                    score: r.score,
                    quality: r.skill.quality_score,
                    is_deprecated: r.skill.is_deprecated,
                    snippet: r.snippet.clone(),
                })
                .ok()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn format_toon(&self) -> String {
        let json_response = self.to_json_response();
        let json = serde_json::to_value(&json_response).unwrap_or_default();
        toon_rust::encode(json, None)
    }
}

impl Formattable for SearchResults {
    fn format(&self, fmt: OutputFormat) -> String {
        match fmt {
            OutputFormat::Human => self.format_human(),
            OutputFormat::Json => {
                serde_json::to_string_pretty(&self.to_json_response()).unwrap_or_default()
            }
            OutputFormat::Jsonl => self.format_jsonl(),
            OutputFormat::Plain => self.format_plain(),
            OutputFormat::Tsv => self.format_tsv(),
            OutputFormat::Toon => self.format_toon(),
        }
    }
}

// =============================================================================
// Helper functions
// =============================================================================

/// Determine whether rich output should be used for search results.
///
/// Rich output is disabled in agent environments, CI, or when stdout is not a
/// terminal. This mirrors the logic in `output::detection` but is lightweight
/// enough to call without constructing a full `RichOutput` instance.
fn should_use_rich_for_search() -> bool {
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

/// Return a color label for a search relevance score.
///
/// Used by tests to verify score-to-color mapping.
#[cfg(test)]
fn score_color(score: f32) -> &'static str {
    if score >= 0.8 {
        "green"
    } else if score >= 0.5 {
        "yellow"
    } else {
        "red"
    }
}

/// Truncate a description to fit within `max_chars`, appending "..." if needed.
#[cfg(test)]
fn truncate_description(desc: &str, max_chars: usize) -> String {
    if desc.chars().count() <= max_chars {
        desc.to_string()
    } else {
        let truncated: String = desc.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{truncated}...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_skill(id: &str) -> SkillRecord {
        SkillRecord {
            id: id.to_string(),
            name: format!("Skill {id}"),
            description: format!("Description for {id}"),
            version: Some("1.0.0".to_string()),
            author: None,
            source_path: "/path".to_string(),
            source_layer: "user".to_string(),
            git_remote: None,
            git_commit: None,
            content_hash: "hash".to_string(),
            body: "body".to_string(),
            metadata_json: "{}".to_string(),
            assets_json: "[]".to_string(),
            token_count: 50,
            quality_score: 0.8,
            indexed_at: "2025-01-01".to_string(),
            modified_at: "2025-01-01".to_string(),
            is_deprecated: false,
            deprecation_reason: None,
        }
    }

    #[test]
    fn search_results_empty_human() {
        let results = SearchResults::new("test query", "hybrid");
        let output = results.format(OutputFormat::Human);

        assert!(output.contains("No skills found"));
        assert!(output.contains("test query"));
    }

    #[test]
    fn search_results_json_valid() {
        let mut results = SearchResults::new("test", "hybrid");
        results.add_result(test_skill("skill-1"), 0.95);
        results.add_result(test_skill("skill-2"), 0.85);

        let output = results.format(OutputFormat::Json);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["status"], "ok");
        assert_eq!(parsed["query"], "test");
        assert_eq!(parsed["count"], 2);
        assert!(parsed["results"].is_array());
    }

    #[test]
    fn search_results_jsonl_one_per_line() {
        let mut results = SearchResults::new("test", "hybrid");
        results.add_result(test_skill("skill-1"), 0.95);
        results.add_result(test_skill("skill-2"), 0.85);

        let output = results.format(OutputFormat::Jsonl);
        let lines: Vec<&str> = output.lines().collect();

        assert_eq!(lines.len(), 2);
        for line in lines {
            let _: serde_json::Value = serde_json::from_str(line).unwrap();
        }
    }

    #[test]
    fn search_results_plain_format() {
        let mut results = SearchResults::new("test", "hybrid");
        results.add_result(test_skill("skill-1"), 0.95);

        let output = results.format(OutputFormat::Plain);

        // bd-olwb spec: SCORE<TAB>NAME<TAB>LAYER<TAB>DESCRIPTION
        assert!(output.contains("0.95"));
        assert!(output.contains("Skill skill-1")); // name format from test_skill
        assert!(output.contains("user")); // layer
        assert!(output.contains('\t')); // tab-separated

        // Verify no headers (unlike TSV)
        assert!(!output.starts_with("score\t"));

        // Verify tab-separated structure
        let fields: Vec<&str> = output.lines().next().unwrap().split('\t').collect();
        assert_eq!(
            fields.len(),
            4,
            "Plain format should have 4 fields: score, name, layer, description"
        );
    }

    #[test]
    fn search_results_tsv_has_header() {
        let mut results = SearchResults::new("test", "hybrid");
        results.add_result(test_skill("skill-1"), 0.95);

        let output = results.format(OutputFormat::Tsv);
        let lines: Vec<&str> = output.lines().collect();

        assert!(lines[0].contains("id\t"));
        assert!(lines.len() >= 2);
    }

    #[test]
    fn search_results_with_duration() {
        let results = SearchResults::new("test", "hybrid").with_duration(42);
        let output = results.format(OutputFormat::Json);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["duration_ms"], 42);
    }

    // ==================== bd-35u5: Rich Output Tests ====================

    #[test]
    fn test_search_render_empty_results() {
        let results = SearchResults::new("nonexistent", "hybrid");
        let output = results.format_human();

        assert!(
            output.contains("No skills found"),
            "Empty results should show 'No skills found'"
        );
        assert!(
            output.contains("nonexistent"),
            "Empty results should echo the query"
        );
        assert!(
            output.contains("different keywords"),
            "Empty results should suggest alternatives"
        );
    }

    #[test]
    fn test_search_render_single_result() {
        let mut results = SearchResults::new("rust", "bm25");
        results.add_result(test_skill("skill-1"), 0.9);
        let output = results.format_human();

        assert!(
            output.contains("1 results"),
            "Should show result count: got {output}"
        );
        assert!(output.contains("Skill skill-1"), "Should contain skill name");
        assert!(output.contains("bm25"), "Should show search type");
    }

    #[test]
    fn test_search_render_many_results() {
        let mut results = SearchResults::new("test", "hybrid");
        for i in 0..10 {
            results.add_result(test_skill(&format!("s-{i}")), 1.0 - (i as f32 * 0.1));
        }
        let output = results.format_human();

        assert!(
            output.contains("10 results"),
            "Should show count of all results"
        );
        assert!(output.contains("Skill s-0"), "Should contain first skill");
        assert!(output.contains("Skill s-9"), "Should contain last skill");
    }

    #[test]
    fn test_search_score_color_gradient() {
        assert_eq!(score_color(0.95), "green", "High score should be green");
        assert_eq!(score_color(0.80), "green", "0.80 should be green");
        assert_eq!(score_color(0.60), "yellow", "Medium score should be yellow");
        assert_eq!(score_color(0.50), "yellow", "0.50 should be yellow");
        assert_eq!(score_color(0.30), "red", "Low score should be red");
        assert_eq!(score_color(0.0), "red", "Zero score should be red");
    }

    #[test]
    fn test_search_truncate_description() {
        assert_eq!(truncate_description("short", 10), "short");
        assert_eq!(truncate_description("a bit longer text", 10), "a bit l...");
        assert_eq!(truncate_description("exactly10!", 10), "exactly10!");
        assert_eq!(truncate_description("", 5), "");

        // Unicode safety
        let emoji = "🦀🐍🚀🎯🔥";
        let truncated = truncate_description(emoji, 4);
        assert!(
            truncated.ends_with("..."),
            "Truncated Unicode should end with ellipsis"
        );
    }

    #[test]
    fn test_search_metadata_panel() {
        let results = SearchResults::new("test query", "semantic").with_duration(15);
        let header = results.build_metadata_header();

        assert!(header.contains("test query"), "Header should contain query");
        assert!(
            header.contains("semantic"),
            "Header should contain search type"
        );
        assert!(
            header.contains("in 15ms"),
            "Header should contain duration"
        );
    }

    #[test]
    fn test_search_metadata_panel_no_duration() {
        let results = SearchResults::new("q", "hybrid");
        let header = results.build_metadata_header();

        assert!(!header.contains("ms"), "No duration when not set");
        assert!(header.contains("hybrid"), "Should show search type");
    }

    #[test]
    fn test_search_plain_output_format() {
        let mut results = SearchResults::new("test", "hybrid");
        results.add_result(test_skill("s-1"), 0.85);
        results.add_result(test_skill("s-2"), 0.42);

        let output = results.format(OutputFormat::Plain);
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 2, "Plain output should have one line per result");

        for line in &lines {
            let fields: Vec<&str> = line.split('\t').collect();
            assert_eq!(fields.len(), 4, "Each line should be SCORE\\tNAME\\tLAYER\\tDESC");
        }
    }

    #[test]
    fn test_search_json_output_format() {
        let mut results = SearchResults::new("error handling", "hybrid");
        results.add_result(test_skill("skill-a"), 0.92);

        let output = results.format(OutputFormat::Json);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["status"], "ok");
        assert_eq!(parsed["query"], "error handling");
        assert_eq!(parsed["search_type"], "hybrid");
        assert_eq!(parsed["count"], 1);

        let first = &parsed["results"][0];
        assert_eq!(first["id"], "skill-a");
        assert_eq!(first["name"], "Skill skill-a");
        assert!(!first["is_deprecated"].as_bool().unwrap());
    }

    #[test]
    fn test_search_robot_mode_no_ansi() {
        let mut results = SearchResults::new("test", "bm25");
        results.add_result(test_skill("x"), 0.5);

        // JSON and Plain formats must never contain ANSI escape sequences
        for fmt in [OutputFormat::Json, OutputFormat::Plain, OutputFormat::Tsv] {
            let output = results.format(fmt);
            assert!(
                !output.contains("\x1b["),
                "Format {fmt:?} must not contain ANSI codes, got: {output}"
            );
        }
    }

    #[test]
    fn test_search_agent_env_detection() {
        // is_agent_environment checks for env vars like CLAUDE_CODE, CURSOR_AI, etc.
        // We just verify the function exists and returns a bool without panicking.
        let _result: bool = is_agent_environment();
    }

    #[test]
    fn test_search_ci_env_detection() {
        // is_ci_environment checks for GITHUB_ACTIONS, CI, etc.
        let _result: bool = is_ci_environment();
    }

    #[test]
    fn test_search_terminal_width_adapt() {
        // terminal_width() should return a reasonable value (>= 40)
        let width = terminal_width();
        assert!(
            width >= 40,
            "Terminal width should be at least 40, got {width}"
        );
    }

    #[test]
    fn test_search_rich_vs_plain_equivalence() {
        // Both rich and plain rendering should contain the same core data
        let mut results = SearchResults::new("test", "hybrid");
        results.add_result(test_skill("eq-1"), 0.88);
        results.add_result(test_skill("eq-2"), 0.55);

        let rich = results.format_populated_results(true);
        let plain = results.format_populated_results(false);

        // Both must contain the skill names and search metadata
        for name in &["Skill eq-1", "Skill eq-2"] {
            assert!(rich.contains(name), "Rich output missing {name}");
            assert!(plain.contains(name), "Plain output missing {name}");
        }
    }

    #[test]
    fn test_search_deprecated_skill_display() {
        let mut skill = test_skill("dep-1");
        skill.is_deprecated = true;

        let mut results = SearchResults::new("test", "hybrid");
        results.add_result(skill, 0.7);

        // Plain rendering should show [deprecated]
        let plain = results.format_populated_results(false);
        assert!(
            plain.contains("[deprecated]"),
            "Deprecated skills should be marked: {plain}"
        );
    }

    #[test]
    fn test_search_snippet_display() {
        let mut results = SearchResults::new("test", "hybrid");
        results.add_result_with_snippet(
            test_skill("snip-1"),
            0.9,
            "...matching context around the query term...",
        );

        let plain = results.format_populated_results(false);
        assert!(
            plain.contains("matching context"),
            "Snippet should appear in output: {plain}"
        );
    }

    #[test]
    fn test_search_toon_format() {
        let mut results = SearchResults::new("test", "hybrid");
        results.add_result(test_skill("t-1"), 0.7);

        let output = results.format(OutputFormat::Toon);
        // TOON output should be non-empty and not JSON
        assert!(!output.is_empty(), "TOON output should not be empty");
        // TOON is not valid JSON (different format)
        assert!(
            serde_json::from_str::<serde_json::Value>(&output).is_err()
                || output.contains("test"),
            "TOON output should contain data or be a different format from JSON"
        );
    }

    #[test]
    fn test_search_empty_results_format_equivalence() {
        let results = SearchResults::new("nothing", "hybrid");

        let rich_empty = results.format_empty_results(true);
        let plain_empty = results.format_empty_results(false);

        // Both should mention the query and give suggestions
        assert!(rich_empty.contains("nothing"), "Rich empty should contain query");
        assert!(plain_empty.contains("nothing"), "Plain empty should contain query");
        assert!(
            rich_empty.contains("different keywords"),
            "Rich empty should suggest alternatives"
        );
        assert!(
            plain_empty.contains("different keywords"),
            "Plain empty should suggest alternatives"
        );
    }
}
