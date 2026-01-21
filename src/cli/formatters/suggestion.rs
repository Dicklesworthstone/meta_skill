//! Suggestion output formatter

use console::style;
use serde::Serialize;

use crate::cli::output::{Formattable, OutputFormat};

/// Percentage breakdown of signal contributions to a suggestion score
#[derive(Debug, Clone, Default, Serialize)]
pub struct ScorePercentageBreakdown {
    /// Project/context match percentage
    pub context_pct: f32,
    /// Historical usage percentage
    pub history_pct: f32,
    /// Exploration bonus percentage
    pub exploration_pct: f32,
    /// Favorites boost percentage
    pub favorites_pct: f32,
}

impl ScorePercentageBreakdown {
    /// Calculate percentages from raw score components
    pub fn from_components(
        contextual_score: f32,
        thompson_score: f32,
        exploration_bonus: f32,
        personal_boost: f32,
    ) -> Self {
        // The final score is approximately: contextual*0.7 + thompson*0.3 + exploration + personal
        // Normalize to percentages
        let total =
            contextual_score * 0.7 + thompson_score * 0.3 + exploration_bonus + personal_boost;
        if total <= 0.0 {
            return Self::default();
        }

        // Calculate contribution percentages
        let context_raw = contextual_score * 0.7;
        let history_raw = thompson_score * 0.3; // Thompson includes history signal
        let exploration_raw = exploration_bonus;
        let favorites_raw = personal_boost;

        let sum = context_raw + history_raw + exploration_raw + favorites_raw;
        if sum <= 0.0 {
            return Self::default();
        }

        Self {
            context_pct: (context_raw / sum * 100.0).round(),
            history_pct: (history_raw / sum * 100.0).round(),
            exploration_pct: (exploration_raw / sum * 100.0).round(),
            favorites_pct: (favorites_raw / sum * 100.0).round(),
        }
    }

    /// Check if any meaningful breakdown is available
    pub fn has_breakdown(&self) -> bool {
        self.context_pct > 0.0
            || self.history_pct > 0.0
            || self.exploration_pct > 0.0
            || self.favorites_pct > 0.0
    }
}

/// A skill suggestion with confidence and explanation
#[derive(Debug, Clone)]
pub struct SuggestionItem {
    /// Skill ID
    pub skill_id: String,
    /// Skill name
    pub name: String,
    /// Short description
    pub description: String,
    /// Confidence score (0.0-1.0)
    pub confidence: f32,
    /// Reason/explanation for the suggestion
    pub reason: Option<String>,
    /// Whether this is a discovery/exploration suggestion
    pub is_discovery: bool,
    /// Tags for the skill
    pub tags: Vec<String>,
    /// Percentage breakdown of score components (for --explain)
    pub breakdown: Option<ScorePercentageBreakdown>,
}

/// Context information for suggestions
#[derive(Debug, Clone, Default, Serialize)]
pub struct SuggestionContext {
    /// Current working directory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Current git branch
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    /// Recent files touched
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent_files: Vec<String>,
    /// Context fingerprint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<u64>,
}

/// Formatted suggestion output
#[derive(Debug, Clone)]
pub struct SuggestionOutput {
    /// Main suggestions (exploitation)
    pub suggestions: Vec<SuggestionItem>,
    /// Discovery suggestions (exploration)
    pub discovery_suggestions: Vec<SuggestionItem>,
    /// Context information
    pub context: SuggestionContext,
}

/// Serializable suggestion for JSON output
#[derive(Debug, Clone, Serialize)]
struct SuggestionJson {
    skill_id: String,
    name: String,
    description: String,
    confidence: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    is_discovery: bool,
    tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    breakdown: Option<ScorePercentageBreakdown>,
}

/// Serializable suggestion response for JSON output
#[derive(Debug, Clone, Serialize)]
struct SuggestionResponseJson {
    status: String,
    context: SuggestionContext,
    suggestions: Vec<SuggestionJson>,
    discovery_suggestions: Vec<SuggestionJson>,
}

impl SuggestionOutput {
    /// Create a new empty suggestion output
    pub fn new() -> Self {
        Self {
            suggestions: Vec::new(),
            discovery_suggestions: Vec::new(),
            context: SuggestionContext::default(),
        }
    }

    /// Add a suggestion
    pub fn add_suggestion(&mut self, item: SuggestionItem) {
        if item.is_discovery {
            self.discovery_suggestions.push(item);
        } else {
            self.suggestions.push(item);
        }
    }

