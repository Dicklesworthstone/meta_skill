//! Conflict-topology-aware writer-routing telemetry inputs.
//!
//! Track E5.1 does not introduce a second telemetry stack. Instead, it pins
//! the existing MVCC/VDBE conflict signals to a stable contract so later
//! routing beads can consume the same hot-path evidence without reopening the
//! capture design.

use std::collections::HashMap;

use fsqlite_types::{CommitSeq, PageNumber, TxnEpoch, TxnId, TxnToken};
use smallvec::SmallVec;

use crate::ssi_validation::DiscoveredEdge;

/// Stable signal identifiers for writer-routing telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WriterRoutingTelemetrySignal {
    TieredWriteCounts,
    ReadPages,
    WriteSetPages,
    HeldLockPages,
    ConflictOnlyPages,
    MetadataExemptPages,
    SamePageConflictPages,
    PageLockWait,
    BusyRetry,
    StaleSnapshotReject,
    PageOneConflictOnly,
    PendingSurfaceClear,
    LockHolderClues,
    SerializableConflictEdges,
}

/// High-level grouping for routing inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WriterRoutingTelemetryClass {
    TouchSurface,
    ConflictHistory,
    OwnershipLineage,
}

/// Phase that currently produces the signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WriterRoutingTelemetryPhase {
    StatementExecution,
    FirstTouchLockAcquire,
    CommitPlanning,
    CommitFinalize,
    RetryLoop,
}

/// Payload shape exposed by the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WriterRoutingTelemetryShape {
    Counter,
    DurationCounter,
    PageSet,
    OwnershipSet,
    EdgeSet,
}

/// Capture-cost rule for the current hot path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WriterRoutingTelemetryCaptureCost {
    /// Reuse an already-maintained counter or timer.
    ExistingCounter,
    /// Reuse an already-maintained in-memory page/txn set.
    ExistingSet,
    /// Clone the data once at prepare/finalize, not per page-touch.
    PrepareBoundaryClone,
    /// Fold existing telemetry after the hot path has completed.
    DeferredFold,
}

/// Design-time source contract for one routing signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriterRoutingTelemetrySourceSpec {
    /// Stable signal identifier.
    pub signal: WriterRoutingTelemetrySignal,
    /// Touch-surface vs conflict-history vs ownership-lineage grouping.
    pub class: WriterRoutingTelemetryClass,
    /// Phase that owns the signal today.
    pub phase: WriterRoutingTelemetryPhase,
    /// Counter/page-set/edge-set payload shape.
    pub shape: WriterRoutingTelemetryShape,
    /// Concrete code touchpoint producing the evidence.
    pub touchpoint: &'static str,
    /// Existing runtime artifact or counter family to reuse.
    pub current_artifact: &'static str,
    /// Allowed capture budget on the hot path.
    pub hot_path_cost: WriterRoutingTelemetryCaptureCost,
    /// Why a routing policy cares about this signal.
    pub routing_use: &'static str,
}

/// Stable routing-input inventory for Track E5.1.
pub const WRITER_ROUTING_TELEMETRY_SOURCES: [WriterRoutingTelemetrySourceSpec; 14] = [
    WriterRoutingTelemetrySourceSpec {
        signal: WriterRoutingTelemetrySignal::TieredWriteCounts,
        class: WriterRoutingTelemetryClass::TouchSurface,
        phase: WriterRoutingTelemetryPhase::StatementExecution,
        shape: WriterRoutingTelemetryShape::Counter,
        touchpoint: "fsqlite-vdbe/src/engine.rs::SharedTxnPageIo::{classify_concurrent_write_tier,write_page_data}",
        current_artifact: "VDBE mvcc_write_path snapshot tier{0,1,2}_*_writes_total",
        hot_path_cost: WriterRoutingTelemetryCaptureCost::ExistingCounter,
        routing_use: "Distinguish already-owned writes from first-touch and commit-surface expansion pressure.",
    },
    WriterRoutingTelemetrySourceSpec {
        signal: WriterRoutingTelemetrySignal::ReadPages,
        class: WriterRoutingTelemetryClass::TouchSurface,
        phase: WriterRoutingTelemetryPhase::CommitPlanning,
        shape: WriterRoutingTelemetryShape::PageSet,
        touchpoint: "fsqlite-mvcc/src/begin_concurrent.rs::ConcurrentHandle::read_set / PreparedConcurrentCommit::read_pages",
        current_artifact: "ConcurrentHandle read_set summarized into PreparedConcurrentCommit::read_pages()",
        hot_path_cost: WriterRoutingTelemetryCaptureCost::PrepareBoundaryClone,
        routing_use: "Identify readers that repeatedly pivot into conflicting write surfaces.",
    },
    WriterRoutingTelemetrySourceSpec {
        signal: WriterRoutingTelemetrySignal::WriteSetPages,
        class: WriterRoutingTelemetryClass::TouchSurface,
        phase: WriterRoutingTelemetryPhase::CommitPlanning,
        shape: WriterRoutingTelemetryShape::PageSet,
        touchpoint: "fsqlite-mvcc/src/begin_concurrent.rs::ConcurrentHandle::write_set_pages / PreparedConcurrentCommit::write_set_pages",
        current_artifact: "Sorted write-set pages already materialized for FCW/SSI prepare",
        hot_path_cost: WriterRoutingTelemetryCaptureCost::PrepareBoundaryClone,
        routing_use: "Feed same-page conflict history and writer-home locality decisions.",
    },
    WriterRoutingTelemetrySourceSpec {
        signal: WriterRoutingTelemetrySignal::HeldLockPages,
        class: WriterRoutingTelemetryClass::TouchSurface,
        phase: WriterRoutingTelemetryPhase::CommitPlanning,
        shape: WriterRoutingTelemetryShape::PageSet,
        touchpoint: "fsqlite-mvcc/src/begin_concurrent.rs::ConcurrentHandle::held_lock_pages / PreparedConcurrentCommit::held_lock_pages",
        current_artifact: "Tracked held page locks already used for commit finalization and release",
        hot_path_cost: WriterRoutingTelemetryCaptureCost::PrepareBoundaryClone,
        routing_use: "Reveal ownership concentration and lock reuse for later writer placement.",
    },
    WriterRoutingTelemetrySourceSpec {
        signal: WriterRoutingTelemetrySignal::ConflictOnlyPages,
        class: WriterRoutingTelemetryClass::TouchSurface,
        phase: WriterRoutingTelemetryPhase::CommitPlanning,
        shape: WriterRoutingTelemetryShape::PageSet,
        touchpoint: "fsqlite-mvcc/src/begin_concurrent.rs::PageTxnState::is_conflict_only",
        current_artifact: "Synthetic conflict-tracking state embedded in ConcurrentHandle page_states",
        hot_path_cost: WriterRoutingTelemetryCaptureCost::DeferredFold,
        routing_use: "Separate structural conflict surfaces from direct row/page ownership.",
    },
    WriterRoutingTelemetrySourceSpec {
        signal: WriterRoutingTelemetrySignal::MetadataExemptPages,
        class: WriterRoutingTelemetryClass::TouchSurface,
        phase: WriterRoutingTelemetryPhase::CommitPlanning,
        shape: WriterRoutingTelemetryShape::PageSet,
        touchpoint: "fsqlite-mvcc/src/begin_concurrent.rs::PageTxnState::metadata_exempt",
        current_artifact: "Metadata-exempt page marks carried in ConcurrentHandle page_states",
        hot_path_cost: WriterRoutingTelemetryCaptureCost::DeferredFold,
        routing_use: "Prevent routing from overreacting to page-one/freelist metadata that is intentionally conflict-exempt.",
    },
    WriterRoutingTelemetrySourceSpec {
        signal: WriterRoutingTelemetrySignal::SamePageConflictPages,
        class: WriterRoutingTelemetryClass::ConflictHistory,
        phase: WriterRoutingTelemetryPhase::CommitPlanning,
        shape: WriterRoutingTelemetryShape::PageSet,
        touchpoint: "fsqlite-mvcc/src/begin_concurrent.rs::PreparedConcurrentCommit::conflict_pages / validate_first_committer_wins",
        current_artifact: "PreparedConcurrentCommit conflict pages plus FCW conflicting page set",
        hot_path_cost: WriterRoutingTelemetryCaptureCost::PrepareBoundaryClone,
        routing_use: "Measure repeated same-page collisions, the primary topology signal for writer routing.",
    },
    WriterRoutingTelemetrySourceSpec {
        signal: WriterRoutingTelemetrySignal::PageLockWait,
        class: WriterRoutingTelemetryClass::ConflictHistory,
        phase: WriterRoutingTelemetryPhase::FirstTouchLockAcquire,
        shape: WriterRoutingTelemetryShape::DurationCounter,
        touchpoint: "fsqlite-vdbe/src/engine.rs::wait_for_page_lock_holder_change / fsqlite-mvcc/src/core_types.rs::InProcessPageLockTable::wait_for_holder_change",
        current_artifact: "VDBE mvcc_write_path snapshot page_lock_waits_total + page_lock_wait_time_ns_total",
        hot_path_cost: WriterRoutingTelemetryCaptureCost::ExistingCounter,
        routing_use: "Quantify how often ownership handoff blocks first-touch progress.",
    },
    WriterRoutingTelemetrySourceSpec {
        signal: WriterRoutingTelemetrySignal::BusyRetry,
        class: WriterRoutingTelemetryClass::ConflictHistory,
        phase: WriterRoutingTelemetryPhase::RetryLoop,
        shape: WriterRoutingTelemetryShape::Counter,
        touchpoint: "fsqlite-vdbe/src/engine.rs wait/busy loop + fsqlite-core/src/connection.rs begin busy handoff",
        current_artifact: "VDBE mvcc_write_path snapshot write_busy_retries_total + write_busy_timeouts_total",
        hot_path_cost: WriterRoutingTelemetryCaptureCost::ExistingCounter,
        routing_use: "Expose retried lock conflicts separately from hard stale-snapshot aborts.",
    },
    WriterRoutingTelemetrySourceSpec {
        signal: WriterRoutingTelemetrySignal::StaleSnapshotReject,
        class: WriterRoutingTelemetryClass::ConflictHistory,
        phase: WriterRoutingTelemetryPhase::RetryLoop,
        shape: WriterRoutingTelemetryShape::Counter,
        touchpoint: "fsqlite-vdbe/src/engine.rs stale-snapshot rejection sites + fsqlite-mvcc/src/begin_concurrent.rs::validate_first_committer_wins",
        current_artifact: "VDBE mvcc_write_path snapshot stale_snapshot_rejects_total",
        hot_path_cost: WriterRoutingTelemetryCaptureCost::ExistingCounter,
        routing_use: "Tell routing when conflicts are snapshot-age driven rather than raw lock ownership.",
    },
    WriterRoutingTelemetrySourceSpec {
        signal: WriterRoutingTelemetrySignal::PageOneConflictOnly,
        class: WriterRoutingTelemetryClass::ConflictHistory,
        phase: WriterRoutingTelemetryPhase::CommitPlanning,
        shape: WriterRoutingTelemetryShape::DurationCounter,
        touchpoint: "fsqlite-vdbe/src/engine.rs::track_concurrent_conflict_only_page",
        current_artifact: "VDBE mvcc_write_path snapshot page_one_conflict_tracks_total + page_one_conflict_track_time_ns_total",
        hot_path_cost: WriterRoutingTelemetryCaptureCost::ExistingCounter,
        routing_use: "Separate structural page-one expansion from genuine data-page overlap.",
    },
    WriterRoutingTelemetrySourceSpec {
        signal: WriterRoutingTelemetrySignal::PendingSurfaceClear,
        class: WriterRoutingTelemetryClass::ConflictHistory,
        phase: WriterRoutingTelemetryPhase::CommitPlanning,
        shape: WriterRoutingTelemetryShape::DurationCounter,
        touchpoint: "fsqlite-vdbe/src/engine.rs::SharedTxnPageIo::clear_stale_synthetic_pending_commit_surface",
        current_artifact: "VDBE mvcc_write_path snapshot pending_commit_surface_clears_total + pending_commit_surface_clear_time_ns_total",
        hot_path_cost: WriterRoutingTelemetryCaptureCost::ExistingCounter,
        routing_use: "Show how often synthetic structural state is cleared before routing blames hot pages.",
    },
    WriterRoutingTelemetrySourceSpec {
        signal: WriterRoutingTelemetrySignal::LockHolderClues,
        class: WriterRoutingTelemetryClass::OwnershipLineage,
        phase: WriterRoutingTelemetryPhase::FirstTouchLockAcquire,
        shape: WriterRoutingTelemetryShape::OwnershipSet,
        touchpoint: "fsqlite-mvcc/src/core_types.rs::InProcessPageLockTable::{try_acquire,holder}",
        current_artifact: "Page-lock holder TxnId returned on contention and available through holder(page)",
        hot_path_cost: WriterRoutingTelemetryCaptureCost::ExistingSet,
        routing_use: "Capture remote-ownership clues for the page currently blocking a writer.",
    },
    WriterRoutingTelemetrySourceSpec {
        signal: WriterRoutingTelemetrySignal::SerializableConflictEdges,
        class: WriterRoutingTelemetryClass::OwnershipLineage,
        phase: WriterRoutingTelemetryPhase::CommitPlanning,
        shape: WriterRoutingTelemetryShape::EdgeSet,
        touchpoint: "fsqlite-mvcc/src/begin_concurrent.rs::PreparedConcurrentCommit::{incoming_edges,outgoing_edges,conflicting_txns}",
        current_artifact: "Prepared SSI edge sets and conflicting_txns() result",
        hot_path_cost: WriterRoutingTelemetryCaptureCost::PrepareBoundaryClone,
        routing_use: "Preserve lineage from lock-holder clues to committed serialization conflicts.",
    },
];

