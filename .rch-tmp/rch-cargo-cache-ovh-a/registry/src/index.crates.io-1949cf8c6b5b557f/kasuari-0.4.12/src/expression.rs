use alloc::vec;
use alloc::vec::Vec;
use core::ops;

use crate::{Term, Variable};

/// An expression that can be the left hand or right hand side of a constraint equation.
///
/// It is a linear combination of variables, i.e., a sum of variables weighted by coefficients, plus
/// an optional constant.
///
/// ```text
/// expression = term_1 + term_2 + ... + term_n + constant
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Expression {
    /// The terms in the expression.
    pub terms: Vec<Term>,

    /// The constant in the expression.
    pub constant: f64,
}

impl Expression {
    /// Create a new Expression.
    ///
    /// ```text
    /// expression = term_1 + term_2 + ... + term_n + constant
    /// ```
    #[inline]
    pub const fn new(terms: Vec<Term>, constant: f64) -> Expression {
        Expression { terms, constant }
    }

    /// Constructs an expression that represents a constant without any terms
    ///
    /// ```text
    /// expression = constant
    /// ```
    #[inline]
    pub const fn from_constant(constant: f64) -> Expression {
        Expression {
            terms: Vec::new(),
            constant,
        }
    }

    /// Constructs an expression from a single term.
    ///
    /// ```text
    /// expression = term
    /// ```
    #[inline]
    pub fn from_term(term: Term) -> Expression {
        Expression {
            terms: vec![term],
            constant: 0.0,
        }
    }

    /// Constructs an expression from a terms
    ///
    /// ```text
    /// expression = term_1 + term_2 + ... + term_n
    /// ```
    #[inline]
    pub const fn from_terms(terms: Vec<Term>) -> Expression {
        Expression {
            terms,
            constant: 0.0,
        }
    }

    /// Constructs an expression from a variable
    ///
    /// ```text
    /// expression = variable
    /// ```
    pub fn from_variable(variable: Variable) -> Expression {
        Expression {
            terms: vec![Term::from_variable(variable)],
            constant: 0.0,
        }
    }
}

impl From<f64> for Expression {
    #[inline]
    fn from(constant: f64) -> Expression {
        Expression::from_constant(constant)
    }
}

impl From<Variable> for Expression {
    #[inline]
    fn from(variable: Variable) -> Expression {
        let term = Term::from(variable);
        Expression::from_term(term)
    }
}

impl From<Term> for Expression {
    #[inline]
    fn from(term: Term) -> Expression {
        Expression::from_term(term)
    }
}

impl FromIterator<Term> for Expression {
    #[inline]
    fn from_iter<I: IntoIterator<Item = Term>>(iter: I) -> Self {
        let terms = iter.into_iter().collect();
        Expression::from_terms(terms)
    }
}

impl ops::Neg for Expression {
    type Output = Expression;

    #[inline]
    fn neg(self) -> Expression {
        Expression {
            terms: self.terms.iter().copied().map(Term::neg).collect(),
            constant: -self.constant,
        }
    }
}

impl ops::Mul<f64> for Expression {
    type Output = Expression;

    #[inline]
    fn mul(mut self, rhs: f64) -> Expression {
        self *= rhs;
        self
    }
}

impl ops::MulAssign<f64> for Expression {
    #[inline]
    fn mul_assign(&mut self, rhs: f64) {
        self.constant *= rhs;
        for term in &mut self.terms {
            *term = *term * rhs;
        }
    }
}

impl ops::Mul<f32> for Expression {
    type Output = Expression;

    #[inline]
    fn mul(self, rhs: f32) -> Expression {
        self * rhs as f64
    }
}

impl ops::MulAssign<f32> for Expression {
    #[inline]
    fn mul_assign(&mut self, rhs: f32) {
        *self *= rhs as f64;
    }
}

impl ops::Mul<Expression> for f64 {
    type Output = Expression;

    #[inline]
    fn mul(self, mut rhs: Expression) -> Expression {
        rhs.constant *= self;
        for term in &mut rhs.terms {
            *term *= self;
        }
        rhs
    }
}

