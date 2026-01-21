//! Context-aware error suggestions.
//!
//! This module provides dynamic suggestion generation based on error context,
//! complementing the static suggestions in the `codes` module.

use serde_json::Value;

use super::codes::ErrorCode;

/// Generate a context-aware suggestion for an error.
///
/// This function uses contextual information to provide more specific
/// suggestions than the default static ones.
///
/// # Arguments
/// * `code` - The error code
/// * `context` - Optional JSON context with additional error details
///
/// # Returns
/// A suggestion string tailored to the specific error context
pub fn suggest_for_error(code: ErrorCode, context: Option<&Value>) -> String {
    match code {
        ErrorCode::SkillNotFound => suggest_skill_not_found(context),
        ErrorCode::SkillInvalid => suggest_skill_invalid(context),
        ErrorCode::SkillParseError => suggest_skill_parse_error(context),
        ErrorCode::SkillDependencyMissing => suggest_skill_dependency_missing(context),
        ErrorCode::SkillCyclicDependency => suggest_skill_cyclic_dependency(context),
        ErrorCode::SkillParentNotFound => suggest_skill_parent_not_found(context),
        ErrorCode::IndexEmpty => suggest_index_empty(context),
        ErrorCode::ConfigMissingRequired => suggest_config_missing_required(context),
        ErrorCode::SearchNoResults => suggest_search_no_results(context),
        ErrorCode::ValidationFailed => suggest_validation_failed(context),
        // Fall back to static suggestion for other codes
        _ => code.suggestion().to_string(),
    }
}

