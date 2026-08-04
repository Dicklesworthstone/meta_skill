-- 002_add_fts.sql
-- Full-text search index (historical).
--
-- This added an FTS5 `skills_fts` table for lexical search. It turned out to be
-- unusable on fsqlite: fsqlite 0.1.10 does not route FTS5 `MATCH` through its SQL
-- planner (FTS5 is only reachable via a programmatic API), so `WHERE skills_fts
-- MATCH ?` always failed with `column not found: skills_fts`, and the sync
-- triggers broke `ms index` (meta_skill#120).
--
-- Migration 013 drops this table and lexical search now runs as a substring scan
-- over the `skills` text columns (see `Database::search_fts`). The bare table is
-- still created here for migration-history fidelity (and is immediately removed by
-- 013 on a fresh database); the original sync triggers are intentionally omitted
-- since 013 removes the table regardless.

CREATE VIRTUAL TABLE skills_fts USING fts5(
    name,
    description,
    body,
    tags
);
