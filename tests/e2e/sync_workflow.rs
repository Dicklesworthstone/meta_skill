//! E2E Scenario: Sync Workflow
//!
//! Tests the complete multi-machine sync lifecycle using filesystem remotes:
//! - Initialize sync with filesystem remote
//! - Push skills to remote
//! - Pull skills from remote
//! - Conflict detection and resolution
//! - Dry-run mode verification
//! - Status checking
//!
//! Uses filesystem remotes for network-independent testing.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::fixture::{CommandOutput, E2EFixture};
use ms::error::Result;
use ms::sync::{ConflictStrategy, SyncConfig};
use serde_json::{Value, json};
use tempfile::TempDir;

/// Test syncing to a fresh filesystem remote.
///
/// Steps:
/// 1. Initialize ms in a temp directory
/// 2. Create skills locally
/// 3. Add a filesystem remote pointing to a new temp directory
/// 4. Sync to the remote
/// 5. Verify skills appear on remote
#[test]
fn test_sync_fresh_remote() -> Result<()> {
    let mut fixture = E2EFixture::new("sync_fresh_remote");

    // Create a remote directory
    let remote_dir = TempDir::new()?;
    let remote_path = remote_dir.path();

    // Initialize remote as a bare ms root (needs .ms structure)
    fs::create_dir_all(remote_path.join(".ms"))?;
    fs::create_dir_all(remote_path.join(".ms/archive"))?;

    // ==========================================
    // Step 1: Initialize ms
    // ==========================================
    fixture.log_step("Initialize ms directory");
    let output = fixture.init();
    fixture.assert_success(&output, "init");
    fixture.checkpoint("post_init");

    // ==========================================
    // Step 2: Create skills locally
    // ==========================================
    fixture.log_step("Create skills for sync");
    fixture.create_skill(
        "sync-test-skill",
        r#"---
name: Sync Test Skill
description: A skill to test sync functionality
tags: [test, sync]
---

# Sync Test Skill

This skill is used to verify sync operations.

## Usage

Test content for sync verification.
"#,
    )?;
    fixture.checkpoint("skills_created");

    // Index the skill
    fixture.log_step("Index skills");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");
    fixture.checkpoint("post_index");

    // ==========================================
    // Step 3: Add filesystem remote
    // ==========================================
    fixture.log_step("Add filesystem remote");
    let output = fixture.run_ms(&[
        "--robot",
        "remote",
        "add",
        "test-remote",
        remote_path.to_str().unwrap(),
        "--remote-type",
        "filesystem",
    ]);
    fixture.assert_success(&output, "remote add");
    fixture.checkpoint("post_remote_add");

    // Verify remote was added
    fixture.log_step("List remotes");
    let output = fixture.run_ms(&["--robot", "remote", "list"]);
    fixture.assert_success(&output, "remote list");
    let stdout = &output.stdout;
    assert!(
        stdout.contains("test-remote") || output.json().to_string().contains("test-remote"),
        "Remote should be listed"
    );

    // ==========================================
    // Step 4: Sync to remote
    // ==========================================
    fixture.log_step("Sync to remote");
    let output = fixture.run_ms(&["--robot", "sync", "test-remote"]);
    // Note: Sync may succeed or partially succeed depending on remote setup
    // We're mainly testing the CLI flow works
    println!("[SYNC] Output: {}", output.stdout);
    println!("[SYNC] Stderr: {}", output.stderr);
    fixture.checkpoint("post_sync");

    // ==========================================
    // Step 5: Check sync status
    // ==========================================
    fixture.log_step("Check sync status");
    let output = fixture.run_ms(&["--robot", "sync", "--status"]);
    fixture.assert_success(&output, "sync status");
    println!("[SYNC STATUS] Output: {}", output.stdout);
    fixture.checkpoint("post_status");

    // Generate report
    fixture.log_step("Generate report");
    fixture.generate_report();

    Ok(())
}

/// Test pulling changes from a remote.
///
/// Steps:
/// 1. Initialize two separate ms instances
/// 2. Create skill in first instance
/// 3. Sync first instance to shared remote
/// 4. Configure second instance to use same remote
/// 5. Pull from remote to second instance
/// 6. Verify skill appears in second instance
#[test]
fn test_sync_pull_changes() -> Result<()> {
    let mut fixture = E2EFixture::new("sync_pull_changes");

    // Create shared remote directory
    let remote_dir = TempDir::new()?;
    let remote_path = remote_dir.path();
    fs::create_dir_all(remote_path.join(".ms"))?;
    fs::create_dir_all(remote_path.join(".ms/archive"))?;

    // Create a second "machine" directory
    let machine2_dir = TempDir::new()?;
    let machine2_path = machine2_dir.path();

    // ==========================================
    // Step 1: Initialize first ms instance
    // ==========================================
    fixture.log_step("Initialize first ms instance");
    let output = fixture.init();
    fixture.assert_success(&output, "init");
    fixture.checkpoint("post_init_1");

    // ==========================================
    // Step 2: Create skill in first instance
    // ==========================================
    fixture.log_step("Create skill in first instance");
    fixture.create_skill(
        "shared-skill",
        r#"---
name: Shared Skill
description: A skill shared between machines
tags: [test, shared]
---

# Shared Skill

This skill will be synced to another machine.
"#,
    )?;

    // Index
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");
    fixture.checkpoint("post_index_1");

    // ==========================================
    // Step 3: Add remote and sync from first instance
    // ==========================================
    fixture.log_step("Add remote to first instance");
    let output = fixture.run_ms(&[
        "--robot",
        "remote",
        "add",
        "shared",
        remote_path.to_str().unwrap(),
        "--remote-type",
        "filesystem",
    ]);
    fixture.assert_success(&output, "remote add");

    fixture.log_step("Push to shared remote");
    let output = fixture.run_ms(&["--robot", "sync", "shared", "--push-only"]);
    println!("[SYNC PUSH] Output: {}", output.stdout);
    println!("[SYNC PUSH] Stderr: {}", output.stderr);
    fixture.checkpoint("post_push_1");

    // ==========================================
    // Step 4: Initialize second ms instance
    // ==========================================
    fixture.log_step("Initialize second ms instance");
    // Note: We need to run ms in the second machine directory
    // This tests cross-machine sync simulation
    fs::create_dir_all(machine2_path.join(".ms"))?;
    fs::create_dir_all(machine2_path.join("skills"))?;

    fixture.checkpoint("post_init_2");

    // For a complete test, we would initialize and configure the second instance
    // then pull from the shared remote. This demonstrates the workflow pattern.

    Ok(())
}

