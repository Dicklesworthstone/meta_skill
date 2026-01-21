//! Unit tests for the agent_mail module.
//!
//! Tests cover:
//! - AgentMailClient configuration validation
//! - InboxMessage serialization/deserialization
//! - Error message formatting

use ms::agent_mail::{AgentMailClient, InboxMessage};
use ms::config::AgentMailConfig;

// ============================================================================
// Configuration Validation Tests
// ============================================================================

#[test]
fn from_config_fails_when_disabled() {
    let config = AgentMailConfig {
        enabled: false,
        endpoint: "http://localhost:3000/mcp".to_string(),
        project_key: "test-project".to_string(),
        agent_name: "test-agent".to_string(),
        timeout_secs: 10,
    };

    let Err(err) = AgentMailClient::from_config(&config) else {
        panic!("Expected error when disabled");
    };
    let err_str = err.to_string();
    assert!(
        err_str.contains("disabled"),
        "Error should mention 'disabled': {err_str}"
    );
}

#[test]
fn from_config_fails_when_endpoint_empty() {
    let config = AgentMailConfig {
        enabled: true,
        endpoint: "".to_string(),
        project_key: "test-project".to_string(),
        agent_name: "test-agent".to_string(),
        timeout_secs: 10,
    };

    let Err(err) = AgentMailClient::from_config(&config) else {
        panic!("Expected error when endpoint empty");
    };
    let err_str = err.to_string();
    assert!(
        err_str.contains("endpoint") && err_str.contains("empty"),
        "Error should mention 'endpoint' and 'empty': {err_str}"
    );
}

#[test]
fn from_config_fails_when_endpoint_whitespace_only() {
    let config = AgentMailConfig {
        enabled: true,
        endpoint: "   \t  ".to_string(),
        project_key: "test-project".to_string(),
        agent_name: "test-agent".to_string(),
        timeout_secs: 10,
    };

    let Err(err) = AgentMailClient::from_config(&config) else {
        panic!("Expected error when endpoint is whitespace");
    };
    let err_str = err.to_string();
    assert!(
        err_str.contains("endpoint"),
        "Error should mention 'endpoint': {err_str}"
    );
}

#[test]
fn from_config_fails_when_project_key_empty() {
    let config = AgentMailConfig {
        enabled: true,
        endpoint: "http://localhost:3000/mcp".to_string(),
        project_key: "".to_string(),
        agent_name: "test-agent".to_string(),
        timeout_secs: 10,
    };

    let Err(err) = AgentMailClient::from_config(&config) else {
        panic!("Expected error when project_key empty");
    };
    let err_str = err.to_string();
    assert!(
        err_str.contains("project_key") && err_str.contains("empty"),
        "Error should mention 'project_key' and 'empty': {err_str}"
    );
}

#[test]
fn from_config_fails_when_project_key_whitespace_only() {
    let config = AgentMailConfig {
        enabled: true,
        endpoint: "http://localhost:3000/mcp".to_string(),
        project_key: "  \n  ".to_string(),
        agent_name: "test-agent".to_string(),
        timeout_secs: 10,
    };

    let Err(err) = AgentMailClient::from_config(&config) else {
        panic!("Expected error when project_key is whitespace");
    };
    let err_str = err.to_string();
    assert!(
        err_str.contains("project_key"),
        "Error should mention 'project_key': {err_str}"
    );
}

#[test]
fn from_config_fails_when_agent_name_empty() {
    let config = AgentMailConfig {
        enabled: true,
        endpoint: "http://localhost:3000/mcp".to_string(),
        project_key: "test-project".to_string(),
        agent_name: "".to_string(),
        timeout_secs: 10,
    };

    let Err(err) = AgentMailClient::from_config(&config) else {
        panic!("Expected error when agent_name empty");
    };
    let err_str = err.to_string();
    assert!(
        err_str.contains("agent_name") && err_str.contains("empty"),
        "Error should mention 'agent_name' and 'empty': {err_str}"
    );
}

#[test]
fn from_config_fails_when_agent_name_whitespace_only() {
    let config = AgentMailConfig {
        enabled: true,
        endpoint: "http://localhost:3000/mcp".to_string(),
        project_key: "test-project".to_string(),
        agent_name: "\t\t".to_string(),
        timeout_secs: 10,
    };

    let Err(err) = AgentMailClient::from_config(&config) else {
        panic!("Expected error when agent_name is whitespace");
    };
    let err_str = err.to_string();
    assert!(
        err_str.contains("agent_name"),
        "Error should mention 'agent_name': {err_str}"
    );
}

