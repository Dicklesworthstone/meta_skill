//! Content formatting utilities for skill generation.
//!
//! Provides text normalization, code extraction, and checklist parsing
//! for transforming classified blocks into well-formed skill content.

use regex::Regex;
use std::sync::LazyLock;

// =============================================================================
// RULE FORMATTING
// =============================================================================

/// Normalization patterns for converting various phrasings to imperative form.
/// Each tuple is (regex, replacement) where $N refers to capture groups.
static NORMALIZATION_PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        // "You should always X" -> "Always X" (preserve "always")
        (Regex::new(r"(?i)^you should always ").unwrap(), "Always "),
        // "You should X" -> "X"
        (Regex::new(r"(?i)^you should ").unwrap(), ""),
        // "It is important to X" -> "X"
        (Regex::new(r"(?i)^it is important to ").unwrap(), ""),
        // "Make sure you X" / "Make sure to X" -> "X"
        (Regex::new(r"(?i)^make sure (you |to )?").unwrap(), ""),
        // "Remember to X" -> "X"
        (Regex::new(r"(?i)^remember to ").unwrap(), ""),
        // "Be sure to X" -> "X"
        (Regex::new(r"(?i)^be sure to ").unwrap(), ""),
        // "Don't forget to X" -> "X"
        (Regex::new(r"(?i)^don'?t forget to ").unwrap(), ""),
        // "You must X" -> "X"
        (Regex::new(r"(?i)^you must ").unwrap(), ""),
        // "You need to X" -> "X"
        (Regex::new(r"(?i)^you need to ").unwrap(), ""),
        // "It's best to X" -> "X"
        (Regex::new(r"(?i)^it'?s best to ").unwrap(), ""),
    ]
});

/// Normalize text to imperative form.
///
/// Removes common preambles like "You should always", "Make sure to", etc.
/// and capitalizes the first letter.
///
/// # Examples
///
/// ```ignore
/// assert_eq!(normalize_to_imperative("You should always handle errors"), "Always handle errors");
/// assert_eq!(normalize_to_imperative("Make sure to validate input"), "Validate input");
/// ```
#[must_use]
pub fn normalize_to_imperative(text: &str) -> String {
    let mut result = text.trim().to_string();

    for (pattern, replacement) in NORMALIZATION_PATTERNS.iter() {
        if pattern.is_match(&result) {
            result = pattern.replace(&result, *replacement).into_owned();
            break;
        }
    }

    capitalize_first(&result)
}

/// Capitalize the first character of a string.
#[must_use]
pub fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// Format a rule block for inclusion in a skill.
///
/// - Normalizes to imperative form
/// - Cleans up whitespace
/// - Keeps short rules on single line
#[must_use]
pub fn format_rule(content: &str) -> String {
    let normalized = normalize_to_imperative(content.trim());

    // If short and single line, keep as-is
    if normalized.lines().count() == 1 && normalized.len() < 100 {
        return normalized;
    }

    // Multi-line: clean up each line
    normalized
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

// =============================================================================
// EXAMPLE FORMATTING
// =============================================================================

/// Code fence regex for extracting code blocks.
static CODE_FENCE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"```(\w*)\n([\s\S]*?)```").unwrap());

/// A code block extracted from content.
#[derive(Debug, Clone)]
pub struct ExtractedCode {
    /// The code content
    pub code: String,
    /// Detected or specified language
    pub language: Option<String>,
}

/// Extract code blocks from content.
///
/// Finds all fenced code blocks (```...```) and returns them with
/// their detected language.
#[must_use]
pub fn extract_code_blocks(content: &str) -> Vec<ExtractedCode> {
    CODE_FENCE_REGEX
        .captures_iter(content)
        .map(|cap| {
            let lang = cap.get(1).map(|m| m.as_str()).filter(|s| !s.is_empty());
            let code = cap.get(2).map(|m| m.as_str()).unwrap_or("");

            ExtractedCode {
                code: code.trim_end().to_string(),
                language: lang.map(String::from),
            }
        })
        .collect()
}

