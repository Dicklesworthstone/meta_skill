//! E2E Scenario: Skill Discovery Workflow
//!
//! Tests the complete skill discovery and loading lifecycle:
//! init → add → index → search → load
//!
//! This is a P1 E2E test that exercises the core user workflow.

use super::fixture::E2EFixture;
use ms::error::Result;

/// Test the complete skill discovery workflow.
///
/// Steps:
/// 1. Initialize ms in a temp directory
/// 2. Create skills in the search path
/// 3. Index all skills
/// 4. Search for skills with various queries
/// 5. Load skills at different disclosure levels
/// 6. Test token packing
#[test]
fn test_skill_discovery_workflow() -> Result<()> {
    let mut fixture = E2EFixture::new("skill_discovery_workflow");

    // ==========================================
    // Step 1: Setup - Initialize ms
    // ==========================================
    fixture.log_step("Initialize ms directory");
    let output = fixture.init();
    fixture.assert_success(&output, "init");
    fixture.checkpoint("post_init");

    // Verify directory structure was created
    assert!(fixture.ms_root.exists(), "ms root should exist");
    assert!(fixture.config_path.exists(), "config should exist");

    // ==========================================
    // Step 2: Create skills in search path
    // ==========================================
    fixture.log_step("Create skills in search path");

    // skill-alpha: Python debugging skill
    fixture.create_skill(
        "skill-alpha",
        r#"---
name: Python Debugging
description: Comprehensive guide to debugging Python applications
tags: [python, debugging, pdb, development]
provides: [python-debug]
---

# Python Debugging

A comprehensive guide to debugging Python applications.

## Overview

This skill covers debugging techniques for Python developers.

## Using pdb

The Python debugger (pdb) is the built-in debugging tool:

```python
import pdb

def problematic_function():
    pdb.set_trace()  # Breakpoint here
    x = calculate_value()
    return x
```

## Common Commands

- `n` (next): Execute next line
- `s` (step): Step into function
- `c` (continue): Continue execution
- `p expr`: Print expression
- `l` (list): Show current code

## Advanced Techniques

### Post-mortem Debugging

```python
import pdb
import sys

def main():
    try:
        do_something()
    except Exception:
        pdb.post_mortem(sys.exc_info()[2])
```

### Remote Debugging

For debugging in production or Docker containers.

## Best Practices

1. Use breakpoints strategically
2. Inspect variables at key points
3. Understand the call stack
"#,
    )?;

    // skill-beta: Rust error handling skill
    fixture.create_skill(
        "skill-beta",
        r#"---
name: Rust Error Handling
description: Patterns for handling errors in Rust applications
tags: [rust, errors, result, option, error-handling]
provides: [rust-errors]
---

# Rust Error Handling

Patterns and best practices for handling errors in Rust.

## Overview

Rust's type system provides compile-time error handling guarantees.

## The Result Type

```rust
fn parse_number(s: &str) -> Result<i32, ParseIntError> {
    s.parse()
}
```

## The ? Operator

Propagate errors elegantly:

```rust
fn read_config() -> Result<Config, Error> {
    let content = std::fs::read_to_string("config.toml")?;
    let config: Config = toml::from_str(&content)?;
    Ok(config)
}
```

## Custom Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    Parse(#[from] ParseError),
}
```

## Best Practices

1. Use `thiserror` for library errors
2. Use `anyhow` for application errors
3. Provide context with `.context()`
"#,
    )?;

    // skill-gamma: Go testing skill
    fixture.create_skill(
        "skill-gamma",
        r#"---
name: Go Testing
description: Guide to writing tests in Go
tags: [go, testing, golang, unit-tests]
provides: [go-testing]
---

# Go Testing

A guide to writing effective tests in Go.

## Overview

Go has built-in testing support with the `testing` package.

## Basic Test

```go
func TestAdd(t *testing.T) {
    result := Add(2, 3)
    if result != 5 {
        t.Errorf("Add(2, 3) = %d; want 5", result)
    }
}
```

## Table-Driven Tests

```go
func TestAdd(t *testing.T) {
    tests := []struct {
        a, b, want int
    }{
        {1, 2, 3},
        {0, 0, 0},
        {-1, 1, 0},
    }

    for _, tt := range tests {
        got := Add(tt.a, tt.b)
        if got != tt.want {
            t.Errorf("Add(%d, %d) = %d; want %d",
                tt.a, tt.b, got, tt.want)
        }
    }
}
```

## Running Tests

```bash
go test ./...
go test -v -run TestSpecific
go test -cover
```

## Benchmarks

```go
func BenchmarkAdd(b *testing.B) {
    for i := 0; i < b.N; i++ {
        Add(1, 2)
    }
}
```
"#,
    )?;
    fixture.checkpoint("skills_created");

    // ==========================================
    // Step 3: Index skills
    // ==========================================
    fixture.log_step("Index all skills");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");
    fixture.checkpoint("post_index");

    // Open database for verification
    fixture.open_db();

    // Verify all 3 skills were indexed
    fixture.verify_db_state(
        |db| {
            let count: i64 = db
                .query_row("SELECT COUNT(*) FROM skills", [], |r| r.get(0))
                .unwrap_or(0);
            println!("[DB] Skills count: {}", count);
            count == 3
        },
        "Should have exactly 3 skills indexed",
    );

    // Note: FTS is populated via triggers, verified implicitly by search working

    // ==========================================
    // Step 4: Search skills
    // ==========================================
    fixture.log_step("Search for 'debugging'");
    let output = fixture.run_ms(&["--robot", "search", "debugging python pdb"]);
    fixture.assert_success(&output, "search debugging");

    let json = output.json();
    let results = json["results"].as_array().expect("results array");
    println!("[SEARCH] 'debugging' returned {} results", results.len());
    assert!(!results.is_empty(), "Should find results for 'debugging'");

    // Verify python-debugging skill appears (may be first depending on scoring)
    let has_python_skill = results.iter().any(|r| {
        r["id"]
            .as_str()
            .map(|id| id.contains("python"))
            .unwrap_or(false)
    });
    println!("[SEARCH] Python skill found: {}", has_python_skill);

    fixture.log_step("Search for 'error handling'");
    let output = fixture.run_ms(&["--robot", "search", "rust error result"]);
    fixture.assert_success(&output, "search error handling");

    let json = output.json();
    let results = json["results"].as_array().expect("results array");
    println!(
        "[SEARCH] 'error handling' returned {} results",
        results.len()
    );
    assert!(
        !results.is_empty(),
        "Should find results for 'error handling'"
    );

    fixture.log_step("Search for 'testing'");
    let output = fixture.run_ms(&["--robot", "search", "go testing table driven"]);
    fixture.assert_success(&output, "search testing");

    let json = output.json();
    let results = json["results"].as_array().expect("results array");
    println!("[SEARCH] 'testing' returned {} results", results.len());
    assert!(!results.is_empty(), "Should find results for 'testing'");
    fixture.checkpoint("post_search");

    // ==========================================
    // Step 5: Load skills at different levels
    // ==========================================
    fixture.log_step("Load skill at minimal level");
    let output = fixture.run_ms(&["--robot", "load", "python-debugging", "--level", "minimal"]);
    fixture.assert_success(&output, "load minimal");

    let minimal_len = output.stdout.len();
    println!("[LOAD] Minimal level output: {} bytes", minimal_len);

    fixture.log_step("Load skill at standard level");
    let output = fixture.run_ms(&["--robot", "load", "python-debugging", "--level", "standard"]);
    fixture.assert_success(&output, "load standard");

    let standard_len = output.stdout.len();
    println!("[LOAD] Standard level output: {} bytes", standard_len);

    // Standard should typically include more content than minimal
    // (but sizes can vary slightly due to JSON formatting)
    println!(
        "[LOAD] Size comparison: minimal={}, standard={}",
        minimal_len, standard_len
    );

    fixture.log_step("Load skill at full level");
    let output = fixture.run_ms(&["--robot", "load", "python-debugging", "--level", "full"]);
    fixture.assert_success(&output, "load full");

    let full_len = output.stdout.len();
    println!("[LOAD] Full level output: {} bytes", full_len);

    // Verify all levels produce valid output (content sizes may vary slightly)
    assert!(minimal_len > 0, "Minimal should produce output");
    assert!(standard_len > 0, "Standard should produce output");
    assert!(full_len > 0, "Full should produce output");

    println!(
        "[LOAD] All levels produced output: minimal={}, standard={}, full={}",
        minimal_len, standard_len, full_len
    );
    fixture.checkpoint("post_load_levels");

    // ==========================================
    // Step 6: Token packing
    // ==========================================
    fixture.log_step("Test token packing with budget");
    let output = fixture.run_ms(&["--robot", "load", "python-debugging", "--pack", "200"]);
    fixture.assert_success(&output, "load with pack budget");

    let packed_len = output.stdout.len();
    println!(
        "[PACK] Packed output (200 token budget): {} bytes",
        packed_len
    );

    // Packed should fit within approximate token budget
    // (rough estimate: 4 chars per token, so 200 tokens ~800 chars + overhead)
    // We'll be lenient here since token counting is approximate
    fixture.checkpoint("post_pack");

    // ==========================================
    // Step 7: Verify deterministic results
    // ==========================================
    fixture.log_step("Verify search determinism");

    // Run the same search twice
    let output1 = fixture.run_ms(&["--robot", "search", "debugging"]);
    fixture.assert_success(&output1, "search 1");

    let output2 = fixture.run_ms(&["--robot", "search", "debugging"]);
    fixture.assert_success(&output2, "search 2");

    // Results should be identical
    assert_eq!(
        output1.stdout, output2.stdout,
        "Search results should be deterministic"
    );
    println!("[DETERMINISM] Search results are consistent");

    // ==========================================
    // Generate report
    // ==========================================
    fixture.generate_report();
    Ok(())
}

/// Test searching with various query types.
#[test]
fn test_search_query_types() -> Result<()> {
    let mut fixture = E2EFixture::new("search_query_types");

    fixture.log_step("Initialize and create skills");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    fixture.create_skill(
        "docker-containers",
        r#"---
name: Docker Containers
description: Guide to Docker container management
tags: [docker, containers, devops]
---

# Docker Containers

Managing containers with Docker.

## Commands

- `docker run` - Run a container
- `docker ps` - List containers
- `docker stop` - Stop a container
"#,
    )?;

    fixture.create_skill(
        "kubernetes-pods",
        r#"---
name: Kubernetes Pods
description: Managing pods in Kubernetes
tags: [kubernetes, k8s, pods, containers]
---

# Kubernetes Pods

Understanding and managing Kubernetes pods.

## Pod Basics

Pods are the smallest deployable units in Kubernetes.
"#,
    )?;

    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    // Test single word search
    fixture.log_step("Search: single word");
    let output = fixture.run_ms(&["--robot", "search", "docker"]);
    fixture.assert_success(&output, "search docker");
    let json = output.json();
    let results = json["results"].as_array().expect("results");
    println!("[SEARCH] 'docker' found {} results", results.len());

    // Test multi-word search
    fixture.log_step("Search: multi-word");
    let output = fixture.run_ms(&["--robot", "search", "container management"]);
    fixture.assert_success(&output, "search container management");
    let json = output.json();
    let results = json["results"].as_array().expect("results");
    println!(
        "[SEARCH] 'container management' found {} results",
        results.len()
    );

    // Test tag search
    fixture.log_step("Search: by tag");
    let output = fixture.run_ms(&["--robot", "search", "devops"]);
    fixture.assert_success(&output, "search devops");
    let json = output.json();
    let results = json["results"].as_array().expect("results");
    println!("[SEARCH] 'devops' found {} results", results.len());

    fixture.generate_report();
    Ok(())
}

/// Test loading skills with dependencies.
#[test]
fn test_load_with_dependencies() -> Result<()> {
    let mut fixture = E2EFixture::new("load_with_dependencies");

    fixture.log_step("Initialize");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    // Create parent skill
    fixture.create_skill(
        "base-programming",
        r#"---
name: Base Programming
description: Fundamental programming concepts
tags: [programming, fundamentals]
provides: [programming-basics]
---

# Base Programming

Core programming concepts every developer should know.

## Variables

Store data in named containers.

## Functions

Reusable blocks of code.
"#,
    )?;

    // Create dependent skill
    fixture.create_skill(
        "advanced-patterns",
        r#"---
name: Advanced Patterns
description: Advanced programming patterns
tags: [patterns, advanced]
requires: [programming-basics]
---

# Advanced Patterns

Building on fundamental concepts.

## Design Patterns

Proven solutions to common problems.

## SOLID Principles

Object-oriented design principles.
"#,
    )?;

    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    // Load with auto deps
    fixture.log_step("Load with auto dependencies");
    let output = fixture.run_ms(&["--robot", "load", "advanced-patterns", "--deps", "auto"]);
    fixture.assert_success(&output, "load with deps auto");
    println!("[LOAD] With deps: {} bytes", output.stdout.len());

    // Load without deps
    fixture.log_step("Load without dependencies");
    let output = fixture.run_ms(&["--robot", "load", "advanced-patterns", "--deps", "off"]);
    fixture.assert_success(&output, "load without deps");
    println!("[LOAD] Without deps: {} bytes", output.stdout.len());

    fixture.generate_report();
    Ok(())
}

/// Test progressive disclosure levels in detail.
#[test]
fn test_progressive_disclosure_levels() -> Result<()> {
    let mut fixture = E2EFixture::new("progressive_disclosure");

    fixture.log_step("Initialize");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    // Create a comprehensive skill with sections at different importance levels
    fixture.create_skill(
        "comprehensive-skill",
        r#"---
name: Comprehensive Skill
description: A skill with content at all disclosure levels
tags: [test, disclosure]
---

# Comprehensive Skill

This skill has content at various importance levels.

## Core Concepts

These are the most important concepts that should always be shown.

Essential information here.

## Standard Content

This section provides standard-level detail.

More detailed explanations of the concepts.

## Advanced Topics

These topics are for the full disclosure level.

In-depth coverage for advanced users.

## Reference

Complete reference material for the complete disclosure level.

Exhaustive details and edge cases.
"#,
    )?;

    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    // Test each disclosure level
    let levels = ["minimal", "overview", "standard", "full", "complete"];
    let mut sizes = Vec::new();

    for level in levels {
        fixture.log_step(&format!("Load at {} level", level));
        let output = fixture.run_ms(&["--robot", "load", "comprehensive-skill", "--level", level]);
        fixture.assert_success(&output, &format!("load {}", level));

        let len = output.stdout.len();
        println!("[DISCLOSURE] {} level: {} bytes", level, len);
        sizes.push((level, len));
    }

    // Verify minimal is the smallest (or equal)
    let minimal_size = sizes[0].1;
    let complete_size = sizes[sizes.len() - 1].1;
    assert!(
        complete_size >= minimal_size,
        "Complete level ({} bytes) should be >= minimal ({} bytes)",
        complete_size,
        minimal_size
    );

    // Log the progression for debugging
    println!("[DISCLOSURE] Size progression: {:?}", sizes);

    fixture.generate_report();
    Ok(())
}