// Note: We cannot test successful from_config() without a running server,
// since the MCP client attempts to build an HTTP client. The validation
// tests above cover all error paths in from_config().

// ============================================================================
// InboxMessage Serialization Tests
// ============================================================================

#[test]
fn inbox_message_deserialize_full() {
    let json = r#"{
        "id": 42,
        "subject": "Test Subject",
        "from": "other-agent",
        "created_ts": "2026-01-17T10:00:00Z",
        "importance": "high",
        "ack_required": true,
        "kind": "request",
        "body_md": "Message body content",
        "thread_id": "thread-123"
    }"#;

    let msg: InboxMessage = serde_json::from_str(json).expect("deserialize");
    assert_eq!(msg.id, 42);
    assert_eq!(msg.subject, "Test Subject");
    assert_eq!(msg.from, "other-agent");
    assert_eq!(msg.created_ts, "2026-01-17T10:00:00Z");
    assert_eq!(msg.importance, "high");
    assert!(msg.ack_required);
    assert_eq!(msg.kind, "request");
    assert_eq!(msg.body_md, Some("Message body content".to_string()));
    assert_eq!(msg.thread_id, Some("thread-123".to_string()));
}

#[test]
fn inbox_message_deserialize_minimal() {
    let json = r#"{
        "id": 1,
        "subject": "Minimal",
        "from": "sender",
        "created_ts": "2026-01-17",
        "importance": "normal",
        "ack_required": false,
        "kind": "info"
    }"#;

    let msg: InboxMessage = serde_json::from_str(json).expect("deserialize");
    assert_eq!(msg.id, 1);
    assert_eq!(msg.subject, "Minimal");
    assert_eq!(msg.from, "sender");
    assert_eq!(msg.importance, "normal");
    assert!(!msg.ack_required);
    assert_eq!(msg.kind, "info");
    assert_eq!(msg.body_md, None);
    assert_eq!(msg.thread_id, None);
}

#[test]
fn inbox_message_deserialize_with_null_optionals() {
    let json = r#"{
        "id": 2,
        "subject": "Null optionals",
        "from": "sender",
        "created_ts": "2026-01-17",
        "importance": "low",
        "ack_required": false,
        "kind": "notification",
        "body_md": null,
        "thread_id": null
    }"#;

    let msg: InboxMessage = serde_json::from_str(json).expect("deserialize");
    assert_eq!(msg.id, 2);
    assert_eq!(msg.body_md, None);
    assert_eq!(msg.thread_id, None);
}

#[test]
fn inbox_message_serialize_roundtrip() {
    let msg = InboxMessage {
        id: 100,
        subject: "Roundtrip Test".to_string(),
        from: "test-sender".to_string(),
        created_ts: "2026-01-17T12:00:00Z".to_string(),
        importance: "medium".to_string(),
        ack_required: true,
        kind: "request".to_string(),
        body_md: Some("Body content".to_string()),
        thread_id: Some("thread-456".to_string()),
    };

    let json = serde_json::to_string(&msg).expect("serialize");
    let parsed: InboxMessage = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(parsed.id, msg.id);
    assert_eq!(parsed.subject, msg.subject);
    assert_eq!(parsed.from, msg.from);
    assert_eq!(parsed.created_ts, msg.created_ts);
    assert_eq!(parsed.importance, msg.importance);
    assert_eq!(parsed.ack_required, msg.ack_required);
    assert_eq!(parsed.kind, msg.kind);
    assert_eq!(parsed.body_md, msg.body_md);
    assert_eq!(parsed.thread_id, msg.thread_id);
}

#[test]
fn inbox_message_serialize_without_optionals() {
    let msg = InboxMessage {
        id: 200,
        subject: "No Optionals".to_string(),
        from: "sender".to_string(),
        created_ts: "2026-01-17".to_string(),
        importance: "low".to_string(),
        ack_required: false,
        kind: "info".to_string(),
        body_md: None,
        thread_id: None,
    };

    let json = serde_json::to_string(&msg).expect("serialize");
    let parsed: InboxMessage = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(parsed.body_md, None);
    assert_eq!(parsed.thread_id, None);
}

