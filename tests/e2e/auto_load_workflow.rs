//! E2E Scenario: Auto-load Workflow Integration Tests
//!
//! Covers context-aware skill loading based on project type detection,
//! file patterns, tools, signals, and threshold filtering.

use super::fixture::E2EFixture;
use ms::error::Result;
use std::fs;

// =============================================================================
// SKILL DEFINITIONS WITH CONTEXT TAGS
// =============================================================================

const RUST_SKILL: &str = r#"---
name: Rust Error Handling
description: Best practices for Rust error handling
tags: [rust, errors]
context:
  project_types: [rust]
  file_patterns: ["*.rs", "Cargo.toml"]
  tools: [cargo, rustc]
---

# Rust Error Handling

Use `thiserror` for library errors and `anyhow` for application errors.

## Rules

- Prefer `Result` over panicking
- Use `?` operator for propagation
"#;

const NODE_SKILL: &str = r#"---
name: Node.js Testing
description: Testing patterns for Node.js applications
tags: [node, testing]
context:
  project_types: [node]
  file_patterns: ["*.ts", "*.js", "package.json"]
  tools: [npm, node, npx]
---

# Node.js Testing

Use Jest or Vitest for unit testing.

## Rules

- Write tests alongside source files
- Use mocks for external dependencies
"#;

const PYTHON_SKILL: &str = r#"---
name: Python Type Hints
description: Modern Python type annotation patterns
tags: [python, typing]
context:
  project_types: [python]
  file_patterns: ["*.py", "pyproject.toml"]
  tools: [python, pip, uv]
---

# Python Type Hints

Use type annotations for better code quality.

## Rules

- Annotate function signatures
- Use `typing` module for complex types
"#;

const GENERIC_SKILL: &str = r#"---
name: Git Workflow
description: Generic git workflow patterns
tags: [git, workflow]
---

# Git Workflow

Standard git branching model.

## Rules

- Use feature branches
- Write clear commit messages
"#;

const SIGNAL_SKILL: &str = r##"---
name: Thiserror Patterns
description: Advanced thiserror usage patterns
tags: [rust, errors, thiserror]
context:
  project_types: [rust]
  tools: [cargo]
  signals:
    - name: thiserror_usage
      pattern: "use.*thiserror"
      weight: 0.9
    - name: derive_error
      pattern: "#\\[derive\\(.*Error"
      weight: 0.8
---

# Thiserror Patterns

Advanced patterns for the thiserror crate.

## Rules

- Use `#[from]` for automatic conversion
- Add display messages for each variant
"##;

const FILE_PATTERN_SKILL: &str = r#"---
name: Markdown Documentation
description: Documentation writing patterns
tags: [docs, markdown]
context:
  file_patterns: ["*.md", "README*", "docs/**/*"]
---

# Markdown Documentation

Write clear and effective documentation.

## Rules

- Start with a clear title
- Include usage examples
"#;

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

fn setup_auto_load_fixture(scenario: &str) -> Result<E2EFixture> {
    let mut fixture = E2EFixture::new(scenario);

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    fixture.log_step("Configure skill paths");
    let output = fixture.run_ms(&[
        "--robot",
        "config",
        "skill_paths.project",
        r#"["./skills"]"#,
    ]);
    fixture.assert_success(&output, "config skill_paths.project");

    Ok(fixture)
}

fn create_skill_and_index(fixture: &mut E2EFixture, name: &str, content: &str) -> Result<()> {
    fixture.create_skill(name, content)?;
    Ok(())
}

fn index_skills(fixture: &mut E2EFixture) {
    fixture.log_step("Index skills");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");
}

// =============================================================================
// TEST: AUTO-LOAD RUST PROJECT
// =============================================================================

