use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};
use git2::{build::CheckoutBuilder, Cred, RemoteCallbacks, Repository};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::core::SkillSpec;
use crate::config::RuConfig;
use crate::error::{MsError, Result};
use crate::storage::{Database, GitArchive, TxManager};

use super::SyncConfig;
use super::config::{ConflictStrategy, RemoteAuth, RemoteConfig, RemoteType, validate_remote_name};
use super::machine::MachineIdentity;
use super::ru::{RuClient, RuExitCode, RuSyncOptions};
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
    pub cloned: Vec<String>,
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
    #[must_use] 
    pub fn summary_line(&self) -> String {
        format!(
            "{}: ↓{} +{} ↑{} ⚠{} ↯{}",
            self.remote,
            self.pulled.len(),
            self.cloned.len(),
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
    ru_config: RuConfig,
}

impl SyncEngine {
    pub const fn new(
        config: SyncConfig,
        machine: MachineIdentity,
        state: SyncState,
        git: Arc<GitArchive>,
        db: Arc<Database>,
        ms_root: PathBuf,
        ru_config: RuConfig,
    ) -> Self {
        Self {
            config,
            machine,
            state,
            git,
            db,
            ms_root,
            ru_config,
        }
    }

    #[must_use] 
    pub const fn config(&self) -> &SyncConfig {
        &self.config
    }

    #[must_use] 
    pub const fn state(&self) -> &SyncState {
        &self.state
    }

    #[must_use] 
    pub const fn machine(&self) -> &MachineIdentity {
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
        validate_remote_name(remote_name)?;
        let Some(remote) = self
            .config
            .remotes
            .iter()
            .find(|r| r.name == remote_name)
            .cloned()
        else {
            return Err(MsError::Config(format!(
                "remote not found: {remote_name}"
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
            RemoteType::Ru => {
                self.sync_ru(&remote, options, &mut report)?;
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

    fn sync_ru(
        &mut self,
        _remote: &RemoteConfig,
        options: &SyncOptions,
        report: &mut SyncReport,
    ) -> Result<()> {
        if options.push_only {
            return Err(MsError::Config(
                "ru sync does not support push-only mode".to_string(),
            ));
        }
        if !self.ru_config.enabled {
            return Err(MsError::Config(
                "ru integration is disabled; set [ru].enabled=true".to_string(),
            ));
        }

        let mut client = if let Some(path) = &self.ru_config.ru_path {
            RuClient::with_path(PathBuf::from(path))
        } else {
            RuClient::new()
        };

        let mut ru_options = RuSyncOptions::default();
        ru_options.dry_run = options.dry_run;
        if self.ru_config.parallel > 0 {
            ru_options.parallel = Some(self.ru_config.parallel);
        }

        let result = client.sync(&ru_options)?;
        report.cloned.extend(result.cloned.clone());
        report.pulled.extend(result.pulled.clone());
        report.conflicts
            .extend(result.conflicts.iter().map(|c| c.repo.clone()));
        report.errors.extend(
            result
                .errors
                .iter()
                .map(|e| format!("{}: {}", e.repo, e.error)),
        );
        report.skipped.extend(result.skipped.clone());

        match RuExitCode::from_code(result.exit_code) {
            RuExitCode::Ok | RuExitCode::Partial | RuExitCode::Conflicts => Ok(()),
            RuExitCode::Interrupted => Err(MsError::Config(
                "ru sync interrupted; re-run with --resume".to_string(),
            )),
            RuExitCode::SystemError | RuExitCode::BadArgs => Err(MsError::Config(
                "ru sync failed; check ru output for details".to_string(),
            )),
        }
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
        let git_auth = if remote.remote_type == RemoteType::Git {
            Some(resolve_auth(remote)?)
        } else {
            None
        };
        let mut needs_git_push = false;

        let (local_map, local_errors) = self.snapshot_archive(self.git.as_ref())?;
        let (remote_map, remote_errors) = self.snapshot_archive(remote_git)?;

        report.errors.extend(local_errors);
        report.errors.extend(remote_errors);

        let mut all_ids = HashSet::new();
        all_ids.extend(local_map.keys().cloned());
        all_ids.extend(remote_map.keys().cloned());

        let tx_mgr = TxManager::new(self.db.clone(), self.git.clone(), self.ms_root.clone())?;

        for id in all_ids {
            let local = local_map.get(&id);
            let remote_snap = remote_map.get(&id);

            let base_hash = self
                .state
                .skill_states
                .get(&id)
                .and_then(|state| state.remote_hashes.get(&remote.name).map(std::string::String::as_str));

            let status = determine_sync_status(local, remote_snap, base_hash);

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
                            if remote.remote_type == RemoteType::Git {
                                needs_git_push = true;
                            }
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
                            .copied()
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
                            remote.remote_type == RemoteType::Git,
                            &mut needs_git_push,
                        )?;
                        if final_status == SkillSyncStatus::Synced {
                            report.resolved.push(id.clone());
                        }
                    } else {
                        report.conflicts.push(id.clone());
                        final_status = SkillSyncStatus::Conflict;
                    }
                }
            }

            let existing = self.state.skill_states.get(&id);
            let mut remote_hashes = existing
                .map(|entry| entry.remote_hashes.clone())
                .unwrap_or_default();
            let mut remote_modified = existing
                .map(|entry| entry.remote_modified.clone())
                .unwrap_or_default();
            if let Some(snap) = remote_snap {
                remote_hashes.insert(remote.name.clone(), snap.hash.clone());
                remote_modified.insert(remote.name.clone(), snap.modified);
            } else {
                remote_hashes.remove(&remote.name);
                remote_modified.remove(&remote.name);
            }

            let state_entry = SkillSyncState {
                skill_id: id.clone(),
                local_hash: local.map(|s| s.hash.clone()),
                remote_hashes,
                local_modified: local.map(|s| s.modified),
                remote_modified,
                status: final_status,
                last_modified_by: existing.and_then(|entry| entry.last_modified_by.clone()),
            };
            self.state.skill_states.insert(id, state_entry);
        }

        if needs_git_push && !options.dry_run {
            if let Some(auth) = git_auth.as_ref() {
                push_git_repo(remote_git.repo(), &remote.url, remote.branch.as_deref(), auth)?;
            }
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
        remote_is_git: bool,
        needs_git_push: &mut bool,
    ) -> Result<SkillSyncStatus> {
        match strategy {
            ConflictStrategy::PreferLocal => {
                if allow_push {
                    if !options.dry_run {
                        let spec = self.git.read_skill(id)?;
                        remote_git.write_skill(&spec)?;
                        if remote_is_git {
                            *needs_git_push = true;
                        }
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
                        remote_is_git,
                        needs_git_push,
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
                        remote_is_git,
                        needs_git_push,
                    )
                }
            }
            ConflictStrategy::KeepBoth => {
                if !allow_pull {
                    report.conflicts.push(id.to_string());
                    return Ok(SkillSyncStatus::Conflict);
                }

                let fork_id = unique_fork_id(&self.git, id)?;
                if !options.dry_run {
                    let mut spec = remote_git.read_skill(id)?;
                    spec.metadata.id = fork_id.clone();
                    spec.metadata.name = format!("{} (remote)", spec.metadata.name);
                    tx_mgr.write_skill_locked(&spec)?;
                }
                report.forked.push(fork_id);

                if allow_push {
                    if !options.dry_run {
                        let spec = self.git.read_skill(id)?;
                        remote_git.write_skill(&spec)?;
                        if remote_is_git {
                            *needs_git_push = true;
                        }
                    }
                    report.pushed.push(id.to_string());
                    Ok(SkillSyncStatus::Synced)
                } else {
                    report.conflicts.push(id.to_string());
                    Ok(SkillSyncStatus::Conflict)
                }
            }
        }
    }

    fn snapshot_archive(
        &self,
        archive: &GitArchive,
    ) -> Result<(HashMap<String, SkillSnapshot>, Vec<String>)> {
        let ids = archive.list_skill_ids()?;
        let mut id_to_path = HashMap::new();
        let mut paths = Vec::new();

        for id in &ids {
            if let Some(skill_path) = archive.skill_path(id) {
                let spec_path = skill_path.join("skill.spec.json");
                if let Ok(rel) = spec_path.strip_prefix(archive.root()) {
                    let rel_buf = rel.to_path_buf();
                    id_to_path.insert(id.clone(), rel_buf.clone());
                    paths.push(rel_buf);
                }
            }
        }

        // Bulk fetch modification times (O(1) history walk)
        let modified_times = archive.get_bulk_last_modified(&paths).unwrap_or_default();

        let mut map = HashMap::new();
        let mut errors = Vec::new();

        for id in ids {
            let spec = match archive.read_skill(&id) {
                Ok(s) => s,
                Err(e) => {
                    errors.push(format!("Failed to read skill {id}: {e}"));
                    continue;
                }
            };
            
            let hash = match hash_skill_spec(&spec) {
                Ok(h) => h,
                Err(e) => {
                    errors.push(format!("Failed to hash skill {id}: {e}"));
                    continue;
                }
            };
            
            // Determine modified time: check bulk result, fallback to FS
            let modified = if let Some(rel_path) = id_to_path.get(&id) {
                if let Some(time) = modified_times.get(rel_path) {
                    *time
                } else {
                    // Fallback to FS metadata
                    let abs_path = archive.root().join(rel_path);
                    match std::fs::metadata(&abs_path) {
                        Ok(metadata) => {
                            metadata.modified().map_or_else(|_| Utc::now(), DateTime::<Utc>::from)
                        },
                        Err(_) => Utc::now()
                    }
                }
            } else {
                // Fallback (shouldn't happen if path logic is consistent)
                Utc::now()
            };

            map.insert(id.clone(), SkillSnapshot { hash, id, modified });
        }
        Ok((map, errors))
    }
}

fn unique_fork_id(archive: &GitArchive, id: &str) -> Result<String> {
    let base = format!("{id}-remote");
    if !archive.skill_exists(&base) {
        return Ok(base);
    }
    for suffix in 2..=1000 {
        let candidate = format!("{id}-remote-{suffix}");
        if !archive.skill_exists(&candidate) {
            return Ok(candidate);
        }
    }
    Err(MsError::Config(format!(
        "unable to allocate fork id for {id}"
    )))
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

fn hash_skill_spec(spec: &SkillSpec) -> Result<String> {
    let json = serde_json::to_vec(spec)?;
    let mut hasher = Sha256::new();
    hasher.update(&json);
    Ok(format!("{:x}", hasher.finalize()))
}

fn open_git_remote(remote: &RemoteConfig, ms_root: &Path) -> Result<GitArchive> {
    validate_remote_name(&remote.name)?;
    let cache_root = ms_root.join("sync").join("remotes").join(&remote.name);
    let auth = resolve_auth(remote)?;
    if !cache_root.exists() {
        if let Some(parent) = cache_root.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| MsError::Config(format!("create git cache dir: {err}")))?;
        }
        clone_remote(remote, &cache_root, &auth)?;
    }

    let repo = Repository::open(&cache_root).map_err(MsError::Git)?;
    ensure_origin_url(&repo, &remote.url)?;
    sync_git_repo(&repo, &remote.url, remote.branch.as_deref(), &auth)?;

    let archive_root = resolve_archive_root(&cache_root)?;
    GitArchive::open(archive_root)
}

fn clone_remote(remote: &RemoteConfig, path: &Path, auth: &ResolvedAuth) -> Result<()> {
    let callbacks = build_callbacks(auth)?;
    let mut fetch = git2::FetchOptions::new();
    fetch.remote_callbacks(callbacks);

    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(fetch);
    if let Some(branch) = remote.branch.as_deref() {
        builder.branch(branch);
    }
    builder.clone(&remote.url, path).map_err(MsError::Git)?;
    Ok(())
}

fn sync_git_repo(
    repo: &Repository,
    remote_url: &str,
    branch_override: Option<&str>,
    auth: &ResolvedAuth,
) -> Result<()> {
    let callbacks = build_callbacks(auth)?;
    let mut remote = repo
        .find_remote("origin")
        .or_else(|_| repo.remote_anonymous(remote_url))
        .map_err(MsError::Git)?;

    let mut fetch = git2::FetchOptions::new();
    fetch.remote_callbacks(callbacks);

    remote
        .fetch(&["refs/heads/*:refs/remotes/origin/*"], Some(&mut fetch), None)
        .map_err(MsError::Git)?;

    let branch = resolve_branch_name(repo, branch_override)?;
    let remote_ref = format!("refs/remotes/origin/{branch}");
    let remote_ref = repo
        .find_reference(&remote_ref)
        .map_err(|_| MsError::Config(format!("remote branch not found: {branch}")))?;
    let Some(target) = remote_ref.target() else {
        return Err(MsError::Config(format!(
            "remote branch has no target: {branch}"
        )));
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

    let local_ref = format!("refs/heads/{branch}");
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

fn push_git_repo(
    repo: &Repository,
    remote_url: &str,
    branch_override: Option<&str>,
    auth: &ResolvedAuth,
) -> Result<()> {
    let callbacks = build_callbacks(auth)?;
    let mut remote = repo
        .find_remote("origin")
        .or_else(|_| repo.remote_anonymous(remote_url))
        .map_err(MsError::Git)?;

    let branch = resolve_branch_name(repo, branch_override)?;
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");

    let mut push_options = git2::PushOptions::new();
    push_options.remote_callbacks(callbacks);

    remote
        .push(&[refspec], Some(&mut push_options))
        .map_err(MsError::Git)?;
    Ok(())
}

fn ensure_origin_url(repo: &Repository, url: &str) -> Result<()> {
    match repo.find_remote("origin") {
        Ok(remote) => {
            if remote.url() != Some(url) {
                repo.remote_set_url("origin", url).map_err(MsError::Git)?;
            }
        }
        Err(_) => {
            repo.remote("origin", url).map_err(MsError::Git)?;
        }
    }
    Ok(())
}

fn resolve_branch_name(repo: &Repository, branch_override: Option<&str>) -> Result<String> {
    if let Some(branch) = branch_override {
        return Ok(branch.to_string());
    }
    let head = repo.head().map_err(MsError::Git)?;
    Ok(head.shorthand().unwrap_or("main").to_string())
}

#[derive(Debug, Clone)]
enum ResolvedAuth {
    Default,
    Token { token: String, username: Option<String> },
    SshKey {
        key_path: PathBuf,
        public_key: Option<PathBuf>,
        passphrase: Option<String>,
        username: Option<String>,
    },
}

fn resolve_auth(remote: &RemoteConfig) -> Result<ResolvedAuth> {
    match remote.auth.as_ref() {
        None => Ok(ResolvedAuth::Default),
        Some(RemoteAuth::Token { token_env, username }) => {
            let token = std::env::var(token_env).map_err(|_| {
                MsError::Config(format!("missing token env var: {token_env}"))
            })?;
            Ok(ResolvedAuth::Token {
                token,
                username: username.clone(),
            })
        }
        Some(RemoteAuth::SshKey {
            key_path,
            public_key,
            passphrase_env,
        }) => {
            let passphrase = match passphrase_env {
                Some(env) => Some(
                    std::env::var(env)
                        .map_err(|_| MsError::Config(format!("missing passphrase env var: {env}")))?,
                ),
                None => None,
            };
            Ok(ResolvedAuth::SshKey {
                key_path: key_path.clone(),
                public_key: public_key.clone(),
                passphrase,
                username: None,
            })
        }
    }
}

fn build_callbacks(auth: &ResolvedAuth) -> Result<RemoteCallbacks<'static>> {
    let auth = auth.clone();
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(move |_url, username_from_url, _allowed| match &auth {
        ResolvedAuth::Default => Cred::default(),
        ResolvedAuth::Token { token, username } => {
            let user = username
                .as_deref()
                .or(username_from_url)
                .unwrap_or("x-access-token");
            Cred::userpass_plaintext(user, token)
        }
        ResolvedAuth::SshKey {
            key_path,
            public_key,
            passphrase,
            username,
        } => {
            let user = username
                .as_deref()
                .or(username_from_url)
                .unwrap_or("git");
            Cred::ssh_key(
                user,
                public_key.as_deref(),
                key_path.as_path(),
                passphrase.as_deref(),
            )
        }
    });
    Ok(callbacks)
}

fn determine_sync_status(
    local: Option<&SkillSnapshot>,
    remote: Option<&SkillSnapshot>,
    base_hash: Option<&str>,
) -> SkillSyncStatus {
    match (local, remote) {
        (Some(l), Some(r)) => {
            if l.hash == r.hash {
                return SkillSyncStatus::Synced;
            }

            if let Some(base) = base_hash {
                let local_changed = l.hash != base;
                let remote_changed = r.hash != base;

                if local_changed && remote_changed {
                    SkillSyncStatus::Conflict
                } else if local_changed {
                    SkillSyncStatus::LocalAhead
                } else if remote_changed {
                    SkillSyncStatus::RemoteAhead
                } else {
                    // Theoretically unreachable if l != r and both == base
                    SkillSyncStatus::Synced
                }
            } else {
                // No base state, but contents differ -> Conflict
                SkillSyncStatus::Conflict
            }
        }
        (Some(_), None) => SkillSyncStatus::LocalOnly,
        (None, Some(_)) => SkillSyncStatus::RemoteOnly,
        (None, None) => SkillSyncStatus::Synced,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_snap(hash: &str, seconds_ago: i64) -> SkillSnapshot {
        SkillSnapshot {
            hash: hash.to_string(),
            id: "test".to_string(),
            modified: Utc::now() - chrono::Duration::seconds(seconds_ago),
        }
    }

    #[test]
    fn test_sync_logic_synced() {
        let snap = make_snap("hash1", 10);
        assert_eq!(
            determine_sync_status(Some(&snap), Some(&snap), Some("hash1")),
            SkillSyncStatus::Synced
        );
    }

    #[test]
    fn test_sync_logic_local_ahead() {
        let local = make_snap("hash2", 0);
        let remote = make_snap("hash1", 10);
        // Base matches remote (remote hasn't changed, local has)
        assert_eq!(
            determine_sync_status(Some(&local), Some(&remote), Some("hash1")),
            SkillSyncStatus::LocalAhead
        );
    }

    #[test]
    fn test_sync_logic_remote_ahead() {
        let local = make_snap("hash1", 10);
        let remote = make_snap("hash2", 0);
        // Base matches local (local hasn't changed, remote has)
        assert_eq!(
            determine_sync_status(Some(&local), Some(&remote), Some("hash1")),
            SkillSyncStatus::RemoteAhead
        );
    }

    #[test]
    fn test_sync_logic_conflict() {
        let local = make_snap("hashA", 0);
        let remote = make_snap("hashB", 0);
        // Base matches neither (both changed independently)
        assert_eq!(
            determine_sync_status(Some(&local), Some(&remote), Some("hash1")),
            SkillSyncStatus::Conflict
        );
    }

    #[test]
    fn test_sync_logic_conflict_no_base() {
        let local = make_snap("hashA", 0);
        let remote = make_snap("hashB", 0);
        // No history, but content differs
        assert_eq!(
            determine_sync_status(Some(&local), Some(&remote), None),
            SkillSyncStatus::Conflict
        );
    }

    #[test]
    fn test_sync_logic_lww_failure_reproduction() {
        // Reproduce the LWW bug scenario from legacy logic
        let local = make_snap("hashA", 100); // Modified older
        let remote = make_snap("hashB", 50); // Modified newer
        
        // In legacy LWW logic: remote > local -> RemoteAhead.
        // This would overwrite local changes (hashA).
        
        // In new 3-way logic:
        // Case 1: We started from hash1.
        // Local changed hash1 -> hashA.
        // Remote changed hash1 -> hashB.
        // Result should be Conflict.
        assert_eq!(
            determine_sync_status(Some(&local), Some(&remote), Some("hash1")),
            SkillSyncStatus::Conflict
        );
    }
}
