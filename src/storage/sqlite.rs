//! SQLite database layer

use std::path::Path;

use rusqlite::{params, Connection, Row};
use serde_json::Value as JsonValue;

use crate::error::Result;
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
        let rows = stmt.query_map(params![limit as i64], |row| quarantine_from_row(row))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
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

fn quarantine_from_row(row: &Row<'_>) -> std::result::Result<QuarantineRecord, rusqlite::Error> {
    let classification_json: String = row.get(5)?;
    let classification: JsonValue = serde_json::from_str(&classification_json)
        .map_err(|err| crate::error::MsError::Config(format!("invalid classification json: {err}")))?;
    let acip_classification = serde_json::from_value(classification)
        .map_err(|err| crate::error::MsError::Config(format!("decode classification: {err}")))?;

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

#[cfg(test)]
mod tests {
    use super::*;
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
