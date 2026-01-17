//! TUI components for `meta_skill`.
//!
//! This module provides rich terminal user interfaces using ratatui.

pub mod browse;
pub mod build_tui;

pub use browse::{BrowseTui, run_browse_tui};
pub use build_tui::BuildTui;
