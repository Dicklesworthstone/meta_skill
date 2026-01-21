//! Block classifiers for identifying content types.
//!
//! Each classifier is a stateless heuristic engine that analyzes text
//! and returns a classification result if it matches the classifier's domain.

use regex::Regex;
use std::sync::LazyLock;

use super::types::{ClassificationResult, ClassificationSignal, ContentBlockType};

// =============================================================================
// CLASSIFIER TRAIT
// =============================================================================

/// Trait for content block classifiers.
///
/// Each classifier analyzes a text block and optionally returns a
/// classification result if it detects patterns matching its domain.
pub trait BlockClassifier: Send + Sync {
    /// Attempt to classify a block of text.
    ///
    /// Returns `Some(ClassificationResult)` if this classifier recognizes
    /// the content, or `None` if it doesn't match this classifier's domain.
    fn classify(&self, block: &str) -> Option<ClassificationResult>;

    /// Returns the name of this classifier for debugging.
    fn name(&self) -> &'static str;
}

// =============================================================================
// RULE CLASSIFIER
// =============================================================================

/// Classifier for rule/guideline content.
///
/// Detects imperative statements, policy declarations, and guidelines.
pub struct RuleClassifier;

// Precompiled patterns for rules
static IMPERATIVE_STARTERS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    vec![
        "always",
        "never",
        "must",
        "should",
        "do not",
        "don't",
        "ensure",
        "make sure",
        "remember to",
        "be sure to",
        "prefer",
        "avoid",
        "use",
        "keep",
    ]
});

static RULE_MARKERS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    vec![
        "rule:",
        "guideline:",
        "policy:",
        "principle:",
        "requirement:",
    ]
});

static ACTION_VERBS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    vec![
        "use",
        "avoid",
        "prefer",
        "implement",
        "create",
        "handle",
        "return",
        "throw",
        "catch",
        "log",
        "validate",
        "check",
        "ensure",
        "verify",
        "test",
        "write",
        "read",
        "call",
    ]
});

impl BlockClassifier for RuleClassifier {
    fn classify(&self, block: &str) -> Option<ClassificationResult> {
        let mut signals = Vec::new();
        let mut score = 0.0;
        let lower = block.to_lowercase();

        // Check for imperative starters
        for starter in IMPERATIVE_STARTERS.iter() {
            if lower.starts_with(starter) || lower.starts_with(&format!("- {starter}")) {
                score += 0.4;
                signals.push(ClassificationSignal::new("imperative_start", *starter, 0.4));
                break;
            }
        }

        // Check for rule markers
        for marker in RULE_MARKERS.iter() {
            if lower.contains(marker) {
                score += 0.3;
                signals.push(ClassificationSignal::new("rule_marker", *marker, 0.3));
                break;
            }
        }

        // Check for action verbs at line starts (bullet points)
        let bullet_action_count = block
            .lines()
            .filter(|line| {
                let trimmed = line.trim().to_lowercase();
                let after_bullet = trimmed
                    .strip_prefix("- ")
                    .or_else(|| trimmed.strip_prefix("* "))
                    .or_else(|| trimmed.strip_prefix("• "))
                    .unwrap_or(&trimmed);
                ACTION_VERBS.iter().any(|v| after_bullet.starts_with(v))
            })
            .count();

        if bullet_action_count >= 2 {
            score += 0.3;
            signals.push(ClassificationSignal::new(
                "action_verb_bullets",
                format!("{} lines", bullet_action_count),
                0.3,
            ));
        }

        // Check for modal verbs indicating rules
        let modal_count = ["must", "should", "shall", "will", "can't", "cannot"]
            .iter()
            .filter(|m| lower.contains(*m))
            .count();
        if modal_count > 0 {
            let weight = (modal_count as f32 * 0.1).min(0.3);
            score += weight;
            signals.push(ClassificationSignal::new(
                "modal_verbs",
                format!("{} modals", modal_count),
                weight,
            ));
        }

        // Negative: code fences are more likely examples
        if block.contains("```") {
            score -= 0.2;
        }

        if score > 0.3 {
            Some(ClassificationResult::new(
                ContentBlockType::Rule,
                score.min(1.0),
                signals,
            ))
        } else {
            None
        }
    }

    fn name(&self) -> &'static str {
        "RuleClassifier"
    }
}

// =============================================================================
// EXAMPLE CLASSIFIER
// =============================================================================

/// Classifier for code examples and demonstrations.
///
/// Detects code fences, indented code, and example markers.
pub struct ExampleClassifier;

