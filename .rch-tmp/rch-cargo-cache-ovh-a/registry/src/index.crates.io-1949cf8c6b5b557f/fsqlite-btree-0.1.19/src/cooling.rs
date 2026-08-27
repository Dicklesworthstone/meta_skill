//! Page cooling/heating state machine with eviction integration (§15.3).
//!
//! Implements the HOT/COOLING/COLD protocol from LeanStore (Leis et al. 2018):
//!
//! - **HOT**: Swizzled, actively accessed. Cannot be evicted.
//! - **COOLING**: Swizzled, not recently accessed. Candidate for eviction.
//! - **COLD**: Unswizzled, evicted or never loaded.
//!
//! Transitions:
//! - COLD → HOT: page loaded via `fix_page`, swizzled into parent.
//! - HOT → COOLING: background cooling scan detects low access frequency.
//! - COOLING → HOT: page re-accessed while in COOLING state (re-heated).
//! - COOLING → COLD: page evicted, unswizzled from parent.
//!
//! Root pages are pinned in HOT state and never cooled or evicted.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::swizzle::PageTemperature;

// ── Metrics ──────────────────────────────────────────────────────────────

static COOLING_SCANS_TOTAL: AtomicU64 = AtomicU64::new(0);
static PAGES_COOLED_TOTAL: AtomicU64 = AtomicU64::new(0);
static PAGES_REHEATED_TOTAL: AtomicU64 = AtomicU64::new(0);
static PAGES_EVICTED_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Snapshot of cooling state machine metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CoolingMetricsSnapshot {
    /// Total number of cooling scans performed.
    pub cooling_scans_total: u64,
    /// Total pages transitioned from HOT → COOLING.
    pub pages_cooled_total: u64,
    /// Total pages transitioned from COOLING → HOT (re-heated on access).
    pub pages_reheated_total: u64,
    /// Total pages transitioned from COOLING → COLD (evicted).
    pub pages_evicted_total: u64,
}

impl fmt::Display for CoolingMetricsSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "cooling_scans={} pages_cooled={} pages_reheated={} pages_evicted={}",
            self.cooling_scans_total,
            self.pages_cooled_total,
            self.pages_reheated_total,
            self.pages_evicted_total,
        )
    }
}

/// Return a snapshot of cooling metrics.
#[must_use]
pub fn cooling_metrics_snapshot() -> CoolingMetricsSnapshot {
    CoolingMetricsSnapshot {
        cooling_scans_total: COOLING_SCANS_TOTAL.load(Ordering::Relaxed),
        pages_cooled_total: PAGES_COOLED_TOTAL.load(Ordering::Relaxed),
        pages_reheated_total: PAGES_REHEATED_TOTAL.load(Ordering::Relaxed),
        pages_evicted_total: PAGES_EVICTED_TOTAL.load(Ordering::Relaxed),
    }
}

/// Reset cooling metrics.
pub fn reset_cooling_metrics() {
    COOLING_SCANS_TOTAL.store(0, Ordering::Relaxed);
    PAGES_COOLED_TOTAL.store(0, Ordering::Relaxed);
    PAGES_REHEATED_TOTAL.store(0, Ordering::Relaxed);
    PAGES_EVICTED_TOTAL.store(0, Ordering::Relaxed);
}

// ── Configuration ────────────────────────────────────────────────────────

/// Configuration for the cooling state machine.
#[derive(Debug, Clone, Copy)]
pub struct CoolingConfig {
    /// Access count threshold: pages with fewer accesses since last scan
    /// are transitioned from HOT → COOLING.
    pub cooling_threshold: u32,
}

impl Default for CoolingConfig {
    fn default() -> Self {
        Self {
            cooling_threshold: 2,
        }
    }
}

// ── Per-Page State ───────────────────────────────────────────────────────

/// Internal tracking state for a single page.
#[derive(Debug, Clone)]
struct PageState {
    /// Current temperature.
    temperature: PageTemperature,
    /// Access count since the last cooling scan.
    access_count: u32,
    /// Frame address if swizzled (Hot or Cooling), 0 if Cold.
    frame_addr: u64,
}

// ── Cooling State Machine ────────────────────────────────────────────────

/// The page cooling/heating state machine.
///
/// Tracks per-page thermal state, access counters, and root pinning.
/// Provides a `run_cooling_scan()` method that transitions infrequently
/// accessed pages from HOT → COOLING.
pub struct CoolingStateMachine {
    config: CoolingConfig,
    /// Per-page state tracking.
    pages: Mutex<HashMap<u64, PageState>>,
    /// Root pages that are permanently pinned in HOT state.
    pinned_roots: Mutex<HashSet<u64>>,
}

