//! E2E Scenario: Safety Workflow
//!
//! Tests the safety policy enforcement lifecycle:
//! status → check → log
//!
//! This is a P1 E2E test that exercises the safety gate workflow.
//! Note: Full DCG integration tests require DCG to be installed.

use super::fixture::E2EFixture;
use ms::error::Result;

/// A skill's `run:` steps go to a shell, and skills reach a machine through imports, bundles
/// and remotes. With no guard to ask, a step must not run.
#[cfg(unix)]
mod run_steps {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    use super::E2EFixture;
    use ms::error::Result;
    use serde_json::Value;

    const SKILL: &str = "---\nname: Run Step\ndescription: A skill with a test\n\
                         tags: [tools]\n---\n\n# Run Step\n\n## Usage\n\nA body.\n";

    fn bench(scenario: &str) -> Result<(E2EFixture, PathBuf)> {
        let mut fixture = E2EFixture::new(scenario);
        let output = fixture.init();
        fixture.assert_success(&output, "init");
        fixture.create_skill("run-step", SKILL)?;
        let output = fixture.run_ms(&["--robot", "index"]);
        fixture.assert_success(&output, "index");

        let marker = fixture.root.join("step-ran");
        let tests = fixture.ms_root.join("archive/skills/by-id/run-step/tests");
        fs::create_dir_all(&tests)?;
        fs::write(
            tests.join("run.yaml"),
            format!(
                "name: Run step\nsteps:\n  - run:\n      cmd: \"echo ran >> '{}'\"\n",
                marker.display()
            ),
        )?;
        Ok((fixture, marker))
    }

    fn stand_in_guard(fixture: &E2EFixture, decision: &str) -> Result<PathBuf> {
        let path = fixture.root.join("stand-in-dcg");
        write_executable(
            &path,
            &format!(
                "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'dcg 0.0.0'; exit 0; fi\n\
                 echo '{{\"decision\":\"{decision}\"}}'\n"
            ),
        )?;
        Ok(path)
    }

    fn write_executable(path: &Path, script: &str) -> Result<()> {
        fs::write(path, script)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
        Ok(())
    }

    fn run_tests(fixture: &mut E2EFixture, guard: &Path) -> Value {
        let guard = guard.to_str().unwrap();
        let output = fixture.run_ms_with_env(
            &["--robot", "test", "run-step"],
            &[("MS_SAFETY_DCG_BIN", guard)],
        );
        fixture.assert_success(&output, "test");
        output.json()["reports"][0].clone()
    }

    #[test]
    fn test_run_step_is_refused_with_no_guard() -> Result<()> {
        let (mut fixture, marker) = bench("run_step_no_guard")?;
        let guard = fixture.root.join("no-such-dcg");

        let report = run_tests(&mut fixture, &guard);
        assert!(!marker.exists(), "the step must not run with no guard");
        assert_eq!(report["failed"], 1, "{report}");
        let failure = report["results"][0]["failures"][0]
            .as_str()
            .expect("reason");
        assert!(
            failure.contains("safety system unavailable"),
            "says why: {failure}"
        );
        Ok(())
    }

    #[test]
    fn test_run_step_is_refused_when_the_guard_denies() -> Result<()> {
        let (mut fixture, marker) = bench("run_step_denied")?;
        let guard = stand_in_guard(&fixture, "deny")?;

        let report = run_tests(&mut fixture, &guard);
        assert!(!marker.exists(), "a denied step must not run");
        assert_eq!(report["failed"], 1, "{report}");
        Ok(())
    }

    #[test]
    fn test_run_step_runs_when_the_guard_allows() -> Result<()> {
        let (mut fixture, marker) = bench("run_step_allowed")?;
        let guard = stand_in_guard(&fixture, "allow")?;

        let report = run_tests(&mut fixture, &guard);
        assert!(marker.exists(), "an allowed step runs: {report}");
        assert_eq!(report["passed"], 1, "{report}");
        Ok(())
    }
}

