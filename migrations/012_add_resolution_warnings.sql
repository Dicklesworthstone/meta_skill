-- Migration 012: Add warnings to resolution cache
ALTER TABLE resolved_skill_cache ADD COLUMN warnings_json TEXT NOT NULL DEFAULT '[]';
