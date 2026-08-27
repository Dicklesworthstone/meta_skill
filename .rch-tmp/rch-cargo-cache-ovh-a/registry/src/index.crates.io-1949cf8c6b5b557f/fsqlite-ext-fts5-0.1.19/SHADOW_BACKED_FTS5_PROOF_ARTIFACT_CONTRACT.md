# Shadow-Backed FTS5 Proof Artifact Contract

Bead: `bd-2nzo8.1.4`

Status: verification contract for the full shadow-backed FTS5 program.

This document defines what counts as acceptable proof for every later bead in the shadow-backed FTS5 epic. It exists to prevent low-signal "tests passed locally" evidence and to ensure that unit coverage, real-backend integration, replayable e2e scripts, structured logging, manifests, and performance artifacts all fit one explicit contract.

This bead is upstream of the harness, observability, differential, migration, and downstream-cutover work. Later beads should reference this document instead of inventing their own proof format.

## Existing Project Conventions To Reuse

This proof contract is not starting from zero. It should build directly on the existing repo-level evidence systems:

- Unified E2E log schema:
  - `crates/fsqlite-harness/src/e2e_log_schema.rs`
  - current schema version: `1.0.0`
  - required event fields: `run_id`, `timestamp`, `phase`, `event_type`
  - replayability keys already named there: `scenario_id`, `seed`, `phase`, `context.invariant_ids`, `context.artifact_paths`
- Validation manifest:
  - `crates/fsqlite-harness/src/validation_manifest.rs`
  - current schema version: `1.0.0`
  - already includes `GateRecord`, `ReplayContract`, and artifact-bundle integrity concepts
- Replay harness conventions:
  - `crates/fsqlite-harness/src/replay_harness.rs`
  - deterministic replay, summary artifacts, and regime/drift classification
- E2E JSONL logging pattern:
  - `crates/fsqlite-e2e/src/logging.rs`
  - dual-output human + JSONL logging with structured fields and per-run log files
- Existing artifact-emitting shell scripts:
  - examples include `scripts/verify_bd_1r0ha_3_concurrent_writer_e2e.sh`, `scripts/bd_2g5_1_txn_slots_e2e.sh`, `scripts/verify_e2_2_fused_entry.sh`, and `scripts/verify_bd_2yqp6_1_1_supported_surface_matrix.sh`

The FTS5 program must reuse these patterns unless it has a compelling reason not to. The point is convergence, not a bespoke subsystem-specific evidence format.

## Proof Levels

Every later FTS5 bead must map its proof into one or more of these levels:

1. Nearby unit tests
   - close to the code they validate
   - deterministic and cheap
   - precise enough to isolate codec/parser/state-machine bugs
2. Cross-component integration tests
   - exercise real FrankenSQLite engine paths across crates
   - validate catalog/reload/storage/query interactions
3. Replayable no-mock e2e scripts
   - exercise real backends and real artifact generation
   - emit JSONL logs, manifests, replay commands, and artifact hashes
4. Differential oracle runs
   - compare FrankenSQLite against stock SQLite where parity is claimed
5. Fuzz/property/corruption suites
   - target the oracle-problem areas and corruption surfaces
6. Performance/observability artifacts
   - benchmark and profile data with enough metadata for later regression triage

No later bead may close on unit tests alone if the behavior is user-visible, storage-visible, or concurrency-sensitive.

## Coverage Matrix

The minimum required proof shape for the shadow-backed FTS5 work is:

| Area | Nearby unit tests | Cross-component integration | Replayable no-mock e2e | Differential oracle | Fuzz/property/corruption | Perf/observability |
| --- | --- | --- | --- | --- | --- | --- |
| Vtab substrate and shadow ownership | Required | Required | Optional | Optional | Optional | Optional |
| Catalog/rootpage=0/reload semantics | Required | Required | Required | Required | Optional | Optional |
| `%_config` codec and option persistence | Required | Required | Optional | Required | Required | Optional |
| `%_content` / `%_docsize` behavior | Required | Required | Required | Required | Required | Optional |
| `%_data` / `%_idx` codecs and structure records | Required | Required | Optional | Required | Required | Optional |
| Query semantics and aux functions | Required | Required | Required | Required | Optional | Required for hot paths |
| Tokenizer / locale / tokendata behavior | Required | Required | Required | Required | Required | Optional |
| DML / maintenance / integrity | Required | Required | Required | Required | Required | Required for expensive flows |
| MVCC/savepoint behavior | Required | Required | Required | Optional | Required | Required |
| Migration / legacy-materialized retirement | Optional | Required | Required | Optional | Required for corruption/rebuild paths | Required |
| Downstream cutover | Optional | Optional | Required | Optional | Optional | Required |

