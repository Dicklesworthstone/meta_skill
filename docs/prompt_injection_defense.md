# Prompt Injection Defense (ACIP v1.3 Integration)

This document defines how `ms` integrates ACIP v1.3 as the primary prompt-injection defense layer during session mining. It is implementation guidance for `meta_skill-fma`.

## Goals

- Treat untrusted content as data, never instructions.
- Detect and quarantine injected content without leaking it.
- Preserve operator visibility via audit tags (ACIP_AUDIT_MODE).
- Make defenses deterministic and testable.

## ACIP Source of Truth

- Repository: `/data/projects/acip`
- Prompt file: `/data/projects/acip/ACIP_v_1.3_Full_Text.md`
- Version pinned in config (default: `1.3`)

## Trust Boundaries

All content processed during mining is classified into a trust boundary before extraction:

| Content Source | Trust Level | Allowed to Contain Instructions |
|---|---|---|
| User messages | VerifyRequired | Only after explicit classification |
| Assistant messages | VerifyRequired | Only after explicit classification |
| Tool outputs | Untrusted | Never |
| File contents | Untrusted | Never |

Notes:
- "VerifyRequired" means ACIP classification must pass as Safe or SensitiveAllowed with constraints.
- Untrusted content is treated as data-only and can only be used for pattern extraction after sanitization.

## Decision Discipline (ACIP)

Classification outcomes:

- `Safe`: Extract patterns normally.
- `SensitiveAllowed { constraints }`: Extract patterns with defensive framing. Store constraints.
- `Disallowed { category, action }`: Quarantine; do not extract patterns.

No classification results are exposed to output content. Only safe excerpts are stored.

## Pipeline Flow (Recommended)

1. Load ACIP prompt and config (version + trust boundary settings).
2. Classify each session message (source-aware).
3. If `Disallowed` → quarantine and stop processing that message.
4. If `SensitiveAllowed` → redact and attach constraints.
5. If `Safe` → proceed to extraction.
6. Emit audit tags when audit mode is enabled.

This flow must occur before any pattern extraction or synthesis.

## Redaction Rules (Minimum)

- Strip any instruction-like prefixes (e.g., "ignore previous instructions").
- Replace secret-like substrings with `[REDACTED]`.
- Remove embedded tool-call sequences and JSON tool payloads.
- Preserve just enough context for human review in a safe excerpt.

The redaction layer must never emit raw content for `Disallowed` entries.

## Quarantine Model

Quarantine records store metadata without leaking the full payload:

- `quarantine_id`, `session_id`, `message_index`
- `content_hash` (hash of original content)
- `safe_excerpt` (redacted)
- `acip_classification`
- `audit_tag` (if audit mode enabled)
- `created_at`
- `replay_command` (requires explicit user acknowledgement)

Replay is always opt-in with explicit acknowledgement flags.

## CLI Commands (Design)

```text
ms security scan
ms security scan --session <session-id>
ms security scan --audit-mode

ms security quarantine list
ms security quarantine show <id>
ms security quarantine review <id> --confirm-injection
ms security quarantine review <id> --false-positive --reason "..."
ms security quarantine replay <id> --i-understand-the-risks

ms security acip status
ms security acip config
ms security acip version
ms security test --input "ignore previous instructions..."
```

## Robot Output (JSON Sketch)

All `ms security * --robot` commands should emit JSON only. Suggested shapes:

```json
{ "ok": true, "acip_version": "1.3", "audit_mode": false }
```

```json
{
  "scan": {
    "session_id": "sess_123",
    "classified": 48,
    "quarantined": 2,
    "safe": 42,
    "sensitive_allowed": 4
  }
}
```

```json
{
  "quarantine": [
    {
      "quarantine_id": "q_abc",
      "session_id": "sess_123",
      "message_index": 17,
      "classification": "Disallowed",
      "audit_tag": "ACIP_AUDIT_MODE=ENABLED",
      "created_at": "2026-01-14T07:00:00Z"
    }
  ]
}
```

## Config Keys (Proposed)

```toml
[security.acip]
enabled = true
version = "1.3"
prompt_path = "/data/projects/acip/ACIP_v_1.3_Full_Text.md"
audit_mode = false

[security.acip.trust]
user_messages = "verify_required"
assistant_messages = "verify_required"
tool_outputs = "untrusted"
file_contents = "untrusted"
```

## Data Structures (Rust Sketch)

```rust
pub enum TrustLevel {
    Trusted,
    VerifyRequired,
    Untrusted,
}

pub enum AcipClassification {
    Safe,
    SensitiveAllowed { constraints: Vec<String> },
    Disallowed { category: String, action: String },
}

pub struct QuarantineRecord {
    pub quarantine_id: String,
    pub session_id: String,
    pub message_index: usize,
    pub content_hash: String,
    pub safe_excerpt: String,
    pub acip_classification: AcipClassification,
    pub audit_tag: Option<String>,
    pub created_at: String,
    pub replay_command: String,
}
```

## Storage Requirements

Minimum SQLite fields for quarantine (names may vary):

- `quarantine_id` (TEXT PRIMARY KEY)
- `session_id` (TEXT)
- `message_index` (INTEGER)
- `content_hash` (TEXT)
- `safe_excerpt` (TEXT)
- `classification_json` (TEXT)
- `audit_tag` (TEXT NULL)
- `created_at` (TEXT)
- `replay_command` (TEXT)

Records must never store raw content for `Disallowed`.

## Logging Requirements

- INFO: ACIP version, prompt path, audit mode
- WARN: Classification = SensitiveAllowed (include constraints)
- ERROR: Disallowed classifications and quarantine creation

## Tests (Minimum)

- Load ACIP prompt successfully and validate version pin.
- Trust boundary mapping for each content source.
- Classification decision correctness (Safe, SensitiveAllowed, Disallowed).
- Quarantine record creation with safe excerpt only.
- Audit tag generation when audit mode is enabled.
- Replay requires explicit acknowledgement flag.

## Integration Checklist

- [ ] Load ACIP prompt and confirm version matches config
- [ ] Enforce trust boundaries per content source
- [ ] Classify all messages before extraction
- [ ] Quarantine on `Disallowed` with safe excerpt only
- [ ] Apply redaction for `SensitiveAllowed`
- [ ] Emit audit tags when audit mode enabled
- [ ] Block extraction if ACIP is unavailable (fail closed)
- [ ] Persist quarantine records to SQLite

## Failure Modes

- **ACIP prompt missing**: fail closed; surface `ACIP_PROMPT_MISSING`.
- **Classification error**: fail closed; quarantine with `classification_error`.
- **Redaction failure**: treat as `Disallowed` and quarantine.
- **Audit tag failure**: continue, but warn and mark record.

## Error Codes (Suggested)

| Code | When |
|---|---|
| `ACIP_PROMPT_MISSING` | ACIP prompt file not found |
| `ACIP_VERSION_MISMATCH` | Prompt version != config version |
| `ACIP_CLASSIFICATION_FAILED` | Classifier error or timeout |
| `ACIP_REDACTION_FAILED` | Redaction step failed |
| `ACIP_QUARANTINE_WRITE_FAILED` | Failed to persist quarantine record |

## Implementation Notes

- Classification must occur before any pattern extraction.
- Do not include raw injected content in outputs, logs, or reports.
- ACIP audit mode is mapped to `ACIP_AUDIT_MODE=ENABLED` tags.
- When ACIP is unavailable, fail closed for extraction and surface a clear error.