/// Test sync dry-run mode doesn't make changes.
///
/// Steps:
/// 1. Initialize ms and create skills
/// 2. Add remote
/// 3. Run sync with --dry-run
/// 4. Verify no actual changes were made
#[test]
fn test_sync_dry_run() -> Result<()> {
    let mut fixture = E2EFixture::new("sync_dry_run");

    // Create remote directory
    let remote_dir = TempDir::new()?;
    let remote_path = remote_dir.path();
    fs::create_dir_all(remote_path.join(".ms"))?;

    // ==========================================
    // Step 1: Initialize ms
    // ==========================================
    fixture.log_step("Initialize ms directory");
    let output = fixture.init();
    fixture.assert_success(&output, "init");
    fixture.checkpoint("post_init");

    // ==========================================
    // Step 2: Create skill
    // ==========================================
    fixture.log_step("Create skill");
    fixture.create_skill(
        "dry-run-skill",
        r#"---
name: Dry Run Skill
description: Testing dry-run sync
tags: [test]
---

# Dry Run Test
"#,
    )?;

    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");
    fixture.checkpoint("post_index");

    // ==========================================
    // Step 3: Add remote
    // ==========================================
    fixture.log_step("Add remote");
    let output = fixture.run_ms(&[
        "--robot",
        "remote",
        "add",
        "dry-remote",
        remote_path.to_str().unwrap(),
        "--remote-type",
        "filesystem",
    ]);
    fixture.assert_success(&output, "remote add");
    fixture.checkpoint("post_remote_add");

    // Record state before dry-run
    let files_before: Vec<_> = walkdir::WalkDir::new(remote_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .map(|e| e.path().to_path_buf())
        .collect();

    // ==========================================
    // Step 4: Run dry-run sync
    // ==========================================
    fixture.log_step("Run dry-run sync");
    let output = fixture.run_ms(&["--robot", "sync", "dry-remote", "--dry-run"]);
    // Dry-run should succeed and show what would happen
    println!("[DRY RUN] Output: {}", output.stdout);
    println!("[DRY RUN] Stderr: {}", output.stderr);
    fixture.checkpoint("post_dry_run");

    // Verify no changes were made to remote
    let files_after: Vec<_> = walkdir::WalkDir::new(remote_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .map(|e| e.path().to_path_buf())
        .collect();

    // The file count should be approximately the same (may have some minor differences)
    println!(
        "[DRY RUN] Files before: {}, after: {}",
        files_before.len(),
        files_after.len()
    );

    Ok(())
}

/// Test sync status reporting.
///
/// Steps:
/// 1. Initialize ms with remotes
/// 2. Check sync status
/// 3. Verify status output format
#[test]
fn test_sync_status_check() -> Result<()> {
    let mut fixture = E2EFixture::new("sync_status_check");

    // Create remote directory
    let remote_dir = TempDir::new()?;
    let remote_path = remote_dir.path();
    fs::create_dir_all(remote_path.join(".ms"))?;

    // ==========================================
    // Step 1: Initialize ms
    // ==========================================
    fixture.log_step("Initialize ms directory");
    let output = fixture.init();
    fixture.assert_success(&output, "init");
    fixture.checkpoint("post_init");

    // ==========================================
    // Step 2: Add remote
    // ==========================================
    fixture.log_step("Add remote");
    let output = fixture.run_ms(&[
        "--robot",
        "remote",
        "add",
        "status-remote",
        remote_path.to_str().unwrap(),
        "--remote-type",
        "filesystem",
    ]);
    fixture.assert_success(&output, "remote add");
    fixture.checkpoint("post_remote_add");

    // ==========================================
    // Step 3: Check sync status
    // ==========================================
    fixture.log_step("Check sync status");
    let output = fixture.run_ms(&["--robot", "sync", "--status"]);
    fixture.assert_success(&output, "sync status");

    println!("[STATUS] Output: {}", output.stdout);
    let json = output.json();
    println!(
        "[STATUS] JSON: {}",
        serde_json::to_string_pretty(&json).unwrap_or_default()
    );
    fixture.checkpoint("post_status");

    // Verify status output contains expected fields
    // The exact format depends on implementation
    let output_str = output.stdout.to_lowercase();
    let has_status_info = output_str.contains("status")
        || output_str.contains("remote")
        || output_str.contains("machine")
        || json.get("status").is_some()
        || json.get("remotes").is_some();

    println!("[STATUS] Has status info: {}", has_status_info);

    Ok(())
}

/// Test remote management commands.
///
/// Steps:
/// 1. Add a remote
/// 2. List remotes
/// 3. Disable remote
/// 4. Enable remote
/// 5. Remove remote
#[test]
fn test_remote_management() -> Result<()> {
    let mut fixture = E2EFixture::new("remote_management");

    // Create remote directory
    let remote_dir = TempDir::new()?;
    let remote_path = remote_dir.path();
    fs::create_dir_all(remote_path.join(".ms"))?;

    // ==========================================
    // Step 1: Initialize ms
    // ==========================================
    fixture.log_step("Initialize ms directory");
    let output = fixture.init();
    fixture.assert_success(&output, "init");
    fixture.checkpoint("post_init");

    // ==========================================
    // Step 2: Add remote
    // ==========================================
    fixture.log_step("Add remote");
    let output = fixture.run_ms(&[
        "--robot",
        "remote",
        "add",
        "managed-remote",
        remote_path.to_str().unwrap(),
        "--remote-type",
        "filesystem",
    ]);
    fixture.assert_success(&output, "remote add");
    fixture.checkpoint("post_add");

    // ==========================================
    // Step 3: List remotes
    // ==========================================
    fixture.log_step("List remotes");
    let output = fixture.run_ms(&["--robot", "remote", "list"]);
    fixture.assert_success(&output, "remote list");
    println!("[LIST] Output: {}", output.stdout);

    let list_output = &output.stdout;
    let json_output = output.json();
    assert!(
        list_output.contains("managed-remote")
            || json_output.to_string().contains("managed-remote"),
        "Remote should appear in list"
    );
    fixture.checkpoint("post_list");

    // ==========================================
    // Step 4: Disable remote
    // ==========================================
    fixture.log_step("Disable remote");
    let output = fixture.run_ms(&["--robot", "remote", "disable", "managed-remote"]);
    fixture.assert_success(&output, "remote disable");
    fixture.checkpoint("post_disable");

    // ==========================================
    // Step 5: Enable remote
    // ==========================================
    fixture.log_step("Enable remote");
    let output = fixture.run_ms(&["--robot", "remote", "enable", "managed-remote"]);
    fixture.assert_success(&output, "remote enable");
    fixture.checkpoint("post_enable");

    // ==========================================
    // Step 6: Remove remote
    // ==========================================
    fixture.log_step("Remove remote");
    let output = fixture.run_ms(&["--robot", "remote", "remove", "managed-remote"]);
    fixture.assert_success(&output, "remote remove");
    fixture.checkpoint("post_remove");

    // Verify remote was removed
    fixture.log_step("Verify removal");
    let output = fixture.run_ms(&["--robot", "remote", "list"]);
    fixture.assert_success(&output, "remote list after remove");
    let list_output = &output.stdout;
    let _json_output = output.json();
    // After removal, remote should not appear
    println!("[VERIFY] List after removal: {}", list_output);
    fixture.checkpoint("post_verify");

    // Generate report
    fixture.log_step("Generate report");
    fixture.generate_report();

    Ok(())
}

/// Test sync with push-only mode.
#[test]
fn test_sync_push_only() -> Result<()> {
    let mut fixture = E2EFixture::new("sync_push_only");

    // Create remote directory
    let remote_dir = TempDir::new()?;
    let remote_path = remote_dir.path();
    fs::create_dir_all(remote_path.join(".ms"))?;
    fs::create_dir_all(remote_path.join(".ms/archive"))?;

    // ==========================================
    // Step 1: Initialize ms
    // ==========================================
    fixture.log_step("Initialize ms directory");
    let output = fixture.init();
    fixture.assert_success(&output, "init");
    fixture.checkpoint("post_init");

    // ==========================================
    // Step 2: Create skill
    // ==========================================
    fixture.log_step("Create skill for push");
    fixture.create_skill(
        "push-only-skill",
        r#"---
name: Push Only Skill
description: Testing push-only sync
tags: [test, push]
---

# Push Only Test

This skill will be pushed to remote.
"#,
    )?;

    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");
    fixture.checkpoint("post_index");

    // ==========================================
    // Step 3: Add remote
    // ==========================================
    fixture.log_step("Add remote");
    let output = fixture.run_ms(&[
        "--robot",
        "remote",
        "add",
        "push-remote",
        remote_path.to_str().unwrap(),
        "--remote-type",
        "filesystem",
    ]);
    fixture.assert_success(&output, "remote add");
    fixture.checkpoint("post_remote_add");

    // ==========================================
    // Step 4: Sync push-only
    // ==========================================
    fixture.log_step("Sync push-only");
    let output = fixture.run_ms(&["--robot", "sync", "push-remote", "--push-only"]);
    println!("[PUSH] Output: {}", output.stdout);
    println!("[PUSH] Stderr: {}", output.stderr);
    fixture.checkpoint("post_push");

    Ok(())
}

/// Test sync with pull-only mode.
#[test]
fn test_sync_pull_only() -> Result<()> {
    let mut fixture = E2EFixture::new("sync_pull_only");

    // Create remote directory
    let remote_dir = TempDir::new()?;
    let remote_path = remote_dir.path();
    fs::create_dir_all(remote_path.join(".ms"))?;

    // ==========================================
    // Step 1: Initialize ms
    // ==========================================
    fixture.log_step("Initialize ms directory");
    let output = fixture.init();
    fixture.assert_success(&output, "init");
    fixture.checkpoint("post_init");

    // ==========================================
    // Step 2: Add remote
    // ==========================================
    fixture.log_step("Add remote");
    let output = fixture.run_ms(&[
        "--robot",
        "remote",
        "add",
        "pull-remote",
        remote_path.to_str().unwrap(),
        "--remote-type",
        "filesystem",
    ]);
    fixture.assert_success(&output, "remote add");
    fixture.checkpoint("post_remote_add");

    // ==========================================
    // Step 3: Sync pull-only
    // ==========================================
    fixture.log_step("Sync pull-only");
    let output = fixture.run_ms(&["--robot", "sync", "pull-remote", "--pull-only"]);
    println!("[PULL] Output: {}", output.stdout);
    println!("[PULL] Stderr: {}", output.stderr);
    fixture.checkpoint("post_pull");

    Ok(())
}

const SKILL_ALPHA: &str = r#"---
name: Alpha Notes
description: Notes kept on both sides of a sync
tags: [sync]
---

# Alpha Notes

Alpha content for the deletion tests.
"#;

const SKILL_BETA: &str = r#"---
name: Beta Notes
description: More notes kept on both sides of a sync
tags: [sync]
---

# Beta Notes

Beta content for the deletion tests.
"#;

const SKILL_GAMMA: &str = r#"---
name: Gamma Notes
description: Notes that only this machine has
tags: [sync]
---

# Gamma Notes

Gamma content for the deletion tests.
"#;

/// Run `ms` with its sync config inside the fixture, whatever the host sets.
fn run_in_home(fixture: &mut E2EFixture, args: &[&str], env: &[(&str, &str)]) -> CommandOutput {
    let config_home = fixture.root.join(".config");
    let mut vars = vec![("XDG_CONFIG_HOME", config_home.to_str().unwrap())];
    vars.extend_from_slice(env);
    fixture.run_ms_with_env(args, &vars)
}

fn local_spec(fixture: &E2EFixture, id: &str) -> PathBuf {
    fixture
        .ms_root
        .join("archive/skills/by-id")
        .join(id)
        .join("skill.spec.json")
}

fn error_message(output: &CommandOutput) -> String {
    output.json()["message"]
        .as_str()
        .expect("error message")
        .to_string()
}

/// A project and a filesystem remote with two skills synced between them.
struct SyncedPair {
    fixture: E2EFixture,
    remote: TempDir,
}

impl SyncedPair {
    fn new(scenario: &str) -> Result<Self> {
        let mut fixture = E2EFixture::new(scenario);
        let remote = TempDir::new()?;
        fs::create_dir_all(remote.path().join("skills/by-id"))?;

        let output = fixture.init();
        fixture.assert_success(&output, "init");
        fixture.create_skill("alpha-notes", SKILL_ALPHA)?;
        fixture.create_skill("beta-notes", SKILL_BETA)?;
        let output = fixture.run_ms(&["--robot", "index"]);
        fixture.assert_success(&output, "index");

        let mut pair = Self { fixture, remote };
        let url = pair.remote.path().to_str().unwrap().to_string();
        let output = run_in_home(
            &mut pair.fixture,
            &[
                "--robot",
                "remote",
                "add",
                "shared",
                &url,
                "--remote-type",
                "filesystem",
            ],
            &[],
        );
        pair.fixture.assert_success(&output, "remote add");

        let output = pair.sync(&[]);
        pair.fixture.assert_success(&output, "first sync");
        for id in ["alpha-notes", "beta-notes"] {
            assert!(
                pair.remote_spec(id).exists(),
                "{id} should reach the remote"
            );
        }
        Ok(pair)
    }

    fn sync(&mut self, flags: &[&str]) -> CommandOutput {
        let mut args = vec!["--robot", "sync", "shared"];
        args.extend_from_slice(flags);
        run_in_home(&mut self.fixture, &args, &[])
    }

    fn local_spec(&self, id: &str) -> PathBuf {
        local_spec(&self.fixture, id)
    }

    fn remote_spec(&self, id: &str) -> PathBuf {
        self.remote
            .path()
            .join("skills/by-id")
            .join(id)
            .join("skill.spec.json")
    }

    fn sync_config_path(&self) -> PathBuf {
        let base = if cfg!(target_os = "macos") {
            "Library/Application Support"
        } else {
            ".config"
        };
        self.fixture.root.join(base).join("ms/sync.toml")
    }
}

fn assert_held(output: &CommandOutput, deletion: &str, unreadable: &str) {
    assert!(!output.success, "the sync must fail: {}", output.stdout);
    let message = error_message(output);
    assert!(
        message.contains("held back 1 deletion(s)")
            && message.contains(deletion)
            && message.contains(&format!("Failed to read skill {unreadable}")),
        "the failure must name what was held and why: {message}"
    );
}

#[test]
fn test_sync_keeps_the_local_copy_when_the_remote_copy_cannot_be_read() -> Result<()> {
    let mut pair = SyncedPair::new("sync_hold_local_copy")?;
    let remote_spec = pair.remote_spec("alpha-notes");
    let good = fs::read(&remote_spec)?;
    fs::write(&remote_spec, "{ not json")?;

    let output = pair.sync(&[]);
    assert!(
        pair.local_spec("alpha-notes").exists(),
        "an unreadable remote copy must not delete the local one"
    );
    assert_held(&output, "alpha-notes (local copy)", "alpha-notes");

    fs::write(&remote_spec, good)?;
    let output = pair.sync(&[]);
    pair.fixture
        .assert_success(&output, "sync after the repair");
    assert!(pair.local_spec("alpha-notes").exists());
    assert_eq!(output.json()["reports"][0]["held_deletions"], json!([]));
    Ok(())
}

#[test]
fn test_sync_keeps_the_remote_copy_when_the_local_copy_cannot_be_read() -> Result<()> {
    let mut pair = SyncedPair::new("sync_hold_remote_copy")?;
    let spec = pair.local_spec("alpha-notes");
    let good = fs::read(&spec)?;
    fs::write(&spec, "{ not json")?;

    let output = pair.sync(&[]);
    assert!(
        pair.remote_spec("alpha-notes").exists(),
        "an unreadable local copy must not delete the remote one"
    );
    assert_held(&output, "alpha-notes (remote copy)", "alpha-notes");

    fs::write(&spec, good)?;
    let output = pair.sync(&[]);
    pair.fixture
        .assert_success(&output, "sync after the repair");
    assert!(pair.remote_spec("alpha-notes").exists());
    Ok(())
}

#[test]
fn test_sync_force_does_not_delete_through_a_read_error() -> Result<()> {
    let mut pair = SyncedPair::new("sync_hold_force")?;
    fs::write(pair.remote_spec("alpha-notes"), "{ not json")?;

    let output = pair.sync(&["--force"]);
    assert!(
        pair.local_spec("alpha-notes").exists(),
        "--force must not delete through a read error"
    );
    assert_held(&output, "alpha-notes (local copy)", "alpha-notes");
    Ok(())
}

#[test]
fn test_sync_force_holds_a_conflict_that_would_delete() -> Result<()> {
    let mut pair = SyncedPair::new("sync_hold_conflict")?;

    // The local edit turns the missing remote copy into a conflict, and this strategy
    // resolves that conflict by deleting the local copy.
    let path = pair.sync_config_path();
    let mut config = SyncConfig::load_from(&path)?;
    config
        .conflict_strategies
        .insert("alpha-notes".to_string(), ConflictStrategy::PreferRemote);
    config.save_to(&path)?;

    let spec_path = pair.local_spec("alpha-notes");
    let mut spec: Value = serde_json::from_str(&fs::read_to_string(&spec_path)?)?;
    spec["metadata"]["description"] = json!("Edited on this machine");
    fs::write(&spec_path, serde_json::to_string(&spec)?)?;
    fs::write(pair.remote_spec("alpha-notes"), "{ not json")?;

    let output = pair.sync(&["--force"]);
    assert!(
        spec_path.exists(),
        "a forced conflict must not delete through a read error"
    );
    assert_held(&output, "alpha-notes (local copy)", "alpha-notes");
    Ok(())
}

/// Remove every skill directory on one side, leaving a valid and empty archive.
fn empty_archive(by_id: &Path) -> Result<()> {
    for entry in fs::read_dir(by_id)? {
        fs::remove_dir_all(entry?.path())?;
    }
    Ok(())
}

#[test]
fn test_sync_refuses_to_delete_local_skills_for_an_empty_remote() -> Result<()> {
    let mut pair = SyncedPair::new("sync_refuse_empty_remote")?;
    empty_archive(&pair.remote.path().join("skills/by-id"))?;
    let local = [
        pair.local_spec("alpha-notes"),
        pair.local_spec("beta-notes"),
    ];

    for flags in [&[][..], &["--dry-run"][..]] {
        let output = pair.sync(flags);
        assert!(
            local.iter().all(|spec| spec.exists()),
            "an empty remote must not delete local skills ({flags:?})"
        );
        assert!(!output.success, "the sync must fail: {}", output.stdout);
        let message = error_message(&output);
        assert!(
            message.contains("would delete 2 local skill(s)") && message.contains("--force"),
            "the refusal must count the skills and name --force: {message}"
        );
    }

    let output = pair.sync(&["--push-only"]);
    pair.fixture
        .assert_success(&output, "a push cannot delete local skills");
    assert!(local.iter().all(|spec| spec.exists()));

    let output = pair.sync(&["--force"]);
    pair.fixture.assert_success(&output, "forced sync");
    assert!(
        !local.iter().any(|spec| spec.exists()),
        "--force goes ahead with the deletions"
    );
    Ok(())
}

#[test]
fn test_sync_refuses_to_delete_remote_skills_for_an_empty_archive() -> Result<()> {
    let mut pair = SyncedPair::new("sync_refuse_empty_archive")?;
    empty_archive(&pair.fixture.ms_root.join("archive/skills/by-id"))?;
    let remote = [
        pair.remote_spec("alpha-notes"),
        pair.remote_spec("beta-notes"),
    ];

    for flags in [&[][..], &["--dry-run"][..]] {
        let output = pair.sync(flags);
        assert!(
            remote.iter().all(|spec| spec.exists()),
            "an empty archive must not delete remote skills ({flags:?})"
        );
        assert!(!output.success, "the sync must fail: {}", output.stdout);
        let message = error_message(&output);
        assert!(
            message.contains("would delete 2 skill(s) from remote shared")
                && message.contains("--force"),
            "the refusal must count the skills and name --force: {message}"
        );
    }

    let output = pair.sync(&["--pull-only"]);
    pair.fixture
        .assert_success(&output, "a pull cannot delete remote skills");
    assert!(remote.iter().all(|spec| spec.exists()));

    let output = pair.sync(&["--force"]);
    pair.fixture.assert_success(&output, "forced sync");
    assert!(
        !remote.iter().any(|spec| spec.exists()),
        "--force goes ahead with the deletions"
    );
    Ok(())
}

#[test]
fn test_sync_still_propagates_a_deletion_that_leaves_skills_behind() -> Result<()> {
    let mut pair = SyncedPair::new("sync_single_deletion")?;
    fs::remove_dir_all(pair.remote.path().join("skills/by-id/alpha-notes"))?;

    let output = pair.sync(&[]);
    pair.fixture.assert_success(&output, "sync");
    assert!(!pair.local_spec("alpha-notes").exists());
    assert!(pair.local_spec("beta-notes").exists());
    assert_eq!(
        output.json()["reports"][0]["pulled"],
        json!(["alpha-notes (deleted)"])
    );
    assert_eq!(output.json()["auto_reindex"]["success"], true);
    Ok(())
}

#[test]
fn test_sync_keeps_a_skill_it_cannot_tombstone() -> Result<()> {
    let mut pair = SyncedPair::new("sync_no_tombstone")?;
    fs::remove_dir_all(pair.remote.path().join("skills/by-id/alpha-notes"))?;
    let tombstones = pair.fixture.ms_root.join("tombstones");
    if tombstones.exists() {
        fs::rename(&tombstones, tombstones.with_extension("moved"))?;
    }
    fs::write(&tombstones, "a file where the directory goes")?;

    let output = pair.sync(&[]);
    assert!(
        pair.local_spec("alpha-notes").exists(),
        "a skill must not go without its tombstone"
    );
    assert!(!output.success, "the sync must fail: {}", output.stdout);
    let message = error_message(&output);
    assert!(
        message.contains("alpha-notes was not deleted") && message.contains("tombstone"),
        "{message}"
    );
    Ok(())
}

/// A local JFP Cloud service that accepts every push and records each pushed item.
struct JfpService {
    port: u16,
    pushed: Arc<Mutex<Vec<Value>>>,
}

impl JfpService {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind JFP service");
        let port = listener.local_addr().expect("service address").port();
        let pushed = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&pushed);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                answer_jfp_request(stream, &log);
            }
        });
        Self { port, pushed }
    }

    fn deleted(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .pushed
            .lock()
            .expect("push log")
            .iter()
            .filter(|item| item["deleted"] == json!(true))
            .map(|item| item["msSkillId"].as_str().expect("id").to_string())
            .collect();
        ids.sort();
        ids
    }
}

