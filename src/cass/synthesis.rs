//! Skill synthesis from patterns
//!
//! Generates skill files from extracted patterns.

use std::collections::HashMap;

use crate::Result;

use super::mining::{Pattern, SimplePatternType};

/// Synthesize a skill from extracted patterns.
///
/// Transforms a collection of patterns into a structured skill draft with:
/// - A name derived from the dominant pattern types
/// - A description summarizing the pattern composition
/// - Structured content organized by pattern type
/// - Tags based on the pattern types present
pub fn synthesize_skill(patterns: &[Pattern]) -> Result<SkillDraft> {
    if patterns.is_empty() {
        return Ok(SkillDraft::default());
    }

    // Group patterns by type
    let mut by_type: HashMap<SimplePatternType, Vec<&Pattern>> = HashMap::new();
    for pattern in patterns {
        by_type
            .entry(pattern.pattern_type)
            .or_default()
            .push(pattern);
    }

    // Find dominant pattern type (most patterns)
    let dominant_type = by_type
        .iter()
        .max_by_key(|(_, pats)| pats.len())
        .map(|(t, _)| *t);

    // Generate name based on dominant pattern type
    let name = generate_name(dominant_type, patterns.len());

    // Generate description
    let description = generate_description(&by_type);

    // Generate structured content
    let content = generate_content(&by_type);

    // Extract tags from pattern types
    let tags = by_type.keys().map(|t| pattern_type_tag(*t)).collect();

    Ok(SkillDraft {
        name,
        description,
        content,
        tags,
    })
}

/// Generate a skill name based on dominant pattern type.
fn generate_name(dominant: Option<SimplePatternType>, pattern_count: usize) -> String {
    let type_name = match dominant {
        Some(SimplePatternType::CommandRecipe) => "Command Recipes",
        Some(SimplePatternType::DiagnosticTree) => "Diagnostic Guide",
        Some(SimplePatternType::Invariant) => "Invariants Collection",
        Some(SimplePatternType::Pitfall) => "Pitfalls to Avoid",
        Some(SimplePatternType::PromptMacro) => "Prompt Macros",
        Some(SimplePatternType::RefactorPlaybook) => "Refactoring Playbook",
        Some(SimplePatternType::Checklist) => "Checklist",
        None => "Extracted Patterns",
    };

    if pattern_count > 1 {
        format!("{type_name} ({pattern_count} patterns)")
    } else {
        type_name.to_string()
    }
}

/// Generate a description summarizing pattern composition.
fn generate_description(by_type: &HashMap<SimplePatternType, Vec<&Pattern>>) -> String {
    let mut parts: Vec<String> = Vec::new();

    for (pattern_type, patterns) in by_type {
        let count = patterns.len();
        let avg_confidence: f32 = patterns.iter().map(|p| p.confidence).sum::<f32>() / count as f32;

        let type_desc = match pattern_type {
            SimplePatternType::CommandRecipe => "command recipes",
            SimplePatternType::DiagnosticTree => "diagnostic decision trees",
            SimplePatternType::Invariant => "invariants to maintain",
            SimplePatternType::Pitfall => "pitfalls to avoid",
            SimplePatternType::PromptMacro => "prompt macros",
            SimplePatternType::RefactorPlaybook => "refactoring playbooks",
            SimplePatternType::Checklist => "checklist items",
        };

        parts.push(format!(
            "{count} {type_desc} (avg confidence: {:.0}%)",
            avg_confidence * 100.0
        ));
    }

    format!("Synthesized skill containing: {}", parts.join(", "))
}