/// Extract non-code text from content.
///
/// Returns the content with all code blocks removed, useful for
/// extracting descriptions that surround code examples.
#[must_use]
pub fn extract_description(content: &str) -> String {
    let without_code = CODE_FENCE_REGEX.replace_all(content, "");

    without_code
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Extract title from example content.
///
/// Looks for markdown headers or "Example:" prefixes.
#[must_use]
pub fn extract_example_title(content: &str) -> Option<String> {
    // Look for markdown header
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(title) = trimmed.strip_prefix('#') {
            let title = title.trim_start_matches('#').trim();
            if !title.is_empty() {
                return Some(title.to_string());
            }
        }
    }

    // Look for "Example:" or "Example -" prefix
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed
            .strip_prefix("Example:")
            .or_else(|| trimmed.strip_prefix("Example -"))
        {
            let title = rest.trim();
            if !title.is_empty() {
                return Some(title.to_string());
            }
        }
    }

    None
}

/// Detect programming language from code content.
///
/// Uses heuristics to guess the language if not specified.
#[must_use]
pub fn detect_language(code: &str) -> Option<String> {
    let code = code.trim();

    // Rust indicators
    if code.contains("fn ") && (code.contains("->") || code.contains("pub ")) {
        return Some("rust".to_string());
    }
    if code.contains("let mut ") || code.contains("impl ") {
        return Some("rust".to_string());
    }

    // Python indicators
    if code.contains("def ") && code.contains(":") {
        return Some("python".to_string());
    }
    if code.starts_with("import ") || code.starts_with("from ") {
        return Some("python".to_string());
    }

    // JavaScript/TypeScript indicators
    if code.contains("const ") || code.contains("let ") {
        if code.contains(": ") && (code.contains("string") || code.contains("number")) {
            return Some("typescript".to_string());
        }
        return Some("javascript".to_string());
    }
    if code.contains("function ") || code.contains("=>") {
        return Some("javascript".to_string());
    }

    // Shell indicators
    if code.starts_with("#!/") || code.starts_with("$ ") {
        return Some("bash".to_string());
    }

    // Go indicators
    if code.contains("func ") && code.contains("package ") {
        return Some("go".to_string());
    }

    // YAML indicators
    if code.lines().all(|l| {
        l.trim().is_empty() || l.contains(": ") || l.starts_with('-') || l.starts_with('#')
    }) {
        if code.contains(": ") {
            return Some("yaml".to_string());
        }
    }

    // JSON indicators
    if (code.starts_with('{') && code.ends_with('}'))
        || (code.starts_with('[') && code.ends_with(']'))
    {
        return Some("json".to_string());
    }

    None
}

// =============================================================================
// CHECKLIST FORMATTING
// =============================================================================

/// A parsed checklist item.
#[derive(Debug, Clone)]
pub struct ChecklistItem {
    /// The item text
    pub text: String,
    /// Whether the item is checked
    pub checked: bool,
}

/// Checkbox pattern: "- [ ] item" or "- [x] item"
static CHECKBOX_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*[-*]?\s*\[([ xX])\]\s*(.*)$").unwrap());

/// Numbered item pattern: "1. item" or "1) item"
static NUMBERED_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*\d+[.)]\s+(.*)$").unwrap());

/// Parse checklist items from content.
///
/// Recognizes:
/// - Checkbox items: `- [ ] item` or `- [x] item`
/// - Numbered items: `1. item` or `1) item`
#[must_use]
pub fn parse_checklist(content: &str) -> Vec<ChecklistItem> {
    let mut items = Vec::new();

    for line in content.lines() {
        // Try checkbox pattern first
        if let Some(caps) = CHECKBOX_REGEX.captures(line) {
            let checked = &caps[1] != " ";
            let text = caps[2].trim().to_string();
            if !text.is_empty() {
                items.push(ChecklistItem { text, checked });
                continue;
            }
        }

        // Try numbered pattern
        if let Some(caps) = NUMBERED_REGEX.captures(line) {
            let text = caps[1].trim().to_string();
            if !text.is_empty() {
                items.push(ChecklistItem {
                    text,
                    checked: false,
                });
            }
        }
    }

    items
}

// =============================================================================
// PITFALL FORMATTING
// =============================================================================

