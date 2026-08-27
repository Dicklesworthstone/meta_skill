//! Measurement protocol for determining renderable dimensions.
//!
//! This module provides the `Measurement` struct and associated functions
//! for calculating the minimum and maximum cell widths required to render
//! content in the terminal.

use std::cmp::{max, min};

use crate::console::{Console, ConsoleOptions};

/// Measurement of a renderable's width requirements.
///
/// A `Measurement` captures the minimum and maximum cell widths that a
/// renderable needs. The minimum is the tightest the content can be
/// compressed, while maximum is how wide it is when unconstrained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Measurement {
    /// Minimum cells required (cannot render narrower).
    pub minimum: usize,
    /// Maximum cells required (ideal unconstrained width).
    pub maximum: usize,
}

impl Measurement {
    /// Create a new measurement.
    #[must_use]
    pub const fn new(minimum: usize, maximum: usize) -> Self {
        if minimum <= maximum {
            Self { minimum, maximum }
        } else {
            Self {
                minimum: maximum,
                maximum: minimum,
            }
        }
    }

    /// Create a measurement where min equals max.
    #[must_use]
    pub const fn exact(size: usize) -> Self {
        Self {
            minimum: size,
            maximum: size,
        }
    }

    /// Create a zero measurement.
    #[must_use]
    pub const fn zero() -> Self {
        Self {
            minimum: 0,
            maximum: 0,
        }
    }

    /// Get the span (difference) between minimum and maximum.
    #[must_use]
    pub const fn span(&self) -> usize {
        self.maximum.saturating_sub(self.minimum)
    }

    /// Normalize the measurement to ensure min <= max and both >= 0.
    #[must_use]
    pub fn normalize(&self) -> Self {
        Self::new(self.minimum, self.maximum)
    }

    /// Constrain the maximum to a given width.
    ///
    /// Both minimum and maximum will be clamped to not exceed `width`.
    #[must_use]
    pub fn with_maximum(&self, width: usize) -> Self {
        Self {
            minimum: min(self.minimum, width),
            maximum: min(self.maximum, width),
        }
    }

    /// Ensure the minimum is at least `width`.
    ///
    /// Both minimum and maximum will be at least `width`.
    #[must_use]
    pub fn with_minimum(&self, width: usize) -> Self {
        Self {
            minimum: max(self.minimum, width),
            maximum: max(self.maximum, width),
        }
    }

    /// Clamp measurement to optional min/max bounds.
    #[must_use]
    pub fn clamp(&self, min_width: Option<usize>, max_width: Option<usize>) -> Self {
        let mut result = *self;
        if let Some(min_w) = min_width {
            result = result.with_minimum(min_w);
        }
        if let Some(max_w) = max_width {
            result = result.with_maximum(max_w);
        }
        result
    }

    /// Combine two measurements, taking the tighter constraints.
    ///
    /// The combined minimum is the max of both minimums,
    /// and the combined maximum is the max of both maximums.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        Self {
            minimum: max(self.minimum, other.minimum),
            maximum: max(self.maximum, other.maximum),
        }
    }

    /// Intersect two measurements, taking the overlapping range.
    ///
    /// Returns the intersection of the two ranges, or None if they don't overlap.
    #[must_use]
    pub fn intersect(&self, other: &Self) -> Option<Self> {
        let min_val = max(self.minimum, other.minimum);
        let max_val = min(self.maximum, other.maximum);

        if min_val <= max_val {
            Some(Self {
                minimum: min_val,
                maximum: max_val,
            })
        } else {
            None
        }
    }

    /// Add a constant width to both minimum and maximum.
    #[must_use]
    pub fn add(&self, width: usize) -> Self {
        Self {
            minimum: self.minimum.saturating_add(width),
            maximum: self.maximum.saturating_add(width),
        }
    }

    /// Subtract a constant width from both minimum and maximum.
    #[must_use]
    pub fn subtract(&self, width: usize) -> Self {
        Self {
            minimum: self.minimum.saturating_sub(width),
            maximum: self.maximum.saturating_sub(width),
        }
    }

    /// Check if a width fits within this measurement.
    #[must_use]
    pub fn fits(&self, width: usize) -> bool {
        width >= self.minimum && width <= self.maximum
    }

    /// Compute measurement with optional renderable measurement logic.
    ///
    /// If no measurement is provided, defaults to (0, `max_width`).
    #[must_use]
    pub fn get(
        console: &Console,
        options: &ConsoleOptions,
        renderable: Option<&dyn RichMeasure>,
    ) -> Self {
        let max_width = options.max_width;
        if max_width < 1 {
            return Self::zero();
        }

        if let Some(renderable) = renderable {
            renderable
                .rich_measure(console, options)
                .normalize()
                .with_maximum(max_width)
                .normalize()
        } else {
            Self {
                minimum: 0,
                maximum: max_width,
            }
        }
    }
}

