use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use rusqlite::Connection;
use tempfile::TempDir;

// =============================================================================
// Assertion Macros
// =============================================================================

/// Assert that a file exists at the given path
#[macro_export]
macro_rules! assert_file_exists {
    ($path:expr) => {
        assert!(
            std::path::Path::new($path).exists(),
            "Expected file to exist: {:?}",
            $path
        );
    };
    ($path:expr, $msg:expr) => {
        assert!(std::path::Path::new($path).exists(), $msg);
    };
}

/// Assert that a file contains expected content
#[macro_export]
macro_rules! assert_file_contains {
    ($path:expr, $expected:expr) => {{
        let content = std::fs::read_to_string($path).expect(&format!("Failed to read {:?}", $path));
        assert!(
            content.contains($expected),
            "File {:?} does not contain '{}'\nActual content:\n{}",
            $path,
            $expected,
            &content[..std::cmp::min(content.len(), 500)]
        );
    }};
}

/// Assert that a JSON file matches expected structure
#[macro_export]
macro_rules! assert_json_matches {
    ($path:expr, $expected:expr) => {{
        let content = std::fs::read_to_string($path).expect(&format!("Failed to read {:?}", $path));
        let actual: serde_json::Value =
            serde_json::from_str(&content).expect(&format!("Invalid JSON in {:?}", $path));
        let expected: serde_json::Value = serde_json::json!($expected);
        assert_eq!(
            actual, expected,
            "JSON mismatch in {:?}\nActual: {}\nExpected: {}",
            $path, actual, expected
        );
    }};
}

/// Assert command exit code
#[macro_export]
macro_rules! assert_exit_code {
    ($output:expr, $code:expr) => {
        assert_eq!(
            $output.exit_code, $code,
            "Expected exit code {} but got {}\nstdout: {}\nstderr: {}",
            $code, $output.exit_code, $output.stdout, $output.stderr
        );
    };
}

/// Assert stdout contains expected text
#[macro_export]
macro_rules! assert_stdout_contains {
    ($output:expr, $expected:expr) => {
        assert!(
            $output.stdout.contains($expected),
            "stdout does not contain '{}'\nActual stdout:\n{}",
            $expected,
            $output.stdout
        );
    };
}

/// Assert stderr contains expected text
#[macro_export]
macro_rules! assert_stderr_contains {
    ($output:expr, $expected:expr) => {
        assert!(
            $output.stderr.contains($expected),
            "stderr does not contain '{}'\nActual stderr:\n{}",
            $expected,
            $output.stderr
        );
    };
}

/// Assert command succeeded
#[macro_export]
macro_rules! assert_command_success {
    ($output:expr) => {
        assert!(
            $output.success,
            "Command failed with exit code {}\nstdout: {}\nstderr: {}",
            $output.exit_code, $output.stdout, $output.stderr
        );
    };
    ($output:expr, $msg:expr) => {
        assert!(
            $output.success,
            "{}: exit code {}\nstdout: {}\nstderr: {}",
            $msg, $output.exit_code, $output.stdout, $output.stderr
        );
    };
}

/// Integration test fixture providing isolated environment
pub struct TestFixture {
    /// Root temp directory
    pub temp_dir: TempDir,
    /// Project root (temp_dir path)
    pub root: PathBuf,
    /// ms root directory (./.ms)
    pub ms_root: PathBuf,
    /// Config file path
    pub config_path: PathBuf,
    /// Skills directory (project-local ./skills)
    pub skills_dir: PathBuf,
    /// Search index path
    pub index_path: PathBuf,
    /// Database connection for state verification
    pub db: Option<Connection>,
    /// Test start time for timing
    start_time: std::time::Instant,
    /// Test name for logging
    test_name: String,
}

