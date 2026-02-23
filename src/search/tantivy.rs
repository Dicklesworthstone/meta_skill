//! Tantivy BM25 full-text search
//!
//! Implements BM25 full-text search using Tantivy. Part of the hybrid search
//! system that combines BM25 with hash embeddings via RRF fusion.

use std::path::Path;
use std::sync::RwLock;

use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{
    Field, IndexRecordOption, STORED, STRING, Schema, TextFieldIndexing, TextOptions, Value,
};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument};

use crate::error::{MsError, Result};
use crate::storage::sqlite::SkillRecord;

/// BM25 search index using Tantivy
pub struct Bm25Index {
    index: Index,
    reader: IndexReader,
    writer: Option<RwLock<IndexWriter>>,
    // Field handles for fast access
    fields: BM25Fields,
}

/// Field handles for the BM25 schema
#[derive(Clone)]
struct BM25Fields {
    id: Field,
    name: Field,
    description: Field,
    body: Field,
    tags: Field,
    aliases: Field,
    layer: Field,
    quality_score: Field,
    deprecated: Field,
}

/// A single BM25 search result
#[derive(Debug, Clone)]
pub struct Bm25Result {
    /// Skill ID
    pub skill_id: String,
    /// BM25 score
    pub score: f32,
    /// Skill name (for display)
    pub name: String,
    /// Source layer
    pub layer: String,
}

impl Bm25Index {
    /// Open or create a BM25 index at the given path
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        std::fs::create_dir_all(path)?;

        let schema = build_schema();
        let fields = extract_fields(&schema)?;

        // Try to open existing index, or create new one
        let index = if path.join("meta.json").exists() {
            Index::open_in_dir(path)?
        } else {
            Index::create_in_dir(path, schema)?
        };

        // Create reader with manual reload (we control when to refresh)
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;

        // Create writer with 50MB buffer
        let writer = index.writer(50_000_000)?;

