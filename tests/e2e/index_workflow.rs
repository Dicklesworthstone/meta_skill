//! E2E Scenario: Index Workflow Integration Tests
//!
//! Comprehensive tests for the `ms index` command covering:
//! - Index a workspace with skills
//! - Re-index with force flag
//! - Index with explicit path filters
//! - Index status reporting (indexed count, errors, elapsed)

use super::fixture::E2EFixture;
use ms::error::Result;

// Test skill definitions

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
"#;

const SKILL_GO_ERRORS: &str = r#"---
name: Go Error Handling
description: Error handling patterns in Go
tags: [go, errors, beginner]
---

# Go Error Handling

Check errors explicitly after each function call.

## Guidelines

- Wrap errors with context
- Use sentinel errors sparingly
"#;

const SKILL_PYTHON_TESTING: &str = r#"---
name: Python Testing
description: Testing strategies for Python projects
tags: [python, testing, intermediate]
---

# Python Testing

Use pytest for all testing needs.

## Guidelines

- Write unit tests first
- Use fixtures for setup
"#;

const SKILL_WITH_DISTINCT_PREAMBLE: &str = r#"---
name: Preamble Recall
description: Short catalog summary
tags: [search, regression]
---

# Preamble Recall

The distinct intro contains unique token preamblecanary.

## Notes

Section content remains searchable too.
"#;

#[allow(dead_code)]
const SKILL_INVALID: &str = r#"---
name:
description:
---

Not a valid skill (missing required fields).
"#;

/// Create a fixture with skills in project layer for basic indexing
fn setup_index_fixture(scenario: &str) -> Result<E2EFixture> {
    let mut fixture = E2EFixture::new(scenario);

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    fixture.log_step("Create skills in project layer");
    fixture.create_skill("rust-error-handling", SKILL_RUST_ERRORS)?;
    fixture.create_skill("go-error-handling", SKILL_GO_ERRORS)?;
    fixture.create_skill("python-testing", SKILL_PYTHON_TESTING)?;

    Ok(fixture)
}

#[test]
fn test_index_workspace() -> Result<()> {
    let mut fixture = setup_index_fixture("index_workspace")?;

    // Checkpoint: pre-index
    fixture.checkpoint("index:pre-index");

    fixture.log_step("Index skills");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    // Checkpoint: post-index
    fixture.checkpoint("index:post-index");

    let json = output.json();
    let status = json["status"].as_str().expect("status field");
    let indexed = json["indexed"].as_u64().expect("indexed field");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "index",
        &format!("Indexed {} skills, status={}", indexed, status),
        Some(serde_json::json!({ "status": status, "indexed": indexed })),
    );

    assert_eq!(status, "ok", "Index status should be ok");
    assert_eq!(indexed, 3, "Should have indexed 3 skills");

    // Verify skills are now listable
    fixture.log_step("Verify skills are listed after indexing");
    let list_output = fixture.run_ms(&["--robot", "list"]);
    fixture.assert_success(&list_output, "list after index");

    let list_json = list_output.json();
    let count = list_json["count"].as_u64().expect("count");
    assert_eq!(count, 3, "List should return 3 skills after indexing");

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_index_preserves_and_searches_distinct_h1_preamble() -> Result<()> {
    let mut fixture = E2EFixture::new("index_preamble_recall");

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    fixture.log_step("Create skill with body preamble distinct from frontmatter description");
    fixture.create_skill("preamble-recall", SKILL_WITH_DISTINCT_PREAMBLE)?;

    fixture.log_step("Index skill");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    fixture.log_step("Search for preamble-only token");
    let output = fixture.run_ms(&[
        "--robot",
        "search",
        "preamblecanary",
        "--search-type",
        "bm25",
    ]);
    fixture.assert_success(&output, "search preamble-only token");

    let json = output.json();
    let results = json["results"]
        .as_array()
        .expect("search results array")
        .iter()
        .filter_map(|result| result["id"].as_str())
        .collect::<Vec<_>>();
    assert!(
        results.contains(&"preamble-recall"),
        "preamble-only token must reach the stored body and Tantivy index: {results:?}"
    );

    Ok(())
}

