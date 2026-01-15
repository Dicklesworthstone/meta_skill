//! E2E test fixture with comprehensive logging and checkpointing.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use ms::error::{MsError, Result};
use rusqlite::Connection;
use tempfile::TempDir;

/// Checkpoint snapshot for test debugging.
#[derive(Debug, Clone)]
pub struct Checkpoint {
    pub name: String,
    pub timestamp: Duration,
    pub step_count: usize,
    pub db_state: Option<String>,
    pub files_created: Vec<PathBuf>,
}

/// Step result for report generation.
#[derive(Debug, Clone)]
pub struct StepResult {
    pub name: String,
    pub success: bool,
    pub duration: Duration,
    pub output_summary: String,
}

/// E2E test fixture providing isolated environment with comprehensive logging.
pub struct E2EFixture {
    /// Test scenario name
    pub scenario_name: String,
    /// Root temp directory
    pub temp_dir: TempDir,
    /// Project root (temp_dir path)
    pub root: PathBuf,
    /// ms root directory (./.ms)
    pub ms_root: PathBuf,
    /// Config file path
    pub config_path: PathBuf,
    /// Skills directories for different layers
    pub skills_dirs: HashMap<String, PathBuf>,
    /// Database connection for state verification
    pub db: Option<Connection>,
    /// Test start time
    start_time: Instant,
    /// Current step number
    step_count: usize,
    /// Checkpoints captured
    checkpoints: Vec<Checkpoint>,
    /// Step results for report
    step_results: Vec<StepResult>,
}

impl E2EFixture {
    /// Create a fresh E2E test fixture.
    pub fn new(scenario_name: &str) -> Self {
        let start_time = Instant::now();
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let root = temp_dir.path().to_path_buf();
        let ms_root = root.join(".ms");
        let config_path = ms_root.join("config.toml");

        // Create skills directories for different layers
        let mut skills_dirs = HashMap::new();
        let project_skills = root.join("skills");
        let global_skills = root.join("global_skills");
        let local_skills = root.join("local_skills");

        std::fs::create_dir_all(&project_skills).expect("Failed to create project skills dir");
        std::fs::create_dir_all(&global_skills).expect("Failed to create global skills dir");
        std::fs::create_dir_all(&local_skills).expect("Failed to create local skills dir");

        skills_dirs.insert("project".to_string(), project_skills);
        skills_dirs.insert("global".to_string(), global_skills);
        skills_dirs.insert("local".to_string(), local_skills);

        println!();
        println!("{}", "█".repeat(70));
        println!("█ E2E SCENARIO: {}", scenario_name);
        println!("{}", "█".repeat(70));
        println!();
        println!("[E2E] Root: {:?}", root);
        println!("[E2E] MS Root: {:?}", ms_root);
        println!("[E2E] Config: {:?}", config_path);
        println!(
            "[E2E] Skills Dirs: {:?}",
            skills_dirs.keys().collect::<Vec<_>>()
        );
        println!();

        Self {
            scenario_name: scenario_name.to_string(),
            temp_dir,
            root,
            ms_root,
            config_path,
            skills_dirs,
            db: None,
            start_time,
            step_count: 0,
            checkpoints: Vec::new(),
            step_results: Vec::new(),
        }
    }

    /// Log a step in the E2E workflow.
    pub fn log_step(&mut self, description: &str) {
        self.step_count += 1;
        let elapsed = self.start_time.elapsed();

        println!();
        println!("┌{}", "─".repeat(68));
        println!("│ STEP {}: {}", self.step_count, description);
        println!("│ Time: {:?}", elapsed);
        println!("└{}", "─".repeat(68));
    }

    /// Capture a checkpoint for debugging.
    pub fn checkpoint(&mut self, name: &str) {
        let timestamp = self.start_time.elapsed();

        // Collect files in root
        let files_created: Vec<PathBuf> = walkdir::WalkDir::new(&self.root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| e.path().to_path_buf())
            .collect();

        // Get db state if available
        let db_state = self.db.as_ref().map(|db| self.dump_db_state(db));

        let checkpoint = Checkpoint {
            name: name.to_string(),
            timestamp,
            step_count: self.step_count,
            db_state,
            files_created: files_created.clone(),
        };

        println!();
        println!("[CHECKPOINT] {}", name);
        println!("[CHECKPOINT] Files: {}", files_created.len());
        if let Some(ref state) = checkpoint.db_state {
            println!("[CHECKPOINT] DB: {}", state);
        }

        self.checkpoints.push(checkpoint);
    }

