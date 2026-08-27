use alloc::vec;
use core::ops;

use crate::{Expression, Variable};

/// A variable and a coefficient to multiply that variable by.
///
/// This is a sub-expression in a constraint equation that represents:
///
/// ```text
/// term = coefficient * variable
/// ```
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Term {
    pub variable: Variable,
    pub coefficient: f64,
}

impl Term {
    /// Construct a new Term from a variable and a coefficient.
    #[inline]
    pub const fn new(variable: Variable, coefficient: f64) -> Term {
        Term {
            variable,
            coefficient,
        }
    }

    /// Construct a new Term from a variable with a coefficient of 1.0.
    #[inline]
    pub const fn from_variable(variable: Variable) -> Term {
        Term::new(variable, 1.0)
    }
}

impl From<Variable> for Term {
    #[inline]
    fn from(variable: Variable) -> Term {
        Term::from_variable(variable)
    }
}

impl ops::Mul<f64> for Term {
    type Output = Term;

    #[inline]
    fn mul(self, rhs: f64) -> Term {
        Term::new(self.variable, self.coefficient * rhs)
    }
}

impl ops::Mul<Term> for f64 {
    type Output = Term;

    #[inline]
    fn mul(self, rhs: Term) -> Term {
        Term::new(rhs.variable, self * rhs.coefficient)
    }
}

impl ops::Mul<f32> for Term {
    type Output = Term;

    #[inline]
    fn mul(self, rhs: f32) -> Term {
        Term::new(self.variable, self.coefficient * rhs as f64)
    }
}

impl ops::Mul<Term> for f32 {
    type Output = Term;

    #[inline]
    fn mul(self, rhs: Term) -> Term {
        Term::new(rhs.variable, self as f64 * rhs.coefficient)
    }
}

impl ops::MulAssign<f64> for Term {
    #[inline]
    fn mul_assign(&mut self, rhs: f64) {
        self.coefficient *= rhs;
    }
}

impl ops::MulAssign<f32> for Term {
    #[inline]
    fn mul_assign(&mut self, rhs: f32) {
        self.coefficient *= rhs as f64;
    }
}

impl ops::Div<f64> for Term {
    type Output = Term;

    #[inline]
    fn div(self, rhs: f64) -> Term {
        Term::new(self.variable, self.coefficient / rhs)
    }
}
impl ops::Div<f32> for Term {
    type Output = Term;

    #[inline]
    fn div(self, rhs: f32) -> Term {
        Term::new(self.variable, self.coefficient / rhs as f64)
    }
}

impl ops::DivAssign<f64> for Term {
    #[inline]
    fn div_assign(&mut self, rhs: f64) {
        self.coefficient /= rhs;
    }
}

impl ops::DivAssign<f32> for Term {
    #[inline]
    fn div_assign(&mut self, rhs: f32) {
        self.coefficient /= rhs as f64;
    }
}

impl ops::Add<f64> for Term {
    type Output = Expression;

    #[inline]
    fn add(self, rhs: f64) -> Expression {
        Expression::new(vec![self], rhs)
    }
}

impl ops::Add<f32> for Term {
    type Output = Expression;

    #[inline]
    fn add(self, rhs: f32) -> Expression {
        Expression::new(vec![self], rhs as f64)
    }
}

impl ops::Add<Term> for f64 {
    type Output = Expression;

    #[inline]
    fn add(self, rhs: Term) -> Expression {
        Expression::new(vec![rhs], self)
    }
}

impl ops::Add<Term> for f32 {
    type Output = Expression;

    #[inline]
    fn add(self, rhs: Term) -> Expression {
        Expression::new(vec![rhs], self as f64)
    }
}

impl ops::Add<Term> for Term {
    type Output = Expression;

    #[inline]
    fn add(self, rhs: Term) -> Expression {
        Expression::from_terms(vec![self, rhs])
    }
}

impl ops::Add<Expression> for Term {
    type Output = Expression;

    #[inline]
    fn add(self, mut rhs: Expression) -> Expression {
        rhs.terms.insert(0, self);
        rhs
    }
}

impl ops::Add<Term> for Expression {
    type Output = Expression;

    #[inline]
    fn add(mut self, rhs: Term) -> Expression {
        self.terms.push(rhs);
        self
    }
}

impl ops::AddAssign<Term> for Expression {
    #[inline]
    fn add_assign(&mut self, rhs: Term) {
        self.terms.push(rhs);
    }
}

impl ops::Neg for Term {
    type Output = Term;

    #[inline]
    fn neg(mut self) -> Term {
        self.coefficient = -self.coefficient;
        self
    }
}

impl ops::Sub<f64> for Term {
    type Output = Expression;

    #[inline]
    fn sub(self, rhs: f64) -> Expression {
        Expression::new(vec![self], -rhs)
    }
}

impl ops::Sub<f32> for Term {
    type Output = Expression;

    #[inline]
    fn sub(self, rhs: f32) -> Expression {
        Expression::new(vec![self], -(rhs as f64))
    }
}

impl ops::Sub<Term> for f64 {
    type Output = Expression;

    #[inline]
    fn sub(self, rhs: Term) -> Expression {
        Expression::new(vec![-rhs], self)
    }
}

impl ops::Sub<Term> for f32 {
    type Output = Expression;

    #[inline]
    fn sub(self, rhs: Term) -> Expression {
        Expression::new(vec![-rhs], self as f64)
    }
}

impl ops::Sub<Term> for Term {
    type Output = Expression;

    #[inline]
    fn sub(self, rhs: Term) -> Expression {
        Expression::from_terms(vec![self, -rhs])
    }
}