        Ok(Self {
            index,
            reader,
            writer: Some(RwLock::new(writer)),
            fields,
        })
    }

    /// Open an existing index in read-only mode (no write lock acquired).
    ///
    /// This allows concurrent readers without blocking on the Tantivy write lock,
    /// making it suitable for the MCP server and other read-only access patterns.
    pub fn open_readonly(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.join("meta.json").exists() {
            return Err(MsError::SearchIndex(tantivy::TantivyError::InternalError(
                format!(
                    "Index directory does not exist or is empty: {}",
                    path.display()
                ),
            )));
        }

        let schema = build_schema();
        let fields = extract_fields(&schema)?;
        let index = Index::open_in_dir(path)?;

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        Ok(Self {
            index,
            reader,
            writer: None,
            fields,
        })
    }

    /// Open an in-memory index (for testing)
    pub fn open_in_memory() -> Result<Self> {
        let schema = build_schema();
        let fields = extract_fields(&schema)?;

        let index = Index::create_in_ram(schema);
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        let writer = index.writer(15_000_000)?;

        Ok(Self {
            index,
            reader,
            writer: Some(RwLock::new(writer)),
            fields,
        })
    }

    /// Returns true if this index was opened in read-only mode.
    pub fn is_readonly(&self) -> bool {
        self.writer.is_none()
    }

    fn require_writer(&self) -> Result<&RwLock<IndexWriter>> {
        self.writer.as_ref().ok_or_else(|| {
            MsError::SearchIndex(tantivy::TantivyError::InternalError(
                "Index opened in read-only mode; write operations are not available".to_string(),
            ))
        })
    }

    /// Index a skill record
    pub fn index_skill(&self, skill: &SkillRecord) -> Result<()> {
        // Parse tags and aliases from metadata JSON
        let (tags, aliases) = parse_metadata(&skill.metadata_json);

        let mut doc = TantivyDocument::new();
        doc.add_text(self.fields.id, &skill.id);
        doc.add_text(self.fields.name, &skill.name);
        doc.add_text(self.fields.description, &skill.description);
        doc.add_text(self.fields.body, &skill.body);
        doc.add_text(self.fields.tags, &tags);
        doc.add_text(self.fields.aliases, &aliases);
        doc.add_text(self.fields.layer, &skill.source_layer);
        // Safely convert quality_score to u64, handling NaN/Inf/negative values
        let quality_u64 = if skill.quality_score.is_nan() || skill.quality_score.is_infinite() {
            0u64
        } else {
            (skill.quality_score.clamp(0.0, 100.0) * 100.0) as u64
        };
        doc.add_u64(self.fields.quality_score, quality_u64);
        doc.add_bool(self.fields.deprecated, skill.is_deprecated);

        // Delete any existing document with this ID first
        let id_term = tantivy::Term::from_field_text(self.fields.id, &skill.id);

        let writer = self.require_writer()?.write().map_err(|e| {
            MsError::SearchIndex(tantivy::TantivyError::InternalError(format!(
                "Failed to acquire write lock: {e}"
            )))
        })?;

        writer.delete_term(id_term);
        writer.add_document(doc)?;

        Ok(())
    }

    /// Index multiple skills in a batch
    ///
    /// This method commits changes at the end, making all indexed skills
    /// visible to subsequent searches.
    pub fn index_skills(&self, skills: &[SkillRecord]) -> Result<usize> {
        let mut count = 0;
        for skill in skills {
            self.index_skill(skill)?;
            count += 1;
        }
        // Commit to ensure changes are visible to readers
        self.commit()?;
        Ok(count)
    }

    /// Commit pending changes and reload the reader
    pub fn commit(&self) -> Result<()> {
        let mut writer = self.require_writer()?.write().map_err(|e| {
            MsError::SearchIndex(tantivy::TantivyError::InternalError(format!(
                "Failed to acquire write lock: {e}"
            )))
        })?;

        writer.commit()?;
        drop(writer); // Release lock before reload

        self.reader.reload()?;
        Ok(())
    }

    /// Delete a skill from the index
    pub fn delete_skill(&self, skill_id: &str) -> Result<()> {
        let id_term = tantivy::Term::from_field_text(self.fields.id, skill_id);

        let writer = self.require_writer()?.write().map_err(|e| {
            MsError::SearchIndex(tantivy::TantivyError::InternalError(format!(
                "Failed to acquire write lock: {e}"
            )))
        })?;

        writer.delete_term(id_term);
        Ok(())
    }

    /// Clear the entire index
    pub fn clear(&self) -> Result<()> {
        let mut writer = self.require_writer()?.write().map_err(|e| {
            MsError::SearchIndex(tantivy::TantivyError::InternalError(format!(
                "Failed to acquire write lock: {e}"
            )))
        })?;

        writer.delete_all_documents()?;
        writer.commit()?;
        drop(writer);

        self.reader.reload()?;
        Ok(())
    }

    /// Search skills by query
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<Bm25Result>> {
        let searcher = self.reader.searcher();

        // Multi-field query parser - search name, description, body, tags
        let query_parser = QueryParser::for_index(
            &self.index,
            vec![
                self.fields.name,
                self.fields.description,
                self.fields.body,
                self.fields.tags,
                self.fields.aliases,
            ],
        );

        let parsed_query = query_parser
            .parse_query(query)
            .map_err(|e| MsError::QueryParse(format!("Failed to parse query: {e}")))?;

        let top_docs = searcher.search(&parsed_query, &TopDocs::with_limit(limit))?;

        let mut results = Vec::with_capacity(top_docs.len());
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;

            let skill_id = doc
                .get_first(self.fields.id)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            let name = doc
                .get_first(self.fields.name)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            let layer = doc
                .get_first(self.fields.layer)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            results.push(Bm25Result {
                skill_id,
                score,
                name,
                layer,
            });
        }

        Ok(results)
    }

    /// Search with layer filter
    pub fn search_with_layer(
        &self,
        query: &str,
        layer: &str,
        limit: usize,
    ) -> Result<Vec<Bm25Result>> {
        let searcher = self.reader.searcher();

        // Build query with layer filter
        let query_parser = QueryParser::for_index(
            &self.index,
            vec![
                self.fields.name,
                self.fields.description,
                self.fields.body,
                self.fields.tags,
                self.fields.aliases,
            ],
        );

        // Combine text query with layer filter
        let normalized_layer = normalize_layer(layer);
        let filter_query = if query.trim().is_empty() {
            format!("layer:{normalized_layer}")
        } else {
            format!("{query} AND layer:{normalized_layer}")
        };
        let parsed_query = query_parser
            .parse_query(&filter_query)
            .map_err(|e| MsError::QueryParse(format!("Failed to parse query: {e}")))?;

        let top_docs = searcher.search(&parsed_query, &TopDocs::with_limit(limit))?;

        let mut results = Vec::with_capacity(top_docs.len());
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;

            let skill_id = doc
                .get_first(self.fields.id)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            let name = doc
                .get_first(self.fields.name)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            let layer_val = doc
                .get_first(self.fields.layer)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            results.push(Bm25Result {
                skill_id,
                score,
                name,
                layer: layer_val,
            });
        }

        Ok(results)
    }

    /// Get total number of indexed documents
    pub fn num_docs(&self) -> u64 {
        let searcher = self.reader.searcher();
        searcher.num_docs()
    }

    /// Check if index is empty
    pub fn is_empty(&self) -> bool {
        self.num_docs() == 0
    }
}