impl CoolingStateMachine {
    /// Create a new cooling state machine with the given configuration.
    pub fn new(config: CoolingConfig) -> Self {
        Self {
            config,
            pages: Mutex::new(HashMap::new()),
            pinned_roots: Mutex::new(HashSet::new()),
        }
    }

    /// Register a page as tracked. Starts in COLD state.
    pub fn register_page(&self, page_id: u64) {
        let mut pages = self.pages.lock().unwrap_or_else(|e| e.into_inner());
        pages.entry(page_id).or_insert(PageState {
            temperature: PageTemperature::Cold,
            access_count: 0,
            frame_addr: 0,
        });
    }

    /// Pin a page as a root page (permanently HOT, never cooled or evicted).
    pub fn pin_root(&self, page_id: u64) {
        self.register_page(page_id);
        self.pinned_roots
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(page_id);
    }

    /// Check whether a page is a pinned root.
    #[must_use]
    pub fn is_pinned(&self, page_id: u64) -> bool {
        self.pinned_roots
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&page_id)
    }

    /// Load a page (COLD → HOT transition). Sets frame address.
    ///
    /// Returns `true` if the transition was performed, `false` if the page
    /// was already Hot or Cooling (in which case it's re-heated if Cooling).
    #[allow(clippy::significant_drop_tightening)]
    pub fn load_page(&self, page_id: u64, frame_addr: u64) -> bool {
        let mut pages = self.pages.lock().unwrap_or_else(|e| e.into_inner());
        let entry = pages.entry(page_id).or_insert(PageState {
            temperature: PageTemperature::Cold,
            access_count: 0,
            frame_addr: 0,
        });

        match entry.temperature {
            PageTemperature::Cold => {
                entry.temperature = PageTemperature::Hot;
                entry.frame_addr = frame_addr;
                entry.access_count = 1;
                true
            }
            PageTemperature::Cooling => {
                // Re-heat on load.
                entry.temperature = PageTemperature::Hot;
                entry.frame_addr = frame_addr;
                entry.access_count = 1;
                PAGES_REHEATED_TOTAL.fetch_add(1, Ordering::Relaxed);
                false
            }
            PageTemperature::Hot => {
                entry.access_count += 1;
                false
            }
        }
    }

    /// Record an access to a page. If COOLING, re-heats to HOT.
    pub fn access_page(&self, page_id: u64) {
        let mut pages = self.pages.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = pages.get_mut(&page_id) {
            entry.access_count += 1;
            if entry.temperature == PageTemperature::Cooling {
                entry.temperature = PageTemperature::Hot;
                PAGES_REHEATED_TOTAL.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Evict a page (COOLING → COLD transition). Clears frame address.
    ///
    /// Returns `Ok(())` if evicted, `Err(reason)` if the page cannot be
    /// evicted (e.g., it's HOT, pinned, or already COLD).
    pub fn evict_page(&self, page_id: u64) -> Result<(), &'static str> {
        if self.is_pinned(page_id) {
            return Err("page is a pinned root");
        }

        let mut pages = self.pages.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = pages.get_mut(&page_id) {
            match entry.temperature {
                PageTemperature::Cooling => {
                    entry.temperature = PageTemperature::Cold;
                    entry.frame_addr = 0;
                    entry.access_count = 0;
                    PAGES_EVICTED_TOTAL.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                }
                PageTemperature::Hot => Err("page is HOT; must cool first"),
                PageTemperature::Cold => Err("page is already COLD"),
            }
        } else {
            Err("page not registered")
        }
    }

    /// Run a cooling scan: transition HOT pages with low access frequency
    /// to COOLING. Pinned root pages are skipped.
    ///
    /// Returns `CoolingScanResult` with summary statistics.
    pub fn run_cooling_scan(&self) -> CoolingScanResult {
        COOLING_SCANS_TOTAL.fetch_add(1, Ordering::Relaxed);

        let mut pages = self.pages.lock().unwrap_or_else(|e| e.into_inner());
        let pinned = self.pinned_roots.lock().unwrap_or_else(|e| e.into_inner());

        let mut scanned = 0u32;
        let mut cooled = 0u32;

        for (pid, entry) in pages.iter_mut() {
            scanned += 1;

            // Skip pinned roots.
            if pinned.contains(pid) {
                entry.access_count = 0;
                continue;
            }

            if entry.temperature == PageTemperature::Hot
                && entry.access_count < self.config.cooling_threshold
            {
                entry.temperature = PageTemperature::Cooling;
                cooled += 1;
                PAGES_COOLED_TOTAL.fetch_add(1, Ordering::Relaxed);
            }

            // Reset access counter for the next scan interval.
            entry.access_count = 0;
        }
        drop(pages);

        CoolingScanResult {
            pages_scanned: scanned,
            pages_cooled: cooled,
        }
    }

    /// Return the current temperature of a page.
    #[must_use]
    pub fn temperature(&self, page_id: u64) -> Option<PageTemperature> {
        self.pages
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&page_id)
            .map(|e| e.temperature)
    }

    /// Return the frame address of a page, if it's Hot or Cooling.
    #[must_use]
    pub fn frame_addr(&self, page_id: u64) -> Option<u64> {
        self.pages
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&page_id)
            .and_then(|e| {
                if e.temperature == PageTemperature::Cold {
                    None
                } else {
                    Some(e.frame_addr)
                }
            })
    }

    /// Count pages in each thermal state.
    #[must_use]
    pub fn temperature_counts(&self) -> TemperatureCounts {
        let pages = self.pages.lock().unwrap_or_else(|e| e.into_inner());
        let mut counts = TemperatureCounts::default();
        for entry in pages.values() {
            match entry.temperature {
                PageTemperature::Hot => counts.hot += 1,
                PageTemperature::Cooling => counts.cooling += 1,
                PageTemperature::Cold => counts.cold += 1,
            }
        }
        drop(pages);
        counts
    }

    /// Return the total number of tracked pages.
    #[must_use]
    pub fn tracked_count(&self) -> usize {
        self.pages.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Return the number of pinned root pages.
    #[must_use]
    pub fn pinned_count(&self) -> usize {
        self.pinned_roots
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
    }
}

