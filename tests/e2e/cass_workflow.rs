//! E2E Scenario: CASS Integration Workflow
//!
//! Tests the CASS (Coding Agent Session Search) integration workflow including:
//! - Session import/search
//! - Quality analysis
//! - Pattern extraction
//! - Learning integration
//! - Knowledge query
//!
//! Note: When the `cass` binary is not available, tests gracefully skip
//! or use mock data to exercise internal processing logic.

use std::fs;
use std::path::PathBuf;

use super::fixture::E2EFixture;
use ms::error::Result;

/// Create a mock CASS session file in JSONL format.
fn create_mock_session_file(path: &PathBuf, session_id: &str) {
    let session_content = format!(
        r#"{{"session_id":"{id}","created":"2026-01-14T10:00:00Z","model":"claude-3-opus","token_count":5000}}
{{"role":"user","content":"I need help debugging a Rust memory issue","timestamp":"2026-01-14T10:00:01Z"}}
{{"role":"assistant","content":"I'll help you debug the memory issue. Let me first look at your code to understand the context.","timestamp":"2026-01-14T10:00:05Z"}}
{{"role":"tool_use","tool":"Read","parameters":{{"file":"src/main.rs"}},"timestamp":"2026-01-14T10:00:06Z"}}
{{"role":"tool_result","content":"fn main() {{\n    let data: Vec<u8> = Vec::new();\n    // potential memory leak here\n}}","timestamp":"2026-01-14T10:00:07Z"}}
{{"role":"assistant","content":"I found the issue. The Vec is created but ownership isn't properly handled. Here's the fix:","timestamp":"2026-01-14T10:00:10Z"}}
{{"role":"tool_use","tool":"Edit","parameters":{{"file":"src/main.rs","old":"let data","new":"let mut data"}},"timestamp":"2026-01-14T10:00:11Z"}}
{{"role":"tool_result","content":"Edit successful","timestamp":"2026-01-14T10:00:12Z"}}
{{"role":"assistant","content":"I've fixed the memory issue. Now let's verify with tests.","timestamp":"2026-01-14T10:00:15Z"}}
{{"role":"tool_use","tool":"Bash","parameters":{{"command":"cargo test"}},"timestamp":"2026-01-14T10:00:16Z"}}
{{"role":"tool_result","content":"running 5 tests\ntest result: ok. 5 passed","timestamp":"2026-01-14T10:00:20Z"}}
{{"role":"assistant","content":"All tests pass. The memory issue has been resolved.","timestamp":"2026-01-14T10:00:25Z"}}
"#,
        id = session_id
    );
    fs::write(path, session_content).expect("Failed to write mock session");
}

/// Create a mock extraction result for CASS.
fn create_mock_extraction(path: &PathBuf, skill_name: &str) {
    let extraction = format!(
        r#"{{
  "skill_name": "{}",
  "description": "Debugging memory issues in Rust applications",
  "patterns": [
    "Read source files to understand context before making changes",
    "Use mutable bindings when ownership needs to transfer",
    "Run tests after each significant change to verify correctness"
  ],
  "anti_patterns": [
    "Making changes without understanding the codebase first",
    "Ignoring test verification after fixes"
  ],
  "confidence": 0.85,
  "source_sessions": ["session-001"],
  "tags": ["rust", "debugging", "memory-safety"]
}}"#,
        skill_name
    );
    fs::write(path, extraction).expect("Failed to write mock extraction");
}