static EXAMPLE_MARKERS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    vec![
        "example:",
        "for example:",
        "for instance:",
        "e.g.",
        "such as:",
        "like this:",
        "here's how:",
        "here is how:",
        "consider:",
        "sample:",
        "demo:",
    ]
});

static CODE_FENCE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"```[\w]*").expect("valid regex"));

impl BlockClassifier for ExampleClassifier {
    fn classify(&self, block: &str) -> Option<ClassificationResult> {
        let mut signals = Vec::new();
        let mut score = 0.0;
        let lower = block.to_lowercase();

        // Code fences are strong signals
        let fence_count = CODE_FENCE_REGEX.find_iter(block).count();
        if fence_count >= 2 {
            // Paired fences (opening + closing)
            score += 0.6;
            signals.push(ClassificationSignal::new(
                "code_fence",
                format!("{} fences", fence_count),
                0.6,
            ));
        } else if fence_count == 1 {
            score += 0.3;
            signals.push(ClassificationSignal::new("code_fence", "1 fence", 0.3));
        }

        // Indented code blocks (4 spaces or tab)
        let indented_lines = block
            .lines()
            .filter(|l| l.starts_with("    ") || l.starts_with('\t'))
            .count();
        if indented_lines > 2 {
            let weight = (indented_lines as f32 * 0.05).min(0.4);
            score += weight;
            signals.push(ClassificationSignal::new(
                "indented_code",
                format!("{} lines", indented_lines),
                weight,
            ));
        }

        // Example markers
        for marker in EXAMPLE_MARKERS.iter() {
            if lower.contains(marker) {
                score += 0.3;
                signals.push(ClassificationSignal::new("example_marker", *marker, 0.3));
                break;
            }
        }

        // Before/after pattern (common in examples)
        if (lower.contains("before:") || lower.contains("before\n"))
            && (lower.contains("after:") || lower.contains("after\n"))
        {
            score += 0.4;
            signals.push(ClassificationSignal::new(
                "before_after_pattern",
                "before/after",
                0.4,
            ));
        }

        // Code-like syntax indicators (not in fences)
        if !block.contains("```") {
            let code_indicators = [
                "fn ",
                "def ",
                "function ",
                "class ",
                "import ",
                "require(",
                "const ",
                "let ",
                "var ",
                "return ",
                "if (",
                "for (",
                "while (",
                "=>",
                "->",
                "::",
                "pub fn",
                "async fn",
            ];
            let indicator_count = code_indicators
                .iter()
                .filter(|ind| block.contains(*ind))
                .count();
            if indicator_count >= 2 {
                let weight = (indicator_count as f32 * 0.1).min(0.4);
                score += weight;
                signals.push(ClassificationSignal::new(
                    "code_syntax",
                    format!("{} indicators", indicator_count),
                    weight,
                ));
            }
        }

        if score > 0.3 {
            Some(ClassificationResult::new(
                ContentBlockType::Example,
                score.min(1.0),
                signals,
            ))
        } else {
            None
        }
    }

    fn name(&self) -> &'static str {
        "ExampleClassifier"
    }
}

// =============================================================================
// PITFALL CLASSIFIER
// =============================================================================

/// Classifier for warnings, anti-patterns, and things to avoid.
///
/// Detects warning markers, anti-pattern phrases, and cautionary language.
pub struct PitfallClassifier;

static WARNING_MARKERS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    vec![
        "warning:",
        "caution:",
        "note:",
        "important:",
        "danger:",
        "alert:",
        "beware:",
        "careful:",
    ]
});

static WARNING_EMOJIS: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| vec!["⚠️", "❌", "🚫", "❗", "⛔", "💀", "🔥", "⚡"]);

static ANTI_PATTERN_PHRASES: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    vec![
        "don't",
        "do not",
        "avoid",
        "never",
        "common mistake",
        "anti-pattern",
        "pitfall",
        "wrong way",
        "bad practice",
        "gotcha",
        "trap",
        "foot gun",
        "footgun",
        "careful not to",
        "watch out",
        "be careful",
        "prone to",
    ]
});

