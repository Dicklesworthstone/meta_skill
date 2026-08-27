//! Learned rowid -> page index with split-conformal prediction intervals.
//!
//! IMPL-22 / AAC-P8.
//!
//! # Concept
//!
//! B-tree rowid lookup costs `O(log n)` page descents. If rowids are
//! (approximately) dense and predictable, a learned model (linear regression)
//! can predict the page containing a rowid in `O(1)`. Rather than trust the
//! model blindly, we wrap its prediction with a **split-conformal prediction
//! interval** — with probability `>= 1 - alpha` (alpha chosen at build time),
//! the true page lies in `[predicted_page - radius, predicted_page + radius]`.
//! On cache miss inside that window, fall through to the canonical B-tree
//! descent.
//!
//! # Math
//!
//! Linear regression (closed-form least squares):
//!
//! ```text
//!   slope     = Cov(x, y) / Var(x)
//!   intercept = mean(y) - slope * mean(x)
//! ```
//!
//! Split-conformal calibration (Vovk 2005):
//!
//! 1. Randomly (deterministically, via hashed shuffle) split the training set
//!    `70/30` into a fit set and a calibration set.
//! 2. Fit slope/intercept on the fit set.
//! 3. On the calibration set of size `n`, compute the residuals
//!    `r_i = |predicted_i - actual_i|`.
//! 4. Sort ascending. Take `r_k` where `k = ceil((1 - alpha) * (n + 1)) - 1`
//!    (clamped to `[0, n-1]`). This is the conformal radius.
//!
//! Under exchangeability of calibration and test points, the marginal coverage
//! guarantee is `P(|predicted - actual| <= r_k) >= 1 - alpha`.
//!
//! # Wiring (deferred)
//!
//! This module ships the model + tests only. It is NOT wired into any seek
//! path on this commit. When wiring later:
//!
//! - B-tree seek can call [`LearnedRowIdIndex::page_range`], probe the pager
//!   cache in that range first; on miss, fall through to the canonical
//!   descent.
//! - Build the index on `ANALYZE` from a sample of `(rowid, page)` pairs
//!   collected during a full scan.
//! - Invalidate on any balance/split/merge that shifts the rowid-to-page map.
//!
//! Ref: Vovk et al., "Algorithmic Learning in a Random World" (2005);
//! Kraska et al., "The Case for Learned Index Structures", SIGMOD 2018.

use std::ops::RangeInclusive;

/// A learned rowid -> page mapping wrapped in a split-conformal prediction
/// interval.
///
/// Given a collection of `(rowid, page)` observations, [`LearnedRowIdIndex::fit`]
/// learns a linear model and a calibration radius such that, for any future
/// rowid drawn from the same distribution, the true page lies within
/// `[predicted - radius, predicted + radius]` with probability at least
/// `1 - alpha`.
#[derive(Debug, Clone)]
pub struct LearnedRowIdIndex {
    /// Linear regression slope: `page ~ slope * rowid + intercept`.
    pub slope: f64,
    /// Linear regression intercept.
    pub intercept: f64,
    /// Sorted (ascending) absolute residuals on the held-out calibration
    /// split. Exposed for tests and instrumentation.
    pub residuals_calibration: Vec<f64>,
    /// Desired miscoverage rate. With probability `>= 1 - alpha`, the true
    /// page lies within the conformal radius of the prediction.
    pub alpha: f64,
    /// Cached conformal radius derived from `residuals_calibration` and
    /// `alpha`. `u32::MAX` if there is no calibration information (degenerate
    /// fit); callers should fall through to canonical descent in that case.
    radius: u32,
}

