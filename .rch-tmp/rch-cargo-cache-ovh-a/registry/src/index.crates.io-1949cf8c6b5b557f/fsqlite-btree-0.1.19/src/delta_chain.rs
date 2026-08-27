//! BwTree-style delta chains for hot-row UPDATEs (IMPL-23 / AG-1B).
//!
//! Background
//! ----------
//! In a conventional row store, an UPDATE to an existing row means: locate the
//! row, rewrite the row's bytes in place, and maintain any affected indexes.
//! For hot rows — the same rowid updated many times in rapid succession — this
//! pays the full row-rewrite cost on every UPDATE.
//!
//! The BwTree (Levandoski, Lomet & Sengupta, ICDE 2013) introduced the idea of
//! staging each update as a small *delta node* that points back to the prior
//! version and records only the per-column change. Reads walk the chain from
//! the head toward the base, applying the most recent write for each column.
//! When the chain grows past a threshold, it is *consolidated* — materialized
//! into a fresh base row — amortizing the bookkeeping cost.
//!
//! Scope of this module
//! --------------------
//! This is the standalone delta-chain primitive plus tests. It is NOT wired
//! into the main UPDATE path: doing that safely requires a dedicated
//! eligibility gate (hot-row detection, per-rowid lock discipline, index
//! invalidation) that is tracked separately.
//!
//! Design notes
//! ------------
//! * `DeltaNode::prev` links toward older deltas and eventually terminates in
//!   `None`; the base row lives on `HotRowDeltaChain::base`.
//! * The chain's `head` is optional because a freshly-constructed chain has no
//!   deltas applied yet — `materialize` on an empty chain simply returns the
//!   base.
//! * All linkage uses `Arc` so the chain can be shared across readers without
//!   cloning the per-delta state; writers replace the head atomically via
//!   `apply_update`.
//! * `consolidate` returns a fresh `Arc<Vec<SqliteValue>>` suitable for
//!   swapping into the chain's `base`; rebuilding the chain around the new
//!   base is left to the caller (the eventual integration layer will decide
//!   whether consolidation happens in place or via a copy-on-write swap).

use std::sync::Arc;

use fsqlite_types::SqliteValue;

/// A single delta: one column's new value plus a link to the prior delta.
///
/// Delta nodes are immutable once constructed; an UPDATE creates a new node
/// pointing at the current head. Older nodes are reachable transitively via
/// the `prev` chain and ultimately terminate in `None`.
#[derive(Debug)]
pub struct DeltaNode {
    /// Zero-based column index affected by this delta.
    pub column_idx: u16,
    /// The new value written to `column_idx`.
    pub new_value: SqliteValue,
    /// Link to the previous delta (older than this one), or `None` if this is
    /// the oldest delta in the chain.
    pub prev: Option<Arc<Self>>,
}

/// A hot-row delta chain: a base row plus a LIFO stack of per-column deltas.
///
/// The logical current row is the `base`, with each delta from the head to
/// the oldest applied in stack order. Because we walk head-first and assign
/// each column at most once during materialization, only the most-recent
/// delta per column contributes to the output.
#[derive(Debug, Clone)]
pub struct HotRowDeltaChain {
    /// The base row the chain was built on. Wrapped in `Arc` so consolidation
    /// can atomically swap it without copying the full row for every reader.
    pub base: Arc<Vec<SqliteValue>>,
    /// The most recent delta, or `None` if no updates have been applied yet.
    pub head: Option<Arc<DeltaNode>>,
    /// Number of deltas in the chain (i.e. chain length from head to oldest).
    pub depth: u32,
}

impl HotRowDeltaChain {
    /// Build a new chain rooted at `base` with no deltas applied.
    #[must_use]
    pub fn new(base: Arc<Vec<SqliteValue>>) -> Self {
        Self {
            base,
            head: None,
            depth: 0,
        }
    }

    /// Number of columns in the base row.
    #[must_use]
    pub fn column_count(&self) -> usize {
        self.base.len()
    }
}

/// Push a new delta onto `chain`, recording a write of `new_value` to
/// `column_idx`.
///
/// Panics (debug only) if `column_idx` is out of range for the base row. In
/// release builds the out-of-range delta is still linked — `materialize` will
/// simply skip deltas whose index is outside the row width, matching the
/// behavior of the main UPDATE path where column bounds are already checked
/// upstream.
pub fn apply_update(chain: &mut HotRowDeltaChain, column_idx: u16, new_value: SqliteValue) {
    debug_assert!(
        (column_idx as usize) < chain.base.len(),
        "delta_chain::apply_update column_idx={column_idx} out of range for base len={}",
        chain.base.len()
    );
    let node = Arc::new(DeltaNode {
        column_idx,
        new_value,
        prev: chain.head.take(),
    });
    chain.head = Some(node);
    chain.depth = chain.depth.saturating_add(1);
}

