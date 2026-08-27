# Shadow-Backed FTS5 Contract Matrix

Bead: `bd-2nzo8.1.1`

Status: implementation input for the shadow-backed FTS5 epic. This document is the durable contract for later engine, storage, SQL-surface, and verification beads. The goal is to make the required stock-SQLite behavior explicit enough that later work does not need to re-open the vendored C sources just to answer "what are we actually trying to match?".

## Why This Exists

FrankenSQLite currently has a working FTS5 feature surface, but it is not the same storage model as stock SQLite FTS5:

- `crates/fsqlite-ext-fts5/src/lib.rs` implements an in-memory `Fts5Table` backed by `InvertedIndex` and `HashMap` state.
- `crates/fsqlite-core/src/connection.rs` currently persists and reloads FTS5 through a materialized/live-vtab path, not through the stock SQLite `%_config`, `%_content`, `%_docsize`, `%_data`, and `%_idx` shadow tables.
- `crates/fsqlite-func/src/vtab.rs` exposes a generic virtual-table trait, but it does not yet have an `xShadowName`-style contract for module-owned shadow tables.

The shadow-backed epic exists to replace that architectural mismatch with one canonical backend that can:

- open stock SQLite FTS5 databases,
- create stock-compatible rootpage=0 virtual-table catalog rows,
- use shadow tables as the live storage backend,
- preserve FrankenSQLite's page-level MVCC and concurrent-writer defaults.

## How To Use This Matrix

Each section below captures one feature area in five layers:

1. What stock SQLite does.
2. Where that behavior lives in the vendored source.
3. Whether parity is mandatory, staged-but-required, or an explicit non-goal for the first full cut.
4. What FrankenSQLite does today.
5. Which later beads depend on this behavior.

Compatibility tiers:

- Mandatory parity: the final epic cannot close without this behavior.
- Staged delivery: can land in phases, but still must be implemented before epic closure.
- Explicit non-goal: consciously deferred from the first full shadow-backed backend. This document currently keeps non-goals extremely narrow on purpose.

## Current FrankenSQLite Baseline

These current-state facts are important because the later beads are not starting from zero:

- `crates/fsqlite-ext-fts5/src/lib.rs:1592` defines `Fts5Table` as an in-memory virtual table with `InvertedIndex` and per-document `HashMap` storage.
- `crates/fsqlite-ext-fts5/src/lib.rs:1800` wires that in-memory table into the generic `VirtualTable` trait.
- `crates/fsqlite-core/src/connection.rs:6726` evaluates `MATCH` through the live in-memory `Fts5Table`.
- `crates/fsqlite-core/src/connection.rs:78357` has a reload test proving stock `rootpage=0` FTS5 tables are currently only held strongly enough to drop and rebuild.
- `crates/fsqlite-core/src/connection.rs:78446` and `crates/fsqlite-core/src/connection.rs:78504` have reload tests for the current materialized positive-rootpage FTS5 path.
- `crates/fsqlite-func/src/vtab.rs:250` defines the `VirtualTable` trait, but there is no shadow-table ownership hook analogous to SQLite's `xShadowName`.

That means later work must not "add shadow tables" next to the current backend. It must replace the current primary persistence model with shadow-backed storage and keep the generic vtab substrate honest about module-owned tables.

## Hard Requirements Summary

The following are hard requirements for the final backend:

- Root virtual-table rows for FTS5 must behave like stock SQLite catalog rows with `rootpage=0`.
- The engine must understand module-owned shadow tables and protect them with SQLite-like defensive restrictions.
- The canonical persistent state must live in `%_config`, `%_content`, `%_docsize`, `%_data`, and `%_idx`.
- Stored, external-content, contentless, and contentless-delete modes must all be modeled explicitly.
- Maintenance commands and config control writes must operate through the FTS5 command channel, not through ad-hoc FrankenSQLite-only entrypoints.
- Integrity, rebuild, optimize, merge, rank configuration, tokenizer registration, locale handling, `tokendata`, and auxiliary functions must all have first-class parity coverage.
- Differential tests must validate the user-visible behavior and the shadow-table bytes/layout expectations that matter for compatibility.