/// `ms_root` is the nearest `.ms`, so any repository can ship a config file. These run the
/// real binary from inside such a repository, with no `MS_ROOT` or `MS_CONFIG`, against an
/// executable that records every run, so "nothing ran" is observed, not assumed.
#[cfg(unix)]
mod repository_config {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output};

    use ms::error::Result;
    use serde_json::Value;
    use tempfile::TempDir;

    const INIT_DEFAULTS: &str = "[safety]\ndcg_bin = \"dcg\"\ndcg_packs = []\n\
                                 dcg_explain_format = \"json\"\n\
                                 require_verbatim_approval = true\n";

    struct Repository {
        home: TempDir,
    }

    impl Repository {
        fn new() -> Result<Self> {
            let bench = Self {
                home: TempDir::new()?,
            };
            fs::create_dir_all(bench.path().join(".ms"))?;
            Ok(bench)
        }

        fn path(&self) -> PathBuf {
            self.home.path().join("repository")
        }

        fn config_path(&self) -> PathBuf {
            self.path().join(".ms/config.toml")
        }

        fn marker(&self) -> PathBuf {
            self.home.path().join("planted-ran")
        }

        fn user_config_path(&self) -> PathBuf {
            if cfg!(target_os = "macos") {
                self.home
                    .path()
                    .join("Library/Application Support/ms/config.toml")
            } else {
                self.home.path().join(".config/ms/config.toml")
            }
        }

        /// An executable that records that it ran.
        fn plant(&self) -> Result<String> {
            let path = self.home.path().join("planted");
            let script = format!("#!/bin/sh\necho ran >> '{}'\n", self.marker().display());
            write_executable(&path, &script)?;
            Ok(path.to_str().unwrap().to_string())
        }

        /// A stand-in for dcg that allows every command.
        fn gate(&self) -> Result<String> {
            let path = self.home.path().join("stand-in-dcg");
            let script = "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'dcg 0.0.0'; exit 0; fi\n\
                          echo '{\"decision\":\"allow\"}'\n";
            write_executable(&path, script)?;
            Ok(path.to_str().unwrap().to_string())
        }

        fn ms(&self, args: &[&str], env: &[(&str, &str)]) -> Output {
            let mut command = Command::new(env!("CARGO_BIN_EXE_ms"));
            command
                .args(args)
                .current_dir(self.path())
                .env("HOME", self.home.path())
                .env("XDG_CONFIG_HOME", self.home.path().join(".config"))
                .env("XDG_DATA_HOME", self.home.path().join(".local/share"))
                .env_remove("MS_ROOT")
                .env_remove("MS_CONFIG");
            for (key, value) in env {
                command.env(key, value);
            }
            command.output().expect("run ms")
        }

        fn safety_status(&self) -> (Value, String) {
            let output = self.ms(&["--robot", "safety", "status"], &[]);
            assert!(output.status.success(), "safety status: {output:?}");
            let status = serde_json::from_slice(&output.stdout).expect("status JSON");
            (status, stderr(&output))
        }

        fn planted_ran(&self) -> bool {
            self.marker().exists()
        }
    }

    fn write_executable(path: &Path, script: &str) -> Result<()> {
        fs::write(path, script)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
        Ok(())
    }

    fn stderr(output: &Output) -> String {
        String::from_utf8_lossy(&output.stderr).to_string()
    }

    fn assert_warned(stderr: &str, key: &str) {
        assert!(stderr.contains(key), "names {key}: {stderr}");
        assert!(
            stderr.contains(".ms/config.toml"),
            "names the file: {stderr}"
        );
    }

    #[test]
    fn test_repository_config_cannot_choose_the_guard() -> Result<()> {
        let bench = Repository::new()?;
        let planted = bench.plant()?;
        fs::write(
            bench.config_path(),
            format!("[safety]\ndcg_bin = \"{planted}\"\n"),
        )?;

        let (status, stderr) = bench.safety_status();
        assert!(
            !bench.planted_ran(),
            "a repository must not choose the guard"
        );
        assert_eq!(status["dcg_bin"], "dcg");
        assert_warned(&stderr, "safety.dcg_bin");
        Ok(())
    }

    #[test]
    fn test_repository_config_cannot_turn_approval_off() -> Result<()> {
        let bench = Repository::new()?;
        fs::write(
            bench.config_path(),
            "[safety]\nrequire_verbatim_approval = false\n",
        )?;

        let (status, stderr) = bench.safety_status();
        assert_eq!(status["require_verbatim_approval"], true);
        assert_warned(&stderr, "safety.require_verbatim_approval");
        Ok(())
    }

    #[test]
    fn test_repository_config_cannot_run_cm() -> Result<()> {
        let bench = Repository::new()?;
        let planted = bench.plant()?;
        let gate = bench.gate()?;
        fs::write(
            bench.config_path(),
            format!("[cm]\ncm_path = \"{planted}\"\n"),
        )?;

        let output = bench.ms(
            &["--robot", "cm", "status"],
            &[("MS_SAFETY_DCG_BIN", gate.as_str())],
        );
        assert!(!bench.planted_ran(), "a repository must not choose cm");
        assert_warned(&stderr(&output), "cm.cm_path");
        Ok(())
    }

    #[test]
    fn test_repository_config_cannot_run_ru() -> Result<()> {
        let bench = Repository::new()?;
        let planted = bench.plant()?;
        fs::write(
            bench.config_path(),
            format!("[ru]\nru_path = \"{planted}\"\n"),
        )?;

        let output = bench.ms(&["--robot", "index", "--from-ru"], &[]);
        assert!(!bench.planted_ran(), "a repository must not choose ru");
        assert_warned(&stderr(&output), "ru.ru_path");
        Ok(())
    }

    #[test]
    fn test_repository_config_cannot_run_cass() -> Result<()> {
        let bench = Repository::new()?;
        let planted = bench.plant()?;
        fs::write(
            bench.config_path(),
            format!("[cass]\ncass_path = \"{planted}\"\n"),
        )?;

        let output = bench.ms(&["--robot", "antipatterns", "mine", "session-1"], &[]);
        assert!(!bench.planted_ran(), "a repository must not choose cass");
        assert_warned(&stderr(&output), "cass.cass_path");
        Ok(())
    }

    #[test]
    fn test_user_config_keeps_its_guard_inside_a_project() -> Result<()> {
        let bench = Repository::new()?;
        let gate = bench.gate()?;
        let user_config = bench.user_config_path();
        fs::create_dir_all(user_config.parent().unwrap())?;
        fs::write(&user_config, format!("[safety]\ndcg_bin = \"{gate}\"\n"))?;
        fs::write(bench.config_path(), INIT_DEFAULTS)?;

        let (status, stderr) = bench.safety_status();
        assert_eq!(status["dcg_bin"], gate.as_str());
        assert!(
            !stderr.contains("ignoring"),
            "defaults stay quiet: {stderr}"
        );
        Ok(())
    }

    #[test]
    fn test_init_leaves_user_only_keys_out_of_a_project() -> Result<()> {
        let bench = Repository::new()?;
        let output = bench.ms(&["--robot", "init", "--force"], &[]);
        assert!(output.status.success(), "init: {output:?}");

        let config = fs::read_to_string(bench.config_path())?;
        assert!(!config.contains("[safety]"), "{config}");
        assert!(!config.contains("cm_path"), "{config}");
        assert!(config.contains("[skill_paths]"), "{config}");
        Ok(())
    }

    #[test]
    fn test_config_set_refuses_a_user_only_key_in_a_project() -> Result<()> {
        let bench = Repository::new()?;
        fs::write(bench.config_path(), "[disclosure]\ntoken_budget = 900\n")?;

        let output = bench.ms(&["config", "set", "safety.dcg_bin", "/opt/dcg"], &[]);
        assert_eq!(
            fs::read_to_string(bench.config_path())?,
            "[disclosure]\ntoken_budget = 900\n",
            "a refused write changes nothing"
        );
        assert!(!output.status.success(), "must fail: {output:?}");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            stderr(&output)
        );
        assert!(text.contains("safety.dcg_bin"), "names the key: {text}");
        Ok(())
    }

    #[test]
    fn test_config_add_refuses_a_user_only_key_in_a_project() -> Result<()> {
        let bench = Repository::new()?;
        fs::write(bench.config_path(), "[disclosure]\ntoken_budget = 900\n")?;

        let output = bench.ms(&["config", "add", "cm.default_flags", "--x"], &[]);
        assert_eq!(
            fs::read_to_string(bench.config_path())?,
            "[disclosure]\ntoken_budget = 900\n",
            "a refused write changes nothing"
        );
        assert!(!output.status.success(), "must fail: {output:?}");
        Ok(())
    }

    #[test]
    fn test_config_set_still_writes_a_project_key() -> Result<()> {
        let bench = Repository::new()?;
        fs::write(bench.config_path(), "[disclosure]\ntoken_budget = 900\n")?;

        let output = bench.ms(&["config", "set", "disclosure.token_budget", "1000"], &[]);
        assert!(output.status.success(), "set: {output:?}");
        let config = fs::read_to_string(bench.config_path())?;
        assert!(config.contains("1000"), "{config}");
        Ok(())
    }
}