#[test]
fn test_reindex_force() -> Result<()> {
    let mut fixture = setup_index_fixture("reindex_force")?;

    fixture.log_step("Initial index");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "initial index");

    let json = output.json();
    let first_indexed = json["indexed"].as_u64().expect("indexed");

    // Checkpoint: after first index
    fixture.checkpoint("index:first-pass");

    fixture.log_step("Re-index without force (should skip unchanged)");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "reindex without force");

    let json = output.json();
    let second_indexed = json["indexed"].as_u64().expect("indexed");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "index",
        &format!(
            "First pass: {}, Second pass (no force): {}",
            first_indexed, second_indexed
        ),
        Some(serde_json::json!({
            "first_indexed": first_indexed,
            "second_indexed": second_indexed,
        })),
    );

    // Without force, unchanged skills should be skipped, so indexed count may differ
    // The important thing is that the command succeeds
    assert!(
        second_indexed <= first_indexed,
        "Re-index without force should index same or fewer skills"
    );

    // Checkpoint: after second index
    fixture.checkpoint("index:second-pass");

    fixture.log_step("Re-index with --force (should re-index all)");
    let output = fixture.run_ms(&["--robot", "index", "--force"]);
    fixture.assert_success(&output, "reindex with force");

    let json = output.json();
    let force_indexed = json["indexed"].as_u64().expect("indexed");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "index",
        &format!("Force re-index: {} skills", force_indexed),
        Some(serde_json::json!({ "force_indexed": force_indexed })),
    );

    assert_eq!(
        force_indexed, first_indexed,
        "Force re-index should re-index all skills"
    );

    // Checkpoint: after force index
    fixture.checkpoint("index:force-pass");

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_index_with_explicit_path() -> Result<()> {
    let mut fixture = E2EFixture::new("index_explicit_path");

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    // Create skills in a custom subdirectory (not the default skills dir)
    fixture.log_step("Create skills in custom directory");
    let custom_dir = fixture.root.join("custom_skills");
    std::fs::create_dir_all(&custom_dir).expect("create custom_skills dir");

    let skill_dir = custom_dir.join("rust-error-handling");
    std::fs::create_dir_all(&skill_dir).expect("create skill dir");
    std::fs::write(skill_dir.join("SKILL.md"), SKILL_RUST_ERRORS).expect("write skill");

    let skill_dir2 = custom_dir.join("go-error-handling");
    std::fs::create_dir_all(&skill_dir2).expect("create skill dir");
    std::fs::write(skill_dir2.join("SKILL.md"), SKILL_GO_ERRORS).expect("write skill");

    // Checkpoint: pre-index
    fixture.checkpoint("index:pre-explicit");

    fixture.log_step("Index with explicit path");
    let output = fixture.run_ms(&["--robot", "index", "./custom_skills"]);
    fixture.assert_success(&output, "index explicit path");

    let json = output.json();
    let indexed = json["indexed"].as_u64().expect("indexed");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "index",
        &format!("Indexed {} skills from explicit path", indexed),
        Some(serde_json::json!({
            "indexed": indexed,
            "path": "./custom_skills",
        })),
    );

    assert_eq!(indexed, 2, "Should index 2 skills from custom directory");

    // Checkpoint: post-index
    fixture.checkpoint("index:post-explicit");

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_index_status_reporting() -> Result<()> {
    let mut fixture = setup_index_fixture("index_status_reporting")?;

    fixture.log_step("Index skills and check status fields");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    let json = output.json();

    // Verify JSON response contains all expected fields
    assert!(
        json.get("status").is_some(),
        "Response should have 'status' field"
    );
    assert!(
        json.get("indexed").is_some(),
        "Response should have 'indexed' field"
    );
    assert!(
        json.get("errors").is_some(),
        "Response should have 'errors' field"
    );
    assert!(
        json.get("elapsed_ms").is_some(),
        "Response should have 'elapsed_ms' field"
    );

    let status = json["status"].as_str().expect("status");
    let indexed = json["indexed"].as_u64().expect("indexed");
    let errors = json["errors"].as_array().expect("errors array");
    let elapsed_ms = json["elapsed_ms"].as_u64().expect("elapsed_ms");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "index",
        &format!(
            "Index status: status={}, indexed={}, errors={}, elapsed={}ms",
            status,
            indexed,
            errors.len(),
            elapsed_ms
        ),
        Some(serde_json::json!({
            "status": status,
            "indexed": indexed,
            "error_count": errors.len(),
            "elapsed_ms": elapsed_ms,
        })),
    );

    assert_eq!(status, "ok", "Status should be ok with no errors");
    assert_eq!(indexed, 3, "Should have indexed 3 skills");
    assert!(errors.is_empty(), "Should have no errors");
    assert!(elapsed_ms > 0, "Elapsed time should be positive");

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_index_empty_workspace() -> Result<()> {
    let mut fixture = E2EFixture::new("index_empty_workspace");

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    // Don't create any skills

    fixture.log_step("Index empty workspace");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index empty");

    let json = output.json();
    let indexed = json["indexed"].as_u64().expect("indexed");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "index",
        "Indexed empty workspace",
        Some(serde_json::json!({ "indexed": indexed })),
    );

    assert_eq!(indexed, 0, "Should index 0 skills in empty workspace");

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_index_with_multi_layer_paths() -> Result<()> {
    let mut fixture = E2EFixture::new("index_multi_layer");

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    fixture.log_step("Configure skill paths for all layers");
    let output = fixture.run_ms(&[
        "--robot",
        "config",
        "skill_paths.global",
        r#"["./global_skills"]"#,
    ]);
    fixture.assert_success(&output, "config skill_paths.global");

    let output = fixture.run_ms(&[
        "--robot",
        "config",
        "skill_paths.local",
        r#"["./local_skills"]"#,
    ]);
    fixture.assert_success(&output, "config skill_paths.local");

    fixture.log_step("Create skills in different layers");
    fixture.create_skill_in_layer("rust-error-handling", SKILL_RUST_ERRORS, "project")?;
    fixture.create_skill_in_layer("go-error-handling", SKILL_GO_ERRORS, "global")?;
    fixture.create_skill_in_layer("python-testing", SKILL_PYTHON_TESTING, "local")?;

    // Checkpoint: pre-index
    fixture.checkpoint("index:pre-multi-layer");

    fixture.log_step("Index all layers");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index all layers");

    let json = output.json();
    let indexed = json["indexed"].as_u64().expect("indexed");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "index",
        &format!("Indexed {} skills across multiple layers", indexed),
        Some(serde_json::json!({ "indexed": indexed })),
    );

    assert_eq!(indexed, 3, "Should index 3 skills across all layers");

    // Verify that skills from different layers are present
    fixture.log_step("Verify skills from all layers");
    let list_output = fixture.run_ms(&["--robot", "list"]);
    fixture.assert_success(&list_output, "list after multi-layer index");

    let list_json = list_output.json();
    let skills = list_json["skills"].as_array().expect("skills array");

    let skill_ids: Vec<&str> = skills.iter().filter_map(|s| s["id"].as_str()).collect();
    assert!(
        skill_ids.contains(&"rust-error-handling"),
        "Should contain project-layer skill"
    );
    assert!(
        skill_ids.contains(&"go-error-handling"),
        "Should contain global-layer skill"
    );
    assert!(
        skill_ids.contains(&"python-testing"),
        "Should contain local-layer skill"
    );

    // Checkpoint: post-index
    fixture.checkpoint("index:post-multi-layer");

    fixture.generate_report();
    Ok(())
}

