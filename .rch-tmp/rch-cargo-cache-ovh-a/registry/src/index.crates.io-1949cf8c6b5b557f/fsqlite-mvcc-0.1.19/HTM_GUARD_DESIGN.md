# HTM Fast-Path Guard Design — bd-77l3t

> Architecture, fallback proof, abort telemetry, rollout shape, proof obligations.
> No implementation code. No new crates. No Cargo changes.

---

## 1. Problem Statement

At low concurrency (c1/c4), `FlatCombiner` and `CommitSequenceCombiner` pay
combiner-lock acquisition overhead even when contention is minimal. The uncontested
`try_lock()` path costs ~25ns (mutex CAS + memory barriers). Under c4 with disjoint
keys, this fixed cost dominates over useful batching work.

Hardware Transactional Memory (Intel TSX RTM / ARM TME) can speculatively execute the
`combine_locked()` body in L1/L2 cache without broadcasting cache invalidations,
reducing uncontested fast-path cost to ~5-8ns.

**Goal:** Define a guard architecture that wraps the combiner hot-path with an
HTM-first/lock-fallback discipline, including abort telemetry, dynamic disable,
and formal correctness proofs — all without changing the existing lock behavior
or introducing new crates.

---

## 2. Existing Module Seam

All code changes live within **`crates/fsqlite-mvcc/src/flat_combining.rs`**
and **`crates/fsqlite-mvcc/src/commit_combiner.rs`**. These are the only two
files that contain combiner lock paths.

### 2.1 FlatCombiner (`flat_combining.rs`)

**Existing infrastructure already in place:**

| Line(s) | What exists | Status |
|---------|------------|--------|
| 40-44 | HTM metric names in doc comment | Stub |
| 78-82 | Global metric atomics: `FC_HTM_ATTEMPTS`, `FC_HTM_ABORTS_{CONFLICT,CAPACITY,EXPLICIT,OTHER}` | Allocated, never incremented from real path |
| 84-91 | XABORT bit constants (`XABORT_EXPLICIT`, `XABORT_RETRY`, etc.) | Defined |
| 92-107 | `HtmAbortReason` enum + `HtmAbortClassification` struct | Defined |
| 168-191 | `classify_htm_abort_status()` — const fn abort classifier | Implemented |
| 193-226 | `record_htm_attempt()`, `record_htm_abort_status()`, `note_htm_attempt()`, `note_htm_abort()` | Implemented, never called from real path |
| 425-427 | `try_lock()` → `combine_locked()` — **THE integration point** | Lock path |

The module already has the abort classification, metric counters, and public
recording functions. What's missing is:
1. A **guard state machine** that decides whether to attempt HTM
2. An **abort-rate monitor** with EWMA and dynamic disable
3. The **try-HTM-first call site** in `FcHandle::apply()`
4. Tracing integration per the bead spec

### 2.2 CommitSequenceCombiner (`commit_combiner.rs`)

Identical structure: `try_lock()` → `combine_locked()` at the same logical
position in `CommitCombineHandle::alloc()`. Same guard would apply.

### 2.3 No other files touched

The guard is entirely internal to these two modules. No public API changes.
No new dependencies. No Cargo.toml modifications.

---

## 3. Guard State Machine

```
                    ┌──────────────┐
        startup     │  NOT_PROBED  │
        ┌──────────►│    (0)       │
        │           └──────┬───────┘
        │                  │ cpu_probe()
        │                  ▼
        │    ┌─────────────────────────┐
        │    │                         │
        │    ▼                         ▼
   ┌─────────────┐             ┌──────────────┐
   │  AVAILABLE   │             │ UNAVAILABLE  │
   │    (1)       │◄────┐       │    (2)       │
   └──────┬───────┘     │       └──────────────┘
          │             │              ▲
          │ ewma > 50%  │              │ buggy stepping
          ▼             │              │
   ┌──────────────┐     │       ┌──────────────┐
   │  DISABLED    │─────┘       │ BLACKLISTED  │
   │    (4)       │ cooldown    │    (3)       │
   └──────────────┘ expires     └──────────────┘
          ▲
          │ PRAGMA disable_htm = ON
   ┌──────────────┐
   │ USER_DISABLED│
   │    (5)       │
   └──────────────┘
```