/// Per-tier counts for the local MVCC write path.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WriterTierSurfaceCounts {
    pub tier0_already_owned: u64,
    pub tier1_first_touch: u64,
    pub tier2_commit_surface_rare: u64,
}

/// Per-attempt page surfaces relevant to routing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WriterTouchSurfaceTelemetry {
    /// Pages read by the transaction before it became a writer.
    pub read_pages: SmallVec<[PageNumber; 16]>,
    /// Pages directly written or freed by the transaction.
    pub write_set_pages: SmallVec<[PageNumber; 16]>,
    /// Pages whose locks are currently or were recently held by the transaction.
    pub held_lock_pages: SmallVec<[PageNumber; 16]>,
    /// Synthetic conflict-only pages added for structural safety.
    pub conflict_only_pages: SmallVec<[PageNumber; 8]>,
    /// Pages intentionally excluded from FCW conflict tracking.
    pub metadata_exempt_pages: SmallVec<[PageNumber; 4]>,
    /// Pages that actually collided during FCW/SSI prepare.
    pub same_page_conflict_pages: SmallVec<[PageNumber; 8]>,
    /// Aggregate write-path classification counts to combine with the page sets.
    pub tier_counts: WriterTierSurfaceCounts,
}

/// Stable retry-cause labels for second-pass routing telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WriterRetryCause {
    PageLockContention,
    StructuralPageOne,
    PendingSurfaceExpansion,
    PublicationAdvance,
    StaleSnapshot,
    BusyTimeout,
}

/// One retry-cause bucket tied to a small page sample.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriterRetryAttribution {
    pub cause: WriterRetryCause,
    pub count: u64,
    pub wait_nanos: u64,
    pub pages: SmallVec<[PageNumber; 4]>,
}

/// Aggregate conflict-frequency inputs for routing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WriterConflictHistoryTelemetry {
    pub same_page_conflict_count: u64,
    pub page_lock_wait_count: u64,
    pub page_lock_wait_nanos: u64,
    pub busy_retry_count: u64,
    pub busy_timeout_count: u64,
    pub stale_snapshot_reject_count: u64,
    pub page_one_conflict_only_count: u64,
    pub page_one_conflict_only_nanos: u64,
    pub pending_surface_clear_count: u64,
    pub pending_surface_clear_nanos: u64,
    pub retry_attributions: SmallVec<[WriterRetryAttribution; 4]>,
}

/// Immediate ownership clue returned by the page-lock table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriterLockHolderClue {
    pub page: PageNumber,
    pub holder: TxnId,
}

/// Ownership lineage inputs spanning lock holders and SSI edges.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WriterOwnershipLineageTelemetry {
    /// Active lock holders currently blocking the writer.
    pub lock_holder_clues: SmallVec<[WriterLockHolderClue; 8]>,
    /// Distinct txns discovered as conflicting during prepare/finalize.
    pub conflicting_txns: SmallVec<[TxnToken; 8]>,
    /// Incoming rw-antidependencies discovered during prepare.
    pub incoming_edges: SmallVec<[DiscoveredEdge; 4]>,
    /// Outgoing rw-antidependencies discovered during prepare.
    pub outgoing_edges: SmallVec<[DiscoveredEdge; 4]>,
}

/// Routing input bundle assembled from the existing MVCC/VDBE telemetry planes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriterRoutingTelemetryInput {
    pub session_id: Option<u64>,
    pub txn_token: TxnToken,
    pub begin_seq: CommitSeq,
    pub planned_commit_seq: Option<CommitSeq>,
    pub touch_surface: WriterTouchSurfaceTelemetry,
    pub conflict_history: WriterConflictHistoryTelemetry,
    pub ownership_lineage: WriterOwnershipLineageTelemetry,
}

/// Logical writer lane identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WriterRoutingLaneId(u16);

impl WriterRoutingLaneId {
    #[must_use]
    pub const fn new(raw: u16) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Optional NUMA / host partition identifier for a lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WriterRoutingNodeId(u16);

impl WriterRoutingNodeId {
    #[must_use]
    pub const fn new(raw: u16) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Per-lane snapshot consumed by the writer-routing decision function.
///
/// This is intentionally advisory. It represents recent ownership and conflict
/// telemetry already available from E5.1 inputs after they are summarized per
/// lane by a future coordinator, not a correctness-critical authority plane.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WriterRoutingLaneSnapshot {
    pub lane: Option<WriterRoutingLaneId>,
    pub node: Option<WriterRoutingNodeId>,
    /// Pages that recently stayed local to this lane.
    pub home_pages: SmallVec<[PageNumber; 16]>,
    /// Pages whose same-page conflicts have recently been attributed here.
    pub conflict_pages: SmallVec<[PageNumber; 16]>,
    /// Lock-holder txns currently or recently associated with this lane.
    pub lock_holder_txns: SmallVec<[TxnId; 8]>,
    /// SSI-conflicting txns that recently resolved on this lane.
    pub conflicting_txns: SmallVec<[TxnToken; 8]>,
    /// Aggregated recent same-page conflict count.
    pub recent_same_page_conflicts: u64,
    /// Aggregated recent page-lock wait time for this lane.
    pub recent_page_lock_wait_nanos: u64,
    /// Aggregated recent busy retries for this lane.
    pub recent_busy_retries: u64,
    /// Aggregated recent stale-snapshot rejects for this lane.
    pub recent_stale_snapshot_rejects: u64,
    /// Current in-flight writers already placed on this lane.
    pub in_flight_writers: u16,
}

/// Advisory home hint for a new writer.
///
/// The hint may point at a concrete lane, a coarser home node, or both. It is
/// never correctness-critical; stale hints degrade to weaker affinity or are
/// ignored entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriterHomeHint {
    pub home_lane: Option<WriterRoutingLaneId>,
    pub home_node: Option<WriterRoutingNodeId>,
    /// Commit sequence when this hint was last refreshed.
    pub observed_commit_seq: CommitSeq,
}

/// How the current topology interprets the supplied home hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriterHomeHintDisposition {
    Missing,
    FreshLane,
    FreshNode,
    FreshLaneReducedToNode,
    StaleCommitAge,
    StaleTargetUnavailable,
}

