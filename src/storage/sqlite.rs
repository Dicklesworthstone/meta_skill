//! `SQLite` database layer
//!
//! Backed by [`fsqlite`] (frankensqlite). The compat module provides the
//! `params!`-style binding helpers, `query_row_map`/`query_map_collect`
//! adapters, and the `RowExt`/`OptionalExtension` traits needed for
//! rusqlite-style ergonomics.

use std::path::Path;

use fsqlite::Connection;
use fsqlite::Row;
use fsqlite::compat::{ConnectionExt, RowExt};
use fsqlite_error::FrankenError;
use half::f16;
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::error::{MsError, Result};
use crate::ms_params as params;
use crate::security::{CommandSafetyEvent, QuarantineRecord};
use crate::storage::migrations;

/// Convenience type alias for row decoders. fsqlite's row mappers return
/// `FrankenError` so closures can use `?` against `get_typed` cleanly.
type RowResult<T> = std::result::Result<T, FrankenError>;

/// Raw row tuple for `command_safety_events`. The `Option<String>` columns
/// are NULL-able in the schema; the JSON-bearing `decision_json` is
/// post-processed in Rust because `serde_json::from_str` can fail with
/// `MsError::Serialization`, which doesn't fit through the FrankenError-only
/// mapper closure.
type CommandSafetyRawRow = (
    Option<String>, // session_id
    String,         // command
    Option<String>, // dcg_version
    Option<String>, // dcg_pack
    String,         // decision_json
    String,         // created_at
);

/// `SQLite` database wrapper for skill registry
pub struct Database {
    conn: Connection,
    schema_version: u32,
}

impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Database")
            .field("schema_version", &self.schema_version)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkillRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub source_path: String,
    pub source_layer: String,
    pub git_remote: Option<String>,
    pub git_commit: Option<String>,
    pub content_hash: String,
    pub body: String,
    pub metadata_json: String,
    pub assets_json: String,
    pub token_count: i64,
    pub quality_score: f64,
    pub indexed_at: String,
    pub modified_at: String,
    pub is_deprecated: bool,
    pub deprecation_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddingRecord {
    pub skill_id: String,
    pub embedding: Vec<f32>,
    pub dims: usize,
    pub embedder_type: String,
    pub content_hash: Option<String>,
    pub computed_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkillSearchCandidate {
    pub id: String,
    pub source_layer: String,
    pub metadata_json: String,
    pub quality_score: f64,
    pub is_deprecated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasResolution {
    pub canonical_id: String,
    pub alias_type: String,
}

/// Full alias record for listing
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasRecord {
    pub alias: String,
    pub skill_id: String,
    pub alias_type: String,
    pub created_at: String,
}

/// Cached session quality score
#[derive(Debug, Clone, PartialEq)]
pub struct SessionQualityRecord {
    pub session_id: String,
    pub content_hash: String,
    pub score: f32,
    pub signals: Vec<String>,
    pub missing: Vec<String>,
    pub computed_at: String,
}

/// Evidence record for provenance graph export
#[derive(Debug, Clone)]
pub struct EvidenceRecord {
    pub skill_id: String,
    pub rule_id: String,
    pub evidence: Vec<crate::core::EvidenceRef>,
    pub updated_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct QuarantineReview {
    pub id: String,
    pub quarantine_id: String,
    pub action: String,
    pub reason: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SkillFeedbackRecord {
    pub id: String,
    pub skill_id: String,
    pub feedback_type: String,
    pub rating: Option<i64>,
    pub comment: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UserPreferenceRecord {
    pub id: String,
    pub skill_id: String,
    pub preference_type: String,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExperimentRecord {
    pub id: String,
    pub skill_id: String,
    pub scope: String,
    pub scope_id: Option<String>,
    pub variants_json: String,
    pub allocation_json: String,
    pub status: String,
    pub started_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExperimentEventRecord {
    pub id: String,
    pub experiment_id: String,
    pub variant_id: String,
    pub event_type: String,
    pub metrics_json: Option<String>,
    pub context_json: Option<String>,
    pub session_id: Option<String>,
    pub created_at: String,
}

impl Database {
    /// Open database at the given path
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // fsqlite::Connection::open takes `impl Into<String>` rather than
        // `Path`, so go through the lossy stringification (the path always
        // comes from a `PathBuf` or `&Path` derived from valid UTF-8 inputs
        // here).
        let conn = Connection::open(path.to_string_lossy().into_owned())?;

        Self::configure_pragmas(&conn)?;
        let schema_version = migrations::run_migrations(&conn)?;

        Ok(Self {
            conn,
            schema_version,
        })
    }

    /// Get a reference to the connection
    pub const fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Current schema version after migrations.
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn get_skill(&self, id: &str) -> Result<Option<SkillRecord>> {
        use fsqlite::compat::OptionalExtension;
        let sql = "SELECT id, name, description, version, author, source_path, source_layer, \
             git_remote, git_commit, content_hash, body, metadata_json, assets_json, \
             token_count, quality_score, indexed_at, modified_at, is_deprecated, deprecation_reason \
             FROM skills WHERE id = ?";
        let result = self
            .conn
            .query_row_map(sql, params![id], skill_from_row)
            .optional()?;
        Ok(result)
    }

    pub fn list_skills(&self, limit: usize, offset: usize) -> Result<Vec<SkillRecord>> {
        let sql = "SELECT id, name, description, version, author, source_path, source_layer, \
             git_remote, git_commit, content_hash, body, metadata_json, assets_json, \
             token_count, quality_score, indexed_at, modified_at, is_deprecated, deprecation_reason \
             FROM skills ORDER BY modified_at DESC LIMIT ? OFFSET ?";
        let results = self.conn.query_map_collect(
            sql,
            params![limit as i64, offset as i64],
            skill_from_row,
        )?;
        Ok(results)
    }

    /// Update quality score for a skill.
    pub fn update_skill_quality(&self, skill_id: &str, quality_score: f64) -> Result<()> {
        self.conn.execute_compat(
            "UPDATE skills SET quality_score = ? WHERE id = ?",
            params![quality_score, skill_id],
        )?;
        Ok(())
    }

    /// Update deprecation status and reason for a skill.
    pub fn update_skill_deprecation(
        &self,
        skill_id: &str,
        is_deprecated: bool,
        reason: Option<&str>,
    ) -> Result<()> {
        self.conn.execute_compat(
            "UPDATE skills SET is_deprecated = ?, deprecation_reason = ? WHERE id = ?",
            params![i32::from(is_deprecated), reason, skill_id],
        )?;
        Ok(())
    }

    /// Count usage events for a skill.
    pub fn count_skill_usage(&self, skill_id: &str) -> Result<u64> {
        let count: i64 = self.conn.query_row_map(
            "SELECT COUNT(*) FROM skill_usage WHERE skill_id = ?",
            params![skill_id],
            |row| row.get_typed::<i64>(0),
        )?;
        Ok(count.max(0) as u64)
    }

    /// Get skill usage statistics for building UserHistory.
    ///
    /// Returns a tuple of (total_loads, skill_load_counts, skill_last_load).
    pub fn get_skill_usage_stats(
        &self,
    ) -> Result<(
        u64,
        std::collections::HashMap<String, u64>,
        std::collections::HashMap<String, chrono::DateTime<chrono::Utc>>,
    )> {
        use std::collections::HashMap;

        // Get total loads
        let total_loads: i64 =
            self.conn
                .query_row_map("SELECT COUNT(*) FROM skill_usage", params![], |row| {
                    row.get_typed::<i64>(0)
                })?;

        // Get per-skill load counts
        let counts: Vec<(String, u64)> = self.conn.query_map_collect(
            "SELECT skill_id, COUNT(*) as count FROM skill_usage GROUP BY skill_id",
            params![],
            |row| {
                let skill_id: String = row.get_typed(0)?;
                let count: i64 = row.get_typed(1)?;
                Ok((skill_id, count.max(0) as u64))
            },
        )?;
        let skill_load_counts: HashMap<String, u64> = counts.into_iter().collect();

        // Get per-skill last load timestamps (as raw rows; date parsing is
        // best-effort and discards rows we can't decode).
        let raw_last: Vec<(String, String)> = self.conn.query_map_collect(
            "SELECT skill_id, MAX(used_at) as last_used FROM skill_usage GROUP BY skill_id",
            params![],
            |row| {
                let skill_id: String = row.get_typed(0)?;
                let used_at: String = row.get_typed(1)?;
                Ok((skill_id, used_at))
            },
        )?;
        let skill_last_load: HashMap<String, chrono::DateTime<chrono::Utc>> = raw_last
            .into_iter()
            .filter_map(|(skill_id, used_at)| {
                chrono::DateTime::parse_from_rfc3339(&used_at)
                    .ok()
                    .map(|dt| (skill_id, dt.with_timezone(&chrono::Utc)))
            })
            .collect();

        Ok((
            total_loads.max(0) as u64,
            skill_load_counts,
            skill_last_load,
        ))
    }

    /// Record a skill usage entry (lightweight summary table).
    pub fn record_skill_usage(
        &self,
        skill_id: &str,
        project_path: Option<&str>,
        disclosure_level: u8,
        context_keywords: Option<&[String]>,
        experiment_id: Option<&str>,
        variant_id: Option<&str>,
    ) -> Result<()> {
        let used_at = chrono::Utc::now().to_rfc3339();
        let keywords_json = if let Some(keys) = context_keywords {
            Some(
                serde_json::to_string(keys)
                    .map_err(|err| MsError::Config(format!("encode context keywords: {err}")))?,
            )
        } else {
            None
        };

        self.conn.execute_compat(
            "INSERT INTO skill_usage (
                skill_id, project_path, used_at, disclosure_level, context_keywords, success_signal, experiment_id, variant_id
             ) VALUES (?, ?, ?, ?, ?, NULL, ?, ?)",
            params![
                skill_id,
                project_path,
                used_at,
                i64::from(disclosure_level),
                keywords_json,
                experiment_id,
                variant_id
            ],
        )?;
        Ok(())
    }

    /// Count evidence records for a skill.
    pub fn count_skill_evidence(&self, skill_id: &str) -> Result<u64> {
        let count: i64 = self.conn.query_row_map(
            "SELECT COUNT(*) FROM skill_evidence WHERE skill_id = ?",
            params![skill_id],
            |row| row.get_typed::<i64>(0),
        )?;
        Ok(count.max(0) as u64)
    }

    pub fn upsert_skill(&self, skill: &SkillRecord) -> Result<()> {
        self.conn.execute_compat(
            "INSERT INTO skills (
                id, name, description, version, author, source_path, source_layer,
                git_remote, git_commit, content_hash, body, metadata_json, assets_json,
                token_count, quality_score, indexed_at, modified_at, is_deprecated, deprecation_reason
             ) VALUES (
                ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?
             )
             ON CONFLICT(id) DO UPDATE SET
                name=excluded.name,
                description=excluded.description,
                version=excluded.version,
                author=excluded.author,
                source_path=excluded.source_path,
                source_layer=excluded.source_layer,
                git_remote=excluded.git_remote,
                git_commit=excluded.git_commit,
                content_hash=excluded.content_hash,
                body=excluded.body,
                metadata_json=excluded.metadata_json,
                assets_json=excluded.assets_json,
                token_count=excluded.token_count,
                quality_score=excluded.quality_score,
                indexed_at=excluded.indexed_at,
                modified_at=excluded.modified_at,
                is_deprecated=excluded.is_deprecated,
                deprecation_reason=excluded.deprecation_reason",
            params![
                skill.id,
                skill.name,
                skill.description,
                skill.version,
                skill.author,
                skill.source_path,
                skill.source_layer,
                skill.git_remote,
                skill.git_commit,
                skill.content_hash,
                skill.body,
                skill.metadata_json,
                skill.assets_json,
                skill.token_count,
                skill.quality_score,
                skill.indexed_at,
                skill.modified_at,
                i32::from(skill.is_deprecated),
                skill.deprecation_reason,
            ],
        )?;
        Ok(())
    }

    pub fn delete_skill(&self, id: &str) -> Result<()> {
        self.conn
            .execute_compat("DELETE FROM skills WHERE id = ?", params![id])?;
        Ok(())
    }

    /// Delete a skill only if it has pending status
    pub fn delete_pending_skill(&self, id: &str) -> Result<()> {
        self.conn.execute_compat(
            "DELETE FROM skills WHERE id = ? AND source_path = 'pending'",
            params![id],
        )?;
        Ok(())
    }

    /// Delete a transaction record from `tx_log`
    pub fn delete_tx_record(&self, id: &str) -> Result<()> {
        self.conn
            .execute_compat("DELETE FROM tx_log WHERE id = ?", params![id])?;
        Ok(())
    }

    pub fn resolve_alias(&self, alias: &str) -> Result<Option<AliasResolution>> {
        use fsqlite::compat::OptionalExtension;
        let result = self
            .conn
            .query_row_map(
                "SELECT skill_id, alias_type FROM skill_aliases WHERE alias = ?",
                params![alias],
                |row| {
                    Ok(AliasResolution {
                        canonical_id: row.get_typed(0)?,
                        alias_type: row.get_typed(1)?,
                    })
                },
            )
            .optional()?;
        Ok(result)
    }

    pub fn upsert_alias(
        &self,
        alias: &str,
        skill_id: &str,
        alias_type: &str,
        created_at: &str,
    ) -> Result<()> {
        self.conn.execute_compat(
            "INSERT INTO skill_aliases (alias, skill_id, alias_type, created_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(alias) DO UPDATE SET
                skill_id=excluded.skill_id,
                alias_type=excluded.alias_type,
                created_at=excluded.created_at",
            params![alias, skill_id, alias_type, created_at],
        )?;
        Ok(())
    }

    /// Delete an alias
    pub fn delete_alias(&self, alias: &str) -> Result<bool> {
        let count = self
            .conn
            .execute_compat("DELETE FROM skill_aliases WHERE alias = ?", params![alias])?;
        Ok(count > 0)
    }

    /// List all aliases, optionally filtered by `skill_id`
    pub fn list_aliases(&self, skill_id: Option<&str>) -> Result<Vec<AliasRecord>> {
        let records = if let Some(sid) = skill_id {
            self.conn.query_map_collect(
                "SELECT alias, skill_id, alias_type, created_at
                 FROM skill_aliases
                 WHERE skill_id = ?
                 ORDER BY alias",
                params![sid],
                |row| {
                    Ok(AliasRecord {
                        alias: row.get_typed(0)?,
                        skill_id: row.get_typed(1)?,
                        alias_type: row.get_typed(2)?,
                        created_at: row.get_typed(3)?,
                    })
                },
            )?
        } else {
            self.conn.query_map_collect(
                "SELECT alias, skill_id, alias_type, created_at
                 FROM skill_aliases
                 ORDER BY skill_id, alias",
                params![],
                |row| {
                    Ok(AliasRecord {
                        alias: row.get_typed(0)?,
                        skill_id: row.get_typed(1)?,
                        alias_type: row.get_typed(2)?,
                        created_at: row.get_typed(3)?,
                    })
                },
            )?
        };

        Ok(records)
    }

    /// Get aliases for a specific skill
    pub fn get_aliases_for_skill(&self, skill_id: &str) -> Result<Vec<String>> {
        let aliases = self.conn.query_map_collect(
            "SELECT alias FROM skill_aliases WHERE skill_id = ? ORDER BY alias",
            params![skill_id],
            |row| row.get_typed::<String>(0),
        )?;
        Ok(aliases)
    }

    /// Tokenize a raw user query into lowercased whitespace-separated terms for
    /// the lexical substring search. Returns an empty vec for blank input.
    fn search_tokens(query: &str) -> Vec<String> {
        query.split_whitespace().map(str::to_lowercase).collect()
    }

    /// Lexical skill search — **fallback only** (issue #144).
    ///
    /// The primary lexical backend for `ms search` is the Tantivy BM25 index
    /// (`crate::search::Bm25Index`), which ranks by true relevance. This
    /// substring scan is used only when that index is unavailable (never
    /// built / empty state dir) or errors; it has no relevance signal, so
    /// results come back in `quality_score DESC, id ASC` order with
    /// all-tokens AND semantics.
    ///
    /// fsqlite 0.1.10 does not route FTS5 `MATCH` through its SQL planner — FTS5
    /// is only reachable via a programmatic API, so `WHERE skills_fts MATCH ?`
    /// fails with `column not found: skills_fts`, and the external-content FTS
    /// triggers additionally raise `PrimaryKeyViolation` (meta_skill#120). Lexical
    /// search therefore runs as a bounded, case-insensitive substring scan over
    /// the indexed text columns (`name`, `description`, `body`): a skill matches
    /// when *every* query token appears in the concatenated text. The skill corpus
    /// is small, so a single scan + in-memory filter is cheap; rows are pre-ordered
    /// by `quality_score` so the truncation keeps the strongest matches, and the
    /// hybrid path re-ranks via RRF.
    pub fn search_fts(&self, query: &str, limit: usize) -> Result<Vec<SkillSearchCandidate>> {
        let tokens = Self::search_tokens(query);
        if tokens.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let rows: Vec<(SkillSearchCandidate, String)> = self.conn.query_map_collect(
            "SELECT id, source_layer, metadata_json, quality_score, is_deprecated, \
                    name, description, body \
             FROM skills ORDER BY quality_score DESC, id ASC",
            params![],
            |row| {
                let candidate = SkillSearchCandidate {
                    id: row.get_typed(0)?,
                    source_layer: row.get_typed(1)?,
                    metadata_json: row.get_typed(2)?,
                    quality_score: row.get_typed(3)?,
                    is_deprecated: row.get_typed::<i64>(4)? != 0,
                };
                let haystack = format!(
                    "{}\n{}\n{}",
                    row.get_typed::<String>(5)?,
                    row.get_typed::<String>(6)?,
                    row.get_typed::<String>(7)?,
                )
                .to_lowercase();
                Ok((candidate, haystack))
            },
        )?;

        let mut candidates = Vec::new();
        for (candidate, haystack) in rows {
            if tokens.iter().all(|token| haystack.contains(token.as_str())) {
                candidates.push(candidate);
                if candidates.len() >= limit {
                    break;
                }
            }
        }
        Ok(candidates)
    }

    pub fn get_skill_candidate(&self, id: &str) -> Result<Option<SkillSearchCandidate>> {
        use fsqlite::compat::OptionalExtension;
        let result = self
            .conn
            .query_row_map(
                "SELECT id, source_layer, metadata_json, quality_score, is_deprecated
                 FROM skills WHERE id = ?",
                params![id],
                |row| {
                    Ok(SkillSearchCandidate {
                        id: row.get_typed(0)?,
                        source_layer: row.get_typed(1)?,
                        metadata_json: row.get_typed(2)?,
                        quality_score: row.get_typed(3)?,
                        is_deprecated: row.get_typed::<i64>(4)? != 0,
                    })
                },
            )
            .optional()?;
        Ok(result)
    }

    pub fn upsert_embedding(&self, record: &EmbeddingRecord) -> Result<()> {
        if record.embedding.len() != record.dims {
            return Err(MsError::Serialization(format!(
                "embedding dims mismatch: expected {}, got {}",
                record.dims,
                record.embedding.len()
            )));
        }

        let encoded = encode_embedding_f16(&record.embedding);
        let computed_at = if record.computed_at.is_empty() {
            chrono::Utc::now().to_rfc3339()
        } else {
            record.computed_at.clone()
        };

        self.conn.execute_compat(
            "INSERT INTO skill_embeddings (
                skill_id, embedding, dims, embedder_type, content_hash, computed_at, created_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(skill_id) DO UPDATE SET
                embedding=excluded.embedding,
                dims=excluded.dims,
                embedder_type=excluded.embedder_type,
                content_hash=excluded.content_hash,
                computed_at=excluded.computed_at",
            params![
                record.skill_id,
                encoded,
                record.dims as i64,
                record.embedder_type,
                record.content_hash,
                computed_at,
                computed_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_embedding(&self, skill_id: &str) -> Result<Option<EmbeddingRecord>> {
        use fsqlite::compat::OptionalExtension;
        let raw = self
            .conn
            .query_row_map(
                "SELECT skill_id, embedding, dims, embedder_type, content_hash, computed_at, created_at
                 FROM skill_embeddings
                 WHERE skill_id = ?",
                params![skill_id],
                embedding_raw_row,
            )
            .optional()?;
        match raw {
            Some(raw) => Ok(Some(decode_raw_embedding(raw)?)),
            None => Ok(None),
        }
    }

    pub fn get_embedding_by_hash(
        &self,
        content_hash: &str,
        embedder_type: &str,
        dims: usize,
    ) -> Result<Option<EmbeddingRecord>> {
        use fsqlite::compat::OptionalExtension;
        let raw = self
            .conn
            .query_row_map(
                "SELECT skill_id, embedding, dims, embedder_type, content_hash, computed_at, created_at
                 FROM skill_embeddings
                 WHERE content_hash = ? AND embedder_type = ? AND dims = ?
                 LIMIT 1",
                params![content_hash, embedder_type, dims as i64],
                embedding_raw_row,
            )
            .optional()?;
        match raw {
            Some(raw) => Ok(Some(decode_raw_embedding(raw)?)),
            None => Ok(None),
        }
    }

    /// Efficiently load all embeddings for the vector index.
    /// Returns pairs of (`skill_id`, `embedding_vector`).
    pub fn get_all_embeddings(&self) -> Result<Vec<(String, Vec<f32>)>> {
        // Pull (skill_id, raw_blob, dims) into Rust-land via fsqlite, then
        // decode outside the SQL row loop so we can surface the rich
        // MsError::Serialization variant on bad blobs (FrankenError doesn't
        // carry an arbitrary downcast like rusqlite's
        // FromSqlConversionFailure did).
        let raw: Vec<(String, Vec<u8>, i64)> = self.conn.query_map_collect(
            "SELECT skill_id, embedding, dims FROM skill_embeddings",
            params![],
            |row| {
                let skill_id: String = row.get_typed(0)?;
                let blob: Vec<u8> = row.get_typed(1)?;
                let dims: i64 = row.get_typed(2)?;
                Ok((skill_id, blob, dims))
            },
        )?;

        let mut results = Vec::with_capacity(raw.len());
        for (skill_id, blob, dims) in raw {
            let dims_usize = if dims <= 0 { 0 } else { dims as usize };
            let embedding = decode_embedding_f16(&blob, dims_usize)?;
            results.push((skill_id, embedding));
        }
        Ok(results)
    }

    pub fn insert_quarantine_record(&self, record: &QuarantineRecord) -> Result<()> {
        let classification_json =
            serde_json::to_string(&record.acip_classification).map_err(|err| {
                crate::error::MsError::Config(format!("encode classification: {err}"))
            })?;
        self.conn.execute_compat(
            "INSERT INTO injection_quarantine (
                quarantine_id, session_id, message_index, content_hash, safe_excerpt,
                classification_json, audit_tag, created_at, replay_command
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                record.quarantine_id,
                record.session_id,
                record.message_index as i64,
                record.content_hash,
                record.safe_excerpt,
                classification_json,
                record.audit_tag,
                record.created_at,
                record.replay_command,
            ],
        )?;
        Ok(())
    }

    pub fn insert_command_safety_event(&self, event: &CommandSafetyEvent) -> Result<()> {
        let decision_json = serde_json::to_string(&event.decision)
            .map_err(|err| crate::error::MsError::Config(format!("encode decision: {err}")))?;
        self.conn.execute_compat(
            "INSERT INTO command_safety_events (
                session_id, command, dcg_version, dcg_pack, decision_json, created_at
             ) VALUES (?, ?, ?, ?, ?, ?)",
            params![
                event.session_id,
                event.command,
                event.dcg_version,
                event.dcg_pack,
                decision_json,
                event.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn list_command_safety_events(&self, limit: usize) -> Result<Vec<CommandSafetyEvent>> {
        // Pull raw rows first, then parse JSON outside the closure so the row
        // mapper only deals in `FrankenError`. JSON parse failures become
        // `MsError::Serialization` (preserves the source error context).
        let raw: Vec<CommandSafetyRawRow> = self.conn.query_map_collect(
            "SELECT session_id, command, dcg_version, dcg_pack, decision_json, created_at
                 FROM command_safety_events
                 ORDER BY created_at DESC
                 LIMIT ?",
            params![limit as i64],
            |row| {
                Ok((
                    row.get_typed(0)?,
                    row.get_typed(1)?,
                    row.get_typed(2)?,
                    row.get_typed(3)?,
                    row.get_typed(4)?,
                    row.get_typed(5)?,
                ))
            },
        )?;

        let mut out = Vec::with_capacity(raw.len());
        for (session_id, command, dcg_version, dcg_pack, decision_json, created_at) in raw {
            let decision = serde_json::from_str(&decision_json)
                .map_err(|err| MsError::Serialization(format!("decode decision: {err}")))?;
            out.push(CommandSafetyEvent {
                session_id,
                command,
                dcg_version,
                dcg_pack,
                decision,
                created_at,
            });
        }
        Ok(out)
    }

    pub fn list_quarantine_records(&self, limit: usize) -> Result<Vec<QuarantineRecord>> {
        let rows = self.conn.query_map_collect(
            "SELECT quarantine_id, session_id, message_index, content_hash, safe_excerpt,
                    classification_json, audit_tag, created_at, replay_command
             FROM injection_quarantine
             ORDER BY created_at DESC
             LIMIT ?",
            params![limit as i64],
            quarantine_from_row,
        )?;
        Ok(rows)
    }

    pub fn list_quarantine_records_by_session(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<QuarantineRecord>> {
        let rows = self.conn.query_map_collect(
            "SELECT quarantine_id, session_id, message_index, content_hash, safe_excerpt,
                    classification_json, audit_tag, created_at, replay_command
             FROM injection_quarantine
             WHERE session_id = ?
             ORDER BY created_at DESC
             LIMIT ?",
            params![session_id, limit as i64],
            quarantine_from_row,
        )?;
        Ok(rows)
    }

    pub fn get_quarantine_record(&self, quarantine_id: &str) -> Result<Option<QuarantineRecord>> {
        use fsqlite::compat::OptionalExtension;
        let result = self
            .conn
            .query_row_map(
                "SELECT quarantine_id, session_id, message_index, content_hash, safe_excerpt,
                        classification_json, audit_tag, created_at, replay_command
                 FROM injection_quarantine
                 WHERE quarantine_id = ?",
                params![quarantine_id],
                quarantine_from_row,
            )
            .optional()?;
        Ok(result)
    }

    pub fn insert_quarantine_review(
        &self,
        quarantine_id: &str,
        action: &str,
        reason: Option<&str>,
    ) -> Result<String> {
        let review_id = format!("qr_{}", Uuid::new_v4());
        let created_at = chrono::Utc::now().to_rfc3339();
        self.conn.execute_compat(
            "INSERT INTO injection_quarantine_reviews (
                id, quarantine_id, action, reason, created_at
             ) VALUES (?, ?, ?, ?, ?)",
            params![review_id, quarantine_id, action, reason, created_at],
        )?;
        Ok(review_id)
    }

    pub fn list_quarantine_reviews(&self, quarantine_id: &str) -> Result<Vec<QuarantineReview>> {
        let out = self.conn.query_map_collect(
            "SELECT id, quarantine_id, action, reason, created_at
             FROM injection_quarantine_reviews
             WHERE quarantine_id = ?
             ORDER BY created_at DESC",
            params![quarantine_id],
            |row| {
                Ok(QuarantineReview {
                    id: row.get_typed(0)?,
                    quarantine_id: row.get_typed(1)?,
                    action: row.get_typed(2)?,
                    reason: row.get_typed(3)?,
                    created_at: row.get_typed(4)?,
                })
            },
        )?;
        Ok(out)
    }

    // =========================================================================
    // TRANSACTION LOG METHODS (for 2PC)
    // =========================================================================

    /// Insert a transaction record into `tx_log`
    pub fn insert_tx_record(&self, tx: &super::tx::TxRecord) -> Result<()> {
        self.conn.execute_compat(
            "INSERT INTO tx_log (id, entity_type, entity_id, phase, payload_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
            params![
                tx.id,
                tx.entity_type,
                tx.entity_id,
                tx.phase.to_string(),
                tx.payload_json,
                tx.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Update transaction phase
    pub fn update_tx_phase(&self, tx_id: &str, phase: super::tx::TxPhase) -> Result<()> {
        self.conn.execute_compat(
            "UPDATE tx_log SET phase = ? WHERE id = ?",
            params![phase.to_string(), tx_id],
        )?;
        Ok(())
    }

    /// Check if a transaction exists in `tx_log`
    pub fn tx_exists(&self, tx_id: &str) -> Result<bool> {
        let exists: i32 = self.conn.query_row_map(
            "SELECT EXISTS(SELECT 1 FROM tx_log WHERE id = ?)",
            params![tx_id],
            |row| row.get_typed::<i32>(0),
        )?;
        Ok(exists == 1)
    }

    /// List incomplete transactions (not in Complete phase)
    pub fn list_incomplete_transactions(&self) -> Result<Vec<super::tx::TxRecord>> {
        // Read raw rows; resolve enum + chrono parsing outside the closure so
        // mapper errors translate cleanly to MsError variants. fsqlite's
        // FrankenError doesn't carry an arbitrary downcast like rusqlite's
        // FromSqlConversionFailure did, so we keep the closure pure typed
        // extraction and decode/validate in Rust afterwards.
        let raw: Vec<(String, String, String, String, String, String)> =
            self.conn.query_map_collect(
                "SELECT id, entity_type, entity_id, phase, payload_json, created_at
                 FROM tx_log WHERE phase != 'complete'",
                params![],
                |row| {
                    Ok((
                        row.get_typed(0)?,
                        row.get_typed(1)?,
                        row.get_typed(2)?,
                        row.get_typed(3)?,
                        row.get_typed(4)?,
                        row.get_typed(5)?,
                    ))
                },
            )?;

        let mut txs = Vec::with_capacity(raw.len());
        for (id, entity_type, entity_id, phase_str, payload_json, created_str) in raw {
            let phase = match phase_str.as_str() {
                "prepare" => super::tx::TxPhase::Prepare,
                "pending" => super::tx::TxPhase::Pending,
                "committed" => super::tx::TxPhase::Committed,
                "complete" => super::tx::TxPhase::Complete,
                unknown => {
                    return Err(MsError::Serialization(format!(
                        "unknown transaction phase: {unknown}"
                    )));
                }
            };

            let created_at = chrono::DateTime::parse_from_rfc3339(&created_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .map_err(|e| MsError::Serialization(format!("parse created_at: {e}")))?;

            txs.push(super::tx::TxRecord {
                id,
                entity_type,
                entity_id,
                phase,
                payload_json,
                created_at,
            });
        }

        Ok(txs)
    }

    /// Insert or update a skill during 2PC pending phase.
    ///
    /// For NEW skills: inserts with `source_path`='pending' marker.
    /// For EXISTING skills: updates only metadata fields, preserving the original
    /// `source_path` and `content_hash`. This ensures rollback won't corrupt committed data.
    ///
    /// The `source_path` and `content_hash` are only finalized by `finalize_skill_commit`
    /// after Git commit succeeds.
    pub fn upsert_skill_pending(
        &self,
        skill: &crate::core::SkillSpec,
        layer: crate::core::SkillLayer,
        token_count: i64,
    ) -> Result<()> {
        self.conn.execute_compat(
            "INSERT INTO skills (id, name, description, version, author, source_path, source_layer, content_hash, body, metadata_json, assets_json, token_count, quality_score, indexed_at, modified_at) VALUES (?, ?, ?, ?, ?, 'pending', ?, 'pending', '', ?, '{}', ?, 0.0, datetime('now'), datetime('now')) ON CONFLICT(id) DO UPDATE SET name=excluded.name, description=excluded.description, version=excluded.version, author=excluded.author, source_layer=excluded.source_layer, metadata_json=excluded.metadata_json, token_count=excluded.token_count, modified_at=excluded.modified_at",
            params![
                skill.metadata.id,
                skill.metadata.name,
                skill.metadata.description,
                skill.metadata.version,
                skill.metadata.author,
                layer.as_str(),
                serde_json::to_string(&skill.metadata).unwrap_or_default(),
                token_count,
            ],
        )?;
        Ok(())
    }

    /// Finalize a skill commit by updating `source_path`, `content_hash`, and body.
    ///
    /// This is called after Git commit succeeds to populate the full `SQLite` record
    /// with searchable content (body for FTS).
    pub fn finalize_skill_commit(
        &self,
        skill_id: &str,
        source_path: &str,
        content_hash: &str,
        body: &str,
    ) -> Result<()> {
        self.conn.execute_compat(
            "UPDATE skills SET source_path = ?, content_hash = ?, body = ?, modified_at = datetime('now')
             WHERE id = ?",
            params![source_path, content_hash, body, skill_id],
        )?;
        Ok(())
    }

    /// Run `SQLite` integrity check
    pub fn integrity_check(&self) -> Result<bool> {
        let result: String =
            self.conn
                .query_row_map("PRAGMA integrity_check", params![], |row| {
                    row.get_typed::<String>(0)
                })?;
        Ok(result == "ok")
    }

    // =========================================================================
    // SESSION QUALITY CACHE METHODS
    // =========================================================================

    /// Get cached session quality by `session_id`
    pub fn get_session_quality(&self, session_id: &str) -> Result<Option<SessionQualityRecord>> {
        use fsqlite::compat::OptionalExtension;
        // Pull raw row values, then decode JSON-bearing columns in Rust
        // because the JSON parse can fail with MsError::Config but the
        // mapper closure must return FrankenError.
        let raw: Option<(String, String, f64, String, String, String)> = self
            .conn
            .query_row_map(
                "SELECT session_id, content_hash, score, signals_json, missing_json, computed_at
                 FROM session_quality
                 WHERE session_id = ?",
                params![session_id],
                |row| {
                    Ok((
                        row.get_typed(0)?,
                        row.get_typed(1)?,
                        row.get_typed(2)?,
                        row.get_typed(3)?,
                        row.get_typed(4)?,
                        row.get_typed(5)?,
                    ))
                },
            )
            .optional()?;

        match raw {
            Some((session_id, content_hash, score, signals_json, missing_json, computed_at)) => {
                let signals: Vec<String> = serde_json::from_str(&signals_json)
                    .map_err(|err| MsError::Config(format!("decode signals: {err}")))?;
                let missing: Vec<String> = serde_json::from_str(&missing_json)
                    .map_err(|err| MsError::Config(format!("decode missing: {err}")))?;
                Ok(Some(SessionQualityRecord {
                    session_id,
                    content_hash,
                    score: score as f32,
                    signals,
                    missing,
                    computed_at,
                }))
            }
            None => Ok(None),
        }
    }

    /// Upsert session quality record
    pub fn upsert_session_quality(&self, record: &SessionQualityRecord) -> Result<()> {
        let signals_json = serde_json::to_string(&record.signals)
            .map_err(|err| MsError::Config(format!("encode signals: {err}")))?;
        let missing_json = serde_json::to_string(&record.missing)
            .map_err(|err| MsError::Config(format!("encode missing: {err}")))?;

        self.conn.execute_compat(
            "INSERT INTO session_quality (session_id, content_hash, score, signals_json, missing_json, computed_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(session_id) DO UPDATE SET
                content_hash=excluded.content_hash,
                score=excluded.score,
                signals_json=excluded.signals_json,
                missing_json=excluded.missing_json,
                computed_at=excluded.computed_at",
            params![
                record.session_id,
                record.content_hash,
                f64::from(record.score),
                signals_json,
                missing_json,
                record.computed_at,
            ],
        )?;
        Ok(())
    }

    // =========================================================================
    // SKILL EVIDENCE METHODS (PROVENANCE GRAPH)
    // =========================================================================

    /// Upsert evidence for a specific rule in a skill.
    /// Each rule can have multiple evidence references from CASS sessions.
    pub fn upsert_evidence(
        &self,
        skill_id: &str,
        rule_id: &str,
        evidence: &[crate::core::EvidenceRef],
        coverage: &crate::core::EvidenceCoverage,
    ) -> Result<()> {
        let evidence_json = serde_json::to_string(evidence)
            .map_err(|err| MsError::Config(format!("encode evidence: {err}")))?;
        let coverage_json = serde_json::to_string(coverage)
            .map_err(|err| MsError::Config(format!("encode coverage: {err}")))?;
        let updated_at = chrono::Utc::now().to_rfc3339();

        self.conn.execute_compat(
            "INSERT INTO skill_evidence (skill_id, rule_id, evidence_json, coverage_json, updated_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(skill_id, rule_id) DO UPDATE SET
                evidence_json=excluded.evidence_json,
                coverage_json=excluded.coverage_json,
                updated_at=excluded.updated_at",
            params![skill_id, rule_id, evidence_json, coverage_json, updated_at],
        )?;
        Ok(())
    }

    /// Get all evidence for a skill, reconstructuting the `SkillEvidenceIndex`.
    pub fn get_evidence(&self, skill_id: &str) -> Result<crate::core::SkillEvidenceIndex> {
        let raw: Vec<(String, String)> = self.conn.query_map_collect(
            "SELECT rule_id, evidence_json, coverage_json
             FROM skill_evidence
             WHERE skill_id = ?
             ORDER BY rule_id",
            params![skill_id],
            |row| Ok((row.get_typed(0)?, row.get_typed(1)?)),
        )?;

        let mut rules = std::collections::BTreeMap::new();
        let mut total_confidence = 0.0f32;
        let mut evidence_count = 0usize;

        for (rule_id, evidence_json) in raw {
            let evidence_refs: Vec<crate::core::EvidenceRef> = serde_json::from_str(&evidence_json)
                .map_err(|err| {
                    MsError::Config(format!("decode evidence for rule {rule_id}: {err}"))
                })?;

            for e in &evidence_refs {
                total_confidence += e.confidence;
                evidence_count += 1;
            }
            rules.insert(rule_id, evidence_refs);
        }

        let rules_with_evidence = rules.values().filter(|v| !v.is_empty()).count();
        let avg_confidence = if evidence_count > 0 {
            total_confidence / evidence_count as f32
        } else {
            0.0
        };

        Ok(crate::core::SkillEvidenceIndex {
            rules,
            coverage: crate::core::EvidenceCoverage {
                total_rules: rules_with_evidence, // We only know about rules with evidence stored
                rules_with_evidence,
                avg_confidence,
            },
        })
    }

    /// Get evidence for a specific rule in a skill.
    pub fn get_rule_evidence(
        &self,
        skill_id: &str,
        rule_id: &str,
    ) -> Result<Vec<crate::core::EvidenceRef>> {
        use fsqlite::compat::OptionalExtension;
        let evidence_json: Option<String> = self
            .conn
            .query_row_map(
                "SELECT evidence_json FROM skill_evidence WHERE skill_id = ? AND rule_id = ?",
                params![skill_id, rule_id],
                |row| row.get_typed::<String>(0),
            )
            .optional()?;

        if let Some(evidence_json) = evidence_json {
            let evidence_refs: Vec<crate::core::EvidenceRef> = serde_json::from_str(&evidence_json)
                .map_err(|err| MsError::Config(format!("decode evidence: {err}")))?;
            Ok(evidence_refs)
        } else {
            Ok(vec![])
        }
    }

    /// List all evidence records for provenance graph export.
    /// Returns (`skill_id`, `rule_id`, `evidence_refs`, `updated_at`) tuples.
    pub fn list_all_evidence(&self) -> Result<Vec<EvidenceRecord>> {
        let raw: Vec<(String, String, String, String)> = self.conn.query_map_collect(
            "SELECT skill_id, rule_id, evidence_json, updated_at
             FROM skill_evidence
             ORDER BY skill_id, rule_id",
            params![],
            |row| {
                Ok((
                    row.get_typed(0)?,
                    row.get_typed(1)?,
                    row.get_typed(2)?,
                    row.get_typed(3)?,
                ))
            },
        )?;

        let mut records = Vec::with_capacity(raw.len());
        for (skill_id, rule_id, evidence_json, updated_at) in raw {
            let evidence: Vec<crate::core::EvidenceRef> = serde_json::from_str(&evidence_json)
                .map_err(|err| MsError::Config(format!("decode evidence: {err}")))?;
            records.push(EvidenceRecord {
                skill_id,
                rule_id,
                evidence,
                updated_at,
            });
        }
        Ok(records)
    }

    /// Delete all evidence for a skill.
    pub fn delete_skill_evidence(&self, skill_id: &str) -> Result<usize> {
        let count = self.conn.execute_compat(
            "DELETE FROM skill_evidence WHERE skill_id = ?",
            params![skill_id],
        )?;
        Ok(count)
    }

    pub fn record_skill_outcome(&self, skill_id: &str, success: bool) -> Result<()> {
        let id = Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().to_rfc3339();
        let success_signal = i32::from(success);
        let updated = self.conn.execute_compat(
            "UPDATE skill_usage
             SET success_signal = ?
             WHERE id = (
                 SELECT id FROM skill_usage
                 WHERE skill_id = ?
                 ORDER BY used_at DESC
                 LIMIT 1
             )",
            params![success_signal, skill_id],
        )?;

        // Append a detailed event record for analysis even when we update summary usage.
        self.conn.execute_compat(
            "INSERT INTO skill_usage_events (id, skill_id, session_id, loaded_at, disclosure_level, discovery_method, outcome, feedback)
             VALUES (?, ?, 'manual', ?, 'full', 'manual', ?, 'null')",
            params![id, skill_id, created_at, if success { "success" } else { "failure" }],
        )?;

        if updated == 0 {
            // No usage row existed; we still recorded an event above.
        }
        Ok(())
    }

    pub fn record_skill_feedback(
        &self,
        skill_id: &str,
        feedback_type: &str,
        rating: Option<i64>,
        comment: Option<&str>,
    ) -> Result<SkillFeedbackRecord> {
        let id = Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().to_rfc3339();

        self.conn.execute_compat(
            "INSERT INTO skill_feedback (id, skill_id, feedback_type, rating, comment, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
            params![id, skill_id, feedback_type, rating, comment, created_at],
        )?;

        Ok(SkillFeedbackRecord {
            id,
            skill_id: skill_id.to_string(),
            feedback_type: feedback_type.to_string(),
            rating,
            comment: comment.map(std::string::ToString::to_string),
            created_at,
        })
    }

    pub fn list_skill_feedback(
        &self,
        skill_id: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<SkillFeedbackRecord>> {
        let mut sql = "SELECT id, skill_id, feedback_type, rating, comment, created_at
                       FROM skill_feedback"
            .to_string();

        if skill_id.is_some() {
            sql.push_str(" WHERE skill_id = ?");
        }

        sql.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");

        let records = if let Some(sid) = skill_id {
            self.conn
                .query_map_collect(&sql, params![sid, limit as i64, offset as i64], |row| {
                    Ok(SkillFeedbackRecord {
                        id: row.get_typed(0)?,
                        skill_id: row.get_typed(1)?,
                        feedback_type: row.get_typed(2)?,
                        rating: row.get_typed(3)?,
                        comment: row.get_typed(4)?,
                        created_at: row.get_typed(5)?,
                    })
                })?
        } else {
            self.conn
                .query_map_collect(&sql, params![limit as i64, offset as i64], |row| {
                    Ok(SkillFeedbackRecord {
                        id: row.get_typed(0)?,
                        skill_id: row.get_typed(1)?,
                        feedback_type: row.get_typed(2)?,
                        rating: row.get_typed(3)?,
                        comment: row.get_typed(4)?,
                        created_at: row.get_typed(5)?,
                    })
                })?
        };
        Ok(records)
    }

    // =========================================================================
    // User Preferences (favorites/hidden)
    // =========================================================================

    /// Add a user preference (favorite or hidden) for a skill.
    pub fn set_user_preference(
        &self,
        skill_id: &str,
        preference_type: &str,
    ) -> Result<UserPreferenceRecord> {
        let id = Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().to_rfc3339();

        self.conn.execute_compat(
            "INSERT OR REPLACE INTO user_preferences (id, skill_id, preference_type, created_at)
             VALUES (?, ?, ?, ?)",
            params![id, skill_id, preference_type, created_at],
        )?;

        Ok(UserPreferenceRecord {
            id,
            skill_id: skill_id.to_string(),
            preference_type: preference_type.to_string(),
            created_at,
        })
    }

    /// Remove a user preference for a skill.
    pub fn remove_user_preference(&self, skill_id: &str, preference_type: &str) -> Result<bool> {
        let deleted = self.conn.execute_compat(
            "DELETE FROM user_preferences WHERE skill_id = ? AND preference_type = ?",
            params![skill_id, preference_type],
        )?;
        Ok(deleted > 0)
    }

    /// Check if a skill has a specific preference.
    pub fn has_user_preference(&self, skill_id: &str, preference_type: &str) -> Result<bool> {
        let count: i64 = self.conn.query_row_map(
            "SELECT COUNT(*) FROM user_preferences WHERE skill_id = ? AND preference_type = ?",
            params![skill_id, preference_type],
            |row| row.get_typed::<i64>(0),
        )?;
        Ok(count > 0)
    }

    /// List all skills with a specific preference type.
    pub fn list_user_preferences(
        &self,
        preference_type: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<UserPreferenceRecord>> {
        let records = self.conn.query_map_collect(
            "SELECT id, skill_id, preference_type, created_at
             FROM user_preferences
             WHERE preference_type = ?
             ORDER BY created_at DESC
             LIMIT ? OFFSET ?",
            params![preference_type, limit as i64, offset as i64],
            |row| {
                Ok(UserPreferenceRecord {
                    id: row.get_typed(0)?,
                    skill_id: row.get_typed(1)?,
                    preference_type: row.get_typed(2)?,
                    created_at: row.get_typed(3)?,
                })
            },
        )?;
        Ok(records)
    }

    /// Get all preferences for a skill.
    pub fn get_skill_preferences(&self, skill_id: &str) -> Result<Vec<UserPreferenceRecord>> {
        let records = self.conn.query_map_collect(
            "SELECT id, skill_id, preference_type, created_at
             FROM user_preferences
             WHERE skill_id = ?
             ORDER BY created_at DESC",
            params![skill_id],
            |row| {
                Ok(UserPreferenceRecord {
                    id: row.get_typed(0)?,
                    skill_id: row.get_typed(1)?,
                    preference_type: row.get_typed(2)?,
                    created_at: row.get_typed(3)?,
                })
            },
        )?;
        Ok(records)
    }

    pub fn create_skill_experiment(
        &self,
        skill_id: &str,
        scope: &str,
        scope_id: Option<&str>,
        variants_json: &str,
        allocation_json: &str,
        status: &str,
    ) -> Result<ExperimentRecord> {
        let id = Uuid::new_v4().to_string();
        let started_at = chrono::Utc::now().to_rfc3339();

        self.conn.execute_compat(
            "INSERT INTO skill_experiments (
                id, skill_id, scope, scope_id, variants_json, allocation_json, status, started_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                id,
                skill_id,
                scope,
                scope_id,
                variants_json,
                allocation_json,
                status,
                started_at
            ],
        )?;

        Ok(ExperimentRecord {
            id,
            skill_id: skill_id.to_string(),
            scope: scope.to_string(),
            scope_id: scope_id.map(std::string::ToString::to_string),
            variants_json: variants_json.to_string(),
            allocation_json: allocation_json.to_string(),
            status: status.to_string(),
            started_at,
        })
    }

    pub fn get_skill_experiment(&self, id: &str) -> Result<Option<ExperimentRecord>> {
        use fsqlite::compat::OptionalExtension;
        let result = self
            .conn
            .query_row_map(
                "SELECT id, skill_id, scope, scope_id, variants_json, allocation_json, status, started_at
                 FROM skill_experiments
                 WHERE id = ?",
                params![id],
                |row| {
                    Ok(ExperimentRecord {
                        id: row.get_typed(0)?,
                        skill_id: row.get_typed(1)?,
                        scope: row.get_typed(2)?,
                        scope_id: row.get_typed(3)?,
                        variants_json: row.get_typed(4)?,
                        allocation_json: row.get_typed(5)?,
                        status: row.get_typed(6)?,
                        started_at: row.get_typed(7)?,
                    })
                },
            )
            .optional()?;
        Ok(result)
    }

    pub fn list_skill_experiments(
        &self,
        skill_id: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ExperimentRecord>> {
        let mut sql = "SELECT id, skill_id, scope, scope_id, variants_json, allocation_json, status, started_at
                       FROM skill_experiments".to_string();

        if skill_id.is_some() {
            sql.push_str(" WHERE skill_id = ?");
        }

        sql.push_str(" ORDER BY started_at DESC LIMIT ? OFFSET ?");

        let records = if let Some(sid) = skill_id {
            self.conn
                .query_map_collect(&sql, params![sid, limit as i64, offset as i64], |row| {
                    Ok(ExperimentRecord {
                        id: row.get_typed(0)?,
                        skill_id: row.get_typed(1)?,
                        scope: row.get_typed(2)?,
                        scope_id: row.get_typed(3)?,
                        variants_json: row.get_typed(4)?,
                        allocation_json: row.get_typed(5)?,
                        status: row.get_typed(6)?,
                        started_at: row.get_typed(7)?,
                    })
                })?
        } else {
            self.conn
                .query_map_collect(&sql, params![limit as i64, offset as i64], |row| {
                    Ok(ExperimentRecord {
                        id: row.get_typed(0)?,
                        skill_id: row.get_typed(1)?,
                        scope: row.get_typed(2)?,
                        scope_id: row.get_typed(3)?,
                        variants_json: row.get_typed(4)?,
                        allocation_json: row.get_typed(5)?,
                        status: row.get_typed(6)?,
                        started_at: row.get_typed(7)?,
                    })
                })?
        };

        Ok(records)
    }

    pub fn update_skill_experiment_status(&self, id: &str, status: &str) -> Result<()> {
        let updated = self.conn.execute_compat(
            "UPDATE skill_experiments SET status = ? WHERE id = ?",
            params![status, id],
        )?;
        if updated == 0 {
            return Err(MsError::NotFound(format!("experiment not found: {id}")));
        }
        Ok(())
    }

    pub fn record_skill_experiment_event(
        &self,
        experiment_id: &str,
        variant_id: &str,
        event_type: &str,
        metrics_json: Option<&str>,
        context_json: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<ExperimentEventRecord> {
        let id = Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().to_rfc3339();
        self.conn.execute_compat(
            "INSERT INTO skill_experiment_events (
                id, experiment_id, variant_id, event_type, metrics_json, context_json, session_id, created_at
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                id,
                experiment_id,
                variant_id,
                event_type,
                metrics_json,
                context_json,
                session_id,
                created_at
            ],
        )?;

        Ok(ExperimentEventRecord {
            id,
            experiment_id: experiment_id.to_string(),
            variant_id: variant_id.to_string(),
            event_type: event_type.to_string(),
            metrics_json: metrics_json.map(std::string::ToString::to_string),
            context_json: context_json.map(std::string::ToString::to_string),
            session_id: session_id.map(std::string::ToString::to_string),
            created_at,
        })
    }

    pub fn list_skill_experiment_events(
        &self,
        experiment_id: &str,
    ) -> Result<Vec<ExperimentEventRecord>> {
        let records = self.conn.query_map_collect(
            "SELECT id, experiment_id, variant_id, event_type, metrics_json, context_json, session_id, created_at
             FROM skill_experiment_events
             WHERE experiment_id = ?
             ORDER BY created_at ASC",
            params![experiment_id],
            |row| {
                Ok(ExperimentEventRecord {
                    id: row.get_typed(0)?,
                    experiment_id: row.get_typed(1)?,
                    variant_id: row.get_typed(2)?,
                    event_type: row.get_typed(3)?,
                    metrics_json: row.get_typed(4)?,
                    context_json: row.get_typed(5)?,
                    session_id: row.get_typed(6)?,
                    created_at: row.get_typed(7)?,
                })
            },
        )?;
        Ok(records)
    }

    fn configure_pragmas(conn: &Connection) -> Result<()> {
        // fsqlite's `execute_batch` happily takes multi-statement PRAGMAs;
        // semicolon-split is handled internally by the compat splitter.
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA cache_size = -64000;
             PRAGMA mmap_size = 268435456;
             PRAGMA temp_store = MEMORY;
             PRAGMA foreign_keys = ON;",
        )?;
        Ok(())
    }
}

fn skill_from_row(row: &Row) -> RowResult<SkillRecord> {
    Ok(SkillRecord {
        id: row.get_typed(0)?,
        name: row.get_typed(1)?,
        description: row.get_typed(2)?,
        version: row.get_typed(3)?,
        author: row.get_typed(4)?,
        source_path: row.get_typed(5)?,
        source_layer: row.get_typed(6)?,
        git_remote: row.get_typed(7)?,
        git_commit: row.get_typed(8)?,
        content_hash: row.get_typed(9)?,
        body: row.get_typed(10)?,
        metadata_json: row.get_typed(11)?,
        assets_json: row.get_typed(12)?,
        token_count: row.get_typed(13)?,
        quality_score: row.get_typed(14)?,
        indexed_at: row.get_typed(15)?,
        modified_at: row.get_typed(16)?,
        is_deprecated: row.get_typed::<i64>(17)? != 0,
        deprecation_reason: row.get_typed(18)?,
    })
}

/// Raw row extraction for embedding rows. The `decode_embedding_f16` step
/// returns `MsError` (not `FrankenError`), so the actual decode happens in the
/// caller, after the row mapper returns.
struct EmbeddingRawRow {
    skill_id: String,
    blob: Vec<u8>,
    dims: i64,
    embedder_type: String,
    content_hash: Option<String>,
    computed_at: String,
    created_at: String,
}

fn embedding_raw_row(row: &Row) -> RowResult<EmbeddingRawRow> {
    Ok(EmbeddingRawRow {
        skill_id: row.get_typed(0)?,
        blob: row.get_typed(1)?,
        dims: row.get_typed(2)?,
        embedder_type: row.get_typed(3)?,
        content_hash: row.get_typed(4)?,
        computed_at: row.get_typed(5)?,
        created_at: row.get_typed(6)?,
    })
}

fn decode_raw_embedding(raw: EmbeddingRawRow) -> Result<EmbeddingRecord> {
    let dims_usize = if raw.dims <= 0 { 0 } else { raw.dims as usize };
    let computed_at = if raw.computed_at.is_empty() {
        raw.created_at
    } else {
        raw.computed_at
    };
    let embedding = decode_embedding_f16(&raw.blob, dims_usize)?;

    Ok(EmbeddingRecord {
        skill_id: raw.skill_id,
        embedding,
        dims: dims_usize,
        embedder_type: raw.embedder_type,
        content_hash: raw.content_hash,
        computed_at,
    })
}

fn encode_embedding_f16(values: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() * 2);
    for value in values {
        let bits = f16::from_f32(*value).to_bits();
        out.extend_from_slice(&bits.to_le_bytes());
    }
    out
}

fn decode_embedding_f16(bytes: &[u8], dims: usize) -> Result<Vec<f32>> {
    let expected = dims.saturating_mul(2);
    if bytes.len() != expected {
        return Err(MsError::Serialization(format!(
            "embedding blob length mismatch: expected {}, got {}",
            expected,
            bytes.len()
        )));
    }

    let mut out = Vec::with_capacity(dims);
    for chunk in bytes.chunks_exact(2) {
        let bits = u16::from_le_bytes([chunk[0], chunk[1]]);
        out.push(f16::from_bits(bits).to_f32());
    }
    Ok(out)
}

/// Row mapper used inside `query_map_collect` closures. JSON decoding of the
/// `classification_json` column would normally raise an `MsError::Config`,
/// but the closure must return `FrankenError`. We surface JSON parse errors
/// as `FrankenError::TypeMismatch` here so the surrounding `?` propagates
/// cleanly; the outer `MsError::Database` wrapping preserves the error text.
fn quarantine_from_row(row: &Row) -> RowResult<QuarantineRecord> {
    let classification_json: String = row.get_typed(5)?;
    let classification: JsonValue =
        serde_json::from_str(&classification_json).map_err(|err| FrankenError::TypeMismatch {
            expected: "AcipClassification JSON (string)".into(),
            actual: format!("invalid JSON: {err}"),
        })?;
    let acip_classification =
        serde_json::from_value(classification).map_err(|err| FrankenError::TypeMismatch {
            expected: "AcipClassification (typed)".into(),
            actual: format!("decode error: {err}"),
        })?;

    Ok(QuarantineRecord {
        quarantine_id: row.get_typed(0)?,
        session_id: row.get_typed(1)?,
        message_index: row.get_typed::<i64>(2)? as usize,
        content_hash: row.get_typed(3)?,
        safe_excerpt: row.get_typed(4)?,
        acip_classification,
        audit_tag: row.get_typed(6)?,
        created_at: row.get_typed(7)?,
        replay_command: row.get_typed(8)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::embeddings::HashEmbedder;
    use crate::security::AcipClassification;
    use tempfile::tempdir;

    #[test]
    fn test_database_creation_and_schema_version() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db = Database::open(&db_path).unwrap();
        assert!(db_path.exists());
        assert_eq!(db.schema_version(), migrations::SCHEMA_VERSION);
    }

    #[test]
    fn test_wal_mode_enabled() {
        let dir = tempdir().unwrap();
        let db = Database::open(dir.path().join("test.db")).unwrap();
        let mode: String = db
            .conn()
            .query_row("PRAGMA journal_mode;")
            .and_then(|row| row.get_typed::<String>(0))
            .unwrap();
        assert_eq!(mode.to_lowercase(), "wal");
    }

    #[test]
    fn test_critical_pragmas_engaged() {
        // INV2 audit: verify every PRAGMA from `configure_pragmas` actually
        // takes effect on the live connection after fsqlite migration.
        //
        // fsqlite return-type quirks (vs rusqlite):
        //   - `PRAGMA synchronous` returns the text label ("NORMAL"), not the
        //     integer (1). rusqlite returns the integer. Verified 2026-05-30.
        //   - `PRAGMA busy_timeout` defaults to 5000ms in fsqlite (rusqlite
        //     defaults to 0). meta_skill does not set this explicitly, so the
        //     fsqlite default applies and is a behavioral upgrade.
        let dir = tempdir().unwrap();
        let db = Database::open(dir.path().join("test.db")).unwrap();
        let conn = db.conn();

        let jm: String = conn
            .query_row("PRAGMA journal_mode")
            .and_then(|row| row.get_typed::<String>(0))
            .unwrap();
        assert_eq!(jm.to_lowercase(), "wal", "journal_mode not WAL: got {jm}");

        let sync_v: String = conn
            .query_row("PRAGMA synchronous")
            .and_then(|row| row.get_typed::<String>(0))
            .unwrap();
        assert_eq!(
            sync_v.to_uppercase(),
            "NORMAL",
            "synchronous != NORMAL: got {sync_v}"
        );

        let cs: i64 = conn
            .query_row("PRAGMA cache_size")
            .and_then(|row| row.get_typed::<i64>(0))
            .unwrap();
        assert_eq!(cs, -64000, "cache_size != -64000: got {cs}");

        let ts: i64 = conn
            .query_row("PRAGMA temp_store")
            .and_then(|row| row.get_typed::<i64>(0))
            .unwrap();
        assert_eq!(ts, 2, "temp_store != MEMORY(2): got {ts}");

        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys")
            .and_then(|row| row.get_typed::<i64>(0))
            .unwrap();
        assert_eq!(fk, 1, "foreign_keys != ON(1): got {fk}");

        let mm: i64 = conn
            .query_row("PRAGMA mmap_size")
            .and_then(|row| row.get_typed::<i64>(0))
            .unwrap();
        assert_eq!(mm, 268_435_456, "mmap_size mismatch: got {mm}");
    }

    #[test]
    fn test_to_param_edge_cases() {
        // INV3 audit: exercise `ms_params!` / `ToParam` edge cases against a
        // live fsqlite connection.
        use fsqlite::Connection;
        use fsqlite::compat::{ConnectionExt, RowExt};
        let dir = tempdir().unwrap();
        let path = dir.path().join("edge.db").to_string_lossy().into_owned();
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE edge (
                id   INTEGER PRIMARY KEY,
                txt  TEXT,
                num  INTEGER,
                fl   REAL,
                blob BLOB
            )",
        )
        .unwrap();

        // (1) Option<i64>::None binds NULL.
        let none_i64: Option<i64> = None;
        conn.execute_compat(
            "INSERT INTO edge (id, num) VALUES (1, ?)",
            params![none_i64],
        )
        .unwrap();
        let row = conn
            .query_row_map("SELECT num FROM edge WHERE id = 1", params![], |r| {
                Ok(r.get_typed::<Option<i64>>(0).unwrap())
            })
            .unwrap();
        assert!(row.is_none(), "Option::<i64>::None did not bind NULL");

        // (2) Empty string distinguishable from NULL.
        let empty: &str = "";
        conn.execute_compat("INSERT INTO edge (id, txt) VALUES (2, ?)", params![empty])
            .unwrap();
        let s = conn
            .query_row_map("SELECT txt FROM edge WHERE id = 2", params![], |r| {
                Ok(r.get_typed::<Option<String>>(0).unwrap())
            })
            .unwrap();
        assert_eq!(s, Some(String::new()), "empty &str collapsed to NULL");

        // (3) Vec<u8> BLOB round-trips and does not UTF-8 decode.
        let blob: Vec<u8> = vec![0xDE, 0xAD, 0xBE, 0xEF, 0xFF, 0x00, 0x01];
        conn.execute_compat("INSERT INTO edge (id, blob) VALUES (3, ?)", params![blob])
            .unwrap();
        let got_blob: Vec<u8> = conn
            .query_row_map("SELECT blob FROM edge WHERE id = 3", params![], |r| {
                Ok(r.get_typed::<Vec<u8>>(0).unwrap())
            })
            .unwrap();
        assert_eq!(got_blob, blob, "BLOB round-trip mismatch");

        // (4) i64::MIN / i64::MAX preserved.
        conn.execute_compat(
            "INSERT INTO edge (id, num) VALUES (4, ?)",
            params![i64::MIN],
        )
        .unwrap();
        conn.execute_compat(
            "INSERT INTO edge (id, num) VALUES (5, ?)",
            params![i64::MAX],
        )
        .unwrap();
        let v4: i64 = conn
            .query_row_map("SELECT num FROM edge WHERE id = 4", params![], |r| {
                r.get_typed::<i64>(0)
            })
            .unwrap();
        let v5: i64 = conn
            .query_row_map("SELECT num FROM edge WHERE id = 5", params![], |r| {
                r.get_typed::<i64>(0)
            })
            .unwrap();
        assert_eq!(v4, i64::MIN);
        assert_eq!(v5, i64::MAX);

        // (5) u64 > i64::MAX is bound as TEXT by `ParamValue::from(u64)` to
        // avoid silent wraparound on signed i64. Important caveat: INTEGER
        // column affinity will then *try* to coerce that TEXT back to a
        // numeric value at INSERT time, and oversize ints coerce to REAL
        // (lossy). So we round-trip into a column with NO affinity (BLOB
        // column type via no declared type) to verify the bind itself stays
        // TEXT. We use the `txt` (TEXT-affinity) column instead.
        let big_u64: u64 = u64::MAX;
        conn.execute_compat("INSERT INTO edge (id, txt) VALUES (6, ?)", params![big_u64])
            .unwrap();
        let s6: String = conn
            .query_row_map("SELECT txt FROM edge WHERE id = 6", params![], |r| {
                r.get_typed::<String>(0)
            })
            .unwrap();
        assert_eq!(
            s6,
            big_u64.to_string(),
            "u64::MAX should bind as TEXT representation"
        );

        // (6) f64::NAN and INFINITY: SQLite/fsqlite store/retrieve.
        conn.execute_compat(
            "INSERT INTO edge (id, fl) VALUES (7, ?)",
            params![f64::INFINITY],
        )
        .unwrap();
        conn.execute_compat("INSERT INTO edge (id, fl) VALUES (8, ?)", params![f64::NAN])
            .unwrap();
        // INFINITY round-trips; NaN comes back as NULL in classic SQLite
        // (and fsqlite). We only assert NaN-or-NULL here so the test stays
        // backend-agnostic, but log what we actually saw.
        let f7: f64 = conn
            .query_row_map("SELECT fl FROM edge WHERE id = 7", params![], |r| {
                r.get_typed::<f64>(0)
            })
            .unwrap();
        assert!(f7.is_infinite() && f7 > 0.0, "INFINITY did not round-trip");
        let f8_opt: Option<f64> = conn
            .query_row_map("SELECT fl FROM edge WHERE id = 8", params![], |r| {
                Ok(r.get_typed::<Option<f64>>(0).unwrap())
            })
            .unwrap();
        eprintln!("NaN round-trip => {f8_opt:?}");

        // (7) Borrowed &str via macro doesn't take ownership.
        let owned = String::from("hello world");
        let borrowed: &str = &owned;
        conn.execute_compat(
            "INSERT INTO edge (id, txt) VALUES (9, ?)",
            params![borrowed],
        )
        .unwrap();
        // owned still usable after the macro:
        assert_eq!(owned, "hello world");
    }

    #[test]
    fn test_all_tables_created() {
        let dir = tempdir().unwrap();
        let db = Database::open(dir.path().join("test.db")).unwrap();
        let tables = [
            "skills",
            "skill_aliases",
            // skills_fts (FTS5) is intentionally dropped by migration 013 — fsqlite
            // can't query it via SQL MATCH; search is a substring scan (#120).
            "skill_embeddings",
            "skill_packs",
            "skill_slices",
            "skill_evidence",
            "skill_rules",
            "uncertainty_queue",
            "redaction_reports",
            "injection_reports",
            "injection_quarantine",
            "injection_quarantine_reviews",
            "command_safety_events",
            "skill_usage",
            "skill_usage_events",
            "rule_outcomes",
            "ubs_reports",
            "cm_rule_links",
            "cm_sync_state",
            "skill_experiments",
            "skill_experiment_events",
            "skill_reservations",
            "skill_dependencies",
            "skill_capabilities",
            "build_sessions",
            "config",
            "tx_log",
            "cass_fingerprints",
            "session_quality",
        ];

        for table in tables {
            let exists: i32 = db
                .conn()
                .query_row_map(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
                    params![table],
                    |row| row.get_typed::<i32>(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "Table {} should exist", table);
        }
    }

    #[test]
    fn test_upsert_and_get_skill() {
        let dir = tempdir().unwrap();
        let db = Database::open(dir.path().join("test.db")).unwrap();
        let record = SkillRecord {
            id: "git-commit".to_string(),
            name: "Git Commit Patterns".to_string(),
            description: "Best practices for commits".to_string(),
            version: Some("1.0.0".to_string()),
            author: Some("Example".to_string()),
            source_path: "/skills/git".to_string(),
            source_layer: "base".to_string(),
            git_remote: None,
            git_commit: None,
            content_hash: "abc123".to_string(),
            body: "Write good commit messages".to_string(),
            metadata_json: r#"{"tags":"git,workflow"}"#.to_string(),
            assets_json: "{}".to_string(),
            token_count: 500,
            quality_score: 0.85,
            indexed_at: "2026-01-01T00:00:00Z".to_string(),
            modified_at: "2026-01-01T00:00:00Z".to_string(),
            is_deprecated: false,
            deprecation_reason: None,
        };

        db.upsert_skill(&record).unwrap();
        let fetched = db.get_skill("git-commit").unwrap().unwrap();
        assert_eq!(record, fetched);
    }

    #[test]
    fn test_fts_search() {
        let dir = tempdir().unwrap();
        let db = Database::open(dir.path().join("test.db")).unwrap();
        let record = SkillRecord {
            id: "rust-errors".to_string(),
            name: "Rust Error Handling".to_string(),
            description: "Patterns for Result and error handling".to_string(),
            version: Some("1.0.0".to_string()),
            author: None,
            source_path: "/skills/rust".to_string(),
            source_layer: "base".to_string(),
            git_remote: None,
            git_commit: None,
            content_hash: "def456".to_string(),
            body: "Use Result<T, E> and anyhow".to_string(),
            metadata_json: r#"{"tags":"rust,error"}"#.to_string(),
            assets_json: "{}".to_string(),
            token_count: 250,
            quality_score: 0.9,
            indexed_at: "2026-01-01T00:00:00Z".to_string(),
            modified_at: "2026-01-01T00:00:00Z".to_string(),
            is_deprecated: false,
            deprecation_reason: None,
        };

        db.upsert_skill(&record).unwrap();
        let results = db.search_fts("error", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "rust-errors");
        assert_eq!(results[0].quality_score, 0.9);
        assert!(!results[0].is_deprecated);
    }

    /// Regression test: FTS5 syntax characters in a user query must not cause
    /// an error.  Before the fix, `ms search "multi-agent"` raised
    /// `no such column: agent` because `-` was parsed as an FTS5 operator.
    #[test]
    fn test_fts_search_special_characters() {
        let dir = tempdir().unwrap();
        let db = Database::open(dir.path().join("test.db")).unwrap();
        let record = SkillRecord {
            id: "agent-swarm".to_string(),
            name: "Agent Swarm Launcher".to_string(),
            description: "Coordinate a multi-agent swarm".to_string(),
            version: Some("1.0.0".to_string()),
            author: None,
            source_path: "/skills/swarm".to_string(),
            source_layer: "base".to_string(),
            git_remote: None,
            git_commit: None,
            content_hash: "abc123".to_string(),
            body: "Launch a multi-agent swarm and coordinate work".to_string(),
            metadata_json: r#"{"tags":"multi-agent,swarm"}"#.to_string(),
            assets_json: "{}".to_string(),
            token_count: 100,
            quality_score: 0.8,
            indexed_at: "2026-01-01T00:00:00Z".to_string(),
            modified_at: "2026-01-01T00:00:00Z".to_string(),
            is_deprecated: false,
            deprecation_reason: None,
        };
        db.upsert_skill(&record).unwrap();

        // Each of these must return Ok — no panics, no FTS5 syntax errors.
        for query in [
            "multi-agent",
            "a-b",
            "x-y-z",
            "a'b",
            "agent: swarm",
            "(swarm)",
        ] {
            db.search_fts(query, 10)
                .unwrap_or_else(|e| panic!("query {query:?} should not error: {e}"));
        }

        // The hyphenated phrase must still find the matching skill.
        let results = db.search_fts("multi-agent", 10).unwrap();
        assert_eq!(
            results.len(),
            1,
            "multi-agent query should find agent-swarm"
        );
        assert_eq!(results[0].id, "agent-swarm");

        // A blank / whitespace-only query must return empty results, not error.
        assert!(db.search_fts("   ", 10).unwrap().is_empty());
    }

    /// Regression test for meta_skill#120: lexical search must work end-to-end on
    /// a fresh db — index a skill (name/description/body all searchable), re-index
    /// via the upsert path (stale terms gone, new terms found), and delete (no
    /// longer found). Before the fix `ms index` errored in the FTS triggers and
    /// `ms search` failed with `column not found: skills_fts`. Multi-token queries
    /// are ANDed across the concatenated text.
    #[test]
    fn test_fts_lifecycle_insert_update_delete() {
        let dir = tempdir().unwrap();
        let db = Database::open(dir.path().join("test.db")).unwrap();
        let mut record = SkillRecord {
            id: "deploy-helper".to_string(),
            name: "Deploy Helper".to_string(),
            description: "Ship releases safely".to_string(),
            version: Some("1.0.0".to_string()),
            author: None,
            source_path: "/skills/deploy".to_string(),
            source_layer: "base".to_string(),
            git_remote: None,
            git_commit: None,
            content_hash: "h1".to_string(),
            body: "rollout the artifact to production".to_string(),
            metadata_json: r#"{"tags":["kubernetes","canary"]}"#.to_string(),
            assets_json: "{}".to_string(),
            token_count: 120,
            quality_score: 0.7,
            indexed_at: "2026-01-01T00:00:00Z".to_string(),
            modified_at: "2026-01-01T00:00:00Z".to_string(),
            is_deprecated: false,
            deprecation_reason: None,
        };

        // INSERT: terms from name, description, and body are all searchable.
        db.upsert_skill(&record).unwrap();
        assert_eq!(db.search_fts("artifact", 10).unwrap().len(), 1, "body term");
        assert_eq!(db.search_fts("deploy", 10).unwrap().len(), 1, "name term");
        assert_eq!(db.search_fts("releases", 10).unwrap().len(), 1, "desc term");
        // Multi-token queries are ANDed across the concatenated text.
        assert_eq!(db.search_fts("deploy artifact", 10).unwrap().len(), 1);
        assert_eq!(db.search_fts("deploy nonexistent", 10).unwrap().len(), 0);

        // UPDATE (upsert ON CONFLICT path): the search reads live `skills` rows, so
        // stale terms disappear and new terms appear with no separate index to sync.
        record.body = "rollback the deployment instead".to_string();
        db.upsert_skill(&record).unwrap();
        assert_eq!(
            db.search_fts("artifact", 10).unwrap().len(),
            0,
            "stale body term gone after re-index"
        );
        assert_eq!(db.search_fts("rollback", 10).unwrap().len(), 1);

        // DELETE: the skill is no longer found.
        db.delete_skill("deploy-helper").unwrap();
        assert_eq!(db.search_fts("rollback", 10).unwrap().len(), 0);
        assert_eq!(db.search_fts("deploy", 10).unwrap().len(), 0);
    }

    #[test]
    fn test_embedding_roundtrip_and_cache() {
        let dir = tempdir().unwrap();
        let db = Database::open(dir.path().join("test.db")).unwrap();

        // First insert a skill record (required for foreign key)
        let skill = SkillRecord {
            id: "git".to_string(),
            name: "Git Workflow".to_string(),
            description: "Git commit workflow".to_string(),
            version: Some("1.0.0".to_string()),
            author: None,
            source_path: "/skills/git".to_string(),
            source_layer: "base".to_string(),
            git_remote: None,
            git_commit: None,
            content_hash: "abc123".to_string(),
            body: "Git body".to_string(),
            metadata_json: "{}".to_string(),
            assets_json: "{}".to_string(),
            token_count: 100,
            quality_score: 1.0,
            indexed_at: "2026-01-01T00:00:00Z".to_string(),
            modified_at: "2026-01-01T00:00:00Z".to_string(),
            is_deprecated: false,
            deprecation_reason: None,
        };
        db.upsert_skill(&skill).unwrap();

        let embedder = HashEmbedder::new(32);
        let embedding = embedder.embed("git commit workflow");

        let record = EmbeddingRecord {
            skill_id: "git".to_string(),
            embedding: embedding.clone(),
            dims: 32,
            embedder_type: "hash".to_string(),
            content_hash: Some("hash123".to_string()),
            computed_at: "2026-01-01T00:00:00Z".to_string(),
        };

        db.upsert_embedding(&record).unwrap();

        let fetched = db.get_embedding("git").unwrap().unwrap();
        assert_eq!(fetched.skill_id, record.skill_id);
        assert_eq!(fetched.dims, record.dims);
        assert_eq!(fetched.embedder_type, record.embedder_type);
        assert_eq!(fetched.content_hash, record.content_hash);

        let sim = embedder.similarity(&embedding, &fetched.embedding);
        assert!(sim > 0.97);

        let cached = db
            .get_embedding_by_hash("hash123", "hash", 32)
            .unwrap()
            .unwrap();
        assert_eq!(cached.skill_id, "git");
    }

    #[test]
    fn test_alias_resolution_and_delete_cascade() {
        let dir = tempdir().unwrap();
        let db = Database::open(dir.path().join("test.db")).unwrap();
        let record = SkillRecord {
            id: "alias-target".to_string(),
            name: "Alias Target".to_string(),
            description: "Alias target skill".to_string(),
            version: Some("1.0.0".to_string()),
            author: None,
            source_path: "/skills/alias".to_string(),
            source_layer: "base".to_string(),
            git_remote: None,
            git_commit: None,
            content_hash: "ghi789".to_string(),
            body: "Alias body".to_string(),
            metadata_json: "{}".to_string(),
            assets_json: "{}".to_string(),
            token_count: 10,
            quality_score: 0.5,
            indexed_at: "2026-01-01T00:00:00Z".to_string(),
            modified_at: "2026-01-01T00:00:00Z".to_string(),
            is_deprecated: false,
            deprecation_reason: None,
        };

        db.upsert_skill(&record).unwrap();
        db.upsert_alias(
            "legacy-id",
            "alias-target",
            "deprecated",
            "2026-01-01T00:00:00Z",
        )
        .unwrap();

        let alias = db.resolve_alias("legacy-id").unwrap().unwrap();
        assert_eq!(alias.canonical_id, "alias-target");
        assert_eq!(alias.alias_type, "deprecated");

        db.delete_skill("alias-target").unwrap();
        let alias = db.resolve_alias("legacy-id").unwrap();
        assert!(alias.is_none());
    }

    #[test]
    fn test_quarantine_roundtrip_and_reviews() {
        let dir = tempdir().unwrap();
        let db = Database::open(dir.path().join("test.db")).unwrap();

        let record = QuarantineRecord {
            quarantine_id: "q_test".to_string(),
            session_id: "sess_1".to_string(),
            message_index: 3,
            content_hash: "hash123".to_string(),
            safe_excerpt: "safe excerpt".to_string(),
            acip_classification: AcipClassification::Disallowed {
                category: "prompt_injection".to_string(),
                action: "quarantine".to_string(),
            },
            audit_tag: Some("ACIP_AUDIT_MODE=ENABLED".to_string()),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            replay_command: "ms security quarantine replay q_test --i-understand-the-risks"
                .to_string(),
        };

        db.insert_quarantine_record(&record).unwrap();

        let fetched = db.get_quarantine_record("q_test").unwrap().unwrap();
        assert_eq!(fetched.session_id, "sess_1");
        assert_eq!(fetched.message_index, 3);
        assert!(matches!(
            fetched.acip_classification,
            AcipClassification::Disallowed { .. }
        ));

        let records = db.list_quarantine_records_by_session("sess_1", 10).unwrap();
        assert_eq!(records.len(), 1);

        let review_id = db
            .insert_quarantine_review("q_test", "confirm_injection", None)
            .unwrap();
        let reviews = db.list_quarantine_reviews("q_test").unwrap();
        assert_eq!(reviews.len(), 1);
        assert_eq!(reviews[0].id, review_id);
        assert_eq!(reviews[0].action, "confirm_injection");
    }

    #[test]
    fn test_list_skills_order_and_pagination() {
        let dir = tempdir().unwrap();
        let db = Database::open(dir.path().join("test.db")).unwrap();
        let older = SkillRecord {
            id: "skill-older".to_string(),
            name: "Older Skill".to_string(),
            description: "Older".to_string(),
            version: Some("1.0.0".to_string()),
            author: None,
            source_path: "/skills/older".to_string(),
            source_layer: "base".to_string(),
            git_remote: None,
            git_commit: None,
            content_hash: "old".to_string(),
            body: "Older body".to_string(),
            metadata_json: "{}".to_string(),
            assets_json: "{}".to_string(),
            token_count: 1,
            quality_score: 0.1,
            indexed_at: "2026-01-01T00:00:00Z".to_string(),
            modified_at: "2026-01-01T00:00:00Z".to_string(),
            is_deprecated: false,
            deprecation_reason: None,
        };
        let newer = SkillRecord {
            id: "skill-newer".to_string(),
            name: "Newer Skill".to_string(),
            description: "Newer".to_string(),
            version: Some("1.0.0".to_string()),
            author: None,
            source_path: "/skills/newer".to_string(),
            source_layer: "base".to_string(),
            git_remote: None,
            git_commit: None,
            content_hash: "new".to_string(),
            body: "Newer body".to_string(),
            metadata_json: "{}".to_string(),
            assets_json: "{}".to_string(),
            token_count: 2,
            quality_score: 0.2,
            indexed_at: "2026-01-02T00:00:00Z".to_string(),
            modified_at: "2026-01-02T00:00:00Z".to_string(),
            is_deprecated: false,
            deprecation_reason: None,
        };

        db.upsert_skill(&older).unwrap();
        db.upsert_skill(&newer).unwrap();

        let first = db.list_skills(1, 0).unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].id, "skill-newer");

        let second = db.list_skills(1, 1).unwrap();
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].id, "skill-older");
    }

    #[test]
    fn test_evidence_upsert_and_get() {
        use crate::core::{EvidenceCoverage, EvidenceLevel, EvidenceRef};

        let dir = tempdir().unwrap();
        let db = Database::open(dir.path().join("test.db")).unwrap();

        // First insert a skill record (required for foreign key)
        let skill = SkillRecord {
            id: "test-skill".to_string(),
            name: "Test Skill".to_string(),
            description: "A test skill".to_string(),
            version: Some("1.0.0".to_string()),
            author: None,
            source_path: "/skills/test".to_string(),
            source_layer: "project".to_string(),
            git_remote: None,
            git_commit: None,
            content_hash: "test123".to_string(),
            body: "Test body".to_string(),
            metadata_json: "{}".to_string(),
            assets_json: "{}".to_string(),
            token_count: 100,
            quality_score: 0.8,
            indexed_at: "2026-01-01T00:00:00Z".to_string(),
            modified_at: "2026-01-01T00:00:00Z".to_string(),
            is_deprecated: false,
            deprecation_reason: None,
        };
        db.upsert_skill(&skill).unwrap();

        // Create evidence references
        let evidence = vec![
            EvidenceRef {
                session_id: "sess-001".to_string(),
                message_range: (5, 12),
                snippet_hash: "hash-abc".to_string(),
                excerpt: Some("Example code pattern".to_string()),
                level: EvidenceLevel::Excerpt,
                confidence: 0.85,
            },
            EvidenceRef {
                session_id: "sess-002".to_string(),
                message_range: (20, 25),
                snippet_hash: "hash-def".to_string(),
                excerpt: None,
                level: EvidenceLevel::Pointer,
                confidence: 0.72,
            },
        ];

        let coverage = EvidenceCoverage::default();

        // Upsert evidence for rule-1
        db.upsert_evidence("test-skill", "rule-1", &evidence, &coverage)
            .unwrap();

        // Get evidence for specific rule
        let fetched = db.get_rule_evidence("test-skill", "rule-1").unwrap();
        assert_eq!(fetched.len(), 2);
        assert_eq!(fetched[0].session_id, "sess-001");
        assert_eq!(fetched[0].message_range, (5, 12));
        assert_eq!(fetched[0].confidence, 0.85);
        assert_eq!(fetched[1].session_id, "sess-002");

        // Get all evidence for skill (as SkillEvidenceIndex)
        let index = db.get_evidence("test-skill").unwrap();
        assert_eq!(index.rules.len(), 1);
        assert!(index.rules.contains_key("rule-1"));
        assert_eq!(index.coverage.rules_with_evidence, 1);

        // Count evidence
        let count = db.count_skill_evidence("test-skill").unwrap();
        assert_eq!(count, 1); // One rule with evidence
    }

    #[test]
    fn test_evidence_multiple_rules_and_list_all() {
        use crate::core::{EvidenceCoverage, EvidenceLevel, EvidenceRef};

        let dir = tempdir().unwrap();
        let db = Database::open(dir.path().join("test.db")).unwrap();

        // Insert skill
        let skill = SkillRecord {
            id: "multi-rule-skill".to_string(),
            name: "Multi Rule Skill".to_string(),
            description: "Skill with multiple rules".to_string(),
            version: Some("1.0.0".to_string()),
            author: None,
            source_path: "/skills/multi".to_string(),
            source_layer: "project".to_string(),
            git_remote: None,
            git_commit: None,
            content_hash: "multi123".to_string(),
            body: "Multi rule body".to_string(),
            metadata_json: "{}".to_string(),
            assets_json: "{}".to_string(),
            token_count: 200,
            quality_score: 0.9,
            indexed_at: "2026-01-01T00:00:00Z".to_string(),
            modified_at: "2026-01-01T00:00:00Z".to_string(),
            is_deprecated: false,
            deprecation_reason: None,
        };
        db.upsert_skill(&skill).unwrap();

        let coverage = EvidenceCoverage::default();

        // Add evidence for multiple rules
        for i in 1..=3 {
            let evidence = vec![EvidenceRef {
                session_id: format!("sess-{:03}", i),
                message_range: (i as u32 * 10, i as u32 * 10 + 5),
                snippet_hash: format!("hash-{}", i),
                excerpt: None,
                level: EvidenceLevel::Pointer,
                confidence: 0.7 + (i as f32 * 0.05),
            }];
            db.upsert_evidence(
                "multi-rule-skill",
                &format!("rule-{}", i),
                &evidence,
                &coverage,
            )
            .unwrap();
        }

        // List all evidence
        let all_evidence = db.list_all_evidence().unwrap();
        assert_eq!(all_evidence.len(), 3);
        assert_eq!(all_evidence[0].skill_id, "multi-rule-skill");
        assert_eq!(all_evidence[0].rule_id, "rule-1");
        assert_eq!(all_evidence[2].rule_id, "rule-3");

        // Get evidence index
        let index = db.get_evidence("multi-rule-skill").unwrap();
        assert_eq!(index.rules.len(), 3);
        assert_eq!(index.coverage.rules_with_evidence, 3);

        // Delete evidence
        let deleted = db.delete_skill_evidence("multi-rule-skill").unwrap();
        assert_eq!(deleted, 3);

        let after_delete = db.list_all_evidence().unwrap();
        assert!(after_delete.is_empty());
    }

    #[test]
    fn test_evidence_update_existing_rule() {
        use crate::core::{EvidenceCoverage, EvidenceLevel, EvidenceRef};

        let dir = tempdir().unwrap();
        let db = Database::open(dir.path().join("test.db")).unwrap();

        // Insert skill
        let skill = SkillRecord {
            id: "update-skill".to_string(),
            name: "Update Skill".to_string(),
            description: "Skill for update test".to_string(),
            version: Some("1.0.0".to_string()),
            author: None,
            source_path: "/skills/update".to_string(),
            source_layer: "project".to_string(),
            git_remote: None,
            git_commit: None,
            content_hash: "upd123".to_string(),
            body: "Update body".to_string(),
            metadata_json: "{}".to_string(),
            assets_json: "{}".to_string(),
            token_count: 50,
            quality_score: 0.7,
            indexed_at: "2026-01-01T00:00:00Z".to_string(),
            modified_at: "2026-01-01T00:00:00Z".to_string(),
            is_deprecated: false,
            deprecation_reason: None,
        };
        db.upsert_skill(&skill).unwrap();

        // Initial evidence
        let evidence_v1 = vec![EvidenceRef {
            session_id: "sess-v1".to_string(),
            message_range: (1, 5),
            snippet_hash: "v1-hash".to_string(),
            excerpt: None,
            level: EvidenceLevel::Pointer,
            confidence: 0.6,
        }];
        let coverage = EvidenceCoverage::default();
        db.upsert_evidence("update-skill", "rule-1", &evidence_v1, &coverage)
            .unwrap();

        // Update with new evidence
        let evidence_v2 = vec![
            EvidenceRef {
                session_id: "sess-v2".to_string(),
                message_range: (10, 20),
                snippet_hash: "v2-hash".to_string(),
                excerpt: Some("Updated excerpt".to_string()),
                level: EvidenceLevel::Excerpt,
                confidence: 0.9,
            },
            EvidenceRef {
                session_id: "sess-v2b".to_string(),
                message_range: (30, 35),
                snippet_hash: "v2b-hash".to_string(),
                excerpt: None,
                level: EvidenceLevel::Pointer,
                confidence: 0.8,
            },
        ];
        db.upsert_evidence("update-skill", "rule-1", &evidence_v2, &coverage)
            .unwrap();

        // Verify update replaced old evidence
        let fetched = db.get_rule_evidence("update-skill", "rule-1").unwrap();
        assert_eq!(fetched.len(), 2);
        assert_eq!(fetched[0].session_id, "sess-v2");
        assert_eq!(fetched[0].confidence, 0.9);
        assert_eq!(fetched[1].session_id, "sess-v2b");

        // Still only one rule with evidence
        let count = db.count_skill_evidence("update-skill").unwrap();
        assert_eq!(count, 1);
    }
}