## Narrow Explicit Non-Goals

The first full shadow-backed backend still does not need to do the following:

- Preserve FrankenSQLite's current positive-rootpage materialized FTS5 format as a compatibility mode. Migration/rebuild tooling may consume it, but it is not a permanent backend.
- Vendor or invoke SQLite C at runtime. The vendored C remains a behavioral specification and a differential oracle only.
- Support partial compatibility where `MATCH` works but maintenance/integrity/control-channel behavior is silently different. That is explicitly forbidden.

## Matrix A: Module, Catalog, and Shadow-Table Ownership

| Feature area | Stock SQLite contract | Vendored source anchors | Tier | FrankenSQLite today | Downstream beads |
| --- | --- | --- | --- | --- | --- |
| Virtual-table API advertises shadow-table ownership | The module API includes `xShadowName(const char*)`, allowing the core to recognize module-owned shadow tables by suffix/name. | `legacy_sqlite_code/sqlite/src/sqlite.h.in:7657-7692`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_main.c:3684-3759` | Mandatory parity | `crates/fsqlite-func/src/vtab.rs:250-343` has no shadow-name hook. | `bd-2nzo8.2.1`, `bd-2nzo8.2.4` |
| FTS5 shadow-table names are exact and closed | FTS5 claims `config`, `content`, `data`, `docsize`, and `idx` as shadow-table suffixes. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_main.c:3684-3691` | Mandatory parity | No engine-level recognition of these names today. | `bd-2nzo8.2.1`, `bd-2nzo8.3.1`, `bd-2nzo8.3.2`, `bd-2nzo8.3.4` |
| Catalog row for the virtual table uses `rootpage=0` | Virtual tables are represented in `sqlite_master/sqlite_schema` with `rootpage=0`; the actual state lives elsewhere. | `legacy_sqlite_code/sqlite/src/vtab.c:490`, `legacy_sqlite_code/sqlite/src/build.c:2659-2670` | Mandatory parity | Current primary FTS5 create path persists a positive-rootpage materialized table; reload tests confirm rootpage-0 support is placeholder-grade only. | `bd-2nzo8.2.2`, `bd-2nzo8.4.1`, `bd-2nzo8.6.1` |
| Core can detect whether a table name is a shadow table of a vtab | The core uses module metadata to answer "is this shadow table owned by that virtual table?" and can mark tables with `TF_Shadow`. | `legacy_sqlite_code/sqlite/src/build.c:2513-2582`, `legacy_sqlite_code/sqlite/src/build.c:2652-2653` | Mandatory parity | No equivalent catalog typing or marking exists in the Rust vtab/catalog substrate. | `bd-2nzo8.2.1`, `bd-2nzo8.2.2`, `bd-2nzo8.2.4` |
| Risk and schema-scope flags belong to the module surface | SQLite tracks vtab risk flags such as `SQLITE_VTAB_INNOCUOUS`, `SQLITE_VTAB_DIRECTONLY`, and `SQLITE_VTAB_USES_ALL_SCHEMAS`. | `legacy_sqlite_code/sqlite/src/sqlite.h.in:10223-10255`, `legacy_sqlite_code/sqlite/src/vtab.c` risk paths | Staged delivery | FrankenSQLite's `VirtualTable` trait currently has no equivalent policy surface. | `bd-2nzo8.2.1`, `bd-2nzo8.2.4` |

Notes:

- `xShadowName` is not optional plumbing. It is the contract that lets the core recognize shadow tables in schema logic, DDL restrictions, and defensive mode.
- Rootpage=0 is not a cosmetic schema detail. It is the catalog signal that the virtual table is not backed by a normal b-tree root page.

## Matrix B: Defensive Restrictions, DDL, DML, and Trigger Rules

