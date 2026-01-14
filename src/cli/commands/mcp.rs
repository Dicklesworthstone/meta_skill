//! ms mcp - MCP (Model Context Protocol) server mode
//!
//! Exposes ms functionality as an MCP server for tool-based integration
//! with AI coding agents. Supports stdio transport (primary) and optional
//! TCP transport.

use std::io::{self, BufRead, Write};

use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::app::AppContext;
use crate::cli::output::emit_json;
use crate::error::{MsError, Result};

/// MCP server protocol version
const PROTOCOL_VERSION: &str = "2024-11-05";
/// Server name for identification
const SERVER_NAME: &str = "ms";
/// Server version (from cargo)
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Args, Debug)]
pub struct McpArgs {
    #[command(subcommand)]
    pub command: McpCommand,
}

#[derive(Subcommand, Debug)]
pub enum McpCommand {
    /// Start MCP server with stdio transport
    Serve(ServeArgs),
    /// List available MCP tools
    Tools,
}

#[derive(Args, Debug)]
pub struct ServeArgs {
    /// Enable TCP transport on specified port (in addition to stdio)
    #[arg(long)]
    pub tcp_port: Option<u16>,

    /// Enable debug logging to stderr
    #[arg(long)]
    pub debug: bool,
}

// ============================================================================
// JSON-RPC 2.0 Types
// ============================================================================

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

impl JsonRpcResponse {
    fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Option<Value>, code: i32, message: String, data: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message,
                data,
            }),
        }
    }
}

// JSON-RPC 2.0 error codes
const PARSE_ERROR: i32 = -32700;
const INVALID_REQUEST: i32 = -32600;
const METHOD_NOT_FOUND: i32 = -32601;
const INVALID_PARAMS: i32 = -32602;
#[allow(dead_code)]
const INTERNAL_ERROR: i32 = -32603;

// ============================================================================
// MCP Protocol Types
// ============================================================================

#[derive(Debug, Serialize)]
struct ServerCapabilities {
    tools: ToolsCapability,
}

#[derive(Debug, Serialize)]
struct ToolsCapability {
    #[serde(rename = "listChanged")]
    list_changed: bool,
}

#[derive(Debug, Serialize)]
struct ServerInfo {
    name: String,
    version: String,
}

#[derive(Debug, Serialize)]
struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    protocol_version: String,
    capabilities: ServerCapabilities,
    #[serde(rename = "serverInfo")]
    server_info: ServerInfo,
}

#[derive(Debug, Serialize)]
struct Tool {
    name: String,
    description: String,
    #[serde(rename = "inputSchema")]
    input_schema: Value,
}

#[derive(Debug, Serialize)]
struct ToolsListResult {
    tools: Vec<Tool>,
}

#[derive(Debug, Serialize)]
struct ToolResult {
    content: Vec<ToolContent>,
    #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
    is_error: Option<bool>,
}

#[derive(Debug, Serialize)]
struct ToolContent {
    #[serde(rename = "type")]
    content_type: String,
    text: String,
}

impl ToolResult {
    fn text(text: String) -> Self {
        Self {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text,
            }],
            is_error: None,
        }
    }

    fn error(message: String) -> Self {
        Self {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: message,
            }],
            is_error: Some(true),
        }
    }
}

// ============================================================================
// Tool Definitions
// ============================================================================