impl LearnedRowIdIndex {
    /// Fit a learned rowid -> page index to the given observations.
    ///
    /// - Splits the input `70/30` into a fit set and a calibration set using a
    ///   deterministic stride shuffle (every 10th element goes to calibration,
    ///   cycling through offsets so coverage is uniform across the input).
    /// - Fits a linear regression on the fit set via closed-form least
    ///   squares.
    /// - Computes absolute residuals on the calibration set, sorts them, and
    ///   records the `ceil((1 - alpha) * (n_calib + 1)) - 1`-th entry as the
    ///   conformal radius.
    ///
    /// Degenerate inputs are handled gracefully:
    ///
    /// - Empty input: slope = 0, intercept = 0, empty calibration. `predict`
    ///   returns `(0, u32::MAX)` — callers fall through.
    /// - Single point `(r, p)`: slope = 0, intercept = p, empty calibration.
    /// - All-equal rowids (zero x-variance): slope = 0, intercept = mean(y).
    /// - `alpha <= 0`: treated as requesting full coverage (radius covers max
    ///   calibration residual).
    /// - `alpha >= 1`: treated as no coverage guarantee (radius = 0).
    #[must_use]
    pub fn fit(rowid_to_page: &[(i64, u32)], alpha: f64) -> Self {
        let alpha = alpha.clamp(0.0, 1.0);

        if rowid_to_page.is_empty() {
            return Self {
                slope: 0.0,
                intercept: 0.0,
                residuals_calibration: Vec::new(),
                alpha,
                radius: u32::MAX,
            };
        }

        if rowid_to_page.len() == 1 {
            // Single data point: predict constant, no calibration possible.
            let (_, p) = rowid_to_page[0];
            return Self {
                slope: 0.0,
                intercept: f64::from(p),
                residuals_calibration: Vec::new(),
                alpha,
                radius: u32::MAX,
            };
        }

        // Deterministic 70/30 split. Every 10th index (0, 10, 20, ...) and
        // every 10th + 3 goes to calibration (30% of indices); the remainder
        // goes to fit. This preserves order in both splits and is uniform
        // across the input.
        let mut fit_pts: Vec<(f64, f64)> = Vec::with_capacity((rowid_to_page.len() * 7) / 10 + 1);
        let mut calib_pts: Vec<(f64, f64)> = Vec::with_capacity((rowid_to_page.len() * 3) / 10 + 1);
        for (idx, &(r, p)) in rowid_to_page.iter().enumerate() {
            let x = r as f64;
            let y = f64::from(p);
            // Indices with (idx % 10) in {0, 3, 7} go to calibration (30%).
            let is_calib = matches!(idx % 10, 0 | 3 | 7);
            if is_calib {
                calib_pts.push((x, y));
            } else {
                fit_pts.push((x, y));
            }
        }

        // If the deterministic split leaves the fit set too small (possible
        // only for very small inputs), fall back to fitting on everything
        // and calibrating on everything — still better than nothing.
        if fit_pts.len() < 2 {
            fit_pts = rowid_to_page
                .iter()
                .map(|&(r, p)| (r as f64, f64::from(p)))
                .collect();
            if calib_pts.is_empty() {
                calib_pts.clone_from(&fit_pts);
            }
        }

        let (slope, intercept) = fit_linear_regression(&fit_pts);

        // Compute calibration residuals.
        let mut residuals: Vec<f64> = calib_pts
            .iter()
            .map(|&(x, y)| {
                let predicted = slope.mul_add(x, intercept);
                (predicted - y).abs()
            })
            .collect();
        residuals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let radius = conformal_radius(&residuals, alpha);

        Self {
            slope,
            intercept,
            residuals_calibration: residuals,
            alpha,
            radius,
        }
    }

    /// Predict the page for `rowid` and return the `(center, radius)` of the
    /// split-conformal prediction interval.
    ///
    /// With probability `>= 1 - alpha` (under exchangeability with the
    /// calibration set), the true page for `rowid` lies in
    /// `[center.saturating_sub(radius), center.saturating_add(radius)]`.
    #[must_use]
    pub fn predict(&self, rowid: i64) -> (u32, u32) {
        let x = rowid as f64;
        let predicted = self.slope.mul_add(x, self.intercept);
        let center = if predicted.is_finite() {
            // Clamp to u32 range. Sub-page-1 predictions clamp to 0; very
            // large predictions clamp to u32::MAX.
            if predicted <= 0.0 {
                0
            } else if predicted >= f64::from(u32::MAX) {
                u32::MAX
            } else {
                predicted.round() as u32
            }
        } else {
            0
        };
        (center, self.radius)
    }

