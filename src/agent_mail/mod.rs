use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::AgentMailConfig;
use crate::error::{MsError, Result};

const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

#[derive(Debug)]
pub struct AgentMailClient {
    mcp: McpClient,
    project_key: String,
    agent_name: String,
}

impl AgentMailClient {
    pub fn from_config(config: &AgentMailConfig) -> Result<Self> {
        if !config.enabled {
            return Err(MsError::Config(
                "agent mail is disabled; set [agent_mail].enabled=true".to_string(),
            ));
        }
        if config.endpoint.trim().is_empty() {
            return Err(MsError::Config(
                "agent mail endpoint is empty; set [agent_mail].endpoint".to_string(),
            ));
        }
        if config.project_key.trim().is_empty() {
            return Err(MsError::Config(
                "agent mail project_key is empty; set [agent_mail].project_key".to_string(),
            ));
        }
        if config.agent_name.trim().is_empty() {
            return Err(MsError::Config(
                "agent mail agent_name is empty; set [agent_mail].agent_name".to_string(),
            ));
        }
        let mcp = McpClient::new(&config.endpoint, config.timeout_secs)?;
        Ok(Self {
            mcp,
            project_key: config.project_key.clone(),
            agent_name: config.agent_name.clone(),
        })
    }

    #[must_use]
    pub fn agent_name(&self) -> &str {
        &self.agent_name
    }

    #[must_use]
    pub fn project_key(&self) -> &str {
        &self.project_key
    }

    pub fn fetch_inbox(&mut self, limit: usize, include_bodies: bool) -> Result<Vec<InboxMessage>> {
        let args = serde_json::json!({
            "project_key": self.project_key,
            "agent_name": self.agent_name,
            "limit": limit,
            "include_bodies": include_bodies,
        });
        let value = self.mcp.call_tool("fetch_inbox", args)?;
        let value = unwrap_tool_result(value)?;
        let messages: Vec<InboxMessage> = serde_json::from_value(value)?;
        Ok(messages)
    }

    pub fn acknowledge(&mut self, message_id: i64) -> Result<()> {
        let args = serde_json::json!({
            "project_key": self.project_key,
            "agent_name": self.agent_name,
            "message_id": message_id,
        });
        let value = self.mcp.call_tool("acknowledge_message", args)?;
        let _ = unwrap_tool_result(value)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboxMessage {
    pub id: i64,
    pub subject: String,
    pub from: String,
    pub created_ts: String,
    pub importance: String,
    pub ack_required: bool,
    pub kind: String,
    #[serde(default)]
    pub body_md: Option<String>,
    #[serde(default)]
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: u64,
    method: String,
    params: Value,
}

#[derive(Debug, Clone, Serialize)]
struct JsonRpcNotification {
    jsonrpc: String,
    method: String,
    params: Value,
}

#[derive(Debug, Clone, Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: Option<Value>,
    result: Option<Value>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[allow(dead_code)]
    data: Option<Value>,
}

struct McpClient {
    endpoint: String,
    client: reqwest::blocking::Client,
    next_id: u64,
    initialized: bool,
}

impl std::fmt::Debug for McpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpClient")
            .field("endpoint", &self.endpoint)
            .field("next_id", &self.next_id)
            .field("initialized", &self.initialized)
            .finish_non_exhaustive()
    }
}

