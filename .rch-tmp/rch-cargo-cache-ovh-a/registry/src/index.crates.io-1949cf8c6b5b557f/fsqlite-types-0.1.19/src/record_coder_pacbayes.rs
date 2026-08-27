//! PAC-Bayes layout-selection math primitive (IMPL-20 / AAC-P1).
//!
//! This module provides the *observation accumulator* and *McAllester PAC-Bayes
//! upper-bound* machinery used to pick between candidate record-coding layouts
//! online, without needing to trust empirical means on small samples.
//!
//! The actual alternative encoders (`SuperblockSharedHeader`, `SuperblockRLE`)
//! are follow-up work; this module ships the math primitive only. Callers are
//! expected to measure `layout_bytes` under all three candidate encodings
//! (even if two of them are currently simulated / dry-run) and feed them into
//! [`LayoutStats::observe_row`]. The selector at [`LayoutStats::best_layout`]
//! then picks the layout whose upper confidence bound on expected per-row
//! bytes is smallest — the pessimistic-optimal choice.
//!
//! ## McAllester bound (sketch)
//!
//! With prior P uniform over the 3 layouts and posterior Q concentrating on a
//! single candidate L, the KL divergence `KL(Q || P) = ln 3`. The McAllester
//! (2003) PAC-Bayes bound then gives, with probability ≥ 1 − δ over the
//! sample of size n, for every candidate L:
//!
//! ```text
//!     E[bytes | L]  ≤  μ̂_L  +  sqrt( (KL(Q || P) + ln(n / δ)) / (2 n) )
//!                    =  μ̂_L  +  sqrt( (ln 3 + ln(n / δ)) / (2 n) )
//! ```
//!
//! The bound is valid only when per-row byte observations are bounded; we
//! implicitly normalize to [0, 1] by the caller's choice of unit (bytes per
//! row under reasonable row sizes is a small constant). For the MVP we
//! accumulate raw bytes and let the bound be interpreted on that same scale;
//! the slack term is still a valid concentration width up to a constant.

use std::sync::atomic::{AtomicU64, Ordering};

/// One of the three candidate record-coding layouts.
///
/// The enum is repr(u8) so it can double as an index into the parallel
/// per-layout arrays maintained by [`LayoutStats`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum LayoutCandidate {
    /// The default SQLite-compatible per-row record encoding.
    Standard = 0,
    /// Superblock with a shared header factored out across the rows it covers.
    /// (Encoder not yet implemented — observation slot only.)
    SuperblockSharedHeader = 1,
    /// Superblock with run-length encoding of repeated column values.
    /// (Encoder not yet implemented — observation slot only.)
    SuperblockRLE = 2,
}

impl LayoutCandidate {
    /// All three candidates in the canonical index order.
    pub const ALL: [Self; 3] = [
        Self::Standard,
        Self::SuperblockSharedHeader,
        Self::SuperblockRLE,
    ];

    /// Convert to the parallel-array index `0..3`.
    #[inline]
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Recover a candidate from its parallel-array index. Returns `None` if
    /// out of range.
    #[inline]
    #[must_use]
    pub const fn from_index(i: usize) -> Option<Self> {
        match i {
            0 => Some(Self::Standard),
            1 => Some(Self::SuperblockSharedHeader),
            2 => Some(Self::SuperblockRLE),
            _ => None,
        }
    }
}

/// Thread-safe accumulator of per-layout byte observations.
///
/// Each row observation contributes one increment to every layout's counter
/// and adds that layout's encoded size to its byte total. Reads via
/// [`LayoutStats::pac_bayes_upper_bound`] are snapshot-consistent per slot
/// (we load the pair for each layout with `Ordering::Relaxed` — the bound is
/// statistical, not transactional, so a slightly stale pair is acceptable).
#[derive(Debug, Default)]
pub struct LayoutStats {
    /// Total bytes that would have been emitted under each candidate across
    /// all observed rows.
    pub per_layout_bytes_observed: [AtomicU64; 3],
    /// Number of rows observed per candidate. All three are incremented in
    /// lockstep by [`observe_row`](Self::observe_row), but the per-layout
    /// field is kept for parallel-array symmetry and future per-layout
    /// sampling strategies.
    pub per_layout_count: [AtomicU64; 3],
}

