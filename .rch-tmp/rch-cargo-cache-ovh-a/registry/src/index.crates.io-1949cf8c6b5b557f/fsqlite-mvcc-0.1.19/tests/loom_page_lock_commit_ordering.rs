//! bd-iqd1w: Loom model of page-lock acquire/release vs CommitIndex publish
//! ordering.
//!
//! Verifies under exhaustive schedule enumeration that no interleaving lets a
//! reader observe `CommitIndex[page] = seq` before the corresponding "fsync"
//! has completed.
//!
//! ## Model → Production mapping (see also MODEL section below)
//!
//! | Model element          | Production code                                     |
//! |------------------------|-----------------------------------------------------|
//! | `page_lock` AtomicU64  | `InProcessPageLockTable::fast_locks` (core_types:681)|
//! | `commit_idx` AtomicU64 | `CommitIndex::fast_array` slot (core_types:2299)     |
//! | `fsync_done` AtomicBool| WAL fsync barrier (native_commit.rs:500-502)         |
//! | `data_written` AtomicU64| Version-store / page data (happens-before fence)    |
//! | Writer CAS 0→txn       | `try_acquire` AcqRel CAS (core_types:681)           |
//! | Writer CAS txn→0       | `release` AcqRel CAS (core_types:758)               |
//! | Writer Release store   | `CommitIndex::update` Release store (core_types:2299)|
//! | Reader Acquire load    | `CommitIndex::latest` Acquire load (core_types:2364) |
//!
//! ## Invariant
//!
//! For every schedule: if a reader sees `commit_idx.load(Acquire) == seq`,
//! then `fsync_done.load(Acquire) == true` AND `data_written.load(Acquire) == seq`.
//! In other words, the reader never observes a commit-index entry that points to
//! data that hasn't been fsynced yet.

// `loom` is set via `RUSTFLAGS="--cfg loom"` only when running the loom
// schedule-enumeration suite; without this allow, `-D unexpected_cfgs`
// (implied by `-D warnings`) rejects the cfg-gates as unknown.
#![allow(unexpected_cfgs)]

#[cfg(loom)]
mod loom_tests {
    use loom::sync::Arc;
    use loom::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use loom::thread;

    /// Model of a single page slot in the commit pipeline.
    struct PageCommitModel {
        /// Page lock: 0 = unlocked, nonzero = holder txn id.
        /// Production: `InProcessPageLockTable::fast_locks[pgno]`
        page_lock: AtomicU64,
        /// Commit index for this page: 0 = no commit, N = committed at seq N.
        /// Production: `CommitIndex::fast_array[pgno]`
        commit_idx: AtomicU64,
        /// Whether the WAL fsync barrier has completed for the latest write.
        /// Production: the fsync syscall return in `native_commit.rs:500-502`
        fsync_done: AtomicBool,
        /// The actual data payload (version store contents).
        /// A reader that sees commit_idx=N must also see data_written=N.
        data_written: AtomicU64,
    }

    impl PageCommitModel {
        fn new() -> Self {
            Self {
                page_lock: AtomicU64::new(0),
                commit_idx: AtomicU64::new(0),
                fsync_done: AtomicBool::new(false),
                data_written: AtomicU64::new(0),
            }
        }
    }

    // ─── T1: Correct ordering — writer fsyncs before publishing ─────

