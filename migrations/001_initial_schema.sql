-- 001_initial_schema.sql
-- Core tables (excluding FTS and embeddings)

PRAGMA foreign_keys = ON;

-- Core skill registry
CREATE TABLE skills (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    version TEXT,
    author TEXT,

    -- Source tracking
    source_path TEXT NOT NULL,
    source_layer TEXT NOT NULL,  -- base | org | project | user
    git_remote TEXT,
    git_commit TEXT,
    content_hash TEXT NOT NULL,

    -- Content
    body TEXT NOT NULL,
    metadata_json TEXT NOT NULL,
    assets_json TEXT NOT NULL,

    -- Computed
    token_count INTEGER NOT NULL,
    quality_score REAL NOT NULL,

    -- Timestamps
    indexed_at TEXT NOT NULL,
    modified_at TEXT NOT NULL,

    -- Status
    is_deprecated INTEGER NOT NULL DEFAULT 0,
    deprecation_reason TEXT
);

-- Alternate names / legacy ids
CREATE TABLE skill_aliases (
    alias TEXT PRIMARY KEY,
    skill_id TEXT NOT NULL,
    alias_type TEXT NOT NULL, -- alias | deprecated
    created_at TEXT NOT NULL,
    FOREIGN KEY(skill_id) REFERENCES skills(id) ON DELETE CASCADE
);

CREATE INDEX idx_skill_aliases_skill ON skill_aliases(skill_id);

-- Precompiled runtime skillpack cache
CREATE TABLE skill_packs (
    skill_id TEXT PRIMARY KEY REFERENCES skills(id),
    pack_path TEXT NOT NULL,
    spec_hash TEXT NOT NULL,
    slices_hash TEXT NOT NULL,
    embedding_hash TEXT NOT NULL,
    predicate_index_hash TEXT NOT NULL,
    generated_at TEXT NOT NULL
);

-- Pre-sliced content blocks for token packing
CREATE TABLE skill_slices (
    skill_id TEXT NOT NULL REFERENCES skills(id),
    slices_json TEXT NOT NULL,  -- SkillSliceIndex
    updated_at TEXT NOT NULL,
    PRIMARY KEY (skill_id)
);

-- Rule-level evidence and provenance
CREATE TABLE skill_evidence (
    skill_id TEXT NOT NULL REFERENCES skills(id),
    rule_id TEXT NOT NULL,
    evidence_json TEXT NOT NULL,   -- JSON array of EvidenceRef
    coverage_json TEXT NOT NULL,   -- EvidenceCoverage snapshot
    updated_at TEXT NOT NULL,
    PRIMARY KEY (skill_id, rule_id)
);

CREATE INDEX idx_evidence_skill ON skill_evidence(skill_id);

-- Rule strength calibration (0.0 - 1.0)
CREATE TABLE skill_rules (
    skill_id TEXT NOT NULL REFERENCES skills(id),
    rule_id TEXT NOT NULL,
    strength REAL NOT NULL DEFAULT 0.5,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (skill_id, rule_id)
);

-- Uncertainty queue for low-confidence generalizations
CREATE TABLE uncertainty_queue (
    id TEXT PRIMARY KEY,
    pattern_json TEXT NOT NULL,      -- ExtractedPattern
    reason TEXT NOT NULL,
    confidence REAL NOT NULL,
    suggested_queries TEXT NOT NULL, -- JSON array
    auto_mine_attempts INTEGER NOT NULL DEFAULT 0,
    last_mined_at TEXT,
    status TEXT NOT NULL,            -- pending | resolved | discarded
    created_at TEXT NOT NULL
);

CREATE INDEX idx_uncertainty_status ON uncertainty_queue(status);

-- Redaction reports for privacy and secret-scrubbing
CREATE TABLE redaction_reports (
    id INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL,
    report_json TEXT NOT NULL,   -- RedactionReport
    created_at TEXT NOT NULL
);

CREATE INDEX idx_redaction_session ON redaction_reports(session_id);

-- Prompt injection reports for safety filtering
CREATE TABLE injection_reports (
    id INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL,
    acip_version TEXT,
    acip_mode TEXT,
    acip_audit_mode INTEGER,
    report_json TEXT NOT NULL,   -- InjectionReport
    created_at TEXT NOT NULL
);

CREATE INDEX idx_injection_session ON injection_reports(session_id);

-- Command safety events (DCG decisions + policy enforcement)
CREATE TABLE command_safety_events (
    id INTEGER PRIMARY KEY,
    session_id TEXT,
    command TEXT NOT NULL,
    dcg_version TEXT,
    dcg_pack TEXT,
    decision_json TEXT NOT NULL,  -- DcgDecision
    created_at TEXT NOT NULL
);

CREATE INDEX idx_command_safety_session ON command_safety_events(session_id);

-- Skill usage tracking
CREATE TABLE skill_usage (
    id INTEGER PRIMARY KEY,
    skill_id TEXT NOT NULL REFERENCES skills(id),
    project_path TEXT,
    used_at TEXT NOT NULL,
    disclosure_level INTEGER NOT NULL,
    context_keywords TEXT,  -- JSON array
    success_signal INTEGER, -- 1 = worked well, 0 = didn't help, NULL = unknown
    experiment_id TEXT,
    variant_id TEXT
);