impl BlockClassifier for PitfallClassifier {
    fn classify(&self, block: &str) -> Option<ClassificationResult> {
        let mut signals = Vec::new();
        let mut score = 0.0;
        let lower = block.to_lowercase();

        // Warning markers (text)
        for marker in WARNING_MARKERS.iter() {
            if lower.contains(marker) {
                score += 0.4;
                signals.push(ClassificationSignal::new("warning_marker", *marker, 0.4));
                break;
            }
        }

        // Warning emojis
        for emoji in WARNING_EMOJIS.iter() {
            if block.contains(emoji) {
                score += 0.3;
                signals.push(ClassificationSignal::new("warning_emoji", *emoji, 0.3));
                break;
            }
        }

        // Anti-pattern phrases
        let anti_pattern_count = ANTI_PATTERN_PHRASES
            .iter()
            .filter(|p| lower.contains(*p))
            .count();
        if anti_pattern_count > 0 {
            let weight = (anti_pattern_count as f32 * 0.15).min(0.5);
            score += weight;
            signals.push(ClassificationSignal::new(
                "anti_pattern_phrase",
                format!("{} phrases", anti_pattern_count),
                weight,
            ));
        }

        // Negative consequence language
        let consequence_phrases = [
            "will fail",
            "will crash",
            "will break",
            "causes",
            "leads to",
            "results in",
            "performance issue",
            "memory leak",
            "security risk",
        ];
        let consequence_count = consequence_phrases
            .iter()
            .filter(|p| lower.contains(*p))
            .count();
        if consequence_count > 0 {
            let weight = (consequence_count as f32 * 0.1).min(0.3);
            score += weight;
            signals.push(ClassificationSignal::new(
                "consequence_language",
                format!("{} phrases", consequence_count),
                weight,
            ));
        }

        if score > 0.3 {
            Some(ClassificationResult::new(
                ContentBlockType::Pitfall,
                score.min(1.0),
                signals,
            ))
        } else {
            None
        }
    }

    fn name(&self) -> &'static str {
        "PitfallClassifier"
    }
}

// =============================================================================
// CHECKLIST CLASSIFIER
// =============================================================================

/// Classifier for checklists, numbered steps, and procedural content.
///
/// Detects checkbox patterns, numbered lists, and step markers.
pub struct ChecklistClassifier;

static CHECKBOX_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*[-*]\s*\[[xX ]\]").expect("valid regex"));

static NUMBERED_STEP_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*\d+\.\s+\w").expect("valid regex"));

static STEP_MARKERS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    vec![
        "step 1", "step 2", "step 3", "first,", "second,", "third,", "then,", "next,", "finally,",
        "firstly", "secondly", "thirdly", "lastly",
    ]
});

impl BlockClassifier for ChecklistClassifier {
    fn classify(&self, block: &str) -> Option<ClassificationResult> {
        let mut signals = Vec::new();
        let mut score = 0.0;
        let lower = block.to_lowercase();

        // Checkbox pattern (strongest signal)
        let checkbox_count = block.lines().filter(|l| CHECKBOX_REGEX.is_match(l)).count();
        if checkbox_count > 0 {
            let weight = (checkbox_count as f32 * 0.2).min(0.7);
            score += weight;
            signals.push(ClassificationSignal::new(
                "checkbox_pattern",
                format!("{} checkboxes", checkbox_count),
                weight,
            ));
        }

        // Numbered list with action words
        let numbered_count = block
            .lines()
            .filter(|l| NUMBERED_STEP_REGEX.is_match(l))
            .count();
        if numbered_count > 2 {
            let weight = (numbered_count as f32 * 0.1).min(0.5);
            score += weight;
            signals.push(ClassificationSignal::new(
                "numbered_steps",
                format!("{} steps", numbered_count),
                weight,
            ));
        }

        // Step markers
        let step_marker_count = STEP_MARKERS.iter().filter(|m| lower.contains(*m)).count();
        if step_marker_count > 0 {
            let weight = (step_marker_count as f32 * 0.1).min(0.4);
            score += weight;
            signals.push(ClassificationSignal::new(
                "step_markers",
                format!("{} markers", step_marker_count),
                weight,
            ));
        }

        // Checklist header indicators
        if lower.contains("checklist")
            || lower.contains("before you")
            || lower.contains("prerequisites")
            || lower.contains("requirements:")
            || lower.contains("to do:")
            || lower.contains("todo:")
        {
            score += 0.3;
            signals.push(ClassificationSignal::new(
                "checklist_header",
                "header keyword",
                0.3,
            ));
        }

        // Process/procedure indicators
        if lower.contains("process:") || lower.contains("procedure:") || lower.contains("workflow:")
        {
            score += 0.2;
            signals.push(ClassificationSignal::new(
                "procedure_marker",
                "procedure keyword",
                0.2,
            ));
        }

        if score > 0.3 {
            Some(ClassificationResult::new(
                ContentBlockType::Checklist,
                score.min(1.0),
                signals,
            ))
        } else {
            None
        }
    }