**Storage:** Single `AtomicU8` — fits in the existing `FlatCombiner` struct
alongside `value`, `slots`, `owners`, `combiner_lock`. Zero heap allocation.

**Transitions:**
- `NOT_PROBED → AVAILABLE`: First call to `apply()` triggers lazy probe
- `NOT_PROBED → UNAVAILABLE`: CPU lacks RTM (or non-x86_64)
- `NOT_PROBED → BLACKLISTED`: Known-buggy CPU stepping detected
- `AVAILABLE → DISABLED`: EWMA abort rate > 50%
- `DISABLED → AVAILABLE`: Cooldown timer expires (5s initial, 2x backoff, max 60s)
- `AVAILABLE → USER_DISABLED`: PRAGMA fsqlite_disable_htm = ON
- `USER_DISABLED → {probe result}`: PRAGMA fsqlite_disable_htm = OFF

---

## 4. Abort Rate Monitor (EWMA)

Stored as additional fields alongside the guard state:

```
ewma_abort_rate: AtomicU32,      // Fixed-point [0..10000] = [0.0000..1.0000]
window_attempts: AtomicU64,
window_aborts:   AtomicU64,
window_start_ns: AtomicU64,
disable_count:   AtomicU32,      // For exponential backoff
last_disable_ns: AtomicU64,
```

**All fit within the existing struct.** No heap allocation. ~48 bytes total.

### Update rule

Every 1000 attempts OR every 100ms (whichever comes first):

```
alpha = 0.3
new_rate = window_aborts / window_attempts
ewma = alpha * new_rate + (1 - alpha) * ewma
```

Fixed-point arithmetic avoids floating-point in the hot path:
```
ewma_fp = (3000 * new_rate_fp + 7000 * old_ewma_fp) / 10000
```

### Disable threshold

If `ewma_abort_rate > 5000` (50%), transition to DISABLED state.

### Cooldown with exponential backoff

```
cooldown_ms = min(5000 * 2^disable_count, 60_000)
```

After cooldown expires, reset EWMA and window, re-enable, increment `disable_count`.

---

## 5. `FcHandle::apply()` — Modified Call Site Shape

The existing code at lines 409-465:

```rust
// Current:
pub fn apply(&self, op: u64, arg: u64) -> u64 {
    // ... publish request to slot ...
    if let Some(_guard) = self.combiner.combiner_lock.try_lock() {
        self.combiner.combine_locked();
    }
    // ... spin-wait ...
}
```

The guard inserts a **pre-lock fast-path check**:

```
pub fn apply(&self, op: u64, arg: u64) -> u64 {
    // ... publish request to slot ...

    // HTM fast-path guard
    let htm_state = self.combiner.htm_state.load(Relaxed);
    if htm_state == AVAILABLE {
        // <-- HTM TRANSACTION BOUNDARY -->
        // Attempt: execute combine_locked() speculatively
        // On commit: return immediately (slot has result)
        // On abort: record_htm_abort_status(eax), fall through
        //
        // The actual _xbegin/_xend calls require a safe wrapper.
        // This design defines WHERE they go and WHAT surrounds them.
        // The wrapper itself is a separate implementation decision.
    }

    // Existing lock path (unchanged)
    if let Some(_guard) = self.combiner.combiner_lock.try_lock() {
        self.combiner.combine_locked();
    }
    // ... spin-wait (unchanged) ...
}
```

**Key constraint:** The code *inside* the HTM boundary must be exactly
`combine_locked()` — the same function, same arguments, same result. The only
difference is the exclusion mechanism.

---

## 6. What Runs Inside the HTM Transaction

The `combine_locked()` body (lines 334-380) does:

1. Load `self.value` (1 AtomicU64 read)
2. Scan `self.slots[0..64]`: for each, load `state` + `payload` (128 atomic reads)
3. For pending slots: compute result, store `payload` + `state` (up to 128 atomic writes)
4. Store `self.value` (1 atomic write)
5. Update 4 metric atomics (Relaxed stores)

