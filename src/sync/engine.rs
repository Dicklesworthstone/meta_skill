use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};
use git2::{Repository, build::CheckoutBuilder};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::core::SkillSpec;
use crate::error::{MsError, Result};
use crate::storage::{Database, GitArchive, TxManager};

use super::SyncConfig;
use super::config::{ConflictStrategy, RemoteConfig, RemoteType};
use super::machine::MachineIdentity;
use super::state::{SkillSyncState, SkillSyncStatus, SyncState};

#[derive(Debug, Clone, Default)]
pub struct SyncOptions {
    pub push_only: bool,
    pub pull_only: bool,
    pub dry_run: bool,
    pub force: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SyncReport {
    pub remote: String,
    pub pushed: Vec<String>,
    pub pulled: Vec<String>,
    pub resolved: Vec<String>,
    pub conflicts: Vec<String>,
    pub forked: Vec<String>,
    pub skipped: Vec<String>,
    pub errors: Vec<String>,
    pub duration_ms: u128,
}

impl SyncReport {
    pub fn summary_line(&self) -> String {
        format!(
            "{}: ↓{} ↑{} ⚠{} ↯{}",
            self.remote,
            self.pulled.len(),
            self.pushed.len(),
            self.conflicts.len(),
            self.forked.len()
        )
    }
}

#[derive(Debug, Clone)]
struct SkillSnapshot {
    hash: String,
    #[allow(dead_code)]
    id: String,
    modified: DateTime<Utc>,
}

pub struct SyncEngine {
    config: SyncConfig,
    machine: MachineIdentity,
    state: SyncState,
    git: Arc<GitArchive>,
    db: Arc<Database>,
    ms_root: PathBuf,
}

impl SyncEngine {
    pub fn new(
        config: SyncConfig,
        machine: MachineIdentity,
        state: SyncState,
        git: Arc<GitArchive>,
        db: Arc<Database>,
        ms_root: PathBuf,
    ) -> Self {
        Self {
            config,
            machine,
            state,
            git,
            db,
            ms_root,
        }
    }

    pub fn config(&self) -> &SyncConfig {
        &self.config
    }

    pub fn state(&self) -> &SyncState {
        &self.state
    }

    pub fn machine(&self) -> &MachineIdentity {
        &self.machine
    }

    pub fn sync_all(&mut self, options: &SyncOptions) -> Result<Vec<SyncReport>> {
        let mut reports = Vec::new();
        for remote in self.config.remotes.clone() {
            if !remote.enabled {
                continue;
            }
            reports.push(self.sync_remote(&remote.name, options)?);
        }
        Ok(reports)
    }

    pub fn sync_remote(&mut self, remote_name: &str, options: &SyncOptions) -> Result<SyncReport> {
        let Some(remote) = self
            .config
            .remotes
            .iter()
            .find(|r| r.name == remote_name)
            .cloned()
        else {
            return Err(MsError::Config(format!(
                "remote not found: {}",
                remote_name
            )));
        };

        let start = Instant::now();
        let mut report = SyncReport {
            remote: remote_name.to_string(),
            ..Default::default()
        };

        match remote.remote_type {
            RemoteType::FileSystem => {
                self.sync_filesystem(&remote, options, &mut report)?;
            }
            RemoteType::Git => {
                let remote_git = open_git_remote(&remote, &self.ms_root)?;
                self.sync_with_archive(&remote, options, &mut report, &remote_git)?;
            }
        }

        if !options.dry_run {
            self.state
                .last_full_sync
                .insert(remote.name.clone(), Utc::now());
            self.machine.record_sync(&remote.name);
            self.state.save(&self.ms_root)?;
            self.machine.save()?;
        }

        report.duration_ms = start.elapsed().as_millis();
        Ok(report)
    }

    fn sync_filesystem(
        &mut self,
        remote: &RemoteConfig,
        options: &SyncOptions,
        report: &mut SyncReport,
    ) -> Result<()> {
        let remote_root = resolve_archive_root(Path::new(&remote.url))?;
        let remote_git = GitArchive::open(&remote_root)?;
        self.sync_with_archive(remote, options, report, &remote_git)
    }