/// Trait for renderables that can provide a measurement.
pub trait RichMeasure {
    /// Measure minimum and maximum width requirements for a renderable.
    fn rich_measure(&self, console: &Console, options: &ConsoleOptions) -> Measurement;
}

impl std::ops::Add for Measurement {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            minimum: self.minimum.saturating_add(rhs.minimum),
            maximum: self.maximum.saturating_add(rhs.maximum),
        }
    }
}

impl std::ops::AddAssign for Measurement {
    fn add_assign(&mut self, rhs: Self) {
        self.minimum = self.minimum.saturating_add(rhs.minimum);
        self.maximum = self.maximum.saturating_add(rhs.maximum);
    }
}

/// Combine multiple measurements by taking the union.
///
/// The resulting minimum is the max of all minimums (tightest constraint),
/// and the maximum is the max of all maximums (most flexible).
#[must_use]
pub fn measure_union(measurements: &[Measurement]) -> Measurement {
    if measurements.is_empty() {
        return Measurement::zero();
    }

    Measurement {
        minimum: measurements
            .iter()
            .map(|m| m.normalize().minimum)
            .max()
            .unwrap_or(0),
        maximum: measurements
            .iter()
            .map(|m| m.normalize().maximum)
            .max()
            .unwrap_or(0),
    }
}

/// Sum multiple measurements.
///
/// Useful for measuring horizontal concatenation of renderables.
#[must_use]
pub fn measure_sum(measurements: &[Measurement]) -> Measurement {
    if measurements.is_empty() {
        return Measurement::zero();
    }

    Measurement {
        minimum: measurements.iter().map(|m| m.normalize().minimum).sum(),
        maximum: measurements.iter().map(|m| m.normalize().maximum).sum(),
    }
}

