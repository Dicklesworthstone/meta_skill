//! Search results formatter

use console::style;
use serde::Serialize;

use crate::cli::output::{Formattable, OutputFormat};
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

    fn format_human(&self) -> String {
        if self.results.is_empty() {
            let mut out = format!(
                "{} No skills found for '{}'\n\n",
                style("!").yellow(),
                style(&self.query).cyan()
            );
            out.push_str("Try:\n");
            out.push_str("  - Using different keywords\n");
            out.push_str("  - Removing filters (--tags, --layer, --min-quality)\n");
            out.push_str("  - Including deprecated skills: --include-deprecated\n");
            return out;
        }

        let mut out = format!(
            "{} results for '{}' ({} search)",
            style(self.results.len().to_string()).bold(),
            style(&self.query).cyan(),
            self.search_type
        );

        if let Some(ms) = self.duration_ms {
            out.push_str(&format!(" in {ms}ms"));
        }
        out.push_str(":\n\n");

        for (i, result) in self.results.iter().enumerate() {
            // Rank and name
            out.push_str(&format!(
                "{}. {} ",
                style(i + 1).dim(),
                style(&result.skill.name).cyan().bold()
            ));

            // Score and quality
            out.push_str(&format!(
                "{} {} {}\n",
                style(format!("[{:.2}]", result.score)).dim(),
                style(&result.skill.source_layer).dim(),
                if result.skill.is_deprecated {
                    style("[deprecated]").red().to_string()
                } else {
                    String::new()
                }
            ));

            // Description
            if !result.skill.description.is_empty() {
                out.push_str(&format!("   {}\n", result.skill.description));
            }

            // Snippet if available
            if let Some(ref snippet) = result.snippet {
                out.push_str(&format!("   {}\n", style(snippet).dim()));
            }

            out.push('\n');
        }

        out
    }

    fn format_plain(&self) -> String {
        self.results
            .iter()
            .map(|r| format!("{}: {:.2}", r.skill.id, r.score))
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
        }
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

        assert!(output.contains("skill-1"));
        assert!(output.contains("0.95"));
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
}
