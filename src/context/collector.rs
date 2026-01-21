//! Working context collector for file and tool detection.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};
use lru::LruCache;
use serde::{Deserialize, Serialize};
use std::num::NonZeroUsize;
use std::sync::Mutex;

use crate::context::detector::{DefaultDetector, DetectedProject, ProjectDetector};
use crate::error::Result;

/// Configuration for context collection
#[derive(Debug, Clone)]
pub struct ContextCollectorConfig {
    pub max_recent_files: usize,
    pub recent_file_max_age: Duration,
    pub scan_depth: usize,
    pub ignore_patterns: Vec<String>,
}

impl Default for ContextCollectorConfig {
    fn default() -> Self {
        Self {
            max_recent_files: 20,
            recent_file_max_age: Duration::from_secs(24 * 3600), // 24 hours
            scan_depth: 3,
            ignore_patterns: vec![
                ".git".to_string(),
                "node_modules".to_string(),
                "target".to_string(),
                "dist".to_string(),
                "build".to_string(),
                "vendor".to_string(),
            ],
        }
    }
}

/// A recently accessed or modified file in the context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentFile {
    pub path: PathBuf,
    pub extension: Option<String>,
    pub modified_at: DateTime<Utc>,
    pub size: u64,
}

/// Git context information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitContext {
    pub branch: String,
    pub repo_root: PathBuf,
    pub has_uncommitted_changes: bool,
    pub recent_commits: Vec<String>,
}

/// Fingerprint for context caching
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct CollectorFingerprint(pub u64);

/// Full working context snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectedContext {
    pub cwd: PathBuf,
    pub detected_projects: Vec<DetectedProject>,
    pub recent_files: Vec<RecentFile>,
    pub detected_tools: HashSet<String>,
    pub git_context: Option<GitContext>,
    pub env_signals: HashMap<String, String>,
    pub collected_at: DateTime<Utc>,
    pub fingerprint: CollectorFingerprint,
}

impl CollectedContext {
    #[must_use]
    pub fn fingerprint(&self) -> CollectorFingerprint {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();

        self.cwd.hash(&mut hasher);
        // We hash key structural elements, ignoring timestamps that change too frequently
        // unless they are materially relevant (like recent file list changes)

        // Hash detected projects (stable)
        for project in &self.detected_projects {
            project.project_type.hash(&mut hasher);
        }

        // Hash recent file paths and extensions (stable-ish)
        for file in &self.recent_files {
            file.path.hash(&mut hasher);
            file.extension.hash(&mut hasher);
        }

        // Hash tools (very stable)
        let mut tools: Vec<_> = self.detected_tools.iter().collect();
        tools.sort();
        for tool in tools {
            tool.hash(&mut hasher);
        }

        // Hash git branch (semi-stable)
        if let Some(git) = &self.git_context {
            git.branch.hash(&mut hasher);
            git.repo_root.hash(&mut hasher);
        }

        CollectorFingerprint(hasher.finish())
    }
}

/// Collector for working context
pub struct ContextCollector {
    config: ContextCollectorConfig,
    project_detector: Box<dyn ProjectDetector>,
    cache: Mutex<LruCache<PathBuf, CollectedContext>>,
}

impl ContextCollector {
    #[must_use]
    pub fn new(config: ContextCollectorConfig) -> Self {
        Self {
            config,
            project_detector: Box::new(DefaultDetector::new()),
            cache: Mutex::new(LruCache::new(NonZeroUsize::new(10).unwrap())),
        }
    }

    /// Collect full working context for the given directory
    pub fn collect(&self, cwd: &Path) -> Result<CollectedContext> {
        // Check cache first
        if let Ok(mut cache) = self.cache.lock() {
            if let Some(ctx) = cache.get(cwd) {
                // Simple TTL check: if less than 10s old, return cached
                let elapsed = Utc::now().signed_duration_since(ctx.collected_at);
                if elapsed.num_seconds() < 10 {
                    return Ok(ctx.clone());
                }
            }
        }

        let detected_projects = self.project_detector.detect(cwd);
        let recent_files = self.collect_recent_files(cwd);
        let detected_tools = self.detect_tools();
        let git_context = self.collect_git_context(cwd);
        let env_signals = self.collect_env_signals();

        let mut ctx = CollectedContext {
            cwd: cwd.to_path_buf(),
            detected_projects,
            recent_files,
            detected_tools,
            git_context,
            env_signals,
            collected_at: Utc::now(),
            fingerprint: CollectorFingerprint(0), // Placeholder
        };

        ctx.fingerprint = ctx.fingerprint();

        // Update cache
        if let Ok(mut cache) = self.cache.lock() {
            cache.put(cwd.to_path_buf(), ctx.clone());
        }

        Ok(ctx)
    }