    /// Set context information
    #[must_use]
    pub fn with_context(mut self, context: SuggestionContext) -> Self {
        self.context = context;
        self
    }

    /// Set context fingerprint
    #[must_use]
    pub fn with_fingerprint(mut self, fingerprint: u64) -> Self {
        self.context.fingerprint = Some(fingerprint);
        self
    }

    fn to_suggestion_json(item: &SuggestionItem) -> SuggestionJson {
        SuggestionJson {
            skill_id: item.skill_id.clone(),
            name: item.name.clone(),
            description: item.description.clone(),
            confidence: item.confidence,
            reason: item.reason.clone(),
            is_discovery: item.is_discovery,
            tags: item.tags.clone(),
            breakdown: item.breakdown.clone(),
        }
    }

    fn to_json_response(&self) -> SuggestionResponseJson {
        SuggestionResponseJson {
            status: "ok".to_string(),
            context: self.context.clone(),
            suggestions: self
                .suggestions
                .iter()
                .map(Self::to_suggestion_json)
                .collect(),
            discovery_suggestions: self
                .discovery_suggestions
                .iter()
                .map(Self::to_suggestion_json)
                .collect(),
        }
    }

    fn format_human(&self) -> String {
        let mut out = String::new();

        if self.suggestions.is_empty() && self.discovery_suggestions.is_empty() {
            out.push_str(&format!(
                "{} No suggestions available for current context.\n\n",
                style("!").yellow()
            ));
            out.push_str("Try:\n");
            out.push_str("  - Working in a project directory with recognizable files\n");
            out.push_str("  - Using --discover to explore novel skills\n");
            return out;
        }

        // Main suggestions
        if !self.suggestions.is_empty() {
            out.push_str(&format!("{}\n\n", style("Suggested skills:").bold()));

            for (i, suggestion) in self.suggestions.iter().enumerate() {
                out.push_str(&format!(
                    "{}. {} ",
                    style(i + 1).dim(),
                    style(&suggestion.name).green().bold()
                ));

                // Confidence
                let conf_pct = suggestion.confidence * 100.0;
                let conf_str = format!("({:.0}%)", conf_pct);
                let conf_styled = if conf_pct >= 80.0 {
                    style(conf_str).green()
                } else if conf_pct >= 50.0 {
                    style(conf_str).yellow()
                } else {
                    style(conf_str).dim()
                };
                out.push_str(&format!("{}\n", conf_styled));

                // Description
                if !suggestion.description.is_empty() {
                    out.push_str(&format!("   {}\n", suggestion.description));
                }

                // Reason
                if let Some(ref reason) = suggestion.reason {
                    out.push_str(&format!("   {}\n", style(reason).dim()));
                }

                // Percentage breakdown (when --explain is used)
                if let Some(ref breakdown) = suggestion.breakdown {
                    if breakdown.has_breakdown() {
                        // Format: "  - [80%] Context match"
                        if breakdown.context_pct > 0.0 {
                            out.push_str(&format!(
                                "   {} Context/project match\n",
                                style(format!("[{:.0}%]", breakdown.context_pct)).cyan()
                            ));
                        }
                        if breakdown.history_pct > 0.0 {
                            out.push_str(&format!(
                                "   {} Historical usage patterns\n",
                                style(format!("[{:.0}%]", breakdown.history_pct)).cyan()
                            ));
                        }
                        if breakdown.exploration_pct > 0.0 {
                            out.push_str(&format!(
                                "   {} Exploration bonus\n",
                                style(format!("[{:.0}%]", breakdown.exploration_pct)).cyan()
                            ));
                        }
                        if breakdown.favorites_pct > 0.0 {
                            out.push_str(&format!(
                                "   {} Favorites boost\n",
                                style(format!("[{:.0}%]", breakdown.favorites_pct)).magenta()
                            ));
                        }
                    }
                }

                // Tags
                if !suggestion.tags.is_empty() {
                    let tags_str = suggestion
                        .tags
                        .iter()
                        .map(|t| format!("#{t}"))
                        .collect::<Vec<_>>()
                        .join(" ");
                    out.push_str(&format!("   {}\n", style(tags_str).dim()));
                }

                out.push('\n');
            }
        }

        // Discovery suggestions
        if !self.discovery_suggestions.is_empty() {
            out.push_str(&format!(
                "{}\n\n",
                style("Discover something new:").cyan().bold()
            ));

            for (i, suggestion) in self.discovery_suggestions.iter().enumerate() {
                out.push_str(&format!(
                    "{}. {} {}\n",
                    style(i + 1).dim(),
                    style(&suggestion.name).cyan(),
                    style("[explore]").dim()
                ));

                if !suggestion.description.is_empty() {
                    out.push_str(&format!("   {}\n", suggestion.description));
                }

                out.push('\n');
            }
        }

        out
    }