Interpretation:

- Required means the bead is not allowed to close without that proof level.
- Optional means the proof level may still be helpful, but is not mandatory for every bead in that area.

## Mandatory Scenario Buckets

Across the whole epic, the proof corpus must include all of these scenario buckets:

1. Catalog and lifecycle
   - create rootpage=0 FTS5 table
   - reopen and reload stock SQLite FTS5 tables
   - rename and drop virtual table plus shadow tables
   - reject invalid catalog combinations
2. Defensive restrictions
   - direct writes to shadow tables
   - triggers on virtual tables
   - triggers on shadow tables
   - defensive-mode restrictions and authorizer decisions
3. Content modes
   - stored
   - external-content
   - contentless
   - contentless-delete
   - `columnsize=0`
4. Config and command channel
   - `pgsz`
   - `automerge`
   - `usermerge`
   - `crisismerge`
   - `deletemerge`
   - `rank`
   - `secure-delete`
   - `insttoken`
   - command-channel `delete-all`, `rebuild`, `optimize`, `merge`, `integrity-check`
5. Query and aux behavior
   - phrase
   - prefix
   - NEAR
   - column filter
   - caret
   - rowid/rank/highlight/snippet
   - `fts5vocab`-relevant introspection
6. Tokenizer and locale
   - built-in tokenizer matrix
   - custom tokenizer registration/find flows
   - `fts5_locale()` valid and invalid usage
   - locale-preserving updates
   - `tokendata=1` query and integrity behavior
7. MVCC and savepoints
   - concurrent writers on independent pages
   - conflicting writers on shared shadow pages
   - savepoint rollback of pending FTS state
   - commit/rollback/sync behavior
8. Migration and corruption
   - legacy positive-rootpage materialized FTS5 migration or rebuild
   - malformed structure record
   - malformed config rows
   - shadow-table count mismatches
   - rebuild after corruption
9. Downstream cutover
   - real consumer database open/query
   - real migration/rebuild path
   - artifact bundle and failure triage pack generation

## Structured Logging Contract

The FTS5 program should inherit the repo-wide E2E schema and extend it with FTS5-specific context fields rather than inventing a new base schema.

## Base event contract

Every replayable integration/e2e script and every operator-grade validation driver run must emit JSONL events conforming to the repo-wide schema from `crates/fsqlite-harness/src/e2e_log_schema.rs`.

Minimum required top-level fields:

- `run_id`
- `timestamp`
- `phase`
- `event_type`

Strongly required for FTS5 even though some are "recommended" in the global schema:

- `scenario_id`
- `seed`
- `backend`

For FTS5 work, `artifact_hash` becomes required whenever the event announces a durable artifact.

## Required FTS5-specific context keys

Every FTS5 run must populate the following context keys when applicable:

- `context.bead_id`
- `context.fixture_id`
- `context.db_shape`
- `context.content_mode`
- `context.content_rowid_mode`
- `context.tokenizer`
- `context.prefix_config`
- `context.detail_mode`
- `context.columnsize_mode`
- `context.locale_enabled`
- `context.tokendata_enabled`
- `context.command_name`
- `context.command_args`
- `context.invariant_ids`
- `context.artifact_paths`
- `context.diff_oracle`
- `context.rootpage_mode`
- `context.shadow_table_names`
- `context.segment_generation`
- `context.structure_record_version`
- `context.page_conflicts`
- `context.busy_class`
- `context.migration_mode`
- `context.downstream_repo`

Additional required numeric/timing context when applicable:

- `context.elapsed_ms`
- `context.query_count`
- `context.rows_examined`
- `context.rows_returned`
- `context.page_reads`
- `context.page_writes`
- `context.segment_reads`
- `context.segment_writes`
- `context.pending_bytes`
- `context.rss_bytes`

## Required event types per run

Each replayable script/driver run must emit, at minimum:

1. `start`
2. `info` for fixture and backend setup
3. `info` or `artifact_generated` when the manifest is written
4. `first_divergence` on the first behavioral mismatch, if any
5. `pass` or `fail`
6. `artifact_generated` for the final summary or report artifact

Each run must also emit at least one event for each of these phases:

- `setup`
- `execute`
- `validate`
- `report`

`teardown` is required when the run creates or mutates durable files, temp databases, or downstream fixture directories.

## Machine-readable event example

