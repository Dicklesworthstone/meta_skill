//! Skill template library for rapid authoring.

use crate::error::{MsError, Result};

#[derive(Debug, Clone)]
pub struct SkillTemplate {
    pub id: &'static str,
    pub name: &'static str,
    pub summary: &'static str,
    pub default_tags: &'static [&'static str],
    pub body: &'static str,
}

#[derive(Debug, Clone)]
pub struct TemplateContext {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
}

impl TemplateContext {
    fn description_yaml(&self) -> String {
        indent_lines(&self.description, "  ")
    }

    fn tags_yaml(&self, defaults: &[&str]) -> String {
        let mut tags = if self.tags.is_empty() {
            defaults.iter().map(std::string::ToString::to_string).collect::<Vec<_>>()
        } else {
            self.tags.clone()
        };
        tags = tags
            .into_iter()
            .map(|tag| tag.trim().to_string())
            .filter(|tag| !tag.is_empty())
            .collect::<Vec<_>>();
        if tags.is_empty() {
            tags.push("general".to_string());
        }
        tags.sort();
        tags.dedup();
        tags.iter()
            .map(|tag| format!("  - {}", yaml_scalar(tag)))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[must_use] 
pub fn list_templates() -> &'static [SkillTemplate] {
    TEMPLATES
}

#[must_use] 
pub fn find_template(id: &str) -> Option<&'static SkillTemplate> {
    TEMPLATES
        .iter()
        .find(|template| template.id == id || template.name.eq_ignore_ascii_case(id))
}

pub fn render_template(template: &SkillTemplate, ctx: &TemplateContext) -> Result<String> {
    if ctx.id.trim().is_empty() {
        return Err(MsError::ValidationFailed("template id is required".to_string()));
    }
    if ctx.name.trim().is_empty() {
        return Err(MsError::ValidationFailed(
            "template name is required".to_string(),
        ));
    }
    if ctx.description.trim().is_empty() {
        return Err(MsError::ValidationFailed(
            "template description is required".to_string(),
        ));
    }

    let rendered = template
        .body
        .replace("{{id}}", ctx.id.trim())
        .replace("{{name}}", ctx.name.trim())
        .replace("{{description}}", ctx.description.trim())
        .replace("{{description_yaml}}", &ctx.description_yaml())
        .replace("{{tags_yaml}}", &ctx.tags_yaml(template.default_tags));

    Ok(rendered)
}

fn indent_lines(input: &str, indent: &str) -> String {
    input
        .lines()
        .map(|line| format!("{indent}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn yaml_scalar(value: &str) -> String {
    let simple = value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.');
    if simple {
        value.to_string()
    } else {
        format!("\"{}\"", value.replace('"', "\\\""))
    }
}

const DEBUGGING_TEMPLATE: &str = r#"---
id: {{id}}
name: {{name}}
description: >-
{{description_yaml}}
tags:
{{tags_yaml}}
version: "0.1.0"
---

# {{name}}

{{description}}

## Quick start
- Confirm the failing behavior and expected outcome.
- Capture the minimal repro and exact error output.
- Identify the last known good state (commit, deploy, config).

## Diagnosis
- Gather signals: logs, stack traces, metrics, recent diffs.
- Narrow scope with binary search or feature flags.
- Validate invariants before changing behavior.

## Fix plan
1. Explain the root cause in one sentence.
2. Apply the smallest safe change.
3. Add a regression guard (test, assertion, or monitor).

## Verification
- Run the minimal repro scenario.
- Run impacted test suites or smoke checks.
- Confirm no new warnings or lints.

## Pitfalls
- Avoid masking the symptom without fixing the cause.
- Don’t ship a fix without a reproducible validation step.
"#;

const REFACTOR_TEMPLATE: &str = r#"---
id: {{id}}
name: {{name}}
description: >-
{{description_yaml}}
tags:
{{tags_yaml}}
version: "0.1.0"
---

# {{name}}

{{description}}

## Intent
- What behavior must remain identical?
- Which modules or contracts are in scope?

## Constraints
- No API changes without updating callers.
- Keep diffs minimal and reversible.

## Approach
1. Add characterization tests if behavior is unclear.
2. Extract seams (interfaces, pure functions).
3. Replace implementation behind the seams.
4. Remove dead code only after validation.

## Verification
- Run focused tests covering the refactored surface.
- Compare outputs or snapshots before/after.

## Notes
- Document any new assumptions or invariants.
"#;

const UI_POLISH_TEMPLATE: &str = r#"---
id: {{id}}
name: {{name}}
description: >-
{{description_yaml}}
tags:
{{tags_yaml}}
version: "0.1.0"
---

# {{name}}

{{description}}

## Goals
- Clarify hierarchy and scan paths.
- Reduce visual noise while preserving intent.

## Design moves
- Align typography scale with content priority.
- Use consistent spacing tokens across sections.
- Introduce contrast only where it signals importance.

## Interaction
- Ensure states (hover/focus/disabled) are distinct.
- Keep motion purposeful and minimal.

## Verification
- Check layout at mobile and desktop breakpoints.
- Validate color contrast for key text and controls.
"#;

static TEMPLATES: &[SkillTemplate] = &[
    SkillTemplate {
        id: "debugging",
        name: "Debugging",
        summary: "Template for diagnostic and root-cause workflows.",
        default_tags: &["debugging", "diagnostics", "reliability"],
        body: DEBUGGING_TEMPLATE,
    },
    SkillTemplate {
        id: "refactor",
        name: "Refactor",
        summary: "Template for safe, behavior-preserving refactors.",
        default_tags: &["refactor", "maintenance", "architecture"],
        body: REFACTOR_TEMPLATE,
    },
    SkillTemplate {
        id: "ui-polish",
        name: "UI Polish",
        summary: "Template for UI refinement and visual hierarchy.",
        default_tags: &["ui", "design", "frontend"],
        body: UI_POLISH_TEMPLATE,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_template_replaces_placeholders() {
        let template = &SkillTemplate {
            id: "test",
            name: "Test",
            summary: "Summary",
            default_tags: &["one"],
            body: "id={{id}}\nname={{name}}\n{{description}}\n{{description_yaml}}\n{{tags_yaml}}",
        };
        let ctx = TemplateContext {
            id: "demo-id".to_string(),
            name: "Demo Name".to_string(),
            description: "Line one\nLine two".to_string(),
            tags: vec!["alpha".to_string(), "beta".to_string()],
        };

        let rendered = render_template(template, &ctx).unwrap();
        assert!(rendered.contains("id=demo-id"));
        assert!(rendered.contains("name=Demo Name"));
        assert!(rendered.contains("Line one"));
        assert!(rendered.contains("  - alpha"));
        assert!(rendered.contains("  - beta"));
    }
}