fn normalize_layer(input: &str) -> &'static str {
    // Normalize layer names to match stored values: base, org, project, user
    match input.to_lowercase().as_str() {
        "base" | "system" => "base",
        "org" | "global" => "org",
        "project" => "project",
        "user" | "local" => "user",
        _ => "project", // Default fallback
    }
}

/// Build the Tantivy schema for skill indexing
fn build_schema() -> Schema {
    let mut builder = Schema::builder();

    // Text field options with positions for phrase queries
    let text_options = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("default")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );

    // Skill identification (stored for retrieval)
    builder.add_text_field("id", STRING | STORED);
    builder.add_text_field("name", text_options.clone() | STORED);

    // Searchable content (not stored, just indexed)
    builder.add_text_field("description", text_options.clone());
    builder.add_text_field("body", text_options.clone());
    builder.add_text_field("tags", text_options.clone());
    builder.add_text_field("aliases", text_options);

    // Metadata for filtering (stored)
    builder.add_text_field("layer", STRING | STORED);
    builder.add_u64_field("quality_score", tantivy::schema::FAST | STORED);
    builder.add_bool_field("deprecated", STORED);

    builder.build()
}

/// Extract field handles from schema
fn extract_fields(schema: &Schema) -> Result<BM25Fields> {
    Ok(BM25Fields {
        id: schema.get_field("id").map_err(|_| {
            MsError::SearchIndex(tantivy::TantivyError::SchemaError(
                "missing id field".into(),
            ))
        })?,
        name: schema.get_field("name").map_err(|_| {
            MsError::SearchIndex(tantivy::TantivyError::SchemaError(
                "missing name field".into(),
            ))
        })?,
        description: schema.get_field("description").map_err(|_| {
            MsError::SearchIndex(tantivy::TantivyError::SchemaError(
                "missing description field".into(),
            ))
        })?,
        body: schema.get_field("body").map_err(|_| {
            MsError::SearchIndex(tantivy::TantivyError::SchemaError(
                "missing body field".into(),
            ))
        })?,
        tags: schema.get_field("tags").map_err(|_| {
            MsError::SearchIndex(tantivy::TantivyError::SchemaError(
                "missing tags field".into(),
            ))
        })?,
        aliases: schema.get_field("aliases").map_err(|_| {
            MsError::SearchIndex(tantivy::TantivyError::SchemaError(
                "missing aliases field".into(),
            ))
        })?,
        layer: schema.get_field("layer").map_err(|_| {
            MsError::SearchIndex(tantivy::TantivyError::SchemaError(
                "missing layer field".into(),
            ))
        })?,
        quality_score: schema.get_field("quality_score").map_err(|_| {
            MsError::SearchIndex(tantivy::TantivyError::SchemaError(
                "missing quality_score field".into(),
            ))
        })?,
        deprecated: schema.get_field("deprecated").map_err(|_| {
            MsError::SearchIndex(tantivy::TantivyError::SchemaError(
                "missing deprecated field".into(),
            ))
        })?,
    })
}

