-- 002_add_fts.sql
-- Full-text search (FTS5) and triggers.
--
-- skills_fts is a SELF-CONTAINED FTS5 table: it owns its own copy of the
-- indexed columns. We deliberately do NOT use `content='skills'` /
-- `content_rowid`. fsqlite's external-content FTS5 requires the content table to
-- expose a backing column for every FTS column, but `tags` is derived from the
-- metadata JSON and has no column on `skills` (meta_skill#120 — external content
-- read of `tags` fails with `no such column: T.tags`). A self-contained table
-- sidesteps that; search still joins back to `skills` on rowid for the canonical
-- row, so storing a second copy of the indexed text costs little.
--
-- The triggers populate skills_fts with `INSERT ... SELECT FROM skills` rather
-- than passing `NEW.*` / `OLD.*` straight into the FTS INSERT. fsqlite's VDBE
-- codegen rejects `NEW.<col>` / `OLD.<col>` used directly as virtual-table
-- INSERT arguments inside a trigger (NotImplemented: "expression form is not
-- supported in this connection path: Column(ColumnRef table=NEW ...)"). Reading
-- the columns back from a `skills` table scan keyed by the row's `id` avoids that
-- gap entirely and lets `tags` be derived with `json_extract` on the table's own
-- `metadata_json` column. The selected `rowid` is written as the FTS rowid so the
-- search-time join (`skills_fts.rowid = skills.rowid`) resolves.

CREATE VIRTUAL TABLE skills_fts USING fts5(
    name,
    description,
    body,
    tags
);

-- INSERT: index the freshly inserted skill row.
CREATE TRIGGER skills_ai AFTER INSERT ON skills BEGIN
    INSERT INTO skills_fts(rowid, name, description, body, tags)
    SELECT rowid, name, description, body,
           json_extract(metadata_json, '$.tags')
    FROM skills WHERE id = NEW.id;
END;

-- DELETE: drop the FTS row. Runs BEFORE the delete so the `skills` row (and its
-- rowid) is still resolvable via the canonical `id` key.
CREATE TRIGGER skills_bd BEFORE DELETE ON skills BEGIN
    DELETE FROM skills_fts
    WHERE rowid = (SELECT rowid FROM skills WHERE id = OLD.id);
END;

-- UPDATE: re-index in place. `id` is the immutable primary key, so the rowid is
-- stable across the upsert's ON CONFLICT path; delete the stale FTS row then
-- re-insert the current values.
CREATE TRIGGER skills_au AFTER UPDATE ON skills BEGIN
    DELETE FROM skills_fts
    WHERE rowid = (SELECT rowid FROM skills WHERE id = NEW.id);
    INSERT INTO skills_fts(rowid, name, description, body, tags)
    SELECT rowid, name, description, body,
           json_extract(metadata_json, '$.tags')
    FROM skills WHERE id = NEW.id;
END;
