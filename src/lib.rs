pub mod app;
pub mod bundler;
pub mod cass;
pub mod cli;
pub mod config;
pub mod core;
pub mod error;
pub mod quality;
pub mod search;
pub mod security;
pub mod storage;
pub mod updater;
pub mod utils;

pub use error::{MsError, Result};

/// Package version from Cargo.toml.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