/// Explicit degradation mode when the hint cannot be followed literally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriterRoutingHintDegradation {
    None,
    HintIgnoredAsStale,
    HintTargetUnavailable,
    HintOverriddenByConflictHistory,
}

/// Top-level reason for the selected lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriterRoutingDecisionReason {
    FreshHomeLaneHint,
    FreshHomeNodeHint,
    OwnershipLocality,
    ConflictAvoidance,
    StableHashFallback,
    LowestConflictScore,
}

/// Scoring knobs for the advisory routing function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriterRoutingDecisionConfig {
    /// Maximum tolerated hint age in commit-sequence distance.
    pub max_hint_age_commits: u64,
    pub home_page_bonus: i64,
    pub lock_holder_bonus: i64,
    pub conflicting_txn_bonus: i64,
    pub fresh_home_lane_bonus: i64,
    pub fresh_home_node_bonus: i64,
    pub conflict_page_penalty: i64,
    pub recent_same_page_conflict_penalty: i64,
    pub busy_retry_penalty: i64,
    pub stale_snapshot_penalty: i64,
    pub in_flight_writer_penalty: i64,
    /// Each `page_lock_wait_nanos_divisor` nanoseconds contributes one penalty unit.
    pub page_lock_wait_nanos_divisor: u64,
}

impl Default for WriterRoutingDecisionConfig {
    fn default() -> Self {
        Self {
            max_hint_age_commits: 64,
            home_page_bonus: 8,
            lock_holder_bonus: 6,
            conflicting_txn_bonus: 4,
            fresh_home_lane_bonus: 10,
            fresh_home_node_bonus: 4,
            conflict_page_penalty: 12,
            recent_same_page_conflict_penalty: 3,
            busy_retry_penalty: 2,
            stale_snapshot_penalty: 3,
            in_flight_writer_penalty: 1,
            page_lock_wait_nanos_divisor: 50_000,
        }
    }
}

/// Per-candidate scoring breakdown returned for diagnostics and benchmarking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriterRoutingLaneScore {
    pub lane: WriterRoutingLaneId,
    pub node: Option<WriterRoutingNodeId>,
    pub total_score: i64,
    pub home_page_overlap: usize,
    pub lock_holder_overlap: usize,
    pub conflicting_txn_overlap: usize,
    pub conflict_page_overlap: usize,
    pub locality_bonus: i64,
    pub hint_bonus: i64,
    pub conflict_penalty: i64,
}

/// One advisory routing decision for a new writer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriterRoutingDecision {
    pub selected_lane: WriterRoutingLaneId,
    pub selected_node: Option<WriterRoutingNodeId>,
    pub reason: WriterRoutingDecisionReason,
    pub hint_disposition: WriterHomeHintDisposition,
    pub hint_degradation: WriterRoutingHintDegradation,
    pub scores: SmallVec<[WriterRoutingLaneScore; 8]>,
}

/// Error returned when no candidate lanes are available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriterRoutingDecisionError {
    NoCandidateLanes,
}

/// Baseline vs routed placement mode for synthetic writer-routing evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriterRoutingMode {
    BaselineStableHash,
    ConflictTopologyHints,
}

impl WriterRoutingMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BaselineStableHash => "baseline_stable_hash",
            Self::ConflictTopologyHints => "conflict_topology_hints",
        }
    }
}

/// Canonical synthetic workload shapes for routing evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriterRoutingSyntheticWorkload {
    DisjointPages,
    OverlappingPages,
    HotPageContention,
}

impl WriterRoutingSyntheticWorkload {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DisjointPages => "disjoint_pages",
            Self::OverlappingPages => "overlapping_pages",
            Self::HotPageContention => "hot_page_contention",
        }
    }
}

/// Placement profile used when producing synthetic home hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriterRoutingPlacementProfile {
    BaselineUnpinned,
    HomeLanePinned,
    HomeNodePinned,
}

impl WriterRoutingPlacementProfile {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BaselineUnpinned => "baseline_unpinned",
            Self::HomeLanePinned => "home_lane_pinned",
            Self::HomeNodePinned => "home_node_pinned",
        }
    }
}

/// Deterministic synthetic workload configuration for routing evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriterRoutingSyntheticConfig {
    pub scenario_id: String,
    pub workload: WriterRoutingSyntheticWorkload,
    pub placement_profile: WriterRoutingPlacementProfile,
    pub lane_count: u16,
    pub iterations: u32,
    pub writers_per_iteration: u16,
}

impl Default for WriterRoutingSyntheticConfig {
    fn default() -> Self {
        Self {
            scenario_id: "writer_routing.synthetic".to_owned(),
            workload: WriterRoutingSyntheticWorkload::OverlappingPages,
            placement_profile: WriterRoutingPlacementProfile::HomeLanePinned,
            lane_count: 4,
            iterations: 16,
            writers_per_iteration: 4,
        }
    }
}

impl WriterRoutingSyntheticConfig {
    #[must_use]
    pub const fn effective_lane_count(&self) -> u16 {
        if self.lane_count == 0 {
            1
        } else {
            self.lane_count
        }
    }

    #[must_use]
    pub const fn effective_iterations(&self) -> u32 {
        if self.iterations == 0 {
            1
        } else {
            self.iterations
        }
    }

    #[must_use]
    pub const fn effective_writers_per_iteration(&self) -> u16 {
        if self.writers_per_iteration == 0 {
            1
        } else {
            self.writers_per_iteration
        }
    }
}

/// Lane-distribution evidence for the synthetic routing protocol.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WriterRoutingSyntheticFairnessSummary {
    pub lane_writer_counts: Vec<u64>,
    pub max_minus_min_assignments: u64,
}

impl WriterRoutingSyntheticFairnessSummary {
    /// Jain fairness index over `lane_writer_counts`.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn jain_fairness_index(&self) -> f64 {
        if self.lane_writer_counts.is_empty() {
            return 0.0;
        }
        let sum = self.lane_writer_counts.iter().sum::<u64>() as f64;
        let squared_sum = self
            .lane_writer_counts
            .iter()
            .map(|count| {
                let value = *count as f64;
                value * value
            })
            .sum::<f64>();
        if squared_sum == 0.0 {
            return 0.0;
        }
        (sum * sum) / (self.lane_writer_counts.len() as f64 * squared_sum)
    }
}

/// One deterministic synthetic run for a routing mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriterRoutingSyntheticSummary {
    pub scenario_id: String,
    pub workload: WriterRoutingSyntheticWorkload,
    pub placement_profile: WriterRoutingPlacementProfile,
    pub routing_mode: WriterRoutingMode,
    pub total_writers: u64,
    pub total_runtime_nanos: u64,
    pub latency_nanos: Vec<u64>,
    pub conflicts_total: u64,
    pub retries_total: u64,
    pub fallback_decisions_total: u64,
    pub remote_ownership_events: u64,
    pub publication_retry_total: u64,
    pub visibility_handoff_nanos_total: u64,
    pub stale_snapshot_rejects_total: u64,
    pub fairness: WriterRoutingSyntheticFairnessSummary,
}

impl WriterRoutingSyntheticSummary {
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn conflict_rate(&self) -> f64 {
        if self.total_writers == 0 {
            return 0.0;
        }
        self.conflicts_total as f64 / self.total_writers as f64
    }

    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn retry_rate(&self) -> f64 {
        if self.total_writers == 0 {
            return 0.0;
        }
        self.retries_total as f64 / self.total_writers as f64
    }

    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn fallback_rate(&self) -> f64 {
        if self.total_writers == 0 {
            return 0.0;
        }
        self.fallback_decisions_total as f64 / self.total_writers as f64
    }

    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn publication_retry_rate(&self) -> f64 {
        if self.total_writers == 0 {
            return 0.0;
        }
        self.publication_retry_total as f64 / self.total_writers as f64
    }

    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn stale_snapshot_rate(&self) -> f64 {
        if self.total_writers == 0 {
            return 0.0;
        }
        self.stale_snapshot_rejects_total as f64 / self.total_writers as f64
    }

    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn ops_per_sec(&self) -> f64 {
        if self.total_runtime_nanos == 0 {
            return 0.0;
        }
        self.total_writers as f64 / (self.total_runtime_nanos as f64 / 1_000_000_000.0)
    }

    #[must_use]
    pub fn p50_latency_ns(&self) -> u64 {
        percentile_u64(&self.latency_nanos, 0.50)
    }

    #[must_use]
    pub fn p95_latency_ns(&self) -> u64 {
        percentile_u64(&self.latency_nanos, 0.95)
    }

    #[must_use]
    pub fn p99_latency_ns(&self) -> u64 {
        percentile_u64(&self.latency_nanos, 0.99)
    }
}

/// Baseline-versus-routed synthetic comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriterRoutingSyntheticComparison {
    pub baseline: WriterRoutingSyntheticSummary,
    pub routed: WriterRoutingSyntheticSummary,
}

impl WriterRoutingSyntheticComparison {
    #[must_use]
    pub fn conflict_rate_delta(&self) -> f64 {
        self.routed.conflict_rate() - self.baseline.conflict_rate()
    }

    #[must_use]
    pub fn retry_rate_delta(&self) -> f64 {
        self.routed.retry_rate() - self.baseline.retry_rate()
    }

    #[must_use]
    pub fn publication_retry_rate_delta(&self) -> f64 {
        self.routed.publication_retry_rate() - self.baseline.publication_retry_rate()
    }
}