-- Skill usage events (full detail for effectiveness analysis)
CREATE TABLE skill_usage_events (
    id TEXT PRIMARY KEY,
    skill_id TEXT NOT NULL REFERENCES skills(id),
    session_id TEXT NOT NULL,
    loaded_at TEXT NOT NULL,
    disclosure_level TEXT NOT NULL,   -- JSON
    discovery_method TEXT NOT NULL,   -- JSON
    experiment_id TEXT,
    variant_id TEXT,
    outcome TEXT,                     -- JSON
    feedback TEXT                     -- JSON
);

-- Per-rule outcomes for calibration
CREATE TABLE rule_outcomes (
    id TEXT PRIMARY KEY,
    skill_id TEXT NOT NULL REFERENCES skills(id),
    rule_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    followed INTEGER NOT NULL,
    outcome TEXT NOT NULL,     -- JSON SessionOutcome
    created_at TEXT NOT NULL
);

-- UBS static analysis reports (quality gates)
CREATE TABLE ubs_reports (
    id INTEGER PRIMARY KEY,
    project_path TEXT,
    run_at TEXT NOT NULL,
    exit_code INTEGER NOT NULL,
    report_json TEXT NOT NULL      -- UbsReport
);

CREATE INDEX idx_ubs_project ON ubs_reports(project_path);

-- CM (cass-memory) rule link registry
CREATE TABLE cm_rule_links (
    id TEXT PRIMARY KEY,
    cm_rule_id TEXT NOT NULL,
    ms_rule_id TEXT NOT NULL,
    linkage_json TEXT NOT NULL,    -- CmRuleLink
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_cm_rule ON cm_rule_links(cm_rule_id);

-- CM sync state (import/export checkpoints)
CREATE TABLE cm_sync_state (
    id INTEGER PRIMARY KEY,
    cm_db_path TEXT,
    last_imported_at TEXT,
    last_exported_at TEXT,
    status_json TEXT,              -- CmSyncStatus
    updated_at TEXT NOT NULL
);

-- A/B experiments for skill variants
CREATE TABLE skill_experiments (
    id TEXT PRIMARY KEY,
    skill_id TEXT NOT NULL REFERENCES skills(id),
    scope TEXT NOT NULL DEFAULT 'skill', -- skill | slice
    scope_id TEXT,                       -- slice_id if scope = slice
    variants_json TEXT NOT NULL,      -- Vec<ExperimentVariant>
    allocation_json TEXT NOT NULL,    -- AllocationStrategy
    status TEXT NOT NULL,
    started_at TEXT NOT NULL
);

-- Local reservation fallback (when Agent Mail is unavailable)
CREATE TABLE skill_reservations (
    id TEXT PRIMARY KEY,
    path_pattern TEXT NOT NULL,
    holder TEXT NOT NULL,
    exclusive INTEGER NOT NULL,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);

-- Skill relationships
CREATE TABLE skill_dependencies (
    skill_id TEXT NOT NULL REFERENCES skills(id),
    depends_on TEXT NOT NULL REFERENCES skills(id),
    PRIMARY KEY (skill_id, depends_on)
);

-- Capability index (for "provides")
CREATE TABLE skill_capabilities (
    capability TEXT NOT NULL,
    skill_id TEXT NOT NULL REFERENCES skills(id),
    PRIMARY KEY (capability, skill_id)
);

-- Build sessions (CASS integration)
CREATE TABLE build_sessions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    status TEXT NOT NULL,  -- 'draft', 'refining', 'complete', 'published'

    -- CASS queries that seeded this build
    cass_queries TEXT NOT NULL,  -- JSON array

    -- Extracted patterns
    patterns_json TEXT NOT NULL,

    -- Generated skill (in progress or complete)
    draft_skill_json TEXT,

    -- Deterministic source-of-truth
    skill_spec_json TEXT,   -- SkillSpec (structured parts)

    -- Iteration tracking
    iteration_count INTEGER NOT NULL DEFAULT 0,
    last_feedback TEXT,

    -- Timestamps
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Config store
CREATE TABLE config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Two-phase commit transactions
CREATE TABLE tx_log (
    id TEXT PRIMARY KEY,
    entity_type TEXT NOT NULL,   -- skill | usage | config | build
    entity_id TEXT NOT NULL,
    phase TEXT NOT NULL,         -- prepare | commit | complete
    payload_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);

-- CASS session fingerprints for incremental processing
CREATE TABLE cass_fingerprints (
    session_id TEXT PRIMARY KEY,
    content_hash TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Indexes
CREATE INDEX idx_skills_name ON skills(name);
CREATE INDEX idx_skills_modified ON skills(modified_at);
CREATE INDEX idx_skills_quality ON skills(quality_score DESC);
CREATE INDEX idx_usage_skill ON skill_usage(skill_id);
CREATE INDEX idx_usage_time ON skill_usage(used_at);
