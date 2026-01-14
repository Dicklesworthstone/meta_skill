pub mod app;
pub mod bundler;
pub mod cass;
pub mod cm;
pub mod cli;
pub mod config;
pub mod context;
pub mod core;
pub mod error;
pub mod meta_skills;
pub mod quality;
pub mod search;
pub mod security;
pub mod storage;
pub mod test_utils;
pub mod suggestions;
pub mod testing;
pub mod updater;
pub mod utils;

pub use error::{MsError, Result};

/// Package version from Cargo.toml.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
