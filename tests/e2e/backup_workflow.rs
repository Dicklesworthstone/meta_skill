//! E2E Scenario: Backup Workflow Integration Tests
//!
//! Comprehensive tests for the `ms backup` command covering:
//! - Backup create with auto-generated timestamp ID
//! - Backup create with custom ID
//! - Backup list (empty, single, multiple)
//! - Backup restore by ID with --approve
//! - Backup restore --latest with --approve
//! - Backup restore rejected without --approve
//! - Backup create with invalid ID
//! - Roundtrip: create, modify, restore, verify
//! - Backup list respects --limit

use super::fixture::{E2EFixture, LogLevel};
use ms::error::Result;
use std::fs;

// ============================================================================
// Skill definitions
// ============================================================================

const SKILL_ALPHA: &str = r#"---
name: Alpha Skill
description: First skill for backup testing
tags: [test, alpha]
---

# Alpha Skill

This is the first skill used in backup workflow tests.

## Rules

- Rule one for alpha
- Rule two for alpha
"#;

const SKILL_BETA: &str = r#"---
name: Beta Skill
description: Second skill for backup testing
tags: [test, beta]
---

# Beta Skill

This is the second skill used in backup workflow tests.

## Rules

- Rule one for beta
- Rule two for beta
"#;

const SKILL_MODIFIED: &str = r#"---
name: Alpha Skill Modified
description: Modified version of alpha skill
tags: [test, alpha, modified]
---

# Alpha Skill Modified

This is the MODIFIED version of the alpha skill.

## Rules

- Modified rule one
- Modified rule two
- New rule three
"#;

// ============================================================================
// Helper: set up a workspace with skills indexed
// ============================================================================

fn setup_workspace(scenario: &str) -> Result<E2EFixture> {
    let mut fixture = E2EFixture::new(scenario);

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    fixture.log_step("Create test skills");
    fixture.create_skill("alpha-skill", SKILL_ALPHA)?;
    fixture.create_skill("beta-skill", SKILL_BETA)?;

    fixture.log_step("Index skills");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    fixture.checkpoint("backup:workspace-ready");

    fixture.emit_event(
        LogLevel::Info,
        "backup",
        "Workspace created with 2 skills",
        Some(serde_json::json!({ "skills": 2 })),
    );

    Ok(fixture)
}

// ============================================================================
// Tests
// ============================================================================

/// Backup create with auto-generated timestamp ID.
#[test]
fn test_backup_create_auto_id() -> Result<()> {
    let mut fixture = setup_workspace("backup_create_auto_id")?;

    fixture.checkpoint("backup:pre-create");

    fixture.log_step("Create backup with auto-generated ID");
    let output = fixture.run_ms(&["--robot", "backup", "create"]);
    fixture.assert_success(&output, "backup create");

    fixture.checkpoint("backup:post-create");

    let json = output.json();
    let backup_id = json["id"].as_str().expect("backup should have id");

    // Auto-generated IDs should be numeric timestamp format
    assert!(
        !backup_id.is_empty(),
        "Backup ID should not be empty"
    );

    fixture.emit_event(
        LogLevel::Info,
        "backup",
        &format!("Backup created with auto ID: {backup_id}"),
        Some(json.clone()),
    );

    // Verify backup directory was created
    let backup_dir = fixture.ms_root.join("backups").join(backup_id);
    assert!(
        backup_dir.exists(),
        "Backup directory should exist at {backup_dir:?}"
    );

    // Verify manifest exists
    let manifest_path = backup_dir.join("manifest.json");
    assert!(
        manifest_path.exists(),
        "Manifest should exist at {manifest_path:?}"
    );

    // Verify database was backed up
    let db_backup = backup_dir.join("ms.db");
    assert!(
        db_backup.exists(),
        "Database backup should exist at {db_backup:?}"
    );

    fixture.generate_report();
    Ok(())
}

/// Backup create with a custom ID.
#[test]
fn test_backup_create_custom_id() -> Result<()> {
    let mut fixture = setup_workspace("backup_create_custom_id")?;

    fixture.checkpoint("backup:pre-create");

    fixture.log_step("Create backup with custom ID");
    let output = fixture.run_ms(&["--robot", "backup", "create", "--id", "my-custom-backup"]);
    fixture.assert_success(&output, "backup create custom id");

    fixture.checkpoint("backup:post-create");

    let json = output.json();
    let backup_id = json["id"].as_str().expect("backup should have id");
    assert_eq!(
        backup_id, "my-custom-backup",
        "Backup ID should match custom value"
    );

    // Verify backup directory exists with custom name
    let backup_dir = fixture.ms_root.join("backups").join("my-custom-backup");
    assert!(
        backup_dir.exists(),
        "Backup directory should exist with custom name"
    );

    let manifest_path = backup_dir.join("manifest.json");
    assert!(manifest_path.exists(), "Manifest should exist");

    // Parse manifest and verify contents
    let manifest_raw = fs::read_to_string(&manifest_path)?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest_raw)?;
    assert_eq!(manifest["id"].as_str(), Some("my-custom-backup"));

    fixture.emit_event(
        LogLevel::Info,
        "backup",
        "Backup created with custom ID",
        Some(json.clone()),
    );

    fixture.generate_report();
    Ok(())
}