/// Measure a list of renderables and combine their measurements.
#[must_use]
pub fn measure_renderables(
    console: &Console,
    options: &ConsoleOptions,
    renderables: &[&dyn RichMeasure],
) -> Measurement {
    if renderables.is_empty() {
        return Measurement::zero();
    }

    let mut minimum = 0;
    let mut maximum = 0;

    for renderable in renderables {
        let measurement = Measurement::get(console, options, Some(*renderable));
        minimum = minimum.max(measurement.minimum);
        maximum = maximum.max(measurement.maximum);
    }

    Measurement { minimum, maximum }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::Console;

    struct DummyMeasure {
        measurement: Measurement,
    }

    impl RichMeasure for DummyMeasure {
        fn rich_measure(&self, _console: &Console, _options: &ConsoleOptions) -> Measurement {
            self.measurement
        }
    }

    #[test]
    fn test_measurement_new() {
        let m = Measurement::new(5, 10);
        assert_eq!(m.minimum, 5);
        assert_eq!(m.maximum, 10);

        let reversed = Measurement::new(10, 5);
        assert_eq!(reversed.minimum, 5);
        assert_eq!(reversed.maximum, 10);
    }

    #[test]
    fn test_measurement_exact() {
        let m = Measurement::exact(7);
        assert_eq!(m.minimum, 7);
        assert_eq!(m.maximum, 7);
        assert_eq!(m.span(), 0);
    }

    #[test]
    fn test_measurement_span() {
        let m = Measurement::new(5, 10);
        assert_eq!(m.span(), 5);
    }

    #[test]
    fn test_measurement_normalize() {
        let m = Measurement::new(10, 5); // Wrong order
        let normalized = m.normalize();
        assert_eq!(normalized.minimum, 5);
        assert_eq!(normalized.maximum, 10);
    }

    #[test]
    fn test_with_maximum() {
        let m = Measurement::new(5, 20);
        let constrained = m.with_maximum(10);
        assert_eq!(constrained.minimum, 5);
        assert_eq!(constrained.maximum, 10);
    }

    #[test]
    fn test_with_maximum_clamps_min() {
        let m = Measurement::new(15, 20);
        let constrained = m.with_maximum(10);
        assert_eq!(constrained.minimum, 10);
        assert_eq!(constrained.maximum, 10);
    }

    #[test]
    fn test_with_minimum() {
        let m = Measurement::new(5, 20);
        let constrained = m.with_minimum(10);
        assert_eq!(constrained.minimum, 10);
        assert_eq!(constrained.maximum, 20);
    }

    #[test]
    fn test_clamp() {
        let m = Measurement::new(3, 30);
        let clamped = m.clamp(Some(5), Some(20));
        assert_eq!(clamped.minimum, 5);
        assert_eq!(clamped.maximum, 20);
    }

    #[test]
    fn test_clamp_inverted_bounds() {
        let m = Measurement::new(3, 30);
        let clamped = m.clamp(Some(40), Some(20));
        assert_eq!(clamped.minimum, 20);
        assert_eq!(clamped.maximum, 20);
    }

    #[test]
    fn test_union() {
        let a = Measurement::new(5, 15);
        let b = Measurement::new(10, 12);
        let union = a.union(&b);
        assert_eq!(union.minimum, 10); // max of minimums
        assert_eq!(union.maximum, 15); // max of maximums
    }

    #[test]
    fn test_intersect() {
        let a = Measurement::new(5, 15);
        let b = Measurement::new(10, 20);
        let intersect = a.intersect(&b).unwrap();
        assert_eq!(intersect.minimum, 10);
        assert_eq!(intersect.maximum, 15);
    }

    #[test]
    fn test_intersect_no_overlap() {
        let a = Measurement::new(5, 10);
        let b = Measurement::new(15, 20);
        assert!(a.intersect(&b).is_none());
    }

    #[test]
    fn test_add_measurement() {
        let a = Measurement::new(5, 10);
        let b = Measurement::new(3, 7);
        let sum = a + b;
        assert_eq!(sum.minimum, 8);
        assert_eq!(sum.maximum, 17);
    }

    #[test]
    fn test_add_width() {
        let m = Measurement::new(5, 10);
        let added = m.add(3);
        assert_eq!(added.minimum, 8);
        assert_eq!(added.maximum, 13);
    }

    #[test]
    fn test_subtract_width() {
        let m = Measurement::new(5, 10);
        let subtracted = m.subtract(3);
        assert_eq!(subtracted.minimum, 2);
        assert_eq!(subtracted.maximum, 7);
    }

    #[test]
    fn test_fits() {
        let m = Measurement::new(5, 10);
        assert!(!m.fits(4));
        assert!(m.fits(5));
        assert!(m.fits(7));
        assert!(m.fits(10));
        assert!(!m.fits(11));
    }

    #[test]
    fn test_measure_union() {
        let measurements = vec![
            Measurement::new(5, 10),
            Measurement::new(3, 15),
            Measurement::new(8, 12),
        ];
        let union = measure_union(&measurements);
        assert_eq!(union.minimum, 8); // max of minimums
        assert_eq!(union.maximum, 15); // max of maximums
    }

    #[test]
    fn test_measure_union_empty() {
        let union = measure_union(&[]);
        assert_eq!(union.minimum, 0);
        assert_eq!(union.maximum, 0);
    }

    #[test]
    fn test_measure_sum() {
        let measurements = vec![Measurement::new(5, 10), Measurement::new(3, 7)];
        let sum = measure_sum(&measurements);
        assert_eq!(sum.minimum, 8);
        assert_eq!(sum.maximum, 17);
    }

    #[test]
    fn test_measurement_get_none() {
        let console = Console::new();
        let mut options = console.options();
        options.max_width = 10;

        let m = Measurement::get(&console, &options, None);
        assert_eq!(m.minimum, 0);
        assert_eq!(m.maximum, 10);
    }

    #[test]
    fn test_measurement_get_clamped_and_normalized() {
        let console = Console::new();
        let mut options = console.options();
        options.max_width = 12;

        let dummy = DummyMeasure {
            measurement: Measurement {
                minimum: 20,
                maximum: 10,
            },
        };

        let m = Measurement::get(&console, &options, Some(&dummy));
        assert_eq!(m.minimum, 10);
        assert_eq!(m.maximum, 12);
    }

    #[test]
    fn test_measurement_get_zero_width() {
        let console = Console::new();
        let mut options = console.options();
        options.max_width = 0;

        let dummy = DummyMeasure {
            measurement: Measurement::new(5, 10),
        };

        let m = Measurement::get(&console, &options, Some(&dummy));
        assert_eq!(m.minimum, 0);
        assert_eq!(m.maximum, 0);
    }

    #[test]
    fn test_measure_renderables() {
        let console = Console::new();
        let mut options = console.options();
        options.max_width = 15;

        let a = DummyMeasure {
            measurement: Measurement::new(5, 50),
        };
        let b = DummyMeasure {
            measurement: Measurement::new(8, 12),
        };

        let combined = measure_renderables(&console, &options, &[&a, &b]);
        assert_eq!(combined.minimum, 8);
        assert_eq!(combined.maximum, 15);
    }
}
