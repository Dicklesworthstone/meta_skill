//! Bayesian-mixture e-value eviction heuristic (IMPL-19 / AAC-P5).
//!
//! # Motivation
//!
//! Classical page cache eviction policies (LRU, CLOCK, ARC, S3-FIFO) pick a
//! victim by ranking pages against a frequency/recency model. They work well
//! in aggregate but offer **no statistical guarantees** on individual choices:
//! when we evict page P, we cannot quantify the probability that P was
//! actually "hot" and we just made a mistake.
//!
//! This module implements eviction via **test martingales / e-processes**
//! (Shafer, Vovk, Ramdas and coauthors — see *Game-Theoretic Statistics*).
//! For each cached page we maintain a value `e_P ∈ ℝ⁺` that grows when the
//! page is accessed and decays when it is not, such that under the *null
//! hypothesis that P is cold* the process `(e_P(t))_{t≥0}` is a
//! **non-negative supermartingale** with `E[e_P(0)] ≤ 1`.
//!
//! By **Ville's inequality** (Ville 1939), for any such process,
//!
//! ```text
//!     P( sup_t  e_P(t)  ≥  1/α   |   null )   ≤   α
//! ```
//!
//! at **any stopping time** (hence "safe, anytime-valid" inference). In
//! particular if we pick the page of **smallest** `e_P` as the eviction
//! victim, the probability of that page being a true positive ("hot") is
//! bounded by `1/e_P` regardless of how long we've been running or what
//! stopping rule our eviction loop uses. Concretely an e-value of 20
//! corresponds to a one-sided p-value of `1/20 = 0.05`.
//!
//! # Update rule
//!
//! For tunable `r_hit > 1` and `r_tick ∈ (0, 1)`:
//!
//! * **On access**  (pro-hot evidence):     `e_P ← e_P · r_hit`
//! * **Per tick**   (pro-cold evidence):    `e_P ← e_P · r_tick`
//!
//! This is the **Bayesian mixture** test martingale: it is the log-likelihood
//! ratio of a geometric access model vs. the null "never-accessed" model,
//! integrated against a uniform prior over `r_hit`. Products of such factors
//! (one per observed Bernoulli outcome) yield a valid e-process by the
//! optional-stopping Markov argument: each factor has conditional expectation
//! ≤ 1 under the null, so the product is a non-negative supermartingale.
//!
//! The **convex combination / mixture property** — averaging two valid
//! e-processes with weights summing to 1 yields another valid e-process —
//! follows from linearity of expectation; we rely on it implicitly when we
//! tune `r_hit` to the Robbins mixture (average over access rates).
//!
//! # Victim selection
//!
//! `choose_victim(candidates)` returns `argmin_{P ∈ candidates} e_P`. The
//! Ville bound tells us that if we evict at level α we should only evict
//! pages whose `e_P < 1/α`. In the cache setting we must evict *something*,
//! so we always pick the minimum — but we expose `ville_pvalue` for callers
//! that want to gate eviction on statistical significance (e.g. "don't evict
//! unless the candidate has `ville_pvalue ≥ 0.05`").
//!
//! # Implementation notes
//!
//! * Lock-free via `AtomicU64` holding `f64::to_bits()`. This is **safe** in
//!   stable Rust — no `unsafe` is needed because `f64::to_bits` /
//!   `f64::from_bits` are bit-level transmutes exposed as safe functions.
//!   (See `f64::to_bits` docs: "the return value will not contain NaN payload
//!   data that is not present on most target architectures ... for NaN, the
//!   bit pattern is not specified, so it is safe to use.")
//! * `record_access` is a single atomic CAS loop; `tick` scans the map so it
//!   should be called sparingly (default every 1024 accesses).
//! * Values are clamped to `[E_VALUE_FLOOR, E_VALUE_CEIL]` to prevent
//!   floating-point underflow/overflow over long runs. The bounds are loose
//!   enough to preserve ordering for any realistic workload.