    fn name(&self) -> &'static str {
        "ChecklistClassifier"
    }
}

// =============================================================================
// CONTEXT CLASSIFIER
// =============================================================================

/// Classifier for background information and context.
///
/// Detects descriptive text, prerequisites, and explanatory content.
pub struct ContextClassifier;

static CONTEXT_MARKERS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    vec![
        "background:",
        "context:",
        "overview:",
        "introduction:",
        "about:",
        "description:",
        "summary:",
        "tldr:",
        "this document",
        "this guide",
        "this skill",
        "in this",
        "the following",
    ]
});

impl BlockClassifier for ContextClassifier {
    fn classify(&self, block: &str) -> Option<ClassificationResult> {
        let mut signals = Vec::new();
        let mut score = 0.0;
        let lower = block.to_lowercase();

        // Context markers
        for marker in CONTEXT_MARKERS.iter() {
            if lower.contains(marker) {
                score += 0.4;
                signals.push(ClassificationSignal::new("context_marker", *marker, 0.4));
                break;
            }
        }

        // Explanatory sentence patterns
        let explanatory_patterns = [
            "is a ",
            "is an ",
            "are ",
            "was ",
            "were ",
            "provides ",
            "enables ",
            "allows ",
            "helps ",
            "designed to ",
            "intended to ",
            "meant to ",
        ];
        let explanatory_count = explanatory_patterns
            .iter()
            .filter(|p| lower.contains(*p))
            .count();
        if explanatory_count >= 2 {
            let weight = (explanatory_count as f32 * 0.1).min(0.3);
            score += weight;
            signals.push(ClassificationSignal::new(
                "explanatory_patterns",
                format!("{} patterns", explanatory_count),
                weight,
            ));
        }

        // Paragraph-like structure (multiple sentences, no bullets)
        let sentence_count = block.matches(". ").count() + block.matches(".\n").count();
        let has_bullets = block.contains("- ") || block.contains("* ") || block.contains("• ");
        if sentence_count >= 2 && !has_bullets && !block.contains("```") {
            score += 0.2;
            signals.push(ClassificationSignal::new(
                "paragraph_structure",
                format!("{} sentences", sentence_count),
                0.2,
            ));
        }

        // Prerequisites/assumptions
        if lower.contains("prerequisite")
            || lower.contains("assumes")
            || lower.contains("assuming")
            || lower.contains("requires")
        {
            score += 0.3;
            signals.push(ClassificationSignal::new(
                "prerequisite_language",
                "prerequisite",
                0.3,
            ));
        }

        if score > 0.3 {
            Some(ClassificationResult::new(
                ContentBlockType::Context,
                score.min(1.0),
                signals,
            ))
        } else {
            None
        }
    }

    fn name(&self) -> &'static str {
        "ContextClassifier"
    }
}

// =============================================================================
// METADATA CLASSIFIER
// =============================================================================

/// Classifier for metadata, titles, and header content.
///
/// Detects skill names, version info, and structural metadata.
pub struct MetadataClassifier;

static HEADER_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^#{1,3}\s+.+$").expect("valid regex"));

impl BlockClassifier for MetadataClassifier {
    fn classify(&self, block: &str) -> Option<ClassificationResult> {
        let mut signals = Vec::new();
        let mut score = 0.0;
        let lower = block.to_lowercase();
        let trimmed = block.trim();

        // Markdown headers (especially top-level)
        if HEADER_REGEX.is_match(trimmed) {
            let header_level = trimmed.chars().take_while(|c| *c == '#').count();
            let weight = if header_level == 1 { 0.6 } else { 0.4 };
            score += weight;
            signals.push(ClassificationSignal::new(
                "markdown_header",
                format!("h{}", header_level),
                weight,
            ));
        }

        // YAML frontmatter indicators
        if trimmed.starts_with("---") && trimmed.lines().count() > 1 {
            score += 0.5;
            signals.push(ClassificationSignal::new(
                "yaml_frontmatter",
                "--- block",
                0.5,
            ));
        }

        // Metadata-like key-value pairs
        let kv_patterns = [
            "version:",
            "author:",
            "date:",
            "tags:",
            "id:",
            "name:",
            "description:",
            "license:",
            "created:",
            "updated:",
            "category:",
            "type:",
            "status:",
        ];
        let kv_count = kv_patterns.iter().filter(|p| lower.contains(*p)).count();
        if kv_count >= 2 {
            let weight = (kv_count as f32 * 0.15).min(0.5);
            score += weight;
            signals.push(ClassificationSignal::new(
                "metadata_keys",
                format!("{} keys", kv_count),
                weight,
            ));
        }

        // Short, title-like content (all caps or title case, short)
        if trimmed.len() < 80 && !trimmed.contains('\n') {
            let words: Vec<&str> = trimmed.split_whitespace().collect();
            let capitalized_words = words
                .iter()
                .filter(|w| w.chars().next().is_some_and(|c| c.is_uppercase()))
                .count();
            if words.len() <= 8 && capitalized_words > words.len() / 2 {
                score += 0.2;
                signals.push(ClassificationSignal::new(
                    "title_case",
                    "short title-like",
                    0.2,
                ));
            }
        }

        if score > 0.3 {
            Some(ClassificationResult::new(
                ContentBlockType::Metadata,
                score.min(1.0),
                signals,
            ))
        } else {
            None
        }
    }