/// Clean up pitfall/warning content.
///
/// - Removes warning emoji prefixes
/// - Normalizes "Don't" / "Avoid" phrasing
/// - Cleans up whitespace
#[must_use]
pub fn format_pitfall(content: &str) -> String {
    let mut text = content.trim().to_string();

    // Remove common warning prefixes
    let prefixes = ["⚠️", "⛔", "🚫", "❌", "Warning:", "WARN:", "Caution:"];
    for prefix in prefixes {
        if let Some(rest) = text.strip_prefix(prefix) {
            text = rest.trim().to_string();
            break;
        }
    }

    // Capitalize first letter
    capitalize_first(&text)
}

// =============================================================================
// METADATA EXTRACTION
// =============================================================================

/// Key-value pair extracted from metadata.
#[derive(Debug, Clone)]
pub struct MetadataKV {
    pub key: String,
    pub value: String,
}

/// YAML-like key-value pattern.
static KV_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\w+):\s*(.+)$").unwrap());

/// Extract key-value pairs from metadata content.
///
/// Recognizes YAML-style `key: value` pairs.
#[must_use]
pub fn extract_metadata_kv(content: &str) -> Vec<MetadataKV> {
    let mut pairs = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        // Skip YAML delimiters
        if trimmed == "---" {
            continue;
        }

        if let Some(caps) = KV_REGEX.captures(trimmed) {
            pairs.push(MetadataKV {
                key: caps[1].to_string(),
                value: caps[2].trim().to_string(),
            });
        }
    }

    pairs
}

/// Extract skill ID from content.
///
/// Looks for explicit id field or derives from title.
#[must_use]
pub fn extract_skill_id(content: &str) -> Option<String> {
    // Look for explicit id field
    for kv in extract_metadata_kv(content) {
        if kv.key.to_lowercase() == "id" {
            return Some(kv.value);
        }
    }

    // Look for title/name and convert to ID
    for kv in extract_metadata_kv(content) {
        if kv.key.to_lowercase() == "name" || kv.key.to_lowercase() == "title" {
            return Some(slugify(&kv.value));
        }
    }

    // Look for markdown header
    for line in content.lines() {
        if let Some(title) = line.trim().strip_prefix('#') {
            let title = title.trim_start_matches('#').trim();
            if !title.is_empty() {
                return Some(slugify(title));
            }
        }
    }

    None
}