impl ops::Sub<Expression> for Term {
    type Output = Expression;

    #[inline]
    fn sub(self, mut rhs: Expression) -> Expression {
        rhs = -rhs;
        rhs.terms.insert(0, self);
        rhs
    }
}

impl ops::Sub<Term> for Expression {
    type Output = Expression;

    #[inline]
    fn sub(mut self, rhs: Term) -> Expression {
        self -= rhs;
        self
    }
}

impl ops::SubAssign<Term> for Expression {
    #[inline]
    fn sub_assign(&mut self, rhs: Term) {
        self.terms.push(-rhs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEFT: Variable = Variable::from_id(0);
    const RIGHT: Variable = Variable::from_id(1);
    const LEFT_TERM: Term = Term::from_variable(LEFT);
    const RIGHT_TERM: Term = Term::from_variable(RIGHT);

    #[test]
    fn new() {
        assert_eq!(
            Term::new(LEFT, 2.0),
            Term {
                variable: LEFT,
                coefficient: 2.0
            }
        );
    }

    #[test]
    fn from_variable() {
        assert_eq!(
            Term::from_variable(LEFT),
            Term {
                variable: LEFT,
                coefficient: 1.0
            }
        );
    }

    #[test]
    fn mul_f64() {
        assert_eq!(
            LEFT_TERM * 2.0,
            Term {
                variable: LEFT,
                coefficient: 2.0
            }
        );
        assert_eq!(
            2.0 * LEFT_TERM,
            Term {
                variable: LEFT,
                coefficient: 2.0
            }
        );
    }

    #[test]
    fn mul_f32() {
        assert_eq!(
            LEFT_TERM * 2.0f32,
            Term {
                variable: LEFT,
                coefficient: 2.0
            }
        );
        assert_eq!(
            2.0f32 * LEFT_TERM,
            Term {
                variable: LEFT,
                coefficient: 2.0
            }
        );
    }

    #[test]
    fn mul_assign_f64() {
        let mut term = LEFT_TERM;
        term *= 2.0;
        assert_eq!(
            term,
            Term {
                variable: LEFT,
                coefficient: 2.0
            }
        );
    }

    #[test]
    fn mul_assign_f32() {
        let mut term = LEFT_TERM;
        term *= 2.0f32;
        assert_eq!(
            term,
            Term {
                variable: LEFT,
                coefficient: 2.0
            }
        );
    }

    #[test]
    fn div_f64() {
        assert_eq!(
            LEFT_TERM / 2.0,
            Term {
                variable: LEFT,
                coefficient: 0.5
            }
        );
    }

    #[test]
    fn div_f32() {
        assert_eq!(
            LEFT_TERM / 2.0f32,
            Term {
                variable: LEFT,
                coefficient: 0.5
            }
        );
    }

    #[test]
    fn div_assign_f64() {
        let mut term = LEFT_TERM;
        term /= 2.0;
        assert_eq!(
            term,
            Term {
                variable: LEFT,
                coefficient: 0.5
            }
        );
    }

    #[test]
    fn div_assign_f32() {
        let mut term = LEFT_TERM;
        term /= 2.0f32;
        assert_eq!(
            term,
            Term {
                variable: LEFT,
                coefficient: 0.5
            }
        );
    }

    #[test]
    fn add_f64() {
        assert_eq!(LEFT_TERM + 2.0, Expression::new(vec![LEFT_TERM], 2.0));
        assert_eq!(2.0 + LEFT_TERM, Expression::new(vec![LEFT_TERM], 2.0));
    }

    #[test]
    fn add_f32() {
        assert_eq!(LEFT_TERM + 2.0f32, Expression::new(vec![LEFT_TERM], 2.0));
        assert_eq!(2.0f32 + LEFT_TERM, Expression::new(vec![LEFT_TERM], 2.0));
    }

    #[test]
    fn add_term() {
        assert_eq!(
            LEFT_TERM + RIGHT_TERM,
            Expression::from_terms(vec![LEFT_TERM, RIGHT_TERM])
        );
    }

    #[test]
    fn add_expression() {
        assert_eq!(
            LEFT_TERM + Expression::new(vec![RIGHT_TERM], 1.0),
            Expression::new(vec![LEFT_TERM, RIGHT_TERM], 1.0)
        );
    }

    #[test]
    fn sub_f64() {
        assert_eq!(LEFT_TERM - 2.0, Expression::new(vec![LEFT_TERM], -2.0));
        assert_eq!(2.0 - LEFT_TERM, Expression::new(vec![-LEFT_TERM], 2.0));
    }

    #[test]
    fn sub_f32() {
        assert_eq!(LEFT_TERM - 2.0f32, Expression::new(vec![LEFT_TERM], -2.0));
        assert_eq!(2.0f32 - LEFT_TERM, Expression::new(vec![-LEFT_TERM], 2.0));
    }

    #[test]
    fn sub_term() {
        assert_eq!(
            LEFT_TERM - RIGHT_TERM,
            Expression::from_terms(vec![LEFT_TERM, -RIGHT_TERM])
        );
    }

    #[test]
    fn sub_expression() {
        assert_eq!(
            LEFT_TERM - Expression::new(vec![RIGHT_TERM], 1.0),
            Expression::new(vec![LEFT_TERM, -RIGHT_TERM], -1.0)
        );
    }

    #[test]
    fn neg() {
        assert_eq!(
            -LEFT_TERM,
            Term {
                variable: LEFT,
                coefficient: -1.0
            }
        );
    }
}