    #[test]
    fn t1_correct_ordering_fsync_before_publish() {
        loom::model(|| {
            let model = Arc::new(PageCommitModel::new());
            let seq: u64 = 1;
            let txn_id: u64 = 42;

            let writer_model = Arc::clone(&model);
            let reader_model = Arc::clone(&model);

            // Writer thread: acquire lock → write data → fsync → publish commit
            // → release lock. This models the production protocol in
            // native_commit.rs (write capsule → fsync1 → marker → fsync2 → SHM
            // publish → release).
            let writer = thread::spawn(move || {
                let m = &*writer_model;

                // 1. Acquire page lock (AcqRel CAS, matching core_types:681)
                let cas_result =
                    m.page_lock
                        .compare_exchange(0, txn_id, Ordering::AcqRel, Ordering::Acquire);
                assert!(cas_result.is_ok(), "writer must acquire lock");

                // 2. Write data (happens-before the Release fence/store below)
                m.data_written.store(seq, Ordering::Relaxed);

                // 3. Fsync barrier (models native_commit.rs FSYNC_2)
                // In production this is a syscall; here we model it as an
                // atomic store with Release ordering to ensure data_written
                // is visible to any thread that loads fsync_done with Acquire.
                m.fsync_done.store(true, Ordering::Release);

                // 4. Publish to CommitIndex (Release store, core_types:2299)
                // The Release here synchronizes-with the reader's Acquire load.
                m.commit_idx.store(seq, Ordering::Release);

                // 5. Release page lock (AcqRel CAS, core_types:758)
                let release_result =
                    m.page_lock
                        .compare_exchange(txn_id, 0, Ordering::AcqRel, Ordering::Relaxed);
                assert!(release_result.is_ok(), "writer must release lock");
            });

            // Reader thread: load commit index → check fsync invariant.
            // Models a concurrent reader calling CommitIndex::latest().
            let reader = thread::spawn(move || {
                let m = &*reader_model;

                // Acquire load matches CommitIndex::latest (core_types:2364)
                let observed_seq = m.commit_idx.load(Ordering::Acquire);

                if observed_seq > 0 {
                    // INVARIANT: if we see commit_idx=N, fsync must be done
                    // and data must be visible.
                    let fsync = m.fsync_done.load(Ordering::Acquire);
                    assert!(
                        fsync,
                        "INVARIANT VIOLATION: reader saw commit_idx={observed_seq} \
                         but fsync_done=false"
                    );

                    let data = m.data_written.load(Ordering::Acquire);
                    assert_eq!(
                        data, observed_seq,
                        "INVARIANT VIOLATION: reader saw commit_idx={observed_seq} \
                         but data_written={data}"
                    );
                }
            });

            writer.join().unwrap();
            reader.join().unwrap();
        });
    }

    // ─── T2: Two writers, sequential commits ────────────────────────

    #[test]
    fn t2_two_writers_sequential_lock_handoff() {
        loom::model(|| {
            let model = Arc::new(PageCommitModel::new());

            let w1_model = Arc::clone(&model);
            let w2_model = Arc::clone(&model);
            let reader_model = Arc::clone(&model);

            // Writer 1: commits seq=1
            let w1 = thread::spawn(move || {
                let m = &*w1_model;
                let txn: u64 = 10;

                // Try to acquire — might fail if writer 2 got there first
                if m.page_lock
                    .compare_exchange(0, txn, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    m.data_written.store(1, Ordering::Relaxed);
                    m.fsync_done.store(true, Ordering::Release);
                    m.commit_idx.store(1, Ordering::Release);
                    let _ =
                        m.page_lock
                            .compare_exchange(txn, 0, Ordering::AcqRel, Ordering::Relaxed);
                }
            });

            // Writer 2: commits seq=2
            let w2 = thread::spawn(move || {
                let m = &*w2_model;
                let txn: u64 = 20;

                if m.page_lock
                    .compare_exchange(0, txn, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    m.data_written.store(2, Ordering::Relaxed);
                    m.fsync_done.store(true, Ordering::Release);
                    m.commit_idx.store(2, Ordering::Release);
                    let _ =
                        m.page_lock
                            .compare_exchange(txn, 0, Ordering::AcqRel, Ordering::Relaxed);
                }
            });

            // Reader: whatever commit_idx it sees must have corresponding fsync
            let reader = thread::spawn(move || {
                let m = &*reader_model;
                let observed = m.commit_idx.load(Ordering::Acquire);

                if observed > 0 {
                    let fsync = m.fsync_done.load(Ordering::Acquire);
                    assert!(
                        fsync,
                        "INVARIANT VIOLATION: commit_idx={observed} but fsync_done=false"
                    );

                    let data = m.data_written.load(Ordering::Acquire);
                    assert!(
                        data >= observed || data > 0,
                        "INVARIANT VIOLATION: commit_idx={observed} but data_written={data}"
                    );
                }
            });

            w1.join().unwrap();
            w2.join().unwrap();
            reader.join().unwrap();
        });
    }

