//! Regression tests for GitHub issues #244 and #249 and v0.4.0 bug fixes.

use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn ms_cmd(home: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ms"));
    for k in [
        "MS_CONFIG",
        "MS_ROOT",
        "MS_ROBOT",
        "MS_ROBOT_FORMAT",
        "MS_OUTPUT_FORMAT",
    ] {
        cmd.env_remove(k);
    }
    cmd.env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("XDG_STATE_HOME", home.join(".local/state"));
    cmd
}

fn setup_env() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
    let tmp = TempDir::new().expect("tempdir");
    let home = tmp.path().join("home");
    let skills = tmp.path().join("skills");
    fs::create_dir_all(home.join(".config/ms")).unwrap();
    fs::create_dir_all(&skills).unwrap();

    let init = ms_cmd(&home)
        .args(["init", "--global"])
        .current_dir(tmp.path())
        .output()
        .expect("ms init");
    assert!(
        init.status.success(),
        "ms init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    fs::write(
        home.join(".config/ms/config.toml"),
        format!(
            "[skill_paths]\nglobal = [\"{}\"]\ncommunity = []\n\n[search]\nuse_embeddings = false\n",
            skills.display()
        ),
    )
    .unwrap();
    (tmp, home, skills)
}

#[test]
fn issue_244_h1_retitle_reconciles_superseded_id_and_restores_cleanly() {
    let (tmp, home, skills) = setup_env();
    let skill_dir = skills.join("retitle-probe");
    fs::create_dir_all(&skill_dir).unwrap();
    let skill_file = skill_dir.join("SKILL.md");

    let write_skill = |title: &str, body: &str| {
        fs::write(
            &skill_file,
            format!(
                "---\nname: retitle-probe\ndescription: Reproduce H1 retitle indexing.\n---\n\n# {title}\n\n{body}\n"
            ),
        )
        .unwrap();
    };

    // 1. Index initial H1
    write_skill("Retitle probe old", "OLD_BODY_SENTINEL");
    let out = ms_cmd(&home)
        .args(["index", "--force", skills.to_str().unwrap()])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // 2. Retitle H1 at same source path and reindex with --force
    write_skill("Retitle probe new", "NEW_BODY_SENTINEL");
    let out = ms_cmd(&home)
        .args(["index", "--force", skills.to_str().unwrap()])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // list must contain ONLY retitle-probe-new
    let list_out = ms_cmd(&home)
        .args(["list", "-O", "json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(list_out.status.success());
    let list_json: serde_json::Value = serde_json::from_slice(&list_out.stdout).unwrap();
    assert_eq!(list_json["count"].as_u64(), Some(1), "list: {list_json}");
    assert_eq!(
        list_json["skills"][0]["id"].as_str(),
        Some("retitle-probe-new")
    );

    // old ID must not be loadable
    let load_old = ms_cmd(&home)
        .args(["load", "retitle-probe-old", "--level", "4", "-O", "json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(
        !load_old.status.success(),
        "stale H1 ID must not remain loadable"
    );

    // new ID loads NEW_BODY_SENTINEL
    let load_new = ms_cmd(&home)
        .args(["load", "retitle-probe-new", "--level", "4", "-O", "json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(load_new.status.success());
    assert!(String::from_utf8_lossy(&load_new.stdout).contains("NEW_BODY_SENTINEL"));

    // show by frontmatter name resolves unambiguously
    let show_out = ms_cmd(&home)
        .args(["show", "retitle-probe", "-O", "json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(
        show_out.status.success(),
        "show retitle-probe must resolve unambiguously: {}",
        String::from_utf8_lossy(&show_out.stderr)
    );
    let show_json: serde_json::Value = serde_json::from_slice(&show_out.stdout).unwrap();
    assert_eq!(show_json["skill"]["id"].as_str(), Some("retitle-probe-new"));

    // 3. Restore original H1 and reindex -> old ID returns active (non-deprecated) and new ID retires
    write_skill("Retitle probe old", "RESTORED_BODY_SENTINEL");
    let out = ms_cmd(&home)
        .args(["index", "--force", skills.to_str().unwrap()])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(out.status.success());

    let list_out = ms_cmd(&home)
        .args(["list", "-O", "json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    let list_json: serde_json::Value = serde_json::from_slice(&list_out.stdout).unwrap();
    assert_eq!(list_json["count"].as_u64(), Some(1), "list: {list_json}");
    assert_eq!(
        list_json["skills"][0]["id"].as_str(),
        Some("retitle-probe-old")
    );
    assert_eq!(
        list_json["skills"][0]["is_deprecated"].as_bool(),
        Some(false)
    );

    let load_restored = ms_cmd(&home)
        .args(["load", "retitle-probe-old", "--level", "4", "-O", "json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(load_restored.status.success());
    assert!(String::from_utf8_lossy(&load_restored.stdout).contains("RESTORED_BODY_SENTINEL"));
}

#[test]
fn issue_244_show_by_name_prefers_active_over_deprecated_skill() {
    let (tmp, home, skills) = setup_env();
    for (dir_name, h1) in [("skill-a", "Shared Name A"), ("skill-b", "Shared Name B")] {
        let d = skills.join(dir_name);
        fs::create_dir_all(&d).unwrap();
        fs::write(
            d.join("SKILL.md"),
            format!("---\nname: shared-name\ndescription: Test.\n---\n\n# {h1}\n\nBody {h1}.\n"),
        )
        .unwrap();
    }
    let idx = ms_cmd(&home)
        .args(["index", skills.to_str().unwrap()])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(idx.status.success());

    // Initially ambiguous
    let ambig = ms_cmd(&home)
        .args(["show", "shared-name", "-O", "json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(!ambig.status.success());

    // Deprecate skill-b via dedup merge
    let merge = ms_cmd(&home)
        .args([
            "dedup",
            "merge",
            "shared-name-a",
            "shared-name-b",
            "--reason",
            "superseded",
        ])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(
        merge.status.success(),
        "{}",
        String::from_utf8_lossy(&merge.stderr)
    );

    // Now `show shared-name` resolves unambiguously to active skill shared-name-a
    let show = ms_cmd(&home)
        .args(["show", "shared-name", "-O", "json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(
        show.status.success(),
        "show after deprecating duplicate must succeed: {}",
        String::from_utf8_lossy(&show.stderr)
    );
    let show_json: serde_json::Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(show_json["skill"]["id"].as_str(), Some("shared-name-a"));
}

#[test]
fn issue_249_batch_index_and_readonly_commands_succeed() {
    let (tmp, home, skills) = setup_env();
    for i in 1..=10 {
        let d = skills.join(format!("batch-skill-{i}"));
        fs::create_dir_all(&d).unwrap();
        fs::write(
            d.join("SKILL.md"),
            format!("---\nname: batch-skill-{i}\ndescription: Batch skill {i}.\n---\n\n# Batch Skill {i}\n\nBody content {i}.\n"),
        )
        .unwrap();
    }

    // Batch index creates all 10 skills cleanly
    let idx1 = ms_cmd(&home)
        .args(["index", "-O", "json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(
        idx1.status.success(),
        "{}",
        String::from_utf8_lossy(&idx1.stderr)
    );
    let j1: serde_json::Value = serde_json::from_slice(&idx1.stdout).unwrap();
    assert_eq!(j1["indexed"].as_u64(), Some(10));

    // Unchanged reindex succeeds cleanly
    let idx2 = ms_cmd(&home)
        .args(["index", "-O", "json"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(idx2.status.success());

    // Read-only commands (list, show, search, load --no-track) all work
    for args in [
        vec!["list", "-O", "json"],
        vec!["show", "batch-skill-1", "--meta", "-O", "json"],
        vec!["search", "Batch", "-O", "json"],
        vec![
            "load",
            "batch-skill-1",
            "--no-track",
            "--level",
            "4",
            "-O",
            "json",
        ],
    ] {
        let out = ms_cmd(&home)
            .args(&args)
            .current_dir(tmp.path())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "command {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