impl ops::Mul<Expression> for f32 {
    type Output = Expression;

    #[inline]
    fn mul(self, rhs: Expression) -> Expression {
        self as f64 * rhs
    }
}

impl ops::Div<f64> for Expression {
    type Output = Expression;

    #[inline]
    fn div(mut self, rhs: f64) -> Expression {
        self /= rhs;
        self
    }
}

impl ops::DivAssign<f64> for Expression {
    #[inline]
    fn div_assign(&mut self, rhs: f64) {
        self.constant /= rhs;
        for term in &mut self.terms {
            *term = *term / rhs;
        }
    }
}

impl ops::Div<f32> for Expression {
    type Output = Expression;

    #[inline]
    fn div(self, rhs: f32) -> Expression {
        self.div(rhs as f64)
    }
}

impl ops::DivAssign<f32> for Expression {
    #[inline]
    fn div_assign(&mut self, v: f32) {
        self.div_assign(v as f64)
    }
}

impl ops::Add<f64> for Expression {
    type Output = Expression;

    #[inline]
    fn add(mut self, rhs: f64) -> Expression {
        self += rhs;
        self
    }
}

impl ops::AddAssign<f64> for Expression {
    #[inline]
    fn add_assign(&mut self, rhs: f64) {
        self.constant += rhs;
    }
}

impl ops::Add<f32> for Expression {
    type Output = Expression;

    #[inline]
    fn add(self, rhs: f32) -> Expression {
        self.add(rhs as f64)
    }
}

impl ops::AddAssign<f32> for Expression {
    #[inline]
    fn add_assign(&mut self, rhs: f32) {
        self.add_assign(rhs as f64)
    }
}

impl ops::Add<Expression> for f64 {
    type Output = Expression;

    #[inline]
    fn add(self, mut rhs: Expression) -> Expression {
        rhs.constant += self;
        rhs
    }
}

impl ops::Add<Expression> for f32 {
    type Output = Expression;

    #[inline]
    fn add(self, rhs: Expression) -> Expression {
        (self as f64).add(rhs)
    }
}

impl ops::Add<Expression> for Expression {
    type Output = Expression;

    #[inline]
    fn add(mut self, rhs: Expression) -> Expression {
        self += rhs;
        self
    }
}

impl ops::AddAssign<Expression> for Expression {
    #[inline]
    fn add_assign(&mut self, mut rhs: Expression) {
        self.terms.append(&mut rhs.terms);
        self.constant += rhs.constant;
    }
}

impl ops::Sub<f64> for Expression {
    type Output = Expression;

    #[inline]
    fn sub(mut self, rhs: f64) -> Expression {
        self -= rhs;
        self
    }
}

impl ops::SubAssign<f64> for Expression {
    #[inline]
    fn sub_assign(&mut self, rhs: f64) {
        self.constant -= rhs;
    }
}

impl ops::Sub<f32> for Expression {
    type Output = Expression;

    #[inline]
    fn sub(self, rhs: f32) -> Expression {
        self.sub(rhs as f64)
    }
}

impl ops::SubAssign<f32> for Expression {
    #[inline]
    fn sub_assign(&mut self, rhs: f32) {
        self.sub_assign(rhs as f64)
    }
}

impl ops::Sub<Expression> for f64 {
    type Output = Expression;

    #[inline]
    fn sub(self, mut rhs: Expression) -> Expression {
        rhs = -rhs;
        rhs.constant += self;
        rhs
    }
}

impl ops::Sub<Expression> for f32 {
    type Output = Expression;

    #[inline]
    fn sub(self, rhs: Expression) -> Expression {
        (self as f64).sub(rhs)
    }
}

impl ops::Sub<Expression> for Expression {
    type Output = Expression;

    #[inline]
    fn sub(mut self, rhs: Expression) -> Expression {
        self -= rhs;
        self
    }
}

impl ops::SubAssign<Expression> for Expression {
    #[inline]
    fn sub_assign(&mut self, mut rhs: Expression) {
        rhs = -rhs;
        self.terms.append(&mut rhs.terms);
        self.constant += rhs.constant;
    }
}