| Feature area | Stock SQLite contract | Vendored source anchors | Tier | FrankenSQLite today | Downstream beads |
| --- | --- | --- | --- | --- | --- |
| Shadow tables become read-only in defensive mode | Shadow tables may not be written directly when shadow tables are read-only. | `legacy_sqlite_code/sqlite/src/build.c:3455-3477`, `legacy_sqlite_code/sqlite/src/delete.c:73-120` | Mandatory parity | No shadow-table-aware defensive gate exists because shadow tables are not first-class yet. | `bd-2nzo8.2.4`, `bd-2nzo8.4.6`, `bd-2nzo8.6.1` |
| Trigger creation on virtual tables is forbidden | SQLite rejects `CREATE TRIGGER` on virtual tables. | `legacy_sqlite_code/sqlite/src/trigger.c:186` | Mandatory parity | Current materialized/live-vtab path needs to stay aligned once rootpage=0 FTS5 becomes real. | `bd-2nzo8.2.4`, `bd-2nzo8.4.6` |
| Trigger creation on shadow tables is forbidden when shadow tables are read-only | SQLite rejects triggers on shadow tables and rejects new triggers that write to them in defensive mode. | `legacy_sqlite_code/sqlite/src/trigger.c:189-190`, `legacy_sqlite_code/sqlite/src/trigger.c:367-377` | Mandatory parity | No direct shadow-table trigger policy exists yet. | `bd-2nzo8.2.4`, `bd-2nzo8.4.6`, `bd-2nzo8.6.1` |
| Rename/drop lifecycle respects shadow ownership | Rename/drop flows operate on the virtual table and its owned shadow tables as a unit. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_storage.c:247-296`, `legacy_sqlite_code/sqlite/src/build.c:2536-2558` | Mandatory parity | Current rootpage-0 reopen tests only guarantee "drop and rebuild is possible". | `bd-2nzo8.4.1`, `bd-2nzo8.4.6`, `bd-2nzo8.6.1` |
| Users do not treat shadow tables as ordinary writable row tables | The command surface is via the virtual table, not ad-hoc direct mutation of `%_data`/`%_idx`/friends. | `legacy_sqlite_code/sqlite/src/delete.c:73-120`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_main.c` module entrypoints | Mandatory parity | Today there is no canonical shadow-table path at all. | `bd-2nzo8.2.4`, `bd-2nzo8.4.3`, `bd-2nzo8.4.7` |

## Matrix C: Content Modes and Config Directives