    fn collect_recent_files(&self, cwd: &Path) -> Vec<RecentFile> {
        let mut files = Vec::new();
        let now = SystemTime::now();

        // Use walkdir to scan recursively
        let walker = walkdir::WalkDir::new(cwd)
            .max_depth(self.config.scan_depth)
            .follow_links(false)
            .into_iter();

        for entry in walker.filter_map(std::result::Result::ok) {
            if !entry.file_type().is_file() {
                continue;
            }

            // Check ignore patterns
            let path_str = entry.path().to_string_lossy();
            if self
                .config
                .ignore_patterns
                .iter()
                .any(|p| path_str.contains(p))
            {
                continue;
            }

            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(age) = now.duration_since(modified) {
                        if age <= self.config.recent_file_max_age {
                            files.push(RecentFile {
                                path: entry.path().to_path_buf(),
                                extension: entry
                                    .path()
                                    .extension()
                                    .map(|s| s.to_string_lossy().to_string()),
                                modified_at: DateTime::from(modified),
                                size: metadata.len(),
                            });
                        }
                    }
                }
            }
        }

        // Sort by recency (newest first) and limit
        files.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
        files.truncate(self.config.max_recent_files);

        files
    }

    fn detect_tools(&self) -> HashSet<String> {
        let checklist = [
            "cargo",
            "rustc",
            "rust-analyzer",
            "npm",
            "node",
            "yarn",
            "pnpm",
            "bun",
            "python",
            "python3",
            "pip",
            "poetry",
            "uv",
            "go",
            "gofmt",
            "java",
            "javac",
            "mvn",
            "gradle",
            "dotnet",
            "msbuild",
            "ruby",
            "bundle",
            "gem",
            "elixir",
            "mix",
            "docker",
            "kubectl",
            "git",
        ];

        checklist
            .iter()
            .filter(|tool| which::which(tool).is_ok())
            .map(std::string::ToString::to_string)
            .collect()
    }

    fn collect_git_context(&self, cwd: &Path) -> Option<GitContext> {
        let repo = git2::Repository::discover(cwd).ok()?;
        let head = repo.head().ok()?;
        let branch = head.shorthand().unwrap_or("HEAD").to_string();
        let repo_root = repo.workdir()?.to_path_buf();

        // Simple check for uncommitted changes
        let has_uncommitted_changes = repo.statuses(None).map(|s| !s.is_empty()).unwrap_or(false);

        let mut recent_commits = Vec::new();
        if let Ok(mut revwalk) = repo.revwalk() {
            revwalk.push_head().ok()?;
            for oid in revwalk.take(5).flatten() {
                if let Ok(commit) = repo.find_commit(oid) {
                    if let Some(summary) = commit.summary() {
                        recent_commits.push(summary.to_string());
                    }
                }
            }
        }

        Some(GitContext {
            branch,
            repo_root,
            has_uncommitted_changes,
            recent_commits,
        })
    }

    fn collect_env_signals(&self) -> HashMap<String, String> {
        let signals = [
            "RUST_LOG",
            "NODE_ENV",
            "PYTHON_ENV",
            "DEBUG",
            "CI",
            "EDITOR",
        ];

        signals
            .iter()
            .filter_map(|var| std::env::var(var).ok().map(|v| (var.to_string(), v)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::TempDir;

    #[test]
    fn test_config_defaults() {
        let config = ContextCollectorConfig::default();
        assert_eq!(config.max_recent_files, 20);
        assert_eq!(config.scan_depth, 3);
        assert!(config.ignore_patterns.contains(&".git".to_string()));
    }

    #[test]
    fn test_fingerprint_stability() {
        let ctx1 = CollectedContext {
            cwd: PathBuf::from("/tmp/test"),
            detected_projects: vec![],
            recent_files: vec![],
            detected_tools: HashSet::new(),
            git_context: None,
            env_signals: HashMap::new(),
            collected_at: Utc::now(),
            fingerprint: CollectorFingerprint(0),
        };

        let mut ctx2 = ctx1.clone();
        // Timestamp change shouldn't affect fingerprint
        ctx2.collected_at = ctx1.collected_at + chrono::TimeDelta::hours(1);

        assert_eq!(ctx1.fingerprint(), ctx2.fingerprint());

        ctx2.detected_tools.insert("cargo".to_string());
        assert_ne!(ctx1.fingerprint(), ctx2.fingerprint());
    }

    #[test]
    fn test_collect_env_signals() {
        let config = ContextCollectorConfig::default();
        let collector = ContextCollector::new(config);

        // Test that the method runs without panicking and returns a HashMap
        let signals = collector.collect_env_signals();

        // Verify it returns a HashMap (may or may not have entries depending on env)
        assert!(signals.len() <= 6); // We only check 6 specific env vars
    }

    #[test]
    fn test_collect_recent_files() {
        let temp = TempDir::new().unwrap();
        let config = ContextCollectorConfig {
            max_recent_files: 5,
            recent_file_max_age: Duration::from_secs(60),
            scan_depth: 1,
            ignore_patterns: vec![],
        };
        let collector = ContextCollector::new(config);

        // Create some files
        let file1 = temp.path().join("a.txt");
        File::create(&file1).unwrap();

        let recent = collector.collect_recent_files(temp.path());
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].path, file1);
        assert_eq!(recent[0].extension.as_deref(), Some("txt"));
    }

    #[test]
    fn test_detect_tools_real() {
        // This test relies on actual tools being present (e.g. git, cargo)
        let config = ContextCollectorConfig::default();
        let collector = ContextCollector::new(config);

        let tools = collector.detect_tools();
        // We can't assert specific tools unless we know the environment,
        // but we can assert it runs without panicking.
        // Assuming we are in a dev environment, 'git' should likely be found.
        if which::which("git").is_ok() {
            assert!(tools.contains("git"));
        }
    }
}
