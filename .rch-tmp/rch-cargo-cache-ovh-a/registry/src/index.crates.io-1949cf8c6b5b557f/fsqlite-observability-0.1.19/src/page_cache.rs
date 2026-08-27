use serde::Serialize;

const PERCENT_SCALE: u64 = 100;
const HIGH_OCCUPANCY_PERCENT: u64 = 90;
const HIGH_EVICTION_PRESSURE_PERCENT: u64 = 25;
const HIGH_MVCC_OVERHEAD_PERCENT: u64 = 25;
const HIGH_DIRTY_RATIO_PERCENT: u64 = 75;

/// Coarse health bucket for page-cache efficiency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PageCacheEfficiencyLevel {
    Cold,
    Mixed,
    Warm,
}

/// Operator-facing page-cache pressure bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PageCachePressureLevel {
    Idle,
    Moderate,
    High,
}

/// Serializable derived diagnostics for page-cache tuning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PageCacheEfficiencyAssessment {
    pub total_accesses: u64,
    pub hit_rate_pct: u64,
    pub miss_rate_pct: u64,
    pub occupancy_pct: u64,
    pub eviction_pressure_pct: u64,
    pub mvcc_overhead_pct: u64,
    pub resident_queue_pages: usize,
    pub ghost_queue_pages: usize,
    pub efficiency_level: PageCacheEfficiencyLevel,
    pub pressure_level: PageCachePressureLevel,
    pub dirty_backlog: bool,
    pub mvcc_version_pressure: bool,
}

/// Serializable summary of page-cache efficiency counters and compatibility
/// gauges exported by the pager layer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct PageCacheEfficiencySnapshot {
    pub hits: u64,
    pub misses: u64,
    pub admits: u64,
    pub evictions: u64,
    pub cached_pages: usize,
    pub pool_capacity: usize,
    pub dirty_ratio_pct: u64,
    pub t1_size: usize,
    pub t2_size: usize,
    pub b1_size: usize,
    pub b2_size: usize,
    pub p_target: usize,
    pub mvcc_multi_version_pages: usize,
}

impl PageCacheEfficiencySnapshot {
    fn rounded_percent(numerator: u64, denominator: u64) -> u64 {
        if denominator == 0 {
            return 0;
        }
        let scaled = u128::from(numerator)
            .saturating_mul(u128::from(PERCENT_SCALE))
            .saturating_add(u128::from(denominator / 2))
            / u128::from(denominator);
        scaled.min(u128::from(u64::MAX)) as u64
    }

    /// Total cache probes (`hits + misses`).
    #[must_use]
    pub fn total_accesses(self) -> u64 {
        self.hits.saturating_add(self.misses)
    }

    /// Hit-rate as a percentage in `[0.0, 100.0]`.
    #[must_use]
    pub fn hit_rate_percent(self) -> f64 {
        let total = self.total_accesses();
        if total == 0 {
            0.0
        } else {
            (self.hits as f64 * 100.0) / total as f64
        }
    }

    /// Miss-rate as a percentage in `[0.0, 100.0]`.
    #[must_use]
    pub fn miss_rate_percent(self) -> f64 {
        let total = self.total_accesses();
        if total == 0 {
            0.0
        } else {
            (self.misses as f64 * 100.0) / total as f64
        }
    }

    /// Resident-page occupancy as a percentage of configured pool capacity.
    #[must_use]
    pub fn occupancy_percent(self) -> f64 {
        if self.pool_capacity == 0 {
            0.0
        } else {
            (self.cached_pages as f64 * 100.0) / self.pool_capacity as f64
        }
    }

    /// Rounded hit-rate percentage for PRAGMA/JSON surfaces.
    #[must_use]
    pub fn hit_rate_percent_rounded(self) -> u64 {
        Self::rounded_percent(self.hits, self.total_accesses())
    }

    /// Rounded miss-rate percentage for PRAGMA/JSON surfaces.
    #[must_use]
    pub fn miss_rate_percent_rounded(self) -> u64 {
        Self::rounded_percent(self.misses, self.total_accesses())
    }

