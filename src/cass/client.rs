//! CASS CLI client
//!
//! Wraps the CASS CLI for programmatic access using robot mode.
//! Never runs bare cass - always uses --robot/--json for automation.

use std::path::PathBuf;
use std::process::Command;

use fsqlite::Connection;
use fsqlite::compat::{ConnectionExt, OptionalExtension, RowExt};
use serde::{Deserialize, Serialize};

use crate::ms_params as params;

use crate::error::{MsError, Result};
use crate::security::SafetyGate;

/// Client for interacting with CASS (Coding Agent Session Search)
pub struct CassClient {
    /// Path to cass binary (default: "cass")
    cass_bin: PathBuf,

    /// CASS data directory (optional, uses default if not set)
    data_dir: Option<PathBuf>,

    /// Session fingerprint cache for incremental processing
    fingerprint_cache: Option<FingerprintCache>,

    /// Optional safety gate for command execution
    safety: Option<SafetyGate>,
}

impl CassClient {
    /// Create a new CASS client with default settings
    #[must_use]
    pub fn new() -> Self {
        Self {
            cass_bin: PathBuf::from("cass"),
            data_dir: None,
            fingerprint_cache: None,
            safety: None,
        }
    }

    /// Create a CASS client with custom binary path
    pub fn with_binary(binary: impl Into<PathBuf>) -> Self {
        Self {
            cass_bin: binary.into(),
            data_dir: None,
            fingerprint_cache: None,
            safety: None,
        }
    }

    /// Set the CASS data directory
    pub fn with_data_dir(mut self, data_dir: impl Into<PathBuf>) -> Self {
        self.data_dir = Some(data_dir.into());
        self
    }

    /// Set the fingerprint cache for incremental processing
    pub fn with_fingerprint_cache(mut self, cache: FingerprintCache) -> Self {
        self.fingerprint_cache = Some(cache);
        self
    }

    pub fn with_safety(mut self, safety: SafetyGate) -> Self {
        self.safety = Some(safety);
        self
    }

    /// Check if CASS is available and responsive
    pub fn is_available(&self) -> bool {
        let mut cmd = Command::new(&self.cass_bin);
        cmd.arg("--version");
        if let Some(gate) = self.safety.as_ref() {
            let command_str = command_string(&cmd);
            if gate.enforce(&command_str, None).is_err() {
                return false;
            }
        }
        cmd.output().map(|o| o.status.success()).unwrap_or(false)
    }

    /// Get CASS health status
    pub fn health(&self) -> Result<CassHealth> {
        let output = self.run_command(&["health", "--robot"])?;
        serde_json::from_slice(&output)
            .map_err(|e| MsError::CassUnavailable(format!("Failed to parse health output: {e}")))
    }

    /// Search sessions with the given query
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SessionMatch>> {
        let output =
            self.run_command(&["search", query, "--robot", "--limit", &limit.to_string()])?;

        let results: CassSearchResults = serde_json::from_slice(&output)
            .map_err(|e| MsError::CassUnavailable(format!("Failed to parse search output: {e}")))?;

        Ok(results.hits.into_iter().map(normalize_match).collect())
    }

    /// Fetch a full session by its file path.
    ///
    /// cass 0.6.x removed `cass show`; the conversation body is now obtained via
    /// `cass export <path> --format json`, which returns a JSON array of messages.
    pub fn get_session(&self, session_path: &str) -> Result<Session> {
        let output = self.run_command(&["export", session_path, "--format", "json"])?;
        let mut messages: Vec<SessionMessage> = serde_json::from_slice(&output).map_err(|e| {
            MsError::CassUnavailable(format!("Failed to parse session export: {e}"))
        })?;
        // cass export does not emit a per-message index; assign positional indices
        // so downstream consumers that key on `SessionMessage::index`
        // (e.g. mining taint tracking) get stable, distinct values.
        for (i, message) in messages.iter_mut().enumerate() {
            message.index = i;
        }
        Ok(Session {
            id: session_id_from_path(session_path),
            path: session_path.to_string(),
            messages,
            metadata: SessionMetadata::default(),
            content_hash: content_hash_of(&output),
        })
    }