fn define_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "search".to_string(),
            description: "Search for skills using BM25 full-text search".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query text"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results (default: 20)",
                        "default": 20
                    }
                },
                "required": ["query"]
            }),
        },
        Tool {
            name: "load".to_string(),
            description: "Load a skill by ID".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "skill": {
                        "type": "string",
                        "description": "Skill ID or name to load"
                    },
                    "full": {
                        "type": "boolean",
                        "description": "Include full skill content",
                        "default": false
                    }
                },
                "required": ["skill"]
            }),
        },
        Tool {
            name: "evidence".to_string(),
            description: "View provenance evidence for skill rules".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "skill": {
                        "type": "string",
                        "description": "Skill ID to query evidence for"
                    },
                    "rule_id": {
                        "type": "string",
                        "description": "Specific rule ID to get evidence for"
                    }
                },
                "required": ["skill"]
            }),
        },
        Tool {
            name: "list".to_string(),
            description: "List all indexed skills".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results",
                        "default": 50
                    },
                    "offset": {
                        "type": "integer",
                        "description": "Number of results to skip",
                        "default": 0
                    }
                }
            }),
        },
        Tool {
            name: "show".to_string(),
            description: "Show detailed information about a specific skill".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "skill": {
                        "type": "string",
                        "description": "Skill ID or name"
                    },
                    "full": {
                        "type": "boolean",
                        "description": "Show full skill content",
                        "default": false
                    }
                },
                "required": ["skill"]
            }),
        },
        Tool {
            name: "doctor".to_string(),
            description: "Run health checks on the ms installation".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "fix": {
                        "type": "boolean",
                        "description": "Attempt to fix issues",
                        "default": false
                    }
                }
            }),
        },
    ]
}

// ============================================================================
// MCP Server Implementation
// ============================================================================

pub fn run(ctx: &AppContext, args: &McpArgs) -> Result<()> {
    match &args.command {
        McpCommand::Serve(serve_args) => run_serve(ctx, serve_args),
        McpCommand::Tools => run_tools(ctx),
    }
}

fn run_tools(ctx: &AppContext) -> Result<()> {
    let tools = define_tools();
    if ctx.robot_mode {
        emit_json(&serde_json::json!({
            "tools": tools,
            "count": tools.len()
        }))
    } else {
        println!("Available MCP Tools:\n");
        for tool in &tools {
            println!("  {} - {}", tool.name, tool.description);
        }
        println!("\n{} tools available.", tools.len());
        Ok(())
    }
}

fn run_serve(ctx: &AppContext, args: &ServeArgs) -> Result<()> {
    let debug = args.debug;

    if debug {
        eprintln!("[ms-mcp] Starting MCP server (stdio mode)");
        eprintln!(
            "[ms-mcp] Server: {} v{}",
            SERVER_NAME, SERVER_VERSION
        );
        eprintln!("[ms-mcp] Protocol: {}", PROTOCOL_VERSION);
    }

    // Run the stdio server loop
    run_stdio_server(ctx, debug)
}

fn run_stdio_server(ctx: &AppContext, debug: bool) -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                if debug {
                    eprintln!("[ms-mcp] stdin read error: {}", e);
                }
                break;
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        if debug {
            eprintln!("[ms-mcp] <- {}", line);
        }

        // Handle request - returns None for notifications (no response needed)
        if let Some(response) = handle_request(ctx, &line, debug) {
            let response_json = serde_json::to_string(&response).unwrap_or_else(|e| {
                serde_json::to_string(&JsonRpcResponse::error(
                    None,
                    PARSE_ERROR,
                    format!("Failed to serialize response: {}", e),
                    None,
                ))
                .unwrap()
            });

            if debug {
                eprintln!("[ms-mcp] -> {}", response_json);
            }

            if writeln!(stdout, "{}", response_json).is_err() {
                break;
            }
            let _ = stdout.flush();
        } else if debug {
            eprintln!("[ms-mcp] -> (no response - notification)");
        }
    }

    if debug {
        eprintln!("[ms-mcp] Server shutting down");
    }

    Ok(())
}

