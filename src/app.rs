use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::cli::OutputFormat;
use crate::config::Config;
use crate::error::{MsError, Result};
use crate::search::SearchIndex;
use crate::storage::{Database, GitArchive};

pub struct AppContext {
    pub ms_root: PathBuf,
    pub config_path: PathBuf,
    pub config: Config,
    pub db: Arc<Database>,
    pub git: Arc<GitArchive>,
    pub search: Arc<SearchIndex>,
    /// Deprecated: use output_format instead
    pub robot_mode: bool,
    pub output_format: OutputFormat,
    pub verbosity: u8,
}

impl AppContext {
    pub fn from_cli(cli: &crate::cli::Cli) -> Result<Self> {
        let ms_root = Self::find_ms_root()?;
        let config_path = cli
            .config
            .clone()
            .unwrap_or_else(|| default_config_path(&ms_root));
        let config = Config::load(cli.config.as_deref(), &ms_root)?;

        Ok(Self {
            ms_root: ms_root.clone(),
            config_path,
            config,
            db: Arc::new(Database::open(ms_root.join("ms.db"))?),
            git: Arc::new(GitArchive::open(ms_root.join("archive"))?),
            search: Arc::new({
                let index_path = ms_root.join("index");
                // Try writable first; if the write lock is busy (another process),
                // fall back to read-only mode so concurrent MCP servers and CLI
                // commands can coexist without "LockBusy" errors.
                SearchIndex::open(&index_path)
                    .or_else(|_| SearchIndex::open_readonly(&index_path))?
            }),
            robot_mode: cli.robot,
            output_format: cli.output_format(),
            verbosity: cli.verbose,
        })
    }

    fn find_ms_root() -> Result<PathBuf> {
        if let Ok(root) = std::env::var("MS_ROOT") {
            return Ok(PathBuf::from(root));
        }
        let cwd = std::env::current_dir()?;
        if let Some(found) = find_upwards(&cwd, ".ms")? {
            return Ok(found);
        }

        let data_dir = dirs::data_dir()
            .ok_or_else(|| MsError::MissingConfig("data directory not found".to_string()))?;
        Ok(data_dir.join("ms"))
    }
}

fn default_config_path(ms_root: &Path) -> PathBuf {
    if ms_root.ends_with(".ms") {
        ms_root.join("config.toml")
    } else {
        dirs::config_dir()
            .unwrap_or_else(|| ms_root.to_path_buf())
            .join("ms/config.toml")
    }
}

fn find_upwards(start: &Path, name: &str) -> Result<Option<PathBuf>> {
    let mut current = Some(start);
    while let Some(dir) = current {
        let candidate = dir.join(name);
        if candidate.is_dir() {
            return Ok(Some(candidate));
        }
        current = dir.parent();
    }
    Ok(None)
}
