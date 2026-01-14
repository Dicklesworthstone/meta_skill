//! Test file specifications and parsing

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{MsError, Result};

/// A complete test definition (YAML spec)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestDefinition {
    /// Test name (required)
    pub name: String,

    /// What this test validates
    #[serde(default)]
    pub description: Option<String>,

    /// Skill ID to test (required)
    #[serde(default)]
    pub skill: Option<String>,

    /// Setup steps (run before test)
    #[serde(default)]
    pub setup: Option<Vec<TestStep>>,

    /// Main test steps (required)
    #[serde(default)]
    pub steps: Vec<TestStep>,

    /// Cleanup steps (run after test, even on failure)
    #[serde(default)]
    pub cleanup: Option<Vec<TestStep>>,

    /// Test timeout
    #[serde(default)]
    #[serde(with = "humantime_serde")]
    pub timeout: Option<Duration>,

    /// Tags for filtering
    #[serde(default)]
    pub tags: Vec<String>,

    /// Conditions to skip the test
    #[serde(default)]
    pub skip_if: Option<Vec<SkipCondition>>,

    /// System requirements
    #[serde(default)]
    pub requires: Option<Vec<Requirement>>,
}

/// Alias for backward compatibility
pub type TestSpec = TestDefinition;

fn default_timeout() -> Duration {
    Duration::from_secs(60)
}

/// A single test step
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestStep {
    /// Load a skill
    LoadSkill(LoadSkillStep),

    /// Run a shell command
    Run(RunStep),

    /// Assert conditions
    Assert(AssertStep),

    /// Write a file
    WriteFile(WriteFileStep),

    /// Create a directory
    Mkdir(MkdirStep),

    /// Remove a file or directory
    Remove(RemoveStep),

    /// Copy a file
    Copy(CopyStep),

    /// Sleep for a duration
    Sleep(SleepStep),

    /// Set a variable
    Set(SetStep),

    /// Conditional execution
    If(IfStep),
}

/// Load a skill step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadSkillStep {
    /// Disclosure level
    #[serde(default = "default_level")]
    pub level: String,

    /// Token budget
    pub budget: Option<usize>,

    /// Suggestion context
    #[serde(default)]
    pub context: HashMap<String, serde_json::Value>,
}

fn default_level() -> String {
    "standard".to_string()
}

/// Run a command step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunStep {
    /// Command to run
    pub cmd: String,

    /// Working directory
    pub cwd: Option<String>,

    /// Environment variables
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// Stdin input
    pub stdin: Option<String>,

    /// Command timeout
    #[serde(default)]
    #[serde(with = "humantime_serde")]
    pub timeout: Option<Duration>,
}

/// Assert conditions step
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssertStep {
    /// Expected exit code (from previous run)
    pub exit_code: Option<i32>,

    /// stdout should contain this text
    pub stdout_contains: Option<String>,

    /// stdout should not contain this text
    pub stdout_not_contains: Option<String>,

    /// stderr should be empty
    pub stderr_empty: Option<bool>,

    /// File should exist
    pub file_exists: Option<String>,

    /// File should contain text
    pub file_contains: Option<FileContains>,

    /// Skill should be loaded
    pub skill_loaded: Option<bool>,

    /// Sections that should be present
    pub sections_present: Option<Vec<String>>,

    /// Tokens used should be less than
    pub tokens_used_lt: Option<usize>,

    /// Retrieval rank should be at most
    pub retrieval_rank_le: Option<usize>,
}

/// File contains assertion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContains {
    pub path: String,
    pub text: String,
}

/// Type alias for backward compatibility with steps.rs
pub type Assertions = AssertStep;

/// Condition for if-step evaluation (struct-based for combining multiple checks)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Condition {
    /// Platform check (e.g., "linux", "macos", "windows")
    pub platform: Option<String>,
    /// Environment variable must exist
    pub env_exists: Option<String>,
    /// Environment variables must equal specific values
    #[serde(default)]
    pub env_equals: Option<std::collections::HashMap<String, String>>,
}

/// Write a file step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteFileStep {
    pub path: String,
    pub content: String,
}

/// Create directory step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MkdirStep {
    pub path: String,
    #[serde(default)]
    pub parents: bool,
}

/// Remove file/directory step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveStep {
    pub path: String,
    #[serde(default)]
    pub recursive: bool,
}

/// Copy file step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopyStep {
    pub from: String,
    pub to: String,
}

/// Sleep step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SleepStep {
    #[serde(with = "humantime_serde")]
    pub duration: Duration,
}

/// Set variable step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetStep {
    pub name: String,
    pub value: String,
}

/// Conditional execution step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IfStep {
    pub condition: Condition,
    #[serde(rename = "then")]
    pub then_steps: Vec<TestStep>,
    #[serde(rename = "else", default)]
    pub else_steps: Option<Vec<TestStep>>,
}

/// Conditions for skipping tests
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipCondition {
    /// Skip on specific platform
    Platform(String),
    /// Skip if command not found
    CommandMissing(String),
    /// Skip if file doesn't exist
    FileMissing(String),
    /// Skip if environment variable not set
    EnvMissing(String),
}

/// System requirements
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Requirement {
    /// Requires a command to be available
    Command(String),
    /// Requires a file to exist
    File(String),
    /// Requires an environment variable
    Env(String),
    /// Requires a specific platform
    Platform(String),
}

impl TestSpec {
    /// Parse a test spec from YAML
    pub fn from_yaml(content: &str) -> Result<Self> {
        serde_yaml::from_str(content).map_err(|err| {
            MsError::ValidationFailed(format!("invalid test YAML: {err}"))
        })
    }