```json
{
  "run_id": "bd-2nzo8.6.1-20260408T235500Z-4242",
  "timestamp": "2026-04-08T23:55:00Z",
  "phase": "validate",
  "event_type": "artifact_generated",
  "scenario_id": "FTS5-ROOTPAGE-OPEN-001",
  "seed": 1729,
  "backend": "frankensqlite",
  "artifact_hash": "sha256:...",
  "context": {
    "bead_id": "bd-2nzo8.6.1",
    "fixture_id": "stock_rootpage0_external_content_small",
    "db_shape": "rootpage0+external_content+prefix+detail_full",
    "content_mode": "external_content",
    "tokenizer": "porter",
    "locale_enabled": "false",
    "tokendata_enabled": "false",
    "diff_oracle": "rusqlite-sqlite-3.x",
    "rootpage_mode": "zero",
    "shadow_table_names": "docs_fts_config,docs_fts_content,docs_fts_docsize,docs_fts_data,docs_fts_idx",
    "artifact_paths": "artifacts/bd-2nzo8.6.1/<run_id>/manifest.json",
    "elapsed_ms": "47"
  }
}
```

## Artifact Bundle Contract

Replayable integration/e2e and benchmark runs must produce an artifact bundle rooted at:

- `artifacts/<bead_id>/<run_id>/`

Required files:

- `events.jsonl`
- `manifest.json`
- `summary.json`
- `artifact_hashes.txt`
- `replay.env`

Conditionally required files:

- `diff_report.json`
- `benchmark_summary.json`
- `memory_io_summary.json`
- `first_divergence.json`
- `proof_note.md`
- `migration_report.json`
- `cutover_report.json`
- `stdout.txt`
- `stderr.txt`

## Required `manifest.json` fields

Each artifact bundle manifest must include:

- `schema_version`
- `bead_id`
- `run_id`
- `trace_id`
- `scenario_id`
- `seed`
- `backend`
- `oracle_backend` when differential comparison is involved
- `fixture_id`
- `fixture_fingerprint`
- `replay_command`
- `artifact_paths`
- `artifact_hashes`
- `result`
- `started_at`
- `finished_at`

Required FTS5-specific manifest fields:

- `db_shape`
- `content_mode`
- `tokenizer`
- `detail_mode`
- `columnsize_mode`
- `locale_enabled`
- `tokendata_enabled`
- `rootpage_mode`
- `shadow_table_layout`
- `command_surface`
- `mvcc_mode`
- `migration_mode`

## Machine-readable manifest example

```json
{
  "schema_version": "1.0.0",
  "bead_id": "bd-2nzo8.5.3",
  "run_id": "bd-2nzo8.5.3-20260408T235500Z-4242",
  "trace_id": "fts5-shadow-20260408-01",
  "scenario_id": "FTS5-REOPEN-MEMORY-003",
  "seed": 1729,
  "backend": "frankensqlite",
  "oracle_backend": "rusqlite-sqlite-3.x",
  "fixture_id": "large_prefix_external_content_reopen",
  "fixture_fingerprint": "blake3:...",
  "db_shape": "rootpage0+external_content+prefix+large_docs",
  "content_mode": "external_content",
  "tokenizer": "unicode61",
  "detail_mode": "full",
  "columnsize_mode": "table",
  "locale_enabled": false,
  "tokendata_enabled": false,
  "rootpage_mode": "zero",
  "shadow_table_layout": "stock_fts5_v1",
  "command_surface": ["match", "optimize", "integrity-check"],
  "mvcc_mode": "begin_concurrent",
  "migration_mode": "native_shadow",
  "replay_command": "RUN_ID=... TRACE_ID=... SCENARIO_ID=... SEED=1729 ./scripts/verify_bd_2nzo8_5_3_reopen_memory.sh",
  "artifact_paths": {
    "events_jsonl": "artifacts/bd-2nzo8.5.3/<run_id>/events.jsonl",
    "summary_json": "artifacts/bd-2nzo8.5.3/<run_id>/summary.json"
  },
  "artifact_hashes": {
    "events_jsonl": "sha256:...",
    "summary_json": "sha256:..."
  },
  "result": "pass",
  "started_at": "2026-04-08T23:55:00Z",
  "finished_at": "2026-04-08T23:55:47Z"
}
```

## Replay Contract

Every replayable script or driver run must produce deterministic replay material:

- `replay_command` in `manifest.json`
- `replay.env` with exported `RUN_ID`, `TRACE_ID`, `SCENARIO_ID`, `SEED`, and any artifact path overrides
- a stable `fixture_id`
- a stable `fixture_fingerprint`

Deterministic replay means:

1. The same fixture plus seed plus backend selection reproduces the same logical scenario.
2. Run-specific values such as timestamps do not change the semantic result or the normalized golden output.
3. Corruption, migration, and downstream-cutover scenarios must name the exact source fixture or source database snapshot used.

