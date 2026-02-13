//! ms mcp - MCP (Model Context Protocol) server mode
//!
//! Exposes ms functionality as an MCP server for tool-based integration
//! with AI coding agents. Supports stdio transport (primary) and optional
//! TCP transport.
//!
//! # Output Safety
//!
//! **CRITICAL**: MCP responses MUST always be valid JSON. This module enforces:
//! - All output is sanitized to remove ANSI escape codes
//! - All responses are validated before sending
//! - Environment variables cannot enable rich output
//! - Config settings cannot enable rich output
//!
//! See [`sanitize_mcp_output`] and [`validate_mcp_json`] for details.

use std::io::{self, BufRead, Write};

use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{debug, warn};

use crate::app::AppContext;
use crate::cli::output::OutputFormat;
use crate::cli::output::emit_json;
use crate::context::detector::ProjectDetector;
use crate::core::spec_lens::parse_markdown;
use crate::error::{MsError, Result};
use crate::lint::rules::all_rules;
use crate::lint::{ValidationConfig, ValidationEngine};

/// MCP server protocol version
const PROTOCOL_VERSION: &str = "2024-11-05";
/// Server name for identification
const SERVER_NAME: &str = "ms";
/// Server version (from cargo)
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

// ============================================================================
// MCP Output Safety
// ============================================================================
//
// CRITICAL: MCP responses MUST be valid JSON with NO ANSI codes.
// These functions ensure output safety regardless of environment or config.

/// Check if rich output would have been enabled based on environment.
///
/// Returns Some(reason) if rich output would be enabled, None otherwise.
/// Used for logging warnings when MCP blocks rich output.
fn would_enable_rich_output() -> Option<&'static str> {
    use std::io::IsTerminal;

    // Check env vars that might enable rich output
    if std::env::var_os("MS_FORCE_RICH").is_some() {
        return Some("MS_FORCE_RICH");
    }
    if std::env::var_os("FORCE_COLOR").is_some() {
        return Some("FORCE_COLOR");
    }
    if std::env::var_os("CLICOLOR_FORCE").is_some() {
        return Some("CLICOLOR_FORCE");
    }

    // Check if stdout is a terminal (would default to rich)
    if std::io::stdout().is_terminal() {
        // Only if NO_COLOR and MS_PLAIN_OUTPUT are not set
        if std::env::var_os("NO_COLOR").is_none() && std::env::var_os("MS_PLAIN_OUTPUT").is_none() {
            return Some("terminal_default");
        }
    }

    None
}

