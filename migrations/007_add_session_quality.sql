CREATE TABLE session_quality (
    session_id TEXT PRIMARY KEY,
    content_hash TEXT NOT NULL,
    score REAL NOT NULL,
    signals_json TEXT NOT NULL,
    missing_json TEXT NOT NULL,
    computed_at TEXT NOT NULL
);

CREATE INDEX idx_session_quality_score ON session_quality(score);
