//! E2E Scenario: Import Workflow Integration Tests
//!
//! Comprehensive tests for the `ms import` command covering:
//! - Importing a skill from a markdown file
//! - Importing from a system prompt file
//! - Importing with metadata hints (--id, --name, --tags)
//! - Dry-run mode
//! - Importing a non-existent file
//! - Batch import from a directory
//! - Import with linting enabled
//! - Robot/JSON output format verification

use super::fixture::E2EFixture;
use ms::error::Result;

// ---------------------------------------------------------------------------
// Sample input documents for import
// ---------------------------------------------------------------------------

const MARKDOWN_DOC: &str = r#"# Kubernetes Deployment Best Practices

Always use resource limits for your pods.

## Rules

- Set CPU and memory limits on every container
- Use liveness and readiness probes
- Never run containers as root
- Use namespaces to isolate workloads

## Examples

```yaml
resources:
  limits:
    cpu: "500m"
    memory: "128Mi"
  requests:
    cpu: "100m"
    memory: "64Mi"
```

## Pitfalls

- Forgetting to set resource limits can cause node starvation
- Over-provisioning wastes cluster resources
"#;

const SYSTEM_PROMPT_DOC: &str = r#"You are an expert Python code reviewer. Follow these rules:

1. Always check for proper type hints on function signatures.
2. Ensure all public functions have docstrings.
3. Flag any use of mutable default arguments.
4. Recommend dataclasses over plain dicts for structured data.
5. Verify exception handling follows the principle of least surprise.

When reviewing:
- Look for unused imports
- Check for proper context manager usage with files
- Ensure tests cover edge cases
"#;

const PLAINTEXT_DOC: &str = r#"Git Workflow Guide

Always create feature branches from main.
Write descriptive commit messages.
Squash commits before merging.
Use conventional commits format.

Do not force push to shared branches.
Do not commit secrets or credentials.
"#;

const MINIMAL_DOC: &str = r#"# Tiny Skill

Just a single rule: keep it simple.
"#;

// ---------------------------------------------------------------------------
// Setup helpers
// ---------------------------------------------------------------------------

/// Create a fixture with ms initialized and sample documents written to disk.
fn setup_import_fixture(scenario: &str) -> Result<E2EFixture> {
    let mut fixture = E2EFixture::new(scenario);

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    fixture.checkpoint("import:initialized");

    Ok(fixture)
}

