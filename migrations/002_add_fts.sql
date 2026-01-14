-- 002_add_fts.sql
-- Full-text search (FTS5) and triggers

CREATE VIRTUAL TABLE skills_fts USING fts5(
    name,
    description,
    body,
    tags,
    content='skills',
    content_rowid='rowid'
);

-- Triggers to keep FTS in sync (INSERT, UPDATE, DELETE)
CREATE TRIGGER skills_ai AFTER INSERT ON skills BEGIN
    INSERT INTO skills_fts(rowid, name, description, body, tags)
    VALUES (NEW.rowid, NEW.name, NEW.description, NEW.body,
            (SELECT json_extract(NEW.metadata_json, '$.tags')));
END;

CREATE TRIGGER skills_ad AFTER DELETE ON skills BEGIN
    INSERT INTO skills_fts(skills_fts, rowid, name, description, body, tags)
    VALUES ('delete', OLD.rowid, OLD.name, OLD.description, OLD.body,
            (SELECT json_extract(OLD.metadata_json, '$.tags')));
END;

CREATE TRIGGER skills_au AFTER UPDATE ON skills BEGIN
    INSERT INTO skills_fts(skills_fts, rowid, name, description, body, tags)
    VALUES ('delete', OLD.rowid, OLD.name, OLD.description, OLD.body,
            (SELECT json_extract(OLD.metadata_json, '$.tags')));
    INSERT INTO skills_fts(rowid, name, description, body, tags)
    VALUES (NEW.rowid, NEW.name, NEW.description, NEW.body,
            (SELECT json_extract(NEW.metadata_json, '$.tags')));
END;
