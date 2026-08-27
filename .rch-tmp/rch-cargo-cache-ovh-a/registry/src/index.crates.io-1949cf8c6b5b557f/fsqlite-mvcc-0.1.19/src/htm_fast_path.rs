//! Shared HTM fast-path contract surface for combiner-style batching.
//!
//! This module is intentionally non-operative. It codifies the rollout shape
//! already described in `crates/fsqlite-mvcc/HTM_GUARD_DESIGN.md` and gives
//! later beads one shared place to wire a real hardware probe and a safe HTM
//! backend without editing the live combiner loops first.
//!
//! Current status:
//! - `flat_combining.rs` already carries a Phase 1 guard skeleton.
//! - `commit_combiner.rs` still uses the pure lock path.
//! - This module defines the shared state names, retry policy, probe surface,
//!   and combiner integration contracts both sites will use when the real HTM
//!   backend lands.
//!
//! Non-goals for this slice:
//! - No CPUID or platform intrinsics.
//! - No `unsafe`.
//! - No behavior change in the existing flat combiner or commit combiner.
//! - No new Cargo features or dependencies.

/// Combiner sites that may eventually use the HTM fast path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombinerSite {
    /// `flat_combining.rs::FcHandle::apply`
    FlatCombiner,
    /// `commit_combiner.rs::CommitCombineHandle::alloc`
    CommitSequenceCombiner,
}

impl CombinerSite {
    /// Stable symbolic identifier for diagnostics and manifest output.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FlatCombiner => "flat_combiner",
            Self::CommitSequenceCombiner => "commit_sequence_combiner",
        }
    }
}

/// Shared HTM fast-path state names across combiner implementations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FastPathState {
    NotProbed,
    Available,
    Unavailable,
    Blacklisted,
    Disabled,
    UserDisabled,
}

impl FastPathState {
    /// Stable string name used by metrics, traces, and future SQL surfaces.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotProbed => "not_probed",
            Self::Available => "available",
            Self::Unavailable => "unavailable",
            Self::Blacklisted => "blacklisted",
            Self::Disabled => "disabled",
            Self::UserDisabled => "user_disabled",
        }
    }

    /// Compact persisted representation for future shmem or telemetry export.
    #[must_use]
    pub const fn to_raw(self) -> u8 {
        match self {
            Self::NotProbed => 0,
            Self::Available => 1,
            Self::Unavailable => 2,
            Self::Blacklisted => 3,
            Self::Disabled => 4,
            Self::UserDisabled => 5,
        }
    }

    /// Decode a compact state representation.
    #[must_use]
    pub const fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::NotProbed),
            1 => Some(Self::Available),
            2 => Some(Self::Unavailable),
            3 => Some(Self::Blacklisted),
            4 => Some(Self::Disabled),
            5 => Some(Self::UserDisabled),
            _ => None,
        }
    }
}

/// Why the stub fast path falls back to the existing combiner lock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackReason {
    NotProbedYet,
    ProbeMarkedUnavailable,
    BlacklistedStepping,
    DynamicDisableActive,
    UserDisabled,
}

impl FallbackReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotProbedYet => "not_probed_yet",
            Self::ProbeMarkedUnavailable => "probe_marked_unavailable",
            Self::BlacklistedStepping => "blacklisted_stepping",
            Self::DynamicDisableActive => "dynamic_disable_active",
            Self::UserDisabled => "user_disabled",
        }
    }
}

/// Decision surface a combiner call site uses before attempting HTM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptDisposition {
    AttemptHardwareTransaction,
    FallBackToLock(FallbackReason),
}

impl FastPathState {
    /// Convert a state into the default call-site decision.
    #[must_use]
    pub const fn disposition(self) -> AttemptDisposition {
        match self {
            Self::Available => AttemptDisposition::AttemptHardwareTransaction,
            Self::NotProbed => AttemptDisposition::FallBackToLock(FallbackReason::NotProbedYet),
            Self::Unavailable => {
                AttemptDisposition::FallBackToLock(FallbackReason::ProbeMarkedUnavailable)
            }
            Self::Blacklisted => {
                AttemptDisposition::FallBackToLock(FallbackReason::BlacklistedStepping)
            }
            Self::Disabled => {
                AttemptDisposition::FallBackToLock(FallbackReason::DynamicDisableActive)
            }
            Self::UserDisabled => AttemptDisposition::FallBackToLock(FallbackReason::UserDisabled),
        }
    }
}