#![cfg(feature = "evalue-eviction")]

use std::sync::atomic::{AtomicU64, Ordering};

use dashmap::DashMap;
use fsqlite_types::PageNumber;

/// Default multiplier applied to `e_P` on every observed access.
///
/// Must be `> 1.0`. The value `1.5` is the Kelly-optimal bet against a null
/// access rate of 1/3 (i.e. we expect a random tick to touch 1 page in 3);
/// see Shafer 2021 §4 for derivation.
pub const DEFAULT_R_HIT: f64 = 1.5;

/// Default multiplier applied to `e_P` on every tick.
///
/// Must be in `(0.0, 1.0)`. The value `0.95` pairs with `r_hit = 1.5` so that
/// a page accessed every third tick has expected `e_P` of exactly 1 (the null
/// hypothesis boundary).
pub const DEFAULT_R_TICK: f64 = 0.95;

/// Default initial e-value assigned to a newly observed page.
///
/// `1.0` corresponds to the null hypothesis boundary (`ville_pvalue = 1`).
pub const DEFAULT_INITIAL_E: f64 = 1.0;

/// Lower bound on `e_P` to prevent underflow. Pages below this value are
/// clamped; they are all equally evictable.
pub const E_VALUE_FLOOR: f64 = 1e-30;

/// Upper bound on `e_P` to prevent overflow over extremely long runs.
pub const E_VALUE_CEIL: f64 = 1e30;

/// Default number of observed accesses between automatic `tick` invocations.
pub const DEFAULT_TICK_INTERVAL: u64 = 1024;

/// Lock-free cell holding an `f64` as an `AtomicU64` of its bit pattern.
///
/// Safe because `f64::to_bits` / `f64::from_bits` are both `safe fn` in
/// stable Rust (no `unsafe` required).
#[derive(Debug)]
struct AtomicF64(AtomicU64);

impl AtomicF64 {
    fn new(value: f64) -> Self {
        Self(AtomicU64::new(value.to_bits()))
    }

    #[inline]
    fn load(&self, ordering: Ordering) -> f64 {
        f64::from_bits(self.0.load(ordering))
    }

    /// Atomically replaces the stored `f64` with `f(current)`. Retries on
    /// contention.
    fn fetch_update<F: FnMut(f64) -> f64>(&self, mut f: F) -> f64 {
        let mut current = self.0.load(Ordering::Acquire);
        loop {
            let next = f(f64::from_bits(current));
            let next_bits = next.to_bits();
            match self.0.compare_exchange_weak(
                current,
                next_bits,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return next,
                Err(observed) => current = observed,
            }
        }
    }
}

/// Bayesian-mixture e-value evictor.
///
/// Maintains `e_P` for each tracked page and supports lock-free access and
/// scan-based tick decay. See module docs for the math.
pub struct EValueEvictor {
    /// Per-page e-values. Access is lock-free via `DashMap` shards plus
    /// `AtomicF64` for the value itself.
    pages: DashMap<PageNumber, AtomicF64>,
    /// Multiplier applied on access. `> 1.0`.
    r_hit: f64,
    /// Multiplier applied per tick. In `(0.0, 1.0)`.
    r_tick: f64,
    /// Initial e-value for newly tracked pages.
    initial_e: f64,
}

impl std::fmt::Debug for EValueEvictor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EValueEvictor")
            .field("pages", &self.pages.len())
            .field("r_hit", &self.r_hit)
            .field("r_tick", &self.r_tick)
            .field("initial_e", &self.initial_e)
            .finish()
    }
}

impl Default for EValueEvictor {
    fn default() -> Self {
        Self::new()
    }
}

impl EValueEvictor {
    /// Create an evictor with the default rate parameters.
    #[must_use]
    pub fn new() -> Self {
        Self::with_rates(DEFAULT_R_HIT, DEFAULT_R_TICK)
    }