    /// Expand a session with context window
    pub fn expand_session(
        &self,
        session_id: &str,
        context_lines: usize,
    ) -> Result<SessionExpanded> {
        let output = self.run_command(&[
            "expand",
            session_id,
            "--robot",
            "--context",
            &context_lines.to_string(),
        ])?;
        serde_json::from_slice(&output)
            .map_err(|e| MsError::CassUnavailable(format!("Failed to expand session: {e}")))
    }

    /// Get targeted excerpt from session
    pub fn view_excerpt(
        &self,
        session_id: &str,
        start_line: usize,
        end_line: usize,
    ) -> Result<String> {
        let output = self.run_command(&[
            "view",
            session_id,
            "--robot",
            "--start",
            &start_line.to_string(),
            "--end",
            &end_line.to_string(),
        ])?;
        String::from_utf8(output)
            .map_err(|e| MsError::CassUnavailable(format!("Invalid UTF-8 in excerpt: {e}")))
    }

    /// Incremental scan: only return sessions not seen or changed since last scan
    pub fn incremental_sessions(&self, limit: usize) -> Result<Vec<SessionMatch>> {
        let output =
            self.run_command(&["search", "*", "--robot", "--limit", &limit.to_string()])?;

        let results: CassSearchResults = serde_json::from_slice(&output)
            .map_err(|e| MsError::CassUnavailable(format!("Failed to parse search output: {e}")))?;
        let hits: Vec<SessionMatch> = results.hits.into_iter().map(normalize_match).collect();

        // If no fingerprint cache, return all results
        let cache = match &self.fingerprint_cache {
            Some(c) => c,
            None => return Ok(hits),
        };

        // Filter to only new or changed sessions
        let mut delta = Vec::new();
        for m in hits {
            let content_hash = m.content_hash.as_deref().unwrap_or("");
            if cache.is_new_or_changed(&m.session_id, content_hash)? {
                delta.push(m);
            }
        }

        Ok(delta)
    }

    /// Update fingerprint cache after processing a session
    pub fn mark_session_processed(&self, session_id: &str, content_hash: &str) -> Result<()> {
        if let Some(cache) = &self.fingerprint_cache {
            cache.update(session_id, content_hash)?;
        }
        Ok(())
    }

    /// Get CASS capabilities and schema information
    pub fn capabilities(&self) -> Result<CassCapabilities> {
        let output = self.run_command(&["capabilities", "--robot"])?;
        serde_json::from_slice(&output)
            .map_err(|e| MsError::CassUnavailable(format!("Failed to parse capabilities: {e}")))
    }

    /// Get lightweight session metadata.
    ///
    /// cass 0.6.x removed `cass metadata`, so this derives what it can from the
    /// exported conversation (currently the message count).
    pub fn session_metadata(&self, session_path: &str) -> Result<SessionMetadata> {
        let session = self.get_session(session_path)?;
        Ok(SessionMetadata {
            message_count: session.messages.len(),
            ..SessionMetadata::default()
        })
    }

    /// Run a CASS command and return stdout
    fn run_command(&self, args: &[&str]) -> Result<Vec<u8>> {
        if !self.is_available() {
            return Err(MsError::CassUnavailable(
                "CASS binary not found or not executable".into(),
            ));
        }

        let mut cmd = Command::new(&self.cass_bin);
        cmd.args(args);

        // Add data directory if set
        if let Some(ref data_dir) = self.data_dir {
            cmd.args(["--data-dir", &data_dir.to_string_lossy()]);
        }

        if let Some(gate) = self.safety.as_ref() {
            let command_str = command_string(&cmd);
            gate.enforce(&command_str, None)?;
        }

        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let exit_code = output.status.code().unwrap_or(-1);

            // Classify errors
            return Err(classify_cass_error(exit_code, &stderr));
        }

