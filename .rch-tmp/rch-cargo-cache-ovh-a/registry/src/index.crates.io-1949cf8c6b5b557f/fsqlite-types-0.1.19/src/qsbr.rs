//! Quiescent-State-Based Reclamation (QSBR) — safe variant.
//!
//! QSBR is a memory reclamation scheme for lock-free data structures. A thread
//! declares itself "quiescent" when it has no outstanding references to any
//! protected data. Once all threads have been quiescent at least once since an
//! object was retired, that object may be safely reclaimed.
//!
//! # Safe variant
//!
//! A strict, lock-free QSBR implementation requires `unsafe` (raw
//! `thread_local!` cells, relaxed atomics with manual SeqCst fences, and
//! pointer-based retire lists). FrankenSQLite forbids `unsafe` in the
//! workspace, so this module provides a **safe approximation**:
//!
//! * The retire-queue is guarded by a [`Mutex`], which is contended only on
//!   `retire()` / `try_reclaim()`. Readers never touch it.
//! * Reader guards ([`QsbrGuard`]) are still lock-free — they perform only a
//!   single relaxed atomic load on construction and a no-op on `Drop`.
//! * Reclamation is cooperative: the user must call [`QsbrDomain::advance_epoch`]
//!   between reader quiescent points and [`QsbrDomain::try_reclaim`] to drain
//!   the retire queue.
//!
//! The cost compared to a true QSBR is a mutex on the retire/reclaim path.
//! That is acceptable because retire/reclaim happen at coarse boundaries
//! (e.g. after a page eviction or schema change), not in the hot read path.
//!
//! # Semantics
//!
//! * `advance_epoch()` bumps the domain epoch by 1 (wrapping).
//! * `retire(obj)` stamps the object with the current epoch and queues it.
//! * `try_reclaim()` drops every retired object whose stamped epoch is
//!   **strictly less than** the current epoch. This is the standard
//!   "epoch-based" rule: anything retired in epoch `e` is safe to reclaim
//!   once the domain reaches epoch `e + 1` and all guards taken at or before
//!   `e` have dropped.
//!
//! Note: this module does **not** track live guards globally. The caller is
//! responsible for ensuring all guards from epoch `e` have dropped before
//! calling `try_reclaim()` after `advance_epoch()`. In FrankenSQLite this is
//! driven by the pager's page-lifecycle state machine, which only advances
//! the epoch after it has observed zero outstanding readers.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::sync_primitives::Mutex;

/// A QSBR reclamation domain.
///
/// Multiple domains may coexist (e.g. one per logical region). Each domain
/// has its own epoch counter and retire queue.
pub struct QsbrDomain {
    /// Current epoch. Monotonically increasing (wrapping at `u64::MAX`, which
    /// is billions of years away at any realistic rate).
    epoch: AtomicU64,
    /// Retired objects, each tagged with the epoch in which it was retired.
    /// The closure drops the object when invoked.
    retired: Mutex<Vec<RetiredEntry>>,
}

struct RetiredEntry {
    /// Epoch at which this object was retired.
    epoch: u64,
    /// Drops the object when invoked. `Option` so we can `take()` and call
    /// without needing `FnOnce` move semantics through a `Box<dyn FnOnce>`.
    dropper: Option<Box<dyn FnOnce() + Send>>,
}

impl Default for QsbrDomain {
    fn default() -> Self {
        Self::new()
    }
}

impl QsbrDomain {
    /// Create a new domain starting at epoch 0 with an empty retire queue.
    #[must_use]
    pub fn new() -> Self {
        Self {
            epoch: AtomicU64::new(0),
            retired: Mutex::new(Vec::new()),
        }
    }

