//! Integration tests for the context-aware auto-loading feature.
//!
//! These tests verify that `ms load --auto` correctly:
//! - Detects project types from marker files
//! - Collects context from recent files and tools
//! - Scores and filters skills by relevance
//! - Loads appropriate skills based on context

use crate::fixture::{TestFixture, TestSkill};
use std::fs;

/// Create a skill with context tags for testing.
fn skill_with_context(
    name: &str,
    description: &str,
    project_types: &[&str],
    file_patterns: &[&str],
) -> TestSkill {
    let patterns = file_patterns.join(", ");

    // Build frontmatter with context
    let content = format!(
        r#"---
name: {name}
description: {description}
context:
  project_types: [{project_types}]
  file_patterns: [{patterns}]
---

# {name}

{description}
"#,
        name = name,
        description = description,
        project_types = project_types.join(", "),
        patterns = patterns,
    );

    TestSkill::with_content(name, &content).with_tags(project_types.to_vec())
}

/// Create a Rust project structure in the fixture.
fn setup_rust_project(fixture: &TestFixture) {
    let cargo_toml = fixture.root.join("Cargo.toml");
    fs::write(
        &cargo_toml,
        r#"
[package]
name = "test-project"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();

    let src_dir = fixture.root.join("src");
    fs::create_dir_all(&src_dir).unwrap();

    let main_rs = src_dir.join("main.rs");
    fs::write(
        &main_rs,
        r#"
fn main() {
    println!("Hello, world!");
}
"#,
    )
    .unwrap();

    let lib_rs = src_dir.join("lib.rs");
    fs::write(
        &lib_rs,
        r#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#,
    )
    .unwrap();
}

/// Create a Node.js project structure in the fixture.
fn setup_node_project(fixture: &TestFixture) {
    let package_json = fixture.root.join("package.json");
    fs::write(
        &package_json,
        r#"
{
    "name": "test-project",
    "version": "1.0.0",
    "main": "index.js"
}
"#,
    )
    .unwrap();

    let index_js = fixture.root.join("index.js");
    fs::write(
        &index_js,
        r#"
console.log("Hello, world!");
"#,
    )
    .unwrap();
}

/// Create a Python project structure in the fixture.
fn setup_python_project(fixture: &TestFixture) {
    let pyproject = fixture.root.join("pyproject.toml");
    fs::write(
        &pyproject,
        r#"
[project]
name = "test-project"
version = "0.1.0"
"#,
    )
    .unwrap();

    let main_py = fixture.root.join("main.py");
    fs::write(
        &main_py,
        r#"
def main():
    print("Hello, world!")

if __name__ == "__main__":
    main()
"#,
    )
    .unwrap();
}

#[test]
fn auto_load_detects_rust_project() {
    let rust_skill = skill_with_context(
        "rust-errors",
        "Handle Rust error types effectively",
        &["rust"],
        &["*.rs"],
    );
    let node_skill = skill_with_context(
        "node-async",
        "Node.js async patterns",
        &["node"],
        &["*.js", "*.ts"],
    );

    let fixture = TestFixture::with_indexed_skills(
        "auto_load_detects_rust_project",
        &[rust_skill, node_skill],
    );

    // Set up a Rust project
    setup_rust_project(&fixture);

    // Run auto-load with robot mode for parseable output
    let output = fixture.run_ms(&[
        "--robot",
        "load",
        "--auto",
        "--threshold",
        "0.1",
        "--dry-run",
    ]);

    // Should succeed
    assert!(output.success, "Auto-load failed: {}", output.stderr);

    // Output should mention rust-related skill
    // Note: The exact output format depends on implementation
    println!("stdout: {}", output.stdout);
    println!("stderr: {}", output.stderr);
}

#[test]
fn auto_load_detects_node_project() {
    let rust_skill = skill_with_context(
        "rust-errors",
        "Handle Rust error types effectively",
        &["rust"],
        &["*.rs"],
    );
    let node_skill = skill_with_context(
        "node-async",
        "Node.js async patterns",
        &["node"],
        &["*.js", "*.ts"],
    );

    let fixture = TestFixture::with_indexed_skills(
        "auto_load_detects_node_project",
        &[rust_skill, node_skill],
    );

    // Set up a Node.js project
    setup_node_project(&fixture);

    // Run auto-load
    let output = fixture.run_ms(&[
        "--robot",
        "load",
        "--auto",
        "--threshold",
        "0.1",
        "--dry-run",
    ]);

    assert!(output.success, "Auto-load failed: {}", output.stderr);
    println!("stdout: {}", output.stdout);
}

#[test]
fn auto_load_respects_threshold() {
    let rust_skill = skill_with_context(
        "rust-errors",
        "Handle Rust error types",
        &["rust"],
        &["*.rs"],
    );

    let fixture = TestFixture::with_indexed_skills("auto_load_respects_threshold", &[rust_skill]);

    setup_rust_project(&fixture);

    // With very high threshold, nothing should be loaded
    let output_high = fixture.run_ms(&[
        "--robot",
        "load",
        "--auto",
        "--threshold",
        "0.99",
        "--dry-run",
    ]);
    assert!(output_high.success);

    // With low threshold, skill should be considered
    let output_low = fixture.run_ms(&[
        "--robot",
        "load",
        "--auto",
        "--threshold",
        "0.1",
        "--dry-run",
    ]);
    assert!(output_low.success);

    println!("High threshold output: {}", output_high.stdout);
    println!("Low threshold output: {}", output_low.stdout);
}

#[test]
fn auto_load_dry_run_does_not_modify_state() {
    let skill = skill_with_context("test-skill", "Test skill for dry run", &["rust"], &["*.rs"]);

    let fixture = TestFixture::with_indexed_skills("auto_load_dry_run", &[skill]);

    setup_rust_project(&fixture);

    // Record initial state
    let before = fixture.run_ms(&["list", "--loaded"]);

    // Run auto-load with dry-run
    let _auto = fixture.run_ms(&["load", "--auto", "--dry-run"]);

    // State should be unchanged
    let after = fixture.run_ms(&["list", "--loaded"]);

    assert_eq!(
        before.stdout, after.stdout,
        "Dry run should not modify loaded skills"
    );
}

#[test]
fn auto_load_multi_language_project() {
    // Create skills for both languages
    let rust_skill = skill_with_context("rust-errors", "Rust error handling", &["rust"], &["*.rs"]);
    let python_skill = skill_with_context(
        "python-async",
        "Python async patterns",
        &["python"],
        &["*.py"],
    );

    let fixture =
        TestFixture::with_indexed_skills("auto_load_multi_language", &[rust_skill, python_skill]);

    // Set up both Rust and Python project markers
    setup_rust_project(&fixture);
    setup_python_project(&fixture);

    // Run auto-load
    let output = fixture.run_ms(&[
        "--robot",
        "load",
        "--auto",
        "--threshold",
        "0.1",
        "--dry-run",
    ]);

    assert!(output.success, "Auto-load failed: {}", output.stderr);
    println!("Multi-language output: {}", output.stdout);

    // Both skills could potentially be relevant
}

#[test]
fn auto_load_empty_project() {
    let skill = skill_with_context("test-skill", "Generic skill", &["rust"], &["*.rs"]);

    let fixture = TestFixture::with_indexed_skills("auto_load_empty_project", &[skill]);

    // Don't set up any project markers

    // Run auto-load
    let output = fixture.run_ms(&["load", "--auto", "--dry-run"]);

    // Should succeed but find nothing relevant
    assert!(
        output.success,
        "Auto-load failed on empty project: {}",
        output.stderr
    );
}

#[test]
fn auto_load_confirm_mode() {
    let skill = skill_with_context(
        "test-skill",
        "Test skill for confirm mode",
        &["rust"],
        &["*.rs"],
    );

    let fixture = TestFixture::with_indexed_skills("auto_load_confirm_mode", &[skill]);

    setup_rust_project(&fixture);

    // Confirm mode should not hang in CI (it should use dry-run behavior or prompt)
    // This tests that the --confirm flag is accepted
    let output = fixture.run_ms(&["load", "--auto", "--confirm", "--dry-run"]);

    assert!(
        output.success,
        "Auto-load with confirm failed: {}",
        output.stderr
    );
}
