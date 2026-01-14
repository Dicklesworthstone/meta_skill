# Configuration System (ms.toml)

This document expands the configuration system design for `ms` (meta_skill). It mirrors the plan and adds concrete merge rules, env var mapping, and CLI behaviors so implementation can be unambiguous.

## Goals

- Deterministic precedence across all layers.
- Simple, inspectable defaults with explicit overrides.
- Helpful errors for invalid values and unknown keys.
- Zero-surprise merge semantics for lists and tables.

## Config Locations

Precedence (lowest to highest):

1. Built-in defaults (compiled into binary)
2. Global config: `~/.config/ms/config.toml`
3. Project config: `.ms/config.toml` (project-local; preferred)
4. Environment variables: `MS_*`
5. CLI flags

Overrides:

- `MS_CONFIG` sets an explicit config path (skips global + project unless CLI flag says otherwise).
- `MS_ROOT` sets the ms root (affects default search paths).

## File Structure (TOML)

Top-level sections and purpose:

- `[skill_paths]`: skill discovery roots by layer.
- `[layers]`: layer ordering + auto-detection.
- `[disclosure]`: default load level, budgets, and suggestion policy.
- `[search]`: weights and backend choice.
- `[embeddings]`: embedding backend configuration (if separate from `[search]`).
- `[cass]`: CASS discovery and session file patterns.
- `[cm]`: cass-memory integration.
- `[cache]`: local cache sizing and TTL.
- `[update]`: auto-update policy and channel.
- `[robot]`: output format and metadata inclusion.
- `[display]`: formatting and UI toggles.
- `[daemon]`: background watcher/daemon settings.
- `[sync]` / `[ru]`: repo sync integration (if used).

## Merge Semantics

The merge behavior is per-field and must be deterministic:

- Scalars (string, bool, number): higher-precedence layer replaces lower.
- Tables (structs): merge recursively by key.
- Arrays:
  - Default: replace entirely at the highest-precedence layer.
  - Optional: for specific keys, support `+` merge semantics (documented explicitly).

If a merge strategy is not explicitly defined for a key, default to replace.

Recommended explicit merge exceptions (if needed):

- `skill_paths.*`: merge unique values (preserve higher-precedence order first).
- `layers.priority`: replace (user intent is to define ordering).
- `keywords` or tag arrays: replace unless a `merge=true` flag is added.

## Environment Variable Mapping

Support two styles:

1. Global toggles:
   - `MS_CONFIG`, `MS_ROOT`, `MS_ROBOT`, `MS_LOG_LEVEL`, `MS_CACHE_DISABLED`

2. Hierarchical overrides:
   - `MS_<SECTION>_<KEY>` (uppercase, `.` and `-` become `_`).
   - Example: `MS_DISCLOSURE_TOKEN_BUDGET=800`
   - Example: `MS_SEARCH_SEMANTIC_WEIGHT=0.5`

Parsing rules:

- Booleans: `1/0`, `true/false`, `yes/no` (case-insensitive).
- Arrays: comma-separated values (trim whitespace).
- Durations: allow `300`, `300s`, `5m`, `2h` (parse to seconds).

Unknown env vars should produce a warning (not error) unless `--strict` is enabled.

## CLI Commands

Behavioral contract:

- `ms config show` prints effective config (after all merges).
- `ms config get <key>` supports dot-paths (e.g., `search.semantic_weight`).
- `ms config set <key> <value>` updates global config unless `--project` is set.
- `ms config reset <key>` deletes the key from the targeted config file.
- `ms config edit` opens file in `$EDITOR` (global by default).

Output formats:

- Human default: pretty table or TOML snippet.
- `--json` / `--robot`: machine output with metadata (source, resolved).

## Validation

Validation should be strict and actionable:

- Unknown keys: warn (or error with `--strict`).
- Invalid enum values: error with list of allowed values.
- Numeric bounds: enforce min/max where defined.
- `ms_version`, `version`: SemVer parse if present.

## Error Messages

Prefer structured errors:

- Code: `CONFIG_PARSE_ERROR`, `CONFIG_INVALID_VALUE`, `CONFIG_UNKNOWN_KEY`
- Include `path`, `value`, `expected` when available.

## Example (ms.toml)

```toml
[skill_paths]
global = ["~/.local/share/ms/skills"]
project = [".ms/skills"]
community = ["~/.local/share/ms/community"]

[layers]
priority = ["project", "global", "community"]
auto_detect = true

[disclosure]
default_level = "moderate"
token_budget = 800
auto_suggest = true
cooldown_seconds = 300

[search]
use_embeddings = true
embedding_backend = "hash"
embedding_dims = 384
bm25_weight = 0.5
semantic_weight = 0.5

[cass]
auto_detect = true
cass_path = null
session_pattern = "*.jsonl"

[cache]
enabled = true
max_size_mb = 100
ttl_seconds = 3600

[update]
auto_check = true
check_interval_hours = 24
channel = "stable"

[robot]
format = "json"
include_metadata = true
```

See full examples:
- `docs/examples/ms.toml`
- `docs/examples/project.toml`

## Implementation Notes

- Load defaults, then merge global, project, env, CLI flags in order.
- Keep a trace of origin per field (useful for `config show`).
- Use a single source of truth for schema (derive `serde` + validation).

## Testing

- Unit: parse/merge, env var mapping, error cases.
- Integration: `ms config show/get/set/reset`.
- Regression: precedence chain snapshots.

## Acceptance Criteria

- Configs load from all layers with correct precedence.
- Environment variables override file configs.
- CLI flags override everything.
- `ms config show` displays effective config.
- Invalid config produces helpful error messages.
