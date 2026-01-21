//! Quality and performance validation rules for skills.
//!
//! These rules check for content quality (descriptions, actionable rules, examples)
//! and provide performance guidance (token budgets, embedding quality).

use crate::core::skill::{BlockType, SkillSpec};
use crate::lint::config::ValidationContext;
use crate::lint::diagnostic::{Diagnostic, RuleCategory, Severity};
use crate::lint::rule::ValidationRule;

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/// Extract all text content from a skill for analysis.
fn extract_all_content(skill: &SkillSpec) -> String {
    let mut content = String::new();

    // Add metadata content
    content.push_str(&skill.metadata.name);
    content.push('\n');
    content.push_str(&skill.metadata.description);
    content.push('\n');

    // Add all section content
    for section in &skill.sections {
        content.push_str(&section.title);
        content.push('\n');
        for block in &section.blocks {
            content.push_str(&block.content);
            content.push('\n');
        }
    }

    content
}

/// Estimate code ratio in content (rough heuristic).
fn estimate_code_ratio(content: &str) -> f64 {
    if content.is_empty() {
        return 0.0;
    }

    let total_lines: usize = content.lines().count();
    if total_lines == 0 {
        return 0.0;
    }

    let mut in_code_block = false;
    let mut code_lines = 0;

    for line in content.lines() {
        if line.trim().starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block {
            code_lines += 1;
        } else if line.starts_with("    ") || line.starts_with('\t') {
            // Indented code
            code_lines += 1;
        }
    }

    f64::from(code_lines) / total_lines as f64
}

/// Estimate token count from text (rough: ~4 chars per token).
const fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}

// =============================================================================
// QUALITY RULES
// =============================================================================

/// Rule that checks for meaningful descriptions.
pub struct MeaningfulDescriptionRule {
    min_length: usize,
    max_length: usize,
}

impl Default for MeaningfulDescriptionRule {
    fn default() -> Self {
        Self {
            min_length: 20,
            max_length: 500,
        }
    }
}

impl MeaningfulDescriptionRule {
    /// Create a rule with custom length bounds.
    #[must_use]
    pub const fn with_bounds(min_length: usize, max_length: usize) -> Self {
        Self {
            min_length,
            max_length,
        }
    }
}

impl ValidationRule for MeaningfulDescriptionRule {
    fn id(&self) -> &'static str {
        "meaningful-description"
    }

    fn name(&self) -> &'static str {
        "Meaningful Description"
    }

    fn description(&self) -> &'static str {
        "Description should be informative and appropriate length"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Quality
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn validate(&self, ctx: &ValidationContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let skill = ctx.skill;
        let desc = &skill.metadata.description;

        // If empty, the required-metadata rule handles that
        if desc.is_empty() {
            return diagnostics;
        }

        // Length checks
        if desc.len() < self.min_length {
            diagnostics.push(
                Diagnostic::warning(
                    self.id(),
                    format!(
                        "Description is too short ({} chars, minimum {})",
                        desc.len(),
                        self.min_length
                    ),
                )
                .with_suggestion("Add more detail about what this skill helps with")
                .with_category(RuleCategory::Quality),
            );
        }

        if desc.len() > self.max_length {
            diagnostics.push(
                Diagnostic::info(
                    self.id(),
                    format!("Description is quite long ({} chars)", desc.len()),
                )
                .with_suggestion("Consider moving details to the overview section")
                .with_category(RuleCategory::Quality),
            );
        }

        // Same as title check
        let id_as_text = skill.metadata.id.replace(['-', '_'], " ");
        if desc.to_lowercase() == id_as_text.to_lowercase() {
            diagnostics.push(
                Diagnostic::warning(self.id(), "Description is the same as the skill ID")
                    .with_suggestion("Provide a more informative description")
                    .with_category(RuleCategory::Quality),
            );
        }

        // Same as name check
        if desc.to_lowercase() == skill.metadata.name.to_lowercase() {
            diagnostics.push(
                Diagnostic::warning(self.id(), "Description is the same as the skill name")
                    .with_suggestion("Provide a more informative description")
                    .with_category(RuleCategory::Quality),
            );
        }

        // Placeholder detection
        let placeholders = [
            "todo",
            "fixme",
            "placeholder",
            "description here",
            "add description",
            "tbd",
            "to be determined",
        ];
        let desc_lower = desc.to_lowercase();
        for placeholder in placeholders {
            if desc_lower.contains(placeholder) {
                diagnostics.push(
                    Diagnostic::warning(self.id(), "Description appears to be a placeholder")
                        .with_suggestion("Replace placeholder with actual description")
                        .with_category(RuleCategory::Quality),
                );
                break;
            }
        }

        diagnostics
    }
}