| Feature area | Stock SQLite contract | Vendored source anchors | Tier | FrankenSQLite today | Downstream beads |
| --- | --- | --- | --- | --- | --- |
| Stored / external-content / contentless modes are explicit first-class modes | FTS5 config distinguishes stored content, external content, and contentless configurations, including `content=` and `content_rowid=` behavior. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_config.c`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_storage.c:956-1104` | Mandatory parity | Current crate models `Stored` and `Contentless`; external-content semantics are not the primary live backend. | `bd-2nzo8.3.1`, `bd-2nzo8.3.2`, `bd-2nzo8.4.1`, `bd-2nzo8.4.3`, `bd-2nzo8.6.1` |
| `contentless_delete=1` has strict validity rules | It is only valid for contentless tables and is incompatible with `columnsize=0`. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_config.c:354-366`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_config.c:647-664` | Mandatory parity | Current crate parses `contentless_delete`, but not in the full stock storage/layout model. | `bd-2nzo8.3.1`, `bd-2nzo8.4.3`, `bd-2nzo8.6.1` |
| `contentless_unindexed=1` is only valid for contentless tables | SQLite validates the mode combination. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_config.c:364-366`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_config.c:669-677` | Mandatory parity | Not yet modeled as part of a shadow-backed config codec. | `bd-2nzo8.3.1`, `bd-2nzo8.6.1` |
| `columnsize`, `detail`, `locale`, and `tokendata` are schema-time mode bits with downstream storage/query consequences | These settings alter `%_docsize`, retokenization, integrity logic, and iterator semantics. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_config.c:384-420`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_storage.c:1216-1336`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c:6309-6428`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c:7407-7424` | Mandatory parity | Current crate accepts some directives but documents that several are compatibility placeholders rather than real storage-backed semantics. | `bd-2nzo8.3.1`, `bd-2nzo8.3.3`, `bd-2nzo8.4.4`, `bd-2nzo8.4.8`, `bd-2nzo8.6.1`, `bd-2nzo8.6.2` |
| Runtime config keys are persisted and interpreted by the backend | Keys include `pgsz`, `automerge`, `usermerge`, `crisismerge`, `deletemerge`, `rank`, `secure-delete`, and `insttoken`. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_config.c:928-1031` | Mandatory parity | Current crate has partial control-command parsing, but not canonical shadow-backed persistence. | `bd-2nzo8.3.1`, `bd-2nzo8.4.7`, `bd-2nzo8.6.1` |

Notes:

- `locale` and `tokendata` are not fringe features. They affect stored bytes, query interpretation, and integrity checks.
- `contentless_delete=1` specifically forces V2 structure-record semantics in `%_data`.

## Matrix D: Shadow Tables and Persistent Storage Responsibilities

| Shadow table / storage area | Stock SQLite contract | Vendored source anchors | Tier | FrankenSQLite today | Downstream beads |
| --- | --- | --- | --- | --- | --- |
| `%_config` | Stores durable table metadata and runtime config values, including versioning and control settings. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_storage.c:411`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_storage.c:1503-1514`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_config.c:928-1031` | Mandatory parity | No exact `%_config` codec yet. | `bd-2nzo8.3.1` |
| `%_content` | Stores canonical content rows when the table is not contentless and not external-content-only. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_storage.c:956-1033`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_storage.c:1327-1336` | Mandatory parity | Current primary backend stores document values in memory/materialized rows instead of stock shadow-table layout. | `bd-2nzo8.3.2`, `bd-2nzo8.4.3` |
| `%_docsize` | Stores per-row token-count blobs unless `columnsize=0`; participates in totals and integrity checks. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_storage.c:77-112`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_storage.c:657-699`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_storage.c:1412-1465` | Mandatory parity | No exact `%_docsize` representation yet. | `bd-2nzo8.3.2`, `bd-2nzo8.6.1` |
| `%_data` | Holds structure record, averages, segment leaves/internal pages, and maintenance metadata. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c:269-292`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c:1102-1435` | Mandatory parity | No stock `%_data` segment backend exists today. | `bd-2nzo8.3.3`, `bd-2nzo8.3.4`, `bd-2nzo8.5.2`, `bd-2nzo8.6.1` |
| `%_idx` | Stores segment-index rows used to accelerate segment/page lookups and merge work. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c`, especially segment/dlidx maintenance paths around `4202`, `4550`, `5586` | Mandatory parity | No stock `%_idx` codec/backend exists today. | `bd-2nzo8.3.4`, `bd-2nzo8.5.2`, `bd-2nzo8.6.1` |

Storage split of responsibility:

- `fts5_storage.c` owns row-content, docsize, config, rebuild/delete-all/open/rename/integrity orchestration.
- `fts5_index.c` owns the segment engine, structure records, `%_data`, `%_idx`, iterators, merge behavior, and low-level integrity checks.

That split matters for the Rust port. Later implementation should not collapse everything into one monolithic "fts5 backend" type unless the same conceptual ownership is preserved.

## Matrix E: Structure Record, Segment Format, and Low-Level Index Semantics