        Ok(output.stdout)
    }
}

fn command_string(cmd: &Command) -> String {
    let program = cmd.get_program().to_string_lossy().to_string();
    let args = cmd
        .get_args()
        .map(|arg| {
            let s = arg.to_string_lossy();
            if s.chars()
                .any(|c| c.is_whitespace() || "()[]{}$|&;<>`'\"*?!".contains(c))
            {
                format!("'{}'", s.replace('\'', "'\\''"))
            } else {
                s.to_string()
            }
        })
        .collect::<Vec<_>>();
    if args.is_empty() {
        program
    } else {
        format!("{program} {}", args.join(" "))
    }
}

impl Default for CassClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Classify CASS errors into actionable categories
fn classify_cass_error(exit_code: i32, stderr: &str) -> MsError {
    let stderr_lower = stderr.to_lowercase();

    // Not found errors (exit code 2 or specific messages)
    if exit_code == 2 || stderr_lower.contains("not found") || stderr_lower.contains("no such") {
        return MsError::SkillNotFound(stderr.to_string());
    }

    // Database/IO errors (transient, retriable)
    if stderr_lower.contains("database") || stderr_lower.contains("locked") {
        return MsError::TransactionFailed(stderr.to_string());
    }

    // Mining/extraction failures
    if stderr_lower.contains("mining") || stderr_lower.contains("extract") {
        return MsError::MiningFailed(stderr.to_string());
    }

    // Default: CASS unavailable/generic error
    MsError::CassUnavailable(format!("CASS command failed (exit {exit_code}): {stderr}"))
}

/// Derive a stable session id from a session file path: the file stem
/// (e.g. `/…/<uuid>.jsonl` → `<uuid>`).
fn session_id_from_path(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .map_or_else(|| path.to_string(), str::to_string)
}

/// Fill in fields cass 0.6.x no longer emits (currently the derived `session_id`).
fn normalize_match(mut m: SessionMatch) -> SessionMatch {
    if m.session_id.is_empty() {
        m.session_id = session_id_from_path(&m.path);
    }
    m
}

/// SHA-256 of the raw export bytes, used as a change-detection fingerprint
/// (cass 0.6.x no longer emits a `content_hash`).
fn content_hash_of(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Deserialize an optional timestamp that cass may emit as an epoch-ms integer
/// (`created_at`) or a string, into `Option<String>`.
fn de_opt_timestamp<'de, D>(deserializer: D) -> std::result::Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(match value {
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::String(s) => Some(s),
        _ => None,
    })
}

/// Deserialize a message string field (`role`, `content`) that cass may emit as
/// a JSON string or `null` (some messages — e.g. tool-only turns — carry no
/// textual body, and some carry no role). `#[serde(default)]` alone does not
/// cover a present-but-`null` value, which would otherwise fail the whole
/// session export with "invalid type: null, expected a string".
fn de_string_or_null<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

// =============================================================================
// Data Types
// =============================================================================

/// CASS search results wrapper
#[derive(Debug, Clone, Deserialize)]
pub struct CassSearchResults {
    #[serde(alias = "matches")]
    pub hits: Vec<SessionMatch>,
    #[serde(default, alias = "total_matches")]
    pub total_count: usize,
    #[serde(default, alias = "hits_clamped")]
    pub truncated: bool,
}