**Cache footprint:**
- `value`: 1 cache line (8 bytes in 64-byte line)
- `slots`: 64 × 16 bytes = 1024 bytes = 16 cache lines
- `owners`: not accessed during combine
- Metrics: 4 × 8 bytes = 32 bytes = 1 cache line

**Total: ~18 cache lines ≈ 1152 bytes.** TSX L1D capacity tracks ~32KB.
Well within capacity. Capacity aborts should be rare.

**No I/O, syscalls, or non-transactional stores.** The function is pure
computation over atomic memory. This is the ideal HTM workload.

---

## 7. Tracing Integration (per bead acceptance criteria)

```rust
// At CPU probe time (once per process):
tracing::info!(target: "fsqlite::htm", event = "cpu_probe",
    tsx_available = %tsx, tme_available = %tme,
    stepping = %step, known_buggy = %buggy);

// Before each HTM attempt (debug-level, conditional):
tracing::debug!(target: "fsqlite::htm", event = "xbegin",
    combiner_id = %id, batch_size = %batch);

// After each abort (debug-level):
tracing::debug!(target: "fsqlite::htm", event = "xabort",
    combiner_id = %id, abort_code = %code,
    reason = %reason);  // "conflict"|"capacity"|"explicit"|"retry_exceeded"

// On dynamic disable (warn-level):
tracing::warn!(target: "fsqlite::htm", event = "dynamic_disable",
    abort_rate = %rate, threshold = %thresh, window_ms = %window);

// Periodic snapshot (info-level, every 10s or on demand):
tracing::info!(target: "fsqlite::htm", event = "stats_snapshot",
    attempts = %total, aborts_conflict = %c,
    aborts_capacity = %cap, success_rate = %pct);
```

---

## 8. Rollout Shape

### Phase 1: Guard skeleton + abort telemetry (this bead)
- Add `htm_state: AtomicU8` + EWMA fields to `FlatCombiner` struct
- Add guard state machine (probe → available/unavailable/disabled transitions)
- Wire abort telemetry into existing metric counters
- Add PRAGMA handler
- **No actual HTM intrinsics.** Default probe returns `UNAVAILABLE` on all platforms.
- **Net effect: zero behavior change.** All threads use lock path as today.
- This can be tested, merged, and verified safe.

### Phase 2: Loom model (child bead bd-2phz6)
- Model the guard+fallback in Loom with exhaustive state exploration
- Prove: no lost updates, no deadlock, no livelock under the guard transitions
- Prove: combine_locked() under HTM produces same linearization as under lock

### Phase 3: Safe HTM wrapper (child bead bd-2w7no)
- Implementation decision for how to call _xbegin/_xend safely
- Options: (a) external C shim linked via build.rs, (b) future safe intrinsics
  RFC, (c) third-party crate with audited unsafe, (d) feature-gated crate-level
  lint override
- This decision is deferred until Phase 1 and Phase 2 are complete

### Phase 4: Abort telemetry (child bead bd-1571a)
- Wire real abort data into the monitor
- Calibrate EWMA alpha and disable threshold against benchmark data
- Publish via `fsqlite_htm_metrics` virtual table

---

## 9. Proof Obligations

### P1: Lock-equivalence of HTM path
**Claim:** A committed HTM transaction produces the same observable result as
executing `combine_locked()` while holding `combiner_lock`.

**Proof sketch:**
- The HTM path executes identical code to `combine_locked()`.
- HTM commit atomically publishes all writes. The combiner lock provides
  equivalent mutual exclusion.
- Both paths: (a) read the same slots, (b) compute the same results,
  (c) store the same values to the same locations.
- The only difference is the exclusion mechanism (hardware vs. software).
- **Obligation:** Verify no state is accessed between `try_lock()` return and
  `combine_locked()` entry that would differ under HTM. Inspection of lines
  425-427 confirms: `try_lock()` returns `MutexGuard`, then `combine_locked()`
  is called immediately. No intervening state.

### P2: Abort safety (no side effects)
**Claim:** An aborted HTM transaction has no observable side effects.

**Proof sketch:**
- Intel TSX spec: on abort, all speculative memory writes are discarded.
  Architectural register state is restored to XBEGIN snapshot.
