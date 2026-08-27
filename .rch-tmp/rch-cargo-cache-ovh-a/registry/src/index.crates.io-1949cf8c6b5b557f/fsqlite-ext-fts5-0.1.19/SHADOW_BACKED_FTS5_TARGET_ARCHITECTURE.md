# Shadow-Backed FTS5 Target Architecture

Bead: `bd-2nzo8.1.2`

Status: target architecture and invariant set for the first-class shadow-backed FTS5 backend.

This document turns the contract matrix into the concrete FrankenSQLite design target. It answers the questions that would otherwise create rework later: what the long-lived runtime objects are, what persists only in shadow tables, how savepoints and MVCC interact with FTS5 writes, how legacy materialized FTS5 tables are retired, and which transitional bridges are allowed versus forbidden.

This is not a generic architecture note. It is the implementation shape that later beads are expected to realize.

## Starting Point Being Replaced

Current code paths show exactly what is being retired:

- `crates/fsqlite-ext-fts5/src/lib.rs:1592-2005` implements `Fts5Table` as a mutable in-memory virtual table with `InvertedIndex`, `HashMap<i64, Vec<String>>`, and snapshot-based savepoint rollback.
- `crates/fsqlite-core/src/connection.rs:21958` handles `CREATE VIRTUAL TABLE` through the current module path.
- `crates/fsqlite-core/src/connection.rs:17898-17936` persists materialized live-vtab rows for the current backend.
- `crates/fsqlite-core/src/connection.rs:36103-36802` rebuilds materialized virtual-table instances during schema reload and has a special-case placeholder path for legacy `rootpage=0` virtual tables.
- `crates/fsqlite-core/src/connection.rs:6357-6590` coordinates live-vtab begin/sync/savepoint/release/rollback/commit behavior across transactional hooks.
- `crates/fsqlite-core/src/connection.rs:5012`, `5339`, and `5563` keep `concurrent_mode_default` on by default, and the e2e harness defaults in `crates/fsqlite-e2e/src/lib.rs:164`, `crates/fsqlite-e2e/src/fsqlite_executor.rs:91`, and `crates/fsqlite-e2e/src/fairness.rs:52` ratchet the same behavior.

The final design must replace the materialized/in-memory FTS5 persistence model without regressing those concurrency ratchets.

## Architectural Goals

The target backend must satisfy all of the following simultaneously:

1. One canonical persistent representation.
   The only steady-state persisted FTS5 representation is the stock-compatible virtual-table row plus shadow tables. No dual-primary backend.
2. Lightweight runtime handles.
   Opening a database must not require rebuilding a whole `HashMap`/`InvertedIndex` corpus in memory.
3. MVCC-native writes.
   FTS5 writes must remain page-level MVCC participants. No file-level write serialization, no single global FTS mutex, no rollback to SQLite-style writer exclusion.
4. First-class catalog semantics.
   The engine must treat `rootpage=0` FTS5 virtual tables and their shadow tables as normal, live engine objects rather than repair placeholders.
5. Exact user-visible parity.
   Create/open/query/DML/maintenance/integrity/tokenizer/locale/aux behavior must match the contract matrix closely enough for differential testing against stock SQLite.
6. Deliberate retirement of the legacy materialized path.
   Temporary migration bridges are allowed only if they have explicit removal criteria and do not remain as the main query/write path.

## Final Object Model

The final implementation should converge on the following conceptual objects.

## 1. `Fts5ModuleDescriptor`

Static, process-wide module metadata:

- module name (`fts5`)
- shadow-table ownership contract (the Rust equivalent of `xShadowName`)
- module risk / direct-only / schema-scope policy
- factory hooks for create/connect/open/maintenance helpers
- tokenizer and auxiliary-function registration surface

Purpose:

- lets the catalog, authorizer, DDL, and defensive-mode code ask module-level questions without opening a mutable runtime table,
- makes shadow ownership and policy part of the engine substrate instead of hiding it in FTS5-specific string heuristics.

Primary bead consumers:

- `bd-2nzo8.2.1`
- `bd-2nzo8.2.4`
- `bd-2nzo8.2.5`

## 2. `Fts5CatalogBinding`

Per-table catalog metadata reconstructed from `sqlite_schema` and the shadow-table catalog:

- virtual-table schema row (`rootpage=0`, SQL text, module args)
- canonical shadow-table names
- parsed config surface (`content`, `content_rowid`, `columnsize`, `detail`, `locale`, `tokendata`, prefix list, merge knobs, rank, secure-delete, insttoken)
- stable identifiers needed to open shadow-backed readers/writers

Purpose:

- decouples catalog identity from live mutable state,
- provides a single place where schema reload decides whether a table is:
  - native shadow-backed FTS5,
  - a legacy positive-rootpage materialized FTS5 table that must migrate or rebuild,
  - or an invalid/corrupt catalog state.

Primary bead consumers:

- `bd-2nzo8.2.2`
- `bd-2nzo8.4.1`
- `bd-2nzo8.6.3`