| Feature area | Stock SQLite contract | Vendored source anchors | Tier | FrankenSQLite today | Downstream beads |
| --- | --- | --- | --- | --- | --- |
| Structure record has V1 and V2 formats | V2 starts with `FTS5_STRUCTURE_V2` and carries additional state required for features such as `contentless_delete=1`. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c:60-95`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c:1126-1140`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c:1435` | Mandatory parity | No exact structure-record codec today. | `bd-2nzo8.3.3`, `bd-2nzo8.3.4`, `bd-2nzo8.6.1` |
| `%_data` rowid layout is meaningful | Fixed rowids exist for averages and structure records; segment and dlidx rows are encoded via rowid formulas. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c:269-292` | Mandatory parity | No shadow-backed `%_data` layout today. | `bd-2nzo8.3.3`, `bd-2nzo8.3.4`, `bd-2nzo8.6.1` |
| dlidx is part of the real on-disk format | dlidx pages are added when sparse/large enough and are part of fast segment navigation. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c:49`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c:1766`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c:4202`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c:9012-9019` | Mandatory parity | Not modeled today. | `bd-2nzo8.3.4`, `bd-2nzo8.5.2`, `bd-2nzo8.6.1` |
| Secure-delete changes how postings are removed, not just a logical flag | Secure delete ensures removed terms/positions leave no stale searchable traces and interacts with merge/segment maintenance. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c:3103`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c:5099-5128`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c:5528`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_config.c:1019-1029` | Mandatory parity | Current in-memory backend has a simplified delete model. | `bd-2nzo8.3.4`, `bd-2nzo8.4.3`, `bd-2nzo8.6.1`, `bd-2nzo8.6.2` |
| Merge policy is part of correctness and performance behavior | `automerge`, `usermerge`, `crisismerge`, and `deletemerge` alter merge scheduling and segment evolution. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_config.c:952-1001`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c:4914-4956` | Staged delivery | No stock merge-policy backend today. | `bd-2nzo8.3.4`, `bd-2nzo8.5.2`, `bd-2nzo8.5.4`, `bd-2nzo8.6.1` |

Implementation consequence:

- A lazy segment-reader architecture is compatible with this contract.
- A "rehydrate the whole inverted index into a `HashMap` on open" design is not compatible with the intended performance shape and does not honor the segment-native storage contract.

## Matrix F: DML Semantics and Row-Change Processing

| Feature area | Stock SQLite contract | Vendored source anchors | Tier | FrankenSQLite today | Downstream beads |
| --- | --- | --- | --- | --- | --- |
| Insert/update/delete flow is routed through storage + index subsystems | Content rows, docsize rows, totals, and segment updates are coordinated by storage/index routines. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_storage.c:739-1104`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c` segment flush/update paths | Mandatory parity | Current DML mutates in-memory FTS state through the live-vtab object. | `bd-2nzo8.3.2`, `bd-2nzo8.3.4`, `bd-2nzo8.4.3`, `bd-2nzo8.5.1` |
| `sqlite3_value_nochange()` locale-preserving updates matter | On UPDATE, unchanged columns can retain locale-tagged values via saved-row logic instead of lossy retokenization. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_storage.c:29-45`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_storage.c:471-507`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_storage.c:993-995` | Mandatory parity | No equivalent saved-row locale-preserving write path exists yet. | `bd-2nzo8.3.2`, `bd-2nzo8.4.3`, `bd-2nzo8.4.8`, `bd-2nzo8.6.1` |
| Contentless-delete uses tombstone semantics, not ordinary content deletion | Deletes on `contentless_delete=1` tables consult stored origins and emit tombstone behavior through index maintenance. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_storage.c:623-656`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_storage.c:638` | Mandatory parity | Current crate has a simplified contentless-delete behavior without stock shadow-table persistence. | `bd-2nzo8.3.2`, `bd-2nzo8.4.3`, `bd-2nzo8.6.1`, `bd-2nzo8.6.2` |
| `columnsize=0` changes write-time and integrity expectations | Missing `%_docsize` rows are valid only under specific config; other flows must adapt accordingly. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_storage.c:90`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_storage.c:661-665`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_storage.c:1293-1295` | Mandatory parity | Current backend does not yet represent this through stock `%_docsize` semantics. | `bd-2nzo8.3.2`, `bd-2nzo8.4.3`, `bd-2nzo8.6.1` |

## Matrix G: Command Channel, Maintenance, and Integrity

