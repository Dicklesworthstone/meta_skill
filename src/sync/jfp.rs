//! JFP Cloud Sync Client
//!
//! Handles communication with the JeffreysPrompts Premium Cloud API for skill synchronization.
//! Supports push/pull operations, conflict resolution, tombstones, and idempotency keys.

use std::collections::HashMap;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::core::SkillSpec;
use crate::error::{MsError, Result};

/// JFP Cloud sync protocol version.
pub const JFP_PROTOCOL_VERSION: &str = "1.0";

/// Default JFP Cloud API base URL.
pub const JFP_DEFAULT_BASE_URL: &str = "https://pro.jeffreysprompts.com/api/ms/sync";

/// Retry configuration for cloud operations.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
    pub jitter_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 500,
            max_delay_ms: 30_000,
            jitter_factor: 0.25,
        }
    }
}

/// Device information for JFP Cloud.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JfpDeviceInfo {
    pub device_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    pub client_version: String,
}

impl JfpDeviceInfo {
    /// Create device info from current system.
    pub fn from_system(device_id: &str, name: &str) -> Self {
        let platform = std::env::consts::OS.to_string();
        let client_version = env!("CARGO_PKG_VERSION").to_string();
        Self {
            device_id: device_id.to_string(),
            name: name.to_string(),
            platform: Some(platform),
            client_version,
        }
    }
}

/// Sync cursor for tracking pagination.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JfpSyncCursor {
    pub value: String,
}

/// Skill payload for JFP Cloud.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JfpSkillPayload {
    pub ms_skill_id: String,
    pub format_version: String,
    pub content_hash: String,
    pub spec: serde_json::Value,
    pub skill_md: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_device_id: Option<String>,
}

/// Tombstone entry for deleted skills.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JfpSkillTombstone {
    pub ms_skill_id: String,
    pub deleted_at: String,
    pub content_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision_id: Option<i64>,
}

/// Handshake request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JfpHandshakeRequest {
    pub protocol_version: String,
    pub client_version: String,
    pub device: JfpDeviceInfo,
}

/// Handshake response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JfpHandshakeResponse {
    pub server_time: String,
    pub min_protocol_version: String,
    pub protocol_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<JfpSyncCursor>,
    pub capabilities: JfpCapabilities,
    pub device: JfpDeviceInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upgrade_url: Option<String>,
}

/// Server capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JfpCapabilities {
    #[serde(default)]
    pub supports_assets: bool,
    #[serde(default = "default_true")]
    pub supports_tombstones: bool,
    #[serde(default = "default_true")]
    pub supports_dry_run: bool,
    #[serde(default = "default_true")]
    pub supports_conflict_hints: bool,
}

fn default_true() -> bool {
    true
}

/// Pull changes request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JfpPullRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<JfpSyncCursor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub if_none_match: Option<String>,
}

/// Pull changes response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JfpPullResponse {
    pub server_time: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<JfpSyncCursor>,
    pub has_more: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    pub skills: Vec<JfpSkillPayload>,
    #[serde(default)]
    pub tombstones: Vec<JfpSkillTombstone>,
}

/// Push item for upload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JfpPushItem {
    pub ms_skill_id: String,
    pub content_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_revision_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spec: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_md: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted: Option<bool>,
}

/// Push request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JfpPushRequest {
    pub idempotency_key: String,
    pub client_txn_id: String,
    #[serde(default)]
    pub dry_run: bool,
    pub items: Vec<JfpPushItem>,
}

/// Push item result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JfpPushItemResult {
    pub ms_skill_id: String,
    pub status: JfpPushStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conflict: Option<JfpConflict>,
}

/// Push status.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JfpPushStatus {
    Applied,
    Skipped,
    Conflict,
    Rejected,
}

/// Conflict details.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JfpConflict {
    pub ms_skill_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_revision_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_content_hash: Option<String>,
    pub client_content_hash: String,
    pub reason: JfpConflictReason,
}

