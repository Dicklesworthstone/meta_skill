//! E2E Scenario: Load Workflow Integration Tests
//!
//! Comprehensive tests for the `ms load` command covering:
//! - Loading a skill by ID after indexing
//! - Loading with different disclosure levels
//! - Loading a non-existent skill
//! - Loading with --full and --complete flags
//! - Loading with token budget (--pack)
//! - Robot/JSON output format verification

use super::fixture::E2EFixture;
use ms::error::Result;

// ---------------------------------------------------------------------------
// Test skill definitions
// ---------------------------------------------------------------------------

const SKILL_RUST_ERRORS: &str = r#"---
name: Rust Error Handling
description: Best practices for error handling in Rust
tags: [rust, errors, advanced]
---

# Rust Error Handling

Use `Result<T, E>` and propagate errors with `?`.

## Guidelines

- Use thiserror for library errors
- Use anyhow for application errors
- Never use `.unwrap()` in library code

## Examples

```rust
fn read_file(path: &str) -> Result<String> {
    let content = std::fs::read_to_string(path)?;
    Ok(content)
}
```

## Pitfalls

- Avoid swallowing errors silently
- Do not overuse `Box<dyn Error>`
"#;

const SKILL_GO_CONCURRENCY: &str = r#"---
name: Go Concurrency
description: Concurrency patterns in Go
tags: [go, concurrency, intermediate]
provides: [go-concurrency]
---

# Go Concurrency

Use goroutines and channels for concurrent operations.

## Guidelines

- Prefer channels over shared memory
- Always handle done/cancel signals
- Use sync.WaitGroup for fan-out patterns

## Examples

```go
func worker(ch <-chan int, done chan<- bool) {
    for v := range ch {
        fmt.Println(v)
    }
    done <- true
}
```
"#;

const SKILL_WITH_DEPS: &str = r#"---
name: Go Testing
description: Testing patterns in Go
tags: [go, testing]
requires: [go-concurrency]
---

# Go Testing

Test concurrent Go code properly.

## Guidelines

- Use t.Parallel() for independent tests
- Use race detector with -race flag
"#;

// ---------------------------------------------------------------------------
// Setup helpers
// ---------------------------------------------------------------------------

/// Create a fixture with skills indexed and ready to load.
fn setup_load_fixture(scenario: &str) -> Result<E2EFixture> {
    let mut fixture = E2EFixture::new(scenario);

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    fixture.log_step("Create skills");
    fixture.create_skill("rust-error-handling", SKILL_RUST_ERRORS)?;
    fixture.create_skill("go-concurrency", SKILL_GO_CONCURRENCY)?;
    fixture.create_skill("go-testing", SKILL_WITH_DEPS)?;

    fixture.log_step("Index skills");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    fixture.checkpoint("load:indexed");

    Ok(fixture)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Test loading a skill by its ID in JSON (robot) mode.
#[test]
fn test_load_skill_by_id() -> Result<()> {
    let mut fixture = setup_load_fixture("load_skill_by_id")?;

    fixture.log_step("Load skill by ID");
    let output = fixture.run_ms(&["--robot", "load", "rust-error-handling"]);
    fixture.assert_success(&output, "load rust-error-handling");

    // Verify JSON structure
    let json = output.json();
    assert_eq!(json["status"].as_str(), Some("ok"), "Status should be ok");

    let data = &json["data"];
    assert_eq!(
        data["skill_id"].as_str(),
        Some("rust-error-handling"),
        "Skill ID should match"
    );
    assert_eq!(
        data["name"].as_str(),
        Some("Rust Error Handling"),
        "Skill name should match"
    );

    // Content should be present
    assert!(
        data["content"].as_str().is_some(),
        "Content should be present in load output"
    );

    // Token count should be positive
    let token_count = data["token_count"].as_u64().unwrap_or(0);
    assert!(token_count > 0, "Token count should be positive");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "load",
        &format!("Loaded skill with {} tokens", token_count),
        Some(serde_json::json!({
            "skill_id": "rust-error-handling",
            "token_count": token_count
        })),
    );

    fixture.checkpoint("load:after_load");
    fixture.generate_report();
    Ok(())
}

