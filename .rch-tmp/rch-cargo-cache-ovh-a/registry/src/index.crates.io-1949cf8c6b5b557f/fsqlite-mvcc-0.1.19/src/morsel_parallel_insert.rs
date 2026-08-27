//! Morsel-driven parallel bulk INSERT primitive (AG-3B / IMPL-24 follow-up).
//!
//! MT-bench shows bulk `INSERT ... VALUES (...)` and `INSERT ... SELECT`
//! serializing on a single writer thread even when the underlying workload is
//! embarrassingly parallel (contiguous rowid ranges, no FK fan-out). The
//! morsel-driven execution model from Leis et al. ("Morsel-Driven Parallelism:
//! A NUMA-Aware Query Evaluation Framework for the Many-Core Age", SIGMOD
//! 2014) splits the row stream into fixed-size *morsels* and round-robins
//! them across workers so each worker operates on a cache-friendly batch.
//!
//! This module provides the **scheduling primitive only** — it is deliberately
//! not wired into any execution path. Actually executing the morsels requires
//! per-worker storage cursors, FK/trigger replay, and MVCC handoff that live
//! elsewhere. Keeping the split + schedule surface standalone makes it easy
//! to unit-test rowid assignment and round-robin distribution in isolation,
//! and to swap worker-selection strategies later (e.g. steal-based scheduling,
//! NUMA-aware pinning).
//!
//! # Contract
//! * [`MorselScheduler::split_into_morsels`] takes a row stream, starting
//!   rowid, and target root page; produces `Morsel`s with contiguous
//!   non-overlapping rowid ranges. The caller must pre-reserve
//!   `[start_rowid, start_rowid + total_rows)` via the rowid allocator.
//! * [`MorselScheduler::schedule`] round-robins morsels to worker indices:
//!   morsel `i` lands on worker `i % worker_count`. Empty inputs produce
//!   empty outputs without allocating per-worker bins.

use fsqlite_types::value::SqliteValue;

/// A contiguous batch of rows destined for a single table root page, with a
/// pre-reserved rowid range `[start_rowid, start_rowid + rows.len())`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Morsel {
    /// B-tree root page of the target table.
    pub table_root: i32,
    /// Rows to insert, in input order. Each inner `Vec<SqliteValue>` is a
    /// single record's column values in table-column order.
    pub rows: Vec<Vec<SqliteValue>>,
    /// Rowid of the first row in this morsel. Subsequent rows occupy
    /// `start_rowid + 1`, `start_rowid + 2`, ... contiguously.
    pub start_rowid: i64,
}

/// Round-robin work assignment for a single worker thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkAssignment {
    /// Zero-based worker index in `[0, worker_count)`.
    pub worker_id: usize,
    /// Morsels assigned to this worker, preserving the original morsel order
    /// modulo the round-robin stride.
    pub morsels: Vec<Morsel>,
}

/// Splits bulk-insert row streams into morsels and round-robins them across
/// a fixed pool of worker indices.
///
/// The scheduler is a plain value type — construction is `O(1)` and it holds
/// no resources, so callers typically create one per bulk-insert statement.
#[derive(Debug, Clone, Copy)]
pub struct MorselScheduler {
    /// Number of worker lanes to distribute morsels across. Must be `>= 1`;
    /// `new` clamps `0` up to `1` to avoid a modulo-by-zero pitfall for
    /// callers that compute this from thread-pool size.
    pub worker_count: usize,
    /// Target rows per morsel. Must be `>= 1`; `new` clamps `0` up to `1` so
    /// that `split_into_morsels` always makes progress.
    pub morsel_size: usize,
}

impl MorselScheduler {
    /// Build a new scheduler. `worker_count` and `morsel_size` are both
    /// clamped to a minimum of `1` so the split / schedule loops always
    /// terminate even when a caller passes `0` by accident (e.g. from an
    /// uninitialized thread-pool size).
    #[must_use]
    pub fn new(worker_count: usize, morsel_size: usize) -> Self {
        Self {
            worker_count: worker_count.max(1),
            morsel_size: morsel_size.max(1),
        }
    }

    /// Split a row iterator into morsels of up to `self.morsel_size` rows
    /// each, assigning each morsel a contiguous rowid range starting at
    /// `start_rowid`.
    ///
    /// The caller must ensure `start_rowid + total_rows` does not overflow
    /// `i64`; SQLite's rowid space is bounded by `i64::MAX` and the rowid
    /// allocator already enforces that bound upstream, so this primitive
    /// does not re-validate it.
    ///
    /// An empty input produces an empty output without allocating a stub
    /// morsel.
    pub fn split_into_morsels<I>(&self, table_root: i32, start_rowid: i64, rows: I) -> Vec<Morsel>
    where
        I: IntoIterator<Item = Vec<SqliteValue>>,
    {
        let iter = rows.into_iter();
        // Pre-size the output vector using the iterator's lower-bound size
        // hint to avoid "one reallocation per morsel" on large inputs.
        let expected = iter.size_hint().0.div_ceil(self.morsel_size);
        let mut morsels: Vec<Morsel> = Vec::with_capacity(expected);
        let mut buf: Vec<Vec<SqliteValue>> = Vec::with_capacity(self.morsel_size);
        let mut next_rowid = start_rowid;

        for row in iter {
            buf.push(row);
            if buf.len() >= self.morsel_size {
                let len = buf.len() as i64;
                morsels.push(Morsel {
                    table_root,
                    rows: std::mem::replace(&mut buf, Vec::with_capacity(self.morsel_size)),
                    start_rowid: next_rowid,
                });
                next_rowid = next_rowid.saturating_add(len);
            }
        }

        if !buf.is_empty() {
            morsels.push(Morsel {
                table_root,
                rows: buf,
                start_rowid: next_rowid,
            });
        }

        morsels
    }

