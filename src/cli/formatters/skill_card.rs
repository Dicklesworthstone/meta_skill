//! Skill card formatter for displaying skill information

use console::style;
use serde::Serialize;

use crate::cli::output::{Formattable, HumanLayout, OutputFormat};
use crate::storage::sqlite::SkillRecord;

/// A formatted view of a skill for display
#[derive(Debug, Clone)]
pub struct SkillCard<'a> {
    /// The skill record to display
    pub skill: &'a SkillRecord,
    /// Whether to show the full body
    pub show_body: bool,
    /// Whether to show extended metadata
    pub show_metadata: bool,
}

/// Serializable skill summary for JSON output
#[derive(Debug, Clone, Serialize)]
struct SkillSummary {
    id: String,
    name: String,
    description: String,
    layer: String,
    version: Option<String>,
    author: Option<String>,
    quality_score: f64,
    is_deprecated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    deprecation_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<String>,
}

impl<'a> SkillCard<'a> {
    /// Create a new skill card
    pub fn new(skill: &'a SkillRecord) -> Self {
        Self {
            skill,
            show_body: false,
            show_metadata: false,
        }
    }

    /// Show the full body content
    #[must_use]
    pub const fn with_body(mut self) -> Self {
        self.show_body = true;
        self
    }

    /// Show extended metadata
    #[must_use]
    pub const fn with_metadata(mut self) -> Self {
        self.show_metadata = true;
        self
    }

    fn to_summary(&self) -> SkillSummary {
        SkillSummary {
            id: self.skill.id.clone(),
            name: self.skill.name.clone(),
            description: self.skill.description.clone(),
            layer: self.skill.source_layer.clone(),
            version: self.skill.version.clone(),
            author: self.skill.author.clone(),
            quality_score: self.skill.quality_score,
            is_deprecated: self.skill.is_deprecated,
            deprecation_reason: self.skill.deprecation_reason.clone(),
            body: if self.show_body {
                Some(self.skill.body.clone())
            } else {
                None
            },
        }
    }

    fn format_human(&self) -> String {
        let mut layout = HumanLayout::new();

        // Title with deprecation warning
        if self.skill.is_deprecated {
            layout.title(&format!(
                "{} {}",
                self.skill.name,
                style("[DEPRECATED]").red().bold()
            ));
        } else {
            layout.title(&self.skill.name);
        }

        // Core info
        layout.kv("ID", &self.skill.id);
        layout.kv("Layer", &self.skill.source_layer);

        if !self.skill.description.is_empty() {
            layout.kv("Description", &self.skill.description);
        }

        if let Some(ref version) = self.skill.version {
            layout.kv("Version", version);
        }

        if let Some(ref author) = self.skill.author {
            layout.kv("Author", author);
        }

        // Quality score with color
        let quality_str = format!("{:.0}%", self.skill.quality_score * 100.0);
        let quality_display = if self.skill.quality_score >= 0.8 {
            style(&quality_str).green().to_string()
        } else if self.skill.quality_score >= 0.5 {
            style(&quality_str).yellow().to_string()
        } else {
            style(&quality_str).red().to_string()
        };
        layout.kv("Quality", &quality_display);

        // Deprecation reason
        if let Some(ref reason) = self.skill.deprecation_reason {
            layout.blank();
            layout.kv("Deprecation", reason);
        }

        // Extended metadata
        if self.show_metadata {
            layout.blank();
            layout.section("Metadata");
            layout.kv("Path", &self.skill.source_path);
            layout.kv("Hash", &self.skill.content_hash);
            layout.kv("Tokens", &self.skill.token_count.to_string());
            layout.kv("Indexed", &self.skill.indexed_at);
            layout.kv("Modified", &self.skill.modified_at);

            if let Some(ref remote) = self.skill.git_remote {
                layout.kv("Git Remote", remote);
            }
            if let Some(ref commit) = self.skill.git_commit {
                layout.kv("Git Commit", commit);
            }
        }

        // Body content
        if self.show_body && !self.skill.body.is_empty() {
            layout.blank();
            layout.section("Content");
            layout.push_line(&self.skill.body);
        }

        layout.build()
    }