impl McpClient {
    fn new(endpoint: &str, timeout_secs: u64) -> Result<Self> {
        if endpoint.starts_with("http://") {
            tracing::warn!(
                "Agent mail endpoint uses unencrypted HTTP. Credentials will be sent in plain text."
            );
        }

        let timeout = Duration::from_secs(timeout_secs.max(1));
        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|err| MsError::Config(format!("agent mail http client: {err}")))?;
        Ok(Self {
            endpoint: endpoint.to_string(),
            client,
            next_id: 1,
            initialized: false,
        })
    }

    fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value> {
        self.ensure_initialized()?;
        self.call_method(
            "tools/call",
            serde_json::json!({
                "name": name,
                "arguments": arguments,
            }),
        )
    }

    fn ensure_initialized(&mut self) -> Result<()> {
        if self.initialized {
            return Ok(());
        }
        let params = serde_json::json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "clientInfo": {
                "name": "ms",
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": {
                "tools": {
                    "listChanged": false
                }
            }
        });
        let _ = self.call_method("initialize", params)?;
        self.send_notification("initialized", serde_json::json!({}))?;
        self.initialized = true;
        Ok(())
    }

    fn call_method(&mut self, method: &str, params: Value) -> Result<Value> {
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: self.next_id,
            method: method.to_string(),
            params,
        };
        self.next_id = self.next_id.saturating_add(1);

        let response = self
            .client
            .post(&self.endpoint)
            .json(&request)
            .send()
            .map_err(|err| MsError::Config(format!("agent mail request failed: {err}")))?;

        if !response.status().is_success() {
            return Err(MsError::Config(format!(
                "agent mail HTTP {}",
                response.status()
            )));
        }

        let response: JsonRpcResponse = response
            .json()
            .map_err(|err| MsError::Config(format!("agent mail response parse: {err}")))?;

        if let Some(error) = response.error {
            return Err(MsError::Config(format!(
                "agent mail error {}: {}",
                error.code, error.message
            )));
        }

        response
            .result
            .ok_or_else(|| MsError::Config(format!("agent mail empty response for {method}")))
    }

    fn send_notification(&self, method: &str, params: Value) -> Result<()> {
        let request = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
        };
        let response = self
            .client
            .post(&self.endpoint)
            .json(&request)
            .send()
            .map_err(|err| MsError::Config(format!("agent mail notify failed: {err}")))?;
        if !response.status().is_success() {
            return Err(MsError::Config(format!(
                "agent mail notify HTTP {}",
                response.status()
            )));
        }
        Ok(())
    }
}

