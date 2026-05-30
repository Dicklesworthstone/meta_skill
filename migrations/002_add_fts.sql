-- 002_add_fts.sql
-- Full-text search (FTS5) and triggers
--
-- NOTE: fsqlite's FTS5 extension does not recognise the `content_rowid='rowid'`
-- option, even though `rowid` is already the SQLite default. We drop the
-- explicit option here; the external-content table still maps to `skills`
-- via `content='skills'` and uses the default rowid mapping.
--
-- NOTE: fsqlite's VDBE codegen does not currently support `NEW.rowid` /
-- `OLD.rowid` as virtual-table INSERT arguments inside an AFTER trigger
-- (NotImplemented: "expression form is not supported in this connection
-- path: Column(ColumnRef table=NEW column=rowid)"). We sidestep that gap by
-- looking up the rowid via the canonical `skills.id` primary key. The
-- `(SELECT rowid FROM skills WHERE id = NEW.id)` round-trip is cheap because
-- `id` is a UNIQUE TEXT primary key with an autoindex.

CREATE VIRTUAL TABLE skills_fts USING fts5(
    name,
    description,
    body,
    tags,
    content='skills'
);

-- Triggers to keep FTS in sync (INSERT, UPDATE, DELETE)
CREATE TRIGGER skills_ai AFTER INSERT ON skills BEGIN
    INSERT INTO skills_fts(rowid, name, description, body, tags)
    VALUES ((SELECT rowid FROM skills WHERE id = NEW.id),
            NEW.name, NEW.description, NEW.body,
            (SELECT json_extract(NEW.metadata_json, '$.tags')));
END;

CREATE TRIGGER skills_ad AFTER DELETE ON skills BEGIN
    INSERT INTO skills_fts(skills_fts, rowid, name, description, body, tags)
    VALUES ('delete',
            (SELECT rowid FROM skills WHERE id = OLD.id),
            OLD.name, OLD.description, OLD.body,
            (SELECT json_extract(OLD.metadata_json, '$.tags')));
END;

CREATE TRIGGER skills_au AFTER UPDATE ON skills BEGIN
    INSERT INTO skills_fts(skills_fts, rowid, name, description, body, tags)
    VALUES ('delete',
            (SELECT rowid FROM skills WHERE id = OLD.id),
            OLD.name, OLD.description, OLD.body,
            (SELECT json_extract(OLD.metadata_json, '$.tags')));
    INSERT INTO skills_fts(rowid, name, description, body, tags)
    VALUES ((SELECT rowid FROM skills WHERE id = NEW.id),
            NEW.name, NEW.description, NEW.body,
            (SELECT json_extract(NEW.metadata_json, '$.tags')));
END;