/// Test the safety status command.
#[test]
fn test_safety_status() -> Result<()> {
    let mut fixture = E2EFixture::new("safety_status");

    // ==========================================
    // Step 1: Initialize
    // ==========================================
    fixture.log_step("Initialize ms directory");
    let output = fixture.init();
    fixture.assert_success(&output, "init");
    fixture.checkpoint("post_init");

    // ==========================================
    // Step 2: Check safety status
    // ==========================================
    fixture.log_step("Check safety gate status");
    let output = fixture.run_ms(&["--robot", "safety", "status"]);
    fixture.assert_success(&output, "safety status");

    let json = output.json();
    println!("[SAFETY] Status: {:?}", json);

    // Status should include DCG availability info
    assert!(
        json.get("dcg_available").is_some() || json.get("dcg_version").is_some(),
        "Status should report DCG availability"
    );
    fixture.checkpoint("post_status");

    fixture.generate_report();
    Ok(())
}

/// Test safety check for safe commands.
#[test]
fn test_safety_check_safe_command() -> Result<()> {
    let mut fixture = E2EFixture::new("safety_check_safe");

    fixture.log_step("Initialize");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    // ==========================================
    // Check a safe command
    // ==========================================
    fixture.log_step("Check safe command: ls -la");
    let output = fixture.run_ms(&["--robot", "safety", "check", "ls -la"]);

    // The check should complete (might not be allowed if DCG not installed)
    println!("[SAFETY] Check result: {:?}", output.stdout);

    let json = output.json();
    // Should have command info in output
    if let Some(cmd) = json.get("command") {
        println!("[SAFETY] Command checked: {:?}", cmd);
    }
    if let Some(allowed) = json.get("allowed") {
        println!("[SAFETY] Allowed: {:?}", allowed);
    }

    fixture.generate_report();
    Ok(())
}