## 3. `Fts5ShadowStorage`

The canonical persistent storage facade over `%_config`, `%_content`, `%_docsize`, `%_data`, and `%_idx`.

It should be internally split, even if implementation details are shared:

- `ConfigStore`
- `ContentStore`
- `DocsizeStore`
- `SegmentStore`
- `IdxStore`

Purpose:

- keep storage responsibilities aligned with the upstream split between `fts5_storage.c` and `fts5_index.c`,
- make codec and corruption handling local and testable,
- prevent a monolithic "everything object" that becomes impossible to optimize or verify cleanly.

Primary bead consumers:

- `bd-2nzo8.3.1`
- `bd-2nzo8.3.2`
- `bd-2nzo8.3.3`
- `bd-2nzo8.3.4`

## 4. `Fts5ReadSnapshot`

An immutable reader view opened against the current transaction snapshot:

- schema/config snapshot
- lazy segment readers over `%_data` / `%_idx`
- optional bounded caches for structure records, segment pages, and docsize/content lookups
- no ownership of global mutable index state

Purpose:

- provides query execution with snapshot-correct reads,
- keeps reopen cheap,
- allows auxiliary functions and vocab/introspection flows to read from the same snapshot as the query.

Primary bead consumers:

- `bd-2nzo8.4.4`
- `bd-2nzo8.5.3`
- `bd-2nzo8.6.1`

## 5. `Fts5WriteBuffer`

Transaction-local pending state for a connection's writes to one FTS5 table:

- pending row changes
- pending docsize/totals deltas
- pending segment flush work
- savepoint-addressable undo/logical rollback state

This is explicitly not a second canonical backend. It is transient state that becomes durable only when published into the shadow tables through normal transaction machinery.

Purpose:

- lets writers batch and defer expensive segment/index publication work,
- avoids whole-table mutation in memory,
- preserves savepoint semantics without requiring a copy of the entire table.

Primary bead consumers:

- `bd-2nzo8.5.1`
- `bd-2nzo8.5.2`
- `bd-2nzo8.4.3`

## 6. `Fts5RuntimeHandle`

The per-connection live vtab instance should become a lightweight orchestrator over:

- `Fts5CatalogBinding`
- `Fts5ReadSnapshot`
- optional `Fts5WriteBuffer` if the transaction writes
- query/aux cursor factories

What it must not own in the final design:

- the canonical inverted index,
- a full in-memory copy of all documents,
- any global writer lock,
- a materialized positive-rootpage table as its source of truth.

Purpose:

- keeps the virtual-table trait surface compatible with the rest of the engine while moving persistence out of the in-memory table object.

## 7. `Fts5AuxContext`

Per-query or per-cursor state used by rank/highlight/snippet/insttoken/vocab flows:

- query term view
- iterator/snapshot handles
- column metadata
- ranking configuration

Purpose:

- prevents auxiliary functions from secretly depending on global mutable table state,
- keeps aux behavior tied to the same snapshot and tokenizer/config semantics as the query that invoked it.

## Read Path

The final read path should be:

1. Resolve `Fts5CatalogBinding` from schema/catalog state.
2. Open an `Fts5ReadSnapshot` from the active transaction snapshot.
3. Read `%_config` and structure metadata eagerly.
4. Read `%_data`, `%_idx`, `%_content`, and `%_docsize` lazily as queries actually demand them.
5. Build query cursors and aux contexts against the read snapshot.

Required properties:

- `MATCH` does not require full-table hydration.
- rank/highlight/snippet/vocab use the same snapshot.
- reopen cost is dominated by small metadata reads, not by rebuilding the whole index.

## Write Path

The final write path should be:

1. Parse DML or command-channel intent through the normal SQL surface.
2. Open or reuse the table's `Fts5WriteBuffer` for the current transaction.
3. Record logical row/config/maintenance intent in the write buffer.
4. On sync/commit publication, transform pending writes into shadow-table updates:
   - `%_content` / `%_docsize` row updates as required by content mode,
   - `%_data` / `%_idx` segment and structure updates,
   - `%_config` value updates when control commands mutate durable settings.
5. On rollback or savepoint rollback, discard or rewind only the pending FTS state associated with the affected scope.

Required properties:

- publication touches only the necessary pages and remains compatible with page-level MVCC conflict detection,
- there is no connection-global or process-global serialized writer section,
- write amplification is controlled by batching and merge policy rather than by accidental whole-index rewrites.

## Savepoint and MVCC Semantics

The current in-memory `Fts5Table` uses full snapshot copies for savepoints. That is not acceptable for the final backend because it scales with corpus size.

The final invariant set is:

1. Savepoints rewind pending FTS state, not the whole table.
2. Read snapshots remain immutable for the life of the statement/transaction snapshot.
3. Pending writes become visible according to the same transaction/savepoint rules as other engine writes.
4. MVCC conflicts are page conflicts on touched shadow pages, not global "another writer is in FTS" conflicts.
5. FTS5 never introduces a file-level or connection-global writer lock that defeats `BEGIN CONCURRENT`.

