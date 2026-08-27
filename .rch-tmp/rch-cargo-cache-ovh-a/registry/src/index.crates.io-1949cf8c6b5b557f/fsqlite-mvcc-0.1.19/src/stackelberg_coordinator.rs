//! Stackelberg commit coordinator (IMPL-27).
//!
//! Scaffolding-only — this coordinator is not wired into real commit paths yet.
//!
//! The coordinator plays the leader in a Stackelberg game against a pool of
//! commit-issuing followers: each follower submits a `CommitRequest` with a
//! declared priority and work estimate, and the leader picks a schedule that
//! maximises aggregate weighted throughput.
//!
//! For now we ship the classical weighted-shortest-processing-time heuristic
//! (WSPT / Smith's rule): sort descending by `priority / work_estimate`. On a
//! single machine with preemption disallowed this is the provably optimal
//! schedule for minimising weighted sum of completion times; it also happens
//! to be the Nash-equilibrium schedule when followers truthfully declare work.
//! For small `N` a branch-and-bound search over permutations would give a
//! tighter bound, but WSPT is within a small constant factor and is what we
//! want as a baseline.

use core::cmp::Ordering;

/// A single pending commit awaiting coordinator approval.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CommitRequest {
    /// Stable identifier — typically a transaction id.
    pub id: u64,
    /// Declared priority. Higher is more urgent. Non-finite or negative
    /// priorities are treated as zero.
    pub priority: f64,
    /// Declared work estimate (CPU, IO, bytes, whatever the caller picks).
    /// Must be strictly positive; non-positive or non-finite values get
    /// coerced to a small epsilon so they sort to the *front* of the queue
    /// (zero work, any priority is a free win).
    pub work_estimate: f64,
}

/// Smallest effective work estimate, protects divisions.
const MIN_WORK: f64 = 1e-9;

/// Stackelberg (leader-follower) commit coordinator.
///
/// Stateless on purpose: the scheduler is a pure function of the pending set,
/// which makes it trivial to unit test and to swap out for an LP/QP solver
/// later without touching callers.
#[derive(Debug, Default, Clone, Copy)]
pub struct StackelbergCoordinator;

impl StackelbergCoordinator {
    /// Produce a schedule (ordered list of commit ids) from the pending set.
    ///
    /// The heuristic is weighted-shortest-processing-time: sort descending by
    /// `priority / work_estimate`. Ties break on `work_estimate` ascending,
    /// then on `id` ascending for determinism.
    #[must_use]
    pub fn schedule(pending: &[CommitRequest]) -> Vec<u64> {
        if pending.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<(f64, f64, u64)> = pending
            .iter()
            .map(|r| {
                let prio = if r.priority.is_finite() && r.priority > 0.0 {
                    r.priority
                } else {
                    0.0
                };
                let work = if r.work_estimate.is_finite() && r.work_estimate > 0.0 {
                    r.work_estimate
                } else {
                    MIN_WORK
                };
                (prio / work, work, r.id)
            })
            .collect();

        // Sort: ratio DESC, work ASC, id ASC.
        scored.sort_by(
            |a, b| match b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal) {
                Ordering::Equal => match a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal) {
                    Ordering::Equal => a.2.cmp(&b.2),
                    other => other,
                },
                other => other,
            },
        );

        scored.into_iter().map(|(_, _, id)| id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stackelberg_greedy_orders_by_ratio() {
        // ratios: id=1 -> 1/2=0.5, id=2 -> 2/1=2.0, id=3 -> 3/3=1.0.
        // Expected descending order: id=2, id=3, id=1.
        let pending = [
            CommitRequest {
                id: 1,
                priority: 1.0,
                work_estimate: 2.0,
            },
            CommitRequest {
                id: 2,
                priority: 2.0,
                work_estimate: 1.0,
            },
            CommitRequest {
                id: 3,
                priority: 3.0,
                work_estimate: 3.0,
            },
        ];
        assert_eq!(StackelbergCoordinator::schedule(&pending), vec![2, 3, 1]);
    }

    #[test]
    fn stackelberg_empty_input_returns_empty() {
        assert_eq!(StackelbergCoordinator::schedule(&[]), Vec::<u64>::new());
    }

    #[test]
    fn stackelberg_ties_break_on_work_then_id() {
        // Same ratio = 1.0, but smaller work should come first.
        let pending = [
            CommitRequest {
                id: 10,
                priority: 4.0,
                work_estimate: 4.0,
            },
            CommitRequest {
                id: 11,
                priority: 2.0,
                work_estimate: 2.0,
            },
            CommitRequest {
                id: 12,
                priority: 1.0,
                work_estimate: 1.0,
            },
        ];
        assert_eq!(StackelbergCoordinator::schedule(&pending), vec![12, 11, 10]);
    }

    #[test]
    fn stackelberg_sanitises_bad_inputs() {
        // NaN priority -> treated as 0 ratio. Non-positive work -> epsilon.
        let pending = [
            CommitRequest {
                id: 1,
                priority: f64::NAN,
                work_estimate: 1.0,
            },
            CommitRequest {
                id: 2,
                priority: 5.0,
                work_estimate: 0.0,
            },
            CommitRequest {
                id: 3,
                priority: 1.0,
                work_estimate: 1.0,
            },
        ];
        // id=2 has huge effective ratio (5 / epsilon), should come first.
        // id=3 has ratio 1.0, id=1 has ratio 0.0.
        let order = StackelbergCoordinator::schedule(&pending);
        assert_eq!(order[0], 2);
        assert_eq!(order[1], 3);
        assert_eq!(order[2], 1);
    }
}
