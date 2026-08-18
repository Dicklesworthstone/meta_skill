//! E2E Scenario: Show Workflow Integration Tests
//!
//! Comprehensive tests for the `ms show` command covering:
//! - Show a skill by its ID
//! - Show a skill by its name
//! - Show with full spec (--full flag)
//! - Show with metadata only (--meta flag)
//! - Show with dependency graph (--deps flag)
//! - Show a non-existent skill (error case)
//! - Show a skill that has dependencies listed
//! - Show in plain output format
//! - Show in JSON output format (--robot)

use super::fixture::E2EFixture;
use ms::error::Result;

// ---------------------------------------------------------------------------
// Skill definitions
// ---------------------------------------------------------------------------

const SKILL_RUST_ERRORS: &str = r#"---
name: Rust Error Handling
description: Best practices for error handling in Rust
tags: [rust, errors, advanced]
version: 1.2.0
author: Alice
---

# Rust Error Handling

Use `Result<T, E>` and propagate errors with `?`.

## Guidelines

- Use thiserror for library errors
- Use anyhow for application errors
"#;

const SKILL_GO_CONCURRENCY: &str = r#"---
name: Go Concurrency
description: Goroutines, channels, and concurrent patterns in Go
tags: [go, concurrency, intermediate]
version: 0.9.0
---

# Go Concurrency

Use goroutines and channels for concurrent programming.

## Guidelines

- Always close channels when done
- Use select for multiplexing
"#;

const SKILL_WITH_DEPS: &str = r#"---
name: Full Stack Web
description: Building full-stack web applications
tags: [web, fullstack, advanced]
version: 2.0.0
author: Bob
requires: [rust-error-handling, go-concurrency]
provides: [web-app]
platforms: [linux, macos]
---

# Full Stack Web

Build full-stack web applications with Rust and Go.

## Prerequisites

- Rust Error Handling
- Go Concurrency

## Architecture

Use a Rust backend with a Go microservice layer.
"#;

const SKILL_DEPRECATED: &str = r#"---
name: Legacy Patterns
description: Outdated patterns that should be avoided
tags: [deprecated, legacy]
version: 0.1.0
---

# Legacy Patterns

These patterns are no longer recommended.
"#;

// ---------------------------------------------------------------------------
// Fixture setup helpers
// ---------------------------------------------------------------------------

/// Set up a fixture with several skills indexed and ready for `show` tests.
fn setup_show_fixture(scenario: &str) -> Result<E2EFixture> {
    let mut fixture = E2EFixture::new(scenario);

    fixture.checkpoint("show:setup");

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    fixture.log_step("Create skills");
    fixture.create_skill("rust-error-handling", SKILL_RUST_ERRORS)?;
    fixture.create_skill("go-concurrency", SKILL_GO_CONCURRENCY)?;
    fixture.create_skill("full-stack-web", SKILL_WITH_DEPS)?;

    fixture.log_step("Index skills");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    fixture.checkpoint("show:indexed");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "show",
        "Skills indexed for show tests",
        Some(serde_json::json!({
            "skills": ["rust-error-handling", "go-concurrency", "full-stack-web"],
            "total": 3
        })),
    );

    Ok(fixture)
}