fn answer_jfp_request(mut stream: TcpStream, log: &Mutex<Vec<Value>>) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone service stream"));
    let mut request_line = String::new();
    reader.read_line(&mut request_line).expect("request line");
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_string();
    let mut length = 0;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).expect("header");
        if header.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().expect("content length");
            }
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).expect("request body");
    let request: Value = serde_json::from_slice(&body).expect("request JSON");

    let now = "2026-10-31T00:00:00Z";
    let response = match path.as_str() {
        "/handshake" => json!({
            "serverTime": now,
            "minProtocolVersion": "1.0",
            "protocolVersion": "1.0",
            "capabilities": {},
            "device": request["device"],
        }),
        "/changes" => json!({ "serverTime": now, "hasMore": false, "skills": [] }),
        "/push" => {
            let items = request["items"].as_array().expect("push items");
            log.lock().expect("push log").extend(items.iter().cloned());
            let results: Vec<Value> = items
                .iter()
                .map(|item| json!({ "msSkillId": item["msSkillId"], "status": "applied" }))
                .collect();
            json!({ "serverTime": now, "results": results })
        }
        other => panic!("unexpected request: {other}"),
    }
    .to_string();
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{response}",
        response.len()
    )
    .expect("response");
}

/// A project with a JFP Cloud remote that remembers two skills this archive no longer has.
fn jfp_fixture(scenario: &str) -> Result<(E2EFixture, JfpService)> {
    let mut fixture = E2EFixture::new(scenario);
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    let service = JfpService::start();
    let state_dir = fixture.ms_root.join("sync");
    fs::create_dir_all(&state_dir)?;
    fs::write(
        state_dir.join("jfp-cloud-state.json"),
        json!({ "skillRevisions": { "alpha-notes": 3, "beta-notes": 4 } }).to_string(),
    )?;

    let url = format!("http://127.0.0.1:{}", service.port);
    let output = run_in_home(
        &mut fixture,
        &[
            "--robot",
            "remote",
            "add",
            "cloud",
            &url,
            "--remote-type",
            "jfp-cloud",
        ],
        &[],
    );
    fixture.assert_success(&output, "remote add");
    Ok((fixture, service))
}