#[test]
fn inbox_message_deserialize_array() {
    let json = r#"[
        {"id": 1, "subject": "First", "from": "a", "created_ts": "2026-01-17", "importance": "high", "ack_required": true, "kind": "request"},
        {"id": 2, "subject": "Second", "from": "b", "created_ts": "2026-01-16", "importance": "low", "ack_required": false, "kind": "info"}
    ]"#;

    let msgs: Vec<InboxMessage> = serde_json::from_str(json).expect("deserialize");
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[0].id, 1);
    assert_eq!(msgs[0].subject, "First");
    assert_eq!(msgs[1].id, 2);
    assert_eq!(msgs[1].subject, "Second");
}

#[test]
fn inbox_message_clone() {
    let msg = InboxMessage {
        id: 300,
        subject: "Clone Test".to_string(),
        from: "sender".to_string(),
        created_ts: "2026-01-17".to_string(),
        importance: "normal".to_string(),
        ack_required: false,
        kind: "info".to_string(),
        body_md: Some("Body".to_string()),
        thread_id: None,
    };

    let cloned = msg.clone();
    assert_eq!(cloned.id, msg.id);
    assert_eq!(cloned.subject, msg.subject);
    assert_eq!(cloned.body_md, msg.body_md);
}

#[test]
fn inbox_message_debug_format() {
    let msg = InboxMessage {
        id: 400,
        subject: "Debug Test".to_string(),
        from: "sender".to_string(),
        created_ts: "2026-01-17".to_string(),
        importance: "normal".to_string(),
        ack_required: false,
        kind: "info".to_string(),
        body_md: None,
        thread_id: None,
    };

    let debug_str = format!("{msg:?}");
    assert!(debug_str.contains("InboxMessage"));
    assert!(debug_str.contains("400"));
    assert!(debug_str.contains("Debug Test"));
}

// ============================================================================
// AgentMailConfig Tests
// ============================================================================

#[test]
fn agent_mail_config_default_values() {
    let config = AgentMailConfig::default();

    // enabled should be false by default
    assert!(!config.enabled);

    // endpoint should have a default localhost value
    assert!(!config.endpoint.is_empty());
    assert!(config.endpoint.contains("localhost") || config.endpoint.contains("127.0.0.1"));

    // project_key should have a default
    assert!(!config.project_key.is_empty());

    // agent_name should have a default (hostname-based)
    assert!(!config.agent_name.is_empty());

    // timeout_secs should have a reasonable default
    assert!(config.timeout_secs > 0);
}

#[test]
fn agent_mail_config_deserialize_minimal() {
    let toml = r#"
        enabled = true
        endpoint = "https://mail.example.com/mcp"
    "#;

    let config: AgentMailConfig = toml::from_str(toml).expect("deserialize");
    assert!(config.enabled);
    assert_eq!(config.endpoint, "https://mail.example.com/mcp");
    // Other fields should be defaults (empty strings due to serde(default))
}

#[test]
fn agent_mail_config_deserialize_full() {
    let toml = r#"
        enabled = true
        endpoint = "https://mail.example.com/mcp"
        project_key = "my-project"
        agent_name = "my-agent"
        timeout_secs = 30
    "#;

    let config: AgentMailConfig = toml::from_str(toml).expect("deserialize");
    assert!(config.enabled);
    assert_eq!(config.endpoint, "https://mail.example.com/mcp");
    assert_eq!(config.project_key, "my-project");
    assert_eq!(config.agent_name, "my-agent");
    assert_eq!(config.timeout_secs, 30);
}

#[test]
fn agent_mail_config_serialize_roundtrip() {
    let config = AgentMailConfig {
        enabled: true,
        endpoint: "https://test.example.com/mcp".to_string(),
        project_key: "test-project".to_string(),
        agent_name: "test-agent".to_string(),
        timeout_secs: 60,
    };

    let toml_str = toml::to_string(&config).expect("serialize");
    let parsed: AgentMailConfig = toml::from_str(&toml_str).expect("deserialize");

    assert_eq!(parsed.enabled, config.enabled);
    assert_eq!(parsed.endpoint, config.endpoint);
    assert_eq!(parsed.project_key, config.project_key);
    assert_eq!(parsed.agent_name, config.agent_name);
    assert_eq!(parsed.timeout_secs, config.timeout_secs);
}

// ============================================================================
// Edge Cases and Error Handling
// ============================================================================