impl TestFixture {
    /// Create a fresh test fixture
    pub fn new(test_name: &str) -> Self {
        let start_time = std::time::Instant::now();
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let root = temp_dir.path().to_path_buf();
        let ms_root = root.join(".ms");
        let config_path = ms_root.join("config.toml");
        let skills_dir = root.join("skills");
        let index_path = ms_root.join("index");

        std::fs::create_dir_all(&skills_dir).expect("Failed to create skills dir");

        println!("\n{}", "=".repeat(70));
        println!("[FIXTURE] Test: {}", test_name);
        println!("[FIXTURE] Root: {:?}", root);
        println!("[FIXTURE] MS Root: {:?}", ms_root);
        println!("[FIXTURE] Config: {:?}", config_path);
        println!("[FIXTURE] Skills: {:?}", skills_dir);
        println!("[FIXTURE] Index: {:?}", index_path);
        println!("{}", "=".repeat(70));

        Self {
            temp_dir,
            root,
            ms_root,
            config_path,
            skills_dir,
            index_path,
            db: None,
            start_time,
            test_name: test_name.to_string(),
        }
    }

    /// Create fixture with pre-indexed skills
    pub fn with_indexed_skills(test_name: &str, skills: &[TestSkill]) -> Self {
        let mut fixture = Self::new(test_name);
        let init = fixture.init();
        assert!(init.success, "init failed: {}", init.stderr);

        for skill in skills {
            fixture.add_skill(skill);
        }

        let output = fixture.run_ms(&["--robot", "index"]);
        assert!(output.success, "Failed to index skills: {}", output.stderr);

        fixture.open_db();
        fixture
    }

    /// Create fixture with mock CASS integration
    pub fn with_mock_cass(test_name: &str) -> Self {
        let fixture = Self::new(test_name);

        let cass_dir = fixture.root.join("mock_cass");
        std::fs::create_dir_all(&cass_dir).expect("Failed to create mock CASS dir");

        let extraction = r#"{
  "skill_name": "test-skill",
  "description": "A test skill for integration testing",
  "patterns": ["pattern1", "pattern2"],
  "confidence": 0.85
}"#;
        std::fs::write(cass_dir.join("extraction.json"), extraction)
            .expect("Failed to write mock extraction");

        println!("[FIXTURE] Mock CASS configured at: {:?}", cass_dir);