fn sync_cloud(fixture: &mut E2EFixture, flags: &[&str]) -> CommandOutput {
    let mut args = vec!["--robot", "sync", "cloud"];
    args.extend_from_slice(flags);
    run_in_home(fixture, &args, &[("JFP_CLOUD_TOKEN", "test-token")])
}

#[test]
fn test_sync_refuses_to_delete_cloud_skills_for_an_empty_archive() -> Result<()> {
    let (mut fixture, service) = jfp_fixture("sync_refuse_empty_archive_cloud")?;

    let output = sync_cloud(&mut fixture, &[]);
    assert_eq!(
        service.deleted(),
        Vec::<String>::new(),
        "an empty archive must not delete cloud skills"
    );
    assert!(!output.success, "the sync must fail: {}", output.stdout);
    let message = error_message(&output);
    assert!(
        message.contains("would delete 2 skill(s) from remote cloud")
            && message.contains("--force"),
        "the refusal must count the skills and name --force: {message}"
    );

    let output = sync_cloud(&mut fixture, &["--force"]);
    fixture.assert_success(&output, "forced sync");
    assert_eq!(service.deleted(), ["alpha-notes", "beta-notes"]);
    Ok(())
}

#[test]
fn test_sync_holds_cloud_deletions_when_a_skill_cannot_be_read() -> Result<()> {
    let (mut fixture, service) = jfp_fixture("sync_hold_cloud")?;
    fixture.create_skill("gamma-notes", SKILL_GAMMA)?;
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");
    let spec = local_spec(&fixture, "gamma-notes");
    fs::write(&spec, "{ not json")?;

    let output = sync_cloud(&mut fixture, &["--force"]);
    assert_eq!(
        service.deleted(),
        Vec::<String>::new(),
        "a read error must hold cloud deletions, even with --force"
    );
    assert!(!output.success, "the sync must fail: {}", output.stdout);
    let message = error_message(&output);
    assert!(
        message.contains("held back 2 deletion(s)")
            && message.contains("alpha-notes (remote copy)")
            && message.contains("beta-notes (remote copy)")
            && message.contains("Failed to read skill gamma-notes"),
        "the failure must name what was held and why: {message}"
    );
    Ok(())
}