/// Conflict reason.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JfpConflictReason {
    BaseRevisionMismatch,
    ContentConflict,
}

/// Push response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JfpPushResponse {
    pub server_time: String,
    pub results: Vec<JfpPushItemResult>,
}

/// API error response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JfpErrorResponse {
    pub error: JfpError,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_time: Option<String>,
}

/// API error details.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JfpError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Cloud sync state for persistent caching.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JfpCloudState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_server_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_etag: Option<String>,
    #[serde(default)]
    pub skill_revisions: HashMap<String, i64>,
    #[serde(default)]
    pub pending_queue: Vec<JfpPendingChange>,
}

/// Pending change for offline queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JfpPendingChange {
    pub skill_id: String,
    pub change_type: JfpChangeType,
    pub content_hash: String,
    pub queued_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_revision_id: Option<i64>,
}

/// Change type for pending queue.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JfpChangeType {
    Create,
    Update,
    Delete,
}

/// JFP Cloud client for API interactions.
pub struct JfpCloudClient {
    base_url: String,
    token: String,
    device: JfpDeviceInfo,
    retry_config: RetryConfig,
    http_client: reqwest::blocking::Client,
    request_id: String,
}

impl JfpCloudClient {
    /// Create a new JFP Cloud client.
    pub fn new(
        base_url: Option<&str>,
        token: &str,
        device: JfpDeviceInfo,
    ) -> Result<Self> {
        let base_url = base_url.unwrap_or(JFP_DEFAULT_BASE_URL).to_string();
        let http_client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| MsError::Config(format!("HTTP client error: {e}")))?;
        let request_id = format!("ms-{}", Uuid::new_v4().to_string().split('-').next().unwrap_or("0"));