        fixture
    }

    /// Create fixture with pre-populated sample bundles
    pub fn with_sample_bundles(test_name: &str) -> Self {
        let fixture = Self::new(test_name);
        let init = fixture.init();
        assert!(init.success, "init failed: {}", init.stderr);

        // Add sample bundles
        fixture.add_bundle(&sample_bundles::rust_patterns());
        fixture.add_bundle(&sample_bundles::testing_patterns());

        println!("[FIXTURE] Sample bundles added");

        fixture
    }

    /// Create fixture with pre-populated sample skills
    pub fn with_sample_skills(test_name: &str) -> Self {
        let mut fixture = Self::new(test_name);
        let init = fixture.init();
        assert!(init.success, "init failed: {}", init.stderr);

        // Add all sample skills from the sample_skills module
        for skill in sample_skills::all() {
            fixture.add_skill(&skill);
        }

        // Index everything
        let output = fixture.run_ms(&["--robot", "index"]);
        assert!(output.success, "Failed to index skills: {}", output.stderr);

        fixture.open_db();
        println!("[FIXTURE] Sample skills setup complete");

        fixture
    }

    /// Create fixture with both skills and bundles
    pub fn with_full_setup(test_name: &str) -> Self {
        let mut fixture = Self::new(test_name);
        let init = fixture.init();
        assert!(init.success, "init failed: {}", init.stderr);

        // Add all sample skills from the sample_skills module
        for skill in sample_skills::all() {
            fixture.add_skill(&skill);
        }

        // Add sample bundles from the sample_bundles module
        fixture.add_bundle(&sample_bundles::rust_patterns());
        fixture.add_bundle(&sample_bundles::testing_patterns());

        // Index everything
        let output = fixture.run_ms(&["--robot", "index"]);
        assert!(output.success, "Failed to index: {}", output.stderr);

        fixture.open_db();
        println!("[FIXTURE] Full setup complete");

        fixture
    }

    /// Add a skill to the test environment
    pub fn add_skill(&self, skill: &TestSkill) {
        let skill_dir = self.skills_dir.join(&skill.name);
        std::fs::create_dir_all(&skill_dir).expect("Failed to create skill dir");

        let skill_file = skill_dir.join("SKILL.md");
        std::fs::write(&skill_file, &skill.content).expect("Failed to write skill");

        println!(
            "[FIXTURE] Added skill: {} ({} bytes)",
            skill.name,
            skill.content.len()
        );
    }

    /// Run ms CLI command and capture output
    pub fn run_ms(&self, args: &[&str]) -> CommandOutput {
        self.run_ms_with_timeout(args, Duration::from_secs(30))
    }

    /// Run ms CLI command with custom timeout
    pub fn run_ms_with_timeout(&self, args: &[&str], timeout: Duration) -> CommandOutput {
        use std::io::Read;
        use std::process::Stdio;

        let start = std::time::Instant::now();
        println!("\n[CMD] ms {} (timeout: {:?})", args.join(" "), timeout);

        let mut child = Command::new(env!("CARGO_BIN_EXE_ms"))
            .args(args)
            .env("HOME", &self.root)
            .env("MS_ROOT", &self.ms_root)
            .env("MS_CONFIG", &self.config_path)
            .current_dir(&self.root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn ms command");

        // Wait with timeout
        let result: Result<std::process::ExitStatus, String> = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => {
                    if start.elapsed() > timeout {
                        let _ = child.kill();
                        break Err("Command timed out".to_string());
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => break Err(format!("Error waiting: {}", e)),
            }
        };

        let elapsed = start.elapsed();

        let (success, exit_code, stdout, stderr) = match result {
            Ok(status) => {
                let mut stdout_str = String::new();
                let mut stderr_str = String::new();
                if let Some(mut out) = child.stdout.take() {
                    let _ = out.read_to_string(&mut stdout_str);
                }
                if let Some(mut err) = child.stderr.take() {
                    let _ = err.read_to_string(&mut stderr_str);
                }
                (status.success(), status.code().unwrap_or(-1), stdout_str, stderr_str)
            }
            Err(msg) => (false, -1, String::new(), msg),
        };

        println!("[CMD] Exit code: {}", exit_code);
        println!("[CMD] Timing: {:?}", elapsed);

        // Warn about slow operations (threshold: 5 seconds)
        const SLOW_THRESHOLD: Duration = Duration::from_secs(5);
        if elapsed > SLOW_THRESHOLD {
            println!("[SLOW] ⚠ Command took {:?} (threshold: {:?})", elapsed, SLOW_THRESHOLD);
        }

        if !stdout.is_empty() {
            println!("[STDOUT]\n{}", stdout);
        }
        if !stderr.is_empty() {
            println!("[STDERR]\n{}", stderr);
        }

        CommandOutput {
            success,
            exit_code,
            stdout,
            stderr,
            elapsed,
        }
    }

    /// Run ms CLI command with environment variables
    pub fn run_ms_with_env(&self, args: &[&str], env_vars: &[(&str, &str)]) -> CommandOutput {
        let start = std::time::Instant::now();
        println!("\n[CMD] ms {} (with {} env vars)", args.join(" "), env_vars.len());

        let mut cmd = Command::new(env!("CARGO_BIN_EXE_ms"));
        cmd.args(args)
            .env("HOME", &self.root)
            .env("MS_ROOT", &self.ms_root)
            .env("MS_CONFIG", &self.config_path)
            .current_dir(&self.root);

        for (key, value) in env_vars {
            cmd.env(key, value);
        }

        let output = cmd.output().expect("Failed to execute ms command");

        let elapsed = start.elapsed();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        println!("[CMD] Exit code: {}", output.status.code().unwrap_or(-1));
        println!("[CMD] Timing: {:?}", elapsed);

        // Warn about slow operations (threshold: 5 seconds)
        const SLOW_THRESHOLD: Duration = Duration::from_secs(5);
        if elapsed > SLOW_THRESHOLD {
            println!("[SLOW] ⚠ Command took {:?} (threshold: {:?})", elapsed, SLOW_THRESHOLD);
        }

        if !stdout.is_empty() {
            println!("[STDOUT]\n{}", stdout);
        }
        if !stderr.is_empty() {
            println!("[STDERR]\n{}", stderr);
        }

        CommandOutput {
            success: output.status.success(),
            exit_code: output.status.code().unwrap_or(-1),
            stdout,
            stderr,
            elapsed,
        }
    }

    pub fn init(&self) -> CommandOutput {
        self.run_ms(&["--robot", "init"])
    }

    /// Add a bundle to the test environment
    pub fn add_bundle(&self, bundle: &TestBundle) {
        let bundle_dir = self.root.join("bundles").join(&bundle.name);
        std::fs::create_dir_all(&bundle_dir).expect("Failed to create bundle dir");

        // Write manifest
        let manifest_path = bundle_dir.join("bundle.json");
        std::fs::write(&manifest_path, &bundle.manifest).expect("Failed to write bundle manifest");

        // Write skills if any
        for (skill_name, skill_content) in &bundle.skills {
            let skill_dir = bundle_dir.join("skills").join(skill_name);
            std::fs::create_dir_all(&skill_dir).expect("Failed to create bundle skill dir");
            let skill_file = skill_dir.join("SKILL.md");
            std::fs::write(&skill_file, skill_content).expect("Failed to write bundle skill");
        }

        println!(
            "[FIXTURE] Added bundle: {} ({} skills)",
            bundle.name,
            bundle.skills.len()
        );
    }

    pub fn db_path(&self) -> PathBuf {
        self.ms_root.join("ms.db")
    }

    /// Verify database state
    pub fn verify_db_state(&self, check: impl FnOnce(&Connection) -> bool, description: &str) {
        if let Some(ref db) = self.db {
            let db_state = self.dump_db_state(db);
            println!("[DB STATE] {}", db_state);

            let result = check(db);
            assert!(result, "Database state check failed: {}", description);

            println!("[DB CHECK] {} - PASSED", description);
        } else {
            println!("[DB CHECK] Skipped (no database connection): {}", description);
        }
    }

    pub fn open_db(&mut self) {
        let db_path = self.ms_root.join("ms.db");
        if db_path.exists() {
            self.db = Some(Connection::open(&db_path).expect("Failed to open db"));
            println!("[FIXTURE] Database opened: {:?}", db_path);
        }
    }

    /// Dump database state for logging
    fn dump_db_state(&self, db: &Connection) -> String {
        let mut state = String::new();

        if let Ok(count) = db.query_row::<i64, _, _>("SELECT COUNT(*) FROM skills", [], |r| r.get(0)) {
            state.push_str(&format!("skills={} ", count));
        }
        if let Ok(count) = db.query_row::<i64, _, _>("SELECT COUNT(*) FROM skills_fts", [], |r| r.get(0)) {
            state.push_str(&format!("fts={} ", count));
        }

        state
    }

    /// Dump directory tree for debugging
    pub fn dump_directory_tree(&self) -> String {
        self.dump_directory_tree_at(&self.root)
    }

    /// Dump directory tree starting from a specific path
    pub fn dump_directory_tree_at(&self, path: &Path) -> String {
        let mut output = String::new();
        self.build_tree(&mut output, path, "", true);
        output
    }

    fn build_tree(&self, output: &mut String, path: &Path, prefix: &str, is_last: bool) {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let connector = if is_last { "└── " } else { "├── " };

        if prefix.is_empty() {
            output.push_str(&format!("{}\n", name));
        } else {
            output.push_str(&format!("{}{}{}\n", prefix, connector, name));
        }

        if path.is_dir() {
            let mut entries: Vec<_> = std::fs::read_dir(path)
                .ok()
                .map(|rd| rd.filter_map(|e| e.ok()).collect())
                .unwrap_or_default();
            entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

            let new_prefix = if prefix.is_empty() {
                String::new()
            } else if is_last {
                format!("{}    ", prefix)
            } else {
                format!("{}│   ", prefix)
            };

            for (i, entry) in entries.iter().enumerate() {
                let is_last_entry = i == entries.len() - 1;
                self.build_tree(output, &entry.path(), &new_prefix, is_last_entry);
            }
        }
    }

    /// Dump index state for debugging
    pub fn dump_index_state(&self) -> String {
        let index_path = &self.index_path;
        let mut state = String::new();

        state.push_str(&format!("Index path: {:?}\n", index_path));

        if !index_path.exists() {
            state.push_str("  Index directory does not exist\n");
            return state;
        }

        // Count files in index directory
        let file_count = std::fs::read_dir(index_path)
            .ok()
            .map(|rd| rd.filter_map(|e| e.ok()).count())
            .unwrap_or(0);

        state.push_str(&format!("  Files: {}\n", file_count));

        // Get total size
        let total_size: u64 = walkdir::WalkDir::new(index_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter_map(|e| e.metadata().ok())
            .filter(|m| m.is_file())
            .map(|m| m.len())
            .sum();

        state.push_str(&format!("  Total size: {} bytes\n", total_size));

        // List index files
        state.push_str("  Contents:\n");
        if let Ok(entries) = std::fs::read_dir(index_path) {
            for entry in entries.filter_map(|e| e.ok()) {
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                state.push_str(&format!(
                    "    {} ({} bytes)\n",
                    entry.file_name().to_string_lossy(),
                    size
                ));
            }
        }

        state
    }

    /// Get elapsed time since fixture creation
    pub fn elapsed(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }

    /// Log timing information
    pub fn log_timing(&self, operation: &str) {
        println!("[TIMING] {} completed at {:?}", operation, self.elapsed());
    }
}

impl Drop for TestFixture {
    fn drop(&mut self) {
        let elapsed = self.start_time.elapsed();
        println!("\n{}", "=".repeat(70));
        println!("[FIXTURE] Test complete: {}", self.test_name);
        println!("[FIXTURE] Total time: {:?}", elapsed);
        println!("[FIXTURE] Cleaning up: {:?}", self.temp_dir.path());
        println!("{}\n", "=".repeat(70));
    }
}

/// Test skill definition
pub struct TestSkill {
    pub name: String,
    pub content: String,
}

impl TestSkill {
    pub fn new(name: &str, description: &str) -> Self {
        let content = format!(
            "# {}\n\n{}\n\n## Overview\n\n{}\n",
            name, description, description
        );

        Self {
            name: name.to_string(),
            content,
        }
    }

    pub fn with_content(name: &str, content: &str) -> Self {
        Self {
            name: name.to_string(),
            content: content.to_string(),
        }
    }
}

/// Test bundle definition
pub struct TestBundle {
    pub name: String,
    pub manifest: String,
    pub skills: Vec<(String, String)>,
}

impl TestBundle {
    /// Create a minimal test bundle
    pub fn new(name: &str, description: &str) -> Self {
        let manifest = format!(
            r#"{{
  "name": "{}",
  "version": "1.0.0",
  "description": "{}",
  "skills": []
}}"#,
            name, description
        );

        Self {
            name: name.to_string(),
            manifest,
            skills: Vec::new(),
        }
    }

    /// Create a bundle with skills
    pub fn with_skills(name: &str, description: &str, skills: Vec<(&str, &str)>) -> Self {
        let skill_names: Vec<_> = skills.iter().map(|(n, _)| format!(r#""{}""#, n)).collect();
        let manifest = format!(
            r#"{{
  "name": "{}",
  "version": "1.0.0",
  "description": "{}",
  "skills": [{}]
}}"#,
            name,
            description,
            skill_names.join(", ")
        );

        Self {
            name: name.to_string(),
            manifest,
            skills: skills
                .into_iter()
                .map(|(n, c)| (n.to_string(), c.to_string()))
                .collect(),
        }
    }

    /// Add a skill to this bundle
    pub fn add_skill(mut self, name: &str, content: &str) -> Self {
        self.skills.push((name.to_string(), content.to_string()));
        self
    }
}