    fn sync_with_archive(
        &mut self,
        remote: &RemoteConfig,
        options: &SyncOptions,
        report: &mut SyncReport,
        remote_git: &GitArchive,
    ) -> Result<()> {
        let allow_push = remote.direction.allows_push() && !options.pull_only;
        let allow_pull = remote.direction.allows_pull() && !options.push_only;

        let local_map = self.snapshot_archive(self.git.as_ref())?;
        let remote_map = self.snapshot_archive(remote_git)?;

        let mut all_ids = HashSet::new();
        all_ids.extend(local_map.keys().cloned());
        all_ids.extend(remote_map.keys().cloned());

        let tx_mgr = TxManager::new(self.db.clone(), self.git.clone(), self.ms_root.clone())?;

        for id in all_ids {
            let local = local_map.get(&id);
            let remote_snap = remote_map.get(&id);

            let status = match (local, remote_snap) {
                (Some(l), Some(r)) if l.hash == r.hash => SkillSyncStatus::Synced,
                (Some(l), Some(r)) => {
                    if l.modified > r.modified {
                        SkillSyncStatus::LocalAhead
                    } else if r.modified > l.modified {
                        SkillSyncStatus::RemoteAhead
                    } else {
                        SkillSyncStatus::Diverged
                    }
                }
                (Some(_), None) => SkillSyncStatus::LocalOnly,
                (None, Some(_)) => SkillSyncStatus::RemoteOnly,
                (None, None) => continue,
            };

            let mut final_status = status.clone();

            match status {
                SkillSyncStatus::Synced => {
                    report.skipped.push(id.clone());
                }
                SkillSyncStatus::LocalAhead | SkillSyncStatus::LocalOnly => {
                    if allow_push {
                        if !options.dry_run {
                            let spec = self.git.read_skill(&id)?;
                            remote_git.write_skill(&spec)?;
                        }
                        report.pushed.push(id.clone());
                        final_status = SkillSyncStatus::Synced;
                    } else {
                        report.skipped.push(id.clone());
                    }
                }
                SkillSyncStatus::RemoteAhead | SkillSyncStatus::RemoteOnly => {
                    if allow_pull {
                        if !options.dry_run {
                            let spec = remote_git.read_skill(&id)?;
                            tx_mgr.write_skill_locked(&spec)?;
                        }
                        report.pulled.push(id.clone());
                        final_status = SkillSyncStatus::Synced;
                    } else {
                        report.skipped.push(id.clone());
                    }
                }
                SkillSyncStatus::Diverged | SkillSyncStatus::Conflict => {
                    if options.force {
                        let strategy = self
                            .config
                            .conflict_strategies
                            .get(&id)
                            .cloned()
                            .unwrap_or(self.config.sync.default_conflict_strategy);
                        final_status = self.apply_conflict_strategy(
                            &id,
                            local,
                            remote_snap,
                            remote_git,
                            allow_push,
                            allow_pull,
                            options,
                            &tx_mgr,
                            report,
                            strategy,
                        )?;
                        report.resolved.push(id.clone());
                    } else {
                        report.conflicts.push(id.clone());
                        final_status = SkillSyncStatus::Conflict;
                    }
                }
            }

            let state_entry = SkillSyncState {
                skill_id: id.clone(),
                local_hash: local.map(|s| s.hash.clone()),
                remote_hashes: remote_snap
                    .map(|s| {
                        vec![(remote.name.clone(), s.hash.clone())]
                            .into_iter()
                            .collect()
                    })
                    .unwrap_or_default(),
                local_modified: local.map(|s| s.modified),
                remote_modified: remote_snap
                    .map(|s| {
                        vec![(remote.name.clone(), s.modified)]
                            .into_iter()
                            .collect()
                    })
                    .unwrap_or_default(),
                status: final_status,
                last_modified_by: None,
            };
            self.state.skill_states.insert(id, state_entry);
        }

        Ok(())
    }

    #[allow(dead_code)]
    fn sync_git(
        &mut self,
        _remote: &RemoteConfig,
        _options: &SyncOptions,
        _report: &mut SyncReport,
    ) -> Result<()> {
        Err(MsError::NotImplemented(
            "Git remote sync not yet implemented".to_string(),
        ))
    }

    fn apply_conflict_strategy(
        &self,
        id: &str,
        local: Option<&SkillSnapshot>,
        remote_snap: Option<&SkillSnapshot>,
        remote_git: &GitArchive,
        allow_push: bool,
        allow_pull: bool,
        options: &SyncOptions,
        tx_mgr: &TxManager,
        report: &mut SyncReport,
        strategy: ConflictStrategy,
    ) -> Result<SkillSyncStatus> {
        match strategy {
            ConflictStrategy::PreferLocal => {
                if allow_push {
                    if !options.dry_run {
                        let spec = self.git.read_skill(id)?;
                        remote_git.write_skill(&spec)?;
                    }
                    report.pushed.push(id.to_string());
                    Ok(SkillSyncStatus::Synced)
                } else {
                    report.conflicts.push(id.to_string());
                    Ok(SkillSyncStatus::Conflict)
                }
            }
            ConflictStrategy::PreferRemote => {
                if allow_pull {
                    if !options.dry_run {
                        let spec = remote_git.read_skill(id)?;
                        tx_mgr.write_skill_locked(&spec)?;
                    }
                    report.pulled.push(id.to_string());
                    Ok(SkillSyncStatus::Synced)
                } else {
                    report.conflicts.push(id.to_string());
                    Ok(SkillSyncStatus::Conflict)
                }
            }
            ConflictStrategy::PreferNewest => {
                let local_time = local.map(|s| s.modified);
                let remote_time = remote_snap.map(|s| s.modified);
                if local_time >= remote_time {
                    self.apply_conflict_strategy(
                        id,
                        local,
                        remote_snap,
                        remote_git,
                        allow_push,
                        allow_pull,
                        options,
                        tx_mgr,
                        report,
                        ConflictStrategy::PreferLocal,
                    )
                } else {
                    self.apply_conflict_strategy(
                        id,
                        local,
                        remote_snap,
                        remote_git,
                        allow_push,
                        allow_pull,
                        options,
                        tx_mgr,
                        report,
                        ConflictStrategy::PreferRemote,
                    )
                }
            }
            ConflictStrategy::KeepBoth => {
                if allow_pull {
                    if !options.dry_run {
                        let mut spec = remote_git.read_skill(id)?;
                        let fork_id = format!("{}-remote", id);
                        spec.metadata.id = fork_id.clone();
                        spec.metadata.name = format!("{} (remote)", spec.metadata.name);
                        tx_mgr.write_skill_locked(&spec)?;
                        report.forked.push(fork_id);
                    } else {
                        report.forked.push(format!("{}-remote", id));
                    }
                    Ok(SkillSyncStatus::Synced)
                } else {
                    report.conflicts.push(id.to_string());
                    Ok(SkillSyncStatus::Conflict)
                }
            }
        }
    }