    /// Round-robin the morsels across `self.worker_count` workers. Morsel
    /// `i` lands on worker `i % worker_count`. Workers that receive no
    /// morsels (possible when `morsels.len() < worker_count`) are omitted
    /// from the returned vector so callers can iterate directly without a
    /// per-worker empty-check.
    #[must_use]
    pub fn schedule(&self, morsels: Vec<Morsel>) -> Vec<WorkAssignment> {
        if morsels.is_empty() {
            return Vec::new();
        }
        let worker_count = self.worker_count;
        let active = worker_count.min(morsels.len());
        let mut bins: Vec<Vec<Morsel>> = (0..active).map(|_| Vec::new()).collect();
        // With `active = min(worker_count, morsels.len())`, every residue
        // `idx % worker_count` produced by the loop below is guaranteed to be
        // in `0..active`, so direct indexing is safe without bounds logic.
        for (idx, morsel) in morsels.into_iter().enumerate() {
            bins[idx % worker_count].push(morsel);
        }
        bins.into_iter()
            .enumerate()
            .map(|(worker_id, morsels)| WorkAssignment { worker_id, morsels })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_row(rowid_hint: i64) -> Vec<SqliteValue> {
        vec![
            SqliteValue::Integer(rowid_hint),
            SqliteValue::Text(format!("r{rowid_hint}").into()),
        ]
    }

    #[test]
    fn split_100_rows_into_5_morsels_of_20() {
        let scheduler = MorselScheduler::new(4, 20);
        let rows: Vec<Vec<SqliteValue>> = (0..100i64).map(make_row).collect();
        let morsels = scheduler.split_into_morsels(42, 1000, rows);

        assert_eq!(morsels.len(), 5, "100 rows / 20 per morsel = 5 morsels");
        for (i, m) in morsels.iter().enumerate() {
            assert_eq!(m.table_root, 42);
            assert_eq!(m.rows.len(), 20, "morsel {i} should hold exactly 20 rows");
            let expected_start = 1000 + (i as i64) * 20;
            assert_eq!(
                m.start_rowid, expected_start,
                "morsel {i} start_rowid should be contiguous ({expected_start})"
            );
        }
        // Contiguity end-to-end: last morsel ends at 1000 + 100 - 1 == 1099.
        let last = morsels.last().unwrap();
        assert_eq!(last.start_rowid + last.rows.len() as i64 - 1, 1099);
    }

    #[test]
    fn split_17_rows_with_morsel_size_20_produces_one_partial_morsel() {
        let scheduler = MorselScheduler::new(4, 20);
        let rows: Vec<Vec<SqliteValue>> = (0..17i64).map(make_row).collect();
        let morsels = scheduler.split_into_morsels(7, 500, rows);

        assert_eq!(morsels.len(), 1);
        assert_eq!(morsels[0].table_root, 7);
        assert_eq!(morsels[0].rows.len(), 17);
        assert_eq!(morsels[0].start_rowid, 500);
    }

    #[test]
    fn schedule_round_robin_10_morsels_across_4_workers() {
        let scheduler = MorselScheduler::new(4, 20);
        let morsels: Vec<Morsel> = (0..10i64)
            .map(|i| Morsel {
                table_root: 1,
                rows: vec![make_row(i)],
                start_rowid: i,
            })
            .collect();
        let assignments = scheduler.schedule(morsels);
        assert_eq!(assignments.len(), 4);
        // Round-robin: w0=[0,4,8], w1=[1,5,9], w2=[2,6], w3=[3,7].
        let expected: [&[i64]; 4] = [&[0, 4, 8], &[1, 5, 9], &[2, 6], &[3, 7]];
        for assignment in &assignments {
            let got: Vec<i64> = assignment.morsels.iter().map(|m| m.start_rowid).collect();
            assert_eq!(got, expected[assignment.worker_id].to_vec());
        }
    }

    #[test]
    fn empty_input_produces_empty_output_without_panic() {
        let scheduler = MorselScheduler::new(4, 20);
        let empty: Vec<Vec<SqliteValue>> = Vec::new();

        let morsels = scheduler.split_into_morsels(1, 0, empty);
        assert!(morsels.is_empty(), "empty row input -> no morsels");

        let assignments = scheduler.schedule(morsels);
        assert!(assignments.is_empty(), "no morsels -> no work assignments");
    }

    #[test]
    fn clamps_zero_worker_count_and_morsel_size() {
        // Defensive: exercises the `.max(1)` guards so a future refactor that
        // drops them will fail a test instead of dividing by zero at runtime.
        let scheduler = MorselScheduler::new(0, 0);
        assert_eq!(scheduler.worker_count, 1);
        assert_eq!(scheduler.morsel_size, 1);

        let rows: Vec<Vec<SqliteValue>> = (0..3i64).map(make_row).collect();
        let morsels = scheduler.split_into_morsels(9, 100, rows);
        assert_eq!(morsels.len(), 3, "morsel_size=1 -> one morsel per row");
        assert_eq!(morsels[0].start_rowid, 100);
        assert_eq!(morsels[1].start_rowid, 101);
        assert_eq!(morsels[2].start_rowid, 102);
    }
}