        Ok(Self {
            base_url,
            token: token.to_string(),
            device,
            retry_config: RetryConfig::default(),
            http_client,
            request_id,
        })
    }

    /// Generate a new request ID.
    fn new_request_id(&mut self) -> String {
        self.request_id = format!("ms-{}", Uuid::new_v4().to_string().split('-').next().unwrap_or("0"));
        self.request_id.clone()
    }

    /// Perform handshake with server.
    pub fn handshake(&mut self) -> Result<JfpHandshakeResponse> {
        let request_id = self.new_request_id();
        info!(request_id = %request_id, device_id = %self.device.device_id, "JFP Cloud handshake");

        let request = JfpHandshakeRequest {
            protocol_version: JFP_PROTOCOL_VERSION.to_string(),
            client_version: self.device.client_version.clone(),
            device: self.device.clone(),
        };

        let response = self.post("/handshake", &request)?;
        let handshake: JfpHandshakeResponse = serde_json::from_str(&response)
            .map_err(|e| MsError::Config(format!("Invalid handshake response: {e}")))?;

        debug!(
            request_id = %request_id,
            server_time = %handshake.server_time,
            protocol = %handshake.protocol_version,
            "Handshake successful"
        );

        Ok(handshake)
    }

    /// Pull changes from server.
    pub fn pull_changes(
        &mut self,
        cursor: Option<&str>,
        limit: Option<u32>,
        if_none_match: Option<&str>,
    ) -> Result<JfpPullResponse> {
        let request_id = self.new_request_id();
        info!(
            request_id = %request_id,
            cursor = ?cursor,
            limit = ?limit,
            "Pulling changes from JFP Cloud"
        );

        let request = JfpPullRequest {
            cursor: cursor.map(|v| JfpSyncCursor { value: v.to_string() }),
            limit,
            if_none_match: if_none_match.map(String::from),
        };

        let response = self.post("/changes", &request)?;
        let pull_response: JfpPullResponse = serde_json::from_str(&response)
            .map_err(|e| MsError::Config(format!("Invalid pull response: {e}")))?;

        info!(
            request_id = %request_id,
            skills = pull_response.skills.len(),
            tombstones = pull_response.tombstones.len(),
            has_more = pull_response.has_more,
            "Pull completed"
        );

        Ok(pull_response)
    }

    /// Push changes to server.
    pub fn push_changes(
        &mut self,
        items: Vec<JfpPushItem>,
        dry_run: bool,
    ) -> Result<JfpPushResponse> {
        let request_id = self.new_request_id();
        let idempotency_key = format!("ms-push-{}", Uuid::new_v4());
        let client_txn_id = format!("txn-{}-{}", chrono::Utc::now().timestamp_millis(), &idempotency_key[9..17]);

        info!(
            request_id = %request_id,
            idempotency_key = %idempotency_key,
            items = items.len(),
            dry_run = dry_run,
            "Pushing changes to JFP Cloud"
        );

        let request = JfpPushRequest {
            idempotency_key,
            client_txn_id,
            dry_run,
            items,
        };

        let response = self.post("/push", &request)?;
        let push_response: JfpPushResponse = serde_json::from_str(&response)
            .map_err(|e| MsError::Config(format!("Invalid push response: {e}")))?;

        let applied = push_response.results.iter().filter(|r| r.status == JfpPushStatus::Applied).count();
        let conflicts = push_response.results.iter().filter(|r| r.status == JfpPushStatus::Conflict).count();

        info!(
            request_id = %request_id,
            applied = applied,
            conflicts = conflicts,
            total = push_response.results.len(),
            "Push completed"
        );

        Ok(push_response)
    }

    /// Make a POST request with retry logic.
    fn post<T: Serialize>(&self, endpoint: &str, body: &T) -> Result<String> {
        let url = format!("{}{}", self.base_url, endpoint);
        let mut last_error = None;

        for attempt in 0..=self.retry_config.max_retries {
            if attempt > 0 {
                let delay = self.calculate_delay(attempt);
                debug!(
                    attempt = attempt,
                    delay_ms = delay.as_millis(),
                    "Retrying request"
                );
                std::thread::sleep(delay);
            }

            match self.do_post(&url, body) {
                Ok(response) => return Ok(response),
                Err(e) => {
                    warn!(
                        attempt = attempt,
                        error = %e,
                        "Request failed"
                    );
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| MsError::Config("Unknown error".to_string())))
    }

    fn do_post<T: Serialize>(&self, url: &str, body: &T) -> Result<String> {
        let response = self.http_client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .header("X-Request-ID", &self.request_id)
            .header("X-Device-ID", &self.device.device_id)
            .header("X-Protocol-Version", JFP_PROTOCOL_VERSION)
            .json(body)
            .send()
            .map_err(|e| MsError::Config(format!("HTTP request failed: {e}")))?;

        let status = response.status();
        let body = response.text()
            .map_err(|e| MsError::Config(format!("Failed to read response: {e}")))?;

        if status.is_success() {
            Ok(body)
        } else if status.as_u16() == 429 {
            // Rate limited
            let error: JfpErrorResponse = serde_json::from_str(&body).ok()
                .unwrap_or(JfpErrorResponse {
                    error: JfpError {
                        code: "rate_limited".to_string(),
                        message: "Rate limited".to_string(),
                        retry_after_seconds: Some(30),
                        details: None,
                    },
                    request_id: None,
                    server_time: None,
                });
            Err(MsError::Config(format!(
                "Rate limited: {} (retry after {} seconds)",
                error.error.message,
                error.error.retry_after_seconds.unwrap_or(30)
            )))
        } else {
            let error: std::result::Result<JfpErrorResponse, _> = serde_json::from_str(&body);
            match error {
                Ok(e) => Err(MsError::Config(format!(
                    "JFP Cloud error [{}]: {}",
                    e.error.code, e.error.message
                ))),
                Err(_) => Err(MsError::Config(format!(
                    "JFP Cloud error ({}): {}",
                    status, body
                ))),
            }
        }
    }

    fn calculate_delay(&self, attempt: u32) -> Duration {
        let base = self.retry_config.base_delay_ms as f64;
        let exp_delay = base * 2_f64.powi(attempt as i32);
        let capped = exp_delay.min(self.retry_config.max_delay_ms as f64);

        // Add jitter
        let jitter_range = capped * self.retry_config.jitter_factor;
        let jitter = (rand::random::<f64>() - 0.5) * 2.0 * jitter_range;
        let final_delay = (capped + jitter).max(0.0);

        Duration::from_millis(final_delay as u64)
    }
}