/// Sample bundles for testing
pub mod sample_bundles {
    use super::TestBundle;

    /// A bundle for Rust development patterns
    pub fn rust_patterns() -> TestBundle {
        TestBundle::with_skills(
            "rust-patterns",
            "Common Rust development patterns",
            vec![
                (
                    "error-handling",
                    "# Error Handling\n\nPatterns for Rust error handling with Result and ?",
                ),
                (
                    "async-patterns",
                    "# Async Patterns\n\nAsync/await patterns for Tokio and async-std",
                ),
            ],
        )
    }

    /// A bundle for testing patterns
    pub fn testing_patterns() -> TestBundle {
        TestBundle::with_skills(
            "testing-patterns",
            "Testing patterns and best practices",
            vec![(
                "unit-testing",
                "# Unit Testing\n\nUnit testing patterns with #[test] and assertions",
            )],
        )
    }

    /// An empty bundle for edge case testing
    pub fn empty_bundle() -> TestBundle {
        TestBundle::new("empty-bundle", "An empty bundle for testing")
    }
}

/// Sample skills for testing
pub mod sample_skills {
    use super::TestSkill;

    /// A comprehensive Rust error handling skill
    pub fn rust_error_handling() -> TestSkill {
        TestSkill::with_content(
            "rust-error-handling",
            r#"# Rust Error Handling

Use Result<T, E> for recoverable errors and panic! for unrecoverable ones.

## Overview

Rust's error handling is based on the Result type for operations that can fail.

## Rules

- Use `?` operator to propagate errors
- Define custom error types with thiserror
- Use anyhow for application errors
- Reserve panic! for programming errors

## Examples

```rust
fn read_file(path: &str) -> Result<String, std::io::Error> {
    std::fs::read_to_string(path)
}
```
"#,
        )
    }