/// Rule that checks if rules are actionable (start with verbs).
pub struct ActionableRulesRule {
    action_verbs: Vec<&'static str>,
}

impl Default for ActionableRulesRule {
    fn default() -> Self {
        Self {
            action_verbs: vec![
                "use",
                "avoid",
                "always",
                "never",
                "prefer",
                "ensure",
                "check",
                "verify",
                "validate",
                "test",
                "document",
                "consider",
                "implement",
                "create",
                "define",
                "handle",
                "add",
                "remove",
                "delete",
                "update",
                "modify",
                "run",
                "execute",
                "apply",
                "enable",
                "disable",
                "set",
                "configure",
                "do",
                "don't",
                "must",
                "should",
                "shall",
                "keep",
                "maintain",
                "follow",
                "include",
                "exclude",
                "review",
                "write",
                "read",
            ],
        }
    }
}

impl ValidationRule for ActionableRulesRule {
    fn id(&self) -> &'static str {
        "actionable-rules"
    }

    fn name(&self) -> &'static str {
        "Actionable Rules"
    }

    fn description(&self) -> &'static str {
        "Rules should be actionable with verbs"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Quality
    }

    fn default_severity(&self) -> Severity {
        Severity::Info
    }

    fn validate(&self, ctx: &ValidationContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let skill = ctx.skill;

        // Collect all Rule-type blocks
        let mut rule_index = 0;
        for section in &skill.sections {
            for block in &section.blocks {
                if block.block_type == BlockType::Rule {
                    rule_index += 1;
                    let content = block.content.trim();

                    // Skip very short rules
                    if content.len() < 10 {
                        continue;
                    }

                    let first_word = content
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .to_lowercase()
                        .trim_end_matches(['.', ':', '-', '!'])
                        .to_string();

                    let has_action = self.action_verbs.iter().any(|v| first_word == *v);

                    if !has_action {
                        diagnostics.push(
                            Diagnostic::info(
                                self.id(),
                                format!(
                                    "Rule {} in section '{}' may not be actionable",
                                    rule_index, section.title
                                ),
                            )
                            .with_suggestion(
                                "Start rules with action verbs like 'Use', 'Avoid', 'Always'",
                            )
                            .with_category(RuleCategory::Quality),
                        );
                    }
                }
            }
        }

        diagnostics
    }
}

/// Rule that checks if examples contain actual code.
pub struct ExamplesHaveCodeRule;

impl ValidationRule for ExamplesHaveCodeRule {
    fn id(&self) -> &'static str {
        "examples-have-code"
    }

    fn name(&self) -> &'static str {
        "Examples Have Code"
    }

    fn description(&self) -> &'static str {
        "Example sections should contain actual code examples"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Quality
    }

    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn validate(&self, ctx: &ValidationContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let skill = ctx.skill;

        // Check sections that look like examples
        for section in &skill.sections {
            let title_lower = section.title.to_lowercase();
            let is_example_section =
                title_lower.contains("example") || title_lower.contains("usage");

            if !is_example_section {
                continue;
            }

            // Check if section has code blocks
            let has_code = section.blocks.iter().any(|b| {
                b.block_type == BlockType::Code
                    || b.block_type == BlockType::Command
                    || b.content.contains("```")
                    || b.content.lines().any(|line| line.starts_with("    "))
            });

            // Get total content length to avoid flagging very short sections
            let total_content: usize = section.blocks.iter().map(|b| b.content.len()).sum();

            if !has_code && total_content > 50 {
                diagnostics.push(
                    Diagnostic::warning(
                        self.id(),
                        format!("Section '{}' has no code examples", section.title),
                    )
                    .with_suggestion("Add code blocks to example sections")
                    .with_category(RuleCategory::Quality),
                );
            }
        }

        diagnostics
    }
}