    /// Run ms CLI command and capture output.
    pub fn run_ms(&mut self, args: &[&str]) -> CommandOutput {
        let step_name = format!("ms {}", args.join(" "));
        let start = Instant::now();

        println!();
        println!("[CMD] {}", step_name);

        let output = Command::new(env!("CARGO_BIN_EXE_ms"))
            .args(args)
            .env("HOME", &self.root)
            .env("MS_ROOT", &self.ms_root)
            .env("MS_CONFIG", &self.config_path)
            .current_dir(&self.root)
            .output()
            .expect("Failed to execute ms command");

        let elapsed = start.elapsed();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        let result = CommandOutput {
            success: output.status.success(),
            exit_code: output.status.code().unwrap_or(-1),
            stdout: stdout.clone(),
            stderr: stderr.clone(),
            elapsed,
        };

        println!("[CMD] Exit: {} ({:?})", result.exit_code, elapsed);
        if !stdout.is_empty() {
            let preview = if stdout.len() > 500 {
                format!("{}...", &stdout[..500])
            } else {
                stdout.clone()
            };
            println!("[STDOUT] {}", preview);
        }
        if !stderr.is_empty() {
            println!("[STDERR] {}", stderr);
        }

        // Record step result
        let summary = if result.success {
            format!("OK ({})", truncate(&stdout, 50))
        } else {
            format!("FAIL: {}", truncate(&stderr, 100))
        };

        self.step_results.push(StepResult {
            name: step_name,
            success: result.success,
            duration: elapsed,
            output_summary: summary,
        });

        result
    }

    /// Initialize ms in the test environment.
    pub fn init(&mut self) -> CommandOutput {
        self.run_ms(&["--robot", "init"])
    }

    /// Create a skill in the specified layer.
    pub fn create_skill(&self, name: &str, content: &str) -> Result<()> {
        self.create_skill_in_layer(name, content, "project")
    }

    /// Create a skill in a specific layer.
    pub fn create_skill_in_layer(&self, name: &str, content: &str, layer: &str) -> Result<()> {
        let skills_dir = self
            .skills_dirs
            .get(layer)
            .ok_or_else(|| MsError::ValidationFailed(format!("Unknown layer: {}", layer)))?;

        let skill_dir = skills_dir.join(name);
        std::fs::create_dir_all(&skill_dir)?;

        let skill_file = skill_dir.join("SKILL.md");
        std::fs::write(&skill_file, content)?;

        println!(
            "[SKILL] Created '{}' in layer '{}' ({} bytes)",
            name,
            layer,
            content.len()
        );
        Ok(())
    }

    /// Open database connection for verification.
    pub fn open_db(&mut self) {
        let db_path = self.ms_root.join("ms.db");
        if db_path.exists() {
            self.db = Some(Connection::open(&db_path).expect("Failed to open db"));
            println!("[E2E] Database opened: {:?}", db_path);
        } else {
            println!("[E2E] Database not found: {:?}", db_path);
        }
    }

    /// Assert command succeeded.
    pub fn assert_success(&self, output: &CommandOutput, operation: &str) {
        assert!(
            output.success,
            "[E2E] {} failed with exit code {}: {}",
            operation, output.exit_code, output.stderr
        );
        println!("[ASSERT] {} - SUCCESS", operation);
    }

    /// Assert output contains expected text.
    pub fn assert_output_contains(&self, output: &CommandOutput, expected: &str) {
        let found = output.stdout.contains(expected) || output.stderr.contains(expected);
        assert!(
            found,
            "[E2E] Output does not contain '{}'\nStdout: {}\nStderr: {}",
            expected,
            truncate(&output.stdout, 500),
            truncate(&output.stderr, 500)
        );
        println!("[ASSERT] Output contains '{}' - PASSED", expected);
    }

    /// Assert output does not contain text.
    pub fn assert_output_not_contains(&self, output: &CommandOutput, unexpected: &str) {
        let found = output.stdout.contains(unexpected) || output.stderr.contains(unexpected);
        assert!(
            !found,
            "[E2E] Output unexpectedly contains '{}'\nStdout: {}\nStderr: {}",
            unexpected,
            truncate(&output.stdout, 500),
            truncate(&output.stderr, 500)
        );
        println!("[ASSERT] Output does not contain '{}' - PASSED", unexpected);
    }

