use core::{fmt, ops};

use crate::{Expression, PartialConstraint, Strength, Term, Variable};

/// The possible relations that a constraint can specify.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RelationalOperator {
    /// `<=`
    LessOrEqual,
    /// `==`
    Equal,
    /// `>=`
    GreaterOrEqual,
}

impl fmt::Display for RelationalOperator {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            RelationalOperator::LessOrEqual => write!(fmt, "<=")?,
            RelationalOperator::Equal => write!(fmt, "==")?,
            RelationalOperator::GreaterOrEqual => write!(fmt, ">=")?,
        };
        Ok(())
    }
}

/// This is part of the syntactic sugar used for specifying constraints. This enum should be used as
/// part of a constraint expression. See the module documentation for more information.
pub enum WeightedRelation {
    /// `==`
    EQ(Strength),
    /// `<=`
    LE(Strength),
    /// `>=`
    GE(Strength),
}

impl From<WeightedRelation> for (RelationalOperator, Strength) {
    fn from(relation: WeightedRelation) -> (RelationalOperator, Strength) {
        match relation {
            WeightedRelation::EQ(s) => (RelationalOperator::Equal, s),
            WeightedRelation::LE(s) => (RelationalOperator::LessOrEqual, s),
            WeightedRelation::GE(s) => (RelationalOperator::GreaterOrEqual, s),
        }
    }
}

impl ops::BitOr<WeightedRelation> for f64 {
    type Output = PartialConstraint;

    #[inline]
    fn bitor(self, rhs: WeightedRelation) -> PartialConstraint {
        PartialConstraint::new(Expression::from_constant(self), rhs)
    }
}

impl ops::BitOr<WeightedRelation> for f32 {
    type Output = PartialConstraint;

    #[inline]
    fn bitor(self, rhs: WeightedRelation) -> PartialConstraint {
        (self as f64).bitor(rhs)
    }
}

impl ops::BitOr<WeightedRelation> for Variable {
    type Output = PartialConstraint;

    #[inline]
    fn bitor(self, rhs: WeightedRelation) -> PartialConstraint {
        PartialConstraint::new(Expression::from_variable(self), rhs)
    }
}

impl ops::BitOr<WeightedRelation> for Term {
    type Output = PartialConstraint;

    #[inline]
    fn bitor(self, rhs: WeightedRelation) -> PartialConstraint {
        PartialConstraint::new(Expression::from_term(self), rhs)
    }
}

impl ops::BitOr<WeightedRelation> for Expression {
    type Output = PartialConstraint;

    #[inline]
    fn bitor(self, rhs: WeightedRelation) -> PartialConstraint {
        PartialConstraint::new(self, rhs)
    }
}
