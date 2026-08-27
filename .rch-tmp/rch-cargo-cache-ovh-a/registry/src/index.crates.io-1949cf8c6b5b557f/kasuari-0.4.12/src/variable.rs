use core::ops;
#[cfg(not(feature = "portable-atomic"))]
use core::sync::atomic::{AtomicUsize, Ordering};

#[cfg(feature = "portable-atomic")]
use portable_atomic::{AtomicUsize, Ordering};

use crate::{Expression, Term};

/// Identifies a variable for the constraint solver.
/// Each new variable is unique in the view of the solver, but copying or cloning the variable
/// produces a copy of the same variable.
#[derive(Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Variable(usize);

impl Variable {
    /// Produces a new unique variable for use in constraint solving.
    #[inline]
    pub fn new() -> Self {
        static VARIABLE_ID: AtomicUsize = AtomicUsize::new(0);
        Self(VARIABLE_ID.fetch_add(1, Ordering::Relaxed))
    }

    #[cfg(test)]
    pub(crate) const fn from_id(id: usize) -> Self {
        Self(id)
    }
}

impl Default for Variable {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl ops::Add<f64> for Variable {
    type Output = Expression;

    #[inline]
    fn add(self, constant: f64) -> Expression {
        Term::from(self) + constant
    }
}

impl ops::Add<f32> for Variable {
    type Output = Expression;

    #[inline]
    fn add(self, constant: f32) -> Expression {
        Term::from(self) + constant
    }
}

impl ops::Add<Variable> for f64 {
    type Output = Expression;

    #[inline]
    fn add(self, variable: Variable) -> Expression {
        Term::from(variable) + self
    }
}

impl ops::Add<Variable> for f32 {
    type Output = Expression;

    #[inline]
    fn add(self, variable: Variable) -> Expression {
        Term::from(variable) + self
    }
}

impl ops::Add<Variable> for Variable {
    type Output = Expression;

    #[inline]
    fn add(self, other: Variable) -> Expression {
        Term::from(self) + Term::from(other)
    }
}

impl ops::Add<Term> for Variable {
    type Output = Expression;

    #[inline]
    fn add(self, term: Term) -> Expression {
        Term::from(self) + term
    }
}

impl ops::Add<Variable> for Term {
    type Output = Expression;

    #[inline]
    fn add(self, variable: Variable) -> Expression {
        self + Term::from(variable)
    }
}

impl ops::Add<Expression> for Variable {
    type Output = Expression;

    #[inline]
    fn add(self, expression: Expression) -> Expression {
        Term::from(self) + expression
    }
}

impl ops::Add<Variable> for Expression {
    type Output = Expression;

    #[inline]
    fn add(self, variable: Variable) -> Expression {
        self + Term::from(variable)
    }
}

impl ops::AddAssign<Variable> for Expression {
    #[inline]
    fn add_assign(&mut self, variable: Variable) {
        *self += Term::from(variable);
    }
}

impl ops::Neg for Variable {
    type Output = Term;

    #[inline]
    fn neg(self) -> Term {
        -Term::from(self)
    }
}

impl ops::Sub<f64> for Variable {
    type Output = Expression;

    #[inline]
    fn sub(self, constant: f64) -> Expression {
        Term::from(self) - constant
    }
}

impl ops::Sub<f32> for Variable {
    type Output = Expression;

    #[inline]
    fn sub(self, constant: f32) -> Expression {
        Term::from(self) - constant
    }
}

impl ops::Sub<Variable> for f64 {
    type Output = Expression;

    #[inline]
    fn sub(self, variable: Variable) -> Expression {
        self - Term::from(variable)
    }
}

impl ops::Sub<Variable> for f32 {
    type Output = Expression;

    #[inline]
    fn sub(self, v: Variable) -> Expression {
        self - Term::from(v)
    }
}

impl ops::Sub<Variable> for Variable {
    type Output = Expression;

    #[inline]
    fn sub(self, other: Variable) -> Expression {
        Term::from(self) - Term::from(other)
    }
}

impl ops::Sub<Term> for Variable {
    type Output = Expression;

    #[inline]
    fn sub(self, term: Term) -> Expression {
        Term::from(self) - term
    }
}

impl ops::Sub<Variable> for Term {
    type Output = Expression;

    #[inline]
    fn sub(self, variable: Variable) -> Expression {
        self - Term::from(variable)
    }
}

impl ops::Sub<Expression> for Variable {
    type Output = Expression;

    #[inline]
    fn sub(self, expression: Expression) -> Expression {
        Term::from(self) - expression
    }
}

impl ops::Sub<Variable> for Expression {
    type Output = Expression;

    #[inline]
    fn sub(self, variable: Variable) -> Expression {
        self - Term::from(variable)
    }
}

impl ops::SubAssign<Variable> for Expression {
    #[inline]
    fn sub_assign(&mut self, variable: Variable) {
        *self -= Term::from(variable);
    }
}

impl ops::Mul<f64> for Variable {
    type Output = Term;

    #[inline]
    fn mul(self, coefficient: f64) -> Term {
        Term::new(self, coefficient)
    }
}

impl ops::Mul<f32> for Variable {
    type Output = Term;

    #[inline]
    fn mul(self, coefficient: f32) -> Term {
        Term::new(self, coefficient as f64)
    }
}

impl ops::Mul<Variable> for f64 {
    type Output = Term;

    #[inline]
    fn mul(self, variable: Variable) -> Term {
        Term::new(variable, self)
    }
}

impl ops::Mul<Variable> for f32 {
    type Output = Term;