    fn format_plain(&self) -> String {
        let mut parts = vec![
            self.skill.id.clone(),
            self.skill.name.clone(),
            format!("({})", self.skill.source_layer),
        ];

        if self.skill.is_deprecated {
            parts.push("[DEPRECATED]".to_string());
        }

        parts.join(" ")
    }

    fn format_tsv(&self) -> String {
        // Escape tabs and newlines in description
        let desc = self.skill.description.replace('\t', " ").replace('\n', " ");

        format!(
            "{}\t{}\t{}\t{}\t{:.2}\t{}",
            self.skill.id,
            self.skill.name,
            self.skill.source_layer,
            desc,
            self.skill.quality_score,
            self.skill.is_deprecated
        )
    }
}

impl Formattable for SkillCard<'_> {
    fn format(&self, fmt: OutputFormat) -> String {
        match fmt {
            OutputFormat::Human => self.format_human(),
            OutputFormat::Json => {
                serde_json::to_string_pretty(&self.to_summary()).unwrap_or_default()
            }
            OutputFormat::Jsonl => serde_json::to_string(&self.to_summary()).unwrap_or_default(),
            OutputFormat::Plain => self.format_plain(),
            OutputFormat::Tsv => self.format_tsv(),
            OutputFormat::Toon => {
                let summary = self.to_summary();
                let json = serde_json::to_value(&summary).unwrap_or_default();
                toon_rust::encode(json, None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_skill() -> SkillRecord {
        SkillRecord {
            id: "test-skill".to_string(),
            name: "Test Skill".to_string(),
            description: "A test skill for testing".to_string(),
            version: Some("1.0.0".to_string()),
            author: Some("Test Author".to_string()),
            source_path: "/path/to/skill".to_string(),
            source_layer: "user".to_string(),
            git_remote: None,
            git_commit: None,
            content_hash: "abc123".to_string(),
            body: "Skill body content".to_string(),
            metadata_json: "{}".to_string(),
            assets_json: "[]".to_string(),
            token_count: 100,
            quality_score: 0.85,
            indexed_at: "2025-01-01T00:00:00Z".to_string(),
            modified_at: "2025-01-01T00:00:00Z".to_string(),
            is_deprecated: false,
            deprecation_reason: None,
        }
    }

    #[test]
    fn skill_card_plain_format() {
        let skill = test_skill();
        let card = SkillCard::new(&skill);
        let output = card.format(OutputFormat::Plain);

        assert!(output.contains("test-skill"));
        assert!(output.contains("Test Skill"));
        assert!(output.contains("user"));
    }

    #[test]
    fn skill_card_json_is_valid() {
        let skill = test_skill();
        let card = SkillCard::new(&skill);
        let output = card.format(OutputFormat::Json);

        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["id"], "test-skill");
        assert_eq!(parsed["name"], "Test Skill");
    }

    #[test]
    fn skill_card_jsonl_is_compact() {
        let skill = test_skill();
        let card = SkillCard::new(&skill);
        let output = card.format(OutputFormat::Jsonl);

        // JSONL should be a single line
        assert!(!output.contains('\n'));
        let _: serde_json::Value = serde_json::from_str(&output).unwrap();
    }

    #[test]
    fn skill_card_tsv_has_tabs() {
        let skill = test_skill();
        let card = SkillCard::new(&skill);
        let output = card.format(OutputFormat::Tsv);

        assert!(output.contains('\t'));
        let fields: Vec<&str> = output.split('\t').collect();
        assert_eq!(fields.len(), 6);
    }

    #[test]
    fn skill_card_human_contains_name() {
        let skill = test_skill();
        let card = SkillCard::new(&skill);
        let output = card.format(OutputFormat::Human);

        assert!(output.contains("Test Skill"));
    }

    #[test]
    fn skill_card_with_body_includes_content() {
        let skill = test_skill();
        let card = SkillCard::new(&skill).with_body();
        let output = card.format(OutputFormat::Json);

        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(parsed["body"].is_string());
    }

    #[test]
    fn skill_card_deprecated_shown_in_plain() {
        let mut skill = test_skill();
        skill.is_deprecated = true;
        let card = SkillCard::new(&skill);
        let output = card.format(OutputFormat::Plain);

        assert!(output.contains("[DEPRECATED]"));
    }
}