    /// Open a reader guard at the current epoch. Lock-free (single atomic
    /// load).
    ///
    /// The guard's only job is to document the epoch a reader observed. It
    /// does not itself block reclamation — the caller's pager logic must
    /// ensure no guard is live when it calls [`Self::try_reclaim`] after
    /// [`Self::advance_epoch`].
    #[must_use]
    pub fn guard(&self) -> QsbrGuard<'_> {
        QsbrGuard {
            domain: self,
            epoch_taken: self.epoch.load(Ordering::Acquire),
        }
    }

    /// Return the current epoch. Primarily useful for tests and diagnostics.
    #[must_use]
    pub fn current_epoch(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }

    /// Advance the domain epoch by 1. Safe to call concurrently with readers;
    /// new guards will observe the new epoch, existing guards keep their
    /// original `epoch_taken`.
    pub fn advance_epoch(&self) {
        // Release ordering so that any retire() that happens-after this
        // advance observes the new epoch when it loads with Acquire.
        self.epoch.fetch_add(1, Ordering::AcqRel);
    }

    /// Queue `obj` for eventual reclamation. The object is moved into an
    /// internal closure and dropped by [`Self::try_reclaim`] once the epoch
    /// has advanced past the current one.
    pub fn retire<T: Send + 'static>(&self, obj: T) {
        let epoch = self.epoch.load(Ordering::Acquire);
        let dropper: Box<dyn FnOnce() + Send> = Box::new(move || {
            drop(obj);
        });
        let mut queue = self.retired.lock();
        queue.push(RetiredEntry {
            epoch,
            dropper: Some(dropper),
        });
    }

    /// Drop every retired object whose retire-epoch is strictly less than the
    /// current domain epoch. Returns the number of objects reclaimed.
    ///
    /// Safe to call concurrently with readers, but as documented in the
    /// module header: the caller must have ensured no guard taken at an
    /// epoch `< current` is still live before invoking this.
    pub fn try_reclaim(&self) -> usize {
        let current = self.epoch.load(Ordering::Acquire);
        // Swap out the queue to minimize time spent holding the lock while
        // we run the drop closures (which could themselves allocate, log,
        // etc.).
        let mut to_drop: Vec<RetiredEntry> = Vec::new();
        {
            let mut queue = self.retired.lock();
            // Partition in-place: keep entries with epoch >= current, extract
            // entries with epoch < current into `to_drop`.
            let mut i = 0;
            while i < queue.len() {
                if queue[i].epoch < current {
                    to_drop.push(queue.swap_remove(i));
                } else {
                    i += 1;
                }
            }
        }
        let reclaimed = to_drop.len();
        for mut entry in to_drop {
            if let Some(dropper) = entry.dropper.take() {
                dropper();
            }
        }
        reclaimed
    }

    /// Number of currently-queued retired entries (for tests / diagnostics).
    #[must_use]
    pub fn pending_retired_count(&self) -> usize {
        self.retired.lock().len()
    }
}

/// RAII reader guard. Records the epoch at which the reader became active.
/// `Drop` is a no-op — reclamation is driven by the domain, not the guard.
///
/// The guard holds a reference to its domain purely to tie its lifetime to
/// the domain at compile time, preventing use-after-free of the domain
/// itself.
pub struct QsbrGuard<'a> {
    domain: &'a QsbrDomain,
    epoch_taken: u64,
}

impl QsbrGuard<'_> {
    /// Epoch at which this guard was taken.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch_taken
    }

    /// Reference to the guard's domain.
    #[must_use]
    pub fn domain(&self) -> &QsbrDomain {
        self.domain
    }
}