    fn snapshot_archive(&self, archive: &GitArchive) -> Result<HashMap<String, SkillSnapshot>> {
        let mut map = HashMap::new();
        for id in archive.list_skill_ids()? {
            let spec = archive.read_skill(&id)?;
            let hash = hash_skill_spec(&spec)?;
            let modified = skill_modified_time(archive, &id)?;

            map.insert(id.clone(), SkillSnapshot { id, hash, modified });
        }
        Ok(map)
    }
}

fn resolve_archive_root(path: &Path) -> Result<PathBuf> {
    if path.join("skills").join("by-id").exists() {
        return Ok(path.to_path_buf());
    }
    let candidate = path.join("archive");
    if candidate.join("skills").join("by-id").exists() {
        return Ok(candidate);
    }
    Err(MsError::Config(format!(
        "remote path {} does not look like an ms archive (expected skills/by-id)",
        path.display()
    )))
}

fn skill_modified_time(archive: &GitArchive, id: &str) -> Result<DateTime<Utc>> {
    let skill_path = archive
        .skill_path(id)
        .ok_or_else(|| MsError::ValidationFailed("invalid skill id".to_string()))?;
    let spec_path = skill_path.join("skill.spec.json");
    let metadata = std::fs::metadata(&spec_path)?;
    let modified = metadata.modified()?;
    Ok(DateTime::<Utc>::from(modified))
}

fn hash_skill_spec(spec: &SkillSpec) -> Result<String> {
    let json = serde_json::to_vec(spec)?;
    let mut hasher = Sha256::new();
    hasher.update(&json);
    Ok(format!("{:x}", hasher.finalize()))
}

fn open_git_remote(remote: &RemoteConfig, ms_root: &Path) -> Result<GitArchive> {
    let cache_root = ms_root.join("sync").join("remotes").join(&remote.name);
    if !cache_root.exists() {
        Repository::clone(&remote.url, &cache_root).map_err(MsError::Git)?;
    }

    let repo = Repository::open(&cache_root).map_err(MsError::Git)?;
    sync_git_repo(&repo, &remote.url)?;

    let archive_root = resolve_archive_root(&cache_root)?;
    GitArchive::open(archive_root)
}

fn sync_git_repo(repo: &Repository, remote_url: &str) -> Result<()> {
    let mut remote = repo
        .find_remote("origin")
        .or_else(|_| repo.remote_anonymous(remote_url))
        .map_err(MsError::Git)?;
    remote
        .fetch(&["refs/heads/*:refs/remotes/origin/*"], None, None)
        .map_err(MsError::Git)?;

    let head = repo.head().map_err(MsError::Git)?;
    let branch = head.shorthand().unwrap_or("main");
    let remote_ref = format!("refs/remotes/origin/{}", branch);
    let Ok(remote_ref) = repo.find_reference(&remote_ref) else {
        return Ok(());
    };
    let Some(target) = remote_ref.target() else {
        return Ok(());
    };

    let analysis = repo.merge_analysis(&[&repo
        .reference_to_annotated_commit(&remote_ref)
        .map_err(MsError::Git)?])?;
    if analysis.0.is_up_to_date() {
        return Ok(());
    }
    if !analysis.0.is_fast_forward() {
        return Err(MsError::Config(format!(
            "non-fast-forward update required for branch {branch}"
        )));
    }

    let local_ref = format!("refs/heads/{}", branch);
    let mut reference = match repo.find_reference(&local_ref) {
        Ok(r) => r,
        Err(_) => repo
            .reference(&local_ref, target, true, "init branch")
            .map_err(MsError::Git)?,
    };

    reference
        .set_target(target, "fast-forward")
        .map_err(MsError::Git)?;
    repo.set_head(&local_ref).map_err(MsError::Git)?;
    repo.checkout_head(Some(CheckoutBuilder::new().force()))
        .map_err(MsError::Git)?;
    Ok(())
}