/// Decide which lane or partition should receive a new writer.
///
/// The function is intentionally pure and advisory:
/// - it consumes only existing telemetry and a summarized lane snapshot,
/// - it never introduces correctness-critical ownership state,
/// - stale hints degrade to weaker affinity or are ignored.
pub fn decide_writer_routing_target(
    input: &WriterRoutingTelemetryInput,
    candidate_lanes: &[WriterRoutingLaneSnapshot],
    home_hint: Option<WriterHomeHint>,
    config: WriterRoutingDecisionConfig,
) -> Result<WriterRoutingDecision, WriterRoutingDecisionError> {
    if candidate_lanes.is_empty() {
        return Err(WriterRoutingDecisionError::NoCandidateLanes);
    }
    let valid_lane_count = candidate_lanes
        .iter()
        .filter(|lane| lane.lane.is_some())
        .count();
    if valid_lane_count == 0 {
        return Err(WriterRoutingDecisionError::NoCandidateLanes);
    }

    let current_commit_seq = input.planned_commit_seq.unwrap_or(input.begin_seq);
    let (hint_disposition, preferred_lane, preferred_node) =
        evaluate_home_hint(home_hint, candidate_lanes, current_commit_seq, config);
    let locality_pages = collect_locality_pages(input);
    let conflict_pages = collect_conflict_pages(input);
    let lock_holder_txns = collect_lock_holder_txns(input);
    let conflicting_txns = collect_conflicting_txns(input);
    let anchor_index = stable_anchor_index(input, valid_lane_count);

    let mut best_score_index = 0usize;
    let mut scores = SmallVec::<[WriterRoutingLaneScore; 8]>::new();

    for lane in candidate_lanes {
        let Some(lane_id) = lane.lane else {
            continue;
        };
        let home_page_overlap = count_page_overlap(&locality_pages, &lane.home_pages);
        let lock_holder_overlap = count_txnid_overlap(&lock_holder_txns, &lane.lock_holder_txns);
        let conflicting_txn_overlap =
            count_txn_token_overlap(&conflicting_txns, &lane.conflicting_txns);
        let conflict_page_overlap = count_page_overlap(&conflict_pages, &lane.conflict_pages);

        let locality_bonus = overlap_to_score(home_page_overlap, config.home_page_bonus)
            .saturating_add(overlap_to_score(
                lock_holder_overlap,
                config.lock_holder_bonus,
            ))
            .saturating_add(overlap_to_score(
                conflicting_txn_overlap,
                config.conflicting_txn_bonus,
            ));

        let hint_bonus = if Some(lane_id) == preferred_lane {
            config.fresh_home_lane_bonus
        } else if lane.node.is_some() && lane.node == preferred_node {
            config.fresh_home_node_bonus
        } else {
            0
        };

        let conflict_penalty =
            overlap_to_score(conflict_page_overlap, config.conflict_page_penalty)
                .saturating_add(scale_u64_penalty(
                    lane.recent_same_page_conflicts,
                    config.recent_same_page_conflict_penalty,
                ))
                .saturating_add(scale_u64_penalty(
                    lane.recent_busy_retries,
                    config.busy_retry_penalty,
                ))
                .saturating_add(scale_u64_penalty(
                    lane.recent_stale_snapshot_rejects,
                    config.stale_snapshot_penalty,
                ))
                .saturating_add(scale_u64_penalty(
                    lane.recent_page_lock_wait_nanos / config.page_lock_wait_nanos_divisor.max(1),
                    1,
                ))
                .saturating_add(
                    i64::from(lane.in_flight_writers) * config.in_flight_writer_penalty,
                );

        let total_score = locality_bonus
            .saturating_add(hint_bonus)
            .saturating_sub(conflict_penalty);
        let score_index = scores.len();
        let score = WriterRoutingLaneScore {
            lane: lane_id,
            node: lane.node,
            total_score,
            home_page_overlap,
            lock_holder_overlap,
            conflicting_txn_overlap,
            conflict_page_overlap,
            locality_bonus,
            hint_bonus,
            conflict_penalty,
        };

        if scores.is_empty()
            || candidate_score_better(
                &score,
                &scores[best_score_index],
                score_index,
                best_score_index,
                anchor_index,
            )
        {
            best_score_index = score_index;
        }
        scores.push(score);
    }

    if scores.is_empty() {
        return Err(WriterRoutingDecisionError::NoCandidateLanes);
    }

    let selected = scores[best_score_index];
    let hint_degradation = match hint_disposition {
        WriterHomeHintDisposition::StaleCommitAge => {
            WriterRoutingHintDegradation::HintIgnoredAsStale
        }
        WriterHomeHintDisposition::StaleTargetUnavailable => {
            WriterRoutingHintDegradation::HintTargetUnavailable
        }
        WriterHomeHintDisposition::FreshLane
            if preferred_lane.is_some() && Some(selected.lane) != preferred_lane =>
        {
            WriterRoutingHintDegradation::HintOverriddenByConflictHistory
        }
        WriterHomeHintDisposition::FreshLaneReducedToNode
        | WriterHomeHintDisposition::FreshNode
            if preferred_node.is_some() && selected.node != preferred_node =>
        {
            WriterRoutingHintDegradation::HintOverriddenByConflictHistory
        }
        _ => WriterRoutingHintDegradation::None,
    };

    let fallback_is_neutral = scores.iter().all(|score| {
        score.home_page_overlap == 0
            && score.lock_holder_overlap == 0
            && score.conflicting_txn_overlap == 0
            && score.conflict_page_overlap == 0
            && score.hint_bonus == 0
            && score.conflict_penalty == scores[0].conflict_penalty
            && score.total_score == scores[0].total_score
    });

    let reason = if selected.hint_bonus == config.fresh_home_lane_bonus {
        WriterRoutingDecisionReason::FreshHomeLaneHint
    } else if selected.hint_bonus == config.fresh_home_node_bonus {
        WriterRoutingDecisionReason::FreshHomeNodeHint
    } else if selected.locality_bonus > 0 {
        WriterRoutingDecisionReason::OwnershipLocality
    } else if fallback_is_neutral {
        WriterRoutingDecisionReason::StableHashFallback
    } else if selected.conflict_page_overlap == 0
        && scores.iter().any(|score| score.conflict_page_overlap > 0)
    {
        WriterRoutingDecisionReason::ConflictAvoidance
    } else {
        WriterRoutingDecisionReason::LowestConflictScore
    };

    Ok(WriterRoutingDecision {
        selected_lane: selected.lane,
        selected_node: selected.node,
        reason,
        hint_disposition,
        hint_degradation,
        scores,
    })
}

#[derive(Debug, Clone, Copy)]
struct SyntheticPageOwner {
    lane: WriterRoutingLaneId,
    txn: TxnToken,
}

#[derive(Debug, Clone, Copy, Default)]
struct SyntheticOperationOutcome {
    latency_nanos: u64,
    conflicts: u64,
    retries: u64,
    fallback: bool,
    remote_ownership_events: u64,
    publication_retries: u64,
    visibility_handoff_nanos: u64,
    stale_snapshot_rejects: u64,
}