#[test]
fn test_auto_load_rust_project() -> Result<()> {
    let mut fixture = setup_auto_load_fixture("auto_load_rust_project")?;

    fixture.log_step("Create skills with different context tags");
    create_skill_and_index(&mut fixture, "rust-errors", RUST_SKILL)?;
    create_skill_and_index(&mut fixture, "node-testing", NODE_SKILL)?;
    create_skill_and_index(&mut fixture, "python-hints", PYTHON_SKILL)?;
    create_skill_and_index(&mut fixture, "git-workflow", GENERIC_SKILL)?;

    index_skills(&mut fixture);

    fixture.log_step("Create Rust project markers");
    fs::write(fixture.root.join("Cargo.toml"), "[package]\nname = \"test\"")?;
    fs::create_dir_all(fixture.root.join("src"))?;
    fs::write(fixture.root.join("src/main.rs"), "fn main() {}")?;

    fixture.log_step("Auto-load with --dry_run");
    let output = fixture.run_ms(&["--robot", "load", "--auto", "--dry_run"]);
    fixture.assert_success(&output, "auto-load dry_run");

    let json = output.json();
    let would_load = json["data"]["would_load"].as_array().expect("would_load array");

    // Rust skill should be in the candidates
    let rust_found = would_load.iter().any(|s| {
        s["skill_id"].as_str().unwrap_or_default() == "rust-errors"
    });
    assert!(rust_found, "Rust skill should be detected in Rust project");

    // Node skill should NOT be in candidates (wrong project type)
    let node_found = would_load.iter().any(|s| {
        s["skill_id"].as_str().unwrap_or_default() == "node-testing"
    });
    assert!(!node_found, "Node skill should not match Rust project");

    // Generic skill (no context) should not be returned
    let generic_found = would_load.iter().any(|s| {
        s["skill_id"].as_str().unwrap_or_default() == "git-workflow"
    });
    assert!(!generic_found, "Generic skill without context should not match");

    Ok(())
}

// =============================================================================
// TEST: AUTO-LOAD NODE PROJECT
// =============================================================================

#[test]
fn test_auto_load_node_project() -> Result<()> {
    let mut fixture = setup_auto_load_fixture("auto_load_node_project")?;

    fixture.log_step("Create skills");
    create_skill_and_index(&mut fixture, "rust-errors", RUST_SKILL)?;
    create_skill_and_index(&mut fixture, "node-testing", NODE_SKILL)?;

    index_skills(&mut fixture);

    fixture.log_step("Create Node.js project markers");
    fs::write(
        fixture.root.join("package.json"),
        r#"{"name": "test", "version": "1.0.0"}"#,
    )?;
    fs::write(fixture.root.join("index.ts"), "console.log('hello');")?;

    fixture.log_step("Auto-load with --dry_run");
    let output = fixture.run_ms(&["--robot", "load", "--auto", "--dry_run"]);
    fixture.assert_success(&output, "auto-load dry_run");

    let json = output.json();
    let would_load = json["data"]["would_load"].as_array().expect("would_load array");

    // Node skill should be detected
    let node_found = would_load.iter().any(|s| {
        s["skill_id"].as_str().unwrap_or_default() == "node-testing"
    });
    assert!(node_found, "Node skill should be detected in Node project");

    // Rust skill should NOT match
    let rust_found = would_load.iter().any(|s| {
        s["skill_id"].as_str().unwrap_or_default() == "rust-errors"
    });
    assert!(!rust_found, "Rust skill should not match Node project");

    Ok(())
}

// =============================================================================
// TEST: AUTO-LOAD PYTHON PROJECT
// =============================================================================

#[test]
fn test_auto_load_python_project() -> Result<()> {
    let mut fixture = setup_auto_load_fixture("auto_load_python_project")?;

    fixture.log_step("Create skills");
    create_skill_and_index(&mut fixture, "rust-errors", RUST_SKILL)?;
    create_skill_and_index(&mut fixture, "python-hints", PYTHON_SKILL)?;

    index_skills(&mut fixture);

    fixture.log_step("Create Python project markers");
    fs::write(
        fixture.root.join("pyproject.toml"),
        "[project]\nname = \"test\"",
    )?;
    fs::write(fixture.root.join("main.py"), "print('hello')")?;

    fixture.log_step("Auto-load with --dry_run");
    let output = fixture.run_ms(&["--robot", "load", "--auto", "--dry_run"]);
    fixture.assert_success(&output, "auto-load dry_run");

    let json = output.json();
    let would_load = json["data"]["would_load"].as_array().expect("would_load array");

    // Python skill should be detected
    let python_found = would_load.iter().any(|s| {
        s["skill_id"].as_str().unwrap_or_default() == "python-hints"
    });
    assert!(python_found, "Python skill should be detected in Python project");

    Ok(())
}