Operationally this means:

- `begin`, `sync_txn`, `savepoint`, `release`, `rollback_to`, `commit`, and `rollback` hooks remain real and important,
- but they operate on a small pending-state machine plus shadow-table publication, not on whole-table snapshots.

## Catalog and Lifecycle Semantics

The final lifecycle rules are:

1. `CREATE VIRTUAL TABLE ... USING fts5(...)`
   - creates a `rootpage=0` virtual-table catalog row,
   - creates the owned shadow tables,
   - writes canonical config/version metadata,
   - registers a shadow-backed runtime handle.
2. `OPEN` / schema reload
   - rebuilds `Fts5CatalogBinding` objects from catalog state,
   - binds runtime handles to shadow storage,
   - never treats valid stock `rootpage=0` FTS5 tables as placeholder-only artifacts.
3. `DROP` / `ALTER ... RENAME`
   - operate on the virtual table and all owned shadow tables as a single lifecycle unit.
4. Defensive mode / authorizer / trigger enforcement
   - consult module shadow-ownership metadata instead of hand-coded table-name heuristics.

## Legacy Materialized FTS5 Tables

Legacy positive-rootpage FTS5 tables are a temporary migration concern only.

Steady-state policy:

- they are not a permanent supported primary backend,
- they are not allowed to silently coexist forever as an alternate live query engine,
- they are either:
  - migrated in place to the canonical shadow-backed representation,
  - rebuilt into the canonical representation,
  - or rejected with a clear migration/rebuild path when the implementation is not yet available.

Temporary bridge policy:

- a short-lived bridge may exist during implementation to detect and convert legacy materialized FTS5 tables,
- but the bridge must be explicitly scoped to migration/rebuild,
- and the epic does not close until the old materialized path is retired as a normal create/open/query/write path.

## Concurrency Invariants

These invariants are non-negotiable:

1. `concurrent_mode_default` remains `true`.
2. FTS5 shadow-backed writes do not introduce SQLite-style serialized file locking.
3. No single FTS5 mutex or background worker becomes the hidden writer bottleneck.
4. Hot pages such as structure rows or `%_idx` pages are mitigated with batching, merge policy, and layout-aware design, not with broad locks.
5. Retry pressure from `BusySnapshot`, `WriteConflict`, or `SerializationFailure` is treated as an optimization signal, not an excuse to serialize all FTS writes.

## Explicit Anti-Patterns

The following designs are out of contract:

- Keeping the current materialized positive-rootpage backend as the main FTS5 implementation.
- Hydrating the entire FTS corpus into a `HashMap`/`InvertedIndex` on every open.
- Solving shadow-table support by routing stock FTS5 through rusqlite at runtime.
- Introducing a global writer section for FTS5 publication.
- Treating `%_data` and `%_idx` as opaque blobs that do not need exact codec ownership and tests.
- Building a sidecar/tantivy-style engine and calling it "FTS5 support".

## Staged Cutover Sequence

The intended implementation sequence is:

1. Contract and architecture lock.
   Land the contract matrix, this architecture document, and the proof-artifact contract.
2. Engine substrate.
   Add shadow ownership/risk metadata to the vtab substrate and make catalog/reload logic understand `rootpage=0` FTS5 plus owned shadow tables.
3. Storage codecs.
   Implement `%_config`, `%_content`, `%_docsize`, `%_data`, and `%_idx` codecs plus structure-record handling.
4. Shadow-backed open/query.
   Bind runtime handles to the canonical storage backend with lazy readers and snapshot-correct aux contexts.
5. Shadow-backed writes and maintenance.
   Route DML, command-channel writes, merge/optimize/rebuild/integrity, and tokenizer/locale/tokendata-sensitive behavior through the real backend.
6. Migration and legacy retirement.
   Rebuild or migrate old positive-rootpage materialized FTS5 tables, remove placeholder logic, and stop treating the legacy path as live backend support.
7. Performance hardening and downstream cutover.
   Use the proof/harness artifacts to optimize the real backend and validate downstream repos.

## What Later Beads Must Be Able To Assume

After this architecture is accepted, later beads may assume:

- there is exactly one final persistent backend,
- rootpage=0 is first-class,
- shadow tables are module-owned engine objects,
- FTS5 savepoints are incremental/pending-state based rather than whole-table snapshot copies,
- query and aux behavior run against lazy, snapshot-correct readers,
- migration exists to retire legacy materialized FTS5 instead of preserving it forever.

## Bottom Line

The final FrankenSQLite FTS5 backend is:

- stock-compatible in catalog and shadow-table semantics,
- native to FrankenSQLite's MVCC and concurrent-writer design,
- segment-native and lazy on reopen,
- migration-oriented with respect to the old materialized backend,
- and explicit about the engine substrate changes required to make shadow-backed FTS5 a first-class storage subsystem.