/// Regression test for issue #133.
///
/// When the search index can only be opened read-only — here another process
/// holds the Tantivy writer lock, exactly as a live `ms mcp serve` would —
/// `ms index` must fail fast with a clear, actionable diagnostic instead of
/// silently degrading to a read-only index and aborting mid-run with the opaque
/// "Index opened in read-only mode" Tantivy error. It must also NOT partially
/// write skills into the database before failing.
#[test]
fn test_index_readonly_index_reports_clear_error() -> Result<()> {
    use ms::search::tantivy::Bm25Index;

    let mut fixture = setup_index_fixture("index_readonly_guard")?;

    // Hold the Tantivy writer lock from this test process, emulating a live
    // `ms mcp serve`. The `ms index` subprocess below will find the index busy,
    // fall back to a read-only handle, and must reject the write up front.
    let index_dir = fixture.ms_root.join("index");
    let _writer_guard = Bm25Index::open(&index_dir).expect("open index writable in test process");

    let output = fixture.run_ms(&["index"]);

    assert!(
        !output.success,
        "ms index must fail when the index is read-only; got success.\nstdout={}\nstderr={}",
        output.stdout, output.stderr
    );

    let combined = format!("{}\n{}", output.stdout, output.stderr);

    // The clear, actionable diagnostic — not the opaque Tantivy internal error.
    assert!(
        combined.contains("Cannot write to the search index"),
        "expected the actionable #133 diagnostic; got:\n{combined}"
    );
    assert!(
        combined.contains("writer lock") || combined.contains("read-only filesystem"),
        "diagnostic must name the concrete cause; got:\n{combined}"
    );
    assert!(
        !combined.contains("Index opened in read-only mode"),
        "must not surface the opaque Tantivy read-only error; got:\n{combined}"
    );

    // The guard runs before any indexing work, so nothing should have been
    // written to the database. (The write lock is still held here, so `list`
    // opens the index read-only and reads skills straight from SQLite.)
    let list = fixture.run_ms(&["--robot", "list"]);
    let count = list.json()["count"].as_u64().unwrap_or(0);
    assert_eq!(
        count, 0,
        "index must not partially write skills before failing; list returned {count}"
    );

    drop(_writer_guard);
    fixture.generate_report();
    Ok(())
}