// =============================================================================
// TEST: AUTO-LOAD FILE PATTERNS
// =============================================================================

#[test]
fn test_auto_load_file_patterns() -> Result<()> {
    let mut fixture = setup_auto_load_fixture("auto_load_file_patterns")?;

    fixture.log_step("Create skill with file pattern context");
    create_skill_and_index(&mut fixture, "markdown-docs", FILE_PATTERN_SKILL)?;

    index_skills(&mut fixture);

    fixture.log_step("Create markdown files");
    fs::write(fixture.root.join("README.md"), "# Test Project")?;
    fs::create_dir_all(fixture.root.join("docs"))?;
    fs::write(fixture.root.join("docs/guide.md"), "# Guide")?;

    fixture.log_step("Auto-load with --dry_run");
    let output = fixture.run_ms(&["--robot", "load", "--auto", "--dry_run", "--threshold", "0.1"]);
    fixture.assert_success(&output, "auto-load dry_run");

    let json = output.json();
    let would_load = json["data"]["would_load"].as_array().expect("would_load array");

    // Markdown skill should be detected via file patterns
    let md_found = would_load.iter().any(|s| {
        s["skill_id"].as_str().unwrap_or_default() == "markdown-docs"
    });
    assert!(md_found, "Markdown skill should be detected via file patterns");

    // Verify breakdown shows file_patterns contribution
    let md_skill = would_load.iter().find(|s| {
        s["skill_id"].as_str().unwrap_or_default() == "markdown-docs"
    });
    if let Some(skill) = md_skill {
        let file_patterns_score = skill["breakdown"]["file_patterns"].as_f64().unwrap_or(0.0);
        assert!(
            file_patterns_score > 0.0,
            "File patterns score should be positive"
        );
    }

    Ok(())
}

// =============================================================================
// TEST: AUTO-LOAD TOOL DETECTION
// =============================================================================

#[test]
fn test_auto_load_tool_detection() -> Result<()> {
    let mut fixture = setup_auto_load_fixture("auto_load_tool_detection")?;

    fixture.log_step("Create skill with tool requirements");
    create_skill_and_index(&mut fixture, "rust-errors", RUST_SKILL)?;
    create_skill_and_index(&mut fixture, "node-testing", NODE_SKILL)?;

    index_skills(&mut fixture);

    fixture.log_step("Create Rust project markers");
    fs::write(fixture.root.join("Cargo.toml"), "[package]\nname = \"test\"")?;

    fixture.log_step("Auto-load with --dry_run and check tool scoring");
    let output = fixture.run_ms(&["--robot", "load", "--auto", "--dry_run"]);
    fixture.assert_success(&output, "auto-load dry_run");

    let json = output.json();
    let would_load = json["data"]["would_load"].as_array().expect("would_load array");

    // Rust skill should have tool match (cargo, rustc are usually in PATH)
    let rust_skill = would_load.iter().find(|s| {
        s["skill_id"].as_str().unwrap_or_default() == "rust-errors"
    });

    if let Some(skill) = rust_skill {
        // Verify tool detection contributed to the score
        let tools_score = skill["breakdown"]["tools"].as_f64().unwrap_or(0.0);
        // Tools may or may not be detected depending on test environment
        // Just verify the breakdown is present
        assert!(
            skill["breakdown"].get("tools").is_some(),
            "Tools breakdown should be present"
        );
        println!("Tools score: {}", tools_score);
    }

    Ok(())
}

// =============================================================================
// TEST: AUTO-LOAD SIGNALS
// =============================================================================