    fn name(&self) -> &'static str {
        "MetadataClassifier"
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Rule classifier tests
    #[test]
    fn test_rule_classifier_imperative() {
        let classifier = RuleClassifier;

        let result = classifier.classify("Always handle errors explicitly");
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.block_type, ContentBlockType::Rule);
        assert!(result.confidence > 0.3);
    }

    #[test]
    fn test_rule_classifier_bullet_rules() {
        let classifier = RuleClassifier;

        let text =
            "- Use meaningful variable names\n- Avoid global state\n- Handle errors properly";
        let result = classifier.classify(text);
        assert!(result.is_some());
    }

    #[test]
    fn test_rule_classifier_no_match() {
        let classifier = RuleClassifier;

        let result = classifier.classify("This is just some descriptive text.");
        // Should not match or have low confidence
        assert!(result.is_none() || result.unwrap().confidence <= 0.3);
    }

    // Example classifier tests
    #[test]
    fn test_example_classifier_code_fence() {
        let classifier = ExampleClassifier;

        let text = "```rust\nfn main() {\n    println!(\"Hello\");\n}\n```";
        let result = classifier.classify(text);
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.block_type, ContentBlockType::Example);
        assert!(result.confidence >= 0.5);
    }

    #[test]
    fn test_example_classifier_before_after() {
        let classifier = ExampleClassifier;

        let text = "Before:\n  old_code()\n\nAfter:\n  new_code()";
        let result = classifier.classify(text);
        assert!(result.is_some());
    }

    // Pitfall classifier tests
    #[test]
    fn test_pitfall_classifier_warning() {
        let classifier = PitfallClassifier;

        let text = "Warning: Never use eval() with user input";
        let result = classifier.classify(text);
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.block_type, ContentBlockType::Pitfall);
    }

    #[test]
    fn test_pitfall_classifier_emoji() {
        let classifier = PitfallClassifier;

        let text = "⚠️ Common mistake: forgetting to close file handles";
        let result = classifier.classify(text);
        assert!(result.is_some());
    }

    // Checklist classifier tests
    #[test]
    fn test_checklist_classifier_checkboxes() {
        let classifier = ChecklistClassifier;

        let text = "- [ ] Write tests\n- [x] Review code\n- [ ] Deploy";
        let result = classifier.classify(text);
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.block_type, ContentBlockType::Checklist);
        assert!(result.confidence >= 0.5);
    }

    #[test]
    fn test_checklist_classifier_numbered() {
        let classifier = ChecklistClassifier;

        let text = "1. Clone the repository\n2. Install dependencies\n3. Run tests\n4. Deploy";
        let result = classifier.classify(text);
        assert!(result.is_some());
    }

    // Context classifier tests
    #[test]
    fn test_context_classifier_overview() {
        let classifier = ContextClassifier;

        let text =
            "Overview: This document describes the error handling patterns used in our codebase.";
        let result = classifier.classify(text);
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.block_type, ContentBlockType::Context);
    }

    #[test]
    fn test_context_classifier_paragraph() {
        let classifier = ContextClassifier;

        let text = "This skill provides guidance on error handling. It is designed to help developers write more robust code. The patterns here have been battle-tested in production.";
        let result = classifier.classify(text);
        assert!(result.is_some());
    }

    // Metadata classifier tests
    #[test]
    fn test_metadata_classifier_header() {
        let classifier = MetadataClassifier;

        let result = classifier.classify("# Error Handling Best Practices");
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.block_type, ContentBlockType::Metadata);
    }

    #[test]
    fn test_metadata_classifier_yaml() {
        let classifier = MetadataClassifier;

        let text = "---\nid: error-handling\nversion: 1.0\nauthor: test\n---";
        let result = classifier.classify(text);
        assert!(result.is_some());
    }
}
