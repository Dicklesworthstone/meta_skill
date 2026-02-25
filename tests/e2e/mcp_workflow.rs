//! E2E Scenario: MCP Server Workflow
//!
//! Tests the Model Context Protocol server functionality including:
//! - Server startup and shutdown
//! - Protocol compliance (JSON-RPC 2.0)
//! - All MCP tools (search, load, list, show, doctor, lint, etc.)
//! - Output safety (no ANSI escape codes)
//! - Error handling

use super::fixture::E2EFixture;
use ms::error::Result;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

// ============================================================================
// Test Skills
// ============================================================================

const SKILL_RUST_ERROR: &str = r#"---
name: Rust Error Handling
description: Best practices for error handling in Rust
tags: [rust, error-handling, result]
---

# Rust Error Handling

Use Result<T, E> for fallible operations.

## Key Patterns

- Use `?` operator for error propagation
- Define custom error types with thiserror
- Use anyhow for application code

## Example

```rust
fn read_file(path: &str) -> Result<String, std::io::Error> {
    std::fs::read_to_string(path)
}
```
"#;

const SKILL_RUST_TESTING: &str = r#"---
name: Rust Testing
description: Unit and integration testing in Rust
tags: [rust, testing, unit-tests]
---

# Rust Testing

Write tests with `#[test]` attribute.

## Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_example() {
        assert_eq!(2 + 2, 4);
    }
}
```

## Integration Tests

Place in `tests/` directory.
"#;

const SKILL_PYTHON_ASYNC: &str = r#"---
name: Python Async
description: Async programming in Python with asyncio
tags: [python, async, asyncio]
---

# Python Async

Use async/await for concurrent I/O operations.

## Example

```python
import asyncio

async def fetch_data():
    await asyncio.sleep(1)
    return "data"