| Feature area | Stock SQLite contract | Vendored source anchors | Tier | FrankenSQLite today | Downstream beads |
| --- | --- | --- | --- | --- | --- |
| Special INSERT command channel is part of the public surface | FTS5 interprets special INSERTs for commands such as `delete-all`, `rebuild`, `optimize`, `merge`, `integrity-check`, and config writes. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_main.c:1742-1778`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_main.c:1964` | Mandatory parity | `crates/fsqlite-core/src/connection.rs` has maintenance handling, but not yet through a stock shadow-backed backend. | `bd-2nzo8.4.7`, `bd-2nzo8.6.1` |
| Delete-all and rebuild are real backend operations | They reset `%_data`, `%_docsize`, totals, config version state, and rebuild from content. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_storage.c:801-846`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_storage.c:832` | Mandatory parity | Current path is not stock shadow-backed rebuild logic. | `bd-2nzo8.4.5`, `bd-2nzo8.4.7`, `bd-2nzo8.6.1`, `bd-2nzo8.6.3` |
| Optimize/merge/reset/sync/rollback are backend lifecycle hooks | They affect persistent segment state and transactional visibility. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_storage.c:914-922`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_storage.c:1482-1498`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c` merge paths | Mandatory parity | Current live-vtab transactional hooks do not map to stock shadow-table storage. | `bd-2nzo8.4.5`, `bd-2nzo8.5.1`, `bd-2nzo8.5.2`, `bd-2nzo8.6.1` |
| Integrity-check is storage-aware and content-mode-aware | Integrity uses tokenization, docsize validation, content counts, and low-level index checks. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_storage.c:1130-1356`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c:8153-8769` | Mandatory parity | No stock-equivalent integrity pass exists today. | `bd-2nzo8.4.5`, `bd-2nzo8.6.1`, `bd-2nzo8.6.2` |
| `fts5vocab`-relevant internal iteration behavior must match the main engine | Internal iterators and command semantics used by vocab/introspection flows depend on the actual storage format and query flags. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c:7407-7424`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c:7501`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c:7597` | Mandatory parity | Current backend does not expose stock shadow-backed iterator behavior. | `bd-2nzo8.4.7`, `bd-2nzo8.6.1` |

## Matrix H: Query Semantics, Auxiliary Functions, Tokenizers, Locale, and Tokendata

| Feature area | Stock SQLite contract | Vendored source anchors | Tier | FrankenSQLite today | Downstream beads |
| --- | --- | --- | --- | --- | --- |
| Core query expression behavior | FTS5 supports its own expression parsing/evaluation model including phrase, prefix, column filters, NEAR, and other FTS-specific query constructs. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_expr.c`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_main.c`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c` iterator paths | Mandatory parity | Current crate already implements a large part of this surface in-memory. | `bd-2nzo8.4.4`, `bd-2nzo8.6.1` |
| Auxiliary functions are bound to FTS runtime context | Ranking, highlighting, snippets, insttoken behavior, and vocab-related flows depend on query/runtime state. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_aux.c`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_main.c:3667-3818` | Mandatory parity | Current crate has `bm25`, `highlight`, and `snippet`, but not yet on top of shadow-backed runtime state. | `bd-2nzo8.2.5`, `bd-2nzo8.4.4`, `bd-2nzo8.6.1` |
| Tokenizer API surface includes v1 and v2 registration/find flows | SQLite exposes tokenizer registration and lookup APIs, including v2 entrypoints. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_main.c:3223-3443`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_main.c:3777-3778`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_tokenize.c` | Mandatory parity | Current crate has local tokenizer factory logic but not the full stock extension API contract. | `bd-2nzo8.2.5`, `bd-2nzo8.4.8`, `bd-2nzo8.6.1` |
| `fts5_locale()` is a real typed-value channel, gated by `locale=1` | Locale-tagged values must be preserved and validated; writes without `locale=1` are rejected. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_main.c:90`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_main.c:1311-1379`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_main.c:2005-2012`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_aux.c:750-812` | Mandatory parity | Current backend does not fully model locale-tagged storage/update semantics. | `bd-2nzo8.3.2`, `bd-2nzo8.4.4`, `bd-2nzo8.4.8`, `bd-2nzo8.6.1`, `bd-2nzo8.6.2` |
| `fts5_insttoken()` and `insttoken` config are real behavior, not stubs | Insttoken affects query/runtime inspection behavior and is configurable. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_main.c:3667-3818`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_config.c:1031` | Mandatory parity | Only partial surface exists today. | `bd-2nzo8.4.4`, `bd-2nzo8.4.7`, `bd-2nzo8.4.8`, `bd-2nzo8.6.1` |
| `tokendata=1` changes both query and internal iteration semantics | Integrity and vocab queries may need `NOTOKENDATA` handling; iterators operate differently for token-data payloads. | `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c:6309-6428`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c:7003`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c:7407-7424`, `legacy_sqlite_code/sqlite/ext/fts5/fts5_index.c:7597` | Mandatory parity | Not yet modeled as part of a stock-compatible segment engine. | `bd-2nzo8.3.4`, `bd-2nzo8.4.4`, `bd-2nzo8.4.8`, `bd-2nzo8.6.1`, `bd-2nzo8.6.2` |