    /// Load a test spec from a file
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|err| {
            MsError::Io(std::io::Error::new(
                err.kind(),
                format!("read test file {}: {err}", path.display()),
            ))
        })?;
        Self::from_yaml(&content)
    }

    /// Check if this test should be skipped based on conditions
    pub fn should_skip(&self) -> Option<String> {
        let conditions = self.skip_if.as_ref()?;
        for condition in conditions {
            match condition {
                SkipCondition::Platform(p) => {
                    let current = std::env::consts::OS;
                    if current == p {
                        return Some(format!("skip on platform: {p}"));
                    }
                }
                SkipCondition::CommandMissing(cmd) => {
                    if which::which(cmd).is_err() {
                        return Some(format!("command not found: {cmd}"));
                    }
                }
                SkipCondition::FileMissing(f) => {
                    if !std::path::Path::new(f).exists() {
                        return Some(format!("file missing: {f}"));
                    }
                }
                SkipCondition::EnvMissing(var) => {
                    if std::env::var(var).is_err() {
                        return Some(format!("env var missing: {var}"));
                    }
                }
            }
        }
        None
    }

    /// Check if requirements are met
    pub fn check_requirements(&self) -> Result<()> {
        let requirements = match &self.requires {
            Some(r) => r,
            None => return Ok(()),
        };
        for req in requirements {
            match req {
                Requirement::Command(cmd) => {
                    if which::which(cmd).is_err() {
                        return Err(MsError::ValidationFailed(format!(
                            "required command not found: {cmd}"
                        )));
                    }
                }
                Requirement::File(path) => {
                    if !std::path::Path::new(path).exists() {
                        return Err(MsError::ValidationFailed(format!(
                            "required file not found: {path}"
                        )));
                    }
                }
                Requirement::Env(var) => {
                    if std::env::var(var).is_err() {
                        return Err(MsError::ValidationFailed(format!(
                            "required env var not set: {var}"
                        )));
                    }
                }
                Requirement::Platform(platform) => {
                    let current = std::env::consts::OS;
                    if current != platform {
                        return Err(MsError::ValidationFailed(format!(
                            "requires platform {platform}, got {current}"
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    /// Check if test has a specific tag
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t.eq_ignore_ascii_case(tag))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_TEST: &str = r#"
name: "Basic load test"
description: "Test that skill loads correctly"
skill: rust-error-handling
timeout: 30s
tags: [smoke, load]

setup:
  - mkdir:
      path: "/tmp/test-workspace"
      parents: true

steps:
  - load_skill:
      level: standard
  - run:
      cmd: "echo hello"
  - assert:
      exit_code: 0
      stdout_contains: "hello"

cleanup:
  - remove:
      path: "/tmp/test-workspace"
      recursive: true
"#;

    #[test]
    fn parse_test_spec() {
        let spec = TestSpec::from_yaml(SAMPLE_TEST).unwrap();
        assert_eq!(spec.name, "Basic load test");
        assert_eq!(spec.skill, Some("rust-error-handling".to_string()));
        assert_eq!(spec.timeout, Some(Duration::from_secs(30)));
        assert!(spec.has_tag("smoke"));
        assert!(spec.has_tag("load"));
        assert!(!spec.has_tag("integration"));
        assert_eq!(spec.setup.as_ref().map(|s| s.len()), Some(1));
        assert_eq!(spec.steps.len(), 3);
        assert_eq!(spec.cleanup.as_ref().map(|c| c.len()), Some(1));
    }

    #[test]
    fn parse_load_skill_step() {
        let yaml = r#"
name: test
skill: test-skill
steps:
  - load_skill:
      level: comprehensive
      budget: 2000
"#;
        let spec = TestSpec::from_yaml(yaml).unwrap();
        match &spec.steps[0] {
            TestStep::LoadSkill(s) => {
                assert_eq!(s.level, "comprehensive");
                assert_eq!(s.budget, Some(2000));
            }
            _ => panic!("expected LoadSkill step"),
        }
    }

    #[test]
    fn parse_run_step() {
        let yaml = r#"
name: test
skill: test-skill
steps:
  - run:
      cmd: "cargo build"
      cwd: "/tmp"
      env:
        RUST_BACKTRACE: "1"
      timeout: 10s
"#;
        let spec = TestSpec::from_yaml(yaml).unwrap();
        match &spec.steps[0] {
            TestStep::Run(s) => {
                assert_eq!(s.cmd, "cargo build");
                assert_eq!(s.cwd, Some("/tmp".to_string()));
                assert_eq!(s.env.get("RUST_BACKTRACE"), Some(&"1".to_string()));
                assert_eq!(s.timeout, Some(Duration::from_secs(10)));
            }
            _ => panic!("expected Run step"),
        }
    }

    #[test]
    fn parse_assert_step() {
        let yaml = r#"
name: test
skill: test-skill
steps:
  - assert:
      exit_code: 0
      stdout_contains: "success"
      file_exists: "/tmp/output.txt"
"#;
        let spec = TestSpec::from_yaml(yaml).unwrap();
        match &spec.steps[0] {
            TestStep::Assert(s) => {
                assert_eq!(s.exit_code, Some(0));
                assert_eq!(s.stdout_contains, Some("success".to_string()));
                assert_eq!(s.file_exists, Some("/tmp/output.txt".to_string()));
            }
            _ => panic!("expected Assert step"),
        }
    }
}