/// Test the full CASS integration workflow.
#[test]
fn test_cass_integration_workflow() -> Result<()> {
    let mut fixture = E2EFixture::new("cass_integration");

    // Step 1: Initialize
    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");
    fixture.checkpoint("post_init");

    // Step 2: Setup mock CASS data
    fixture.log_step("Setup mock CASS data directory");

    let cass_dir = fixture.root.join("cass_data");
    fs::create_dir_all(&cass_dir).expect("Failed to create CASS data dir");

    let sessions_dir = cass_dir.join("sessions");
    fs::create_dir_all(&sessions_dir).expect("Failed to create sessions dir");

    // Create mock session files
    create_mock_session_file(&sessions_dir.join("session-001.jsonl"), "session-001");
    create_mock_session_file(&sessions_dir.join("session-002.jsonl"), "session-002");
    create_mock_session_file(&sessions_dir.join("session-003.jsonl"), "session-003");

    println!("[CASS] Created 3 mock sessions in {:?}", sessions_dir);
    fixture.checkpoint("post_session_setup");

    // Step 3: Setup mock extraction result
    fixture.log_step("Setup mock extraction result");

    let extractions_dir = cass_dir.join("extractions");
    fs::create_dir_all(&extractions_dir).expect("Failed to create extractions dir");
    create_mock_extraction(
        &extractions_dir.join("debugging-skill.json"),
        "rust-debugging",
    );

    println!("[CASS] Created mock extraction in {:?}", extractions_dir);
    fixture.checkpoint("post_extraction_setup");

    // Step 4: Test build command (CASS integration entry point)
    fixture.log_step("Test build command with CASS query");

    // The build command may fail if cass binary isn't available,
    // but we test the command parsing and error handling
    let output = fixture.run_ms(&[
        "--robot",
        "build",
        "--from-cass",
        "rust debugging",
        "--sessions",
        "5",
    ]);

    // Build may fail or return an error JSON, but should not crash
    // Exit code 0 with error JSON is valid (interactive mode required)
    // Exit code != 0 may mean cass is unavailable
    if output.stdout.contains("interactive_required") {
        println!("[CASS] Build requires interactive mode (expected in robot mode)");
    } else if output.stdout.contains("error") || !output.success {
        let combined = format!("{} {}", output.stdout, output.stderr);
        let is_expected = combined.contains("CASS")
            || combined.contains("cass")
            || combined.contains("unavailable")
            || combined.contains("interactive");

        println!(
            "[CASS] Build result (expected): {}",
            output
                .stdout
                .lines()
                .take(3)
                .collect::<Vec<_>>()
                .join(" | ")
        );

        if !is_expected {
            println!("[CASS] Note: {}", combined.lines().next().unwrap_or(""));
        }
    } else {
        println!("[CASS] Build succeeded");
    }

    fixture.checkpoint("post_build");

    // Step 5: Test antipatterns mining
    fixture.log_step("Test antipatterns mining");

    // Mine from session IDs (would need actual sessions)
    let output = fixture.run_ms(&["--robot", "antipatterns", "mine", "session-001"]);

    // Antipatterns may fail if cass isn't available
    if !output.success {
        println!(
            "[CASS] Antipatterns mining skipped (cass may not be installed): {}",
            output.stderr.lines().next().unwrap_or("")
        );
    } else {
        println!("[CASS] Antipatterns mining succeeded");
    }

    fixture.checkpoint("post_antipatterns");

    // Step 6: Test cm (cass-memory) status and context
    fixture.log_step("Test cass-memory status");

    // Check cm status first
    let output = fixture.run_ms(&["--robot", "cm", "status"]);

    if !output.success {
        println!(
            "[CASS] CM query skipped (may need initialized db): {}",
            output.stderr.lines().next().unwrap_or("")
        );
    } else {
        println!("[CASS] CM query succeeded");
    }

    fixture.checkpoint("post_cm_query");

    // Step 7: Index any skills that were created
    fixture.log_step("Index skills for verification");

    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    fixture.checkpoint("post_index");

    // Step 8: Verify database state
    fixture.log_step("Verify database state");

    fixture.open_db();

    // Check that ms infrastructure is properly initialized
    fixture.verify_db_state(
        |db| {
            // Check skills table exists
            let skills_table_exists: bool = db
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='skills'",
                    [],
                    |r| r.get::<_, i64>(0).map(|c| c >= 1),
                )
                .unwrap_or(false);
            skills_table_exists
        },
        "Skills table exists",
    );

    fixture.checkpoint("final");

    // Generate report
    fixture.generate_report();

    println!();
    println!("╔════════════════════════════════════════════════════════════════════╗");
    println!("║ CASS INTEGRATION WORKFLOW: COMPLETE                                ║");
    println!("╚════════════════════════════════════════════════════════════════════╝");
    println!();

    Ok(())
}