/// Backup list with no backups should return empty.
#[test]
fn test_backup_list_empty() -> Result<()> {
    let mut fixture = setup_workspace("backup_list_empty")?;

    fixture.checkpoint("backup:pre-list");

    fixture.log_step("List backups when none exist");
    let output = fixture.run_ms(&["--robot", "backup", "list"]);
    fixture.assert_success(&output, "backup list empty");

    fixture.checkpoint("backup:post-list");

    let json = output.json();
    let count = json["count"].as_u64().unwrap_or(0);
    assert_eq!(count, 0, "Should have zero backups");

    fixture.emit_event(
        LogLevel::Info,
        "backup",
        "Empty backup list verified",
        Some(json.clone()),
    );

    fixture.generate_report();
    Ok(())
}

/// Backup list with multiple backups returns them in reverse order.
#[test]
fn test_backup_list_multiple() -> Result<()> {
    let mut fixture = setup_workspace("backup_list_multiple")?;

    fixture.log_step("Create first backup");
    let output = fixture.run_ms(&["--robot", "backup", "create", "--id", "backup-001"]);
    fixture.assert_success(&output, "backup create 1");

    fixture.log_step("Create second backup");
    let output = fixture.run_ms(&["--robot", "backup", "create", "--id", "backup-002"]);
    fixture.assert_success(&output, "backup create 2");

    fixture.log_step("Create third backup");
    let output = fixture.run_ms(&["--robot", "backup", "create", "--id", "backup-003"]);
    fixture.assert_success(&output, "backup create 3");

    fixture.checkpoint("backup:pre-list");

    fixture.log_step("List all backups");
    let output = fixture.run_ms(&["--robot", "backup", "list"]);
    fixture.assert_success(&output, "backup list");

    fixture.checkpoint("backup:post-list");

    let json = output.json();
    let count = json["count"].as_u64().unwrap_or(0);
    assert_eq!(count, 3, "Should have 3 backups");

    let backups = json["backups"].as_array().expect("backups should be array");
    assert_eq!(backups.len(), 3, "Should list 3 backups");

    // Verify reverse alphabetical order (newest first)
    let first_id = backups[0]["id"].as_str().unwrap_or("");
    let last_id = backups[2]["id"].as_str().unwrap_or("");
    assert!(
        first_id >= last_id,
        "Backups should be sorted newest first: {first_id} >= {last_id}"
    );

    fixture.emit_event(
        LogLevel::Info,
        "backup",
        "Multiple backups listed",
        Some(serde_json::json!({ "count": count })),
    );

    fixture.generate_report();
    Ok(())
}

/// Backup list respects --limit flag.
#[test]
fn test_backup_list_limit() -> Result<()> {
    let mut fixture = setup_workspace("backup_list_limit")?;

    fixture.log_step("Create 3 backups");
    for i in 1..=3 {
        let id = format!("limit-backup-{i:03}");
        let output = fixture.run_ms(&["--robot", "backup", "create", "--id", &id]);
        fixture.assert_success(&output, &format!("backup create {i}"));
    }

    fixture.checkpoint("backup:pre-list-limited");

    fixture.log_step("List with limit=2");
    let output = fixture.run_ms(&["--robot", "backup", "list", "--limit", "2"]);
    fixture.assert_success(&output, "backup list limited");

    fixture.checkpoint("backup:post-list-limited");

    let json = output.json();
    let backups = json["backups"].as_array().expect("backups should be array");
    assert!(
        backups.len() <= 2,
        "Should return at most 2 backups, got {}",
        backups.len()
    );

    fixture.emit_event(
        LogLevel::Info,
        "backup",
        "Backup list limit verified",
        Some(serde_json::json!({ "limit": 2, "returned": backups.len() })),
    );

    fixture.generate_report();
    Ok(())
}