/// Rule that checks for balanced content (not too much code, not too little).
pub struct BalancedContentRule {
    max_code_ratio: f64,
}

impl Default for BalancedContentRule {
    fn default() -> Self {
        Self {
            max_code_ratio: 0.8,
        }
    }
}

impl ValidationRule for BalancedContentRule {
    fn id(&self) -> &'static str {
        "balanced-content"
    }

    fn name(&self) -> &'static str {
        "Balanced Content"
    }

    fn description(&self) -> &'static str {
        "Skills should have a balance of prose and code"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Quality
    }

    fn default_severity(&self) -> Severity {
        Severity::Info
    }

    fn validate(&self, ctx: &ValidationContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let content = extract_all_content(ctx.skill);

        if content.len() < 100 {
            // Too short to judge
            return diagnostics;
        }

        let code_ratio = estimate_code_ratio(&content);

        if code_ratio > self.max_code_ratio {
            diagnostics.push(
                Diagnostic::info(
                    self.id(),
                    format!("Content is ~{}% code", (code_ratio * 100.0).round() as u32),
                )
                .with_suggestion("Add prose descriptions for better understanding and search")
                .with_category(RuleCategory::Quality),
            );
        }

        diagnostics
    }
}

// =============================================================================
// PERFORMANCE RULES
// =============================================================================

/// Rule that checks token budget and estimates.
pub struct TokenBudgetRule {
    total_warn_threshold: usize,
    total_error_threshold: usize,
}

impl Default for TokenBudgetRule {
    fn default() -> Self {
        Self {
            total_warn_threshold: 4000,
            total_error_threshold: 8000,
        }
    }
}

impl TokenBudgetRule {
    /// Create a rule with custom thresholds.
    #[must_use]
    pub const fn with_thresholds(warn: usize, error: usize) -> Self {
        Self {
            total_warn_threshold: warn,
            total_error_threshold: error,
        }
    }
}

impl ValidationRule for TokenBudgetRule {
    fn id(&self) -> &'static str {
        "token-budget"
    }

    fn name(&self) -> &'static str {
        "Token Budget"
    }

    fn description(&self) -> &'static str {
        "Estimates token usage and suggests optimizations"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Performance
    }

    fn default_severity(&self) -> Severity {
        Severity::Info
    }

    fn validate(&self, ctx: &ValidationContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let content = extract_all_content(ctx.skill);
        let estimated_tokens = estimate_tokens(&content);

        if estimated_tokens > self.total_error_threshold {
            diagnostics.push(
                Diagnostic::warning(
                    self.id(),
                    format!(
                        "Skill is very large (~{estimated_tokens} tokens). Consider splitting."
                    ),
                )
                .with_suggestion("Large skills load slowly. Split into focused skills.")
                .with_category(RuleCategory::Performance),
            );
        } else if estimated_tokens > self.total_warn_threshold {
            diagnostics.push(
                Diagnostic::info(
                    self.id(),
                    format!(
                        "Skill has ~{} tokens (suggested: <{})",
                        estimated_tokens, self.total_warn_threshold
                    ),
                )
                .with_suggestion("Consider moving content to higher disclosure levels")
                .with_category(RuleCategory::Performance),
            );
        }

        diagnostics
    }
}

/// Rule that checks if content is suitable for semantic search.
pub struct EmbeddingQualityRule {
    min_content_length: usize,
}

impl Default for EmbeddingQualityRule {
    fn default() -> Self {
        Self {
            min_content_length: 100,
        }
    }
}

