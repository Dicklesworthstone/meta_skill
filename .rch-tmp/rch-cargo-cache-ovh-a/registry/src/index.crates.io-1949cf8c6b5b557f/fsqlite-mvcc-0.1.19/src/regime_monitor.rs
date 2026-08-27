//! Unified Regime Monitor interface.
//!
//! Provides an enum-based dispatch over different regime shift detection
//! algorithms (e.g. BOCPD vs Conformal Martingale).

use crate::bocpd::{BocpdConfig, BocpdMonitor, RegimeStats};
use crate::conformal_martingale::{ConformalMartingaleConfig, ConformalMartingaleMonitor};

#[derive(Debug, Clone)]
pub enum RegimeMonitorConfig {
    Bocpd(BocpdConfig),
    ConformalMartingale(ConformalMartingaleConfig),
}

impl Default for RegimeMonitorConfig {
    fn default() -> Self {
        // Default to the new Conformal Martingale approach due to its
        // superior distribution-free properties and performance.
        Self::ConformalMartingale(ConformalMartingaleConfig::default())
    }
}

pub enum RegimeMonitor {
    Bocpd(BocpdMonitor),
    ConformalMartingale(ConformalMartingaleMonitor),
}

impl RegimeMonitor {
    pub fn new(config: RegimeMonitorConfig) -> Self {
        match config {
            RegimeMonitorConfig::Bocpd(cfg) => Self::Bocpd(BocpdMonitor::new(cfg)),
            RegimeMonitorConfig::ConformalMartingale(cfg) => {
                Self::ConformalMartingale(ConformalMartingaleMonitor::new(cfg))
            }
        }
    }

    pub fn observe(&mut self, x: f64) {
        match self {
            Self::Bocpd(monitor) => monitor.observe(x),
            Self::ConformalMartingale(monitor) => monitor.observe(x),
        }
    }

    pub fn change_point_detected(&self) -> bool {
        match self {
            Self::Bocpd(monitor) => monitor.change_point_detected(),
            Self::ConformalMartingale(monitor) => monitor.change_point_detected(),
        }
    }

    pub fn current_regime_stats(&self) -> RegimeStats {
        match self {
            Self::Bocpd(monitor) => monitor.current_regime_stats(),
            Self::ConformalMartingale(monitor) => monitor.current_regime_stats(),
        }
    }

    pub fn observation_count(&self) -> u64 {
        match self {
            Self::Bocpd(monitor) => monitor.observation_count(),
            Self::ConformalMartingale(monitor) => monitor.observation_count(),
        }
    }
}