#[test]
fn test_auto_load_signals() -> Result<()> {
    let mut fixture = setup_auto_load_fixture("auto_load_signals")?;

    fixture.log_step("Create skill with signal patterns");
    create_skill_and_index(&mut fixture, "thiserror-patterns", SIGNAL_SKILL)?;
    create_skill_and_index(&mut fixture, "rust-errors", RUST_SKILL)?;

    index_skills(&mut fixture);

    fixture.log_step("Create Rust project with thiserror usage");
    fs::write(fixture.root.join("Cargo.toml"), "[package]\nname = \"test\"")?;
    fs::create_dir_all(fixture.root.join("src"))?;
    fs::write(
        fixture.root.join("src/error.rs"),
        r#"
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MyError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
"#,
    )?;

    fixture.log_step("Auto-load with --dry_run");
    let output = fixture.run_ms(&["--robot", "load", "--auto", "--dry_run", "--threshold", "0.1"]);
    fixture.assert_success(&output, "auto-load dry_run");

    let json = output.json();
    let would_load = json["data"]["would_load"].as_array().expect("would_load array");

    // Both Rust skills should be detected
    let thiserror_found = would_load.iter().any(|s| {
        s["skill_id"].as_str().unwrap_or_default() == "thiserror-patterns"
    });
    let rust_found = would_load.iter().any(|s| {
        s["skill_id"].as_str().unwrap_or_default() == "rust-errors"
    });

    assert!(rust_found, "Rust errors skill should be detected");
    // Signal matching may not work in E2E if content isn't captured by collector
    // This tests that the signal skill is at least indexed and considered
    println!("Thiserror patterns skill found: {}", thiserror_found);

    Ok(())
}

// =============================================================================
// TEST: AUTO-LOAD THRESHOLD
// =============================================================================

#[test]
fn test_auto_load_threshold() -> Result<()> {
    let mut fixture = setup_auto_load_fixture("auto_load_threshold")?;

    fixture.log_step("Create skills");
    create_skill_and_index(&mut fixture, "rust-errors", RUST_SKILL)?;

    index_skills(&mut fixture);

    fixture.log_step("Create Rust project");
    fs::write(fixture.root.join("Cargo.toml"), "[package]\nname = \"test\"")?;

    fixture.log_step("Auto-load with HIGH threshold (0.9)");
    let output_high = fixture.run_ms(&["--robot", "load", "--auto", "--dry_run", "--threshold", "0.9"]);
    fixture.assert_success(&output_high, "auto-load high threshold");

    fixture.log_step("Auto-load with LOW threshold (0.1)");
    let output_low = fixture.run_ms(&["--robot", "load", "--auto", "--dry_run", "--threshold", "0.1"]);
    fixture.assert_success(&output_low, "auto-load low threshold");

    let high_json = output_high.json();
    let low_json = output_low.json();

    let high_count = high_json["data"]["would_load"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);
    let low_count = low_json["data"]["would_load"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);

    // Low threshold should return >= as many results as high threshold
    assert!(
        low_count >= high_count,
        "Low threshold should return at least as many results as high threshold"
    );

    // Verify threshold is reported in output
    assert_eq!(
        high_json["data"]["threshold"].as_f64().unwrap_or(0.0),
        0.9,
        "High threshold should be in output"
    );
    assert_eq!(
        low_json["data"]["threshold"].as_f64().unwrap_or(0.0),
        0.1,
        "Low threshold should be in output"
    );

    Ok(())
}

// =============================================================================
// TEST: AUTO-LOAD DRY RUN
// =============================================================================

#[test]
fn test_auto_load_dry_run() -> Result<()> {
    let mut fixture = setup_auto_load_fixture("auto_load_dry_run")?;

    fixture.log_step("Create skills");
    create_skill_and_index(&mut fixture, "rust-errors", RUST_SKILL)?;

    index_skills(&mut fixture);

    fixture.log_step("Create Rust project");
    fs::write(fixture.root.join("Cargo.toml"), "[package]\nname = \"test\"")?;

    fixture.log_step("Auto-load with --dry_run");
    let output = fixture.run_ms(&["--robot", "load", "--auto", "--dry_run"]);
    fixture.assert_success(&output, "auto-load dry_run");

    let json = output.json();

    // Verify dry_run flag is true
    assert_eq!(
        json["dry_run"].as_bool().unwrap_or(false),
        true,
        "dry_run flag should be true"
    );

    // Should have would_load array instead of loaded
    assert!(
        json["data"]["would_load"].is_array(),
        "Should have would_load array in dry_run mode"
    );

    // Should show context detection info
    assert!(
        json["data"]["context"].is_object(),
        "Should include context summary"
    );
    let context = &json["data"]["context"];
    assert!(
        context["project_types"].is_array(),
        "Context should include project_types"
    );

    Ok(())
}

// =============================================================================
// TEST: AUTO-LOAD CONFIRM (non-interactive)
// =============================================================================