/// Run one deterministic synthetic workload for one routing mode.
#[must_use]
pub fn evaluate_writer_routing_synthetic_workload(
    config: &WriterRoutingSyntheticConfig,
    routing_mode: WriterRoutingMode,
    decision_config: WriterRoutingDecisionConfig,
) -> WriterRoutingSyntheticSummary {
    let lane_count = usize::from(config.effective_lane_count());
    let iterations = config.effective_iterations();
    let writers_per_iteration = config.effective_writers_per_iteration();

    let mut candidate_lanes = (0..lane_count)
        .map(|lane_index| WriterRoutingLaneSnapshot {
            lane: Some(WriterRoutingLaneId::new(
                u16::try_from(lane_index + 1).unwrap_or(u16::MAX),
            )),
            node: Some(WriterRoutingNodeId::new(
                u16::try_from(lane_index % 2).unwrap_or(u16::MAX),
            )),
            ..WriterRoutingLaneSnapshot::default()
        })
        .collect::<Vec<_>>();
    let mut page_owners = HashMap::<PageNumber, SyntheticPageOwner>::new();
    let mut lane_writer_counts = vec![0_u64; lane_count];

    let mut total_writers = 0_u64;
    let mut total_runtime_nanos = 0_u64;
    let mut latency_nanos = Vec::with_capacity(
        usize::try_from(iterations)
            .unwrap_or(usize::MAX)
            .saturating_mul(usize::from(writers_per_iteration)),
    );
    let mut conflicts_total = 0_u64;
    let mut retries_total = 0_u64;
    let mut fallback_decisions_total = 0_u64;
    let mut remote_ownership_events = 0_u64;
    let mut publication_retry_total = 0_u64;
    let mut visibility_handoff_nanos_total = 0_u64;
    let mut stale_snapshot_rejects_total = 0_u64;

    for writer_index in 0..u64::from(iterations) * u64::from(writers_per_iteration) {
        for lane in &mut candidate_lanes {
            lane.in_flight_writers = lane.in_flight_writers.saturating_sub(1);
        }

        let txn_token = synthetic_txn_token(writer_index.saturating_add(1));
        let session_id = writer_index.saturating_add(1);
        let begin_seq = CommitSeq::new(session_id.saturating_mul(2).saturating_sub(1));
        let planned_commit_seq = CommitSeq::new(session_id.saturating_mul(2));
        let natural_lane_index = usize::try_from(writer_index).unwrap_or(usize::MAX) % lane_count;
        let natural_lane = candidate_lanes[natural_lane_index]
            .lane
            .expect("synthetic lane must exist");
        let pages = synthetic_workload_pages(config.workload, writer_index, natural_lane_index);
        let mut input = WriterRoutingTelemetryInput {
            session_id: Some(session_id),
            txn_token,
            begin_seq,
            planned_commit_seq: Some(planned_commit_seq),
            touch_surface: WriterTouchSurfaceTelemetry::default(),
            conflict_history: WriterConflictHistoryTelemetry::default(),
            ownership_lineage: WriterOwnershipLineageTelemetry::default(),
        };
        input
            .touch_surface
            .write_set_pages
            .extend_from_slice(&pages);
        if matches!(
            config.workload,
            WriterRoutingSyntheticWorkload::DisjointPages
        ) {
            input.touch_surface.read_pages.push(pages[0]);
        }

        let shared_pages = synthetic_shared_pages(config.workload, writer_index);
        for &shared_page in &shared_pages {
            if let Some(owner) = page_owners.get(&shared_page).copied() {
                input
                    .touch_surface
                    .same_page_conflict_pages
                    .push(shared_page);
                input.conflict_history.same_page_conflict_count = input
                    .conflict_history
                    .same_page_conflict_count
                    .saturating_add(1);
                input
                    .ownership_lineage
                    .lock_holder_clues
                    .push(WriterLockHolderClue {
                        page: shared_page,
                        holder: owner.txn.id,
                    });
                if !input
                    .ownership_lineage
                    .conflicting_txns
                    .contains(&owner.txn)
                {
                    input.ownership_lineage.conflicting_txns.push(owner.txn);
                }
                input
                    .conflict_history
                    .retry_attributions
                    .push(WriterRetryAttribution {
                        cause: WriterRetryCause::PublicationAdvance,
                        count: 1,
                        wait_nanos: synthetic_wait_nanos(config.workload),
                        pages: SmallVec::from_slice(&[shared_page]),
                    });
            }
        }

        let home_hint = synthetic_home_hint(
            config.placement_profile,
            &candidate_lanes,
            &page_owners,
            &pages,
            natural_lane,
            planned_commit_seq,
        );

        let (selected_lane, fallback) = match routing_mode {
            WriterRoutingMode::BaselineStableHash => {
                (synthetic_baseline_lane(&candidate_lanes, session_id), true)
            }
            WriterRoutingMode::ConflictTopologyHints => {
                let decision = decide_writer_routing_target(
                    &input,
                    &candidate_lanes,
                    home_hint,
                    decision_config,
                )
                .expect("synthetic candidate lanes must be non-empty");
                (
                    decision.selected_lane,
                    decision.reason == WriterRoutingDecisionReason::StableHashFallback,
                )
            }
        };
        let selected_index = candidate_lanes
            .iter()
            .position(|lane| lane.lane == Some(selected_lane))
            .expect("selected synthetic lane must exist");

        let outcome =
            evaluate_synthetic_selection(config.workload, &pages, selected_lane, &page_owners);
        total_writers = total_writers.saturating_add(1);
        total_runtime_nanos = total_runtime_nanos.saturating_add(outcome.latency_nanos);
        latency_nanos.push(outcome.latency_nanos);
        conflicts_total = conflicts_total.saturating_add(outcome.conflicts);
        retries_total = retries_total.saturating_add(outcome.retries);
        fallback_decisions_total =
            fallback_decisions_total.saturating_add(u64::from(outcome.fallback || fallback));
        remote_ownership_events =
            remote_ownership_events.saturating_add(outcome.remote_ownership_events);
        publication_retry_total =
            publication_retry_total.saturating_add(outcome.publication_retries);
        visibility_handoff_nanos_total =
            visibility_handoff_nanos_total.saturating_add(outcome.visibility_handoff_nanos);
        stale_snapshot_rejects_total =
            stale_snapshot_rejects_total.saturating_add(outcome.stale_snapshot_rejects);
        lane_writer_counts[selected_index] = lane_writer_counts[selected_index].saturating_add(1);

        update_synthetic_lane_snapshot(
            &mut candidate_lanes[selected_index],
            txn_token,
            &pages,
            &shared_pages,
            &outcome,
        );

        for page in pages {
            page_owners.insert(
                page,
                SyntheticPageOwner {
                    lane: selected_lane,
                    txn: txn_token,
                },
            );
        }
    }

    let max_assignments = lane_writer_counts.iter().copied().max().unwrap_or(0);
    let min_assignments = lane_writer_counts.iter().copied().min().unwrap_or(0);

    WriterRoutingSyntheticSummary {
        scenario_id: config.scenario_id.clone(),
        workload: config.workload,
        placement_profile: config.placement_profile,
        routing_mode,
        total_writers,
        total_runtime_nanos,
        latency_nanos,
        conflicts_total,
        retries_total,
        fallback_decisions_total,
        remote_ownership_events,
        publication_retry_total,
        visibility_handoff_nanos_total,
        stale_snapshot_rejects_total,
        fairness: WriterRoutingSyntheticFairnessSummary {
            lane_writer_counts,
            max_minus_min_assignments: max_assignments.saturating_sub(min_assignments),
        },
    }
}

/// Compare baseline stable hashing against the routing decision function on the
/// same synthetic workload.
#[must_use]
pub fn compare_writer_routing_synthetic_workload(
    config: &WriterRoutingSyntheticConfig,
    decision_config: WriterRoutingDecisionConfig,
) -> WriterRoutingSyntheticComparison {
    WriterRoutingSyntheticComparison {
        baseline: evaluate_writer_routing_synthetic_workload(
            config,
            WriterRoutingMode::BaselineStableHash,
            decision_config,
        ),
        routed: evaluate_writer_routing_synthetic_workload(
            config,
            WriterRoutingMode::ConflictTopologyHints,
            decision_config,
        ),
    }
}

fn synthetic_txn_token(raw: u64) -> TxnToken {
    TxnToken::new(
        TxnId::new(raw.max(1)).expect("synthetic txn id must be valid"),
        TxnEpoch::new(1),
    )
}

fn synthetic_workload_pages(
    workload: WriterRoutingSyntheticWorkload,
    writer_index: u64,
    natural_lane_index: usize,
) -> SmallVec<[PageNumber; 4]> {
    let unique_base =
        u32::try_from(1_000_u64.saturating_add(writer_index.saturating_mul(8))).unwrap_or(u32::MAX);
    match workload {
        WriterRoutingSyntheticWorkload::DisjointPages => SmallVec::from_slice(&[
            PageNumber::new(unique_base).expect("synthetic disjoint page should be valid"),
            PageNumber::new(unique_base.saturating_add(1))
                .expect("synthetic disjoint page should be valid"),
        ]),
        WriterRoutingSyntheticWorkload::OverlappingPages => {
            let shared_page = PageNumber::new(
                64_u32.saturating_add(u32::try_from(writer_index % 3).unwrap_or(0)),
            )
            .expect("synthetic overlap page should be valid");
            let unique_page = PageNumber::new(
                2_000_u32
                    .saturating_add(u32::try_from(natural_lane_index).unwrap_or(u32::MAX))
                    .saturating_mul(16)
                    .saturating_add(u32::try_from(writer_index / 3).unwrap_or(0)),
            )
            .expect("synthetic overlap unique page should be valid");
            SmallVec::from_slice(&[shared_page, unique_page])
        }
        WriterRoutingSyntheticWorkload::HotPageContention => SmallVec::from_slice(&[
            PageNumber::ONE,
            PageNumber::new(unique_base.saturating_add(2))
                .expect("synthetic hot-page unique page should be valid"),
        ]),
    }
}

fn synthetic_shared_pages(
    workload: WriterRoutingSyntheticWorkload,
    writer_index: u64,
) -> SmallVec<[PageNumber; 2]> {
    match workload {
        WriterRoutingSyntheticWorkload::DisjointPages => SmallVec::new(),
        WriterRoutingSyntheticWorkload::OverlappingPages => {
            SmallVec::from_slice(&[PageNumber::new(
                64_u32.saturating_add(u32::try_from(writer_index % 3).unwrap_or(0)),
            )
            .expect("synthetic overlap page should be valid")])
        }
        WriterRoutingSyntheticWorkload::HotPageContention => {
            SmallVec::from_slice(&[PageNumber::ONE])
        }
    }
}

fn synthetic_home_hint(
    placement_profile: WriterRoutingPlacementProfile,
    candidate_lanes: &[WriterRoutingLaneSnapshot],
    page_owners: &HashMap<PageNumber, SyntheticPageOwner>,
    pages: &[PageNumber],
    natural_lane: WriterRoutingLaneId,
    current_commit_seq: CommitSeq,
) -> Option<WriterHomeHint> {
    match placement_profile {
        WriterRoutingPlacementProfile::BaselineUnpinned => None,
        WriterRoutingPlacementProfile::HomeLanePinned => {
            let hinted_lane = pages
                .iter()
                .find_map(|page| page_owners.get(page).map(|owner| owner.lane))
                .unwrap_or(natural_lane);
            Some(WriterHomeHint {
                home_lane: Some(hinted_lane),
                home_node: synthetic_lane_node(candidate_lanes, hinted_lane),
                observed_commit_seq: current_commit_seq,
            })
        }
        WriterRoutingPlacementProfile::HomeNodePinned => {
            let hinted_lane = pages
                .iter()
                .find_map(|page| page_owners.get(page).map(|owner| owner.lane));
            let home_node = hinted_lane
                .and_then(|lane| synthetic_lane_node(candidate_lanes, lane))
                .or_else(|| synthetic_lane_node(candidate_lanes, natural_lane));
            Some(WriterHomeHint {
                home_lane: None,
                home_node,
                observed_commit_seq: current_commit_seq,
            })
        }
    }
}

fn synthetic_baseline_lane(
    candidate_lanes: &[WriterRoutingLaneSnapshot],
    session_id: u64,
) -> WriterRoutingLaneId {
    let valid_lanes = candidate_lanes
        .iter()
        .filter_map(|lane| lane.lane)
        .collect::<Vec<_>>();
    let index = usize::try_from(session_id).unwrap_or(usize::MAX) % valid_lanes.len().max(1);
    valid_lanes[index]
}

fn synthetic_lane_node(
    candidate_lanes: &[WriterRoutingLaneSnapshot],
    lane_id: WriterRoutingLaneId,
) -> Option<WriterRoutingNodeId> {
    candidate_lanes
        .iter()
        .find(|lane| lane.lane == Some(lane_id))
        .and_then(|lane| lane.node)
}

fn synthetic_wait_nanos(workload: WriterRoutingSyntheticWorkload) -> u64 {
    match workload {
        WriterRoutingSyntheticWorkload::DisjointPages => 0,
        WriterRoutingSyntheticWorkload::OverlappingPages => 18_000,
        WriterRoutingSyntheticWorkload::HotPageContention => 32_000,
    }
}

