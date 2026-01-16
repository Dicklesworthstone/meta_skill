-- Migration 010: Add resolution cache table
-- Caches fully resolved skills (inheritance + composition applied)
-- Enables fast lookups and automatic invalidation

CREATE TABLE IF NOT EXISTS resolved_skill_cache (
    skill_id TEXT PRIMARY KEY,
    resolved_json TEXT NOT NULL,
    cache_key_hash TEXT NOT NULL,
    cached_at TEXT NOT NULL DEFAULT (datetime('now')),
    inheritance_chain TEXT NOT NULL,  -- JSON array of skill IDs
    included_from TEXT NOT NULL,       -- JSON array of skill IDs
    dependency_hashes TEXT NOT NULL    -- JSON object {skill_id: content_hash}
);

-- Index for cache key validation
CREATE INDEX IF NOT EXISTS idx_resolved_cache_key ON resolved_skill_cache(cache_key_hash);

-- Index for finding skills by dependency (for invalidation)
CREATE INDEX IF NOT EXISTS idx_resolved_cached_at ON resolved_skill_cache(cached_at);

-- Table for tracking skill dependency graph (for cache invalidation)
CREATE TABLE IF NOT EXISTS skill_dependency_graph (
    skill_id TEXT NOT NULL,
    depends_on TEXT NOT NULL,
    dependency_type TEXT NOT NULL,  -- 'extends' or 'includes'
    PRIMARY KEY (skill_id, depends_on)
);

-- Index for finding dependents (skills that depend on a given skill)
CREATE INDEX IF NOT EXISTS idx_dependency_depends_on ON skill_dependency_graph(depends_on);