    fn format_plain(&self) -> String {
        let mut lines = Vec::new();

        for s in &self.suggestions {
            lines.push(s.skill_id.clone());
        }
        for s in &self.discovery_suggestions {
            lines.push(s.skill_id.clone());
        }

        lines.join("\n")
    }

    fn format_tsv(&self) -> String {
        let mut out = String::from("skill_id\tname\tconfidence\tis_discovery\treason\n");

        for s in &self.suggestions {
            let reason = s.reason.as_deref().unwrap_or("");
            out.push_str(&format!(
                "{}\t{}\t{:.4}\tfalse\t{}\n",
                s.skill_id, s.name, s.confidence, reason
            ));
        }
        for s in &self.discovery_suggestions {
            let reason = s.reason.as_deref().unwrap_or("");
            out.push_str(&format!(
                "{}\t{}\t{:.4}\ttrue\t{}\n",
                s.skill_id, s.name, s.confidence, reason
            ));
        }

        out
    }

    fn format_jsonl(&self) -> String {
        let all: Vec<_> = self
            .suggestions
            .iter()
            .chain(self.discovery_suggestions.iter())
            .filter_map(|s| serde_json::to_string(&Self::to_suggestion_json(s)).ok())
            .collect();

        all.join("\n")
    }
}

impl Default for SuggestionOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl Formattable for SuggestionOutput {
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

    fn test_suggestion() -> SuggestionItem {
        SuggestionItem {
            skill_id: "git-commit".to_string(),
            name: "Git Commit".to_string(),
            description: "Best practices for git commits".to_string(),
            confidence: 0.85,
            reason: Some("Recent git activity detected".to_string()),
            is_discovery: false,
            tags: vec!["git".to_string(), "vcs".to_string()],
            breakdown: None,
        }
    }

    #[test]
    fn suggestion_output_empty_human() {
        let output = SuggestionOutput::new();
        let formatted = output.format(OutputFormat::Human);

        assert!(formatted.contains("No suggestions"));
    }

    #[test]
    fn suggestion_output_json_valid() {
        let mut output = SuggestionOutput::new();
        output.add_suggestion(test_suggestion());

        let formatted = output.format(OutputFormat::Json);
        let parsed: serde_json::Value = serde_json::from_str(&formatted).unwrap();

        assert_eq!(parsed["status"], "ok");
        assert!(parsed["suggestions"].is_array());
        assert_eq!(parsed["suggestions"][0]["skill_id"], "git-commit");
    }

    #[test]
    fn suggestion_output_plain_ids_only() {
        let mut output = SuggestionOutput::new();
        output.add_suggestion(test_suggestion());

        let formatted = output.format(OutputFormat::Plain);

        assert_eq!(formatted.trim(), "git-commit");
    }

    #[test]
    fn suggestion_output_tsv_has_header() {
        let mut output = SuggestionOutput::new();
        output.add_suggestion(test_suggestion());

        let formatted = output.format(OutputFormat::Tsv);
        let lines: Vec<&str> = formatted.lines().collect();

        assert!(lines[0].contains("skill_id\t"));
        assert!(lines[1].contains("git-commit"));
    }

    #[test]
    fn suggestion_output_jsonl_one_per_line() {
        let mut output = SuggestionOutput::new();
        output.add_suggestion(test_suggestion());

        let mut discovery = test_suggestion();
        discovery.skill_id = "rust-basics".to_string();
        discovery.is_discovery = true;
        output.add_suggestion(discovery);

        let formatted = output.format(OutputFormat::Jsonl);
        let lines: Vec<&str> = formatted.lines().collect();

        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn suggestion_output_with_context() {
        let mut output = SuggestionOutput::new();
        output.add_suggestion(test_suggestion());

        let context = SuggestionContext {
            cwd: Some("/home/user/project".to_string()),
            git_branch: Some("main".to_string()),
            recent_files: vec!["src/main.rs".to_string()],
            fingerprint: Some(12345),
        };

        let output = output.with_context(context);
        let formatted = output.format(OutputFormat::Json);
        let parsed: serde_json::Value = serde_json::from_str(&formatted).unwrap();

        assert_eq!(parsed["context"]["cwd"], "/home/user/project");
        assert_eq!(parsed["context"]["git_branch"], "main");
        assert_eq!(parsed["context"]["fingerprint"], 12345);
    }
}