/// Convert text to a URL-safe slug.
#[must_use]
pub fn slugify(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c
            } else if c.is_whitespace() || c == '-' || c == '_' {
                '-'
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

// =============================================================================
// DOMAIN INFERENCE
// =============================================================================

/// Infer domain from content keywords.
#[must_use]
pub fn infer_domain(content: &str) -> Option<String> {
    let content_lower = content.to_lowercase();

    let domain_keywords: &[(&str, &[&str])] = &[
        (
            "programming",
            &["code", "function", "class", "variable", "api", "debug"],
        ),
        (
            "devops",
            &[
                "deploy",
                "ci/cd",
                "docker",
                "kubernetes",
                "pipeline",
                "terraform",
            ],
        ),
        (
            "security",
            &[
                "authentication",
                "authorization",
                "encrypt",
                "vulnerability",
                "oauth",
            ],
        ),
        (
            "testing",
            &[
                "test",
                "assert",
                "mock",
                "coverage",
                "unit test",
                "integration",
            ],
        ),
        (
            "frontend",
            &["react", "vue", "angular", "css", "html", "component", "ui"],
        ),
        (
            "backend",
            &[
                "server",
                "database",
                "rest",
                "graphql",
                "endpoint",
                "middleware",
            ],
        ),
        (
            "data",
            &["sql", "query", "database", "etl", "analytics", "pandas"],
        ),
    ];

    let mut best_domain = None;
    let mut best_score = 0usize;

    for (domain, keywords) in domain_keywords {
        let score = keywords
            .iter()
            .filter(|kw| content_lower.contains(*kw))
            .count();

        if score > best_score && score >= 2 {
            best_score = score;
            best_domain = Some(*domain);
        }
    }

    best_domain.map(String::from)
}

/// Infer tags from content.
#[must_use]
pub fn infer_tags(content: &str) -> Vec<String> {
    let content_lower = content.to_lowercase();
    let mut tags = Vec::new();

    // Language detection
    let languages = [
        ("rust", &["rust", "cargo", "crate"][..]),
        ("python", &["python", "pip", "pytest"]),
        ("javascript", &["javascript", "js", "node", "npm"]),
        ("typescript", &["typescript", "ts", "tsc"]),
        ("go", &["golang", "go mod"]),
        ("java", &["java", "maven", "gradle"]),
    ];

    for (tag, keywords) in languages {
        if keywords.iter().any(|kw| content_lower.contains(kw)) {
            tags.push(tag.to_string());
        }
    }

    // Topic detection
    let topics = [
        (
            "error-handling",
            &["error", "exception", "try", "catch"][..],
        ),
        ("async", &["async", "await", "promise", "future"]),
        ("testing", &["test", "assert", "mock"]),
        ("security", &["security", "auth", "encrypt"]),
        ("performance", &["performance", "optimize", "cache"]),
        ("logging", &["log", "trace", "debug"]),
    ];

    for (tag, keywords) in topics {
        if keywords.iter().any(|kw| content_lower.contains(kw)) {
            tags.push(tag.to_string());
        }
    }

    // Deduplicate
    tags.sort();
    tags.dedup();
    tags
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // Rule formatting tests

    #[test]
    fn test_normalize_to_imperative_you_should() {
        assert_eq!(
            normalize_to_imperative("You should always handle errors"),
            "Always handle errors"
        );
        assert_eq!(
            normalize_to_imperative("you should validate input"),
            "Validate input"
        );
    }

    #[test]
    fn test_normalize_to_imperative_make_sure() {
        assert_eq!(
            normalize_to_imperative("Make sure you test thoroughly"),
            "Test thoroughly"
        );
        assert_eq!(normalize_to_imperative("Make sure to validate"), "Validate");
    }

    #[test]
    fn test_normalize_to_imperative_remember() {
        assert_eq!(
            normalize_to_imperative("Remember to close connections"),
            "Close connections"
        );
    }

    #[test]
    fn test_normalize_to_imperative_already_imperative() {
        assert_eq!(
            normalize_to_imperative("Always use descriptive names"),
            "Always use descriptive names"
        );
    }

    #[test]
    fn test_format_rule_single_line() {
        assert_eq!(
            format_rule("You should always handle errors"),
            "Always handle errors"
        );
    }

    #[test]
    fn test_format_rule_multiline() {
        let input = "  Line one  \n  Line two  \n\n  Line three  ";
        let expected = "Line one\nLine two\nLine three";
        assert_eq!(format_rule(input), expected);
    }

    // Example formatting tests

    #[test]
    fn test_extract_code_blocks() {
        let content =
            "Some text\n```rust\nfn main() {}\n```\nMore text\n```python\nprint('hi')\n```";
        let blocks = extract_code_blocks(content);

        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].language, Some("rust".to_string()));
        assert_eq!(blocks[0].code, "fn main() {}");
        assert_eq!(blocks[1].language, Some("python".to_string()));
        assert_eq!(blocks[1].code, "print('hi')");
    }

    #[test]
    fn test_extract_code_blocks_no_language() {
        let content = "```\nsome code\n```";
        let blocks = extract_code_blocks(content);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].language, None);
        assert_eq!(blocks[0].code, "some code");
    }

    #[test]
    fn test_extract_description() {
        let content = "This is a description.\n```rust\ncode here\n```\nMore description.";
        let desc = extract_description(content);
        assert_eq!(desc, "This is a description.\nMore description.");
    }

    #[test]
    fn test_extract_example_title_header() {
        let content = "## Good Error Handling\n```rust\ncode\n```";
        assert_eq!(
            extract_example_title(content),
            Some("Good Error Handling".to_string())
        );
    }

    #[test]
    fn test_extract_example_title_prefix() {
        let content = "Example: Handling errors\n```rust\ncode\n```";
        assert_eq!(
            extract_example_title(content),
            Some("Handling errors".to_string())
        );
    }

    #[test]
    fn test_detect_language_rust() {
        assert_eq!(
            detect_language("fn main() -> Result<()> {}"),
            Some("rust".to_string())
        );
        assert_eq!(detect_language("let mut x = 5;"), Some("rust".to_string()));
        assert_eq!(
            detect_language("impl Foo for Bar {}"),
            Some("rust".to_string())
        );
    }

    #[test]
    fn test_detect_language_python() {
        assert_eq!(
            detect_language("def foo():\n    pass"),
            Some("python".to_string())
        );
        assert_eq!(
            detect_language("import os\nfrom sys import argv"),
            Some("python".to_string())
        );
    }

    #[test]
    fn test_detect_language_javascript() {
        assert_eq!(
            detect_language("const x = 5;"),
            Some("javascript".to_string())
        );
        assert_eq!(
            detect_language("const fn = () => {}"),
            Some("javascript".to_string())
        );
    }

    // Checklist formatting tests

    #[test]
    fn test_parse_checklist_checkboxes() {
        let content = "- [ ] Unchecked item\n- [x] Checked item\n- [X] Also checked";
        let items = parse_checklist(content);

        assert_eq!(items.len(), 3);
        assert_eq!(items[0].text, "Unchecked item");
        assert!(!items[0].checked);
        assert_eq!(items[1].text, "Checked item");
        assert!(items[1].checked);
        assert!(items[2].checked);
    }

    #[test]
    fn test_parse_checklist_numbered() {
        let content = "1. First step\n2. Second step\n3) Third step";
        let items = parse_checklist(content);

        assert_eq!(items.len(), 3);
        assert_eq!(items[0].text, "First step");
        assert_eq!(items[1].text, "Second step");
        assert_eq!(items[2].text, "Third step");
        assert!(!items[0].checked);
    }

    // Pitfall formatting tests

    #[test]
    fn test_format_pitfall_emoji() {
        assert_eq!(format_pitfall("⚠️ Don't use eval"), "Don't use eval");
        assert_eq!(
            format_pitfall("🚫 Never hardcode secrets"),
            "Never hardcode secrets"
        );
    }

    #[test]
    fn test_format_pitfall_prefix() {
        assert_eq!(
            format_pitfall("Warning: This is dangerous"),
            "This is dangerous"
        );
    }

    // Metadata tests

    #[test]
    fn test_extract_metadata_kv() {
        let content = "---\nid: my-skill\nname: My Skill\nversion: 1.0.0\n---";
        let kvs = extract_metadata_kv(content);

        assert_eq!(kvs.len(), 3);
        assert_eq!(kvs[0].key, "id");
        assert_eq!(kvs[0].value, "my-skill");
    }

    #[test]
    fn test_extract_skill_id_explicit() {
        let content = "id: explicit-id\nname: Some Name";
        assert_eq!(extract_skill_id(content), Some("explicit-id".to_string()));
    }

    #[test]
    fn test_extract_skill_id_from_name() {
        let content = "name: My Cool Skill";
        assert_eq!(extract_skill_id(content), Some("my-cool-skill".to_string()));
    }

    #[test]
    fn test_extract_skill_id_from_header() {
        let content = "# Error Handling Best Practices\n\nSome content here.";
        assert_eq!(
            extract_skill_id(content),
            Some("error-handling-best-practices".to_string())
        );
    }

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify("My--Cool--Skill"), "my-cool-skill");
        assert_eq!(slugify("Some_Special_Name!"), "some-special-name");
    }

    // Domain inference tests

    #[test]
    fn test_infer_domain() {
        assert_eq!(
            infer_domain("Always handle errors in your code function"),
            Some("programming".to_string())
        );
        assert_eq!(
            infer_domain("Deploy using docker and kubernetes"),
            Some("devops".to_string())
        );
        assert_eq!(
            infer_domain("Use proper authentication and authorization"),
            Some("security".to_string())
        );
    }

    #[test]
    fn test_infer_domain_insufficient() {
        // Only one keyword match - below threshold
        assert_eq!(infer_domain("Some random text with api"), None);
    }

    // Tag inference tests

    #[test]
    fn test_infer_tags() {
        let tags = infer_tags("Use rust and cargo for error handling with async/await");
        assert!(tags.contains(&"rust".to_string()));
        assert!(tags.contains(&"error-handling".to_string()));
        assert!(tags.contains(&"async".to_string()));
    }

    #[test]
    fn test_infer_tags_deduplication() {
        let tags = infer_tags("rust rust rust cargo crate");
        // Should only have one "rust" entry
        assert_eq!(tags.iter().filter(|t| *t == "rust").count(), 1);
    }
}