/// Test safety check for potentially dangerous commands.
#[test]
fn test_safety_check_dangerous_command() -> Result<()> {
    let mut fixture = E2EFixture::new("safety_check_dangerous");

    fixture.log_step("Initialize");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    // ==========================================
    // Check a potentially dangerous command
    // ==========================================
    fixture.log_step("Check dangerous command: rm -rf /");
    let output = fixture.run_ms(&["--robot", "safety", "check", "rm -rf /"]);

    // The check should complete
    println!("[SAFETY] Check result: {:?}", output.stdout);

    let json = output.json();
    // If DCG is available, this should be blocked
    if let Some(allowed) = json.get("allowed") {
        if allowed == &serde_json::Value::Bool(false) {
            println!("[SAFETY] Command correctly blocked");
            // Should have a reason
            if let Some(reason) = json.get("reason") {
                println!("[SAFETY] Reason: {:?}", reason);
            }
        }
    }

    fixture.generate_report();
    Ok(())
}

/// Test safety log viewing.
#[test]
fn test_safety_log() -> Result<()> {
    let mut fixture = E2EFixture::new("safety_log");

    fixture.log_step("Initialize");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    // ==========================================
    // View safety log
    // ==========================================
    fixture.log_step("View safety log");
    let output = fixture.run_ms(&["--robot", "safety", "log"]);
    fixture.assert_success(&output, "safety log");

    let json = output.json();
    println!("[SAFETY] Log: {:?}", json);

    // Should have events array (may be empty)
    if let Some(events) = json.get("events") {
        if let Some(arr) = events.as_array() {
            println!("[SAFETY] Events count: {}", arr.len());
        }
    }

    fixture.generate_report();
    Ok(())
}

/// Test safety log with filters.
#[test]
fn test_safety_log_filters() -> Result<()> {
    let mut fixture = E2EFixture::new("safety_log_filters");

    fixture.log_step("Initialize");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    // ==========================================
    // View safety log with limit
    // ==========================================
    fixture.log_step("View safety log with limit");
    let output = fixture.run_ms(&["--robot", "safety", "log", "--limit", "5"]);
    fixture.assert_success(&output, "safety log with limit");

    let json = output.json();
    if let Some(events) = json.get("events").and_then(|e| e.as_array()) {
        assert!(events.len() <= 5, "Should have at most 5 events");
        println!("[SAFETY] Events with limit: {}", events.len());
    }

    // ==========================================
    // View safety log with blocked-only filter
    // ==========================================
    fixture.log_step("View blocked-only events");
    let output = fixture.run_ms(&["--robot", "safety", "log", "--blocked-only"]);
    fixture.assert_success(&output, "safety log blocked-only");

    let json = output.json();
    println!("[SAFETY] Blocked-only log: {:?}", json);

    fixture.generate_report();
    Ok(())
}

