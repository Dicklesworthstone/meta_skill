-- Add user preferences for favorites and hidden skills

CREATE TABLE IF NOT EXISTS user_preferences (
    id TEXT PRIMARY KEY,
    skill_id TEXT NOT NULL REFERENCES skills(id),
    preference_type TEXT NOT NULL CHECK(preference_type IN ('favorite', 'hidden')),
    created_at TEXT NOT NULL,
    UNIQUE(skill_id, preference_type)
);

CREATE INDEX IF NOT EXISTS idx_user_preferences_skill ON user_preferences(skill_id);
CREATE INDEX IF NOT EXISTS idx_user_preferences_type ON user_preferences(preference_type);