    #[inline]
    fn mul(self, variable: Variable) -> Term {
        Term::new(variable, self as f64)
    }
}

impl ops::Div<f64> for Variable {
    type Output = Term;

    #[inline]
    fn div(self, coefficient: f64) -> Term {
        Term::new(self, 1.0 / coefficient)
    }
}

impl ops::Div<f32> for Variable {
    type Output = Term;

    #[inline]
    fn div(self, coefficient: f32) -> Term {
        Term::new(self, 1.0 / coefficient as f64)
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    const LEFT: Variable = Variable(0);
    const RIGHT: Variable = Variable(1);
    const LEFT_TERM: Term = Term::from_variable(LEFT);
    const RIGHT_TERM: Term = Term::from_variable(RIGHT);

    #[test]
    fn variable_default() {
        assert_ne!(LEFT, RIGHT);
    }

    #[test]
    fn variable_add_f64() {
        assert_eq!(LEFT + 5.0, Expression::new(vec![LEFT_TERM], 5.0),);
        assert_eq!(5.0 + LEFT, Expression::new(vec![LEFT_TERM], 5.0),);
    }

    #[test]
    fn variable_add_f32() {
        assert_eq!(LEFT + 5.0_f32, Expression::new(vec![LEFT_TERM], 5.0),);
        assert_eq!(5.0_f32 + LEFT, Expression::new(vec![LEFT_TERM], 5.0),);
    }

    #[test]
    fn variable_add_variable() {
        assert_eq!(
            LEFT + RIGHT,
            Expression::new(vec![LEFT_TERM, RIGHT_TERM], 0.0),
        );
    }

    #[test]
    fn variable_add_term() {
        assert_eq!(
            LEFT + RIGHT_TERM,
            Expression::new(vec![LEFT_TERM, RIGHT_TERM], 0.0),
        );
        assert_eq!(
            LEFT_TERM + RIGHT,
            Expression::new(vec![LEFT_TERM, RIGHT_TERM], 0.0),
        );
    }

    #[test]
    fn variable_add_expression() {
        assert_eq!(
            LEFT + Expression::from_term(RIGHT_TERM),
            Expression::new(vec![LEFT_TERM, RIGHT_TERM], 0.0),
        );
        assert_eq!(
            Expression::from_term(LEFT_TERM) + RIGHT,
            Expression::new(vec![LEFT_TERM, RIGHT_TERM], 0.0),
        );
    }

    #[test]
    fn variable_add_assign() {
        let mut expression = Expression::from_term(LEFT_TERM);
        expression += RIGHT;
        assert_eq!(
            expression,
            Expression::new(vec![LEFT_TERM, RIGHT_TERM], 0.0),
        );
    }

    #[test]
    fn variable_sub_f64() {
        assert_eq!(LEFT - 5.0, Expression::new(vec![LEFT_TERM], -5.0),);
        assert_eq!(5.0 - LEFT, Expression::new(vec![-LEFT_TERM], 5.0),);
    }

    #[test]
    fn variable_sub_f32() {
        assert_eq!(LEFT - 5.0_f32, Expression::new(vec![LEFT_TERM], -5.0),);
        assert_eq!(5.0_f32 - LEFT, Expression::new(vec![-LEFT_TERM], 5.0),);
    }

    #[test]
    fn variable_sub_variable() {
        assert_eq!(
            LEFT - RIGHT,
            Expression::new(vec![LEFT_TERM, -RIGHT_TERM], 0.0),
        );
    }

    #[test]
    fn variable_sub_term() {
        assert_eq!(
            LEFT - RIGHT_TERM,
            Expression::new(vec![LEFT_TERM, -RIGHT_TERM], 0.0),
        );
        assert_eq!(
            LEFT_TERM - RIGHT,
            Expression::new(vec![LEFT_TERM, -RIGHT_TERM], 0.0),
        );
    }

    #[test]
    fn variable_sub_expression() {
        assert_eq!(
            LEFT - Expression::from_term(RIGHT_TERM),
            Expression::new(vec![LEFT_TERM, -RIGHT_TERM], 0.0),
        );
        assert_eq!(
            Expression::from_term(LEFT_TERM) - RIGHT,
            Expression::new(vec![LEFT_TERM, -RIGHT_TERM], 0.0),
        );
    }

    #[test]
    fn variable_sub_assign() {
        let mut expression = Expression::from_term(LEFT_TERM);
        expression -= RIGHT;
        assert_eq!(
            expression,
            Expression::new(vec![LEFT_TERM, -RIGHT_TERM], 0.0),
        );
    }

    #[test]
    fn variable_mul_f64() {
        assert_eq!(LEFT * 5.0, Term::new(LEFT, 5.0));
        assert_eq!(5.0 * LEFT, Term::new(LEFT, 5.0));
    }

    #[test]
    fn variable_mul_f32() {
        assert_eq!(LEFT * 5.0_f32, Term::new(LEFT, 5.0));
        assert_eq!(5.0_f32 * LEFT, Term::new(LEFT, 5.0));
    }

    #[test]
    fn variable_div_f64() {
        assert_eq!(LEFT / 5.0, Term::new(LEFT, 1.0 / 5.0));
    }

    #[test]
    fn variable_div_f32() {
        assert_eq!(LEFT / 5.0_f32, Term::new(LEFT, 1.0 / 5.0));
    }

    #[test]
    fn variable_neg() {
        assert_eq!(-LEFT, -LEFT_TERM);
    }
}