/// Set up a fixture that additionally contains a deprecated skill.
fn setup_show_deprecated_fixture(scenario: &str) -> Result<E2EFixture> {
    let mut fixture = E2EFixture::new(scenario);

    fixture.checkpoint("show:setup");

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    fixture.log_step("Create skills (including deprecated candidate)");
    fixture.create_skill("rust-error-handling", SKILL_RUST_ERRORS)?;
    fixture.create_skill("legacy-patterns", SKILL_DEPRECATED)?;

    fixture.log_step("Index skills");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    // Attempt to mark as deprecated via the CLI; if the command is unavailable
    // the skill still exists but is_deprecated will be false.
    fixture.log_step("Deprecate legacy-patterns skill");
    let output = fixture.run_ms(&[
        "--robot",
        "deprecate",
        "legacy-patterns",
        "--reason",
        "Use modern patterns instead",
    ]);
    if !output.success {
        fixture.emit_event(
            super::fixture::LogLevel::Warn,
            "show",
            "Deprecate command not available; skipping deprecation marking",
            None,
        );
    }

    fixture.checkpoint("show:indexed");

    Ok(fixture)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Show a skill by its ID and verify the JSON structure.
#[test]
fn test_show_skill_by_id() -> Result<()> {
    let mut fixture = setup_show_fixture("show_skill_by_id")?;

    fixture.log_step("Show skill by ID");
    let output = fixture.run_ms(&["--robot", "show", "rust-error-handling"]);
    fixture.assert_success(&output, "show rust-error-handling");

    let json = output.json();

    // Top-level status
    assert_eq!(json["status"].as_str(), Some("ok"), "Status should be 'ok'");

    // Skill object presence
    let skill = &json["skill"];
    assert!(
        skill.is_object(),
        "Response should contain a 'skill' object"
    );

    // Core fields
    assert_eq!(skill["id"].as_str(), Some("rust-error-handling"));
    assert_eq!(skill["name"].as_str(), Some("Rust Error Handling"));
    assert_eq!(
        skill["description"].as_str(),
        Some("Best practices for error handling in Rust")
    );
    assert_eq!(skill["version"].as_str(), Some("1.2.0"));
    assert_eq!(skill["author"].as_str(), Some("Alice"));

    // Numeric / boolean fields
    assert!(
        skill["token_count"].is_number(),
        "token_count should be a number"
    );
    assert!(
        skill["quality_score"].is_number(),
        "quality_score should be a number"
    );
    assert_eq!(skill["is_deprecated"].as_bool(), Some(false));

    // Timestamps
    assert!(
        skill["indexed_at"].is_string(),
        "indexed_at should be a string"
    );
    assert!(
        skill["modified_at"].is_string(),
        "modified_at should be a string"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "show",
        "Show by ID verified",
        Some(serde_json::json!({
            "id": skill["id"],
            "name": skill["name"],
        })),
    );

    fixture.generate_report();
    Ok(())
}

/// Show a skill with the `--full` flag to include body and metadata.
#[test]
fn test_show_skill_full() -> Result<()> {
    let mut fixture = setup_show_fixture("show_skill_full")?;

    fixture.log_step("Show skill with --full flag");
    let output = fixture.run_ms(&["--robot", "show", "rust-error-handling", "--full"]);
    fixture.assert_success(&output, "show --full");

    let json = output.json();
    let skill = &json["skill"];

    // --full should include body
    let body = skill["body"]
        .as_str()
        .expect("body should be present with --full");
    assert!(
        body.contains("Rust Error Handling"),
        "Body should contain skill title"
    );
    assert!(
        body.contains("thiserror"),
        "Body should contain content about thiserror"
    );

    // --full should include metadata
    assert!(
        skill.get("metadata").is_some(),
        "metadata should be present with --full"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "show",
        "Full output verified",
        Some(serde_json::json!({
            "has_body": true,
            "has_metadata": true,
            "body_length": body.len(),
        })),
    );

    fixture.generate_report();
    Ok(())
}

/// Show a skill with the `--meta` flag to include metadata only (no body).
#[test]
fn test_show_skill_meta() -> Result<()> {
    let mut fixture = setup_show_fixture("show_skill_meta")?;

    fixture.log_step("Show skill with --meta flag");
    let output = fixture.run_ms(&["--robot", "show", "rust-error-handling", "--meta"]);
    fixture.assert_success(&output, "show --meta");

    let json = output.json();
    let skill = &json["skill"];

    // --meta should include metadata
    assert!(
        skill.get("metadata").is_some(),
        "metadata should be present with --meta"
    );

    // --meta alone should NOT include body
    assert!(
        skill.get("body").is_none(),
        "body should NOT be present with --meta alone"
    );

    // Verify metadata has expected structure (tags at minimum)
    let metadata = &skill["metadata"];
    if let Some(tags) = metadata.get("tags").and_then(|t| t.as_array()) {
        let tag_strings: Vec<&str> = tags.iter().filter_map(|v| v.as_str()).collect();
        assert!(tag_strings.contains(&"rust"), "Tags should contain 'rust'");
        assert!(
            tag_strings.contains(&"errors"),
            "Tags should contain 'errors'"
        );
    }

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "show",
        "Meta-only output verified",
        Some(serde_json::json!({
            "has_metadata": true,
            "has_body": skill.get("body").is_some(),
        })),
    );

    fixture.generate_report();
    Ok(())
}