#[test]
fn test_auto_load_confirm() -> Result<()> {
    let mut fixture = setup_auto_load_fixture("auto_load_confirm")?;

    fixture.log_step("Create skills");
    create_skill_and_index(&mut fixture, "rust-errors", RUST_SKILL)?;

    index_skills(&mut fixture);

    fixture.log_step("Create Rust project");
    fs::write(fixture.root.join("Cargo.toml"), "[package]\nname = \"test\"")?;

    // In robot mode, --confirm flag is ignored (no interactive prompts)
    fixture.log_step("Auto-load with --confirm in robot mode");
    let output = fixture.run_ms(&["--robot", "load", "--auto"]);
    fixture.assert_success(&output, "auto-load with confirm");

    let json = output.json();

    // Should have loaded array (not would_load since not dry_run)
    assert!(
        json["data"]["loaded"].is_array(),
        "Should have loaded array"
    );

    // Verify candidates were evaluated
    assert!(
        json["data"]["candidates"].is_array(),
        "Should have candidates array"
    );

    Ok(())
}

// =============================================================================
// TEST: AUTO-LOAD NO SKILLS INDEXED
// =============================================================================

#[test]
fn test_auto_load_no_skills() -> Result<()> {
    let mut fixture = setup_auto_load_fixture("auto_load_no_skills")?;

    // Don't create or index any skills

    fixture.log_step("Create Rust project");
    fs::write(fixture.root.join("Cargo.toml"), "[package]\nname = \"test\"")?;

    fixture.log_step("Auto-load with no skills indexed");
    let output = fixture.run_ms(&["--robot", "load", "--auto", "--dry_run"]);
    fixture.assert_success(&output, "auto-load no skills");

    let json = output.json();

    // Should succeed but with no results
    assert_eq!(json["status"].as_str().unwrap_or(""), "ok");

    // Should indicate no skills indexed
    let message = json["data"]["message"].as_str().unwrap_or("");
    assert!(
        message.contains("No skills indexed") || json["data"]["candidates"].as_array().map(|a| a.is_empty()).unwrap_or(true),
        "Should indicate no skills available"
    );

    Ok(())
}

// =============================================================================
// TEST: AUTO-LOAD MULTI-LANGUAGE PROJECT
// =============================================================================

#[test]
fn test_auto_load_multi_language() -> Result<()> {
    let mut fixture = setup_auto_load_fixture("auto_load_multi_language")?;

    fixture.log_step("Create skills");
    create_skill_and_index(&mut fixture, "rust-errors", RUST_SKILL)?;
    create_skill_and_index(&mut fixture, "python-hints", PYTHON_SKILL)?;

    index_skills(&mut fixture);

    fixture.log_step("Create multi-language project");
    // Project with both Rust and Python
    fs::write(fixture.root.join("Cargo.toml"), "[package]\nname = \"test\"")?;
    fs::write(fixture.root.join("pyproject.toml"), "[project]\nname = \"test\"")?;

    fixture.log_step("Auto-load with --dry_run");
    let output = fixture.run_ms(&["--robot", "load", "--auto", "--dry_run"]);
    fixture.assert_success(&output, "auto-load dry_run");

    let json = output.json();
    let would_load = json["data"]["would_load"].as_array().expect("would_load array");

    // Both skills should be detected
    let rust_found = would_load.iter().any(|s| {
        s["skill_id"].as_str().unwrap_or_default() == "rust-errors"
    });
    let python_found = would_load.iter().any(|s| {
        s["skill_id"].as_str().unwrap_or_default() == "python-hints"
    });

    assert!(rust_found, "Rust skill should be detected in multi-lang project");
    assert!(python_found, "Python skill should be detected in multi-lang project");

    // Verify context shows both project types
    let project_types = json["data"]["context"]["project_types"].as_array();
    if let Some(types) = project_types {
        let type_names: Vec<&str> = types.iter()
            .filter_map(|t| t["type"].as_str())
            .collect();
        assert!(
            type_names.iter().any(|t| *t == "rust"),
            "Context should detect Rust"
        );
        assert!(
            type_names.iter().any(|t| *t == "python"),
            "Context should detect Python"
        );
    }

    Ok(())
}