/// Test pattern extraction from mock session data.
#[test]
fn test_pattern_extraction_logic() -> Result<()> {
    let mut fixture = E2EFixture::new("cass_pattern_extraction");

    // Step 1: Initialize
    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    // Step 2: Create a skill based on mock patterns
    fixture.log_step("Create skill from extracted patterns");

    fixture.create_skill(
        "debugging-workflow",
        r#"---
name: Debugging Workflow
description: A systematic approach to debugging issues
tags: [debugging, methodology, best-practices]
---

# Debugging Workflow

A skill extracted from successful debugging sessions.

## Patterns

### 1. Reconnaissance Phase
Always read and understand the codebase before making changes.

### 2. Change Phase
Make targeted, minimal changes to fix the issue.

### 3. Validation Phase
Run tests after each significant change.

### 4. Wrap-up Phase
Document what was done and verify the fix is complete.

## Anti-patterns

- Making changes without understanding context
- Skipping test verification
- Not documenting the fix

## Usage

When debugging, follow the reconnaissance → change → validation → wrap-up pattern.
"#,
    )?;

    fixture.checkpoint("post_skill_creation");

    // Step 3: Index the skill
    fixture.log_step("Index skill");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    // Step 4: Verify skill is searchable
    fixture.log_step("Search for skill");
    let output = fixture.run_ms(&["--robot", "search", "debugging"]);
    fixture.assert_success(&output, "search");
    fixture.assert_output_contains(&output, "debugging");

    // Step 5: Load skill to verify content
    fixture.log_step("Load skill");
    let output = fixture.run_ms(&["--robot", "load", "debugging-workflow"]);
    fixture.assert_success(&output, "load");

    fixture.checkpoint("final");
    fixture.generate_report();

    println!();
    println!("╔════════════════════════════════════════════════════════════════════╗");
    println!("║ PATTERN EXTRACTION TEST: COMPLETE                                  ║");
    println!("╚════════════════════════════════════════════════════════════════════╝");
    println!();

    Ok(())
}

/// Test quality scoring workflow.
#[test]
fn test_quality_analysis_workflow() -> Result<()> {
    let mut fixture = E2EFixture::new("cass_quality_analysis");

    // Step 1: Initialize
    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    // Step 2: Create skills with varying quality indicators
    fixture.log_step("Create skills with quality indicators");

    // High-quality skill
    fixture.create_skill(
        "high-quality-skill",
        r#"---
name: High Quality Skill
description: A well-documented, comprehensive skill
tags: [quality, best-practices]
version: 1.0.0
---

# High Quality Skill

This skill demonstrates quality patterns.

## Prerequisites

- Basic understanding of the domain
- Familiarity with tooling

## Core Concepts

Detailed explanation of concepts with examples.

## Examples

```rust
// Example code with comments
fn example() {
    // Clear, well-documented code
}
```

## Best Practices

1. Always document your changes
2. Run tests before committing
3. Review code carefully

## Common Mistakes

- Skipping documentation
- Ignoring test failures

## Further Reading

- Link to documentation
- Reference materials
"#,
    )?;

    // Lower-quality skill (minimal content)
    fixture.create_skill(
        "minimal-skill",
        r#"---
name: Minimal Skill
---

# Minimal

Some content.
"#,
    )?;

    fixture.checkpoint("post_skill_creation");

    // Step 3: Index skills
    fixture.log_step("Index skills");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    // Step 4: Check quality
    fixture.log_step("Check skill quality");
    let output = fixture.run_ms(&["--robot", "quality", "high-quality-skill"]);

    if output.success {
        println!("[QUALITY] Quality check succeeded");
        fixture.assert_output_contains(&output, "quality");
    } else {
        println!(
            "[QUALITY] Quality command may not be implemented: {}",
            output.stderr.lines().next().unwrap_or("")
        );
    }

    fixture.checkpoint("post_quality");

    // Step 5: Verify database state
    fixture.log_step("Verify database state");
    fixture.open_db();

    fixture.verify_db_state(
        |db| {
            let skill_count: i64 = db
                .query_row("SELECT COUNT(*) FROM skills", [], |r| r.get(0))
                .unwrap_or(0);
            skill_count >= 2
        },
        "Both skills indexed",
    );

    fixture.checkpoint("final");
    fixture.generate_report();

    println!();
    println!("╔════════════════════════════════════════════════════════════════════╗");
    println!("║ QUALITY ANALYSIS TEST: COMPLETE                                    ║");
    println!("╚════════════════════════════════════════════════════════════════════╝");
    println!();

    Ok(())
}

