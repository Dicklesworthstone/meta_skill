//! SQLite database layer

use std::path::Path;

use half::f16;
use rusqlite::{params, Connection, Row};
use serde_json::Value as JsonValue;
use uuid::Uuid;

use crate::error::{MsError, Result};
use crate::security::QuarantineRecord;
use crate::storage::migrations;

/// SQLite database wrapper for skill registry
pub struct Database {
    conn: Connection,
    schema_version: u32,
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
        self.conn
            .execute("DELETE FROM tx_log WHERE id = ?", [id])?;
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
        let classification_json = serde_json::to_string(&record.acip_classification)
            .map_err(|err| crate::error::MsError::Config(format!("encode classification: {err}")))?;
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
                    _ => super::tx::TxPhase::Complete,
                };

                let created_str: String = row.get(5)?;
                let created_at = chrono::DateTime::parse_from_rfc3339(&created_str)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now());

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

    /// Upsert a skill with pending source_path marker
    pub fn upsert_skill_pending(&self, skill: &crate::core::SkillSpec) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO skills
             (id, name, description, version, author, source_path, source_layer,
              content_hash, body, metadata_json, assets_json, token_count, quality_score,
              indexed_at, modified_at)
             VALUES (?, ?, ?, ?, ?, 'pending', 'user', 'pending', '', ?, '{}', 0, 0.0,
                     datetime('now'), datetime('now'))",
            params![
                skill.metadata.id,
                skill.metadata.name,
                skill.metadata.description,
                skill.metadata.version,
                skill.metadata.author,
                serde_json::to_string(&skill.metadata).unwrap_or_default(),
            ],
        )?;
        Ok(())
    }

    /// Finalize a skill commit by updating source_path and content_hash
    pub fn finalize_skill_commit(
        &self,
        skill_id: &str,
        source_path: &str,
        content_hash: &str,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE skills SET source_path = ?, content_hash = ?, modified_at = datetime('now')
             WHERE id = ?",
            params![source_path, content_hash, skill_id],
        )?;
        Ok(())
    }

    /// Run SQLite integrity check
    pub fn integrity_check(&self) -> Result<bool> {
        let result: String = self.conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        Ok(result == "ok")
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
    let classification: JsonValue = serde_json::from_str(&classification_json)
        .map_err(|err| rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(err)))?;
    let acip_classification = serde_json::from_value(classification)
        .map_err(|err| rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(err)))?;

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

#[derive(Debug, Clone, serde::Serialize)]
pub struct QuarantineReview {
    pub id: String,
    pub quarantine_id: String,
    pub action: String,
    pub reason: Option<String>,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use crate::security::AcipClassification;
    use crate::search::embeddings::HashEmbedder;

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
        db.upsert_alias("legacy-id", "alias-target", "deprecated", "2026-01-01T00:00:00Z")
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
            replay_command: "ms security quarantine replay q_test --i-understand-the-risks".to_string(),
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
}