/// Parse tags and aliases from metadata JSON
fn parse_metadata(metadata_json: &str) -> (String, String) {
    let mut tags = String::new();
    let mut aliases = String::new();

    if let Ok(meta) = serde_json::from_str::<serde_json::Value>(metadata_json) {
        // Extract tags
        if let Some(tag_array) = meta.get("tags").and_then(|t| t.as_array()) {
            tags = tag_array
                .iter()
                .filter_map(|v| v.as_str())
                .map(str::to_lowercase)
                .collect::<Vec<_>>()
                .join(" ");
        }

        // Extract aliases
        if let Some(alias_array) = meta.get("aliases").and_then(|a| a.as_array()) {
            aliases = alias_array
                .iter()
                .filter_map(|v| v.as_str())
                .map(str::to_lowercase)
                .collect::<Vec<_>>()
                .join(" ");
        }
    }

    (tags, aliases)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_skill(id: &str, name: &str, description: &str, body: &str) -> SkillRecord {
        SkillRecord {
            id: id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            version: Some("1.0.0".to_string()),
            author: Some("test".to_string()),
            source_path: "/test/path".to_string(),
            source_layer: "project".to_string(),
            git_remote: None,
            git_commit: None,
            content_hash: "test-hash".to_string(),
            body: body.to_string(),
            metadata_json: r#"{"tags": ["git", "workflow"], "aliases": ["commit-skill"]}"#
                .to_string(),
            assets_json: "{}".to_string(),
            token_count: 100,
            quality_score: 0.85,
            indexed_at: "2025-01-01T00:00:00Z".to_string(),
            modified_at: "2025-01-01T00:00:00Z".to_string(),
            is_deprecated: false,
            deprecation_reason: None,
        }
    }

    #[test]
    fn test_open_in_memory() {
        let index = Bm25Index::open_in_memory().unwrap();
        assert!(index.is_empty());
    }

    #[test]
    fn test_index_and_search() {
        let index = Bm25Index::open_in_memory().unwrap();

        let skill = make_test_skill(
            "git-commit",
            "Git Commit Workflow",
            "How to create good git commits",
            "Use git commit -m to create commits. Always write descriptive messages.",
        );

        index.index_skill(&skill).unwrap();
        index.commit().unwrap();

        assert_eq!(index.num_docs(), 1);

        // Search for the skill
        let results = index.search("git commit", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].skill_id, "git-commit");
        assert!(results[0].score > 0.0);
    }

    #[test]
    fn test_search_multiple_skills() {
        let index = Bm25Index::open_in_memory().unwrap();

        let skill1 = make_test_skill(
            "git-commit",
            "Git Commit Workflow",
            "How to create good git commits",
            "Use git commit -m for commits.",
        );

        let skill2 = make_test_skill(
            "rust-error",
            "Rust Error Handling",
            "Best practices for error handling in Rust",
            "Use Result and Option types. Propagate errors with ?.",
        );

        index.index_skill(&skill1).unwrap();
        index.index_skill(&skill2).unwrap();
        index.commit().unwrap();

        assert_eq!(index.num_docs(), 2);

        // Search for git should return git skill first
        let results = index.search("git", 10).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].skill_id, "git-commit");

        // Search for rust should return rust skill first
        let results = index.search("rust error", 10).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].skill_id, "rust-error");
    }

    #[test]
    fn test_delete_skill() {
        let index = Bm25Index::open_in_memory().unwrap();

        let skill = make_test_skill("test-skill", "Test Skill", "A test skill", "Test content");

        index.index_skill(&skill).unwrap();
        index.commit().unwrap();
        assert_eq!(index.num_docs(), 1);

        index.delete_skill("test-skill").unwrap();
        index.commit().unwrap();
        assert_eq!(index.num_docs(), 0);
    }

    #[test]
    fn test_update_skill() {
        let index = Bm25Index::open_in_memory().unwrap();

        let mut skill = make_test_skill(
            "test-skill",
            "Test Skill",
            "Original description",
            "Original content",
        );

        index.index_skill(&skill).unwrap();
        index.commit().unwrap();

        // Update the skill
        skill.description = "Updated description".to_string();
        index.index_skill(&skill).unwrap();
        index.commit().unwrap();

        // Should still have only 1 document (replaced, not duplicated)
        assert_eq!(index.num_docs(), 1);

        // Search should find the updated content
        let results = index.search("updated", 10).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_clear_index() {
        let index = Bm25Index::open_in_memory().unwrap();

        for i in 0..5 {
            let skill = make_test_skill(
                &format!("skill-{i}"),
                &format!("Skill {i}"),
                "Description",
                "Content",
            );
            index.index_skill(&skill).unwrap();
        }
        index.commit().unwrap();
        assert_eq!(index.num_docs(), 5);

        index.clear().unwrap();
        assert_eq!(index.num_docs(), 0);
    }

    #[test]
    fn test_search_by_tags() {
        let index = Bm25Index::open_in_memory().unwrap();

        let skill = make_test_skill(
            "git-skill",
            "Git Skill",
            "Git workflow",
            "Content about git",
        );

        index.index_skill(&skill).unwrap();
        index.commit().unwrap();

        // Search by tag from metadata
        let results = index.search("workflow", 10).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_parse_metadata() {
        let json = r#"{"tags": ["git", "workflow"], "aliases": ["commit", "version-control"]}"#;
        let (tags, aliases) = parse_metadata(json);
        assert_eq!(tags, "git workflow");
        assert_eq!(aliases, "commit version-control");
    }

    #[test]
    fn test_parse_metadata_empty() {
        let (tags, aliases) = parse_metadata("{}");
        assert!(tags.is_empty());
        assert!(aliases.is_empty());
    }

    #[test]
    fn test_parse_metadata_invalid() {
        let (tags, aliases) = parse_metadata("not json");
        assert!(tags.is_empty());
        assert!(aliases.is_empty());
    }
}