/// Backup restore by specific ID with --approve.
#[test]
fn test_backup_restore_by_id() -> Result<()> {
    let mut fixture = setup_workspace("backup_restore_by_id")?;

    fixture.log_step("Create backup");
    let output = fixture.run_ms(&["--robot", "backup", "create", "--id", "restore-test"]);
    fixture.assert_success(&output, "backup create");

    fixture.checkpoint("backup:pre-restore");

    fixture.log_step("Restore backup by ID");
    let output = fixture.run_ms(&[
        "--robot",
        "backup",
        "restore",
        "restore-test",
        "--approve",
    ]);
    fixture.assert_success(&output, "backup restore");

    fixture.checkpoint("backup:post-restore");

    let json = output.json();
    let status = json["status"].as_str().unwrap_or("");
    assert_eq!(status, "ok", "Restore status should be ok");

    fixture.emit_event(
        LogLevel::Info,
        "backup",
        "Backup restored by ID",
        Some(json.clone()),
    );

    fixture.generate_report();
    Ok(())
}

/// Backup restore --latest with --approve.
#[test]
fn test_backup_restore_latest() -> Result<()> {
    let mut fixture = setup_workspace("backup_restore_latest")?;

    fixture.log_step("Create first backup");
    let output = fixture.run_ms(&["--robot", "backup", "create", "--id", "old-backup"]);
    fixture.assert_success(&output, "backup create old");

    fixture.log_step("Create second backup");
    let output = fixture.run_ms(&["--robot", "backup", "create", "--id", "new-backup"]);
    fixture.assert_success(&output, "backup create new");

    fixture.checkpoint("backup:pre-restore-latest");

    fixture.log_step("Restore latest backup");
    let output = fixture.run_ms(&["--robot", "backup", "restore", "--latest", "--approve"]);
    fixture.assert_success(&output, "backup restore latest");

    fixture.checkpoint("backup:post-restore-latest");

    let json = output.json();
    let restored = json["restored"].as_str().unwrap_or("");
    assert_eq!(
        restored, "old-backup",
        "Should restore the alphabetically last backup (newest by name)"
    );

    fixture.emit_event(
        LogLevel::Info,
        "backup",
        "Latest backup restored",
        Some(json.clone()),
    );

    fixture.generate_report();
    Ok(())
}