/// Write a sample document file into the fixture root and return its path.
fn write_doc(fixture: &E2EFixture, filename: &str, content: &str) -> std::path::PathBuf {
    let path = fixture.root.join(filename);
    std::fs::write(&path, content).expect("Failed to write test document");
    path
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Test importing a markdown document into a skill.
#[test]
fn test_import_markdown_file() -> Result<()> {
    let mut fixture = setup_import_fixture("import_markdown")?;

    let doc_path = write_doc(&fixture, "k8s-practices.md", MARKDOWN_DOC);

    fixture.log_step("Import markdown document");
    let output = fixture.run_ms(&[
        "--robot",
        "import",
        doc_path.to_str().unwrap(),
        "--non-interactive",
    ]);
    fixture.assert_success(&output, "import markdown");

    // Verify JSON output from robot mode
    let json: serde_json::Value = serde_json::from_str(&output.stdout)
        .expect("Import robot output should be valid JSON");

    // Check stats are present
    let stats = &json["stats"];
    assert!(
        stats.get("total_blocks").is_some(),
        "Import output should include stats.total_blocks"
    );
    let total_blocks = stats["total_blocks"].as_u64().unwrap_or(0);
    assert!(
        total_blocks > 0,
        "Should have parsed at least one content block"
    );

    // Check output file was created
    let expected_output = fixture.root.join("k8s-practices.skill.md");
    assert!(
        expected_output.exists(),
        "Generated skill file should exist at {:?}",
        expected_output
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "import",
        &format!("Imported markdown with {} blocks", total_blocks),
        Some(serde_json::json!({
            "source": "k8s-practices.md",
            "total_blocks": total_blocks
        })),
    );

    fixture.checkpoint("import:after_markdown");
    fixture.generate_report();
    Ok(())
}

/// Test importing a system-prompt-style document.
#[test]
fn test_import_system_prompt() -> Result<()> {
    let mut fixture = setup_import_fixture("import_system_prompt")?;

    let doc_path = write_doc(&fixture, "python-reviewer.txt", SYSTEM_PROMPT_DOC);

    fixture.log_step("Import system prompt document");
    let output = fixture.run_ms(&[
        "--robot",
        "import",
        doc_path.to_str().unwrap(),
        "--non-interactive",
        "--format",
        "system-prompt",
    ]);
    fixture.assert_success(&output, "import system prompt");

    // Output file should be created
    let expected_output = fixture.root.join("python-reviewer.skill.md");
    assert!(
        expected_output.exists(),
        "Generated skill file should exist at {:?}",
        expected_output
    );

    // Read the generated skill and verify it has content
    let generated = std::fs::read_to_string(&expected_output)
        .expect("Should read generated skill file");
    assert!(
        !generated.is_empty(),
        "Generated skill should not be empty"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "import",
        "System prompt imported successfully",
        Some(serde_json::json!({
            "source": "python-reviewer.txt",
            "output_size": generated.len()
        })),
    );

    fixture.generate_report();
    Ok(())
}

/// Test importing with metadata hints.
#[test]
fn test_import_with_hints() -> Result<()> {
    let mut fixture = setup_import_fixture("import_with_hints")?;

    let doc_path = write_doc(&fixture, "git-guide.txt", PLAINTEXT_DOC);

    fixture.log_step("Import with metadata hints");
    let output = fixture.run_ms(&[
        "--robot",
        "import",
        doc_path.to_str().unwrap(),
        "--non-interactive",
        "--id",
        "git-workflow",
        "--name",
        "Git Workflow Guide",
        "--tags",
        "git,workflow,best-practices",
    ]);
    fixture.assert_success(&output, "import with hints");

    // Check JSON output for metadata
    let json: serde_json::Value = serde_json::from_str(&output.stdout)
        .expect("Import robot output should be valid JSON");

    let metadata = &json["metadata"];
    assert_eq!(
        metadata["id"].as_str(),
        Some("git-workflow"),
        "Imported skill should use the hinted ID"
    );
    assert_eq!(
        metadata["name"].as_str(),
        Some("Git Workflow Guide"),
        "Imported skill should use the hinted name"
    );

    // Tags should include the provided ones
    let tags = metadata["tags"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert!(
        tags.contains(&"git"),
        "Tags should include 'git'"
    );
    assert!(
        tags.contains(&"workflow"),
        "Tags should include 'workflow'"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "import",
        "Import with metadata hints verified",
        Some(serde_json::json!({
            "id": "git-workflow",
            "name": "Git Workflow Guide",
            "tags": tags
        })),
    );

    fixture.generate_report();
    Ok(())
}

/// Test dry-run mode (parse and classify without writing).
#[test]
fn test_import_dry_run() -> Result<()> {
    let mut fixture = setup_import_fixture("import_dry_run")?;

    let doc_path = write_doc(&fixture, "dry-run-test.md", MARKDOWN_DOC);
    let expected_output = fixture.root.join("dry-run-test.skill.md");

    fixture.log_step("Import in dry-run mode");
    let output = fixture.run_ms(&[
        "--robot",
        "import",
        doc_path.to_str().unwrap(),
        "--non-interactive",
        "--dry-run",
    ]);
    fixture.assert_success(&output, "import --dry-run");

    // In dry-run mode, the output file should NOT be created
    assert!(
        !expected_output.exists(),
        "Dry-run should not create output file"
    );

    // But the JSON output should still have stats
    let json: serde_json::Value = serde_json::from_str(&output.stdout)
        .expect("Import dry-run robot output should be valid JSON");
    let total_blocks = json["stats"]["total_blocks"].as_u64().unwrap_or(0);
    assert!(
        total_blocks > 0,
        "Dry-run should still parse and report blocks"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "import",
        "Dry-run mode verified (no file written)",
        Some(serde_json::json!({
            "file_exists": false,
            "total_blocks": total_blocks
        })),
    );

    fixture.generate_report();
    Ok(())
}

/// Test importing a non-existent file.
#[test]
fn test_import_nonexistent_file() -> Result<()> {
    let mut fixture = setup_import_fixture("import_nonexistent")?;

    fixture.log_step("Attempt to import non-existent file");
    let output = fixture.run_ms(&[
        "--robot",
        "import",
        "/tmp/this-file-definitely-does-not-exist-12345.md",
        "--non-interactive",
    ]);

    // The command should fail
    assert!(
        !output.success,
        "Importing a non-existent file should fail"
    );

    // Error output should mention the file or a read error
    let combined = format!("{}{}", output.stdout, output.stderr);
    let mentions_error = combined.to_lowercase().contains("fail")
        || combined.to_lowercase().contains("error")
        || combined.to_lowercase().contains("not found")
        || combined.to_lowercase().contains("no such file");
    assert!(
        mentions_error,
        "Output should indicate the file could not be read. Got: {}",
        combined
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "import",
        "Non-existent file correctly rejected",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Test importing to a custom output path.
#[test]
fn test_import_custom_output_path() -> Result<()> {
    let mut fixture = setup_import_fixture("import_custom_output")?;

    let doc_path = write_doc(&fixture, "source.md", MINIMAL_DOC);
    let custom_output = fixture.root.join("custom-output-dir");
    std::fs::create_dir_all(&custom_output).expect("create custom output dir");
    let output_file = custom_output.join("my-skill.skill.md");

    fixture.log_step("Import with custom output path");
    let output = fixture.run_ms(&[
        "--robot",
        "import",
        doc_path.to_str().unwrap(),
        "--non-interactive",
        "--output",
        output_file.to_str().unwrap(),
    ]);
    fixture.assert_success(&output, "import with custom output");

    assert!(
        output_file.exists(),
        "Skill file should be created at custom output path {:?}",
        output_file
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "import",
        "Custom output path verified",
        Some(serde_json::json!({
            "output_path": output_file.to_str()
        })),
    );

    fixture.generate_report();
    Ok(())
}

/// Test batch import of multiple files in a directory.
#[test]
fn test_import_batch() -> Result<()> {
    let mut fixture = setup_import_fixture("import_batch")?;

    // Create a source directory with multiple files
    let source_dir = fixture.root.join("import-sources");
    std::fs::create_dir_all(&source_dir).expect("create source dir");

    std::fs::write(source_dir.join("doc1.md"), MARKDOWN_DOC)
        .expect("write doc1");
    std::fs::write(source_dir.join("doc2.md"), PLAINTEXT_DOC)
        .expect("write doc2");
    std::fs::write(source_dir.join("doc3.md"), MINIMAL_DOC)
        .expect("write doc3");

    let output_dir = fixture.root.join("batch-output");
    std::fs::create_dir_all(&output_dir).expect("create output dir");

    fixture.log_step("Batch import from directory");
    let output = fixture.run_ms(&[
        "--robot",
        "import",
        source_dir.to_str().unwrap(),
        "--batch",
        "--non-interactive",
        "--output",
        output_dir.to_str().unwrap(),
        "--pattern",
        "*.md",
    ]);
    fixture.assert_success(&output, "import --batch");

    // Verify JSON batch report
    let json: serde_json::Value = serde_json::from_str(&output.stdout)
        .expect("Batch import robot output should be valid JSON");

    let total = json["total"].as_u64().unwrap_or(0);
    assert!(total > 0, "Batch should process at least one file");

    let imported = json["imported"].as_u64().unwrap_or(0);
    assert!(imported > 0, "At least one file should be imported");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "import",
        &format!("Batch import: {} total, {} imported", total, imported),
        Some(serde_json::json!({
            "total": total,
            "imported": imported,
            "skipped": json["skipped"]
        })),
    );

    fixture.checkpoint("import:after_batch");
    fixture.generate_report();
    Ok(())
}

/// Test import with linting enabled.
#[test]
fn test_import_with_lint() -> Result<()> {
    let mut fixture = setup_import_fixture("import_with_lint")?;

    let doc_path = write_doc(&fixture, "lint-test.md", MARKDOWN_DOC);

    fixture.log_step("Import with --lint flag");
    let output = fixture.run_ms(&[
        "--robot",
        "import",
        doc_path.to_str().unwrap(),
        "--non-interactive",
        "--lint",
    ]);
    fixture.assert_success(&output, "import --lint");

    // JSON output should include lint information
    let json: serde_json::Value = serde_json::from_str(&output.stdout)
        .expect("Import with lint robot output should be valid JSON");

    // lint_passed should be present when --lint is used
    assert!(
        json.get("lint_passed").is_some(),
        "Import with --lint should report lint_passed"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "import",
        "Import with lint verified",
        Some(serde_json::json!({
            "lint_passed": json["lint_passed"],
            "lint_errors": json["lint_errors"],
            "lint_warnings": json["lint_warnings"]
        })),
    );

    fixture.generate_report();
    Ok(())
}

/// Test that batch import on a non-directory path fails.
#[test]
fn test_import_batch_requires_directory() -> Result<()> {
    let mut fixture = setup_import_fixture("import_batch_requires_dir")?;

    let file_path = write_doc(&fixture, "not-a-dir.md", MINIMAL_DOC);

    fixture.log_step("Attempt batch import on a file (not directory)");
    let output = fixture.run_ms(&[
        "--robot",
        "import",
        file_path.to_str().unwrap(),
        "--batch",
        "--non-interactive",
    ]);

    assert!(
        !output.success,
        "Batch import on a file (not directory) should fail"
    );

    let combined = format!("{}{}", output.stdout, output.stderr);
    let mentions_dir = combined.to_lowercase().contains("directory")
        || combined.to_lowercase().contains("dir")
        || combined.to_lowercase().contains("batch");
    assert!(
        mentions_dir,
        "Error should mention directory requirement. Got: {}",
        combined
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "import",
        "Batch on non-directory correctly rejected",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Test import with domain hint.
#[test]
fn test_import_with_domain_hint() -> Result<()> {
    let mut fixture = setup_import_fixture("import_domain_hint")?;

    let doc_path = write_doc(&fixture, "devops-doc.md", MARKDOWN_DOC);

    fixture.log_step("Import with --domain hint");
    let output = fixture.run_ms(&[
        "--robot",
        "import",
        doc_path.to_str().unwrap(),
        "--non-interactive",
        "--domain",
        "devops",
    ]);
    fixture.assert_success(&output, "import --domain devops");

    // Verify the domain is reflected in the JSON output
    let json: serde_json::Value = serde_json::from_str(&output.stdout)
        .expect("Import robot output should be valid JSON");

    let domain = json["metadata"]["domain"].as_str();
    assert_eq!(
        domain,
        Some("devops"),
        "Domain should be 'devops' in output"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "import",
        "Domain hint verified",
        Some(serde_json::json!({ "domain": "devops" })),
    );

    fixture.generate_report();
    Ok(())
}
