-- 013_fix_fts.sql
-- Remove the FTS5 search objects (meta_skill#120).
--
-- `skills_fts` was an FTS5 virtual table queried via `WHERE skills_fts MATCH ?`.
-- fsqlite 0.1.10 does not route FTS5 `MATCH` through its SQL planner (FTS5 is
-- only reachable through a programmatic API), so that query always failed with
-- `column not found: skills_fts` and `ms search` never returned results. The
-- external-content sync triggers also broke `ms index`: on fsqlite they raise
-- "expression form is not supported ... Column(ColumnRef table=NEW ...)" (when
-- they pass NEW.* straight into the FTS insert) and `PrimaryKeyViolation`.
--
-- Lexical search now runs as a bounded substring scan over the `skills` text
-- columns (see `Database::search_fts`), which needs no FTS index, so the FTS5
-- table and its triggers are dropped here. `user_version`-tracked migrations
-- never re-run 002, so this migration is what repairs already-deployed databases
-- (whose `ms index` is otherwise broken by the leftover triggers). Idempotent:
-- safe on a fresh database that created the objects in 002.

DROP TRIGGER IF EXISTS skills_ai;
DROP TRIGGER IF EXISTS skills_ad;
DROP TRIGGER IF EXISTS skills_bd;
DROP TRIGGER IF EXISTS skills_au;
DROP TABLE IF EXISTS skills_fts;
