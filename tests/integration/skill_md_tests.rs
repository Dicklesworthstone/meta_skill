//! Integration tests for SKILL.md auto-generation.
//!
//! These tests verify the SKILL.md generation functionality works correctly
//! and produces valid markdown content.

use ms::skill_md::{SkillMdGenerator, generate_skill_md_for_project};
use tempfile::TempDir;

/// Test that SKILL.md is generated at the correct path.
#[test]
fn test_generate_skill_md_creates_file() {
    let temp = TempDir::new().unwrap();
    let result = generate_skill_md_for_project(temp.path());

    assert!(result.is_ok());
    let path = result.unwrap();
    assert_eq!(path, temp.path().join("SKILL.md"));
    assert!(path.exists());
}

/// Test that generated SKILL.md contains expected sections.
#[test]
fn test_generated_skill_md_has_sections() {
    let temp = TempDir::new().unwrap();
    generate_skill_md_for_project(temp.path()).unwrap();

    let content = std::fs::read_to_string(temp.path().join("SKILL.md")).unwrap();

    // Check required sections
    assert!(content.contains("# ms"), "Missing title");
    assert!(
        content.contains("## Capabilities"),
        "Missing Capabilities section"
    );
    assert!(
        content.contains("## MCP Server"),
        "Missing MCP Server section"
    );
    assert!(
        content.contains("## Context Integration"),
        "Missing Context section"
    );
    assert!(content.contains("## Examples"), "Missing Examples section");
}

/// Test that generated content includes MCP tools.
#[test]
fn test_generated_skill_md_has_mcp_tools() {
    let generator = SkillMdGenerator::new();
    let content = generator.generate();

    // Check for MCP tools
    assert!(
        content.contains("### Available MCP Tools"),
        "Missing MCP tools heading"
    );
    assert!(content.contains("`search`"), "Missing search tool");
    assert!(content.contains("`load`"), "Missing load tool");
    assert!(content.contains("`lint`"), "Missing lint tool");
    assert!(content.contains("`doctor`"), "Missing doctor tool");
}

/// Test that generated content includes CLI commands.
#[test]
fn test_generated_skill_md_has_cli_commands() {
    let generator = SkillMdGenerator::new();
    let content = generator.generate();

    // Check for CLI commands in Capabilities section
    assert!(content.contains("**search**"), "Missing search command");
    assert!(content.contains("**load**"), "Missing load command");
    assert!(content.contains("**suggest**"), "Missing suggest command");
    assert!(content.contains("**build**"), "Missing build command");
}

/// Test that generated content includes robot mode documentation.
#[test]
fn test_generated_skill_md_has_robot_mode() {
    let generator = SkillMdGenerator::new();
    let content = generator.generate();

    assert!(
        content.contains("### Robot Mode"),
        "Missing Robot Mode section"
    );
    assert!(content.contains("-O json"), "Missing robot mode flag");
}

/// Test that the version is embedded correctly.
#[test]
fn test_generated_skill_md_version() {
    let generator = SkillMdGenerator::new();
    let content = generator.generate();

    let version = env!("CARGO_PKG_VERSION");
    assert!(
        content.contains(&format!("Version: {}", version)),
        "Missing or incorrect version"
    );
}

/// Test that the file can be overwritten.
#[test]
fn test_generate_skill_md_overwrites_existing() {
    let temp = TempDir::new().unwrap();
    let skill_md_path = temp.path().join("SKILL.md");

    // Create an existing file
    std::fs::write(&skill_md_path, "old content").unwrap();

    // Generate new SKILL.md
    generate_skill_md_for_project(temp.path()).unwrap();

    let content = std::fs::read_to_string(&skill_md_path).unwrap();
    assert!(
        !content.contains("old content"),
        "Old content should be replaced"
    );
    assert!(content.contains("# ms"), "Should contain new content");
}

/// Test that MCP tools list is not empty.
#[test]
fn test_skill_md_generator_has_tools() {
    let generator = SkillMdGenerator::new();

    let tools = generator.mcp_tools();
    assert!(!tools.is_empty(), "Should have MCP tools");
    assert!(tools.len() >= 7, "Should have at least 7 MCP tools");
}

/// Test that CLI commands list is not empty.
#[test]
fn test_skill_md_generator_has_commands() {
    let generator = SkillMdGenerator::new();

    let commands = generator.commands();
    assert!(!commands.is_empty(), "Should have CLI commands");
    assert!(commands.len() >= 8, "Should have at least 8 CLI commands");
}

/// Test that MCP server startup examples are present.
#[test]
fn test_generated_skill_md_has_mcp_startup() {
    let generator = SkillMdGenerator::new();
    let content = generator.generate();

    assert!(
        content.contains("ms mcp serve"),
        "Missing MCP serve command"
    );
    assert!(
        content.contains("stdio transport"),
        "Missing stdio transport docs"
    );
}

/// Test that example commands are present and valid.
#[test]
fn test_generated_skill_md_has_examples() {
    let generator = SkillMdGenerator::new();
    let content = generator.generate();

    assert!(content.contains("ms search"), "Missing search example");
    assert!(content.contains("ms load"), "Missing load example");
    assert!(content.contains("ms suggest"), "Missing suggest example");
    assert!(content.contains("ms lint"), "Missing lint example");
}

/// Test that generated markdown is valid structure.
#[test]
fn test_generated_skill_md_valid_structure() {
    let generator = SkillMdGenerator::new();
    let content = generator.generate();

    // Should start with h1
    assert!(content.starts_with("# "), "Should start with h1 header");

    // Should have proper section hierarchy
    let h2_count = content.matches("\n## ").count();
    assert!(h2_count >= 4, "Should have at least 4 h2 sections");

    // Should have code blocks
    assert!(content.contains("```bash"), "Should have bash code blocks");
    assert!(content.contains("```"), "Should have closing code blocks");
}

/// Test that Default trait is implemented.
#[test]
fn test_skill_md_generator_default() {
    let generator = SkillMdGenerator::default();
    assert!(!generator.version().is_empty());
}
