//! E2E Scenario: Fresh Install to Search Workflow
//!
//! Tests the complete workflow from initializing a fresh installation
//! through indexing skills and searching for them.

use super::fixture::E2EFixture;
use ms::error::Result;

/// Test the complete fresh install workflow.
#[test]
fn test_fresh_install_to_search() -> Result<()> {
    let mut fixture = E2EFixture::new("fresh_install_to_search");

    // Step 1: Initialize fresh installation
    fixture.log_step("Initialize fresh installation");
    let output = fixture.init();
    fixture.assert_success(&output, "init");
    fixture.checkpoint("post_init");

    // Step 2: Create test skills
    fixture.log_step("Create test skills");
    fixture.create_skill(
        "rust-patterns",
        r#"---
name: Rust Patterns
description: Common Rust design patterns and idioms
tags: [rust, patterns, design]
---

# Rust Patterns

Common patterns for Rust development including error handling,
builder pattern, type state, and newtype pattern.

## Overview

This skill covers essential Rust patterns that every developer should know.

## Error Handling

Use the `?` operator for propagating errors:

```rust
fn read_config() -> Result<Config, Error> {
    let content = std::fs::read_to_string("config.toml")?;
    let config = toml::from_str(&content)?;
    Ok(config)
}
```

## Builder Pattern

For complex struct construction:

```rust
struct Server {
    host: String,
    port: u16,
}

impl Server {
    fn builder() -> ServerBuilder {
        ServerBuilder::default()
    }
}
```
"#,
    )?;

    fixture.create_skill(
        "async-rust",
        r#"---
name: Async Rust
description: Asynchronous programming in Rust
tags: [rust, async, tokio]
---

# Async Rust

Guide to asynchronous programming in Rust using async/await.

## Overview

Modern Rust applications often need to handle concurrent operations.

## Basic Async Function

```rust
async fn fetch_data(url: &str) -> Result<String, Error> {
    let response = reqwest::get(url).await?;
    let body = response.text().await?;
    Ok(body)
}
```

## Spawning Tasks

```rust
tokio::spawn(async move {
    process_item(item).await;
});
```
"#,
    )?;
    fixture.checkpoint("skills_created");

    // Step 3: Index skills
    fixture.log_step("Index skills");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    let json = output.json();
    assert_eq!(json["status"], "ok", "Index should return status ok");
    fixture.checkpoint("post_index");

    // Step 4: List skills
    fixture.log_step("List indexed skills");
    let output = fixture.run_ms(&["--robot", "list"]);
    fixture.assert_success(&output, "list");

    let json = output.json();
    let skills = json["skills"].as_array().expect("skills should be array");
    assert!(skills.len() >= 2, "Should have at least 2 skills indexed");

    // Check both skills are present
    let skill_ids: Vec<&str> = skills.iter().filter_map(|s| s["id"].as_str()).collect();
    assert!(
        skill_ids.contains(&"rust-patterns"),
        "rust-patterns should be indexed"
    );
    assert!(
        skill_ids.contains(&"async-rust"),
        "async-rust should be indexed"
    );
    fixture.checkpoint("post_list");

    // Step 5: Search for skills
    fixture.log_step("Search for skills");
    let output = fixture.run_ms(&["--robot", "search", "rust patterns"]);
    fixture.assert_success(&output, "search");

    let json = output.json();
    assert_eq!(json["status"], "ok");
    let results = json["results"].as_array().expect("results should be array");
    assert!(!results.is_empty(), "Search should return results");

    // rust-patterns should be in results
    let found = results.iter().any(|r| r["id"] == "rust-patterns");
    assert!(found, "rust-patterns should be in search results");
    fixture.checkpoint("post_search");

    // Step 6: Load skill and verify output
    fixture.log_step("Load skill");
    let output = fixture.run_ms(&["--robot", "load", "rust-patterns"]);
    fixture.assert_success(&output, "load");
    // Just verify it succeeded - the load command returns skill body in JSON
    fixture.checkpoint("post_load");

    // Step 7: Verify database state
    fixture.log_step("Verify database state");
    fixture.open_db();
    fixture.verify_db_state(
        |db| {
            let count: i64 = db
                .query_row("SELECT COUNT(*) FROM skills", [], |r| r.get(0))
                .unwrap_or(0);
            count >= 2
        },
        "Should have at least 2 skills in database",
    );

    // Generate final report
    fixture.generate_report();
    Ok(())
}

/// Test that search with no results handles gracefully.
#[test]
fn test_search_no_results() -> Result<()> {
    let mut fixture = E2EFixture::new("search_no_results");

    fixture.log_step("Initialize");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    fixture.log_step("Create a skill");
    fixture.create_skill(
        "python-basics",
        r#"---
name: Python Basics
description: Basic Python programming
tags: [python, basics]
---

# Python Basics

Getting started with Python programming.
"#,
    )?;

    fixture.log_step("Index");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    fixture.log_step("Search for non-existent topic");
    let output = fixture.run_ms(&["--robot", "search", "fortran assembly"]);
    // Search should succeed even with no results
    fixture.assert_success(&output, "search");

    let json = output.json();
    let results = json["results"].as_array().expect("results should be array");
    assert!(
        results.is_empty(),
        "Should have no results for unrelated query"
    );

    fixture.generate_report();
    Ok(())
}

