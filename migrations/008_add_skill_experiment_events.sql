-- Add experiment event tracking for A/B testing
CREATE TABLE skill_experiment_events (
    id TEXT PRIMARY KEY,
    experiment_id TEXT NOT NULL REFERENCES skill_experiments(id),
    variant_id TEXT NOT NULL,
    event_type TEXT NOT NULL,        -- assign | outcome | conclude
    metrics_json TEXT,               -- JSON object
    context_json TEXT,               -- JSON object
    session_id TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_experiment_events_experiment ON skill_experiment_events(experiment_id);
CREATE INDEX idx_experiment_events_variant ON skill_experiment_events(experiment_id, variant_id);
