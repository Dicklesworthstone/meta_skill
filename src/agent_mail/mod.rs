use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::AgentMailConfig;
use crate::error::{MsError, Result};

const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

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

    pub fn agent_name(&self) -> &str {
        &self.agent_name
    }

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

impl McpClient {
    fn new(endpoint: &str, timeout_secs: u64) -> Result<Self> {
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

        response.result.ok_or_else(|| {
            MsError::Config(format!(
                "agent mail empty response for {}",
                method
            ))
        })
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
        .and_then(|flag| flag.as_bool())
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
        return Ok(value);
    };
    for item in content {
        let Some(text) = item.get("text").and_then(|t| t.as_str()) else {
            continue;
        };
        if let Ok(parsed) = serde_json::from_str::<Value>(text) {
            return Ok(parsed);
        }
    }
    Ok(value)
}
