//! Minimal logging example using RichLogger.

use log::LevelFilter;
use rich_rust::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let console = Console::new().shared();
    RichLogger::new(console)
        .level(LevelFilter::Info)
        .show_path(true)
        .init()?;

    log::info!("Server started");
    log::warn!("Cache miss");
    log::error!("Request failed");

    Ok(())
}