fn unwrap_tool_result(value: Value) -> Result<Value> {
    if value
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        let message = value
            .get("content")
            .and_then(|content| content.as_array())
            .and_then(|items| items.iter().find_map(|item| item.get("text")))
            .and_then(|text| text.as_str())
            .unwrap_or("agent mail tool error");
        return Err(MsError::Config(message.to_string()));
    }
    let Some(content) = value.get("content").and_then(|c| c.as_array()) else {
        return Err(MsError::Config(
            "agent mail response missing content array".to_string(),
        ));
    };
    for item in content {
        let Some(text) = item.get("text").and_then(|t| t.as_str()) else {
            continue;
        };
        if let Ok(parsed) = serde_json::from_str::<Value>(text) {
            return Ok(parsed);
        }
    }

    // If we reach here, we found content but no valid JSON in text fields.
    // This is unexpected for our tools which should return JSON.
    Err(MsError::Config(
        "agent mail response contained no valid JSON payload".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ============================================
    // InboxMessage Serialization/Deserialization Tests
    // ============================================

    #[test]
    fn inbox_message_deserialize_full() {
        let json = json!({
            "id": 42,
            "subject": "Test Subject",
            "from": "agent-alpha",
            "created_ts": "2024-01-15T10:30:00Z",
            "importance": "high",
            "ack_required": true,
            "kind": "notification",
            "body_md": "# Hello\nThis is a message.",
            "thread_id": "thread-123"
        });
        let msg: InboxMessage = serde_json::from_value(json).unwrap();
        assert_eq!(msg.id, 42);
        assert_eq!(msg.subject, "Test Subject");
        assert_eq!(msg.from, "agent-alpha");
        assert_eq!(msg.created_ts, "2024-01-15T10:30:00Z");
        assert_eq!(msg.importance, "high");
        assert!(msg.ack_required);
        assert_eq!(msg.kind, "notification");
        assert_eq!(msg.body_md.as_deref(), Some("# Hello\nThis is a message."));
        assert_eq!(msg.thread_id.as_deref(), Some("thread-123"));
    }

    #[test]
    fn inbox_message_deserialize_minimal() {
        let json = json!({
            "id": 1,
            "subject": "Minimal",
            "from": "sender",
            "created_ts": "2024-01-01T00:00:00Z",
            "importance": "low",
            "ack_required": false,
            "kind": "info"
        });
        let msg: InboxMessage = serde_json::from_value(json).unwrap();
        assert_eq!(msg.id, 1);
        assert_eq!(msg.subject, "Minimal");
        assert!(msg.body_md.is_none());
        assert!(msg.thread_id.is_none());
    }

    #[test]
    fn inbox_message_serialize_roundtrip() {
        let original = InboxMessage {
            id: 99,
            subject: "Round Trip Test".to_string(),
            from: "test-agent".to_string(),
            created_ts: "2024-06-01T12:00:00Z".to_string(),
            importance: "medium".to_string(),
            ack_required: true,
            kind: "request".to_string(),
            body_md: Some("Body content here".to_string()),
            thread_id: Some("thread-abc".to_string()),
        };
        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: InboxMessage = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized.id, original.id);
        assert_eq!(deserialized.subject, original.subject);
        assert_eq!(deserialized.from, original.from);
        assert_eq!(deserialized.body_md, original.body_md);
        assert_eq!(deserialized.thread_id, original.thread_id);
    }

    #[test]
    fn inbox_message_with_empty_strings() {
        let json = json!({
            "id": 0,
            "subject": "",
            "from": "",
            "created_ts": "",
            "importance": "",
            "ack_required": false,
            "kind": "",
            "body_md": "",
            "thread_id": ""
        });
        let msg: InboxMessage = serde_json::from_value(json).unwrap();
        assert_eq!(msg.id, 0);
        assert_eq!(msg.subject, "");
        assert_eq!(msg.body_md.as_deref(), Some(""));
    }

    #[test]
    fn inbox_message_with_special_characters() {
        let json = json!({
            "id": 1,
            "subject": "Test with \"quotes\" and \n newlines",
            "from": "agent<>@test",
            "created_ts": "2024-01-01",
            "importance": "high",
            "ack_required": true,
            "kind": "test",
            "body_md": "Unicode: \u{1F600} emoji and 日本語"
        });
        let msg: InboxMessage = serde_json::from_value(json).unwrap();
        assert!(msg.subject.contains("quotes"));
        assert!(msg.body_md.unwrap().contains('\u{1F600}'));
    }

    #[test]
    fn inbox_message_negative_id() {
        let json = json!({
            "id": -1,
            "subject": "Negative ID",
            "from": "test",
            "created_ts": "2024-01-01",
            "importance": "low",
            "ack_required": false,
            "kind": "test"
        });
        let msg: InboxMessage = serde_json::from_value(json).unwrap();
        assert_eq!(msg.id, -1);
    }

    #[test]
    fn inbox_message_large_id() {
        let json = json!({
            "id": i64::MAX,
            "subject": "Large ID",
            "from": "test",
            "created_ts": "2024-01-01",
            "importance": "low",
            "ack_required": false,
            "kind": "test"
        });
        let msg: InboxMessage = serde_json::from_value(json).unwrap();
        assert_eq!(msg.id, i64::MAX);
    }

    // ============================================
    // unwrap_tool_result Tests
    // ============================================

    #[test]
    fn unwrap_tool_result_success_simple() {
        let value = json!({
            "content": [{
                "type": "text",
                "text": "{\"result\": \"success\"}"
            }]
        });
        let result = unwrap_tool_result(value).unwrap();
        assert_eq!(result["result"], "success");
    }

    #[test]
    fn unwrap_tool_result_success_with_array() {
        let value = json!({
            "content": [{
                "type": "text",
                "text": "[1, 2, 3]"
            }]
        });
        let result = unwrap_tool_result(value).unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn unwrap_tool_result_error_flagged() {
        let value = json!({
            "isError": true,
            "content": [{
                "type": "text",
                "text": "Something went wrong"
            }]
        });
        let err = unwrap_tool_result(value).unwrap_err();
        assert!(err.to_string().contains("Something went wrong"));
    }

    #[test]
    fn unwrap_tool_result_error_no_message() {
        let value = json!({
            "isError": true,
            "content": []
        });
        let err = unwrap_tool_result(value).unwrap_err();
        assert!(err.to_string().contains("agent mail tool error"));
    }

    #[test]
    fn unwrap_tool_result_missing_content() {
        let value = json!({
            "status": "ok"
        });
        let err = unwrap_tool_result(value).unwrap_err();
        assert!(err.to_string().contains("missing content array"));
    }

    #[test]
    fn unwrap_tool_result_content_not_array() {
        let value = json!({
            "content": "not an array"
        });
        let err = unwrap_tool_result(value).unwrap_err();
        assert!(err.to_string().contains("missing content array"));
    }

    #[test]
    fn unwrap_tool_result_empty_content() {
        let value = json!({
            "content": []
        });
        let err = unwrap_tool_result(value).unwrap_err();
        assert!(err.to_string().contains("no valid JSON payload"));
    }

    #[test]
    fn unwrap_tool_result_content_no_text_field() {
        let value = json!({
            "content": [{
                "type": "image",
                "data": "base64..."
            }]
        });
        let err = unwrap_tool_result(value).unwrap_err();
        assert!(err.to_string().contains("no valid JSON payload"));
    }

    #[test]
    fn unwrap_tool_result_text_not_json() {
        let value = json!({
            "content": [{
                "type": "text",
                "text": "plain text, not json"
            }]
        });
        let err = unwrap_tool_result(value).unwrap_err();
        assert!(err.to_string().contains("no valid JSON payload"));
    }

    #[test]
    fn unwrap_tool_result_multiple_content_items() {
        let value = json!({
            "content": [
                {"type": "text", "text": "not json"},
                {"type": "text", "text": "{\"found\": true}"}
            ]
        });
        let result = unwrap_tool_result(value).unwrap();
        assert_eq!(result["found"], true);
    }

    #[test]
    fn unwrap_tool_result_first_valid_json_wins() {
        let value = json!({
            "content": [
                {"type": "text", "text": "{\"first\": 1}"},
                {"type": "text", "text": "{\"second\": 2}"}
            ]
        });
        let result = unwrap_tool_result(value).unwrap();
        assert_eq!(result["first"], 1);
        assert!(result.get("second").is_none());
    }

    #[test]
    fn unwrap_tool_result_nested_json() {
        let nested = json!({
            "outer": {
                "inner": {
                    "deep": [1, 2, 3]
                }
            }
        });
        let value = json!({
            "content": [{
                "type": "text",
                "text": nested.to_string()
            }]
        });
        let result = unwrap_tool_result(value).unwrap();
        assert_eq!(result["outer"]["inner"]["deep"], json!([1, 2, 3]));
    }

    #[test]
    fn unwrap_tool_result_is_error_false() {
        let value = json!({
            "isError": false,
            "content": [{
                "type": "text",
                "text": "{\"ok\": true}"
            }]
        });
        let result = unwrap_tool_result(value).unwrap();
        assert_eq!(result["ok"], true);
    }

    // ============================================
    // JSON-RPC Structure Tests
    // ============================================

    #[test]
    fn json_rpc_request_serialize() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "test_method".to_string(),
            params: json!({"key": "value"}),
        };
        let serialized = serde_json::to_value(&req).unwrap();
        assert_eq!(serialized["jsonrpc"], "2.0");
        assert_eq!(serialized["id"], 1);
        assert_eq!(serialized["method"], "test_method");
        assert_eq!(serialized["params"]["key"], "value");
    }

    #[test]
    fn json_rpc_notification_serialize() {
        let notif = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: "initialized".to_string(),
            params: json!({}),
        };
        let serialized = serde_json::to_value(&notif).unwrap();
        assert_eq!(serialized["jsonrpc"], "2.0");
        assert_eq!(serialized["method"], "initialized");
        assert!(serialized.get("id").is_none());
    }

    #[test]
    fn json_rpc_response_deserialize_success() {
        let json = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {"status": "ok"}
        });
        let resp: JsonRpcResponse = serde_json::from_value(json).unwrap();
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
        assert_eq!(resp.result.unwrap()["status"], "ok");
    }

    #[test]
    fn json_rpc_response_deserialize_error() {
        let json = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": -32600,
                "message": "Invalid Request",
                "data": {"detail": "missing field"}
            }
        });
        let resp: JsonRpcResponse = serde_json::from_value(json).unwrap();
        assert!(resp.result.is_none());
        assert!(resp.error.is_some());
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32600);
        assert_eq!(err.message, "Invalid Request");
    }

    #[test]
    fn json_rpc_response_null_id() {
        let json = json!({
            "jsonrpc": "2.0",
            "id": null,
            "result": "ok"
        });
        let resp: JsonRpcResponse = serde_json::from_value(json).unwrap();
        assert!(resp.result.is_some());
    }

    #[test]
    fn json_rpc_error_minimal() {
        let json = json!({
            "code": -32700,
            "message": "Parse error"
        });
        let err: JsonRpcError = serde_json::from_value(json).unwrap();
        assert_eq!(err.code, -32700);
        assert_eq!(err.message, "Parse error");
        assert!(err.data.is_none());
    }

    // ============================================
    // AgentMailClient::from_config Validation Tests
    // ============================================

    #[test]
    fn from_config_disabled() {
        let config = AgentMailConfig {
            enabled: false,
            endpoint: "http://localhost:3000".to_string(),
            project_key: "test".to_string(),
            agent_name: "test-agent".to_string(),
            timeout_secs: 10,
        };
        let result = AgentMailClient::from_config(&config);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("disabled"));
    }

    #[test]
    fn from_config_empty_endpoint() {
        let config = AgentMailConfig {
            enabled: true,
            endpoint: "".to_string(),
            project_key: "test".to_string(),
            agent_name: "test-agent".to_string(),
            timeout_secs: 10,
        };
        let result = AgentMailClient::from_config(&config);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("endpoint"));
    }

    #[test]
    fn from_config_whitespace_endpoint() {
        let config = AgentMailConfig {
            enabled: true,
            endpoint: "   ".to_string(),
            project_key: "test".to_string(),
            agent_name: "test-agent".to_string(),
            timeout_secs: 10,
        };
        let result = AgentMailClient::from_config(&config);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("endpoint"));
    }

    #[test]
    fn from_config_empty_project_key() {
        let config = AgentMailConfig {
            enabled: true,
            endpoint: "http://localhost:3000".to_string(),
            project_key: "".to_string(),
            agent_name: "test-agent".to_string(),
            timeout_secs: 10,
        };
        let result = AgentMailClient::from_config(&config);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("project_key"));
    }

    #[test]
    fn from_config_whitespace_project_key() {
        let config = AgentMailConfig {
            enabled: true,
            endpoint: "http://localhost:3000".to_string(),
            project_key: "  \t ".to_string(),
            agent_name: "test-agent".to_string(),
            timeout_secs: 10,
        };
        let result = AgentMailClient::from_config(&config);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("project_key"));
    }

    #[test]
    fn from_config_empty_agent_name() {
        let config = AgentMailConfig {
            enabled: true,
            endpoint: "http://localhost:3000".to_string(),
            project_key: "test".to_string(),
            agent_name: "".to_string(),
            timeout_secs: 10,
        };
        let result = AgentMailClient::from_config(&config);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("agent_name"));
    }

    #[test]
    fn from_config_whitespace_agent_name() {
        let config = AgentMailConfig {
            enabled: true,
            endpoint: "http://localhost:3000".to_string(),
            project_key: "test".to_string(),
            agent_name: "\n\t".to_string(),
            timeout_secs: 10,
        };
        let result = AgentMailClient::from_config(&config);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("agent_name"));
    }

    // ============================================
    // MCP Protocol Constant Tests
    // ============================================

    #[test]
    fn mcp_protocol_version_format() {
        // Ensure the protocol version is in expected date format
        assert!(MCP_PROTOCOL_VERSION.contains('-'));
        let parts: Vec<&str> = MCP_PROTOCOL_VERSION.split('-').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].len(), 4); // Year
        assert_eq!(parts[1].len(), 2); // Month
        assert_eq!(parts[2].len(), 2); // Day
    }
}