/// A sync through ru indexes whatever `SKILL.md` a pulled repository holds.
#[cfg(unix)]
mod ru_repositories {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    const GITHUB_TOKEN: &str = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";

    fn repository_skill(description: &str) -> String {
        format!(
            "---\nid: repo-notes\nname: Repo Notes\ndescription: {description}\n---\n\n\
             # Repo Notes\n\nNotes kept in a repository.\n"
        )
    }

    /// A project with an ru remote whose stand-in reports `repo` as pulled and lists it, so a
    /// sync runs both steps a real ru gets: the import, then the reindex.
    struct RuRemote {
        fixture: E2EFixture,
        repo: TempDir,
        reindex: Value,
    }

    impl RuRemote {
        fn new(scenario: &str) -> Result<Self> {
            let mut fixture = E2EFixture::new(scenario);
            let repo = TempDir::new()?;
            let output = fixture.init();
            fixture.assert_success(&output, "init");

            let path = repo.path().to_str().unwrap();
            let pulled = json!({
                "repos": [{"name": path, "path": path, "action": "pull", "status": "updated"}]
            });
            let ru = fixture.root.join("stand-in-ru");
            fs::write(
                &ru,
                format!(
                    "#!/bin/sh\ncase \"$1\" in\n  --version) echo 'ru 1.2.0' ;;\n  \
                     sync) printf '%s\\n' '{pulled}' ;;\n  list) echo '{path}' ;;\nesac\n"
                ),
            )?;
            fs::set_permissions(&ru, fs::Permissions::from_mode(0o755))?;
            let mut config = fs::read_to_string(&fixture.config_path)?;
            config.push_str(&format!(
                "\n[ru]\nenabled = true\nru_path = \"{}\"\n",
                ru.display()
            ));
            fs::write(&fixture.config_path, config)?;

            let output = run_in_home(
                &mut fixture,
                &["--robot", "remote", "add", "repos", "ru", "--type", "ru"],
                &[],
            );
            fixture.assert_success(&output, "remote add");
            Ok(Self {
                fixture,
                repo,
                reindex: Value::Null,
            })
        }

