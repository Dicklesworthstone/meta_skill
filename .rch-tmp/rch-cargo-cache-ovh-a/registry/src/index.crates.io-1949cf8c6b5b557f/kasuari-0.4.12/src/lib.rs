//! This crate contains an implementation of the Cassowary constraint solving algorithm, based upon
//! the work by G.J. Badros et al. in 2001. This algorithm is designed primarily for use
//! constraining elements in user interfaces. Constraints are linear combinations of the problem
//! variables. The notable features of Cassowary that make it ideal for user interfaces are that it
//! is incremental (i.e. you can add and remove constraints at runtime and it will perform the
//! minimum work to update the result) and that the constraints can be violated if necessary, with
//! the order in which they are violated specified by setting a "strength" for each constraint. This
//! allows the solution to gracefully degrade, which is useful for when a user interface needs to
//! compromise on its constraints in order to still be able to display something.
//!
//! ## Constraint syntax
//!
//! This crate aims to provide syntax for describing linear constraints as naturally as possible,
//! within the limitations of Rust's type system. Generally you can write constraints as you would
//! naturally, however the operator symbol (for greater-than, less-than, equals) is replaced with an
//! instance of the `WeightedRelation` enum wrapped in "pipe brackets".
//!
//! For example, for the constraint `(a + b) * 2 + c >= d + 1` with strength `s`, the code to use is
//!
//! ```ignore
//! (a + b) * 2.0 + c |GE(s)| d + 1.0
//! ```
//!
//! # A simple example
//!
//! Imagine a layout consisting of two elements laid out horizontally. For small window widths the
//! elements should compress to fit, but if there is enough space they should display at their
//! preferred widths. The first element will align to the left, and the second to the right. For
//! this example we will ignore vertical layout.
//!
//! First we need to include the relevant parts of `cassowary`:
//!
//! ```
//! use kasuari::WeightedRelation::*;
//! use kasuari::{Solver, Variable};
//! ```
//!
//! And we'll construct some conveniences for pretty printing (which should hopefully be
//! self-explanatory):
//!
//! ```ignore
//! use hashbrown::HashMap;
//! let mut names = HashMap::new();
//! fn print_changes(names: &HashMap<Variable, &'static str>, changes: &[(Variable, f64)]) {
//!     println!("Changes:");
//!     for &(ref var, ref val) in changes {
//!         println!("{}: {}", names[var], val);
//!     }
//! }
//! ```
//!
//! Let's define the variables required - the left and right edges of the elements, and the width of
//! the window.
//!
//! ```ignore
//! let window_width = Variable::new();
//! names.insert(window_width, "window_width");
//!
//! struct Element {
//!     left: Variable,
//!     right: Variable
//! }
//! let box1 = Element {
//!     left: Variable::new(),
//!     right: Variable::new()
//! };
//! names.insert(box1.left, "box1.left");
//! names.insert(box1.right, "box1.right");
//!
//! let box2 = Element {
//!     left: Variable::new(),
//!     right: Variable::new()
//! };
//! names.insert(box2.left, "box2.left");
//! names.insert(box2.right, "box2.right");
//! ```
//!
//! Now to set up the solver and constraints.
//!
//! ```ignore
//! let mut solver = Solver::new();
//! solver.add_constraints(&[
//!     window_width |GE(REQUIRED)| 0.0, // positive window width
//!     box1.left |EQ(REQUIRED)| 0.0, // left align
//!     box2.right |EQ(REQUIRED)| window_width, // right align
//!     box2.left |GE(REQUIRED)| box1.right, // no overlap
//!     // positive widths
//!     box1.left |LE(REQUIRED)| box1.right,
//!     box2.left |LE(REQUIRED)| box2.right,
//!     // preferred widths:
//!     box1.right - box1.left |EQ(WEAK)| 50.0,
//!     box2.right - box2.left |EQ(WEAK)| 100.0
//! ])?;
//! # Ok::<(), kasuari::InternalSolverError>(())
//! ```
//!
//! The window width is currently free to take any positive value. Let's constrain it to a
//! particular value. Since for this example we will repeatedly change the window width, it is most
//! efficient to use an "edit variable", instead of repeatedly removing and adding constraints (note
//! that for efficiency reasons we cannot edit a normal constraint that has been added to the
//! solver).
//!
//! ```ignore
//! solver.add_edit_variable(window_width, STRONG).unwrap();
//! solver.suggest_value(window_width, 300.0).unwrap();
//! ```
//!
//! This value of 300 is enough to fit both boxes in with room to spare, so let's check that this is
//! the case. We can fetch a list of changes to the values of variables in the solver. Using the
//! pretty printer defined earlier we can see what values our variables now hold.
//!
//! ```ignore
//! print_changes(&names, solver.fetch_changes());
//! ```
//!
//! This should print (in a possibly different order):
//!
//! ```ignore
//! Changes:
//! window_width: 300
//! box1.right: 50
//! box2.left: 200
//! box2.right: 300
//! ```
//!
//! Note that the value of `box1.left` is not mentioned. This is because `solver.fetch_changes` only
//! lists *changes* to variables, and since each variable starts in the solver with a value of zero,
//! any values that have not changed from zero will not be reported.
//!
//! Now let's try compressing the window so that the boxes can't take up their preferred widths.
//!
//! ```ignore
//! solver.suggest_value(window_width, 75.0);
//! print_changes(&names, solver.fetch_changes);
//! ```
//!
//! Now the solver can't satisfy all of the constraints. It will pick at least one of the weakest
//! constraints to violate. In this case it will be one or both of the preferred widths. For
//! efficiency reasons this is picked nondeterministically, so there are two possible results. This
//! could be
//!
//! ```ignore
//! Changes:
//! window_width: 75
//! box1.right: 0
//! box2.left: 0
//! box2.right: 75
//! ```
//!
//! or
//!
//! ```ignore
//! Changes:
//! window_width: 75
//! box2.left: 50
//! box2.right: 75
//! ```
//!
//! Due to the nature of the algorithm, "in-between" solutions, although just as valid, are not
//! picked.
//!
//! In a user interface this is not likely a result we would prefer. The solution is to add another
//! constraint to control the behaviour when the preferred widths cannot both be satisfied. In this
//! example we are going to constrain the boxes to try to maintain a ratio between their widths.
//!
//! ```
//! # use kasuari::{ Solver, Variable, Strength };
//! # use kasuari::WeightedRelation::*;
//! #
//! # use hashbrown::HashMap;
//! # let mut names = HashMap::new();
//! # fn print_changes(names: &HashMap<Variable, &'static str>, changes: &[(Variable, f64)]) {
//! #     println!("Changes:");
//! #     for &(ref var, ref val) in changes {
//! #         println!("{}: {}", names[var], val);
//! #     }
//! # }
//! #
//! # let window_width = Variable::new();
//! # names.insert(window_width, "window_width");
//! # struct Element {
//! #    left: Variable,
//! #    right: Variable
//! # }
//! # let box1 = Element {
//! #     left: Variable::new(),
//! #     right: Variable::new()
//! # };
//! # names.insert(box1.left, "box1.left");
//! # names.insert(box1.right, "box1.right");
//! # let box2 = Element {
//! #     left: Variable::new(),
//! #     right: Variable::new()
//! # };
//! # names.insert(box2.left, "box2.left");
//! # names.insert(box2.right, "box2.right");
//! # let mut solver = Solver::new();
//! # solver.add_constraints([
//! #     window_width |GE(Strength::REQUIRED)| 0.0, // positive window width
//! #     box1.left |EQ(Strength::REQUIRED)| 0.0, // left align
//! #     box2.right |EQ(Strength::REQUIRED)| window_width, // right align
//! #     box2.left |GE(Strength::REQUIRED)| box1.right, // no overlap
//! #     // positive widths
//! #     box1.left |LE(Strength::REQUIRED)| box1.right,
//! #     box2.left |LE(Strength::REQUIRED)| box2.right,
//! #     // preferred widths:
//! #     box1.right - box1.left |EQ(Strength::WEAK)| 50.0,
//! #     box2.right - box2.left |EQ(Strength::WEAK)| 100.0]).unwrap();
//! # solver.add_edit_variable(window_width, Strength::STRONG).unwrap();
//! # solver.suggest_value(window_width, 300.0).unwrap();
//! # print_changes(&names, solver.fetch_changes());
//! # solver.suggest_value(window_width, 75.0);
//! # print_changes(&names, solver.fetch_changes());
//! solver.add_constraint(
//!     (box1.right - box1.left) / 50.0 |EQ(Strength::MEDIUM)| (box2.right - box2.left) / 100.0
//!     ).unwrap();
//! print_changes(&names, solver.fetch_changes());
//! ```
//!
//! Now the result gives values that maintain the ratio between the sizes of the two boxes:
//!
//! ```ignore
//! Changes:
//! box1.right: 25
//! box2.left: 25
//! ```
//!
//! This example may have appeared somewhat contrived, but hopefully it shows the power of the
//! cassowary algorithm for laying out user interfaces.
//!
//! One thing that this example exposes is that this crate is a rather low level library. It does
//! not have any inherent knowledge of user interfaces, directions or boxes. Thus for use in a user
//! interface this crate should ideally be wrapped by a higher level API, which is outside the scope
//! of this crate.
#![cfg_attr(feature = "document-features", doc = "\n## Features")]
#![cfg_attr(feature = "document-features", doc = document_features::document_features!())]
#![no_std]
extern crate alloc;

mod constraint;
mod error;
mod expression;
mod relations;
mod row;
mod solver;
mod strength;
mod term;
mod variable;

pub use self::constraint::{Constraint, PartialConstraint};
pub use self::error::{
    AddConstraintError, AddEditVariableError, RemoveConstraintError, RemoveEditVariableError,
    SuggestValueError,
};
pub use self::expression::Expression;
pub use self::relations::{RelationalOperator, WeightedRelation};
pub use self::solver::{InternalSolverError, Solver};
pub use self::strength::Strength;
pub use self::term::Term;
pub use self::variable::Variable;
