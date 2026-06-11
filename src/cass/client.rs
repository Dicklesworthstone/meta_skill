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
    ///
    /// The export shape is a flat array of `{role, content, timestamp, author}`
    /// objects (NOT the raw Claude Code JSONL). cass renders every tool
    /// invocation as an inline text marker inside `content`, e.g.
    /// `[Tool: Read - /path/to/file]` or `[Tool: Bash - <description>]`, rather
    /// than emitting structured tool fields. The pattern miner
    /// ([`crate::cass::mining`]) and the quality scorer
    /// ([`crate::cass::quality`]) both key off the structured
    /// [`SessionMessage::tool_calls`] / [`SessionMessage::tool_results`]
    /// collections, so a straight deserialize leaves those empty and yields
    /// zero patterns even though sessions are processed (issue #114). We
    /// therefore reconstruct `tool_calls` from the inline markers so the
    /// existing mining/quality logic sees tool activity again.
    pub fn get_session(&self, session_path: &str) -> Result<Session> {
        // `--include-tools` makes cass emit the inline `[Tool: …]` markers we
        // reconstruct below; without it some export paths omit them entirely.
        let output = self.run_command(&[
            "export",
            session_path,
            "--format",
            "json",
            "--include-tools",
        ])?;
        let mut messages: Vec<SessionMessage> = serde_json::from_slice(&output).map_err(|e| {
            MsError::CassUnavailable(format!("Failed to parse session export: {e}"))
        })?;
        // cass export does not emit a per-message index; assign positional indices
        // so downstream consumers that key on `SessionMessage::index`
        // (e.g. mining taint tracking) get stable, distinct values.
        for (i, message) in messages.iter_mut().enumerate() {
            message.index = i;
            // Recover structured tool calls from cass's inline `[Tool: …]`
            // markers so mining/quality see tool activity (issue #114).
            if message.tool_calls.is_empty() {
                message.tool_calls = parse_inline_tool_markers(&message.content, i);
            }
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

/// Reconstruct structured [`ToolCall`]s from cass's inline `[Tool: …]` markers.
///
/// cass 0.6.x renders tool invocations as plain-text markers inside a message's
/// `content` instead of structured fields. The two observed shapes are:
/// `[Tool: <Name>]` (name only, e.g. `[Tool: Grep]`) and
/// `[Tool: <Name> - <detail>]`, where `<detail>` is a file path for
/// file-oriented tools (`Read`/`Edit`/`Write`/`NotebookEdit`) and a
/// human-readable description for shell tools (`Bash`/`Shell`/…). A single
/// message may contain several markers.
///
/// We map the recovered detail into whichever argument key the miner reads:
/// `command` for shell tools (mining keys command/error patterns and bash
/// phase classification on `arguments.command`) and `file_path` for
/// file-mutating tools (code-change detection / error-resolution steps key on
/// `arguments.file_path`). `msg_index` is woven into a synthetic, unique id so
/// taint/evidence tracking sees distinct calls. Markers are best-effort: the
/// raw shell command is unrecoverable (cass emits only the description), so the
/// reconstruction is necessarily lossy but restores enough structure for the
/// existing extractors to produce patterns again (issue #114).
fn parse_inline_tool_markers(content: &str, msg_index: usize) -> Vec<ToolCall> {
    let mut calls = Vec::new();
    let mut rest = content;
    let mut seq = 0usize;

    while let Some(start) = rest.find("[Tool:") {
        // Advance past the matched prefix, then find the closing bracket.
        let after_prefix = &rest[start + "[Tool:".len()..];
        let Some(end_rel) = after_prefix.find(']') else {
            break;
        };
        let inner = after_prefix[..end_rel].trim();
        rest = &after_prefix[end_rel + 1..];

        if inner.is_empty() {
            continue;
        }

        // Split "<Name>" or "<Name> - <detail>".
        let (name, detail) = match inner.split_once(" - ") {
            Some((n, d)) => (n.trim(), Some(d.trim())),
            None => (inner, None),
        };
        if name.is_empty() {
            continue;
        }

        let name_lower = name.to_lowercase();
        let arguments = match (name_lower.as_str(), detail) {
            // Shell tools: the detail is a description, but mining reads it from
            // `arguments.command`, so surface it there.
            ("bash" | "shell" | "command" | "terminal" | "exec", Some(d)) => {
                serde_json::json!({ "command": d })
            }
            // File-oriented tools: the detail is a file path.
            ("read" | "edit" | "write" | "notebookedit" | "multiedit", Some(d)) => {
                serde_json::json!({ "file_path": d })
            }
            // Any other tool with a detail: keep it under a generic key.
            (_, Some(d)) => serde_json::json!({ "detail": d }),
            // Name-only markers (e.g. `[Tool: Grep]`).
            (_, None) => serde_json::json!({}),
        };

        calls.push(ToolCall {
            id: format!("inline_{msg_index}_{seq}"),
            name: name.to_string(),
            arguments,
        });
        seq += 1;
    }

    calls
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

    /// Regression test for issue #114: a representative cass 0.6.x
    /// `export --format json` payload must deserialize into correctly-roled,
    /// positionally-indexed messages, tolerate null `role`/`content`, and
    /// reconstruct structured `tool_calls` from cass's inline `[Tool: …]`
    /// markers so the pattern miner sees tool activity.
    #[test]
    fn test_parse_cass_export_recovers_messages_and_tool_calls() {
        // Mirrors the real cass 0.6.x export shape: a flat array of
        // {role, content, timestamp, author}. Includes a plain user turn, an
        // assistant turn whose content carries multiple inline tool markers,
        // and a tool-only turn with BOTH `role` and `content` present-but-null.
        let raw = serde_json::json!([
            {
                "role": "user",
                "content": "Fix the failing tests in the auth module",
                "timestamp": 1771881674174i64
            },
            {
                "role": "assistant",
                "content": "Let me look at the file and run the tests.\n[Tool: Read - /src/auth/mod.rs]\n[Tool: Bash - Run the auth test suite and show output]\n[Tool: Grep]",
                "timestamp": 1771881676620i64,
                "author": "claude-opus-4-6"
            },
            {
                "role": null,
                "content": null,
                "timestamp": 1771881680000i64
            }
        ]);
        let bytes = serde_json::to_vec(&raw).unwrap();

        // Parse exactly as `get_session` does (sans the subprocess call).
        let mut messages: Vec<SessionMessage> = serde_json::from_slice(&bytes).unwrap();
        for (i, message) in messages.iter_mut().enumerate() {
            message.index = i;
            if message.tool_calls.is_empty() {
                message.tool_calls = parse_inline_tool_markers(&message.content, i);
            }
        }

        // Three messages survived (null role/content tolerated, not dropped).
        assert_eq!(messages.len(), 3, "all records should deserialize");

        // Positional indexing.
        assert_eq!(messages[0].index, 0);
        assert_eq!(messages[1].index, 1);
        assert_eq!(messages[2].index, 2);

        // Roles and content correctly extracted from the flat shape.
        assert_eq!(messages[0].role, "user");
        assert_eq!(
            messages[0].content,
            "Fix the failing tests in the auth module"
        );
        assert_eq!(messages[1].role, "assistant");
        assert!(messages[1].content.contains("run the tests"));

        // Null role/content coerced to empty strings (not a hard parse failure).
        assert_eq!(messages[2].role, "");
        assert_eq!(messages[2].content, "");

        // Inline `[Tool: …]` markers reconstructed into structured tool_calls.
        let tcs = &messages[1].tool_calls;
        assert_eq!(tcs.len(), 3, "Read, Bash and Grep markers recovered");

        let read = tcs.iter().find(|t| t.name == "Read").expect("Read call");
        assert_eq!(
            read.arguments.get("file_path").and_then(|v| v.as_str()),
            Some("/src/auth/mod.rs"),
            "file path routed to arguments.file_path"
        );

        let bash = tcs.iter().find(|t| t.name == "Bash").expect("Bash call");
        assert_eq!(
            bash.arguments.get("command").and_then(|v| v.as_str()),
            Some("Run the auth test suite and show output"),
            "shell detail routed to arguments.command"
        );

        let grep = tcs.iter().find(|t| t.name == "Grep").expect("Grep call");
        assert!(
            grep.arguments
                .as_object()
                .is_some_and(serde_json::Map::is_empty),
            "name-only marker has empty arguments"
        );

        // Synthetic ids are unique within the message.
        let ids: std::collections::HashSet<_> = tcs.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids.len(), 3, "tool_call ids are distinct");
    }

    /// The marker parser must be robust to malformed / partial markers and to
    /// content with no markers at all.
    #[test]
    fn test_parse_inline_tool_markers_edge_cases() {
        // No markers -> no calls.
        assert!(parse_inline_tool_markers("just some prose", 0).is_empty());
        // Unterminated marker -> no panic, no calls.
        assert!(parse_inline_tool_markers("[Tool: Bash - never closed", 0).is_empty());
        // Empty marker body -> skipped.
        assert!(parse_inline_tool_markers("[Tool: ]", 0).is_empty());
        // Two valid markers back to back.
        let calls = parse_inline_tool_markers("[Tool: Edit - /a/b.rs][Tool: Write - /c/d.rs]", 7);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "Edit");
        assert_eq!(
            calls[0].arguments.get("file_path").and_then(|v| v.as_str()),
            Some("/a/b.rs")
        );
        assert!(calls[0].id.starts_with("inline_7_"));
    }
}