/// Shared retry and dynamic-disable policy for the HTM fast path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Per-invocation retry budget before the combiner falls back to the lock.
    pub max_retries: u32,
    /// EWMA alpha in fixed point, where `3000 == 0.3`.
    pub ewma_alpha_fp: u32,
    /// Dynamic disable threshold in fixed point, where `5000 == 50%`.
    pub disable_threshold_fp: u32,
    /// Window size used by the abort-rate monitor.
    pub ewma_window_size: u64,
    /// Initial cooldown after a dynamic disable event.
    pub initial_cooldown_ms: u64,
    /// Maximum cooldown once backoff saturates.
    pub max_cooldown_ms: u64,
}

impl RetryPolicy {
    /// Compute the cooldown after `disable_count` prior disable events.
    #[must_use]
    pub const fn cooldown_ms(self, disable_count: u32) -> u64 {
        let shift = if disable_count >= 63 {
            63
        } else {
            disable_count
        };
        let scaled = self.initial_cooldown_ms.saturating_mul(1_u64 << shift);
        if scaled > self.max_cooldown_ms {
            self.max_cooldown_ms
        } else {
            scaled
        }
    }
}

/// Default retry policy taken from the existing Phase 1 guard scaffolding.
pub const DEFAULT_RETRY_POLICY: RetryPolicy = RetryPolicy {
    max_retries: 3,
    ewma_alpha_fp: 3000,
    disable_threshold_fp: 5000,
    ewma_window_size: 1000,
    initial_cooldown_ms: 5000,
    max_cooldown_ms: 60_000,
};

/// Why the current platform probe did not enable HTM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeReason {
    /// This slice only defines the contract surface; no CPU probe backend yet.
    StubNoBackend,
    /// The current target architecture has no planned HTM backend.
    UnsupportedArchitecture,
}

impl ProbeReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StubNoBackend => "stub_no_backend",
            Self::UnsupportedArchitecture => "unsupported_architecture",
        }
    }
}

/// Result of probing whether the current platform can support the HTM fast path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlatformProbe {
    pub state: FastPathState,
    pub tsx_available: bool,
    pub tme_available: bool,
    pub vendor: &'static str,
    pub stepping: &'static str,
    pub reason: ProbeReason,
}

/// Stub probe used by this slice. It is explicit about being non-operative.
#[must_use]
pub const fn probe_current_platform_stub() -> PlatformProbe {
    #[cfg(target_arch = "x86_64")]
    {
        PlatformProbe {
            state: FastPathState::Unavailable,
            tsx_available: false,
            tme_available: false,
            vendor: "x86_64",
            stepping: "unknown",
            reason: ProbeReason::StubNoBackend,
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        PlatformProbe {
            state: FastPathState::Unavailable,
            tsx_available: false,
            tme_available: false,
            vendor: "non_x86_64",
            stepping: "unknown",
            reason: ProbeReason::UnsupportedArchitecture,
        }
    }
}

/// Design-time integration contract for one combiner site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CombinerContract {
    pub site: CombinerSite,
    pub hot_entrypoint: &'static str,
    pub fallback_entrypoint: &'static str,
    pub telemetry_target: &'static str,
    pub metrics_surface: &'static str,
    pub design_doc: &'static str,
}

/// Shared HTM fast-path contracts for both combiner sites.
pub const COMBINER_CONTRACTS: [CombinerContract; 2] = [
    CombinerContract {
        site: CombinerSite::FlatCombiner,
        hot_entrypoint: "flat_combining.rs::FcHandle::apply",
        fallback_entrypoint: "flat_combining.rs::FlatCombiner::combine_locked",
        telemetry_target: "fsqlite::htm",
        metrics_surface: "flat_combining_metrics_from",
        design_doc: "crates/fsqlite-mvcc/HTM_GUARD_DESIGN.md",
    },
    CombinerContract {
        site: CombinerSite::CommitSequenceCombiner,
        hot_entrypoint: "commit_combiner.rs::CommitCombineHandle::alloc",
        fallback_entrypoint: "commit_combiner.rs::CommitSequenceCombiner::combine_locked",
        telemetry_target: "fsqlite::htm",
        metrics_surface: "future commit-combiner HTM metrics surface",
        design_doc: "crates/fsqlite-mvcc/HTM_GUARD_DESIGN.md",
    },
];

/// Candidate primitive considered for the bd-db300.5.2.3 residual serialized
/// entry region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResidualFastPathCandidate {
    /// Hardware transaction around the combiner body, with lock fallback.
    HardwareTransaction,
    /// Per-socket or per-cohort reservation blocks reconciled into a total
    /// commit order.
    CohortReservation,
    /// The existing lock-backed combiner path.
    LockFallback,
}

