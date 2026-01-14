use std::path::{Path, PathBuf};
use std::process::Command;

use rusqlite::Connection;
use tempfile::TempDir;

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
  \"skill_name\": \"test-skill\",
  \"description\": \"A test skill for integration testing\",
  \"patterns\": [\"pattern1\", \"pattern2\"],
  \"confidence\": 0.85
}"#;
        std::fs::write(cass_dir.join("extraction.json"), extraction)
            .expect("Failed to write mock extraction");

        println!("[FIXTURE] Mock CASS configured at: {:?}", cass_dir);

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
        let start = std::time::Instant::now();

        println!("\n[CMD] ms {}", args.join(" "));

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

        println!("[CMD] Exit code: {}", output.status.code().unwrap_or(-1));
        println!("[CMD] Timing: {:?}", elapsed);
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

/// Command output structure
pub struct CommandOutput {
    pub success: bool,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub elapsed: std::time::Duration,
}

#[allow(dead_code)]
fn ensure_parent(path: &Path) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
}