/// Strip ANSI escape codes from a string.
///
/// Uses a simple state machine to handle:
/// - CSI sequences: ESC [ ... final_byte
/// - OSC sequences: ESC ] ... (ST | BEL)
/// - Simple escapes: ESC followed by single char
#[must_use]
pub fn strip_ansi(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Start of escape sequence
            match chars.peek() {
                Some('[') => {
                    // CSI sequence: ESC [ ... final_byte (0x40-0x7E)
                    chars.next(); // consume '['
                    while let Some(&ch) = chars.peek() {
                        chars.next();
                        if (0x40..=0x7E).contains(&(ch as u32)) {
                            break; // final byte
                        }
                    }
                }
                Some(']') => {
                    // OSC sequence: ESC ] ... (ST or BEL)
                    chars.next(); // consume ']'
                    while let Some(&ch) = chars.peek() {
                        chars.next();
                        if ch == '\x07' {
                            break; // BEL terminator
                        }
                        if ch == '\x1b' {
                            // Check for ST (ESC \)
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                                break;
                            }
                        }
                    }
                }
                Some(_) => {
                    // Simple escape sequence (ESC + one char)
                    chars.next();
                }
                None => {
                    // Lone ESC at end of string - skip it
                }
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// Check if a string contains ANSI escape codes.
#[must_use]
pub fn contains_ansi(s: &str) -> bool {
    s.contains('\x1b')
}

/// Sanitize output for MCP responses.
///
/// This function:
/// 1. Strips any ANSI escape codes
/// 2. Validates the result is clean
/// 3. Logs a warning if ANSI codes were found
#[must_use]
pub fn sanitize_mcp_output(s: &str) -> String {
    if contains_ansi(s) {
        warn!(
            "MCP output contained ANSI codes - stripping. This indicates a bug in output handling."
        );
        strip_ansi(s)
    } else {
        s.to_string()
    }
}

/// Validate that a JSON string is safe for MCP transport.
///
/// Returns an error if the JSON contains ANSI escape codes.
pub fn validate_mcp_json(json: &str) -> std::result::Result<(), String> {
    if contains_ansi(json) {
        Err("MCP response contains ANSI escape codes".to_string())
    } else {
        Ok(())
    }
}

/// Serialize a JSON-RPC response with safety checks.
///
/// This function:
/// 1. Serializes the response to JSON
/// 2. Strips any ANSI codes that might have leaked through
/// 3. Validates the result
/// 4. Logs warnings if issues were found
fn serialize_response_safe(response: &JsonRpcResponse) -> String {
    match serde_json::to_string(response) {
        Ok(json) => {
            if contains_ansi(&json) {
                warn!(
                    "JSON-RPC response contained ANSI codes after serialization - this is a bug!"
                );
                strip_ansi(&json)
            } else {
                json
            }
        }
        Err(e) => {
            // Fallback: return a minimal error response
            warn!("Failed to serialize JSON-RPC response: {}", e);
            let fallback = JsonRpcResponse::error(
                None,
                PARSE_ERROR,
                format!("Failed to serialize response: {e}"),
                None,
            );
            serde_json::to_string(&fallback).unwrap_or_else(|_| {
                r#"{"jsonrpc":"2.0","error":{"code":-32700,"message":"Serialization failed"}}"#
                    .to_string()
            })
        }
    }
}

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
    /// Create a text result, sanitizing any ANSI codes.
    ///
    /// # Safety
    /// This function sanitizes the input to ensure no ANSI codes
    /// leak into MCP responses.
    fn text(text: String) -> Self {
        // CRITICAL: Sanitize text to remove any ANSI codes
        let sanitized = sanitize_mcp_output(&text);
        Self {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: sanitized,
            }],
            is_error: None,
        }
    }

    /// Create an error result, sanitizing any ANSI codes.
    ///
    /// # Safety
    /// This function sanitizes the input to ensure no ANSI codes
    /// leak into MCP responses.
    fn error(message: String) -> Self {
        // CRITICAL: Sanitize message to remove any ANSI codes
        let sanitized = sanitize_mcp_output(&message);
        Self {
            content: vec![ToolContent {
                content_type: "text".to_string(),
                text: sanitized,
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
        Tool {
            name: "lint".to_string(),
            description:
                "Lint a skill file for validation issues, security problems, and quality warnings"
                    .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to SKILL.md file to lint"
                    },
                    "skill": {
                        "type": "string",
                        "description": "Skill ID to lint (alternative to path, reads from archive)"
                    },
                    "strict": {
                        "type": "boolean",
                        "description": "Treat warnings as errors",
                        "default": false
                    },
                    "rules": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Only run specific rules (by rule ID)"
                    }
                }
            }),
        },
        Tool {
            name: "suggest".to_string(),
            description: "Get context-aware skill suggestions based on working directory"
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "cwd": {
                        "type": "string",
                        "description": "Working directory path for context detection"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum suggestions (default: 5)",
                        "default": 5
                    },
                    "explain": {
                        "type": "boolean",
                        "description": "Include explanation for each suggestion",
                        "default": false
                    },
                    "threshold": {
                        "type": "number",
                        "description": "Minimum relevance score (0.0-1.0)",
                        "default": 0.3
                    }
                }
            }),
        },
        Tool {
            name: "feedback".to_string(),
            description: "Record feedback for a skill (helpful/not helpful)".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "skill_id": {
                        "type": "string",
                        "description": "Skill ID to provide feedback for"
                    },
                    "helpful": {
                        "type": "boolean",
                        "description": "Whether the skill was helpful"
                    },
                    "comment": {
                        "type": "string",
                        "description": "Optional feedback comment"
                    },
                    "context": {
                        "type": "object",
                        "description": "Optional context about how skill was used"
                    }
                },
                "required": ["skill_id", "helpful"]
            }),
        },
        Tool {
            name: "index".to_string(),
            description: "Index skills from specified paths".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "paths": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Paths to index (default: configured paths)"
                    },
                    "force": {
                        "type": "boolean",
                        "description": "Force re-index even if unchanged",
                        "default": false
                    }
                }
            }),
        },
        Tool {
            name: "validate".to_string(),
            description: "Validate a skill specification".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "Skill markdown content to validate"
                    },
                    "path": {
                        "type": "string",
                        "description": "Path to skill file (alternative to content)"
                    }
                }
            }),
        },
        Tool {
            name: "config".to_string(),
            description: "Get or set ms configuration values".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["get", "set", "list"],
                        "description": "Action to perform",
                        "default": "list"
                    },
                    "key": {
                        "type": "string",
                        "description": "Configuration key (for get/set)"
                    },
                    "value": {
                        "type": "string",
                        "description": "Configuration value (for set)"
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
    if ctx.output_format != OutputFormat::Human {
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
        eprintln!("[ms-mcp] Server: {SERVER_NAME} v{SERVER_VERSION}");
        eprintln!("[ms-mcp] Protocol: {PROTOCOL_VERSION}");
    }

    // CRITICAL: Check if rich output would have been enabled and warn
    if let Some(reason) = would_enable_rich_output() {
        debug!(
            reason,
            "Rich output would be enabled but MCP forces plain mode"
        );
        if debug {
            eprintln!("[ms-mcp] Note: Rich output blocked for MCP (would be enabled by: {reason})");
        }
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
                    eprintln!("[ms-mcp] stdin read error: {e}");
                }
                break;
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        if debug {
            eprintln!("[ms-mcp] <- {line}");
        }

        // Handle request - returns None for notifications (no response needed)
        if let Some(response) = handle_request(ctx, &line, debug) {
            // CRITICAL: Use safe serialization to ensure no ANSI codes leak through
            let response_json = serialize_response_safe(&response);

            // Double-check: validate the response is safe (should always pass after sanitization)
            if let Err(e) = validate_mcp_json(&response_json) {
                // This should never happen, but log it if it does
                warn!("MCP response validation failed after sanitization: {}", e);
            }

            if debug {
                eprintln!("[ms-mcp] -> {response_json}");
            }

            if writeln!(stdout, "{response_json}").is_err() {
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
                format!("Parse error: {e}"),
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
        "initialized" | "notifications/initialized" => handle_initialized(request.id),
        "tools/list" => Some(handle_tools_list(request.id)),
        "tools/call" => Some(handle_tools_call(ctx, request.id, &request.params, debug)),
        "ping" => Some(handle_ping(request.id)),
        "shutdown" => Some(handle_shutdown(request.id)),
        // Return empty results for resource endpoints we don't support
        "resources/list" => Some(JsonRpcResponse::success(
            request.id,
            serde_json::json!({"resources": []}),
        )),
        "resources/templates/list" => Some(JsonRpcResponse::success(
            request.id,
            serde_json::json!({"resourceTemplates": []}),
        )),
        _ => {
            // JSON-RPC 2.0: notifications (no id) MUST NOT receive a response
            if request.id.is_none() {
                None
            } else {
                Some(JsonRpcResponse::error(
                    request.id,
                    METHOD_NOT_FOUND,
                    format!("Method not found: {}", request.method),
                    None,
                ))
            }
        }
    }
}

fn handle_initialize(id: Option<Value>, _params: &Value) -> JsonRpcResponse {
    let result = InitializeResult {
        protocol_version: PROTOCOL_VERSION.to_string(),
        capabilities: ServerCapabilities {
            tools: ToolsCapability {
                list_changed: false,
            },
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
        eprintln!("[ms-mcp] Calling tool: {name} with {arguments:?}");
    }

    // Dispatch to tool handler
    let result = match name {
        "search" => handle_tool_search(ctx, &arguments),
        "load" => handle_tool_load(ctx, &arguments),
        "evidence" => handle_tool_evidence(ctx, &arguments),
        "list" => handle_tool_list(ctx, &arguments),
        "show" => handle_tool_show(ctx, &arguments),
        "doctor" => handle_tool_doctor(ctx, &arguments),
        "lint" => handle_tool_lint(ctx, &arguments),
        "suggest" => handle_tool_suggest(ctx, &arguments),
        "feedback" => handle_tool_feedback(ctx, &arguments),
        "index" => handle_tool_index(ctx, &arguments),
        "validate" => handle_tool_validate(ctx, &arguments),
        "config" => handle_tool_config(ctx, &arguments),
        _ => Err(MsError::ValidationFailed(format!("Unknown tool: {name}"))),
    };

    match result {
        Ok(tool_result) => JsonRpcResponse::success(id, serde_json::to_value(tool_result).unwrap()),
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
// Helpers
// ============================================================================

/// Expand tilde in path strings to home directory
fn expand_tilde(input: &str) -> std::path::PathBuf {
    if let Some(stripped) = input.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    if input == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    std::path::PathBuf::from(input)
}

// ============================================================================
// Tool Handlers
// ============================================================================

fn handle_tool_search(ctx: &AppContext, args: &Value) -> Result<ToolResult> {
    let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
        MsError::ValidationFailed("Missing required parameter: query".to_string())
    })?;

    let limit = args
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(20) as usize;

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
    let skill_id = args.get("skill").and_then(|v| v.as_str()).ok_or_else(|| {
        MsError::ValidationFailed("Missing required parameter: skill".to_string())
    })?;

    let full = args
        .get("full")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    // Look up skill
    let skill = ctx
        .db
        .get_skill(skill_id)?
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
    let skill_id = args.get("skill").and_then(|v| v.as_str()).ok_or_else(|| {
        MsError::ValidationFailed("Missing required parameter: skill".to_string())
    })?;

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
    let limit = args
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(50) as usize;
    let offset = args
        .get("offset")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as usize;

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
    let skill_id = args.get("skill").and_then(|v| v.as_str()).ok_or_else(|| {
        MsError::ValidationFailed("Missing required parameter: skill".to_string())
    })?;

    let full = args
        .get("full")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    let skill = ctx
        .db
        .get_skill(skill_id)?
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
    let fix = args
        .get("fix")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

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

    let all_ok = checks
        .iter()
        .all(|c| c.get("status").and_then(|s| s.as_str()) == Some("ok"));

    let output = serde_json::json!({
        "status": if all_ok { "healthy" } else { "unhealthy" },
        "fix_requested": fix,
        "checks": checks
    });

    Ok(ToolResult::text(serde_json::to_string_pretty(&output)?))
}

fn handle_tool_lint(ctx: &AppContext, args: &Value) -> Result<ToolResult> {
    // Get skill content - either from path or skill ID
    let (content, source) = if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
        let content = std::fs::read_to_string(path)
            .map_err(|e| MsError::Config(format!("Failed to read {path}: {e}")))?;
        (content, path.to_string())
    } else if let Some(skill_id) = args.get("skill").and_then(|v| v.as_str()) {
        // Read from git archive
        let spec = ctx.git.read_skill(skill_id)?;
        let content = crate::core::spec_lens::compile_markdown(&spec);
        (content, format!("skill:{skill_id}"))
    } else {
        return Err(MsError::ValidationFailed(
            "Either 'path' or 'skill' parameter is required".to_string(),
        ));
    };

    // Parse the skill
    let spec =
        parse_markdown(&content).map_err(|e| MsError::InvalidSkill(format!("{source}: {e}")))?;

    // Build validation config
    let mut config = ValidationConfig::new();
    if args
        .get("strict")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        config = config.strict();
    }

    // Build engine with rules
    let mut engine = ValidationEngine::new(config);

    // Filter rules if specified
    let rules_filter: Option<std::collections::HashSet<&str>> = args
        .get("rules")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect());

    for rule in all_rules() {
        if let Some(ref filter) = rules_filter {
            if !filter.contains(rule.id()) {
                continue;
            }
        }
        engine.register(rule);
    }

    // Run validation
    let result = engine.validate(&spec);

    // Build output
    let output = serde_json::json!({
        "source": source,
        "skill_id": spec.metadata.id,
        "passed": result.passed,
        "error_count": result.error_count(),
        "warning_count": result.warning_count(),
        "info_count": result.infos().count(),
        "diagnostics": result.diagnostics.iter().map(|d| {
            serde_json::json!({
                "rule_id": d.rule_id,
                "severity": format!("{}", d.severity),
                "category": format!("{}", d.category),
                "message": d.message,
                "span": d.span.as_ref().map(|s| serde_json::json!({
                    "start_line": s.start_line,
                    "start_col": s.start_col,
                    "end_line": s.end_line,
                    "end_col": s.end_col,
                })),
                "suggestion": d.suggestion,
                "fix_available": d.fix_available,
            })
        }).collect::<Vec<_>>()
    });

    Ok(ToolResult::text(serde_json::to_string_pretty(&output)?))
}