    /// Convenience wrapper around [`predict`](Self::predict) that returns the
    /// full inclusive page range `[center - radius, center + radius]`,
    /// saturating at `u32` bounds.
    ///
    /// Callers probing the pager cache should iterate this range; on miss,
    /// fall through to canonical B-tree descent.
    #[must_use]
    pub fn page_range(&self, rowid: i64) -> RangeInclusive<u32> {
        let (center, radius) = self.predict(rowid);
        let lo = center.saturating_sub(radius);
        let hi = center.saturating_add(radius);
        lo..=hi
    }

    /// Exposed for tests: the cached conformal radius.
    #[must_use]
    pub fn radius(&self) -> u32 {
        self.radius
    }
}

/// Closed-form ordinary least squares on `(x, y)` pairs.
///
/// Returns `(slope, intercept)` for the model `y ~ slope * x + intercept`.
/// If the sample variance of `x` is zero (all-equal), returns
/// `(0.0, mean(y))`.
fn fit_linear_regression(points: &[(f64, f64)]) -> (f64, f64) {
    if points.is_empty() {
        return (0.0, 0.0);
    }
    let n = points.len() as f64;
    let (sum_x, sum_y) = points
        .iter()
        .fold((0.0_f64, 0.0_f64), |(sx, sy), &(x, y)| (sx + x, sy + y));
    let mean_x = sum_x / n;
    let mean_y = sum_y / n;

    let (cov_xy, var_x) = points
        .iter()
        .fold((0.0_f64, 0.0_f64), |(cxy, vx), &(x, y)| {
            let dx = x - mean_x;
            let dy = y - mean_y;
            (dx.mul_add(dy, cxy), dx.mul_add(dx, vx))
        });

    if var_x <= f64::EPSILON {
        // Zero-variance predictor: can't fit a slope. Collapse to mean(y).
        return (0.0, mean_y);
    }

    let slope = cov_xy / var_x;
    let intercept = slope.mul_add(-mean_x, mean_y);
    (slope, intercept)
}

