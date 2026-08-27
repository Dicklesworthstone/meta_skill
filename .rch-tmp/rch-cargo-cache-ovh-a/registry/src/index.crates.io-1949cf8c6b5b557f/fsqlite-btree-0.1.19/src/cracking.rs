//! Database Cracking / Adaptive Indexing (§8.8)
//!
//! Implements database cracking (Idreos et al. 2007) for zero-admin indexing.
//! Each range query incrementally partitions the column data so that
//! frequently queried ranges converge toward sorted order. After Q queries
//! on a column of N elements, lookup cost decreases from O(N) to O(N/Q).
//!
//! # Design
//!
//! - `CrackedColumn<T: Ord + Copy>` wraps a `Vec<T>` with two crack indexes:
//!   a *lower* index (position where elements >= k begin) and an *upper*
//!   index (position where elements > k begin).
//! - On a range query `[lo, hi]`, the column partitions at `lo` (lower)
//!   and `hi` (upper) to isolate the matching elements into a contiguous slice.
//! - Crack boundaries accumulate across queries so that hot ranges become
//!   progressively cheaper to scan.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

// ── Metrics ──────────────────────────────────────────────────────────────

static CRACK_OPS_TOTAL: AtomicU64 = AtomicU64::new(0);
static CRACK_ELEMENTS_PARTITIONED_TOTAL: AtomicU64 = AtomicU64::new(0);
static CRACK_QUERIES_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Snapshot of cracking metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CrackingMetricsSnapshot {
    /// Total crack (partition) operations performed.
    pub crack_ops_total: u64,
    /// Total elements moved by partitioning.
    pub elements_partitioned_total: u64,
    /// Total range queries served.
    pub queries_total: u64,
}

impl fmt::Display for CrackingMetricsSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "crack_ops={} elements_partitioned={} queries={}",
            self.crack_ops_total, self.elements_partitioned_total, self.queries_total,
        )
    }
}

/// Return a snapshot of cracking metrics.
#[must_use]
pub fn cracking_metrics_snapshot() -> CrackingMetricsSnapshot {
    CrackingMetricsSnapshot {
        crack_ops_total: CRACK_OPS_TOTAL.load(Ordering::Relaxed),
        elements_partitioned_total: CRACK_ELEMENTS_PARTITIONED_TOTAL.load(Ordering::Relaxed),
        queries_total: CRACK_QUERIES_TOTAL.load(Ordering::Relaxed),
    }
}

/// Reset cracking metrics to zero.
pub fn reset_cracking_metrics() {
    CRACK_OPS_TOTAL.store(0, Ordering::Relaxed);
    CRACK_ELEMENTS_PARTITIONED_TOTAL.store(0, Ordering::Relaxed);
    CRACK_QUERIES_TOTAL.store(0, Ordering::Relaxed);
}

fn record_crack_op(elements: usize) {
    CRACK_OPS_TOTAL.fetch_add(1, Ordering::Relaxed);
    CRACK_ELEMENTS_PARTITIONED_TOTAL.fetch_add(elements as u64, Ordering::Relaxed);
}

fn record_query() {
    CRACK_QUERIES_TOTAL.fetch_add(1, Ordering::Relaxed);
}

// ── CrackedColumn ────────────────────────────────────────────────────────

/// A column that adaptively indexes itself through cracking.
///
/// Range queries on the column cause in-place partitioning at the query
/// boundaries. Over time, frequently queried ranges become fully sorted.
pub struct CrackedColumn<T> {
    /// The underlying data. Elements are rearranged in-place by crack ops.
    data: Vec<T>,
    /// Lower crack index: maps key k -> position p where elements >= k begin.
    /// data[..p] contains elements < k (within the segment).
    lower_cracks: BTreeMap<T, usize>,
    /// Upper crack index: maps key k -> position p where elements > k begin.
    /// data[..p] contains elements <= k (within the segment).
    upper_cracks: BTreeMap<T, usize>,
    /// All partition positions for fast segment boundary lookup.
    positions: BTreeSet<usize>,
}

impl<T: Ord + Copy + fmt::Debug> CrackedColumn<T> {
    /// Create a new cracked column from unordered data.
    pub fn new(data: Vec<T>) -> Self {
        Self {
            data,
            lower_cracks: BTreeMap::new(),
            upper_cracks: BTreeMap::new(),
            positions: BTreeSet::new(),
        }
    }

    /// Return the total number of elements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Return whether the column is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Return the number of crack boundaries (lower + upper combined).
    #[must_use]
    pub fn num_cracks(&self) -> usize {
        self.positions.len()
    }

    /// Return a reference to the underlying data slice (current physical order).
    #[must_use]
    pub fn data(&self) -> &[T] {
        &self.data
    }

