//! End-to-end tests for `UbsClient::check_staged`.
//!
//! Regression coverage for #127: a commit that stages file *deletions* must not
//! hand the deleted path to UBS (it no longer exists on disk → "file not found"
//! → clean:false / findings:0, blocking a legitimate commit). `check_staged`
//! enumerates staged files with `--diff-filter=ACMR`, so deletions are excluded
//! and a delete-only commit is clean.

use std::fs;
use std::path::Path;
use std::process::Command;

use ms::quality::ubs::UbsClient;
use tempfile::TempDir;

fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(repo)
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?} failed");
}

fn init_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "t@example.com"]);
    git(root, &["config", "user.name", "Test"]);
    // Disable any inherited hooks so our staging is clean.
    git(root, &["config", "core.hooksPath", "/dev/null"]);
    dir
}

/// Build a stub `ubs` script that records every argument it receives (one per
/// line) into `record_path` and exits 0 with no findings. Returns its path.
fn make_recording_ubs_stub(dir: &Path, record_path: &Path) -> std::path::PathBuf {
    let stub = dir.join("ubs_stub.sh");
    let script = format!(
        "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\" >> '{}'; done\nexit 0\n",
        record_path.display()
    );
    fs::write(&stub, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();
    }
    stub
}

#[cfg(unix)]
#[test]
fn check_staged_delete_only_is_clean_and_passes_no_paths() {
    let repo = init_repo();
    let root = repo.path();

    // Commit a baseline with two files.
    fs::write(root.join("keep.txt"), "alpha\n").unwrap();
    fs::write(root.join("gone.txt"), "beta\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "init"]);

    // Stage ONLY a deletion.
    git(root, &["rm", "-q", "gone.txt"]);

    let record = root.join("ubs_args.log");
    let stub = make_recording_ubs_stub(root, &record);
    let client = UbsClient::new(Some(stub));

    let result = client.check_staged(root).expect("check_staged");

    // A delete-only commit yields no staged files to scan → empty/clean result,
    // and the stub must never have been invoked (no args recorded).
    assert!(
        result.is_clean(),
        "delete-only commit should be clean, got {result:?}"
    );
    assert_eq!(result.findings.len(), 0);
    assert!(
        !record.exists(),
        "UBS should not be invoked for a delete-only commit (no existing paths)"
    );
}

#[cfg(unix)]
#[test]
fn check_staged_mixed_add_and_delete_excludes_deleted_path() {
    let repo = init_repo();
    let root = repo.path();

    fs::write(root.join("keep.txt"), "alpha\n").unwrap();
    fs::write(root.join("gone.txt"), "beta\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "init"]);

    // Stage a deletion AND an addition in the same commit.
    git(root, &["rm", "-q", "gone.txt"]);
    fs::write(root.join("added.txt"), "gamma\n").unwrap();
    git(root, &["add", "added.txt"]);

    let record = root.join("ubs_args.log");
    let stub = make_recording_ubs_stub(root, &record);
    let client = UbsClient::new(Some(stub));

    let result = client.check_staged(root).expect("check_staged");
    assert!(
        result.is_clean(),
        "mixed commit should be clean: {result:?}"
    );

    let passed = fs::read_to_string(&record).expect("stub should have run for added.txt");
    assert!(
        passed.contains("added.txt"),
        "added file must be scanned, got: {passed:?}"
    );
    assert!(
        !passed.contains("gone.txt"),
        "deleted file must NOT be handed to UBS, got: {passed:?}"
    );
}

#[cfg(unix)]
#[test]
fn check_staged_rename_scans_new_path_not_old() {
    let repo = init_repo();
    let root = repo.path();

    // Use substantial identical content so git records this as a rename (R).
    let content = "the quick brown fox jumps over the lazy dog\n".repeat(20);
    fs::write(root.join("old_name.txt"), &content).unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "init"]);

    git(root, &["mv", "old_name.txt", "new_name.txt"]);

    let record = root.join("ubs_args.log");
    let stub = make_recording_ubs_stub(root, &record);
    let client = UbsClient::new(Some(stub));

    let result = client.check_staged(root).expect("check_staged");
    assert!(
        result.is_clean(),
        "rename commit should be clean: {result:?}"
    );

    let passed = fs::read_to_string(&record).expect("stub should have run for the new path");
    assert!(
        passed.contains("new_name.txt"),
        "rename target (existing path) must be scanned, got: {passed:?}"
    );
    assert!(
        !passed.contains("old_name.txt"),
        "rename source (deleted path) must NOT be handed to UBS, got: {passed:?}"
    );
}