/// Test that load fails gracefully for missing skills.
#[test]
fn test_load_missing_skill() -> Result<()> {
    let mut fixture = E2EFixture::new("load_missing_skill");

    fixture.log_step("Initialize");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    fixture.log_step("Try to load non-existent skill");
    let output = fixture.run_ms(&["load", "nonexistent-skill-xyz"]);
    assert!(!output.success, "Load should fail for missing skill");

    fixture.generate_report();
    Ok(())
}

/// Every `ms setup` form that reaches `path` must fail and name the file and the reason.
fn assert_setup_refuses(fixture: &mut E2EFixture, reason: &str) {
    for mut args in [
        vec!["setup", "--claude-code"],
        vec!["setup", "--claude-code", "--force"],
        vec!["setup", "--claude-code", "--dry-run"],
    ] {
        let human = fixture.run_ms(&args);
        assert!(!human.success, "{args:?} must fail: {}", human.stdout);
        assert!(
            human.stdout.contains(reason),
            "{args:?} must say why: {}",
            human.stdout
        );

        args.insert(0, "--robot");
        let robot = fixture.run_ms(&args);
        assert!(!robot.success, "{args:?} must fail: {}", robot.stdout);
        let message = robot.json()["message"]
            .as_str()
            .expect("error message")
            .to_string();
        assert!(
            message.contains(".claude/config.json") && message.contains(reason),
            "{args:?} must name the file and why: {message}"
        );
    }
}

/// Run `ms setup` against a Claude Code config holding `content`, which it must not change.
fn assert_setup_leaves_config_alone(scenario: &str, content: &str, reason: &str) -> Result<()> {
    let mut fixture = E2EFixture::new(scenario);
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    let config_dir = fixture.root.join(".claude");
    std::fs::create_dir_all(&config_dir)?;
    let config_path = config_dir.join("config.json");
    std::fs::write(&config_path, content)?;

    assert_setup_refuses(&mut fixture, reason);
    assert_eq!(std::fs::read_to_string(&config_path)?, content);

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_setup_refuses_an_agent_config_with_comments() -> Result<()> {
    assert_setup_leaves_config_alone(
        "setup_config_with_comments",
        "{\n  // kept by hand\n  \"theme\": \"dark\"\n}\n",
        "is not valid JSON",
    )
}

#[test]
fn test_setup_refuses_an_agent_config_with_a_trailing_comma() -> Result<()> {
    assert_setup_leaves_config_alone(
        "setup_config_trailing_comma",
        "{\"theme\": \"dark\",}\n",
        "is not valid JSON",
    )
}

#[test]
fn test_setup_refuses_an_agent_config_that_is_not_an_object() -> Result<()> {
    assert_setup_leaves_config_alone(
        "setup_config_not_object",
        "[\"theme\", \"dark\"]\n",
        "is not a JSON object",
    )
}

#[test]
fn test_setup_refuses_an_agent_config_whose_servers_are_not_an_object() -> Result<()> {
    assert_setup_leaves_config_alone(
        "setup_config_servers_not_object",
        "{\"theme\": \"dark\", \"mcpServers\": [\"other\"]}\n",
        "mcpServers in",
    )
}

#[test]
fn test_setup_refuses_an_agent_config_it_cannot_read() -> Result<()> {
    let mut fixture = E2EFixture::new("setup_config_unreadable");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    // A directory at the path cannot be read, whichever user runs the test.
    let config_path = fixture.root.join(".claude/config.json");
    std::fs::create_dir_all(&config_path)?;
    let inside = config_path.join("keep.txt");
    std::fs::write(&inside, "keep")?;

    assert_setup_refuses(&mut fixture, "cannot read");
    assert_eq!(std::fs::read_to_string(&inside)?, "keep");

    fixture.generate_report();
    Ok(())
}

#[test]
fn test_setup_keeps_the_rest_of_an_agent_config() -> Result<()> {
    let mut fixture = E2EFixture::new("setup_config_merge");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    let config_dir = fixture.root.join(".claude");
    std::fs::create_dir_all(&config_dir)?;
    let config_path = config_dir.join("config.json");
    std::fs::write(
        &config_path,
        r#"{"theme": "dark", "mcpServers": {"other": {"command": "other-server"}}}"#,
    )?;

    let output = fixture.run_ms(&["--robot", "setup", "--claude-code"]);
    fixture.assert_success(&output, "setup");
    let json = output.json();
    let action = json["data"]["actions_taken"]
        .as_array()
        .expect("actions")
        .iter()
        .find(|action| action["action_type"] == "configure_mcp_server")
        .expect("agent action");
    assert_eq!(action["status"], "updated");

    let config: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&config_path)?)?;
    assert_eq!(config["theme"], "dark");
    assert_eq!(config["mcpServers"]["other"]["command"], "other-server");
    assert_eq!(config["mcpServers"]["ms"]["command"], "ms");

    fixture.generate_report();
    Ok(())
}