fn evaluate_synthetic_selection(
    workload: WriterRoutingSyntheticWorkload,
    pages: &[PageNumber],
    selected_lane: WriterRoutingLaneId,
    page_owners: &HashMap<PageNumber, SyntheticPageOwner>,
) -> SyntheticOperationOutcome {
    let mut outcome = SyntheticOperationOutcome {
        latency_nanos: match workload {
            WriterRoutingSyntheticWorkload::DisjointPages => 12_000,
            WriterRoutingSyntheticWorkload::OverlappingPages => 18_000,
            WriterRoutingSyntheticWorkload::HotPageContention => 24_000,
        },
        ..SyntheticOperationOutcome::default()
    };

    for page in pages {
        let Some(owner) = page_owners.get(page) else {
            continue;
        };
        if owner.lane == selected_lane {
            continue;
        }
        outcome.conflicts = outcome.conflicts.saturating_add(1);
        outcome.remote_ownership_events = outcome.remote_ownership_events.saturating_add(1);
        outcome.publication_retries = outcome.publication_retries.saturating_add(1);
        outcome.visibility_handoff_nanos =
            outcome
                .visibility_handoff_nanos
                .saturating_add(match workload {
                    WriterRoutingSyntheticWorkload::DisjointPages => 0,
                    WriterRoutingSyntheticWorkload::OverlappingPages => 14_000,
                    WriterRoutingSyntheticWorkload::HotPageContention => 26_000,
                });
        outcome.retries = outcome.retries.saturating_add(match workload {
            WriterRoutingSyntheticWorkload::DisjointPages => 0,
            WriterRoutingSyntheticWorkload::OverlappingPages => 1,
            WriterRoutingSyntheticWorkload::HotPageContention => 2,
        });
        outcome.stale_snapshot_rejects =
            outcome
                .stale_snapshot_rejects
                .saturating_add(match workload {
                    WriterRoutingSyntheticWorkload::DisjointPages => 0,
                    WriterRoutingSyntheticWorkload::OverlappingPages
                    | WriterRoutingSyntheticWorkload::HotPageContention => 1,
                });
    }

    outcome.latency_nanos = outcome
        .latency_nanos
        .saturating_add(outcome.conflicts.saturating_mul(48_000))
        .saturating_add(outcome.retries.saturating_mul(22_000))
        .saturating_add(outcome.remote_ownership_events.saturating_mul(11_000))
        .saturating_add(outcome.visibility_handoff_nanos)
        .saturating_add(outcome.stale_snapshot_rejects.saturating_mul(17_000));
    outcome
}

fn update_synthetic_lane_snapshot(
    lane: &mut WriterRoutingLaneSnapshot,
    txn_token: TxnToken,
    pages: &[PageNumber],
    shared_pages: &[PageNumber],
    outcome: &SyntheticOperationOutcome,
) {
    lane.in_flight_writers = lane.in_flight_writers.saturating_add(1);
    push_unique_txnid_capped(&mut lane.lock_holder_txns, txn_token.id, 8);
    push_unique_txn_token_capped(&mut lane.conflicting_txns, txn_token, 8);

    for page in pages {
        push_unique_page_capped(&mut lane.home_pages, *page, 16);
    }
    if outcome.conflicts > 0 {
        for page in shared_pages {
            push_unique_page_capped(&mut lane.conflict_pages, *page, 16);
        }
        lane.recent_same_page_conflicts = lane
            .recent_same_page_conflicts
            .saturating_add(outcome.conflicts);
        lane.recent_busy_retries = lane.recent_busy_retries.saturating_add(outcome.retries);
        lane.recent_stale_snapshot_rejects = lane
            .recent_stale_snapshot_rejects
            .saturating_add(outcome.stale_snapshot_rejects);
        lane.recent_page_lock_wait_nanos = lane
            .recent_page_lock_wait_nanos
            .saturating_add(outcome.visibility_handoff_nanos);
    }
}

fn evaluate_home_hint(
    home_hint: Option<WriterHomeHint>,
    candidate_lanes: &[WriterRoutingLaneSnapshot],
    current_commit_seq: CommitSeq,
    config: WriterRoutingDecisionConfig,
) -> (
    WriterHomeHintDisposition,
    Option<WriterRoutingLaneId>,
    Option<WriterRoutingNodeId>,
) {
    let Some(home_hint) = home_hint else {
        return (WriterHomeHintDisposition::Missing, None, None);
    };

    let hint_age = current_commit_seq
        .get()
        .saturating_sub(home_hint.observed_commit_seq.get());
    if hint_age > config.max_hint_age_commits {
        return (WriterHomeHintDisposition::StaleCommitAge, None, None);
    }

    let preferred_lane = home_hint.home_lane.filter(|lane_id| {
        candidate_lanes
            .iter()
            .any(|candidate| candidate.lane == Some(*lane_id))
    });
    let preferred_node = home_hint.home_node.filter(|node_id| {
        candidate_lanes
            .iter()
            .any(|candidate| candidate.node == Some(*node_id))
    });

    let disposition = match (preferred_lane, preferred_node, home_hint.home_lane) {
        (Some(_), _, _) => WriterHomeHintDisposition::FreshLane,
        (None, Some(_), Some(_)) => WriterHomeHintDisposition::FreshLaneReducedToNode,
        (None, Some(_), None) => WriterHomeHintDisposition::FreshNode,
        (None, None, _) => WriterHomeHintDisposition::StaleTargetUnavailable,
    };

    (disposition, preferred_lane, preferred_node)
}

fn collect_locality_pages(input: &WriterRoutingTelemetryInput) -> SmallVec<[PageNumber; 16]> {
    let mut pages = SmallVec::<[PageNumber; 16]>::new();
    for page in &input.touch_surface.held_lock_pages {
        if !input.touch_surface.metadata_exempt_pages.contains(page)
            && !input.touch_surface.conflict_only_pages.contains(page)
        {
            push_unique_page(&mut pages, *page);
        }
    }
    for page in &input.touch_surface.write_set_pages {
        if !input.touch_surface.metadata_exempt_pages.contains(page)
            && !input.touch_surface.conflict_only_pages.contains(page)
        {
            push_unique_page(&mut pages, *page);
        }
    }
    if pages.is_empty() {
        for page in &input.touch_surface.read_pages {
            if !input.touch_surface.metadata_exempt_pages.contains(page)
                && !input.touch_surface.conflict_only_pages.contains(page)
            {
                push_unique_page(&mut pages, *page);
            }
        }
    }
    pages
}

fn collect_conflict_pages(input: &WriterRoutingTelemetryInput) -> SmallVec<[PageNumber; 16]> {
    let mut pages = SmallVec::<[PageNumber; 16]>::new();
    for page in &input.touch_surface.same_page_conflict_pages {
        if !input.touch_surface.metadata_exempt_pages.contains(page) {
            push_unique_page(&mut pages, *page);
        }
    }
    for attribution in &input.conflict_history.retry_attributions {
        for page in &attribution.pages {
            if !input.touch_surface.metadata_exempt_pages.contains(page) {
                push_unique_page(&mut pages, *page);
            }
        }
    }
    pages
}

fn collect_lock_holder_txns(input: &WriterRoutingTelemetryInput) -> SmallVec<[TxnId; 8]> {
    let mut txns = SmallVec::<[TxnId; 8]>::new();
    for clue in &input.ownership_lineage.lock_holder_clues {
        if !txns.contains(&clue.holder) {
            txns.push(clue.holder);
        }
    }
    txns
}

fn collect_conflicting_txns(input: &WriterRoutingTelemetryInput) -> SmallVec<[TxnToken; 8]> {
    let mut txns = SmallVec::<[TxnToken; 8]>::new();
    for txn in &input.ownership_lineage.conflicting_txns {
        if !txns.contains(txn) {
            txns.push(*txn);
        }
    }
    for edge in &input.ownership_lineage.incoming_edges {
        if edge.from != input.txn_token && !txns.contains(&edge.from) {
            txns.push(edge.from);
        }
        if edge.to != input.txn_token && !txns.contains(&edge.to) {
            txns.push(edge.to);
        }
    }
    for edge in &input.ownership_lineage.outgoing_edges {
        if edge.from != input.txn_token && !txns.contains(&edge.from) {
            txns.push(edge.from);
        }
        if edge.to != input.txn_token && !txns.contains(&edge.to) {
            txns.push(edge.to);
        }
    }
    txns
}

fn push_unique_page(into: &mut SmallVec<[PageNumber; 16]>, page: PageNumber) {
    if !into.contains(&page) {
        into.push(page);
    }
}

fn push_unique_page_capped(into: &mut SmallVec<[PageNumber; 16]>, page: PageNumber, cap: usize) {
    if into.contains(&page) {
        return;
    }
    if into.len() == cap {
        into.remove(0);
    }
    into.push(page);
}

fn push_unique_txnid_capped(into: &mut SmallVec<[TxnId; 8]>, txn: TxnId, cap: usize) {
    if into.contains(&txn) {
        return;
    }
    if into.len() == cap {
        into.remove(0);
    }
    into.push(txn);
}

fn push_unique_txn_token_capped(into: &mut SmallVec<[TxnToken; 8]>, txn: TxnToken, cap: usize) {
    if into.contains(&txn) {
        return;
    }
    if into.len() == cap {
        into.remove(0);
    }
    into.push(txn);
}

fn count_page_overlap(lhs: &[PageNumber], rhs: &[PageNumber]) -> usize {
    lhs.iter().filter(|page| rhs.contains(page)).count()
}

fn count_txnid_overlap(lhs: &[TxnId], rhs: &[TxnId]) -> usize {
    lhs.iter().filter(|txn| rhs.contains(txn)).count()
}

fn count_txn_token_overlap(lhs: &[TxnToken], rhs: &[TxnToken]) -> usize {
    lhs.iter().filter(|txn| rhs.contains(txn)).count()
}

fn overlap_to_score(overlap: usize, weight: i64) -> i64 {
    i64::try_from(overlap)
        .unwrap_or(i64::MAX)
        .saturating_mul(weight)
}