// `Drop` is explicit (and empty) to document intent.
impl Drop for QsbrGuard<'_> {
    fn drop(&mut self) {
        // No-op: see module docs. Reclamation is cooperative via
        // `QsbrDomain::try_reclaim`.
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use std::time::Duration;

    use super::*;

    /// Tracks drops by decrementing a shared counter when dropped.
    struct DropSpy {
        live: Arc<AtomicUsize>,
    }

    impl DropSpy {
        fn new(live: Arc<AtomicUsize>) -> Self {
            live.fetch_add(1, Ordering::SeqCst);
            Self { live }
        }
    }

    impl Drop for DropSpy {
        fn drop(&mut self) {
            self.live.fetch_sub(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn retire_without_advance_does_not_drop() {
        let domain = QsbrDomain::new();
        let live = Arc::new(AtomicUsize::new(0));

        domain.retire(DropSpy::new(live.clone()));
        assert_eq!(live.load(Ordering::SeqCst), 1);
        assert_eq!(domain.pending_retired_count(), 1);

        // No advance_epoch → nothing is reclaimable.
        let reclaimed = domain.try_reclaim();
        assert_eq!(reclaimed, 0);
        assert_eq!(live.load(Ordering::SeqCst), 1);
        assert_eq!(domain.pending_retired_count(), 1);
    }

    #[test]
    fn advance_epoch_then_reclaim_drops() {
        let domain = QsbrDomain::new();
        let live = Arc::new(AtomicUsize::new(0));

        domain.retire(DropSpy::new(live.clone()));
        domain.retire(DropSpy::new(live.clone()));
        assert_eq!(live.load(Ordering::SeqCst), 2);

        domain.advance_epoch();
        let reclaimed = domain.try_reclaim();
        assert_eq!(reclaimed, 2);
        assert_eq!(live.load(Ordering::SeqCst), 0);
        assert_eq!(domain.pending_retired_count(), 0);

        // Retire after the advance → requires another advance to reclaim.
        domain.retire(DropSpy::new(live.clone()));
        assert_eq!(live.load(Ordering::SeqCst), 1);
        assert_eq!(domain.try_reclaim(), 0);
        assert_eq!(live.load(Ordering::SeqCst), 1);

        domain.advance_epoch();
        assert_eq!(domain.try_reclaim(), 1);
        assert_eq!(live.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn guard_records_current_epoch() {
        let domain = QsbrDomain::new();
        let g0 = domain.guard();
        assert_eq!(g0.epoch(), 0);
        drop(g0);

        domain.advance_epoch();
        let g1 = domain.guard();
        assert_eq!(g1.epoch(), 1);
        assert_eq!(g1.domain().current_epoch(), 1);

        domain.advance_epoch();
        // Existing guard keeps its original epoch.
        assert_eq!(g1.epoch(), 1);
        assert_eq!(domain.current_epoch(), 2);
    }

    #[test]
    fn multithreaded_readers_and_retirer() {
        // Scenario:
        //   - 4 reader threads each take a guard at epoch 0, hold it briefly,
        //     then drop. They check the guard's epoch matches what they read.
        //   - 1 retirer thread retires N objects at epoch 0, waits for the
        //     readers to finish, advances the epoch, then calls try_reclaim.
        //   - Assert: all N objects are reclaimed exactly once; no object is
        //     reclaimed before the epoch advance.
        let domain = Arc::new(QsbrDomain::new());
        let live = Arc::new(AtomicUsize::new(0));
        const N_RETIRED: usize = 32;
        const N_READERS: usize = 4;

        // Barrier-ish coordination via atomics (no external deps).
        let readers_started = Arc::new(AtomicUsize::new(0));
        let allow_readers_finish = Arc::new(AtomicUsize::new(0));

        thread::scope(|s| {
            // Spawn readers.
            let mut reader_handles = Vec::new();
            for _ in 0..N_READERS {
                let domain = domain.clone();
                let readers_started = readers_started.clone();
                let allow_readers_finish = allow_readers_finish.clone();
                reader_handles.push(s.spawn(move || {
                    let guard = domain.guard();
                    let epoch_when_taken = guard.epoch();
                    readers_started.fetch_add(1, Ordering::SeqCst);
                    // Wait until retirer signals we may drop the guard.
                    while allow_readers_finish.load(Ordering::SeqCst) == 0 {
                        thread::sleep(Duration::from_millis(1));
                    }
                    epoch_when_taken
                }));
            }

            // Spawn retirer.
            let retirer = {
                let domain = domain.clone();
                let live = live.clone();
                let readers_started = readers_started.clone();
                let allow_readers_finish = allow_readers_finish.clone();
                s.spawn(move || {
                    // Wait until every reader has entered its critical region.
                    while readers_started.load(Ordering::SeqCst) < N_READERS {
                        thread::sleep(Duration::from_millis(1));
                    }

                    // Retire N objects at epoch 0.
                    for _ in 0..N_RETIRED {
                        domain.retire(DropSpy::new(live.clone()));
                    }
                    assert_eq!(live.load(Ordering::SeqCst), N_RETIRED);

                    // With readers still active at epoch 0, try_reclaim
                    // without advance must NOT drop anything.
                    let pre = domain.try_reclaim();
                    assert_eq!(pre, 0, "nothing should reclaim pre-advance");
                    assert_eq!(live.load(Ordering::SeqCst), N_RETIRED);

                    // Advance epoch. Readers' guards remain at epoch 0.
                    domain.advance_epoch();

                    // Release the readers. They will drop their guards.
                    allow_readers_finish.store(1, Ordering::SeqCst);
                })
            };

            // Join readers first so all guards are dropped.
            let reader_epochs: Vec<u64> = reader_handles
                .into_iter()
                .map(|h| h.join().expect("reader thread panicked"))
                .collect();
            retirer.join().expect("retirer thread panicked");

            // All readers should have observed epoch 0.
            for e in &reader_epochs {
                assert_eq!(*e, 0, "reader saw unexpected epoch {e}");
            }

            // All guards dropped + epoch advanced → reclaim everything.
            let reclaimed = domain.try_reclaim();
            assert_eq!(reclaimed, N_RETIRED);
            assert_eq!(live.load(Ordering::SeqCst), 0);
            assert_eq!(domain.pending_retired_count(), 0);
        });
    }

    #[test]
    fn mixed_epoch_retirement_is_partial() {
        // Retire two batches at different epochs; advancing only once should
        // reclaim only the first batch.
        let domain = QsbrDomain::new();
        let live = Arc::new(AtomicUsize::new(0));

        // Batch at epoch 0.
        domain.retire(DropSpy::new(live.clone()));
        domain.retire(DropSpy::new(live.clone()));
        assert_eq!(live.load(Ordering::SeqCst), 2);

        domain.advance_epoch(); // now at epoch 1

        // Batch at epoch 1.
        domain.retire(DropSpy::new(live.clone()));
        assert_eq!(live.load(Ordering::SeqCst), 3);

        // try_reclaim sees current=1. Entries with epoch<1 (the first batch)
        // drop. Entries with epoch==1 (the second batch) stay.
        let reclaimed = domain.try_reclaim();
        assert_eq!(reclaimed, 2);
        assert_eq!(live.load(Ordering::SeqCst), 1);
        assert_eq!(domain.pending_retired_count(), 1);

        domain.advance_epoch(); // now at epoch 2
        let reclaimed2 = domain.try_reclaim();
        assert_eq!(reclaimed2, 1);
        assert_eq!(live.load(Ordering::SeqCst), 0);
    }
}