Required environment variables for replayable runs:

- `RUN_ID`
- `TRACE_ID`
- `SCENARIO_ID`
- `SEED`

Conditionally required:

- `FSQLITE_FTS5_ARTIFACT_DIR`
- `FSQLITE_FTS5_FIXTURE_ROOT`
- `FSQLITE_FTS5_ORACLE_DB`
- `FSQLITE_FTS5_DOWNSTREAM_REPO`

## Golden Output Contract

Goldens should freeze semantic outputs, not incidental run noise.

Required normalization rules:

- scrub or normalize wall-clock timestamps
- scrub filesystem-specific temp paths
- scrub run-specific IDs that are not semantically meaningful
- sort JSON object keys
- normalize path separators to `/`
- normalize floating-point benchmark summaries to declared precision
- normalize query result ordering only when the SQL contract does not define order

Never scrub:

- rootpage mode
- content mode
- tokenizer/config surface
- shadow-table names
- structure-record version
- command-channel names
- first divergence details
- performance measurements in benchmark summaries

Required golden classes:

- result goldens
- differential goldens
- manifest goldens
- log-schema compliance goldens
- benchmark summary goldens where thresholds depend on artifact shape

## Unit and Integration Placement Rules

Placement matters. The contract is:

- codec/unit tests live next to codec code
- parser/query/tokenizer tests live next to parser/query/tokenizer code
- catalog/reload/integration tests live in the crate that owns the integration point
- cross-component real-engine tests live in `crates/fsqlite-e2e/`, `crates/fsqlite-harness/`, or workspace `tests/` only when they truly span components

Expected placement by later workstream:

- `bd-2nzo8.2.*`:
  - nearby tests in `fsqlite-func` and `fsqlite-core`
- `bd-2nzo8.3.*`:
  - nearby tests in `fsqlite-ext-fts5`
- `bd-2nzo8.4.*`:
  - integration tests in `fsqlite-core`, `fsqlite-e2e`, and differential harnesses
- `bd-2nzo8.5.*`:
  - perf/observability suites in `fsqlite-e2e`, `fsqlite-harness`, and `benches/`
- `bd-2nzo8.6.*`:
  - validation, corruption, migration, and cutover drivers in `scripts/` plus harness coverage

## No-Mock Rule

The FTS5 critical path is:

- create/open/reload
- query/aux execution
- DML and command-channel writes
- integrity and rebuild
- migration and downstream cutover

For those paths:

- unit tests may use small in-process fixtures,
- but integration/e2e proof must use real FrankenSQLite codepaths and, when parity is claimed, real stock SQLite oracle runs,
- no mock-only substitute is acceptable for epic closure.

## Performance Artifact Contract

Any performance or observability bead must emit machine-readable artifacts with:

- scenario identifiers
- corpus identity
- backend identity
- seed
- timing summaries (`p50`, `p95`, `p99`, aggregate throughput where relevant)
- memory summaries (`rss_bytes`, allocation counts if available)
- IO summaries (`page_reads`, `page_writes`, `segment_reads`, `segment_writes`)
- conflict summaries (`busy_snapshot_count`, `write_conflict_count`, `serialization_failure_count`)
- merge debt or segment-count summaries where applicable

Benchmark runs must capture both:

- the measured values, and
- the exact replay command needed to regenerate them.

## Corruption, Migration, and Downstream Cutover Proof

These scenarios are special and must always include:

- the source fixture or source DB fingerprint
- the replay command
- explicit before/after manifests
- explicit pass/fail summaries
- first-failure diagnostics if the run fails

Migration and downstream cutover artifacts must additionally include:

- source backend/state classification
- chosen migration path (`in_place`, `rebuild`, `reject_with_guidance`)
- resulting shadow-table layout summary
- user-visible behavior verification summary

## Closure Rule For Later Beads

A later FTS5 bead may close only when its evidence bundle includes:

1. the required proof levels from the coverage matrix,
2. structured logs conforming to the repo-wide schema plus the FTS5-specific fields named here,
3. a manifest with replay metadata and artifact hashes,
4. goldens normalized according to this contract where goldens are used,
5. explicit pass/fail summaries and first-divergence details where applicable.

If any of those are missing, the bead is not done.

## Bottom Line

The user asked for comprehensive unit tests and e2e scripts with detailed logging. This document makes that a binding engineering contract:

- nearby unit tests for local correctness,
- real-backend integration and e2e runs for system behavior,
- deterministic replay for every important scenario,
- machine-readable logs and manifests,
- goldens and hashes for regression detection,
- and artifact bundles strong enough to support optimization, corruption triage, migration, and real downstream cutover.
