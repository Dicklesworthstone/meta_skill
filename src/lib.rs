pub mod agent_mail;
pub mod antipatterns;
pub mod app;
pub mod beads;
pub mod bundler;
pub mod cass;
pub mod cli;
pub mod cm;
pub mod config;
pub mod context;
pub mod core;
pub mod dedup;
pub mod error;
pub mod graph;
pub mod lint;
pub mod meta_skills;
pub mod quality;
pub mod search;
pub mod security;
pub mod simulation;
pub mod storage;
pub mod suggestions;
pub mod sync;
pub mod test_utils;
pub mod testing;
pub mod templates;
pub mod tui;
pub mod updater;
pub mod utils;

pub use error::{MsError, Result};

/// Package version from Cargo.toml.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