    /// Verify database state with custom check.
    pub fn verify_db_state(&self, check: impl FnOnce(&Connection) -> bool, description: &str) {
        if let Some(ref db) = self.db {
            let state = self.dump_db_state(db);
            println!("[DB STATE] {}", state);

            let result = check(db);
            assert!(result, "[E2E] Database check failed: {}", description);

            println!("[ASSERT] DB: {} - PASSED", description);
        } else {
            println!("[ASSERT] DB: {} - SKIPPED (no connection)", description);
        }
    }

    /// Generate final test report.
    pub fn generate_report(&self) {
        let total_time = self.start_time.elapsed();

        println!();
        println!("{}", "█".repeat(70));
        println!("█ E2E REPORT: {}", self.scenario_name);
        println!("{}", "█".repeat(70));
        println!();

        println!("SUMMARY");
        println!("───────────────────────────────────────────────────");
        println!("Total Steps: {}", self.step_count);
        println!("Checkpoints: {}", self.checkpoints.len());
        println!("Total Time:  {:?}", total_time);
        println!();

        println!("STEP RESULTS");
        println!("───────────────────────────────────────────────────");
        for (i, step) in self.step_results.iter().enumerate() {
            let status = if step.success { "✓" } else { "✗" };
            println!(
                "{:2}. {} {} ({:?})",
                i + 1,
                status,
                step.name,
                step.duration
            );
            if !step.success {
                println!("     └─ {}", step.output_summary);
            }
        }
        println!();

        println!("CHECKPOINTS");
        println!("───────────────────────────────────────────────────");
        for checkpoint in &self.checkpoints {
            println!(
                "  [{:?}] {} (step {}, {} files)",
                checkpoint.timestamp,
                checkpoint.name,
                checkpoint.step_count,
                checkpoint.files_created.len()
            );
        }
        println!();

        // Overall result
        let all_passed = self.step_results.iter().all(|s| s.success);
        if all_passed {
            println!("RESULT: ✓ ALL STEPS PASSED");
        } else {
            let failed_count = self.step_results.iter().filter(|s| !s.success).count();
            println!("RESULT: ✗ {} STEPS FAILED", failed_count);
        }

        println!();
        println!("{}", "█".repeat(70));
    }

    /// Dump database state for logging.
    fn dump_db_state(&self, db: &Connection) -> String {
        let mut state = String::new();

        if let Ok(count) =
            db.query_row::<i64, _, _>("SELECT COUNT(*) FROM skills", [], |r| r.get(0))
        {
            state.push_str(&format!("skills={} ", count));
        }
        if let Ok(count) =
            db.query_row::<i64, _, _>("SELECT COUNT(*) FROM skills_fts", [], |r| r.get(0))
        {
            state.push_str(&format!("fts={} ", count));
        }
        if let Ok(count) =
            db.query_row::<i64, _, _>("SELECT COUNT(*) FROM skill_aliases", [], |r| r.get(0))
        {
            state.push_str(&format!("aliases={} ", count));
        }

        state
    }
}

impl Drop for E2EFixture {
    fn drop(&mut self) {
        let elapsed = self.start_time.elapsed();
        println!();
        println!("{}", "█".repeat(70));
        println!("█ E2E CLEANUP: {}", self.scenario_name);
        println!("█ Total time: {:?}", elapsed);
        println!("█ Temp dir: {:?}", self.temp_dir.path());
        println!("{}", "█".repeat(70));
    }
}

/// Command output structure.
pub struct CommandOutput {
    pub success: bool,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub elapsed: Duration,
}

impl CommandOutput {
    /// Parse stdout as JSON.
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.stdout).expect("stdout should be valid JSON")
    }
}

/// Truncate string for display.
fn truncate(s: &str, max_len: usize) -> String {
    if max_len == 0 {
        return String::new();
    }
    if s.chars().count() <= max_len {
        s.to_string()
    } else if max_len < 3 {
        ".".repeat(max_len)
    } else {
        let trimmed: String = s.chars().take(max_len - 3).collect();
        format!("{trimmed}...")
    }
}
