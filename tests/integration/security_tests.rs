//! Security integration tests
//!
//! Tests for path traversal prevention, secret redaction, and error message hygiene.

use super::fixture::TestFixture;

/// Test that path traversal is blocked in skill names
#[test]
fn test_path_traversal_in_skill_name_blocked() {
    let fixture = TestFixture::new("path_traversal_skill_name");
    let init = fixture.init();
    assert!(init.success, "init failed: {}", init.stderr);

    // Try to create a skill with path traversal in name
    let traversal_skill_dir = fixture.skills_dir.join("../escape");
    std::fs::create_dir_all(&traversal_skill_dir).ok();

    let traversal_skill = traversal_skill_dir.join("SKILL.md");
    std::fs::write(
        &traversal_skill,
        "# Malicious Skill\n\nShould not be indexed.\n",
    )
    .ok();

    // Index should not index skills with traversal paths
    let output = fixture.run_ms(&["--robot", "index"]);

    // The traversal skill should not appear in output
    assert!(
        !output.stdout.contains("escape"),
        "Path traversal skill should not be indexed"
    );
}

/// Test that secrets in skill content don't leak in error messages
#[test]
fn test_error_messages_dont_leak_secrets() {
    let fixture = TestFixture::new("error_message_secrets");
    let init = fixture.init();
    assert!(init.success, "init failed: {}", init.stderr);

    // Create a skill with secrets in it
    let skill_content = r#"# Skill With Secrets

This skill has AWS credentials: AKIAIOSFODNN7EXAMPLE

And a password: password="supersecret123"

## Usage
Use this carefully.
"#;

    let skill_dir = fixture.skills_dir.join("secret-skill");
    std::fs::create_dir_all(&skill_dir).expect("create skill dir");
    std::fs::write(skill_dir.join("SKILL.md"), skill_content).expect("write skill");

    // Try to load the skill and check errors don't contain secrets
    let output = fixture.run_ms(&["--robot", "show", "nonexistent-skill"]);

    // Error output should not contain the AWS key or password
    assert!(
        !output.stderr.contains("AKIAIOSFODNN7EXAMPLE"),
        "AWS key should not leak in errors"
    );
    assert!(
        !output.stderr.contains("supersecret123"),
        "Password should not leak in errors"
    );
    assert!(
        !output.stdout.contains("AKIAIOSFODNN7EXAMPLE"),
        "AWS key should not leak in stdout"
    );
    assert!(
        !output.stdout.contains("supersecret123"),
        "Password should not leak in stdout"
    );
}

/// Test that the doctor security check runs
#[test]
fn test_doctor_security_check() {
    let fixture = TestFixture::new("doctor_security");
    let init = fixture.init();
    assert!(init.success, "init failed: {}", init.stderr);

    // Run doctor with security check
    let output = fixture.run_ms(&["doctor", "--check=security"]);

    // Should complete (may have warnings about DCG not available)
    assert!(
        output.stdout.contains("Security Checks") || output.stderr.contains("Security Checks"),
        "Security check header should be present"
    );
}

/// Test that symlink escape attempts are handled safely
#[test]
#[cfg(unix)]
fn test_symlink_escape_blocked() {
    use std::os::unix::fs::symlink;

    let fixture = TestFixture::new("symlink_escape");
    let init = fixture.init();
    assert!(init.success, "init failed: {}", init.stderr);

    // Create a directory outside the skills dir
    let outside_dir = fixture.root.join("outside");
    std::fs::create_dir_all(&outside_dir).expect("create outside dir");
    std::fs::write(outside_dir.join("SKILL.md"), "# Outside Skill\n").expect("write outside skill");

    // Create a symlink inside skills pointing outside
    let escape_link = fixture.skills_dir.join("escape-link");
    symlink(&outside_dir, &escape_link).expect("create symlink");

    // Index should not follow symlinks that escape the skills directory
    let output = fixture.run_ms(&["--robot", "index"]);

    // The escaped skill should not be indexed
    assert!(
        !output.stdout.contains("Outside Skill"),
        "Symlink escape should not be followed"
    );
}

/// Test that null bytes in paths are rejected
#[test]
fn test_null_byte_in_path_rejected() {
    // This tests at the library level since we can't easily pass null bytes via CLI
    use ms::security::path_policy::validate_path_component;

    assert!(
        validate_path_component("normal").is_ok(),
        "Normal component should pass"
    );
    assert!(
        validate_path_component("with\0null").is_err(),
        "Null byte should be rejected"
    );
}

/// Test secret scanner detection
#[test]
fn test_secret_scanner_integration() {
    use ms::security::{SecretType, contains_secrets, redact_secrets, scan_secrets};

    let content_with_secrets = r#"
        config = {
            aws_key = "AKIAIOSFODNN7EXAMPLE",
            github_token = "ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
            password = "hunter2"
        }
    "#;

    // Should detect secrets
    assert!(
        contains_secrets(content_with_secrets),
        "Should detect secrets in content"
    );

    // Should find AWS key
    let matches = scan_secrets(content_with_secrets);
    let has_aws = matches
        .iter()
        .any(|m| m.secret_type == SecretType::AwsAccessKey);
    let has_github = matches
        .iter()
        .any(|m| m.secret_type == SecretType::GitHubToken);

    assert!(has_aws, "Should detect AWS access key");
    assert!(has_github, "Should detect GitHub token");

    // Redaction should remove secrets
    let redacted = redact_secrets(content_with_secrets);
    assert!(
        !redacted.contains("AKIAIOSFODNN7EXAMPLE"),
        "AWS key should be redacted"
    );
    assert!(
        !redacted.contains("ghp_xxxx"),
        "GitHub token should be redacted"
    );
    assert!(
        redacted.contains("[REDACTED]"),
        "Redaction markers should be present"
    );
}

/// Test path policy utilities
#[test]
fn test_path_policy_integration() {
    use ms::security::path_policy::{is_under_root, normalize_path, safe_join};
    use std::path::Path;

    let root = Path::new("/data/skills");

    // Safe joins should work
    assert!(safe_join(root, "my-skill/file.txt", false).is_ok());
    assert!(safe_join(root, "nested/deep/file", false).is_ok());

    // Traversal should fail
    assert!(safe_join(root, "../escape", false).is_err());
    assert!(safe_join(root, "nested/../../escape", false).is_err());

    // Absolute paths should fail
    assert!(safe_join(root, "/etc/passwd", false).is_err());

    // is_under_root checks
    assert!(is_under_root(Path::new("/data/skills/test"), root));
    assert!(!is_under_root(Path::new("/data/other"), root));

    // Normalization
    assert_eq!(
        normalize_path(Path::new("/foo/./bar/../baz")),
        std::path::PathBuf::from("/foo/baz")
    );
}

/// Test that command safety gate is available
#[test]
fn test_safety_gate_status() {
    let fixture = TestFixture::new("safety_gate_status");
    let init = fixture.init();
    assert!(init.success, "init failed: {}", init.stderr);

    // Check safety status via doctor
    let output = fixture.run_ms(&["doctor", "--check=safety"]);

    // Should report on DCG status (may or may not be available)
    assert!(
        output.stdout.contains("dcg") || output.stderr.contains("dcg"),
        "Should report DCG status"
    );
}