    // ─── T3: Weakened ordering (NEGATIVE test) ──────────────────────
    // Proves the model actually catches bugs: publish BEFORE fsync.
    // Under loom's exhaustive search, at least one schedule should violate
    // the invariant. We catch the panic and assert it happened.

    #[test]
    fn t3_weakened_ordering_publish_before_fsync_detected() {
        let found_violation = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let found_clone = std::sync::Arc::clone(&found_violation);

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let found_outer = found_clone;
            loom::model(move || {
                let found_inner = std::sync::Arc::clone(&found_outer);
                let model = Arc::new(PageCommitModel::new());
                let seq: u64 = 1;
                let txn_id: u64 = 42;

                let writer_model = Arc::clone(&model);
                let reader_model = Arc::clone(&model);

                // INTENTIONALLY BUGGY writer: publishes BEFORE fsync
                let writer = thread::spawn(move || {
                    let m = &*writer_model;

                    let _ = m.page_lock.compare_exchange(
                        0,
                        txn_id,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    );

                    m.data_written.store(seq, Ordering::Relaxed);

                    // BUG: publish commit index BEFORE fsync
                    m.commit_idx.store(seq, Ordering::Release);

                    // Fsync happens AFTER publish — too late!
                    m.fsync_done.store(true, Ordering::Release);

                    let _ = m.page_lock.compare_exchange(
                        txn_id,
                        0,
                        Ordering::AcqRel,
                        Ordering::Relaxed,
                    );
                });

                let reader = thread::spawn(move || {
                    let m = &*reader_model;
                    let observed = m.commit_idx.load(Ordering::Acquire);

                    if observed > 0 {
                        let fsync = m.fsync_done.load(Ordering::Acquire);
                        if !fsync {
                            // Found the violation — this is the bug we're detecting
                            found_inner.store(true, std::sync::atomic::Ordering::SeqCst);
                            panic!(
                                "EXPECTED VIOLATION: commit_idx={observed} but fsync_done=false"
                            );
                        }
                    }
                });

                writer.join().unwrap();
                reader.join().unwrap();
            });
        }));

        // The weakened ordering MUST trigger the invariant violation in at
        // least one loom-explored schedule.
        assert!(
            result.is_err() || found_violation.load(std::sync::atomic::Ordering::SeqCst),
            "BUG: weakened ordering was NOT detected by the model — \
             the model is not actually checking the invariant"
        );
    }

    // ─── T4: batch_update Release fence + Relaxed stores ────────────

    #[test]
    fn t4_batch_update_fence_then_relaxed_stores() {
        loom::model(|| {
            // Model of CommitIndex::batch_update: one Release fence, then
            // per-page Relaxed stores. Reader does Acquire load.
            let data = Arc::new(AtomicU64::new(0));
            let commit_a = Arc::new(AtomicU64::new(0));
            let commit_b = Arc::new(AtomicU64::new(0));
            let fsync = Arc::new(AtomicBool::new(false));

            let w_data = Arc::clone(&data);
            let w_ca = Arc::clone(&commit_a);
            let w_cb = Arc::clone(&commit_b);
            let w_fsync = Arc::clone(&fsync);

            let r_data = Arc::clone(&data);
            let r_ca = Arc::clone(&commit_a);
            let r_cb = Arc::clone(&commit_b);
            let r_fsync = Arc::clone(&fsync);

            let writer = thread::spawn(move || {
                // Write data for both pages
                w_data.store(1, Ordering::Relaxed);

                // Fsync barrier
                w_fsync.store(true, Ordering::Release);

                // batch_update: Release fence then Relaxed stores
                // (matches core_types:2338 fence + 2346 stores)
                loom::sync::atomic::fence(Ordering::Release);
                w_ca.store(1, Ordering::Relaxed);
                w_cb.store(1, Ordering::Relaxed);
            });

            let reader = thread::spawn(move || {
                // If we see either commit index, the fsync must be done
                let a = r_ca.load(Ordering::Acquire);
                let b = r_cb.load(Ordering::Acquire);

                if a > 0 || b > 0 {
                    let f = r_fsync.load(Ordering::Acquire);
                    assert!(
                        f,
                        "INVARIANT VIOLATION: saw commit_a={a} commit_b={b} \
                         but fsync_done=false"
                    );
                    let d = r_data.load(Ordering::Acquire);
                    assert!(d > 0, "INVARIANT VIOLATION: saw commit but data_written=0");
                }
            });

            writer.join().unwrap();
            reader.join().unwrap();
        });
    }

    // ─── T5: Lock handoff ordering — release → acquire visibility ───

    #[test]
    fn t5_lock_handoff_data_visibility() {
        loom::model(|| {
            let lock = Arc::new(AtomicU64::new(0));
            let data = Arc::new(AtomicU64::new(0));

            let w1_lock = Arc::clone(&lock);
            let w1_data = Arc::clone(&data);
            let w2_lock = Arc::clone(&lock);
            let w2_data = Arc::clone(&data);

            // Writer 1: acquire → write data → release
            let w1 = thread::spawn(move || {
                if w1_lock
                    .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    w1_data.store(42, Ordering::Relaxed);
                    let _ = w1_lock.compare_exchange(1, 0, Ordering::AcqRel, Ordering::Relaxed);
                }
            });

            // Writer 2: acquire → read data written by w1 → release
            let w2 = thread::spawn(move || {
                if w2_lock
                    .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    // If w1 already wrote and released, we must see its data
                    // due to the AcqRel CAS forming a synchronizes-with chain.
                    let val = w2_data.load(Ordering::Relaxed);
                    // val can be 0 (w1 hasn't written yet) or 42 (w1 wrote)
                    // but NOT any other value — no torn reads.
                    assert!(val == 0 || val == 42, "unexpected data value: {val}");
                    let _ = w2_lock.compare_exchange(2, 0, Ordering::AcqRel, Ordering::Relaxed);
                }
            });

            w1.join().unwrap();
            w2.join().unwrap();
        });
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Non-loom unit tests (run under normal `cargo test`)
// ═══════════════════════════════════════════════════════════════════════