    /// Rounded resident occupancy percentage.
    #[must_use]
    pub fn occupancy_percent_rounded(self) -> u64 {
        Self::rounded_percent(self.cached_pages as u64, self.pool_capacity as u64)
    }

    /// Evictions as a percentage of fresh cache admissions.
    #[must_use]
    pub fn eviction_pressure_percent(self) -> u64 {
        Self::rounded_percent(self.evictions, self.admits)
    }

    /// Cached pages with more than one MVCC version as a percentage of residents.
    #[must_use]
    pub fn mvcc_overhead_percent(self) -> u64 {
        Self::rounded_percent(
            self.mvcc_multi_version_pages as u64,
            self.cached_pages as u64,
        )
    }

    /// Total resident ARC-compatible queue pages.
    #[must_use]
    pub fn resident_queue_pages(self) -> usize {
        self.t1_size.saturating_add(self.t2_size)
    }

    /// Total ARC-compatible ghost queue pages.
    #[must_use]
    pub fn ghost_queue_pages(self) -> usize {
        self.b1_size.saturating_add(self.b2_size)
    }

    /// Coarse cache efficiency bucket derived from the rounded hit rate.
    #[must_use]
    pub fn efficiency_level(self) -> PageCacheEfficiencyLevel {
        match self.hit_rate_percent_rounded() {
            0..=39 => PageCacheEfficiencyLevel::Cold,
            40..=79 => PageCacheEfficiencyLevel::Mixed,
            _ => PageCacheEfficiencyLevel::Warm,
        }
    }

    /// Coarse pressure bucket derived only from observer counters.
    #[must_use]
    pub fn pressure_level(self) -> PageCachePressureLevel {
        if self.occupancy_percent_rounded() >= HIGH_OCCUPANCY_PERCENT
            && self.eviction_pressure_percent() >= HIGH_EVICTION_PRESSURE_PERCENT
        {
            PageCachePressureLevel::High
        } else if self.evictions > 0 || self.cached_pages > 0 {
            PageCachePressureLevel::Moderate
        } else {
            PageCachePressureLevel::Idle
        }
    }