/// Walk the chain from head to base, returning the current logical row.
///
/// The traversal assigns each column at most once: the first delta seen for a
/// given column wins, which — because the head is the most recent write —
/// corresponds to "latest wins" semantics.
#[must_use]
pub fn materialize(chain: &HotRowDeltaChain) -> Vec<SqliteValue> {
    let mut row: Vec<SqliteValue> = (*chain.base).clone();
    let n = row.len();

    // Track which columns have already been written by a more-recent delta so
    // older deltas for the same column are skipped.
    let mut written: Vec<bool> = vec![false; n];

    let mut cursor = chain.head.as_ref();
    while let Some(node) = cursor {
        let idx = node.column_idx as usize;
        if idx < n && !written[idx] {
            row[idx] = node.new_value.clone();
            written[idx] = true;
        }
        cursor = node.prev.as_ref();
    }

    row
}

/// Return true iff the chain's depth has reached or exceeded `threshold`.
///
/// Callers use this to decide whether the amortized read cost of walking the
/// chain has grown large enough to justify rebuilding the base row.
#[must_use]
pub fn should_consolidate(chain: &HotRowDeltaChain, threshold: u32) -> bool {
    chain.depth >= threshold
}

/// Materialize the chain into a fresh base row.
///
/// The returned `Arc<Vec<SqliteValue>>` is suitable for swapping into
/// `HotRowDeltaChain::base`. This function does not mutate `chain`; the
/// caller is responsible for replacing `base` and resetting `head`/`depth`
/// under whatever locking discipline the integration layer requires.
#[must_use]
pub fn consolidate(chain: &HotRowDeltaChain) -> Arc<Vec<SqliteValue>> {
    Arc::new(materialize(chain))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn base_row(n: usize) -> Arc<Vec<SqliteValue>> {
        Arc::new(
            (0..n)
                .map(|i| SqliteValue::Integer(i as i64))
                .collect::<Vec<_>>(),
        )
    }

    fn values_eq(a: &[SqliteValue], b: &[SqliteValue]) -> bool {
        if a.len() != b.len() {
            return false;
        }
        a.iter().zip(b.iter()).all(|(x, y)| match (x, y) {
            (SqliteValue::Null, SqliteValue::Null) => true,
            (SqliteValue::Integer(xi), SqliteValue::Integer(yi)) => xi == yi,
            (SqliteValue::Float(xf), SqliteValue::Float(yf)) => xf.to_bits() == yf.to_bits(),
            (SqliteValue::Text(xt), SqliteValue::Text(yt)) => xt.as_str() == yt.as_str(),
            (SqliteValue::Blob(xb), SqliteValue::Blob(yb)) => xb.as_ref() == yb.as_ref(),
            _ => false,
        })
    }

    #[test]
    fn empty_chain_materializes_to_base() {
        let base = base_row(4);
        let chain = HotRowDeltaChain::new(Arc::clone(&base));
        let out = materialize(&chain);
        assert!(values_eq(&out, base.as_slice()));
        assert_eq!(chain.depth, 0);
        assert!(!should_consolidate(&chain, 1));
    }

    #[test]
    fn column_count_reflects_base_width_and_is_stable_under_updates() {
        // column_count is the base row width; applying deltas (which only
        // overwrite existing columns) must never change it. No existing test
        // exercises column_count directly.
        let base = base_row(3);
        let mut chain = HotRowDeltaChain::new(base);
        assert_eq!(chain.column_count(), 3);

        apply_update(&mut chain, 1, SqliteValue::Integer(42));
        apply_update(&mut chain, 1, SqliteValue::Integer(43));
        apply_update(&mut chain, 2, SqliteValue::Integer(7));

        // Three deltas applied, but the logical row is still 3 columns wide.
        assert_eq!(chain.column_count(), 3);
        assert_eq!(chain.depth, 3);
    }

    #[test]
    fn single_update_visible_in_materialize() {
        let base = base_row(3);
        let mut chain = HotRowDeltaChain::new(Arc::clone(&base));
        apply_update(&mut chain, 1, SqliteValue::Integer(999));
        let out = materialize(&chain);
        assert!(matches!(out[0], SqliteValue::Integer(0)));
        assert!(matches!(out[1], SqliteValue::Integer(999)));
        assert!(matches!(out[2], SqliteValue::Integer(2)));
        assert_eq!(chain.depth, 1);
    }

    #[test]
    fn repeated_updates_same_column_latest_wins() {
        let base = base_row(3);
        let mut chain = HotRowDeltaChain::new(Arc::clone(&base));
        apply_update(&mut chain, 0, SqliteValue::Integer(10));
        apply_update(&mut chain, 0, SqliteValue::Integer(20));
        apply_update(&mut chain, 0, SqliteValue::Integer(30));
        let out = materialize(&chain);
        assert!(matches!(out[0], SqliteValue::Integer(30)));
        assert!(matches!(out[1], SqliteValue::Integer(1)));
        assert!(matches!(out[2], SqliteValue::Integer(2)));
        assert_eq!(chain.depth, 3);
    }

    #[test]
    fn updates_to_different_columns_all_applied() {
        let base = base_row(4);
        let mut chain = HotRowDeltaChain::new(Arc::clone(&base));
        apply_update(&mut chain, 0, SqliteValue::Integer(100));
        apply_update(&mut chain, 2, SqliteValue::Integer(200));
        apply_update(&mut chain, 3, SqliteValue::Null);
        let out = materialize(&chain);
        assert!(matches!(out[0], SqliteValue::Integer(100)));
        assert!(matches!(out[1], SqliteValue::Integer(1)));
        assert!(matches!(out[2], SqliteValue::Integer(200)));
        assert!(matches!(out[3], SqliteValue::Null));
        assert_eq!(chain.depth, 3);
    }

    #[test]
    fn consolidate_preserves_logical_row() {
        let base = base_row(5);
        let mut chain = HotRowDeltaChain::new(Arc::clone(&base));
        apply_update(&mut chain, 1, SqliteValue::Integer(11));
        apply_update(&mut chain, 4, SqliteValue::Integer(44));
        apply_update(&mut chain, 1, SqliteValue::Integer(111));

        let before = materialize(&chain);
        let new_base = consolidate(&chain);
        // Rebuild a fresh chain on top of the consolidated base with no deltas.
        let rebuilt = HotRowDeltaChain::new(Arc::clone(&new_base));
        let after = materialize(&rebuilt);

        assert!(values_eq(&before, &after));
        assert_eq!(rebuilt.depth, 0);
        assert!(rebuilt.head.is_none());
    }

    #[test]
    fn should_consolidate_threshold_boundary() {
        let base = base_row(2);
        let mut chain = HotRowDeltaChain::new(Arc::clone(&base));
        assert!(!should_consolidate(&chain, 3));
        apply_update(&mut chain, 0, SqliteValue::Integer(1));
        apply_update(&mut chain, 0, SqliteValue::Integer(2));
        assert!(!should_consolidate(&chain, 3));
        apply_update(&mut chain, 0, SqliteValue::Integer(3));
        assert!(should_consolidate(&chain, 3));
        assert!(should_consolidate(&chain, 2));
    }

    #[test]
    fn deep_chain_materializes_correctly() {
        // Linear chain of 500 writes to a single column; exercises the
        // head-to-base walk without stack overflow.
        let base = base_row(1);
        let mut chain = HotRowDeltaChain::new(Arc::clone(&base));
        for v in 0..500_i64 {
            apply_update(&mut chain, 0, SqliteValue::Integer(v));
        }
        let out = materialize(&chain);
        assert!(matches!(out[0], SqliteValue::Integer(499)));
        assert_eq!(chain.depth, 500);
    }

    proptest! {
        /// Property: for any sequence of random per-column writes, the
        /// materialized row equals a ground-truth naive-rewrite simulation.
        /// Every N=5 updates, consolidate and continue on the fresh base;
        /// the invariant must hold across consolidation boundaries.
        #[test]
        fn property_random_updates_match_naive_rewrite(
            updates in proptest::collection::vec(
                (0u16..10, any::<i64>()),
                1..100,
            ),
        ) {
            const COLUMNS: usize = 10;
            const THRESHOLD: u32 = 5;

            let base = base_row(COLUMNS);
            let mut chain = HotRowDeltaChain::new(Arc::clone(&base));
            // Ground truth: a plain Vec we rewrite in place.
            let mut ground: Vec<SqliteValue> = (*base).clone();

            for (col, val) in updates {
                apply_update(&mut chain, col, SqliteValue::Integer(val));
                ground[col as usize] = SqliteValue::Integer(val);

                // Cross-consolidation invariant: materialize must agree with
                // ground truth both before and after consolidation.
                let materialized = materialize(&chain);
                prop_assert!(values_eq(&materialized, &ground));

                if should_consolidate(&chain, THRESHOLD) {
                    let new_base = consolidate(&chain);
                    chain = HotRowDeltaChain::new(new_base);
                    let after = materialize(&chain);
                    prop_assert!(values_eq(&after, &ground));
                }
            }

            // Final check after the full sequence.
            let final_row = materialize(&chain);
            prop_assert!(values_eq(&final_row, &ground));
        }
    }
}
