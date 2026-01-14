-- 006_add_embedding_metadata.sql
-- Extend skill_embeddings with metadata for caching + compatibility

ALTER TABLE skill_embeddings ADD COLUMN dims INTEGER NOT NULL DEFAULT 384;
ALTER TABLE skill_embeddings ADD COLUMN embedder_type TEXT NOT NULL DEFAULT 'hash';
ALTER TABLE skill_embeddings ADD COLUMN content_hash TEXT;
ALTER TABLE skill_embeddings ADD COLUMN computed_at TEXT NOT NULL DEFAULT '';

CREATE INDEX IF NOT EXISTS idx_skill_embeddings_type ON skill_embeddings(embedder_type);
CREATE INDEX IF NOT EXISTS idx_skill_embeddings_hash ON skill_embeddings(content_hash);