    /// Perform a lower crack at the given pivot value.
    ///
    /// Partitions: elements < pivot | elements >= pivot.
    /// Returns the position where elements >= pivot begin.
    fn crack_lower(&mut self, pivot: T) -> usize {
        if let Some(&pos) = self.lower_cracks.get(&pivot) {
            return pos;
        }

        if self.data.is_empty() {
            self.lower_cracks.insert(pivot, 0);
            return 0;
        }

        // Find which segment the pivot belongs to by scanning for where
        // the pivot value currently resides.
        let (seg_start, seg_end) = self.find_segment_for_value_lower(&pivot);

        if seg_start >= seg_end {
            let pos = seg_start.min(self.data.len());
            self.lower_cracks.insert(pivot, pos);
            self.positions.insert(pos);
            return pos;
        }

        let segment = &mut self.data[seg_start..seg_end];
        let seg_len = segment.len();

        // Partition: elements < pivot go left, elements >= pivot go right.
        let pp = partition_lower(segment, pivot);
        let absolute_pos = seg_start + pp;

        record_crack_op(seg_len);

        self.lower_cracks.insert(pivot, absolute_pos);
        self.positions.insert(absolute_pos);
        absolute_pos
    }

    /// Perform an upper crack at the given pivot value.
    ///
    /// Partitions: elements <= pivot | elements > pivot.
    /// Returns the position where elements > pivot begin.
    fn crack_upper(&mut self, pivot: T) -> usize {
        if let Some(&pos) = self.upper_cracks.get(&pivot) {
            return pos;
        }

        if self.data.is_empty() {
            self.upper_cracks.insert(pivot, 0);
            return 0;
        }

        let (seg_start, seg_end) = self.find_segment_for_value_upper(&pivot);

        if seg_start >= seg_end {
            let pos = seg_start.min(self.data.len());
            self.upper_cracks.insert(pivot, pos);
            self.positions.insert(pos);
            return pos;
        }

        let segment = &mut self.data[seg_start..seg_end];
        let seg_len = segment.len();

        // Partition: elements <= pivot go left, elements > pivot go right.
        let pp = partition_upper(segment, pivot);
        let absolute_pos = seg_start + pp;

        record_crack_op(seg_len);

        self.upper_cracks.insert(pivot, absolute_pos);
        self.positions.insert(absolute_pos);
        absolute_pos
    }

    /// Find the segment containing the value for a lower crack.
    /// Uses existing crack boundaries to narrow the search.
    fn find_segment_for_value_lower(&self, value: &T) -> (usize, usize) {
        // Start: position of the tightest lower bound we know.
        // Look for the greatest key <= value in lower_cracks.
        let start_from_lower = self
            .lower_cracks
            .range(..=*value)
            .next_back()
            .map(|(_, &pos)| pos);
        // Also check upper_cracks for keys < value (elements <= k are before pos).
        let start_from_upper = self
            .upper_cracks
            .range(..*value)
            .next_back()
            .map(|(_, &pos)| pos);

        let seg_start = start_from_lower
            .into_iter()
            .chain(start_from_upper)
            .max()
            .unwrap_or(0);

        // End: position of the tightest upper bound.
        // Look for the least key > value in lower_cracks.
        let end_from_lower = self
            .lower_cracks
            .range(std::ops::RangeFrom { start: *value })
            .find(|&(&k, _)| k > *value)
            .map(|(_, &pos)| pos);
        // Also check upper_cracks for keys >= value.
        let end_from_upper = self
            .upper_cracks
            .range(*value..)
            .next()
            .map(|(_, &pos)| pos);

        let seg_end = end_from_lower
            .into_iter()
            .chain(end_from_upper)
            .min()
            .unwrap_or(self.data.len());

        (seg_start, seg_end.max(seg_start))
    }

    /// Find the segment containing the value for an upper crack.
    fn find_segment_for_value_upper(&self, value: &T) -> (usize, usize) {
        // Start: position of the tightest lower bound.
        let start_from_lower = self
            .lower_cracks
            .range(..=*value)
            .next_back()
            .map(|(_, &pos)| pos);
        let start_from_upper = self
            .upper_cracks
            .range(..*value)
            .next_back()
            .map(|(_, &pos)| pos);

        let seg_start = start_from_lower
            .into_iter()
            .chain(start_from_upper)
            .max()
            .unwrap_or(0);

        // End: position of the tightest upper bound.
        let end_from_lower = self
            .lower_cracks
            .range(std::ops::RangeFrom { start: *value })
            .find(|&(&k, _)| k > *value)
            .map(|(_, &pos)| pos);
        let end_from_upper = self
            .upper_cracks
            .range(std::ops::RangeFrom { start: *value })
            .find(|&(&k, _)| k > *value)
            .map(|(_, &pos)| pos);

        let seg_end = end_from_lower
            .into_iter()
            .chain(end_from_upper)
            .min()
            .unwrap_or(self.data.len());

        (seg_start, seg_end.max(seg_start))
    }