## Implementation Consequences For Later Beads

This matrix intentionally constrains later design freedom:

1. `bd-2nzo8.2.1` and `bd-2nzo8.2.4` must add a shadow-table ownership contract to the Rust vtab substrate. A name-only heuristic in schema reload is not enough.
2. `bd-2nzo8.2.2` and `bd-2nzo8.4.1` must treat rootpage=0 virtual-table rows as first-class catalog objects, not placeholders.
3. `bd-2nzo8.3.1` through `bd-2nzo8.3.4` must implement exact codecs for `%_config`, `%_content`, `%_docsize`, `%_data`, and `%_idx`, including structure-record V1/V2 details and merge-related metadata.
4. `bd-2nzo8.4.3`, `bd-2nzo8.4.5`, `bd-2nzo8.4.7`, and `bd-2nzo8.4.8` must model user-visible command, DML, aux, tokenizer, locale, and maintenance behavior through the real backend rather than through compatibility shims.
5. `bd-2nzo8.5.*` must optimize a segment-native backend. The target shape is lazy readers plus pending-state publication, not whole-table hydration.
6. `bd-2nzo8.6.*` must validate both behavioral parity and artifact-level evidence: exact config/layout behavior, query parity, corruption detection, migration outcomes, and replayable logs/manifests.

## Recommended Validation Focus Derived From This Matrix

The later proof-artifact and differential-test beads should treat the following as mandatory coverage buckets:

- Catalog/reload: rootpage=0 create/open/reload/rename/drop.
- DDL restrictions: direct writes to shadow tables, triggers on vtabs/shadow tables, defensive-mode failures.
- Content modes: stored, external-content, contentless, contentless-delete, `columnsize=0`.
- Config/control commands: `pgsz`, merge knobs, `rank`, `secure-delete`, `insttoken`.
- Query semantics: phrase, prefix, column filter, NEAR, caret, ranking, highlight, snippet.
- Tokenizer/locale/tokendata: custom tokenizer registration, locale-tagged values, token-data queries and vocab behavior.
- Maintenance/integrity: rebuild, optimize, merge, delete-all, integrity-check, corruption cases.
- Performance evidence: open/reopen latency, bounded RSS, query p50/p95, write throughput, segment-merge debt, and conflict behavior under concurrent writers.

## Bottom Line

The shadow-backed FTS5 epic is not "make MATCH work on old databases". The real contract is:

- stock SQLite catalog semantics,
- stock SQLite shadow-table ownership and protections,
- stock SQLite shadow-table storage layout,
- stock SQLite command and integrity behavior,
- FrankenSQLite-native MVCC and concurrency characteristics.

Any plan that keeps the current in-memory/materialized backend as the primary persistence model, omits `xShadowName`-equivalent behavior, or treats `%_data`/`%_idx` as an implementation detail instead of the canonical backend is out of contract with this epic.