/// Compute the split-conformal radius from a sorted ascending slice of
/// calibration residuals.
///
/// Returns `u32::MAX` (sentinel: "no guarantee, fall through") if the slice
/// is empty.
///
/// Otherwise returns the `ceil((1 - alpha) * (n + 1))`-th smallest residual
/// (1-indexed), clamped to `[1, n]`, converted to a `u32` page count. The
/// `(n + 1)` term is the standard finite-sample correction for split
/// conformal prediction (Vovk 2005).
fn conformal_radius(sorted_residuals: &[f64], alpha: f64) -> u32 {
    if sorted_residuals.is_empty() {
        return u32::MAX;
    }
    let n = sorted_residuals.len();
    // ceil((1 - alpha) * (n + 1)) in 1-indexed terms.
    let target = ((1.0 - alpha) * (n as f64 + 1.0)).ceil();
    let k_1indexed = (target as usize).clamp(1, n);
    let residual = sorted_residuals[k_1indexed - 1];
    // Round up to the nearest whole page — we want a coverage-valid interval
    // in page-index space, not a fractional one.
    let ceil = residual.ceil();
    if !ceil.is_finite() || ceil <= 0.0 {
        0
    } else if ceil >= f64::from(u32::MAX) {
        u32::MAX
    } else {
        ceil as u32
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// Build a synthetic set of `(rowid, page)` observations where rowids are
    /// dense in `[0, n)` and `rows_per_page` rows live on each page.
    fn synthetic_dense(n: i64, rows_per_page: i64) -> Vec<(i64, u32)> {
        (0..n)
            .map(|r| {
                let page = (r / rows_per_page) as u32;
                (r, page)
            })
            .collect()
    }

    fn rmse(idx: &LearnedRowIdIndex, obs: &[(i64, u32)]) -> f64 {
        if obs.is_empty() {
            return 0.0;
        }
        let n = obs.len() as f64;
        let sum_sq: f64 = obs
            .iter()
            .map(|&(r, p)| {
                let (center, _) = idx.predict(r);
                let err = f64::from(center) - f64::from(p);
                err * err
            })
            .sum();
        (sum_sq / n).sqrt()
    }

    #[test]
    fn fit_uniform_dense_is_low_rmse() {
        let obs = synthetic_dense(10_000, 100);
        let idx = LearnedRowIdIndex::fit(&obs, 0.05);
        let rmse = rmse(&idx, &obs);
        // The true map is step-constant but the linear fit tracks it tightly
        // on a dense uniform distribution: expected slope ~ 1/100, residual
        // error bounded by the step function's sawtooth, which is < 50 pages
        // peak-to-peak => RMSE well under 50.
        assert!(
            rmse < 50.0,
            "RMSE too high for uniform-dense synthetic: {rmse}"
        );
        // Surface the number when running with --nocapture.
        println!(
            "fit_uniform_dense_is_low_rmse: slope={} intercept={} rmse={} radius={}",
            idx.slope,
            idx.intercept,
            rmse,
            idx.radius(),
        );
    }

    #[test]
    fn conformal_coverage_meets_guarantee() {
        // Train/test split from the same distribution.
        let mut all = synthetic_dense(10_000, 100);
        // Shuffle deterministically via index rotation so train and test
        // draws are exchangeable w.r.t. the source distribution.
        let rot = 12345 % all.len();
        all.rotate_left(rot);
        let split = (all.len() * 7) / 10;
        let train = &all[..split];
        let test = &all[split..];

        let alpha = 0.1;
        let idx = LearnedRowIdIndex::fit(train, alpha);

        let mut covered = 0usize;
        for &(r, p) in test {
            let (center, radius) = idx.predict(r);
            let lo = center.saturating_sub(radius);
            let hi = center.saturating_add(radius);
            if (lo..=hi).contains(&p) {
                covered += 1;
            }
        }
        let coverage = covered as f64 / test.len() as f64;
        // Coverage should be at least 1 - alpha minus a small slack for
        // finite-sample variance. With a 3000-point test set and alpha=0.1,
        // a Hoeffding bound gives +/- ~0.03 at high confidence.
        assert!(
            coverage >= (1.0 - alpha) - 0.03,
            "coverage {coverage} < 1 - alpha - slack = {}",
            1.0 - alpha - 0.03
        );
        println!(
            "conformal_coverage_meets_guarantee: alpha={alpha} coverage={coverage} radius={}",
            idx.radius()
        );
    }

    #[test]
    fn empty_input_degenerates_gracefully() {
        let idx = LearnedRowIdIndex::fit(&[], 0.1);
        assert_eq!(idx.slope, 0.0);
        assert_eq!(idx.intercept, 0.0);
        assert!(idx.residuals_calibration.is_empty());
        // Sentinel: "no guarantee, fall through".
        assert_eq!(idx.radius(), u32::MAX);
        let (center, radius) = idx.predict(42);
        assert_eq!(center, 0);
        assert_eq!(radius, u32::MAX);
    }

    #[test]
    fn single_point_degenerates_gracefully() {
        let idx = LearnedRowIdIndex::fit(&[(100, 7)], 0.1);
        assert_eq!(idx.slope, 0.0);
        assert!((idx.intercept - 7.0).abs() < 1e-9);
        let (center, _) = idx.predict(100);
        assert_eq!(center, 7);
        // No calibration data available.
        assert_eq!(idx.radius(), u32::MAX);
    }

    #[test]
    fn all_same_rowid_collapses_to_mean() {
        // Zero variance in x => slope must be 0, intercept = mean(y) on the
        // fit split (not on the full set, since fit() holds out a calibration
        // split before training).
        let obs: Vec<(i64, u32)> = (0..20).map(|i| (42, i as u32)).collect();
        let idx = LearnedRowIdIndex::fit(&obs, 0.1);
        assert_eq!(idx.slope, 0.0);
        // Reconstruct the fit set the same way fit() does and verify the
        // intercept matches the mean of the fit subset's y values.
        let fit_y_mean: f64 = {
            let ys: Vec<f64> = obs
                .iter()
                .enumerate()
                .filter(|(i, _)| !matches!(i % 10, 0 | 3 | 7))
                .map(|(_, &(_, p))| f64::from(p))
                .collect();
            ys.iter().sum::<f64>() / ys.len() as f64
        };
        assert!(
            (idx.intercept - fit_y_mean).abs() < 1e-9,
            "intercept {} != fit_y_mean {}",
            idx.intercept,
            fit_y_mean
        );
    }

    #[test]
    fn page_range_saturates_at_u32_bounds() {
        let obs = synthetic_dense(100, 10);
        let idx = LearnedRowIdIndex::fit(&obs, 0.1);

        // Very negative rowid -> center saturates to 0, range starts at 0.
        let range_neg = idx.page_range(i64::MIN);
        assert_eq!(*range_neg.start(), 0);

        // Very positive rowid -> center clamps to u32::MAX, range ends there.
        let range_pos = idx.page_range(i64::MAX);
        assert_eq!(*range_pos.end(), u32::MAX);
    }

    #[test]
    fn page_range_contains_predict_center() {
        let obs = synthetic_dense(1_000, 50);
        let idx = LearnedRowIdIndex::fit(&obs, 0.1);
        for r in (0..1_000).step_by(17) {
            let (center, _) = idx.predict(r);
            let range = idx.page_range(r);
            assert!(
                range.contains(&center),
                "page_range {:?} does not contain predict center {center} for rowid {r}",
                range
            );
        }
    }

    #[test]
    fn predict_radius_is_global_and_page_range_matches_predict() {
        // predict returns (center, radius); the radius is the GLOBAL conformal
        // radius (== radius()), identical for every rowid -- not recomputed per
        // query. page_range is exactly [center - radius, center + radius]
        // (saturating). page_range_contains_predict_center only checks
        // containment, not these equalities.
        let obs = synthetic_dense(1_000, 50);
        let idx = LearnedRowIdIndex::fit(&obs, 0.1);
        let r = idx.radius();

        for rowid in [0_i64, 137, 499, 999] {
            let (center, pred_radius) = idx.predict(rowid);
            // predict reports the same global radius regardless of rowid.
            assert_eq!(pred_radius, r);

            // page_range is exactly center +/- radius, saturating at u32 bounds.
            let range = idx.page_range(rowid);
            assert_eq!(*range.start(), center.saturating_sub(r));
            assert_eq!(*range.end(), center.saturating_add(r));
        }
    }

    #[test]
    fn alpha_clamped_out_of_range() {
        let obs = synthetic_dense(1_000, 50);
        let a = LearnedRowIdIndex::fit(&obs, -1.0);
        let b = LearnedRowIdIndex::fit(&obs, 2.0);
        // alpha clamped to [0, 1] so fit should succeed without panic.
        assert!(a.alpha >= 0.0 && a.alpha <= 1.0);
        assert!(b.alpha >= 0.0 && b.alpha <= 1.0);
    }

    proptest! {
        /// Property: on uniform-dense rowids in [0, 10000] with ~100 rows
        /// per page, the conformal interval covers the true page at least
        /// (1 - alpha) fraction of the time.
        ///
        /// Uses a random 70/30 train/test split (seeded by proptest); the
        /// fit itself uses a deterministic 70/30 sub-split for calibration,
        /// so this exercises the full split-conformal pipeline.
        #[test]
        fn prop_conformal_coverage_uniform_dense(
            alpha in 0.05f64..0.3,
            seed in 0u64..1_000,
        ) {
            let obs = synthetic_dense(10_000, 100);
            // Deterministic seeded shuffle (LCG — no need for a real PRNG dep).
            let mut permuted = obs;
            let mut s = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            for i in (1..permuted.len()).rev() {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let j = (s >> 33) as usize % (i + 1);
                permuted.swap(i, j);
            }
            let split = (permuted.len() * 7) / 10;
            let train = &permuted[..split];
            let test = &permuted[split..];

            let idx = LearnedRowIdIndex::fit(train, alpha);
            let mut covered = 0usize;
            for &(r, p) in test {
                let (center, radius) = idx.predict(r);
                let lo = center.saturating_sub(radius);
                let hi = center.saturating_add(radius);
                if (lo..=hi).contains(&p) {
                    covered += 1;
                }
            }
            let coverage = covered as f64 / test.len() as f64;
            // 3000-sample test set; Hoeffding slack ~ 0.04 at conservative
            // confidence across 256 proptest cases.
            let slack = 0.04;
            prop_assert!(
                coverage >= (1.0 - alpha) - slack,
                "coverage {} < 1 - alpha - slack = {} (alpha={}, seed={})",
                coverage,
                1.0 - alpha - slack,
                alpha,
                seed,
            );
        }
    }
}