impl ResidualFastPathCandidate {
    /// Stable symbolic identifier for the evaluation table.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HardwareTransaction => "hardware_transaction",
            Self::CohortReservation => "cohort_reservation",
            Self::LockFallback => "lock_fallback",
        }
    }
}

/// Decision made by bd-db300.5.2.3 for one residual fast-path candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResidualFastPathDecision {
    /// Keep the design hook, but do not enable without real abort telemetry and
    /// tail-latency evidence.
    DeferUntilAbortTelemetry,
    /// Do not use this primitive until an ordering proof removes the risk.
    RejectUntilOrderingProof,
    /// Keep this path as the mandatory always-correct execution route.
    RetainMandatoryFallback,
}

impl ResidualFastPathDecision {
    /// Stable symbolic identifier for diagnostics and docs.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DeferUntilAbortTelemetry => "defer_until_abort_telemetry",
            Self::RejectUntilOrderingProof => "reject_until_ordering_proof",
            Self::RetainMandatoryFallback => "retain_mandatory_fallback",
        }
    }
}

/// Evaluation record for the residual E2.3 fast-path candidates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResidualFastPathEvaluation {
    pub candidate: ResidualFastPathCandidate,
    pub decision: ResidualFastPathDecision,
    pub serialized_region: &'static str,
    pub abort_evidence: &'static str,
    pub fallback_evidence: &'static str,
    pub tail_evidence: &'static str,
    pub mandatory_fallback: bool,
    pub entrypoint: &'static str,
    pub design_doc: &'static str,
}

/// bd-db300.5.2.3 evaluation matrix. This is deliberately descriptive rather
/// than operative: it records the recommendation while preserving the existing
/// lock path as the only active behavior.
pub const DB300_E2_3_RESIDUAL_FAST_PATH_EVALUATION: [ResidualFastPathEvaluation; 3] = [
    ResidualFastPathEvaluation {
        candidate: ResidualFastPathCandidate::HardwareTransaction,
        decision: ResidualFastPathDecision::DeferUntilAbortTelemetry,
        serialized_region: "CommitSequenceCombiner::combine_locked plus flat combiner batch body",
        abort_evidence: "probe_current_platform_stub reports unavailable; no runtime abort stream is enabled",
        fallback_evidence: "FallbackReason::ProbeMarkedUnavailable routes to the existing combiner lock",
        tail_evidence: "must compare wait_ns_max and benchmark p95/p99 before any attempt state can become available",
        mandatory_fallback: true,
        entrypoint: "commit_combiner.rs::CommitCombineHandle::alloc",
        design_doc: "crates/fsqlite-mvcc/HTM_GUARD_DESIGN.md#14-bd-db300523-evaluation",
    },
    ResidualFastPathEvaluation {
        candidate: ResidualFastPathCandidate::CohortReservation,
        decision: ResidualFastPathDecision::RejectUntilOrderingProof,
        serialized_region: "global commit-sequence allocation and active-registry publication",
        abort_evidence: "not applicable: cohort reservations do not provide abort telemetry",
        fallback_evidence: "existing single total-order combiner remains the fallback and authority",
        tail_evidence: "only reconsider if commit-combine wait tail dominates after fused entry and publication work",
        mandatory_fallback: true,
        entrypoint: "commit_combiner.rs::CommitSequenceCombiner::alloc_one_shot",
        design_doc: "crates/fsqlite-mvcc/HTM_GUARD_DESIGN.md#14-bd-db300523-evaluation",
    },
    ResidualFastPathEvaluation {
        candidate: ResidualFastPathCandidate::LockFallback,
        decision: ResidualFastPathDecision::RetainMandatoryFallback,
        serialized_region: "existing combiner_lock-protected batch publication",
        abort_evidence: "not applicable: no speculative region",
        fallback_evidence: "mandatory fallback path for every unavailable, disabled, aborted, or rejected fast path",
        tail_evidence: "current metrics expose wait_ns_total and wait_ns_max for scoped tail checks",
        mandatory_fallback: true,
        entrypoint: "flat_combining.rs::FlatCombiner::combine_locked",
        design_doc: "crates/fsqlite-mvcc/HTM_GUARD_DESIGN.md#14-bd-db300523-evaluation",
    },
];