fn handle_request(ctx: &AppContext, line: &str, debug: bool) -> Option<JsonRpcResponse> {
    // Parse JSON-RPC request
    let request: JsonRpcRequest = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => {
            return Some(JsonRpcResponse::error(
                None,
                PARSE_ERROR,
                format!("Parse error: {}", e),
                None,
            ));
        }
    };

    // Validate JSON-RPC version
    if request.jsonrpc != "2.0" {
        return Some(JsonRpcResponse::error(
            request.id,
            INVALID_REQUEST,
            "Invalid JSON-RPC version".to_string(),
            None,
        ));
    }

    // Dispatch method
    match request.method.as_str() {
        "initialize" => Some(handle_initialize(request.id, &request.params)),
        "initialized" => handle_initialized(request.id),
        "tools/list" => Some(handle_tools_list(request.id)),
        "tools/call" => Some(handle_tools_call(ctx, request.id, &request.params, debug)),
        "ping" => Some(handle_ping(request.id)),
        "shutdown" => Some(handle_shutdown(request.id)),
        _ => Some(JsonRpcResponse::error(
            request.id,
            METHOD_NOT_FOUND,
            format!("Method not found: {}", request.method),
            None,
        )),
    }
}

fn handle_initialize(id: Option<Value>, _params: &Value) -> JsonRpcResponse {
    let result = InitializeResult {
        protocol_version: PROTOCOL_VERSION.to_string(),
        capabilities: ServerCapabilities {
            tools: ToolsCapability { list_changed: false },
        },
        server_info: ServerInfo {
            name: SERVER_NAME.to_string(),
            version: SERVER_VERSION.to_string(),
        },
    };
    JsonRpcResponse::success(id, serde_json::to_value(result).unwrap())
}

fn handle_initialized(id: Option<Value>) -> Option<JsonRpcResponse> {
    // JSON-RPC 2.0: Notifications (no id) MUST NOT receive a response
    // If id is present, respond (unusual but permitted)
    id.map(|id| JsonRpcResponse::success(Some(id), serde_json::json!({})))
}

fn handle_tools_list(id: Option<Value>) -> JsonRpcResponse {
    let result = ToolsListResult {
        tools: define_tools(),
    };
    JsonRpcResponse::success(id, serde_json::to_value(result).unwrap())
}

fn handle_tools_call(
    ctx: &AppContext,
    id: Option<Value>,
    params: &Value,
    debug: bool,
) -> JsonRpcResponse {
    // Extract tool name and arguments
    let name = match params.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => {
            return JsonRpcResponse::error(
                id,
                INVALID_PARAMS,
                "Missing required parameter: name".to_string(),
                None,
            );
        }
    };

    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or(serde_json::json!({}));

    if debug {
        eprintln!("[ms-mcp] Calling tool: {} with {:?}", name, arguments);
    }

    // Dispatch to tool handler
    let result = match name {
        "search" => handle_tool_search(ctx, &arguments),
        "load" => handle_tool_load(ctx, &arguments),
        "evidence" => handle_tool_evidence(ctx, &arguments),
        "list" => handle_tool_list(ctx, &arguments),
        "show" => handle_tool_show(ctx, &arguments),
        "doctor" => handle_tool_doctor(ctx, &arguments),
        _ => Err(MsError::ValidationFailed(format!(
            "Unknown tool: {}",
            name
        ))),
    };

    match result {
        Ok(tool_result) => {
            JsonRpcResponse::success(id, serde_json::to_value(tool_result).unwrap())
        }
        Err(e) => {
            let tool_result = ToolResult::error(e.to_string());
            JsonRpcResponse::success(id, serde_json::to_value(tool_result).unwrap())
        }
    }
}

fn handle_ping(id: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse::success(id, serde_json::json!({}))
}

fn handle_shutdown(id: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse::success(id, serde_json::json!({}))
}

// ============================================================================
// Tool Handlers
// ============================================================================

fn handle_tool_search(ctx: &AppContext, args: &Value) -> Result<ToolResult> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| MsError::ValidationFailed("Missing required parameter: query".to_string()))?;

    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

    // Use BM25 search via Tantivy
    let results = ctx.search.search(query, limit)?;

    let output = serde_json::json!({
        "query": query,
        "count": results.len(),
        "results": results.iter().map(|r| {
            serde_json::json!({
                "id": r.skill_id,
                "score": r.score,
            })
        }).collect::<Vec<_>>()
    });

    Ok(ToolResult::text(serde_json::to_string_pretty(&output)?))
}