    /// A Git workflow skill
    pub fn git_workflow() -> TestSkill {
        TestSkill::with_content(
            "git-workflow",
            r#"# Git Workflow

Standard Git workflow patterns for feature development.

## Overview

Use feature branches with meaningful commit messages.

## Rules

- Create feature branches from main
- Write descriptive commit messages
- Squash commits before merging
- Use conventional commit format
"#,
        )
    }

    /// A testing best practices skill
    pub fn testing_best_practices() -> TestSkill {
        TestSkill::with_content(
            "testing-best-practices",
            r#"# Testing Best Practices

Guidelines for writing effective tests.

## Overview

Good tests are fast, isolated, and readable.

## Rules

- Test one thing per test
- Use descriptive test names
- Arrange-Act-Assert pattern
- Mock external dependencies
"#,
        )
    }

    /// A minimal skill for edge case testing
    pub fn minimal() -> TestSkill {
        TestSkill::new("minimal-skill", "A minimal skill for testing")
    }

    /// All sample skills as a vector
    pub fn all() -> Vec<TestSkill> {
        vec![
            rust_error_handling(),
            git_workflow(),
            testing_best_practices(),
        ]
    }
}

/// Command output structure
pub struct CommandOutput {
    pub success: bool,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub elapsed: std::time::Duration,
}