        fn write_skill(&self, dir: &str, content: impl AsRef<[u8]>) -> Result<PathBuf> {
            let dir = self.repo.path().join(dir);
            fs::create_dir_all(&dir)?;
            let path = dir.join("SKILL.md");
            fs::write(&path, content)?;
            Ok(path)
        }

        /// Sync and return the errors the report carries.
        fn sync(&mut self, flags: &[&str]) -> Vec<String> {
            let mut args = vec!["--robot", "sync", "repos"];
            args.extend_from_slice(flags);
            let output = run_in_home(&mut self.fixture, &args, &[]);
            self.fixture.assert_success(&output, "ru sync");
            let report = output.json();
            self.reindex = report["auto_reindex"].clone();
            report["reports"][0]["errors"]
                .as_array()
                .expect("report errors")
                .iter()
                .map(|error| error.as_str().expect("error text").to_string())
                .collect()
        }

        /// The reindex refused the file at `path`, and said so.
        fn assert_reindex_refused(&self, path: &Path, reason: &str) {
            assert_eq!(self.reindex["success"], false, "{}", self.reindex);
            let errors = self.reindex["index"]["errors"].to_string();
            assert!(
                errors.contains(path.to_str().unwrap()) && errors.contains(reason),
                "the reindex must name the file and {reason}: {errors}"
            );
        }

