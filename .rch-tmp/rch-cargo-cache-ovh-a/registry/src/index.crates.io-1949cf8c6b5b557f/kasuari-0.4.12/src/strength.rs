//! Contains useful constants and functions for producing strengths for use in the constraint
//! solver. Each constraint added to the solver has an associated strength specifying the precedence
//! the solver should impose when choosing which constraints to enforce. It will try to enforce all
//! constraints, but if that is impossible the lowest strength constraints are the first to be
//! violated.
//!
//! Strengths are simply real numbers. The strongest legal strength is 1,001,001,000.0. The weakest
//! is 0.0. For convenience constants are declared for commonly used strengths. These are
//! [`REQUIRED`], [`STRONG`], [`MEDIUM`] and [`WEAK`]. Feel free to multiply these by other values
//! to get intermediate strengths. Note that the solver will clip given strengths to the legal
//! range.
//!
//! [`REQUIRED`] signifies a constraint that cannot be violated under any circumstance. Use this
//! special strength sparingly, as the solver will fail completely if it find that not all of the
//! [`REQUIRED`] constraints can be satisfied. The other strengths represent fallible constraints.
//! These should be the most commonly used strenghts for use cases where violating a constraint is
//! acceptable or even desired.
//!
//! The solver will try to get as close to satisfying the constraints it violates as possible,
//! strongest first. This behaviour can be used (for example) to provide a "default" value for a
//! variable should no other stronger constraints be put upon it.

use core::ops;

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Strength(f64);

impl Strength {
    /// The required strength for a constraint. This is the strongest possible strength.
    pub const REQUIRED: Strength = Strength(1_001_001_000.0);

    /// A strong strength for a constraint. This is weaker than `REQUIRED` but stronger than
    /// `MEDIUM`.
    pub const STRONG: Strength = Strength(1_000_000.0);

    /// A medium strength for a constraint. This is weaker than `STRONG` but stronger than `WEAK`.
    pub const MEDIUM: Strength = Strength(1_000.0);

    /// A weak strength for a constraint. This is weaker than `MEDIUM` but stronger than `0.0`.
    pub const WEAK: Strength = Strength(1.0);

    /// The weakest possible strength for a constraint. This is weaker than `WEAK`.
    pub const ZERO: Strength = Strength(0.0);

    /// Create a new strength with the given value, clipped to the legal range (0.0, REQUIRED)
    #[inline]
    pub const fn new(value: f64) -> Self {
        Self(value.clamp(0.0, Self::REQUIRED.value()))
    }

    /// Create a constraint as a linear combination of STRONG, MEDIUM and WEAK strengths.
    ///
    /// Each weight is multiplied by the multiplier, clamped to the legal range and then multiplied
    /// by the corresponding strength. The resulting strengths are then summed.
    #[inline]
    pub const fn create(strong: f64, medium: f64, weak: f64, multiplier: f64) -> Self {
        let strong = (strong * multiplier).clamp(0.0, 1000.0) * Self::STRONG.value();
        let medium = (medium * multiplier).clamp(0.0, 1000.0) * Self::MEDIUM.value();
        let weak = (weak * multiplier).clamp(0.0, 1000.0) * Self::WEAK.value();
        Self::new(strong + medium + weak)
    }

    /// The value of the strength
    #[inline]
    pub const fn value(&self) -> f64 {
        self.0
    }

    /// Add two strengths together, clamping the result to the legal range
    #[inline]
    pub const fn add(self, rhs: Self) -> Self {
        Self::new(self.0 + rhs.0)
    }

    /// Subtract one strength from another, clipping the result to the legal range
    #[inline]
    pub const fn sub(self, rhs: Self) -> Self {
        Self::new(self.0 - rhs.0)
    }

    /// Multiply a strength by a scalar, clipping the result to the legal range
    #[inline]
    pub const fn mul_f64(self, rhs: f64) -> Self {
        Self::new(self.0 * rhs)
    }

    /// Multiply a strength by a scalar, clipping the result to the legal range
    #[inline]
    pub const fn mul_f32(self, rhs: f32) -> Self {
        Self::new(self.0 * rhs as f64)
    }

    /// Divide a strength by a scalar, clipping the result to the legal range
    #[inline]
    pub const fn div_f64(self, rhs: f64) -> Self {
        Self::new(self.0 / rhs)
    }

    /// Divide a strength by a scalar, clipping the result to the legal range
    #[inline]
    pub const fn div_f32(self, rhs: f32) -> Self {
        Self::new(self.0 / rhs as f64)
    }
}

impl ops::Add<Strength> for Strength {
    type Output = Self;

    /// Add two strengths together, clipping the result to the legal range
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self::add(self, rhs)
    }
}

impl ops::Sub<Strength> for Strength {
    type Output = Strength;