impl ValidationRule for EmbeddingQualityRule {
    fn id(&self) -> &'static str {
        "embedding-quality"
    }

    fn name(&self) -> &'static str {
        "Embedding Quality"
    }

    fn description(&self) -> &'static str {
        "Checks if content is suitable for semantic search"
    }

    fn category(&self) -> RuleCategory {
        RuleCategory::Performance
    }

    fn default_severity(&self) -> Severity {
        Severity::Info
    }

    fn validate(&self, ctx: &ValidationContext<'_>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let skill = ctx.skill;
        let content = extract_all_content(skill);

        // Too short for good embeddings
        if content.len() < self.min_content_length {
            diagnostics.push(
                Diagnostic::info(
                    self.id(),
                    format!(
                        "Skill has very little text content ({} chars)",
                        content.len()
                    ),
                )
                .with_suggestion("Add more descriptive content for better search")
                .with_category(RuleCategory::Performance),
            );
        }

        // Check description for searchable keywords
        let desc = &skill.metadata.description;
        if !desc.is_empty() {
            // Simple heuristic: description should have at least 3 words
            let word_count = desc.split_whitespace().count();
            if word_count < 3 {
                diagnostics.push(
                    Diagnostic::info(self.id(), "Description has very few keywords")
                        .with_suggestion("Include key terms users might search for")
                        .with_category(RuleCategory::Performance),
                );
            }
        }

        // Check tags
        if skill.metadata.tags.is_empty() {
            diagnostics.push(
                Diagnostic::info(self.id(), "Skill has no tags")
                    .with_suggestion("Add tags to improve discoverability")
                    .with_category(RuleCategory::Performance),
            );
        }

        diagnostics
    }
}

// =============================================================================
// RULE COLLECTION
// =============================================================================

/// Returns all quality validation rules.
#[must_use]
pub fn quality_rules() -> Vec<Box<dyn ValidationRule>> {
    vec![
        Box::new(MeaningfulDescriptionRule::default()),
        Box::new(ActionableRulesRule::default()),
        Box::new(ExamplesHaveCodeRule),
        Box::new(BalancedContentRule::default()),
    ]
}

/// Returns all performance validation rules.
#[must_use]
pub fn performance_rules() -> Vec<Box<dyn ValidationRule>> {
    vec![
        Box::new(TokenBudgetRule::default()),
        Box::new(EmbeddingQualityRule::default()),
    ]
}