fn scale_u64_penalty(count: u64, weight: i64) -> i64 {
    i64::try_from(count)
        .unwrap_or(i64::MAX)
        .saturating_mul(weight)
}

fn stable_anchor_index(input: &WriterRoutingTelemetryInput, lane_count: usize) -> usize {
    let anchor = input.session_id.unwrap_or_else(|| input.txn_token.id.get());
    usize::try_from(anchor).unwrap_or(usize::MAX) % lane_count.max(1)
}

fn candidate_score_better(
    candidate: &WriterRoutingLaneScore,
    best: &WriterRoutingLaneScore,
    candidate_index: usize,
    best_index: usize,
    anchor_index: usize,
) -> bool {
    if candidate.total_score != best.total_score {
        return candidate.total_score > best.total_score;
    }
    if candidate.hint_bonus != best.hint_bonus {
        return candidate.hint_bonus > best.hint_bonus;
    }
    if candidate.locality_bonus != best.locality_bonus {
        return candidate.locality_bonus > best.locality_bonus;
    }
    if candidate.conflict_penalty != best.conflict_penalty {
        return candidate.conflict_penalty < best.conflict_penalty;
    }

    let candidate_distance = cyclic_distance(candidate_index, anchor_index);
    let best_distance = cyclic_distance(best_index, anchor_index);
    if candidate_distance != best_distance {
        return candidate_distance < best_distance;
    }

    candidate.lane < best.lane
}

fn cyclic_distance(index: usize, anchor: usize) -> usize {
    index.abs_diff(anchor)
}