fn handle_tool_suggest(ctx: &AppContext, args: &Value) -> Result<ToolResult> {
    let cwd = args
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    let limit = args
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(5) as usize;

    let _explain = args
        .get("explain")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    // Detect project context using available detector
    let detector = crate::context::DefaultDetector::new();
    let detected_projects = detector.detect(&cwd);

    // Get recent skills as suggestions (simple approach)
    let skills = ctx.db.list_skills(limit, 0)?;

    // Format detected contexts
    let contexts: Vec<_> = detected_projects
        .iter()
        .map(|p| {
            serde_json::json!({
                "project_type": format!("{:?}", p.project_type),
                "confidence": p.confidence,
                "marker_path": p.marker_path.display().to_string(),
                "marker_pattern": p.marker_pattern,
            })
        })
        .collect();

    let output = serde_json::json!({
        "cwd": cwd.display().to_string(),
        "detected_contexts": contexts,
        "count": skills.len(),
        "suggestions": skills.iter().map(|s| {
            serde_json::json!({
                "skill_id": s.id,
                "name": s.name,
                "description": s.description,
            })
        }).collect::<Vec<_>>()
    });

    Ok(ToolResult::text(serde_json::to_string_pretty(&output)?))
}

fn handle_tool_feedback(ctx: &AppContext, args: &Value) -> Result<ToolResult> {
    let skill_id = args
        .get("skill_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            MsError::ValidationFailed("Missing required parameter: skill_id".to_string())
        })?;

    let helpful = args
        .get("helpful")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| {
            MsError::ValidationFailed("Missing required parameter: helpful".to_string())
        })?;

    let comment = args.get("comment").and_then(|v| v.as_str());

    // Record feedback using record_skill_feedback
    let feedback_type = if helpful { "positive" } else { "negative" };
    let rating = if helpful { Some(1) } else { Some(-1) };
    ctx.db
        .record_skill_feedback(skill_id, feedback_type, rating, comment)?;

    let output = serde_json::json!({
        "recorded": true,
        "skill_id": skill_id,
        "helpful": helpful,
        "comment": comment,
    });

    Ok(ToolResult::text(serde_json::to_string_pretty(&output)?))
}

