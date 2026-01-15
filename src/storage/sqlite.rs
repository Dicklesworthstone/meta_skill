//! SQLite database layer

use std::path::Path;

use half::f16;
use rusqlite::{Connection, Row, params};
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::error::{MsError, Result};
use crate::security::{CommandSafetyEvent, QuarantineRecord};
use crate::storage::migrations;

/// SQLite database wrapper for skill registry
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

impl Database {
    /// Open database at the given path
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)?;

        Self::configure_pragmas(&conn)?;
        let schema_version = migrations::run_migrations(&conn)?;

        Ok(Self {
            conn,
            schema_version,
        })
    }

    /// Get a reference to the connection
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Current schema version after migrations.
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn get_skill(&self, id: &str) -> Result<Option<SkillRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, description, version, author, source_path, source_layer, \
             git_remote, git_commit, content_hash, body, metadata_json, assets_json, \
             token_count, quality_score, indexed_at, modified_at, is_deprecated, deprecation_reason \
             FROM skills WHERE id = ?",
        )?;
        let mut rows = stmt.query([id])?;
        if let Some(row) = rows.next()? {
            return Ok(Some(skill_from_row(row)?));
        }
        Ok(None)
    }

    pub fn list_skills(&self, limit: usize, offset: usize) -> Result<Vec<SkillRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, description, version, author, source_path, source_layer, \
             git_remote, git_commit, content_hash, body, metadata_json, assets_json, \
             token_count, quality_score, indexed_at, modified_at, is_deprecated, deprecation_reason \
             FROM skills ORDER BY modified_at DESC LIMIT ? OFFSET ?",
        )?;
        let rows = stmt.query_map(params![limit as i64, offset as i64], |row| {
            skill_from_row(row)
        })?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Update quality score for a skill.
    pub fn update_skill_quality(&self, skill_id: &str, quality_score: f64) -> Result<()> {
        self.conn.execute(
            "UPDATE skills SET quality_score = ? WHERE id = ?",
            params![quality_score, skill_id],
        )?;
        Ok(())
    }

    /// Count usage events for a skill.
    pub fn count_skill_usage(&self, skill_id: &str) -> Result<u64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM skill_usage WHERE skill_id = ?",
            [skill_id],
            |row| row.get(0),
        )?;
        Ok(count.max(0) as u64)
    }

    /// Count evidence records for a skill.
    pub fn count_skill_evidence(&self, skill_id: &str) -> Result<u64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM skill_evidence WHERE skill_id = ?",
            [skill_id],
            |row| row.get(0),
        )?;
        Ok(count.max(0) as u64)
    }

    pub fn upsert_skill(&self, skill: &SkillRecord) -> Result<()> {
        self.conn.execute(
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
                source_layer=excluded.source_layer,
                metadata_json=excluded.metadata_json,
                token_count=excluded.token_count,
                modified_at=excluded.modified_at",
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
                if skill.is_deprecated { 1 } else { 0 },
                skill.deprecation_reason,
            ],
        )?;
        Ok(())
    }

    pub fn delete_skill(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM skills WHERE id = ?", [id])?;
        Ok(())
    }

    /// Delete a skill only if it has pending status
    pub fn delete_pending_skill(&self, id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM skills WHERE id = ? AND source_path = 'pending'",
            [id],
        )?;
        Ok(())
    }

    /// Delete a transaction record from tx_log
    pub fn delete_tx_record(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM tx_log WHERE id = ?", [id])?;
        Ok(())
    }

    pub fn resolve_alias(&self, alias: &str) -> Result<Option<AliasResolution>> {
        let mut stmt = self
            .conn
            .prepare("SELECT skill_id, alias_type FROM skill_aliases WHERE alias = ?")?;
        let mut rows = stmt.query([alias])?;
        if let Some(row) = rows.next()? {
            return Ok(Some(AliasResolution {
                canonical_id: row.get(0)?,
                alias_type: row.get(1)?,
            }));
        }
        Ok(None)
    }

    pub fn upsert_alias(
        &self,
        alias: &str,
        skill_id: &str,
        alias_type: &str,
        created_at: &str,
    ) -> Result<()> {
        self.conn.execute(
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
            .execute("DELETE FROM skill_aliases WHERE alias = ?", [alias])?;
        Ok(count > 0)
    }

    /// List all aliases, optionally filtered by skill_id
    pub fn list_aliases(&self, skill_id: Option<&str>) -> Result<Vec<AliasRecord>> {
        let mut records = Vec::new();

        if let Some(sid) = skill_id {
            let mut stmt = self.conn.prepare(
                "SELECT alias, skill_id, alias_type, created_at
                 FROM skill_aliases
                 WHERE skill_id = ?
                 ORDER BY alias",
            )?;
            let rows = stmt.query_map([sid], |row| {
                Ok(AliasRecord {
                    alias: row.get(0)?,
                    skill_id: row.get(1)?,
                    alias_type: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })?;
            for row in rows {
                records.push(row?);
            }
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT alias, skill_id, alias_type, created_at
                 FROM skill_aliases
                 ORDER BY skill_id, alias",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(AliasRecord {
                    alias: row.get(0)?,
                    skill_id: row.get(1)?,
                    alias_type: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })?;
            for row in rows {
                records.push(row?);
            }
        }

        Ok(records)
    }

    /// Get aliases for a specific skill
    pub fn get_aliases_for_skill(&self, skill_id: &str) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT alias FROM skill_aliases WHERE skill_id = ? ORDER BY alias")?;
        let rows = stmt.query_map([skill_id], |row| row.get(0))?;
        let mut aliases = Vec::new();
        for row in rows {
            aliases.push(row?);
        }
        Ok(aliases)
    }

    pub fn search_fts(&self, query: &str, limit: usize) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id
             FROM skills_fts f
             JOIN skills s ON s.rowid = f.rowid
             WHERE skills_fts MATCH ?
             ORDER BY bm25(skills_fts)
             LIMIT ?",
        )?;
        let rows = stmt.query_map(params![query, limit as i64], |row| row.get(0))?;
        let mut ids = Vec::new();
        for row in rows {
            ids.push(row?);
        }
        Ok(ids)
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

        self.conn.execute(
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
        let mut stmt = self.conn.prepare(
            "SELECT skill_id, embedding, dims, embedder_type, content_hash, computed_at, created_at
             FROM skill_embeddings
             WHERE skill_id = ?",
        )?;
        let mut rows = stmt.query([skill_id])?;
        if let Some(row) = rows.next()? {
            return Ok(Some(embedding_from_row(row)?));
        }
        Ok(None)
    }

    pub fn get_embedding_by_hash(
        &self,
        content_hash: &str,
        embedder_type: &str,
        dims: usize,
    ) -> Result<Option<EmbeddingRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT skill_id, embedding, dims, embedder_type, content_hash, computed_at, created_at
             FROM skill_embeddings
             WHERE content_hash = ? AND embedder_type = ? AND dims = ?
             LIMIT 1",
        )?;
        let mut rows = stmt.query(params![content_hash, embedder_type, dims as i64])?;
        if let Some(row) = rows.next()? {
            return Ok(Some(embedding_from_row(row)?));
        }
        Ok(None)
    }

    pub fn insert_quarantine_record(&self, record: &QuarantineRecord) -> Result<()> {
        let classification_json =
            serde_json::to_string(&record.acip_classification).map_err(|err| {
                crate::error::MsError::Config(format!("encode classification: {err}"))
            })?;
        self.conn.execute(
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
        self.conn.execute(
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
        let mut stmt = self.conn.prepare(
            "SELECT session_id, command, dcg_version, dcg_pack, decision_json, created_at
             FROM command_safety_events
             ORDER BY created_at DESC
             LIMIT ?",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            let decision_json: String = row.get(4)?;
            let decision = serde_json::from_str(&decision_json).map_err(|err| {
                rusqlite::Error::FromSqlConversionFailure(
                    4,
                    rusqlite::types::Type::Text,
                    Box::new(err),
                )
            })?;
            Ok(CommandSafetyEvent {
                session_id: row.get(0)?,
                command: row.get(1)?,
                dcg_version: row.get(2)?,
                dcg_pack: row.get(3)?,
                decision,
                created_at: row.get(5)?,
            })
        })?;

        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_quarantine_records(&self, limit: usize) -> Result<Vec<QuarantineRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT quarantine_id, session_id, message_index, content_hash, safe_excerpt,
                    classification_json, audit_tag, created_at, replay_command
             FROM injection_quarantine
             ORDER BY created_at DESC
             LIMIT ?",
        )?;
        let mut rows = stmt.query(params![limit as i64])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(quarantine_from_row(row)?);
        }
        Ok(out)
    }

    pub fn list_quarantine_records_by_session(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<QuarantineRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT quarantine_id, session_id, message_index, content_hash, safe_excerpt,
                    classification_json, audit_tag, created_at, replay_command
             FROM injection_quarantine
             WHERE session_id = ?
             ORDER BY created_at DESC
             LIMIT ?",
        )?;
        let mut rows = stmt.query(params![session_id, limit as i64])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(quarantine_from_row(row)?);
        }
        Ok(out)
    }

    pub fn get_quarantine_record(&self, quarantine_id: &str) -> Result<Option<QuarantineRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT quarantine_id, session_id, message_index, content_hash, safe_excerpt,
                    classification_json, audit_tag, created_at, replay_command
             FROM injection_quarantine
             WHERE quarantine_id = ?",
        )?;
        let mut rows = stmt.query([quarantine_id])?;
        if let Some(row) = rows.next()? {
            return Ok(Some(quarantine_from_row(row)?));
        }
        Ok(None)
    }

    pub fn insert_quarantine_review(
        &self,
        quarantine_id: &str,
        action: &str,
        reason: Option<&str>,
    ) -> Result<String> {
        let review_id = format!("qr_{}", Uuid::new_v4());
        let created_at = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO injection_quarantine_reviews (
                id, quarantine_id, action, reason, created_at
             ) VALUES (?, ?, ?, ?, ?)",
            params![review_id, quarantine_id, action, reason, created_at],
        )?;
        Ok(review_id)
    }

    pub fn list_quarantine_reviews(&self, quarantine_id: &str) -> Result<Vec<QuarantineReview>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, quarantine_id, action, reason, created_at
             FROM injection_quarantine_reviews
             WHERE quarantine_id = ?
             ORDER BY created_at DESC",
        )?;
        let mut rows = stmt.query([quarantine_id])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(QuarantineReview {
                id: row.get(0)?,
                quarantine_id: row.get(1)?,
                action: row.get(2)?,
                reason: row.get(3)?,
                created_at: row.get(4)?,
            });
        }
        Ok(out)
    }

    // =========================================================================
    // TRANSACTION LOG METHODS (for 2PC)
    // =========================================================================

    /// Insert a transaction record into tx_log
    pub fn insert_tx_record(&self, tx: &super::tx::TxRecord) -> Result<()> {
        self.conn.execute(
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
        self.conn.execute(
            "UPDATE tx_log SET phase = ? WHERE id = ?",
            params![phase.to_string(), tx_id],
        )?;
        Ok(())
    }

    /// Check if a transaction exists in tx_log
    pub fn tx_exists(&self, tx_id: &str) -> Result<bool> {
        let exists: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM tx_log WHERE id = ?)",
            [tx_id],
            |row| row.get(0),
        )?;
        Ok(exists)
    }

    /// List incomplete transactions (not in Complete phase)
    pub fn list_incomplete_transactions(&self) -> Result<Vec<super::tx::TxRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, entity_type, entity_id, phase, payload_json, created_at
             FROM tx_log WHERE phase != 'complete'",
        )?;

        let txs = stmt
            .query_map([], |row| {
                let phase_str: String = row.get(3)?;
                let phase = match phase_str.as_str() {
                    "prepare" => super::tx::TxPhase::Prepare,
                    "pending" => super::tx::TxPhase::Pending,
                    "committed" => super::tx::TxPhase::Committed,
                    "complete" => super::tx::TxPhase::Complete,
                    unknown => {
                        return Err(rusqlite::Error::FromSqlConversionFailure(
                            3,
                            rusqlite::types::Type::Text,
                            Box::new(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!("unknown transaction phase: {}", unknown),
                            )),
                        ));
                    }
                };

                let created_str: String = row.get(5)?;
                let created_at = chrono::DateTime::parse_from_rfc3339(&created_str)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            5,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;

                Ok(super::tx::TxRecord {
                    id: row.get(0)?,
                    entity_type: row.get(1)?,
                    entity_id: row.get(2)?,
                    phase,
                    payload_json: row.get(4)?,
                    created_at,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(txs)
    }

    /// Insert or update a skill during 2PC pending phase.
    ///
    /// For NEW skills: inserts with source_path='pending' marker.
    /// For EXISTING skills: updates only metadata fields, preserving the original
    /// source_path and content_hash. This ensures rollback won't corrupt committed data.
    ///
    /// The source_path and content_hash are only finalized by `finalize_skill_commit`
    /// after Git commit succeeds.
    pub fn upsert_skill_pending(
        &self,
        skill: &crate::core::SkillSpec,
        layer: crate::core::SkillLayer,
        token_count: i64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO skills
             (id, name, description, version, author, source_path, source_layer,
              content_hash, body, metadata_json, assets_json, token_count, quality_score,
              indexed_at, modified_at)
             VALUES (?, ?, ?, ?, ?, 'pending', ?, 'pending', '', ?, '{}', ?, 0.0,
                     datetime('now'), datetime('now'))
             ON CONFLICT(id) DO UPDATE SET
                name=excluded.name,
                description=excluded.description,
                version=excluded.version,
                author=excluded.author,
                source_layer=excluded.source_layer,
                metadata_json=excluded.metadata_json,
                token_count=excluded.token_count,
                modified_at=excluded.modified_at",
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

    /// Finalize a skill commit by updating source_path, content_hash, and body.
    ///
    /// This is called after Git commit succeeds to populate the full SQLite record
    /// with searchable content (body for FTS).
    pub fn finalize_skill_commit(
        &self,
        skill_id: &str,
        source_path: &str,
        content_hash: &str,
        body: &str,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE skills SET source_path = ?, content_hash = ?, body = ?, modified_at = datetime('now')
             WHERE id = ?",
            params![source_path, content_hash, body, skill_id],
        )?;
        Ok(())
    }

    /// Run SQLite integrity check
    pub fn integrity_check(&self) -> Result<bool> {
        let result: String = self
            .conn
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        Ok(result == "ok")
    }

    // =========================================================================
    // SESSION QUALITY CACHE METHODS
    // =========================================================================

    /// Get cached session quality by session_id
    pub fn get_session_quality(&self, session_id: &str) -> Result<Option<SessionQualityRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT session_id, content_hash, score, signals_json, missing_json, computed_at
             FROM session_quality
             WHERE session_id = ?",
        )?;
        let mut rows = stmt.query([session_id])?;
        if let Some(row) = rows.next()? {
            return Ok(Some(session_quality_from_row(row)?));
        }
        Ok(None)
    }

    /// Upsert session quality record
    pub fn upsert_session_quality(&self, record: &SessionQualityRecord) -> Result<()> {
        let signals_json = serde_json::to_string(&record.signals)
            .map_err(|err| MsError::Config(format!("encode signals: {err}")))?;
        let missing_json = serde_json::to_string(&record.missing)
            .map_err(|err| MsError::Config(format!("encode missing: {err}")))?;

        self.conn.execute(
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
                record.score as f64,
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

        self.conn.execute(
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

    /// Get all evidence for a skill, reconstructuting the SkillEvidenceIndex.
    pub fn get_evidence(&self, skill_id: &str) -> Result<crate::core::SkillEvidenceIndex> {
        let mut stmt = self.conn.prepare(
            "SELECT rule_id, evidence_json, coverage_json
             FROM skill_evidence
             WHERE skill_id = ?
             ORDER BY rule_id",
        )?;

        let mut rules = std::collections::BTreeMap::new();
        let mut total_confidence = 0.0f32;
        let mut evidence_count = 0usize;

        let rows = stmt.query_map([skill_id], |row| {
            let rule_id: String = row.get(0)?;
            let evidence_json: String = row.get(1)?;
            Ok((rule_id, evidence_json))
        })?;

        for row in rows {
            let (rule_id, evidence_json) = row?;
            let evidence_refs: Vec<crate::core::EvidenceRef> = serde_json::from_str(&evidence_json)
                .map_err(|err| {
                    MsError::Config(format!("decode evidence for rule {}: {}", rule_id, err))
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
        let mut stmt = self.conn.prepare(
            "SELECT evidence_json FROM skill_evidence WHERE skill_id = ? AND rule_id = ?",
        )?;

        let mut rows = stmt.query(params![skill_id, rule_id])?;
        if let Some(row) = rows.next()? {
            let evidence_json: String = row.get(0)?;
            let evidence_refs: Vec<crate::core::EvidenceRef> = serde_json::from_str(&evidence_json)
                .map_err(|err| MsError::Config(format!("decode evidence: {err}")))?;
            return Ok(evidence_refs);
        }
        Ok(vec![])
    }

    /// List all evidence records for provenance graph export.
    /// Returns (skill_id, rule_id, evidence_refs, updated_at) tuples.
    pub fn list_all_evidence(&self) -> Result<Vec<EvidenceRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT skill_id, rule_id, evidence_json, updated_at
             FROM skill_evidence
             ORDER BY skill_id, rule_id",
        )?;

        let rows = stmt.query_map([], |row| {
            let skill_id: String = row.get(0)?;
            let rule_id: String = row.get(1)?;
            let evidence_json: String = row.get(2)?;
            let updated_at: String = row.get(3)?;
            Ok((skill_id, rule_id, evidence_json, updated_at))
        })?;

        let mut records = Vec::new();
        for row in rows {
            let (skill_id, rule_id, evidence_json, updated_at) = row?;
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
        let count = self
            .conn
            .execute("DELETE FROM skill_evidence WHERE skill_id = ?", [skill_id])?;
        Ok(count)
    }

    pub fn record_skill_outcome(&self, skill_id: &str, success: bool) -> Result<()> {
        let id = Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().to_rfc3339();
        // We use skill_usage_events for outcomes for now
        self.conn.execute(
            "INSERT INTO skill_usage_events (id, skill_id, session_id, loaded_at, disclosure_level, discovery_method, outcome, feedback)
             VALUES (?, ?, 'manual', ?, 'full', 'manual', ?, 'null')",
            params![id, skill_id, created_at, if success { "success" } else { "failure" }],
        )?;
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

        self.conn.execute(
            "INSERT INTO skill_feedback (id, skill_id, feedback_type, rating, comment, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
            params![id, skill_id, feedback_type, rating, comment, created_at],
        )?;

        Ok(SkillFeedbackRecord {
            id,
            skill_id: skill_id.to_string(),
            feedback_type: feedback_type.to_string(),
            rating,
            comment: comment.map(|s| s.to_string()),
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
        let mut args = Vec::new();

        if let Some(sid) = skill_id {
            sql.push_str(" WHERE skill_id = ?");
            args.push(sid.to_string()); // Ownership work-around
        }

        sql.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");

        // Use a simpler query flow to avoid borrowing issues
        let mut stmt = self.conn.prepare(&sql)?;

        let mut rows = if let Some(sid) = skill_id {
            stmt.query(params![sid, limit as i64, offset as i64])?
        } else {
            stmt.query(params![limit as i64, offset as i64])?
        };

        let mut records = Vec::new();
        while let Some(row) = rows.next()? {
            records.push(SkillFeedbackRecord {
                id: row.get(0)?,
                skill_id: row.get(1)?,
                feedback_type: row.get(2)?,
                rating: row.get(3)?,
                comment: row.get(4)?,
                created_at: row.get(5)?,
            });
        }
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

        self.conn.execute(
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
            scope_id: scope_id.map(|s| s.to_string()),
            variants_json: variants_json.to_string(),
            allocation_json: allocation_json.to_string(),
            status: status.to_string(),
            started_at,
        })
    }

    pub fn list_skill_experiments(
        &self,
        skill_id: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ExperimentRecord>> {
        let mut sql = "SELECT id, skill_id, scope, scope_id, variants_json, allocation_json, status, started_at 
                       FROM skill_experiments".to_string();

        if let Some(_) = skill_id {
            sql.push_str(" WHERE skill_id = ?");
        }

        sql.push_str(" ORDER BY started_at DESC LIMIT ? OFFSET ?");

        let mut stmt = self.conn.prepare(&sql)?;

        let mut rows = if let Some(sid) = skill_id {
            stmt.query(params![sid, limit as i64, offset as i64])?
        } else {
            stmt.query(params![limit as i64, offset as i64])?
        };

        let mut records = Vec::new();
        while let Some(row) = rows.next()? {
            records.push(ExperimentRecord {
                id: row.get(0)?,
                skill_id: row.get(1)?,
                scope: row.get(2)?,
                scope_id: row.get(3)?,
                variants_json: row.get(4)?,
                allocation_json: row.get(5)?,
                status: row.get(6)?,
                started_at: row.get(7)?,
            });
        }
        Ok(records)
    }

    fn configure_pragmas(conn: &Connection) -> Result<()> {
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

fn skill_from_row(row: &Row<'_>) -> rusqlite::Result<SkillRecord> {
    Ok(SkillRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        description: row.get(2)?,
        version: row.get(3)?,
        author: row.get(4)?,
        source_path: row.get(5)?,
        source_layer: row.get(6)?,
        git_remote: row.get(7)?,
        git_commit: row.get(8)?,
        content_hash: row.get(9)?,
        body: row.get(10)?,
        metadata_json: row.get(11)?,
        assets_json: row.get(12)?,
        token_count: row.get(13)?,
        quality_score: row.get(14)?,
        indexed_at: row.get(15)?,
        modified_at: row.get(16)?,
        is_deprecated: row.get::<_, i64>(17)? != 0,
        deprecation_reason: row.get(18)?,
    })
}

fn embedding_from_row(row: &Row<'_>) -> Result<EmbeddingRecord> {
    let skill_id: String = row.get(0)?;
    let blob: Vec<u8> = row.get(1)?;
    let dims: i64 = row.get(2)?;
    let embedder_type: String = row.get(3)?;
    let content_hash: Option<String> = row.get(4)?;
    let computed_at: String = row.get(5)?;
    let created_at: String = row.get(6)?;

    let dims_usize = if dims <= 0 { 0 } else { dims as usize };
    let computed_at = if computed_at.is_empty() {
        created_at
    } else {
        computed_at
    };

    let embedding = decode_embedding_f16(&blob, dims_usize)?;

    Ok(EmbeddingRecord {
        skill_id,
        embedding,
        dims: dims_usize,
        embedder_type,
        content_hash,
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

fn quarantine_from_row(row: &Row<'_>) -> std::result::Result<QuarantineRecord, rusqlite::Error> {
    let classification_json: String = row.get(5)?;
    let classification: JsonValue = serde_json::from_str(&classification_json).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(err))
    })?;
    let acip_classification = serde_json::from_value(classification).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(err))
    })?;

    Ok(QuarantineRecord {
        quarantine_id: row.get(0)?,
        session_id: row.get(1)?,
        message_index: row.get::<_, i64>(2)? as usize,
        content_hash: row.get(3)?,
        safe_excerpt: row.get(4)?,
        acip_classification,
        audit_tag: row.get(6)?,
        created_at: row.get(7)?,
        replay_command: row.get(8)?,
    })
}

fn session_quality_from_row(row: &Row<'_>) -> Result<SessionQualityRecord> {
    let signals_json: String = row.get(3)?;
    let missing_json: String = row.get(4)?;

    let signals: Vec<String> = serde_json::from_str(&signals_json)
        .map_err(|err| MsError::Config(format!("decode signals: {err}")))?;
    let missing: Vec<String> = serde_json::from_str(&missing_json)
        .map_err(|err| MsError::Config(format!("decode missing: {err}")))?;

    Ok(SessionQualityRecord {
        session_id: row.get(0)?,
        content_hash: row.get(1)?,
        score: row.get::<_, f64>(2)? as f32,
        signals,
        missing,
        computed_at: row.get(5)?,
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
            .query_row("PRAGMA journal_mode;", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mode.to_lowercase(), "wal");
    }

    #[test]
    fn test_all_tables_created() {
        let dir = tempdir().unwrap();
        let db = Database::open(dir.path().join("test.db")).unwrap();
        let tables = [
            "skills",
            "skill_aliases",
            "skills_fts",
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
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
                    [table],
                    |row| row.get(0),
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
        assert!(results.contains(&"rust-errors".to_string()));
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

        let coverage = EvidenceCoverage {
            total_rules: 5,
            rules_with_evidence: 1,
            avg_confidence: 0.785,
        };

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

        let coverage = EvidenceCoverage::default();

        // Initial evidence
        let evidence_v1 = vec![EvidenceRef {
            session_id: "sess-v1".to_string(),
            message_range: (1, 5),
            snippet_hash: "v1-hash".to_string(),
            excerpt: None,
            level: EvidenceLevel::Pointer,
            confidence: 0.6,
        }];
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