/// Returns all quality and performance validation rules.
#[must_use]
pub fn quality_and_performance_rules() -> Vec<Box<dyn ValidationRule>> {
    let mut rules = quality_rules();
    rules.extend(performance_rules());
    rules
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::skill::{SkillBlock, SkillSection};
    use crate::lint::config::ValidationConfig;

    fn make_context<'a>(
        skill: &'a SkillSpec,
        config: &'a ValidationConfig,
    ) -> ValidationContext<'a> {
        ValidationContext::new(skill, config)
    }

    fn skill_with_description(desc: &str) -> SkillSpec {
        let mut skill = SkillSpec::new("test", "Test Skill");
        skill.metadata.description = desc.to_string();
        skill
    }

    fn skill_with_content(content: &str) -> SkillSpec {
        let mut skill = SkillSpec::new("test", "Test Skill");
        skill.metadata.description = "A test skill with some content.".to_string();
        skill.sections.push(SkillSection {
            id: "main".to_string(),
            title: "Main".to_string(),
            blocks: vec![SkillBlock {
                id: "block-1".to_string(),
                block_type: BlockType::Text,
                content: content.to_string(),
            }],
        });
        skill
    }

    // MeaningfulDescriptionRule tests

    #[test]
    fn test_description_too_short() {
        let rule = MeaningfulDescriptionRule::default();
        let config = ValidationConfig::new();
        let skill = skill_with_description("Short");
        let ctx = make_context(&skill, &config);

        let diagnostics = rule.validate(&ctx);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("too short"));
    }

    #[test]
    fn test_description_appropriate_length() {
        let rule = MeaningfulDescriptionRule::default();
        let config = ValidationConfig::new();
        let skill = skill_with_description(
            "This is a meaningful description that explains what the skill does and how it helps developers write better code.",
        );
        let ctx = make_context(&skill, &config);

        let diagnostics = rule.validate(&ctx);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_description_placeholder() {
        let rule = MeaningfulDescriptionRule::default();
        let config = ValidationConfig::new();
        let skill = skill_with_description("TODO: Add description here for this skill");
        let ctx = make_context(&skill, &config);

        let diagnostics = rule.validate(&ctx);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("placeholder"))
        );
    }

    #[test]
    fn test_description_same_as_id() {
        let rule = MeaningfulDescriptionRule::default();
        let config = ValidationConfig::new();
        let mut skill = SkillSpec::new("test-skill", "Test Skill");
        skill.metadata.description = "test skill".to_string(); // same as id with hyphens replaced
        let ctx = make_context(&skill, &config);

        let diagnostics = rule.validate(&ctx);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("same as the skill ID"))
        );
    }

    // ActionableRulesRule tests

    #[test]
    fn test_actionable_rules_clean() {
        let rule = ActionableRulesRule::default();
        let config = ValidationConfig::new();
        let mut skill = SkillSpec::new("test", "Test Skill");
        skill.metadata.description = "A test skill.".to_string();
        skill.sections.push(SkillSection {
            id: "rules".to_string(),
            title: "Rules".to_string(),
            blocks: vec![
                SkillBlock {
                    id: "rule-1".to_string(),
                    block_type: BlockType::Rule,
                    content: "Always validate user input before processing.".to_string(),
                },
                SkillBlock {
                    id: "rule-2".to_string(),
                    block_type: BlockType::Rule,
                    content: "Use error handling for all async operations.".to_string(),
                },
            ],
        });
        let ctx = make_context(&skill, &config);

        let diagnostics = rule.validate(&ctx);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_actionable_rules_non_actionable() {
        let rule = ActionableRulesRule::default();
        let config = ValidationConfig::new();
        let mut skill = SkillSpec::new("test", "Test Skill");
        skill.metadata.description = "A test skill.".to_string();
        skill.sections.push(SkillSection {
            id: "rules".to_string(),
            title: "Rules".to_string(),
            blocks: vec![SkillBlock {
                id: "rule-1".to_string(),
                block_type: BlockType::Rule,
                content: "Input validation is important for security.".to_string(),
            }],
        });
        let ctx = make_context(&skill, &config);

        let diagnostics = rule.validate(&ctx);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("may not be actionable"));
    }

    // ExamplesHaveCodeRule tests

    #[test]
    fn test_examples_with_code() {
        let rule = ExamplesHaveCodeRule;
        let config = ValidationConfig::new();
        let mut skill = SkillSpec::new("test", "Test Skill");
        skill.metadata.description = "A test skill.".to_string();
        skill.sections.push(SkillSection {
            id: "examples".to_string(),
            title: "Examples".to_string(),
            blocks: vec![SkillBlock {
                id: "example-1".to_string(),
                block_type: BlockType::Code,
                content: "let x = 42;".to_string(),
            }],
        });
        let ctx = make_context(&skill, &config);

        let diagnostics = rule.validate(&ctx);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_examples_without_code() {
        let rule = ExamplesHaveCodeRule;
        let config = ValidationConfig::new();
        let mut skill = SkillSpec::new("test", "Test Skill");
        skill.metadata.description = "A test skill.".to_string();
        skill.sections.push(SkillSection {
            id: "examples".to_string(),
            title: "Examples".to_string(),
            blocks: vec![SkillBlock {
                id: "example-1".to_string(),
                block_type: BlockType::Text,
                content: "Here is a long description without any code examples at all, which is not ideal for an example section that should demonstrate usage.".to_string(),
            }],
        });
        let ctx = make_context(&skill, &config);

        let diagnostics = rule.validate(&ctx);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("no code"));
    }

    // TokenBudgetRule tests

    #[test]
    fn test_token_budget_small_skill() {
        let rule = TokenBudgetRule::default();
        let config = ValidationConfig::new();
        let skill = skill_with_content("Small content.");
        let ctx = make_context(&skill, &config);

        let diagnostics = rule.validate(&ctx);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_token_budget_large_skill() {
        let rule = TokenBudgetRule::with_thresholds(100, 200);
        let config = ValidationConfig::new();
        // Create content that exceeds error threshold (200 tokens = ~800 chars)
        let large_content = "x".repeat(1000);
        let skill = skill_with_content(&large_content);
        let ctx = make_context(&skill, &config);

        let diagnostics = rule.validate(&ctx);
        assert!(!diagnostics.is_empty());
        assert!(diagnostics[0].message.contains("large"));
    }

    // EmbeddingQualityRule tests

    #[test]
    fn test_embedding_quality_good() {
        let rule = EmbeddingQualityRule::default();
        let config = ValidationConfig::new();
        let mut skill = skill_with_content(
            "This skill provides comprehensive guidance on error handling patterns in Rust applications.",
        );
        skill.metadata.tags = vec!["rust".to_string(), "error-handling".to_string()];
        let ctx = make_context(&skill, &config);

        let diagnostics = rule.validate(&ctx);
        // Should be clean - has content, tags, and good description
        assert!(
            diagnostics
                .iter()
                .all(|d| !d.message.contains("very little"))
        );
    }

    #[test]
    fn test_embedding_quality_no_tags() {
        let rule = EmbeddingQualityRule::default();
        let config = ValidationConfig::new();
        let skill = skill_with_content("Some content here.");
        let ctx = make_context(&skill, &config);

        let diagnostics = rule.validate(&ctx);
        assert!(diagnostics.iter().any(|d| d.message.contains("no tags")));
    }

    #[test]
    fn test_embedding_quality_sparse_description() {
        let rule = EmbeddingQualityRule::default();
        let config = ValidationConfig::new();
        let mut skill = SkillSpec::new("test", "Test");
        skill.metadata.description = "ab".to_string(); // Only 2 words
        let ctx = make_context(&skill, &config);

        let diagnostics = rule.validate(&ctx);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("few keywords"))
        );
    }

    // BalancedContentRule tests

    #[test]
    fn test_balanced_content_ok() {
        let rule = BalancedContentRule::default();
        let config = ValidationConfig::new();
        let skill = skill_with_content(
            "Here is a prose description explaining the concept.\n\n```rust\nlet x = 42;\n```\n\nAnd more prose explaining what happens.",
        );
        let ctx = make_context(&skill, &config);

        let diagnostics = rule.validate(&ctx);
        assert!(diagnostics.is_empty());
    }

    // Helper function tests

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("1234"), 1);
        assert_eq!(estimate_tokens("12345678"), 2);
    }

    #[test]
    fn test_estimate_code_ratio() {
        let prose_only = "Just some prose text.";
        assert!(estimate_code_ratio(prose_only) < 0.1);

        let mostly_code = "```\ncode line 1\ncode line 2\ncode line 3\n```";
        assert!(estimate_code_ratio(mostly_code) > 0.5);
    }

    // Rule collection tests

    #[test]
    fn test_quality_rules_count() {
        let rules = quality_rules();
        assert_eq!(rules.len(), 4);
    }

    #[test]
    fn test_performance_rules_count() {
        let rules = performance_rules();
        assert_eq!(rules.len(), 2);
    }

    #[test]
    fn test_all_rules_count() {
        let rules = quality_and_performance_rules();
        assert_eq!(rules.len(), 6);
    }

    #[test]
    fn test_rule_ids_unique() {
        let rules = quality_and_performance_rules();
        let mut ids: Vec<&str> = rules.iter().map(|r| r.id()).collect();
        let original_len = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), original_len, "All rule IDs must be unique");
    }
}