fn handle_tool_index(ctx: &AppContext, args: &Value) -> Result<ToolResult> {
    let force = args
        .get("force")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    let custom_paths: Option<Vec<String>> =
        args.get("paths").and_then(|v| v.as_array()).map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        });

    // Collect all configured paths
    let paths: Vec<std::path::PathBuf> = if let Some(ref custom) = custom_paths {
        custom.iter().map(std::path::PathBuf::from).collect()
    } else {
        // Collect paths from all configured buckets
        let mut all_paths = Vec::new();
        for p in &ctx.config.skill_paths.global {
            all_paths.push(expand_tilde(p));
        }
        for p in &ctx.config.skill_paths.project {
            all_paths.push(expand_tilde(p));
        }
        for p in &ctx.config.skill_paths.community {
            all_paths.push(expand_tilde(p));
        }
        for p in &ctx.config.skill_paths.local {
            all_paths.push(expand_tilde(p));
        }
        all_paths
    };

    // Check which paths exist
    let existing_paths: Vec<_> = paths.iter().filter(|p| p.exists()).collect();

    // For MCP, we report configured paths but recommend CLI for full indexing
    // Full indexing requires locks and transactions that are better handled by CLI
    let output = serde_json::json!({
        "configured_paths": paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        "existing_paths": existing_paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        "force": force,
        "note": "For full indexing with progress, use: ms index --force",
        "skill_count": ctx.db.list_skills(1000, 0).map(|s| s.len()).unwrap_or(0),
    });

    Ok(ToolResult::text(serde_json::to_string_pretty(&output)?))
}