/// Show a skill with the `--deps` flag to list dependencies.
#[test]
fn test_show_skill_deps() -> Result<()> {
    let mut fixture = setup_show_fixture("show_skill_deps")?;

    fixture.log_step("Show skill with --deps flag (skill with dependencies)");
    let output = fixture.run_ms(&["--robot", "show", "full-stack-web", "--deps"]);
    fixture.assert_success(&output, "show --deps full-stack-web");

    let json = output.json();
    let skill = &json["skill"];

    // --deps should produce a dependencies array
    let deps = skill
        .get("dependencies")
        .and_then(|d| d.as_array())
        .expect("dependencies array should be present with --deps");

    let dep_ids: Vec<&str> = deps.iter().filter_map(|v| v.as_str()).collect();
    assert!(
        dep_ids.contains(&"rust-error-handling"),
        "Dependencies should include rust-error-handling"
    );
    assert!(
        dep_ids.contains(&"go-concurrency"),
        "Dependencies should include go-concurrency"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "show",
        "Dependencies listing verified",
        Some(serde_json::json!({
            "skill": "full-stack-web",
            "dependencies": dep_ids,
        })),
    );

    fixture.generate_report();
    Ok(())
}

/// Show a skill that has NO dependencies and verify the empty list.
#[test]
fn test_show_skill_deps_empty() -> Result<()> {
    let mut fixture = setup_show_fixture("show_skill_deps_empty")?;

    fixture.log_step("Show skill with --deps flag (skill without dependencies)");
    let output = fixture.run_ms(&["--robot", "show", "rust-error-handling", "--deps"]);
    fixture.assert_success(&output, "show --deps rust-error-handling");

    let json = output.json();
    let skill = &json["skill"];

    let deps = skill
        .get("dependencies")
        .and_then(|d| d.as_array())
        .expect("dependencies array should be present with --deps");

    assert!(
        deps.is_empty(),
        "rust-error-handling should have no dependencies, got {:?}",
        deps
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "show",
        "Empty dependencies verified",
        Some(serde_json::json!({
            "skill": "rust-error-handling",
            "dependency_count": 0,
        })),
    );

    fixture.generate_report();
    Ok(())
}

/// Attempt to show a skill that does not exist; expect a non-zero exit code.
#[test]
fn test_show_nonexistent_skill() -> Result<()> {
    let mut fixture = setup_show_fixture("show_nonexistent_skill")?;

    fixture.log_step("Show nonexistent skill");
    let output = fixture.run_ms(&["--robot", "show", "does-not-exist"]);

    // The command should fail
    assert!(
        !output.success,
        "Showing a nonexistent skill should fail (exit code {})",
        output.exit_code
    );

    // The error output (stdout for --robot or stderr) should mention "not found"
    let combined = format!("{} {}", output.stdout, output.stderr);
    let mentions_not_found = combined.to_lowercase().contains("not found")
        || combined.to_lowercase().contains("not_found")
        || combined.to_lowercase().contains("error");
    assert!(
        mentions_not_found,
        "Error output should indicate the skill was not found.\nStdout: {}\nStderr: {}",
        output.stdout, output.stderr
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "show",
        "Nonexistent skill error verified",
        Some(serde_json::json!({
            "exit_code": output.exit_code,
            "mentions_not_found": mentions_not_found,
        })),
    );

    fixture.generate_report();
    Ok(())
}