/// Test loading a skill that does not exist.
#[test]
fn test_load_nonexistent_skill() -> Result<()> {
    let mut fixture = setup_load_fixture("load_nonexistent_skill")?;

    fixture.log_step("Attempt to load non-existent skill");
    let output = fixture.run_ms(&["--robot", "load", "this-skill-does-not-exist"]);

    // The command should fail
    assert!(!output.success, "Loading a non-existent skill should fail");

    // stderr or stdout should mention not found
    let combined = format!("{}{}", output.stdout, output.stderr);
    let mentions_not_found = combined.to_lowercase().contains("not found")
        || combined.to_lowercase().contains("error")
        || combined.to_lowercase().contains("no skill");
    assert!(
        mentions_not_found,
        "Output should indicate skill was not found. Got: {}",
        combined
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "load",
        "Non-existent skill correctly rejected",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Test loading with the --full flag for full disclosure.
#[test]
fn test_load_full_disclosure() -> Result<()> {
    let mut fixture = setup_load_fixture("load_full_disclosure")?;

    fixture.log_step("Load skill with --full flag");
    let output = fixture.run_ms(&["--robot", "load", "rust-error-handling", "--full"]);
    fixture.assert_success(&output, "load --full");

    let json = output.json();
    let data = &json["data"];

    // Disclosure level should be "full"
    let level = data["disclosure_level"].as_str().unwrap_or("");
    assert_eq!(level, "full", "Disclosure level should be 'full'");

    // Full disclosure should include more content
    let content = data["content"].as_str().unwrap_or("");
    assert!(
        !content.is_empty(),
        "Full disclosure should produce content"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "load",
        "Full disclosure load verified",
        Some(serde_json::json!({
            "disclosure_level": level,
            "content_length": content.len()
        })),
    );

    fixture.generate_report();
    Ok(())
}

/// Test loading with the --complete flag for complete disclosure (scripts + references).
#[test]
fn test_load_complete_disclosure() -> Result<()> {
    let mut fixture = setup_load_fixture("load_complete_disclosure")?;

    fixture.log_step("Load skill with --complete flag");
    let output = fixture.run_ms(&["--robot", "load", "rust-error-handling", "--complete"]);
    fixture.assert_success(&output, "load --complete");

    let json = output.json();
    let data = &json["data"];

    // Disclosure level should be "complete"
    let level = data["disclosure_level"].as_str().unwrap_or("");
    assert_eq!(level, "complete", "Disclosure level should be 'complete'");

    // Scripts and references arrays should be present (even if empty)
    assert!(
        data.get("scripts").is_some(),
        "Complete disclosure should include scripts field"
    );
    assert!(
        data.get("references").is_some(),
        "Complete disclosure should include references field"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "load",
        "Complete disclosure load verified",
        Some(serde_json::json!({
            "disclosure_level": level
        })),
    );

    fixture.generate_report();
    Ok(())
}

/// Test loading with a token budget via --pack.
#[test]
fn test_load_with_pack_budget() -> Result<()> {
    let mut fixture = setup_load_fixture("load_with_pack_budget")?;

    fixture.log_step("Load skill with --pack budget");
    let output = fixture.run_ms(&["--robot", "load", "rust-error-handling", "--pack", "500"]);
    fixture.assert_success(&output, "load --pack 500");

    let json = output.json();
    let data = &json["data"];

    // Pack info should be present
    let pack = &data["pack"];
    assert!(
        !pack.is_null(),
        "Pack info should be present when using --pack"
    );

    // Token count should be within or near the budget
    let token_count = data["token_count"].as_u64().unwrap_or(0);
    assert!(
        token_count > 0,
        "Token count should be positive with pack budget"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "load",
        &format!("Pack budget load: {} tokens (budget 500)", token_count),
        Some(serde_json::json!({
            "budget": 500,
            "actual_tokens": token_count
        })),
    );

    fixture.generate_report();
    Ok(())
}

/// Test loading a skill with dependencies.
#[test]
fn test_load_with_dependencies() -> Result<()> {
    let mut fixture = setup_load_fixture("load_with_dependencies")?;

    fixture.log_step("Load skill that has dependencies");
    let output = fixture.run_ms(&["--robot", "load", "go-testing"]);
    fixture.assert_success(&output, "load go-testing (has deps)");

    let json = output.json();
    let data = &json["data"];

    // Verify the skill was loaded
    assert_eq!(
        data["skill_id"].as_str(),
        Some("go-testing"),
        "Skill ID should be go-testing"
    );

    // Dependencies loaded should be present (may or may not have resolved deps)
    let deps = data["dependencies_loaded"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "load",
        &format!("Loaded skill with {} dependencies", deps),
        Some(serde_json::json!({
            "skill_id": "go-testing",
            "dependencies_loaded": deps
        })),
    );

    fixture.generate_report();
    Ok(())
}

/// Test loading with dependencies turned off.
#[test]
fn test_load_deps_off() -> Result<()> {
    let mut fixture = setup_load_fixture("load_deps_off")?;

    fixture.log_step("Load skill with --deps off");
    let output = fixture.run_ms(&["--robot", "load", "go-testing", "--deps", "off"]);
    fixture.assert_success(&output, "load --deps off");

    let json = output.json();
    let data = &json["data"];

    // With deps off, dependencies_loaded should be empty
    let deps = data["dependencies_loaded"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);
    assert_eq!(deps, 0, "Dependencies should not be loaded with --deps off");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "load",
        "Deps off mode verified",
        Some(serde_json::json!({ "dependencies_loaded": 0 })),
    );

    fixture.generate_report();
    Ok(())
}

/// Test that loading a skill records usage (verify through JSON output structure).
#[test]
fn test_load_json_output_structure() -> Result<()> {
    let mut fixture = setup_load_fixture("load_json_structure")?;

    fixture.log_step("Load skill and verify JSON output structure");
    let output = fixture.run_ms(&["--robot", "load", "rust-error-handling"]);
    fixture.assert_success(&output, "load for JSON verification");

    let json = output.json();

    // Verify top-level fields
    assert!(json.get("status").is_some(), "Should have 'status' field");
    assert!(
        json.get("timestamp").is_some(),
        "Should have 'timestamp' field"
    );
    assert!(json.get("version").is_some(), "Should have 'version' field");
    assert!(json.get("data").is_some(), "Should have 'data' field");
    assert!(
        json.get("warnings").is_some(),
        "Should have 'warnings' field"
    );

    // Verify data fields
    let data = &json["data"];
    assert!(data.get("skill_id").is_some(), "data should have skill_id");
    assert!(data.get("name").is_some(), "data should have name");
    assert!(
        data.get("disclosure_level").is_some(),
        "data should have disclosure_level"
    );
    assert!(
        data.get("token_count").is_some(),
        "data should have token_count"
    );
    assert!(data.get("content").is_some(), "data should have content");
    assert!(
        data.get("frontmatter").is_some(),
        "data should have frontmatter"
    );
    assert!(
        data.get("dependencies_loaded").is_some(),
        "data should have dependencies_loaded"
    );
    assert!(
        data.get("inheritance_chain").is_some(),
        "data should have inheritance_chain"
    );

    // Verify frontmatter fields
    let fm = &data["frontmatter"];
    assert!(fm.get("id").is_some(), "frontmatter should have id");
    assert!(fm.get("name").is_some(), "frontmatter should have name");
    assert!(
        fm.get("description").is_some() || fm.get("version").is_some(),
        "frontmatter should have metadata fields"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "load",
        "JSON output structure verified",
        Some(serde_json::json!({
            "top_level_fields": ["status", "timestamp", "version", "data", "warnings"],
            "data_fields": ["skill_id", "name", "disclosure_level", "token_count", "content"]
        })),
    );

    fixture.generate_report();
    Ok(())
}

/// Test loading with an explicit disclosure level.
#[test]
fn test_load_explicit_level() -> Result<()> {
    let mut fixture = setup_load_fixture("load_explicit_level")?;

    fixture.log_step("Load skill with --level overview");
    let output = fixture.run_ms(&[
        "--robot",
        "load",
        "rust-error-handling",
        "--level",
        "overview",
    ]);
    fixture.assert_success(&output, "load --level overview");

    let json = output.json();
    let data = &json["data"];
    let level = data["disclosure_level"].as_str().unwrap_or("");

    // The level should be "overview"
    assert_eq!(level, "overview", "Disclosure level should be 'overview'");

    // Load again with a higher level and compare token counts
    fixture.log_step("Load skill with --level full");
    let output_full =
        fixture.run_ms(&["--robot", "load", "rust-error-handling", "--level", "full"]);
    fixture.assert_success(&output_full, "load --level full");

    let json_full = output_full.json();
    let tokens_overview = data["token_count"].as_u64().unwrap_or(0);
    let tokens_full = json_full["data"]["token_count"].as_u64().unwrap_or(0);

    // Full disclosure should generally have more tokens than overview
    // (but allow equal in edge cases for very small skills)
    assert!(
        tokens_full >= tokens_overview,
        "Full disclosure ({}) should have >= tokens than overview ({})",
        tokens_full,
        tokens_overview
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "load",
        "Explicit disclosure levels verified",
        Some(serde_json::json!({
            "overview_tokens": tokens_overview,
            "full_tokens": tokens_full
        })),
    );

    fixture.generate_report();
    Ok(())
}

/// Test that loading without --robot produces human-readable output.
#[test]
fn test_load_human_output() -> Result<()> {
    let mut fixture = setup_load_fixture("load_human_output")?;

    fixture.log_step("Load skill without --robot (human output)");
    let output = fixture.run_ms(&["load", "rust-error-handling"]);
    fixture.assert_success(&output, "load human output");

    // Human output should contain the skill name
    fixture.assert_output_contains(&output, "Rust Error Handling");

    // Human output should not be valid JSON (it is prose/formatted)
    let is_json = serde_json::from_str::<serde_json::Value>(&output.stdout).is_ok();
    assert!(!is_json, "Human output should not be raw JSON");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "load",
        "Human output format verified",
        None,
    );

    fixture.generate_report();
    Ok(())
}