/// Lookup helper for the static combiner contracts.
#[must_use]
pub const fn contract_for(site: CombinerSite) -> CombinerContract {
    match site {
        CombinerSite::FlatCombiner => COMBINER_CONTRACTS[0],
        CombinerSite::CommitSequenceCombiner => COMBINER_CONTRACTS[1],
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AttemptDisposition, COMBINER_CONTRACTS, CombinerSite,
        DB300_E2_3_RESIDUAL_FAST_PATH_EVALUATION, DEFAULT_RETRY_POLICY, FallbackReason,
        FastPathState, ProbeReason, ResidualFastPathCandidate, ResidualFastPathDecision,
        contract_for, probe_current_platform_stub,
    };

    #[test]
    fn test_fast_path_state_raw_roundtrip() {
        for (raw, expected_name) in [
            (0, "not_probed"),
            (1, "available"),
            (2, "unavailable"),
            (3, "blacklisted"),
            (4, "disabled"),
            (5, "user_disabled"),
        ] {
            let state = FastPathState::from_raw(raw).expect("known HTM state");
            assert_eq!(state.to_raw(), raw);
            assert_eq!(state.as_str(), expected_name);
        }
        assert!(FastPathState::from_raw(6).is_none());
    }

    #[test]
    fn test_retry_policy_backoff_clamps_to_max() {
        assert_eq!(DEFAULT_RETRY_POLICY.cooldown_ms(0), 5_000);
        assert_eq!(DEFAULT_RETRY_POLICY.cooldown_ms(1), 10_000);
        assert_eq!(DEFAULT_RETRY_POLICY.cooldown_ms(3), 40_000);
        assert_eq!(DEFAULT_RETRY_POLICY.cooldown_ms(4), 60_000);
        assert_eq!(DEFAULT_RETRY_POLICY.cooldown_ms(12), 60_000);
    }

    #[test]
    fn test_contracts_cover_both_combiner_sites() {
        assert_eq!(COMBINER_CONTRACTS.len(), 2);
        assert_eq!(
            contract_for(CombinerSite::FlatCombiner).hot_entrypoint,
            "flat_combining.rs::FcHandle::apply"
        );
        assert_eq!(
            contract_for(CombinerSite::CommitSequenceCombiner).hot_entrypoint,
            "commit_combiner.rs::CommitCombineHandle::alloc"
        );
    }

    #[test]
    fn test_probe_stub_is_explicitly_non_operational() {
        let probe = probe_current_platform_stub();
        assert_eq!(probe.state, FastPathState::Unavailable);
        #[cfg(target_arch = "x86_64")]
        assert_eq!(probe.reason, ProbeReason::StubNoBackend);
        #[cfg(not(target_arch = "x86_64"))]
        assert_eq!(probe.reason, ProbeReason::UnsupportedArchitecture);
    }

    #[test]
    fn test_available_state_maps_to_attempt_disposition() {
        assert_eq!(
            FastPathState::Available.disposition(),
            AttemptDisposition::AttemptHardwareTransaction
        );
        assert_eq!(
            FastPathState::Disabled.disposition(),
            AttemptDisposition::FallBackToLock(FallbackReason::DynamicDisableActive)
        );
    }

    #[test]
    fn test_e2_3_evaluation_keeps_fallback_mandatory() {
        for evaluation in DB300_E2_3_RESIDUAL_FAST_PATH_EVALUATION {
            assert!(evaluation.mandatory_fallback);
            assert!(
                evaluation.fallback_evidence.contains("fallback")
                    || evaluation.fallback_evidence.contains("Fallback")
                    || evaluation.fallback_evidence.contains("authority")
            );
        }
    }

    #[test]
    fn test_e2_3_htm_is_deferred_until_abort_telemetry_exists() {
        let htm = DB300_E2_3_RESIDUAL_FAST_PATH_EVALUATION
            .iter()
            .find(|evaluation| {
                evaluation.candidate == ResidualFastPathCandidate::HardwareTransaction
            })
            .copied();

        assert_eq!(
            htm.map(|evaluation| evaluation.decision),
            Some(ResidualFastPathDecision::DeferUntilAbortTelemetry)
        );
        assert_eq!(
            probe_current_platform_stub().state,
            FastPathState::Unavailable
        );
        assert_eq!(
            FastPathState::Unavailable.disposition(),
            AttemptDisposition::FallBackToLock(FallbackReason::ProbeMarkedUnavailable)
        );
    }

    #[test]
    fn test_e2_3_cohort_path_is_rejected_until_ordering_proof_exists() {
        let cohort = DB300_E2_3_RESIDUAL_FAST_PATH_EVALUATION
            .iter()
            .find(|evaluation| evaluation.candidate == ResidualFastPathCandidate::CohortReservation)
            .copied();

        assert_eq!(
            cohort.map(|evaluation| evaluation.decision),
            Some(ResidualFastPathDecision::RejectUntilOrderingProof)
        );
        assert!(cohort.is_some_and(|evaluation| {
            evaluation.serialized_region.contains("commit-sequence")
                && evaluation.fallback_evidence.contains("total-order")
        }));
    }
}
