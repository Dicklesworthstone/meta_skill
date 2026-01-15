-- Add skill feedback tracking

CREATE TABLE IF NOT EXISTS skill_feedback (
    id TEXT PRIMARY KEY,
    skill_id TEXT NOT NULL REFERENCES skills(id),
    feedback_type TEXT NOT NULL,
    rating INTEGER,
    comment TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_skill_feedback_skill ON skill_feedback(skill_id);
CREATE INDEX IF NOT EXISTS idx_skill_feedback_created ON skill_feedback(created_at);