/// Convert a SkillSpec to JfpSkillPayload.
pub fn skill_spec_to_payload(spec: &SkillSpec, device_id: &str) -> Result<JfpSkillPayload> {
    let spec_json = serde_json::to_value(spec)
        .map_err(|e| MsError::Config(format!("Failed to serialize spec: {e}")))?;
    let content_hash = hash_spec_json(&spec_json)?;

    Ok(JfpSkillPayload {
        ms_skill_id: spec.metadata.id.clone(),
        format_version: spec.format_version.clone(),
        content_hash,
        spec: spec_json,
        skill_md: String::new(), // Would need to be read from SKILL.md file
        version: Some(spec.metadata.version.clone()),
        updated_at: Some(chrono::Utc::now().to_rfc3339()),
        source_device_id: Some(device_id.to_string()),
    })
}

/// Convert JfpSkillPayload to SkillSpec.
pub fn payload_to_skill_spec(payload: &JfpSkillPayload) -> Result<SkillSpec> {
    serde_json::from_value(payload.spec.clone())
        .map_err(|e| MsError::Config(format!("Failed to deserialize spec: {e}")))
}

/// Hash spec JSON for content comparison.
pub fn hash_spec_json(spec: &serde_json::Value) -> Result<String> {
    let json = serde_json::to_vec(spec)
        .map_err(|e| MsError::Config(format!("Failed to serialize spec: {e}")))?;
    let mut hasher = Sha256::new();
    hasher.update(&json);
    Ok(format!("{:x}", hasher.finalize()))
}

/// Create a push item from a SkillSpec.
pub fn create_push_item(
    spec: &SkillSpec,
    base_revision_id: Option<i64>,
    deleted: bool,
) -> Result<JfpPushItem> {
    let spec_json = serde_json::to_value(spec)
        .map_err(|e| MsError::Config(format!("Failed to serialize spec: {e}")))?;
    let content_hash = hash_spec_json(&spec_json)?;

    Ok(JfpPushItem {
        ms_skill_id: spec.metadata.id.clone(),
        content_hash,
        base_revision_id,
        spec: if deleted { None } else { Some(spec_json) },
        skill_md: None, // Would need to be read from SKILL.md file
        deleted: if deleted { Some(true) } else { None },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_version() {
        assert_eq!(JFP_PROTOCOL_VERSION, "1.0");
    }

    #[test]
    fn test_retry_config_defaults() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.base_delay_ms, 500);
    }

    #[test]
    fn test_device_info_from_system() {
        let device = JfpDeviceInfo::from_system("test-device", "Test Machine");
        assert_eq!(device.device_id, "test-device");
        assert_eq!(device.name, "Test Machine");
        assert!(device.platform.is_some());
    }

    #[test]
    fn test_push_status_serialization() {
        let applied = serde_json::to_string(&JfpPushStatus::Applied).unwrap();
        assert_eq!(applied, "\"applied\"");

        let conflict = serde_json::to_string(&JfpPushStatus::Conflict).unwrap();
        assert_eq!(conflict, "\"conflict\"");
    }

    #[test]
    fn test_conflict_reason_serialization() {
        let reason = serde_json::to_string(&JfpConflictReason::BaseRevisionMismatch).unwrap();
        assert_eq!(reason, "\"base_revision_mismatch\"");
    }
}