    /// Read-only derived assessment for operator-facing diagnostics.
    #[must_use]
    pub fn assessment(self) -> PageCacheEfficiencyAssessment {
        let mvcc_overhead_pct = self.mvcc_overhead_percent();
        PageCacheEfficiencyAssessment {
            total_accesses: self.total_accesses(),
            hit_rate_pct: self.hit_rate_percent_rounded(),
            miss_rate_pct: self.miss_rate_percent_rounded(),
            occupancy_pct: self.occupancy_percent_rounded(),
            eviction_pressure_pct: self.eviction_pressure_percent(),
            mvcc_overhead_pct,
            resident_queue_pages: self.resident_queue_pages(),
            ghost_queue_pages: self.ghost_queue_pages(),
            efficiency_level: self.efficiency_level(),
            pressure_level: self.pressure_level(),
            dirty_backlog: self.dirty_ratio_pct >= HIGH_DIRTY_RATIO_PERCENT,
            mvcc_version_pressure: mvcc_overhead_pct >= HIGH_MVCC_OVERHEAD_PERCENT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PageCacheEfficiencyLevel, PageCacheEfficiencySnapshot, PageCachePressureLevel};

    #[test]
    fn test_page_cache_efficiency_rates_are_zero_safe() {
        let snapshot = PageCacheEfficiencySnapshot::default();
        assert_eq!(snapshot.total_accesses(), 0);
        assert_eq!(snapshot.hit_rate_percent(), 0.0);
        assert_eq!(snapshot.miss_rate_percent(), 0.0);
        assert_eq!(snapshot.occupancy_percent(), 0.0);
        assert_eq!(snapshot.hit_rate_percent_rounded(), 0);
        assert_eq!(snapshot.miss_rate_percent_rounded(), 0);
        assert_eq!(snapshot.occupancy_percent_rounded(), 0);
        assert_eq!(snapshot.eviction_pressure_percent(), 0);
        assert_eq!(snapshot.mvcc_overhead_percent(), 0);
        assert_eq!(
            snapshot.assessment().pressure_level,
            PageCachePressureLevel::Idle
        );
    }

    #[test]
    fn test_page_cache_efficiency_rates_track_raw_counters() {
        let snapshot = PageCacheEfficiencySnapshot {
            hits: 9,
            misses: 3,
            admits: 4,
            evictions: 1,
            cached_pages: 12,
            pool_capacity: 48,
            dirty_ratio_pct: 25,
            t1_size: 7,
            t2_size: 5,
            b1_size: 2,
            b2_size: 1,
            p_target: 6,
            mvcc_multi_version_pages: 0,
        };

        assert_eq!(snapshot.total_accesses(), 12);
        assert!((snapshot.hit_rate_percent() - 75.0).abs() < f64::EPSILON);
        assert!((snapshot.miss_rate_percent() - 25.0).abs() < f64::EPSILON);
        assert!((snapshot.occupancy_percent() - 25.0).abs() < f64::EPSILON);
        assert_eq!(snapshot.hit_rate_percent_rounded(), 75);
        assert_eq!(snapshot.miss_rate_percent_rounded(), 25);
        assert_eq!(snapshot.occupancy_percent_rounded(), 25);
        assert_eq!(snapshot.eviction_pressure_percent(), 25);
        assert_eq!(snapshot.resident_queue_pages(), 12);
        assert_eq!(snapshot.ghost_queue_pages(), 3);
        assert_eq!(snapshot.efficiency_level(), PageCacheEfficiencyLevel::Mixed);
    }

    #[test]
    fn test_page_cache_assessment_flags_pressure_without_mutating_snapshot() {
        let snapshot = PageCacheEfficiencySnapshot {
            hits: 950,
            misses: 50,
            admits: 40,
            evictions: 20,
            cached_pages: 95,
            pool_capacity: 100,
            dirty_ratio_pct: 80,
            t1_size: 25,
            t2_size: 70,
            b1_size: 8,
            b2_size: 6,
            p_target: 64,
            mvcc_multi_version_pages: 25,
        };

        let assessment = snapshot.assessment();
        assert_eq!(assessment.total_accesses, 1000);
        assert_eq!(assessment.hit_rate_pct, 95);
        assert_eq!(assessment.miss_rate_pct, 5);
        assert_eq!(assessment.occupancy_pct, 95);
        assert_eq!(assessment.eviction_pressure_pct, 50);
        assert_eq!(assessment.mvcc_overhead_pct, 26);
        assert_eq!(assessment.resident_queue_pages, 95);
        assert_eq!(assessment.ghost_queue_pages, 14);
        assert_eq!(assessment.efficiency_level, PageCacheEfficiencyLevel::Warm);
        assert_eq!(assessment.pressure_level, PageCachePressureLevel::High);
        assert!(assessment.dirty_backlog);
        assert!(assessment.mvcc_version_pressure);
        assert_eq!(snapshot.cached_pages, 95);
    }

    #[test]
    fn test_page_cache_assessment_uses_saturating_totals() {
        let snapshot = PageCacheEfficiencySnapshot {
            hits: u64::MAX,
            misses: 1,
            admits: 0,
            evictions: u64::MAX,
            cached_pages: usize::MAX,
            pool_capacity: usize::MAX,
            dirty_ratio_pct: 0,
            t1_size: usize::MAX,
            t2_size: 1,
            b1_size: usize::MAX,
            b2_size: 1,
            p_target: 0,
            mvcc_multi_version_pages: usize::MAX,
        };

        assert_eq!(snapshot.total_accesses(), u64::MAX);
        assert_eq!(snapshot.eviction_pressure_percent(), 0);
        assert_eq!(snapshot.resident_queue_pages(), usize::MAX);
        assert_eq!(snapshot.ghost_queue_pages(), usize::MAX);
        assert_eq!(snapshot.mvcc_overhead_percent(), 100);
    }
}