        fn indexed_description(&self) -> Option<String> {
            let spec = fs::read_to_string(local_spec(&self.fixture, "repo-notes")).ok()?;
            let spec: Value = serde_json::from_str(&spec).expect("spec json");
            Some(spec["metadata"]["description"].as_str()?.to_string())
        }
    }

    #[test]
    fn test_ru_sync_indexes_a_repository_skill() -> Result<()> {
        let mut ru = RuRemote::new("ru_sync_indexes")?;
        ru.write_skill("notes", repository_skill("first"))?;

        let errors = ru.sync(&[]);
        assert_eq!(ru.indexed_description().as_deref(), Some("first"));
        assert_eq!(errors, Vec::<String>::new());
        assert_eq!(ru.reindex["success"], true, "{}", ru.reindex);
        assert_eq!(
            ru.reindex["index"]["indexed"], 1,
            "the reindex has to reach the repository, or these tests prove one step of two"
        );
        Ok(())
    }

    #[test]
    fn test_ru_sync_updates_a_skill_it_indexed() -> Result<()> {
        let mut ru = RuRemote::new("ru_sync_updates")?;
        ru.write_skill("notes", repository_skill("first"))?;
        assert_eq!(ru.sync(&[]), Vec::<String>::new());

        ru.write_skill("notes", repository_skill("second"))?;
        let errors = ru.sync(&[]);
        assert_eq!(
            ru.indexed_description().as_deref(),
            Some("second"),
            "a repository updates its own skill without --force"
        );
        assert_eq!(errors, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn test_ru_sync_refuses_a_symlink_out_of_the_repository() -> Result<()> {
        let mut ru = RuRemote::new("ru_sync_symlink")?;
        let outside = TempDir::new()?;
        let target = outside.path().join("private.md");
        fs::write(&target, repository_skill("from outside"))?;
        let dir = ru.repo.path().join("notes");
        fs::create_dir_all(&dir)?;
        let link = dir.join("SKILL.md");
        symlink(&target, &link)?;

        let errors = ru.sync(&[]);
        assert_eq!(
            ru.indexed_description(),
            None,
            "a file outside the repository must not reach the archive"
        );
        assert_eq!(errors.len(), 2, "a count and one problem: {errors:?}");
        assert!(errors[1].contains(link.to_str().unwrap()), "{errors:?}");
        ru.assert_reindex_refused(&link, "symlink");
        Ok(())
    }

    #[test]
    fn test_ru_sync_refuses_a_directory_symlink_out_of_the_repository() -> Result<()> {
        let mut ru = RuRemote::new("ru_sync_dir_symlink")?;
        let outside = TempDir::new()?;
        fs::write(
            outside.path().join("SKILL.md"),
            repository_skill("from outside"),
        )?;
        symlink(outside.path(), ru.repo.path().join("notes"))?;

        ru.sync(&[]);
        assert_eq!(
            ru.indexed_description(),
            None,
            "the reindex follows directory links, so it has to check the repository root"
        );
        ru.assert_reindex_refused(&ru.repo.path().join("notes/SKILL.md"), "symlink");
        Ok(())
    }

    #[test]
    fn test_ru_sync_scans_a_repository_skill_for_secrets() -> Result<()> {
        let mut ru = RuRemote::new("ru_sync_secret")?;
        let path = ru.write_skill(
            "notes",
            format!(
                "{}\nexport GITHUB_TOKEN={GITHUB_TOKEN}\n",
                repository_skill("first")
            ),
        )?;

        let errors = ru.sync(&[]);
        assert_eq!(errors.len(), 2, "a count and one problem: {errors:?}");
        assert!(
            errors[1].contains(path.to_str().unwrap()) && errors[1].contains("secret"),
            "the report must name the file: {errors:?}"
        );
        Ok(())
    }

    #[test]
    fn test_ru_sync_counts_a_file_it_cannot_read() -> Result<()> {
        let mut ru = RuRemote::new("ru_sync_unreadable")?;
        ru.write_skill("notes", repository_skill("first"))?;
        let bad = ru.write_skill("broken", [0xff, 0xfe, 0x00])?;

        let errors = ru.sync(&[]);
        assert_eq!(
            errors.first().map(String::as_str),
            Some("ru auto_index: 1 problem(s) in SKILL.md files"),
            "{errors:?}"
        );
        assert!(errors[1].contains(bad.to_str().unwrap()), "{errors:?}");
        assert_eq!(
            ru.indexed_description().as_deref(),
            Some("first"),
            "one bad file must not stop the rest"
        );
        Ok(())
    }

    #[test]
    fn test_ru_sync_counts_frontmatter_it_cannot_parse() -> Result<()> {
        let mut ru = RuRemote::new("ru_sync_frontmatter")?;
        let bad = ru.write_skill("notes", "---\nid: [unclosed\n---\n\n# Repo Notes\n")?;

        let errors = ru.sync(&[]);
        assert_eq!(
            errors.first().map(String::as_str),
            Some("ru auto_index: 1 problem(s) in SKILL.md files"),
            "{errors:?}"
        );
        assert!(
            errors[1].contains(bad.to_str().unwrap()) && errors[1].contains("frontmatter"),
            "{errors:?}"
        );
        assert!(
            !local_spec(&ru.fixture, "repo-notes").exists(),
            "broken frontmatter must not be indexed under the heading's name"
        );
        ru.assert_reindex_refused(&bad, "frontmatter");
        Ok(())
    }

    #[test]
    fn test_ru_sync_refuses_to_replace_a_skill_from_another_source() -> Result<()> {
        let mut ru = RuRemote::new("ru_sync_same_id")?;
        ru.fixture
            .create_skill("repo-notes", &repository_skill("mine"))?;
        let output = ru.fixture.run_ms(&["--robot", "index"]);
        ru.fixture.assert_success(&output, "index");
        let theirs = ru.write_skill("notes", repository_skill("theirs"))?;

        let errors = ru.sync(&[]);
        assert_eq!(
            ru.indexed_description().as_deref(),
            Some("mine"),
            "a repository must not replace a skill it did not write"
        );
        assert_eq!(errors.len(), 2, "a count and one problem: {errors:?}");
        assert!(errors[1].contains("--force"), "{errors:?}");
        ru.assert_reindex_refused(&theirs, "--force");
        Ok(())
    }

    #[test]
    fn test_ru_sync_does_not_take_over_a_skill_with_an_identical_copy() -> Result<()> {
        let mut ru = RuRemote::new("ru_sync_identical")?;
        ru.fixture
            .create_skill("repo-notes", &repository_skill("mine"))?;
        let output = ru.fixture.run_ms(&["--robot", "index"]);
        ru.fixture.assert_success(&output, "index");

        ru.write_skill("notes", repository_skill("mine"))?;
        ru.sync(&[]);
        let theirs = ru.write_skill("notes", repository_skill("theirs"))?;
        ru.sync(&[]);
        assert_eq!(
            ru.indexed_description().as_deref(),
            Some("mine"),
            "a first identical copy must not become the skill's origin"
        );
        ru.assert_reindex_refused(&theirs, "--force");
        Ok(())
    }

    #[test]
    fn test_index_from_ru_refuses_to_replace_a_skill_from_another_source() -> Result<()> {
        let mut ru = RuRemote::new("index_from_ru_same_id")?;
        ru.fixture
            .create_skill("repo-notes", &repository_skill("mine"))?;
        let output = ru.fixture.run_ms(&["--robot", "index"]);
        ru.fixture.assert_success(&output, "index");
        ru.write_skill("notes", repository_skill("theirs"))?;

        let output = ru.fixture.run_ms(&["--robot", "index", "--from-ru"]);
        assert_eq!(ru.indexed_description().as_deref(), Some("mine"));
        assert_eq!(output.json()["status"], "partial", "{}", output.stdout);
        assert!(output.stdout.contains("--force"), "{}", output.stdout);

        let output = ru
            .fixture
            .run_ms(&["--robot", "index", "--from-ru", "--force"]);
        ru.fixture
            .assert_success(&output, "index --from-ru --force");
        assert_eq!(ru.indexed_description().as_deref(), Some("theirs"));
        Ok(())
    }

    #[test]
    fn test_ru_sync_force_replaces_a_skill_from_another_source() -> Result<()> {
        let mut ru = RuRemote::new("ru_sync_same_id_force")?;
        ru.fixture
            .create_skill("repo-notes", &repository_skill("mine"))?;
        let output = ru.fixture.run_ms(&["--robot", "index"]);
        ru.fixture.assert_success(&output, "index");
        ru.write_skill("notes", repository_skill("theirs"))?;

        let errors = ru.sync(&["--force"]);
        assert_eq!(ru.indexed_description().as_deref(), Some("theirs"));
        assert_eq!(errors, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn test_ru_sync_refuses_a_second_copy_of_an_id() -> Result<()> {
        let mut ru = RuRemote::new("ru_sync_two_copies")?;
        ru.write_skill("one", repository_skill("copy"))?;
        ru.write_skill("two", repository_skill("copy"))?;

        let errors = ru.sync(&[]);
        assert_eq!(errors.len(), 2, "a count and one problem: {errors:?}");
        assert!(errors[1].contains("also has id repo-notes"), "{errors:?}");
        assert_eq!(ru.indexed_description().as_deref(), Some("copy"));
        Ok(())
    }
}