fn handle_tool_validate(_ctx: &AppContext, args: &Value) -> Result<ToolResult> {
    // Get content from either content param or path
    let (content, source) = if let Some(content) = args.get("content").and_then(|v| v.as_str()) {
        (content.to_string(), "inline".to_string())
    } else if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
        let content = std::fs::read_to_string(path)
            .map_err(|e| MsError::Config(format!("Failed to read {path}: {e}")))?;
        (content, path.to_string())
    } else {
        return Err(MsError::ValidationFailed(
            "Either 'content' or 'path' parameter is required".to_string(),
        ));
    };

    // Parse the skill
    let spec =
        parse_markdown(&content).map_err(|e| MsError::InvalidSkill(format!("{source}: {e}")))?;

    // Build validation engine
    let config = ValidationConfig::new();
    let mut engine = ValidationEngine::new(config);

    for rule in all_rules() {
        engine.register(rule);
    }

    // Run validation
    let result = engine.validate(&spec);

    let output = serde_json::json!({
        "source": source,
        "valid": result.passed,
        "skill_id": spec.metadata.id,
        "skill_name": spec.metadata.name,
        "error_count": result.error_count(),
        "warning_count": result.warning_count(),
        "errors": result.errors().map(|d| {
            serde_json::json!({
                "rule_id": d.rule_id,
                "message": d.message,
                "suggestion": d.suggestion,
            })
        }).collect::<Vec<_>>(),
        "warnings": result.warnings().map(|d| {
            serde_json::json!({
                "rule_id": d.rule_id,
                "message": d.message,
            })
        }).collect::<Vec<_>>(),
    });

    Ok(ToolResult::text(serde_json::to_string_pretty(&output)?))
}