- ARM TME: TSTART/TCOMMIT provide same guarantee.
- After abort, thread falls through to existing `try_lock()` path.
- **Obligation:** Verify `combine_locked()` performs no I/O, syscalls, or
  non-transactional stores (e.g., `volatile` writes, MMIO). Code inspection
  confirms: only `AtomicU64` load/store operations and arithmetic.
- **Obligation:** Verify metric updates happen OUTSIDE the HTM boundary.
  Design places `record_htm_abort_status()` call AFTER abort, not inside
  the transaction.

### P3: No deadlock
**Claim:** HTM fast-path cannot introduce deadlocks.

**Proof:**
- HTM transactions are non-blocking: they either commit or abort in bounded
  time. No thread waits for another thread's HTM transaction.
- The fallback lock path is single-lock, no nested acquisition → deadlock-free.
- Guard state transitions are wait-free (atomic CAS on `htm_state`).

### P4: Progress guarantee (no livelock)
**Claim:** The system makes forward progress even under sustained HTM aborts.

**Proof:**
- After MAX_HTM_RETRIES (3) consecutive aborts on a single `apply()` call,
  the thread unconditionally falls through to the lock path.
- The lock path guarantees progress: `try_lock()` either succeeds (combiner
  processes batch) or fails (another thread is combining, and will process
  this thread's published slot).
- Dynamic disable ensures sustained abort storms cause a clean transition
  to lock-only mode within 100ms.
- **Obligation:** Verify the retry count is per-invocation, not global.

### P5: No false-sharing from guard state
**Claim:** Adding `htm_state` and EWMA fields to `FlatCombiner` does not
introduce false sharing on the hot path.

**Design mitigation:**
- Place guard fields on a SEPARATE cache line from `value` and `combiner_lock`.
- `htm_state` (1 byte) + EWMA fields (~48 bytes) fit in one 64-byte cache line.
- The existing `value` field is on its own line; `slots` array is cache-line
  padded; `combiner_lock` is separate.
- **Obligation:** Verify with `#[repr(align(64))]` or explicit padding.

### P6: Metric accuracy
**Claim:** HTM abort telemetry counters are monotonically accurate.

**Proof:**
- Counters use `fetch_add(1, Relaxed)` — no lost updates (atomic increment).
- Updates happen outside HTM transaction boundary — not subject to speculation
  rollback.
- EWMA window resets are racy (two threads may reset simultaneously), but this
  only causes a temporary over-count — the EWMA converges within one window.

---

## 10. Assumptions Ledger

| # | Assumption | Failure mode | Mitigation |
|---|-----------|-------------|------------|
| A1 | TSX not disabled by microcode/BIOS | _xbegin always returns abort code | Dynamic disable triggers within 100ms |
| A2 | Working set fits in L1D (~32KB) | Capacity aborts | Measured at ~1.2KB; if spikes, dynamic disable |
| A3 | No I/O/syscalls in combine_locked() | Transaction always aborts | Verified by code inspection (pure atomic ops) |
| A4 | Metrics updated outside HTM boundary | Correct telemetry | Enforced by design: record after commit/abort |
| A5 | combine_locked() is total (always terminates) | Livelock inside transaction | 64-iteration bounded loop; no dynamic allocation |
| A6 | Slot state machine is correct without HTM | Lock-path regression | Phase 1 adds only guard skeleton; lock path unchanged |
| A7 | No tracing inside HTM boundary | tracing allocates → capacity abort | Design rule: all tracing calls outside HTM region |

---

## 11. Fallback Trigger Matrix

| Condition | Guard action | Recovery |
|-----------|-------------|----------|
| CPU lacks RTM/TME | state = UNAVAILABLE | None — lock path always |
| Known-buggy stepping | state = BLACKLISTED + warn | None — lock path always |
| Single abort, retryable | Retry (up to 3×) | Fall through to lock |
| Single abort, not retryable | No retry | Fall through to lock |
| EWMA > 50% | state = DISABLED + warn | Cooldown (5s, then probe) |
| EWMA > 50% post-probe | DISABLED + 2× backoff | Max 60s cooldown |
| PRAGMA disable_htm = ON | state = USER_DISABLED | PRAGMA OFF to re-enable |
| Phase 1 (no intrinsics) | state = UNAVAILABLE always | Full lock path always |

---

## 12. Virtual Table: `fsqlite_htm_metrics`

Exposes guard state and abort telemetry via SQL:

```sql
SELECT * FROM fsqlite_htm_metrics;
-- Returns single row:
--   htm_state TEXT,          -- 'available'|'unavailable'|'blacklisted'|'disabled'|'user_disabled'
--   attempts INTEGER,
--   aborts_conflict INTEGER,
--   aborts_capacity INTEGER,
--   aborts_explicit INTEGER,
--   aborts_other INTEGER,
--   success_rate REAL,       -- (attempts - total_aborts) / attempts
--   ewma_abort_rate REAL,
--   disable_count INTEGER,
--   cooldown_remaining_ms INTEGER
```

Implementation: read the existing global atomics + guard state. No new storage.

---

## 13. Risk Assessment

**Net risk of Phase 1 (guard skeleton only):** ZERO.
- No behavior change. All threads use lock path.
- Guard state defaults to UNAVAILABLE.
- Adds ~48 bytes to FlatCombiner struct.
- No new dependencies, no unsafe code, no Cargo changes.

**Highest risk in later phases:**
- Phase 3 (safe HTM wrapper): requires solving the unsafe boundary problem.
  Deferred intentionally. The guard design is independent of this decision.
- Phase 4 (calibration): EWMA parameters need tuning against real workloads.
  Wrong alpha/threshold → disable too eagerly (no benefit) or too slowly
  (brief perf dip). Safe fallback in both cases.

---

## 14. bd-db300.5.2.3 Evaluation

E2.3 asks whether the residual serialized region after the fused prepared-DML
entry work is small enough to justify an optional HTM or cohort fast path. The
answer for this slice is deliberately conservative: keep the HTM design hook,
reject cohort reservation for now, and retain the current lock-backed combiner
as the mandatory authority.

This is an evaluation, not an enablement change. No runtime state may transition
to `AVAILABLE` until there is a real platform probe, abort-rate telemetry, and
tail-latency evidence from the target workload. The current stub probe returns
`UNAVAILABLE`, so every call site must route to the existing lock path.

| Candidate | Decision | Abort evidence | Fallback evidence | Tail evidence required |
|-----------|----------|----------------|-------------------|------------------------|
| HTM around `combine_locked()` | Defer | No real abort stream exists; `probe_current_platform_stub()` reports unavailable. | `FallbackReason::ProbeMarkedUnavailable` keeps the existing combiner lock authoritative. | Compare `wait_ns_max` plus workload p95/p99 before permitting `AVAILABLE`. |
| Cohort reservation blocks | Reject until ordering proof | Not applicable; reservations do not abort and therefore cannot self-disable. | Existing single total-order combiner remains the fallback and publication authority. | Reconsider only if commit-combine wait tail dominates after E2.2 and E3 publication work. |
| Lock fallback | Retain | Not applicable; no speculative region. | Mandatory path for unavailable, disabled, aborted, or rejected fast paths. | Existing metrics expose wait totals/maxima for scoped tail checks. |

Why cohort reservation is not selected here: commit sequence allocation is not
just an ID counter. `CommitSequenceCombiner::combine_locked()` can batch-register
newly allocated commit sequences into the active registry before signaling
waiters. A per-cohort reservation scheme would need a reconciliation proof that
preserves total commit order and does not create an allocated-but-unregistered
gap. That proof is larger than E2.3 and would be riskier than the residual
serialized region this bead is trying to optimize.

Why HTM remains a design hook: the hot body is bounded and small enough to be a
candidate, but correctness depends on the exact fallback discipline in this
document. Any future HTM implementation must satisfy all of these conditions:

1. The lock path remains compiled, tested, and reachable on every platform.
2. Abort counters are updated outside the hardware transaction boundary.
3. Dynamic disable routes sustained abort storms back to the lock path.
4. Benchmark evidence reports p50, p95, and p99, not only throughput.
5. `concurrent_mode_default` and all concurrent-writer defaults remain enabled.

The machine-readable record for this decision lives in
`DB300_E2_3_RESIDUAL_FAST_PATH_EVALUATION` in `src/htm_fast_path.rs`.