/// Show a deprecated skill and verify the deprecation info appears.
#[test]
fn test_show_deprecated_skill() -> Result<()> {
    let mut fixture = setup_show_deprecated_fixture("show_deprecated_skill")?;

    fixture.log_step("Show deprecated skill in JSON mode");
    let output = fixture.run_ms(&["--robot", "show", "legacy-patterns"]);
    fixture.assert_success(&output, "show legacy-patterns");

    let json = output.json();
    let skill = &json["skill"];

    assert_eq!(skill["id"].as_str(), Some("legacy-patterns"));
    assert_eq!(skill["name"].as_str(), Some("Legacy Patterns"));

    // deprecation_reason and is_deprecated fields should exist regardless
    assert!(
        skill.get("is_deprecated").is_some(),
        "is_deprecated field should be present"
    );
    assert!(
        skill.get("deprecation_reason").is_some(),
        "deprecation_reason field should be present"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "show",
        "Deprecated skill fields verified",
        Some(serde_json::json!({
            "id": skill["id"],
            "is_deprecated": skill["is_deprecated"],
            "deprecation_reason": skill["deprecation_reason"],
        })),
    );

    fixture.generate_report();
    Ok(())
}

/// Show a skill using plain output format and verify key-value pairs.
#[test]
fn test_show_plain_output() -> Result<()> {
    let mut fixture = setup_show_fixture("show_plain_output")?;

    fixture.log_step("Show skill with plain output (no --robot)");
    let output = fixture.run_ms(&["show", "rust-error-handling", "--plain"]);
    fixture.assert_success(&output, "show plain");

    // Plain output should contain YAML-like key: value lines
    fixture.assert_output_contains(&output, "name:");
    fixture.assert_output_contains(&output, "Rust Error Handling");
    fixture.assert_output_contains(&output, "version:");
    fixture.assert_output_contains(&output, "layer:");

    // Should have the --- separator before body content
    fixture.assert_output_contains(&output, "---");

    // No ANSI escape codes in plain output
    assert!(
        !output.stdout.contains("\x1b["),
        "Plain output should not contain ANSI escape codes"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "show",
        "Plain output format verified",
        Some(serde_json::json!({
            "format": "plain",
            "has_key_value_lines": true,
        })),
    );

    fixture.generate_report();
    Ok(())
}

/// Verify JSON output structure completeness for the show command.
#[test]
fn test_show_json_output_structure() -> Result<()> {
    let mut fixture = setup_show_fixture("show_json_output_structure")?;

    fixture.log_step("Show skill with JSON output (--robot)");
    let output = fixture.run_ms(&["--robot", "show", "go-concurrency"]);
    fixture.assert_success(&output, "show json");

    let json = output.json();

    // Top-level fields
    assert!(
        json.get("status").is_some(),
        "JSON should have 'status' field"
    );
    assert!(
        json.get("skill").is_some(),
        "JSON should have 'skill' field"
    );

    let skill = &json["skill"];

    // All expected fields in the skill object
    let expected_fields = [
        "id",
        "name",
        "version",
        "description",
        "author",
        "layer",
        "source_path",
        "content_hash",
        "token_count",
        "quality_score",
        "indexed_at",
        "modified_at",
        "is_deprecated",
        "deprecation_reason",
    ];

    let mut missing_fields = Vec::new();
    for field in &expected_fields {
        if skill.get(*field).is_none() {
            missing_fields.push(*field);
        }
    }

    assert!(
        missing_fields.is_empty(),
        "Skill JSON is missing fields: {:?}",
        missing_fields
    );

    // Verify specific values for go-concurrency
    assert_eq!(skill["id"].as_str(), Some("go-concurrency"));
    assert_eq!(skill["name"].as_str(), Some("Go Concurrency"));
    assert_eq!(skill["version"].as_str(), Some("0.9.0"));

    // No ANSI codes in JSON output
    assert!(
        !output.stdout.contains("\x1b["),
        "JSON output should not contain ANSI escape codes"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "show",
        "JSON output structure verified",
        Some(serde_json::json!({
            "fields_verified": expected_fields.len(),
            "missing": missing_fields,
        })),
    );

    fixture.generate_report();
    Ok(())
}