#[test]
fn inbox_message_deserialize_empty_strings() {
    let json = r#"{
        "id": 500,
        "subject": "",
        "from": "",
        "created_ts": "",
        "importance": "",
        "ack_required": false,
        "kind": ""
    }"#;

    // Should handle empty strings without error
    let msg: InboxMessage = serde_json::from_str(json).expect("deserialize");
    assert_eq!(msg.id, 500);
    assert!(msg.subject.is_empty());
    assert!(msg.from.is_empty());
}

#[test]
fn inbox_message_deserialize_unicode() {
    let json = r#"{
        "id": 600,
        "subject": "你好世界 🌍",
        "from": "エージェント",
        "created_ts": "2026-01-17",
        "importance": "high",
        "ack_required": false,
        "kind": "info",
        "body_md": "Содержимое 📝"
    }"#;

    let msg: InboxMessage = serde_json::from_str(json).expect("deserialize");
    assert_eq!(msg.id, 600);
    assert_eq!(msg.subject, "你好世界 🌍");
    assert_eq!(msg.from, "エージェント");
    assert_eq!(msg.body_md, Some("Содержимое 📝".to_string()));
}

#[test]
fn inbox_message_deserialize_large_id() {
    let json = r#"{
        "id": 9223372036854775807,
        "subject": "Large ID",
        "from": "sender",
        "created_ts": "2026-01-17",
        "importance": "normal",
        "ack_required": false,
        "kind": "info"
    }"#;

    let msg: InboxMessage = serde_json::from_str(json).expect("deserialize");
    assert_eq!(msg.id, i64::MAX);
}

#[test]
fn inbox_message_deserialize_negative_id() {
    let json = r#"{
        "id": -1,
        "subject": "Negative ID",
        "from": "sender",
        "created_ts": "2026-01-17",
        "importance": "normal",
        "ack_required": false,
        "kind": "info"
    }"#;

    let msg: InboxMessage = serde_json::from_str(json).expect("deserialize");
    assert_eq!(msg.id, -1);
}

#[test]
fn inbox_message_deserialize_extra_fields_ignored() {
    let json = r#"{
        "id": 700,
        "subject": "Extra fields",
        "from": "sender",
        "created_ts": "2026-01-17",
        "importance": "normal",
        "ack_required": false,
        "kind": "info",
        "unknown_field": "should be ignored",
        "another_unknown": 12345
    }"#;

    // Extra fields should be ignored during deserialization
    let msg: InboxMessage = serde_json::from_str(json).expect("deserialize");
    assert_eq!(msg.id, 700);
    assert_eq!(msg.subject, "Extra fields");
}

#[test]
fn inbox_message_missing_required_field_fails() {
    let json = r#"{
        "id": 800,
        "from": "sender",
        "created_ts": "2026-01-17",
        "importance": "normal",
        "ack_required": false,
        "kind": "info"
    }"#;

    // Missing "subject" field should fail
    let result: Result<InboxMessage, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

#[test]
fn inbox_message_wrong_type_fails() {
    let json = r#"{
        "id": "not-a-number",
        "subject": "Wrong type",
        "from": "sender",
        "created_ts": "2026-01-17",
        "importance": "normal",
        "ack_required": false,
        "kind": "info"
    }"#;

    // Wrong type for id should fail
    let result: Result<InboxMessage, _> = serde_json::from_str(json);
    assert!(result.is_err());
}

#[test]
fn inbox_message_json_escape_sequences() {
    let json = r#"{
        "id": 900,
        "subject": "Test\nwith\tescapes",
        "from": "sender",
        "created_ts": "2026-01-17",
        "importance": "normal",
        "ack_required": false,
        "kind": "info",
        "body_md": "Line1\nLine2\nLine3"
    }"#;

    let msg: InboxMessage = serde_json::from_str(json).expect("deserialize");
    assert!(msg.subject.contains('\n'));
    assert!(msg.subject.contains('\t'));
    assert!(msg.body_md.unwrap().contains('\n'));
}

#[test]
fn inbox_message_special_characters_in_strings() {
    let json = r#"{
        "id": 1000,
        "subject": "Quote: \"test\" and backslash: \\",
        "from": "sender",
        "created_ts": "2026-01-17",
        "importance": "normal",
        "ack_required": false,
        "kind": "info"
    }"#;

    let msg: InboxMessage = serde_json::from_str(json).expect("deserialize");
    assert!(msg.subject.contains('"'));
    assert!(msg.subject.contains('\\'));
}