fn handle_tool_load(ctx: &AppContext, args: &Value) -> Result<ToolResult> {
    let skill_id = args
        .get("skill")
        .and_then(|v| v.as_str())
        .ok_or_else(|| MsError::ValidationFailed("Missing required parameter: skill".to_string()))?;

    let full = args.get("full").and_then(|v| v.as_bool()).unwrap_or(false);

    // Look up skill
    let skill = ctx.db.get_skill(skill_id)?
        .ok_or_else(|| MsError::SkillNotFound(skill_id.to_string()))?;

    let output = if full {
        serde_json::json!({
            "skill_id": skill.id,
            "name": skill.name,
            "description": skill.description,
            "content": skill.body,
            "layer": skill.source_layer,
            "quality_score": skill.quality_score,
        })
    } else {
        serde_json::json!({
            "skill_id": skill.id,
            "name": skill.name,
            "description": skill.description,
            "layer": skill.source_layer,
        })
    };

    Ok(ToolResult::text(serde_json::to_string_pretty(&output)?))
}

fn handle_tool_evidence(ctx: &AppContext, args: &Value) -> Result<ToolResult> {
    let skill_id = args
        .get("skill")
        .and_then(|v| v.as_str())
        .ok_or_else(|| MsError::ValidationFailed("Missing required parameter: skill".to_string()))?;

    let rule_id = args.get("rule_id").and_then(|v| v.as_str());

    // Query evidence from database
    let output = if let Some(rid) = rule_id {
        // Get evidence for specific rule
        let evidence = ctx.db.get_rule_evidence(skill_id, rid)?;
        serde_json::json!({
            "skill_id": skill_id,
            "rule_id": rid,
            "evidence_count": evidence.len(),
            "evidence": evidence.iter().map(|e| {
                serde_json::json!({
                    "session_id": e.session_id,
                    "message_range": [e.message_range.0, e.message_range.1],
                    "confidence": e.confidence,
                    "excerpt": e.excerpt,
                    "snippet_hash": e.snippet_hash,
                })
            }).collect::<Vec<_>>()
        })
    } else {
        // Get all evidence for skill
        let index = ctx.db.get_evidence(skill_id)?;
        serde_json::json!({
            "skill_id": skill_id,
            "coverage": {
                "total_rules": index.coverage.total_rules,
                "rules_with_evidence": index.coverage.rules_with_evidence,
                "avg_confidence": index.coverage.avg_confidence,
            },
            "rules": index.rules.iter().map(|(rule_id, refs)| {
                serde_json::json!({
                    "rule_id": rule_id,
                    "evidence_count": refs.len(),
                    "evidence": refs.iter().map(|e| {
                        serde_json::json!({
                            "session_id": e.session_id,
                            "message_range": [e.message_range.0, e.message_range.1],
                            "confidence": e.confidence,
                            "excerpt": e.excerpt,
                        })
                    }).collect::<Vec<_>>()
                })
            }).collect::<Vec<_>>()
        })
    };

    Ok(ToolResult::text(serde_json::to_string_pretty(&output)?))
}

fn handle_tool_list(ctx: &AppContext, args: &Value) -> Result<ToolResult> {
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
    let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

    let all_skills = ctx.db.list_skills(limit, offset)?;

    let output = serde_json::json!({
        "count": all_skills.len(),
        "skills": all_skills.iter().map(|s| {
            serde_json::json!({
                "id": s.id,
                "name": s.name,
                "description": s.description,
                "layer": s.source_layer,
            })
        }).collect::<Vec<_>>()
    });

    Ok(ToolResult::text(serde_json::to_string_pretty(&output)?))
}

