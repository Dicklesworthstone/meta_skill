use crate::fixture::TestFixture;

#[test]
fn backup_create_and_restore_roundtrip() {
    let fixture = TestFixture::new("backup_roundtrip");
    let init = fixture.init();
    assert!(init.success, "init failed: {}", init.stderr);

    let original_config = std::fs::read_to_string(&fixture.config_path).expect("read config");

    let backup = fixture.run_ms(&["--robot", "backup", "create", "--id", "roundtrip"]);
    assert!(backup.success, "backup create failed: {}", backup.stderr);

    let backup_dir = fixture.ms_root.join("backups").join("roundtrip");
    assert!(backup_dir.exists(), "backup dir missing");

    std::fs::write(&fixture.config_path, "changed = true\n").expect("write config");

    let restore = fixture.run_ms(&["--robot", "backup", "restore", "roundtrip", "--approve"]);
    assert!(restore.success, "backup restore failed: {}", restore.stderr);

    let restored_config =
        std::fs::read_to_string(&fixture.config_path).expect("read restored config");
    assert_eq!(restored_config, original_config);
}

#[test]
fn backup_restore_missing_id_errors() {
    let fixture = TestFixture::new("backup_missing_id");
    let init = fixture.init();
    assert!(init.success, "init failed: {}", init.stderr);

    let restore = fixture.run_ms(&["--robot", "backup", "restore", "missing", "--approve"]);
    assert!(!restore.success, "restore should fail for missing backup");
    assert!(restore.stdout.contains("\"error\""));
}