/// Test safety check with session ID for audit.
#[test]
fn test_safety_check_with_session() -> Result<()> {
    let mut fixture = E2EFixture::new("safety_check_session");

    fixture.log_step("Initialize");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    // ==========================================
    // Check command with session ID
    // ==========================================
    fixture.log_step("Check command with session ID");
    let output = fixture.run_ms(&[
        "--robot",
        "safety",
        "check",
        "echo hello",
        "--session-id",
        "test-session-123",
    ]);

    println!("[SAFETY] Check with session: {:?}", output.stdout);

    // Verify session can be used to filter log
    fixture.log_step("Filter log by session");
    let output = fixture.run_ms(&["--robot", "safety", "log", "--session", "test-session-123"]);
    fixture.assert_success(&output, "safety log by session");

    let json = output.json();
    println!("[SAFETY] Session-filtered log: {:?}", json);

    fixture.generate_report();
    Ok(())
}

/// Test safety check with dry-run mode.
#[test]
fn test_safety_check_dry_run() -> Result<()> {
    let mut fixture = E2EFixture::new("safety_check_dry_run");

    fixture.log_step("Initialize");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    // ==========================================
    // Check command in dry-run mode
    // ==========================================
    fixture.log_step("Check command in dry-run mode");
    let output = fixture.run_ms(&["--robot", "safety", "check", "ls -la", "--dry-run"]);

    println!("[SAFETY] Dry-run check: {:?}", output.stdout);

    // Dry-run should not log the event
    // We can't easily verify this without checking log counts

    fixture.generate_report();
    Ok(())
}

/// Test complete safety workflow.
#[test]
fn test_safety_workflow_complete() -> Result<()> {
    let mut fixture = E2EFixture::new("safety_workflow_complete");

    // ==========================================
    // Step 1: Initialize
    // ==========================================
    fixture.log_step("Initialize ms directory");
    let output = fixture.init();
    fixture.assert_success(&output, "init");
    fixture.checkpoint("post_init");

    // ==========================================
    // Step 2: Check status
    // ==========================================
    fixture.log_step("Check safety status");
    let output = fixture.run_ms(&["--robot", "safety", "status"]);
    fixture.assert_success(&output, "status");

    let status_json = output.json();
    let dcg_available = status_json
        .get("dcg_available")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    println!("[SAFETY] DCG available: {}", dcg_available);
    fixture.checkpoint("post_status");

    // ==========================================
    // Step 3: Check various commands
    // ==========================================
    fixture.log_step("Check multiple commands");

    // Safe command
    let output = fixture.run_ms(&[
        "--robot",
        "safety",
        "check",
        "cat /etc/hostname",
        "--session-id",
        "workflow-test",
    ]);
    println!("[SAFETY] cat check: {:?}", output.stdout);

    // Another safe command
    let output = fixture.run_ms(&[
        "--robot",
        "safety",
        "check",
        "pwd",
        "--session-id",
        "workflow-test",
    ]);
    println!("[SAFETY] pwd check: {:?}", output.stdout);

    // Potentially dangerous command
    let output = fixture.run_ms(&[
        "--robot",
        "safety",
        "check",
        "chmod 777 /",
        "--session-id",
        "workflow-test",
    ]);
    println!("[SAFETY] chmod check: {:?}", output.stdout);

    fixture.checkpoint("post_checks");

    // ==========================================
    // Step 4: View log
    // ==========================================
    fixture.log_step("View safety log");
    let output = fixture.run_ms(&["--robot", "safety", "log"]);
    fixture.assert_success(&output, "view log");

    let json = output.json();
    if let Some(events) = json.get("events").and_then(|e| e.as_array()) {
        println!("[SAFETY] Total events: {}", events.len());
    }
    fixture.checkpoint("post_log");

    // ==========================================
    // Step 5: Filter log by session
    // ==========================================
    fixture.log_step("Filter log by session");
    let output = fixture.run_ms(&["--robot", "safety", "log", "--session", "workflow-test"]);
    fixture.assert_success(&output, "filter log");

    let json = output.json();
    println!("[SAFETY] Session events: {:?}", json);
    fixture.checkpoint("post_filter");

    fixture.generate_report();
    Ok(())
}