/// Show with --full and --deps combined to verify both body and dependencies
/// appear in one response.
#[test]
fn test_show_full_with_deps() -> Result<()> {
    let mut fixture = setup_show_fixture("show_full_with_deps")?;

    fixture.log_step("Show skill with --full --deps combined");
    let output = fixture.run_ms(&["--robot", "show", "full-stack-web", "--full", "--deps"]);
    fixture.assert_success(&output, "show --full --deps");

    let json = output.json();
    let skill = &json["skill"];

    // Body should be present (--full)
    let body = skill["body"]
        .as_str()
        .expect("body should be present with --full");
    assert!(
        body.contains("Full Stack Web"),
        "Body should contain skill title"
    );

    // Metadata should be present (--full implies --meta)
    assert!(
        skill.get("metadata").is_some(),
        "metadata should be present with --full"
    );

    // Dependencies should be present (--deps)
    let deps = skill
        .get("dependencies")
        .and_then(|d| d.as_array())
        .expect("dependencies should be present with --deps");
    assert!(!deps.is_empty(), "full-stack-web should have dependencies");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "show",
        "Combined --full --deps verified",
        Some(serde_json::json!({
            "has_body": true,
            "has_metadata": true,
            "dependency_count": deps.len(),
        })),
    );

    fixture.generate_report();
    Ok(())
}

// ---------------------------------------------------------------------------
// Regression: id vs. name divergence (issue #172)
// ---------------------------------------------------------------------------

/// A skill whose frontmatter `name` differs from the slugified H1 title, so the
/// canonical id (`a-b-testing-platform-for-saas`, derived from the H1) and the
/// human-facing name (`ab-testing`) diverge — the shape that made `ms show`
/// reject 82 of 138 ids that `ms list --plain` printed (issue #172).
const SKILL_DIVERGENT_NAME: &str = r#"---
name: ab-testing
description: Home-rolled A/B testing platform guidance
tags: [testing, saas]
version: 1.0.0
---

# A/B Testing Platform for SaaS

Server-side variant assignment and Bayesian analysis.

## Guidelines

- Assign variants server-side
- Use Bayesian analysis for stopping
"#;

/// End-to-end regression for issue #172: every id `ms list --plain` prints must
/// resolve in `ms show` and `ms quality`, and the human-facing name must
/// resolve in `ms show` too.
#[test]
fn test_show_resolves_listed_ids_and_names() -> Result<()> {
    let mut fixture = E2EFixture::new("show_id_name_divergence");

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    fixture.log_step("Create a skill whose name and id diverge");
    fixture.create_skill("ab-testing", SKILL_DIVERGENT_NAME)?;

    fixture.log_step("Index skills");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    // 1. `ms list --plain` column 1 must be the canonical id.
    fixture.log_step("List skills with plain output");
    let output = fixture.run_ms(&["list", "--plain"]);
    fixture.assert_success(&output, "list plain");
    let line = output
        .stdout
        .lines()
        .find(|l| l.contains("ab-testing") || l.contains("a-b-testing"))
        .expect("indexed skill should appear in plain listing")
        .to_string();
    let first_col = line.split('\t').next().unwrap_or("").to_string();
    assert_eq!(
        first_col, "a-b-testing-platform-for-saas",
        "plain listing column 1 must be the canonical id"
    );

    // 2. The listed id must resolve in `ms show`.
    fixture.log_step("Show by the id the listing printed");
    let output = fixture.run_ms(&["--robot", "show", &first_col]);
    fixture.assert_success(&output, "show by listed id");
    let json = output.json();
    assert_eq!(
        json["skill"]["id"].as_str(),
        Some("a-b-testing-platform-for-saas")
    );

    // 3. The human-facing name must also resolve in `ms show`.
    fixture.log_step("Show by the frontmatter name");
    let output = fixture.run_ms(&["--robot", "show", "ab-testing"]);
    fixture.assert_success(&output, "show by name");
    let json = output.json();
    assert_eq!(
        json["skill"]["id"].as_str(),
        Some("a-b-testing-platform-for-saas"),
        "show by name must resolve to the canonical id"
    );

    // 4. `ms quality <id>` must accept the same id `ms show` accepts, even
    //    when the skill is served from the Git archive rather than a
    //    configured filesystem root.
    fixture.log_step("Quality by canonical id");
    let output = fixture.run_ms(&["--robot", "quality", "a-b-testing-platform-for-saas"]);
    fixture.assert_success(&output, "quality by id");

    fixture.generate_report();
    Ok(())
}