fn percentile_u64(values: &[u64], p: f64) -> u64 {
    if values.is_empty() {
        return 0;
    }
    if values.len() == 1 {
        return values[0];
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    #[allow(clippy::cast_precision_loss)]
    let idx = p * (sorted.len() - 1) as f64;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let lo = idx.floor() as usize;
    let hi = lo.saturating_add(1).min(sorted.len() - 1);
    #[allow(clippy::cast_precision_loss)]
    let frac = idx - lo as f64;
    #[allow(clippy::cast_precision_loss)]
    let lo_value = sorted[lo] as f64;
    #[allow(clippy::cast_precision_loss)]
    let hi_value = sorted[hi] as f64;
    round_percentile_value(lo_value.mul_add(1.0 - frac, hi_value * frac))
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn round_percentile_value(value: f64) -> u64 {
    value.round() as u64
}

#[cfg(test)]
mod tests {
    use super::{
        WRITER_ROUTING_TELEMETRY_SOURCES, WriterConflictHistoryTelemetry, WriterHomeHint,
        WriterHomeHintDisposition, WriterOwnershipLineageTelemetry, WriterRoutingDecisionConfig,
        WriterRoutingDecisionError, WriterRoutingDecisionReason, WriterRoutingHintDegradation,
        WriterRoutingLaneId, WriterRoutingLaneSnapshot, WriterRoutingNodeId,
        WriterRoutingPlacementProfile, WriterRoutingSyntheticConfig,
        WriterRoutingSyntheticWorkload, WriterRoutingTelemetryCaptureCost,
        WriterRoutingTelemetryClass, WriterRoutingTelemetryInput, WriterRoutingTelemetrySignal,
        WriterTouchSurfaceTelemetry, compare_writer_routing_synthetic_workload,
        decide_writer_routing_target,
    };
    use fsqlite_types::{CommitSeq, PageNumber, TxnEpoch, TxnId, TxnToken};

    fn has_signal(signal: WriterRoutingTelemetrySignal) -> bool {
        WRITER_ROUTING_TELEMETRY_SOURCES
            .iter()
            .any(|source| source.signal == signal)
    }

    #[test]
    fn test_writer_routing_sources_cover_required_first_pass_signals() {
        assert!(has_signal(WriterRoutingTelemetrySignal::TieredWriteCounts));
        assert!(has_signal(WriterRoutingTelemetrySignal::PageLockWait));
        assert!(has_signal(WriterRoutingTelemetrySignal::BusyRetry));
        assert!(has_signal(
            WriterRoutingTelemetrySignal::StaleSnapshotReject
        ));
        assert!(has_signal(
            WriterRoutingTelemetrySignal::PageOneConflictOnly
        ));
        assert!(has_signal(
            WriterRoutingTelemetrySignal::PendingSurfaceClear
        ));
    }

    #[test]
    fn test_writer_routing_sources_cover_same_page_conflicts_and_ownership() {
        assert!(has_signal(WriterRoutingTelemetrySignal::WriteSetPages));
        assert!(has_signal(
            WriterRoutingTelemetrySignal::SamePageConflictPages
        ));
        assert!(has_signal(WriterRoutingTelemetrySignal::LockHolderClues));
        assert!(has_signal(
            WriterRoutingTelemetrySignal::SerializableConflictEdges
        ));
    }

    #[test]
    fn test_writer_routing_hot_path_budget_reuses_existing_planes() {
        let allowed = [
            WriterRoutingTelemetryCaptureCost::ExistingCounter,
            WriterRoutingTelemetryCaptureCost::ExistingSet,
            WriterRoutingTelemetryCaptureCost::PrepareBoundaryClone,
            WriterRoutingTelemetryCaptureCost::DeferredFold,
        ];
        assert!(
            WRITER_ROUTING_TELEMETRY_SOURCES
                .iter()
                .all(|source| allowed.contains(&source.hot_path_cost)),
            "routing telemetry must only reuse existing hot-path state or fold it after the fact"
        );
        assert!(
            WRITER_ROUTING_TELEMETRY_SOURCES
                .iter()
                .any(|source| source.class == WriterRoutingTelemetryClass::OwnershipLineage),
            "routing contract must include ownership lineage, not just counters"
        );
    }

    fn page(n: u32) -> PageNumber {
        PageNumber::new(n).expect("page number should be valid")
    }

    fn txn_id(raw: u64) -> TxnId {
        TxnId::new(raw).expect("txn id should be valid")
    }

    fn txn_token(raw: u64) -> TxnToken {
        TxnToken::new(txn_id(raw), TxnEpoch::new(1))
    }

    fn base_input() -> WriterRoutingTelemetryInput {
        let mut touch_surface = WriterTouchSurfaceTelemetry::default();
        touch_surface.write_set_pages.push(page(10));
        touch_surface.held_lock_pages.push(page(11));

        WriterRoutingTelemetryInput {
            session_id: Some(7),
            txn_token: txn_token(7),
            begin_seq: CommitSeq::new(100),
            planned_commit_seq: Some(CommitSeq::new(104)),
            touch_surface,
            conflict_history: WriterConflictHistoryTelemetry::default(),
            ownership_lineage: WriterOwnershipLineageTelemetry::default(),
        }
    }

    #[test]
    fn test_writer_routing_prefers_fresh_home_lane_hint_when_scores_are_close() {
        let input = base_input();
        let mut preferred = WriterRoutingLaneSnapshot {
            lane: Some(WriterRoutingLaneId::new(1)),
            node: Some(WriterRoutingNodeId::new(0)),
            recent_same_page_conflicts: 1,
            ..WriterRoutingLaneSnapshot::default()
        };
        preferred.home_pages.push(page(10));
        let alternate = WriterRoutingLaneSnapshot {
            lane: Some(WriterRoutingLaneId::new(2)),
            node: Some(WriterRoutingNodeId::new(1)),
            recent_same_page_conflicts: 1,
            ..WriterRoutingLaneSnapshot::default()
        };
        let hint = WriterHomeHint {
            home_lane: Some(WriterRoutingLaneId::new(1)),
            home_node: Some(WriterRoutingNodeId::new(0)),
            observed_commit_seq: CommitSeq::new(103),
        };

        let decision = decide_writer_routing_target(
            &input,
            &[preferred, alternate],
            Some(hint),
            WriterRoutingDecisionConfig::default(),
        )
        .expect("routing decision should succeed");

        assert_eq!(decision.selected_lane, WriterRoutingLaneId::new(1));
        assert_eq!(
            decision.reason,
            WriterRoutingDecisionReason::FreshHomeLaneHint
        );
        assert_eq!(
            decision.hint_disposition,
            WriterHomeHintDisposition::FreshLane
        );
        assert_eq!(
            decision.hint_degradation,
            WriterRoutingHintDegradation::None
        );
    }

    #[test]
    fn test_writer_routing_ignores_stale_hint_and_avoids_hot_conflict_lane() {
        let mut input = base_input();
        input.touch_surface.same_page_conflict_pages.push(page(42));

        let mut hinted_lane = WriterRoutingLaneSnapshot {
            lane: Some(WriterRoutingLaneId::new(1)),
            node: Some(WriterRoutingNodeId::new(0)),
            recent_same_page_conflicts: 6,
            ..WriterRoutingLaneSnapshot::default()
        };
        hinted_lane.conflict_pages.push(page(42));

        let cooler_lane = WriterRoutingLaneSnapshot {
            lane: Some(WriterRoutingLaneId::new(2)),
            node: Some(WriterRoutingNodeId::new(1)),
            ..WriterRoutingLaneSnapshot::default()
        };

        let hint = WriterHomeHint {
            home_lane: Some(WriterRoutingLaneId::new(1)),
            home_node: Some(WriterRoutingNodeId::new(0)),
            observed_commit_seq: CommitSeq::new(1),
        };

        let decision = decide_writer_routing_target(
            &input,
            &[hinted_lane, cooler_lane],
            Some(hint),
            WriterRoutingDecisionConfig::default(),
        )
        .expect("routing decision should succeed");

        assert_eq!(decision.selected_lane, WriterRoutingLaneId::new(2));
        assert_eq!(
            decision.reason,
            WriterRoutingDecisionReason::ConflictAvoidance
        );
        assert_eq!(
            decision.hint_disposition,
            WriterHomeHintDisposition::StaleCommitAge
        );
        assert_eq!(
            decision.hint_degradation,
            WriterRoutingHintDegradation::HintIgnoredAsStale
        );
    }

    #[test]
    fn test_writer_routing_reduces_missing_lane_hint_to_fresh_node_hint() {
        let input = base_input();
        let lane_a = WriterRoutingLaneSnapshot {
            lane: Some(WriterRoutingLaneId::new(2)),
            node: Some(WriterRoutingNodeId::new(9)),
            ..WriterRoutingLaneSnapshot::default()
        };
        let lane_b = WriterRoutingLaneSnapshot {
            lane: Some(WriterRoutingLaneId::new(3)),
            node: Some(WriterRoutingNodeId::new(9)),
            in_flight_writers: 1,
            ..WriterRoutingLaneSnapshot::default()
        };
        let lane_c = WriterRoutingLaneSnapshot {
            lane: Some(WriterRoutingLaneId::new(4)),
            node: Some(WriterRoutingNodeId::new(1)),
            ..WriterRoutingLaneSnapshot::default()
        };
        let hint = WriterHomeHint {
            home_lane: Some(WriterRoutingLaneId::new(99)),
            home_node: Some(WriterRoutingNodeId::new(9)),
            observed_commit_seq: CommitSeq::new(104),
        };

        let decision = decide_writer_routing_target(
            &input,
            &[lane_a, lane_b, lane_c],
            Some(hint),
            WriterRoutingDecisionConfig::default(),
        )
        .expect("routing decision should succeed");

        assert_eq!(decision.selected_node, Some(WriterRoutingNodeId::new(9)));
        assert_eq!(
            decision.reason,
            WriterRoutingDecisionReason::FreshHomeNodeHint
        );
        assert_eq!(
            decision.hint_disposition,
            WriterHomeHintDisposition::FreshLaneReducedToNode
        );
        assert_eq!(
            decision.hint_degradation,
            WriterRoutingHintDegradation::None
        );
    }

    #[test]
    fn test_writer_routing_overrides_fresh_hint_when_conflict_history_is_worse() {
        let mut input = base_input();
        input.touch_surface.same_page_conflict_pages.push(page(77));
        let mut hinted_lane = WriterRoutingLaneSnapshot {
            lane: Some(WriterRoutingLaneId::new(1)),
            node: Some(WriterRoutingNodeId::new(0)),
            recent_same_page_conflicts: 10,
            recent_busy_retries: 8,
            ..WriterRoutingLaneSnapshot::default()
        };
        hinted_lane.conflict_pages.push(page(77));

        let cooler_lane = WriterRoutingLaneSnapshot {
            lane: Some(WriterRoutingLaneId::new(2)),
            node: Some(WriterRoutingNodeId::new(1)),
            ..WriterRoutingLaneSnapshot::default()
        };
        let hint = WriterHomeHint {
            home_lane: Some(WriterRoutingLaneId::new(1)),
            home_node: Some(WriterRoutingNodeId::new(0)),
            observed_commit_seq: CommitSeq::new(104),
        };

        let decision = decide_writer_routing_target(
            &input,
            &[hinted_lane, cooler_lane],
            Some(hint),
            WriterRoutingDecisionConfig::default(),
        )
        .expect("routing decision should succeed");

        assert_eq!(decision.selected_lane, WriterRoutingLaneId::new(2));
        assert_eq!(
            decision.hint_degradation,
            WriterRoutingHintDegradation::HintOverriddenByConflictHistory
        );
    }

    #[test]
    fn test_writer_routing_uses_stable_hash_fallback_when_no_signals_exist() {
        let input = WriterRoutingTelemetryInput {
            session_id: Some(5),
            txn_token: txn_token(5),
            begin_seq: CommitSeq::new(10),
            planned_commit_seq: None,
            touch_surface: WriterTouchSurfaceTelemetry::default(),
            conflict_history: WriterConflictHistoryTelemetry::default(),
            ownership_lineage: WriterOwnershipLineageTelemetry::default(),
        };
        let lane_a = WriterRoutingLaneSnapshot {
            lane: Some(WriterRoutingLaneId::new(1)),
            ..WriterRoutingLaneSnapshot::default()
        };
        let lane_b = WriterRoutingLaneSnapshot {
            lane: Some(WriterRoutingLaneId::new(2)),
            ..WriterRoutingLaneSnapshot::default()
        };
        let lane_c = WriterRoutingLaneSnapshot {
            lane: Some(WriterRoutingLaneId::new(3)),
            ..WriterRoutingLaneSnapshot::default()
        };

        let decision = decide_writer_routing_target(
            &input,
            &[lane_a, lane_b, lane_c],
            None,
            WriterRoutingDecisionConfig::default(),
        )
        .expect("routing decision should succeed");

        assert_eq!(decision.selected_lane, WriterRoutingLaneId::new(3));
        assert_eq!(
            decision.reason,
            WriterRoutingDecisionReason::StableHashFallback
        );
        assert_eq!(
            decision.hint_disposition,
            WriterHomeHintDisposition::Missing
        );
    }

    #[test]
    fn test_writer_routing_reports_empty_candidate_error() {
        let error = decide_writer_routing_target(
            &base_input(),
            &[],
            None,
            WriterRoutingDecisionConfig::default(),
        )
        .expect_err("empty candidate list should error");
        assert_eq!(error, WriterRoutingDecisionError::NoCandidateLanes);
    }

    #[test]
    fn test_writer_routing_stable_hash_fallback_ignores_unavailable_candidates() {
        let input = WriterRoutingTelemetryInput {
            session_id: Some(3),
            txn_token: txn_token(3),
            begin_seq: CommitSeq::new(10),
            planned_commit_seq: None,
            touch_surface: WriterTouchSurfaceTelemetry::default(),
            conflict_history: WriterConflictHistoryTelemetry::default(),
            ownership_lineage: WriterOwnershipLineageTelemetry::default(),
        };
        let unavailable = WriterRoutingLaneSnapshot::default();
        let lane_a = WriterRoutingLaneSnapshot {
            lane: Some(WriterRoutingLaneId::new(1)),
            ..WriterRoutingLaneSnapshot::default()
        };
        let lane_b = WriterRoutingLaneSnapshot {
            lane: Some(WriterRoutingLaneId::new(2)),
            ..WriterRoutingLaneSnapshot::default()
        };

        let decision = decide_writer_routing_target(
            &input,
            &[unavailable, lane_a, lane_b],
            None,
            WriterRoutingDecisionConfig::default(),
        )
        .expect("routing decision should succeed");

        assert_eq!(decision.selected_lane, WriterRoutingLaneId::new(2));
        assert_eq!(
            decision.reason,
            WriterRoutingDecisionReason::StableHashFallback
        );
    }

    fn synthetic_config(
        scenario_id: &str,
        workload: WriterRoutingSyntheticWorkload,
        placement_profile: WriterRoutingPlacementProfile,
    ) -> WriterRoutingSyntheticConfig {
        WriterRoutingSyntheticConfig {
            scenario_id: scenario_id.to_owned(),
            workload,
            placement_profile,
            lane_count: 4,
            iterations: 12,
            writers_per_iteration: 4,
        }
    }

    #[test]
    fn test_writer_routing_synthetic_disjoint_workload_stays_conflict_free() {
        let comparison = compare_writer_routing_synthetic_workload(
            &synthetic_config(
                "writer_routing.synthetic.disjoint",
                WriterRoutingSyntheticWorkload::DisjointPages,
                WriterRoutingPlacementProfile::HomeLanePinned,
            ),
            WriterRoutingDecisionConfig::default(),
        );

        assert_eq!(comparison.baseline.conflicts_total, 0);
        assert_eq!(comparison.baseline.retries_total, 0);
        assert_eq!(comparison.routed.conflicts_total, 0);
        assert_eq!(comparison.routed.retries_total, 0);
        assert_eq!(comparison.baseline.publication_retry_total, 0);
        assert_eq!(comparison.routed.publication_retry_total, 0);
        assert!(comparison.routed.p99_latency_ns() <= comparison.baseline.p99_latency_ns());
    }

    #[test]
    fn test_writer_routing_synthetic_overlapping_workload_reduces_conflicts_vs_baseline() {
        let comparison = compare_writer_routing_synthetic_workload(
            &synthetic_config(
                "writer_routing.synthetic.overlap",
                WriterRoutingSyntheticWorkload::OverlappingPages,
                WriterRoutingPlacementProfile::HomeLanePinned,
            ),
            WriterRoutingDecisionConfig::default(),
        );

        assert!(comparison.routed.conflicts_total < comparison.baseline.conflicts_total);
        assert!(comparison.routed.retries_total < comparison.baseline.retries_total);
        assert!(
            comparison.routed.remote_ownership_events < comparison.baseline.remote_ownership_events
        );
        assert!(
            comparison.routed.publication_retry_total < comparison.baseline.publication_retry_total
        );
        assert!(comparison.conflict_rate_delta() < 0.0);
        assert!(comparison.retry_rate_delta() < 0.0);
    }

    #[test]
    fn test_writer_routing_synthetic_hot_page_workload_reduces_publication_handoffs() {
        let comparison = compare_writer_routing_synthetic_workload(
            &synthetic_config(
                "writer_routing.synthetic.hot_page",
                WriterRoutingSyntheticWorkload::HotPageContention,
                WriterRoutingPlacementProfile::HomeNodePinned,
            ),
            WriterRoutingDecisionConfig::default(),
        );

        assert!(comparison.routed.conflicts_total < comparison.baseline.conflicts_total);
        assert!(comparison.routed.retries_total < comparison.baseline.retries_total);
        assert!(
            comparison.routed.publication_retry_total < comparison.baseline.publication_retry_total
        );
        assert!(
            comparison.routed.visibility_handoff_nanos_total
                < comparison.baseline.visibility_handoff_nanos_total
        );
        assert!(
            comparison.routed.stale_snapshot_rejects_total
                < comparison.baseline.stale_snapshot_rejects_total
        );
        assert!(comparison.publication_retry_rate_delta() < 0.0);
    }
}