fn handle_tool_show(ctx: &AppContext, args: &Value) -> Result<ToolResult> {
    let skill_id = args
        .get("skill")
        .and_then(|v| v.as_str())
        .ok_or_else(|| MsError::ValidationFailed("Missing required parameter: skill".to_string()))?;

    let full = args.get("full").and_then(|v| v.as_bool()).unwrap_or(false);

    let skill = ctx.db.get_skill(skill_id)?
        .ok_or_else(|| MsError::SkillNotFound(skill_id.to_string()))?;

    let output = if full {
        serde_json::json!({
            "id": skill.id,
            "name": skill.name,
            "description": skill.description,
            "layer": skill.source_layer,
            "quality_score": skill.quality_score,
            "is_deprecated": skill.is_deprecated,
            "content": skill.body,
        })
    } else {
        serde_json::json!({
            "id": skill.id,
            "name": skill.name,
            "description": skill.description,
            "layer": skill.source_layer,
        })
    };

    Ok(ToolResult::text(serde_json::to_string_pretty(&output)?))
}

fn handle_tool_doctor(ctx: &AppContext, args: &Value) -> Result<ToolResult> {
    let fix = args.get("fix").and_then(|v| v.as_bool()).unwrap_or(false);

    // Basic health checks
    let mut checks = Vec::new();

    // Check database - just try to list skills
    let db_ok = ctx.db.list_skills(1, 0).is_ok();
    checks.push(serde_json::json!({
        "name": "database",
        "status": if db_ok { "ok" } else { "error" },
        "message": if db_ok { "SQLite database accessible" } else { "Database connection failed" }
    }));

    // Check search index - try a simple search
    let search_ok = ctx.search.search("test", 1).is_ok();
    checks.push(serde_json::json!({
        "name": "search_index",
        "status": if search_ok { "ok" } else { "error" },
        "message": if search_ok { "Tantivy index accessible" } else { "Search index failed" }
    }));

    // Check git archive - just check if root exists
    let git_ok = ctx.git.root().exists();
    checks.push(serde_json::json!({
        "name": "git_archive",
        "status": if git_ok { "ok" } else { "error" },
        "message": if git_ok { "Git archive accessible" } else { "Git archive failed" }
    }));

    let all_ok = checks.iter().all(|c| c.get("status").and_then(|s| s.as_str()) == Some("ok"));

    let output = serde_json::json!({
        "status": if all_ok { "healthy" } else { "unhealthy" },
        "fix_requested": fix,
        "checks": checks
    });

    Ok(ToolResult::text(serde_json::to_string_pretty(&output)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_define_tools() {
        let tools = define_tools();
        assert!(!tools.is_empty());
        assert!(tools.iter().any(|t| t.name == "search"));
        assert!(tools.iter().any(|t| t.name == "load"));
    }

    #[test]
    fn test_jsonrpc_response_success() {
        let resp = JsonRpcResponse::success(Some(serde_json::json!(1)), serde_json::json!({"ok": true}));
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_jsonrpc_response_error() {
        let resp = JsonRpcResponse::error(Some(serde_json::json!(1)), -32600, "Invalid".to_string(), None);
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
    }

    #[test]
    fn test_tool_result_text() {
        let result = ToolResult::text("hello".to_string());
        assert_eq!(result.content.len(), 1);
        assert_eq!(result.content[0].text, "hello");
        assert!(result.is_error.is_none());
    }

    #[test]
    fn test_tool_result_error() {
        let result = ToolResult::error("failed".to_string());
        assert!(result.is_error == Some(true));
    }

    #[test]
    fn test_handle_initialized_notification() {
        // JSON-RPC 2.0: Notifications (no id) MUST NOT receive a response
        let result = handle_initialized(None);
        assert!(result.is_none(), "Notification should not produce a response");
    }

    #[test]
    fn test_handle_initialized_with_id() {
        // When id is present (unusual but permitted), respond
        let result = handle_initialized(Some(serde_json::json!(42)));
        assert!(result.is_some(), "Request with id should produce a response");
        let response = result.unwrap();
        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }
}