    /// Execute a range query [lo, hi] (inclusive on both ends).
    ///
    /// This cracks the column at `lo` (lower: {< lo | >= lo}) and at
    /// `hi` (upper: {<= hi | > hi}), then returns the contiguous slice
    /// of elements in [lo, hi].
    pub fn range_query(&mut self, lo: T, hi: T) -> &[T] {
        assert!(lo <= hi, "range_query: lo must be <= hi");
        record_query();

        // Crack lower at lo: separates elements < lo from elements >= lo.
        let pos_lo = self.crack_lower(lo);
        // Crack upper at hi: separates elements <= hi from elements > hi.
        let pos_hi = self.crack_upper(hi);

        &self.data[pos_lo..pos_hi]
    }

    /// Execute a point query for a single value.
    ///
    /// Returns the count of elements equal to the value.
    pub fn point_query(&mut self, value: T) -> usize {
        let result = self.range_query(value, value);
        result.iter().filter(|&&v| v == value).count()
    }

    /// Scan the entire column, returning all elements (unordered).
    #[must_use]
    pub fn full_scan(&self) -> &[T] {
        &self.data
    }

    /// Check if the column has converged to fully sorted.
    #[must_use]
    pub fn is_fully_sorted(&self) -> bool {
        self.data.windows(2).all(|w| w[0] <= w[1])
    }

    /// Return the average segment size (total elements / number of segments).
    #[must_use]
    pub fn avg_segment_size(&self) -> f64 {
        if self.data.is_empty() {
            return 0.0;
        }
        let segments = self.positions.len() + 1;
        self.data.len() as f64 / segments as f64
    }
}

#[allow(clippy::missing_fields_in_debug)]
impl<T: Ord + Copy + fmt::Debug> fmt::Debug for CrackedColumn<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CrackedColumn")
            .field("len", &self.data.len())
            .field("num_cracks", &self.positions.len())
            .finish()
    }
}

/// Partition so that elements < pivot come first, elements >= pivot come after.
/// Returns the number of elements < pivot.
fn partition_lower<T: Ord + Copy>(slice: &mut [T], pivot: T) -> usize {
    let mut lo = 0;
    let mut hi = slice.len();
    while lo < hi {
        if slice[lo] < pivot {
            lo += 1;
        } else {
            hi -= 1;
            slice.swap(lo, hi);
        }
    }
    lo
}