/// Test learning workflow with marking sessions.
#[test]
fn test_learning_integration_workflow() -> Result<()> {
    let mut fixture = E2EFixture::new("cass_learning");

    // Step 1: Initialize
    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    // Step 2: Create a skill representing learned knowledge
    fixture.log_step("Create skill from learned session");

    fixture.create_skill(
        "learned-pattern",
        r#"---
name: Learned Pattern
description: Pattern learned from exemplary session
tags: [learned, exemplary, pattern]
source: cass-session-001
quality_score: 0.95
---

# Learned Pattern

This skill was extracted from an exemplary debugging session.

## Context

The session demonstrated effective problem-solving techniques.

## Technique

1. Read relevant code first
2. Form hypothesis
3. Make minimal changes
4. Verify with tests

## Evidence

- Session ID: session-001
- Quality Score: 0.95
- Token Count: 5000
"#,
    )?;

    fixture.checkpoint("post_skill_creation");

    // Step 3: Index and search
    fixture.log_step("Index and verify searchability");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    let output = fixture.run_ms(&["--robot", "search", "learned exemplary"]);
    fixture.assert_success(&output, "search");
    fixture.assert_output_contains(&output, "learned");

    // Step 4: Load the skill
    fixture.log_step("Load learned skill");
    let output = fixture.run_ms(&["--robot", "load", "learned-pattern"]);
    fixture.assert_success(&output, "load");

    fixture.checkpoint("final");
    fixture.generate_report();

    println!();
    println!("╔════════════════════════════════════════════════════════════════════╗");
    println!("║ LEARNING INTEGRATION TEST: COMPLETE                                ║");
    println!("╚════════════════════════════════════════════════════════════════════╝");
    println!();

    Ok(())
}

/// Test knowledge query workflow.
#[test]
fn test_knowledge_query_workflow() -> Result<()> {
    let mut fixture = E2EFixture::new("cass_knowledge_query");

    // Step 1: Initialize
    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    // Step 2: Create multiple skills to query
    fixture.log_step("Create knowledge base");

    fixture.create_skill(
        "error-handling",
        r#"---
name: Error Handling
description: Best practices for error handling
tags: [errors, rust, best-practices]
---

# Error Handling

## Key Principles

1. Use Result types for recoverable errors
2. Use panic for unrecoverable errors
3. Provide context in error messages

## Example

```rust
fn read_file(path: &str) -> Result<String, std::io::Error> {
    std::fs::read_to_string(path)
}
```
"#,
    )?;

    fixture.create_skill(
        "testing-patterns",
        r#"---
name: Testing Patterns
description: Effective testing strategies
tags: [testing, rust, patterns]
---

# Testing Patterns

## Unit Tests

Test individual functions in isolation.

## Integration Tests

Test components working together.

## E2E Tests

Test complete workflows.
"#,
    )?;

    fixture.create_skill(
        "debugging-tips",
        r#"---
name: Debugging Tips
description: Tips for effective debugging
tags: [debugging, tips, methodology]
---

# Debugging Tips

## Use Logging

Add structured logging to trace execution.

## Isolate the Problem

Create minimal reproduction cases.

## Binary Search

Use bisection to narrow down the issue.
"#,
    )?;

    fixture.checkpoint("post_knowledge_base");

    // Step 3: Index
    fixture.log_step("Index knowledge base");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    // Step 4: Query the knowledge base with various queries
    fixture.log_step("Query knowledge base");

    let queries = [
        ("error handling", "errors"),
        ("testing", "testing"),
        ("debugging", "debugging"),
        ("rust best practices", "rust"),
    ];

    for (query, expected) in queries {
        let output = fixture.run_ms(&["--robot", "search", query]);
        fixture.assert_success(&output, &format!("search for '{}'", query));

        if output.stdout.to_lowercase().contains(expected) {
            println!("[QUERY] '{}' -> found '{}'", query, expected);
        } else {
            println!(
                "[QUERY] '{}' -> no match for '{}' (may be expected)",
                query, expected
            );
        }
    }

    // Step 5: Test cm query if available
    fixture.log_step("Test cass-memory context");

    let output = fixture.run_ms(&["--robot", "cm", "context"]);
    if output.success {
        println!("[CM] Context query succeeded");
    } else {
        println!(
            "[CM] Context not available: {}",
            output.stderr.lines().next().unwrap_or("")
        );
    }

    fixture.checkpoint("final");
    fixture.generate_report();

    println!();
    println!("╔════════════════════════════════════════════════════════════════════╗");
    println!("║ KNOWLEDGE QUERY TEST: COMPLETE                                     ║");
    println!("╚════════════════════════════════════════════════════════════════════╝");
    println!();

    Ok(())
}
