# Loom Model: Page-Lock Acquire/Release vs CommitIndex Publish Ordering

**Bead:** bd-iqd1w  
**Test file:** `crates/fsqlite-mvcc/tests/loom_page_lock_commit_ordering.rs`

## Invariant Under Verification

> For every schedule: if a reader sees `CommitIndex[page] = seq`, then the
> WAL fsync for sequence `seq` has completed and the version-store data for
> `seq` is visible to the reader.

Equivalently: no interleaving produces a state where a reader observes a
commit-index entry pointing to data that hasn't been fsynced yet.

## Abstraction → Production Mapping

| Model element            | Production code                                        | File:Line                            |
|--------------------------|--------------------------------------------------------|--------------------------------------|
| `page_lock: AtomicU64`   | `InProcessPageLockTable::fast_locks[pgno]`             | `core_types.rs:401`                  |
| Lock acquire CAS 0→txn   | `try_acquire` fast-path CAS                            | `core_types.rs:681` (AcqRel/Acquire) |
| Lock release CAS txn→0   | `release` fast-path CAS                                | `core_types.rs:758` (AcqRel/Relaxed) |
| `commit_idx: AtomicU64`  | `CommitIndex::fast_array` per-page slot                | `core_types.rs:2225`                 |
| CI publish store          | `CommitIndex::update` per-page store                   | `core_types.rs:2299` (Release)       |
| CI batch fence+stores     | `CommitIndex::batch_update` Release fence + Relaxed    | `core_types.rs:2338-2346`            |
| CI reader load            | `CommitIndex::latest` per-page load                    | `core_types.rs:2364` (Acquire)       |
| `fsync_done: AtomicBool` | WAL fsync barrier (FSYNC_2) return                     | `native_commit.rs:500-502`           |
| `data_written: AtomicU64` | Version-store / page data writes before fsync          | (various write paths)                |

## Model Simplifications

1. **Single page slot.** The production CommitIndex has 65536 fast-array slots
   and a sharded fallback. The model exercises one slot because the ordering
   properties are per-slot (each slot is an independent atomic).

2. **Fsync as atomic store.** The real fsync is a kernel barrier that flushes
   all preceding writes to durable media. In the model, `fsync_done.store(true,
   Release)` serves the same role: it orders all preceding Relaxed stores
   before any reader that loads `fsync_done` with Acquire.

3. **No sharded/LeftRight path.** The sharded fallback path (pages > 65536)
   uses `RwLock` + HashMap, which trivially serializes all accesses. The model
   focuses on the lock-free fast-array path where ordering bugs can hide.

4. **Two writers max.** Loom's state space grows exponentially with thread
   count. Two writers are sufficient to verify lock-handoff visibility.

## Test Catalog

| Test | Threads | Checks | Expected |
|------|---------|--------|----------|
| T1: Correct ordering | 1 writer + 1 reader | fsync-before-publish invariant | Pass (no violation) |
| T2: Two writers | 2 writers + 1 reader | Same invariant with lock contention | Pass |
| T3: Weakened ordering (NEGATIVE) | 1 buggy writer + 1 reader | Publish-before-fsync detected | Must fail (catches bug) |
| T4: Batch update fence | 1 writer + 1 reader | Release fence + Relaxed store pattern | Pass |
| T5: Lock handoff visibility | 2 writers | AcqRel CAS data visibility chain | Pass |

## Running

```bash
# Standard mode (smoke tests only, no loom):
cargo test -p fsqlite-mvcc --test loom_page_lock_commit_ordering

# Loom mode (exhaustive schedule exploration):
RUSTFLAGS="--cfg loom" cargo test -p fsqlite-mvcc --test loom_page_lock_commit_ordering -- --test-threads=1
```

## CI Guidance

- **PR CI:** Run loom tests with `--test-threads=1` (takes ~9s on modern hardware).
- **Nightly CI:** Same — the 5-test state space is tractable for exhaustive search.
- If state space grows (more threads/operations added), consider shuttle fallback
  with `--random=1_000_000` iterations and a pinned seed.