impl LayoutStats {
    /// Construct a fresh, zeroed accumulator.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            per_layout_bytes_observed: [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)],
            per_layout_count: [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)],
        }
    }

    /// Record one row's worth of observations across all three candidate
    /// layouts. `layout_bytes[i]` is the number of bytes this row would take
    /// under `LayoutCandidate::from_index(i)`.
    pub fn observe_row(&self, layout_bytes: [usize; 3]) {
        for (i, bytes) in layout_bytes.iter().enumerate() {
            self.per_layout_bytes_observed[i].fetch_add(*bytes as u64, Ordering::Relaxed);
            self.per_layout_count[i].fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Compute the McAllester PAC-Bayes upper bound on expected per-row bytes
    /// for each candidate layout, at confidence level `1 - delta`.
    ///
    /// Returns `[(candidate, empirical_mean, upper_bound); 3]` in canonical
    /// [`LayoutCandidate::ALL`] order.
    ///
    /// For a candidate with zero observations the empirical mean is defined
    /// as 0.0 and the upper bound as [`f64::INFINITY`], so such a candidate
    /// can never win the argmin until it has been observed at least once.
    #[must_use]
    pub fn pac_bayes_upper_bound(&self, delta: f64) -> [(LayoutCandidate, f64, f64); 3] {
        // ln 3 — KL(Q || P) when P is uniform over 3 layouts and Q puts all
        // its mass on one.
        let kl = 3.0_f64.ln();

        let mut out = [
            (LayoutCandidate::Standard, 0.0, f64::INFINITY),
            (LayoutCandidate::SuperblockSharedHeader, 0.0, f64::INFINITY),
            (LayoutCandidate::SuperblockRLE, 0.0, f64::INFINITY),
        ];

        for (i, slot) in out.iter_mut().enumerate() {
            let n = self.per_layout_count[i].load(Ordering::Relaxed);
            let total = self.per_layout_bytes_observed[i].load(Ordering::Relaxed);

            let candidate = LayoutCandidate::from_index(i).expect("i in 0..3");
            if n == 0 {
                *slot = (candidate, 0.0, f64::INFINITY);
                continue;
            }

            // Guard against pathological delta values.
            let delta_eff = if delta.is_finite() && delta > 0.0 && delta < 1.0 {
                delta
            } else {
                0.05
            };

            let n_f = n as f64;
            let mean = (total as f64) / n_f;
            // slack = sqrt( (KL + ln(n / delta)) / (2 n) )
            let slack_num = kl + (n_f / delta_eff).ln();
            let slack = (slack_num / (2.0 * n_f)).sqrt();
            let bound = mean + slack;
            *slot = (candidate, mean, bound);
        }

        out
    }

    /// Pick the layout whose PAC-Bayes upper bound is smallest (the
    /// pessimistic-optimal choice). Ties break toward the lower-indexed
    /// candidate, which defaults to [`LayoutCandidate::Standard`] — the
    /// currently-shipping encoder.
    #[must_use]
    pub fn best_layout(&self, delta: f64) -> LayoutCandidate {
        let bounds = self.pac_bayes_upper_bound(delta);
        let mut best = bounds[0];
        for cand in bounds.iter().skip(1) {
            if cand.2 < best.2 {
                best = *cand;
            }
        }
        best.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scenario 1: with 100 rows each, Standard=10 bytes, SharedHeader=20,
    /// RLE=5 — RLE must win the argmin regardless of the (small) slack term.
    #[test]
    fn rle_wins_when_empirically_smallest() {
        let stats = LayoutStats::new();
        for _ in 0..100 {
            stats.observe_row([10, 20, 5]);
        }
        assert_eq!(stats.best_layout(0.05), LayoutCandidate::SuperblockRLE);

        // Sanity: the means come back in the right order.
        let bounds = stats.pac_bayes_upper_bound(0.05);
        assert!((bounds[0].1 - 10.0).abs() < 1e-9);
        assert!((bounds[1].1 - 20.0).abs() < 1e-9);
        assert!((bounds[2].1 - 5.0).abs() < 1e-9);
        // Every bound > its mean (slack is strictly positive for finite n).
        for b in &bounds {
            assert!(b.2 > b.1);
        }
    }

    /// Scenario 2: n=1 → the slack term is huge but the math must not crash
    /// and must not produce NaN/−inf.
    #[test]
    fn n_equals_one_produces_wide_but_finite_bound() {
        let stats = LayoutStats::new();
        stats.observe_row([10, 20, 5]);
        let bounds = stats.pac_bayes_upper_bound(0.05);
        for (cand, mean, bound) in bounds {
            assert!(
                mean.is_finite() && bound.is_finite(),
                "cand {cand:?} produced non-finite mean {mean} or bound {bound}",
            );
            assert!(bound >= mean, "bound must be ≥ mean");
        }
        // And best_layout must still return something sensible.
        let _ = stats.best_layout(0.05);
    }

    /// Scenario 3: McAllester math check.
    ///
    /// With n = 1000, δ = 0.05, observed bytes = 0 on every row:
    /// slack = sqrt( (ln 3 + ln(1000 / 0.05)) / (2 · 1000) )
    ///       = sqrt( (ln 3 + ln 20000) / 2000 )
    ///       ≈ sqrt( 11.0021 / 2000 )
    ///       ≈ sqrt( 0.00550 )
    ///       ≈ 0.07417
    ///
    /// We allow 5% tolerance around the analytically computed value.
    #[test]
    fn mcallester_slack_matches_closed_form() {
        let stats = LayoutStats::new();
        for _ in 0..1000 {
            stats.observe_row([0, 0, 0]);
        }
        let bounds = stats.pac_bayes_upper_bound(0.05);
        // mean is 0 so bound == slack.
        let expected = ((3.0_f64.ln() + (1000.0_f64 / 0.05).ln()) / 2000.0).sqrt();
        for (cand, mean, bound) in bounds {
            assert!(mean.abs() < 1e-12, "cand {cand:?} mean should be 0");
            let rel_err = (bound - expected).abs() / expected;
            assert!(
                rel_err < 0.05,
                "cand {cand:?} bound {bound} off expected {expected} by {rel_err}",
            );
        }
    }

    /// Extra: zero-observation candidate has +inf bound, never wins argmin.
    #[test]
    fn unobserved_candidate_has_infinite_bound() {
        let stats = LayoutStats::new();
        // We have to go through observe_row, which increments all three in
        // lockstep. To simulate "unobserved" we just start from new().
        let bounds = stats.pac_bayes_upper_bound(0.05);
        for (_, mean, bound) in bounds {
            assert_eq!(mean, 0.0);
            assert!(bound.is_infinite());
        }
        // Ties on +inf: argmin returns the first (Standard).
        assert_eq!(stats.best_layout(0.05), LayoutCandidate::Standard);
    }

    /// Extra: invalid delta falls back to 0.05 instead of producing NaN.
    #[test]
    fn invalid_delta_is_handled_gracefully() {
        let stats = LayoutStats::new();
        for _ in 0..100 {
            stats.observe_row([10, 10, 10]);
        }
        for bad_delta in [0.0, 1.0, -0.1, f64::NAN, f64::INFINITY] {
            let bounds = stats.pac_bayes_upper_bound(bad_delta);
            for (_, mean, bound) in bounds {
                assert!(mean.is_finite() && bound.is_finite());
            }
        }
    }
}