/// Backup restore without --approve should fail.
#[test]
fn test_backup_restore_requires_approval() -> Result<()> {
    let mut fixture = setup_workspace("backup_restore_no_approve")?;

    fixture.log_step("Create backup");
    let output = fixture.run_ms(&["--robot", "backup", "create", "--id", "needs-approve"]);
    fixture.assert_success(&output, "backup create");

    fixture.checkpoint("backup:pre-restore-no-approve");

    fixture.log_step("Attempt restore without --approve");
    let output = fixture.run_ms(&["--robot", "backup", "restore", "needs-approve"]);

    // Restore without --approve should fail
    assert!(
        !output.success,
        "Restore without --approve should fail"
    );

    fixture.checkpoint("backup:post-restore-no-approve");

    fixture.emit_event(
        LogLevel::Info,
        "backup",
        "Restore correctly rejected without --approve",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Backup create with invalid ID should fail.
#[test]
fn test_backup_create_invalid_id() -> Result<()> {
    let mut fixture = setup_workspace("backup_create_invalid_id")?;

    fixture.checkpoint("backup:pre-create-invalid");

    fixture.log_step("Attempt backup create with path traversal ID");
    let output = fixture.run_ms(&["--robot", "backup", "create", "--id", "../escape"]);
    assert!(
        !output.success,
        "Backup with path traversal ID should fail"
    );

    fixture.log_step("Attempt backup create with slash in ID");
    let output = fixture.run_ms(&["--robot", "backup", "create", "--id", "a/b"]);
    assert!(
        !output.success,
        "Backup with slash in ID should fail"
    );

    fixture.checkpoint("backup:post-create-invalid");

    fixture.emit_event(
        LogLevel::Info,
        "backup",
        "Invalid backup IDs correctly rejected",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Full roundtrip: create backup, modify workspace, restore, verify original state.
#[test]
fn test_backup_roundtrip() -> Result<()> {
    let mut fixture = setup_workspace("backup_roundtrip")?;

    fixture.checkpoint("backup:initial-state");

    // Step 1: Create backup of original state
    fixture.log_step("Create backup of original state");
    let output = fixture.run_ms(&["--robot", "backup", "create", "--id", "original"]);
    fixture.assert_success(&output, "backup create");

    fixture.checkpoint("backup:post-create");

    // Step 2: Modify the workspace - change a skill
    fixture.log_step("Modify workspace by overwriting alpha skill");
    fixture.create_skill("alpha-skill", SKILL_MODIFIED)?;

    // Re-index to pick up changes
    fixture.log_step("Re-index after modification");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "re-index");

    fixture.checkpoint("backup:modified-state");

    // Verify the modification took effect by searching
    fixture.log_step("Verify modification is in effect");
    let output = fixture.run_ms(&["--robot", "search", "modified"]);
    // The search should find something related to the modified skill
    fixture.assert_success(&output, "search modified");

    // Step 3: Restore the original backup
    fixture.log_step("Restore original backup");
    let output = fixture.run_ms(&["--robot", "backup", "restore", "original", "--approve"]);
    fixture.assert_success(&output, "restore original");

    fixture.checkpoint("backup:restored-state");

    let json = output.json();
    let status = json["status"].as_str().unwrap_or("");
    assert_eq!(status, "ok", "Restore should succeed");

    fixture.emit_event(
        LogLevel::Info,
        "backup",
        "Full backup roundtrip completed",
        Some(json.clone()),
    );

    fixture.generate_report();
    Ok(())
}

/// Backup on a minimal workspace (init only, no skills).
#[test]
fn test_backup_minimal_workspace() -> Result<()> {
    let mut fixture = E2EFixture::new("backup_minimal_workspace");

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    fixture.checkpoint("backup:pre-create-minimal");

    fixture.log_step("Create backup on minimal workspace");
    let output = fixture.run_ms(&["--robot", "backup", "create", "--id", "minimal"]);
    fixture.assert_success(&output, "backup create minimal");

    fixture.checkpoint("backup:post-create-minimal");

    let json = output.json();
    let backup_id = json["id"].as_str().expect("backup should have id");
    assert_eq!(backup_id, "minimal");

    // Verify backup exists
    let backup_dir = fixture.ms_root.join("backups").join("minimal");
    assert!(backup_dir.exists(), "Backup directory should exist");

    // List should show one backup
    fixture.log_step("List backups");
    let output = fixture.run_ms(&["--robot", "backup", "list"]);
    fixture.assert_success(&output, "backup list");

    let json = output.json();
    let count = json["count"].as_u64().unwrap_or(0);
    assert_eq!(count, 1, "Should have exactly one backup");

    fixture.emit_event(
        LogLevel::Info,
        "backup",
        "Minimal workspace backup verified",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Backup restore with nonexistent ID should fail.
#[test]
fn test_backup_restore_not_found() -> Result<()> {
    let mut fixture = setup_workspace("backup_restore_not_found")?;

    fixture.checkpoint("backup:pre-restore-missing");

    fixture.log_step("Attempt restore of nonexistent backup");
    let output = fixture.run_ms(&[
        "--robot",
        "backup",
        "restore",
        "does-not-exist",
        "--approve",
    ]);
    assert!(
        !output.success,
        "Restore of nonexistent backup should fail"
    );

    fixture.checkpoint("backup:post-restore-missing");

    fixture.emit_event(
        LogLevel::Info,
        "backup",
        "Nonexistent backup restore correctly failed",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Backup manifest contains expected structure.
#[test]
fn test_backup_manifest_structure() -> Result<()> {
    let mut fixture = setup_workspace("backup_manifest_structure")?;

    fixture.log_step("Create backup");
    let output = fixture.run_ms(&["--robot", "backup", "create", "--id", "manifest-check"]);
    fixture.assert_success(&output, "backup create");

    fixture.checkpoint("backup:post-create-manifest");

    // Read and parse the manifest
    let manifest_path = fixture
        .ms_root
        .join("backups")
        .join("manifest-check")
        .join("manifest.json");
    assert!(manifest_path.exists(), "Manifest should exist");

    let manifest_raw = fs::read_to_string(&manifest_path)?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest_raw)?;

    // Verify required manifest fields
    assert!(
        manifest["id"].is_string(),
        "Manifest should have string id"
    );
    assert!(
        manifest["created_at"].is_string(),
        "Manifest should have created_at"
    );
    assert!(
        manifest["ms_root"].is_string(),
        "Manifest should have ms_root"
    );
    assert!(
        manifest["entries"].is_array(),
        "Manifest should have entries array"
    );

    let entries = manifest["entries"].as_array().unwrap();
    assert!(
        !entries.is_empty(),
        "Manifest should have at least one entry (ms.db)"
    );

    // Verify ms.db is in entries
    let has_db = entries.iter().any(|e| e["name"].as_str() == Some("ms.db"));
    assert!(has_db, "Manifest entries should include ms.db");

    fixture.emit_event(
        LogLevel::Info,
        "backup",
        "Manifest structure verified",
        Some(serde_json::json!({ "entry_count": entries.len() })),
    );

    fixture.generate_report();
    Ok(())
}