/// Generate structured content organized by pattern type.
fn generate_content(by_type: &HashMap<SimplePatternType, Vec<&Pattern>>) -> String {
    let mut sections: Vec<String> = Vec::new();

    // Order pattern types for consistent output
    let type_order = [
        SimplePatternType::CommandRecipe,
        SimplePatternType::DiagnosticTree,
        SimplePatternType::Invariant,
        SimplePatternType::Pitfall,
        SimplePatternType::PromptMacro,
        SimplePatternType::RefactorPlaybook,
        SimplePatternType::Checklist,
    ];

    for pattern_type in type_order {
        if let Some(patterns) = by_type.get(&pattern_type) {
            let section_title = match pattern_type {
                SimplePatternType::CommandRecipe => "## Command Recipes",
                SimplePatternType::DiagnosticTree => "## Diagnostic Trees",
                SimplePatternType::Invariant => "## Invariants",
                SimplePatternType::Pitfall => "## Pitfalls to Avoid",
                SimplePatternType::PromptMacro => "## Prompt Macros",
                SimplePatternType::RefactorPlaybook => "## Refactoring Playbooks",
                SimplePatternType::Checklist => "## Checklist",
            };

            let mut section = String::new();
            section.push_str(section_title);
            section.push('\n');
            section.push('\n');

            // Sort patterns by confidence (highest first)
            let mut sorted_patterns: Vec<&&Pattern> = patterns.iter().collect();
            sorted_patterns.sort_by(|a, b| {
                b.confidence
                    .partial_cmp(&a.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            for pattern in sorted_patterns {
                section.push_str(&format!(
                    "### {} (confidence: {:.0}%)\n\n",
                    pattern.id,
                    pattern.confidence * 100.0
                ));
                section.push_str(&pattern.content);
                section.push_str("\n\n");
            }

            sections.push(section);
        }
    }

    sections.join("\n")
}

/// Convert pattern type to a tag string.
fn pattern_type_tag(pattern_type: SimplePatternType) -> String {
    match pattern_type {
        SimplePatternType::CommandRecipe => "commands".to_string(),
        SimplePatternType::DiagnosticTree => "debugging".to_string(),
        SimplePatternType::Invariant => "invariants".to_string(),
        SimplePatternType::Pitfall => "pitfalls".to_string(),
        SimplePatternType::PromptMacro => "prompts".to_string(),
        SimplePatternType::RefactorPlaybook => "refactoring".to_string(),
        SimplePatternType::Checklist => "checklist".to_string(),
    }
}

/// A draft skill before finalization
#[derive(Debug, Default, Clone)]
pub struct SkillDraft {
    pub name: String,
    pub description: String,
    pub content: String,
    pub tags: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pattern(id: &str, pattern_type: SimplePatternType, confidence: f32) -> Pattern {
        Pattern {
            id: id.to_string(),
            pattern_type,
            content: format!("Content for {id}"),
            confidence,
        }
    }

    #[test]
    fn test_empty_patterns() {
        let draft = synthesize_skill(&[]).unwrap();
        assert!(draft.name.is_empty());
        assert!(draft.content.is_empty());
        assert!(draft.tags.is_empty());
    }

    #[test]
    fn test_single_pattern() {
        let patterns = vec![sample_pattern(
            "cmd-1",
            SimplePatternType::CommandRecipe,
            0.9,
        )];
        let draft = synthesize_skill(&patterns).unwrap();

        assert!(draft.name.contains("Command Recipes"));
        assert!(draft.content.contains("cmd-1"));
        assert!(draft.content.contains("90%"));
        assert!(draft.tags.contains(&"commands".to_string()));
    }

    #[test]
    fn test_mixed_patterns() {
        let patterns = vec![
            sample_pattern("cmd-1", SimplePatternType::CommandRecipe, 0.9),
            sample_pattern("cmd-2", SimplePatternType::CommandRecipe, 0.8),
            sample_pattern("pit-1", SimplePatternType::Pitfall, 0.7),
        ];
        let draft = synthesize_skill(&patterns).unwrap();

        // Dominant type is CommandRecipe (2 patterns)
        assert!(draft.name.contains("Command Recipes"));
        assert!(draft.name.contains("3 patterns"));

        // Content should have both sections
        assert!(draft.content.contains("## Command Recipes"));
        assert!(draft.content.contains("## Pitfalls to Avoid"));

        // Tags should include both types
        assert!(draft.tags.contains(&"commands".to_string()));
        assert!(draft.tags.contains(&"pitfalls".to_string()));
    }

    #[test]
    fn test_patterns_sorted_by_confidence() {
        let patterns = vec![
            sample_pattern("low", SimplePatternType::CommandRecipe, 0.3),
            sample_pattern("high", SimplePatternType::CommandRecipe, 0.95),
            sample_pattern("mid", SimplePatternType::CommandRecipe, 0.6),
        ];
        let draft = synthesize_skill(&patterns).unwrap();

        // High confidence pattern should appear first in content
        let high_pos = draft.content.find("high").unwrap();
        let mid_pos = draft.content.find("mid").unwrap();
        let low_pos = draft.content.find("low").unwrap();

        assert!(high_pos < mid_pos, "High confidence should come first");
        assert!(mid_pos < low_pos, "Mid confidence should come before low");
    }
}