    /// Subtract one strength from another, clipping the result to the legal range
    #[inline]
    fn sub(self, rhs: Strength) -> Strength {
        Self::sub(self, rhs)
    }
}

impl ops::AddAssign<Strength> for Strength {
    /// Perform an in-place addition of two strengths, clipping the result to the legal range
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl ops::SubAssign<Strength> for Strength {
    /// Perform an in-place subtraction of two strengths, clipping the result to the legal range
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl ops::Mul<f64> for Strength {
    type Output = Strength;

    /// Multiply a strength by a scalar, clipping the result to the legal range
    #[inline]
    fn mul(self, rhs: f64) -> Strength {
        self.mul_f64(rhs)
    }
}

impl ops::Mul<Strength> for f64 {
    type Output = Strength;

    /// Multiply a scalar by a strength, clipping the result to the legal range
    #[inline]
    fn mul(self, rhs: Strength) -> Strength {
        rhs.mul_f64(self)
    }
}

impl ops::MulAssign<f64> for Strength {
    /// Perform an in-place multiplication of a strength by a scalar, clipping the result to the
    /// legal range
    #[inline]
    fn mul_assign(&mut self, rhs: f64) {
        *self = *self * rhs;
    }
}

impl core::cmp::Ord for Strength {
    #[inline]
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.0.partial_cmp(&other.0).unwrap()
    }
}

impl core::cmp::PartialOrd for Strength {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl core::cmp::Eq for Strength {}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::under(-1.0, Strength::ZERO)]
    #[case::min(0.0, Strength::ZERO)]
    #[case::weak(1.0, Strength::WEAK)]
    #[case::medium(1_000.0, Strength::MEDIUM)]
    #[case::strong(1_000_000.0, Strength::STRONG)]
    #[case::required(1_001_001_000.0, Strength::REQUIRED)]
    #[case::over(1_001_001_001.0, Strength::REQUIRED)]
    fn new(#[case] value: f64, #[case] expected: Strength) {
        let strength = Strength::new(value);
        assert_eq!(strength, expected);
    }

    #[rstest]
    #[case::all_zeroes(0.0, 0.0, 0.0, 1.0, Strength::ZERO)]
    #[case::weak(0.0, 0.0, 1.0, 1.0, Strength::WEAK)]
    #[case::medium(0.0, 1.0, 0.0, 1.0, Strength::MEDIUM)]
    #[case::strong(1.0, 0.0, 0.0, 1.0, Strength::STRONG)]
    #[case::weak_clip(0.0, 0.0, 1000.0, 2.0, Strength::MEDIUM)]
    #[case::medium_clip(0.0, 1000.0, 0.0, 2.0, Strength::STRONG)]
    #[case::strong_clip(1000.0, 0.0, 0.0, 2.0, 1000.0 * Strength::STRONG)]
    #[case::all_non_zero(1.0, 1.0, 1.0, 1.0, Strength::STRONG + Strength::MEDIUM + Strength::WEAK)]
    #[case::multiplier(1.0, 1.0, 1.0, 2.0, 2.0 * (Strength::STRONG + Strength::MEDIUM + Strength::WEAK))]
    #[case::max(1000.0, 1000.0, 1000.0, 1.0, Strength::REQUIRED)]
    fn create(
        #[case] strong: f64,
        #[case] medium: f64,
        #[case] weak: f64,
        #[case] multiplier: f64,
        #[case] expected: Strength,
    ) {
        let strength = Strength::create(strong, medium, weak, multiplier);
        assert_eq!(strength, expected);
    }