/// A match from CASS search.
///
/// cass 0.6.x emits hits as `{ source_path, score, snippet, workspace, created_at, … }`
/// with no `session_id`. We alias the renamed fields, default everything optional, and
/// derive `session_id` from the path in [`CassClient::search`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMatch {
    /// Session ID. Not emitted by cass 0.6.x — derived from `path` after parsing.
    #[serde(default)]
    pub session_id: String,

    /// Path to session file (cass 0.6.x: `source_path`).
    #[serde(alias = "source_path")]
    pub path: String,

    /// Relevance score
    #[serde(default)]
    pub score: f32,

    /// Preview snippet
    #[serde(default)]
    pub snippet: Option<String>,

    /// Content hash for change detection. Not emitted by cass 0.6.x.
    #[serde(default)]
    pub content_hash: Option<String>,

    /// Project / workspace associated with the session (cass 0.6.x: `workspace`).
    #[serde(default, alias = "workspace")]
    pub project: Option<String>,

    /// Session timestamp. cass 0.6.x emits epoch-ms `created_at`; coerced to a string.
    #[serde(default, alias = "created_at", deserialize_with = "de_opt_timestamp")]
    pub timestamp: Option<String>,
}

/// Full session content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub path: String,
    pub messages: Vec<SessionMessage>,
    pub metadata: SessionMetadata,
    pub content_hash: String,
}

/// A message within a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessage {
    #[serde(default)]
    pub index: usize,
    #[serde(default, deserialize_with = "de_string_or_null")]
    pub role: String,
    #[serde(default, deserialize_with = "de_string_or_null")]
    pub content: String,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default)]
    pub tool_results: Vec<ToolResult>,
}

/// Tool call within a message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Tool result within a message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub content: String,
    pub is_error: bool,
}

/// Session metadata
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionMetadata {
    pub project: Option<String>,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub message_count: usize,
    pub token_count: Option<usize>,
    pub tags: Vec<String>,
}

/// Expanded session with context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionExpanded {
    pub session: Session,
    pub context_before: Vec<SessionMessage>,
    pub context_after: Vec<SessionMessage>,
}

/// CASS health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CassHealth {
    pub healthy: bool,
    pub version: String,
    pub database_ok: bool,
    pub index_ok: bool,
    pub session_count: usize,
    pub last_indexed: Option<String>,
}

/// CASS capabilities and schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CassCapabilities {
    pub version: String,
    pub search_modes: Vec<String>,
    pub output_formats: Vec<String>,
    pub max_results: usize,
    pub supports_incremental: bool,
    pub supports_robot_mode: bool,
}

// =============================================================================
// Fingerprint Cache
// =============================================================================

/// Cache of session fingerprints to avoid reprocessing unchanged sessions
pub struct FingerprintCache {
    conn: Connection,
}

impl FingerprintCache {
    /// Create a new fingerprint cache using the provided `SQLite` connection
    pub const fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Open or create a fingerprint cache at the given path
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self> {
        // fsqlite::Connection::open takes `impl Into<String>` instead of a
        // `Path`, so go through the lossy stringification — the cache path
        // always derives from a UTF-8 `PathBuf`.
        let conn = Connection::open(path.as_ref().to_string_lossy().into_owned())?;

        // Create table if not exists
        conn.execute(
            "CREATE TABLE IF NOT EXISTS cass_fingerprints (
                session_id TEXT PRIMARY KEY,
                content_hash TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )?;

        Ok(Self { conn })
    }

    /// Check if a session is new or has changed since last scan
    pub fn is_new_or_changed(&self, session_id: &str, content_hash: &str) -> Result<bool> {
        let cached_hash: Option<String> = self
            .conn
            .query_row_map(
                "SELECT content_hash FROM cass_fingerprints WHERE session_id = ?",
                params![session_id],
                |row| row.get_typed::<String>(0),
            )
            .optional()?;

        match cached_hash {
            None => Ok(true),                             // New session
            Some(ref h) if h != content_hash => Ok(true), // Changed
            _ => Ok(false),                               // Unchanged
        }
    }