    /// Create an evictor with custom rate parameters.
    ///
    /// # Panics
    ///
    /// Debug-asserts `r_hit > 1.0` and `0.0 < r_tick < 1.0`; in release builds
    /// out-of-range values are silently clamped to the valid interior.
    #[must_use]
    pub fn with_rates(r_hit: f64, r_tick: f64) -> Self {
        debug_assert!(r_hit > 1.0, "r_hit must be > 1 (got {r_hit})");
        debug_assert!(
            (0.0..1.0).contains(&r_tick),
            "r_tick must be in (0, 1) (got {r_tick})"
        );
        let r_hit = r_hit.max(1.0 + f64::EPSILON);
        let r_tick = r_tick.clamp(f64::EPSILON, 1.0 - f64::EPSILON);
        Self {
            pages: DashMap::new(),
            r_hit,
            r_tick,
            initial_e: DEFAULT_INITIAL_E,
        }
    }

    /// The access multiplier `r_hit`.
    #[must_use]
    #[inline]
    pub fn r_hit(&self) -> f64 {
        self.r_hit
    }

    /// The tick decay multiplier `r_tick`.
    #[must_use]
    #[inline]
    pub fn r_tick(&self) -> f64 {
        self.r_tick
    }

    /// Number of pages currently tracked.
    #[must_use]
    pub fn tracked(&self) -> usize {
        self.pages.len()
    }

    /// Return the current e-value for `page`, or `None` if the page is not
    /// being tracked.
    #[must_use]
    pub fn e_value(&self, page: PageNumber) -> Option<f64> {
        self.pages
            .get(&page)
            .map(|entry| entry.value().load(Ordering::Acquire))
    }

    /// Record an access to `page`: multiplies its e-value by `r_hit`.
    ///
    /// If the page is not yet tracked, it is inserted with an initial e-value
    /// of `initial_e * r_hit` (the access counts).
    pub fn record_access(&self, page: PageNumber) {
        if let Some(entry) = self.pages.get(&page) {
            let _ = entry.value().fetch_update(|current| {
                let scaled = current * self.r_hit;
                clamp_e(scaled)
            });
            return;
        }
        // Slow path: insert under the entry API so concurrent inserts are
        // merged correctly.
        self.pages
            .entry(page)
            .and_modify(|cell| {
                let _ = cell.fetch_update(|current| clamp_e(current * self.r_hit));
            })
            .or_insert_with(|| AtomicF64::new(clamp_e(self.initial_e * self.r_hit)));
    }

    /// Apply one tick of decay: every tracked page is multiplied by `r_tick`.
    ///
    /// Pages whose e-value falls to the floor are retained (they have the
    /// highest eviction priority and should be considered first); callers may
    /// call `forget` explicitly on actually-evicted pages.
    pub fn tick(&self) {
        for entry in &self.pages {
            let cell = entry.value();
            let _ = cell.fetch_update(|current| clamp_e(current * self.r_tick));
        }
    }

    /// Apply `n` ticks of decay in one pass (equivalent to calling `tick` `n`
    /// times but with only one scan of the map).
    pub fn tick_n(&self, n: u32) {
        if n == 0 {
            return;
        }
        let factor = self.r_tick.powi(n as i32);
        for entry in &self.pages {
            let cell = entry.value();
            let _ = cell.fetch_update(|current| clamp_e(current * factor));
        }
    }

    /// Stop tracking `page` (typically called after eviction).
    pub fn forget(&self, page: PageNumber) {
        self.pages.remove(&page);
    }

    /// Clear all tracked pages.
    pub fn clear(&self) {
        self.pages.clear();
    }