#[allow(clippy::missing_fields_in_debug)]
impl fmt::Debug for CoolingStateMachine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let counts = self.temperature_counts();
        f.debug_struct("CoolingStateMachine")
            .field("config", &self.config)
            .field("hot", &counts.hot)
            .field("cooling", &counts.cooling)
            .field("cold", &counts.cold)
            .field("pinned", &self.pinned_count())
            .finish()
    }
}

// ── Result Types ─────────────────────────────────────────────────────────

/// Summary of a cooling scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoolingScanResult {
    /// Number of pages scanned.
    pub pages_scanned: u32,
    /// Number of pages transitioned HOT → COOLING.
    pub pages_cooled: u32,
}

/// Count of pages in each thermal state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TemperatureCounts {
    pub hot: usize,
    pub cooling: usize,
    pub cold: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_lifecycle() {
        let csm = CoolingStateMachine::new(CoolingConfig::default());
        csm.register_page(1);

        assert_eq!(csm.temperature(1), Some(PageTemperature::Cold));

        // Load page (Cold → Hot).
        csm.load_page(1, 0x1000);
        assert_eq!(csm.temperature(1), Some(PageTemperature::Hot));

        // Cooling scan without access → Hot → Cooling.
        let result = csm.run_cooling_scan();
        assert_eq!(result.pages_cooled, 1);
        assert_eq!(csm.temperature(1), Some(PageTemperature::Cooling));

        // Evict (Cooling → Cold).
        csm.evict_page(1).expect("evict should succeed");
        assert_eq!(csm.temperature(1), Some(PageTemperature::Cold));
    }

    #[test]
    fn load_page_returns_true_only_on_first_cold_to_hot_transition() {
        // load_page is documented to return true only when it performs the
        // COLD -> HOT transition, and false when the page is already HOT or is
        // re-heated from COOLING. No test pinned this return value.
        let csm = CoolingStateMachine::new(CoolingConfig::default());
        csm.register_page(1);

        // First load performs COLD -> HOT and reports the transition.
        assert!(csm.load_page(1, 0x1000));
        assert_eq!(csm.temperature(1), Some(PageTemperature::Hot));

        // Cool the page (an idle HOT page cools under the default config).
        csm.run_cooling_scan();
        assert_eq!(csm.temperature(1), Some(PageTemperature::Cooling));

        // Loading a COOLING page re-heats it to HOT but returns false: there was
        // no fresh COLD -> HOT transition.
        assert!(!csm.load_page(1, 0x2000));
        assert_eq!(csm.temperature(1), Some(PageTemperature::Hot));

        // Loading an already-HOT page also returns false.
        assert!(!csm.load_page(1, 0x3000));
    }

    #[test]
    fn re_heat_on_access() {
        let csm = CoolingStateMachine::new(CoolingConfig {
            cooling_threshold: 2,
        });
        csm.register_page(1);
        csm.load_page(1, 0x1000);

        // Cool the page.
        csm.run_cooling_scan();
        assert_eq!(csm.temperature(1), Some(PageTemperature::Cooling));

        // Access → re-heat.
        csm.access_page(1);
        assert_eq!(csm.temperature(1), Some(PageTemperature::Hot));
    }

    #[test]
    fn access_page_is_noop_for_unregistered_and_does_not_reheat_cold() {
        // re_heat_on_access pins the COOLING -> HOT re-heat; this pins the other
        // two access_page behaviors: an unregistered page is a safe no-op (not
        // implicitly registered), and a COLD page is not re-heated (only COOLING
        // re-heats).
        let csm = CoolingStateMachine::new(CoolingConfig::default());

        // Accessing an unregistered page does not register it; tracked count
        // stays zero and the page has no temperature.
        csm.access_page(999);
        assert_eq!(csm.tracked_count(), 0);
        assert_eq!(csm.temperature(999), None);

        // A registered-but-COLD page is not re-heated by an access: only a
        // COOLING page re-heats, so it stays COLD.
        csm.register_page(1);
        assert_eq!(csm.temperature(1), Some(PageTemperature::Cold));
        csm.access_page(1);
        assert_eq!(csm.temperature(1), Some(PageTemperature::Cold));
    }

    #[test]
    fn pinned_root_never_cools() {
        let csm = CoolingStateMachine::new(CoolingConfig::default());
        csm.pin_root(1);
        csm.load_page(1, 0x1000);

        // Multiple cooling scans without access.
        for _ in 0..10 {
            csm.run_cooling_scan();
        }

        // Root page should still be HOT.
        assert_eq!(csm.temperature(1), Some(PageTemperature::Hot));
        assert!(csm.is_pinned(1));
    }

    #[test]
    fn cannot_evict_hot_page() {
        let csm = CoolingStateMachine::new(CoolingConfig::default());
        csm.register_page(1);
        csm.load_page(1, 0x1000);

        let err = csm.evict_page(1).unwrap_err();
        assert_eq!(err, "page is HOT; must cool first");
    }

    #[test]
    fn cannot_evict_pinned_root() {
        let csm = CoolingStateMachine::new(CoolingConfig::default());
        csm.pin_root(1);
        csm.load_page(1, 0x1000);
        csm.run_cooling_scan(); // Would cool non-pinned pages.

        let err = csm.evict_page(1).unwrap_err();
        assert_eq!(err, "page is a pinned root");
    }

    #[test]
    fn evict_page_distinguishes_unregistered_from_already_cold() {
        // cannot_evict_hot_page / cannot_evict_pinned_root pin the HOT and
        // pinned rejections; the COLD and unregistered rejections (distinct
        // messages) were not pinned by message.
        let csm = CoolingStateMachine::new(CoolingConfig::default());

        // A page that was never registered.
        assert_eq!(csm.evict_page(99).unwrap_err(), "page not registered");

        // A freshly-registered page is COLD; evicting it reports the distinct
        // already-cold reason rather than "page not registered".
        csm.register_page(1);
        assert_eq!(csm.temperature(1), Some(PageTemperature::Cold));
        assert_eq!(csm.evict_page(1).unwrap_err(), "page is already COLD");

        // After a full load -> cool -> evict cycle the page is COLD again, so a
        // second evict reports already-cold instead of succeeding twice.
        csm.load_page(1, 0x1000);
        csm.run_cooling_scan();
        csm.evict_page(1).expect("a cooling page should evict");
        assert_eq!(csm.temperature(1), Some(PageTemperature::Cold));
        assert_eq!(csm.evict_page(1).unwrap_err(), "page is already COLD");
    }

    #[test]
    fn run_cooling_scan_keeps_frequently_accessed_hot_pages_hot() {
        // A HOT page is cooled only when access_count < cooling_threshold. A
        // page accessed up to the threshold within the interval must stay HOT
        // (the anti-thrash property) and not be counted as cooled. The existing
        // scan tests only cool idle pages; the >= threshold branch was untested.
        let csm = CoolingStateMachine::new(CoolingConfig {
            cooling_threshold: 3,
        });
        csm.register_page(1);
        csm.load_page(1, 0x1000); // COLD -> HOT, access_count = 1
        csm.access_page(1); // access_count = 2
        csm.access_page(1); // access_count = 3 (== threshold)

        let result = csm.run_cooling_scan();
        // access_count (3) is NOT < threshold (3), so the page stays HOT.
        assert_eq!(csm.temperature(1), Some(PageTemperature::Hot));
        assert_eq!(result.pages_cooled, 0);
        assert_eq!(result.pages_scanned, 1);

        // The scan reset the access counter, so the next (idle) scan cools it.
        let result2 = csm.run_cooling_scan();
        assert_eq!(csm.temperature(1), Some(PageTemperature::Cooling));
        assert_eq!(result2.pages_cooled, 1);
    }

    #[test]
    fn cooling_tracker_counts_frame_addr_and_multi_page_scan() {
        let csm = CoolingStateMachine::new(CoolingConfig::default());

        // Two ordinary loaded pages (-> Hot) plus one pinned root (-> Hot).
        csm.register_page(1);
        csm.load_page(1, 0x1000);
        csm.register_page(2);
        csm.load_page(2, 0x2000);
        csm.pin_root(3);
        csm.load_page(3, 0x3000);

        assert_eq!(csm.tracked_count(), 3);
        assert_eq!(csm.pinned_count(), 1);
        assert!(csm.is_pinned(3) && !csm.is_pinned(1));

        // frame_addr reports the loaded address; an unregistered page has none.
        assert_eq!(csm.frame_addr(1), Some(0x1000));
        assert_eq!(csm.frame_addr(2), Some(0x2000));
        assert!(
            csm.temperature(999).is_none(),
            "unregistered page has no temperature"
        );

        // One scan cools both unpinned pages; the pinned root stays Hot.
        let result = csm.run_cooling_scan();
        assert_eq!(
            result.pages_cooled, 2,
            "two unpinned pages cool; the pinned root does not"
        );
        assert_eq!(csm.temperature(1), Some(PageTemperature::Cooling));
        assert_eq!(csm.temperature(2), Some(PageTemperature::Cooling));
        assert_eq!(csm.temperature(3), Some(PageTemperature::Hot));

        // A cooled page is now evictable; the pinned root is not.
        assert!(csm.evict_page(1).is_ok());
        assert!(csm.evict_page(3).is_err());
    }

    #[test]
    fn frame_addr_is_none_for_cold_and_unregistered_pages() {
        // frame_addr reports the address only for Hot/Cooling pages. The
        // cooling_tracker test only checks the Hot case; this pins the None
        // branches and that eviction clears the frame address.
        let csm = CoolingStateMachine::new(CoolingConfig::default());

        // Unregistered page: no frame address.
        assert_eq!(csm.frame_addr(99), None);

        // Freshly registered page is COLD -> no frame address yet.
        csm.register_page(1);
        assert_eq!(csm.frame_addr(1), None);

        // Loaded page is HOT -> reports its frame address.
        csm.load_page(1, 0x1000);
        assert_eq!(csm.frame_addr(1), Some(0x1000));

        // Still reported while COOLING (the frame stays resident).
        csm.run_cooling_scan();
        assert_eq!(csm.temperature(1), Some(PageTemperature::Cooling));
        assert_eq!(csm.frame_addr(1), Some(0x1000));

        // After eviction (COOLING -> COLD) the frame address is cleared.
        csm.evict_page(1).expect("a cooling page evicts");
        assert_eq!(csm.frame_addr(1), None);
    }

    #[test]
    fn temperature_counts_buckets_pages_by_thermal_state() {
        // temperature_counts is only exercised via the Debug impl; pin that it
        // tallies one page into each of hot/cooling/cold and that the buckets
        // sum to the tracked count. Set up one page per state.
        let csm = CoolingStateMachine::new(CoolingConfig::default());
        csm.register_page(1);
        csm.register_page(2);
        csm.register_page(3); // stays COLD (never loaded)

        csm.load_page(1, 0x1000); // -> HOT
        csm.load_page(2, 0x2000); // -> HOT
        csm.run_cooling_scan(); // pages 1 and 2 -> COOLING
        csm.load_page(1, 0x1000); // re-heat page 1 -> HOT, leaving page 2 COOLING

        let counts = csm.temperature_counts();
        assert_eq!(counts.hot, 1);
        assert_eq!(counts.cooling, 1);
        assert_eq!(counts.cold, 1);
        assert_eq!(
            counts.hot + counts.cooling + counts.cold,
            csm.tracked_count()
        );
    }
}