asyncio.run(fetch_data())
```
"#;

// ============================================================================
// MCP Server Helper
// ============================================================================

/// Helper to interact with MCP server via JSON-RPC
struct McpClient {
    child: Child,
    request_id: u64,
}

impl McpClient {
    /// Spawn MCP server process
    fn spawn(fixture: &E2EFixture, debug: bool) -> Result<Self> {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_ms"));
        cmd.args(["mcp", "serve"])
            .env("HOME", &fixture.root)
            .env("MS_ROOT", &fixture.ms_root)
            .env("MS_CONFIG", &fixture.config_path)
            .env("MS_PLAIN_OUTPUT", "1")
            .current_dir(&fixture.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(if debug {
                Stdio::inherit()
            } else {
                Stdio::null()
            });

        if debug {
            cmd.arg("--debug");
        }

        let child = cmd.spawn().expect("Failed to spawn MCP server");

        Ok(Self {
            child,
            request_id: 0,
        })
    }

    /// Send a JSON-RPC request and get response
    fn request(&mut self, method: &str, params: Value) -> Result<McpResponse> {
        self.request_id += 1;
        let id = self.request_id;

        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        let stdin = self.child.stdin.as_mut().expect("No stdin");
        let request_str = serde_json::to_string(&request).expect("Failed to serialize request");
        writeln!(stdin, "{}", request_str).expect("Failed to write request");
        stdin.flush().expect("Failed to flush");

        // Read response with timeout
        let stdout = self.child.stdout.as_mut().expect("No stdout");
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();

        let start = Instant::now();
        let timeout = Duration::from_secs(10);

        loop {
            if start.elapsed() > timeout {
                return Err(ms::error::MsError::Timeout(
                    "MCP response timeout".to_string(),
                ));
            }

            match reader.read_line(&mut line) {
                Ok(0) => {
                    return Err(ms::error::MsError::Io(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "MCP server closed stdout",
                    )));
                }
                Ok(_) => break,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(e) => return Err(ms::error::MsError::Io(e)),
            }
        }

        let response: Value = serde_json::from_str(&line).expect("Failed to parse response");

        Ok(McpResponse {
            raw: line,
            json: response,
            request_id: id,
        })
    }

    /// Send notification (no response expected)
    fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        let notification = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });

        let stdin = self.child.stdin.as_mut().expect("No stdin");
        let request_str =
            serde_json::to_string(&notification).expect("Failed to serialize notification");
        writeln!(stdin, "{}", request_str).expect("Failed to write notification");
        stdin.flush().expect("Failed to flush");

        Ok(())
    }

    /// Initialize the MCP connection
    fn initialize(&mut self) -> Result<McpResponse> {
        let response = self.request(
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "clientInfo": {
                    "name": "ms-e2e-test",
                    "version": "1.0.0"
                },
                "capabilities": {}
            }),
        )?;

        // Send initialized notification
        self.notify("initialized", json!({}))?;

        Ok(response)
    }

    /// Call a tool
    fn call_tool(&mut self, name: &str, arguments: Value) -> Result<McpResponse> {
        self.request(
            "tools/call",
            json!({
                "name": name,
                "arguments": arguments
            }),
        )
    }

    /// List available tools
    fn list_tools(&mut self) -> Result<McpResponse> {
        self.request("tools/list", json!({}))
    }

    /// Shutdown the server
    #[allow(dead_code)]
    fn shutdown(&mut self) -> Result<McpResponse> {
        self.request("shutdown", json!({}))
    }

    /// Kill the server process
    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Response from MCP server
struct McpResponse {
    raw: String,
    json: Value,
    request_id: u64,
}

impl McpResponse {
    /// Check if response contains ANSI escape codes
    fn contains_ansi(&self) -> bool {
        self.raw.contains('\x1b')
    }

    /// Check if response is a success (has result, no error)
    fn is_success(&self) -> bool {
        self.json.get("result").is_some() && self.json.get("error").is_none()
    }

    /// Check if response is an error
    fn is_error(&self) -> bool {
        self.json.get("error").is_some()
    }

    /// Get the result value
    fn result(&self) -> Option<&Value> {
        self.json.get("result")
    }

    /// Get the error value
    fn error(&self) -> Option<&Value> {
        self.json.get("error")
    }

    /// Get error code
    fn error_code(&self) -> Option<i64> {
        self.error()?.get("code")?.as_i64()
    }

    /// Get tool content text (for tools/call responses)
    fn tool_text(&self) -> Option<&str> {
        self.result()?
            .get("content")?
            .as_array()?
            .first()?
            .get("text")?
            .as_str()
    }

    /// Check if tool result is an error
    fn tool_is_error(&self) -> bool {
        self.result()
            .and_then(|r| r.get("isError"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }
}

// ============================================================================
// Test Setup
// ============================================================================

fn setup_mcp_fixture(scenario: &str) -> Result<E2EFixture> {
    let mut fixture = E2EFixture::new(scenario);

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    fixture.log_step("Create test skills");
    fixture.create_skill_in_layer("rust-error-handling", SKILL_RUST_ERROR, "project")?;
    fixture.create_skill_in_layer("rust-testing", SKILL_RUST_TESTING, "project")?;
    fixture.create_skill_in_layer("python-async", SKILL_PYTHON_ASYNC, "global")?;

    fixture.log_step("Index skills");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    fixture.checkpoint("checkpoint:mcp:setup");

    Ok(fixture)
}

// ============================================================================
// Tests
// ============================================================================

#[test]
fn test_mcp_server_startup() -> Result<()> {
    let mut fixture = setup_mcp_fixture("mcp_server_startup")?;

    fixture.log_step("Start MCP server");
    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "checkpoint",
        "checkpoint:mcp:server_start",
        None,
    );

    let mut client = McpClient::spawn(&fixture, false)?;

    fixture.log_step("Initialize connection");
    let response = client.initialize()?;

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "checkpoint",
        "checkpoint:mcp:verify",
        Some(json!({ "response_id": response.request_id })),
    );

    // Verify response structure
    assert!(response.is_success(), "Initialize should succeed");
    assert!(
        !response.contains_ansi(),
        "Response should not contain ANSI codes"
    );

    let result = response.result().expect("Should have result");
    assert_eq!(
        result["protocolVersion"].as_str(),
        Some("2024-11-05"),
        "Protocol version should match"
    );
    assert!(
        result["serverInfo"]["name"].as_str().is_some(),
        "Server name should be present"
    );
    assert!(
        result["serverInfo"]["version"].as_str().is_some(),
        "Server version should be present"
    );
    assert!(
        result["capabilities"]["tools"].is_object(),
        "Tools capability should be present"
    );

    fixture.log_step("Shutdown server");
    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "checkpoint",
        "checkpoint:mcp:server_stop",
        None,
    );
    client.kill();

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "checkpoint",
        "checkpoint:mcp:teardown",
        None,
    );

    Ok(())
}

#[test]
fn test_mcp_tool_list() -> Result<()> {
    let mut fixture = setup_mcp_fixture("mcp_tool_list")?;

    fixture.log_step("Start MCP server and list tools");
    let mut client = McpClient::spawn(&fixture, false)?;
    client.initialize()?;

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "checkpoint",
        "checkpoint:mcp:tool_call:tools_list",
        None,
    );

    let response = client.list_tools()?;

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "checkpoint",
        "checkpoint:mcp:tool_response:tools_list",
        Some(json!({ "success": response.is_success() })),
    );

    assert!(response.is_success(), "tools/list should succeed");
    assert!(
        !response.contains_ansi(),
        "Response should not contain ANSI codes"
    );

    let result = response.result().expect("Should have result");
    let tools = result["tools"].as_array().expect("Should have tools array");

    // Verify expected tools are present
    let tool_names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();

    assert!(tool_names.contains(&"search"), "search tool should exist");
    assert!(tool_names.contains(&"load"), "load tool should exist");
    assert!(tool_names.contains(&"list"), "list tool should exist");
    assert!(tool_names.contains(&"show"), "show tool should exist");
    assert!(tool_names.contains(&"doctor"), "doctor tool should exist");
    assert!(tool_names.contains(&"lint"), "lint tool should exist");
    assert!(tool_names.contains(&"suggest"), "suggest tool should exist");

    println!("[MCP] Found {} tools: {:?}", tools.len(), tool_names);

    client.kill();
    Ok(())
}

#[test]
fn test_mcp_search_tool() -> Result<()> {
    let mut fixture = setup_mcp_fixture("mcp_search_tool")?;

    fixture.log_step("Test search tool");
    let mut client = McpClient::spawn(&fixture, false)?;
    client.initialize()?;

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "checkpoint",
        "checkpoint:mcp:tool_call:search",
        Some(json!({ "query": "rust error" })),
    );

    let response = client.call_tool(
        "search",
        json!({
            "query": "rust error",
            "limit": 10
        }),
    )?;

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "checkpoint",
        "checkpoint:mcp:tool_response:search",
        Some(json!({ "success": response.is_success() })),
    );

    assert!(response.is_success(), "search tool should succeed");
    assert!(
        !response.contains_ansi(),
        "Response should not contain ANSI codes"
    );
    assert!(!response.tool_is_error(), "Tool result should not be error");

    let text = response.tool_text().expect("Should have tool text");
    println!("[MCP] Search result: {}", &text[..text.len().min(500)]);

    // Parse the tool output as JSON
    let results: Value = serde_json::from_str(text).expect("Tool output should be valid JSON");
    assert!(
        results["results"].is_array(),
        "Should have results array in output"
    );

    let results_array = results["results"].as_array().unwrap();
    assert!(!results_array.is_empty(), "Should find at least one result");

    // First result should be rust-error-handling
    let first_id = results_array[0]["id"].as_str().unwrap_or_default();
    assert!(
        first_id.contains("rust-error") || first_id.contains("error-handling"),
        "First result should be about rust error handling, got: {}",
        first_id
    );

    client.kill();
    Ok(())
}

#[test]
fn test_mcp_load_tool() -> Result<()> {
    let mut fixture = setup_mcp_fixture("mcp_load_tool")?;

    fixture.log_step("Test load tool");
    let mut client = McpClient::spawn(&fixture, false)?;
    client.initialize()?;

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "checkpoint",
        "checkpoint:mcp:tool_call:load",
        Some(json!({ "skill": "rust-error-handling" })),
    );

    let response = client.call_tool(
        "load",
        json!({
            "skill": "rust-error-handling",
            "full": true
        }),
    )?;

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "checkpoint",
        "checkpoint:mcp:tool_response:load",
        Some(json!({ "success": response.is_success() })),
    );

    assert!(response.is_success(), "load tool should succeed");
    assert!(
        !response.contains_ansi(),
        "Response should not contain ANSI codes"
    );
    assert!(!response.tool_is_error(), "Tool result should not be error");

    let text = response.tool_text().expect("Should have tool text");
    println!("[MCP] Load result length: {} chars", text.len());

    // Should contain skill content
    assert!(
        text.contains("Rust Error Handling") || text.contains("rust-error-handling"),
        "Should contain skill name"
    );
    assert!(
        text.contains("Result<T, E>") || text.contains("error"),
        "Should contain skill content"
    );

    client.kill();
    Ok(())
}

#[test]
fn test_mcp_list_show_tools() -> Result<()> {
    let mut fixture = setup_mcp_fixture("mcp_list_show_tools")?;

    fixture.log_step("Test list tool");
    let mut client = McpClient::spawn(&fixture, false)?;
    client.initialize()?;

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "checkpoint",
        "checkpoint:mcp:tool_call:list",
        None,
    );

    let response = client.call_tool(
        "list",
        json!({
            "limit": 50,
            "offset": 0
        }),
    )?;

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "checkpoint",
        "checkpoint:mcp:tool_response:list",
        Some(json!({ "success": response.is_success() })),
    );

    assert!(response.is_success(), "list tool should succeed");
    assert!(
        !response.contains_ansi(),
        "Response should not contain ANSI codes"
    );

    let text = response.tool_text().expect("Should have tool text");
    let list_result: Value = serde_json::from_str(text).expect("Should be valid JSON");

    let skills = list_result["skills"]
        .as_array()
        .expect("Should have skills array");
    assert!(skills.len() >= 3, "Should have at least 3 skills indexed");

    // Get first skill ID for show test
    let first_skill_id = skills[0]["id"].as_str().expect("Should have skill id");

    fixture.log_step("Test show tool");
    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "checkpoint",
        "checkpoint:mcp:tool_call:show",
        Some(json!({ "skill": first_skill_id })),
    );

    let response = client.call_tool(
        "show",
        json!({
            "skill": first_skill_id,
            "full": false
        }),
    )?;

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "checkpoint",
        "checkpoint:mcp:tool_response:show",
        Some(json!({ "success": response.is_success() })),
    );

    assert!(response.is_success(), "show tool should succeed");
    assert!(
        !response.contains_ansi(),
        "Response should not contain ANSI codes"
    );

    let show_text = response.tool_text().expect("Should have tool text");
    let show_result: Value = serde_json::from_str(show_text).expect("Should be valid JSON");
    assert!(
        show_result["skill"].is_object(),
        "Should have skill object in show result"
    );

    client.kill();
    Ok(())
}

#[test]
fn test_mcp_doctor_tool() -> Result<()> {
    let mut fixture = setup_mcp_fixture("mcp_doctor_tool")?;

    fixture.log_step("Test doctor tool");
    let mut client = McpClient::spawn(&fixture, false)?;
    client.initialize()?;

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "checkpoint",
        "checkpoint:mcp:tool_call:doctor",
        None,
    );

    let response = client.call_tool(
        "doctor",
        json!({
            "fix": false
        }),
    )?;

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "checkpoint",
        "checkpoint:mcp:tool_response:doctor",
        Some(json!({ "success": response.is_success() })),
    );

    assert!(response.is_success(), "doctor tool should succeed");
    assert!(
        !response.contains_ansi(),
        "Response should not contain ANSI codes"
    );

    let text = response.tool_text().expect("Should have tool text");
    println!("[MCP] Doctor result: {}", &text[..text.len().min(500)]);

    // Doctor output should contain health check info
    assert!(
        text.contains("health")
            || text.contains("ok")
            || text.contains("check")
            || text.contains("status"),
        "Doctor output should contain health-related information"
    );

    client.kill();
    Ok(())
}

#[test]
fn test_mcp_lint_tool() -> Result<()> {
    let mut fixture = setup_mcp_fixture("mcp_lint_tool")?;

    fixture.log_step("Test lint tool with skill ID");
    let mut client = McpClient::spawn(&fixture, false)?;
    client.initialize()?;

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "checkpoint",
        "checkpoint:mcp:tool_call:lint",
        Some(json!({ "skill": "rust-error-handling" })),
    );

    let response = client.call_tool(
        "lint",
        json!({
            "skill": "rust-error-handling"
        }),
    )?;

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "checkpoint",
        "checkpoint:mcp:tool_response:lint",
        Some(json!({ "success": response.is_success() })),
    );

    assert!(response.is_success(), "lint tool should succeed");
    assert!(
        !response.contains_ansi(),
        "Response should not contain ANSI codes"
    );

    let text = response.tool_text().expect("Should have tool text");
    println!("[MCP] Lint result: {}", &text[..text.len().min(500)]);

    // Lint output should be parseable
    let lint_result: Value = serde_json::from_str(text).expect("Lint output should be valid JSON");
    assert!(
        lint_result.is_object(),
        "Lint result should be a JSON object"
    );

    client.kill();
    Ok(())
}

#[test]
fn test_mcp_protocol_compliance() -> Result<()> {
    let mut fixture = setup_mcp_fixture("mcp_protocol_compliance")?;

    fixture.log_step("Test protocol compliance");
    let mut client = McpClient::spawn(&fixture, false)?;
    client.initialize()?;

    // Test 1: Invalid method should return METHOD_NOT_FOUND (-32601)
    fixture.log_step("Test invalid method error");
    let response = client.request("invalid/method", json!({}))?;
    assert!(response.is_error(), "Invalid method should return error");
    assert_eq!(
        response.error_code(),
        Some(-32601),
        "Should be METHOD_NOT_FOUND error code"
    );

    // Test 2: Ping should work
    fixture.log_step("Test ping");
    let response = client.request("ping", json!({}))?;
    assert!(response.is_success(), "Ping should succeed");

    // Test 3: Invalid tool should return error in tool result
    fixture.log_step("Test invalid tool call");
    let response = client.call_tool("nonexistent_tool", json!({}))?;
    // This might succeed at JSON-RPC level but have isError in tool result
    // or it might fail at the MCP level - both are valid
    println!(
        "[MCP] Invalid tool response: success={}, tool_error={}",
        response.is_success(),
        response.tool_is_error()
    );

    // Test 4: Missing required params should return error
    fixture.log_step("Test missing required params");
    let response = client.call_tool(
        "search",
        json!({}), // Missing required "query" param
    )?;
    // Should indicate an error somehow
    assert!(
        response.is_error() || response.tool_is_error(),
        "Missing required param should result in error"
    );

    client.kill();
    Ok(())
}

#[test]
fn test_mcp_no_ansi_in_all_responses() -> Result<()> {
    let mut fixture = setup_mcp_fixture("mcp_no_ansi")?;

    fixture.log_step("Verify no ANSI codes in any response");
    let mut client = McpClient::spawn(&fixture, false)?;

    // Test initialize
    let response = client.initialize()?;
    assert!(
        !response.contains_ansi(),
        "Initialize response should not contain ANSI"
    );

    // Test tools/list
    let response = client.list_tools()?;
    assert!(
        !response.contains_ansi(),
        "Tools list response should not contain ANSI"
    );

    // Test search
    let response = client.call_tool("search", json!({"query": "rust"}))?;
    assert!(
        !response.contains_ansi(),
        "Search response should not contain ANSI"
    );

    // Test list
    let response = client.call_tool("list", json!({}))?;
    assert!(
        !response.contains_ansi(),
        "List response should not contain ANSI"
    );

    // Test load
    let response = client.call_tool(
        "load",
        json!({"skill": "rust-error-handling", "full": true}),
    )?;
    assert!(
        !response.contains_ansi(),
        "Load response should not contain ANSI"
    );

    // Test doctor
    let response = client.call_tool("doctor", json!({}))?;
    assert!(
        !response.contains_ansi(),
        "Doctor response should not contain ANSI"
    );

    // Test error responses
    let response = client.request("invalid/method", json!({}))?;
    assert!(
        !response.contains_ansi(),
        "Error response should not contain ANSI"
    );

    client.kill();
    Ok(())
}

// ============================================================================
// Additional Edge Case Tests
// ============================================================================

#[test]
fn test_mcp_config_tool() -> Result<()> {
    let mut fixture = setup_mcp_fixture("mcp_config_tool")?;

    fixture.log_step("Test config list");
    let mut client = McpClient::spawn(&fixture, false)?;
    client.initialize()?;

    let response = client.call_tool(
        "config",
        json!({
            "action": "list"
        }),
    )?;

    assert!(response.is_success(), "config list should succeed");
    assert!(
        !response.contains_ansi(),
        "Response should not contain ANSI codes"
    );

    let text = response.tool_text().expect("Should have tool text");
    println!("[MCP] Config list result: {}", &text[..text.len().min(500)]);

    client.kill();
    Ok(())
}

#[test]
fn test_mcp_validate_tool() -> Result<()> {
    let mut fixture = setup_mcp_fixture("mcp_validate_tool")?;

    fixture.log_step("Test validate tool with valid content");
    let mut client = McpClient::spawn(&fixture, false)?;
    client.initialize()?;

    let valid_skill = r#"---
name: Test Skill
description: A test skill for validation
tags: [test]
---

# Test Skill

This is a test.
"#;

    let response = client.call_tool(
        "validate",
        json!({
            "content": valid_skill
        }),
    )?;

    assert!(response.is_success(), "validate tool should succeed");
    assert!(
        !response.contains_ansi(),
        "Response should not contain ANSI codes"
    );

    let text = response.tool_text().expect("Should have tool text");
    println!("[MCP] Validate result: {}", &text[..text.len().min(500)]);

    client.kill();
    Ok(())
}

#[test]
fn test_mcp_suggest_tool() -> Result<()> {
    let mut fixture = setup_mcp_fixture("mcp_suggest_tool")?;

    fixture.log_step("Test suggest tool");
    let mut client = McpClient::spawn(&fixture, false)?;
    client.initialize()?;

    let response = client.call_tool(
        "suggest",
        json!({
            "cwd": fixture.root.to_string_lossy(),
            "limit": 5
        }),
    )?;

    assert!(response.is_success(), "suggest tool should succeed");
    assert!(
        !response.contains_ansi(),
        "Response should not contain ANSI codes"
    );

    let text = response.tool_text().expect("Should have tool text");
    println!("[MCP] Suggest result: {}", &text[..text.len().min(500)]);

    client.kill();
    Ok(())
}

#[test]
fn test_mcp_index_tool() -> Result<()> {
    let mut fixture = setup_mcp_fixture("mcp_index_tool")?;

    fixture.log_step("Test index tool (re-index)");
    let mut client = McpClient::spawn(&fixture, false)?;
    client.initialize()?;

    let response = client.call_tool(
        "index",
        json!({
            "force": false
        }),
    )?;

    assert!(response.is_success(), "index tool should succeed");
    assert!(
        !response.contains_ansi(),
        "Response should not contain ANSI codes"
    );

    let text = response.tool_text().expect("Should have tool text");
    println!("[MCP] Index result: {}", &text[..text.len().min(500)]);

    client.kill();
    Ok(())
}