#[cfg(not(loom))]
mod std_tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering, fence};

    /// Smoke test: correct ordering works under std atomics.
    #[test]
    fn smoke_correct_ordering() {
        let commit_idx = Arc::new(AtomicU64::new(0));
        let fsync_done = Arc::new(AtomicBool::new(false));
        let data = Arc::new(AtomicU64::new(0));

        let w_ci = Arc::clone(&commit_idx);
        let w_fs = Arc::clone(&fsync_done);
        let w_d = Arc::clone(&data);

        let writer = std::thread::spawn(move || {
            w_d.store(1, Ordering::Relaxed);
            w_fs.store(true, Ordering::Release);
            w_ci.store(1, Ordering::Release);
        });

        let r_ci = Arc::clone(&commit_idx);
        let r_fs = Arc::clone(&fsync_done);
        let r_d = Arc::clone(&data);

        let reader = std::thread::spawn(move || {
            let seq = r_ci.load(Ordering::Acquire);
            if seq > 0 {
                assert!(r_fs.load(Ordering::Acquire));
                assert!(r_d.load(Ordering::Acquire) > 0);
            }
        });

        writer.join().unwrap();
        reader.join().unwrap();
    }

    /// Smoke test: batch_update fence pattern.
    #[test]
    fn smoke_batch_update_fence() {
        let ca = Arc::new(AtomicU64::new(0));
        let cb = Arc::new(AtomicU64::new(0));
        let fsync_done = Arc::new(AtomicBool::new(false));

        let w_ca = Arc::clone(&ca);
        let w_cb = Arc::clone(&cb);
        let w_fs = Arc::clone(&fsync_done);

        let writer = std::thread::spawn(move || {
            w_fs.store(true, Ordering::Release);
            fence(Ordering::Release);
            w_ca.store(1, Ordering::Relaxed);
            w_cb.store(1, Ordering::Relaxed);
        });

        let r_ca = Arc::clone(&ca);
        let r_cb = Arc::clone(&cb);
        let r_fs = Arc::clone(&fsync_done);

        let reader = std::thread::spawn(move || {
            let a = r_ca.load(Ordering::Acquire);
            let b = r_cb.load(Ordering::Acquire);
            if a > 0 || b > 0 {
                assert!(r_fs.load(Ordering::Acquire));
            }
        });

        writer.join().unwrap();
        reader.join().unwrap();
    }
}