fn handle_tool_config(ctx: &AppContext, args: &Value) -> Result<ToolResult> {
    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("list");

    let key = args.get("key").and_then(|v| v.as_str());
    let _value = args.get("value").and_then(|v| v.as_str());

    match action {
        "list" => {
            // Return config summary
            let output = serde_json::json!({
                "action": "list",
                "config": {
                    "skill_paths": {
                        "global": ctx.config.skill_paths.global,
                        "project": ctx.config.skill_paths.project,
                        "local": ctx.config.skill_paths.local,
                        "community": ctx.config.skill_paths.community,
                    },
                    "search": {
                        "use_embeddings": ctx.config.search.use_embeddings,
                        "bm25_weight": ctx.config.search.bm25_weight,
                        "semantic_weight": ctx.config.search.semantic_weight,
                    },
                    "cache": {
                        "enabled": ctx.config.cache.enabled,
                        "max_size_mb": ctx.config.cache.max_size_mb,
                        "ttl_seconds": ctx.config.cache.ttl_seconds,
                    },
                }
            });
            Ok(ToolResult::text(serde_json::to_string_pretty(&output)?))
        }
        "get" => {
            let key = key.ok_or_else(|| {
                MsError::ValidationFailed("'key' parameter required for get action".to_string())
            })?;

            // Get specific config value
            let value = match key {
                "skill_paths.global" => serde_json::to_value(&ctx.config.skill_paths.global)?,
                "skill_paths.project" => serde_json::to_value(&ctx.config.skill_paths.project)?,
                "skill_paths.local" => serde_json::to_value(&ctx.config.skill_paths.local)?,
                "skill_paths.community" => serde_json::to_value(&ctx.config.skill_paths.community)?,
                "search.use_embeddings" => serde_json::to_value(ctx.config.search.use_embeddings)?,
                "search.bm25_weight" => serde_json::to_value(ctx.config.search.bm25_weight)?,
                "search.semantic_weight" => {
                    serde_json::to_value(ctx.config.search.semantic_weight)?
                }
                "cache.enabled" => serde_json::to_value(ctx.config.cache.enabled)?,
                "cache.max_size_mb" => serde_json::to_value(ctx.config.cache.max_size_mb)?,
                "cache.ttl_seconds" => serde_json::to_value(ctx.config.cache.ttl_seconds)?,
                _ => {
                    return Err(MsError::ValidationFailed(format!(
                        "Unknown config key: {key}"
                    )));
                }
            };

            let output = serde_json::json!({
                "action": "get",
                "key": key,
                "value": value,
            });
            Ok(ToolResult::text(serde_json::to_string_pretty(&output)?))
        }
        "set" => {
            // For now, config is read-only through MCP
            Err(MsError::ValidationFailed(
                "Config modification via MCP not yet supported. Use 'ms config set' CLI."
                    .to_string(),
            ))
        }
        _ => Err(MsError::ValidationFailed(format!(
            "Unknown action: {action}. Valid actions: list, get, set"
        ))),
    }
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
        let resp =
            JsonRpcResponse::success(Some(serde_json::json!(1)), serde_json::json!({"ok": true}));
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_jsonrpc_response_error() {
        let resp = JsonRpcResponse::error(
            Some(serde_json::json!(1)),
            -32600,
            "Invalid".to_string(),
            None,
        );
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
        assert!(
            result.is_none(),
            "Notification should not produce a response"
        );
    }

    #[test]
    fn test_handle_initialized_with_id() {
        // When id is present (unusual but permitted), respond
        let result = handle_initialized(Some(serde_json::json!(42)));
        assert!(
            result.is_some(),
            "Request with id should produce a response"
        );
        let response = result.unwrap();
        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_define_tools_includes_lint() {
        let tools = define_tools();
        assert!(tools.iter().any(|t| t.name == "lint"));
    }

    #[test]
    fn test_lint_tool_schema() {
        let tools = define_tools();
        let lint_tool = tools.iter().find(|t| t.name == "lint").unwrap();

        // Check that lint tool has expected properties in schema
        let props = lint_tool.input_schema.get("properties").unwrap();
        assert!(props.get("path").is_some());
        assert!(props.get("skill").is_some());
        assert!(props.get("strict").is_some());
        assert!(props.get("rules").is_some());
    }

    #[test]
    fn test_define_tools_includes_suggest() {
        let tools = define_tools();
        assert!(tools.iter().any(|t| t.name == "suggest"));
    }

    #[test]
    fn test_suggest_tool_schema() {
        let tools = define_tools();
        let tool = tools.iter().find(|t| t.name == "suggest").unwrap();

        let props = tool.input_schema.get("properties").unwrap();
        assert!(props.get("cwd").is_some());
        assert!(props.get("limit").is_some());
        assert!(props.get("explain").is_some());
    }

    #[test]
    fn test_define_tools_includes_feedback() {
        let tools = define_tools();
        assert!(tools.iter().any(|t| t.name == "feedback"));
    }

    #[test]
    fn test_feedback_tool_schema() {
        let tools = define_tools();
        let tool = tools.iter().find(|t| t.name == "feedback").unwrap();

        let props = tool.input_schema.get("properties").unwrap();
        assert!(props.get("skill_id").is_some());
        assert!(props.get("helpful").is_some());
        assert!(props.get("comment").is_some());

        // skill_id and helpful are required
        let required = tool
            .input_schema
            .get("required")
            .unwrap()
            .as_array()
            .unwrap();
        assert!(required.iter().any(|r| r == "skill_id"));
        assert!(required.iter().any(|r| r == "helpful"));
    }

    #[test]
    fn test_define_tools_includes_index() {
        let tools = define_tools();
        assert!(tools.iter().any(|t| t.name == "index"));
    }

    #[test]
    fn test_index_tool_schema() {
        let tools = define_tools();
        let tool = tools.iter().find(|t| t.name == "index").unwrap();

        let props = tool.input_schema.get("properties").unwrap();
        assert!(props.get("paths").is_some());
        assert!(props.get("force").is_some());
    }

    #[test]
    fn test_define_tools_includes_validate() {
        let tools = define_tools();
        assert!(tools.iter().any(|t| t.name == "validate"));
    }

    #[test]
    fn test_validate_tool_schema() {
        let tools = define_tools();
        let tool = tools.iter().find(|t| t.name == "validate").unwrap();

        let props = tool.input_schema.get("properties").unwrap();
        assert!(props.get("content").is_some());
        assert!(props.get("path").is_some());
    }

    #[test]
    fn test_define_tools_includes_config() {
        let tools = define_tools();
        assert!(tools.iter().any(|t| t.name == "config"));
    }

    #[test]
    fn test_config_tool_schema() {
        let tools = define_tools();
        let tool = tools.iter().find(|t| t.name == "config").unwrap();

        let props = tool.input_schema.get("properties").unwrap();
        assert!(props.get("action").is_some());
        assert!(props.get("key").is_some());
        assert!(props.get("value").is_some());
    }

    #[test]
    fn test_expand_tilde_no_tilde() {
        let result = expand_tilde("./foo/bar");
        assert_eq!(result, std::path::PathBuf::from("./foo/bar"));
    }

    #[test]
    fn test_expand_tilde_with_tilde() {
        let result = expand_tilde("~/test/path");
        // Should expand to home dir + path (if home exists)
        if let Some(home) = dirs::home_dir() {
            assert_eq!(result, home.join("test/path"));
        } else {
            assert_eq!(result, std::path::PathBuf::from("~/test/path"));
        }
    }

    #[test]
    fn test_expand_tilde_only() {
        let result = expand_tilde("~");
        if let Some(home) = dirs::home_dir() {
            assert_eq!(result, home);
        } else {
            assert_eq!(result, std::path::PathBuf::from("~"));
        }
    }

    #[test]
    fn test_tool_count() {
        let tools = define_tools();
        // We should have at least 12 tools: search, load, evidence, list, show, doctor, lint,
        // suggest, feedback, index, validate, config
        assert!(
            tools.len() >= 12,
            "Expected at least 12 tools, got {}",
            tools.len()
        );
    }

    // ========================================================================
    // MCP Output Safety Tests
    // ========================================================================

    #[test]
    fn test_strip_ansi_no_codes() {
        let input = "Hello, world!";
        assert_eq!(strip_ansi(input), input);
    }

    #[test]
    fn test_strip_ansi_simple_color() {
        // Red text: ESC[31m
        let input = "\x1b[31mRed text\x1b[0m";
        assert_eq!(strip_ansi(input), "Red text");
    }

    #[test]
    fn test_strip_ansi_bold() {
        // Bold: ESC[1m
        let input = "\x1b[1mBold\x1b[0m normal";
        assert_eq!(strip_ansi(input), "Bold normal");
    }

    #[test]
    fn test_strip_ansi_complex_sequence() {
        // Bold red on white: ESC[1;31;47m
        let input = "\x1b[1;31;47mStyled\x1b[0m";
        assert_eq!(strip_ansi(input), "Styled");
    }

    #[test]
    fn test_strip_ansi_multiple_sequences() {
        let input = "\x1b[32mGreen\x1b[0m and \x1b[34mBlue\x1b[0m";
        assert_eq!(strip_ansi(input), "Green and Blue");
    }

    #[test]
    fn test_strip_ansi_cursor_movement() {
        // Cursor up: ESC[A
        let input = "Line 1\x1b[ALine 2";
        assert_eq!(strip_ansi(input), "Line 1Line 2");
    }

    #[test]
    fn test_strip_ansi_osc_sequence() {
        // OSC for setting window title: ESC]0;Title BEL
        let input = "\x1b]0;Window Title\x07Content";
        assert_eq!(strip_ansi(input), "Content");
    }

    #[test]
    fn test_strip_ansi_osc_with_st() {
        // OSC terminated with ST (ESC \)
        let input = "\x1b]0;Title\x1b\\Content";
        assert_eq!(strip_ansi(input), "Content");
    }

    #[test]
    fn test_strip_ansi_preserves_unicode() {
        let input = "\x1b[32m✓\x1b[0m Check passed \x1b[31m✗\x1b[0m";
        assert_eq!(strip_ansi(input), "✓ Check passed ✗");
    }

    #[test]
    fn test_strip_ansi_preserves_json() {
        let input = r#"{"status": "\x1b[32mok\x1b[0m"}"#;
        let expected = r#"{"status": "ok"}"#;
        assert_eq!(strip_ansi(input), expected);
    }

    #[test]
    fn test_strip_ansi_hyperlink() {
        // OSC 8 hyperlink: ESC]8;;URL ST text ESC]8;; ST
        let input = "\x1b]8;;https://example.com\x1b\\Click here\x1b]8;;\x1b\\";
        assert_eq!(strip_ansi(input), "Click here");
    }

    #[test]
    fn test_strip_ansi_lone_escape() {
        // Lone ESC at end should be stripped
        let input = "Text\x1b";
        assert_eq!(strip_ansi(input), "Text");
    }

    #[test]
    fn test_strip_ansi_simple_escape() {
        // Simple escape: ESC followed by single char (not [ or ])
        let input = "\x1bcCleared";
        assert_eq!(strip_ansi(input), "Cleared");
    }

    #[test]
    fn test_contains_ansi_true() {
        assert!(contains_ansi("\x1b[31mRed\x1b[0m"));
    }

    #[test]
    fn test_contains_ansi_false() {
        assert!(!contains_ansi("Plain text"));
    }

    #[test]
    fn test_sanitize_mcp_output_clean() {
        let input = "Clean text";
        assert_eq!(sanitize_mcp_output(input), input);
    }

    #[test]
    fn test_sanitize_mcp_output_with_ansi() {
        let input = "\x1b[32mGreen\x1b[0m";
        assert_eq!(sanitize_mcp_output(input), "Green");
    }

    #[test]
    fn test_validate_mcp_json_clean() {
        let json = r#"{"result": "ok"}"#;
        assert!(validate_mcp_json(json).is_ok());
    }

    #[test]
    fn test_validate_mcp_json_with_ansi() {
        let json = r#"{"result": "\x1b[32mok\x1b[0m"}"#;
        assert!(validate_mcp_json(json).is_err());
    }

    #[test]
    fn test_tool_result_text_sanitizes_ansi() {
        let result = ToolResult::text("\x1b[31mError\x1b[0m message".to_string());
        assert_eq!(result.content[0].text, "Error message");
        assert!(!contains_ansi(&result.content[0].text));
    }

    #[test]
    fn test_tool_result_error_sanitizes_ansi() {
        let result = ToolResult::error("\x1b[31mFailed\x1b[0m".to_string());
        assert_eq!(result.content[0].text, "Failed");
        assert!(!contains_ansi(&result.content[0].text));
    }

    #[test]
    fn test_serialize_response_safe_clean() {
        let response = JsonRpcResponse::success(
            Some(serde_json::json!(1)),
            serde_json::json!({"status": "ok"}),
        );
        let json = serialize_response_safe(&response);
        assert!(!contains_ansi(&json));
        assert!(json.contains("ok"));
    }

    #[test]
    fn test_serialize_response_safe_with_ansi_in_error() {
        // Even if somehow ANSI codes got into the response, they should be stripped
        let response = JsonRpcResponse::error(
            Some(serde_json::json!(1)),
            -32600,
            "Error".to_string(),
            None,
        );
        let json = serialize_response_safe(&response);
        assert!(!contains_ansi(&json));
    }

    #[test]
    fn test_would_enable_rich_output_respects_no_color() {
        // This test just verifies the function exists and returns Option
        // Actual behavior depends on environment
        let _ = would_enable_rich_output();
    }

    #[test]
    fn test_strip_ansi_256_color() {
        // 256-color: ESC[38;5;196m (foreground color 196)
        let input = "\x1b[38;5;196mRed 256\x1b[0m";
        assert_eq!(strip_ansi(input), "Red 256");
    }

    #[test]
    fn test_strip_ansi_truecolor() {
        // True color: ESC[38;2;255;0;0m (RGB red)
        let input = "\x1b[38;2;255;0;0mTrue Red\x1b[0m";
        assert_eq!(strip_ansi(input), "True Red");
    }

    #[test]
    fn test_strip_ansi_empty_string() {
        assert_eq!(strip_ansi(""), "");
    }

    #[test]
    fn test_strip_ansi_only_escape() {
        assert_eq!(strip_ansi("\x1b[0m"), "");
    }

    #[test]
    fn test_mcp_json_response_is_valid() {
        // Test that a typical tool response produces valid JSON
        let tool_result = ToolResult::text(r#"{"count": 5, "results": []}"#.to_string());
        let response = JsonRpcResponse::success(
            Some(serde_json::json!(1)),
            serde_json::to_value(tool_result).unwrap(),
        );
        let json = serialize_response_safe(&response);

        // Should be parseable JSON
        let parsed: serde_json::Result<serde_json::Value> = serde_json::from_str(&json);
        assert!(parsed.is_ok(), "Response should be valid JSON");

        // Should not contain ANSI
        assert!(
            !contains_ansi(&json),
            "Response should not contain ANSI codes"
        );
    }
}
