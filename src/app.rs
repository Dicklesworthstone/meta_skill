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

    /// Ensure the search index was opened for writing.
    ///
    /// [`AppContext::from_cli`] transparently falls back to a **read-only**
    /// search index when the writable open fails, so that read-only commands
    /// (`search`, `load`, `list`, …) keep working alongside a live
    /// `ms mcp serve` that holds the Tantivy writer lock. Commands that mutate
    /// the index (`index`, and the other re-indexing paths) must call this
    /// first: without it they would perform partial SQLite/Git writes and then
    /// abort mid-run with an opaque Tantivy "read-only mode" error — exactly the
    /// failure reported in issue #133.
    ///
    /// Performs no side effects; on failure returns a clear, actionable error
    /// naming the concrete cause (held writer lock vs. read-only filesystem)
    /// and the index that was selected.
    pub fn require_writable_search(&self) -> Result<()> {
        if self.search.is_readonly() {
            Err(MsError::SearchIndexReadOnly(
                self.readonly_search_diagnostic(),
            ))
        } else {
            Ok(())
        }
    }

    fn readonly_search_diagnostic(&self) -> String {
        let index_dir = self.ms_root.join("index");
        let writer_lock = index_dir.join(".tantivy-writer.lock");

        let cause = if dir_is_writable(&index_dir) {
            if writer_lock.exists() {
                format!(
                    "another `ms` process is holding the search-index writer lock at {}. \
                     A running `ms mcp serve` keeps this lock for its entire lifetime; \
                     stop it (or any other `ms` writing to this index) and retry. \
                     If you are certain no such process is running, the lock is stale \
                     and can be deleted.",
                    writer_lock.display()
                )
            } else {
                format!(
                    "the search index could not be opened for writing: {}",
                    index_dir.display()
                )
            }
        } else {
            format!(
                "the index directory is on a read-only filesystem or you lack \
                 permission to write to it: {}",
                index_dir.display()
            )
        };

        format!(
            "Cannot write to the search index: {cause}\n\
             This index was selected from {} (via $MS_ROOT, the nearest .ms directory \
             above the current working directory, or the global data dir). If that is \
             not the index you meant to write, run the command from the target project \
             or set MS_ROOT explicitly.",
            self.ms_root.display()
        )
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

/// Best-effort writability probe: try to create (and immediately remove) a
/// uniquely named file inside `dir`, or its nearest existing ancestor. Used to
/// distinguish a genuinely read-only filesystem / permission problem from mere
/// writer-lock contention when reporting why the search index is read-only.
fn dir_is_writable(dir: &Path) -> bool {
    // Walk up to the nearest directory that actually exists — the target may be
    // absent if creation itself failed on a read-only mount.
    let mut probe_dir = dir;
    while !probe_dir.exists() {
        match probe_dir.parent() {
            Some(parent) => probe_dir = parent,
            None => return false,
        }
    }
    let probe = probe_dir.join(format!(".ms-writable-probe-{}", std::process::id()));
    match std::fs::File::create(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
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
