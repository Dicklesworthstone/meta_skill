-- Additive Phase 1 baseline for skill package/resource persistence.

ALTER TABLE skills ADD COLUMN bundle_hash TEXT;
ALTER TABLE skills ADD COLUMN manifest_json TEXT NOT NULL DEFAULT '{}';

CREATE TABLE IF NOT EXISTS skill_resources (
    skill_id TEXT NOT NULL,
    relative_path TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    discovered_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (skill_id, relative_path),
    FOREIGN KEY (skill_id) REFERENCES skills(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_skill_resources_skill_id ON skill_resources(skill_id);
CREATE INDEX IF NOT EXISTS idx_skill_resources_type ON skill_resources(resource_type);
