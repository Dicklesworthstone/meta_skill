-- Migration 014: Skill origin provenance (issue #158)
--
-- The 2PC commit finalizes `skills.source_path` to ms's own git-archive
-- location (`.ms/archive/skills/by-id/<id>`), so once a skill is indexed the
-- row loses all memory of where it was actually discovered on disk. When a
-- skill's markdown source is renamed or removed, the old id's row survives
-- indefinitely and `ms search`/`ms list` keep surfacing it — with no
-- supported way to detect the orphan (the archive path always exists from
-- ms's own point of view).
--
-- This side table records, per skill, the discovered SKILL.md path and the
-- configured index root it was found under. Scoping checks to "origin_path
-- gone but origin_root still present" is what distinguishes a renamed/removed
-- source from a whole root being unplugged (likely intentional), avoiding
-- false-positive floods.
--
-- Rows are intentionally ABSENT for skills indexed before this migration and
-- for skills without a filesystem origin (imports, bundles, templates).
-- Absence means "origin unknown" and is never treated as stale.
CREATE TABLE IF NOT EXISTS skill_origins (
    skill_id TEXT PRIMARY KEY,
    origin_path TEXT NOT NULL,   -- discovered SKILL.md path at index time
    origin_root TEXT NOT NULL,   -- configured index root it was found under
    recorded_at TEXT NOT NULL DEFAULT (datetime('now'))
);