fn suggest_skill_not_found(context: Option<&Value>) -> String {
    let skill_id = context
        .and_then(|c| c.get("skill_id"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");

    if skill_id == "unknown" {
        return ErrorCode::SkillNotFound.suggestion().to_string();
    }

    format!(
        "Skill '{}' not found. Try:\n  - `ms search {}` to find similar skills\n  - `ms list` to see all available skills\n  - `ms index` if you recently added new skills",
        skill_id, skill_id
    )
}

fn suggest_skill_invalid(context: Option<&Value>) -> String {
    let skill_id = context
        .and_then(|c| c.get("skill_id"))
        .and_then(Value::as_str);
    let reason = context
        .and_then(|c| c.get("reason"))
        .and_then(Value::as_str);

    match (skill_id, reason) {
        (Some(id), Some(reason)) => format!(
            "Skill '{}' is invalid: {}\nRun `ms validate {}` for detailed diagnostics",
            id, reason, id
        ),
        (Some(id), None) => format!(
            "Skill '{}' has invalid format. Run `ms validate {}` for details",
            id, id
        ),
        _ => ErrorCode::SkillInvalid.suggestion().to_string(),
    }
}

fn suggest_skill_parse_error(context: Option<&Value>) -> String {
    let file_path = context
        .and_then(|c| c.get("file_path"))
        .and_then(Value::as_str);
    let line = context.and_then(|c| c.get("line")).and_then(Value::as_u64);

    match (file_path, line) {
        (Some(path), Some(line)) => format!(
            "Parse error at {}:{}\nCheck the syntax around line {}. Run `ms template show` for format examples",
            path, line, line
        ),
        (Some(path), None) => format!(
            "Parse error in {}\nRun `ms validate {}` for specific issues",
            path, path
        ),
        _ => ErrorCode::SkillParseError.suggestion().to_string(),
    }
}

fn suggest_skill_dependency_missing(context: Option<&Value>) -> String {
    let missing_dep = context
        .and_then(|c| c.get("missing_dependency"))
        .and_then(Value::as_str);
    let skill_id = context
        .and_then(|c| c.get("skill_id"))
        .and_then(Value::as_str);

    match (skill_id, missing_dep) {
        (Some(skill), Some(dep)) => format!(
            "Skill '{}' depends on '{}' which doesn't exist.\nEither:\n  - Install the dependency: `ms bundle install` or create it manually\n  - Remove the dependency from '{}'",
            skill, dep, skill
        ),
        (_, Some(dep)) => format!(
            "Missing dependency '{}'. Create it with `ms template apply` or install from a bundle",
            dep
        ),
        _ => ErrorCode::SkillDependencyMissing.suggestion().to_string(),
    }
}

fn suggest_skill_cyclic_dependency(context: Option<&Value>) -> String {
    let cycle = context
        .and_then(|c| c.get("cycle"))
        .and_then(Value::as_array);

    match cycle {
        Some(chain) if !chain.is_empty() => {
            let chain_str: Vec<_> = chain.iter().filter_map(Value::as_str).collect();
            format!(
                "Circular dependency detected: {}\nBreak the cycle by removing one of the `extends` or `includes` references",
                chain_str.join(" -> ")
            )
        }
        _ => ErrorCode::SkillCyclicDependency.suggestion().to_string(),
    }
}

fn suggest_skill_parent_not_found(context: Option<&Value>) -> String {
    let parent_id = context
        .and_then(|c| c.get("parent_id"))
        .and_then(Value::as_str);
    let child_id = context
        .and_then(|c| c.get("child_id"))
        .and_then(Value::as_str);

    match (parent_id, child_id) {
        (Some(parent), Some(child)) => format!(
            "Skill '{}' extends '{}' which doesn't exist.\nEither:\n  - Create the parent skill first\n  - Check for typos in the `extends` field of '{}'",
            child, parent, child
        ),
        (Some(parent), None) => format!(
            "Parent skill '{}' not found. Create it or check for typos",
            parent
        ),
        _ => ErrorCode::SkillParentNotFound.suggestion().to_string(),
    }
}

fn suggest_index_empty(context: Option<&Value>) -> String {
    let skill_paths = context
        .and_then(|c| c.get("skill_paths"))
        .and_then(Value::as_array);

    match skill_paths {
        Some(paths) if !paths.is_empty() => {
            let paths_str: Vec<_> = paths
                .iter()
                .filter_map(Value::as_str)
                .take(3)
                .collect();
            format!(
                "No skills indexed. Configured paths: [{}]\nRun `ms index` to index skills from these paths",
                paths_str.join(", ")
            )
        }
        _ => "No skills indexed. Configure skill paths with `ms config skill_paths.project '[\"./skills\"]'` then run `ms index`".to_string(),
    }
}

fn suggest_config_missing_required(context: Option<&Value>) -> String {
    let config_key = context
        .and_then(|c| c.get("config_key"))
        .and_then(Value::as_str);

    match config_key {
        Some(key) => format!(
            "Required config '{}' is missing. Set it with:\n  `ms config {} <value>`",
            key, key
        ),
        None => ErrorCode::ConfigMissingRequired.suggestion().to_string(),
    }
}

fn suggest_search_no_results(context: Option<&Value>) -> String {
    let query = context.and_then(|c| c.get("query")).and_then(Value::as_str);

    match query {
        Some(q) if !q.is_empty() => format!(
            "No results for '{}'. Try:\n  - Broader search terms\n  - `ms list` to see all skills\n  - `ms index` if skills were recently added",
            q
        ),
        _ => ErrorCode::SearchNoResults.suggestion().to_string(),
    }
}

fn suggest_validation_failed(context: Option<&Value>) -> String {
    let errors = context
        .and_then(|c| c.get("errors"))
        .and_then(Value::as_array);
    let skill_id = context
        .and_then(|c| c.get("skill_id"))
        .and_then(Value::as_str);

    match (skill_id, errors) {
        (Some(id), Some(errs)) if !errs.is_empty() => {
            let error_count = errs.len();
            format!(
                "Skill '{}' has {} validation error(s). Run `ms validate {}` to see details and fix each issue",
                id, error_count, id
            )
        }
        (Some(id), _) => format!(
            "Validation failed for '{}'. Run `ms validate {}` for details",
            id, id
        ),
        _ => ErrorCode::ValidationFailed.suggestion().to_string(),
    }
}

/// Get suggestions for similar skills based on a misspelled skill ID.
///
/// This is a helper for `SkillNotFound` errors that can suggest
/// similar skill names using fuzzy matching.
pub fn suggest_similar_skills(
    query: &str,
    available: &[&str],
    max_suggestions: usize,
) -> Vec<String> {
    let query_lower = query.to_lowercase();
    let mut scored: Vec<_> = available
        .iter()
        .map(|s| (s, similarity_score(&query_lower, &s.to_lowercase())))
        .filter(|(_, score)| *score > 0.3)
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored
        .into_iter()
        .take(max_suggestions)
        .map(|(s, _)| (*s).to_string())
        .collect()
}

/// Simple similarity score between two strings (Jaccard-like on character trigrams).
fn similarity_score(a: &str, b: &str) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    let a_trigrams: std::collections::HashSet<_> = trigrams(a).collect();
    let b_trigrams: std::collections::HashSet<_> = trigrams(b).collect();

    if a_trigrams.is_empty() || b_trigrams.is_empty() {
        // Fall back to simple prefix/substring check for short strings
        if a.starts_with(b) || b.starts_with(a) {
            return 0.8;
        }
        if a.contains(b) || b.contains(a) {
            return 0.5;
        }
        return 0.0;
    }

    let intersection = a_trigrams.intersection(&b_trigrams).count();
    let union = a_trigrams.union(&b_trigrams).count();

    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

/// Generate character trigrams from a string.
fn trigrams(s: &str) -> impl Iterator<Item = &str> {
    (0..s.len().saturating_sub(2)).filter_map(move |i| s.get(i..i + 3))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_suggest_skill_not_found_with_context() {
        let context = json!({ "skill_id": "rust-errors" });
        let suggestion = suggest_for_error(ErrorCode::SkillNotFound, Some(&context));
        assert!(suggestion.contains("rust-errors"));
        assert!(suggestion.contains("ms search"));
    }

    #[test]
    fn test_suggest_skill_not_found_without_context() {
        let suggestion = suggest_for_error(ErrorCode::SkillNotFound, None);
        assert!(!suggestion.is_empty());
    }

    #[test]
    fn test_suggest_cyclic_dependency_with_cycle() {
        let context = json!({ "cycle": ["skill-a", "skill-b", "skill-c", "skill-a"] });
        let suggestion = suggest_for_error(ErrorCode::SkillCyclicDependency, Some(&context));
        assert!(suggestion.contains("skill-a -> skill-b -> skill-c -> skill-a"));
    }

    #[test]
    fn test_suggest_validation_failed_with_errors() {
        let context = json!({
            "skill_id": "my-skill",
            "errors": ["missing title", "invalid format"]
        });
        let suggestion = suggest_for_error(ErrorCode::ValidationFailed, Some(&context));
        assert!(suggestion.contains("my-skill"));
        assert!(suggestion.contains("2 validation error"));
    }

    #[test]
    fn test_suggest_similar_skills() {
        let available = vec![
            "rust-error-handling",
            "rust-async",
            "python-errors",
            "go-concurrency",
        ];
        let suggestions = suggest_similar_skills("rust-error", &available, 3);
        // Should suggest rust-error-handling as most similar
        assert!(!suggestions.is_empty());
        assert!(suggestions.iter().any(|s| s.contains("rust")));
    }

    #[test]
    fn test_similarity_score() {
        assert!(similarity_score("rust-errors", "rust-error-handling") > 0.3);
        assert!(similarity_score("abc", "xyz") < 0.1);
        assert!(similarity_score("test", "test") > 0.9);
    }

    #[test]
    fn test_trigrams() {
        let tris: Vec<_> = trigrams("hello").collect();
        assert_eq!(tris, vec!["hel", "ell", "llo"]);
    }

    #[test]
    fn test_fallback_to_static_suggestion() {
        // For codes without special handling, should return static suggestion
        let suggestion = suggest_for_error(ErrorCode::NetworkTimeout, None);
        assert_eq!(suggestion, ErrorCode::NetworkTimeout.suggestion());
    }

    #[test]
    fn test_search_no_results_with_query() {
        let context = json!({ "query": "nonexistent-pattern" });
        let suggestion = suggest_for_error(ErrorCode::SearchNoResults, Some(&context));
        assert!(suggestion.contains("nonexistent-pattern"));
        assert!(suggestion.contains("ms list"));
    }

    #[test]
    fn test_config_missing_with_key() {
        let context = json!({ "config_key": "skill_paths.project" });
        let suggestion = suggest_for_error(ErrorCode::ConfigMissingRequired, Some(&context));
        assert!(suggestion.contains("skill_paths.project"));
        assert!(suggestion.contains("ms config"));
    }
}