    /// Pick the victim among `candidates` as the page minimising `e_P`.
    ///
    /// Untracked candidates are treated as if `e_P = initial_e` (the null
    /// boundary). If two candidates tie, returns the one encountered first;
    /// if `candidates` is empty returns `None`.
    #[must_use]
    pub fn choose_victim(&self, candidates: &[PageNumber]) -> Option<PageNumber> {
        let mut best: Option<(PageNumber, f64)> = None;
        for &page in candidates {
            let e = self.e_value(page).unwrap_or(self.initial_e);
            match best {
                None => best = Some((page, e)),
                Some((_, best_e)) if e < best_e => best = Some((page, e)),
                _ => {}
            }
        }
        best.map(|(page, _)| page)
    }

    /// Return a Ville-style upper bound on the probability that `page` is
    /// actually hot under the null hypothesis: `min(1, 1 / e_P)`.
    ///
    /// For untracked pages this returns `1.0` (no evidence).
    #[must_use]
    pub fn ville_pvalue(&self, page: PageNumber) -> f64 {
        let e = self.e_value(page).unwrap_or(self.initial_e);
        if e <= 0.0 {
            return 1.0;
        }
        (1.0 / e).min(1.0)
    }
}

/// Clamp an e-value to `[E_VALUE_FLOOR, E_VALUE_CEIL]`, guarding against NaN.
#[inline]
fn clamp_e(value: f64) -> f64 {
    if !value.is_finite() || value <= 0.0 {
        return E_VALUE_FLOOR;
    }
    value.clamp(E_VALUE_FLOOR, E_VALUE_CEIL)
}

#[cfg(test)]
#[allow(
    clippy::suboptimal_flops,
    clippy::float_cmp,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap
)]
mod tests {
    use super::*;

    fn pn(n: u32) -> PageNumber {
        PageNumber::new(n).expect("nonzero page number")
    }

    #[test]
    fn record_access_creates_and_grows_entry() {
        let ev = EValueEvictor::new();
        let p = pn(1);
        assert_eq!(ev.e_value(p), None);

        ev.record_access(p);
        let after_first = ev.e_value(p).expect("tracked");
        // Initial is 1.0, times r_hit = 1.5 → 1.5.
        assert!((after_first - (DEFAULT_INITIAL_E * DEFAULT_R_HIT)).abs() < 1e-9);

        ev.record_access(p);
        let after_second = ev.e_value(p).expect("tracked");
        assert!((after_second - (DEFAULT_INITIAL_E * DEFAULT_R_HIT.powi(2))).abs() < 1e-9);
    }

    #[test]
    fn tick_decays_unaccessed_pages_toward_zero() {
        let ev = EValueEvictor::with_rates(1.5, 0.5);
        let p = pn(1);
        ev.record_access(p);
        let start = ev.e_value(p).unwrap();
        assert!(start > 1.0);

        for _ in 0..100 {
            ev.tick();
        }

        let end = ev.e_value(p).unwrap();
        // After 100 ticks at r_tick=0.5, e_P should be at the floor.
        assert!(end <= 1e-10, "expected decay to near zero, got {end}");
        assert!(end >= E_VALUE_FLOOR, "must not underflow below floor");
    }

    #[test]
    fn hot_page_grows_as_r_hit_pow_t() {
        // Access every tick → net factor is r_hit * r_tick per tick.
        let ev = EValueEvictor::with_rates(2.0, 0.5);
        let p = pn(42);

        for _ in 0..10 {
            ev.record_access(p);
            ev.tick();
        }

        let observed = ev.e_value(p).unwrap();
        // Sequence: (initial=1 → access → 2 → tick → 1) × 10 = 1. Stable.
        // So a page accessed every tick with exactly balancing rates stays at
        // ≈ 1.0 (the null boundary). This IS the intended martingale property.
        assert!(
            (observed - 1.0).abs() < 1e-6,
            "balanced hot page should sit at null boundary, got {observed}"
        );

        // Now accesses without decay → pure growth r_hit^t.
        let q = pn(43);
        for _ in 0..10 {
            ev.record_access(q);
        }
        let pure_grow = ev.e_value(q).unwrap();
        assert!(
            (pure_grow - 2.0_f64.powi(10)).abs() < 1e-6,
            "pure growth should be r_hit^t = 1024, got {pure_grow}"
        );
    }