    #[rstest]
    #[case::zero_plus_zero(Strength::ZERO, Strength::ZERO, Strength::ZERO)]
    #[case::zero_plus_weak(Strength::ZERO, Strength::WEAK, Strength::WEAK)]
    #[case::weak_plus_zero(Strength::WEAK, Strength::ZERO, Strength::WEAK)]
    #[case::weak_plus_weak(Strength::WEAK, Strength::WEAK, Strength::new(2.0))]
    #[case::weak_plus_medium(Strength::WEAK, Strength::MEDIUM, Strength::new(1001.0))]
    #[case::medium_plus_strong(Strength::MEDIUM, Strength::STRONG, Strength::new(1_001_000.0))]
    #[case::strong_plus_required(Strength::STRONG, Strength::REQUIRED, Strength::REQUIRED)]
    fn add(#[case] lhs: Strength, #[case] rhs: Strength, #[case] expected: Strength) {
        let result = lhs + rhs;
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case::zero_plus_zero(Strength::ZERO, Strength::ZERO, Strength::ZERO)]
    #[case::zero_plus_weak(Strength::ZERO, Strength::WEAK, Strength::WEAK)]
    #[case::weak_plus_zero(Strength::WEAK, Strength::ZERO, Strength::WEAK)]
    #[case::weak_plus_weak(Strength::WEAK, Strength::WEAK, Strength::new(2.0))]
    #[case::weak_plus_medium(Strength::WEAK, Strength::MEDIUM, Strength::new(1001.0))]
    #[case::medium_plus_strong(Strength::MEDIUM, Strength::STRONG, Strength::new(1_001_000.0))]
    #[case::saturate_high(Strength::STRONG, Strength::REQUIRED, Strength::REQUIRED)]
    fn add_assign(#[case] lhs: Strength, #[case] rhs: Strength, #[case] expected: Strength) {
        let mut result = lhs;
        result += rhs;
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case::saturate_low(Strength::ZERO, Strength::WEAK, Strength::ZERO)]
    #[case::zero_minus_zero(Strength::ZERO, Strength::ZERO, Strength::ZERO)]
    #[case::weak_minus_zero(Strength::WEAK, Strength::ZERO, Strength::WEAK)]
    #[case::weak_minus_weak(Strength::WEAK, Strength::WEAK, Strength::ZERO)]
    #[case::medium_minus_weak(Strength::MEDIUM, Strength::WEAK, Strength::new(999.0))]
    #[case::strong_minus_medium(Strength::STRONG, Strength::MEDIUM, Strength::new(999_000.0))]
    #[case::required_minus_strong(
        Strength::REQUIRED,
        Strength::STRONG,
        Strength::new(1_000_001_000.0)
    )]
    #[case::required_minus_required(Strength::REQUIRED, Strength::REQUIRED, Strength::ZERO)]
    fn sub(#[case] lhs: Strength, #[case] rhs: Strength, #[case] expected: Strength) {
        let result = lhs - rhs;
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case::saturate_low(Strength::ZERO, Strength::WEAK, Strength::ZERO)]
    #[case::zero_minus_zero(Strength::ZERO, Strength::ZERO, Strength::ZERO)]
    #[case::weak_minus_zero(Strength::WEAK, Strength::ZERO, Strength::WEAK)]
    #[case::weak_minus_weak(Strength::WEAK, Strength::WEAK, Strength::ZERO)]
    #[case::medium_minus_weak(Strength::MEDIUM, Strength::WEAK, Strength::new(999.0))]
    #[case::strong_minus_medium(Strength::STRONG, Strength::MEDIUM, Strength::new(999_000.0))]
    #[case::required_minus_strong(
        Strength::REQUIRED,
        Strength::STRONG,
        Strength::new(1_000_001_000.0)
    )]
    #[case::required_minus_required(Strength::REQUIRED, Strength::REQUIRED, Strength::ZERO)]
    fn sub_assign(#[case] lhs: Strength, #[case] rhs: Strength, #[case] expected: Strength) {
        let mut result = lhs;
        result -= rhs;
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case::negative(Strength::WEAK, -1.0, Strength::ZERO)]
    #[case::zero_mul_zero(Strength::ZERO, 0.0, Strength::ZERO)]
    #[case::zero_mul_one(Strength::ZERO, 1.0, Strength::ZERO)]
    #[case::weak_mul_zero(Strength::WEAK, 0.0, Strength::ZERO)]
    #[case::weak_mul_one(Strength::WEAK, 1.0, Strength::WEAK)]
    #[case::weak_mul_two(Strength::WEAK, 2.0, Strength::new(2.0))]
    #[case::medium_mul_half(Strength::MEDIUM, 0.5, Strength::new(500.0))]
    #[case::strong_mul_two(Strength::STRONG, 2.0, Strength::new(2_000_000.0))]
    #[case::required_mul_half(Strength::REQUIRED, 0.5, Strength::new(500_500_500.0))]
    fn mul(#[case] lhs: Strength, #[case] rhs: f64, #[case] expected: Strength) {
        let result = lhs * rhs;
        assert_eq!(result, expected);
    }

    #[rstest]
    #[case::negative(Strength::WEAK, -1.0, Strength::ZERO)]
    #[case::zero_mul_zero(Strength::ZERO, 0.0, Strength::ZERO)]
    #[case::zero_mul_one(Strength::ZERO, 1.0, Strength::ZERO)]
    #[case::weak_mul_zero(Strength::WEAK, 0.0, Strength::ZERO)]
    #[case::weak_mul_one(Strength::WEAK, 1.0, Strength::WEAK)]
    #[case::weak_mul_two(Strength::WEAK, 2.0, Strength::new(2.0))]
    #[case::medium_mul_half(Strength::MEDIUM, 0.5, Strength::new(500.0))]
    #[case::strong_mul_two(Strength::STRONG, 2.0, Strength::new(2_000_000.0))]
    #[case::required_mul_half(Strength::REQUIRED, 0.5, Strength::new(500_500_500.0))]
    fn mul_assign(#[case] lhs: Strength, #[case] rhs: f64, #[case] expected: Strength) {
        let mut result = lhs;
        result *= rhs;
        assert_eq!(result, expected);
    }
}
