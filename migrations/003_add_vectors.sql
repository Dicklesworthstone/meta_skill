-- 003_add_vectors.sql
-- Vector embeddings storage

CREATE TABLE skill_embeddings (
    skill_id TEXT PRIMARY KEY REFERENCES skills(id),
    embedding BLOB NOT NULL,  -- f16 quantized, 384 dimensions
    created_at TEXT NOT NULL
);