/// Partition so that elements <= pivot come first, elements > pivot come after.
/// Returns the number of elements <= pivot.
fn partition_upper<T: Ord + Copy>(slice: &mut [T], pivot: T) -> usize {
    let mut lo = 0;
    let mut hi = slice.len();
    while lo < hi {
        if slice[lo] <= pivot {
            lo += 1;
        } else {
            hi -= 1;
            slice.swap(lo, hi);
        }
    }
    lo
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_crack_and_query() {
        let data = vec![9, 3, 7, 1, 5, 2, 8, 4, 6, 0];
        let mut col = CrackedColumn::new(data);
        assert_eq!(col.len(), 10);
        assert_eq!(col.num_cracks(), 0);

        let result = col.range_query(3, 6);
        let mut sorted_result: Vec<i32> = result.to_vec();
        sorted_result.sort_unstable();
        assert_eq!(sorted_result, vec![3, 4, 5, 6]);

        assert!(col.num_cracks() > 0);
    }

    #[test]
    fn progressive_refinement() {
        let data: Vec<u32> = (0..100).rev().collect();
        let mut col = CrackedColumn::new(data);

        let cracks_before = col.num_cracks();
        let _ = col.range_query(20, 40);
        assert!(col.num_cracks() > cracks_before);

        let result = col.range_query(20, 40);
        let mut sorted: Vec<u32> = result.to_vec();
        sorted.sort_unstable();
        let expected: Vec<u32> = (20..=40).collect();
        assert_eq!(sorted, expected);
    }

    #[test]
    fn point_query_works() {
        let data = vec![5, 3, 5, 1, 5, 2, 8, 4, 5, 0];
        let mut col = CrackedColumn::new(data);
        assert_eq!(col.point_query(5), 4);
        assert_eq!(col.point_query(0), 1);
        assert_eq!(col.point_query(99), 0);
    }

    #[test]
    fn empty_column() {
        let col: CrackedColumn<i32> = CrackedColumn::new(vec![]);
        assert!(col.is_empty());
        assert_eq!(col.len(), 0);
        assert!(col.is_fully_sorted());
        #[allow(clippy::float_cmp)]
        {
            assert_eq!(col.avg_segment_size(), 0.0);
        }
    }

    #[test]
    fn avg_segment_size_and_is_fully_sorted_metrics() {
        // avg_segment_size = len / (num_cracks + 1): a fresh column is a single
        // segment, and cracking shrinks the average. is_fully_sorted is a pure
        // ascending-order check. Neither metric was directly tested.

        // Empty column: zero average, vacuously sorted.
        let empty: CrackedColumn<i32> = CrackedColumn::new(vec![]);
        assert!((empty.avg_segment_size() - 0.0).abs() < f64::EPSILON);
        assert!(empty.is_fully_sorted());

        // A pre-sorted column reports fully sorted even before any cracking.
        let sorted = CrackedColumn::new(vec![1_i32, 2, 3, 4]);
        assert!(sorted.is_fully_sorted());

        // Unsorted fresh column: a single segment (avg == len), not yet sorted.
        let mut col = CrackedColumn::new(vec![5_i32, 1, 4, 2, 8, 3, 7, 6]);
        assert_eq!(col.num_cracks(), 0);
        assert!((col.avg_segment_size() - 8.0).abs() < f64::EPSILON);
        assert!(!col.is_fully_sorted());

        // A range query cracks the column into more segments, dropping the
        // average segment size below the full length.
        let _ = col.range_query(3, 6);
        assert!(col.num_cracks() > 0);
        assert!(col.avg_segment_size() < 8.0);
    }

    #[test]
    fn partition_functions() {
        let mut data = vec![5, 3, 8, 1, 7, 2];
        let p = partition_lower(&mut data, 5);
        assert!(data[..p].iter().all(|&x| x < 5));
        assert!(data[p..].iter().all(|&x| x >= 5));

        let mut data2 = vec![5, 3, 8, 1, 7, 2];
        let p2 = partition_upper(&mut data2, 5);
        assert!(data2[..p2].iter().all(|&x| x <= 5));
        assert!(data2[p2..].iter().all(|&x| x > 5));
    }

    #[test]
    fn partition_lower_and_upper_count_pivot_equal_elements_correctly() {
        // partition_functions checks the partition invariant but not the exact
        // returned count, nor the lower-vs-upper boundary: partition_lower sends
        // pivot-equal elements to the upper side (strictly <), while
        // partition_upper keeps them on the lower side (<=). With three 5s and a
        // lone 8, lower counts only {3,1} and upper counts everything but 8.
        let mut lower = vec![5, 3, 8, 5, 1, 5];
        assert_eq!(
            partition_lower(&mut lower, 5),
            2,
            "only 3 and 1 are strictly < 5"
        );

        let mut upper = vec![5, 3, 8, 5, 1, 5];
        assert_eq!(
            partition_upper(&mut upper, 5),
            5,
            "everything except the lone 8 is <= 5"
        );

        // Edge cases on partition_lower: all-below, none-below, and empty.
        assert_eq!(partition_lower(&mut vec![1, 2, 3], 5), 3);
        assert_eq!(partition_lower(&mut vec![5, 6, 7], 5), 0);
        assert_eq!(partition_lower(&mut Vec::<i32>::new(), 5), 0);
    }

    #[test]
    fn cracking_preserves_permutation_and_answers_every_range() {
        // Database cracking only REORGANIZES the column; after any sequence of
        // queries it must stay a permutation of the original (no element lost
        // or duplicated), and every range query must return exactly the in-range
        // elements -- including with duplicates and degenerate single-value ranges.
        let original: Vec<i32> = vec![9, 3, 7, 1, 5, 2, 8, 4, 6, 0, 9, 5, 1, 7, 3];
        let mut col = CrackedColumn::new(original.clone());

        let mut expected_sorted = original.clone();
        expected_sorted.sort_unstable();

        for (lo, hi) in [
            (3, 6),
            (0, 2),
            (5, 9),
            (1, 1),
            (7, 7),
            (0, 9),
            (4, 8),
            (2, 5),
        ] {
            let mut got: Vec<i32> = col.range_query(lo, hi).to_vec();
            got.sort_unstable();
            let mut want: Vec<i32> = original
                .iter()
                .copied()
                .filter(|x| (lo..=hi).contains(x))
                .collect();
            want.sort_unstable();
            assert_eq!(got, want, "range_query({lo},{hi}) returned the wrong set");
        }

        // The column is still a permutation of the original.
        let mut after: Vec<i32> = col.full_scan().to_vec();
        after.sort_unstable();
        assert_eq!(
            after, expected_sorted,
            "cracking must preserve the multiset"
        );
        assert_eq!(col.len(), original.len());
    }
}
