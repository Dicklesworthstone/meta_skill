//! Timing utilities and deterministic RNG for demo_showcase.

// Timing helpers prepared for animated scene implementations
#![allow(dead_code)]

use std::ops::Range;
use std::time::Duration;

/// Centralized timing policy for demo_showcase.
///
/// - `speed`: animation speed multiplier. Higher is faster.
/// - `quick`: aggressively shortens long delays (CI/dev mode).
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    speed: f64,
    quick: bool,
    quick_max_sleep: Duration,
}

impl Timing {
    /// Construct a timing policy.
    ///
    /// `speed` must be finite and > 0. The CLI enforces this already, but we
    /// keep `Timing` robust so other callers can't accidentally footgun.
    #[must_use]
    pub fn new(speed: f64, quick: bool) -> Self {
        let speed = if speed.is_finite() && speed > 0.0 {
            speed
        } else {
            1.0
        };

        Self {
            speed,
            quick,
            quick_max_sleep: Duration::from_millis(10),
        }
    }

    #[must_use]
    pub const fn quick(&self) -> bool {
        self.quick
    }

    /// Scale a base duration according to `--speed` / `--quick`.
    ///
    /// In quick mode, long sleeps are clamped so the demo finishes quickly
    /// without turning the main loop into a busy-spin.
    #[must_use]
    pub fn scale(&self, base: Duration) -> Duration {
        if base.is_zero() {
            return base;
        }

        let scaled_nanos = (base.as_nanos() as f64 / self.speed).round();
        let scaled_nanos = scaled_nanos.clamp(0.0, u64::MAX as f64);
        let mut scaled = Duration::from_nanos(scaled_nanos as u64);

        if self.quick {
            scaled = scaled.min(self.quick_max_sleep);
        }

        scaled
    }

    /// Sleep for a scaled duration.
    pub fn sleep(&self, base: Duration) {
        std::thread::sleep(self.scale(base));
    }
}

/// A tiny deterministic PRNG for demo_showcase.
///
/// This avoids pulling in a full RNG dependency for the binary, and provides
/// stable outputs for snapshot tests (and human screenshot baselines).
#[derive(Debug, Clone)]
pub struct DemoRng {
    state: u64,
}

impl DemoRng {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// SplitMix64: fast, high-quality, deterministic.
    #[must_use]
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Generate a random u64 in the half-open range `[start, end)`.
    ///
    /// # Panics
    /// Panics if `range.start >= range.end` (empty or inverted range).
    #[must_use]
    pub fn gen_range(&mut self, range: Range<u64>) -> u64 {
        assert!(
            range.start < range.end,
            "gen_range requires non-empty range: {}..{}",
            range.start,
            range.end
        );
        let span = range.end - range.start;
        range.start + (self.next_u64() % span)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_scales_duration() {
        let normal = Timing::new(1.0, false);
        assert_eq!(
            normal.scale(Duration::from_millis(100)),
            Duration::from_millis(100)
        );

        let faster = Timing::new(2.0, false);
        assert_eq!(
            faster.scale(Duration::from_millis(100)),
            Duration::from_millis(50)
        );

        let slower = Timing::new(0.5, false);
        assert_eq!(
            slower.scale(Duration::from_millis(100)),
            Duration::from_millis(200)
        );
    }

    #[test]
    fn quick_clamps_long_sleeps() {
        let quick = Timing::new(1.0, true);
        let scaled = quick.scale(Duration::from_secs(2));
        assert!(scaled <= Duration::from_millis(10));
    }

    #[test]
    fn rng_is_deterministic() {
        let mut a = DemoRng::new(42);
        let mut b = DemoRng::new(42);

        for _ in 0..10 {
            assert_eq!(a.next_u64(), b.next_u64());
        }

        let mut c = DemoRng::new(43);
        assert_ne!(a.next_u64(), c.next_u64());
    }

    #[test]
    fn rng_range_is_in_bounds() {
        let mut rng = DemoRng::new(1);
        for _ in 0..100 {
            let value = rng.gen_range(5..10);
            assert!((5..10).contains(&value));
        }
    }
}