impl CommandOutput {
    /// Check if stdout contains expected text
    pub fn stdout_contains(&self, expected: &str) -> bool {
        self.stdout.contains(expected)
    }

    /// Check if stderr contains expected text
    pub fn stderr_contains(&self, expected: &str) -> bool {
        self.stderr.contains(expected)
    }

    /// Parse stdout as JSON
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.stdout).expect("stdout should be valid JSON")
    }

    /// Try to parse stdout as JSON
    pub fn try_json(&self) -> Option<serde_json::Value> {
        serde_json::from_str(&self.stdout).ok()
    }

    /// Assert this command succeeded
    pub fn assert_success(&self) {
        assert!(
            self.success,
            "Expected success but got exit code {}\nstdout: {}\nstderr: {}",
            self.exit_code, self.stdout, self.stderr
        );
    }

    /// Assert this command failed
    pub fn assert_failure(&self) {
        assert!(
            !self.success,
            "Expected failure but command succeeded\nstdout: {}\nstderr: {}",
            self.stdout, self.stderr
        );
    }

    /// Assert exit code
    pub fn assert_exit_code(&self, expected: i32) {
        assert_eq!(
            self.exit_code, expected,
            "Expected exit code {} but got {}\nstdout: {}\nstderr: {}",
            expected, self.exit_code, self.stdout, self.stderr
        );
    }
}

#[allow(dead_code)]
fn ensure_parent(path: &Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
}