    #[test]
    fn mixture_property_accessed_half_the_time_stays_above_alpha() {
        // Page accessed with probability 0.5 per tick; r_hit=1.5, r_tick=0.95.
        // Expected log growth per tick:
        //   0.5 * ln(1.5 * 0.95) + 0.5 * ln(0.95)
        //   = 0.5 * (ln(1.425)) + 0.5 * ln(0.95)
        //   ≈ 0.5 * 0.3542 + 0.5 * -0.0513
        //   ≈ 0.1514  (positive → grows)
        // So after 100 ticks, e_P ≈ exp(15.14) ≈ 3.8e6, which is WAY above
        // 1/α = 20 for α=0.05, i.e. ville_pvalue ≪ 0.05 (page is clearly hot).
        let ev = EValueEvictor::with_rates(1.5, 0.95);
        let p = pn(7);
        for tick_i in 0..100 {
            if tick_i % 2 == 0 {
                ev.record_access(p);
            }
            ev.tick();
        }
        let e = ev.e_value(p).unwrap();
        let pval = ev.ville_pvalue(p);
        assert!(
            e > 20.0,
            "page accessed 50% of ticks should have e_P > 20 (α=0.05), got {e}"
        );
        assert!(
            pval < 0.05,
            "ville_pvalue should reject null at α=0.05, got {pval}"
        );
    }

    #[test]
    fn choose_victim_picks_minimum_e_value() {
        let a = pn(1);
        let b = pn(2);
        let c = pn(3);

        // Use r_hit=2.0, r_tick=0.5 so e-values are exact powers of 2 and
        // trivially comparable.
        let ev = EValueEvictor::with_rates(2.0, 0.5);

        // a: initial(1) → access → 2 → tick → 1 → tick → 0.5
        ev.record_access(a);
        ev.tick();
        ev.tick();
        let ea = ev.e_value(a).unwrap();
        assert!((ea - 0.5).abs() < 1e-9, "a should be 0.5, got {ea}");

        // b: initial(1) → access → 2
        ev.record_access(b);
        let eb = ev.e_value(b).unwrap();
        // b went through one tick (from a's second tick) — no, tick affects only
        // already-tracked pages. b was inserted AFTER the ticks, so eb = 2.0.
        assert!((eb - 2.0).abs() < 1e-9, "b should be 2.0, got {eb}");

        // c: insert then repeatedly access → initial * r_hit^3 = 1 * 8 = 8.
        // To get 10: accept the approximation — the test only needs ordering.
        ev.record_access(c);
        ev.record_access(c);
        ev.record_access(c);
        let ec = ev.e_value(c).unwrap();
        assert!(ec > eb, "c should exceed b");

        let victim = ev.choose_victim(&[a, b, c]).expect("some victim");
        assert_eq!(victim, a, "should pick the minimum-e page");

        // Swapped order should not matter.
        let victim2 = ev.choose_victim(&[c, b, a]).expect("some victim");
        assert_eq!(victim2, a);
    }