    /// Update the fingerprint for a session
    pub fn update(&self, session_id: &str, content_hash: &str) -> Result<()> {
        self.conn.execute_compat(
            "INSERT INTO cass_fingerprints (session_id, content_hash, updated_at)
             VALUES (?, ?, datetime('now'))
             ON CONFLICT(session_id) DO UPDATE SET
                content_hash = excluded.content_hash,
                updated_at = excluded.updated_at",
            params![session_id, content_hash],
        )?;
        Ok(())
    }

    /// Remove a fingerprint entry
    pub fn remove(&self, session_id: &str) -> Result<()> {
        self.conn.execute_compat(
            "DELETE FROM cass_fingerprints WHERE session_id = ?",
            params![session_id],
        )?;
        Ok(())
    }

    /// Clear all fingerprints (force full rescan)
    pub fn clear(&self) -> Result<()> {
        self.conn.execute("DELETE FROM cass_fingerprints")?;
        Ok(())
    }

    /// Get count of cached fingerprints
    pub fn count(&self) -> Result<usize> {
        let count: i64 = self.conn.query_row_map(
            "SELECT COUNT(*) FROM cass_fingerprints",
            params![],
            |row| row.get_typed::<i64>(0),
        )?;
        Ok(count as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_cass_client_creation() {
        let client = CassClient::new();
        assert_eq!(client.cass_bin, PathBuf::from("cass"));
    }

    #[test]
    fn test_cass_client_builder() {
        let client = CassClient::with_binary("/usr/local/bin/cass").with_data_dir("/data/cass");
        assert_eq!(client.cass_bin, PathBuf::from("/usr/local/bin/cass"));
        assert_eq!(client.data_dir, Some(PathBuf::from("/data/cass")));
    }

    #[test]
    fn test_fingerprint_cache_new_session() {
        let dir = tempdir().unwrap();
        let cache = FingerprintCache::open(dir.path().join("fp.db")).unwrap();

        // New session should return true
        assert!(cache.is_new_or_changed("session-1", "hash-abc").unwrap());
    }

    #[test]
    fn test_fingerprint_cache_unchanged_session() {
        let dir = tempdir().unwrap();
        let cache = FingerprintCache::open(dir.path().join("fp.db")).unwrap();

        // Update fingerprint
        cache.update("session-1", "hash-abc").unwrap();

        // Same hash should return false (unchanged)
        assert!(!cache.is_new_or_changed("session-1", "hash-abc").unwrap());
    }

    #[test]
    fn test_fingerprint_cache_changed_session() {
        let dir = tempdir().unwrap();
        let cache = FingerprintCache::open(dir.path().join("fp.db")).unwrap();

        // Update fingerprint
        cache.update("session-1", "hash-abc").unwrap();

        // Different hash should return true (changed)
        assert!(cache.is_new_or_changed("session-1", "hash-xyz").unwrap());
    }

    #[test]
    fn test_fingerprint_cache_clear() {
        let dir = tempdir().unwrap();
        let cache = FingerprintCache::open(dir.path().join("fp.db")).unwrap();

        cache.update("session-1", "hash-abc").unwrap();
        cache.update("session-2", "hash-def").unwrap();
        assert_eq!(cache.count().unwrap(), 2);

        cache.clear().unwrap();
        assert_eq!(cache.count().unwrap(), 0);
    }

    #[test]
    fn test_error_classification_not_found() {
        let err = classify_cass_error(2, "Session not found: xyz");
        assert!(matches!(err, MsError::SkillNotFound(_)));
    }

    #[test]
    fn test_error_classification_transient() {
        let err = classify_cass_error(1, "Database is locked");
        assert!(matches!(err, MsError::TransactionFailed(_)));
    }

    #[test]
    fn test_error_classification_mining() {
        let err = classify_cass_error(1, "Mining failed: insufficient data");
        assert!(matches!(err, MsError::MiningFailed(_)));
    }

    #[test]
    fn test_error_classification_generic() {
        let err = classify_cass_error(42, "Unknown error");
        assert!(matches!(err, MsError::CassUnavailable(_)));
    }
}
