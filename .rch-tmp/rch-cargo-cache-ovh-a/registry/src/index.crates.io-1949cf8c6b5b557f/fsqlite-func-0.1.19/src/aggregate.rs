//! Aggregate function trait with type-erased state adapter.
//!
//! Aggregate functions accumulate a result across multiple rows (e.g.
//! `SUM`, `COUNT`, `AVG`). Each GROUP BY group gets its own state.
//!
//! # Type Erasure
//!
//! The [`FunctionRegistry`](crate::FunctionRegistry) stores aggregates as
//! `Arc<dyn AggregateFunction<State = Box<dyn Any + Send>>>`. Concrete
//! implementations use [`AggregateAdapter`] to wrap their typed state.
#![allow(clippy::unnecessary_literal_bound)]

use std::any::Any;

use fsqlite_error::Result;
use fsqlite_types::SqliteValue;

/// An aggregate SQL function (e.g. `SUM`, `COUNT`, `AVG`).
///
/// This trait is **open** (user-implementable). Extension authors implement
/// this trait to register custom aggregate functions.
///
/// # State Lifecycle
///
/// 1. [`initial_state`](Self::initial_state) creates a fresh accumulator.
/// 2. [`step`](Self::step) is called once per row.
/// 3. [`finalize`](Self::finalize) consumes the state and returns the result.
///
/// # Send + Sync
///
/// The function object itself is shared across threads via `Arc`. The
/// `State` type must be `Send` so it can be moved between threads.
pub trait AggregateFunction: Send + Sync {
    /// The per-group accumulator type.
    type State: Send;

    /// Create a fresh accumulator (zero/identity state).
    fn initial_state(&self) -> Self::State;

    /// Process one row, updating the accumulator.
    fn step(&self, state: &mut Self::State, args: &[SqliteValue]) -> Result<()>;

    /// Consume the accumulator and produce the final result.
    fn finalize(&self, state: Self::State) -> Result<SqliteValue>;

    /// The number of arguments this function accepts (`-1` = variadic).
    fn num_args(&self) -> i32;

    /// Minimum accepted SQL argument count for variadic functions.
    ///
    /// Fixed-arity functions default to their exact arity. Variadic functions
    /// default to accepting zero arguments unless an implementation tightens
    /// the bound to match SQLite's function surface.
    fn min_args(&self) -> i32 {
        self.num_args().max(0)
    }

    /// Maximum accepted SQL argument count, or `None` for unbounded variadic
    /// functions.
    fn max_args(&self) -> Option<i32> {
        (self.num_args() >= 0).then(|| self.num_args())
    }

    /// Return whether this function accepts `num_args` SQL arguments.
    fn accepts_arg_count(&self, num_args: i32) -> bool {
        num_args >= self.min_args() && self.max_args().is_none_or(|max| num_args <= max)
    }

    /// The function name, used in error messages and EXPLAIN output.
    fn name(&self) -> &str;
}

/// Type-erased adapter that wraps a concrete [`AggregateFunction`] so the
/// registry can store heterogeneous aggregates behind a single trait object.
///
/// The adapter implements `AggregateFunction<State = Box<dyn Any + Send>>`,
/// boxing the concrete state on creation and downcasting on step/finalize.
pub struct AggregateAdapter<F> {
    inner: F,
}

impl<F> AggregateAdapter<F> {
    /// Wrap a concrete aggregate function for type-erased storage.
    pub const fn new(inner: F) -> Self {
        Self { inner }
    }
}

impl<F> AggregateFunction for AggregateAdapter<F>
where
    F: AggregateFunction,
    F::State: 'static,
{
    type State = Box<dyn Any + Send>;

    fn initial_state(&self) -> Self::State {
        Box::new(self.inner.initial_state())
    }

    fn step(&self, state: &mut Self::State, args: &[SqliteValue]) -> Result<()> {
        let concrete = state
            .downcast_mut::<F::State>()
            .expect("aggregate state type mismatch");
        self.inner.step(concrete, args)
    }

    fn finalize(&self, state: Self::State) -> Result<SqliteValue> {
        let concrete = *state
            .downcast::<F::State>()
            .expect("aggregate state type mismatch");
        self.inner.finalize(concrete)
    }

    fn num_args(&self) -> i32 {
        self.inner.num_args()
    }

    fn min_args(&self) -> i32 {
        self.inner.min_args()
    }

    fn max_args(&self) -> Option<i32> {
        self.inner.max_args()
    }

    fn name(&self) -> &str {
        self.inner.name()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    // -- Mock: Sum aggregate --

    struct SumAgg;

    impl AggregateFunction for SumAgg {
        type State = i64;

        fn initial_state(&self) -> i64 {
            0
        }

        fn step(&self, state: &mut i64, args: &[SqliteValue]) -> Result<()> {
            *state += args[0].to_integer();
            Ok(())
        }

        fn finalize(&self, state: i64) -> Result<SqliteValue> {
            Ok(SqliteValue::Integer(state))
        }

        fn num_args(&self) -> i32 {
            1
        }

        fn name(&self) -> &str {
            "sum"
        }
    }

    #[test]
    fn test_aggregate_initial_state() {
        let agg = SumAgg;
        assert_eq!(agg.initial_state(), 0);
    }

    #[test]
    fn test_aggregate_step_and_finalize() {
        let agg = SumAgg;
        let mut state = agg.initial_state();

        agg.step(&mut state, &[SqliteValue::Integer(10)]).unwrap();
        agg.step(&mut state, &[SqliteValue::Integer(20)]).unwrap();
        agg.step(&mut state, &[SqliteValue::Integer(12)]).unwrap();

        let result = agg.finalize(state).unwrap();
        assert_eq!(result, SqliteValue::Integer(42));
    }

    #[test]
    fn test_aggregate_type_erasure_adapter() {
        let adapted: AggregateAdapter<SumAgg> = AggregateAdapter::new(SumAgg);
        let erased: Arc<dyn AggregateFunction<State = Box<dyn Any + Send>>> = Arc::new(adapted);

        let mut state = erased.initial_state();
        erased
            .step(&mut state, &[SqliteValue::Integer(10)])
            .unwrap();
        erased
            .step(&mut state, &[SqliteValue::Integer(32)])
            .unwrap();

        let result = erased.finalize(state).unwrap();
        assert_eq!(result, SqliteValue::Integer(42));

        // Verify we can clone the Arc (shared across threads).
        let e2 = Arc::clone(&erased);
        assert_eq!(e2.name(), "sum");
    }
}