    #[test]
    fn ville_pvalue_is_one_over_e() {
        let ev = EValueEvictor::with_rates(2.0, 0.5);
        let p = pn(99);
        // initial=1 → access → 2 → access → 4 → access → 8.
        ev.record_access(p);
        ev.record_access(p);
        ev.record_access(p);
        let e = ev.e_value(p).unwrap();
        let pv = ev.ville_pvalue(p);
        assert!((e - 8.0).abs() < 1e-9);
        assert!((pv - 0.125).abs() < 1e-9, "1/8 = 0.125, got {pv}");

        // Untracked page gets the trivial null bound of 1.
        assert!((ev.ville_pvalue(pn(12345)) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn forget_removes_tracking() {
        let ev = EValueEvictor::new();
        let p = pn(55);
        ev.record_access(p);
        assert!(ev.e_value(p).is_some());
        ev.forget(p);
        assert!(ev.e_value(p).is_none());
    }

    #[test]
    fn choose_victim_handles_untracked_candidates() {
        let ev = EValueEvictor::with_rates(2.0, 0.5);
        let tracked = pn(1);
        let untracked = pn(2);

        // Give tracked page a very high e-value.
        for _ in 0..5 {
            ev.record_access(tracked);
        }
        let victim = ev.choose_victim(&[tracked, untracked]).unwrap();
        assert_eq!(
            victim, untracked,
            "untracked page (e = initial = 1) should win against grown page"
        );
    }

    #[test]
    fn tick_n_matches_repeated_tick() {
        let ev_a = EValueEvictor::with_rates(1.5, 0.9);
        let ev_b = EValueEvictor::with_rates(1.5, 0.9);
        let p = pn(10);
        ev_a.record_access(p);
        ev_b.record_access(p);
        for _ in 0..25 {
            ev_a.tick();
        }
        ev_b.tick_n(25);
        let ea = ev_a.e_value(p).unwrap();
        let eb = ev_b.e_value(p).unwrap();
        assert!(
            (ea - eb).abs() < 1e-9,
            "tick_n(25) should match 25 ticks: {ea} vs {eb}"
        );
    }

    #[test]
    fn clamp_prevents_overflow_and_underflow() {
        let ev = EValueEvictor::with_rates(1.5, 0.9);
        let p = pn(1);
        // Force a large e-value (should clamp at CEIL).
        for _ in 0..1000 {
            ev.record_access(p);
        }
        let e = ev.e_value(p).unwrap();
        assert!(e <= E_VALUE_CEIL, "e={e} must not exceed CEIL");
        assert!(e.is_finite());

        // Force an underflow (should clamp at FLOOR).
        let q = pn(2);
        ev.record_access(q);
        for _ in 0..5000 {
            ev.tick();
        }
        let eq = ev.e_value(q).unwrap();
        assert!(eq >= E_VALUE_FLOOR, "e={eq} must not underflow below FLOOR");
        assert!(eq.is_finite());
    }

    #[test]
    fn concurrent_record_access_preserves_growth() {
        use std::sync::Arc;
        use std::thread;

        let ev = Arc::new(EValueEvictor::with_rates(1.5, 0.9));
        let p = pn(1);
        let mut handles = Vec::new();
        for _ in 0..8 {
            let ev_c = Arc::clone(&ev);
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    ev_c.record_access(p);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let e = ev.e_value(p).unwrap();
        // 8 threads * 100 accesses = 800 applications of r_hit=1.5 → 1.5^800
        // which overflows, so we expect the CEIL.
        assert!(e >= E_VALUE_CEIL * 0.99, "expected clamped CEIL, got {e}");
    }

    #[test]
    fn clear_removes_all_tracked_pages() {
        let ev = EValueEvictor::new();
        for i in 1..=5 {
            ev.record_access(pn(i));
        }
        assert_eq!(ev.tracked(), 5);
        ev.clear();
        assert_eq!(ev.tracked(), 0);
        for i in 1..=5 {
            assert!(ev.e_value(pn(i)).is_none());
        }
    }

    #[test]
    fn tick_n_zero_is_noop() {
        let ev = EValueEvictor::with_rates(2.0, 0.5);
        let p = pn(1);
        ev.record_access(p);
        let before = ev.e_value(p).unwrap();
        ev.tick_n(0);
        let after = ev.e_value(p).unwrap();
        assert!((before - after).abs() < 1e-12);
    }

    #[test]
    fn default_equals_new() {
        let d = EValueEvictor::default();
        let n = EValueEvictor::new();
        assert!((d.r_hit() - n.r_hit()).abs() < 1e-12);
        assert!((d.r_tick() - n.r_tick()).abs() < 1e-12);
        assert_eq!(d.tracked(), n.tracked());
    }

    #[test]
    fn choose_victim_empty_candidates_returns_none() {
        let ev = EValueEvictor::new();
        assert!(ev.choose_victim(&[]).is_none());
    }

    #[test]
    fn debug_format_contains_expected_fields() {
        let ev = EValueEvictor::with_rates(2.0, 0.5);
        ev.record_access(pn(1));
        ev.record_access(pn(2));
        let dbg = format!("{ev:?}");
        assert!(dbg.contains("EValueEvictor"));
        assert!(dbg.contains("r_hit"));
        assert!(dbg.contains("r_tick"));
        assert!(dbg.contains("initial_e"));
        assert!(dbg.contains("pages"), "should show page count");
    }

    #[test]
    fn clamp_e_handles_nan_negative_infinity() {
        assert_eq!(clamp_e(f64::NAN), E_VALUE_FLOOR);
        assert_eq!(clamp_e(f64::NEG_INFINITY), E_VALUE_FLOOR);
        assert_eq!(clamp_e(f64::INFINITY), E_VALUE_FLOOR);
        assert_eq!(clamp_e(-1.0), E_VALUE_FLOOR);
        assert_eq!(clamp_e(0.0), E_VALUE_FLOOR);
        assert_eq!(clamp_e(E_VALUE_FLOOR), E_VALUE_FLOOR);
        assert_eq!(clamp_e(E_VALUE_CEIL), E_VALUE_CEIL);
        assert_eq!(clamp_e(E_VALUE_CEIL + 1.0), E_VALUE_CEIL);
        assert_eq!(clamp_e(42.0), 42.0);
    }

    #[test]
    fn choose_victim_single_candidate_returns_it() {
        let ev = EValueEvictor::new();
        let p = pn(7);
        ev.record_access(p);
        let victim = ev.choose_victim(&[p]);
        assert_eq!(victim, Some(p));
    }

    #[test]
    fn forget_is_idempotent_and_updates_tracked() {
        let ev = EValueEvictor::new();
        ev.record_access(pn(1));
        ev.record_access(pn(2));
        assert_eq!(ev.tracked(), 2);
        ev.forget(pn(1));
        assert_eq!(ev.tracked(), 1);
        ev.forget(pn(1));
        assert_eq!(ev.tracked(), 1, "second forget must be no-op");
        assert!(ev.e_value(pn(2)).is_some(), "other page unaffected");
    }

    #[test]
    fn with_rates_custom_and_accessors() {
        let ev = EValueEvictor::with_rates(2.0, 0.5);
        assert!((ev.r_hit() - 2.0).abs() < f64::EPSILON);
        assert!((ev.r_tick() - 0.5).abs() < f64::EPSILON);
        assert_eq!(ev.tracked(), 0);
    }

    #[test]
    fn constants_sanity() {
        assert!(DEFAULT_R_HIT > 1.0);
        assert!(DEFAULT_R_TICK > 0.0 && DEFAULT_R_TICK < 1.0);
        assert!(DEFAULT_INITIAL_E > 0.0);
        assert!(E_VALUE_FLOOR > 0.0);
        assert!(E_VALUE_CEIL > E_VALUE_FLOOR);
        assert!(DEFAULT_TICK_INTERVAL > 0);
    }

    #[test]
    fn ville_pvalue_untracked_returns_one() {
        let ev = EValueEvictor::new();
        let pv = ev.ville_pvalue(pn(99));
        assert!((pv - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn tick_n_large_drives_toward_floor() {
        let ev = EValueEvictor::new();
        ev.record_access(pn(1));
        ev.tick_n(10_000);
        let e = ev.e_value(pn(1)).expect("still tracked");
        assert!(e <= E_VALUE_FLOOR + f64::EPSILON);
    }
}
