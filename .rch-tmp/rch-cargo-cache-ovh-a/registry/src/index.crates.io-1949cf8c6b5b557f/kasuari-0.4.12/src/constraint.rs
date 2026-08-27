#[cfg(not(feature = "portable-atomic"))]
use alloc::sync::Arc;
use core::hash::{Hash, Hasher};
use core::ops;

#[cfg(feature = "portable-atomic")]
use portable_atomic_util::Arc;

use crate::{Expression, RelationalOperator, Strength, Term, Variable, WeightedRelation};

#[derive(Debug)]
struct Inner {
    expression: Expression,
    strength: Strength,
    operator: RelationalOperator,
}

/// A constraint, consisting of an equation governed by an expression and a relational operator,
/// and an associated strength.
#[derive(Clone, Debug)]
pub struct Constraint {
    inner: Arc<Inner>,
}

impl Constraint {
    /// Construct a new constraint from an expression, a relational operator and a strength.
    /// This corresponds to the equation `e op 0.0`, e.g. `x + y >= 0.0`. For equations with a
    /// non-zero right hand side, subtract it from the equation to give a zero right hand side.
    pub fn new(
        expression: Expression,
        operator: RelationalOperator,
        strength: Strength,
    ) -> Constraint {
        Constraint {
            inner: Arc::new(Inner {
                expression,
                operator,
                strength,
            }),
        }
    }
    /// The expression of the left hand side of the constraint equation.
    pub fn expr(&self) -> &Expression {
        &self.inner.expression
    }
    /// The relational operator governing the constraint.
    pub fn op(&self) -> RelationalOperator {
        self.inner.operator
    }
    /// The strength of the constraint that the solver will use.
    pub fn strength(&self) -> Strength {
        self.inner.strength
    }
}

impl Hash for Constraint {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        use core::ops::Deref;
        hasher.write_usize(self.inner.deref() as *const _ as usize);
    }
}

impl PartialEq for Constraint {
    fn eq(&self, other: &Constraint) -> bool {
        use core::ops::Deref;
        core::ptr::eq(self.inner.deref(), other.inner.deref())
    }
}

impl Eq for Constraint {}

/// This is an intermediate type used in the syntactic sugar for specifying constraints. You should
/// not use it directly.
pub struct PartialConstraint {
    expression: Expression,
    relation: WeightedRelation,
}

impl PartialConstraint {
    /// Construct a new partial constraint from an expression and a relational operator.
    pub const fn new(expression: Expression, relation: WeightedRelation) -> PartialConstraint {
        PartialConstraint {
            expression,
            relation,
        }
    }
}

impl ops::BitOr<f64> for PartialConstraint {
    type Output = Constraint;
    fn bitor(self, rhs: f64) -> Constraint {
        let (operator, strength) = self.relation.into();
        #[allow(clippy::suspicious_arithmetic_impl)]
        Constraint::new(self.expression - rhs, operator, strength)
    }
}

impl ops::BitOr<f32> for PartialConstraint {
    type Output = Constraint;
    fn bitor(self, rhs: f32) -> Constraint {
        self.bitor(rhs as f64)
    }
}

impl ops::BitOr<Variable> for PartialConstraint {
    type Output = Constraint;
    fn bitor(self, rhs: Variable) -> Constraint {
        let (operator, strength) = self.relation.into();
        #[allow(clippy::suspicious_arithmetic_impl)]
        Constraint::new(self.expression - rhs, operator, strength)
    }
}

impl ops::BitOr<Term> for PartialConstraint {
    type Output = Constraint;
    fn bitor(self, rhs: Term) -> Constraint {
        let (operator, strength) = self.relation.into();
        #[allow(clippy::suspicious_arithmetic_impl)]
        Constraint::new(self.expression - rhs, operator, strength)
    }
}

impl ops::BitOr<Expression> for PartialConstraint {
    type Output = Constraint;
    fn bitor(self, rhs: Expression) -> Constraint {
        let (operator, strength) = self.relation.into();
        #[allow(clippy::suspicious_arithmetic_impl)]
        Constraint::new(self.expression - rhs, operator, strength)
    }
}
